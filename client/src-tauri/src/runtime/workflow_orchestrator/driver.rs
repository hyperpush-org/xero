//! Background driver that advances active workflow runs.
//!
//! The reconcile engine in `reconcile.rs` is a passive step function: a run
//! only progresses when someone calls `reconcile_workflow_run`. This module
//! owns the two pieces that make runs progress on their own:
//!
//! 1. Per-run serialization — every reconcile entry point goes through a
//!    per-run mutex so a driver tick and a UI command can never reconcile the
//!    same run concurrently.
//! 2. A driver thread per active run — ticks reconcile until the run reaches
//!    a terminal status or pauses for a human, emitting
//!    `workflow_run:updated` to the frontend whenever the run changes.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    commands::{
        contracts::workflows::{
            WorkflowNodeRunStatusDto, WorkflowRunDto, WorkflowRunStatusDto,
            WorkflowRunUpdatedPayloadDto, WorkflowTerminalStatusDto,
        },
        CommandError, CommandResult, WORKFLOW_RUN_UPDATED_EVENT,
    },
    db::project_store,
    runtime::process_tree::{process_birth_identity, process_identity_is_live},
    state::DesktopState,
};

use super::reconcile;

const DRIVER_TICK_INTERVAL: Duration = Duration::from_millis(1_000);
const DRIVER_MAX_CONSECUTIVE_ERRORS: u32 = 5;
const DRIVER_FAILURE_CLASS: &str = "workflow_driver_reconcile_failed";
#[cfg(test)]
const DRIVER_FAILURE_EVENT: &str = "workflow_driver_failed";
const DRIVER_FAILURE_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const DRIVER_LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
struct DriverErrorPolicy {
    consecutive_errors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriverErrorDisposition {
    Retry,
    PersistFailure,
}

impl DriverErrorPolicy {
    fn record_success(&mut self) {
        self.consecutive_errors = 0;
    }

