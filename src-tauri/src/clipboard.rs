//! Windows 剪贴板读写与变更监听
#![cfg(windows)]

use crate::store::Payload;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    RegisterClassW, TranslateMessage, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE,
    WNDCLASSW,
};

const CF_TEXT: u32 = 1;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_format(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

/// 同一进程内共享的“忽略本次变更”时间点（毫秒）
static IGNORE_UNTIL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 自己往剪贴板写内容前调用，避免把这次写入又记一遍
pub fn suppress_for(ms: i64) {
    IGNORE_UNTIL.store(now_ms() + ms, std::sync::atomic::Ordering::SeqCst);
}

pub fn is_suppressed() -> bool {
    now_ms() < IGNORE_UNTIL.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn open(retries: u32) -> bool {
    for _ in 0..retries {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    false
}

unsafe fn read_unicode(h: HANDLE) -> Option<String> {
    let hg = HGLOBAL(h.0);
    let ptr = GlobalLock(hg) as *const u16;
    if ptr.is_null() {
        return None;
    }
    let bytes = GlobalSize(hg);
    let len = bytes / 2;
    let slice = std::slice::from_raw_parts(ptr, len);
    let end = slice.iter().position(|&c| c == 0).unwrap_or(len);
    let s = String::from_utf16_lossy(&slice[..end]);
    let _ = GlobalUnlock(hg);
    Some(s)
}

unsafe fn read_bytes(fmt: u32) -> Option<Vec<u8>> {
    let h = GetClipboardData(fmt).ok()?;
    if h.is_invalid() {
        return None;
    }
    let hg = HGLOBAL(h.0);
    let ptr = GlobalLock(hg) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let len = GlobalSize(hg);
    let data = std::slice::from_raw_parts(ptr, len).to_vec();
    let _ = GlobalUnlock(hg);
    Some(data)
}

/// 从 "HTML Format" 的头部里截出真正的 HTML 片段
fn parse_html_format(raw: &str) -> String {
    let frag_start = header_value(raw, "StartFragment");
    let frag_end = header_value(raw, "EndFragment");
    match (frag_start, frag_end) {
        (Some(s), Some(e)) if s < e && e <= raw.len() => raw[s..e].to_string(),
        _ => raw.to_string(),
    }
}

fn header_value(raw: &str, key: &str) -> Option<usize> {
    let pos = raw.find(&format!("{}:", key))?;
    let rest = &raw[pos + key.len() + 1..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn read_files(h: HANDLE) -> Option<Vec<String>> {
    let hdrop = HDROP(h.0);
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut out = Vec::new();
    for i in 0..count {
        let len = unsafe { DragQueryFileW(hdrop, i, None) };
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len as usize + 1];
        unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// CF_DIB / CF_DIBV5 → PNG（只处理常见的 24/32 位无压缩位图）
pub fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }
    let u32_at = |o: usize| u32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
    let i32_at = |o: usize| i32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
    let u16_at = |o: usize| u16::from_le_bytes([dib[o], dib[o + 1]]);

    let hdr = u32_at(0) as usize;
    let width = i32_at(4);
    let height = i32_at(8);
    let bpp = u16_at(14);
    let compression = u32_at(16);
    // BI_RGB(0) 与 32 位 BI_BITFIELDS(3) 都按 BGRA 处理
    let supported = compression == 0 || (compression == 3 && bpp == 32);
    if width <= 0 || height == 0 || !supported || (bpp != 24 && bpp != 32) {
        return None;
    }
    let w = width as usize;
    let h = height.unsigned_abs() as usize;
    if w > 20000 || h > 20000 || hdr < 40 || hdr > dib.len() {
        return None;
    }
    let stride = ((w * bpp as usize + 31) / 32) * 4;
    let off = hdr;
    if dib.len() < off + stride * h {
        return None;
    }
    let top_down = height < 0;
    let mut img = image::RgbaImage::new(w as u32, h as u32);
    let mut all_zero_alpha = true;
    for y in 0..h {
        let src_row = if top_down { y } else { h - 1 - y };
        let base = off + src_row * stride;
        for x in 0..w {
            let p = base + x * (bpp as usize / 8);
            let b = dib[p] as u8;
            let g = dib[p + 1] as u8;
            let r = dib[p + 2] as u8;
            let a = if bpp == 32 { dib[p + 3] } else { 255 };
            if a != 0 {
                all_zero_alpha = false;
            }
            img.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, a]));
        }
    }
    if all_zero_alpha {
        for px in img.pixels_mut() {
            px[3] = 255;
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// 读取当前剪贴板内容；被自己写入触发的变更会被忽略
pub fn read_clipboard() -> Option<Payload> {
    if is_suppressed() {
        return None;
    }
    if !open(10) {
        return None;
    }
    // 密码管理器等会放这个格式，表示"别记录我"
    if crate::privacy::has_ignore_marker() {
        unsafe {
            let _ = CloseClipboard();
        }
        return None;
    }
    let mut payload = Payload::default();

    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_ok() {
            if let Ok(h) = GetClipboardData(CF_HDROP) {
                payload.files = read_files(h);
            }
        }
        if payload.files.is_none() {
            let png_fmt = reg_format("PNG");
            if png_fmt != 0 && IsClipboardFormatAvailable(png_fmt).is_ok() {
                payload.png = read_bytes(png_fmt);
            }
            if payload.png.is_none() {
                let fmt = if IsClipboardFormatAvailable(CF_DIBV5).is_ok() {
                    CF_DIBV5
                } else if IsClipboardFormatAvailable(CF_DIB).is_ok() {
                    CF_DIB
                } else {
                    0
                };
                if fmt != 0 {
                    if let Some(dib) = read_bytes(fmt) {
                        payload.png = dib_to_png(&dib);
                    }
                }
            }
            if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
                    payload.text = read_unicode(h).map(|s| {
                        if s.len() > 4_000_000 {
                            s.chars().take(4_000_000).collect()
                        } else {
                            s
                        }
                    });
                }
            } else if IsClipboardFormatAvailable(CF_TEXT).is_ok() {
                if let Some(bytes) = read_bytes(CF_TEXT) {
                    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                    payload.text = Some(String::from_utf8_lossy(&bytes[..end]).to_string());
                }
            }
            let html_fmt = reg_format("HTML Format");
            if html_fmt != 0 && IsClipboardFormatAvailable(html_fmt).is_ok() {
                if let Some(bytes) = read_bytes(html_fmt) {
                    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                    let raw = String::from_utf8_lossy(&bytes[..end]).to_string();
                    payload.html = Some(parse_html_format(&raw));
                }
            }
            let rtf_fmt = reg_format("Rich Text Format");
            if rtf_fmt != 0 && IsClipboardFormatAvailable(rtf_fmt).is_ok() {
                if let Some(bytes) = read_bytes(rtf_fmt) {
                    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                    payload.rtf = Some(String::from_utf8_lossy(&bytes[..end]).to_string());
                }
            }
        }
    }
    payload.source_app = crate::privacy::clipboard_owner_app();
    unsafe {
        let _ = CloseClipboard();
    }

    let empty = payload.files.is_none()
        && payload.png.is_none()
        && payload.text.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true)
        && payload.html.is_none();
    if empty {
        None
    } else {
        Some(payload)
    }
}

