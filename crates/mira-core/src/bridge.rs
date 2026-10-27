//! Opt-in LAN receiver. Certificate pinning + bearer pairing; no listener at startup.
//! Remote clients can upload immutable transcripts or trigger locally registered keys.
use crate::{Error, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub keys: Vec<String>,
    pub lock: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct RemoteAction {
    #[serde(skip)]
    queued: std::time::Instant,
    pub issued_at: u64,
    pub request_id: String,
    pub action: Action,
}
#[derive(Serialize, Deserialize)]
struct Identity {
    cert: Vec<u8>,
    key: Vec<u8>,
    token: String,
    actions: Vec<Action>,
}
#[derive(Serialize)]
pub struct Pairing {
    pub code: String,
    pub address: String,
    pub fingerprint: String,
    pub qr: Vec<Vec<bool>>,
    pub actions: Vec<Action>,
    pub directory: String,
}
pub struct Receiver {
    pub pairing: Pairing,
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Identity>>,
    jobs: Arc<Mutex<VecDeque<RemoteAction>>>,
    config: PathBuf,
    accept_thread: Option<std::thread::JoinHandle<()>>,
}
impl Receiver {
    pub fn start(config: PathBuf, directory: PathBuf, bind: &str) -> Result<Self> {
        if let Some(parent) = config.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&directory)?;
        let identity: Identity = if config.exists() {
            serde_json::from_slice(&fs::read(&config)?)?
        } else {
            let generated = rcgen::generate_simple_self_signed(vec!["mira.local".into()])
                .map_err(|e| Error::other(e.to_string()))?;
            let mut token = [0u8; 32];
            getrandom::getrandom(&mut token).map_err(|e| Error::other(e.to_string()))?;
            let identity = Identity {
                cert: generated.cert.der().to_vec(),
                key: generated.signing_key.serialize_der(),
                token: hex(&token),
                actions: vec![Action {
                    id: "lock".into(),
                    name: "Blochează PC-ul".into(),
                    keys: vec![],
                    lock: true,
                }],
            };
            save_private(&config, &serde_json::to_vec(&identity)?)?;
            identity
        };
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::other(e.to_string()))?
        .with_no_client_auth()
        .with_single_cert(
            vec![identity.cert.clone().into()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(identity.key.clone()).into(),
        )
        .map_err(|e| Error::other(e.to_string()))?;
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let ip = local_ip();
        let address = format!("{ip}:{port}");
        let fingerprint = hex(&Sha256::digest(&identity.cert));
        let code = format!(
            "mira-pair://{address}?fp={fingerprint}&token={}",
            identity.token
        );
        let qr = qrcodegen::QrCode::encode_text(&code, qrcodegen::QrCodeEcc::Medium)
            .map_err(|e| Error::other(e.to_string()))?;
        let matrix = (0..qr.size())
            .map(|y| (0..qr.size()).map(|x| qr.get_module(x, y)).collect())
            .collect();
        let pairing = Pairing {
            code,
            address,
            fingerprint,
            qr: matrix,
            actions: identity.actions.clone(),
            directory: directory.to_string_lossy().into_owned(),
        };
        let shared = Arc::new(Mutex::new(identity));
        let stop = Arc::new(AtomicBool::new(false));
        let jobs = Arc::new(Mutex::new(VecDeque::new()));
        let state = shared.clone();
        let stopped = stop.clone();
        let queue = jobs.clone();
        let config_tls = Arc::new(tls);
        let connections = Arc::new(AtomicUsize::new(0));
        let receipts = Arc::new(Mutex::new(HashSet::<String>::new()));
        let io_lock = Arc::new(Mutex::new(()));
        let accept_thread = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, peer)) => {
                        if !peer.ip().is_loopback()
                            && !matches!(peer.ip(),std::net::IpAddr::V4(ip) if ip.is_private()||ip.is_link_local())
                        {
                            continue;
                        }
                        if connections.fetch_add(1, Ordering::Relaxed) >= 8 {
                            connections.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let cfg = config_tls.clone();
                        let state = state.clone();
                        let stop = stopped.clone();
                        let jobs = queue.clone();
                        let dir = directory.clone();
                        let active = connections.clone();
                        let seen = receipts.clone();
                        let serial = io_lock.clone();
                        thread::spawn(move || {
                            let _ = serve(socket, cfg, &state, &stop, &jobs, &dir, &seen, &serial);
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(70))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            pairing,
            stop,
            shared,
            jobs,
            config,
            accept_thread: Some(accept_thread),
        })
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(&self.pairing).unwrap_or_default();
        value["actions"] = serde_json::to_value(
            &self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .actions,
        )
        .unwrap_or_default();
        value["running"] = true.into();
        value
    }
    pub fn add_action(&self, name: &str, chord: &str) -> Result<()> {
        let keys = validate_chord(chord)?;
        let name: String = name.trim().chars().take(60).collect();
        if name.is_empty() {
            return Err(Error::other("Dă un nume acțiunii."));
        }
        let mut state = self
            .shared
            .lock()
            .map_err(|_| Error::other("Receiver unavailable"))?;
        if state.actions.len() >= 25 {
            return Err(Error::other("Maxim 25 de acțiuni."));
        }
        let mut id = [0u8; 8];
        getrandom::getrandom(&mut id).map_err(|e| Error::other(e.to_string()))?;
        state.actions.push(Action {
            id: hex(&id),
            name,
            keys,
            lock: false,
        });
        save_private(&self.config, &serde_json::to_vec(&*state)?)
    }
    pub fn remove_action(&self, id: &str) -> Result<()> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| Error::other("Receiver unavailable"))?;
        state.actions.retain(|a| a.id != id);
        save_private(&self.config, &serde_json::to_vec(&*state)?)
    }
    pub fn drain(&self) -> Vec<RemoteAction> {
        self.jobs
            .lock()
            .map(|mut q| {
                q.drain(..)
                    .filter(|job| job.queued.elapsed() < Duration::from_secs(10))
                    .collect()
            })
            .unwrap_or_default()
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        if let Ok(mut q) = self.jobs.lock() {
            q.clear();
        }
    }
}
pub fn paths() -> Result<(PathBuf, PathBuf)> {
    let config = crate::store::db_path()?.with_file_name("phone-pairing.json");
    let directory = dirs::document_dir()
        .unwrap_or(dirs::home_dir().ok_or(Error::NoDataDir)?)
        .join("Mira")
        .join("Transcrieri");
    Ok((config, directory))
}
pub fn validate_chord(text: &str) -> Result<Vec<String>> {
    let keys: Vec<_> = text
        .split('+')
        .map(|s| s.trim().to_ascii_uppercase())
        .collect();
    let modifiers = ["CTRL", "ALT", "SHIFT", "SUPER"];
    let valid = |key: &str| {
        modifiers.contains(&key)
            || (key.len() == 1 && key.bytes().all(|b| b.is_ascii_alphanumeric()))
            || matches!(
                key,
                "ENTER"
                    | "TAB"
                    | "SPACE"
                    | "ESC"
                    | "DELETE"
                    | "BACKSPACE"
                    | "HOME"
                    | "END"
                    | "LEFT"
                    | "RIGHT"
                    | "UP"
                    | "DOWN"
                    | "PAGEUP"
                    | "PAGEDOWN"
                    | "PLUS"
                    | "MINUS"
            )
            || key
                .strip_prefix('F')
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|n| (1..=24).contains(&n))
    };
    if keys.is_empty()
        || keys.len() > 5
        || keys.iter().any(|s| !valid(s))
        || keys
            .iter()
            .filter(|s| !modifiers.contains(&s.as_str()))
            .count()
            != 1
        || keys.iter().collect::<HashSet<_>>().len() != keys.len()
    {
        return Err(Error::other(
            "Exemplu: CTRL+ALT+2. Un singur buton și modificatori CTRL, ALT, SHIFT, SUPER.",
        ));
    }
    let mut ordered: Vec<String> = modifiers
        .iter()
        .filter(|m| keys.iter().any(|k| k == **m))
        .map(|m| (*m).into())
        .collect();
    ordered.extend(
        keys.into_iter()
            .filter(|k| !modifiers.contains(&k.as_str())),
    );
    Ok(ordered)
}
fn save_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension("new");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}
fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or("127.0.0.1".into())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |v, (x, y)| v | (x ^ y)) == 0
}
#[derive(Deserialize)]
struct Upload {
    id: String,
    text: String,
    pdf: String,
    #[serde(default)]
    title: String,
}
#[derive(Deserialize)]
struct Trigger {
    request_id: String,
    action_id: String,
}
#[allow(clippy::too_many_arguments)]
fn serve(
    socket: TcpStream,
    tls: Arc<rustls::ServerConfig>,
    state: &Mutex<Identity>,
    stop: &AtomicBool,
    jobs: &Mutex<VecDeque<RemoteAction>>,
    directory: &Path,
    seen: &Mutex<HashSet<String>>,
    serial: &Mutex<()>,
) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let connection = rustls::ServerConnection::new(tls).map_err(|e| Error::other(e.to_string()))?;
    let mut stream = rustls::StreamOwned::new(connection, socket);
    let mut header = Vec::new();
    let mut one = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 8192 {
            return Ok(());
        }
        stream.read_exact(&mut one)?;
        header.push(one[0]);
    }
    let header = String::from_utf8_lossy(&header);
    let mut lines = header.split("\r\n");
    let request = lines.next().unwrap_or_default();
    let mut request = request.split_whitespace();
    let method = request.next().unwrap_or_default();
    let path = request.next().unwrap_or_default();
    let mut authorization = "";
    let mut length = None;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("authorization") {
                authorization = value.trim()
            }
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok()
            }
        }
    }
    let valid = state
        .lock()
        .map(|s| {
            constant_eq(
                authorization.as_bytes(),
                format!("Bearer {}", s.token).as_bytes(),
            )
        })
        .unwrap_or(false);
    if !valid || stop.load(Ordering::Relaxed) {
        return respond(&mut stream, 401, r#"{"error":"unauthorized"}"#);
    }
    if method == "GET" && path == "/v1/actions" {
        let data =
            serde_json::to_string(&state.lock().map_err(|_| Error::other("state"))?.actions)?;
        return respond(&mut stream, 200, &data);
    }
    if method != "POST" {
        return respond(&mut stream, 404, "{}");
    }
    let Some(length) = length.filter(|n| *n <= 12 * 1024 * 1024) else {
        return respond(&mut stream, 413, "{}");
    };
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    if stop.load(Ordering::Relaxed) {
        return respond(&mut stream, 503, "{}");
    }
    if path == "/v1/actions" {
        let trigger: Trigger = match serde_json::from_slice(&body) {
            Ok(t) => t,
            Err(_) => return respond(&mut stream, 400, "{}"),
        };
        if !safe_id(&trigger.request_id) {
            return respond(&mut stream, 400, "{}");
        }
        let mut seen = seen.lock().map_err(|_| Error::other("state"))?;
        if seen.contains(&trigger.request_id) {
            return respond(&mut stream, 202, r#"{"queued":true}"#);
        }
        let action = state
            .lock()
            .map_err(|_| Error::other("state"))?
            .actions
            .iter()
            .find(|a| a.id == trigger.action_id)
            .cloned();
        let Some(action) = action else {
            return respond(&mut stream, 404, "{}");
        };
        let mut queue = jobs.lock().map_err(|_| Error::other("state"))?;
        if queue.len() >= 20 {
            return respond(&mut stream, 429, "{}");
        }
        if seen.len() >= 4096 {
            return respond(&mut stream, 429, "{}");
        }
        seen.insert(trigger.request_id.clone());
        queue.push_back(RemoteAction {
            queued: std::time::Instant::now(),
            issued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            request_id: trigger.request_id,
            action,
        });
        return respond(&mut stream, 202, r#"{"queued":true}"#);
    }
    if path != "/v1/transcripts" {
        return respond(&mut stream, 404, "{}");
    }
    let upload: Upload = match serde_json::from_slice(&body) {
        Ok(t) => t,
        Err(_) => return respond(&mut stream, 400, "{}"),
    };
    if !safe_id(&upload.id) || upload.text.len() > 1_500_000 || upload.title.len() > 1024 {
        return respond(&mut stream, 400, "{}");
    }
    let pdf = match base64::engine::general_purpose::STANDARD.decode(&upload.pdf) {
        Ok(p) if p.starts_with(b"%PDF-") && p.len() <= 8 * 1024 * 1024 => p,
        _ => return respond(&mut stream, 400, "{}"),
    };
    let digest = hex(&Sha256::digest(
        [pdf.as_slice(), upload.text.as_bytes()].concat(),
    ));
    let _guard = serial.lock().map_err(|_| Error::other("state"))?;
    if stop.load(Ordering::Relaxed) {
        return respond(&mut stream, 503, "{}");
    }
    let marker = directory.join(format!("{}.sha256", upload.id));
    if marker.exists() {
        if fs::read_to_string(marker)? == digest {
            return respond(&mut stream, 200, r#"{"received":true}"#);
        } else {
            return respond(
                &mut stream,
                409,
                r#"{"error":"immutable transcript changed"}"#,
            );
        }
    }
    let pdf_path = directory.join(format!("{}.pdf", upload.id));
    let text_path = directory.join(format!("{}.txt", upload.id));
    if (pdf_path.exists() && fs::read(&pdf_path)? != pdf)
        || (text_path.exists() && fs::read(&text_path)? != upload.text.as_bytes())
    {
        return respond(&mut stream, 409, r#"{"error":"existing file differs"}"#);
    }
    // Generated IDs, private temporary files and marker written last: retries are safe.
    save_private(&directory.join(format!("{}.pdf", upload.id)), &pdf)?;
    save_private(
        &directory.join(format!("{}.txt", upload.id)),
        upload.text.as_bytes(),
    )?;
    save_private(&marker, digest.as_bytes())?;
    respond(&mut stream, 200, r#"{"received":true}"#)
}
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn respond(stream: &mut impl Write, code: u16, body: &str) -> Result<()> {
    write!(stream,"HTTP/1.1 {code} Mira\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())?;
    stream.flush()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_are_whitelisted_and_never_shell_commands() {
        assert_eq!(validate_chord("alt+ctrl+2").unwrap(), ["CTRL", "ALT", "2"]);
        for bad in [
            "CTRL+ALT",
            "CTRL+X+Y",
            "CTRL+CTRL+A",
            "rm -rf /",
            "CTRL+$(whoami)",
            "F25",
        ] {
            assert!(validate_chord(bad).is_err(), "{bad}")
        }
    }
    #[test]
    fn identifiers_cannot_escape_the_inbox() {
        for s in ["../x", "a/b", "a.txt", "", "%2e%2e"] {
            assert!(!safe_id(s))
        }
        assert!(safe_id("abc-123"));
        assert!(constant_eq(b"secret", b"secret"));
        assert!(!constant_eq(b"secret", b"secreX"));
    }
}

#[cfg(test)]
mod tls_integration {
    use super::*;
    fn call(receiver: &Receiver, token: &str, method: &str, path: &str, body: &str) -> String {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(receiver.shared.lock().unwrap().cert.clone().into())
            .unwrap();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connection = rustls::ClientConnection::new(
            Arc::new(config),
            rustls::pki_types::ServerName::try_from("mira.local").unwrap(),
        )
        .unwrap();
        let port = receiver.pairing.address.rsplit(':').next().unwrap();
        let socket = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut stream = rustls::StreamOwned::new(connection, socket);
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: mira.local\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response
    }
    #[test]
    #[ignore = "Local TLS sockets; run explicitly with network sandbox permission"]
    fn tls_auth_receipts_and_registered_actions() {
        let dir = tempfile::tempdir().unwrap();
        let receiver = Receiver::start(
            dir.path().join("pair.json"),
            dir.path().join("inbox"),
            "127.0.0.1:0",
        )
        .unwrap();
        let token = receiver.shared.lock().unwrap().token.clone();
        assert!(call(&receiver, "wrong", "GET", "/v1/actions", "").starts_with("HTTP/1.1 401"));
        receiver.add_action("Baie", "CTRL+ALT+2").unwrap();
        let id = receiver.snapshot()["actions"][1]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let trigger = serde_json::json!({"request_id":"test-command-1","action_id":id}).to_string();
        for _ in 0..2 {
            assert!(call(&receiver, &token, "POST", "/v1/actions", &trigger)
                .starts_with("HTTP/1.1 202"));
        }
        let jobs = receiver.drain();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].action.keys, ["CTRL", "ALT", "2"]);
        assert!(call(
            &receiver,
            &token,
            "POST",
            "/v1/actions",
            r#"{"request_id":"test-command-2","action_id":"shell","keys":["anything"]}"#
        )
        .starts_with("HTTP/1.1 404"));
        let upload=serde_json::json!({"id":"document-1","title":"Mira","text":"Ședință mâine","pdf":base64::engine::general_purpose::STANDARD.encode(b"%PDF-1.4\ntransport test\n%%EOF")}).to_string();
        for _ in 0..2 {
            assert!(call(&receiver, &token, "POST", "/v1/transcripts", &upload)
                .starts_with("HTTP/1.1 200"));
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("inbox/document-1.txt")).unwrap(),
            "Ședință mâine"
        );
        let bad = upload.replace("document-1", "../escape");
        assert!(
            call(&receiver, &token, "POST", "/v1/transcripts", &bad).starts_with("HTTP/1.1 400")
        );
        assert!(call(
            &receiver,
            &token,
            "POST",
            "/v1/transcripts",
            &upload.replace("Ședință mâine", "changed")
        )
        .starts_with("HTTP/1.1 409"));
        let port = receiver
            .pairing
            .address
            .rsplit(':')
            .next()
            .unwrap()
            .to_owned();
        drop(receiver);
        assert!(TcpListener::bind(format!("127.0.0.1:{port}")).is_ok());
    }
}
