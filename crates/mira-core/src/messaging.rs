//! Generic messaging inbox + a client for an opt-in local message gateway.
//!
//! Mira never speaks a chat protocol itself. Instead an **opt-in gateway**
//! process (for example an unofficial WhatsApp bridge such as GOWA) runs on the
//! same machine and exposes a tiny HTTP/JSON surface over `127.0.0.1`. This
//! module models the inbox the notch renders — *one line per chat*, unread
//! counts, quick reply and the QR pairing flow — and a thin async client that
//! talks to such a gateway.
//!
//! The model is deliberately provider-agnostic: WhatsApp is the headline
//! bridge, but the same shapes serve Telegram, Signal or SMS gateways.
//!
//! Privacy & scope: the gateway is local and off by default; Mira stores no
//! message bodies on disk (the inbox lives in memory, like [`crate::app::AppState`]).
//! Unofficial bridges are the user's own responsibility — we surface status so
//! the UI can say so plainly.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which chat network a gateway is bridging. Used only as a label for the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessagingProvider {
    #[default]
    WhatsApp,
    Telegram,
    Signal,
    Sms,
    /// Any other bridge that speaks the same gateway protocol.
    Generic,
}

impl MessagingProvider {
    pub fn label(self) -> &'static str {
        match self {
            MessagingProvider::WhatsApp => "WhatsApp",
            MessagingProvider::Telegram => "Telegram",
            MessagingProvider::Signal => "Signal",
            MessagingProvider::Sms => "SMS",
            MessagingProvider::Generic => "Messages",
        }
    }
}

/// Whether a stored message was received or sent by the user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    #[default]
    Incoming,
    Outgoing,
}

impl MessageDirection {
    pub fn is_outgoing(self) -> bool {
        matches!(self, MessageDirection::Outgoing)
    }
}

/// The kind of a message, so the preview line can read "Voice message" or
/// "Photo" instead of an empty body (mirrors the Coucou-style one-liner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    #[default]
    Text,
    Voice,
    Image,
    Video,
    Document,
    Location,
    Sticker,
    Other,
}

impl MessageKind {
    /// Human label used when there is no text body to show.
    pub fn placeholder(self) -> &'static str {
        match self {
            MessageKind::Text => "",
            MessageKind::Voice => "Voice message",
            MessageKind::Image => "Photo",
            MessageKind::Video => "Video",
            MessageKind::Document => "Document",
            MessageKind::Location => "Location",
            MessageKind::Sticker => "Sticker",
            MessageKind::Other => "Message",
        }
    }
}

/// A single message inside a conversation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Stable id from the gateway (used to de-duplicate on refresh).
    pub id: String,
    /// Display name of the sender (empty for our own outgoing messages).
    pub author: String,
    /// Text body; may be empty for non-text [`MessageKind`]s.
    pub body: String,
    #[serde(default)]
    pub kind: MessageKind,
    #[serde(default)]
    pub direction: MessageDirection,
    /// Unix epoch seconds.
    pub timestamp_epoch: i64,
    /// Whether the user has seen this (incoming) message.
    #[serde(default)]
    pub read: bool,
}

