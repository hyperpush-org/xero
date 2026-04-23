//! Solana workbench backend — Phase 1.
//!
//! Mirrors the `emulator` module layout: a single `SolanaState` held as
//! Tauri state, a narrow set of JSON-in/JSON-out commands, and events
//! emitted onto well-known channel names. Everything is designed so a
//! future autonomous-runtime tool wrapper can drive the same surface that
//! the UI drives.

pub mod cluster;
pub mod events;
pub mod rpc_router;
pub mod snapshot;
pub mod toolchain;
pub mod validator;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::commands::{CommandError, CommandResult};

pub use cluster::{descriptors as cluster_descriptors, ClusterDescriptor, ClusterKind};
pub use events::{
    ValidatorLogLevel, ValidatorLogPayload, ValidatorPhase, ValidatorStatusPayload,
    SOLANA_RPC_HEALTH_EVENT, SOLANA_TOOLCHAIN_STATUS_CHANGED_EVENT, SOLANA_VALIDATOR_LOG_EVENT,
    SOLANA_VALIDATOR_STATUS_EVENT,
};
pub use rpc_router::{EndpointHealth, EndpointSpec, RpcRouter};
pub use snapshot::{
    AccountFetcher, AccountRecord, RpcAccountFetcher, SnapshotManifest, SnapshotMeta, SnapshotStore,
};
pub use toolchain::{ToolProbe, ToolchainStatus};
pub use validator::{
    ClusterHandle, ClusterStatus, StartOpts, ValidatorLauncher, ValidatorSession,
    ValidatorSupervisor,
};

/// Process-wide Solana state. Registered alongside `EmulatorState` in the
/// Tauri builder.
pub struct SolanaState {
    supervisor: Arc<ValidatorSupervisor>,
    rpc_router: Arc<RpcRouter>,
    snapshots: Arc<SnapshotStore>,
}

impl Default for SolanaState {
    fn default() -> Self {
        let snapshots = SnapshotStore::with_default_root(Box::new(RpcAccountFetcher))
            .unwrap_or_else(|_| {
                // Fall back to an in-temp scratch dir if the OS data dir
                // can't be resolved so the app still boots.
                let scratch = std::env::temp_dir().join("cadence-solana-snapshots");
                SnapshotStore::new(scratch, Box::new(RpcAccountFetcher))
            });
        Self {
            supervisor: Arc::new(ValidatorSupervisor::with_default_launcher()),
            rpc_router: Arc::new(RpcRouter::new_with_default_pool()),
            snapshots: Arc::new(snapshots),
        }
    }
}

impl SolanaState {
    pub fn new(
        supervisor: Arc<ValidatorSupervisor>,
        rpc_router: Arc<RpcRouter>,
        snapshots: Arc<SnapshotStore>,
    ) -> Self {
        Self {
            supervisor,
            rpc_router,
            snapshots,
        }
    }

    pub fn supervisor(&self) -> Arc<ValidatorSupervisor> {
        Arc::clone(&self.supervisor)
    }

    pub fn rpc_router(&self) -> Arc<RpcRouter> {
        Arc::clone(&self.rpc_router)
    }

    pub fn snapshots(&self) -> Arc<SnapshotStore> {
        Arc::clone(&self.snapshots)
    }
}

// ---------- Tauri commands --------------------------------------------------

#[tauri::command]
pub fn solana_toolchain_status() -> CommandResult<ToolchainStatus> {
    Ok(toolchain::probe())
}

#[tauri::command]
pub fn solana_cluster_list() -> CommandResult<Vec<ClusterDescriptor>> {
    Ok(cluster_descriptors())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterStartRequest {
    pub kind: ClusterKind,
    #[serde(default)]
    pub opts: StartOpts,
}

#[tauri::command]
pub fn solana_cluster_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
    request: ClusterStartRequest,
) -> CommandResult<ClusterHandle> {
    let (handle, events) = state.supervisor.start(request.kind, request.opts)?;
    for payload in events {
        let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    }
    Ok(handle)
}

#[tauri::command]
pub fn solana_cluster_stop<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
) -> CommandResult<()> {
    let events = state.supervisor.stop()?;
    for payload in events {
        let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    }
    Ok(())
}

