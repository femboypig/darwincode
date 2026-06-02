# Contributing to darwincode

Thanks for wanting to contribute! Here are some quick guidelines to keep the codebase clean.

### What we merge
- Bug fixes
- Better TUI rendering or event handling
- New API providers / model integrations
- OS-specific fixes (Windows, macOS, Linux)
- Doc updates

If you want to add a major UI change or a core feature, please open an issue first to discuss it with the maintainers.

### Dev Setup
You'll need the Rust toolchain (2024 edition).

```bash
git clone https://github.com/femboypig/darwincode.git
cd darwincode
cargo check
cargo test
cargo run
```

### Code Structure
- `src/main.rs`: Entry point.
- `src/api/`: API communication layer (types, client).
- `src/app/`: App state management, chat, settings, sessions.
- `src/tui/`: TUI code:
  - `tui/render/`: UI screens rendering.
  - `tui/events/`: Key handlers & event loops.
- `src/crypto.rs` / `src/config.rs`: Settings and encryption.

### Code Style
- Run `cargo fmt --all` before committing.
- Run `cargo clippy --all-targets -- -D warnings` — no warnings allowed.
- Keep commits clean and focused.

### Pull Requests
- Every PR should link to an open issue.
- Use conventional commits for PR titles (`feat: ...`, `fix: ...`, `docs: ...`).
- If you change the TUI, please include a quick screenshot or video in the PR description.
