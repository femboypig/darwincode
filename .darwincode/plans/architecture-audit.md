# DarwinCode Architecture Audit & Analysis

**Audit Date:** 2026-06-08  
**Project:** darwincode v1.9.98  
**Auditor Role:** Principal Rust Engineer & Systems Architect

---

## Executive Summary

Darwincode is a terminal-based AI coding assistant (TUI) written in Rust using Ratatui for UI and integrating with LLM providers (Gemini/OpenAI-compatible APIs). The codebase demonstrates **solid fundamentals** but has **critical architectural issues** that block scalability, introduce potential deadlocks, and create performance bottlenecks.

**Overall Assessment:** 6.5/10
- ✅ Strong: Security model, config encryption, tool safety
- ⚠️ Concerning: Synchronous blocking in async context, no proper event loop architecture
- ❌ Critical: Missing tokio runtime, threading model fragile, potential deadlocks

---

## 1. CRITICAL ARCHITECTURAL ANALYSIS

### 1.1 ✅ **What's Done Well**

#### Security & Safety (Grade: A-)
```rust
// crypto.rs - Hardware-bound encryption with keyring fallback
pub fn derive_hardware_key() -> Result<[u8; 32]>
// ✅ AES-256-GCM with proper nonce generation
// ✅ Keyring integration for secrets
// ✅ Fallback to encrypted machine-id file
```

**Strengths:**
- Proper crypto: AES-256-GCM, random nonces, secure key derivation
- API keys NOT stored in plaintext (addresses M3 security audit finding)
- Path traversal protection in `tool_executor.rs`
- Validation of external URLs (blocks SSRF, metadata endpoints)

#### Configuration Management (Grade: B+)
```rust
// config.rs - Hierarchical config with workspace trust
StoredConfig::load() // Global + project-level merge
```

**Strengths:**
- JSON merge logic for project-specific overrides
- Workspace trust system prevents prompt injection attacks
- Theme auto-detection (Windows/macOS/Linux)
- Validation prevents http:// for non-loopback hosts

#### Tool Execution Safety (Grade: A)
```rust
// tool_executor.rs - Multi-layer safety
check_path_safety() -> PathSafety::{Safe, Blocked, Prompt}
should_ignore() // .gitignore/.darwincode/ignore respect
```

**Strengths:**
- Blocks sensitive paths (.ssh, .aws, .gnupg, system dirs)
- Prompts user for paths outside project/home
- Respects .gitignore by default
- Agent-based tool restrictions

### 1.2 ❌ **Critical Architectural Problems**

#### Problem 1: NO TOKIO RUNTIME (CRITICAL)
```toml
# Cargo.toml - NO async runtime!
[dependencies]
# ❌ NO tokio, async-std, smol, etc.
ureq = "2.12.1"  # ⚠️ SYNCHRONOUS HTTP only
```

**Impact:**
```rust
// api/client.rs - Blocking I/O on worker threads
pub fn generate_stream(&self, ...) -> Result<()> {
    // ❌ This BLOCKS the thread for 900 seconds!
    let agent = ureq::AgentBuilder::new()
        .timeout_read(std::time::Duration::from_secs(900))
        .build();
    // Every API call spawns a NEW thread that BLOCKS
}
```

**Consequences:**
1. **UI freezes** if network is slow (no async/await)
2. **Thread exhaustion** under high load (each request = 1 blocking thread)
3. **No cancellation** without explicit polling (cancel_token is polled per-chunk, not per-connection)
4. **Cannot scale** to multiple concurrent generations

**Why This Matters:**
- Modern Rust async is designed for I/O-bound workloads (LLM streaming)
- Blocking threads cost ~2MB stack each (async tasks: ~2KB)
- No backpressure mechanism with blocking HTTP

#### Problem 2: FRAGILE THREADING MODEL
```rust
// tui/event_loop.rs - Manual threading with channels
fn run_loop(terminal, app, sender, receiver) -> Result<()> {
    while !app.should_quit {
        // ❌ Polling mpsc receiver in tight loop
        while let Ok(event) = receiver.try_recv() {
            handle_worker_event(app, event, sender);
        }
        
        // ❌ Polling crossterm events with 100ms timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? { /* ... */ }
        }
    }
}
```

