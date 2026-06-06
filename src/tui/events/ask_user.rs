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
        if let Some(name) = app.core.pending_custom_command.take() {
            app.status = format!("Cancelled execution of /{}", name);
        } else {
            app.status = "Aborted by user".to_owned();
        }
        app.ui.screen = Screen::Chat;
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
                        let _ = tx.send(answer.clone());
                    }
                    app.ui.screen = Screen::Chat;
                    app.status = "Ready".to_owned();

                    if let Some(name) = app.core.pending_custom_command.take() {
                        if answer == "yes" {
                            let custom_cmds = crate::app::load_custom_commands(app.chat.config.trust_workspace);
                            if let Some((config, _)) = custom_cmds.get(&name) {
                                crate::app::commands::custom::execute_custom_command_internal(app, &name, config);
                            }
                        } else {
                            app.status = format!("Cancelled execution of /{}", name);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};

    #[test]
    fn test_handle_ask_user_key_navigation() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.ask_user.options = vec!["yes".to_owned(), "no".to_owned()];
        app.ui.ask_user.selected_idx = 0;
        app.ui.ask_user.is_custom = false;

        // Press down arrow
        let key_down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        handle_ask_user_key(&mut app, key_down).unwrap();
        assert_eq!(app.ui.ask_user.selected_idx, 1);

        // Press up arrow
        let key_up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        handle_ask_user_key(&mut app, key_up).unwrap();
        assert_eq!(app.ui.ask_user.selected_idx, 0);
    }

    #[test]
    fn test_handle_ask_user_key_custom_input() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.ask_user.is_custom = true;
        app.ui.ask_user.custom_input = "y".to_owned();

        // Type 'e' and 's'
        handle_ask_user_key(&mut app, KeyEvent::new(KeyCode::Char('e'), KeyModifiers::empty())).unwrap();
        handle_ask_user_key(&mut app, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.ask_user.custom_input, "yes");

        // Backspace
        handle_ask_user_key(&mut app, KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty())).unwrap();
        assert_eq!(app.ui.ask_user.custom_input, "ye");
    }
}