    fn record_error(&mut self) -> DriverErrorDisposition {
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        if self.consecutive_errors >= DRIVER_MAX_CONSECUTIVE_ERRORS {
            DriverErrorDisposition::PersistFailure
        } else {
            DriverErrorDisposition::Retry
        }
    }
}

#[derive(Debug)]
struct DriverFailureIncident {
    id: String,
    error_code: String,
    error_message: String,
    consecutive_errors: u32,
}

#[derive(Debug)]
enum DriverFailurePersistence {
    Missing,
    Superseded(WorkflowRunDto),
    Failed(WorkflowRunDto),
    Recovered(WorkflowRunDto),
}

struct WorkflowDriverLeaseGuard {
    repo_root: PathBuf,
    project_id: String,
    run_id: String,
    owner_instance_id: String,
    owner_process_birth_identity: String,
    lease_token: String,
    lost: Arc<AtomicBool>,
    heartbeat_stop: Option<Sender<()>>,
    heartbeat_thread: Option<thread::JoinHandle<()>>,
}

impl WorkflowDriverLeaseGuard {
    fn try_acquire(
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
    ) -> CommandResult<Option<Self>> {
        let existing = project_store::load_workflow_driver_lease(repo_root, project_id, run_id)?;
        if existing.as_ref().is_some_and(workflow_driver_lease_is_live) {
            return Ok(None);
        }
        let owner_instance_id = workflow_driver_owner_instance_id().to_owned();
        let owner_process_birth_identity =
            process_birth_identity(process::id()).ok_or_else(|| {
                CommandError::system_fault(
                    "workflow_driver_process_identity_unavailable",
                    "Xero could not identify the Workflow driver owner process safely.",
                )
            })?;
        let lease_token = unique_workflow_driver_identity("lease");
        let heartbeat_at = crate::auth::now_timestamp();
        if !project_store::claim_workflow_driver_lease(
            repo_root,
            project_id,
            run_id,
            &owner_instance_id,
            process::id(),
            &owner_process_birth_identity,
            &lease_token,
            existing.as_ref(),
            &heartbeat_at,
        )? {
            return Ok(None);
        }

        let (heartbeat_stop, heartbeat_rx) = mpsc::channel();
        let lost = Arc::new(AtomicBool::new(false));
        let heartbeat_lost = lost.clone();
        let heartbeat_repo_root = repo_root.to_path_buf();
        let heartbeat_project_id = project_id.to_owned();
        let heartbeat_run_id = run_id.to_owned();
        let heartbeat_owner_instance_id = owner_instance_id.clone();
        let heartbeat_lease_token = lease_token.clone();
        let heartbeat_thread = thread::spawn(move || loop {
            match heartbeat_rx.recv_timeout(DRIVER_LEASE_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            match project_store::renew_workflow_driver_lease(
                &heartbeat_repo_root,
                &heartbeat_project_id,
                &heartbeat_run_id,
                &heartbeat_owner_instance_id,
                &heartbeat_lease_token,
                &crate::auth::now_timestamp(),
            ) {
                Ok(true) => {}
                Ok(false) | Err(_) => {
                    heartbeat_lost.store(true, Ordering::Release);
                    return;
                }
            }
        });

        Ok(Some(Self {
            repo_root: repo_root.to_path_buf(),
            project_id: project_id.to_owned(),
            run_id: run_id.to_owned(),
            owner_instance_id,
            owner_process_birth_identity,
            lease_token,
            lost,
            heartbeat_stop: Some(heartbeat_stop),
            heartbeat_thread: Some(heartbeat_thread),
        }))
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    fn owns_durable_lease(&self) -> CommandResult<bool> {
        if self.is_lost() {
            return Ok(false);
        }
        let lease = project_store::load_workflow_driver_lease(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
        )?;
        Ok(lease.is_some_and(|lease| {
            lease.owner_instance_id == self.owner_instance_id
                && lease.owner_process_id == process::id()
                && lease.owner_process_birth_identity == self.owner_process_birth_identity
                && lease.lease_token == self.lease_token
        }))
    }
}

impl Drop for WorkflowDriverLeaseGuard {
    fn drop(&mut self) {
        if let Some(stop) = self.heartbeat_stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.heartbeat_thread.take() {
            let _ = worker.join();
        }
        let _ = project_store::release_workflow_driver_lease(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
            &self.owner_instance_id,
            &self.lease_token,
        );
    }
}

fn workflow_driver_owner_instance_id() -> &'static str {
    static INSTANCE_ID: OnceLock<String> = OnceLock::new();
    INSTANCE_ID
        .get_or_init(|| unique_workflow_driver_identity("app"))
        .as_str()
}

fn unique_workflow_driver_identity(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("workflow-driver-{prefix}-{}-{nanos:x}", process::id())
}

fn workflow_driver_lease_is_live(lease: &project_store::WorkflowDriverLeaseRecord) -> bool {
    // A live process may still be returning from an unfenced reconcile call.
    // Never steal from it solely because a heartbeat was delayed; takeover is
    // safe only after the owning process is gone and can no longer mutate.
    process_identity_is_live(lease.owner_process_id, &lease.owner_process_birth_identity)
}

fn driver_key(project_id: &str, run_id: &str) -> String {
    format!("{project_id}\u{1f}{run_id}")
}

fn active_drivers() -> &'static Mutex<HashSet<String>> {
    static ACTIVE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn reconcile_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn reconcile_lock(project_id: &str, run_id: &str) -> Arc<Mutex<()>> {
    let key = driver_key(project_id, run_id);
    let mut locks = reconcile_locks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.entry(key).or_default().clone()
}

/// Reconcile a workflow run while holding its per-run lock so concurrent
/// callers (driver ticks, UI commands) serialize instead of racing.
pub fn reconcile_workflow_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let Some(_lease) = WorkflowDriverLeaseGuard::try_acquire(&repo_root, project_id, run_id)?
    else {
        return project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        });
    };
    reconcile_workflow_run_locally(app, state, project_id, run_id)
}

fn reconcile_workflow_run_locally<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let lock = reconcile_lock(project_id, run_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reconcile::reconcile_workflow_run(app, state, project_id, run_id)
}