#[tauri::command]
pub fn solana_cluster_status(state: State<'_, SolanaState>) -> CommandResult<ClusterStatus> {
    Ok(state.supervisor.status())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotCreateRequest {
    pub label: String,
    pub accounts: Vec<String>,
    #[serde(default)]
    pub cluster: Option<ClusterKind>,
    #[serde(default)]
    pub rpc_url: Option<String>,
}

#[tauri::command]
pub fn solana_snapshot_create(
    state: State<'_, SolanaState>,
    request: SnapshotCreateRequest,
) -> CommandResult<SnapshotMeta> {
    if request.accounts.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_snapshot_accounts_empty",
            "At least one account pubkey is required to create a snapshot.",
        ));
    }

    let status = state.supervisor.status();
    let cluster_label = request
        .cluster
        .map(|c| c.as_str().to_string())
        .or_else(|| status.kind.map(|c| c.as_str().to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let rpc_url = request
        .rpc_url
        .or(status.rpc_url.clone())
        .or_else(|| {
            request
                .cluster
                .and_then(|c| state.rpc_router.pick_healthy(c).map(|spec| spec.url))
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "solana_snapshot_no_rpc_url",
                "Provide rpcUrl or start a cluster before creating a snapshot.",
            )
        })?;

    state
        .snapshots
        .create(&request.label, &cluster_label, &rpc_url, &request.accounts)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotIdRequest {
    pub id: String,
}

#[tauri::command]
pub fn solana_snapshot_list(state: State<'_, SolanaState>) -> CommandResult<Vec<SnapshotMeta>> {
    state.snapshots.list()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreResponse {
    pub id: String,
    pub account_count: usize,
    pub round_trip_ok: bool,
}

#[tauri::command]
pub fn solana_snapshot_restore(
    state: State<'_, SolanaState>,
    request: SnapshotIdRequest,
) -> CommandResult<SnapshotRestoreResponse> {
    let manifest = state.snapshots.read(&request.id)?;
    // Phase 1 restore semantics: read the manifest and re-pull the same
    // accounts from the live cluster; the round-trip check proves they
    // still match the captured state. Phase 2 will actually push the
    // accounts back into a fresh validator.
    let pubkeys: Vec<String> = manifest.accounts.iter().map(|a| a.pubkey.clone()).collect();
    let fetcher = RpcAccountFetcher;
    let replay = fetcher
        .fetch(&manifest.rpc_url, &pubkeys)
        .unwrap_or_default();
    let round_trip_ok = snapshot::verify_round_trip(&manifest, &replay);
    Ok(SnapshotRestoreResponse {
        id: manifest.id,
        account_count: manifest.accounts.len(),
        round_trip_ok,
    })
}

#[tauri::command]
pub fn solana_snapshot_delete(
    state: State<'_, SolanaState>,
    request: SnapshotIdRequest,
) -> CommandResult<()> {
    state.snapshots.delete(&request.id)
}

#[tauri::command]
pub fn solana_rpc_health(state: State<'_, SolanaState>) -> CommandResult<Vec<EndpointHealth>> {
    Ok(state.rpc_router.refresh_health())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcEndpointsSetRequest {
    pub cluster: ClusterKind,
    pub endpoints: Vec<EndpointSpec>,
}

#[tauri::command]
pub fn solana_rpc_endpoints_set(
    state: State<'_, SolanaState>,
    request: RpcEndpointsSetRequest,
) -> CommandResult<Vec<EndpointHealth>> {
    state
        .rpc_router
        .set_endpoints(request.cluster, request.endpoints)?;
    Ok(state.rpc_router.snapshot_all())
}

/// Lightweight acknowledgement that the frontend can call when it opens
/// the sidebar so the backend emits the current validator status on a
/// well-known channel (matches the emulator `subscribe_ready` pattern).
#[tauri::command]
pub fn solana_subscribe_ready<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SolanaState>,
) -> CommandResult<ClusterStatus> {
    let status = state.supervisor.status();
    let phase = if status.running {
        ValidatorPhase::Ready
    } else {
        ValidatorPhase::Stopped
    };
    let mut payload = ValidatorStatusPayload::new(phase);
    if let Some(kind) = status.kind {
        payload = payload.with_kind(kind.as_str());
    }
    if let Some(url) = status.rpc_url.as_ref() {
        payload = payload.with_rpc_url(url);
    }
    if let Some(url) = status.ws_url.as_ref() {
        payload = payload.with_ws_url(url);
    }
    let _ = app.emit(SOLANA_VALIDATOR_STATUS_EVENT, payload);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_every_cluster_in_router() {
        let state = SolanaState::default();
        let snap = state.rpc_router.snapshot_all();
        for kind in ClusterKind::ALL {
            assert!(snap.iter().any(|e| e.cluster == kind));
        }
    }

    #[test]
    fn default_state_has_idle_supervisor() {
        let state = SolanaState::default();
        assert!(!state.supervisor.status().running);
    }
}
