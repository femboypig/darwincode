use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{App, Screen};

pub(crate) fn handle_ask_user_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
            if let Some((tx, _, _)) = guard.take() {
                let _ = tx.send("Aborted by user".to_owned());
            }
        }
        app.cancel_generation();
        app.screen = Screen::Chat;
        app.status = "Aborted by user".to_owned();
        return Ok(());
    }

    if app.ask_user.is_custom {
        match key.code {
            KeyCode::Enter => {
                let answer = app.ask_user.custom_input.trim().to_owned();
                if !answer.is_empty() {
                    if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                        if let Some((tx, _, _)) = guard.take() {
                            let _ = tx.send(answer);
                        }
                    }
                    app.screen = Screen::Chat;
                    app.status = "Ready".to_owned();
                }
            }
            KeyCode::Esc => {
                if !app.ask_user.options.is_empty() {
                    app.ask_user.is_custom = false;
                }
            }
            KeyCode::Backspace => {
                app.ask_user.custom_input.pop();
            }
            KeyCode::Char(c) => {
                app.ask_user.custom_input.push(c);
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Up => {
                app.ask_user.selected_idx = app.ask_user.selected_idx.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = app.ask_user.options.len();
                if app.ask_user.selected_idx < max {
                    app.ask_user.selected_idx += 1;
                }
            }
            KeyCode::Enter => {
                if app.ask_user.selected_idx == app.ask_user.options.len() {
                    app.ask_user.is_custom = true;
                } else if let Some(opt) = app.ask_user.options.get(app.ask_user.selected_idx) {
                    let answer = opt.clone();
                    if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                        if let Some((tx, _, _)) = guard.take() {
                            let _ = tx.send(answer);
                        }
                    }
                    app.screen = Screen::Chat;
                    app.status = "Ready".to_owned();
                }
            }
            _ => {}
        }
    }
    Ok(())
}
