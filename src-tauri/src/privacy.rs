//! 隐私过滤：进程排除、剪贴板忽略标记、敏感内容识别
#![cfg(windows)]

use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::DataExchange::GetClipboardOwner;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// 取剪贴板所有者进程的 exe 文件名（如 "KeePass.exe"）
pub fn clipboard_owner_app() -> Option<String> {
    unsafe {
        let hwnd = GetClipboardOwner().ok()?;
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        res.ok()?;
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        path.rsplit(std::path::MAIN_SEPARATOR).next().map(|s| s.to_string())
    }
}

/// 剪贴板里是否带"不要记录我"的标记（密码管理器常用）
pub fn has_ignore_marker() -> bool {
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    use windows::core::PCWSTR;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    for name in [
        "ExcludeClipboardContentFromMonitorProcessing",
        "Clipboard Viewer Ignore",
    ] {
        let w = wide(name);
        let fmt = unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) };
        if fmt != 0 && unsafe { IsClipboardFormatAvailable(fmt) }.is_ok() {
            return true;
        }
    }
    false
}

/// 进程是否在排除名单里（大小写不敏感，支持带/不带 .exe）
pub fn is_excluded(app: &str, list: &str) -> bool {
    let app_lower = app.to_lowercase();
    let app_no_ext = app_lower.strip_suffix(".exe").unwrap_or(&app_lower);
    for raw in list.split(['\n', ',', ';']) {
        let item = raw.trim().to_lowercase();
        if item.is_empty() {
            continue;
        }
        let item_no_ext = item.strip_suffix(".exe").unwrap_or(&item);
        if item_no_ext == app_no_ext {
            return true;
        }
    }
    false
}

fn luhn_ok(digits: &str) -> bool {
    let mut sum = 0u32;
    let mut alt = false;
    for c in digits.chars().rev() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if alt {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
        alt = !alt;
    }
    sum % 10 == 0
}

fn looks_like_card(t: &str) -> bool {
    if !t
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-' || c == '\t')
    {
        return false;
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    luhn_ok(&digits)
}

fn looks_like_jwt(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts[0].len() > 10
        && parts[1].len() > 10
        && parts[2].len() > 10
}

/// 粗判一段文本是不是密钥/令牌/卡号
pub fn looks_sensitive(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 12 || t.len() > 8192 {
        return false;
    }
    if t.contains("PRIVATE KEY-----") && t.contains("BEGIN") {
        return true;
    }
    if t.starts_with("AKIA") && t.len() == 20 && t[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    for p in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if t.starts_with(p) && t.len() > 30 {
            return true;
        }
    }
    for p in ["xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-"] {
        if t.starts_with(p) && t.len() > 20 {
            return true;
        }
    }
    if t.starts_with("sk-") && t.len() > 40 {
        return true;
    }
    if looks_like_jwt(t) {
        return true;
    }
    if looks_like_card(t) {
        return true;
    }
    false
}
