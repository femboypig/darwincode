use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub enable_codebase_tools: bool,
    #[serde(default)]
    pub enable_bash_tools: bool,
    #[serde(default)]
    pub show_thoughts: bool,
    #[serde(default)]
    pub permission_level: PermissionLevel,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_true")]
    pub respect_ignore_rules: bool,
    #[serde(default)]
    pub trust_workspace: bool,
    #[serde(default)]
    pub trusted_workspaces: Vec<String>,
    #[serde(skip)]
    pub active_agent: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Theme {
    #[default]
    Auto,
    Dark,
    Light,
    Custom(String),
}

impl<'de> serde::Deserialize<'de> for Theme {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            _ => Ok(Self::Custom(s)),
        }
    }
}

impl serde::Serialize for Theme {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Dark => serializer.serialize_str("dark"),
            Self::Light => serializer.serialize_str("light"),
            Self::Custom(name) => serializer.serialize_str(name),
        }
    }
}

impl Theme {
    pub fn label(&self) -> String {
        match self {
            Self::Auto => "Auto (System/Term)".to_string(),
            Self::Dark => "Dark".to_string(),
            Self::Light => "Light".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Auto => Self::Dark,
            Self::Dark => Self::Light,
            Self::Light => Self::Auto,
            Self::Custom(_) => Self::Auto,
        }
    }
}

fn resolve_auto_theme() -> Theme {
    static AUTO_THEME_CACHE: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    AUTO_THEME_CACHE
        .get_or_init(|| {
            #[cfg(target_os = "windows")]
            {
                if let Ok(output) = std::process::Command::new("reg")
                    .args(&[
                        "query",
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                        "/v",
                        "AppsUseLightTheme",
                    ])
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.contains("0x1") || stdout.contains("1") {
                        return Theme::Light;
                    }
                }
                Theme::Dark
            }
            #[cfg(target_os = "macos")]
            {
                if let Ok(output) = std::process::Command::new("defaults")
                    .args(&["read", "-g", "AppleInterfaceStyle"])
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .to_lowercase();
                    if stdout.contains("dark") {
                        return Theme::Dark;
                    }
                }
                Theme::Light
            }
            #[cfg(target_os = "linux")]
            {
                // 1. Check COLORFGBG environment variable
                if let Ok(colorfgbg) = std::env::var("COLORFGBG")
                    && let Some(bg) = colorfgbg.split(';').next_back()
                    && let Ok(bg_num) = bg.parse::<i32>()
                {
                    let is_light = bg_num == 7 || (9..=15).contains(&bg_num);
                    if is_light {
                        return Theme::Light;
                    } else {
                        return Theme::Dark;
                    }
                }
                // 2. Check gsettings as a fallback (GNOME/Ubuntu preference)
                if let Ok(output) = std::process::Command::new("gsettings")
                    .args(["get", "org.gnome.desktop.interface", "color-scheme"])
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .to_lowercase();
                    if stdout.contains("prefer-dark") {
                        return Theme::Dark;
                    } else if stdout.contains("prefer-light") {
                        return Theme::Light;
                    }
                }
                Theme::Dark
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
            {
                Theme::Dark
            }
        })
        .clone()
}

pub fn resolve_theme(theme: &Theme) -> Theme {
    match theme {
        Theme::Dark => Theme::Dark,
        Theme::Light => Theme::Light,
        Theme::Custom(name) => Theme::Custom(name.clone()),
        Theme::Auto => resolve_auto_theme(),
    }
}

