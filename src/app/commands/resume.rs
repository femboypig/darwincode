use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, session_id: Option<String>) {
    if let Some(id) = session_id {
        if let Err(e) = app.resume_session(&id) {
            app.chat.messages.push(MessageLine::error(format!(
                "Failed to load session '{}': {}",
                id, e
            )));
        }
    } else {
        app.open_sessions();
    }
}
