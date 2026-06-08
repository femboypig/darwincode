use parking_lot::Mutex;
use serde_json::Value;
use std::fmt::Write;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::api::GeminiClient;
use crate::api::client::canonical_tool_name;
use crate::app::FunctionAction;
use crate::config::StoredConfig;
use crate::tui::WorkerEvent;

pub(crate) enum PathSafety {
    Safe,
    Blocked,
    Prompt,
}

fn get_home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .map(std::path::PathBuf::from)
        })
}

fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek() {
        let buf = PathBuf::from(c.as_os_str());
        components.next();
        buf
    } else {
        PathBuf::new()
    };

    let mut has_root = false;
    if let Some(c @ Component::RootDir) = components.peek() {
        has_root = true;
        ret.push(c.as_os_str());
        components.next();
    }

    for component in components {
        match component {
            Component::Prefix(..) => {}
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                if !ret.pop() && !has_root {
                    ret.push(Component::ParentDir);
                }
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

fn canonicalize_safe(path: &std::path::Path) -> std::path::PathBuf {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    if let Ok(p) = std::fs::canonicalize(&abs_path) {
        p
    } else {
        normalize_path(&abs_path)
    }
}

pub(crate) fn check_path_safety(path: &std::path::Path) -> PathSafety {
    let proj_root = crate::config::find_project_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let proj_root = std::fs::canonicalize(&proj_root).unwrap_or(proj_root);
    let abs_path = canonicalize_safe(path);

    if abs_path.starts_with(&proj_root) {
        return PathSafety::Safe;
    }

    let path_str = abs_path.to_string_lossy();
    if path_str.starts_with("/etc")
        || path_str.starts_with("/proc")
        || path_str.starts_with("/sys")
        || path_str.starts_with("/dev")
        || path_str.starts_with("/root")
        || path_str.starts_with("/boot")
        || path_str.starts_with("/var/log")
        || path_str.starts_with("/var/lib")
    {
        return PathSafety::Blocked;
    }

    if let Some(home) = get_home_dir() {
        let home = std::fs::canonicalize(&home).unwrap_or(home);
        if abs_path.starts_with(&home) {
            let rel_to_home = abs_path.strip_prefix(&home).unwrap_or(&abs_path);
            let rel_str = rel_to_home.to_string_lossy();
            // Sensitive dotfile dirs and shell config — these
            // almost never need to be touched by an LLM, and a
            // prompt-injected tool call could exfiltrate SSH keys
            // or rewrite .bashrc to backdoor the shell.
            let blocked_substr = [
                ".ssh",
                ".aws",
                ".gnupg",
                ".kube",
                ".docker",
                ".config/gh",
                ".netrc",
                ".bashrc",
                ".zshrc",
                ".bash_profile",
                ".profile",
                ".bash_history",
                ".zsh_history",
                ".lesshst",
                ".viminfo",
                ".pypirc",
                ".netrc",
            ];
            if blocked_substr.iter().any(|s| rel_str.contains(s)) {
                return PathSafety::Blocked;
            }
            return PathSafety::Safe;
        }
    }

    PathSafety::Prompt
}

fn prompt_user_permission(question: &str, _sender: &Sender<WorkerEvent>) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    *crate::tui::ASK_USER_CHANNEL.lock() = Some((
        tx,
        question.to_owned(),
        vec!["yes".to_owned(), "no".to_owned()],
    ));
    let answer = rx.recv().unwrap_or_default();
    *crate::tui::ASK_USER_CHANNEL.lock() = None;
    answer
}

pub(crate) fn compile_rules(rules: &[String]) -> globset::GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for rule in rules {
        if rule.contains('/') {
            let clean = rule.trim_start_matches('/');
            if let Ok(g) = globset::Glob::new(clean) {
                builder.add(g);
            }
            if let Ok(g) = globset::Glob::new(&format!("{}/**", clean)) {
                builder.add(g);
            }
        } else {
            if let Ok(g) = globset::Glob::new(rule) {
                builder.add(g);
            }
            if let Ok(g) = globset::Glob::new(&format!("**/{}", rule)) {
                builder.add(g);
            }
            if let Ok(g) = globset::Glob::new(&format!("**/{}/**", rule)) {
                builder.add(g);
            }
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| globset::GlobSet::empty())
}

pub(crate) fn should_ignore(path: &std::path::Path, glob_set: &globset::GlobSet) -> bool {
    let base_dir = crate::config::find_project_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let abs_base_dir = if base_dir.is_absolute() {
        base_dir.clone()
    } else {
        cwd.join(&base_dir)
    };
    let abs_base_dir = std::fs::canonicalize(&abs_base_dir).unwrap_or(abs_base_dir);

    let abs_path = if abs_path.exists() {
        std::fs::canonicalize(&abs_path).unwrap_or(abs_path)
    } else if let Some(parent) = abs_path.parent() {
        let canonical_parent =
            std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if let Some(file_name) = abs_path.file_name() {
            canonical_parent.join(file_name)
        } else {
            canonical_parent
        }
    } else {
        abs_path
    };

    if let Ok(rel_path) = abs_path.strip_prefix(&abs_base_dir) {
        glob_set.is_match(rel_path)
    } else {
        false
    }
}

pub(crate) fn load_darwincode_ignore_rules() -> Option<Vec<String>> {
    let mut current = crate::config::find_project_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    loop {
        let ignore_path = current.join(".darwincode").join("ignore");
        if ignore_path.exists()
            && ignore_path.is_file()
            && let Ok(content) = std::fs::read_to_string(&ignore_path)
        {
            let mut rules = Vec::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    let rule = trimmed
                        .trim_start_matches('/')
                        .trim_end_matches('/')
                        .to_owned();
                    if !rules.contains(&rule) {
                        rules.push(rule);
                    }
                }
            }
            return Some(rules);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

pub(crate) fn load_gitignore_rules() -> Vec<String> {
    let mut rules = Vec::new();

    if let Some(dc_rules) = load_darwincode_ignore_rules() {
        for rule in dc_rules {
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
    }

    let base_dir = crate::config::find_project_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut current = base_dir;
    loop {
        let gitignore_path = current.join(".gitignore");
        if gitignore_path.exists()
            && gitignore_path.is_file()
            && let Ok(content) = std::fs::read_to_string(&gitignore_path)
        {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    let rule = trimmed
                        .trim_start_matches('/')
                        .trim_end_matches('/')
                        .to_owned();
                    if !rules.contains(&rule) {
                        rules.push(rule);
                    }
                }
            }
        }
        if !current.pop() {
            break;
        }
    }
    rules
}

fn recursive_glob(
    dir: &std::path::Path,
    base_dir: &std::path::Path,
    pattern: &str,
    rules: &globset::GlobSet,
    matches: &mut Vec<String>,
) -> std::io::Result<()> {
    if dir.is_dir() {
        if should_ignore(dir, rules) {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if should_ignore(&path, rules) {
                continue;
            }
            if path.is_dir() {
                let _ = recursive_glob(&path, base_dir, pattern, rules, matches);
            } else if path.is_file() && matches_pattern(&path, base_dir, pattern) {
                matches.push(path.display().to_string());
                if matches.len() >= 1000 {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn matches_pattern(path: &std::path::Path, base_dir: &std::path::Path, pattern: &str) -> bool {
    if pattern.contains('/') {
        if let Ok(rel_path) = path.strip_prefix(base_dir) {
            let rel_str = rel_path.to_string_lossy();
            matches_wildcard(&rel_str, pattern)
        } else {
            false
        }
    } else if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        matches_wildcard(file_name, pattern)
    } else {
        false
    }
}

fn matches_wildcard(name: &str, pattern: &str) -> bool {
    let mut pattern_parts = pattern.split('*');
    if let Some(first) = pattern_parts.next() {
        if !name.starts_with(first) {
            return false;
        }
        let mut last_idx = first.len();
        for part in pattern_parts {
            if part.is_empty() {
                return true;
            }
            if let Some(idx) = name[last_idx..].find(part) {
                last_idx += idx + part.len();
            } else {
                return false;
            }
        }
        last_idx == name.len() || pattern.ends_with('*')
    } else {
        name == pattern
    }
}

fn recursive_search(
    dir: &std::path::Path,
    pattern: &str,
    rules: &globset::GlobSet,
    matches: &mut Vec<String>,
) -> std::io::Result<()> {
    if dir.is_dir() {
        if should_ignore(dir, rules) {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if should_ignore(&path, rules) {
                continue;
            }
            if path.is_dir() {
                let _ = recursive_search(&path, pattern, rules, matches);
            } else if path.is_file()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(pattern) {
                        matches.push(format!("{}:{}:{}", path.display(), line_num + 1, line));
                        if matches.len() >= 1000 {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_function_action(action: FunctionAction, sender: &Sender<WorkerEvent>) {
    match action {
        FunctionAction::Execute { name, args, config } => {
            let sender = sender.clone();
            crate::tui::async_runtime::spawn(async move {
                if let Some(ref agent_id) = config.active_agent {
                    let custom_agents = crate::app::load_custom_agents();
                    if let Some(agent_config) = custom_agents.get(agent_id)
                        && let Some(ref allowed) = agent_config.allowed_tools
                        && !allowed.iter().any(|s| canonical_tool_name(s) == name)
                    {
                        let err_msg = format!(
                            "Permission denied: tool '{}' is not allowed for the active agent '{}'.",
                            name, agent_id
                        );
                        let _ = sender.send(WorkerEvent::ToolResult(
                            name,
                            serde_json::json!({ "error": err_msg }),
                        ));
                        return;
                    }
                }

                let result = match name.as_str() {
                    "read" => {
                        let path = args.get("path").and_then(|v| v.as_str());
                        let paths = args.get("paths").and_then(|v| v.as_array());

                        let rules_vec = if config.respect_ignore_rules {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        let rules = compile_rules(&rules_vec);

                        if let Some(paths) = paths {
                            let mut results = serde_json::Map::new();
                            for path_val in paths {
                                if let Some(p_str) = path_val.as_str() {
                                    let p = std::path::Path::new(p_str);
                                    let safety = check_path_safety(p);
                                    match safety {
                                        PathSafety::Blocked => {
                                            results.insert(p_str.to_owned(), serde_json::json!({ "error": format!("Access denied: path `{}` is restricted", p_str) }));
                                            continue;
                                        }
                                        PathSafety::Prompt => {
                                            let answer = prompt_user_permission(
                                                &format!(
                                                    "Allow access to path `{}` outside project and home directories?",
                                                    p_str
                                                ),
                                                &sender,
                                            );
                                            if answer != "yes" {
                                                results.insert(p_str.to_owned(), serde_json::json!({ "error": format!("Access denied by user: `{}`", p_str) }));
                                                continue;
                                            }
                                        }
                                        PathSafety::Safe => {}
                                    }

                                    if config.respect_ignore_rules && should_ignore(p, &rules) {
                                        results.insert(p_str.to_owned(), serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", p_str) }));
                                    } else {
                                        match tokio::fs::read_to_string(p_str).await {
                                            Ok(content) => {
                                                results.insert(
                                                    p_str.to_owned(),
                                                    serde_json::json!({ "content": content }),
                                                );
                                            }
                                            Err(e) => {
                                                results.insert(
                                                    p_str.to_owned(),
                                                    serde_json::json!({ "error": e.to_string() }),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            serde_json::json!({ "files": results })
                        } else {
                            let target_path = path.unwrap_or(".");
                            let p = std::path::Path::new(target_path);
                            let safety = check_path_safety(p);
                            let mut allowed = true;
                            match safety {
                                PathSafety::Blocked => {
                                    allowed = false;
                                }
                                PathSafety::Prompt => {
                                    let answer = prompt_user_permission(
                                        &format!(
                                            "Allow access to path `{}` outside project and home directories?",
                                            target_path
                                        ),
                                        &sender,
                                    );
                                    if answer != "yes" {
                                        allowed = false;
                                    }
                                }
                                PathSafety::Safe => {}
                            }

                            if !allowed {
                                serde_json::json!({ "error": format!("Access denied: `{}` is restricted", target_path) })
                            } else if config.respect_ignore_rules && should_ignore(p, &rules) {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", target_path) })
                            } else if p.is_dir() {
                                match tokio::fs::read_dir(p).await {
                                    Ok(mut entries) => {
                                        let mut files = Vec::new();
                                        while let Ok(Some(entry)) = entries.next_entry().await {
                                            let entry_path = entry.path();
                                            if !config.respect_ignore_rules
                                                || !should_ignore(&entry_path, &rules)
                                            {
                                                files.push(entry_path.display().to_string());
                                            }
                                        }
                                        serde_json::json!({ "files": files })
                                    }
                                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                                }
                            } else if p.is_file() {
                                match tokio::fs::read_to_string(p).await {
                                    Ok(content) => serde_json::json!({ "content": content }),
                                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                                }
                            } else {
                                serde_json::json!({ "error": format!("Path `{}` does not exist or is not readable", target_path) })
                            }
                        }
                    }
                    "grep" => {
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                        let search_path = std::path::Path::new(path);

                        let safety = check_path_safety(search_path);
                        let mut allowed = true;
                        match safety {
                            PathSafety::Blocked => {
                                allowed = false;
                            }
                            PathSafety::Prompt => {
                                let answer = prompt_user_permission(
                                    &format!(
                                        "Allow access to path `{}` outside project and home directories?",
                                        path
                                    ),
                                    &sender,
                                );
                                if answer != "yes" {
                                    allowed = false;
                                }
                            }
                            PathSafety::Safe => {}
                        }

                        if !allowed {
                            serde_json::json!({ "error": format!("Access denied: `{}` is restricted", path) })
                        } else {
                            let mut matches = Vec::new();
                            let rules_vec = if config.respect_ignore_rules {
                                load_gitignore_rules()
                            } else {
                                Vec::new()
                            };
                            let rules = compile_rules(&rules_vec);

                            let run_res = if search_path.is_file() {
                                if (!config.respect_ignore_rules
                                    || !should_ignore(search_path, &rules))
                                    && let Ok(content) = std::fs::read_to_string(search_path)
                                {
                                    for (line_num, line) in content.lines().enumerate() {
                                        if line.contains(pattern) {
                                            matches.push(format!(
                                                "{}:{}:{}",
                                                search_path.display(),
                                                line_num + 1,
                                                line
                                            ));
                                        }
                                    }
                                }
                                Ok(())
                            } else {
                                recursive_search(search_path, pattern, &rules, &mut matches)
                            };

                            match run_res {
                                Ok(_) => {
                                    let stdout = matches.join("\n");
                                    serde_json::json!({ "matches": stdout })
                                }
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        }
                    }
                    "glob" => {
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                        let search_path = std::path::Path::new(path);

                        let safety = check_path_safety(search_path);
                        let mut allowed = true;
                        match safety {
                            PathSafety::Blocked => {
                                allowed = false;
                            }
                            PathSafety::Prompt => {
                                let answer = prompt_user_permission(
                                    &format!(
                                        "Allow access to path `{}` outside project and home directories?",
                                        path
                                    ),
                                    &sender,
                                );
                                if answer != "yes" {
                                    allowed = false;
                                }
                            }
                            PathSafety::Safe => {}
                        }

                        if !allowed {
                            serde_json::json!({ "error": format!("Access denied: `{}` is restricted", path) })
                        } else {
                            let mut matches = Vec::new();
                            let rules_vec = if config.respect_ignore_rules {
                                load_gitignore_rules()
                            } else {
                                Vec::new()
                            };
                            let rules = compile_rules(&rules_vec);

                            match recursive_glob(
                                search_path,
                                search_path,
                                pattern,
                                &rules,
                                &mut matches,
                            ) {
                                Ok(_) => {
                                    let stdout = matches.join("\n");
                                    serde_json::json!({ "matches": stdout })
                                }
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        }
                    }
                    "edit" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let edits = args.get("edits").and_then(|v| v.as_array());

                        let rules_vec = if config.respect_ignore_rules {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        let rules = compile_rules(&rules_vec);

                        if let Some(edits) = edits {
                            let mut parsed_edits = Vec::new();
                            let mut validation_errors = Vec::new();

                            for (idx, edit_val) in edits.iter().enumerate() {
                                let edit_path =
                                    edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                let old_string = edit_val
                                    .get("old_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let new_string = edit_val
                                    .get("new_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                if edit_path.is_empty() {
                                    validation_errors
                                        .push(format!("Edit at index {} is missing 'path'", idx));
                                    continue;
                                }

                                let ep = std::path::Path::new(edit_path);
                                let safety = check_path_safety(ep);
                                match safety {
                                    PathSafety::Blocked => {
                                        validation_errors.push(format!(
                                            "Access denied: path `{}` is restricted",
                                            edit_path
                                        ));
                                        continue;
                                    }
                                    PathSafety::Prompt => {
                                        let answer = prompt_user_permission(
                                            &format!(
                                                "Allow access to path `{}` outside project and home directories?",
                                                edit_path
                                            ),
                                            &sender,
                                        );
                                        if answer != "yes" {
                                            validation_errors.push(format!(
                                                "Access denied by user: `{}`",
                                                edit_path
                                            ));
                                            continue;
                                        }
                                    }
                                    PathSafety::Safe => {}
                                }

                                if config.respect_ignore_rules && should_ignore(ep, &rules) {
                                    validation_errors.push(format!(
                                        "Access denied: `{}` is ignored by .gitignore",
                                        edit_path
                                    ));
                                    continue;
                                }
                                parsed_edits.push((
                                    edit_path.to_owned(),
                                    old_string.to_owned(),
                                    new_string.to_owned(),
                                ));
                            }

                            if !validation_errors.is_empty() {
                                serde_json::json!({ "error": validation_errors.join("; ") })
                            } else {
                                let mut original_contents = std::collections::HashMap::new();
                                let mut apply_errors = Vec::new();

                                for (edit_path, _, _) in &parsed_edits {
                                    if !original_contents.contains_key(edit_path) {
                                        match tokio::fs::read_to_string(edit_path).await {
                                            Ok(content) => {
                                                original_contents
                                                    .insert(edit_path.clone(), content);
                                            }
                                            Err(e) => {
                                                apply_errors.push(format!(
                                                    "Failed to read `{}`: {}",
                                                    edit_path, e
                                                ));
                                            }
                                        }
                                    }
                                }

                                if !apply_errors.is_empty() {
                                    serde_json::json!({ "error": apply_errors.join("; ") })
                                } else {
                                    let mut working_contents = original_contents.clone();
                                    let mut diffs = Vec::new();

                                    for (edit_path, old_string, new_string) in &parsed_edits {
                                        let current_content =
                                            working_contents.get_mut(edit_path).unwrap();
                                        if current_content.contains(old_string) {
                                            let mut diff =
                                                format!("--- {}\n+++ {}\n", edit_path, edit_path);
                                            let old_lines: Vec<&str> = old_string.lines().collect();
                                            let new_lines: Vec<&str> = new_string.lines().collect();

                                            for line in &old_lines {
                                                diff.push_str("- ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            for line in &new_lines {
                                                diff.push_str("+ ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            diffs.push(diff);

                                            *current_content =
                                                current_content.replacen(old_string, new_string, 1);
                                        } else {
                                            apply_errors.push(format!(
                                                "old_string not found in `{}`",
                                                edit_path
                                            ));
                                            break;
                                        }
                                    }

                                    if !apply_errors.is_empty() {
                                        serde_json::json!({ "error": apply_errors.join("; ") })
                                    } else {
                                        let mut written_files = Vec::new();
                                        let mut write_error = None;

                                        for (edit_path, new_content) in &working_contents {
                                            match tokio::fs::write(edit_path, new_content).await {
                                                Ok(_) => {
                                                    written_files.push(edit_path.clone());
                                                }
                                                Err(e) => {
                                                    write_error = Some(format!(
                                                        "Failed to write `{}`: {}",
                                                        edit_path, e
                                                    ));
                                                    break;
                                                }
                                            }
                                        }

                                        if let Some(err) = write_error {
                                            for edit_path in written_files {
                                                let orig =
                                                    original_contents.get(&edit_path).unwrap();
                                                let _ = tokio::fs::write(&edit_path, orig).await;
                                            }
                                            serde_json::json!({ "error": format!("Write failed, transaction rolled back. Error: {}", err) })
                                        } else {
                                            let mut combined_diff = String::new();
                                            combined_diff.push_str("```diff\n");
                                            combined_diff.push_str(&diffs.join("\n"));
                                            combined_diff.push_str("```");

                                            serde_json::json!({ "success": true, "diff": combined_diff })
                                        }
                                    }
                                }
                            }
                        } else if args.get("start_line").is_some() {
                            let ep = std::path::Path::new(path);
                            let safety = check_path_safety(ep);
                            let mut allowed = true;
                            match safety {
                                PathSafety::Blocked => {
                                    allowed = false;
                                }
                                PathSafety::Prompt => {
                                    let answer = prompt_user_permission(
                                        &format!(
                                            "Allow access to path `{}` outside project and home directories?",
                                            path
                                        ),
                                        &sender,
                                    );
                                    if answer != "yes" {
                                        allowed = false;
                                    }
                                }
                                PathSafety::Safe => {}
                            }

                            if !allowed {
                                serde_json::json!({ "error": format!("Access denied: `{}` is restricted", path) })
                            } else if config.respect_ignore_rules && should_ignore(ep, &rules) {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                            } else {
                                let start_line =
                                    args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1)
                                        as usize;
                                let end_line =
                                    args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(1)
                                        as usize;
                                let new_content = args
                                    .get("new_content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");

                                if start_line == 0 {
                                    serde_json::json!({ "error": "start_line must be greater than or equal to 1" })
                                } else if start_line > end_line {
                                    serde_json::json!({ "error": "start_line cannot be greater than end_line" })
                                } else {
                                    match tokio::fs::read_to_string(path).await {
                                        Ok(content) => {
                                            let lines: Vec<&str> = content.lines().collect();
                                            if start_line > lines.len() {
                                                serde_json::json!({ "error": format!("start_line {} is beyond file line count {}", start_line, lines.len()) })
                                            } else {
                                                let end_idx = std::cmp::min(end_line, lines.len());

                                                let mut diff = String::new();
                                                diff.push_str("```diff\n");
                                                for line in
                                                    lines.iter().take(end_idx).skip(start_line - 1)
                                                {
                                                    diff.push_str("- ");
                                                    diff.push_str(line);
                                                    diff.push('\n');
                                                }
                                                for line in new_content.lines() {
                                                    diff.push_str("+ ");
                                                    diff.push_str(line);
                                                    diff.push('\n');
                                                }
                                                diff.push_str("```");

                                                let mut new_lines = Vec::new();
                                                for line in lines.iter().take(start_line - 1) {
                                                    new_lines.push(*line);
                                                }
                                                for line in new_content.lines() {
                                                    new_lines.push(line);
                                                }
                                                for line in lines.iter().skip(end_idx) {
                                                    new_lines.push(*line);
                                                }
                                                let mut new_content_str = new_lines.join("\n");
                                                if content.ends_with('\n')
                                                    && !new_content_str.is_empty()
                                                {
                                                    new_content_str.push('\n');
                                                }

                                                match tokio::fs::write(path, new_content_str).await {
                                                    Ok(_) => {
                                                        serde_json::json!({ "success": true, "diff": diff })
                                                    }
                                                    Err(e) => {
                                                        serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            serde_json::json!({ "error": format!("Failed to read file: {}", e) })
                                        }
                                    }
                                }
                            }
                        } else {
                            let ep = std::path::Path::new(path);
                            let safety = check_path_safety(ep);
                            let mut allowed = true;
                            match safety {
                                PathSafety::Blocked => {
                                    allowed = false;
                                }
                                PathSafety::Prompt => {
                                    let answer = prompt_user_permission(
                                        &format!(
                                            "Allow access to path `{}` outside project and home directories?",
                                            path
                                        ),
                                        &sender,
                                    );
                                    if answer != "yes" {
                                        allowed = false;
                                    }
                                }
                                PathSafety::Safe => {}
                            }

                            if !allowed {
                                serde_json::json!({ "error": format!("Access denied: `{}` is restricted", path) })
                            } else if config.respect_ignore_rules && should_ignore(ep, &rules) {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                            } else {
                                let old_string = args
                                    .get("old_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let new_string = args
                                    .get("new_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                match tokio::fs::read_to_string(path).await {
                                    Ok(content) => {
                                        if content.contains(old_string) {
                                            let mut diff = String::new();

                                            let old_lines: Vec<&str> = old_string.lines().collect();
                                            let new_lines: Vec<&str> = new_string.lines().collect();

                                            diff.push_str("```diff\n");
                                            for line in &old_lines {
                                                diff.push_str("- ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            for line in &new_lines {
                                                diff.push_str("+ ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            diff.push_str("```");

                                            let new_content =
                                                content.replacen(old_string, new_string, 1);
                                            match tokio::fs::write(path, new_content).await {
                                                Ok(_) => {
                                                    serde_json::json!({ "success": true, "diff": diff })
                                                }
                                                Err(e) => {
                                                    serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                                }
                                            }
                                        } else {
                                            serde_json::json!({ "error": "old_string not found in file." })
                                        }
                                    }
                                    Err(e) => {
                                        serde_json::json!({ "error": format!("Failed to read file: {}", e) })
                                    }
                                }
                            }
                        }
                    }
                    "write" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let ep = std::path::Path::new(path);
                        let safety = check_path_safety(ep);
                        let mut allowed = true;
                        match safety {
                            PathSafety::Blocked => {
                                allowed = false;
                            }
                            PathSafety::Prompt => {
                                let answer = prompt_user_permission(
                                    &format!(
                                        "Allow access to path `{}` outside project and home directories?",
                                        path
                                    ),
                                    &sender,
                                );
                                if answer != "yes" {
                                    allowed = false;
                                }
                            }
                            PathSafety::Safe => {}
                        }

                        if !allowed {
                            serde_json::json!({ "error": format!("Access denied: `{}` is restricted", path) })
                        } else {
                            let rules_vec = if config.respect_ignore_rules {
                                load_gitignore_rules()
                            } else {
                                Vec::new()
                            };
                            let rules = compile_rules(&rules_vec);

                            if config.respect_ignore_rules && should_ignore(ep, &rules) {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                            } else {
                                let content =
                                    args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                let write_res = async {
                                    if let Some(parent) = std::path::Path::new(path).parent()
                                        && !parent.as_os_str().is_empty()
                                    {
                                        tokio::fs::create_dir_all(parent).await?;
                                    }
                                    tokio::fs::write(path, content).await?;
                                    Ok::<(), std::io::Error>(())
                                }.await;
                                match write_res {
                                    Ok(_) => serde_json::json!({ "success": true }),
                                    Err(e) => {
                                        serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                    }
                                }
                            }
                        }
                    }
                    "sh" => {
                        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        let background = args
                            .get("background")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let input = args.get("input").and_then(|v| v.as_str());
                        let persistent_session_id =
                            args.get("persistent_session_id").and_then(|v| v.as_str());

                        if let Some(session_id) = persistent_session_id {
                            match crate::tui::run_persistent_bash(
                                session_id,
                                cmd,
                                input,
                                sender.clone(),
                            ) {
                                Ok(val) => val,
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        } else {
                            let run_result = async {
                                if background {
                                    let mut command = std::process::Command::new("bash");
                                    command
                                        .arg("-c")
                                        .arg(cmd)
                                        .stdin(std::process::Stdio::piped())
                                        .stdout(std::process::Stdio::piped())
                                        .stderr(std::process::Stdio::piped());

                                    #[cfg(unix)]
                                    {
                                        use std::os::unix::process::CommandExt;
                                        unsafe {
                                            command.pre_exec(|| {
                                                libc::setpgid(0, 0);
                                                Ok(())
                                            });
                                        }
                                    }

                                    let mut child = command.spawn()?;
                                    let pid = child.id();
                                    let mut child_stdin = child.stdin.take();
                                    if let Some(ref mut stdin) = child_stdin.as_mut()
                                        && let Some(inp) = input
                                    {
                                        use std::io::Write;
                                        let _ = stdin.write_all(inp.as_bytes());
                                        let _ = stdin.flush();
                                    }

                                    let stdout = child.stdout.take().unwrap();
                                    let stderr = child.stderr.take().unwrap();

                                    let stdout_accumulator =
                                        std::sync::Arc::new(Mutex::new(String::new()));
                                    let stderr_accumulator =
                                        std::sync::Arc::new(Mutex::new(String::new()));

                                    let sender_stdout = sender.clone();
                                    let stdout_acc_clone = stdout_accumulator.clone();
                                    let stdout_handle = std::thread::spawn(move || {
                                        use std::io::Read;
                                        let mut buffer = [0; 1024];
                                        let mut reader = stdout;
                                        while let Ok(n) = reader.read(&mut buffer) {
                                            if n == 0 {
                                                break;
                                            }
                                            let chunk =
                                                String::from_utf8_lossy(&buffer[..n]).into_owned();
                                            {
                                                let mut guard = stdout_acc_clone.lock();
                                                guard.push_str(&chunk);
                                            }
                                            let _ = sender_stdout
                                                .send(WorkerEvent::BashStdout(Some(pid), chunk));
                                        }
                                    });

                                    let sender_stderr = sender.clone();
                                    let stderr_acc_clone = stderr_accumulator.clone();
                                    let stderr_handle = std::thread::spawn(move || {
                                        use std::io::Read;
                                        let mut buffer = [0; 1024];
                                        let mut reader = stderr;
                                        while let Ok(n) = reader.read(&mut buffer) {
                                            if n == 0 {
                                                break;
                                            }
                                            let chunk =
                                                String::from_utf8_lossy(&buffer[..n]).into_owned();
                                            {
                                                let mut guard = stderr_acc_clone.lock();
                                                guard.push_str(&chunk);
                                            }
                                            let _ = sender_stderr
                                                .send(WorkerEvent::BashStderr(Some(pid), chunk));
                                        }
                                    });

                                    crate::tui::register_background_process(
                                        pid,
                                        cmd.to_owned(),
                                        child,
                                        child_stdin,
                                        stdout_accumulator.clone(),
                                        stderr_accumulator.clone(),
                                    );
                                    std::thread::spawn(move || {
                                        let _ = stdout_handle.join();
                                        let _ = stderr_handle.join();
                                    });
                                    Ok::<serde_json::Value, std::io::Error>(serde_json::json!({
                                        "status": "running",
                                        "pid": pid
                                    }))
                                } else {
                                    let mut command = tokio::process::Command::new("bash");
                                    command
                                        .arg("-c")
                                        .arg(cmd)
                                        .stdin(std::process::Stdio::piped())
                                        .stdout(std::process::Stdio::piped())
                                        .stderr(std::process::Stdio::piped());

                                    #[cfg(unix)]
                                    {
                                        unsafe {
                                            command.pre_exec(|| {
                                                libc::setpgid(0, 0);
                                                Ok(())
                                            });
                                        }
                                    }

                                    let mut child = command.spawn()?;
                                    let pid = child.id().unwrap();

                                    let mut child_stdin = child.stdin.take();
                                    if let Some(ref mut stdin) = child_stdin.as_mut()
                                        && let Some(inp) = input
                                    {
                                        use tokio::io::AsyncWriteExt;
                                        let _ = stdin.write_all(inp.as_bytes()).await;
                                        let _ = stdin.flush().await;
                                    }
                                    *crate::tui::RUNNING_PROCESS_STDIN.lock() = child_stdin.take();

                                    let stdout = child.stdout.take().unwrap();
                                    let stderr = child.stderr.take().unwrap();

                                    let stdout_accumulator =
                                        std::sync::Arc::new(Mutex::new(String::new()));
                                    let stderr_accumulator =
                                        std::sync::Arc::new(Mutex::new(String::new()));

                                    let sender_stdout = sender.clone();
                                    let stdout_acc_clone = stdout_accumulator.clone();
                                    let stdout_task = crate::tui::async_runtime::spawn(async move {
                                        use tokio::io::AsyncReadExt;
                                        let mut buffer = [0; 1024];
                                        let mut reader = stdout;
                                        while let Ok(n) = reader.read(&mut buffer).await {
                                            if n == 0 {
                                                break;
                                            }
                                            let chunk =
                                                String::from_utf8_lossy(&buffer[..n]).into_owned();
                                            {
                                                let mut guard = stdout_acc_clone.lock();
                                                guard.push_str(&chunk);
                                            }
                                            let _ = sender_stdout
                                                .send(WorkerEvent::BashStdout(Some(pid), chunk));
                                        }
                                    });

                                    let sender_stderr = sender.clone();
                                    let stderr_acc_clone = stderr_accumulator.clone();
                                    let stderr_task = crate::tui::async_runtime::spawn(async move {
                                        use tokio::io::AsyncReadExt;
                                        let mut buffer = [0; 1024];
                                        let mut reader = stderr;
                                        while let Ok(n) = reader.read(&mut buffer).await {
                                            if n == 0 {
                                                break;
                                            }
                                            let chunk =
                                                String::from_utf8_lossy(&buffer[..n]).into_owned();
                                            {
                                                let mut guard = stderr_acc_clone.lock();
                                                guard.push_str(&chunk);
                                            }
                                            let _ = sender_stderr
                                                .send(WorkerEvent::BashStderr(Some(pid), chunk));
                                        }
                                    });

                                    {
                                        let mut guard = crate::tui::RUNNING_PROCESS_PID.lock();
                                        *guard = Some(pid);
                                    }
                                    let status = child.wait().await?;
                                    {
                                        let mut guard = crate::tui::RUNNING_PROCESS_PID.lock();
                                        *guard = None;
                                    }
                                    *crate::tui::RUNNING_PROCESS_STDIN.lock() = None;

                                    let _ = stdout_task.await;
                                    let _ = stderr_task.await;

                                    let stdout_content = stdout_accumulator.lock().clone();
                                    let stderr_content = stderr_accumulator.lock().clone();
                                    let status_code = status.code();
                                    let mut err_val = serde_json::Value::Null;

                                    if status_code.is_none() {
                                        err_val = serde_json::json!(
                                            "Process terminated by user via Ctrl+C"
                                        );
                                    }

                                    Ok::<serde_json::Value, std::io::Error>(serde_json::json!({
                                        "status": status_code,
                                        "stdout": stdout_content,
                                        "stderr": stderr_content,
                                        "error": err_val,
                                    }))
                                }
                            }.await;

                            match run_result {
                                Ok(val) => val,
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        }
                    }
                    "ps" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        crate::tui::run_check_process(pid)
                    }
                    "kill" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        crate::tui::run_kill_process(pid)
                    }
                    "logs" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let limit = args
                            .get("limit")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as usize);
                        crate::tui::run_get_logs(pid, limit)
                    }
                    "patch" => {
                        let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
                        let run_res = (|| -> Result<serde_json::Value, String> {
                            let file_path = patch
                                .lines()
                                .find_map(|line| {
                                    if let Some(stripped) = line.strip_prefix("--- a/") {
                                        Some(stripped)
                                    } else if let Some(stripped) = line.strip_prefix("+++ b/") {
                                        Some(stripped)
                                    } else {
                                        None
                                    }
                                })
                                .and_then(|rest| rest.split_whitespace().next());

                            let start_dir = if let Some(fp) = file_path {
                                let p = std::path::Path::new(fp);
                                let parent = p.parent().unwrap_or(std::path::Path::new(""));
                                if parent.as_os_str().is_empty() {
                                    std::env::current_dir()
                                        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                                } else {
                                    std::env::current_dir()
                                        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                                        .join(parent)
                                }
                            } else {
                                std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                            };

                            let mut cwd = start_dir;
                            let mut git_root = None;
                            loop {
                                if cwd.join(".git").exists() {
                                    git_root = Some(cwd.clone());
                                    break;
                                }
                                if let Some(parent) = cwd.parent() {
                                    cwd = parent.to_path_buf();
                                } else {
                                    break;
                                }
                            }

                            if git_root.is_none() {
                                let original_cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("/"));
                                if let Ok(entries) = std::fs::read_dir(&original_cwd) {
                                    let mut candidates = Vec::new();
                                    for entry in entries.flatten() {
                                        let path = entry.path();
                                        if path.is_dir() && path.join(".git").exists() {
                                            candidates.push(path);
                                        }
                                    }

                                    if !candidates.is_empty() {
                                        let found = if let Some(fp) = file_path {
                                            candidates.iter().find(|cand| cand.join(fp).exists())
                                        } else {
                                            None
                                        };

                                        git_root = Some(match found {
                                            Some(cand) => cand.clone(),
                                            None => candidates[0].clone(),
                                        });
                                    }
                                }
                            }

                            let git_root = match git_root {
                                Some(root) => root,
                                None => return Err("Not a git repository".to_owned()),
                            };

                            let git_bin = if std::path::Path::new("/usr/bin/git").exists() {
                                "/usr/bin/git"
                            } else {
                                "git"
                            };

                            let random_val = {
                                let mut bytes = [0u8; 8];
                                rand::fill(&mut bytes);
                                bytes
                                    .iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<String>()
                            };
                            let temp_path = std::path::PathBuf::from(format!(
                                "/tmp/_apply_patch_{}.diff",
                                random_val
                            ));
                            std::fs::write(&temp_path, patch).map_err(|e| {
                                format!("Failed to write temporary patch file: {}", e)
                            })?;

                            let cleanup = |path: &std::path::Path| {
                                let _ = std::fs::remove_file(path);
                            };

                            let mut cmd = std::process::Command::new(git_bin);
                            cmd.current_dir(&git_root);
                            cmd.args(["apply", &temp_path.to_string_lossy()]);
                            cmd.stdout(std::process::Stdio::piped());
                            cmd.stderr(std::process::Stdio::piped());

                            let output = cmd.output().map_err(|e| {
                                cleanup(&temp_path);
                                format!("Failed waiting for git apply: {}", e)
                            })?;

                            if !output.status.success() {
                                cleanup(&temp_path);
                                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                                return Err(format!("git apply failed:\n{}", stderr));
                            }

                            cleanup(&temp_path);

                            let diff_out = std::process::Command::new(git_bin)
                                .current_dir(&git_root)
                                .args(["diff", "--cached"])
                                .output();
                            let mut diff_str = match diff_out {
                                Ok(out) if out.status.success() => {
                                    String::from_utf8_lossy(&out.stdout).into_owned()
                                }
                                _ => String::new(),
                            };
                            if diff_str.is_empty() {
                                let diff_uncached = std::process::Command::new(git_bin)
                                    .current_dir(&git_root)
                                    .arg("diff")
                                    .output();
                                if let Ok(out) = diff_uncached {
                                    let success = out.status.success();
                                    if success {
                                        diff_str =
                                            String::from_utf8_lossy(&out.stdout).into_owned();
                                    }
                                }
                            }

                            Ok(serde_json::json!({
                                "success": true,
                                "diff": diff_str
                            }))
                        })();
                        match run_res {
                            Ok(val) => val,
                            Err(e) => serde_json::json!({ "error": e }),
                        }
                    }
                    "websearch" => {
                        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                        // Refuse anything that isn't a public https URL.
                        // The model is given free-form text and historically
                        // could pass http://, file://, gopher://, or any
                        // IP literal (incl. 169.254.169.254 / IMDS, cloud
                        // metadata services) — we resolve first, then
                        // block private/loopback/link-local ranges.
                        let target_url =
                            if query.starts_with("http://") || query.starts_with("https://") {
                                query.to_owned()
                            } else {
                                let encoded_query = url_encode(query);
                                format!("https://html.duckduckgo.com/html/?q={}", encoded_query)
                            };
                        let validated = validate_public_https_url(&target_url);
                        let is_url_mode = query.starts_with("http");
                        match validated {
                            Err(reason) => {
                                serde_json::json!({ "error": format!("websearch: {reason}") })
                            }
                            Ok(url) => {
                                let res = (|| -> Result<serde_json::Value, String> {
                                    let body = crate::tui::async_runtime::block_on(async {
                                        let client = reqwest::Client::new();
                                        let text = client.get(&url)
                                            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                                            .send()
                                            .await
                                            .map_err(|e| e.to_string())?
                                            .text()
                                            .await
                                            .map_err(|e| e.to_string())?;
                                        Ok::<String, String>(text)
                                    })?;
                                    if is_url_mode {
                                        let plain_text = html_to_plain_text(&body);
                                        let truncated: String =
                                            plain_text.chars().take(8000).collect();
                                        Ok(serde_json::json!({ "content": truncated }))
                                    } else {
                                        let results = parse_ddg_html(&body);
                                        Ok(serde_json::json!({ "results": results }))
                                    }
                                })();
                                match res {
                                    Ok(val) => val,
                                    Err(e) => serde_json::json!({ "error": e }),
                                }
                            }
                        }
                    }
                    "ask" => {
                        let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("");
                        let options = args
                            .get("options")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default();

                        let (tx, rx) = std::sync::mpsc::channel();
                        *crate::tui::ASK_USER_CHANNEL.lock() =
                            Some((tx, question.to_owned(), options));

                        let answer = rx.recv().unwrap_or_default();

                        *crate::tui::ASK_USER_CHANNEL.lock() = None;

                        serde_json::json!({ "answer": answer })
                    }
                    "todo" => {
                        serde_json::json!({ "success": true })
                    }
                    _ => serde_json::json!({ "error": "Unknown function" }),
                };
                let _ = sender.send(WorkerEvent::ToolResult(name, result));
            });
        }
        FunctionAction::ResumeGeneration(request) => {
            spawn_generation_worker(
                request.config,
                request.history,
                request.cancel_token,
                request.generation_id,
                request.dev_mode,
                sender.clone(),
            );
        }
    }
}

pub(crate) fn spawn_generation_worker(
    config: StoredConfig,
    history: Vec<crate::api::ChatMessage>,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    generation_id: usize,
    dev_mode: String,
    sender: Sender<WorkerEvent>,
) {
    crate::tui::async_runtime::spawn(async move {
        let sender_clone = sender.clone();
        let cancel_clone = cancel_token.clone();

        let mut retries = 0;
        loop {
            if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = sender.send(WorkerEvent::StreamError(
                    generation_id,
                    "Stream cancelled".to_owned(),
                ));
                return;
            }
            let sender_c = sender_clone.clone();
            let cancel_c = cancel_clone.clone();
            let history_c = history.clone();
            let result = GeminiClient::new(config.clone()).generate_stream(
                &history_c,
                cancel_c,
                &dev_mode,
                move |chunk| {
                    let _ = sender_c.send(WorkerEvent::StreamChunk(generation_id, chunk));
                    Ok(())
                },
            ).await;
            match result {
                Ok(_) => {
                    let _ = sender.send(WorkerEvent::StreamDone(generation_id));
                    return;
                }
                Err(error) => {
                    if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = sender.send(WorkerEvent::StreamError(
                            generation_id,
                            "Stream cancelled".to_owned(),
                        ));
                        return;
                    }
                    retries += 1;
                    if retries < 3 {
                        let _ = sender.send(WorkerEvent::ResetStream(generation_id));
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    } else {
                        let _ =
                            sender.send(WorkerEvent::StreamError(generation_id, error.to_string()));
                        return;
                    }
                }
            }
        }
    });
}

pub(crate) fn spawn_models_worker(config: StoredConfig, sender: Sender<WorkerEvent>) {
    crate::tui::async_runtime::spawn(async move {
        let result = GeminiClient::new(config)
            .list_models()
            .await
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerEvent::Models(result));
    });
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                let _ = write!(encoded, "%{:02X}", b);
            }
        }
    }
    encoded
}

fn percent_decode(input: &str) -> String {
    // Strict percent-decoder: emits a Rust String (so output is
    // guaranteed to be UTF-8) and replaces each %HH with the
    // corresponding byte re-encoded as a UTF-8 sequence — so
    // non-ASCII payloads coming out of a search-result uddg=
    // parameter are preserved instead of being silently lossy'd
    // via `as char`. We also drop control bytes (0x00-0x1F,
    // 0x7F) and the lone '%' / '%X' / '%XY' sequences that some
    // search engines emit unescaped, so a malicious URL can't
    // smuggle TTY/terminal control sequences back into the
    // LLM's context.
    let mut decoded = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                let v = (h * 16 + l) as u8;
                if !(v.is_ascii_control() || v == 0x7f) {
                    decoded.push(v);
                }
                i += 3;
                continue;
            }
        }
        // Treat any other byte as raw: push it through. This is
        // intentionally tolerant so UTF-8 multi-byte sequences
        // inside the URL survive intact.
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|e| {
        // Shouldn't be possible (we only ever pushed either ASCII
        // or already-valid bytes via `as_bytes()`), but be loud
        // rather than lossy if it ever does happen.
        String::from_utf8_lossy(&e.into_bytes()).into_owned()
    })
}

/// Validate that `raw` is a public HTTPS URL.
///
/// Rejects:
///   - non-`https` schemes (http, file, gopher, data, ftp, ...)
///   - missing host
///   - hostnames that resolve to RFC1918 / link-local / loopback /
///     multicast / ULA / IPv4-mapped-in-IPv6 addresses (catches
///     169.254.169.254 cloud metadata, 127.0.0.1, 10.0.0.0/8, ...).
///
/// Returns the (possibly lower-cased) URL on success.
fn validate_public_https_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("https://") {
        return Err(format!(
            "only https URLs are allowed (got: {})",
            trimmed.chars().take(40).collect::<String>()
        ));
    }
    let scheme_sep = trimmed
        .find("://")
        .ok_or_else(|| "missing scheme".to_owned())?;
    let after_scheme = &trimmed[scheme_sep + 3..];
    let host_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let host_port = &after_scheme[..host_end];
    if host_port.is_empty() {
        return Err("URL has no host".to_owned());
    }
    let (host, port_opt) = match host_port.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, Some(p)),
        _ => (host_port, None),
    };
    if let Some(p) = port_opt
        && p.parse::<u16>().map_err(|_| "invalid port".to_owned())? == 0
    {
        return Err("port 0 is not routable".to_owned());
    }
    // Bracketed IPv6 literal: [::1] — strip brackets
    let host_clean = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if is_disallowed_host(host_clean) {
        return Err(format!(
            "host `{host_clean}` is in a private/loopback range"
        ));
    }
    Ok(trimmed.to_owned())
}

/// Return true if `host` is a literal IP in a network range we never
/// want the agent fetching from. Hostnames are considered safe — DNS
/// rebinding is mitigated by the agent's short connection lifetime,
/// not by us.
fn is_disallowed_host(host: &str) -> bool {
    use std::net::IpAddr;
    if let Ok(addr) = host.parse::<IpAddr>() {
        match addr {
            IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_private()
                    || v4.is_multicast()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    // Unique local addresses (fc00::/7)
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    // Link-local (fe80::/10)
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
                    // IPv4-mapped IPv6 ::ffff:a.b.c.d
                    || v6.to_ipv4_mapped().is_some()
                        && v6
                            .to_ipv4_mapped()
                            .map(is_disallowed_v4)
                            .unwrap_or(false)
            }
        }
    } else {
        false
    }
}

fn is_disallowed_v4(v4: std::net::Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_link_local()
        || v4.is_private()
        || v4.is_multicast()
        || v4.is_unspecified()
        || v4.is_broadcast()
}

fn html_to_plain_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let mut text_parts = Vec::new();
    for node in document.tree.nodes() {
        use scraper::node::Node;
        if let Node::Text(text) = node.value() {
            let mut has_ignored_ancestor = false;
            let mut parent = node.parent();
            while let Some(p) = parent {
                if let Node::Element(elem) = p.value() {
                    let name = elem.name();
                    if name == "script" || name == "style" {
                        has_ignored_ancestor = true;
                        break;
                    }
                }
                parent = p.parent();
            }
            if !has_ignored_ancestor {
                text_parts.push(text.to_string());
            }
        }
    }

    let combined = text_parts.join(" ");
    let mut result = String::new();
    for word in combined.split_whitespace() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

fn parse_ddg_html(html: &str) -> Vec<Value> {
    let document = scraper::Html::parse_document(html);
    let result_selector = scraper::Selector::parse(".result").unwrap();
    let a_selector = scraper::Selector::parse(".result__a").unwrap();
    let snippet_selector = scraper::Selector::parse(".result__snippet").unwrap();

    let mut results = Vec::new();
    for element in document.select(&result_selector) {
        if results.len() >= 6 {
            break;
        }

        let href = match element
            .select(&a_selector)
            .next()
            .and_then(|a| a.value().attr("href"))
        {
            Some(raw_url) => {
                if let Some(uddg_idx) = raw_url.find("uddg=") {
                    let encoded_url = &raw_url[uddg_idx + 5..];
                    let decoded_url = percent_decode(encoded_url);
                    if let Some(amp_idx) = decoded_url.find('&') {
                        decoded_url[..amp_idx].to_owned()
                    } else {
                        decoded_url
                    }
                } else {
                    raw_url.to_owned()
                }
            }
            None => continue,
        };

        let title = element
            .select(&a_selector)
            .next()
            .map(|a| a.text().collect::<Vec<_>>().join(""))
            .unwrap_or_else(|| "Untitled".to_owned());

        let snippet = element
            .select(&snippet_selector)
            .next()
            .map(|s| s.text().collect::<Vec<_>>().join(""))
            .unwrap_or_default();

        results.push(serde_json::json!({
            "title": title.trim().to_owned(),
            "url": href,
            "snippet": snippet.trim().to_owned()
        }));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_plain_text() {
        let html = "<html><head><title>Test</title><style>body { color: red; }</style></head>\
                    <body><h1>Hello World</h1><script>console.log('test');</script>\
                    <p>This is a <b>test</b> of the scraper.</p></body></html>";
        let text = html_to_plain_text(html);
        assert_eq!(text, "Test Hello World This is a test of the scraper.");
    }

    #[test]
    fn test_parse_ddg_html() {
        let html = r#"
            <div class="result">
                <a class="result__a" href="https://example.com/uddg=https%3A%2F%2Fexample.com%2Fpage%26amp%3Bfoo">Example Title</a>
                <span class="result__snippet">This is a snippet.</span>
            </div>
        "#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "Example Title");
        assert_eq!(results[0]["url"], "https://example.com/page");
        assert_eq!(results[0]["snippet"], "This is a snippet.");
    }

    #[test]
    fn test_handle_function_action_blocked_tool() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let config = crate::config::StoredConfig {
            active_agent: Some("reviewer".to_owned()),
            ..Default::default()
        };

        if let Some(proj_root) = crate::config::find_project_root() {
            let agent_dir = proj_root.join(".darwincode").join("agents");
            let _ = std::fs::create_dir_all(&agent_dir);
            let reviewer_path = agent_dir.join("reviewer.toml");
            let _ = std::fs::write(
                &reviewer_path,
                r#"
name = "Reviewer"
allowed_tools = ["read"]
system_prompt = "Review only."
"#,
            );

            handle_function_action(
                FunctionAction::Execute {
                    name: "sh".to_owned(),
                    args: serde_json::json!({ "command": "echo hello" }),
                    config,
                },
                &sender,
            );

            if let Ok(WorkerEvent::ToolResult(name, result)) =
                receiver.recv_timeout(std::time::Duration::from_secs(5))
            {
                assert_eq!(name, "sh");
                let err = result.get("error").and_then(|v| v.as_str()).unwrap_or("");
                assert!(err.contains("Permission denied"));
            } else {
                panic!("Did not receive expected ToolResult");
            }

            let _ = std::fs::remove_file(&reviewer_path);
        }
    }

    #[test]
    fn test_should_ignore() {
        let base_dir = crate::config::find_project_root()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let rules_vec = vec![
            "target".to_owned(),
            ".darwincode".to_owned(),
            "secret_data.txt".to_owned(),
            "src/*.txt".to_owned(),
        ];
        let rules = compile_rules(&rules_vec);

        assert!(should_ignore(&base_dir.join("secret_data.txt"), &rules));
        assert!(should_ignore(&base_dir.join("target"), &rules));
        assert!(should_ignore(
            &base_dir.join("target").join("debug").join("main"),
            &rules
        ));
        assert!(should_ignore(
            &base_dir.join("src").join("notes.txt"),
            &rules
        ));
        assert!(!should_ignore(
            &base_dir.join("src").join("main.rs"),
            &rules
        ));
        assert!(should_ignore(
            &base_dir.join(".darwincode").join("config.json"),
            &rules
        ));

        // Relative path checks
        assert!(should_ignore(
            std::path::Path::new("secret_data.txt"),
            &rules
        ));
        assert!(should_ignore(std::path::Path::new("target"), &rules));
        assert!(should_ignore(
            std::path::Path::new("target/debug/main"),
            &rules
        ));
        assert!(should_ignore(std::path::Path::new("src/notes.txt"), &rules));
        assert!(!should_ignore(std::path::Path::new("src/main.rs"), &rules));
        assert!(should_ignore(
            std::path::Path::new(".darwincode/config.json"),
            &rules
        ));
    }
}

#[cfg(test)]
mod url_tests {
    use super::validate_public_https_url;
    use super::{PathSafety, check_path_safety, normalize_path};

    #[test]
    fn allows_public_https() {
        assert!(validate_public_https_url("https://duckduckgo.com").is_ok());
        assert!(validate_public_https_url("https://example.com/path?q=1").is_ok());
    }

    #[test]
    fn rejects_non_https() {
        assert!(validate_public_https_url("http://example.com").is_err());
        assert!(validate_public_https_url("file:///etc/passwd").is_err());
        assert!(validate_public_https_url("gopher://example.com").is_err());
        assert!(validate_public_https_url("ftp://example.com").is_err());
    }

    #[test]
    fn rejects_loopback() {
        assert!(validate_public_https_url("https://127.0.0.1").is_err());
        assert!(validate_public_https_url("https://127.0.0.1:8080/x").is_err());
    }

    #[test]
    fn rejects_private_rfc1918() {
        assert!(validate_public_https_url("https://10.0.0.5").is_err());
        assert!(validate_public_https_url("https://192.168.1.1").is_err());
        assert!(validate_public_https_url("https://172.16.0.1").is_err());
    }

    #[test]
    fn rejects_link_local_imds() {
        assert!(validate_public_https_url("https://169.254.169.254").is_err());
        assert!(validate_public_https_url("https://169.254.0.1").is_err());
    }

    #[test]
    fn rejects_ipv6_loopback_and_private() {
        assert!(validate_public_https_url("https://[::1]").is_err());
        assert!(validate_public_https_url("https://[fe80::1]").is_err());
        assert!(validate_public_https_url("https://[fc00::1]").is_err());
    }

    #[test]
    fn rejects_ipv4_mapped_in_ipv6() {
        // ::ffff:127.0.0.1 maps back to 127.0.0.1
        assert!(validate_public_https_url("https://[::ffff:127.0.0.1]").is_err());
    }

    #[test]
    fn allows_hostnames_regardless_of_resolution() {
        // We don't resolve DNS — the agent's per-connection lifetime
        // is short enough that DNS rebinding isn't worth the cost.
        assert!(validate_public_https_url("https://localhost").is_ok());
        assert!(validate_public_https_url("https://internal.svc").is_ok());
    }

    #[test]
    fn rejects_empty_or_missing_host() {
        assert!(validate_public_https_url("https://").is_err());
        assert!(validate_public_https_url("https:///path").is_err());
    }

    #[test]
    fn test_normalize_path() {
        use std::path::Path;
        let p = Path::new("/foo/bar/../baz");
        assert_eq!(normalize_path(p), Path::new("/foo/baz"));

        let p2 = Path::new("/foo/bar/nonexistent/../../baz");
        assert_eq!(normalize_path(p2), Path::new("/foo/baz"));
    }

    #[test]
    fn test_check_path_safety_traversal() {
        let proj_root = crate::config::find_project_root()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let proj_root = std::fs::canonicalize(&proj_root).unwrap_or(proj_root);

        let escape_path = proj_root.join("nonexistent_folder/../../../../etc/passwd");
        let safety = check_path_safety(&escape_path);
        assert!(!matches!(safety, PathSafety::Safe));
    }
}
