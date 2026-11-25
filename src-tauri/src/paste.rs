//! 记录前台窗口并把内容“粘”回目标应用
#![cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

static LAST_FG: AtomicIsize = AtomicIsize::new(0);

const VK_CONTROL: u16 = 0x11;
const VK_V: u16 = 0x56;
const VK_SHIFT: u16 = 0x10;

/// 弹出面板前调用，记住当时的前台窗口
pub fn capture_foreground() {
    let h = unsafe { GetForegroundWindow() };
    LAST_FG.store(h.0 as isize, Ordering::SeqCst);
}

pub fn last_foreground() -> Option<HWND> {
    let v = LAST_FG.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        Some(HWND(v as *mut std::ffi::c_void))
    }
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

/// 发送 Ctrl+V（或 Ctrl+Shift+V 走纯文本粘贴）
pub fn send_paste(plain: bool) {
    let mut seq = vec![key_event(VK_CONTROL, false)];
    if plain {
        seq.push(key_event(VK_SHIFT, false));
    }
    seq.push(key_event(VK_V, false));
    seq.push(key_event(VK_V, true));
    if plain {
        seq.push(key_event(VK_SHIFT, true));
    }
    seq.push(key_event(VK_CONTROL, true));
    unsafe {
        SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
    }
}

/// 回到之前的窗口并粘贴；面板需要先隐藏
pub fn paste_to_previous(plain: bool) {
    std::thread::spawn(move || {
        if let Some(h) = last_foreground() {
            unsafe {
                let _ = SetForegroundWindow(h);
            }
        }
        std::thread::sleep(Duration::from_millis(70));
        send_paste(plain);
    });
}
