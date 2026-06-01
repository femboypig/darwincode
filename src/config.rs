use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark (Default)",
            Self::Light => "Light",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
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
        if !path.exists() {
            return Ok(None);
        }

        let key = crate::crypto::derive_hardware_key()?;
        let cipher_data = fs::read(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let plain_data = crate::crypto::decrypt_data(&cipher_data, &key)
            .with_context(|| format!("failed to decrypt config {}", path.display()))?;
        
        let mut config: StoredConfig = serde_json::from_slice(&plain_data)
            .with_context(|| format!("failed to parse config {}", path.display()))?;

        // Load secret from OS securely if available, otherwise use fallback from encrypted file
        if let Ok(entry) = keyring::Entry::new("darwincode", "api_key") {
            if let Ok(secret) = entry.get_password() {
                if !secret.trim().is_empty() {
                    config.api_key = secret;
                }
            }
        }

        Ok(Some(config))
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;

        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        // Save secret to OS keyring securely (if available)
        let mut keyring_succeeded = false;
        if let Ok(entry) = keyring::Entry::new("darwincode", "api_key") {
            if entry.set_password(&self.api_key).is_ok() {
                keyring_succeeded = true;
            }
        }

        // Encrypt the configuration file on disk with the secret field stripped ONLY if stored in keyring
        let mut file_config = self.clone();
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

        if self.base_url.trim().is_empty() {
            bail!("base URL cannot be empty");
        }

        if self.api_key.starts_with("sk-") {
            if self.base_url == "https://generativelanguage.googleapis.com/v1beta" {
                bail!("For OpenAI/OmniRoute keys (starting with sk-), you must specify an OpenAI-compatible Base URL (e.g. http://localhost:20128/v1)");
            }
            if self.model == "gemini-2.0-flash" {
                bail!("For OpenAI/OmniRoute keys (starting with sk-), you must specify an OpenAI-compatible Model (e.g. claude-sonnet-4.6)");
            }
        }

        Ok(())
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
            theme: Theme::Dark,
        }
    }
}



pub fn config_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".config")))
        .context("could not find HOME, USERPROFILE, APPDATA, or XDG_CONFIG_HOME")?;

    Ok(base.join("darwincode").join("config.json"))
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
