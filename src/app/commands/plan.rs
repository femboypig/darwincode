use crate::app::core::{App, DevelopMode};
use crate::app::chat::MessageLine;

pub fn run(app: &mut App) {
    app.core.dev_mode = DevelopMode::Plan;
    app.status = "Switched to Plan mode".to_owned();
    app.chat.messages.push(MessageLine::info(
        "Switched to **Plan** mode (read-only for workspace files)".to_owned(),
    ));
}
