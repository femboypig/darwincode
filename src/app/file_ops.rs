use std::fs;

pub struct Edit {
    pub path: String,
    pub old: String,
    pub new: String,
}

pub struct TransactionReport {
    pub applied: Vec<String>,
}

pub fn apply_transactional_edits(edits: &[Edit]) -> Result<TransactionReport, Vec<String>> {
    let mut original_contents = Vec::new();
    let mut errors = Vec::new();

    for edit in edits {
        match fs::read_to_string(&edit.path) {
            Ok(current) => {
                if current != edit.old {
                    errors.push(format!(
                        "Validation failed for {}: current content does not match expected old string",
                        edit.path
                    ));
                }
                original_contents.push((edit.path.clone(), Some(current)));
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if !edit.old.is_empty() {
                        errors.push(format!(
                            "Validation failed for {}: file does not exist, but expected non-empty old string",
                            edit.path
                        ));
                    }
                    original_contents.push((edit.path.clone(), None));
                } else {
                    errors.push(format!("Failed to read {}: {}", edit.path, e));
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut applied_paths = Vec::new();
    for edit in edits {
        if let Some(parent) = std::path::Path::new(&edit.path).parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    errors.push(format!("Failed to create directories for {}: {}", edit.path, e));
                    break;
                }
            }
        }

        if let Err(e) = fs::write(&edit.path, &edit.new) {
            errors.push(format!("Failed to write {}: {}", edit.path, e));
            break;
        }
        applied_paths.push(edit.path.clone());
    }

    if !errors.is_empty() {
        let mut rollback_errors = Vec::new();
        for path in applied_paths.iter().rev() {
            if let Some((_, original)) = original_contents.iter().find(|(p, _)| p == path) {
                match original {
                    Some(content) => {
                        if let Err(e) = fs::write(path, content) {
                            rollback_errors.push(format!("Rollback failed for {}: {}", path, e));
                        }
                    }
                    None => {
                        if let Err(e) = fs::remove_file(path) {
                            rollback_errors.push(format!("Rollback failed (remove) for {}: {}", path, e));
                        }
                    }
                }
            }
        }
        errors.extend(rollback_errors);
        return Err(errors);
    }

    Ok(TransactionReport {
        applied: applied_paths,
    })
}

pub fn format_shell_output(
    stdout: &str,
    stderr: &str,
    error_field: &str,
    is_still_running_err: bool,
    is_aborted: bool,
    is_running: bool,
    running_suffix: Option<&str>,
) -> String {
    let mut output = String::new();
    if !stdout.is_empty() {
        output.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(stderr);
    }
    if !error_field.is_empty()
        && error_field != "null"
        && !is_still_running_err
    {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(error_field);
    }

    if is_aborted {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("^C\n[Process terminated by user via Ctrl+C]");
    } else if is_running {
        if let Some(suffix) = running_suffix {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(suffix);
        }
    } else if output.is_empty() {
        output = "(empty output)".to_owned();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_transactional_edits_success() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_tx_test_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("a.txt");
        let file2 = temp_dir.join("b.txt");

        fs::write(&file1, "hello").unwrap();

        let edits = vec![
            Edit {
                path: file1.to_str().unwrap().to_owned(),
                old: "hello".to_owned(),
                new: "world".to_owned(),
            },
            Edit {
                path: file2.to_str().unwrap().to_owned(),
                old: "".to_owned(),
                new: "new file".to_owned(),
            },
        ];

        let res = apply_transactional_edits(&edits);
        assert!(res.is_ok());

        assert_eq!(fs::read_to_string(&file1).unwrap(), "world");
        assert_eq!(fs::read_to_string(&file2).unwrap(), "new file");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_transactional_edits_validation_failure() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_tx_test_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("a.txt");
        fs::write(&file1, "hello").unwrap();

        let edits = vec![Edit {
            path: file1.to_str().unwrap().to_owned(),
            old: "wrong old value".to_owned(),
            new: "world".to_owned(),
        }];

        let res = apply_transactional_edits(&edits);
        assert!(res.is_err());
        assert_eq!(fs::read_to_string(&file1).unwrap(), "hello");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_transactional_edits_rollback_on_write_failure() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_tx_test_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("a.txt");
        fs::write(&file1, "original").unwrap();

        // The second edit points to a directory that cannot be created (e.g. under root /)
        // because we don't run as root, so create_dir_all will fail with PermissionDenied.
        let edits = vec![
            Edit {
                path: file1.to_str().unwrap().to_owned(),
                old: "original".to_owned(),
                new: "modified".to_owned(),
            },
            Edit {
                path: "/invalid-path-xyz/file.txt".to_owned(),
                old: "".to_owned(),
                new: "some content".to_owned(),
            },
        ];

        let res = apply_transactional_edits(&edits);
        assert!(res.is_err());

        // The first file should have been rolled back to "original"
        assert_eq!(fs::read_to_string(&file1).unwrap(), "original");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
