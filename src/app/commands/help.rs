use crate::app::chat::MessageLine;
use crate::app::core::App;

pub fn run(app: &mut App) {
    let help_text = "Available commands:\n\
                     - **/settings**: Open configuration settings\n\
                     - **/models**: List and select Gemini/OpenAI models\n\
                     - **/agents**: Open active agent selection picker\n\
                     - **/agent [name]**: Switch to active agent by name (or 'none' to clear)\n\
                     - **/permissions [safe|guardian|chaos]**: View or set permission level\n\
                     - **/resume [session_id]**: Load a saved session (or open selector)\n\
                     - **/new**: Start a new chat session\n\
                     - **/clear**: Clear the current chat history\n\
                     - **/history**: Show all saved chat session IDs\n\
                     - **/undo**: Revert all file changes made in the last prompt\n\
                     - **/shell [session_id_or_pid]**: List or focus active shell sessions\n\
                     - **/plan**: Switch to Plan mode (read-only for workspace files)\n\
                     - **/build**: Switch to Build mode (full tools access)\n\
                     - **/help**: Display this help card\n\
                     - **/exit** / **/quit**: Exit the application\n\n\
                     Hotkeys (in Chat):\n\
                     - **Ctrl+S**: Open Setup screen\n\
                     - **Ctrl+P**: Switch active Model instantly\n\
                     - **Ctrl+T**: Toggle between Plan and Build modes";
    app.chat
        .messages
        .push(MessageLine::info(help_text.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_help_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        run(&mut app);
        assert!(!app.chat.messages.is_empty());
        assert!(app.chat.messages[0].text.contains("Available commands:"));
    }
}
