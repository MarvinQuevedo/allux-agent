use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;

use tokio::sync::mpsc;

use super::types::{
    ChatOptions, ChatRequest, LlmResponse, Message, ModelInfo, ResponseStats, ShowResponse,
    TagsResponse, ToolCallItem, ToolDefinition,
};

/// Capabilities of a model, as reported by Ollama's `/api/show`.
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    /// Model can use native tool calling.
    pub tools: bool,
    /// Model supports chain-of-thought (`think` toggle applies).
    pub thinking: bool,
    /// Model accepts images. Reserved for future multimodal support.
    #[allow(dead_code)]
    pub vision: bool,
    /// Native maximum context length, if reported.
    pub context_length: Option<u64>,
}

/// Events emitted during streaming chat.
#[derive(Debug)]
pub enum StreamEvent {
    /// A text delta from the LLM.
    TextDelta(String),
    /// The LLM finished responding.
    Done(LlmResponse),
    /// An error occurred.
    Error(String),
}

#[derive(Clone)]
pub struct OllamaClient {
    http: Client,
    base_url: String,
    pub model: String,
    /// Top-level `think` toggle sent with every request. `None` = model default.
    /// Set to `Some(false)` to disable reasoning for faster tool loops.
    pub think: Option<bool>,
    /// `keep_alive` sent with every request — keeps the model resident in
    /// VRAM/RAM between turns so large local models don't pay reload latency.
    /// `None` falls back to Ollama's default (5m).
    pub keep_alive: Option<String>,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
            model: model.into(),
            think: None,
            // Keep big local models warm across tool rounds by default.
            keep_alive: Some("30m".to_string()),
        }
    }

    /// Builder: set the `think` toggle for reasoning-capable models.
    pub fn with_think(mut self, think: Option<bool>) -> Self {
        self.think = think;
        self
    }

    /// Builder: set how long Ollama keeps the model loaded after a request.
    /// An empty string leaves Ollama's default (5m) in place.
    pub fn with_keep_alive(mut self, keep_alive: impl Into<String>) -> Self {
        let v = keep_alive.into();
        self.keep_alive = if v.trim().is_empty() { None } else { Some(v) };
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Send a chat request and stream the response.
    /// - Calls `on_chunk` for each text delta (empty string if tool call).
    /// - Returns `LlmResponse::Text` or `LlmResponse::ToolCalls` when done.
    pub async fn chat<F>(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        options: Option<ChatOptions>,
        mut on_chunk: F,
    ) -> Result<LlmResponse>
    where
        F: FnMut(&str),
    {
        let request = ChatRequest {
            model: &self.model,
            messages,
            stream: true,
            tools,
            options,
            think: self.think,
            keep_alive: self.keep_alive.as_deref(),
        };

        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .context("Failed to connect to Ollama. Is it running?")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg = format!("Ollama returned {status}: {body}");
            #[cfg(debug_assertions)]
            let msg = {
                let mut m = msg;
                if body.contains("does not support tools") {
                    m.push_str(
                        "\n\nThis model cannot use tools in Ollama. Try: ollama pull llama3.2 \
                         (or llama3.1, qwen2.5, mistral — see https://ollama.com/search?c=tools), \
                         then /model <name> in Allux.",
                    );
                }
                m
            };
            anyhow::bail!("{msg}");
        }

        let mut stream = response.bytes_stream();
        let mut text_buf = String::new();
        let mut tool_calls: Vec<ToolCallItem> = Vec::new();
        let mut stats = ResponseStats::default();
        let mut raw_buf: Vec<u8> = Vec::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Stream error")?;
            raw_buf.extend_from_slice(&chunk);

            while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = raw_buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<super::types::ChatChunk>(line) {
                    Ok(parsed) => {
                        // Accumulate tool calls
                        tool_calls.extend(parsed.message.tool_calls);

                        // Stream text
                        if !parsed.message.content.is_empty() {
                            on_chunk(&parsed.message.content);
                            text_buf.push_str(&parsed.message.content);
                        }

                        if parsed.done {
                            stats.prompt_tokens = parsed.prompt_eval_count;
                            stats.completion_tokens = parsed.eval_count;
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to parse chunk: {e}");
                    }
                }
            }
        }

        if !tool_calls.is_empty() {
            Ok(LlmResponse::ToolCalls { calls: tool_calls, text: text_buf, stats })
        } else {
            Ok(LlmResponse::Text { content: text_buf, stats })
        }
    }

    /// Send a chat request and stream the response through a channel.
    /// Each text delta is sent as `StreamEvent::TextDelta`, and the final result
    /// as `StreamEvent::Done`. This is non-blocking for use with TUI event loops.
    pub async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        options: Option<ChatOptions>,
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) {
        let request = ChatRequest {
            model: &self.model,
            messages,
            stream: true,
            tools,
            options,
            think: self.think,
            keep_alive: self.keep_alive.as_deref(),
        };

        let response = match self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!(
                    "Failed to connect to Ollama: {e}"
                )));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let _ = tx.send(StreamEvent::Error(format!(
                "Ollama returned {status}: {body}"
            )));
            return;
        }

        let mut stream = response.bytes_stream();
        let mut text_buf = String::new();
        let mut tool_calls: Vec<ToolCallItem> = Vec::new();
        let mut stats = ResponseStats::default();
        let mut raw_buf: Vec<u8> = Vec::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(format!("Stream error: {e}")));
                    return;
                }
            };
            raw_buf.extend_from_slice(&chunk);

            while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = raw_buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<super::types::ChatChunk>(line) {
                    Ok(parsed) => {
                        tool_calls.extend(parsed.message.tool_calls);

                        if !parsed.message.content.is_empty() {
                            let _ = tx.send(StreamEvent::TextDelta(
                                parsed.message.content.clone(),
                            ));
                            text_buf.push_str(&parsed.message.content);
                        }

                        if parsed.done {
                            stats.prompt_tokens = parsed.prompt_eval_count;
                            stats.completion_tokens = parsed.eval_count;
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        let result = if !tool_calls.is_empty() {
            LlmResponse::ToolCalls {
                calls: tool_calls,
                text: text_buf,
                stats,
            }
        } else {
            LlmResponse::Text {
                content: text_buf,
                stats,
            }
        };
        let _ = tx.send(StreamEvent::Done(result));
    }

    /// Unload the current model from Ollama memory (VRAM/RAM) by setting `keep_alive` to 0.
    pub async fn unload_model(&self) -> Result<()> {
        self.http
            .post(format!("{}/api/generate", self.base_url))
            .json(&serde_json::json!({
                "model": self.model,
                "keep_alive": 0
            }))
            .send()
            .await
            .context("Failed to unload model from Ollama")?;
        Ok(())
    }

    /// Query a model's capabilities (tools, thinking, vision, native context)
    /// via `POST /api/show`. Used to configure the session up front instead of
    /// discovering tool support reactively after a failed request.
    pub async fn capabilities(base_url: &str, model: &str) -> Result<ModelCapabilities> {
        let client = Client::new();
        let resp = client
            .post(format!("{base_url}/api/show"))
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
            .context("Cannot reach Ollama /api/show. Is it running on port 11434?")?;
        let show: ShowResponse = resp.json().await.context("Failed to parse /api/show")?;
        let has = |c: &str| show.capabilities.iter().any(|x| x == c);
        let context_length = show
            .model_info
            .iter()
            .find(|(k, _)| k.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64());
        Ok(ModelCapabilities {
            tools: has("tools"),
            thinking: has("thinking"),
            vision: has("vision"),
            context_length,
        })
    }

    /// List all locally available models.
    pub async fn list_models(base_url: &str) -> Result<Vec<ModelInfo>> {
        let client = Client::new();
        let resp = client
            .get(format!("{base_url}/api/tags"))
            .send()
            .await
            .context("Cannot reach Ollama. Is it running on port 11434?")?;
        let tags: TagsResponse = resp.json().await.context("Failed to parse Ollama model list")?;
        Ok(tags.models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::types::Message;

    const OLLAMA_URL: &str = "http://localhost:11434";

    async fn ollama_available() -> bool {
        Client::new().get(format!("{OLLAMA_URL}/api/tags")).send().await.is_ok()
    }

    async fn first_model() -> Option<String> {
        OllamaClient::list_models(OLLAMA_URL).await.ok()?.into_iter().next().map(|m| m.name)
    }

    #[tokio::test]
    async fn test_list_models_returns_vec() {
        if !ollama_available().await {
            eprintln!("SKIP: Ollama not running");
            return;
        }
        let models = OllamaClient::list_models(OLLAMA_URL).await.unwrap();
        assert!(!models.is_empty());
        for m in &models {
            assert!(!m.name.is_empty());
        }
    }

    #[tokio::test]
    async fn test_list_models_bad_url_returns_error() {
        let result = OllamaClient::list_models("http://localhost:1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_bad_model_returns_error() {
        if !ollama_available().await {
            eprintln!("SKIP: Ollama not running");
            return;
        }
        let client = OllamaClient::new(OLLAMA_URL, "nonexistent-model-xyz:latest");
        let messages = vec![Message::user("hi")];
        let result = client.chat(&messages, None, None, |_| {}).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_returns_text_response() {
        if !ollama_available().await {
            eprintln!("SKIP: Ollama not running");
            return;
        }
        let model = match first_model().await {
            Some(m) => m,
            None => return,
        };
        let client = OllamaClient::new(OLLAMA_URL, &model);
        let messages = vec![
            Message::system("Reply with exactly the word: OK"),
            Message::user("Say OK"),
        ];
        let mut output = String::new();
        let result = client.chat(&messages, None, None, |c| output.push_str(c)).await.unwrap();
        assert!(!output.is_empty());
        assert!(matches!(result, LlmResponse::Text { .. }));
        if let LlmResponse::Text { stats, .. } = result {
            assert!(stats.completion_tokens > 0);
        }
    }
}
