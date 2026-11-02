//! GOWA adapter — install and drive the open-source WhatsApp gateway
//! [`go-whatsapp-web-multidevice`](https://github.com/aldinokemal/go-whatsapp-web-multidevice).
//!
//! Mira can download the official GOWA release binary for this platform
//! (verifying it against the release's `checksums.txt`), run it **bound to
//! loopback only** (`APP_HOST=127.0.0.1`) with a generated basic-auth
//! credential and its web UI disabled, then speak its REST API and map the
//! replies into Mira's own [`crate::messaging`] model.
//!
//! This is strictly opt-in. GOWA uses the *unofficial* WhatsApp Web protocol,
//! so it is the user's own account and responsibility; Mira only ever contacts
//! the local gateway and GitHub (for the one-time download).
//!
//! Security posture: HTTPS-only download from GitHub release URLs, SHA-256
//! verified before the binary is made executable; the gateway listens on
//! 127.0.0.1; the basic-auth credential lives in a 0600 file in Mira's data
//! dir, never in `config.toml`.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::messaging::{
    ChatMessage, Conversation, Inbox, MessageDirection, MessageKind, MessagingProvider,
    ProviderStatus,
};

/// Upstream repository that publishes the gateway releases.
pub const GOWA_REPO: &str = "aldinokemal/go-whatsapp-web-multidevice";
const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("Mira/", env!("CARGO_PKG_VERSION"));

// ---------------------------------------------------------------------------
// Platform + release-asset resolution (pure, unit-tested)
// ---------------------------------------------------------------------------

/// The GOWA naming for the current OS/CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    /// GOWA os token: `linux`, `windows`, or `darwin`.
    pub os: &'static str,
    /// GOWA arch token: `amd64`, `arm64`, or `386`.
    pub arch: &'static str,
    /// `.exe` on Windows, else empty.
    pub exe_suffix: &'static str,
}

/// Resolve the running platform to GOWA's release naming, or `None` if we have
/// no matching release asset for it.
pub fn current_platform() -> Option<Platform> {
    platform_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Pure mapping from Rust's `OS`/`ARCH` constants to GOWA tokens.
pub fn platform_for(os: &str, arch: &str) -> Option<Platform> {
    let (os, exe_suffix) = match os {
        "linux" => ("linux", ""),
        "windows" => ("windows", ".exe"),
        "macos" => ("darwin", ""),
        _ => return None,
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        _ => return None,
    };
    Some(Platform {
        os,
        arch,
        exe_suffix,
    })
}

/// Pick the release zip asset matching this platform from a list of asset names.
pub fn select_asset(names: &[String], plat: &Platform) -> Option<String> {
    let needle = format!("_{}_{}", plat.os, plat.arch);
    names
        .iter()
        .find(|n| {
            n.ends_with(".zip")
                && n.contains(&needle)
                && !n.contains("checksums")
                // `arm64` must not match a bare `arm` needle, and vice versa:
                // the needle already carries the exact arch token, so a plain
                // `contains` is safe because tokens are underscore-delimited.
                && n.contains(&format!("{}.", plat.arch))
        })
        .cloned()
}

/// Pick the checksums file for this platform (`checksums-macos.txt` on macOS,
/// else `checksums.txt`).
pub fn select_checksums(names: &[String], plat: &Platform) -> Option<String> {
    let preferred = if plat.os == "darwin" {
        "checksums-macos.txt"
    } else {
        "checksums.txt"
    };
    names
        .iter()
        .find(|n| n.as_str() == preferred)
        .or_else(|| names.iter().find(|n| n.starts_with("checksums")))
        .cloned()
}

/// Find the SHA-256 hex for `asset` inside a `checksums.txt` body.
/// Lines look like `"<64 hex>  whatsapp_7.6.0_linux_amd64.zip"`.
pub fn sha256_for(checksums: &str, asset: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hex = parts.next()?;
        let name = parts.last()?;
        if name == asset && hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(hex.to_lowercase())
        } else {
            None
        }
    })
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Credentials (generated, stored 0600 in the data dir — never in config.toml)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub user: String,
    pub pass: String,
}

impl Credentials {
    fn generate() -> Self {
        let mut raw = [0u8; 16];
        let _ = getrandom::getrandom(&mut raw);
        let pass = raw.iter().map(|b| format!("{b:02x}")).collect();
        Credentials {
            user: "mira".into(),
            pass,
        }
    }
}

