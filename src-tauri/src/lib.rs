mod clipboard;
mod commands;
mod keyhook;
mod paste;
mod settings;
mod store;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use store::Store;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 面板是否被钉住（钉住后粘贴不自动关闭，并保持置顶）
pub static PANEL_PINNED: AtomicBool = AtomicBool::new(false);
/// 面板可见性（钩子回调只读这个，避免调用 Tauri API 拖慢钩子）
pub static PANEL_OPEN: AtomicBool = AtomicBool::new(false);
/// 面板最近一次显示的时间（避免唤出面板的那次点击立刻把它关掉）
pub static PANEL_SHOWN_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// 面板窗口句柄，供鼠标钩子做命中判断
pub static PANEL_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub struct AppState {
    pub store: Arc<Store>,
    pub settings: Mutex<settings::Settings>,
    pub data_dir: PathBuf,
    pub paused: Arc<AtomicBool>,
}

/// 显示/隐藏主面板


/// 用 Win32 判断面板是否真的可见（Tauri 内部状态和我们 SetWindowPos 显示不同步）
pub fn panel_visible(w: &tauri::WebviewWindow) -> bool {
    if let Ok(h) = w.hwnd() {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                windows::Win32::Foundation::HWND(h.0 as _),
            )
            .as_bool()
        }
    } else {
        false
    }
}





/// 把 WebView2 的底色刷成面板同色（透明在这台机器上不生效，至少别漏白）
pub fn make_webview_transparent(w: &tauri::WebviewWindow) {
    let _ = w.with_webview(|wv| {
        #[cfg(windows)]
        unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2Controller2, COREWEBVIEW2_COLOR,
            };
            use windows_core::Interface;

            let ctrl = wv.controller();
            if let Ok(c2) = ctrl.cast::<ICoreWebView2Controller2>() {
                let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 255,
                    R: 20,
                    G: 21,
                    B: 26,
                });
            }
        }
    });

    #[cfg(windows)]
    if let Ok(h) = w.hwnd() {
        unsafe {
            use windows::Win32::Foundation::COLORREF;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE,
                LWA_ALPHA, WS_EX_LAYERED,
            };
            let hw = windows::Win32::Foundation::HWND(h.0 as _);
            let ex = GetWindowLongPtrW(hw, GWL_EXSTYLE);
            if ex & (WS_EX_LAYERED.0 as isize) == 0 {
                SetWindowLongPtrW(hw, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
                let _ = SetLayeredWindowAttributes(hw, COLORREF(0), 255, LWA_ALPHA);
            }
        }
    }
}


/// 用 Win11 的 DWM 原生圆角（有抗锯齿），替代生硬的 SetWindowRgn 裁剪
pub fn apply_dwm_round(w: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE,
    };
    if let Ok(h) = w.hwnd() {
        let hw = windows::Win32::Foundation::HWND(h.0 as _);
        let pref = DWM_WINDOW_CORNER_PREFERENCE(2); // DWMWCP_ROUND
        unsafe {
            let _ = DwmSetWindowAttribute(
                hw,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
        }
    }
}


/// 把面板重新提到最上层（不激活、不改变位置大小）
pub fn raise_panel(w: &tauri::WebviewWindow) {
    if let Ok(h) = w.hwnd() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                windows::Win32::Foundation::HWND(h.0 as _),
                windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
                0,
                0,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
            );
        }
    }
}

/// 同步隐藏面板（Tauri 的 hide 是异步的，会让热键切换状态错乱）
pub fn hide_panel_now(w: &tauri::WebviewWindow) {
    PANEL_OPEN.store(false, Ordering::SeqCst);
    if let Ok(h) = w.hwnd() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                windows::Win32::Foundation::HWND(h.0 as _),
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
    }
}

/// 面板是否真的拿到了前台焦点
pub fn panel_is_active() -> bool {
    // 不能只看进程 PID：托盘窗口、钩子窗口都属于本进程，会把「面板有焦点」误判成真
    let raw = PANEL_HWND.load(Ordering::SeqCst);
    if raw == 0 {
        return false;
    }
    unsafe {
        let fg = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        fg.0 as isize == raw
    }
}

