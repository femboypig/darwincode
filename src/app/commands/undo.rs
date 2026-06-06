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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crate::app::core::FileBackup;

    #[test]
    fn test_undo_empty() {
        let mut app = App::new(Some(StoredConfig::default()));
        run(&mut app);
        assert!(!app.chat.messages.is_empty());
        assert!(app.chat.messages[0].text.contains("No changes to undo"));
    }

    #[test]
    fn test_undo_with_backups() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("test1.txt");
        let file2 = temp_dir.join("test2.txt");

        // file1 exists initially with old content
        std::fs::write(&file1, "old content").unwrap();
        // file2 doesn't exist initially

        let mut app = App::new(Some(StoredConfig::default()));
        app.proc.last_file_backups = vec![
            FileBackup {
                path: file1.to_string_lossy().into_owned(),
                original_content: Some("old content".to_owned()),
            },
            FileBackup {
                path: file2.to_string_lossy().into_owned(),
                original_content: None,
            },
        ];

        // Simulate modifications
        std::fs::write(&file1, "new content").unwrap();
        std::fs::write(&file2, "new file content").unwrap();

        run(&mut app);

        // Verify rollback happened
        assert_eq!(std::fs::read_to_string(&file1).unwrap(), "old content");
        assert!(!file2.exists());

        assert!(!app.chat.messages.is_empty());
        assert!(app.chat.messages[0].text.contains("Undo completed successfully"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

