use crate::app::{App, Screen};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_ask_user_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let tx = crate::tui::ASK_USER_CHANNEL
            .lock()
            .ok()
            .and_then(|mut g| g.take().map(|(tx, _, _)| tx));
        if let Some(tx) = tx {
            let _ = tx.send("Aborted by user".to_owned());
        }
        app.cancel_generation();
        app.screen = Screen::Chat;
        app.status = "Aborted by user".to_owned();
        return Ok(());
    }

    if app.ask_user.is_custom {
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    app.ask_user.custom_input.push('\n');
                } else {
                    let answer = app.ask_user.custom_input.trim().to_owned();
                    if !answer.is_empty() {
                        let tx = crate::tui::ASK_USER_CHANNEL
                            .lock()
                            .ok()
                            .and_then(|mut g| g.take().map(|(tx, _, _)| tx));
                        if let Some(tx) = tx {
                            let _ = tx.send(answer);
                        }
                        app.screen = Screen::Chat;
                        app.status = "Ready".to_owned();
                    }
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
                    let tx = crate::tui::ASK_USER_CHANNEL
                        .lock()
                        .ok()
                        .and_then(|mut g| g.take().map(|(tx, _, _)| tx));
                    if let Some(tx) = tx {
                        let _ = tx.send(answer);
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
