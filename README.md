<p align="center">
  <a href="https://github.com/darwincode/darwincode">
    <img src="assets/logo.png" alt="darwincode logo" width="300">
  </a>
</p>
<p align="center">The open source terminal AI coding agent.</p>
<p align="center">
  <a href="https://github.com/femboypig/darwincode/actions"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/darwincode/darwincode/build.yml?style=flat-square&branch=main" /></a>
  <img alt="Rust version" src="https://img.shields.io/badge/rust-1.75%2B-blue?style=flat-square" />
  <img alt="License" src="https://github.com/femboypig/darwincode/blob/main/LICENSE?style=flat-square" />
</p>

<p align="center">
  <a href="README.md">English</a> |
  <a href="README.ru.md">Русский</a>
</p>

---

### Installation

```bash
# Build from source
git clone https://github.com/femboypig/darwincode.git
cd darwincode
cargo install --path .
```

### Controls

| Shortcut | Action |
| -------- | ------ |
| `Enter` | Run selected setup action / Send message |
| `Alt+Enter` | Insert a newline in the message input |
| `Tab` | Switch setup fields / Auto-complete `/` commands |
| `Up` / `Down` | Choose a setup value / Navigate model picker or history |
| `Ctrl+S` | Open settings from chat |
| `Ctrl+P` | Open interactive model switcher mid-conversation |
| `Ctrl+A` | Auto-apply OmniRoute defaults (when `sk-` key is typed) |
| `Esc` | Quit app, or return from settings to active chat |

### Chat Commands

| Command | Action |
| ------- | ------ |
| `/settings` | Open the settings dashboard |
| `/models` | Open the interactive model picker |
| `/permissions [safe\|guardian\|chaos]` | View or set active tool execution permission level |
| `/resume [session_id]` | Load a saved chat session (or open session selector) |
| `/new` | Start a new chat session |
| `/clear` | Clear the current chat history |
| `/history` | Show a list of all saved session IDs |
| `/help` | Display the help card listing all commands |
| `/exit` or `/quit` | Terminate the application |
| `Tab` (while typing `/`) | Accept the first command suggestion |

### Tool Access & Security Modes

The agent has access to native workspace tools to research, view, and modify code. You can configure three security levels in Settings:

* **Safe (Read-Only)** - Only read codebase tools (`read_file`, `list_directory`, `search_files`) are permitted. Writes and bash commands are blocked.
* **Guardian (Ask)** - The agent will prompt for confirmation before executing any system-modifying tools (`write_file`, `edit_file`, `run_bash_command`).
* **Chaos (Auto)** - The agent will auto-execute all tools instantly.

### Config & Local Encryption

- **Secure Keyring**: API keys are securely saved in your OS native keychain. If the keychain service is missing, it falls back to storing them in the encrypted config file.
- **Hardware-bound Encryption**: Settings (`config.json`) and session histories under `~/.config/darwincode/` are symmetrically encrypted with AES-256-GCM, keyed by your unique hardware machine ID and environment details.
