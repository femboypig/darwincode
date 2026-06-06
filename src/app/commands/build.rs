use crate::app::chat::MessageLine;
use crate::app::core::{App, DevelopMode};

pub fn run(app: &mut App) {
    app.core.dev_mode = DevelopMode::Build;
    app.status = "Switched to Build mode".to_owned();
    app.chat.messages.push(MessageLine::info(
        "Switched to **Build** mode (full tools access)".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_build_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.core.dev_mode = DevelopMode::Plan;
        run(&mut app);
        assert_eq!(app.core.dev_mode, DevelopMode::Build);
        assert_eq!(app.status, "Switched to Build mode");
    }
}
