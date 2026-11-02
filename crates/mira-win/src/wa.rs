//! Background WhatsApp (GOWA) worker for the Windows host.
//!
//! The native UI runs on a single GDI thread and must never block on the
//! network, so this module owns a dedicated OS thread with a current-thread
//! Tokio runtime. The UI talks to it through non-blocking commands and reads an
//! `Arc<Mutex<Snapshot>>` each frame (the same shared-state pattern the clipboard
//! search and phone bridge already use). The worker installs/runs the gateway,
//! polls status + inbox, decodes the login QR to pixels, and sends replies.
//!
//! Pure Rust (Tokio + `mira_core`): it carries no Win32 types, so it builds on
//! every platform even though only the Windows host instantiates it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mira_core::gowa::{decode_png_to_bgra, DecodedImage, GowaClient, GowaManager};
use mira_core::messaging::ProviderStatus;
use mira_core::store::Config;

/// One row in the inbox list.
#[derive(Clone, Default)]
pub struct ChatLine {
    pub jid: String,
    pub title: String,
    pub preview: String,
    pub unread: u32,
}

/// One bubble in an open thread.
#[derive(Clone, Default)]
pub struct ThreadLine {
    pub from_me: bool,
    pub text: String,
}

/// Everything the UI needs to paint the messages page. Cheap to clone: the QR
/// pixels are shared behind an `Arc`.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub enabled: bool,
    /// `connected` | `connecting` | `waiting_for_scan` | `disconnected`.
    pub state: String,
    /// Short human line under the header.
    pub status_text: String,
    /// A transient action note ("Se instalează…"), cleared when it finishes.
    pub busy: String,
    pub qr: Option<Arc<DecodedImage>>,
    pub chats: Vec<ChatLine>,
    pub thread_open: bool,
    pub thread_title: String,
    pub thread_msgs: Vec<ThreadLine>,
}

enum Cmd {
    Install,
    Start,
    Stop,
    Open(String, String),
    Close,
    Send(String, String),
    Tick,
}

/// Handle the UI holds; cloning is not needed (the host owns one).
pub struct Worker {
    snap: Arc<Mutex<Snapshot>>,
    tx: Sender<Cmd>,
    active: Arc<AtomicBool>,
}

impl Worker {
    pub fn new() -> Self {
        let snap = Arc::new(Mutex::new(Snapshot::default()));
        let active = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel::<Cmd>();
        {
            let snap = snap.clone();
            let active = active.clone();
            std::thread::Builder::new()
                .name("mira-wa".into())
                .spawn(move || run(snap, active, rx))
                .ok();
        }
        Worker { snap, tx, active }
    }

    /// Snapshot for painting (cheap clone).
    pub fn snapshot(&self) -> Snapshot {
        self.snap.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Mark whether the messages page is on screen; the worker only polls when
    /// active, to stay quiet in the background.
    pub fn set_active(&self, on: bool) {
        let was = self.active.swap(on, Ordering::Relaxed);
        if on && !was {
            let _ = self.tx.send(Cmd::Tick);
        }
    }

    pub fn install(&self) {
        let _ = self.tx.send(Cmd::Install);
    }
    pub fn start(&self) {
        let _ = self.tx.send(Cmd::Start);
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn open(&self, jid: String, title: String) {
        let _ = self.tx.send(Cmd::Open(jid, title));
    }
    pub fn close_thread(&self) {
        let _ = self.tx.send(Cmd::Close);
    }
    pub fn send(&self, jid: String, text: String) {
        let _ = self.tx.send(Cmd::Send(jid, text));
    }
}

impl Default for Worker {
    fn default() -> Self {
        Worker::new()
    }
}

fn set_busy(snap: &Arc<Mutex<Snapshot>>, note: &str) {
    if let Ok(mut s) = snap.lock() {
        s.busy = note.to_string();
    }
}

fn run(snap: Arc<Mutex<Snapshot>>, active: Arc<AtomicBool>, rx: Receiver<Cmd>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };
    let mgr = GowaManager::new().ok();
    let mut child: Option<std::process::Child> = None;
    let mut open_jid: Option<(String, String)> = None;

    loop {
        let cmd = rx.recv_timeout(Duration::from_secs(3));
        match cmd {
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if !active.load(Ordering::Relaxed) {
                    continue;
                }
            }
            Ok(Cmd::Tick) => {}
            Ok(Cmd::Install) => {
                set_busy(&snap, "Se descarcă și se verifică gateway-ul…");
                if let Some(m) = &mgr {
                    let cfg = load_config();
                    let version = cfg.messaging_gowa_version.trim().to_string();
                    let version = (!version.is_empty()).then_some(version);
                    match rt.block_on(m.install(version.as_deref())) {
                        Ok(_) => set_busy(&snap, "Gateway instalat."),
                        Err(e) => set_busy(&snap, &format!("Instalare eșuată: {e}")),
                    }
                }
            }
            Ok(Cmd::Start) => {
                if let Some(m) = &mgr {
                    let alive = child
                        .as_mut()
                        .map(|c| matches!(c.try_wait(), Ok(None)))
                        .unwrap_or(false);
                    if !alive {
                        child = None;
                        let cfg = load_config();
                        if let Ok(creds) = m.credentials() {
                            match m.start(gateway_port(&cfg), &creds) {
                                Ok(c) => {
                                    child = Some(c);
                                    set_busy(&snap, "Gateway pornit.");
                                }
                                Err(e) => set_busy(&snap, &format!("Pornire eșuată: {e}")),
                            }
                        }
                    }
                }
            }
            Ok(Cmd::Stop) => {
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                set_busy(&snap, "Gateway oprit.");
            }
            Ok(Cmd::Open(jid, title)) => open_jid = Some((jid, title)),
            Ok(Cmd::Close) => open_jid = None,
            Ok(Cmd::Send(jid, text)) => {
                let cfg = load_config();
                if let Ok(client) = GowaClient::from_config(&cfg) {
                    let _ = rt.block_on(client.send(&jid, &text));
                }
            }
        }

        refresh(&rt, &snap, &mgr, &mut child, &open_jid);
    }

