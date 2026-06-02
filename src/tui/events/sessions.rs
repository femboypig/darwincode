use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::App;

pub(crate) fn handle_sessions_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key)
        || app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_sessions();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
        app.sessions.select_previous();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
        app.sessions.select_next();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Submit, key) {
        app.apply_selected_session();
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Backspace, _) => {
            let mut q = app.sessions.query.clone();
            q.pop();
            app.sessions.update_query(q);
        }
        (KeyCode::Char(c), modifiers) if !modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT) => {
            let mut q = app.sessions.query.clone();
            q.push(c);
            app.sessions.update_query(q);
        }
        _ => {}
    }
    Ok(())
}
