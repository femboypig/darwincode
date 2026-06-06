use crate::app::App;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_sessions_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
        || app
            .core.keybindings
            .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_sessions();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
    {
        app.ui.sessions.select_previous();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
    {
        app.ui.sessions.select_next();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Submit, key)
    {
        app.apply_selected_session();
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Backspace, _) => {
            let mut q = app.ui.sessions.query.clone();
            q.pop();
            app.ui.sessions.update_query(q);
        }
        (KeyCode::Char(c), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            let mut q = app.ui.sessions.query.clone();
            q.push(c);
            app.ui.sessions.update_query(q);
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};

    #[test]
    fn test_handle_sessions_key_navigation() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let mut app = App::new(Some(StoredConfig::default()));

        // Mock a session save to make resume_session work
        app.chat.session_id = "a".to_owned();
        crate::app::session::save_session(&app.chat).unwrap();

        app.ui.sessions.sessions = vec![
            crate::app::session::SessionMeta { id: "a".to_owned(), snippet: "a".to_owned() },
            crate::app::session::SessionMeta { id: "b".to_owned(), snippet: "b".to_owned() },
        ];
        app.ui.sessions.selected = 0;

        // Press ScrollDown
        let key_down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        handle_sessions_key(&mut app, key_down).unwrap();
        assert_eq!(app.ui.sessions.selected, 1);

        // Press ScrollUp
        let key_up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        handle_sessions_key(&mut app, key_up).unwrap();
        assert_eq!(app.ui.sessions.selected, 0);

        // Type 'x'
        handle_sessions_key(&mut app, KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.sessions.query, "x");

        // Backspace
        handle_sessions_key(&mut app, KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.sessions.query, "");

        // Cancel
        handle_sessions_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.screen, crate::app::Screen::Chat);

        // Submit (we need to re-enter sessions screen first)
        app.ui.screen = crate::app::Screen::Sessions;
        app.ui.sessions.selected = 0; // Selects session "a"
        handle_sessions_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.screen, crate::app::Screen::Chat);

        // Clean up
        if let Ok(config_path) = crate::config::config_path()
            && let Some(parent) = config_path.parent()
        {
            let sessions_dir = parent.join("sessions");
            let _ = std::fs::remove_file(sessions_dir.join("a.json"));
            let _ = std::fs::remove_file(sessions_dir.join("a.meta"));
        }
    }
}
