use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App) {
    if app.proc.last_file_backups.is_empty() {
        app.chat.messages.push(MessageLine::info(
            "No changes to undo from the last prompt.".to_owned(),
        ));
    } else {
        let undone: Vec<String> = app
            .proc
            .last_file_backups
            .iter()
            .map(|b| {
                if b.original_content.is_some() {
                    format!("reverted `{}`", b.path)
                } else {
                    format!("deleted new file `{}`", b.path)
                }
            })
            .collect();
        app.rollback_transactions();
        app.chat.messages.push(MessageLine::info(format!(
            "Undo completed successfully: {}",
            undone.join(", ")
        )));
    }
}
