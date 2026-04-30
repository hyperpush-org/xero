use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeRunApprovalModeDto, RuntimeRunControlInputDto},
    db::project_store::{
        RuntimeRunActiveControlSnapshotRecord, RuntimeRunControlStateRecord,
        RuntimeRunSnapshotRecord,
    },
    runtime::{protocol::RuntimeSupervisorLaunchContext, OPENAI_CODEX_PROVIDER_ID},
    state::DesktopState,
};

const DEFAULT_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeSupervisorSnapshot {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeSupervisorController {
    inner: Arc<Mutex<HashMap<String, ActiveRuntimeSupervisorSnapshot>>>,
}

impl RuntimeSupervisorController {
    pub fn remember(&self, project_id: &str, agent_session_id: &str, run_id: &str, endpoint: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .insert(
                supervisor_registry_key(project_id, agent_session_id),
                ActiveRuntimeSupervisorSnapshot {
                    project_id: project_id.into(),
                    agent_session_id: agent_session_id.into(),
                    run_id: run_id.into(),
                    endpoint: endpoint.into(),
                },
            );
    }

    pub fn forget(&self, project_id: &str, agent_session_id: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .remove(&supervisor_registry_key(project_id, agent_session_id));
    }

    pub fn snapshot(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> Option<ActiveRuntimeSupervisorSnapshot> {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .get(&supervisor_registry_key(project_id, agent_session_id))
            .cloned()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct RuntimeSupervisorLaunchEnv {
    vars: BTreeMap<String, String>,
}

impl RuntimeSupervisorLaunchEnv {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }
}

impl std::fmt::Debug for RuntimeSupervisorLaunchEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys = self.vars.keys().cloned().collect::<Vec<_>>();
        f.debug_struct("RuntimeSupervisorLaunchEnv")
            .field("keys", &keys)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorLaunchRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub launch_context: RuntimeSupervisorLaunchContext,
    pub launch_env: RuntimeSupervisorLaunchEnv,
    pub program: String,
    pub args: Vec<String>,
    pub startup_timeout: Duration,
    pub control_timeout: Duration,
    pub supervisor_binary: Option<PathBuf>,
    pub run_controls: RuntimeRunControlStateRecord,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorProbeRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorStopRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
    pub shutdown_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorSubmitInputRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub action_id: String,
    pub boundary_id: String,
    pub input: String,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorUpdateControlsRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub run_id: String,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub prompt: Option<String>,
    pub control_timeout: Duration,
}

impl Default for RuntimeSupervisorLaunchRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
            run_id: String::new(),
            session_id: String::new(),
            flow_id: None,
            launch_context: RuntimeSupervisorLaunchContext {
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                session_id: String::new(),
                flow_id: None,
                model_id: OPENAI_CODEX_PROVIDER_ID.into(),
                thinking_effort: None,
            },
            launch_env: RuntimeSupervisorLaunchEnv::default(),
            program: String::new(),
            args: Vec::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            supervisor_binary: None,
            run_controls: default_runtime_run_controls(),
        }
    }
}

impl Default for RuntimeSupervisorProbeRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
        }
    }
}

impl Default for RuntimeSupervisorStopRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            shutdown_timeout: DEFAULT_STOP_TIMEOUT,
        }
    }
}

impl Default for RuntimeSupervisorUpdateControlsRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            run_id: String::new(),
            controls: None,
            prompt: None,
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
        }
    }
}

pub fn launch_legacy_runtime_supervisor(
    _state: &DesktopState,
    _request: RuntimeSupervisorLaunchRequest,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    Err(removed_runtime_supervisor_error())
}

pub fn probe_runtime_run(
    _state: &DesktopState,
    _request: RuntimeSupervisorProbeRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    Err(removed_runtime_supervisor_error())
}

pub fn stop_runtime_run(
    _state: &DesktopState,
    _request: RuntimeSupervisorStopRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    Err(removed_runtime_supervisor_error())
}

pub fn submit_runtime_run_input(
    _state: &DesktopState,
    _request: RuntimeSupervisorSubmitInputRequest,
) -> Result<String, CommandError> {
    Err(removed_runtime_supervisor_error())
}

pub fn update_runtime_run_controls(
    _state: &DesktopState,
    _request: RuntimeSupervisorUpdateControlsRequest,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    Err(removed_runtime_supervisor_error())
}

pub fn run_supervisor_sidecar_from_env() -> Result<(), CommandError> {
    Err(removed_runtime_supervisor_error())
}

fn default_runtime_run_controls() -> RuntimeRunControlStateRecord {
    RuntimeRunControlStateRecord {
        active: RuntimeRunActiveControlSnapshotRecord {
            provider_profile_id: None,
            model_id: OPENAI_CODEX_PROVIDER_ID.into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
            revision: 1,
            applied_at: now_timestamp(),
        },
        pending: None,
    }
}

fn removed_runtime_supervisor_error() -> CommandError {
    CommandError::user_fixable(
        "runtime_supervisor_removed",
        "Xero no longer supports legacy runtime supervisors. Use the Xero-owned agent run flow instead.",
    )
}

fn supervisor_registry_key(project_id: &str, agent_session_id: &str) -> String {
    format!("{project_id}\u{1f}{agent_session_id}")
}