**Issues:**
1. **No actor model** - Direct mutation of `App` from multiple threads
2. **Race conditions** on shared state (`app.chat.streaming_parts`, `app.proc.pending`)
3. **Busy-waiting** on `try_recv()` + `event::poll()` = CPU waste
4. **No structured concurrency** - thread::spawn everywhere, no join handles tracked

#### Problem 3: PARKING_LOT MUTEX ABUSE
```rust
// tui/mod.rs - Global static mutexes
pub static RUNNING_PROCESS_PID: parking_lot::Mutex<Option<u32>> = 
    parking_lot::Mutex::new(None);
pub static RUNNING_PROCESS_STDIN: parking_lot::Mutex<Option<std::process::ChildStdin>> = 
    parking_lot::Mutex::new(None);
pub static ASK_USER_CHANNEL: parking_lot::Mutex<Option<AskUserChannel>> = 
    parking_lot::Mutex::new(None);
```

**Deadlock Risk:**
```rust
// Scenario: User presses Ctrl+C during tool execution
// Thread A (event handler): Acquires RUNNING_PROCESS_STDIN to send Ctrl+C
// Thread B (tool executor): Holds RUNNING_PROCESS_STDIN, waiting on ASK_USER_CHANNEL
// Thread C (UI loop): Processes ask() tool, needs RUNNING_PROCESS_STDIN for validation
// → DEADLOCK if lock acquisition order differs
```

**Why parking_lot Doesn't Help:**
- `parking_lot::Mutex` is faster but STILL blocking
- No timeout, no try_lock in critical paths
- Global statics violate Rust ownership model

#### Problem 4: UNBOUNDED MEMORY GROWTH
```rust
// app/state.rs
pub fn handle_stream_chunk(&mut self, response: GeminiResponse) {
    // ❌ No bounds checking!
    self.chat.streaming_parts.push(part.clone());
    self.chat.messages.push(MessageLine { text, ... });
    // With long-running generation, this LEAKS memory
}
```

**Missing Safeguards:**
1. No max message count (history grows indefinitely)
2. No chunk buffering/flushing (all chunks kept in memory)
3. `cached_wrapped` in MessageLine never gets cleared on old messages

#### Problem 5: SYNCHRONOUS TOOL EXECUTION
```rust
// tui/tool_executor.rs
pub(crate) fn handle_function_action(action: FunctionAction, sender: &Sender<WorkerEvent>) {
    match action {
        FunctionAction::Execute { name, args, config } => {
            let sender = sender.clone();
            thread::spawn(move || {  // ❌ New thread per tool call!
                let result = match name.as_str() {
                    "read" => { /* BLOCKS on filesystem I/O */ }
                    "grep" => { /* BLOCKS scanning entire tree */ }
                    "sh" => { /* BLOCKS waiting for child process */ }
                    // ...
                };
                let _ = sender.send(WorkerEvent::ToolResult(name, result));
            });
        }
    }
}
```

**Problems:**
1. **Thread per tool call** doesn't scale (imagine 10 concurrent grep operations)
2. **No work-stealing** or thread pool
3. **Recursive operations** (grep, glob) can take minutes on large codebases
4. **No timeout** on tool execution (user must Ctrl+C)

---

## 2. НАЙДЕННЫЕ БАГИ И "УЗКИЕ МЕСТА"

### Bug #1: Race Condition in Stream Cancellation (CRITICAL)
```rust
// app/state.rs:cancel_generation()
pub fn cancel_generation(&mut self) {
    if let Some(token) = self.proc.cancel_token.take() {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // ❌ BUG: streaming_parts cleared BEFORE worker thread reads it!
    self.chat.streaming_parts.clear();
    // Worker thread may still be pushing to streaming_parts
}
```

**Race Scenario:**
1. User presses Esc to cancel
2. Main thread: `cancel_token.store(true)` + `streaming_parts.clear()`
3. Worker thread: Still processing chunks, calls `app.handle_stream_chunk()`
4. Worker pushes to `streaming_parts` AFTER clear
5. Orphaned chunks remain in memory forever

**Fix Required:**
- Use `Arc<Mutex<Vec<Part>>>` for streaming_parts
- Or: Add generation_id to each chunk, drop mismatched IDs

