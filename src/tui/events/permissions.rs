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
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        app.cancel_permissions();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_permissions();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
    {
        app.permissions.select_previous();
        return Ok(());
    }
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
    {
        app.permissions.select_next();
        return Ok(());
    }
    if app
        .keybindings
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
