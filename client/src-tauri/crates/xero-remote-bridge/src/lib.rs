use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("remote bridge identity could not be read from {path}: {source}")]
    IdentityRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("remote bridge identity could not be written to {path}: {source}")]
    IdentityWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("remote bridge identity at {path} is malformed: {source}")]
    IdentityDecode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("remote bridge state could not be read from {path}: {source}")]
    StateRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("remote bridge state could not be written to {path}: {source}")]
    StateWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("remote bridge state at {path} is malformed: {source}")]
    StateDecode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("remote bridge URL `{url}` is invalid: {source}")]
    InvalidRelayUrl {
        url: String,
        source: url::ParseError,
    },
    #[error("remote bridge HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("remote bridge server returned {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("remote bridge payload could not be encoded: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("remote bridge payload could not be decoded: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("remote bridge JSON payload failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("remote bridge websocket failed: {0}")]
    WebSocket(#[source] Box<tungstenite::Error>),
    #[error("remote bridge IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("remote bridge URL uses unsupported scheme `{0}`")]
    UnsupportedUrlScheme(String),
    #[error("remote bridge server response is missing `{0}`")]
    MissingServerField(&'static str),
    #[error("remote bridge lock was poisoned")]
    LockPoisoned,
}

pub type BridgeResult<T> = Result<T, BridgeError>;

impl From<tungstenite::Error> for BridgeError {
    fn from(error: tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(error))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeConfig {
    pub relay_url: String,
    #[serde(default)]
    pub device_name: Option<String>,
}

impl BridgeConfig {
    pub const LOCAL_RELAY_URL: &'static str = "http://127.0.0.1:4000";

    pub fn local_default() -> Self {
        Self {
            relay_url: Self::LOCAL_RELAY_URL.into(),
            device_name: Some("Xero Desktop".into()),
        }
    }

    pub fn from_env_or_local(device_name: impl Into<String>) -> Self {
        Self {
            relay_url: configured_relay_url(),
            device_name: Some(device_name.into()),
        }
    }

    fn endpoint(&self, path: &str) -> BridgeResult<Url> {
        let mut base =
            Url::parse(&self.relay_url).map_err(|source| BridgeError::InvalidRelayUrl {
                url: self.relay_url.clone(),
                source,
            })?;
        base.set_path(path);
        Ok(base)
    }

    fn socket_endpoint(&self, socket_path: &str, token: &str) -> BridgeResult<Url> {
        let mut url =
            Url::parse(&self.relay_url).map_err(|source| BridgeError::InvalidRelayUrl {
                url: self.relay_url.clone(),
                source,
            })?;
        let scheme = match url.scheme() {
            "http" => "ws",
            "https" => "wss",
            other => return Err(BridgeError::UnsupportedUrlScheme(other.to_owned())),
        };
        url.set_scheme(scheme)
            .map_err(|_| BridgeError::UnsupportedUrlScheme(scheme.to_owned()))?;
        url.set_path(socket_path);
        url.query_pairs_mut()
            .append_pair("token", token)
            .append_pair("vsn", "2.0.0");
        Ok(url)
    }
}

fn configured_relay_url() -> String {
    relay_url_from_env_values(
        std::env::var_os("XERO_REMOTE_RELAY_URL"),
        std::env::var_os("VITE_XERO_SERVER_URL"),
    )
}

fn relay_url_from_env_values(remote_url: Option<OsString>, server_url: Option<OsString>) -> String {
    remote_url
        .or(server_url)
        .and_then(|value| value.into_string().ok())
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| BridgeConfig::LOCAL_RELAY_URL.to_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIdentity {
    pub account_id: Option<String>,
    pub desktop_device_id: Option<String>,
    pub desktop_jwt: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub relay_token_expires_at: Option<i64>,
    #[serde(default)]
    pub github_login: Option<String>,
    #[serde(default)]
    pub github_avatar_url: Option<String>,
}

impl DesktopIdentity {
    pub fn generate() -> Self {
        Self {
            account_id: None,
            desktop_device_id: None,
            desktop_jwt: None,
            session_id: None,
            relay_token_expires_at: None,
            github_login: None,
            github_avatar_url: None,
        }
    }
}

pub trait IdentityStore: Send + Sync {
    fn load(&self) -> BridgeResult<Option<DesktopIdentity>>;
    fn save(&self, identity: &DesktopIdentity) -> BridgeResult<()>;
}

#[derive(Debug, Clone)]
pub struct FileIdentityStore {
    path: PathBuf,
}

impl FileIdentityStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl IdentityStore for FileIdentityStore {
    fn load(&self) -> BridgeResult<Option<DesktopIdentity>> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => {
                serde_json::from_str(&raw)
                    .map(Some)
                    .map_err(|source| BridgeError::IdentityDecode {
                        path: self.path.clone(),
                        source,
                    })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(BridgeError::IdentityRead {
                path: self.path.clone(),
                source,
            }),
        }
    }

    fn save(&self, identity: &DesktopIdentity) -> BridgeResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| BridgeError::IdentityWrite {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let raw = serde_json::to_string_pretty(identity).map_err(|source| {
            BridgeError::IdentityDecode {
                path: self.path.clone(),
                source,
            }
        })?;
        fs::write(&self.path, raw).map_err(|source| BridgeError::IdentityWrite {
            path: self.path.clone(),
            source,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    pub id: String,
    pub account_id: String,
    pub kind: String,
    pub name: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    pub last_seen: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

pub type AccountDevice = PairedDevice;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeAccount {
    pub github_login: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub connected: bool,
    pub relay_url: String,
    pub signed_in: bool,
    pub account: Option<BridgeAccount>,
    pub devices: Vec<AccountDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub signed_in: bool,
    pub authorization_url: Option<String>,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub device_id: Option<String>,
    pub relay_token_expires_at: Option<i64>,
    pub account: Option<BridgeAccount>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartedGithubLogin {
    authorization_url: String,
    flow_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubFlowSession {
    status: String,
    session_id: Option<String>,
    session: Option<GithubSessionView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubSessionView {
    user: Option<GithubUserView>,
    account_id: Option<String>,
    device_id: Option<String>,
    relay_token: Option<String>,
    relay_token_expires_at: Option<i64>,
    account: Option<GithubAccountView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubUserView {
    login: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubAccountView {
    github_login: Option<String>,
    github_avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    Snapshot,
    Event,
    Presence,
    SessionAdded,
    SessionRemoved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEnvelope {
    pub v: u8,
    pub seq: u64,
    pub computer_id: String,
    pub session_id: String,
    pub kind: EnvelopeKind,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelayFramePayload {
    pub encoding: String,
    pub envelope: String,
    pub seq: u64,
    pub kind: EnvelopeKind,
}

pub fn encode_envelope(envelope: &RuntimeEnvelope) -> BridgeResult<Vec<u8>> {
    Ok(rmp_serde::to_vec_named(envelope)?)
}

pub fn decode_envelope(bytes: &[u8]) -> BridgeResult<RuntimeEnvelope> {
    Ok(rmp_serde::from_slice(bytes)?)
}

pub fn encode_relay_frame_payload(envelope: &RuntimeEnvelope) -> BridgeResult<JsonValue> {
    Ok(serde_json::to_value(RelayFramePayload {
        encoding: "msgpack.base64url".into(),
        envelope: URL_SAFE_NO_PAD.encode(encode_envelope(envelope)?),
        seq: envelope.seq,
        kind: envelope.kind.clone(),
    })?)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InboundCommandKind {
    SendMessage,
    ListSessions,
    SessionAttached,
    StartSession,
    ResolveOperatorAction,
    CancelRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundCommand {
    pub v: u8,
    pub seq: u64,
    pub computer_id: String,
    pub session_id: Option<String>,
    pub kind: InboundCommandKind,
    pub device_id: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhoenixMessage(
    pub Option<String>,
    pub Option<String>,
    pub String,
    pub String,
    pub JsonValue,
);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhoenixSocketKind {
    Desktop,
    Web,
}

pub struct PhoenixChannelClient {
    socket: WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    next_ref: u64,
}

pub struct DesktopRelayConnection {
    pub desktop_device_id: String,
    client: PhoenixChannelClient,
}

impl DesktopRelayConnection {
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> BridgeResult<()> {
        self.client.set_read_timeout(timeout)
    }

    pub fn join_control(&mut self) -> BridgeResult<JsonValue> {
        self.client
            .join(&format!("desktop:{}", self.desktop_device_id), json!({}))
    }

    pub fn join_session(&mut self, session_id: &str) -> BridgeResult<JsonValue> {
        self.client.join(
            &format!("session:{}:{}", self.desktop_device_id, session_id),
            json!({}),
        )
    }

    pub fn push_session_frame(
        &mut self,
        session_id: &str,
        payload: JsonValue,
    ) -> BridgeResult<JsonValue> {
        self.client.push_and_wait(
            &format!("session:{}:{}", self.desktop_device_id, session_id),
            "frame",
            payload,
        )
    }

    pub fn authorize_session_join(
        &mut self,
        join_ref: &str,
        auth_topic: &str,
        authorized: bool,
    ) -> BridgeResult<JsonValue> {
        self.client.push_and_wait(
            &format!("desktop:{}", self.desktop_device_id),
            "session_authorized",
            json!({
                "join_ref": join_ref,
                "auth_topic": auth_topic,
                "authorized": authorized,
            }),
        )
    }

    pub fn read(&mut self) -> BridgeResult<PhoenixMessage> {
        self.client.read()
    }

    pub fn read_timeout(&mut self, timeout: Duration) -> BridgeResult<Option<PhoenixMessage>> {
        self.client.read_timeout(timeout)
    }

    pub fn heartbeat(&mut self) -> BridgeResult<String> {
        self.client.heartbeat()
    }
}

impl PhoenixChannelClient {
    pub fn connect(
        config: &BridgeConfig,
        token: &str,
        kind: PhoenixSocketKind,
    ) -> BridgeResult<Self> {
        let path = match kind {
            PhoenixSocketKind::Desktop => "/socket/desktop/websocket",
            PhoenixSocketKind::Web => "/socket/web/websocket",
        };
        let url = config.socket_endpoint(path, token)?;
        let (socket, _response) = connect(url.as_str())?;
        Ok(Self {
            socket,
            next_ref: 0,
        })
    }

    pub fn join(&mut self, topic: &str, payload: JsonValue) -> BridgeResult<JsonValue> {
        let reference = self.next_reference();
        self.send(PhoenixMessage(
            Some(reference.clone()),
            Some(reference.clone()),
            topic.to_owned(),
            "phx_join".into(),
            payload,
        ))?;

        loop {
            let message = self.read()?;
            if message.1.as_deref() == Some(reference.as_str()) && message.3 == "phx_reply" {
                let status = message.4.get("status").and_then(JsonValue::as_str);
                let response = message
                    .4
                    .get("response")
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                return match status {
                    Some("ok") => Ok(response),
                    _ => Err(BridgeError::HttpStatus {
                        status: 400,
                        body: message.4.to_string(),
                    }),
                };
            }
        }
    }

    pub fn push(&mut self, topic: &str, event: &str, payload: JsonValue) -> BridgeResult<String> {
        let reference = self.next_reference();
        self.send(PhoenixMessage(
            None,
            Some(reference.clone()),
            topic.to_owned(),
            event.to_owned(),
            payload,
        ))?;
        Ok(reference)
    }

    pub fn push_and_wait(
        &mut self,
        topic: &str,
        event: &str,
        payload: JsonValue,
    ) -> BridgeResult<JsonValue> {
        let reference = self.push(topic, event, payload)?;
        loop {
            let message = self.read()?;
            if message.1.as_deref() == Some(reference.as_str()) && message.3 == "phx_reply" {
                let status = message.4.get("status").and_then(JsonValue::as_str);
                let response = message
                    .4
                    .get("response")
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                return match status {
                    Some("ok") => Ok(response),
                    _ => Err(BridgeError::HttpStatus {
                        status: 400,
                        body: message.4.to_string(),
                    }),
                };
            }
        }
    }

    pub fn heartbeat(&mut self) -> BridgeResult<String> {
        self.push("phoenix", "heartbeat", json!({}))
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> BridgeResult<()> {
        match self.socket.get_mut() {
            MaybeTlsStream::Plain(stream) => stream.set_read_timeout(timeout)?,
            MaybeTlsStream::Rustls(stream) => stream.sock.set_read_timeout(timeout)?,
            #[allow(unreachable_patterns)]
            _ => {}
        }
        Ok(())
    }

    pub fn read(&mut self) -> BridgeResult<PhoenixMessage> {
        loop {
            match self.socket.read()? {
                Message::Text(text) => return Ok(serde_json::from_str(text.as_ref())?),
                Message::Binary(bytes) => return Ok(serde_json::from_slice(&bytes)?),
                Message::Ping(bytes) => self.socket.send(Message::Pong(bytes))?,
                Message::Close(_) => return Err(tungstenite::Error::ConnectionClosed.into()),
                _ => {}
            }
        }
    }

    pub fn read_timeout(&mut self, timeout: Duration) -> BridgeResult<Option<PhoenixMessage>> {
        self.set_read_timeout(Some(timeout))?;
        match self.read() {
            Ok(message) => Ok(Some(message)),
            Err(BridgeError::WebSocket(error))
                if matches!(
                    error.as_ref(),
                    tungstenite::Error::Io(io_error)
                        if matches!(
                            io_error.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        )
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub fn send(&mut self, message: PhoenixMessage) -> BridgeResult<()> {
        let raw = serde_json::to_string(&message)?;
        self.socket.send(Message::Text(raw.into()))?;
        Ok(())
    }

    fn next_reference(&mut self) -> String {
        self.next_ref = self.next_ref.saturating_add(1);
        self.next_ref.to_string()
    }
}

pub trait SessionVisibilityStore: Send + Sync {
    fn set_visible(&self, session_id: &str, visible: bool) -> BridgeResult<()>;
    fn is_visible(&self, session_id: &str) -> BridgeResult<bool>;
    fn visible_sessions(&self) -> BridgeResult<Vec<String>>;
}

#[derive(Debug, Clone, Default)]
pub struct MemorySessionVisibilityStore {
    sessions: Arc<Mutex<BTreeMap<String, bool>>>,
}

impl SessionVisibilityStore for MemorySessionVisibilityStore {
    fn set_visible(&self, session_id: &str, visible: bool) -> BridgeResult<()> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        sessions.insert(session_id.to_owned(), visible);
        Ok(())
    }

    fn is_visible(&self, session_id: &str) -> BridgeResult<bool> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        Ok(sessions.get(session_id).copied().unwrap_or(false))
    }

    fn visible_sessions(&self) -> BridgeResult<Vec<String>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        Ok(sessions
            .iter()
            .filter(|(_session_id, visible)| **visible)
            .map(|(session_id, _visible)| session_id.clone())
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FileVisibilityState {
    visible_sessions: BTreeMap<String, bool>,
}

#[derive(Debug, Clone)]
pub struct FileSessionVisibilityStore {
    path: PathBuf,
}

impl FileSessionVisibilityStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn load_state(&self) -> BridgeResult<FileVisibilityState> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(|source| BridgeError::StateDecode {
                path: self.path.clone(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(FileVisibilityState::default())
            }
            Err(source) => Err(BridgeError::StateRead {
                path: self.path.clone(),
                source,
            }),
        }
    }

    fn save_state(&self, state: &FileVisibilityState) -> BridgeResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| BridgeError::StateWrite {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let raw =
            serde_json::to_string_pretty(state).map_err(|source| BridgeError::StateDecode {
                path: self.path.clone(),
                source,
            })?;
        fs::write(&self.path, raw).map_err(|source| BridgeError::StateWrite {
            path: self.path.clone(),
            source,
        })
    }
}

impl SessionVisibilityStore for FileSessionVisibilityStore {
    fn set_visible(&self, session_id: &str, visible: bool) -> BridgeResult<()> {
        let mut state = self.load_state()?;
        if visible {
            state.visible_sessions.insert(session_id.to_owned(), true);
        } else {
            state.visible_sessions.remove(session_id);
        }
        self.save_state(&state)
    }

    fn is_visible(&self, session_id: &str) -> BridgeResult<bool> {
        Ok(self
            .load_state()?
            .visible_sessions
            .get(session_id)
            .copied()
            .unwrap_or(false))
    }

    fn visible_sessions(&self) -> BridgeResult<Vec<String>> {
        Ok(self
            .load_state()?
            .visible_sessions
            .into_iter()
            .filter(|(_session_id, visible)| *visible)
            .map(|(session_id, _visible)| session_id)
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectBackoff {
    base: Duration,
    cap: Duration,
    attempt: u32,
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(250),
            cap: Duration::from_secs(30),
            attempt: 0,
        }
    }
}

impl ReconnectBackoff {
    pub fn next_delay(&mut self) -> Duration {
        let exponent = self.attempt.min(7);
        self.attempt = self.attempt.saturating_add(1);
        let factor = 1_u32 << exponent;
        (self.base * factor).min(self.cap)
    }

    pub fn next_jittered_delay(&mut self) -> Duration {
        let delay = self.next_delay();
        let delay_ms = delay.as_millis().max(1);
        let jitter_window = (delay_ms / 4).max(1);
        let jitter_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() % jitter_window)
            .unwrap_or(0);
        (delay + Duration::from_millis(jitter_ms as u64)).min(self.cap)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

pub struct RemoteBridge<I, V> {
    config: BridgeConfig,
    identity_store: I,
    visibility_store: V,
    client: Client,
    connected: AtomicBool,
    seq_by_session: Mutex<HashMap<String, u64>>,
    replay_by_session: Mutex<HashMap<String, Vec<RuntimeEnvelope>>>,
    outbound_tx: mpsc::Sender<OutboundFrame>,
    outbound_rx: Mutex<mpsc::Receiver<OutboundFrame>>,
    inbound_tx: broadcast::Sender<InboundCommand>,
}

#[derive(Debug, Clone, PartialEq)]
struct OutboundFrame {
    session_id: String,
    payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopBridgeLoopOptions {
    pub heartbeat_interval: Duration,
    pub read_timeout: Duration,
}

impl Default for DesktopBridgeLoopOptions {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(30),
            read_timeout: Duration::from_millis(500),
        }
    }
}

const MAX_SESSION_REPLAY_FRAMES: usize = 512;
const RELAY_TOKEN_REFRESH_SKEW_SECONDS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RelayTokenRefreshAuth {
    Bearer(String),
    SessionId(String),
}

impl<I, V> RemoteBridge<I, V>
where
    I: IdentityStore,
    V: SessionVisibilityStore,
{
    pub fn new(config: BridgeConfig, identity_store: I, visibility_store: V) -> Self {
        let (inbound_tx, _inbound_rx) = broadcast::channel(256);
        let (outbound_tx, outbound_rx) = mpsc::channel();
        Self {
            config,
            identity_store,
            visibility_store,
            client: Client::new(),
            connected: AtomicBool::new(false),
            seq_by_session: Mutex::new(HashMap::new()),
            replay_by_session: Mutex::new(HashMap::new()),
            outbound_tx,
            outbound_rx: Mutex::new(outbound_rx),
            inbound_tx,
        }
    }

    pub fn status(&self) -> BridgeResult<BridgeStatus> {
        let identity = self.identity_store.load()?;
        let devices = match identity.as_ref() {
            Some(identity) if identity.desktop_jwt.is_some() => self.list_account_devices()?,
            _ => Vec::new(),
        };
        let account = identity.as_ref().and_then(identity_account);
        let signed_in = identity
            .as_ref()
            .and_then(|identity| identity.desktop_jwt.as_ref())
            .is_some();

        Ok(BridgeStatus {
            connected: self.connected.load(Ordering::Relaxed),
            relay_url: self.config.relay_url.clone(),
            signed_in,
            account,
            devices,
        })
    }

    pub fn sign_in_with_github(&self) -> BridgeResult<AuthStatus> {
        self.sign_in_with_github_kind("desktop")
    }

    /// Start a GitHub OAuth flow for an arbitrary device `kind`. Desktop uses
    /// `"desktop"`; the `xero mock-web` test harness uses `"web"`.
    pub fn sign_in_with_github_kind(&self, kind: &str) -> BridgeResult<AuthStatus> {
        let response = self
            .client
            .post(self.config.endpoint("/api/github/login")?)
            .json(&json!({
                "kind": kind,
                "name": self.config.device_name.as_deref().unwrap_or("Xero Desktop"),
            }))
            .send()?;
        let started: StartedGithubLogin = serde_json::from_value(decode_http(response)?)?;
        Ok(AuthStatus {
            signed_in: false,
            authorization_url: Some(started.authorization_url),
            flow_id: Some(started.flow_id),
            session_id: None,
            account_id: None,
            device_id: None,
            relay_token_expires_at: None,
            account: None,
        })
    }

    pub fn poll_github_login(&self, flow_id: &str) -> BridgeResult<AuthStatus> {
        let mut endpoint = self.config.endpoint("/api/github/session")?;
        endpoint.query_pairs_mut().append_pair("flowId", flow_id);
        let response = self.client.get(endpoint).send()?;
        let value = decode_http(response)?;
        let session: GithubFlowSession = serde_json::from_value(value)?;
        if session.status != "ready" {
            return Ok(AuthStatus {
                signed_in: false,
                authorization_url: None,
                flow_id: Some(flow_id.to_string()),
                session_id: None,
                account_id: None,
                device_id: None,
                relay_token_expires_at: None,
                account: None,
            });
        }
        let session_id = session
            .session_id
            .ok_or(BridgeError::MissingServerField("sessionId"))?;
        let session = session
            .session
            .ok_or(BridgeError::MissingServerField("session"))?;
        let identity = identity_from_github_session(session_id.clone(), session)?;
        let status = auth_status_from_identity(&identity);
        self.identity_store.save(&identity)?;
        Ok(status)
    }

    pub fn sign_out(&self) -> BridgeResult<()> {
        let Some(identity) = self.identity_store.load()? else {
            return Ok(());
        };
        if let Some(session_id) = identity.session_id.as_deref() {
            let response = self
                .client
                .delete(self.config.endpoint("/api/github/session")?)
                .header("x-xero-github-session-id", session_id)
                .send()?;
            let _ignored = decode_http_allow_empty(response)?;
        }
        self.identity_store.save(&DesktopIdentity::generate())?;
        Ok(())
    }

    pub fn refresh_relay_token(&self) -> BridgeResult<Option<String>> {
        let Some(mut identity) = self.identity_store.load()? else {
            return Ok(None);
        };
        let Some(auth) = relay_token_refresh_auth(&identity) else {
            return Ok(None);
        };

        let request = self
            .client
            .post(self.config.endpoint("/api/relay/token/refresh")?);
        let request = match auth {
            RelayTokenRefreshAuth::Bearer(token) => request.bearer_auth(token),
            RelayTokenRefreshAuth::SessionId(session_id) => {
                request.header("x-xero-github-session-id", session_id)
            }
        };
        let response = request.send()?;
        let value = decode_http(response)?;
        let relay_token = required_server_string(&value, "relayToken")?;
        identity.desktop_jwt = Some(relay_token.clone());
        identity.relay_token_expires_at =
            value.get("relayTokenExpiresAt").and_then(JsonValue::as_i64);
        self.identity_store.save(&identity)?;
        Ok(Some(relay_token))
    }

    pub fn connect_desktop_channel(&self) -> BridgeResult<DesktopRelayConnection> {
        self.ensure_fresh_relay_token()?;
        let identity = self.ensure_registered()?;
        let desktop_device_id = identity
            .desktop_device_id
            .clone()
            .ok_or(BridgeError::MissingServerField("desktop_device_id"))?;
        let jwt = identity
            .desktop_jwt
            .as_deref()
            .ok_or(BridgeError::MissingServerField("desktop_jwt"))?;
        let client = PhoenixChannelClient::connect(&self.config, jwt, PhoenixSocketKind::Desktop)?;
        Ok(DesktopRelayConnection {
            desktop_device_id,
            client,
        })
    }

    pub fn list_account_devices(&self) -> BridgeResult<Vec<AccountDevice>> {
        let Some(identity) = self.identity_store.load()? else {
            return Ok(Vec::new());
        };
        let Some(jwt) = identity.desktop_jwt else {
            return Ok(Vec::new());
        };

        let response = self
            .client
            .get(self.config.endpoint("/api/devices")?)
            .bearer_auth(jwt)
            .send()?;
        let value: JsonValue = decode_http(response)?;
        Ok(
            serde_json::from_value(value.get("devices").cloned().unwrap_or_else(|| json!([])))
                .unwrap_or_default(),
        )
    }

    pub fn revoke_device(&self, device_id: &str) -> BridgeResult<()> {
        let Some(identity) = self.identity_store.load()? else {
            return Ok(());
        };
        let Some(jwt) = identity.desktop_jwt else {
            return Ok(());
        };
        let response = self
            .client
            .post(
                self.config
                    .endpoint(&format!("/api/devices/{device_id}/revoke"))?,
            )
            .bearer_auth(jwt)
            .send()?;
        let _ignored: JsonValue = decode_http_allow_empty(response)?;
        Ok(())
    }

    pub fn set_session_visibility(&self, session_id: &str, visible: bool) -> BridgeResult<()> {
        self.visibility_store.set_visible(session_id, visible)
    }

    pub fn subscribe_inbound(&self) -> broadcast::Receiver<InboundCommand> {
        self.inbound_tx.subscribe()
    }

    pub fn forward(
        &self,
        session_id: &str,
        runtime_event: JsonValue,
    ) -> BridgeResult<Option<Vec<u8>>> {
        if !self.visibility_store.is_visible(session_id)? {
            return Ok(None);
        }

        let identity = self.ensure_registered()?;
        let computer_id = identity.desktop_device_id.unwrap_or_default();
        let seq = self.next_seq(session_id)?;
        let envelope = RuntimeEnvelope {
            v: 1,
            seq,
            computer_id,
            session_id: session_id.to_owned(),
            kind: EnvelopeKind::Event,
            payload: runtime_event,
        };

        self.record_envelope(envelope.clone())?;
        let bytes = encode_envelope(&envelope)?;
        self.enqueue_envelope(&envelope)?;
        Ok(Some(bytes))
    }

    pub fn forward_payload(
        &self,
        session_id: &str,
        runtime_event: JsonValue,
    ) -> BridgeResult<Option<JsonValue>> {
        let Some(bytes) = self.forward(session_id, runtime_event)? else {
            return Ok(None);
        };
        let envelope = decode_envelope(&bytes)?;
        encode_relay_frame_payload(&envelope).map(Some)
    }

    pub fn snapshot(&self, session_id: &str, snapshot: JsonValue) -> BridgeResult<Option<Vec<u8>>> {
        if !self.visibility_store.is_visible(session_id)? {
            return Ok(None);
        }

        let identity = self.ensure_registered()?;
        let computer_id = identity.desktop_device_id.unwrap_or_default();
        let seq = self.next_seq(session_id)?;
        let envelope = RuntimeEnvelope {
            v: 1,
            seq,
            computer_id,
            session_id: session_id.to_owned(),
            kind: EnvelopeKind::Snapshot,
            payload: snapshot,
        };
        self.record_envelope(envelope.clone())?;
        let bytes = encode_envelope(&envelope)?;
        self.enqueue_envelope(&envelope)?;
        Ok(Some(bytes))
    }

    pub fn forward_control_event(
        &self,
        session_id: &str,
        runtime_event: JsonValue,
    ) -> BridgeResult<JsonValue> {
        let identity = self.ensure_registered()?;
        let computer_id = identity.desktop_device_id.unwrap_or_default();
        let seq = self.next_seq(session_id)?;
        let envelope = RuntimeEnvelope {
            v: 1,
            seq,
            computer_id,
            session_id: session_id.to_owned(),
            kind: EnvelopeKind::Event,
            payload: runtime_event,
        };
        self.record_envelope(envelope.clone())?;
        self.enqueue_envelope(&envelope)?;
        encode_relay_frame_payload(&envelope)
    }

    pub fn queue_replay_after(&self, session_id: &str, last_seq: u64) -> BridgeResult<usize> {
        let replay = self.replay_after(session_id, last_seq)?;
        let count = replay.len();
        for envelope in replay {
            self.enqueue_envelope(&envelope)?;
        }
        Ok(count)
    }

    pub fn replay_payloads_after(
        &self,
        session_id: &str,
        last_seq: u64,
    ) -> BridgeResult<Vec<JsonValue>> {
        let replay = self.replay_after(session_id, last_seq)?;
        replay
            .iter()
            .map(encode_relay_frame_payload)
            .collect::<BridgeResult<Vec<_>>>()
    }

    pub fn replay_after(
        &self,
        session_id: &str,
        last_seq: u64,
    ) -> BridgeResult<Vec<RuntimeEnvelope>> {
        let replay = self
            .replay_by_session
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        Ok(replay
            .get(session_id)
            .map(|frames| {
                frames
                    .iter()
                    .filter(|frame| frame.seq > last_seq)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn run_desktop_loop(
        &self,
        shutdown: Arc<AtomicBool>,
        options: DesktopBridgeLoopOptions,
    ) -> BridgeResult<()> {
        let mut backoff = ReconnectBackoff::default();
        while !shutdown.load(Ordering::Relaxed) {
            match self.run_desktop_once(&shutdown, &options) {
                Ok(()) => {
                    self.connected.store(false, Ordering::Relaxed);
                    backoff.reset();
                }
                Err(_error) if !shutdown.load(Ordering::Relaxed) => {
                    self.connected.store(false, Ordering::Relaxed);
                    thread::sleep(backoff.next_jittered_delay());
                }
                Err(error) => {
                    self.connected.store(false, Ordering::Relaxed);
                    return Err(error);
                }
            }
        }
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn spawn_desktop_loop(
        self: Arc<Self>,
        shutdown: Arc<AtomicBool>,
        options: DesktopBridgeLoopOptions,
    ) -> thread::JoinHandle<BridgeResult<()>>
    where
        I: 'static,
        V: 'static,
    {
        thread::spawn(move || self.run_desktop_loop(shutdown, options))
    }

    fn ensure_registered(&self) -> BridgeResult<DesktopIdentity> {
        match self.identity_store.load()? {
            Some(identity) if identity.desktop_jwt.is_some() => Ok(identity),
            _ => Err(BridgeError::MissingServerField("desktop_jwt")),
        }
    }

    fn ensure_fresh_relay_token(&self) -> BridgeResult<()> {
        let Some(identity) = self.identity_store.load()? else {
            return Ok(());
        };
        if relay_token_needs_refresh_for_identity(
            &identity,
            OffsetDateTime::now_utc().unix_timestamp(),
        ) {
            let _ = self.refresh_relay_token()?;
        }
        Ok(())
    }

    fn next_seq(&self, session_id: &str) -> BridgeResult<u64> {
        let mut seqs = self
            .seq_by_session
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        let next = seqs.get(session_id).copied().unwrap_or(0) + 1;
        seqs.insert(session_id.to_owned(), next);
        Ok(next)
    }

    fn record_envelope(&self, envelope: RuntimeEnvelope) -> BridgeResult<()> {
        let mut replay = self
            .replay_by_session
            .lock()
            .map_err(|_| BridgeError::LockPoisoned)?;
        let frames = replay.entry(envelope.session_id.clone()).or_default();
        frames.push(envelope);
        if frames.len() > MAX_SESSION_REPLAY_FRAMES {
            let drain_count = frames.len() - MAX_SESSION_REPLAY_FRAMES;
            frames.drain(0..drain_count);
        }
        Ok(())
    }

    fn enqueue_envelope(&self, envelope: &RuntimeEnvelope) -> BridgeResult<()> {
        let payload = encode_relay_frame_payload(envelope)?;
        let _ = self.outbound_tx.send(OutboundFrame {
            session_id: envelope.session_id.clone(),
            payload,
        });
        Ok(())
    }

    fn run_desktop_once(
        &self,
        shutdown: &AtomicBool,
        options: &DesktopBridgeLoopOptions,
    ) -> BridgeResult<()> {
        let mut connection = self.connect_desktop_channel()?;
        connection.set_read_timeout(Some(options.read_timeout))?;
        connection.join_control()?;
        let mut joined_sessions = BTreeSet::new();
        for session_id in self.visibility_store.visible_sessions()? {
            connection.join_session(&session_id)?;
            joined_sessions.insert(session_id);
        }

        self.connected.store(true, Ordering::Relaxed);
        let mut last_heartbeat = Instant::now();
        while !shutdown.load(Ordering::Relaxed) {
            if self.relay_token_needs_refresh()? {
                let _ = self.refresh_relay_token()?;
            }
            if last_heartbeat.elapsed() >= options.heartbeat_interval {
                connection.heartbeat()?;
                last_heartbeat = Instant::now();
            }
            if let Some(message) = connection.read_timeout(options.read_timeout)? {
                self.handle_desktop_message(&mut connection, &mut joined_sessions, message)?;
            }
            self.drain_outbound(&mut connection, &mut joined_sessions)?;
        }
        Ok(())
    }

    fn relay_token_needs_refresh(&self) -> BridgeResult<bool> {
        let Some(identity) = self.identity_store.load()? else {
            return Ok(false);
        };
        Ok(relay_token_needs_refresh_for_identity(
            &identity,
            OffsetDateTime::now_utc().unix_timestamp(),
        ))
    }

    fn handle_desktop_message(
        &self,
        connection: &mut DesktopRelayConnection,
        joined_sessions: &mut BTreeSet<String>,
        message: PhoenixMessage,
    ) -> BridgeResult<()> {
        match message.3.as_str() {
            "session_join_requested" => {
                let session_id = required_json_string(&message.4, "session_id")?;
                let join_ref = required_json_string(&message.4, "join_ref")?;
                let auth_topic = required_json_string(&message.4, "auth_topic")?;
                let authorized = is_control_session_id(session_id)
                    || self.visibility_store.is_visible(session_id)?;
                connection.authorize_session_join(join_ref, auth_topic, authorized)?;
                if authorized {
                    let _reply = connection.join_session(session_id)?;
                    joined_sessions.insert(session_id.to_owned());
                }
            }
            "session_attached" => {
                let session_id = required_json_string(&message.4, "session_id")?;
                let web_device_id = required_json_string(&message.4, "web_device_id")?;
                let last_seq = message
                    .4
                    .get("last_seq")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(0);
                let _ = self.inbound_tx.send(InboundCommand {
                    v: 1,
                    seq: 0,
                    computer_id: connection.desktop_device_id.clone(),
                    session_id: Some(session_id.to_owned()),
                    kind: InboundCommandKind::SessionAttached,
                    device_id: web_device_id.to_owned(),
                    payload: json!({ "lastSeq": last_seq }),
                });
            }
            "frame" => {
                if message
                    .4
                    .get("from_kind")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|kind| kind == "web")
                {
                    if let Some(payload) = message.4.get("payload").cloned() {
                        if let Ok(mut command) = serde_json::from_value::<InboundCommand>(payload) {
                            if command.session_id.is_none() {
                                command.session_id = session_id_from_topic(&message.2);
                            }
                            let _ = self.inbound_tx.send(command);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn drain_outbound(
        &self,
        connection: &mut DesktopRelayConnection,
        joined_sessions: &mut BTreeSet<String>,
    ) -> BridgeResult<()> {
        loop {
            let frame = {
                let receiver = self
                    .outbound_rx
                    .lock()
                    .map_err(|_| BridgeError::LockPoisoned)?;
                match receiver.try_recv() {
                    Ok(frame) => frame,
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            };
            if joined_sessions.insert(frame.session_id.clone()) {
                connection.join_session(&frame.session_id)?;
            }
            connection.push_session_frame(&frame.session_id, frame.payload)?;
        }
        Ok(())
    }
}

fn decode_http(response: reqwest::blocking::Response) -> BridgeResult<JsonValue> {
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        return Err(BridgeError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    Ok(serde_json::from_str(&body).unwrap_or(JsonValue::Null))
}

fn decode_http_allow_empty(response: reqwest::blocking::Response) -> BridgeResult<JsonValue> {
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        return Err(BridgeError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    if body.trim().is_empty() {
        Ok(JsonValue::Null)
    } else {
        Ok(serde_json::from_str(&body).unwrap_or(JsonValue::Null))
    }
}

fn required_server_string(value: &JsonValue, key: &'static str) -> BridgeResult<String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or(BridgeError::MissingServerField(key))
}

fn identity_from_github_session(
    session_id: String,
    session: GithubSessionView,
) -> BridgeResult<DesktopIdentity> {
    let account_id = session
        .account_id
        .ok_or(BridgeError::MissingServerField("accountId"))?;
    let device_id = session
        .device_id
        .ok_or(BridgeError::MissingServerField("deviceId"))?;
    let relay_token = session
        .relay_token
        .ok_or(BridgeError::MissingServerField("relayToken"))?;
    let github_login = session
        .account
        .as_ref()
        .and_then(|account| account.github_login.clone())
        .or_else(|| session.user.as_ref().and_then(|user| user.login.clone()));
    let github_avatar_url = session
        .account
        .as_ref()
        .and_then(|account| account.github_avatar_url.clone())
        .or_else(|| {
            session
                .user
                .as_ref()
                .and_then(|user| user.avatar_url.clone())
        });

    Ok(DesktopIdentity {
        account_id: Some(account_id),
        desktop_device_id: Some(device_id),
        desktop_jwt: Some(relay_token),
        session_id: Some(session_id),
        relay_token_expires_at: session.relay_token_expires_at,
        github_login,
        github_avatar_url,
    })
}

fn auth_status_from_identity(identity: &DesktopIdentity) -> AuthStatus {
    AuthStatus {
        signed_in: identity.desktop_jwt.is_some(),
        authorization_url: None,
        flow_id: None,
        session_id: identity.session_id.clone(),
        account_id: identity.account_id.clone(),
        device_id: identity.desktop_device_id.clone(),
        relay_token_expires_at: identity.relay_token_expires_at,
        account: identity_account(identity),
    }
}

fn identity_account(identity: &DesktopIdentity) -> Option<BridgeAccount> {
    if identity.github_login.is_none() && identity.github_avatar_url.is_none() {
        return None;
    }
    Some(BridgeAccount {
        github_login: identity.github_login.clone(),
        avatar_url: identity.github_avatar_url.clone(),
    })
}

fn relay_token_needs_refresh_for_identity(identity: &DesktopIdentity, now: i64) -> bool {
    matches!(
        identity.relay_token_expires_at,
        Some(expires_at)
            if expires_at.saturating_sub(RELAY_TOKEN_REFRESH_SKEW_SECONDS) <= now
    )
}

fn relay_token_is_expired_for_identity(identity: &DesktopIdentity, now: i64) -> bool {
    matches!(identity.relay_token_expires_at, Some(expires_at) if expires_at <= now)
}

fn relay_token_refresh_auth(identity: &DesktopIdentity) -> Option<RelayTokenRefreshAuth> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let bearer = identity
        .desktop_jwt
        .as_deref()
        .filter(|token| !token.trim().is_empty())
        .map(|token| RelayTokenRefreshAuth::Bearer(token.to_owned()));
    let session = identity
        .session_id
        .as_deref()
        .filter(|session_id| !session_id.trim().is_empty())
        .map(|session_id| RelayTokenRefreshAuth::SessionId(session_id.to_owned()));

    if relay_token_is_expired_for_identity(identity, now) {
        session.or(bearer)
    } else {
        bearer.or(session)
    }
}

fn required_json_string<'a>(value: &'a JsonValue, key: &'static str) -> BridgeResult<&'a str> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(BridgeError::MissingServerField(key))
}

fn is_control_session_id(session_id: &str) -> bool {
    matches!(session_id, "__sessions__" | "__new__")
}

fn session_id_from_topic(topic: &str) -> Option<String> {
    topic
        .rsplit_once(':')
        .and_then(|(_, session_id)| (!session_id.trim().is_empty()).then(|| session_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn envelope_msgpack_round_trips() {
        let envelope = RuntimeEnvelope {
            v: 1,
            seq: 42,
            computer_id: "desktop-1".into(),
            session_id: "session-1".into(),
            kind: EnvelopeKind::Event,
            payload: json!({"kind": "message", "body": "hello"}),
        };

        let encoded = encode_envelope(&envelope).expect("encode");
        let decoded = decode_envelope(&encoded).expect("decode");

        assert_eq!(decoded, envelope);
    }

    #[test]
    fn relay_url_prefers_remote_env_and_trims_trailing_slash() {
        let url = relay_url_from_env_values(
            Some(OsString::from("https://relay.example.com/")),
            Some(OsString::from("https://server.example.com")),
        );

        assert_eq!(url, "https://relay.example.com");
    }

    #[test]
    fn relay_url_accepts_desktop_server_env_fallback() {
        let url = relay_url_from_env_values(
            None,
            Some(OsString::from("https://desktop-server.example.com/")),
        );

        assert_eq!(url, "https://desktop-server.example.com");
    }

    #[test]
    fn relay_url_falls_back_to_local_default() {
        let url = relay_url_from_env_values(None, None);

        assert_eq!(url, BridgeConfig::LOCAL_RELAY_URL);
    }

    #[test]
    fn relay_token_refresh_decision_uses_expiry_skew() {
        let now = 1_778_890_000;
        let mut identity = test_identity();

        identity.relay_token_expires_at = Some(now + RELAY_TOKEN_REFRESH_SKEW_SECONDS + 1);
        assert!(!relay_token_needs_refresh_for_identity(&identity, now));

        identity.relay_token_expires_at = Some(now + RELAY_TOKEN_REFRESH_SKEW_SECONDS);
        assert!(relay_token_needs_refresh_for_identity(&identity, now));

        identity.relay_token_expires_at = None;
        assert!(!relay_token_needs_refresh_for_identity(&identity, now));
    }

    #[test]
    fn expired_relay_token_refresh_uses_session_auth_before_bearer() {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut identity = test_identity();

        identity.relay_token_expires_at = Some(now - 1);
        assert_eq!(
            relay_token_refresh_auth(&identity),
            Some(RelayTokenRefreshAuth::SessionId("session-1".into()))
        );

        identity.session_id = None;
        assert_eq!(
            relay_token_refresh_auth(&identity),
            Some(RelayTokenRefreshAuth::Bearer("token".into()))
        );

        identity.session_id = Some("session-1".into());
        identity.relay_token_expires_at = Some(now + 3600);
        assert_eq!(
            relay_token_refresh_auth(&identity),
            Some(RelayTokenRefreshAuth::Bearer("token".into()))
        );
    }

    #[test]
    fn bridge_forward_gates_by_remote_visibility() {
        let temp = tempfile_path("bridge-forward");
        let identity_store = FileIdentityStore::new(temp.join("identity.json"));
        identity_store
            .save(&DesktopIdentity {
                account_id: Some("account-1".into()),
                desktop_device_id: Some("desktop-1".into()),
                desktop_jwt: Some("token".into()),
                session_id: Some("session-1".into()),
                relay_token_expires_at: None,
                github_login: Some("octo".into()),
                github_avatar_url: None,
            })
            .expect("identity");
        let visibility = MemorySessionVisibilityStore::default();
        let bridge = RemoteBridge::new(BridgeConfig::local_default(), identity_store, visibility);

        assert!(bridge
            .forward("session-1", json!({"event": 1}))
            .expect("hidden gate")
            .is_none());

        bridge
            .set_session_visibility("session-1", true)
            .expect("visible");
        let first = bridge
            .forward("session-1", json!({"event": 1}))
            .expect("forward")
            .expect("bytes");
        let second = bridge
            .forward("session-1", json!({"event": 2}))
            .expect("forward")
            .expect("bytes");

        assert_eq!(decode_envelope(&first).expect("first").seq, 1);
        assert_eq!(decode_envelope(&second).expect("second").seq, 2);

        let replay = bridge.replay_after("session-1", 1).expect("replay");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, 2);

        let relay_payloads = bridge
            .replay_payloads_after("session-1", 0)
            .expect("relay payloads");
        assert_eq!(relay_payloads.len(), 2);
        assert_eq!(relay_payloads[0]["encoding"], "msgpack.base64url");

        let receiver = bridge.outbound_rx.lock().expect("outbound lock");
        let queued_first = receiver.try_recv().expect("queued first");
        let queued_second = receiver.try_recv().expect("queued second");
        assert_eq!(queued_first.session_id, "session-1");
        assert_eq!(queued_first.payload["seq"], 1);
        assert_eq!(queued_second.payload["seq"], 2);
    }

    #[test]
    fn snapshot_emits_snapshot_envelope_when_session_is_visible() {
        let temp = tempfile_path("bridge-snapshot");
        let identity_store = FileIdentityStore::new(temp.join("identity.json"));
        identity_store
            .save(&DesktopIdentity {
                account_id: Some("account-1".into()),
                desktop_device_id: Some("desktop-1".into()),
                desktop_jwt: Some("token".into()),
                session_id: Some("session-1".into()),
                relay_token_expires_at: None,
                github_login: Some("octo".into()),
                github_avatar_url: None,
            })
            .expect("identity");
        let visibility = MemorySessionVisibilityStore::default();
        let bridge =
            RemoteBridge::new(BridgeConfig::local_default(), identity_store, visibility);

        // Hidden session -> snapshot is a no-op.
        let hidden = bridge
            .snapshot("session-1", json!({ "schema": "x.snapshot.v1" }))
            .expect("hidden snapshot");
        assert!(hidden.is_none());

        bridge
            .set_session_visibility("session-1", true)
            .expect("visible");

        // Visible session -> snapshot returns an encoded envelope keyed to
        // the session and tagged as `Snapshot` (the wire kind a web client
        // uses to seed its store before live events arrive).
        let bytes = bridge
            .snapshot(
                "session-1",
                json!({
                    "schema": "xero.remote_session_snapshot.v1",
                    "session": { "agentSessionId": "session-1" },
                }),
            )
            .expect("snapshot ok")
            .expect("snapshot bytes");

        let envelope = decode_envelope(&bytes).expect("decode");
        assert_eq!(envelope.kind, EnvelopeKind::Snapshot);
        assert_eq!(envelope.session_id, "session-1");
        assert_eq!(envelope.computer_id, "desktop-1");
        assert_eq!(envelope.v, 1);
        assert_eq!(
            envelope.payload.get("schema").and_then(|v| v.as_str()),
            Some("xero.remote_session_snapshot.v1")
        );

        // Subsequent forward bumps the sequence number, proving the snapshot
        // anchors the seq stream that the web client uses for replay.
        let payload = bridge
            .forward("session-1", json!({"event": "noop"}))
            .expect("forward")
            .expect("forward bytes");
        let next_envelope = decode_envelope(&payload).expect("decode forward");
        assert!(next_envelope.seq > envelope.seq);
    }

    #[test]
    fn file_visibility_store_persists_visible_sessions() {
        let path = tempfile_path("visibility").join("state.json");
        let store = FileSessionVisibilityStore::new(&path);

        assert!(!store.is_visible("session-1").expect("default hidden"));
        store.set_visible("session-1", true).expect("show");

        let reloaded = FileSessionVisibilityStore::new(&path);
        assert!(reloaded.is_visible("session-1").expect("reloaded"));
        assert_eq!(
            reloaded.visible_sessions().expect("visible sessions"),
            vec!["session-1".to_string()]
        );
        reloaded.set_visible("session-1", false).expect("hide");
        assert!(!store.is_visible("session-1").expect("hidden"));
    }

    #[test]
    fn reconnect_backoff_increases_and_resets() {
        let mut backoff = ReconnectBackoff::default();
        assert_eq!(backoff.next_delay(), Duration::from_millis(250));
        assert_eq!(backoff.next_delay(), Duration::from_millis(500));
        assert_eq!(backoff.next_delay(), Duration::from_millis(1000));
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(250));

        let mut jittered = ReconnectBackoff::default();
        let jittered_delay = jittered.next_jittered_delay();
        assert!(jittered_delay >= Duration::from_millis(250));
        assert!(jittered_delay <= Duration::from_millis(312));
    }

    fn tempfile_path(name: &str) -> PathBuf {
        let unique = format!(
            "xero-remote-bridge-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    fn test_identity() -> DesktopIdentity {
        DesktopIdentity {
            account_id: Some("account-1".into()),
            desktop_device_id: Some("desktop-1".into()),
            desktop_jwt: Some("token".into()),
            session_id: Some("session-1".into()),
            relay_token_expires_at: None,
            github_login: Some("octo".into()),
            github_avatar_url: None,
        }
    }
}
