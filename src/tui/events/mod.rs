pub(crate) mod setup;
pub(crate) mod chat;
pub(crate) mod models;
pub(crate) mod permissions;
pub(crate) mod sessions;
pub(crate) mod ask_user;
pub(crate) mod common;

use std::sync::mpsc::Sender;
use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::app::{App, Screen};
use crate::tui::WorkerEvent;

pub(crate) fn handle_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    match app.screen {
        Screen::Setup => setup::handle_setup_key(app, sender, key),
        Screen::Chat => chat::handle_chat_key(app, sender, key),
        Screen::Models => models::handle_models_key(app, key),
        Screen::Permissions => permissions::handle_permissions_key(app, sender, key),
        Screen::Sessions => sessions::handle_sessions_key(app, key),
        Screen::AskUser => ask_user::handle_ask_user_key(app, key),
    }
}

pub(crate) fn handle_paste(app: &mut App, text: String) {
    if app.screen == Screen::Chat {
        if matches!(app.pending, Some(crate::app::PendingTask::ConfirmFunction { .. })) {
            return;
        }
        app.chat.insert_text(&text);
    } else if app.screen == Screen::Setup {
        let old_key = app.setup.api_key.clone();
        for c in text.chars() {
            app.setup.push_char(c);
        }
        if app.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
            app.status = "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
        }
    }
}