impl ChatMessage {
    /// A one-line preview: the text body, or the kind's placeholder when empty.
    pub fn preview(&self) -> String {
        let trimmed = self.body.trim();
        if trimmed.is_empty() {
            self.kind.placeholder().to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn is_unread_incoming(&self) -> bool {
        self.direction == MessageDirection::Incoming && !self.read
    }
}

/// One chat thread with its messages, newest last.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    /// Stable gateway id (phone JID, chat id, …). Opaque to the UI.
    pub id: String,
    /// Contact or group name shown in the header.
    pub title: String,
    /// Whether notifications are silenced for this chat.
    #[serde(default)]
    pub muted: bool,
    /// Messages ordered oldest → newest.
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

impl Conversation {
    /// Count of unread *incoming* messages.
    pub fn unread_count(&self) -> u32 {
        self.messages
            .iter()
            .filter(|m| m.is_unread_incoming())
            .count() as u32
    }

    pub fn last_message(&self) -> Option<&ChatMessage> {
        self.messages.last()
    }

    /// Epoch of the most recent message, or 0 for an empty thread.
    pub fn last_activity_epoch(&self) -> i64 {
        self.last_message().map(|m| m.timestamp_epoch).unwrap_or(0)
    }

    /// The compact render data for the inbox list ("who wrote, what, unread").
    pub fn summary_line(&self) -> ConversationLine {
        let last = self.last_message();
        let (preview, outgoing_last, author) = match last {
            Some(m) if m.direction.is_outgoing() => {
                // Our own last message gets a "You:" prefix like the mockups.
                (m.preview(), true, String::new())
            }
            Some(m) => (m.preview(), false, m.author.clone()),
            None => (String::new(), false, String::new()),
        };
        ConversationLine {
            id: self.id.clone(),
            title: self.title.clone(),
            initials: initials(&self.title),
            author,
            preview,
            unread: self.unread_count(),
            timestamp_epoch: self.last_activity_epoch(),
            outgoing_last,
            muted: self.muted,
        }
    }

    /// Mark every incoming message as read; returns `true` if anything changed.
    pub fn mark_read(&mut self) -> bool {
        let mut changed = false;
        for m in &mut self.messages {
            if m.is_unread_incoming() {
                m.read = true;
                changed = true;
            }
        }
        changed
    }
}

/// Flat, render-ready summary of a conversation for the "one line per chat" list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversationLine {
    pub id: String,
    pub title: String,
    /// Up to two uppercase letters for an avatar fallback (e.g. "HD").
    pub initials: String,
    /// Sender name for incoming previews; empty when the last message is ours.
    pub author: String,
    pub preview: String,
    pub unread: u32,
    pub timestamp_epoch: i64,
    /// Whether the last message in the thread was sent by the user.
    pub outgoing_last: bool,
    pub muted: bool,
}

/// Connection state of the gateway, including the QR pairing step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProviderStatus {
    /// The gateway is not running / not reachable.
    Disconnected,
    /// The gateway is starting up or (re)connecting to the chat network.
    Connecting,
    /// Waiting for the user to scan a login QR with their phone. The payload is
    /// the string to encode as a QR (the UI renders it with `qrcodegen`).
    WaitingForScan { qr: String },
    /// Linked and syncing.
    Connected,
}

impl Default for ProviderStatus {
    fn default() -> Self {
        ProviderStatus::Disconnected
    }
}

impl ProviderStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ProviderStatus::Connected)
    }
}

/// The whole inbox the notch renders. Lives in memory only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inbox {
    pub provider: MessagingProvider,
    pub status: ProviderStatus,
    /// Conversations in arbitrary order; use [`Self::one_line_per_chat`] to sort.
    pub conversations: Vec<Conversation>,
}

impl Default for Inbox {
    fn default() -> Self {
        Inbox {
            provider: MessagingProvider::default(),
            status: ProviderStatus::default(),
            conversations: Vec::new(),
        }
    }
}

impl Inbox {
    /// Total unread incoming messages across every chat.
    pub fn total_unread(&self) -> u32 {
        self.conversations.iter().map(|c| c.unread_count()).sum()
    }

    /// Number of chats with at least one unread message.
    pub fn unread_chats(&self) -> u32 {
        self.conversations
            .iter()
            .filter(|c| c.unread_count() > 0)
            .count() as u32
    }

    /// Render-ready summaries, most-recent chat first, matching the mockups.
    pub fn one_line_per_chat(&self) -> Vec<ConversationLine> {
        let mut lines: Vec<ConversationLine> = self
            .conversations
            .iter()
            .map(|c| c.summary_line())
            .collect();
        lines.sort_by(|a, b| b.timestamp_epoch.cmp(&a.timestamp_epoch));
        lines
    }

    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    pub fn conversation_mut(&mut self, id: &str) -> Option<&mut Conversation> {
        self.conversations.iter_mut().find(|c| c.id == id)
    }

