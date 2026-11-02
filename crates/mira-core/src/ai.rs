//! Pluggable AI assistant for the notch.
//!
//! A small provider abstraction lets Mira answer from the island using either a
//! **local model** (Ollama — free, private, fits Mira's local-first stance) or a
//! **hosted API** the user pays for themselves (any OpenAI-compatible endpoint,
//! or Anthropic). There is intentionally no "Mira cloud": requests go straight
//! from the user's machine to the endpoint they chose.
//!
//! ## On "reusing a ChatGPT/Claude subscription"
//! A consumer ChatGPT Plus / Claude Pro subscription only authorizes the
//! vendor's own apps; it grants **no** programmatic access. Driving the web UI
//! or private endpoints violates those terms and risks the account, so Mira does
//! not do it. The free, first-class path here is a **local model via Ollama**;
//! the paid path is the user's **own API key**.
//!
//! ## Secrets
//! Mira never writes an API key to `config.toml`. Settings store only the *name*
//! of an environment variable (see [`AiConfig::api_key_env`]); the key is read
//! from the process environment at call time via [`resolve_key`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};

/// Which backend answers assistant prompts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind {
    /// Local [Ollama](https://ollama.com) daemon. No key, nothing leaves the LAN.
    #[default]
    Ollama,
    /// Any OpenAI-compatible `/v1/chat/completions` endpoint (OpenAI, OpenRouter,
    /// LM Studio, llama.cpp server, …). Bearer-token auth.
    #[serde(rename = "openai")]
    OpenAiCompatible,
    /// Anthropic Messages API (`x-api-key` + `anthropic-version`).
    Anthropic,
}

impl AiProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            AiProviderKind::Ollama => "Ollama (local)",
            AiProviderKind::OpenAiCompatible => "OpenAI-compatible",
            AiProviderKind::Anthropic => "Anthropic",
        }
    }

    /// Whether this backend needs an API key.
    pub fn requires_key(self) -> bool {
        !matches!(self, AiProviderKind::Ollama)
    }

    /// Default base URL (no trailing path) for the backend.
    pub fn default_endpoint(self) -> &'static str {
        match self {
            AiProviderKind::Ollama => "http://127.0.0.1:11434",
            AiProviderKind::OpenAiCompatible => "https://api.openai.com",
            AiProviderKind::Anthropic => "https://api.anthropic.com",
        }
    }

    /// A sensible default model id for first-run config.
    pub fn default_model(self) -> &'static str {
        match self {
            AiProviderKind::Ollama => "llama3.2",
            AiProviderKind::OpenAiCompatible => "gpt-4o-mini",
            AiProviderKind::Anthropic => "claude-3-5-haiku-latest",
        }
    }

    /// Path appended to the base URL for a chat completion.
    fn chat_path(self) -> &'static str {
        match self {
            AiProviderKind::Ollama => "/api/chat",
            AiProviderKind::OpenAiCompatible => "/v1/chat/completions",
            AiProviderKind::Anthropic => "/v1/messages",
        }
    }
}

/// Role of a message in the conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    System,
    #[default]
    User,
    Assistant,
}

impl AiRole {
    fn wire(self) -> &'static str {
        match self {
            AiRole::System => "system",
            AiRole::User => "user",
            AiRole::Assistant => "assistant",
        }
    }
}

/// A single turn in an assistant conversation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

impl AiMessage {
    pub fn user(content: impl Into<String>) -> Self {
        AiMessage {
            role: AiRole::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        AiMessage {
            role: AiRole::Assistant,
            content: content.into(),
        }
    }
    pub fn system(content: impl Into<String>) -> Self {
        AiMessage {
            role: AiRole::System,
            content: content.into(),
        }
    }
}

/// The assistant conversation state the notch renders (in memory only).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiConversation {
    pub messages: Vec<AiMessage>,
    /// True while a reply is in flight, so the UI can show a typing indicator.
    #[serde(default)]
    pub pending: bool,
}

impl AiConversation {
    pub fn push_user(&mut self, text: impl Into<String>) {
        self.messages.push(AiMessage::user(text));
    }
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.messages.push(AiMessage::assistant(text));
    }
    pub fn last_assistant(&self) -> Option<&AiMessage> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::Assistant)
    }
    /// Clear the thread (keeps no history).
    pub fn reset(&mut self) {
        self.messages.clear();
        self.pending = false;
    }
}

/// Resolved runtime configuration for a single request.
#[derive(Clone, Debug, PartialEq)]
pub struct AiConfig {
    pub kind: AiProviderKind,
    pub model: String,
    /// Base URL without the chat path. Falls back to the kind's default if empty.
    pub endpoint: String,
    /// Name of the environment variable holding the API key (never the key).
    pub api_key_env: String,
    /// Persona / behaviour prompt prepended as the system message.
    pub system_prompt: String,
    /// Upper bound on response length (required by Anthropic; advisory elsewhere).
    pub max_tokens: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        let kind = AiProviderKind::default();
        AiConfig {
            kind,
            model: kind.default_model().to_string(),
            endpoint: kind.default_endpoint().to_string(),
            api_key_env: "MIRA_AI_API_KEY".to_string(),
            system_prompt: "You are Mira, a concise assistant living in the screen's notch. \
                Answer briefly and helpfully."
                .to_string(),
            max_tokens: 1024,
        }
    }
}

