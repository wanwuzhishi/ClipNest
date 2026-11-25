use crate::clipboard;
use crate::paste;
use crate::store::ClipItem;
use crate::{settings::Settings, AppState};
use tauri::{AppHandle, Emitter, Manager, State};

type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn list_items(state: State<'_, AppState>, query: String, kind: String, limit: i64) -> R<Vec<ClipItem>> {
    state.store.list(&query, &kind, limit).map_err(err)
}

#[tauri::command]
pub fn get_item(state: State<'_, AppState>, id: i64) -> R<Option<ClipItem>> {
    state.store.get(id).map_err(err)
}

/// 写回剪贴板并粘贴到之前的窗口
#[tauri::command]
pub fn paste_item(app: AppHandle, state: State<'_, AppState>, id: i64, plain: bool) -> R<()> {
    let item = state.store.get(id).map_err(err)?.ok_or("记录不存在")?;
    let text = if plain {
        item.text.clone().or_else(|| Some(item.preview.clone()))
    } else {
        item.text.clone()
    };
    let html = if plain { None } else { item.html.clone() };
    let rtf = if plain { None } else { item.rtf.clone() };
    let png = item.image_path.as_ref().and_then(|p| std::fs::read(p).ok());
    let files = item.files.clone();

    clipboard::write_clipboard(
        text.as_deref(),
        html.as_deref(),
        rtf.as_deref(),
        png.as_deref(),
        files.as_deref(),
    );

    let hide = { state.settings.lock().unwrap().paste_hide };
    if hide {
        if let Some(w) = app.get_webview_window("panel") {
            let _ = w.hide();
        }
    }
    paste::paste_to_previous(plain);
    Ok(())
}

/// 只复制到剪贴板，不做粘贴动作
#[tauri::command]
pub fn copy_item(state: State<'_, AppState>, id: i64, plain: bool) -> R<()> {
    let item = state.store.get(id).map_err(err)?.ok_or("记录不存在")?;
    let text = if plain {
        item.text.clone().or_else(|| Some(item.preview.clone()))
    } else {
        item.text.clone()
    };
    let html = if plain { None } else { item.html.clone() };
    let rtf = if plain { None } else { item.rtf.clone() };
    let png = item.image_path.as_ref().and_then(|p| std::fs::read(p).ok());
    let files = item.files.clone();
    clipboard::write_clipboard(
        text.as_deref(),
        html.as_deref(),
        rtf.as_deref(),
        png.as_deref(),
        files.as_deref(),
    );
    Ok(())
}

#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, id: i64) -> R<()> {
    state.store.delete(id).map_err(err)
}

#[tauri::command]
pub fn clear_all(state: State<'_, AppState>, keep_pinned: bool) -> R<usize> {
    let removed = state.store.clear(keep_pinned).map_err(err)?;
    let _ = state.store.gc_media();
    Ok(removed)
}

#[tauri::command]
pub fn set_flag(state: State<'_, AppState>, id: i64, field: String, value: bool) -> R<()> {
    state.store.set_flag(id, &field, value).map_err(err)
}

#[tauri::command]
pub fn set_note(state: State<'_, AppState>, id: i64, note: String) -> R<()> {
    state.store.set_note(id, &note).map_err(err)
}

#[tauri::command]
pub fn update_text(state: State<'_, AppState>, id: i64, text: String) -> R<()> {
    state.store.update_text(id, &text).map_err(err)
}

#[tauri::command]
pub fn stats(state: State<'_, AppState>) -> R<(i64, i64)> {
    state.store.stats().map_err(err)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> R<Settings> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> R<()> {
    {
        let mut cur = state.settings.lock().unwrap();
        *cur = settings.clone();
        cur.save(&state.data_dir).map_err(err)?;
    }
    let _ = crate::settings::set_autostart(settings.start_with_windows);
    crate::register_hotkey(&app, &settings.hotkey).map_err(err)?;
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn set_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> R<()> {
    state.paused.store(paused, std::sync::atomic::Ordering::SeqCst);
    {
        let mut s = state.settings.lock().unwrap();
        s.paused = paused;
        let _ = s.save(&state.data_dir);
    }
    let _ = app.emit("paused-changed", paused);
    Ok(())
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn close_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn prune_now(state: State<'_, AppState>) -> R<usize> {
    let s = state.settings.lock().unwrap().clone();
    let n = state.store.prune(s.keep_count, s.keep_days).map_err(err)?;
    let _ = state.store.gc_media();
    Ok(n)
}

/// 让前端拿到 asset 协议可用的图片地址（失败时返回 None）
#[tauri::command]
pub fn image_url(path: String) -> R<String> {
    Ok(path)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
