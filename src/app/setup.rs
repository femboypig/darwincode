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
    RespectIgnoreRules,
    TrustWorkspace,
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
            Self::RespectIgnoreRules => 8,
            Self::TrustWorkspace => 9,
            Self::Save => 10,
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
            8 => Self::RespectIgnoreRules,
            9 => Self::TrustWorkspace,
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
    pub respect_ignore_rules: bool,
    pub active_field: SetupField,
    pub models: Vec<String>,
    pub selected_model: usize,
    pub is_editing: bool,
    pub modal_area: std::cell::Cell<Option<ratatui::layout::Rect>>,
    pub trust_workspace: bool,
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
            respect_ignore_rules: config.respect_ignore_rules,
            active_field: SetupField::ApiKey,
            models: Vec::new(),
            selected_model: 0,
            is_editing: false,
            modal_area: std::cell::Cell::new(None),
            trust_workspace: config.trust_workspace,
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
            respect_ignore_rules: self.respect_ignore_rules,
            trust_workspace: self.trust_workspace,
            active_agent: None,
            ..Default::default()
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
            respect_ignore_rules: config.respect_ignore_rules,
            active_field: SetupField::ApiKey,
            models: Vec::new(),
            selected_model: 0,
            is_editing: false,
            modal_area: std::cell::Cell::new(None),
            trust_workspace: config.trust_workspace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_field_index_transitions() {
        for idx in 0..11 {
            let field = SetupField::from_index(idx);
            assert_eq!(field.index(), idx.min(10));
        }
    }

    #[test]
    fn test_setup_state_to_from_config() {
        let config = StoredConfig {
            api_key: "test_key".to_owned(),
            model: "models/gemini-2.0-flash".to_owned(),
            base_url: "https://example.com".to_owned(),
            ..Default::default()
        };

        let state = SetupState::from_config(&config);
        assert_eq!(state.api_key, "test_key");
        assert_eq!(state.model, "models/gemini-2.0-flash");
        assert_eq!(state.base_url, "https://example.com");

        let config2 = state.to_config().unwrap();
        assert_eq!(config2.api_key, "test_key");
        assert_eq!(config2.model, "gemini-2.0-flash");
        assert_eq!(config2.base_url, "https://example.com");
    }

    #[test]
    fn test_setup_state_char_editing() {
        let mut state = SetupState {
            active_field: SetupField::ApiKey,
            ..Default::default()
        };
        state.push_char('a');
        state.push_char('b');
        assert_eq!(state.api_key, "ab");

        state.pop_char();
        assert_eq!(state.api_key, "a");

        state.active_field = SetupField::Model;
        state.model.clear();
        state.push_char('m');
        assert_eq!(state.model, "m");

        state.active_field = SetupField::BaseUrl;
        state.base_url.clear();
        state.push_char('h');
        assert_eq!(state.base_url, "h");
    }

    #[test]
    fn test_setup_state_model_selection() {
        let mut state = SetupState {
            models: vec!["models/model-a".to_owned(), "models/model-b".to_owned()],
            selected_model: 0,
            model: "model-a".to_owned(),
            ..Default::default()
        };

        state.select_next_model();
        assert_eq!(state.selected_model, 1);
        assert_eq!(state.model, "model-b");

        state.select_previous_model();
        assert_eq!(state.selected_model, 0);
        assert_eq!(state.model, "model-a");

        // previous from index 0 should wrap to 1
        state.select_previous_model();
        assert_eq!(state.selected_model, 1);
        assert_eq!(state.model, "model-b");
    }
}
