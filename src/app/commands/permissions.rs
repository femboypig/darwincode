use crate::app::chat::MessageLine;
use crate::app::core::{App, PendingTask, Screen, SubmitAction};
use crate::app::permission::PermissionPickerState;
use crate::config::PermissionLevel;

pub fn run(app: &mut App, level: Option<PermissionLevel>) -> Option<SubmitAction> {
    if let Some(level) = level {
        app.chat.config.permission_level = level;
        let level_label = app.chat.config.permission_level.label();
        if app.chat.messages.last().is_some_and(|m| m.pending) {
            app.chat.messages.pop();
        }
        app.chat.messages.push(MessageLine::info(format!(
            "Permission level set to **{level_label}**"
        )));
        let _ = app.chat.config.save();

        if let Some(PendingTask::ConfirmFunction { name, args }) = app.proc.pending.clone() {
            let auto_allowed = level == PermissionLevel::Chaos
                || (level == PermissionLevel::Safe
                    && (name == "read"
                        || name == "grep"
                        || name == "glob"
                        || name == "websearch"
                        || name == "ask"
                        || name == "todo"
                        || name == "ps"
                        || name == "logs"));
            if auto_allowed {
                app.backup_before_execution(&name, &args);
                app.proc.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                app.status = format!("Auto-executing {name}");
                app.start_function_execution(&name, &args);
                return Some(SubmitAction::ExecuteFunction {
                    name,
                    args,
                    config: app.chat.config.clone(),
                });
            } else if level == PermissionLevel::Safe
                && (name == "sh"
                    || name == "write"
                    || name == "edit"
                    || name == "patch"
                    || name == "kill")
                && let Some(crate::app::FunctionAction::ResumeGeneration(request)) = app
                    .complete_function_execution(
                        name,
                        serde_json::json!({"error": "Permission denied: restricted mode"}),
                    )
            {
                return Some(SubmitAction::Generate(request));
            }
        }
        None
    } else {
        app.ui.screen = Screen::Permissions;
        let current = app.chat.config.permission_level;
        app.ui.permissions.selected = PermissionPickerState::options()
            .iter()
            .position(|(_, _, l)| *l == current)
            .unwrap_or(0);
        app.status = "Select permission level. Enter to apply, Esc to cancel.".to_owned();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;

    #[test]
    fn test_permissions_run_some() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut app = App::new(Some(StoredConfig::default()));
        let result = run(&mut app, Some(PermissionLevel::Safe));
        assert!(result.is_none());
        assert_eq!(app.chat.config.permission_level, PermissionLevel::Safe);
        assert!(!app.chat.messages.is_empty());

        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_permissions_run_none() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.screen = Screen::Chat;
        let result = run(&mut app, None);
        assert!(result.is_none());
        assert_eq!(app.ui.screen, Screen::Permissions);
    }
}
