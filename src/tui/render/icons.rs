#[cfg(target_os = "windows")]
pub(crate) mod win {
    pub const TIP: &str = "TIP: ";
    pub const SAVE: &str = "";
    pub const CHAT_MODE: &str = " CHAT ";
    pub const SETTINGS_MODE: &str = " SETTINGS ";
    pub const MODELS_MODE: &str = " MODELS ";
    pub const SECURITY_MODE: &str = " SECURITY ";
    pub const SESSIONS_MODE: &str = " SESSIONS ";
    pub const ASK_USER_MODE: &str = " ASK USER ";
    pub const CPU: &str = "";
    pub const IDLE: &str = "OK";
    pub const CHECK_ENABLED: &str = "Enabled";
    pub const CROSS_DISABLED: &str = "Disabled";
    pub const CHECK_SHOW_FULL: &str = "Show Full";
    pub const CROSS_LABEL_ONLY: &str = "Label Only";
    pub const SHELL_OK: &str = "+";
    pub const SHELL_ERR: &str = "-";
    pub const ACTIVE_MARKER: &str = " > ";
    pub const INACTIVE_MARKER: &str = "   ";
}
#[cfg(target_os = "windows")]
pub(crate) use win as icons;

#[cfg(not(target_os = "windows"))]
pub(crate) mod unix {
    pub const TIP: &str = "  TIP: ";
    pub const SAVE: &str = "  ";
    pub const CHAT_MODE: &str = " 󰍡 CHAT ";
    pub const SETTINGS_MODE: &str = "  SETTINGS ";
    pub const MODELS_MODE: &str = "  MODELS ";
    pub const SECURITY_MODE: &str = "  SECURITY ";
    pub const SESSIONS_MODE: &str = "  SESSIONS ";
    pub const ASK_USER_MODE: &str = "  ASK USER ";
    pub const CPU: &str = " ";
    pub const IDLE: &str = "";
    pub const CHECK_ENABLED: &str = "✔ Enabled";
    pub const CROSS_DISABLED: &str = "✗ Disabled";
    pub const CHECK_SHOW_FULL: &str = "✔ Show Full";
    pub const CROSS_LABEL_ONLY: &str = "✗ Label Only";
    pub const SHELL_OK: &str = "✓";
    pub const SHELL_ERR: &str = "✗";
    pub const ACTIVE_MARKER: &str = "  ";
    pub const INACTIVE_MARKER: &str = "   ";
}
#[cfg(not(target_os = "windows"))]
pub(crate) use unix as icons;
