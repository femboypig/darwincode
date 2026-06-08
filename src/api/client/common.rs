use anyhow::Result;
use std::thread;
use std::time::Duration;

pub fn read_error(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(code, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unknown error".to_owned());
            anyhow::anyhow!("API request failed with HTTP {code}: {message}")
        }
        ureq::Error::Transport(error) => anyhow::anyhow!("API request failed: {error}"),
    }
}

pub fn lowercase_types(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        if let Some(s) = obj
            .get("type")
            .and_then(|t| t.as_str())
            .map(|s| s.to_lowercase())
        {
            obj.insert("type".to_owned(), serde_json::json!(s));
        }
        for val in obj.values_mut() {
            lowercase_types(val);
        }
    } else if let Some(arr) = value.as_array_mut() {
        for val in arr {
            lowercase_types(val);
        }
    }
}

pub fn model_supports_vision(model: &str, base_url: &str) -> bool {
    let m = model.to_lowercase();
    let b = base_url.to_lowercase();

    if m.contains("deepseek")
        || b.contains("deepseek")
        || m.contains("coder")
        || m.contains("reasoner")
        || m.contains("r1")
    {
        return false;
    }
    true
}

pub fn execute_with_retry<F>(
    agent: &ureq::Agent,
    make_request: F,
) -> Result<ureq::Response, anyhow::Error>
where
    F: Fn(&ureq::Agent) -> Result<ureq::Response, ureq::Error>,
{
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(30);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match make_request(agent) {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                let is_ret = match &err {
                    ureq::Error::Transport(_) => true,
                    ureq::Error::Status(429, _) => true,
                    ureq::Error::Status(code, _) => *code >= 500 && *code < 600,
                };
                if is_ret && attempt < 5 {
                    thread::sleep(backoff);
                    backoff = (backoff * 2).min(max_backoff);
                } else {
                    return Err(read_error(err));
                }
            }
        }
    }
}

pub async fn execute_with_retry_async<F, Fut>(
    client: &reqwest::Client,
    make_request: F,
) -> Result<reqwest::Response, anyhow::Error>
where
    F: Fn(&reqwest::Client) -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(30);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match make_request(client).await {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                let is_ret = if let Some(status) = err.status() {
                    status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error()
                } else {
                    err.is_request() || err.is_connect() || err.is_timeout()
                };
                if is_ret && attempt < 5 {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                } else {
                    return Err(anyhow::anyhow!("API request failed: {}", err));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_supports_vision() {
        assert!(model_supports_vision("gpt-4o", "https://api.openai.com/v1"));
        assert!(model_supports_vision(
            "claude-3-5-sonnet",
            "https://api.anthropic.com/v1"
        ));
        assert!(model_supports_vision(
            "gemini-1.5-flash",
            "https://generativelanguage.googleapis.com"
        ));
        assert!(model_supports_vision(
            "big-pickle",
            "https://opencode.ai/zen/v1"
        ));
        assert!(!model_supports_vision(
            "deepseek-chat",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "deepseek-coder",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "deepseek-reasoner",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "qwen2.5-coder",
            "https://api.openai.com/v1"
        ));
        assert!(!model_supports_vision(
            "big-pickle",
            "https://api.deepseek.com/v1"
        ));
    }
}
