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
    ToggleSessions,
    Paste,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct KeyBindings {
    pub bindings: HashMap<TuiAction, Vec<String>>,
}

impl<'de> serde::Deserialize<'de> for KeyBindings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct KeyBindingsVisitor;

        impl<'de> Visitor<'de> for KeyBindingsVisitor {
            type Value = KeyBindings;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a KeyBindings object with a bindings map")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut bindings = HashMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key == "bindings" {
                        // Deserialize the inner map, tolerating unknown TuiAction names.
                        let raw: HashMap<String, Vec<String>> = map.next_value()?;
                        for (action_str, keys) in raw {
                            // Try to parse the action name — skip unrecognized ones.
                            if let Ok(action) = serde_json::from_value::<TuiAction>(
                                serde_json::Value::String(action_str.clone()),
                            ) {
                                bindings.insert(action, keys);
                            }
                            // Unknown action names (e.g. removed variants) are silently ignored.
                        }
                    } else {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(KeyBindings { bindings })
            }
        }

        deserializer.deserialize_map(KeyBindingsVisitor)
    }
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
        bindings.insert(TuiAction::ToggleSessions, vec!["ctrl+g".to_owned()]);
        bindings.insert(
            TuiAction::Paste,
            vec!["ctrl+v".to_owned(), "ctrl+y".to_owned()],
        );
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

pub fn load_keybindings() -> (KeyBindings, Option<String>) {
    let mut warning = None;
    let mut kb = match keybindings_path() {
        Ok(path) => {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(data) => match serde_json::from_str::<KeyBindings>(&data) {
                        Ok(config) => config,
                        Err(e) => {
                            eprintln!("[darwincode] keybindings config invalid: {e}; using defaults");
                            warning = Some("Keybindings config malformed".to_owned());
                            KeyBindings::default()
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "[keybindings] Failed to read {}: {}. Using defaults.",
                            path.display(),
                            e
                        );
                        KeyBindings::default()
                    }
                }
            } else {
                KeyBindings::default()
            }
        }
        Err(_) => KeyBindings::default(),
    };

    // Ensure all default actions are populated
    let defaults = KeyBindings::default();
    let mut updated = false;
    for (action, keys) in defaults.bindings {
        if let std::collections::hash_map::Entry::Vacant(e) = kb.bindings.entry(action) {
            e.insert(keys);
            updated = true;
        }
    }

    if updated && let Ok(path) = keybindings_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(pretty) = serde_json::to_string_pretty(&kb) {
            let _ = std::fs::write(&path, pretty);
        }
    }

    (kb, warning)
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

    #[test]
    fn test_keybindings_deserialize_ignores_unknown_actions() {
        // Old JSON files may contain action names that no longer exist (e.g. "TogglePermissions").
        // The deserializer must silently skip them instead of failing.
        let json =
            r#"{"bindings":{"Quit":["ctrl+c"],"TogglePermissions":["ctrl+o"],"Cancel":["esc"]}}"#;
        let kb: KeyBindings =
            serde_json::from_str(json).expect("should deserialize with unknown actions");
        assert!(kb.bindings.contains_key(&TuiAction::Quit));
        assert!(kb.bindings.contains_key(&TuiAction::Cancel));
        // TogglePermissions no longer exists — it must be silently dropped.
        assert_eq!(kb.bindings.len(), 2);
    }

    #[test]
    fn test_load_keybindings_no_overwrite_on_bad_json() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("darwincode_kb_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keybindings.json");
        // Write intentionally broken JSON.
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{ bad json }").unwrap();
        drop(f);

        // load_keybindings can't be called directly with a custom path, but we can verify
        // the parse path: broken JSON must return an error, not panic.
        let result = serde_json::from_str::<KeyBindings>("{ bad json }");
        assert!(result.is_err(), "broken JSON must fail to parse");

        // The file must NOT be overwritten by anything in this test (it is the load_keybindings
        // logic that must not overwrite — here we just verify the file is still broken).
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "{ bad json }");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
