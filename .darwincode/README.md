# Darwincode Project Configuration

This directory contains configuration files for `darwincode` that are specific to this project repository.

## Directory Structure

*   `config.json` (created from `config.json.template`) — Project-level overrides for global configuration.
*   `ignore` — Specific files and directories to ignore during workspace scanning (combines with `.gitignore`).
*   `instructions.md` — Custom project-specific guidelines, coding standards, and style rules appended automatically to the AI prompt.
*   `.env` — Key-value secrets (API keys, tokens) loaded for custom tools/commands.
*   `themes/` — Custom theme JSON configuration files.
*   `commands/` — Custom slash-commands configured using TOML (copy `commands/schema.toml.template` to define new commands).
*   `agents/` — Custom specialized agent definitions configured using TOML (copy `agents/schema.toml.template` to define new agents).

To define new custom commands or agents, copy the respective `schema.toml.template` to a new `.toml` file in the same directory (e.g. `commands/build.toml` or `agents/reviewer.toml`) and edit its properties.