    if let Some(mut c) = child.take() {
        let _ = c.kill();
    }
}

/// Rebuild the snapshot from the gateway (status, QR, inbox, open thread).
fn refresh(
    rt: &tokio::runtime::Runtime,
    snap: &Arc<Mutex<Snapshot>>,
    mgr: &Option<GowaManager>,
    child: &mut Option<std::process::Child>,
    open_jid: &Option<(String, String)>,
) {
    let cfg = load_config();
    let installed = mgr.as_ref().map(|m| m.is_installed()).unwrap_or(false);
    let running = child
        .as_mut()
        .map(|c| matches!(c.try_wait(), Ok(None)))
        .unwrap_or(false);

    let mut next = Snapshot {
        enabled: cfg.messaging_enabled,
        thread_open: open_jid.is_some(),
        thread_title: open_jid
            .as_ref()
            .map(|(_, t)| t.clone())
            .unwrap_or_default(),
        busy: snap.lock().map(|s| s.busy.clone()).unwrap_or_default(),
        ..Snapshot::default()
    };

    if !cfg.messaging_enabled {
        next.status_text = "Activează mesageria în config.toml: messaging_enabled = true.".into();
        store(snap, next);
        return;
    }

    let client = match GowaClient::from_config(&cfg) {
        Ok(c) => c,
        Err(e) => {
            next.status_text = format!("Config gateway invalid: {e}");
            store(snap, next);
            return;
        }
    };

    match rt.block_on(client.provider_status()) {
        Ok(status) => {
            next.state = state_name(&status).into();
            match &status {
                ProviderStatus::WaitingForScan { qr } => {
                    next.status_text =
                        "Scanează codul cu telefonul: WhatsApp › Dispozitive conectate.".into();
                    if let Ok(png) = rt.block_on(client.fetch_qr_png(qr)) {
                        if let Ok(img) = decode_png_to_bgra(&png) {
                            next.qr = Some(Arc::new(img));
                        }
                    }
                }
                ProviderStatus::Connected => {
                    next.status_text = "Conectat.".into();
                    if let Some((jid, _)) = open_jid {
                        if let Ok(msgs) = rt.block_on(client.messages(jid, 40)) {
                            next.thread_msgs = msgs
                                .into_iter()
                                .map(|m| ThreadLine {
                                    from_me: m.direction.is_outgoing(),
                                    text: if m.body.trim().is_empty() {
                                        m.kind.placeholder().to_string()
                                    } else {
                                        m.body
                                    },
                                })
                                .collect();
                        }
                    } else if let Ok(inbox) = rt.block_on(client.inbox(25)) {
                        next.chats = inbox
                            .one_line_per_chat()
                            .into_iter()
                            .map(|l| ChatLine {
                                jid: l.id,
                                title: l.title,
                                preview: if l.outgoing_last {
                                    format!("Tu: {}", l.preview)
                                } else {
                                    l.preview
                                },
                                unread: l.unread,
                            })
                            .collect();
                    }
                }
                ProviderStatus::Connecting => {
                    next.status_text = "Se conectează la WhatsApp…".into()
                }
                ProviderStatus::Disconnected => {
                    next.status_text = if installed {
                        "Gateway oprit. Apasă 'Pornește'.".into()
                    } else {
                        "Apasă 'Instalează' ca Mira să descarce gateway-ul.".into()
                    }
                }
            }
        }
        Err(e) => {
            next.state = if running {
                "connecting"
            } else {
                "disconnected"
            }
            .into();
            next.status_text = if installed && !running {
                "Gateway instalat. Apasă 'Pornește'.".into()
            } else if !installed {
                "Apasă 'Instalează' ca Mira să descarce gateway-ul.".into()
            } else {
                format!("Gateway indisponibil: {e}")
            };
        }
    }

    store(snap, next);
}

fn store(snap: &Arc<Mutex<Snapshot>>, next: Snapshot) {
    if let Ok(mut s) = snap.lock() {
        *s = next;
    }
}

fn state_name(s: &ProviderStatus) -> &'static str {
    match s {
        ProviderStatus::Connected => "connected",
        ProviderStatus::Connecting => "connecting",
        ProviderStatus::WaitingForScan { .. } => "waiting_for_scan",
        ProviderStatus::Disconnected => "disconnected",
    }
}

fn load_config() -> Config {
    mira_core::store::config_path()
        .and_then(|p| Config::load_or_default(&p))
        .unwrap_or_default()
}

fn gateway_port(cfg: &Config) -> u16 {
    cfg.messaging_gateway_url
        .rsplit(':')
        .next()
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000)
}