/// 是否让窗口拒绝被激活（不抢焦点）
pub fn set_no_activate(w: &tauri::WebviewWindow, on: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE};
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    if let Ok(h) = w.hwnd() {
        let hw = windows::Win32::Foundation::HWND(h.0 as _);
        unsafe {
            let ex = GetWindowLongPtrW(hw, GWL_EXSTYLE);
            let next = if on {
                ex | WS_EX_NOACTIVATE
            } else {
                ex & !WS_EX_NOACTIVATE
            };
            SetWindowLongPtrW(hw, GWL_EXSTYLE, next);
        }
    }
}

/// 需要输入搜索时，显式把面板变成可激活并抢回焦点
pub fn activate_panel(app: &AppHandle) {
    let Some(w) = app.get_webview_window("panel") else {
        return;
    };
    keyhook::reset();
    set_no_activate(&w, false);
    if let Ok(h) = w.hwnd() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                windows::Win32::Foundation::HWND(h.0 as _),
            );
        }
    }
    let _ = w.set_focus();
}

pub fn toggle_panel(app: &AppHandle) {
    let Some(w) = app.get_webview_window("panel") else {
        return;
    };
    if panel_visible(&w) {
        hide_panel_now(&w);
        PANEL_PINNED.store(false, Ordering::SeqCst);
        return;
    }
    paste::capture_foreground();
    keyhook::reset();
    set_no_activate(&w, true);
    position_near_cursor(&w);
    PANEL_OPEN.store(true, Ordering::SeqCst);
    PANEL_SHOWN_AT.store(now_ms(), Ordering::SeqCst);
    if let Ok(h) = w.hwnd() {
        PANEL_HWND.store(h.0 as isize, Ordering::SeqCst);
    }
    apply_dwm_round(&w);
    let _ = app.emit("panel-shown", ());
}

#[cfg(windows)]
fn position_near_cursor(w: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return;
        }
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return;
        }
        let size = w
            .outer_size()
            .unwrap_or(tauri::PhysicalSize::new(410, 560));
        let (ww, wh) = (size.width as i32, size.height as i32);
        let work = mi.rcWork;
        let mut x = pt.x - ww / 2;
        let mut y = pt.y + 18;
        if y + wh > work.bottom {
            y = pt.y - wh - 18;
        }
        x = x.clamp(work.left, (work.right - ww).max(work.left));
        y = y.clamp(work.top, (work.bottom - wh).max(work.top));
        if let Ok(h) = w.hwnd() {
            let hw = windows::Win32::Foundation::HWND(h.0 as _);
            let res = SetWindowPos(
                hw,
                HWND_TOPMOST,
                x,
                y,
                0,
                0,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE,
            );
            if res.is_err() {
                eprintln!("[clipnest] 面板定位失败: {res:?}");
            }

        }
    }
}

#[cfg(not(windows))]
fn position_near_cursor(_w: &tauri::WebviewWindow) {}

