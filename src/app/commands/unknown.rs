use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, command: String) {
    app.chat.messages.push(MessageLine::info(format!(
        "Unknown command: {command}\nTry /settings, /models, /permissions, or /exit."
    )));
    app.status = "Unknown command".to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_unknown_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        run(&mut app, "/invalid".to_owned());
        assert_eq!(app.status, "Unknown command");
    }
}
