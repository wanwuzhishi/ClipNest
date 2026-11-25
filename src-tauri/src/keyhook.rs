//! 全局低级键盘钩子：面板未激活时把导航键转给前端
#![cfg(windows)]

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, TranslateMessage, DispatchMessageW,
    HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

type Handler = Box<dyn Fn(u32) -> bool + Send + Sync>;

static HANDLER: OnceLock<Handler> = OnceLock::new();
static SWALLOWED: Mutex<Option<HashSet<u32>>> = Mutex::new(None);

fn swallowed() -> std::sync::MutexGuard<'static, Option<HashSet<u32>>> {
    SWALLOWED.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let is_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
        let is_up = wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize;
        let mut state = swallowed();

        if is_down {
            let handled = HANDLER.get().map(|h| h(vk)).unwrap_or(false);
            if handled {
                state.get_or_insert_with(HashSet::new).insert(vk);
                return LRESULT(1);
            }
        } else if is_up {
            if let Some(set) = state.as_mut() {
                if set.remove(&vk) {
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// 安装钩子。handler 返回 true 表示吞掉该按键。
pub fn install<F>(handler: F)
where
    F: Fn(u32) -> bool + Send + Sync + 'static,
{
    let _ = HANDLER.set(Box::new(handler));
    std::thread::spawn(|| unsafe {
        let _hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// 面板被真正激活时清掉残留状态
pub fn reset() {
    if let Some(set) = swallowed().as_mut() {
        set.clear();
    }
}
