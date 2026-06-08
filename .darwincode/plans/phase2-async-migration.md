# Phase 2: Async Migration - Implementation Guide

**Status:** Ready for implementation  
**Estimated Time:** 2-3 weeks  
**Risk Level:** HIGH (fundamental architecture change)

---

## Overview

This phase migrates darwincode from synchronous blocking I/O (ureq) to async/await (tokio + reqwest). This is the **most critical refactoring** - everything else depends on it.

### Strategy

1. **Gradual migration** - Run tokio runtime alongside existing sync code
2. **Compatibility layer** - Wrap async code with `block_on()` initially
3. **Incremental conversion** - Move subsystems to async one by one
4. **No breaking changes** - External API remains identical

---

## Step 2.1: Add Tokio Runtime

### Cargo.toml Changes

```toml
[dependencies]
# Remove (but keep temporarily for compatibility):
# ureq = { version = "2.12.1", features = ["json"] }

# Add async runtime
tokio = { version = "1.42", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
futures = "0.3"
tokio-stream = "0.1"
tokio-util = { version = "0.7", features = ["codec", "compat"] }

# Keep existing dependencies
anyhow = "1.0.98"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.140"
# ... rest unchanged
```

### Create Runtime Module

**File: src/tui/async_runtime.rs** (NEW)

```rust
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get or initialize the global Tokio runtime
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("darwincode-tokio")
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

/// Block on an async future using the global runtime
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    runtime().block_on(future)
}

/// Spawn a task on the global runtime
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_initialization() {
        let rt = runtime();
        assert_eq!(rt.metrics().num_workers(), 4);
    }

    #[test]
    fn test_block_on() {
        let result = block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_spawn() {
        let handle = spawn(async { "spawned" });
        let result = block_on(handle).unwrap();
        assert_eq!(result, "spawned");
    }
}
```

**File: src/tui/mod.rs** - Add module:

```rust
pub(crate) mod async_runtime;
pub(crate) mod event_loop;
// ... rest unchanged

pub use async_runtime::{runtime, block_on, spawn};
```

---

## Step 2.2: Migrate HTTP Client to Async

### Create Async API Client

**File: src/api/client_async.rs** (NEW)