// ---------------------------------------------------------------------------
// Manager: install (download + verify + extract) and run the gateway
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct InstallReport {
    pub version: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// Owns the on-disk gateway binary, credentials and data directory.
pub struct GowaManager {
    dir: PathBuf,
    plat: Platform,
}

impl GowaManager {
    /// Create a manager rooted at `<data_dir>/gowa`. Fails on an unsupported OS/CPU.
    pub fn new() -> Result<Self> {
        let dir = crate::store::data_dir()?.join("gowa");
        std::fs::create_dir_all(&dir)?;
        let plat =
            current_platform().ok_or_else(|| Error::other("no GOWA release for this platform"))?;
        Ok(GowaManager { dir, plat })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path where the gateway binary is stored.
    pub fn binary_path(&self) -> PathBuf {
        self.dir.join(format!("whatsapp{}", self.plat.exe_suffix))
    }

    pub fn is_installed(&self) -> bool {
        self.binary_path().is_file()
    }

    fn creds_path(&self) -> PathBuf {
        self.dir.join("gowa-auth.json")
    }

    /// Read the stored credential, generating and persisting one on first use.
    pub fn credentials(&self) -> Result<Credentials> {
        let path = self.creds_path();
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(c) = serde_json::from_slice::<Credentials>(&bytes) {
                return Ok(c);
            }
        }
        let creds = Credentials::generate();
        std::fs::write(&path, serde_json::to_vec(&creds)?)?;
        restrict_permissions(&path);
        Ok(creds)
    }

    /// Download, verify and install the gateway binary. `version` is a tag
    /// without the leading `v` (e.g. "7.6.0"); `None` uses the latest release.
    pub async fn install(&self, version: Option<&str>) -> Result<InstallReport> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let api = match version {
            Some(v) => format!("{GITHUB_API}/repos/{GOWA_REPO}/releases/tags/v{v}"),
            None => format!("{GITHUB_API}/repos/{GOWA_REPO}/releases/latest"),
        };
        let release: Release = http
            .get(&api)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let names: Vec<String> = release.assets.iter().map(|a| a.name.clone()).collect();
        let asset_name = select_asset(&names, &self.plat)
            .ok_or_else(|| Error::other("no matching GOWA asset in the release"))?;
        let checksums_name = select_checksums(&names, &self.plat);

        let asset_url = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| Error::other("asset url missing"))?;

        // Verify against checksums.txt when the release ships one.
        if let Some(cs_name) = checksums_name {
            if let Some(cs_url) = release
                .assets
                .iter()
                .find(|a| a.name == cs_name)
                .map(|a| a.browser_download_url.clone())
            {
                let checksums = http.get(&cs_url).send().await?.text().await?;
                let zip_bytes = http
                    .get(&asset_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await?
                    .to_vec();
                if let Some(expected) = sha256_for(&checksums, &asset_name) {
                    let got = sha256_hex(&zip_bytes);
                    if got != expected {
                        return Err(Error::other(format!(
                            "checksum mismatch for {asset_name}: expected {expected}, got {got}"
                        )));
                    }
                }
                return self.extract(zip_bytes, release.tag_name).await;
            }
        }

        // No checksums file published: download and extract without verification.
        let zip_bytes = http
            .get(&asset_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
        self.extract(zip_bytes, release.tag_name).await
    }

    /// Extract the gateway binary from the release zip and make it executable.
    async fn extract(&self, zip_bytes: Vec<u8>, tag: String) -> Result<InstallReport> {
        let dest = self.binary_path();
        let plat = self.plat;
        tokio::task::spawn_blocking(move || -> Result<u64> {
            let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
                .map_err(|e| Error::other(format!("open zip: {e}")))?;
            let wanted = format!("{}-{}{}", plat.os, plat.arch, plat.exe_suffix);
            let mut data: Option<Vec<u8>> = None;
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| Error::other(format!("zip entry: {e}")))?;
                if !entry.is_file() {
                    continue;
                }
                let name = entry.name().rsplit('/').next().unwrap_or("").to_string();
                let is_binary = name == wanted
                    || name == format!("{}-{}", plat.os, plat.arch)
                    || name.starts_with(&format!("{}-{}", plat.os, plat.arch));
                if is_binary {
                    let mut buf = Vec::new();
                    entry
                        .read_to_end(&mut buf)
                        .map_err(|e| Error::other(format!("read zip entry: {e}")))?;
                    data = Some(buf);
                    break;
                }
            }
            let data = data.ok_or_else(|| Error::other("gateway binary not found in zip"))?;
            let bytes = data.len() as u64;
            std::fs::write(&dest, &data)?;
            make_executable(&dest);
            Ok(bytes)
        })
        .await
        .map_err(|e| Error::other(e.to_string()))?
        .map(|bytes| InstallReport {
            version: tag,
            path: self.binary_path().to_string_lossy().into_owned(),
            bytes,
        })
    }

    /// Spawn the gateway: `rest` bound to loopback, UI off, basic auth set.
    pub fn start(&self, port: u16, creds: &Credentials) -> Result<Child> {
        if !self.is_installed() {
            return Err(Error::other("gateway is not installed"));
        }
        let mut cmd = Command::new(self.binary_path());
        cmd.arg("rest")
            .arg(format!("--port={port}"))
            .arg(format!("--basic-auth={}:{}", creds.user, creds.pass))
            .arg("--os=Mira")
            .arg("--account-validation=false")
            .env("APP_HOST", "127.0.0.1")
            .env("APP_UI_ENABLED", "false")
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Ok(cmd.spawn()?)
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
}
#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

