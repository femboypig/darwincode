use anyhow::Result;
use crossterm::event::KeyEvent;
use std::sync::mpsc::Sender;

use crate::app::{App, SubmitAction};
use crate::tui::{
    WorkerEvent, handle_function_action, spawn_generation_worker, spawn_models_worker,
};

pub(crate) fn handle_permissions_key(
    app: &mut App,
    sender: &Sender<WorkerEvent>,
    key: KeyEvent,
) -> Result<()> {
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        app.cancel_permissions();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_permissions();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
    {
        app.ui.permissions.select_previous();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
    {
        app.ui.permissions.select_next();
        return Ok(());
    }
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Submit, key)
    {
        if let Some(action) = app.apply_permission_level() {
            match action {
                SubmitAction::Generate(request) => {
                    spawn_generation_worker(
                        request.config,
                        request.history,
                        request.cancel_token,
                        request.generation_id,
                        request.dev_mode,
                        sender.clone(),
                    );
                }
                SubmitAction::LoadModels(config) => {
                    spawn_models_worker(config, sender.clone());
                }
                SubmitAction::ExecuteFunction { name, args, config } => {
                    handle_function_action(
                        crate::app::FunctionAction::Execute { name, args, config },
                        sender,
                    );
                }
            }
        }
        return Ok(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};
    use std::sync::mpsc;

    #[test]
    fn test_handle_permissions_key_navigation() {
        let mut app = App::new(Some(StoredConfig::default()));
        let (sender, _receiver) = mpsc::channel();

        // Initially selected = 0 (Safe)
        assert_eq!(app.ui.permissions.selected, 0);

        // Press ScrollDown (Keybindings map Down key to ScrollDown)
        let key_down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        handle_permissions_key(&mut app, &sender, key_down).unwrap();
        assert_eq!(app.ui.permissions.selected, 1);

        // Press ScrollUp
        let key_up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        handle_permissions_key(&mut app, &sender, key_up).unwrap();
        assert_eq!(app.ui.permissions.selected, 0);
    }
}
