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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crate::api::ChatMessage;
    use crate::app::session::save_session;

    #[test]
    fn test_resume_run_none() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.screen = crate::app::Screen::Chat;
        run(&mut app, None);
        assert_eq!(app.ui.screen, crate::app::Screen::Sessions);
    }

    #[test]
    fn test_resume_run_some_success() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.session_id = "session_123".to_owned();
        app.chat.history.push(ChatMessage::user("Hi".to_owned()));
        save_session(&app.chat).unwrap();

        // Now resume it in a fresh App
        let mut app2 = App::new(Some(StoredConfig::default()));
        run(&mut app2, Some("session_123".to_owned()));

        assert_eq!(app2.chat.session_id, "session_123");
        assert_eq!(app2.chat.history.len(), 1);

        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resume_run_some_fail() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut app = App::new(Some(StoredConfig::default()));
        run(&mut app, Some("nonexistent_session".to_owned()));
        assert!(!app.chat.messages.is_empty());
        // Should have added an error message
        assert!(app.chat.messages.iter().any(|m| m.text.contains("Failed to load session")));

        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