/// 注册（或重新注册）全局热键
pub fn register_hotkey(app: &AppHandle, hotkey: &str) -> anyhow::Result<()> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if hotkey.trim().is_empty() {
        return Ok(());
    }
    gs.on_shortcut(hotkey, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_panel(app);
        }
    })?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let store = Arc::new(Store::open(
                &data_dir.join("clipnest.db"),
                data_dir.join("media"),
            )?);
            let cfg = settings::Settings::load(&data_dir);
            let paused = Arc::new(AtomicBool::new(cfg.paused));

            app.manage(AppState {
                store: store.clone(),
                settings: Mutex::new(cfg.clone()),
                data_dir: data_dir.clone(),
                paused: paused.clone(),
            });

            // 剪贴板监听（后台线程，事件驱动）
            let listener_store = store.clone();
            let listener_app = handle.clone();
            let listener_paused = paused.clone();
            let keep_count = cfg.keep_count;
            let keep_days = cfg.keep_days;
            clipboard::spawn_listener(move || {
                if listener_paused.load(Ordering::SeqCst) || clipboard::is_suppressed() {
                    return;
                }
                // 给源程序一点时间释放剪贴板，减少 CLIPBRD_E_CANT_OPEN
                std::thread::sleep(std::time::Duration::from_millis(35));
                let Some(payload) = clipboard::read_clipboard() else {
                    return;
                };
                match listener_store.insert(&payload) {
                    Ok(item) => {
                        let _ = listener_store.prune(keep_count, keep_days);
                        let _ = listener_app.emit("clip-added", &item);
                    }
                    Err(e) => eprintln!("[clipnest] insert failed: {e}"),
                }
            });

            // 面板默认不抢焦点；键盘导航走全局低级钩子
            if let Some(panel) = app.get_webview_window("panel") {
                set_no_activate(&panel, true);
                // WebView2 默认底色是白的，会把窗口的透明 padding 显示成灰白边
                let _ = panel.set_background_color(Some(tauri::window::Color::from((20, 21, 26, 255))));
                make_webview_transparent(&panel);
                apply_dwm_round(&panel);
            }
            // 钩子回调必须微秒级返回：只读原子变量 + 调 Win32，
            // 事件通过 channel 交给转发线程（低级钩子超 300ms 会被系统静默摘掉）
            let (tx, rx) = std::sync::mpsc::channel::<(String, bool)>();
            let emit_app = handle.clone();
            std::thread::spawn(move || {
                while let Ok((action, shift)) = rx.recv() {
                    let _ = emit_app.emit(
                        "panel-key",
                        serde_json::json!({ "action": action, "shift": shift }),
                    );
                }
            });

            keyhook::install(move |vk| {
                if !PANEL_OPEN.load(Ordering::SeqCst) || panel_is_active() {
                    return false;
                }
                let shift = key_down(0x10);
                let ctrl = key_down(0x11);
                let action = match vk {
                    0x1B => "esc",
                    0x26 => "up",
                    0x28 => "down",
                    0x0D => "enter",
                    0x2E => "delete",
                    0x50 if ctrl => "pin",
                    0x4C if ctrl => "toggle-pin",
                    0x31..=0x39 => {
                        let _ = tx.send((format!("num:{}", vk - 0x30), shift));
                        return true;
                    }
                    _ => return false,
                };
                let _ = tx.send((action.to_string(), shift));
                true
            });

            let _ = APP_HANDLE.set(handle.clone());

            // 固定模式下持续把面板提到最前（防止被其它 TOPMOST 窗口盖住）
            {
                let keep_app = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    if !PANEL_PINNED.load(Ordering::SeqCst) {
                        continue;
                    }
                    if let Some(w) = keep_app.get_webview_window("panel") {
                        if panel_visible(&w) {
                            raise_panel(&w);
                        }
                    }
                });
            }

            // TODO: 点击面板外部关闭（等固定键确认稳定后再接）

            build_tray(&handle)?;
            if let Err(e) = register_hotkey(&handle, &cfg.hotkey) {
                eprintln!("[clipnest] 热键注册失败: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(false) => {
                if window.label() == "panel" {
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::get_item,
            commands::paste_item,
            commands::copy_item,
            commands::delete_item,
            commands::clear_all,
            commands::set_flag,
            commands::set_note,
            commands::update_text,
            commands::stats,
            commands::get_settings,
            commands::save_settings,
            commands::set_paused,
            commands::hide_panel,
            commands::open_settings,
            commands::close_settings,
            commands::prune_now,
            commands::image_url,
            commands::activate_panel,
            commands::set_panel_pinned,
            commands::get_panel_pinned,
            commands::set_panel_width,
            commands::log_debug,
            commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("ClipNest 启动失败");
}


fn key_down(vk: i32) -> bool {
    unsafe {
        (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) as u16 & 0x8000) != 0
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "打开面板", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停记录", true, None::<&str>)?;
    let clear = MenuItem::with_id(app, "clear", "清空历史", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &settings_i,
            &pause,
            &clear,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ClipNest 剪贴板")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_panel(app),
            "settings" => {
                if let Some(w) = app.get_webview_window("settings") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "pause" => {
                let st = app.state::<AppState>();
                let now = !st.paused.load(Ordering::SeqCst);
                st.paused.store(now, Ordering::SeqCst);
                if let Ok(mut s) = st.settings.lock() {
                    s.paused = now;
                    let _ = s.save(&st.data_dir);
                }
                let _ = app.emit("paused-changed", now);
            }
            "clear" => {
                let st = app.state::<AppState>();
                let _ = st.store.clear(true);
                let _ = st.store.gc_media();
                let _ = app.emit("history-cleared", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_panel(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
