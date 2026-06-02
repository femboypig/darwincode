use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TuiAction {
    Quit,
    Cancel,
    Submit,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    HistoryUp,
    HistoryDown,
    ToggleSetup,
    ToggleModels,
    TogglePermissions,
    ToggleSessions,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct KeyBindings {
    pub bindings: HashMap<TuiAction, Vec<String>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert(TuiAction::Quit, vec!["ctrl+c".to_owned()]);
        bindings.insert(TuiAction::Cancel, vec!["esc".to_owned()]);
        bindings.insert(TuiAction::Submit, vec!["enter".to_owned()]);
        bindings.insert(
            TuiAction::ScrollUp,
            vec!["up".to_owned(), "alt+k".to_owned()],
        );
        bindings.insert(
            TuiAction::ScrollDown,
            vec!["down".to_owned(), "alt+j".to_owned()],
        );
        bindings.insert(TuiAction::PageUp, vec!["pageup".to_owned()]);
        bindings.insert(TuiAction::PageDown, vec!["pagedown".to_owned()]);
        bindings.insert(TuiAction::HistoryUp, vec!["ctrl+up".to_owned()]);
        bindings.insert(TuiAction::HistoryDown, vec!["ctrl+down".to_owned()]);
        bindings.insert(TuiAction::ToggleSetup, vec!["ctrl+s".to_owned()]);
        bindings.insert(TuiAction::ToggleModels, vec!["ctrl+p".to_owned()]);
        bindings.insert(TuiAction::TogglePermissions, vec!["ctrl+o".to_owned()]);
        bindings.insert(TuiAction::ToggleSessions, vec!["ctrl+g".to_owned()]);
        Self { bindings }
    }
}

pub fn parse_key_event(s: &str) -> Result<KeyEvent> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        anyhow::bail!("Empty key binding");
    }

    let mut modifiers = KeyModifiers::empty();
    for part in parts.iter().take(parts.len() - 1) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
            "alt" | "option" | "meta" => modifiers.insert(KeyModifiers::ALT),
            "shift" => modifiers.insert(KeyModifiers::SHIFT),
            _ => anyhow::bail!("Unknown modifier: {}", part),
        }
    }

    let key_part = parts.last().unwrap().to_lowercase();
    let code = match key_part.as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "return" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        s if s.starts_with('f') && s[1..].parse::<u8>().is_ok() => {
            let num = s[1..].parse::<u8>().unwrap();
            KeyCode::F(num)
        }
        _ => anyhow::bail!("Unknown key: {}", key_part),
    };

    Ok(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    })
}

impl KeyBindings {
    pub fn matches(&self, action: TuiAction, event: KeyEvent) -> bool {
        if let Some(keys) = self.bindings.get(&action) {
            for key_str in keys {
                if let Ok(parsed) = parse_key_event(key_str)
                    && parsed.code == event.code
                    && parsed.modifiers == event.modifiers
                {
                    return true;
                }
            }
        }
        false
    }
}

pub fn keybindings_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".config")))
        .context("could not find HOME, USERPROFILE, APPDATA, or XDG_CONFIG_HOME")?;

    Ok(base.join("darwincode").join("keybindings.json"))
}

pub fn load_keybindings() -> KeyBindings {
    match keybindings_path() {
        Ok(path) => {
            if path.exists()
                && let Ok(data) = std::fs::read_to_string(&path)
                && let Ok(config) = serde_json::from_str::<KeyBindings>(&data)
            {
                return config;
            }
            let default_bindings = KeyBindings::default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(pretty) = serde_json::to_string_pretty(&default_bindings) {
                let _ = std::fs::write(&path, pretty);
            }
            default_bindings
        }
        Err(_) => KeyBindings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_event() {
        let ev = parse_key_event("ctrl+s").unwrap();
        assert_eq!(ev.code, KeyCode::Char('s'));
        assert_eq!(ev.modifiers, KeyModifiers::CONTROL);

        let ev = parse_key_event("alt+enter").unwrap();
        assert_eq!(ev.code, KeyCode::Enter);
        assert_eq!(ev.modifiers, KeyModifiers::ALT);

        let ev = parse_key_event("esc").unwrap();
        assert_eq!(ev.code, KeyCode::Esc);
        assert_eq!(ev.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn test_keybindings_matches() {
        let bindings = KeyBindings::default();
        let ev = KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        assert!(bindings.matches(TuiAction::ToggleSetup, ev));
    }
}
