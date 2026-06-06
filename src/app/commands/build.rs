use crate::app::core::{App, DevelopMode};
use crate::app::chat::MessageLine;

pub fn run(app: &mut App) {
    app.core.dev_mode = DevelopMode::Build;
    app.status = "Switched to Build mode".to_owned();
    app.chat.messages.push(MessageLine::info(
        "Switched to **Build** mode (full tools access)".to_owned(),
    ));
}