/// Serialized wrapper for `reconcile::resume_workflow_checkpoint`.
pub fn resume_workflow_checkpoint<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    decision: &str,
    payload: Option<JsonValue>,
) -> CommandResult<WorkflowRunDto> {
    let lock = reconcile_lock(project_id, run_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reconcile::resume_workflow_checkpoint(
        app,
        state,
        project_id,
        run_id,
        node_run_id,
        decision,
        payload,
    )
}

/// Serialized wrapper for `reconcile::retry_workflow_node_run`.
pub fn retry_workflow_node_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let lock = reconcile_lock(project_id, run_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reconcile::retry_workflow_node_run(app, state, project_id, run_id, node_run_id)
}

/// Serialized wrapper for `reconcile::skip_workflow_branch`.
pub fn skip_workflow_branch<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    reason: Option<&str>,
) -> CommandResult<WorkflowRunDto> {
    let lock = reconcile_lock(project_id, run_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reconcile::skip_workflow_branch(app, state, project_id, run_id, node_run_id, reason)
}

pub fn emit_workflow_run_updated<R: Runtime>(app: &AppHandle<R>, run: &WorkflowRunDto) {
    let payload = WorkflowRunUpdatedPayloadDto {
        project_id: run.project_id.clone(),
        run: run.clone(),
    };
    let _ = app.emit(WORKFLOW_RUN_UPDATED_EVENT, &payload);
}

/// True while the run still needs the driver: queued/running runs advance on
/// their own, cancelling runs drain execution, and paused runs wait for a
/// human decision that re-arms the driver.
fn run_needs_driver(status: WorkflowRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowRunStatusDto::Queued
            | WorkflowRunStatusDto::Running
            | WorkflowRunStatusDto::Cancelling
    )
}

/// Compact change detector so the driver only emits when the run advanced.
fn run_fingerprint(run: &WorkflowRunDto) -> String {
    serde_json::to_string(run)
        .unwrap_or_else(|error| format!("{run:?}|serialization-error:{error}"))
}

fn node_needs_driver_failure(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Eligible
            | WorkflowNodeRunStatusDto::Starting
            | WorkflowNodeRunStatusDto::Running
            | WorkflowNodeRunStatusDto::WaitingOnGate
    )
}

fn driver_failure_retry_delay(retry_number: u32) -> Duration {
    let exponent = retry_number.min(5);
    let seconds = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
    Duration::from_secs(seconds).min(DRIVER_FAILURE_RETRY_MAX_DELAY)
}

#[cfg(test)]
fn is_driver_failure_event(event_type: &str, event: &JsonValue, incident_id: &str) -> bool {
    event_type == DRIVER_FAILURE_EVENT
        && event.get("incidentId").and_then(JsonValue::as_str) == Some(incident_id)
}

fn driver_failure_event_payload(incident: &DriverFailureIncident) -> JsonValue {
    json!({
        "incidentId": incident.id.as_str(),
        "failureClass": DRIVER_FAILURE_CLASS,
        "consecutiveErrors": incident.consecutive_errors,
        "error": {
            "code": incident.error_code.as_str(),
            "message": incident.error_message.as_str(),
        },
        "recoverable": true,
    })
}

