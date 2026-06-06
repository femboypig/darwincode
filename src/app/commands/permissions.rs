use crate::config::PermissionLevel;
use crate::app::core::{App, Screen, SubmitAction, PendingTask};
use crate::app::chat::MessageLine;
use crate::app::permission::PermissionPickerState;

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
