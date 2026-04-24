//! Events emitted by the Solana workbench backend. Shape mirrors the
//! emulator sidebar so the frontend hook can listen in the same way.

use serde::{Deserialize, Serialize};

pub const SOLANA_VALIDATOR_STATUS_EVENT: &str = "solana:validator:status";
pub const SOLANA_VALIDATOR_LOG_EVENT: &str = "solana:validator:log";
pub const SOLANA_TOOLCHAIN_STATUS_CHANGED_EVENT: &str = "solana:toolchain:changed";
pub const SOLANA_RPC_HEALTH_EVENT: &str = "solana:rpc:health";
pub const SOLANA_PERSONA_EVENT: &str = "solana:persona";
pub const SOLANA_SCENARIO_EVENT: &str = "solana:scenario";
pub const SOLANA_TX_EVENT: &str = "solana:tx";
pub const SOLANA_IDL_CHANGED_EVENT: &str = "solana:idl:changed";
pub const SOLANA_DEPLOY_PROGRESS_EVENT: &str = "solana:deploy:progress";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorPhase {
    Idle,
    Booting,
    Ready,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorStatusPayload {
    pub phase: ValidatorPhase,
    pub kind: Option<String>,
    pub rpc_url: Option<String>,
    pub ws_url: Option<String>,
    pub message: Option<String>,
}

impl ValidatorStatusPayload {
    pub fn new(phase: ValidatorPhase) -> Self {
        Self {
            phase,
            kind: None,
            rpc_url: None,
            ws_url: None,
            message: None,
        }
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn with_rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    pub fn with_ws_url(mut self, url: impl Into<String>) -> Self {
        self.ws_url = Some(url.into());
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorLogPayload {
    pub level: ValidatorLogLevel,
    pub message: String,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PersonaEventKind {
    Created,
    Updated,
    Funded,
    Deleted,
    Imported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonaEventPayload {
    pub kind: PersonaEventKind,
    pub cluster: String,
    pub name: String,
    pub pubkey: Option<String>,
    pub ts_ms: u64,
    pub message: Option<String>,
}

impl PersonaEventPayload {
    pub fn new(kind: PersonaEventKind, cluster: &str, name: &str) -> Self {
        Self {
            kind,
            cluster: cluster.to_string(),
            name: name.to_string(),
            pubkey: None,
            ts_ms: now_ms(),
            message: None,
        }
    }

    pub fn with_pubkey(mut self, pubkey: impl Into<String>) -> Self {
        self.pubkey = Some(pubkey.into());
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TxEventKind {
    Building,
    Simulated,
    Sent,
    Confirmed,
    Failed,
    Decoded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TxEventPayload {
    pub kind: TxEventKind,
    pub cluster: String,
    pub signature: Option<String>,
    pub summary: Option<String>,
    pub ts_ms: u64,
}

impl TxEventPayload {
    pub fn new(kind: TxEventKind, cluster: &str) -> Self {
        Self {
            kind,
            cluster: cluster.to_string(),
            signature: None,
            summary: None,
            ts_ms: now_ms(),
        }
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioEventKind {
    Started,
    Progress,
    Completed,
    Failed,
    PendingPipeline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioEventPayload {
    pub kind: ScenarioEventKind,
    pub id: String,
    pub cluster: String,
    pub persona: String,
    pub ts_ms: u64,
    pub message: Option<String>,
    pub signature_count: u32,
}

impl ScenarioEventPayload {
    pub fn new(kind: ScenarioEventKind, id: &str, cluster: &str, persona: &str) -> Self {
        Self {
            kind,
            id: id.to_string(),
            cluster: cluster.to_string(),
            persona: persona.to_string(),
            ts_ms: now_ms(),
            message: None,
            signature_count: 0,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn with_signature_count(mut self, count: u32) -> Self {
        self.signature_count = count;
        self
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
