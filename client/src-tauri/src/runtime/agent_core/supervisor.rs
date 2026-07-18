use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_store::{self, AgentRunDriveLeaseClaimResult, AgentRunDriveLeaseRecord},
    runtime::process_tree::{process_birth_identity, process_identity_is_live},
};

pub const AGENT_RUN_CANCELLED_CODE: &str = "agent_run_cancelled";
const AGENT_RUN_DRIVE_LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct AgentRunSupervisor {
    inner: Arc<Mutex<AgentRunSupervisorState>>,
    instance: Arc<AgentRunSupervisorInstance>,
}

#[derive(Debug)]
struct AgentRunSupervisorInstance {
    id: String,
    process_id: u32,
    process_birth_identity: Option<String>,
}

#[derive(Debug, Default)]
struct AgentRunSupervisorState {
    active: HashMap<String, ActiveAgentRun>,
}

#[derive(Debug, Clone)]
struct ActiveAgentRun {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    token: AgentRunCancellationToken,
}

#[derive(Debug)]
pub struct AgentRunLease {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    token: AgentRunCancellationToken,
    supervisor: AgentRunSupervisor,
    persisted: Option<PersistedAgentRunDriveLease>,
}

#[derive(Debug)]
struct PersistedAgentRunDriveLease {
    repo_root: PathBuf,
    owner_instance_id: String,
    drive_token: String,
    heartbeat_stop: Option<Sender<()>>,
    heartbeat_thread: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct AgentRunCancellationToken {
    cancelled: Arc<AtomicBool>,
    linked_parents: Arc<Vec<Arc<AtomicBool>>>,
}

impl Default for AgentRunCancellationToken {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            linked_parents: Arc::new(Vec::new()),
        }
    }
}

impl Default for AgentRunSupervisor {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AgentRunSupervisorState::default())),
            instance: Arc::new(AgentRunSupervisorInstance {
                id: unique_runtime_identity("app"),
                process_id: process::id(),
                process_birth_identity: process_birth_identity(process::id()),
            }),
        }
    }
}

impl AgentRunSupervisor {
    pub fn begin(
        &self,
        project_id: &str,
        agent_session_id: &str,
        run_id: &str,
    ) -> CommandResult<AgentRunLease> {
        let mut state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;

        if let Some(active) = state.active.get(run_id) {
            return Err(CommandError::user_fixable(
                "agent_run_already_active",
                format!(
                    "Xero is already driving owned-agent run `{}` for project `{}` session `{}`.",
                    active.run_id, active.project_id, active.agent_session_id
                ),
            ));
        }

        let token = AgentRunCancellationToken::default();
        state.active.insert(
            run_id.into(),
            ActiveAgentRun {
                project_id: project_id.into(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                token: token.clone(),
            },
        );

        Ok(AgentRunLease {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            token,
            supervisor: self.clone(),
            persisted: None,
        })
    }

    pub fn begin_persisted(
        &self,
        repo_root: &Path,
        project_id: &str,
        agent_session_id: &str,
        run_id: &str,
    ) -> CommandResult<AgentRunLease> {
        // The in-process claim comes first. If it succeeds, any persisted lease owned by this
        // exact app instance is orphaned (for example, a previous release failed) and can be
        // replaced safely. A different live process remains authoritative and cannot be stolen.
        let mut lease = self.begin(project_id, agent_session_id, run_id)?;
        let owner_process_birth_identity = self
            .instance
            .process_birth_identity
            .clone()
            .ok_or_else(|| {
                CommandError::system_fault(
                    "agent_run_process_identity_unavailable",
                    "Xero could not establish a process-lifetime identity for durable agent ownership.",
                )
            })?;
        let replacement = AgentRunDriveLeaseRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            owner_instance_id: self.instance.id.clone(),
            owner_process_id: self.instance.process_id,
            owner_process_birth_identity,
            drive_token: unique_runtime_identity("drive"),
            acquired_at: now_timestamp(),
        };
        let claim = project_store::claim_agent_run_drive_lease(
            repo_root,
            project_id,
            run_id,
            &replacement.owner_instance_id,
            replacement.owner_process_id,
            &replacement.owner_process_birth_identity,
            &replacement.drive_token,
            &replacement.acquired_at,
        )?;
        match claim {
            AgentRunDriveLeaseClaimResult::Acquired => {}
            AgentRunDriveLeaseClaimResult::Held(held)
                if held.owner_instance_id == self.instance.id
                    || !process_identity_is_live(
                        held.owner_process_id,
                        &held.owner_process_birth_identity,
                    ) =>
            {
                if !project_store::replace_agent_run_drive_lease(repo_root, &replacement, &held)? {
                    return Err(agent_run_persisted_lease_held_error(run_id));
                }
            }
            AgentRunDriveLeaseClaimResult::Held(_) => {
                return Err(agent_run_persisted_lease_held_error(run_id));
            }
            AgentRunDriveLeaseClaimResult::RunNotDrivable(status) => {
                return Err(CommandError::user_fixable(
                    "agent_run_not_resumable",
                    format!(
                        "Xero cannot drive owned-agent run `{run_id}` because it is {status:?}."
                    ),
                ));
            }
        }
        let persisted = start_persisted_lease_heartbeat(
            repo_root,
            project_id,
            run_id,
            &replacement.owner_instance_id,
            &replacement.drive_token,
            lease.token.clone(),
        );
        let persisted = match persisted {
            Ok(persisted) => persisted,
            Err(error) => {
                let _ = project_store::release_agent_run_drive_lease(
                    repo_root,
                    project_id,
                    run_id,
                    &replacement.owner_instance_id,
                    &replacement.drive_token,
                );
                return Err(error);
            }
        };
        lease.persisted = Some(persisted);
        Ok(lease)
    }

