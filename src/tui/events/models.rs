use crate::app::App;
use anyhow::Result;
use crossterm::event::KeyEvent;

pub(crate) fn handle_models_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
        || app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_models();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
    {
        app.select_previous_model();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
    {
        app.select_next_model();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Submit, key)
    {
        app.apply_selected_model();
        return Ok(());
    }
    Ok(())
}
