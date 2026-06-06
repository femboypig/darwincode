use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App) {
    match crate::app::session::list_saved_sessions() {
        Ok(list) => {
            if list.is_empty() {
                app.chat
                    .messages
                    .push(MessageLine::info("No saved sessions found.".to_owned()));
            } else {
                let mut msg = "Saved sessions:\n".to_owned();
                for meta in list {
                    msg.push_str(&format!("- **{}**: {}\n", meta.id, meta.snippet));
                }
                app.chat.messages.push(MessageLine::info(msg));
            }
        }
        Err(e) => {
            app.chat.messages.push(MessageLine::error(format!(
                "Failed to list sessions: {}",
                e
            )));
        }
    }
}