    pub fn cancel(&self, run_id: &str) -> CommandResult<bool> {
        let state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;
        let Some(active) = state.active.get(run_id) else {
            return Ok(false);
        };
        active.token.cancel();
        Ok(true)
    }

    pub fn is_active(&self, run_id: &str) -> CommandResult<bool> {
        let state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;
        Ok(state.active.contains_key(run_id))
    }

    pub(crate) fn owns_persisted_lease(&self, lease: &AgentRunDriveLeaseRecord) -> bool {
        lease.owner_instance_id == self.instance.id
            && lease.owner_process_id == self.instance.process_id
    }

    pub(crate) fn persisted_lease_is_foreign_and_live(
        &self,
        lease: &AgentRunDriveLeaseRecord,
    ) -> bool {
        !self.owns_persisted_lease(lease)
            && process_identity_is_live(lease.owner_process_id, &lease.owner_process_birth_identity)
    }

    fn finish(&self, lease: &AgentRunLease) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        let should_remove = state.active.get(&lease.run_id).is_some_and(|active| {
            active.project_id == lease.project_id
                && active.agent_session_id == lease.agent_session_id
                && active.run_id == lease.run_id
                && active.token.same_token(&lease.token)
        });
        if should_remove {
            state.active.remove(&lease.run_id);
        }
    }
}

impl AgentRunLease {
    pub fn token(&self) -> AgentRunCancellationToken {
        self.token.clone()
    }

    pub(crate) fn matches(&self, project_id: &str, agent_session_id: &str, run_id: &str) -> bool {
        self.project_id == project_id
            && self.agent_session_id == agent_session_id
            && self.run_id == run_id
    }
}

impl Drop for AgentRunLease {
    fn drop(&mut self) {
        if let Some(mut persisted) = self.persisted.take() {
            if let Some(stop) = persisted.heartbeat_stop.take() {
                let _ = stop.send(());
            }
            if let Some(worker) = persisted.heartbeat_thread.take() {
                let _ = worker.join();
            }
            let _ = project_store::release_agent_run_drive_lease(
                &persisted.repo_root,
                &self.project_id,
                &self.run_id,
                &persisted.owner_instance_id,
                &persisted.drive_token,
            );
        }
        self.supervisor.finish(self);
    }
}

impl AgentRunCancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
            || self
                .linked_parents
                .iter()
                .any(|parent| parent.load(Ordering::SeqCst))
    }

    pub fn check_cancelled(&self) -> CommandResult<()> {
        if self.is_cancelled() {
            return Err(cancelled_error());
        }
        Ok(())
    }

    fn same_token(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }

    pub fn linked_child(&self) -> Self {
        let mut linked_parents = self.linked_parents.as_ref().clone();
        linked_parents.push(self.cancelled.clone());
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            linked_parents: Arc::new(linked_parents),
        }
    }
}

pub fn cancelled_error() -> CommandError {
    CommandError::retryable(
        AGENT_RUN_CANCELLED_CODE,
        "Owned agent run was cancelled before it could finish.",
    )
}

fn agent_run_persisted_lease_held_error(run_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_run_already_active",
        format!(
            "Xero is already driving owned-agent run `{run_id}`. Wait for it to finish or cancel it before sending another message."
        ),
    )
}

