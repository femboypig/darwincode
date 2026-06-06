use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, name: String) {
    let custom_cmds = crate::app::load_custom_commands(app.chat.config.trust_workspace);
    if let Some((config, is_workspace)) = custom_cmds.get(&name) {
        if *is_workspace && !app.chat.config.trust_workspace {
            app.ui.screen = crate::app::Screen::AskUser;
            app.ui.ask_user.question = format!("Run untrusted workspace command /{}?", name);
            app.ui.ask_user.options = vec!["yes".to_owned(), "no".to_owned()];
            app.ui.ask_user.selected_idx = 0;
            app.ui.ask_user.custom_input.clear();
            app.ui.ask_user.is_custom = false;
            app.core.pending_custom_command = Some(name);
            app.status = "Confirmation required".to_owned();
            return;
        }

        execute_custom_command_internal(app, &name, config);
    }
}

pub fn execute_custom_command_internal(app: &mut App, name: &str, config: &crate::app::custom::CustomCommandConfig) {
    if let Some(ref model_override) = config.model {
        app.chat.config.model =
            model_override.trim_start_matches("models/").to_owned();
    }
    match config.execute() {
        Ok(prompt_content) => {
            app.chat.input = prompt_content;
            app.chat.cursor = app.chat.input.chars().count();
            app.chat.suggestion_idx = 0;
            app.status =
                format!("Custom command /{} executed into input buffer", name);
        }
        Err(e) => {
            app.chat.messages.push(MessageLine::error(format!(
                "Error executing custom command /{}: {}",
                name, e
            )));
            app.status = format!("Error executing custom command /{}", name);
        }
    }
}
