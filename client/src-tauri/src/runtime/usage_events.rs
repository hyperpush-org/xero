//! Bridge for emitting frontend events from code paths that don't have an
//! `AppHandle` in scope (e.g. the provider loop deep in a worker thread).
//!
//! At app boot, `lib.rs` registers a closure that captures the global
//! `AppHandle` and forwards calls to `tauri::Emitter::emit`. The provider loop
//! then calls `emit_agent_usage_updated()` after persisting a usage row, and
//! the frontend (`useXeroDesktopState`) listens on `agent_usage_updated`
//! to refresh the spend totals shown in the footer + sidebar.

use std::sync::OnceLock;

use serde::Serialize;

/// Payload delivered to the frontend `agent_usage_updated` event listener.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageUpdatedPayload {
    pub project_id: String,
    pub run_id: String,
}

pub const AGENT_USAGE_UPDATED_EVENT: &str = "agent_usage_updated";

type Emitter = Box<dyn Fn(AgentUsageUpdatedPayload) + Send + Sync + 'static>;

static EMITTER: OnceLock<Emitter> = OnceLock::new();

/// Install the global emitter. Idempotent: subsequent calls are ignored.
/// Called once from `lib.rs` setup with a closure that captures `AppHandle`.
pub fn set_usage_event_emitter<F>(emitter: F)
where
    F: Fn(AgentUsageUpdatedPayload) + Send + Sync + 'static,
{
    let _ = EMITTER.set(Box::new(emitter));
}

/// Best-effort fire-and-forget. If no emitter is registered (tests that don't
/// boot a real Tauri app) this is a no-op.
pub fn emit_agent_usage_updated(project_id: &str, run_id: &str) {
    let Some(emitter) = EMITTER.get() else {
        return;
    };
    emitter(AgentUsageUpdatedPayload {
        project_id: project_id.to_string(),
        run_id: run_id.to_string(),
    });
}