fn start_persisted_lease_heartbeat(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    drive_token: &str,
    cancellation: AgentRunCancellationToken,
) -> CommandResult<PersistedAgentRunDriveLease> {
    let (heartbeat_stop, stop_receiver) = mpsc::channel();
    let heartbeat_repo_root = repo_root.to_path_buf();
    let heartbeat_project_id = project_id.to_owned();
    let heartbeat_run_id = run_id.to_owned();
    let heartbeat_owner_instance_id = owner_instance_id.to_owned();
    let heartbeat_drive_token = drive_token.to_owned();
    let heartbeat_thread = thread::Builder::new()
        .name(format!("xero-agent-lease-{run_id}"))
        .spawn(move || loop {
            match stop_receiver.recv_timeout(AGENT_RUN_DRIVE_LEASE_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let renewed = project_store::renew_agent_run_drive_lease(
                        &heartbeat_repo_root,
                        &heartbeat_project_id,
                        &heartbeat_run_id,
                        &heartbeat_owner_instance_id,
                        &heartbeat_drive_token,
                        &now_timestamp(),
                    );
                    match renewed {
                        Ok(true) => {}
                        Ok(false) => {
                            cancellation.cancel();
                            break;
                        }
                        Err(_) => {
                            // A transient database write failure does not prove that ownership
                            // was lost. In particular, an isolated mutation worker can hold the
                            // project database long enough for this heartbeat attempt to time
                            // out. Keep the run alive and re-check the durable lease on the next
                            // interval; a recovered database will then return `false` if another
                            // owner actually replaced the lease.
                        }
                    }
                }
            }
        })
        .map_err(|error| {
            CommandError::system_fault(
                "agent_run_drive_lease_heartbeat_spawn_failed",
                format!("Xero could not start the owned-agent lease heartbeat: {error}"),
            )
        })?;
    Ok(PersistedAgentRunDriveLease {
        repo_root: repo_root.to_path_buf(),
        owner_instance_id: owner_instance_id.to_owned(),
        drive_token: drive_token.to_owned(),
        heartbeat_stop: Some(heartbeat_stop),
        heartbeat_thread: Some(heartbeat_thread),
    })
}

