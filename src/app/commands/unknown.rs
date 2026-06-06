use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, command: String) {
    app.chat.messages.push(MessageLine::info(format!(
        "Unknown command: {command}\nTry /settings, /models, /permissions, or /exit."
    )));
    app.status = "Unknown command".to_owned();
}
