use crate::config::{PermissionLevel, StoredConfig, Theme};
use anyhow::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupField {
    ApiKey,
    Model,
    BaseUrl,
    EnableCodebase,
    EnableBash,
    PermissionLevel,
    ShowThoughts,
    Theme,
    RespectGitignore,
    Save,
}

impl SetupField {
    pub fn index(self) -> usize {
        match self {
            Self::ApiKey => 0,
            Self::Model => 1,
            Self::BaseUrl => 2,
            Self::EnableCodebase => 3,
            Self::EnableBash => 4,
            Self::PermissionLevel => 5,
            Self::ShowThoughts => 6,
            Self::Theme => 7,
            Self::RespectGitignore => 8,
            Self::Save => 9,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::ApiKey,
            1 => Self::Model,
            2 => Self::BaseUrl,
            3 => Self::EnableCodebase,
            4 => Self::EnableBash,
            5 => Self::PermissionLevel,
            6 => Self::ShowThoughts,
            7 => Self::Theme,
            8 => Self::RespectGitignore,
            _ => Self::Save,
        }
    }
}

#[derive(Debug)]
pub struct SetupState {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub enable_codebase_tools: bool,
    pub enable_bash_tools: bool,
    pub permission_level: PermissionLevel,
    pub show_thoughts: bool,
    pub theme: Theme,
    pub respect_gitignore: bool,
    pub active_field: SetupField,
    pub models: Vec<String>,
    pub selected_model: usize,
    pub is_editing: bool,
}

impl SetupState {
    pub fn from_config(config: &StoredConfig) -> Self {
        Self {
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            enable_codebase_tools: config.enable_codebase_tools,
            enable_bash_tools: config.enable_bash_tools,
            permission_level: config.permission_level,
            show_thoughts: config.show_thoughts,
            theme: config.theme.clone(),
            respect_gitignore: config.respect_gitignore,
            active_field: SetupField::ApiKey,
            models: Vec::new(),
            selected_model: 0,
            is_editing: false,
        }
    }

    pub fn to_config(&self) -> Result<StoredConfig> {
        let config = StoredConfig {
            api_key: self.api_key.trim().to_owned(),
            model: self.model.trim().trim_start_matches("models/").to_owned(),
            base_url: self.base_url.trim().trim_end_matches('/').to_owned(),
            enable_codebase_tools: self.enable_codebase_tools,
            enable_bash_tools: self.enable_bash_tools,
            show_thoughts: self.show_thoughts,
            permission_level: self.permission_level,
            theme: self.theme.clone(),
            respect_gitignore: self.respect_gitignore,
        };

        config.validate()?;
        Ok(config)
    }

    pub fn push_char(&mut self, value: char) {
        match self.active_field {
            SetupField::ApiKey => {
                self.api_key.push(value);
            }
            SetupField::Model => self.model.push(value),
            SetupField::BaseUrl => self.base_url.push(value),
            _ => {}
        }
    }

    pub fn pop_char(&mut self) {
        match self.active_field {
            SetupField::ApiKey => {
                self.api_key.pop();
            }
            SetupField::Model => {
                self.model.pop();
            }
            SetupField::BaseUrl => {
                self.base_url.pop();
            }
            _ => {}
        }
    }

    pub fn select_next_model(&mut self) {
        if self.models.is_empty() {
            return;
        }

        self.selected_model = (self.selected_model + 1) % self.models.len();
        self.model = self.models[self.selected_model]
            .trim_start_matches("models/")
            .to_owned();
    }

    pub fn select_previous_model(&mut self) {
        if self.models.is_empty() {
            return;
        }

        self.selected_model = self
            .selected_model
            .checked_sub(1)
            .unwrap_or_else(|| self.models.len() - 1);
        self.model = self.models[self.selected_model]
            .trim_start_matches("models/")
            .to_owned();
    }
}

impl Default for SetupState {
    fn default() -> Self {
        let config = StoredConfig::default();

        Self {
            api_key: config.api_key,
            model: config.model,
            base_url: config.base_url,
            enable_codebase_tools: config.enable_codebase_tools,
            enable_bash_tools: config.enable_bash_tools,
            permission_level: config.permission_level,
            show_thoughts: config.show_thoughts,
            theme: config.theme.clone(),
            respect_gitignore: config.respect_gitignore,
            active_field: SetupField::ApiKey,
            models: Vec::new(),
            selected_model: 0,
            is_editing: false,
        }
    }
}
