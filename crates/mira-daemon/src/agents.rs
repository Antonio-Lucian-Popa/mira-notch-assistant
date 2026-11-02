//! Agent monitoring: a local Unix-socket server that `mira-hook` connects to,
//! an in-memory registry of live sessions, and the `ro.novabytecode.Mira.Agents`
//! D-Bus interface the GNOME extension reads and responds through.
//!
//! A `PreToolUse` connection is held open while the user decides in the notch;
//! the decision is delivered over a oneshot and written back to the hook. Any
//! other event is recorded and the connection closes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{oneshot, Mutex};

use mira_core::agent::{self, AgentRegistry, Decision, HookEvent, PreOutcome};

/// Shared state: the session registry plus the open approvals awaiting a click.
pub struct AgentServer {
    registry: AgentRegistry,
    pending: HashMap<String, oneshot::Sender<Decision>>,
}

impl AgentServer {
    pub fn new() -> Self {
        AgentServer {
            registry: AgentRegistry::new(),
            pending: HashMap::new(),
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn decision_str(d: Decision) -> &'static str {
    match d {
        Decision::Allow => "allow",
        Decision::Deny => "deny",
        Decision::Ask => "ask",
        Decision::Pass => "pass",
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Bind the agent socket and serve hook connections forever.
pub async fn serve(server: Arc<Mutex<AgentServer>>) -> std::io::Result<()> {
    let path = agent::socket_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(&path); // clear a stale socket
    let listener = UnixListener::bind(&path)?;
    tracing::info!("agent socket at {}", path.display());
    loop {
        let (stream, _) = listener.accept().await?;
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, server).await {
                tracing::debug!("agent hook connection ended: {e}");
            }
        });
    }
}

async fn handle(stream: UnixStream, server: Arc<Mutex<AgentServer>>) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(());
    }
    let payload: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let event = agent::parse_hook(&payload);

    if let HookEvent::PreToolUse {
        session_id,
        cwd,
        agent: ag,
        tool,
        input,
    } = &event
    {
        let outcome = {
            let mut s = server.lock().await;
            s.registry
                .begin_pretooluse(session_id, cwd, ag, tool, input, now(), || {
                    format!("ap{}", COUNTER.fetch_add(1, Ordering::Relaxed))
                })
        };
        let decision = match outcome {
            PreOutcome::Immediate(d) => d,
            PreOutcome::Prompt(approval) => {
                let (tx, rx) = oneshot::channel();
                server.lock().await.pending.insert(approval.id.clone(), tx);
                // Wait for the user's click (or a dropped sender → fall back).
                rx.await.unwrap_or(Decision::Ask)
            }
        };
        let resp = json!({ "decision": decision_str(decision) }).to_string();
        write.write_all(resp.as_bytes()).await?;
        write.write_all(b"\n").await?;
        write.flush().await?;
    } else {
        server.lock().await.registry.apply(&event, now());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// D-Bus interface
// ---------------------------------------------------------------------------

pub struct AgentsIface {
    server: Arc<Mutex<AgentServer>>,
}

impl AgentsIface {
    pub fn new(server: Arc<Mutex<AgentServer>>) -> Self {
        AgentsIface { server }
    }
}

#[zbus::interface(name = "ro.novabytecode.Mira.Agents")]
impl AgentsIface {
    /// JSON snapshot of all sessions (and how many wait for approval).
    async fn state(&self) -> String {
        self.server
            .lock()
            .await
            .registry
            .snapshot_json()
            .to_string()
    }

    /// Answer a pending approval. `decision` is "allow", "deny" or "ask";
    /// `always` records an allow-rule for that session+tool.
    async fn respond(&self, id: String, decision: String, always: bool) -> String {
        let choice = match decision.as_str() {
            "allow" => Decision::Allow,
            "deny" => Decision::Deny,
            _ => Decision::Ask,
        };
        let (dec, tx) = {
            let mut s = self.server.lock().await;
            let d = s.registry.resolve(&id, choice, always);
            let t = s.pending.remove(&id);
            (d, t)
        };
        if let (Some(dec), Some(tx)) = (dec, tx) {
            let _ = tx.send(dec);
        }
        self.server
            .lock()
            .await
            .registry
            .snapshot_json()
            .to_string()
    }

    /// Install Mira's hooks into `~/.claude/settings.json` (with a backup).
    async fn install_hooks(&self) -> String {
        install_hooks().unwrap_or_else(|e| json!({"ok": false, "error": e}).to_string())
    }

    /// Whether the hooks are installed, plus the resolved paths.
    async fn hooks_status(&self) -> String {
        let hook = hook_path();
        let settings = settings_path();
        let installed = std::fs::read_to_string(&settings)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| agent::hooks_installed(&v, &hook))
            .unwrap_or(false);
        json!({
            "installed": installed,
            "hook": hook,
            "settings": settings,
            "hook_present": std::path::Path::new(&hook).exists(),
        })
        .to_string()
    }
}

/// Resolve the `mira-hook` binary next to the running daemon.
fn hook_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("mira-hook")))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mira-hook".into())
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn settings_path() -> String {
    format!("{}/.claude/settings.json", home())
}

fn install_hooks() -> Result<String, String> {
    let hook = hook_path();
    if !std::path::Path::new(&hook).exists() {
        return Err(format!("mira-hook not found at {hook}"));
    }
    let settings = settings_path();
    let existing: Value = match std::fs::read_to_string(&settings) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s).map_err(|e| format!("settings.json is not valid JSON: {e}"))?
        }
        _ => json!({}),
    };
    // Back up before touching anything.
    let mut backup = String::new();
    if std::path::Path::new(&settings).exists() {
        backup = format!("{settings}.mira-backup-{}", now());
        std::fs::copy(&settings, &backup).map_err(|e| e.to_string())?;
    }
    let merged = agent::merge_hooks(&existing, &hook, 590);
    if let Some(dir) = std::path::Path::new(&settings).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let pretty = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    std::fs::write(&settings, pretty).map_err(|e| e.to_string())?;
    Ok(json!({"ok": true, "settings": settings, "backup": backup, "hook": hook}).to_string())
}