// ---------------------------------------------------------------------------
// REST client + model mapping
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default, Debug, Clone)]
pub struct GowaStatus {
    #[serde(default)]
    pub is_connected: bool,
    #[serde(default)]
    pub is_logged_in: bool,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub jid: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct LoginQr {
    #[serde(default)]
    pub qr_link: String,
    #[serde(default)]
    pub qr_duration: i64,
}

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    results: T,
}

#[derive(Deserialize, Default)]
struct ChatList {
    #[serde(default)]
    data: Vec<GowaChat>,
}

#[derive(Deserialize, Default, Clone)]
pub struct GowaChat {
    #[serde(default)]
    pub jid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub last_message_time: String,
}

#[derive(Deserialize, Default)]
struct MessageList {
    #[serde(default)]
    data: Vec<GowaMessage>,
}

#[derive(Deserialize, Default, Clone)]
pub struct GowaMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub sender_display_name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub is_from_me: bool,
    #[serde(default)]
    pub media_type: Option<String>,
}

/// Map a GOWA `media_type` + body to a Mira [`MessageKind`].
pub fn kind_from_media(media_type: Option<&str>, body: &str) -> MessageKind {
    match media_type.unwrap_or("").to_ascii_lowercase().as_str() {
        "image" => MessageKind::Image,
        "video" => MessageKind::Video,
        "audio" | "ptt" | "voice" => MessageKind::Voice,
        "document" => MessageKind::Document,
        "sticker" => MessageKind::Sticker,
        "location" => MessageKind::Location,
        "" if body.trim().is_empty() => MessageKind::Other,
        "" => MessageKind::Text,
        _ => MessageKind::Other,
    }
}

