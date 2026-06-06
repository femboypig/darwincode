# Security Policy

This document describes the security policies and configuration guidelines for `darwincode`.

## API Key Security

To protect API credentials, `darwincode` transmits API keys securely:
- **Gemini API Requests**: Transmitted via the `"x-goog-api-key"` HTTP header instead of query parameters (which could be logged in plaintext by proxies or routers).
- **OpenAI/Other API Requests**: Transmitted via the standard `"Authorization"` header.
- **Key Storage**: Keys are stored using the OS-native secure keyring. Plaintext files are only used as an fallback when home/appdata directories are undefined or keyring is unavailable, and are protected with strict file permissions (`0600` on Unix).

## Workspace Security and Remote Code Execution (RCE) Warning

`darwincode` supports custom slash commands defined at both the global level and the workspace level:
- Global custom commands defined in `~/.config/darwincode/commands/` are considered trusted.
- Workspace-specific custom commands defined in `.darwincode/commands/*.toml` are considered **untrusted** by default.

> [!WARNING]
> Running custom workspace commands from untrusted repositories can lead to Remote Code Execution (RCE). A malicious repository could define a custom command that executes harmful shell scripts when triggered.

### Protection Model
To mitigate RCE risks, workspace commands will not run silently. If a workspace-specific command is triggered:
1. `darwincode` will check if the workspace is trusted via:
   - CLI flag: `--trust-workspace` passed during startup.
   - Global config: `"trust_workspace": true` in `config.json`.
2. If the workspace is not trusted, the TUI will display an interactive confirmation dialog prompting the user to approve command execution.

## Reporting a Vulnerability

If you discover a security vulnerability, please do not open a public GitHub issue. Instead, report it privately to the maintainers.