unsafe fn set_bytes(fmt: u32, data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let h = match GlobalAlloc(GMEM_MOVEABLE, data.len()) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let ptr = GlobalLock(h) as *mut u8;
    if ptr.is_null() {
        let _ = GlobalFree(h);
        return false;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    let _ = GlobalUnlock(h);
    if SetClipboardData(fmt, HANDLE(h.0)).is_err() {
        let _ = GlobalFree(h);
        return false;
    }
    true
}

fn utf16_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2 + 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// 按 Windows HTML Format 规范拼出带正确偏移头的字符串
pub fn build_html_format(frag: &str) -> String {
    let header_len = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n".len();
    let pre = "<html><body><!--StartFragment-->";
    let post = "<!--EndFragment--></body></html>";
    let start_html = header_len;
    let start_frag = start_html + pre.len();
    let end_frag = start_frag + frag.len();
    let end_html = end_frag + post.len();
    let mut s = format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n",
        start_html, end_html, start_frag, end_frag
    );
    s.push_str(pre);
    s.push_str(frag);
    s.push_str(post);
    s
}

fn png_to_dib(png: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let stride = w as usize * 4;
    let mut out = Vec::with_capacity(40 + stride * h as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((stride * h as usize) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..h).rev() {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    Some(out)
}

fn build_hdrop(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + paths.len() * 64);
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    for p in paths {
        for u in p.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// 把内容写回剪贴板，尽可能多写几种格式，保证粘贴时格式不丢
pub fn write_clipboard(
    text: Option<&str>,
    html: Option<&str>,
    rtf: Option<&str>,
    png: Option<&[u8]>,
    files: Option<&[String]>,
) -> bool {
    suppress_for(1500);
    if !open(12) {
        return false;
    }
    let mut ok = false;
    unsafe {
        let _ = EmptyClipboard();
        if let Some(paths) = files {
            let data = build_hdrop(paths);
            ok |= set_bytes(CF_HDROP, &data);
        } else {
            if let Some(p) = png {
                let png_fmt = reg_format("PNG");
                if png_fmt != 0 {
                    ok |= set_bytes(png_fmt, p);
                }
                if let Some(dib) = png_to_dib(p) {
                    ok |= set_bytes(CF_DIB, &dib);
                }
            }
            if let Some(t) = text {
                ok |= set_bytes(CF_UNICODETEXT, &utf16_bytes(t));
            }
            if let Some(h) = html {
                let full = build_html_format(h);
                let f = reg_format("HTML Format");
                if f != 0 {
                    ok |= set_bytes(f, &utf16_bytes(&full));
                }
            }
            if let Some(r) = rtf {
                let f = reg_format("Rich Text Format");
                if f != 0 {
                    ok |= set_bytes(f, &utf16_bytes(r));
                }
            }
        }
        let _ = CloseClipboard();
    }
    ok
}

static CALLBACK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if let Some(cb) = CALLBACK.get() {
            cb();
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// 启动剪贴板监听线程（消息循环 + AddClipboardFormatListener，空闲零 CPU）
pub fn spawn_listener<F: Fn() + Send + Sync + 'static>(f: F) {
    let _ = CALLBACK.set(Box::new(f));
    std::thread::spawn(|| unsafe {
        let hinst = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };
        let class_name = wide("ClipNestClipboardListener");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            HINSTANCE(hinst.0),
            None,
        );
        if let Ok(h) = hwnd {
            let _ = AddClipboardFormatListener(h);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}