pub fn resolve_theme_mode(theme: &Theme) -> crate::tui::theme::ThemeMode {
    match theme {
        Theme::Dark => crate::tui::theme::ThemeMode::Dark,
        Theme::Light => crate::tui::theme::ThemeMode::Light,
        Theme::Custom(_) | Theme::Auto => {
            if resolve_auto_theme() == Theme::Light {
                crate::tui::theme::ThemeMode::Light
            } else {
                crate::tui::theme::ThemeMode::Dark
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum PermissionLevel {
    /// Restricted: Read-only codebase, No bash.
    Safe,
    /// Wary: Always ask (Default).
    #[default]
    Guardian,
    /// Full: Auto-execute everything.
    Chaos,
}

impl PermissionLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Safe => "Safe (Read-Only)",
            Self::Guardian => "Guardian (Ask)",
            Self::Chaos => "Chaos (Auto)",
        }
    }

    #[allow(dead_code)]
    pub fn next(self) -> Self {
        match self {
            Self::Safe => Self::Guardian,
            Self::Guardian => Self::Chaos,
            Self::Chaos => Self::Safe,
        }
    }
}

impl StoredConfig {
    pub fn load() -> Result<Option<Self>> {
        let path = config_path()?;
        let mut config = if path.exists() {
            if crate::crypto::is_home_appdata_missing() {
                let plain_data = fs::read(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let cfg: StoredConfig = serde_json::from_slice(&plain_data)
                    .with_context(|| format!("failed to parse config {}", path.display()))?;
                Some(cfg)
            } else {
                let key = crate::crypto::derive_hardware_key()?;
                let cipher_data = fs::read(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let plain_data = crate::crypto::decrypt_data(&cipher_data, &key)
                    .with_context(|| format!("failed to decrypt config {}", path.display()))?;

                let mut cfg: StoredConfig = serde_json::from_slice(&plain_data)
                    .with_context(|| format!("failed to parse config {}", path.display()))?;

                // Load secret from OS securely if available, otherwise use fallback from encrypted file
                if let Ok(entry) = keyring::Entry::new("darwincode", "api_key")
                    && let Ok(secret) = entry.get_password()
                    && !secret.trim().is_empty()
                {
                    cfg.api_key = secret;
                }
                Some(cfg)
            }
        } else {
            None
        };

        // Merge with project-level config if it exists
        if let Some(local_path) = find_project_config()
            && let Ok(local_data) = fs::read_to_string(&local_path)
            && let Ok(local_val) = serde_json::from_str::<serde_json::Value>(&local_data)
        {
            let base_config = config.clone().unwrap_or_default();
            if let Ok(mut config_val) = serde_json::to_value(&base_config) {
                merge_json_values(&mut config_val, local_val);
                if let Ok(merged_config) = serde_json::from_value::<StoredConfig>(config_val) {
                    config = Some(merged_config);
                }
            }
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let mut normalized_config = self.clone();
        normalized_config.base_url = normalized_config
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_owned();
        normalized_config.validate()?;

        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        if crate::crypto::is_home_appdata_missing() {
            let plain_data = serde_json::to_vec(&normalized_config)?;
            let mut file = secure_config_file(&path)?;
            file.write_all(&plain_data)
                .with_context(|| format!("failed to write {}", path.display()))?;
            return Ok(());
        }

        // Save secret to OS keyring securely (if available)
        let mut keyring_succeeded = false;
        if let Ok(entry) = keyring::Entry::new("darwincode", "api_key")
            && entry.set_password(&normalized_config.api_key).is_ok()
        {
            keyring_succeeded = true;
        }

        // Encrypt the configuration file on disk with the secret field stripped ONLY if stored in keyring
        let mut file_config = normalized_config.clone();
        if keyring_succeeded {
            file_config.api_key = String::new(); // Strip plain text secret from disk representation
        }

        let key = crate::crypto::derive_hardware_key()?;
        let plain_data = serde_json::to_vec(&file_config)?;
        let encrypted_data = crate::crypto::encrypt_data(&plain_data, &key)?;

        let mut file = secure_config_file(&path)?;
        file.write_all(&encrypted_data)
            .with_context(|| format!("failed to write {}", path.display()))?;

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.api_key.trim().is_empty() {
            bail!("API key cannot be empty");
        }

        if self.model.trim().is_empty() {
            bail!("model cannot be empty");
        }

        let url_str = self.base_url.trim();
        if url_str.is_empty() {
            bail!("base URL cannot be empty");
        }

        let url_str_trimmed = url_str.trim_end_matches('/');
        if !url_str_trimmed.starts_with("http://") && !url_str_trimmed.starts_with("https://") {
            bail!("base URL must start with http:// or https://");
        }
        if url_str_trimmed.contains(' ') || url_str_trimmed.len() < 8 {
            bail!("base URL is not a valid format");
        }

        if self.api_key.starts_with("sk-") {
            if url_str_trimmed == "https://generativelanguage.googleapis.com/v1beta" {
                bail!(
                    "For OpenAI/OmniRoute keys (starting with sk-), you must specify an OpenAI-compatible Base URL (e.g. http://localhost:20128/v1)"
                );
            }
            if self.model == "gemini-2.0-flash" {
                bail!(
                    "For OpenAI/OmniRoute keys (starting with sk-), you must specify an OpenAI-compatible Model (e.g. claude-sonnet-4.6)"
                );
            }
        }

        Ok(())
    }

    pub fn is_workspace_trusted(&self) -> bool {
        if self.trust_workspace {
            return true;
        }
        if let Some(proj_root) = find_project_root() {
            let proj_path = std::fs::canonicalize(&proj_root)
                .unwrap_or(proj_root)
                .to_string_lossy()
                .to_string();
            return self.trusted_workspaces.contains(&proj_path);
        }
        false
    }
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gemini-2.0-flash".to_owned(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_owned(),
            enable_codebase_tools: false,
            enable_bash_tools: false,
            show_thoughts: true,
            permission_level: PermissionLevel::Guardian,
            theme: Theme::Auto,
            respect_ignore_rules: true,
            trust_workspace: false,
            trusted_workspaces: Vec::new(),
            active_agent: None,
        }
    }
}

#[cfg(test)]
pub static TEST_CONFIG_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn config_path() -> Result<PathBuf> {
    #[cfg(test)]
    {
        if let Ok(guard) = TEST_CONFIG_DIR.lock()
            && let Some(ref path) = *guard
        {
            return Ok(path.join("config.json"));
        }
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".config"))
        });

    if let Some(base_path) = base {
        Ok(base_path.join("darwincode").join("config.json"))
    } else {
        let root = find_project_root().unwrap_or_else(|| PathBuf::from("."));
        Ok(root.join("config.json"))
    }
}

