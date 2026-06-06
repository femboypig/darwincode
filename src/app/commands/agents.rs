use crate::app::core::App;
use crate::app::agent_picker::AgentPickerState;
use crate::app::chat::MessageLine;

pub fn run_picker(app: &mut App) {
    app.ui.agent_picker = AgentPickerState::new(&app.core.active_agent);
    app.ui.agent_picker_open = true;
    app.status = "Select agent. Enter to apply, Esc to cancel.".to_owned();
}

pub fn run_agent(app: &mut App, name: Option<String>) {
    let custom_agents = crate::app::load_custom_agents();
    if let Some(agent_name) = name {
        if agent_name.to_lowercase() == "none" {
            app.core.active_agent = None;
            app.chat.config.active_agent = None;
            app.chat.messages.push(MessageLine::info("Active agent cleared.".to_owned()));
            app.status = "Agent cleared".to_owned();
        } else if custom_agents.contains_key(&agent_name) {
            app.core.active_agent = Some(agent_name.clone());
            app.chat.config.active_agent = Some(agent_name.clone());
            let display_name = &custom_agents[&agent_name].name;
            app.chat.messages.push(MessageLine::info(format!(
                "Active agent set to: **{}**",
                display_name
            )));
            app.status = format!("Agent set to {}", display_name);
        } else {
            app.chat.messages.push(MessageLine::error(format!(
                "Agent '{}' not found.",
                agent_name
            )));
            app.status = format!("Agent '{}' not found", agent_name);
        }
    } else {
        run_picker(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_run_picker() {
        let mut app = App::new(Some(StoredConfig::default()));
        run_picker(&mut app);
        assert!(app.ui.agent_picker_open);
        assert_eq!(app.status, "Select agent. Enter to apply, Esc to cancel.");
    }

    #[test]
    fn test_run_agent_none() {
        let mut app = App::new(Some(StoredConfig::default()));
        run_agent(&mut app, None);
        assert!(app.ui.agent_picker_open);
    }

    #[test]
    fn test_run_agent_clear() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.core.active_agent = Some("helper".to_owned());
        run_agent(&mut app, Some("none".to_owned()));
        assert!(app.core.active_agent.is_none());
        assert!(app.chat.config.active_agent.is_none());
        assert!(!app.chat.messages.is_empty());
        assert!(app.chat.messages[0].text.contains("Active agent cleared"));
    }

    #[test]
    fn test_run_agent_not_found() {
        let mut app = App::new(Some(StoredConfig::default()));
        run_agent(&mut app, Some("nonexistent".to_owned()));
        assert!(!app.chat.messages.is_empty());
        assert!(app.chat.messages[0].text.contains("not found"));
    }
}