    /// Merge a freshly fetched snapshot of conversations into this inbox,
    /// replacing threads by id and appending new ones. Returns `true` when the
    /// visible state changed (so hosts only redraw on real movement).
    pub fn merge(&mut self, incoming: Vec<Conversation>) -> bool {
        let mut changed = false;
        for conv in incoming {
            match self.conversation_mut(&conv.id) {
                Some(existing) if *existing != conv => {
                    *existing = conv;
                    changed = true;
                }
                Some(_) => {}
                None => {
                    self.conversations.push(conv);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Up to two uppercase initials from a display name ("Hugo Dubois" -> "HD").
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

// ---------------------------------------------------------------------------
// Local gateway client
// ---------------------------------------------------------------------------

/// What we ask a gateway to send. Kept separate from [`ChatMessage`] so the
/// wire shape can evolve without touching stored state.
#[derive(Clone, Debug, Serialize)]
pub struct OutgoingMessage<'a> {
    pub conversation_id: &'a str,
    pub body: &'a str,
}

/// Raw conversation shape as returned by the gateway's `/conversations` route.
/// Deserialized then mapped to [`Conversation`]; isolating it keeps the model
/// free of wire concerns and unit-testable.
#[derive(Clone, Debug, Deserialize)]
pub struct GatewayConversation {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub messages: Vec<GatewayMessage>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GatewayMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: MessageKind,
    #[serde(default)]
    pub direction: MessageDirection,
    #[serde(default)]
    pub timestamp_epoch: i64,
    #[serde(default)]
    pub read: bool,
}

impl From<GatewayMessage> for ChatMessage {
    fn from(m: GatewayMessage) -> Self {
        ChatMessage {
            id: m.id,
            author: m.author,
            body: m.body,
            kind: m.kind,
            direction: m.direction,
            timestamp_epoch: m.timestamp_epoch,
            read: m.read,
        }
    }
}

impl From<GatewayConversation> for Conversation {
    fn from(c: GatewayConversation) -> Self {
        Conversation {
            id: c.id,
            title: c.title,
            muted: c.muted,
            messages: c.messages.into_iter().map(ChatMessage::from).collect(),
        }
    }
}

/// Map a decoded gateway payload into model conversations.
pub fn conversations_from_gateway(raw: Vec<GatewayConversation>) -> Vec<Conversation> {
    raw.into_iter().map(Conversation::from).collect()
}

/// Thin async client around a local message gateway's HTTP/JSON API.
///
/// The default base points at loopback; nothing is contacted until a method is
/// called, and the gateway is expected to be a process the user started
/// themselves. Mirrors [`crate::lrclib::LrclibClient`] so tests can target a
/// mock server via [`Self::with_base`].
#[derive(Clone)]
pub struct GatewayClient {
    http: reqwest::Client,
    base: String,
}

/// Default loopback address a local gateway listens on.
pub const DEFAULT_GATEWAY_URL: &str = "http://127.0.0.1:3000";

impl GatewayClient {
    pub fn new() -> Result<Self> {
        Self::with_base(DEFAULT_GATEWAY_URL)
    }

    pub fn with_base(base: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(GatewayClient {
            http,
            base: base.into(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base.trim_end_matches('/'), path)
    }

    /// Current gateway connection status (including a pending QR).
    pub async fn status(&self) -> Result<ProviderStatus> {
        let resp = self.http.get(self.url("/status")).send().await?;
        if !resp.status().is_success() {
            return Ok(ProviderStatus::Disconnected);
        }
        Ok(resp.json().await?)
    }

    /// Fetch the inbox snapshot.
    pub async fn conversations(&self) -> Result<Vec<Conversation>> {
        let resp = self.http.get(self.url("/conversations")).send().await?;
        if !resp.status().is_success() {
            return Err(Error::other(format!(
                "gateway returned HTTP {}",
                resp.status()
            )));
        }
        let raw: Vec<GatewayConversation> = resp.json().await?;
        Ok(conversations_from_gateway(raw))
    }

    /// Send a reply into a conversation. Returns the gateway's accepted id, if any.
    pub async fn send(&self, conversation_id: &str, body: &str) -> Result<()> {
        let payload = OutgoingMessage {
            conversation_id,
            body,
        };
        let resp = self
            .http
            .post(self.url("/send"))
            .json(&payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::other(format!(
                "gateway refused send: HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, author: &str, body: &str, dir: MessageDirection, ts: i64) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            author: author.into(),
            body: body.into(),
            kind: MessageKind::Text,
            direction: dir,
            timestamp_epoch: ts,
            read: false,
        }
    }

    #[test]
    fn preview_falls_back_to_kind_placeholder() {
        let mut m = msg("1", "Mom", "", MessageDirection::Incoming, 10);
        m.kind = MessageKind::Voice;
        assert_eq!(m.preview(), "Voice message");
        let t = msg(
            "2",
            "Lea",
            "  Dinner tonight?  ",
            MessageDirection::Incoming,
            11,
        );
        assert_eq!(t.preview(), "Dinner tonight?");
    }

    #[test]
    fn unread_counts_only_incoming_unread() {
        let mut c = Conversation {
            id: "lea".into(),
            title: "Léa".into(),
            muted: false,
            messages: vec![
                msg(
                    "1",
                    "Léa",
                    "Are you free tonight?",
                    MessageDirection::Incoming,
                    1,
                ),
                msg(
                    "2",
                    "Léa",
                    "Dinner tonight? I booked for 8",
                    MessageDirection::Incoming,
                    2,
                ),
            ],
        };
        assert_eq!(c.unread_count(), 2);
        // One of them read -> 1.
        c.messages[0].read = true;
        assert_eq!(c.unread_count(), 1);
        // An outgoing message never counts as unread.
        c.messages
            .push(msg("3", "", "Yes! See you", MessageDirection::Outgoing, 3));
        assert_eq!(c.unread_count(), 1);
    }

    #[test]
    fn summary_line_prefixes_our_own_last_message() {
        let c = Conversation {
            id: "hugo".into(),
            title: "Hugo Dubois".into(),
            muted: false,
            messages: vec![msg(
                "1",
                "",
                "deal, send me the deck",
                MessageDirection::Outgoing,
                100,
            )],
        };
        let line = c.summary_line();
        assert!(line.outgoing_last);
        assert_eq!(line.initials, "HD");
        assert_eq!(line.preview, "deal, send me the deck");
        assert_eq!(line.unread, 0);
    }

    #[test]
    fn mark_read_clears_unread() {
        let mut c = Conversation {
            id: "mom".into(),
            title: "Mom".into(),
            muted: false,
            messages: vec![msg("1", "Mom", "", MessageDirection::Incoming, 5)],
        };
        c.messages[0].kind = MessageKind::Voice;
        assert_eq!(c.unread_count(), 1);
        assert!(c.mark_read());
        assert_eq!(c.unread_count(), 0);
        // Idempotent.
        assert!(!c.mark_read());
    }

    #[test]
    fn inbox_sorts_most_recent_first_and_totals_unread() {
        let inbox = Inbox {
            provider: MessagingProvider::WhatsApp,
            status: ProviderStatus::Connected,
            conversations: vec![
                Conversation {
                    id: "hugo".into(),
                    title: "Hugo Dubois".into(),
                    muted: false,
                    messages: vec![msg("1", "", "deal", MessageDirection::Outgoing, 50)],
                },
                Conversation {
                    id: "lea".into(),
                    title: "Léa".into(),
                    muted: false,
                    messages: vec![msg("2", "Léa", "Dinner?", MessageDirection::Incoming, 200)],
                },
                Conversation {
                    id: "mom".into(),
                    title: "Mom".into(),
                    muted: false,
                    messages: vec![msg("3", "Mom", "hi", MessageDirection::Incoming, 120)],
                },
            ],
        };
        let lines = inbox.one_line_per_chat();
        assert_eq!(lines[0].title, "Léa"); // ts 200
        assert_eq!(lines[1].title, "Mom"); // ts 120
        assert_eq!(lines[2].title, "Hugo Dubois"); // ts 50
        assert_eq!(inbox.total_unread(), 2);
        assert_eq!(inbox.unread_chats(), 2);
    }

    #[test]
    fn merge_replaces_by_id_and_appends_new() {
        let mut inbox = Inbox::default();
        assert!(inbox.merge(vec![Conversation {
            id: "lea".into(),
            title: "Léa".into(),
            muted: false,
            messages: vec![msg("1", "Léa", "hi", MessageDirection::Incoming, 1)],
        }]));
        // Same snapshot again -> no change.
        assert!(!inbox.merge(vec![inbox.conversations[0].clone()]));
        // Updated thread -> change, replaced not duplicated.
        assert!(inbox.merge(vec![Conversation {
            id: "lea".into(),
            title: "Léa".into(),
            muted: false,
            messages: vec![
                msg("1", "Léa", "hi", MessageDirection::Incoming, 1),
                msg("2", "Léa", "you there?", MessageDirection::Incoming, 2),
            ],
        }]));
        assert_eq!(inbox.conversations.len(), 1);
        assert_eq!(inbox.conversations[0].messages.len(), 2);
    }

    #[test]
    fn gateway_json_maps_into_model() {
        let json = r#"[
            {"id":"lea","title":"Léa","messages":[
                {"id":"m1","author":"Léa","body":"Dinner tonight? I booked for 8","direction":"incoming","timestamp_epoch":200,"read":false},
                {"id":"m2","author":"Léa","kind":"voice","direction":"incoming","timestamp_epoch":205}
            ]}
        ]"#;
        let raw: Vec<GatewayConversation> = serde_json::from_str(json).unwrap();
        let convs = conversations_from_gateway(raw);
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].messages.len(), 2);
        assert_eq!(convs[0].unread_count(), 2);
        assert_eq!(convs[0].messages[1].preview(), "Voice message");
    }

    #[test]
    fn status_json_round_trips_with_qr() {
        let s = ProviderStatus::WaitingForScan {
            qr: "2@abc123".into(),
        };
        let encoded = serde_json::to_string(&s).unwrap();
        assert!(encoded.contains("waiting_for_scan"));
        let back: ProviderStatus = serde_json::from_str(&encoded).unwrap();
        assert_eq!(back, s);
        assert!(!back.is_connected());
    }
}
