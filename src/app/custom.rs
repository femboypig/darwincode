use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct CustomCommandConfig {
    pub description: String,
    pub model: Option<String>,
    pub context: Option<HashMap<String, String>>,
    pub prompt: CustomPromptConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CustomPromptConfig {
    pub template: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CustomAgentConfig {
    pub name: String,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub system_prompt: String,
}

impl CustomCommandConfig {
    pub fn execute(&self) -> anyhow::Result<String> {
        let mut template = self.prompt.template.clone();
        if let Some(ref context_map) = self.context {
            for (key, cmd) in context_map {
                let output = if cfg!(target_os = "windows") {
                    std::process::Command::new("cmd").args(["/C", cmd]).output()
                } else {
                    std::process::Command::new("sh").args(["-c", cmd]).output()
                };

                let result_str = match output {
                    Ok(out) => {
                        let out_str = String::from_utf8_lossy(&out.stdout).to_string();
                        let err_str = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            out_str
                        } else {
                            format!("Error running command '{}':\n{}", cmd, err_str)
                        }
                    }
                    Err(e) => format!("Failed to execute command '{}': {}", cmd, e),
                };

                let placeholder = format!("{{{{{}}}}}", key);
                template = template.replace(&placeholder, &result_str);
            }
        }
        Ok(template)
    }
}

pub fn find_global_commands_dir() -> Option<std::path::PathBuf> {
    crate::config::config_path().ok().and_then(|p| p.parent().map(|p| p.join("commands")))
}

pub fn load_custom_commands(trust_workspace: bool) -> HashMap<String, (CustomCommandConfig, bool)> {
    let mut commands = HashMap::new();

    // 1. Load global commands
    if let Some(global_dir) = find_global_commands_dir() {
        if let Ok(entries) = std::fs::read_dir(global_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if !file_name.contains("schema")
                        && !file_name.contains("template")
                        && !file_name.contains("example")
                        && let Ok(toml_content) = std::fs::read_to_string(&path)
                        && let Ok(config) = toml::from_str::<CustomCommandConfig>(&toml_content)
                    {
                        commands.insert(file_name.to_owned(), (config, false));
                    }
                }
            }
        }
    }

    // 2. Load workspace commands
    if let Some(proj_root) = crate::config::find_project_root() {
        let cmd_dir = proj_root.join(".darwincode").join("commands");
        if let Ok(entries) = std::fs::read_dir(cmd_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if !file_name.contains("schema")
                        && !file_name.contains("template")
                        && !file_name.contains("example")
                        && let Ok(toml_content) = std::fs::read_to_string(&path)
                        && let Ok(config) = toml::from_str::<CustomCommandConfig>(&toml_content)
                    {
                        if commands.contains_key(file_name) {
                            if trust_workspace {
                                commands.insert(file_name.to_owned(), (config, true));
                            }
                        } else {
                            commands.insert(file_name.to_owned(), (config, true));
                        }
                    }
                }
            }
        }
    }
    commands
}

pub fn load_custom_agents() -> HashMap<String, CustomAgentConfig> {
    let mut agents = HashMap::new();
    if let Some(proj_root) = crate::config::find_project_root() {
        let agent_dir = proj_root.join(".darwincode").join("agents");
        if let Ok(entries) = std::fs::read_dir(agent_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if !file_name.contains("schema")
                        && !file_name.contains("template")
                        && !file_name.contains("example")
                        && let Ok(toml_content) = std::fs::read_to_string(&path)
                        && let Ok(config) = toml::from_str::<CustomAgentConfig>(&toml_content)
                    {
                        agents.insert(file_name.to_owned(), config);
                    }
                }
            }
        }
    }
    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_command_execution() {
        let config_toml = r#"
description = "Test command"
prompt.template = "Hello, {{name}}! The status is {{status}}."
"#;
        let mut config: CustomCommandConfig = toml::from_str(config_toml).unwrap();

        let result = config.execute().unwrap();
        assert_eq!(result, "Hello, {{name}}! The status is {{status}}.");

        let mut context = HashMap::new();
        context.insert("name".to_owned(), "echo Antigravity".to_owned());
        context.insert("status".to_owned(), "echo Active".to_owned());
        config.context = Some(context);

        let result2 = config.execute().unwrap();
        assert!(result2.contains("Antigravity"));
        assert!(result2.contains("Active"));
    }

    #[test]
    fn test_custom_agent_deserialization() {
        let agent_toml = r#"
name = "Reviewer Agent"
model = "gemini-2.5-pro"
allowed_tools = ["read", "grep"]
system_prompt = "You are a code reviewer."
"#;
        let config: CustomAgentConfig = toml::from_str(agent_toml).unwrap();
        assert_eq!(config.name, "Reviewer Agent");
        assert_eq!(config.model, Some("gemini-2.5-pro".to_owned()));
        assert_eq!(
            config.allowed_tools,
            Some(vec!["read".to_owned(), "grep".to_owned()])
        );
        assert_eq!(config.system_prompt, "You are a code reviewer.");
    }
}