fn unique_runtime_identity(prefix: &str) -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{}-{nanos:x}-{sequence:x}", process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use rusqlite::{params, Connection};

    use crate::{
        commands::RuntimeAgentIdDto,
        db::{
            configure_connection, migrations::migrations, register_project_database_path_for_tests,
        },
        runtime::process_tree::process_is_alive,
    };

    fn seeded_agent_run() -> (tempfile::TempDir, PathBuf, String, String) {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let database_path = repo_root.join("state.db");
        register_project_database_path_for_tests(&repo_root, database_path.clone());
        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        let project_id = "project-drive-supervisor".to_string();
        let run_id = "run-drive-supervisor".to_string();
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
        connection
            .execute(
                "INSERT INTO agent_sessions (project_id, agent_session_id, title, status, selected) VALUES (?1, 'session-1', 'Default', 'active', 1)",
                params![project_id],
            )
            .expect("insert agent session");
        project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.clone(),
                agent_session_id: "session-1".into(),
                run_id: run_id.clone(),
                provider_id: "provider-1".into(),
                model_id: "model-1".into(),
                prompt: "Do the thing".into(),
                system_prompt: "System prompt".into(),
                now: "2026-07-15T12:00:00Z".into(),
            },
        )
        .expect("insert agent run");
        (temp, repo_root, project_id, run_id)
    }

    fn supervisor_with_identity(id: &str, process_id: u32) -> AgentRunSupervisor {
        AgentRunSupervisor {
            inner: Arc::new(Mutex::new(AgentRunSupervisorState::default())),
            instance: Arc::new(AgentRunSupervisorInstance {
                id: id.into(),
                process_id,
                process_birth_identity: process_birth_identity(process_id)
                    .or_else(|| Some(format!("test-process-birth-{process_id}"))),
            }),
        }
    }

    #[test]
    fn persisted_drive_lease_blocks_a_second_live_supervisor_until_release() {
        let (_temp, repo_root, project_id, run_id) = seeded_agent_run();
        let first = AgentRunSupervisor::default();
        let second = AgentRunSupervisor::default();

        let first_lease = first
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect("first supervisor claims run");
        let persisted = project_store::load_agent_run_drive_lease(&repo_root, &project_id, &run_id)
            .expect("load first lease")
            .expect("first lease exists");
        assert!(first.owns_persisted_lease(&persisted));
        assert!(second.persisted_lease_is_foreign_and_live(&persisted));
        let same_instance = first.clone();
        let same_instance_error = same_instance
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect_err("same app instance cannot replace its active drive token");
        assert_eq!(same_instance_error.code, "agent_run_already_active");
        let error = second
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect_err("second live supervisor cannot claim run");
        assert_eq!(error.code, "agent_run_already_active");

        drop(first_lease);
        let second_lease = second
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect("released lease can be claimed");
        drop(second_lease);
    }

    #[test]
    fn persisted_drive_lease_recovers_only_after_prior_process_is_dead() {
        let (_temp, repo_root, project_id, run_id) = seeded_agent_run();
        let dead_process_id = (1_000_000..1_000_100)
            .find(|process_id| !process_is_alive(*process_id))
            .expect("find an unused process id");
        let prior = supervisor_with_identity("prior-app-instance", dead_process_id);
        let prior_lease = prior
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect("seed prior-process lease");

        let current = AgentRunSupervisor::default();
        let current_lease = current
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect("dead prior process lease is recoverable");
        drop(prior_lease);

        let competing_live = AgentRunSupervisor::default();
        let error = competing_live
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect_err("old token release cannot clear replacement lease");
        assert_eq!(error.code, "agent_run_already_active");
        drop(current_lease);
        let recovered = competing_live
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect("replacement lease releases normally");
        drop(recovered);
    }

    #[test]
    fn persisted_drive_lease_never_steals_from_a_live_pid_on_heartbeat_age_alone() {
        let (_temp, repo_root, project_id, run_id) = seeded_agent_run();
        assert_eq!(
            project_store::claim_agent_run_drive_lease(
                &repo_root,
                &project_id,
                &run_id,
                "crashed-app-instance",
                process::id(),
                &process_birth_identity(process::id()).expect("current process identity"),
                "crashed-drive-token",
                "2000-01-01T00:00:00Z",
            )
            .expect("seed expired lease"),
            AgentRunDriveLeaseClaimResult::Acquired
        );

        let current = AgentRunSupervisor::default();
        let error = current
            .begin_persisted(&repo_root, &project_id, "session-1", &run_id)
            .expect_err("an old heartbeat cannot fence a still-live owner");
        assert_eq!(error.code, "agent_run_already_active");
        let persisted = project_store::load_agent_run_drive_lease(&repo_root, &project_id, &run_id)
            .expect("load original lease")
            .expect("original lease exists");
        assert_eq!(persisted.owner_instance_id, "crashed-app-instance");
        assert_eq!(persisted.drive_token, "crashed-drive-token");
    }

    #[test]
    fn foreign_live_owner_receives_durable_cancellation_intent_without_lease_theft() {
        let (_temp, repo_root, project_id, run_id) = seeded_agent_run();
        let owner_birth = process_birth_identity(process::id()).expect("current process identity");
        assert_eq!(
            project_store::claim_agent_run_drive_lease(
                &repo_root,
                &project_id,
                &run_id,
                "foreign-live-owner",
                process::id(),
                &owner_birth,
                "foreign-drive-token",
                "2026-07-15T12:00:00Z",
            )
            .expect("seed foreign live lease"),
            AgentRunDriveLeaseClaimResult::Acquired
        );

        let runtime = crate::runtime::DesktopAgentCoreRuntime::new(AgentRunSupervisor::default());
        let cancelling = runtime
            .cancel_run(repo_root.clone(), project_id.clone(), run_id.clone())
            .expect("persist cancellation intent");
        assert_eq!(
            cancelling.run.status,
            project_store::AgentRunStatus::Cancelling
        );
        assert!(
            project_store::load_agent_run_drive_lease(&repo_root, &project_id, &run_id)
                .expect("load retained owner lease")
                .is_some()
        );
        assert!(!project_store::renew_agent_run_drive_lease(
            &repo_root,
            &project_id,
            &run_id,
            "foreign-live-owner",
            "foreign-drive-token",
            "2026-07-15T12:00:05Z",
        )
        .expect("cancellation fences heartbeat renewal"));

        let cancelled = project_store::update_agent_run_status(
            &repo_root,
            &project_id,
            &run_id,
            project_store::AgentRunStatus::Cancelled,
            None,
            "2026-07-15T12:00:06Z",
        )
        .expect("owner finalizes cancellation");
        assert_eq!(
            cancelled.run.status,
            project_store::AgentRunStatus::Cancelled
        );
    }
}
