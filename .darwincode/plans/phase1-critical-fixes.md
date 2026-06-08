# Phase 1: Critical Bug Fixes - Implementation Guide

**Status:** Ready for implementation  
**Estimated Time:** 1 week  
**Risk Level:** Medium (requires careful testing)

---

## Bug #1: Race Condition in cancel_generation()

### Problem Analysis
```rust
// Current code in src/app/state.rs:cancel_generation()
pub fn cancel_generation(&mut self) {
    if let Some(token) = self.proc.cancel_token.take() {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // ❌ BUG: streaming_parts cleared BEFORE worker thread reads it!
    self.chat.streaming_parts.clear();
    // Worker thread may still be pushing to streaming_parts
}
```

### Root Cause
- Main thread clears `streaming_parts` immediately
- Worker thread still processing chunks in background
- Worker calls `handle_stream_chunk()` which pushes to already-cleared vec
- Orphaned chunks leak memory

### Solution: Add generation_id validation

**File: src/app/chat/state.rs**

```rust
// Add to ChatState struct:
pub streaming_parts: Vec<(usize, Part)>,  // (generation_id, part)
```

**File: src/app/state.rs**

```rust
// Modify handle_stream_chunk:
pub fn handle_stream_chunk(&mut self, response: GeminiResponse) {
    if !matches!(self.proc.pending, Some(PendingTask::Generating)) {
        return;
    }
    
    // ✅ Validate generation_id before processing
    let current_gen_id = self.proc.generation_id;
    
    let GeminiResponse::Turn(parts) = response;
    let show_thoughts = self.chat.config.show_thoughts;

    if self.chat.messages.last().is_some_and(|m| m.pending) {
        self.chat.messages.pop();
    }

    if self.chat.selection.is_some() {
        self.clear_text_selection();
    }

    for part in parts {
        // ✅ Tag with generation_id
        self.chat.streaming_parts.push((current_gen_id, part.clone()));
        
        // ... rest of logic unchanged
    }
}

// Modify cancel_generation:
pub fn cancel_generation(&mut self) {
    if let Some(token) = self.proc.cancel_token.take() {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    self.proc.pending = None;
    self.status = "Generation stopped".to_owned();
    
    // ✅ Filter out current generation chunks instead of clearing all
    let current_gen_id = self.proc.generation_id;
    self.chat.streaming_parts.retain(|(id, _)| *id != current_gen_id);
    
    if self.chat.messages.last().is_some_and(|m| m.pending) {
        self.chat.messages.pop();
    }
    self.chat.message_queue.clear();
    self.chat.last_chunk_was_thought = false;
    self.save_session();
}

// Modify complete_stream:
pub fn complete_stream(&mut self) -> Option<FunctionAction> {
    if !matches!(self.proc.pending, Some(PendingTask::Generating)) {
        self.chat.streaming_parts.clear();
        return None;
    }
    
    // ✅ Filter by generation_id
    let current_gen_id = self.proc.generation_id;
    let parts: Vec<Part> = self.chat.streaming_parts
        .iter()
        .filter(|(id, _)| *id == current_gen_id)
        .map(|(_, part)| part.clone())
        .collect();
    
    // Clear all parts for this generation
    self.chat.streaming_parts.retain(|(id, _)| *id != current_gen_id);
    
    // ... rest unchanged
}
```

### Testing
```rust
// Add to src/app/state.rs tests
#[test]
fn test_cancel_generation_race_condition() {
    let mut app = App::new(Some(StoredConfig::default()));
    app.proc.generation_id = 1;
    app.chat.streaming_parts.push((1, serde_json::json!({"text": "chunk1"})));
    
    // Simulate race: new generation starts while old chunks exist
    app.proc.generation_id = 2;
    app.chat.streaming_parts.push((2, serde_json::json!({"text": "chunk2"})));
    
    // Cancel generation 2
    app.cancel_generation();
    
    // Only gen 1 chunks should remain
    assert_eq!(app.chat.streaming_parts.len(), 1);
    assert_eq!(app.chat.streaming_parts[0].0, 1);
}
```

---

## Bug #2: Persistent Shell Session Leak

### Problem Analysis
```rust
// Current code in src/tui/mod.rs:run_persistent_bash()
let mut check_count = 0;
let max_checks = 100;  // 5 seconds total
while check_count < max_checks {
    std::thread::sleep(Duration::from_millis(50));
    // If bash process hangs, we timeout BUT process stays alive!
    if stdout_guard[start_stdout_len..].contains(&sentinel) {
        found = true;
        break;
    }
    check_count += 1;
}
// ❌ No cleanup if !found
```

