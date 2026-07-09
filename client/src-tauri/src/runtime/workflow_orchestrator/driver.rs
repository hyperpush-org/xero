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
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use serde_json::Value as JsonValue;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    commands::{
        contracts::workflows::{
            WorkflowRunDto, WorkflowRunStatusDto, WorkflowRunUpdatedPayloadDto,
        },
        CommandResult, WORKFLOW_RUN_UPDATED_EVENT,
    },
    state::DesktopState,
};

use super::reconcile;

const DRIVER_TICK_INTERVAL: Duration = Duration::from_millis(1_000);
const DRIVER_MAX_CONSECUTIVE_ERRORS: u32 = 5;

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
/// their own; paused runs wait for a human decision that re-arms the driver.
fn run_needs_driver(status: WorkflowRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowRunStatusDto::Queued | WorkflowRunStatusDto::Running
    )
}

/// Compact change detector so the driver only emits when the run advanced.
fn run_fingerprint(run: &WorkflowRunDto) -> String {
    use std::fmt::Write as _;

    let mut fingerprint = format!(
        "{:?}|{:?}|{}|{}|{}|{}|{}|{}",
        run.status,
        run.terminal_status,
        run.updated_at,
        run.edge_decisions.len(),
        run.artifacts.len(),
        run.gate_decisions.len(),
        run.loop_attempts.len(),
        run.events.len(),
    );
    for node in &run.nodes {
        let _ = write!(
            fingerprint,
            "|{}:{:?}:{}",
            node.id, node.status, node.attempt_number
        );
    }
    fingerprint
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
    let mut last_fingerprint: Option<String> = None;
    let mut consecutive_errors = 0_u32;

    loop {
        let state = app.state::<DesktopState>();
        match reconcile_workflow_run(app, state.inner(), project_id, run_id) {
            Ok(run) => {
                consecutive_errors = 0;
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
                consecutive_errors += 1;
                if consecutive_errors >= DRIVER_MAX_CONSECUTIVE_ERRORS {
                    eprintln!(
                        "[workflow-driver] run `{run_id}` driver stopped after repeated {}: {}",
                        error.code, error.message
                    );
                    return;
                }
            }
        }
        thread::sleep(DRIVER_TICK_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!run_needs_driver(WorkflowRunStatusDto::Paused));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Completed));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Failed));
        assert!(!run_needs_driver(WorkflowRunStatusDto::Cancelled));
    }
}