/// Persist a latched driver failure while holding the same per-run lock used
/// by reconcile and UI controls. Every owned execution must terminate before
/// the run, its in-flight nodes, and the failure event commit together.
fn persist_driver_failure<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    incident: &DriverFailureIncident,
    lease: &WorkflowDriverLeaseGuard,
) -> CommandResult<DriverFailurePersistence> {
    if !lease.owns_durable_lease()? {
        let run = project_store::get_workflow_run(&lease.repo_root, project_id, run_id)?;
        return Ok(run.map_or(
            DriverFailurePersistence::Missing,
            DriverFailurePersistence::Superseded,
        ));
    }
    let lock = reconcile_lock(project_id, run_id);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let Some(run) = project_store::get_workflow_run(&repo_root, project_id, run_id)? else {
        return Ok(DriverFailurePersistence::Missing);
    };
    if !run_needs_driver(run.status) {
        return Ok(DriverFailurePersistence::Superseded(run));
    }
    for node in run
        .nodes
        .iter()
        .filter(|node| node_needs_driver_failure(node.status))
    {
        reconcile::terminate_workflow_node_execution(state, &repo_root, project_id, &run, node)?;
    }
    if !lease.owns_durable_lease()? {
        let run = project_store::get_workflow_run(&repo_root, project_id, run_id)?;
        return Ok(run.map_or(
            DriverFailurePersistence::Missing,
            DriverFailurePersistence::Superseded,
        ));
    }
    let committed = project_store::fail_workflow_run_from_driver_atomically(
        &repo_root,
        project_id,
        &project_store::WorkflowDriverFailureRecord {
            run_id: run_id.to_owned(),
            incident_id: incident.id.clone(),
            failure_class: DRIVER_FAILURE_CLASS.to_owned(),
            event: driver_failure_event_payload(incident),
            owner_instance_id: lease.owner_instance_id.clone(),
            lease_token: lease.lease_token.clone(),
        },
    )?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::system_fault(
                "workflow_run_missing_after_driver_failure",
                format!("Workflow run `{run_id}` disappeared after its driver failed."),
            )
        })?;
    if committed
        && run.status == WorkflowRunStatusDto::Failed
        && run.terminal_status == Some(WorkflowTerminalStatusDto::Failure)
    {
        Ok(DriverFailurePersistence::Failed(run))
    } else if run_needs_driver(run.status) && lease.owns_durable_lease()? {
        Ok(DriverFailurePersistence::Recovered(run))
    } else {
        Ok(DriverFailurePersistence::Superseded(run))
    }
}

/// Arm the driver only when the run can still advance on its own.
pub fn ensure_workflow_run_driver_if_active<R: Runtime + 'static>(
    app: &AppHandle<R>,
    run: &WorkflowRunDto,
) {
    if run_needs_driver(run.status) {
        ensure_workflow_run_driver(app, &run.project_id, &run.id);
    }
}

/// Ensure a background driver thread is advancing the given run. Idempotent:
/// at most one driver runs per `(project_id, run_id)` at a time. Safe to call
/// for runs in any status — the driver exits immediately when the run no
/// longer needs it.
pub fn ensure_workflow_run_driver<R: Runtime + 'static>(
    app: &AppHandle<R>,
    project_id: &str,
    run_id: &str,
) {
    let key = driver_key(project_id, run_id);
    {
        let mut active = active_drivers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !active.insert(key.clone()) {
            return;
        }
    }

    let app = app.clone();
    let project_id = project_id.to_owned();
    let run_id = run_id.to_owned();
    thread::spawn(move || {
        drive_workflow_run(&app, &project_id, &run_id);
        let mut active = active_drivers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active.remove(&key);
    });
}