impl AiConfig {
    /// Effective base URL (configured value, or the kind's default when blank).
    pub fn base_url(&self) -> String {
        let e = self.endpoint.trim().trim_end_matches('/');
        if e.is_empty() {
            self.kind.default_endpoint().to_string()
        } else {
            e.to_string()
        }
    }
}

/// Read the API key from the environment variable named in `cfg`.
/// Returns `None` for local providers that need none, or when the var is unset.
pub fn resolve_key(cfg: &AiConfig) -> Option<String> {
    if !cfg.kind.requires_key() {
        return None;
    }
    std::env::var(&cfg.api_key_env)
        .ok()
        .filter(|v| !v.is_empty())
}

/// Build the JSON request body for a provider from the full message list.
///
/// Pure and side-effect free so it can be asserted in tests. For Anthropic the
/// system turns are lifted into the top-level `system` field and only
/// user/assistant turns remain in `messages`.
pub fn build_request_body(cfg: &AiConfig, messages: &[AiMessage]) -> Value {
    match cfg.kind {
        AiProviderKind::Ollama => json!({
            "model": cfg.model,
            "messages": wire_messages(messages),
            "stream": false,
        }),
        AiProviderKind::OpenAiCompatible => json!({
            "model": cfg.model,
            "messages": wire_messages(messages),
            "max_tokens": cfg.max_tokens,
        }),
        AiProviderKind::Anthropic => {
            let system: String = messages
                .iter()
                .filter(|m| m.role == AiRole::System)
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let turns: Vec<Value> = messages
                .iter()
                .filter(|m| m.role != AiRole::System)
                .map(|m| json!({"role": m.role.wire(), "content": m.content}))
                .collect();
            let mut body = json!({
                "model": cfg.model,
                "max_tokens": cfg.max_tokens,
                "messages": turns,
            });
            if !system.is_empty() {
                body["system"] = Value::String(system);
            }
            body
        }
    }
}

fn wire_messages(messages: &[AiMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| json!({"role": m.role.wire(), "content": m.content}))
        .collect()
}

/// Extract the assistant's reply text from a provider's JSON response.
pub fn parse_response(kind: AiProviderKind, body: &Value) -> Result<String> {
    let text = match kind {
        AiProviderKind::Ollama => body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string),
        AiProviderKind::OpenAiCompatible => body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string),
        AiProviderKind::Anthropic => body.get("content").and_then(Value::as_array).map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        }),
    };
    match text {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err(Error::Ai(format!(
            "could not parse a reply from the {} response",
            kind.label()
        ))),
    }
}

/// Async client that performs a chat completion against the configured provider.
#[derive(Clone)]
pub struct AiClient {
    http: reqwest::Client,
    cfg: AiConfig,
    key: Option<String>,
}

