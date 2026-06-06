pub(crate) mod ask_user;
pub(crate) mod chat;
pub(crate) mod common;
pub(crate) mod mouse;
pub(crate) mod permissions;
pub(crate) mod sessions;
pub(crate) mod setup;

use anyhow::Result;
use crossterm::event::KeyEvent;
use std::sync::mpsc::Sender;

use crate::app::{App, Screen};
use crate::tui::WorkerEvent;

pub(crate) fn handle_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    match app.ui.screen {
        Screen::Setup => setup::handle_setup_key(app, sender, key),
        Screen::Chat => chat::handle_chat_key(app, sender, key),
        Screen::Permissions => permissions::handle_permissions_key(app, sender, key),
        Screen::Sessions => sessions::handle_sessions_key(app, key),
        Screen::AskUser => ask_user::handle_ask_user_key(app, key),
    }
}

pub(crate) fn handle_paste(app: &mut App, text: String) {
    if app.ui.screen == Screen::Chat {
        if matches!(
            app.proc.pending,
            Some(crate::app::PendingTask::ConfirmFunction { .. })
        ) {
            return;
        }
        if app.ui.model_picker_open {
            for c in text.chars() {
                app.ui.models.push_query(c);
            }
            return;
        }
        if app.ui.theme_picker_open {
            for c in text.chars() {
                app.ui.theme_picker.push_query(c);
            }
            return;
        }
        app.chat.insert_text(&text);
    } else if app.ui.screen == Screen::Setup {
        let old_key = app.ui.setup.api_key.clone();
        for c in text.chars() {
            app.ui.setup.push_char(c);
        }
        if app.ui.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
            app.status =
                "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
        }
    }
}