### Bug #2: Persistent Shell Session Leak (HIGH SEVERITY)
```rust
// tui/mod.rs:run_persistent_bash()
pub(crate) fn run_persistent_bash(
    session_id: &str, cmd: &str, input: Option<&str>, sender: Sender<WorkerEvent>
) -> Result<serde_json::Value, std::io::Error> {
    // ...
    let nonce = format!("CMD_DONE_{}", ...);
    let sentinel = format!("___SENTINEL_{}___", nonce);
    
    // ❌ BUG: If bash process hangs, we loop forever!
    let mut check_count = 0;
    let max_checks = 100;  // 5 seconds total
    while check_count < max_checks {
        std::thread::sleep(Duration::from_millis(50));
        // If output never contains sentinel, we timeout and return
        // BUT: bash process remains alive, leaking resources!
    }
}
```

**Leak Scenario:**
1. User runs `sh(cmd="sleep 999", persistent_session_id="main")`
2. Function times out after 5 seconds
3. Bash process still running, consuming memory
4. `PERSISTENT_SESSIONS` map retains entry
5. Subsequent commands to same session_id fail (process exited untracked)

**Fix Required:**
- Implement timeout with `child.kill()` on failure
- Track zombie sessions and cleanup

### Bug #3: File Backup Doesn't Handle Symlinks (MEDIUM)
```rust
// app/state.rs:backup_before_execution()
pub fn backup_before_execution(&mut self, name: &str, args: &serde_json::Value) {
    // ...
    let original_content = if resolved_path.exists() {
        std::fs::read_to_string(&resolved_path).ok()  // ❌ Follows symlinks!
    } else {
        None
    };
}
```

**Issue:**
- If file is a symlink to sensitive data (e.g., `/etc/passwd`), backup reads target
- Rollback writes to symlink target, potentially corrupting system files
- No `symlink_metadata()` check before read

### Bug #4: Gitignore Rules Over-Match (LOW)
```rust
// tool_executor.rs:compile_rules()
if rule.contains('/') {
    builder.add(glob::new(clean)); // Path-specific
    builder.add(glob::new(&format!("{}/**", clean))); // Descendants
} else {
    builder.add(glob::new(rule));
    builder.add(glob::new(&format!("**/{}", rule)));  // ❌ BUG HERE
    builder.add(glob::new(&format!("**/{}/**", rule))); // Over-match!
}
```

**Example:**
- Rule: `test`
- Matches: `test/`, `src/test/`, `src/tests/` (correct)
- But ALSO: `src/contest/`, `src/attest/` (false positive!)
- Reason: `**/test/**` matches any path segment containing "test"

### Bug #5: Session Save Doesn't Detect Write Failures (MEDIUM)
```rust
// app/session.rs (implied from state.rs)
pub fn save_session(&mut self) {
    if self.chat.session_id.starts_with("test_mock") {
        return;  // ✅ Good: Skip test sessions
    }
    let _ = super::session::save_session(&self.chat);  // ❌ Ignores Result!
}
```

**Impact:**
- Disk full / permission error silently fails
- User loses session data with no warning
- Should at least log error or show status message

### Bug #6: Model Name Trimming Inconsistency (LOW)
```rust
// app/core.rs
pub fn model_label(&self) -> &str {
    self.chat.config.model.trim_start_matches("models/")
}

// api/client.rs
let mut active_model = self.config.model.trim_start_matches("models/").to_owned();
```

**Issue:**
- Model stored with "models/" prefix in config
- Trimmed in UI and API layer independently
- If model name is "gemini-2.0-flash", it's stored AS-IS
- If model name is "models/gemini-2.0-flash", trimming works
- Inconsistent: Some paths trim, others don't

---

## 3. РЕКОМЕНДАЦИИ ПО РЕФАКТОРИНГУ

### Recommendation #1: Migrate to Tokio (HIGH PRIORITY)
```rust
// NEW: Cargo.toml
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["stream", "json"] }
tokio-util = { version = "0.7", features = ["codec"] }
```

**Architecture:**
```rust
// NEW: tui/async_runtime.rs
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

// NEW: api/client.rs
impl GeminiClient {
    pub async fn generate_stream_async(
        &self,
        history: &[ChatMessage],
        cancel_token: CancellationToken,  // tokio::sync::CancellationToken
    ) -> impl Stream<Item = Result<GeminiResponse>> {
        let response = self.client
            .post(&url)
            .timeout(Duration::from_secs(900))
            .send()
            .await?;
        
        response.bytes_stream()
            .map(/* parse SSE chunks */)
            .take_until(cancel_token.cancelled())
    }
}
```

