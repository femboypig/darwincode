use crate::app::core::App;

pub fn run(app: &mut App) {
    app.chat.history.clear();
    app.chat.messages.clear();
    app.chat.scroll.set(0);
    app.chat.last_total_lines.set(0);
    app.chat.message_queue.clear();
    app.chat.sent_history_index = None;
    app.chat.input_draft.clear();
    app.chat.session_id = format!(
        "session_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    app.save_session();
    app.status = "New chat started".to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_new_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.input_draft = "some draft".to_owned();
        run(&mut app);
        assert!(app.chat.input_draft.is_empty());
        assert_eq!(app.status, "New chat started");
    }
}
