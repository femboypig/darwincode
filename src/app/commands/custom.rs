use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, name: String) {
    let custom_cmds = crate::app::load_custom_commands();
    if let Some(config) = custom_cmds.get(&name) {
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
}
