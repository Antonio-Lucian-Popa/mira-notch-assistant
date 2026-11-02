use mira_core::ai::{self, AiClient, AiConversation};
use mira_core::gowa::{self, GowaClient, GowaManager};
use mira_core::store::Config;
use mira_core::workspace::{self, Workspace};
use serde_json::json;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct WorkspaceIface {
    state: Arc<Mutex<Workspace>>,
    generation: Arc<AtomicU64>,
    receiver: Mutex<Option<mira_core::bridge::Receiver>>,
    /// Multi-turn history for the AI assistant page (in memory only).
    assistant: Mutex<AiConversation>,
    /// The GOWA gateway child process, when Mira is managing it.
    gowa: Mutex<Option<std::process::Child>>,
}

/// Load `config.toml` fresh so edits (provider, endpoint, enable flags) are
/// picked up without restarting the daemon. Falls back to defaults on error.
fn load_config() -> Config {
    mira_core::store::config_path()
        .and_then(|p| Config::load_or_default(&p))
        .unwrap_or_default()
}

/// Port the gateway listens on, parsed from the configured URL (default 3000).
fn gateway_port(cfg: &Config) -> u16 {
    cfg.messaging_gateway_url
        .rsplit(':')
        .next()
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000)
}

/// JSON snapshot of the assistant conversation plus provider readiness.
fn assistant_json(conv: &AiConversation, cfg: &Config, error: Option<String>) -> String {
    let ai_cfg = cfg.ai_config();
    let needs_key = ai_cfg.kind.requires_key();
    let ready = cfg.ai_enabled && (!needs_key || ai::resolve_key(&ai_cfg).is_some());
    json!({
        "enabled": cfg.ai_enabled,
        "provider": ai_cfg.kind.label(),
        "model": ai_cfg.model,
        "endpoint": ai_cfg.base_url(),
        "needs_key": needs_key,
        "key_env": ai_cfg.api_key_env,
        "ready": ready,
        "pending": conv.pending,
        "messages": conv.messages,
        "error": error,
    })
    .to_string()
}
impl WorkspaceIface {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Workspace::new())),
            ..Self::default()
        }
    }
}
#[zbus::interface(name = "ro.novabytecode.Mira.Workspace")]
impl WorkspaceIface {
    async fn phone_start(&self) -> zbus::fdo::Result<String> {
        let mut r = self.receiver.lock().await;
        if r.is_none() {
            let (config, dir) =
                mira_core::bridge::paths().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            *r = Some(
                mira_core::bridge::Receiver::start(config, dir, "0.0.0.0:45271")
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
            );
        }
        Ok(r.as_ref().unwrap().snapshot().to_string())
    }
    async fn phone_stop(&self) {
        self.receiver.lock().await.take();
    }
    async fn phone_revoke(&self) -> zbus::fdo::Result<()> {
        self.receiver.lock().await.take();
        let (config, _) =
            mira_core::bridge::paths().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if config.exists() {
            mira_core::bridge::revoke_pairing(&config)
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }
        Ok(())
    }
    async fn phone_state(&self) -> String {
        self.receiver
            .lock()
            .await
            .as_ref()
            .map(|r| r.snapshot().to_string())
            .unwrap_or("{\"running\":false}".into())
    }
    async fn phone_actions(&self) -> String {
        self.receiver
            .lock()
            .await
            .as_ref()
            .map(|r| serde_json::to_string(&r.drain()).unwrap_or_default())
            .unwrap_or("[]".into())
    }
    async fn phone_add_action(&self, name: String, keys: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        r.as_ref()
            .ok_or_else(|| zbus::fdo::Error::Failed("Pornește primirea mai întâi.".into()))?
            .add_action(&name, &keys)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn phone_set_shake(&self, first: String, second: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        r.as_ref()
            .ok_or_else(|| zbus::fdo::Error::Failed("Pornește primirea mai întâi.".into()))?
            .set_shake(&first, &second)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn phone_remove_action(&self, id: String) -> zbus::fdo::Result<()> {
        let r = self.receiver.lock().await;
        if let Some(r) = r.as_ref() {
            r.remove_action(&id)
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        }
        Ok(())
    }
    async fn snapshot(&self) -> String {
        serde_json::to_string(&*self.state.lock().await).unwrap_or_default()
    }
    async fn add_paths(&self, paths: Vec<String>) -> zbus::fdo::Result<String> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut w = state.blocking_lock();
            w.add_paths(&paths);
            serde_json::to_string(&*w).unwrap_or_default()
        })
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn note(&self, text: String) {
        self.state.lock().await.set_note(&text);
    }
    async fn capture(&self, enabled: bool) {
        self.state.lock().await.capture = enabled;
    }
    async fn push_clip(&self, text: String) {
        self.state.lock().await.push_clip(&text);
    }
    async fn clear(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.state.lock().await.clear();
    }
    async fn remove(&self, path: String) {
        self.state.lock().await.links.retain(|l| l.path != path);
        self.state.lock().await.roots.retain(|r| r != &path);
    }
    async fn cancel_search(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    async fn search(&self, query: String) -> zbus::fdo::Result<String> {
        let roots = self.state.lock().await.roots.clone();
        let generation = self.generation.clone();
        let request = generation.fetch_add(1, Ordering::Relaxed) + 1;
        tokio::task::spawn_blocking(move || {
            serde_json::to_string(&workspace::search(&roots, &query, &generation, request))
                .unwrap_or_default()
        })
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    // --- messaging via the GOWA WhatsApp gateway (opt-in, unofficial) ---

    /// Download, verify and install the GOWA gateway binary for this platform.
    async fn messaging_install(&self) -> String {
        let cfg = load_config();
        let version = cfg.messaging_gowa_version.trim();
        let version = (!version.is_empty()).then(|| version.to_string());
        let mgr = match GowaManager::new() {
            Ok(m) => m,
            Err(e) => return json!({"ok": false, "error": e.to_string()}).to_string(),
        };
        match mgr.install(version.as_deref()).await {
            Ok(rep) => {
                json!({"ok": true, "version": rep.version, "path": rep.path, "bytes": rep.bytes})
                    .to_string()
            }
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }

    /// Start the managed gateway (loopback, UI off, generated basic auth).
    async fn messaging_start(&self) -> String {
        let cfg = load_config();
        let mgr = match GowaManager::new() {
            Ok(m) => m,
            Err(e) => return json!({"ok": false, "error": e.to_string()}).to_string(),
        };
        if !mgr.is_installed() {
            return json!({"ok": false, "error": "gateway not installed"}).to_string();
        }
        let mut guard = self.gowa.lock().await;
        let alive = guard
            .as_mut()
            .map(|c| matches!(c.try_wait(), Ok(None)))
            .unwrap_or(false);
        if !alive {
            *guard = None;
            let creds = match mgr.credentials() {
                Ok(c) => c,
                Err(e) => return json!({"ok": false, "error": e.to_string()}).to_string(),
            };
            match mgr.start(gateway_port(&cfg), &creds) {
                Ok(child) => *guard = Some(child),
                Err(e) => return json!({"ok": false, "error": e.to_string()}).to_string(),
            }
        }
        json!({"ok": true, "running": true}).to_string()
    }

    /// Stop the managed gateway.
    async fn messaging_stop(&self) -> String {
        let mut guard = self.gowa.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        json!({"ok": true, "running": false}).to_string()
    }

    /// Gateway status: install/run state plus the WhatsApp link state, inlining
    /// the login QR (as a data URI) while waiting to be scanned.
    async fn messaging_status(&self) -> String {
        let cfg = load_config();
        if !cfg.messaging_enabled {
            return json!({"enabled": false}).to_string();
        }
        let installed = GowaManager::new()
            .map(|m| m.is_installed())
            .unwrap_or(false);
        let running = {
            let mut g = self.gowa.lock().await;
            g.as_mut()
                .map(|c| matches!(c.try_wait(), Ok(None)))
                .unwrap_or(false)
        };
        let mut out = json!({
            "enabled": true,
            "auto_manage": cfg.messaging_auto_manage,
            "installed": installed,
            "running": running,
            "provider": cfg.messaging_provider.label(),
            "gateway": cfg.messaging_gateway_url,
        });
        match GowaClient::from_config(&cfg) {
            Ok(client) => match client.provider_status().await {
                Ok(status) => {
                    if let mira_core::messaging::ProviderStatus::WaitingForScan { qr } = &status {
                        if let Ok(png) = client.fetch_qr_png(qr).await {
                            out["qr_png"] = json!(gowa::png_data_uri(&png));
                        }
                    }
                    out["status"] =
                        serde_json::to_value(&status).unwrap_or(json!({"state": "disconnected"}));
                }
                Err(e) => {
                    out["status"] =
                        json!({"state": if running { "connecting" } else { "disconnected" }});
                    out["error"] = json!(e.to_string());
                }
            },
            Err(e) => out["error"] = json!(e.to_string()),
        }
        out.to_string()
    }

    /// One-line-per-chat inbox snapshot, most recent first.
    async fn inbox(&self) -> String {
        let cfg = load_config();
        if !cfg.messaging_enabled {
            return json!({"enabled": false, "chats": []}).to_string();
        }
        let client = match GowaClient::from_config(&cfg) {
            Ok(c) => c,
            Err(e) => {
                return json!({"enabled": true, "chats": [], "error": e.to_string()}).to_string()
            }
        };
        match client.inbox(25).await {
            Ok(inbox) => json!({
                "enabled": true,
                "provider": inbox.provider.label(),
                "status": inbox.status,
                "total_unread": inbox.total_unread(),
                "chats": inbox.one_line_per_chat(),
            })
            .to_string(),
            Err(e) => json!({"enabled": true, "chats": [], "error": e.to_string()}).to_string(),
        }
    }

    /// Full message list for a single conversation (chat JID).
    async fn thread(&self, conversation_id: String) -> String {
        let cfg = load_config();
        if !cfg.messaging_enabled {
            return json!({"enabled": false, "messages": []}).to_string();
        }
        let client = match GowaClient::from_config(&cfg) {
            Ok(c) => c,
            Err(e) => return json!({"messages": [], "error": e.to_string()}).to_string(),
        };
        match client.messages(&conversation_id, 50).await {
            Ok(messages) => json!({"id": conversation_id, "messages": messages}).to_string(),
            Err(e) => json!({"messages": [], "error": e.to_string()}).to_string(),
        }
    }

    /// Send a reply into a conversation (chat JID) through the gateway.
    async fn send_reply(&self, conversation_id: String, text: String) -> String {
        let cfg = load_config();
        if !cfg.messaging_enabled {
            return json!({"ok": false, "error": "messaging is disabled"}).to_string();
        }
        if text.trim().is_empty() {
            return json!({"ok": false, "error": "empty message"}).to_string();
        }
        let client = match GowaClient::from_config(&cfg) {
            Ok(c) => c,
            Err(e) => return json!({"ok": false, "error": e.to_string()}).to_string(),
        };
        match client.send(&conversation_id, &text).await {
            Ok(()) => json!({"ok": true}).to_string(),
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }

    // --- AI assistant (opt-in; local Ollama or the user's own API key) ---

    /// Current assistant conversation + provider readiness.
    async fn assistant_state(&self) -> String {
        let cfg = load_config();
        let conv = self.assistant.lock().await;
        assistant_json(&conv, &cfg, None)
    }

    /// Ask the assistant a question; returns the updated conversation.
    async fn ask(&self, prompt: String) -> String {
        let cfg = load_config();
        if !cfg.ai_enabled {
            let conv = self.assistant.lock().await;
            return assistant_json(&conv, &cfg, Some("the assistant is disabled".into()));
        }
        if prompt.trim().is_empty() {
            let conv = self.assistant.lock().await;
            return assistant_json(&conv, &cfg, None);
        }

        // Record the user turn and mark pending, then release the lock so a
        // concurrent status poll can show the typing indicator while we wait.
        let history = {
            let mut conv = self.assistant.lock().await;
            conv.push_user(prompt);
            conv.pending = true;
            conv.messages.clone()
        };

        let client = match AiClient::new(cfg.ai_config()) {
            Ok(c) => c,
            Err(e) => {
                let mut conv = self.assistant.lock().await;
                conv.pending = false;
                return assistant_json(&conv, &cfg, Some(e.to_string()));
            }
        };

        let result = client.complete(&history).await;
        let mut conv = self.assistant.lock().await;
        conv.pending = false;
        match result {
            Ok(reply) => {
                conv.push_assistant(reply);
                assistant_json(&conv, &cfg, None)
            }
            Err(e) => assistant_json(&conv, &cfg, Some(e.to_string())),
        }
    }

    /// Clear the assistant conversation.
    async fn assistant_reset(&self) -> String {
        let cfg = load_config();
        let mut conv = self.assistant.lock().await;
        conv.reset();
        assistant_json(&conv, &cfg, None)
    }
}