fn drive_workflow_run<R: Runtime + 'static>(app: &AppHandle<R>, project_id: &str, run_id: &str) {
    let state = app.state::<DesktopState>();
    let Ok(repo_root) =
        crate::commands::runtime_support::resolve_project_root(app, state.inner(), project_id)
    else {
        return;
    };
    let Ok(Some(lease)) = WorkflowDriverLeaseGuard::try_acquire(&repo_root, project_id, run_id)
    else {
        return;
    };
    let mut last_fingerprint: Option<String> = None;
    let mut error_policy = DriverErrorPolicy::default();
    let mut pending_failure: Option<DriverFailureIncident> = None;
    let mut persistence_retry_number = 0_u32;

    loop {
        if !lease.owns_durable_lease().unwrap_or(false) {
            return;
        }
        let state = app.state::<DesktopState>();
        if let Some(incident) = pending_failure.as_mut() {
            match persist_driver_failure(app, state.inner(), project_id, run_id, incident, &lease) {
                Ok(DriverFailurePersistence::Missing) => return,
                Ok(DriverFailurePersistence::Failed(run))
                | Ok(DriverFailurePersistence::Superseded(run)) => {
                    let fingerprint = run_fingerprint(&run);
                    if last_fingerprint.as_deref() != Some(fingerprint.as_str()) {
                        emit_workflow_run_updated(app, &run);
                    }
                    return;
                }
                Ok(DriverFailurePersistence::Recovered(run)) => {
                    let fingerprint = run_fingerprint(&run);
                    if last_fingerprint.as_deref() != Some(fingerprint.as_str()) {
                        last_fingerprint = Some(fingerprint);
                        emit_workflow_run_updated(app, &run);
                    }
                    error_policy.record_success();
                    pending_failure = None;
                    persistence_retry_number = 0;
                    if !run_needs_driver(run.status) {
                        return;
                    }
                    continue;
                }
                Err(persistence_error) => {
                    let delay = driver_failure_retry_delay(persistence_retry_number);
                    persistence_retry_number = persistence_retry_number.saturating_add(1);
                    eprintln!(
                        "[workflow-driver] run `{run_id}` could not persist driver failure {}: {}; retrying in {}s",
                        persistence_error.code,
                        persistence_error.message,
                        delay.as_secs()
                    );
                    // `persist_driver_failure` releases the per-run mutex
                    // before returning, so the capped sleep never blocks UI
                    // controls for this run.
                    thread::sleep(delay);
                    continue;
                }
            }
        }

        match reconcile_workflow_run_locally(app, state.inner(), project_id, run_id) {
            Ok(run) => {
                error_policy.record_success();
                let fingerprint = run_fingerprint(&run);
                if last_fingerprint.as_deref() != Some(fingerprint.as_str()) {
                    last_fingerprint = Some(fingerprint);
                    emit_workflow_run_updated(app, &run);
                }
                if !run_needs_driver(run.status) {
                    return;
                }
            }
            Err(error) => {
                if error.code == "workflow_run_not_found" {
                    return;
                }
                if project_store::workflow_run_cancellation_pending(&repo_root, project_id, run_id)
                    .unwrap_or(false)
                {
                    thread::sleep(DRIVER_TICK_INTERVAL);
                    continue;
                }
                if error_policy.record_error() == DriverErrorDisposition::PersistFailure {
                    pending_failure = Some(DriverFailureIncident {
                        id: project_store::generate_workflow_id("workflow-driver-failure"),
                        error_code: error.code,
                        error_message: error.message,
                        consecutive_errors: error_policy.consecutive_errors,
                    });
                    continue;
                }
            }
        }
        thread::sleep(DRIVER_TICK_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::contracts::workflows::{
        WorkflowDefinitionDto, WorkflowEventDto, WorkflowNodeDto, WorkflowRunNodeDto,
        WorkflowRunPolicyDto,
    };
    use rusqlite::{params, Connection};
    use std::fs;
    use tauri::Manager;

    #[test]
    fn driver_registry_deduplicates_by_project_and_run() {
        let key = driver_key("project-a", "run-1");
        assert!(active_drivers().lock().expect("lock").insert(key.clone()));
        assert!(!active_drivers().lock().expect("lock").insert(key.clone()));
        active_drivers().lock().expect("lock").remove(&key);
    }

    #[test]
    fn reconcile_lock_is_shared_per_run() {
        let first = reconcile_lock("project-b", "run-1");
        let second = reconcile_lock("project-b", "run-1");
        let other = reconcile_lock("project-b", "run-2");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &other));
    }

    #[test]
    fn run_needs_driver_only_for_active_statuses() {
        assert!(run_needs_driver(WorkflowRunStatusDto::Queued));
        assert!(run_needs_driver(WorkflowRunStatusDto::Running));
        assert!(run_needs_driver(WorkflowRunStatusDto::Cancelling));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Paused));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Completed));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Failed));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Cancelled));
    }

    #[test]
    fn repeated_driver_errors_trip_only_after_transient_retry_budget() {
        let mut policy = DriverErrorPolicy::default();

        for _ in 0..(DRIVER_MAX_CONSECUTIVE_ERRORS - 1) {
            assert_eq!(policy.record_error(), DriverErrorDisposition::Retry);
        }
        assert_eq!(
            policy.record_error(),
            DriverErrorDisposition::PersistFailure
        );
        assert_eq!(policy.consecutive_errors, DRIVER_MAX_CONSECUTIVE_ERRORS);
    }

    #[test]
    fn successful_reconcile_resets_consecutive_driver_errors() {
        let mut policy = DriverErrorPolicy::default();
        for _ in 0..(DRIVER_MAX_CONSECUTIVE_ERRORS - 1) {
            assert_eq!(policy.record_error(), DriverErrorDisposition::Retry);
        }

        policy.record_success();

        assert_eq!(policy.consecutive_errors, 0);
        assert_eq!(policy.record_error(), DriverErrorDisposition::Retry);
    }

    #[test]
    fn durable_failure_persistence_uses_capped_backoff() {
        assert_eq!(driver_failure_retry_delay(0), Duration::from_secs(1));
        assert_eq!(driver_failure_retry_delay(1), Duration::from_secs(2));
        assert_eq!(driver_failure_retry_delay(4), Duration::from_secs(16));
        assert_eq!(driver_failure_retry_delay(5), Duration::from_secs(30));
        assert_eq!(driver_failure_retry_delay(30), Duration::from_secs(30));
    }

    #[test]
    fn driver_failure_payload_preserves_diagnostic_context() {
        let incident = DriverFailureIncident {
            id: "incident-1".into(),
            error_code: "workflow_database_busy".into(),
            error_message: "database remained locked".into(),
            consecutive_errors: DRIVER_MAX_CONSECUTIVE_ERRORS,
        };

        assert_eq!(
            driver_failure_event_payload(&incident),
            json!({
                "incidentId": "incident-1",
                "failureClass": DRIVER_FAILURE_CLASS,
                "consecutiveErrors": DRIVER_MAX_CONSECUTIVE_ERRORS,
                "error": {
                    "code": "workflow_database_busy",
                    "message": "database remained locked",
                },
                "recoverable": true,
            })
        );
    }

    #[test]
    fn driver_failure_event_is_idempotent_per_incident() {
        let event = json!({ "incidentId": "incident-1" });

        assert!(is_driver_failure_event(
            DRIVER_FAILURE_EVENT,
            &event,
            "incident-1"
        ));
        assert!(!is_driver_failure_event(
            DRIVER_FAILURE_EVENT,
            &event,
            "incident-2"
        ));
        assert!(!is_driver_failure_event(
            "workflow_metric",
            &event,
            "incident-1"
        ));
    }

    #[test]
    fn only_in_flight_nodes_are_stalled_for_driver_failure() {
        for status in [
            WorkflowNodeRunStatusDto::Eligible,
            WorkflowNodeRunStatusDto::Starting,
            WorkflowNodeRunStatusDto::Running,
            WorkflowNodeRunStatusDto::WaitingOnGate,
        ] {
            assert!(node_needs_driver_failure(status));
        }
        for status in [
            WorkflowNodeRunStatusDto::Pending,
            WorkflowNodeRunStatusDto::Succeeded,
            WorkflowNodeRunStatusDto::Failed,
            WorkflowNodeRunStatusDto::Stalled,
            WorkflowNodeRunStatusDto::Skipped,
            WorkflowNodeRunStatusDto::Cancelled,
        ] {
            assert!(!node_needs_driver_failure(status));
        }
    }

    #[test]
    fn run_fingerprint_detects_same_status_node_and_event_content_changes() {
        let run = workflow_run_fixture();
        let baseline = run_fingerprint(&run);

        let mut linked = run.clone();
        linked.nodes[0].runtime_run_id = Some("runtime-run-1".into());
        assert_ne!(run_fingerprint(&linked), baseline);

        let mut event_changed = run;
        event_changed.events[0].event = json!({"progress": 2});
        assert_ne!(run_fingerprint(&event_changed), baseline);
    }

    #[test]
    fn driver_identities_and_lease_liveness_bind_to_the_owner_process() {
        let identity = unique_workflow_driver_identity("fixture");
        assert!(identity.starts_with(&format!("workflow-driver-fixture-{}-", process::id())));
        assert_eq!(
            workflow_driver_owner_instance_id(),
            workflow_driver_owner_instance_id()
        );

        let process_birth =
            process_birth_identity(process::id()).expect("current process identity");
        let mut lease = project_store::WorkflowDriverLeaseRecord {
            project_id: "project-1".into(),
            workflow_run_id: "run-1".into(),
            owner_instance_id: "owner-1".into(),
            owner_process_id: process::id(),
            owner_process_birth_identity: process_birth,
            lease_token: "lease-1".into(),
            acquired_at: "2026-07-18T12:00:00Z".into(),
            heartbeat_at: "2026-07-18T12:00:00Z".into(),
        };
        assert!(workflow_driver_lease_is_live(&lease));

        lease.owner_process_id = u32::MAX;
        lease.owner_process_birth_identity = "missing-process".into();
        assert!(!workflow_driver_lease_is_live(&lease));
    }

    #[test]
    fn driver_error_counter_saturates_without_reopening_the_retry_budget() {
        let mut policy = DriverErrorPolicy {
            consecutive_errors: u32::MAX,
        };

        assert_eq!(
            policy.record_error(),
            DriverErrorDisposition::PersistFailure
        );
        assert_eq!(policy.consecutive_errors, u32::MAX);
    }

    #[test]
    fn driver_failure_fixture_commits_once_and_rejects_lost_or_terminal_leases() {
        let fixture = tempfile::tempdir().expect("driver failure fixture");
        let fixture_root = fs::canonicalize(fixture.path()).expect("canonical fixture root");
        let repo_root = fixture_root.join("repo");
        fs::create_dir_all(&repo_root).expect("create fixture repository");
        let repo_root = fs::canonicalize(repo_root).expect("canonical fixture repository");
        let project_id = "project-driver-failure";
        let database_path = fixture_root
            .join("app-data/projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("project database parent"))
            .expect("create project database directory");
        crate::db::register_project_database_path_for_tests(&repo_root, database_path.clone());
        let mut connection = Connection::open(&database_path).expect("open project database");
        crate::db::configure_connection(&connection).expect("configure project database");
        crate::db::migrations::migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");

        let global_db_path = fixture_root.join("app-data/global.db");
        crate::registry::upsert_project(
            &global_db_path,
            crate::registry::RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: "repo-1".into(),
                root_path: repo_root.to_string_lossy().into_owned(),
                is_git_repo: true,
            },
            &crate::state::ImportFailpoints::default(),
        )
        .expect("register fixture project");
        let app = crate::configure_builder_with_state(
            tauri::test::mock_builder(),
            DesktopState::default().with_global_db_path_override(global_db_path),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build driver fixture app");

        let definition = project_store::create_workflow_definition(
            &repo_root,
            &WorkflowDefinitionDto {
                schema: "xero.workflow_definition.v1".into(),
                id: "workflow-driver-failure".into(),
                project_id: project_id.into(),
                name: "Driver failure fixture".into(),
                description: String::new(),
                version: 1,
                start_node_id: "done".into(),
                nodes: vec![WorkflowNodeDto::Terminal {
                    id: "done".into(),
                    title: "Done".into(),
                    description: String::new(),
                    position: Default::default(),
                    terminal_status: WorkflowTerminalStatusDto::Success,
                }],
                edges: Vec::new(),
                subgraphs: Vec::new(),
                artifact_contracts: Vec::new(),
                run_policy: WorkflowRunPolicyDto::default(),
                created_at: None,
                updated_at: None,
            },
        )
        .expect("create fixture workflow");
        let run = project_store::create_workflow_run(
            &repo_root,
            project_id,
            &definition.id,
            Some(json!({"fixture": true})),
        )
        .expect("create queued workflow run");
        project_store::insert_workflow_run_node(
            &repo_root,
            project_id,
            &run.id,
            "done",
            "terminal",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "driver-failure-terminal-attempt",
        )
        .expect("insert eligible fixture node");
        let lease = WorkflowDriverLeaseGuard::try_acquire(&repo_root, project_id, &run.id)
            .expect("acquire driver lease")
            .expect("fixture owns driver lease");
        let incident = DriverFailureIncident {
            id: "driver-incident-1".into(),
            error_code: "workflow_reconcile_fixture_failed".into(),
            error_message: "fixture reconcile failure".into(),
            consecutive_errors: DRIVER_MAX_CONSECUTIVE_ERRORS,
        };

        let persisted = persist_driver_failure(
            app.handle(),
            app.state::<DesktopState>().inner(),
            project_id,
            &run.id,
            &incident,
            &lease,
        )
        .expect("persist driver failure");
        let DriverFailurePersistence::Failed(failed) = persisted else {
            panic!("owned active lease should commit the driver failure");
        };
        assert_eq!(failed.status, WorkflowRunStatusDto::Failed);
        assert_eq!(
            failed.terminal_status,
            Some(WorkflowTerminalStatusDto::Failure)
        );
        assert_eq!(
            failed
                .events
                .iter()
                .filter(|event| is_driver_failure_event(
                    &event.event_type,
                    &event.event,
                    &incident.id
                ))
                .count(),
            1
        );

        assert!(matches!(
            persist_driver_failure(
                app.handle(),
                app.state::<DesktopState>().inner(),
                project_id,
                &run.id,
                &incident,
                &lease,
            )
            .expect("terminal replay is superseded"),
            DriverFailurePersistence::Superseded(run) if run.status == WorkflowRunStatusDto::Failed
        ));

        lease.lost.store(true, Ordering::Release);
        assert!(matches!(
            persist_driver_failure(
                app.handle(),
                app.state::<DesktopState>().inner(),
                project_id,
                &run.id,
                &incident,
                &lease,
            )
            .expect("lost lease cannot mutate the run"),
            DriverFailurePersistence::Superseded(run) if run.status == WorkflowRunStatusDto::Failed
        ));
    }

    fn workflow_run_fixture() -> WorkflowRunDto {
        WorkflowRunDto {
            id: "run-1".into(),
            project_id: "project-1".into(),
            workflow_version_id: "workflow-version-1".into(),
            workflow_id: "workflow-1".into(),
            workflow_version_number: 1,
            status: WorkflowRunStatusDto::Running,
            terminal_status: None,
            definition_snapshot: WorkflowDefinitionDto {
                schema: "xero.workflow_definition.v1".into(),
                id: "workflow-1".into(),
                project_id: "project-1".into(),
                name: "Fixture workflow".into(),
                description: String::new(),
                version: 1,
                start_node_id: "agent-1".into(),
                nodes: Vec::new(),
                edges: Vec::new(),
                subgraphs: Vec::new(),
                artifact_contracts: Vec::new(),
                run_policy: WorkflowRunPolicyDto::default(),
                created_at: None,
                updated_at: None,
            },
            initial_input: Some(json!({"goal": "verify"})),
            started_at: "2026-07-18T12:00:00Z".into(),
            updated_at: "2026-07-18T12:00:01Z".into(),
            completed_at: None,
            cancellation_reason: None,
            nodes: vec![WorkflowRunNodeDto {
                id: "node-run-1".into(),
                workflow_run_id: "run-1".into(),
                node_id: "agent-1".into(),
                node_type: "agent".into(),
                status: WorkflowNodeRunStatusDto::Starting,
                attempt_number: 1,
                runtime_run_id: None,
                agent_session_id: Some("session-1".into()),
                failure_class: None,
                started_at: Some("2026-07-18T12:00:01Z".into()),
                updated_at: "2026-07-18T12:00:01Z".into(),
                completed_at: None,
                idempotency_key: "node-run-1:1".into(),
            }],
            edge_decisions: Vec::new(),
            artifacts: Vec::new(),
            gate_decisions: Vec::new(),
            loop_attempts: Vec::new(),
            events: vec![WorkflowEventDto {
                id: "event-1".into(),
                workflow_run_id: "run-1".into(),
                node_run_id: Some("node-run-1".into()),
                event_type: "workflow_node_progress".into(),
                event: json!({"progress": 1}),
                created_at: "2026-07-18T12:00:01Z".into(),
            }],
        }
    }
}
