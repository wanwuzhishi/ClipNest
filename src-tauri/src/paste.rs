//! 记录前台窗口与焦点控件，粘贴时把它们还原
#![cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    SetForegroundWindow,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};

static LAST_FG: AtomicIsize = AtomicIsize::new(0);
static LAST_FOCUS: AtomicIsize = AtomicIsize::new(0);

const VK_CONTROL: u16 = 0x11;
const VK_V: u16 = 0x56;
const VK_SHIFT: u16 = 0x10;

fn as_hwnd(v: isize) -> Option<HWND> {
    if v == 0 {
        None
    } else {
        Some(HWND(v as *mut std::ffi::c_void))
    }
}

/// 弹出面板前调用：记住前台窗口，以及它内部真正有焦点的控件
pub fn capture_foreground() {
    unsafe {
        let fg = GetForegroundWindow();
        LAST_FG.store(fg.0 as isize, Ordering::SeqCst);
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(tid, &mut gi).is_ok() && !gi.hwndFocus.0.is_null() {
            LAST_FOCUS.store(gi.hwndFocus.0 as isize, Ordering::SeqCst);
        } else {
            LAST_FOCUS.store(0, Ordering::SeqCst);
        }
    }
}

fn last_foreground() -> Option<HWND> {
    as_hwnd(LAST_FG.load(Ordering::SeqCst))
}

fn last_focus() -> Option<HWND> {
    as_hwnd(LAST_FOCUS.load(Ordering::SeqCst))
}

fn key_event(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 发送 Ctrl+V（plain 时用 Ctrl+Shift+V 走纯文本粘贴）
pub fn send_paste(plain: bool) {
    // 热键触发后 Alt 有时会残留按下状态，先把所有修饰键弹起来
    let mut seq: Vec<INPUT> = [0x12u16, 0x11, 0x10, 0x5B, 0x5C]
        .iter()
        .map(|&vk| key_event(vk, true))
        .collect();
    seq.push(key_event(VK_CONTROL, false));
    if plain {
        seq.push(key_event(VK_SHIFT, false));
    }
    seq.push(key_event(VK_V, false));
    seq.push(key_event(VK_V, true));
    if plain {
        seq.push(key_event(VK_SHIFT, true));
    }
    seq.push(key_event(VK_CONTROL, true));
    let _ = unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}


/// 还原目标窗口和控件焦点后粘贴
pub fn paste_to_previous(plain: bool) {
    std::thread::spawn(move || unsafe {
        let target = last_foreground();
        let focus = last_focus();
        // 大多数情况下面板从未激活，前台仍是目标窗口，直接发键即可
        let mut wait_ms = 6u64;
        if let Some(h) = target {
            if GetForegroundWindow() != h {
                let _ = SetForegroundWindow(h);
                wait_ms = 45;
            }
            if let Some(f) = focus {
                // 只有真正的子控件才需要接上输入队列设焦点
                if f != h {
                    let tid = GetWindowThreadProcessId(h, None);
                    let cur = GetCurrentThreadId();
                    let _ = AttachThreadInput(cur, tid, true);
                    let _ = SetFocus(f);
                    let _ = AttachThreadInput(cur, tid, false);
                    wait_ms = wait_ms.max(18);
                }
            }
        }
        if wait_ms > 0 {
            std::thread::sleep(Duration::from_millis(wait_ms));
        }
        send_paste(plain);
    });
}