### Root Cause
- Bash process spawned in `PERSISTENT_SESSIONS` map
- Command times out waiting for sentinel
- Function returns error but process remains alive
- Accumulates zombie processes over time

### Solution: Kill on timeout

**File: src/tui/mod.rs**

```rust
pub(crate) fn run_persistent_bash(
    session_id: &str,
    cmd: &str,
    input: Option<&str>,
    sender: Sender<WorkerEvent>,
) -> Result<serde_json::Value, std::io::Error> {
    // ... existing code up to timeout loop ...
    
    let mut check_count = 0;
    let max_checks = 100;
    let mut found = false;
    let mut has_exited = false;
    let mut exit_status = None;

    while check_count < max_checks {
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Check if bash process has exited early
        if let Some(status) = entry.child.lock().try_wait().ok().flatten() {
            has_exited = true;
            exit_status = Some(status.code().unwrap_or(-1));
            break;
        }

        let stdout_guard = entry.stdout_accumulator.lock();
        if stdout_guard[start_stdout_len..].contains(&sentinel) {
            found = true;
            break;
        }
        check_count += 1;
    }

    // ✅ NEW: Kill process on timeout
    if !found && !has_exited {
        // Command timed out - kill the process
        let mut child_guard = entry.child.lock();
        let kill_result = child_guard.kill();
        drop(child_guard);
        
        // Clean up from registry
        let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        {
            let mut map = registry.lock();
            map.remove(session_id);
        }
        
        *ACTIVE_PERSISTENT_SESSION_ID.lock() = None;
        
        return Ok(serde_json::json!({
            "status": -1,
            "stdout": "",
            "stderr": "",
            "error": "Command timed out and shell process was terminated"
        }));
    }

    // Clear active persistent session ID
    *ACTIVE_PERSISTENT_SESSION_ID.lock() = None;

    // ✅ Clean up registry entry if process has exited
    if has_exited {
        let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        {
            let mut map = registry.lock();
            map.remove(session_id);
        }
    }

    // ... rest unchanged
}
```

### Additional: Add cleanup command

**File: src/tui/mod.rs**

```rust
/// Clean up all zombie persistent sessions
pub(crate) fn cleanup_zombie_sessions() -> usize {
    let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock();
    
    let mut zombies = Vec::new();
    for (id, session) in map.iter() {
        if let Some(mut child) = session.child.try_lock() {
            if let Ok(Some(_)) = child.try_wait() {
                zombies.push(id.clone());
            }
        }
    }
    
    let count = zombies.len();
    for id in zombies {
        map.remove(&id);
    }
    count
}
```

### Testing
```rust
#[test]
fn test_persistent_bash_timeout_cleanup() {
    let (sender, _receiver) = std::sync::mpsc::channel();
    
    // Run command that will timeout
    let result = run_persistent_bash(
        "test_timeout_session",
        "sleep 999",
        None,
        sender,
    );
    
    assert!(result.is_ok());
    let json = result.unwrap();
    assert!(json.get("error").is_some());
    assert!(json["error"].as_str().unwrap().contains("timed out"));
    
    // Verify session was removed from registry
    let registry = PERSISTENT_SESSIONS.get().unwrap();
    let map = registry.lock();
    assert!(!map.contains_key("test_timeout_session"));
}
```

---

## Bug #3: Symlink Handling in File Backup

### Problem Analysis
```rust
// Current code in src/app/state.rs:backup_before_execution()
let original_content = if resolved_path.exists() {
    std::fs::read_to_string(&resolved_path).ok()  // ❌ Follows symlinks!
} else {
    None
};
```

### Root Cause
- `read_to_string()` follows symlinks automatically
- If file is symlink to `/etc/passwd`, backup reads target
- Rollback writes to target → system file corruption
- No validation that path is regular file

### Solution: Check symlink metadata first

**File: src/app/state.rs**