**Benefits:**
- Non-blocking I/O (UI never freezes)
- Proper cancellation with `CancellationToken`
- Work-stealing scheduler (better CPU utilization)
- Async/await ergonomics

### Recommendation #2: Actor Model for State Management
```rust
// NEW: app/actor.rs
use tokio::sync::mpsc;

pub enum AppCommand {
    SubmitPrompt(String),
    CancelGeneration,
    HandleStreamChunk(usize, GeminiResponse),
    HandleToolResult(String, serde_json::Value),
}

pub struct AppActor {
    rx: mpsc::UnboundedReceiver<AppCommand>,
    state: App,
}

impl AppActor {
    pub async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                AppCommand::SubmitPrompt(text) => {
                    // ✅ All state mutation happens in ONE place
                    self.state.submit_chat_input();
                }
                AppCommand::HandleStreamChunk(id, chunk) => {
                    if id == self.state.proc.generation_id {
                        self.state.handle_stream_chunk(chunk);
                    }
                }
                // ...
            }
        }
    }
}
```

**Benefits:**
- No more `&mut App` shared across threads
- Message passing via channels (race-free)
- Easier to reason about state transitions

### Recommendation #3: Replace parking_lot with Async Primitives
```rust
// REPLACE:
pub static RUNNING_PROCESS_PID: parking_lot::Mutex<Option<u32>> = ...;

// WITH:
pub static RUNNING_PROCESS_PID: tokio::sync::RwLock<Option<u32>> = 
    tokio::sync::RwLock::const_new(None);

// Or better: Move to AppActor state
pub struct AppActor {
    running_process_pid: Option<u32>,
    process_stdin: Option<ChildStdin>,
}
```

### Recommendation #4: Implement Streaming Backpressure
```rust
// NEW: api/client.rs
pub async fn generate_stream_async(&self) -> impl Stream<Item = Result<GeminiResponse>> {
    response.bytes_stream()
        .map(parse_sse_chunk)
        .buffer_unordered(10)  // Max 10 chunks in flight
        .try_chunks(5)         // Flush every 5 chunks or 100ms
        .throttle(Duration::from_millis(100))
}
```

### Recommendation #5: Add Message History Bounds
```rust
// app/chat/mod.rs
const MAX_MESSAGES: usize = 1000;
const MAX_HISTORY_TOKENS: usize = 128_000;

impl ChatState {
    pub fn push_message(&mut self, msg: MessageLine) {
        self.messages.push(msg);
        
        // ✅ Prune old messages
        if self.messages.len() > MAX_MESSAGES {
            self.messages.drain(0..100);  // Remove oldest 100
        }
    }
    
    pub fn trim_history_by_tokens(&mut self) {
        // Implement token counting (approximate)
        // Drop oldest messages if exceeding MAX_HISTORY_TOKENS
    }
}
```

---

## 4. ПОШАГОВЫЙ ПЛАН ИСПРАВЛЕНИЯ (TODO)

### Phase 1: Critical Bugs (Week 1)
**Priority: CRITICAL**

1. **Fix race condition in cancel_generation()**
   - Status: pending
   - Priority: high
   - Content: Wrap `streaming_parts` in `Arc<Mutex<Vec<Part>>>`, check generation_id on push
   
2. **Fix persistent shell session leak**
   - Status: pending
   - Priority: high
   - Content: Add timeout with `child.kill()`, cleanup zombie sessions
   
3. **Fix symlink handling in file backup**
   - Status: pending
   - Priority: high
   - Content: Use `symlink_metadata()`, reject symlinks or backup link itself

### Phase 2: Async Migration (Week 2-3)
**Priority: HIGH**

4. **Add Tokio runtime**
   - Status: pending
   - Priority: high
   - Content: Add tokio + reqwest to Cargo.toml, create runtime singleton
   
5. **Migrate HTTP client to async**
   - Status: pending
   - Priority: high
   - Content: Replace ureq with reqwest, implement async streaming API
   
6. **Convert tool executor to async**
   - Status: pending
   - Priority: medium
   - Content: Use tokio::fs for file I/O, tokio::process for shell commands
   
7. **Implement actor model for App state**
   - Status: pending
   - Priority: high
   - Content: Create AppActor with mpsc channel, move all state mutations to actor

### Phase 3: Performance & Scalability (Week 4)
**Priority: MEDIUM**

8. **Add message history bounds**
   - Status: pending
   - Priority: medium
   - Content: Implement MAX_MESSAGES + token-based pruning
   
