use crate::app::{App, Screen};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_ask_user_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        let mut g = crate::tui::ASK_USER_CHANNEL.lock();
        let tx = g.take().map(|(tx, _, _)| tx);
        if let Some(tx) = tx {
            let _ = tx.send("Aborted by user".to_owned());
        }
        app.cancel_generation();
        app.ui.screen = Screen::Chat;
        app.status = "Aborted by user".to_owned();
        return Ok(());
    }

    if app.ui.ask_user.is_custom {
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    app.ui.ask_user.custom_input.push('\n');
                } else {
                    let answer = app.ui.ask_user.custom_input.trim().to_owned();
                    if !answer.is_empty() {
                        let mut g = crate::tui::ASK_USER_CHANNEL.lock();
                        let tx = g.take().map(|(tx, _, _)| tx);
                        if let Some(tx) = tx {
                            let _ = tx.send(answer);
                        }
                        app.ui.screen = Screen::Chat;
                        app.status = "Ready".to_owned();
                    }
                }
            }
            KeyCode::Esc => {
                if !app.ui.ask_user.options.is_empty() {
                    app.ui.ask_user.is_custom = false;
                }
            }
            KeyCode::Backspace => {
                app.ui.ask_user.custom_input.pop();
            }
            KeyCode::Char(c) => {
                app.ui.ask_user.custom_input.push(c);
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Up => {
                app.ui.ask_user.selected_idx = app.ui.ask_user.selected_idx.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = app.ui.ask_user.options.len();
                if app.ui.ask_user.selected_idx < max {
                    app.ui.ask_user.selected_idx += 1;
                }
            }
            KeyCode::Enter => {
                if app.ui.ask_user.selected_idx == app.ui.ask_user.options.len() {
                    app.ui.ask_user.is_custom = true;
                } else if let Some(opt) = app.ui.ask_user.options.get(app.ui.ask_user.selected_idx) {
                    let answer = opt.clone();
                    let mut g = crate::tui::ASK_USER_CHANNEL.lock();
                    let tx = g.take().map(|(tx, _, _)| tx);
                    if let Some(tx) = tx {
                        let _ = tx.send(answer);
                    }
                    app.ui.screen = Screen::Chat;
                    app.status = "Ready".to_owned();
                }
            }
            _ => {}
        }
    }
    Ok(())
}