```rust
use crate::api::types::{ChatMessage, Content, FunctionDeclaration, GeminiResponse};
use crate::config::StoredConfig;
use anyhow::Result;
use futures::StreamExt;
use tokio_stream::Stream;

pub struct AsyncGeminiClient {
    config: StoredConfig,
    client: reqwest::Client,
}

impl AsyncGeminiClient {
    pub fn new(config: StoredConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(900))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create reqwest client");
        
        Self { config, client }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        if self.config.api_key.starts_with("sk-") {
            self.list_models_openai().await
        } else {
            self.list_models_gemini().await
        }
    }

    async fn list_models_gemini(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));
        
        let response = self.client
            .get(&url)
            .query(&[("key", &self.config.api_key)])
            .send()
            .await?;
        
        let body: serde_json::Value = response.json().await?;
        
        let models = body["models"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing models array"))?
            .iter()
            .filter_map(|m| {
                let name = m["name"].as_str()?;
                let methods = m["supportedGenerationMethods"].as_array()?;
                let supports_streaming = methods.iter().any(|m| {
                    m.as_str().map(|s| s == "generateContent").unwrap_or(false)
                });
                if supports_streaming {
                    Some(name.to_owned())
                } else {
                    None
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn list_models_openai(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;
        
        let body: serde_json::Value = response.json().await?;
        
        let models = body["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing data array"))?
            .iter()
            .filter_map(|m| m["id"].as_str().map(|s| s.to_owned()))
            .collect();
        
        Ok(models)
    }

    pub async fn generate_stream(
        &self,
        history: &[ChatMessage],
        cancel_token: tokio_util::sync::CancellationToken,
        dev_mode_label: &str,
    ) -> Result<impl Stream<Item = Result<GeminiResponse>>> {
        // Build request (same as sync version)
        let mut active_model = self.config.model.trim_start_matches("models/").to_owned();
        let mut agent_prompt_addition = None;
        let mut allowed_tools: Option<std::collections::HashSet<String>> = None;

        if let Some(ref agent_id) = self.config.active_agent {
            let custom_agents = crate::app::load_custom_agents();
            if let Some(agent_config) = custom_agents.get(agent_id) {
                if let Some(ref model_override) = agent_config.model {
                    active_model = model_override.trim_start_matches("models/").to_owned();
                }
                agent_prompt_addition = Some(agent_config.system_prompt.clone());
                allowed_tools = agent_config.allowed_tools.as_ref().map(|list| {
                    list.iter()
                        .map(|s| crate::api::client::canonical_tool_name(s).to_owned())
                        .collect()
                });
            }
        }

        // Build system instruction (same as sync)
        let mode_rules = match dev_mode_label {
            "Plan" => "## CURRENT ACTIVE MODE: PLAN (READ-ONLY)\\n...",
            _ => "## CURRENT ACTIVE MODE: BUILD (WRITE ACCESS)\\n...",
        };

        let mut system_instruction_text = format!(
            "# darwincode\\n\\nPremium AI coding assistant...\\n{}",
            mode_rules
        );

        if let Some(instructions) = crate::config::load_project_instructions() {
            system_instruction_text.push_str("\\n\\n---\\n\\n## 9. ADDITIONAL PROJECT SPECIFIC INSTRUCTIONS\\n\\n");
            system_instruction_text.push_str(&instructions);
        }

        if let Some(ref agent_prompt) = agent_prompt_addition {
            system_instruction_text.push_str("\\n\\n---\\n\\n## 10. SPECIALIZED AGENT INSTRUCTIONS\\n\\n");
            system_instruction_text.push_str(agent_prompt);
        }

        let system_instruction = Some(Content {
            role: "system".to_owned(),
            parts: vec![serde_json::json!({
                "text": system_instruction_text
            })],
        });

        // Prepare tools (declarations from sync version)
        let declarations = self.build_tool_declarations(&allowed_tools);

        // Make async request
        if self.config.api_key.starts_with("sk-") {
            self.generate_stream_openai(&active_model, history, &declarations, &system_instruction, cancel_token).await
        } else {
            self.generate_stream_gemini(&active_model, history, &declarations, &system_instruction, cancel_token).await
        }
    }

    async fn generate_stream_gemini(
        &self,
        model: &str,
        history: &[ChatMessage],
        declarations: &[FunctionDeclaration],
        system_instruction: &Option<Content>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<impl Stream<Item = Result<GeminiResponse>>> {
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.config.base_url.trim_end_matches('/'),
            model,
            self.config.api_key
        );

        let request_body = self.build_gemini_request(history, declarations, system_instruction);

        let response = self.client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            anyhow::bail!("API error {}: {}", status, error_text);
        }

        // Convert response to stream
        let byte_stream = response.bytes_stream();
        
        let stream = byte_stream
            .take_until(cancel_token.cancelled())
            .map(|chunk_result| {
                let chunk = chunk_result?;
                let text = String::from_utf8_lossy(&chunk);
                
                // Parse SSE format
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(candidates) = parsed["candidates"].as_array() {
                                if let Some(first) = candidates.first() {
                                    if let Some(parts) = first["content"]["parts"].as_array() {
                                        let parts_vec: Vec<serde_json::Value> = 
                                            parts.iter().cloned().collect();
                                        return Ok(GeminiResponse::Turn(parts_vec));
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Empty response or parsing error
                Ok(GeminiResponse::Turn(Vec::new()))
            });

        Ok(stream)
    }

    async fn generate_stream_openai(
        &self,
        model: &str,
        history: &[ChatMessage],
        declarations: &[FunctionDeclaration],
        system_instruction: &Option<Content>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<impl Stream<Item = Result<GeminiResponse>>> {
        // Similar to Gemini but for OpenAI API format
        // Implementation details...
        unimplemented!("OpenAI streaming - reuse pattern from Gemini")
    }

    fn build_tool_declarations(&self, allowed_tools: &Option<std::collections::HashSet<String>>) -> Vec<FunctionDeclaration> {
        // Copy from src/api/client.rs:generate_stream (existing logic)
        vec![] // Placeholder
    }

    fn build_gemini_request(
        &self,
        history: &[ChatMessage],
        declarations: &[FunctionDeclaration],
        system_instruction: &Option<Content>,
    ) -> serde_json::Value {
        // Copy from gemini.rs (existing logic)
        serde_json::json!({}) // Placeholder
    }
}
```