#[cfg(unix)]
fn secure_config_file(path: &PathBuf) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))
}

#[cfg(not(unix))]
fn secure_config_file(path: &PathBuf) -> Result<fs::File> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))
}

pub fn find_project_root() -> Option<PathBuf> {
    let mut cwd = std::env::current_dir().ok()?;
    loop {
        if cwd.join(".git").exists() || cwd.join(".darwincode").exists() {
            return Some(cwd.clone());
        }
        if let Some(parent) = cwd.parent() {
            cwd = parent.to_path_buf();
        } else {
            break;
        }
    }
    std::env::current_dir().ok()
}

pub fn find_project_config() -> Option<PathBuf> {
    let root = find_project_root()?;
    let dc_path = root.join(".darwincode").join("config.json");
    if dc_path.exists() && dc_path.is_file() {
        Some(dc_path)
    } else {
        None
    }
}

pub fn load_project_instructions() -> Option<String> {
    let root = find_project_root()?;
    let path = root.join(".darwincode").join("instructions.md");
    if path.exists()
        && path.is_file()
        && let Ok(content) = std::fs::read_to_string(&path)
    {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    None
}

fn merge_json_values(a: &mut serde_json::Value, b: serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                merge_json_values(a.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (a, b) => {
            *a = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_transitions() {
        assert_eq!(Theme::Auto.next(), Theme::Dark);
        assert_eq!(Theme::Dark.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Auto);
    }

    #[test]
    fn test_theme_labels() {
        assert_eq!(Theme::Auto.label(), "Auto (System/Term)");
        assert_eq!(Theme::Dark.label(), "Dark");
        assert_eq!(Theme::Light.label(), "Light");
    }

    #[test]
    fn test_permission_level_labels() {
        assert_eq!(PermissionLevel::Safe.label(), "Safe (Read-Only)");
        assert_eq!(PermissionLevel::Guardian.label(), "Guardian (Ask)");
        assert_eq!(PermissionLevel::Chaos.label(), "Chaos (Auto)");
    }

    #[test]
    fn test_permission_level_transitions() {
        assert_eq!(PermissionLevel::Safe.next(), PermissionLevel::Guardian);
        assert_eq!(PermissionLevel::Guardian.next(), PermissionLevel::Chaos);
        assert_eq!(PermissionLevel::Chaos.next(), PermissionLevel::Safe);
    }

    #[test]
    fn test_resolve_explicit_themes() {
        assert_eq!(resolve_theme(&Theme::Dark), Theme::Dark);
        assert_eq!(resolve_theme(&Theme::Light), Theme::Light);
    }

    #[test]
    fn test_resolve_theme_mode() {
        assert_eq!(
            resolve_theme_mode(&Theme::Dark),
            crate::tui::theme::ThemeMode::Dark
        );
        assert_eq!(
            resolve_theme_mode(&Theme::Light),
            crate::tui::theme::ThemeMode::Light
        );
        let mode = resolve_theme_mode(&Theme::Auto);
        assert!(matches!(
            mode,
            crate::tui::theme::ThemeMode::Dark | crate::tui::theme::ThemeMode::Light
        ));
    }

    #[test]
    fn test_merge_json_values() {
        use serde_json::json;
        let mut a = json!({
            "key1": "value1",
            "key2": {
                "sub1": "subval1"
            }
        });
        let b = json!({
            "key2": {
                "sub2": "subval2"
            },
            "key3": "value3"
        });
        merge_json_values(&mut a, b);
        assert_eq!(a["key1"], "value1");
        assert_eq!(a["key2"]["sub1"], "subval1");
        assert_eq!(a["key2"]["sub2"], "subval2");
        assert_eq!(a["key3"], "value3");
    }

    #[test]
    fn test_config_save_load() {
        let _lock = TEST_LOCK.lock().unwrap();
        let root = find_project_root().unwrap_or_else(|| PathBuf::from("."));
        let temp_dir = root.join("target").join(format!(
            "darwincode_config_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        *TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let config = StoredConfig {
            api_key: "test_api_key_123".to_string(),
            ..Default::default()
        };

        assert!(config.save().is_ok());

        let loaded = StoredConfig::load().unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.model, config.model);
        assert_eq!(loaded.base_url, config.base_url);
        assert!(!loaded.api_key.is_empty());

        *TEST_CONFIG_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stored_config_validation() {
        let mut config = StoredConfig::default();
        // default config with empty key should be invalid
        assert!(config.validate().is_err());

        // with a key, it should be valid
        config.api_key = "dummy_key".to_string();
        assert!(config.validate().is_ok());

        // invalid api key starting with sk- but keeping default gemini base url / model
        config.api_key = "sk-12345".to_string();
        assert!(config.validate().is_err());

        // valid sk- config
        config.model = "claude-sonnet-4.6".to_string();
        config.base_url = "http://localhost:20128/v1".to_string();
        assert!(config.validate().is_ok());
    }
}