```rust
pub fn backup_before_execution(&mut self, name: &str, args: &serde_json::Value) {
    // ... existing git_root detection code ...

    for path in paths_to_backup {
        if self.proc.last_file_backups.iter().any(|b| b.path == path) {
            continue;
        }
        
        let resolved_path = if let Some(ref root) = git_root
            && !std::path::Path::new(&path).is_absolute()
        {
            root.join(&path)
        } else {
            std::path::PathBuf::from(&path)
        };

        // ✅ NEW: Check if path is symlink
        let original_content = if resolved_path.exists() {
            match std::fs::symlink_metadata(&resolved_path) {
                Ok(metadata) if metadata.is_symlink() => {
                    // Don't backup symlinks - too risky
                    self.status = format!(
                        "Warning: Skipping backup of symlink: {}",
                        resolved_path.display()
                    );
                    None
                }
                Ok(metadata) if metadata.is_file() => {
                    // Regular file - safe to backup
                    std::fs::read_to_string(&resolved_path).ok()
                }
                Ok(_) => {
                    // Directory or special file - skip
                    None
                }
                Err(_) => {
                    // Can't read metadata - skip
                    None
                }
            }
        } else {
            None
        };
        
        self.proc.last_file_backups.push(FileBackup {
            path: resolved_path.to_string_lossy().into_owned(),
            original_content,
        });
    }
}
```

### Additional: Validate in rollback

**File: src/app/state.rs**

```rust
pub fn rollback_transactions(&mut self) {
    if !self.proc.last_file_backups.is_empty() {
        for backup in &self.proc.last_file_backups {
            let path = std::path::Path::new(&backup.path);
            
            // ✅ Safety check before rollback
            if let Ok(metadata) = std::fs::symlink_metadata(path) {
                if metadata.is_symlink() {
                    eprintln!("Warning: Refusing to rollback symlink: {}", backup.path);
                    continue;
                }
            }
            
            match &backup.original_content {
                Some(content) => {
                    let _ = std::fs::write(&backup.path, content);
                }
                None => {
                    // File didn't exist before - remove it
                    let _ = std::fs::remove_file(&backup.path);
                }
            }
        }
        self.proc.last_file_backups.clear();
    }
}
```

### Testing
```rust
#[test]
fn test_backup_refuses_symlinks() {
    use std::os::unix::fs::symlink;
    
    let temp_dir = std::env::temp_dir().join("darwin_backup_test");
    std::fs::create_dir_all(&temp_dir).unwrap();
    
    let real_file = temp_dir.join("real.txt");
    let symlink_file = temp_dir.join("link.txt");
    
    std::fs::write(&real_file, "sensitive data").unwrap();
    symlink(&real_file, &symlink_file).unwrap();
    
    let mut app = App::new(Some(StoredConfig::default()));
    
    let args = serde_json::json!({
        "path": symlink_file.to_str().unwrap(),
        "old_string": "test",
        "new_string": "test2"
    });
    
    app.backup_before_execution("edit", &args);
    
    // Backup should be skipped (no content stored)
    assert_eq!(app.proc.last_file_backups.len(), 1);
    assert!(app.proc.last_file_backups[0].original_content.is_none());
    assert!(app.status.contains("symlink"));
    
    std::fs::remove_dir_all(&temp_dir).unwrap();
}
```

---

## Verification Checklist

Before committing Phase 1:

- [ ] All tests pass: `cargo test`
- [ ] No clippy warnings: `cargo clippy --all-targets`
- [ ] Code formatted: `cargo fmt --check`
- [ ] Manual testing:
  - [ ] Cancel generation during streaming (no orphaned chunks)
  - [ ] Timeout persistent bash command (process killed)
  - [ ] Edit symlink file (backup skipped, warning shown)
  - [ ] Rollback after editing regular file (works)
  - [ ] Rollback after editing symlink (refused)

---

## Commit Plan

```bash
# Bug #1
git add src/app/chat/state.rs src/app/state.rs
git commit -m "Fix race condition in cancel_generation with generation_id validation"

# Bug #2
git add src/tui/mod.rs
git commit -m "Fix persistent shell session leak by killing process on timeout"

# Bug #3
git add src/app/state.rs
git commit -m "Fix symlink handling in file backup with metadata validation"
```

---

## Risk Assessment

**Bug #1 (Race Condition):**
- Risk: LOW - Only changes data structure, doesn't affect control flow
- Mitigation: Backward compatible (old code with empty vec still works)

**Bug #2 (Session Leak):**
- Risk: MEDIUM - Kills processes, could affect running shells
- Mitigation: Only kills on timeout (5s), user-visible error message

**Bug #3 (Symlink):**
- Risk: LOW - More conservative (skips backups), doesn't break existing functionality
- Mitigation: Warning shown to user, operation continues

**Overall:** Safe to deploy incrementally. Each bug fix is independent.
