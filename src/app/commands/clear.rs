use crate::app::core::App;

pub fn run(app: &mut App) {
    app.chat.history.clear();
    app.chat.messages.clear();
    app.chat.scroll = 0;
    app.chat.message_queue.clear();
    app.chat.sent_history_index = None;
    app.chat.input_draft.clear();
    app.save_session();
    app.status = "Chat history cleared".to_owned();
}
