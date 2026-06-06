use crate::config::{StoredConfig, Theme};
use super::chat::ChatState;
use super::agent_picker::AgentPickerState;
use super::model::ModelPickerState;
use super::permission::PermissionPickerState;
use super::session::SessionPickerState;
use super::setup::SetupState;
use super::theme_picker::ThemePickerState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Setup,
    Chat,
    Permissions,
    Sessions,
    AskUser,
}

#[derive(Clone, Debug)]
pub struct AskUserState {
    pub question: String,
    pub options: Vec<String>,
    pub selected_idx: usize,
    pub custom_input: String,
    pub is_custom: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DevelopMode {
    Plan,
    Build,
}

#[derive(Clone, Debug)]
pub struct FileBackup {
    pub path: String,
    pub original_content: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PendingTask {
    Generating,
    LoadingModels,
    ConfirmFunction {
        name: String,
        args: serde_json::Value,
    },
    ExecutingFunction {
        name: String,
    },
}

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub config: StoredConfig,
    pub history: Vec<crate::api::ChatMessage>,
    pub cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub generation_id: usize,
    pub dev_mode: String,
}

#[derive(Clone, Debug)]
pub enum SubmitAction {
    Generate(GenerationRequest),
    LoadModels(StoredConfig),
    ExecuteFunction {
        name: String,
        args: serde_json::Value,
        config: StoredConfig,
    },
}

#[derive(Clone, Debug)]
pub enum FunctionAction {
    Execute {
        name: String,
        args: serde_json::Value,
        config: StoredConfig,
    },
    ResumeGeneration(GenerationRequest),
}

#[derive(Debug)]
pub struct AppUiState {
    pub screen: Screen,
    pub setup: SetupState,
    pub models: ModelPickerState,
    pub permissions: PermissionPickerState,
    pub sessions: SessionPickerState,
    pub ask_user: AskUserState,
    pub model_picker_open: bool,
    pub theme_picker: ThemePickerState,
    pub theme_picker_open: bool,
    pub agent_picker: AgentPickerState,
    pub agent_picker_open: bool,
    pub confirm_scroll: std::cell::Cell<u16>,
}

#[derive(Debug)]
pub struct AppProcessManager {
    pub pending: Option<PendingTask>,
    pub cancel_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub generation_id: usize,
    pub last_file_backups: Vec<FileBackup>,
}

#[derive(Debug)]
pub struct AppCore {
    pub keybindings: crate::tui::keybindings::KeyBindings,
    pub sessions_cache: std::sync::Arc<std::sync::Mutex<Option<Vec<crate::app::session::SessionMeta>>>>,
    pub dev_mode: DevelopMode,
    pub active_agent: Option<String>,
}

#[derive(Debug)]
pub struct App {
    pub ui: AppUiState,
    pub proc: AppProcessManager,
    pub core: AppCore,
    pub chat: ChatState,
    pub status: String,
    pub tick: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: Option<StoredConfig>) -> Self {
        let keybindings = crate::tui::keybindings::load_keybindings();
        let sessions_cache = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cache_clone = sessions_cache.clone();
        std::thread::spawn(move || {
            if let Ok(sessions) = crate::app::session::list_saved_sessions()
                && let Ok(mut guard) = cache_clone.lock()
            {
                *guard = Some(sessions);
            }
        });

        match config {
            Some(config) => {
                let theme_picker = ThemePickerState::new(&config.theme);
                let agent_picker = AgentPickerState::new(&config.active_agent);
                let active_agent = config.active_agent.clone();
                let ui = AppUiState {
                    screen: Screen::Chat,
                    setup: SetupState::default(),
                    models: ModelPickerState::default(),
                    permissions: PermissionPickerState::default(),
                    sessions: SessionPickerState::default(),
                    ask_user: AskUserState {
                        question: String::new(),
                        options: Vec::new(),
                        selected_idx: 0,
                        custom_input: String::new(),
                        is_custom: false,
                    },
                    model_picker_open: false,
                    theme_picker,
                    theme_picker_open: false,
                    agent_picker,
                    agent_picker_open: false,
                    confirm_scroll: std::cell::Cell::new(0),
                };
                let proc = AppProcessManager {
                    pending: None,
                    cancel_token: None,
                    generation_id: 0,
                    last_file_backups: Vec::new(),
                };
                let core = AppCore {
                    keybindings,
                    sessions_cache,
                    dev_mode: DevelopMode::Build,
                    active_agent,
                };
                Self {
                    ui,
                    proc,
                    core,
                    chat: ChatState::new(config),
                    status: "Ready".to_owned(),
                    tick: 0,
                    should_quit: false,
                }
            }
            None => {
                let agent_picker = AgentPickerState::new(&None);
                let ui = AppUiState {
                    screen: Screen::Setup,
                    setup: SetupState::default(),
                    models: ModelPickerState::default(),
                    permissions: PermissionPickerState::default(),
                    sessions: SessionPickerState::default(),
                    ask_user: AskUserState {
                        question: String::new(),
                        options: Vec::new(),
                        selected_idx: 0,
                        custom_input: String::new(),
                        is_custom: false,
                    },
                    model_picker_open: false,
                    theme_picker: ThemePickerState::new(&crate::config::Theme::Auto),
                    theme_picker_open: false,
                    agent_picker,
                    agent_picker_open: false,
                    confirm_scroll: std::cell::Cell::new(0),
                };
                let proc = AppProcessManager {
                    pending: None,
                    cancel_token: None,
                    generation_id: 0,
                    last_file_backups: Vec::new(),
                };
                let core = AppCore {
                    keybindings,
                    sessions_cache,
                    dev_mode: DevelopMode::Build,
                    active_agent: None,
                };
                Self {
                    ui,
                    proc,
                    core,
                    chat: ChatState::new(StoredConfig::default()),
                    status: "Enter a Gemini API key. Use Tab to move, Enter to run an action.".to_owned(),
                    tick: 0,
                    should_quit: false,
                }
            }
        }
    }