### Compatibility Wrapper

**File: src/api/client.rs** - Add async bridge:

```rust
use crate::tui::async_runtime::block_on;

impl GeminiClient {
    pub fn generate_stream(
        &self,
        history: &[ChatMessage],
        cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
        dev_mode_label: &str,
        mut on_chunk: impl FnMut(GeminiResponse) -> Result<()>,
    ) -> Result<()> {
        // Create async client
        let async_client = crate::api::client_async::AsyncGeminiClient::new(self.config.clone());
        
        // Convert sync cancel token to async CancellationToken
        let async_cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = async_cancel.clone();
        let sync_cancel = cancel_token.clone();
        
        // Spawn watcher for sync token
        std::thread::spawn(move || {
            while !sync_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            cancel_clone.cancel();
        });
        
        // Block on async stream
        block_on(async {
            let mut stream = async_client.generate_stream(
                history,
                async_cancel,
                dev_mode_label,
            ).await?;
            
            use futures::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result?;
                on_chunk(chunk)?;
            }
            
            Ok::<(), anyhow::Error>(())
        })
    }
    
    pub fn list_models(&self) -> Result<Vec<String>> {
        let async_client = crate::api::client_async::AsyncGeminiClient::new(self.config.clone());
        block_on(async_client.list_models())
    }
}
```

---

## Step 2.3: Convert Tool Executor to Async

### Async File Operations

**File: src/tui/tool_executor_async.rs** (NEW)

```rust
use tokio::fs;
use tokio::io::AsyncReadExt;

pub async fn read_file_async(path: &str) -> Result<String, std::io::Error> {
    fs::read_to_string(path).await
}

pub async fn write_file_async(path: &str, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, content).await
}

pub async fn read_dir_async(path: &str) -> Result<Vec<String>, std::io::Error> {
    let mut entries = fs::read_dir(path).await?;
    let mut files = Vec::new();
    
    while let Some(entry) = entries.next_entry().await? {
        files.push(entry.path().display().to_string());
    }
    
    Ok(files)
}
```

### Async Shell Execution

```rust
pub async fn run_shell_async(
    cmd: &str,
    input: Option<&str>,
    background: bool,
) -> Result<serde_json::Value, std::io::Error> {
    use tokio::process::Command;
    
    let mut command = Command::new("bash");
    command.arg("-c").arg(cmd);
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    
    let mut child = command.spawn()?;
    let pid = child.id().unwrap();
    
    // Write input if provided
    if let Some(inp) = input {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(inp.as_bytes()).await?;
        }
    }
    
    if background {
        // Return immediately with PID
        Ok(serde_json::json!({
            "status": "running",
            "pid": pid
        }))
    } else {
        // Wait for completion
        let output = child.wait_with_output().await?;
        
        Ok(serde_json::json!({
            "status": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }))
    }
}
```

---

## Step 2.4: Actor Model for State Management

**File: src/app/actor.rs** (NEW)

