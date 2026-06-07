# Security Policy

This document describes the security model of `darwincode`, the
trust boundaries we operate in, and how to report a vulnerability
privately.

## Threat Model

`darwincode` is a terminal agent: the user types a prompt, the LLM
emits tool calls, and the runtime executes them on the user's
machine. The two attackers we plan against are:

1. **A compromised / malicious LLM provider.** A prompt-injection
   payload in fetched web content, a malicious tool-call argument,
   or a model that's been jailbroken by the upstream API. We treat
   every model response as adversarial input.
2. **A malicious workspace** (`git clone && darwincode`). A
   repository that ships a `.darwincode/commands/*.toml` or
   `.darwincode/agents/*.toml` designed to run shell commands the
   moment the user opens the project.

The trust boundary is the user. The LLM is never trusted; the
runtime is the only authority for "is this command safe to run
unsupervised?". We try to make that authority robust by default
but ultimately the user is the one who decides which prompts and
tool calls to confirm.

## API Key Security

- **Transmission**: the key is sent in the `Authorization: Bearer …`
  header (OpenAI-compatible) or the `x-goog-api-key` header
  (Gemini). It is never embedded in a URL.
- **At rest**: keys are encrypted with AES-256-GCM under a key
  derived from a machine-id file (`0600` on Unix) and/or the OS
  keyring. The encrypted config is checked into the keyring
  transparently; the file on disk only ever contains the empty
  string or the encrypted blob.
- **No quiet cleartext fallback**: if the environment provides no
  writable config dir (`$XDG_CONFIG_HOME` / `$HOME` / `$APPDATA` /
  `$USERPROFILE`), `darwincode` exits with status 2 and refuses to
  start. There is no code path that writes the API key in cleartext
  on disk.

## Network Egress (SSRF)

The `websearch` tool is the only way the LLM-driven runtime reaches
the network on its own. It is constrained as follows:

- Only `https://` is accepted; `http://`, `file://`, `gopher://`,
  `data:` etc. are rejected before the request is built.
- Literal IPv4 / IPv6 hosts are checked against private, loopback,
  link-local, multicast, broadcast, and ULA ranges. This blocks
  the cloud-instance-metadata endpoint (`169.254.169.254`),
  `127.0.0.1`, `10.0.0.0/8`, `192.168.0.0/16`, `172.16.0.0/12`,
  `fc00::/7`, `fe80::/10`, and their IPv4-mapped IPv6 equivalents.
- Hostnames are not resolved ahead of time. We rely on the
  short-lived connection and the user being able to abort the run
  via Ctrl+C.

## Path Safety

The `read` / `write` / `edit` / `grep` / `glob` tools consult a
blocklist before any FS access. Blocked paths include:

- System: `/etc`, `/proc`, `/sys`, `/dev`, `/root`, `/boot`,
  `/var/log`, `/var/lib`.
- Credentials under `$HOME`: `.ssh/`, `.aws/`, `.gnupg/`,
  `.kube/`, `.docker/`, `.config/gh/`, `.netrc`, `.pypirc`, plus
  shell history and config files (`.bashrc`, `.zshrc`, `.profile`,
  `.bash_history`, `.zsh_history`, `.lesshst`, `.viminfo`).

Paths inside the project root are allowed without confirmation.
Other paths under `$HOME` are allowed; everything else requires an
explicit y/n confirmation.

## Workspace Trust and RCE Warning

`darwincode` supports custom slash commands and agents at both the
global level and the workspace level:

- Global custom commands in `~/.config/darwincode/commands/` are
  trusted.
- Workspace-specific commands in `.darwincode/commands/*.toml`
  are **untrusted** by default. They may run `sh -c` / `cmd /C`
  against the project tree when the user invokes them.

> [!WARNING]
> Running custom workspace commands from untrusted repositories
> can lead to Remote Code Execution. A malicious repository can
> ship a `.darwincode/commands/build.toml` whose `context` block
> invokes arbitrary shell the moment the user types `/build`.

### Protection model

1. The first time `darwincode` enters a project, a **trust modal**
   asks the user explicitly:
   - **Yes, trust this workspace** — the project's canonical path
     is appended to `trusted_workspaces` in the config, and
     workspace commands become unconfirmed.
   - **No, keep asking** — the config stays untouched, and every
     workspace command still pops the y/n confirmation.
2. Mouse and scroll-wheel input is blocked while the modal is
   open, so the user must make a keyboard choice.
3. The trust list is canonical-path based: renaming the repo or
   moving the project tree re-prompts the user.

This is a **user-acknowledgment-based** model, not a technical
sandbox. The LLM can still drive `sh -c` directly via the
`sh` tool (which has its own per-call confirmation). Sandbox
isolation (bubblewrap / landlock / seccomp) is on the roadmap.

## Custom Agents

`~/.darwincode/agents/<name>.toml` may declare an
`allowed_tools` allow-list. Names are normalised through
`api::client::canonical_tool_name` so an agent can write
`read_file`, `grep_search`, `list_dir`, `bash`, `web_search` etc.
and the runtime maps them to the actual tool names it emits
(`read`, `grep`, `glob`, `sh`, `websearch`).

## Reporting a Vulnerability

Please don't open a public GitHub issue for suspected
vulnerabilities. Email the maintainers privately so we can ship
a fix before the details are public. Include a minimal
reproducer and the commit hash you tested against.