    pub fn is_busy(&self) -> bool {
        self.proc.pending.is_some()
    }

    pub fn dev_mode_label(&self) -> &'static str {
        match self.core.dev_mode {
            DevelopMode::Plan => "Plan",
            DevelopMode::Build => "Build",
        }
    }

    pub fn model_label(&self) -> &str {
        self.chat.config.model.trim_start_matches("models/")
    }

    pub fn toggle_dev_mode(&mut self) {
        self.core.dev_mode = match self.core.dev_mode {
            DevelopMode::Plan => DevelopMode::Build,
            DevelopMode::Build => DevelopMode::Plan,
        };
        self.status = format!("Switched to {} mode", self.dev_mode_label());
    }

    pub fn save_setup(&mut self) -> anyhow::Result<()> {
        if self.is_busy() {
            return Ok(());
        }

        let config = self.ui.setup.to_config()?;
        config.save()?;
        self.chat.config = config;
        self.ui.screen = Screen::Chat;
        self.status = "Settings saved".to_owned();
        Ok(())
    }

    pub fn begin_load_chat_models(&mut self) -> Option<StoredConfig> {
        if self.is_busy() {
            return None;
        }

        self.proc.pending = Some(PendingTask::LoadingModels);
        self.status = "Loading models".to_owned();
        Some(self.chat.config.clone())
    }

    pub fn complete_load_models(&mut self, result: Result<Vec<String>, String>) {
        self.proc.pending = None;

        let models = match result {
            Ok(models) => models,
            Err(error) => {
                self.status = error;
                return;
            }
        };

        if self.ui.screen == Screen::Chat {
            let count = models.len();
            self.ui.models = ModelPickerState::new(models, &self.chat.config.model);
            self.ui.model_picker_open = true;
            self.status = format!("Loaded {count} models");
            return;
        }

        self.ui.setup.models = models;
        self.ui.setup.selected_model = 0;

        if let Some(model) = self.ui.setup.models.first() {
            self.ui.setup.model = model.trim_start_matches("models/").to_owned();
        }

        self.status = format!("Loaded {} models", self.ui.setup.models.len());
    }
}