9. **Implement streaming backpressure**
   - Status: pending
   - Priority: medium
   - Content: Add buffer_unordered + throttle to stream processing
   
10. **Add tool execution timeout**
    - Status: pending
    - Priority: medium
    - Content: Wrap tool calls with tokio::time::timeout()

### Phase 4: Architecture Improvements (Week 5-6)
**Priority: LOW**

11. **Replace parking_lot globals with tokio::sync**
    - Status: pending
    - Priority: low
    - Content: Convert all static Mutexes to RwLock or move to actor state
    
12. **Fix gitignore over-matching**
    - Status: pending
    - Priority: low
    - Content: Use proper glob anchoring, add test cases
    
13. **Add session save error handling**
    - Status: pending
    - Priority: low
    - Content: Check Result from save_session(), show user notification on failure
    
14. **Normalize model name handling**
    - Status: pending
    - Priority: low
    - Content: Store model name WITHOUT "models/" prefix, add/remove only at API boundary

### Phase 5: Testing & Validation (Week 7)
**Priority: MEDIUM**

15. **Add integration tests for async runtime**
    - Status: pending
    - Priority: medium
    - Content: Test cancellation, backpressure, concurrent generations
    
16. **Benchmark before/after async migration**
    - Status: pending
    - Priority: medium
    - Content: Measure memory usage, latency, throughput
    
17. **Load test with 100+ concurrent tool calls**
    - Status: pending
    - Priority: low
    - Content: Ensure no thread exhaustion or deadlocks

---

## 5. МЕТРИКИ КАЧЕСТВА КОДА

### Complexity Analysis
```
src/api/client.rs          - 520 lines  - Complexity: HIGH (HTTP + tool declarations)
src/tui/tool_executor.rs   - 2150 lines - Complexity: VERY HIGH (all tool logic in one file)
src/app/state.rs           - 1200 lines - Complexity: HIGH (state machine + event handlers)
src/tui/event_loop.rs      - 150 lines  - Complexity: MEDIUM (polling loop)
```

**Refactoring Recommendations:**
- Split `tool_executor.rs` into separate files per tool category:
  - `tools/filesystem.rs` (read, write, edit, glob, grep)
  - `tools/shell.rs` (sh, ps, kill, logs)
  - `tools/network.rs` (websearch)
  - `tools/interaction.rs` (ask, todo)

### Test Coverage (Estimated)
```
Overall:        ~15% (only unit tests in isolated modules)
Critical paths: ~5%  (no tests for event_loop, state machine, tool executor)
```

**Missing Test Categories:**
1. Integration tests for tool execution
2. Property-based tests for config merging
3. Concurrency tests (race conditions, deadlocks)
4. Fuzzing for HTTP parsing and tool arguments

---

## 6. ЗАКЛЮЧЕНИЕ

### Strong Points ✅
1. **Security-first design** - Encryption, path validation, SSRF protection
2. **Well-structured config system** - Hierarchical, validated, extensible
3. **Good separation of concerns** - TUI, API, app state in separate modules
4. **Comprehensive tool safety** - Agent restrictions, .gitignore respect

### Weak Points ⚠️
1. **No async runtime** - Blocks on I/O, cannot scale
2. **Fragile threading** - Manual channels + mutexes = deadlock risk
3. **Unbounded memory** - History grows forever, no pruning
4. **Race conditions** - Shared mutable state across threads

### Critical Fixes Required ❌
1. **Migrate to Tokio** - Blocking HTTP is architectural dead-end
2. **Implement actor model** - Eliminate shared mutable state
3. **Fix persistent shell leak** - Resource exhaustion in long sessions
4. **Add cancellation safety** - Race condition in stream handling

### Effort Estimate
- **Async migration:** 3-4 weeks (1 senior engineer)
- **Bug fixes:** 1 week
- **Testing + validation:** 1 week
- **Total:** ~6 weeks for production-ready async architecture

### Recommendation
**DO NOT SHIP** current architecture to production with concurrent workloads. The synchronous HTTP + threading model will fail under load. Migrate to Tokio first, then address race conditions and resource leaks.

---

**Final Grade: 6.5/10**
- Deductions: -2.0 for missing async runtime
- Deductions: -1.0 for race conditions
- Deductions: -0.5 for resource leaks
- Bonus: +1.0 for excellent security model

**With Tokio migration: Projected 8.5/10**