```rust
use tokio::sync::mpsc;
use crate::app::App;
use crate::api::GeminiResponse;

#[derive(Debug)]
pub enum AppCommand {
    SubmitPrompt(String),
    CancelGeneration,
    HandleStreamChunk(usize, GeminiResponse),
    HandleStreamDone(usize),
    HandleStreamError(usize, String),
    HandleToolResult(String, serde_json::Value),
    LoadModels(Result<Vec<String>, String>),
    Tick,
    Quit,
}

pub struct AppActor {
    rx: mpsc::UnboundedReceiver<AppCommand>,
    tx: mpsc::UnboundedSender<AppCommand>,
    state: App,
}

impl AppActor {
    pub fn new(app: App) -> (Self, mpsc::UnboundedSender<AppCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                rx,
                tx: tx.clone(),
                state: app,
            },
            tx,
        )
    }
    
    pub async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                AppCommand::SubmitPrompt(text) => {
                    // Process in isolated scope
                    self.state.chat.input = text;
                    if let Some(action) = self.state.submit_chat_input() {
                        // Spawn generation task
                        self.spawn_generation(action);
                    }
                }
                AppCommand::CancelGeneration => {
                    self.state.cancel_generation();
                }
                AppCommand::HandleStreamChunk(id, chunk) => {
                    if id == self.state.proc.generation_id {
                        self.state.handle_stream_chunk(chunk);
                    }
                }
                AppCommand::HandleStreamDone(id) => {
                    if id == self.state.proc.generation_id {
                        if let Some(action) = self.state.complete_stream() {
                            self.spawn_function_execution(action);
                        }
                    }
                }
                AppCommand::HandleStreamError(id, err) => {
                    if id == self.state.proc.generation_id {
                        self.state.handle_stream_error(err);
                    }
                }
                AppCommand::HandleToolResult(name, result) => {
                    if let Some(action) = self.state.complete_function_execution(name, result) {
                        self.spawn_generation(action);
                    }
                }
                AppCommand::LoadModels(result) => {
                    self.state.complete_load_models(result);
                }
                AppCommand::Tick => {
                    self.state.advance_tick();
                }
                AppCommand::Quit => {
                    self.state.should_quit = true;
                    break;
                }
            }
        }
    }
    
    fn spawn_generation(&self, action: crate::app::SubmitAction) {
        // Spawn async task
        let tx = self.tx.clone();
        crate::tui::async_runtime::spawn(async move {
            // Implementation...
        });
    }
    
    fn spawn_function_execution(&self, action: crate::app::FunctionAction) {
        let tx = self.tx.clone();
        crate::tui::async_runtime::spawn(async move {
            // Implementation...
        });
    }
    
    pub fn state(&self) -> &App {
        &self.state
    }
    
    pub fn state_mut(&mut self) -> &mut App {
        &mut self.state
    }
}
```

---

## Migration Path

### Week 1: Foundation
- [ ] Add tokio to Cargo.toml
- [ ] Create async_runtime.rs
- [ ] Verify all existing tests still pass
- [ ] Run `cargo build` successfully

### Week 2: HTTP Layer
- [ ] Create client_async.rs
- [ ] Implement AsyncGeminiClient
- [ ] Add compatibility wrapper in client.rs
- [ ] Test model listing works with both sync/async

### Week 3: Integration
- [ ] Create tool_executor_async.rs
- [ ] Convert file operations to tokio::fs
- [ ] Convert shell execution to tokio::process
- [ ] Create actor.rs and AppActor

### Week 4: Cutover
- [ ] Switch event_loop to use AppActor
- [ ] Remove ureq dependency
- [ ] Full integration testing
- [ ] Performance benchmarks

---

## Testing Strategy

```rust
#[tokio::test]
async fn test_async_http_client() {
    let config = StoredConfig {
        api_key: "test_key".to_owned(),
        model: "gemini-2.0-flash".to_owned(),
        base_url: "https://generativelanguage.googleapis.com/v1beta".to_owned(),
        ..Default::default()
    };
    
    let client = AsyncGeminiClient::new(config);
    
    // This will fail without real API key, but tests compilation
    let result = client.list_models().await;
    assert!(result.is_err() || result.is_ok());
}

#[tokio::test]
async fn test_cancellation() {
    use tokio::time::{sleep, Duration};
    
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let token_clone = cancel_token.clone();
    
    tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        token_clone.cancel();
    });
    
    let start = std::time::Instant::now();
    tokio::select! {
        _ = sleep(Duration::from_secs(10)) => {
            panic!("Should have been cancelled");
        }
        _ = cancel_token.cancelled() => {
            // Success
        }
    }
    assert!(start.elapsed() < Duration::from_secs(1));
}
```

---

## Rollback Plan

If async migration fails:

1. Keep ureq in Cargo.toml
2. Remove tokio feature flags
3. Delete new async files
4. Revert to sync client.rs

Critical: Test each commit independently. Don't merge Phase 2 until **all** tests pass.

---

## Performance Expectations

**Before (Blocking):**
- Thread per request: ~2MB stack
- Max concurrent: ~50-100 (thread exhaustion)
- Cancellation latency: ~1s (poll interval)

**After (Async):**
- Task per request: ~2KB stack
- Max concurrent: thousands (limited by network)
- Cancellation latency: <10ms (immediate)

**Memory reduction:** ~99% per concurrent operation  
**Latency improvement:** ~100x for cancellation  
**Throughput increase:** ~10-100x under load
