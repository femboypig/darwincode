use crate::app::chat::MessageLine;
use crate::app::core::{App, DevelopMode};

pub fn run(app: &mut App) {
    app.core.dev_mode = DevelopMode::Plan;
    app.status = "Switched to Plan mode".to_owned();
    app.chat.messages.push(MessageLine::info(
        "Switched to **Plan** mode (read-only for workspace files)".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_plan_run() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.core.dev_mode = DevelopMode::Build;
        run(&mut app);
        assert_eq!(app.core.dev_mode, DevelopMode::Plan);
        assert_eq!(app.status, "Switched to Plan mode");
    }
}