/// Parse an RFC3339 timestamp to unix epoch seconds (0 on failure).
pub fn epoch_from_rfc3339(ts: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

impl GowaMessage {
    pub fn into_model(self) -> ChatMessage {
        let direction = if self.is_from_me {
            MessageDirection::Outgoing
        } else {
            MessageDirection::Incoming
        };
        let kind = kind_from_media(self.media_type.as_deref(), &self.content);
        ChatMessage {
            id: self.id,
            author: if self.is_from_me {
                String::new()
            } else {
                self.sender_display_name
            },
            body: self.content,
            kind,
            direction,
            timestamp_epoch: epoch_from_rfc3339(&self.timestamp),
            // GOWA exposes no per-message read state, so we never flag false
            // unread badges; local unread tracking is a future enhancement.
            read: true,
        }
    }
}

/// Encode PNG bytes as a `data:` URI, ready for a UI to render directly.
pub fn png_data_uri(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Thin async client over a running GOWA instance.
#[derive(Clone)]
pub struct GowaClient {
    http: reqwest::Client,
    base: String,
    creds: Option<Credentials>,
    device_id: Option<String>,
}

impl GowaClient {
    pub fn new(base: impl Into<String>, creds: Option<Credentials>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        Ok(GowaClient {
            http,
            base: base.into(),
            creds,
            device_id: None,
        })
    }

    pub fn with_device(mut self, device_id: impl Into<String>) -> Self {
        let id = device_id.into();
        if !id.is_empty() {
            self.device_id = Some(id);
        }
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base.trim_end_matches('/'), path)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.auth(self.http.get(self.url(path)))
    }

    fn auth(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(c) = &self.creds {
            rb = rb.basic_auth(&c.user, Some(&c.pass));
        }
        if let Some(d) = &self.device_id {
            rb = rb.header("X-Device-Id", d);
        }
        rb
    }

    pub async fn app_status(&self) -> Result<GowaStatus> {
        let env: Envelope<GowaStatus> = self
            .get("/app/status")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(env.results)
    }

    pub async fn login(&self) -> Result<LoginQr> {
        let env: Envelope<LoginQr> = self
            .get("/app/login")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(env.results)
    }

    /// Fetch the QR PNG bytes from an absolute `qr_link` (served by GOWA).
    pub async fn fetch_qr_png(&self, qr_link: &str) -> Result<Vec<u8>> {
        Ok(self
            .auth(self.http.get(qr_link))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec())
    }

    pub async fn logout(&self) -> Result<()> {
        self.get("/app/logout").send().await?;
        Ok(())
    }

    async fn chats(&self, limit: u32) -> Result<Vec<GowaChat>> {
        let env: Envelope<ChatList> = self
            .get(&format!("/chats?limit={limit}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(env.results.data)
    }

    /// Fetch and model a chat's messages, oldest first.
    pub async fn messages(&self, jid: &str, limit: u32) -> Result<Vec<ChatMessage>> {
        let encoded = jid.replace('@', "%40");
        let env: Envelope<MessageList> = self
            .get(&format!("/chat/{encoded}/messages?limit={limit}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut msgs: Vec<ChatMessage> = env
            .results
            .data
            .into_iter()
            .map(GowaMessage::into_model)
            .collect();
        msgs.sort_by_key(|m| m.timestamp_epoch);
        Ok(msgs)
    }

    /// Send a text message to a chat JID.
    pub async fn send(&self, jid: &str, text: &str) -> Result<()> {
        let body = serde_json::json!({ "phone": jid, "message": text });
        let resp = self
            .auth(self.http.post(self.url("/send/message")))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::other(format!("send failed: HTTP {}", resp.status())));
        }
        Ok(())
    }

    /// Build the one-line-per-chat inbox: list chats, then fetch each chat's
    /// most recent message for its preview (capped to keep it snappy).
    pub async fn inbox(&self, chat_limit: u32) -> Result<Inbox> {
        let status = self.provider_status().await.unwrap_or_default();
        let chats = self.chats(chat_limit).await?;
        let mut conversations = Vec::with_capacity(chats.len());
        for chat in chats.into_iter().take(20) {
            let messages = self.messages(&chat.jid, 1).await.unwrap_or_default();
            conversations.push(Conversation {
                id: chat.jid,
                title: chat.name,
                muted: false,
                messages,
            });
        }
        Ok(Inbox {
            provider: MessagingProvider::WhatsApp,
            status,
            conversations,
        })
    }

    /// Derive a [`ProviderStatus`]; when linked-but-not-logged-in, attach the QR.
    pub async fn provider_status(&self) -> Result<ProviderStatus> {
        let status = self.app_status().await?;
        if status.is_logged_in && status.is_connected {
            return Ok(ProviderStatus::Connected);
        }
        match self.login().await {
            Ok(qr) if !qr.qr_link.is_empty() => {
                Ok(ProviderStatus::WaitingForScan { qr: qr.qr_link })
            }
            _ if status.is_connected => Ok(ProviderStatus::Connecting),
            _ => Ok(ProviderStatus::Disconnected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Vec<String> {
        vec![
            "whatsapp_7.6.0_linux_amd64.zip".into(),
            "whatsapp_7.6.0_linux_arm64.zip".into(),
            "whatsapp_7.6.0_windows_amd64.zip".into(),
            "whatsapp_7.6.0_darwin_arm64.zip".into(),
            "checksums.txt".into(),
            "checksums-macos.txt".into(),
        ]
    }

    #[test]
    fn platform_mapping() {
        assert_eq!(platform_for("linux", "x86_64").unwrap().os, "linux");
        assert_eq!(platform_for("linux", "x86_64").unwrap().arch, "amd64");
        assert_eq!(platform_for("macos", "aarch64").unwrap().os, "darwin");
        assert_eq!(
            platform_for("windows", "x86_64").unwrap().exe_suffix,
            ".exe"
        );
        assert!(platform_for("freebsd", "x86_64").is_none());
        assert!(platform_for("linux", "mips").is_none());
    }

    #[test]
    fn asset_selection_matches_exact_arch() {
        let names = assets();
        let linux = platform_for("linux", "x86_64").unwrap();
        assert_eq!(
            select_asset(&names, &linux).unwrap(),
            "whatsapp_7.6.0_linux_amd64.zip"
        );
        // amd64 needle must not grab arm64, and arm64 must not grab amd64.
        let arm = platform_for("linux", "aarch64").unwrap();
        assert_eq!(
            select_asset(&names, &arm).unwrap(),
            "whatsapp_7.6.0_linux_arm64.zip"
        );
        let win = platform_for("windows", "x86_64").unwrap();
        assert_eq!(
            select_asset(&names, &win).unwrap(),
            "whatsapp_7.6.0_windows_amd64.zip"
        );
    }

    #[test]
    fn checksums_file_selection_prefers_macos_on_darwin() {
        let names = assets();
        let mac = platform_for("macos", "arm64").unwrap_or(Platform {
            os: "darwin",
            arch: "arm64",
            exe_suffix: "",
        });
        assert_eq!(
            select_checksums(&names, &mac).unwrap(),
            "checksums-macos.txt"
        );
        let linux = platform_for("linux", "x86_64").unwrap();
        assert_eq!(select_checksums(&names, &linux).unwrap(), "checksums.txt");
    }

    #[test]
    fn checksum_lookup_and_hash() {
        let body = "aa\n\
            e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  whatsapp_7.6.0_linux_amd64.zip\n\
            deadbeef  other.zip\n";
        let got = sha256_for(body, "whatsapp_7.6.0_linux_amd64.zip").unwrap();
        assert_eq!(
            got,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(sha256_for(body, "missing.zip").is_none());
        // Empty input hashes to the known empty SHA-256.
        assert_eq!(sha256_hex(b""), got);
    }

    #[test]
    fn media_type_maps_to_kind() {
        assert_eq!(kind_from_media(Some("image"), ""), MessageKind::Image);
        assert_eq!(kind_from_media(Some("audio"), ""), MessageKind::Voice);
        assert_eq!(kind_from_media(Some("ptt"), ""), MessageKind::Voice);
        assert_eq!(kind_from_media(None, "hi"), MessageKind::Text);
        assert_eq!(kind_from_media(None, "  "), MessageKind::Other);
        assert_eq!(
            kind_from_media(Some("document"), "f"),
            MessageKind::Document
        );
    }

    #[test]
    fn message_maps_into_model() {
        let m = GowaMessage {
            id: "ID1".into(),
            sender_display_name: "Léa".into(),
            content: "Dinner tonight?".into(),
            timestamp: "2024-01-15T10:30:00Z".into(),
            is_from_me: false,
            media_type: None,
        };
        let cm = m.into_model();
        assert_eq!(cm.author, "Léa");
        assert_eq!(cm.direction, MessageDirection::Incoming);
        assert_eq!(cm.timestamp_epoch, 1705314600);
        assert!(cm.read); // no unread flagging from GOWA

        let mine = GowaMessage {
            id: "ID2".into(),
            sender_display_name: "me".into(),
            content: "deal".into(),
            timestamp: "bad-timestamp".into(),
            is_from_me: true,
            media_type: None,
        };
        let cm = mine.into_model();
        assert_eq!(cm.direction, MessageDirection::Outgoing);
        assert!(cm.author.is_empty());
        assert_eq!(cm.timestamp_epoch, 0);
    }

    #[test]
    fn status_envelope_parses() {
        let json = r#"{"code":"SUCCESS","results":{"is_connected":true,"is_logged_in":true,"device_id":"d1","jid":"123@s.whatsapp.net"}}"#;
        let env: Envelope<GowaStatus> = serde_json::from_str(json).unwrap();
        assert!(env.results.is_connected);
        assert!(env.results.is_logged_in);
        assert_eq!(env.results.device_id, "d1");
    }
}