impl AiClient {
    /// Build a client, resolving the API key from the environment if needed.
    /// Fails fast when a hosted provider is configured without a key in its env var.
    pub fn new(cfg: AiConfig) -> Result<Self> {
        let key = resolve_key(&cfg);
        if cfg.kind.requires_key() && key.is_none() {
            return Err(Error::Ai(format!(
                "{} needs an API key; set the ${} environment variable",
                cfg.kind.label(),
                cfg.api_key_env
            )));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(AiClient { http, cfg, key })
    }

    /// Full URL of the chat endpoint.
    fn url(&self) -> String {
        format!("{}{}", self.cfg.base_url(), self.cfg.kind.chat_path())
    }

    /// Send `history` (plus the configured system prompt) and return the reply.
    pub async fn complete(&self, history: &[AiMessage]) -> Result<String> {
        let mut messages: Vec<AiMessage> = Vec::with_capacity(history.len() + 1);
        if !self.cfg.system_prompt.trim().is_empty() {
            messages.push(AiMessage::system(self.cfg.system_prompt.clone()));
        }
        messages.extend_from_slice(history);

        let body = build_request_body(&self.cfg, &messages);
        let mut req = self.http.post(self.url()).json(&body);
        req = match self.cfg.kind {
            AiProviderKind::Ollama => req,
            AiProviderKind::OpenAiCompatible => {
                req.bearer_auth(self.key.as_deref().unwrap_or_default())
            }
            AiProviderKind::Anthropic => req
                .header("x-api-key", self.key.as_deref().unwrap_or_default())
                .header("anthropic-version", "2023-06-01"),
        };

        let resp = req.send().await?;
        let status = resp.status();
        let value: Value = resp.json().await.map_err(|e| {
            Error::Ai(format!(
                "{} returned a non-JSON response: {e}",
                self.cfg.kind.label()
            ))
        })?;
        if !status.is_success() {
            let detail = value
                .get("error")
                .map(|e| e.to_string())
                .unwrap_or_else(|| value.to_string());
            return Err(Error::Ai(format!(
                "{} error (HTTP {status}): {detail}",
                self.cfg.kind.label()
            )));
        }
        parse_response(self.cfg.kind, &value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_are_consistent() {
        for k in [
            AiProviderKind::Ollama,
            AiProviderKind::OpenAiCompatible,
            AiProviderKind::Anthropic,
        ] {
            assert!(!k.default_endpoint().is_empty());
            assert!(!k.default_model().is_empty());
        }
        assert!(!AiProviderKind::Ollama.requires_key());
        assert!(AiProviderKind::OpenAiCompatible.requires_key());
        assert!(AiProviderKind::Anthropic.requires_key());
    }

    #[test]
    fn provider_kind_serde_is_stable() {
        assert_eq!(
            serde_json::to_string(&AiProviderKind::OpenAiCompatible).unwrap(),
            "\"openai\""
        );
        let k: AiProviderKind = serde_json::from_str("\"anthropic\"").unwrap();
        assert_eq!(k, AiProviderKind::Anthropic);
    }

    #[test]
    fn base_url_falls_back_when_blank() {
        let mut cfg = AiConfig {
            kind: AiProviderKind::Ollama,
            endpoint: String::new(),
            ..AiConfig::default()
        };
        assert_eq!(cfg.base_url(), "http://127.0.0.1:11434");
        cfg.endpoint = "http://box:11434/".into();
        assert_eq!(cfg.base_url(), "http://box:11434");
    }

    #[test]
    fn ollama_body_keeps_system_as_message() {
        let cfg = AiConfig {
            kind: AiProviderKind::Ollama,
            model: "llama3.2".into(),
            ..AiConfig::default()
        };
        let msgs = vec![AiMessage::system("be nice"), AiMessage::user("hi")];
        let body = build_request_body(&cfg, &msgs);
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn anthropic_body_lifts_system_out_of_messages() {
        let cfg = AiConfig {
            kind: AiProviderKind::Anthropic,
            model: "claude-3-5-haiku-latest".into(),
            max_tokens: 256,
            ..AiConfig::default()
        };
        let msgs = vec![
            AiMessage::system("persona one"),
            AiMessage::system("persona two"),
            AiMessage::user("hello"),
        ];
        let body = build_request_body(&cfg, &msgs);
        assert_eq!(body["system"], "persona one\n\npersona two");
        assert_eq!(body["max_tokens"], 256);
        let turns = body["messages"].as_array().unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0]["role"], "user");
    }

    #[test]
    fn openai_body_sets_max_tokens() {
        let cfg = AiConfig {
            kind: AiProviderKind::OpenAiCompatible,
            model: "gpt-4o-mini".into(),
            max_tokens: 500,
            ..AiConfig::default()
        };
        let body = build_request_body(&cfg, &[AiMessage::user("hi")]);
        assert_eq!(body["max_tokens"], 500);
        assert_eq!(body["model"], "gpt-4o-mini");
    }

    #[test]
    fn parse_ollama_response() {
        let v = serde_json::json!({"message": {"role": "assistant", "content": "hello there"}});
        assert_eq!(
            parse_response(AiProviderKind::Ollama, &v).unwrap(),
            "hello there"
        );
    }

    #[test]
    fn parse_openai_response() {
        let v = serde_json::json!({"choices": [{"message": {"content": "hi!"}}]});
        assert_eq!(
            parse_response(AiProviderKind::OpenAiCompatible, &v).unwrap(),
            "hi!"
        );
    }

    #[test]
    fn parse_anthropic_concatenates_text_blocks() {
        let v = serde_json::json!({"content": [
            {"type": "text", "text": "part one "},
            {"type": "thinking", "text": "ignored"},
            {"type": "text", "text": "part two"}
        ]});
        assert_eq!(
            parse_response(AiProviderKind::Anthropic, &v).unwrap(),
            "part one part two"
        );
    }

    #[test]
    fn parse_empty_response_is_error() {
        let v = serde_json::json!({"choices": []});
        assert!(parse_response(AiProviderKind::OpenAiCompatible, &v).is_err());
    }

    #[test]
    fn hosted_provider_without_key_fails_to_build() {
        let cfg = AiConfig {
            kind: AiProviderKind::Anthropic,
            api_key_env: "MIRA_TEST_DEFINITELY_UNSET_KEY".into(),
            ..AiConfig::default()
        };
        std::env::remove_var("MIRA_TEST_DEFINITELY_UNSET_KEY");
        assert!(AiClient::new(cfg).is_err());
    }

    #[test]
    fn local_provider_builds_without_key() {
        let cfg = AiConfig {
            kind: AiProviderKind::Ollama,
            ..AiConfig::default()
        };
        assert!(AiClient::new(cfg).is_ok());
    }

    #[test]
    fn conversation_tracks_turns() {
        let mut c = AiConversation::default();
        c.push_user("what's the weather?");
        c.pending = true;
        c.push_assistant("Sunny and 22°C.");
        c.pending = false;
        assert_eq!(c.messages.len(), 2);
        assert_eq!(c.last_assistant().unwrap().content, "Sunny and 22°C.");
        c.reset();
        assert!(c.messages.is_empty());
    }
}
