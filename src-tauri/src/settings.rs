use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 全局热键（Tauri 快捷键字符串）
    pub hotkey: String,
    /// 最多保留条数，0 表示不限制
    pub keep_count: i64,
    /// 最多保留天数，0 表示不限制
    pub keep_days: i64,
    /// 单条文本上限（KB）
    pub max_text_kb: i64,
    /// 粘贴后是否隐藏面板
    pub paste_hide: bool,
    /// 开机自启
    pub start_with_windows: bool,
    /// 面板主题：system / light / dark
    pub theme: String,
    /// 单击条目直接粘贴（否则只选中，靠回车粘贴）
    pub click_paste: bool,
    /// 隐私排除总开关
    pub privacy_filter: bool,
    /// 不记录的进程名单（换行/逗号分隔）
    pub excluded_apps: String,
    /// 跳过看起来像密钥/令牌/卡号的文本
    pub skip_sensitive: bool,
    /// 暂停监听
    pub paused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Alt+V".into(),
            keep_count: 2000,
            keep_days: 30,
            max_text_kb: 4096,
            paste_hide: true,
            start_with_windows: false,
            theme: "dark".into(),
            click_paste: false,
            privacy_filter: true,
            excluded_apps: "KeePass.exe\nKeePassXC.exe\n1Password.exe\nBitwarden.exe\nLastPass.exe\nDashlane.exe\nEnpass.exe\nKeeper.exe\nNordPass.exe\nRoboForm.exe".into(),
            skip_sensitive: true,
            paused: false,
        }
    }
}

impl Settings {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load(dir: &Path) -> Self {
        let p = Self::path(dir);
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let p = Self::path(dir);
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(windows)]
pub fn set_autostart(enable: bool) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let exe = std::env::current_exe()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_ALL_ACCESS,
    )?;
    if enable {
        run.set_value("ClipNest", &exe.to_string_lossy().to_string())?;
    } else {
        let _ = run.delete_value("ClipNest");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_autostart(_enable: bool) -> Result<()> {
    Ok(())
}
