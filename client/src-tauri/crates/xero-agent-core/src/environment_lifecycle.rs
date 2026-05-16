use std::{
    collections::{BTreeMap, VecDeque},
    env,
    path::Path,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    AgentCoreStore, CoreError, CoreResult, MessageRole, NewMessageRecord, NewRuntimeEvent,
    RunStatus, RuntimeEventKind,
};

pub const ENVIRONMENT_LIFECYCLE_SCHEMA: &str = "xero.environment_lifecycle.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentLifecycleState {
    Created,
    WaitingForSandbox,
    PreparingRepository,
    LoadingProjectInstructions,
    RunningSetupScripts,
    SettingUpHooks,
    SettingUpSkillsPlugins,
    IndexingWorkspace,
    StartingConversation,
    Ready,
    Failed,
    Paused,
    Archived,
}

impl EnvironmentLifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::WaitingForSandbox => "waiting_for_sandbox",
            Self::PreparingRepository => "preparing_repository",
            Self::LoadingProjectInstructions => "loading_project_instructions",
            Self::RunningSetupScripts => "running_setup_scripts",
            Self::SettingUpHooks => "setting_up_hooks",
            Self::SettingUpSkillsPlugins => "setting_up_skills_plugins",
            Self::IndexingWorkspace => "indexing_workspace",
            Self::StartingConversation => "starting_conversation",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Paused => "paused",
            Self::Archived => "archived",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "created" => Some(Self::Created),
            "waiting_for_sandbox" => Some(Self::WaitingForSandbox),
            "preparing_repository" => Some(Self::PreparingRepository),
            "loading_project_instructions" => Some(Self::LoadingProjectInstructions),
            "running_setup_scripts" => Some(Self::RunningSetupScripts),
            "setting_up_hooks" => Some(Self::SettingUpHooks),
            "setting_up_skills_plugins" => Some(Self::SettingUpSkillsPlugins),
            "indexing_workspace" => Some(Self::IndexingWorkspace),
            "starting_conversation" => Some(Self::StartingConversation),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            "paused" => Some(Self::Paused),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Archived)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxGroupingPolicy {
    None,
    ReuseNewest,
    ReuseLeastBusy,
    ReuseByProject,
    DedicatedPerSession,
}

impl SandboxGroupingPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ReuseNewest => "reuse_newest",
            Self::ReuseLeastBusy => "reuse_least_busy",
            Self::ReuseByProject => "reuse_by_project",
            Self::DedicatedPerSession => "dedicated_per_session",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "reuse_newest" => Some(Self::ReuseNewest),
            "reuse_least_busy" => Some(Self::ReuseLeastBusy),
            "reuse_by_project" => Some(Self::ReuseByProject),
            "dedicated_per_session" => Some(Self::DedicatedPerSession),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentDiagnostic {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

impl EnvironmentDiagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            next_action: None,
        }
    }

    pub fn with_next_action(mut self, next_action: impl Into<String>) -> Self {
        self.next_action = Some(next_action.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentHealthCheckKind {
    FilesystemAccessible,
    GitStateAvailable,
    RequiredBinariesAvailable,
    ProviderCredentialsValid,
    ToolPacksAvailable,
    SemanticIndexStatus,
}

impl EnvironmentHealthCheckKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FilesystemAccessible => "filesystem_accessible",
            Self::GitStateAvailable => "git_state_available",
            Self::RequiredBinariesAvailable => "required_binaries_available",
            Self::ProviderCredentialsValid => "provider_credentials_valid",
            Self::ToolPacksAvailable => "tool_packs_available",
            Self::SemanticIndexStatus => "semantic_index_status",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentHealthStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentSemanticIndexState {
    Ready,
    Indexing,
    Stale,
    #[default]
    Empty,
    Failed,
    Unavailable,
}

impl EnvironmentSemanticIndexState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Indexing => "indexing",
            Self::Stale => "stale",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "ready" => Some(Self::Ready),
            "indexing" => Some(Self::Indexing),
            "stale" => Some(Self::Stale),
            "empty" => Some(Self::Empty),
            "failed" => Some(Self::Failed),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentHealthCheck {
    pub kind: EnvironmentHealthCheckKind,
    pub status: EnvironmentHealthStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<EnvironmentDiagnostic>,
    pub checked_at: String,
}

impl EnvironmentHealthCheck {
    pub fn passed(kind: EnvironmentHealthCheckKind, summary: impl Into<String>) -> Self {
        Self::new(kind, EnvironmentHealthStatus::Passed, summary, None)
    }

    pub fn warning(
        kind: EnvironmentHealthCheckKind,
        summary: impl Into<String>,
        diagnostic: EnvironmentDiagnostic,
    ) -> Self {
        Self::new(
            kind,
            EnvironmentHealthStatus::Warning,
            summary,
            Some(diagnostic),
        )
    }

    pub fn failed(
        kind: EnvironmentHealthCheckKind,
        summary: impl Into<String>,
        diagnostic: EnvironmentDiagnostic,
    ) -> Self {
        Self::new(
            kind,
            EnvironmentHealthStatus::Failed,
            summary,
            Some(diagnostic),
        )
    }

    pub fn skipped(kind: EnvironmentHealthCheckKind, summary: impl Into<String>) -> Self {
        Self::new(kind, EnvironmentHealthStatus::Skipped, summary, None)
    }

    fn new(
        kind: EnvironmentHealthCheckKind,
        status: EnvironmentHealthStatus,
        summary: impl Into<String>,
        diagnostic: Option<EnvironmentDiagnostic>,
    ) -> Self {
        Self {
            kind,
            status,
            summary: summary.into(),
            diagnostic,
            checked_at: now_timestamp(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentConfigTrust {
    TrustedProject,
    TrustedApp,
    UntrustedProject,
}

impl EnvironmentConfigTrust {
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::TrustedProject | Self::TrustedApp)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentApprovalStatus {
    NotRequired,
    Pending,
    Approved,
    Denied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentActionApproval {
    pub required: bool,
    pub status: EnvironmentApprovalStatus,
}

impl EnvironmentActionApproval {
    pub const fn not_required() -> Self {
        Self {
            required: false,
            status: EnvironmentApprovalStatus::NotRequired,
        }
    }

    pub const fn pending() -> Self {
        Self {
            required: true,
            status: EnvironmentApprovalStatus::Pending,
        }
    }

    pub const fn approved() -> Self {
        Self {
            required: true,
            status: EnvironmentApprovalStatus::Approved,
        }
    }

    pub const fn is_satisfied(self) -> bool {
        !self.required
            || matches!(
                self.status,
                EnvironmentApprovalStatus::NotRequired | EnvironmentApprovalStatus::Approved
            )
    }
}

impl Default for EnvironmentActionApproval {
    fn default() -> Self {
        Self::not_required()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentSetupScript {
    pub script_id: String,
    pub label: String,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub config_trust: EnvironmentConfigTrust,
    pub approval: EnvironmentActionApproval,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentGitHookSetup {
    pub hook_name: String,
    pub script_path: String,
    pub config_trust: EnvironmentConfigTrust,
    pub approval: EnvironmentActionApproval,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentSetupStepState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentSetupStep {
    pub step_id: String,
    pub label: String,
    pub state: EnvironmentSetupStepState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<EnvironmentDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentSetupStepResult {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingEnvironmentMessage {
    pub message_id: String,
    pub role: MessageRole,
    pub content: String,
    pub submitted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentLifecycleConfig {
    pub environment_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_id: Option<String>,
    pub sandbox_grouping_policy: SandboxGroupingPolicy,
    #[serde(default)]
    pub setup_scripts: Vec<EnvironmentSetupScript>,
    #[serde(default)]
    pub git_hooks: Vec<EnvironmentGitHookSetup>,
    #[serde(default)]
    pub required_binaries: Vec<String>,
    #[serde(default)]
    pub git_state_required: bool,
    #[serde(default)]
    pub provider_credentials_required: bool,
    #[serde(default)]
    pub provider_credentials_valid: bool,
    #[serde(default)]
    pub tool_packs: Vec<String>,
    #[serde(default)]
    pub semantic_index_required: bool,
    #[serde(default)]
    pub semantic_index_available: bool,
    #[serde(default)]
    pub semantic_index_state: EnvironmentSemanticIndexState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_index_requirement_reasons: Vec<String>,
    #[serde(default = "default_project_instructions_loaded")]
    pub project_instructions_loaded: bool,
}

impl EnvironmentLifecycleConfig {
    pub fn local(project_id: impl Into<String>, run_id: impl Into<String>) -> Self {
        let project_id = project_id.into();
        let run_id = run_id.into();
        Self {
            environment_id: format!("env-{project_id}-{run_id}"),
            project_id,
            run_id,
            workspace_root: ".".into(),
            sandbox_id: None,
            sandbox_grouping_policy: SandboxGroupingPolicy::None,
            setup_scripts: Vec::new(),
            git_hooks: Vec::new(),
            required_binaries: Vec::new(),
            git_state_required: false,
            provider_credentials_required: false,
            provider_credentials_valid: false,
            tool_packs: vec!["owned_agent_core".into()],
            semantic_index_required: false,
            semantic_index_available: false,
            semantic_index_state: EnvironmentSemanticIndexState::Empty,
            semantic_index_requirement_reasons: Vec::new(),
            project_instructions_loaded: true,
        }
    }
}

impl Default for EnvironmentLifecycleConfig {
    fn default() -> Self {
        Self::local("project", "run")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentLifecycleSnapshot {
    pub environment_id: String,
    pub project_id: String,
    pub run_id: String,
    pub state: EnvironmentLifecycleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<EnvironmentLifecycleState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_id: Option<String>,
    pub sandbox_grouping_policy: SandboxGroupingPolicy,
    #[serde(default)]
    pub pending_messages: Vec<PendingEnvironmentMessage>,
    #[serde(default)]
    pub health_checks: Vec<EnvironmentHealthCheck>,
    #[serde(default)]
    pub setup_steps: Vec<EnvironmentSetupStep>,
    #[serde(default)]
    pub semantic_index_required: bool,
    #[serde(default)]
    pub semantic_index_available: bool,
    #[serde(default)]
    pub semantic_index_state: EnvironmentSemanticIndexState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_index_requirement_reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<EnvironmentDiagnostic>,
    pub updated_at: String,
}

impl EnvironmentLifecycleSnapshot {
    fn new(config: &EnvironmentLifecycleConfig) -> Self {
        Self {
            environment_id: config.environment_id.clone(),
            project_id: config.project_id.clone(),
            run_id: config.run_id.clone(),
            state: EnvironmentLifecycleState::Created,
            previous_state: None,
            sandbox_id: config.sandbox_id.clone(),
            sandbox_grouping_policy: config.sandbox_grouping_policy,
            pending_messages: Vec::new(),
            health_checks: Vec::new(),
            setup_steps: Vec::new(),
            semantic_index_required: config.semantic_index_required,
            semantic_index_available: semantic_index_ready(config),
            semantic_index_state: config.semantic_index_state,
            semantic_index_requirement_reasons: config.semantic_index_requirement_reasons.clone(),
            diagnostic: None,
            updated_at: now_timestamp(),
        }
    }
}

#[derive(Debug, Default)]
struct EnvironmentLifecycleStateStore {
    environments: BTreeMap<(String, String), EnvironmentLifecycleSnapshot>,
    pending_messages: BTreeMap<(String, String), VecDeque<PendingEnvironmentMessage>>,
}

pub trait EnvironmentLifecycleExecutor: Send + Sync {
    fn run_setup_script(
        &self,
        script: &EnvironmentSetupScript,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic>;

    fn setup_git_hook(
        &self,
        hook: &EnvironmentGitHookSetup,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic>;

    fn setup_skills_plugins(
        &self,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic>;

    fn index_workspace(
        &self,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RecordingEnvironmentLifecycleExecutor;

impl EnvironmentLifecycleExecutor for RecordingEnvironmentLifecycleExecutor {
    fn run_setup_script(
        &self,
        script: &EnvironmentSetupScript,
        _config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        if script.command.is_empty() {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_setup_script_empty",
                format!(
                    "Setup script `{}` does not include a command to run.",
                    script.script_id
                ),
            )
            .with_next_action("Remove the setup script or add a trusted command."));
        }
        Ok(EnvironmentSetupStepResult {
            summary: format!("Setup script `{}` is approved for execution.", script.label),
        })
    }

    fn setup_git_hook(
        &self,
        hook: &EnvironmentGitHookSetup,
        _config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        Ok(EnvironmentSetupStepResult {
            summary: format!("Git hook `{}` is approved for setup.", hook.hook_name),
        })
    }

    fn setup_skills_plugins(
        &self,
        _config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        Ok(EnvironmentSetupStepResult {
            summary: "Skill and plugin setup descriptors were checked.".into(),
        })
    }

    fn index_workspace(
        &self,
        _config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        Ok(EnvironmentSetupStepResult {
            summary: "Workspace index setup was checked.".into(),
        })
    }
}

#[derive(Clone)]
pub struct EnvironmentLifecycleService<S> {
    store: S,
    state: Arc<Mutex<EnvironmentLifecycleStateStore>>,
    executor: Arc<dyn EnvironmentLifecycleExecutor>,
}

impl<S> EnvironmentLifecycleService<S>
where
    S: AgentCoreStore,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            state: Arc::new(Mutex::new(EnvironmentLifecycleStateStore::default())),
            executor: Arc::new(RecordingEnvironmentLifecycleExecutor),
        }
    }

    pub fn with_executor(store: S, executor: Arc<dyn EnvironmentLifecycleExecutor>) -> Self {
        Self {
            store,
            state: Arc::new(Mutex::new(EnvironmentLifecycleStateStore::default())),
            executor,
        }
    }

    pub fn queue_user_message(
        &self,
        project_id: &str,
        run_id: &str,
        content: impl Into<String>,
    ) -> CoreResult<EnvironmentLifecycleSnapshot> {
        let content = content.into();
        if content.trim().is_empty() {
            return Err(CoreError::invalid_request(
                "agent_environment_pending_message_empty",
                "Pending environment messages must include non-empty content.",
            ));
        }

        let mut state = self.lock_state()?;
        let key = (project_id.to_string(), run_id.to_string());
        let Some(snapshot) = state.environments.get_mut(&key) else {
            return Err(CoreError::invalid_request(
                "agent_environment_not_found",
                format!("Environment for run `{run_id}` was not found."),
            ));
        };
        if snapshot.state.is_ready() {
            drop(state);
            self.store.append_message(NewMessageRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                role: MessageRole::User,
                content: content.clone(),
                provider_metadata: None,
            })?;
            self.store.append_event(NewRuntimeEvent {
                project_id: project_id.into(),
                run_id: run_id.into(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({ "role": "user", "text": content }),
            })?;
            return self.snapshot(project_id, run_id);
        }

        let message;
        let snapshot = {
            let next_index = snapshot.pending_messages.len().saturating_add(1);
            message = PendingEnvironmentMessage {
                message_id: format!("pending-env-message-{next_index}"),
                role: MessageRole::User,
                content,
                submitted_at: now_timestamp(),
            };
            snapshot.pending_messages.push(message.clone());
            snapshot.updated_at = now_timestamp();
            snapshot.clone()
        };
        state
            .pending_messages
            .entry(key)
            .or_default()
            .push_back(message);
        drop(state);
        self.emit_update(
            &snapshot,
            Some("Queued user message while environment starts."),
            None,
        )?;
        Ok(snapshot)
    }

    pub fn start_environment(
        &self,
        config: EnvironmentLifecycleConfig,
    ) -> CoreResult<EnvironmentLifecycleSnapshot> {
        validate_environment_config(&config)?;
        self.store.load_run(&config.project_id, &config.run_id)?;
        let mut snapshot = EnvironmentLifecycleSnapshot::new(&config);
        snapshot.pending_messages =
            self.pending_messages_for(&config.project_id, &config.run_id)?;
        self.save_snapshot(snapshot.clone())?;
        self.emit_update(&snapshot, Some("Environment lifecycle created."), None)?;

        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::WaitingForSandbox,
            "Waiting for the selected local sandbox profile.",
            None,
        )?;
        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::PreparingRepository,
            "Preparing repository access and startup metadata.",
            None,
        )?;
        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::LoadingProjectInstructions,
            "Loading project instructions for the runtime context.",
            None,
        )?;
        if !config.project_instructions_loaded {
            return self.fail_environment(
                &mut snapshot,
                EnvironmentDiagnostic::new(
                    "agent_environment_project_instructions_unavailable",
                    "Project instructions could not be loaded before startup.",
                )
                .with_next_action("Check repository instructions and app-data permissions."),
            );
        }

        self.run_setup_scripts(&mut snapshot, &config)?;
        if snapshot.state == EnvironmentLifecycleState::Paused {
            return Ok(snapshot);
        }
        self.setup_git_hooks(&mut snapshot, &config)?;
        if snapshot.state == EnvironmentLifecycleState::Paused {
            return Ok(snapshot);
        }
        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::SettingUpSkillsPlugins,
            "Setting up skill and plugin descriptors.",
            None,
        )?;
        if let Err(diagnostic) = self.executor.setup_skills_plugins(&config) {
            return self.fail_environment(&mut snapshot, diagnostic);
        }
        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::IndexingWorkspace,
            "Checking workspace index readiness.",
            None,
        )?;
        if let Err(diagnostic) = self.executor.index_workspace(&config) {
            snapshot.health_checks = collect_health_checks(&config);
            self.save_snapshot(snapshot.clone())?;
            return self.fail_environment(&mut snapshot, diagnostic);
        }

        snapshot.health_checks = collect_health_checks(&config);
        self.save_snapshot(snapshot.clone())?;
        if let Some(diagnostic) = first_failed_health_diagnostic(&snapshot.health_checks) {
            return self.fail_environment(&mut snapshot, diagnostic);
        }

        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::StartingConversation,
            "Environment is healthy; preparing to start the agent conversation.",
            None,
        )?;
        self.transition(
            &mut snapshot,
            EnvironmentLifecycleState::Ready,
            "Environment is ready.",
            None,
        )?;
        self.store
            .update_run_status(&config.project_id, &config.run_id, RunStatus::Running)?;
        self.deliver_pending_messages(&mut snapshot)?;
        Ok(snapshot)
    }

    pub fn snapshot(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> CoreResult<EnvironmentLifecycleSnapshot> {
        let state = self.lock_state()?;
        state
            .environments
            .get(&(project_id.to_string(), run_id.to_string()))
            .cloned()
            .ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_environment_not_found",
                    format!("Environment for run `{run_id}` was not found."),
                )
            })
    }

    fn run_setup_scripts(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
        config: &EnvironmentLifecycleConfig,
    ) -> CoreResult<()> {
        self.transition(
            snapshot,
            EnvironmentLifecycleState::RunningSetupScripts,
            "Checking trusted setup scripts.",
            None,
        )?;
        if config.setup_scripts.is_empty() {
            snapshot.setup_steps.push(EnvironmentSetupStep {
                step_id: "setup_scripts".into(),
                label: "No setup scripts configured".into(),
                state: EnvironmentSetupStepState::Skipped,
                diagnostic: None,
            });
            self.save_snapshot(snapshot.clone())?;
            return Ok(());
        }

        for script in &config.setup_scripts {
            if !script.config_trust.is_trusted() {
                return self
                    .fail_environment(
                        snapshot,
                        EnvironmentDiagnostic::new(
                            "agent_environment_setup_script_untrusted",
                            format!(
                                "Setup script `{}` comes from untrusted project configuration.",
                                script.script_id
                            ),
                        )
                        .with_next_action(
                            "Move setup scripts into trusted project or app configuration.",
                        ),
                    )
                    .map(drop);
            }
            if !script.approval.is_satisfied() {
                return self.pause_for_approval(
                    snapshot,
                    "setup_script",
                    &script.script_id,
                    format!("Setup script `{}` requires approval.", script.label),
                );
            }
            match self.executor.run_setup_script(script, config) {
                Ok(result) => snapshot.setup_steps.push(EnvironmentSetupStep {
                    step_id: script.script_id.clone(),
                    label: result.summary,
                    state: EnvironmentSetupStepState::Succeeded,
                    diagnostic: None,
                }),
                Err(diagnostic) if script.required => {
                    return self.fail_environment(snapshot, diagnostic).map(drop);
                }
                Err(diagnostic) => snapshot.setup_steps.push(EnvironmentSetupStep {
                    step_id: script.script_id.clone(),
                    label: script.label.clone(),
                    state: EnvironmentSetupStepState::Failed,
                    diagnostic: Some(diagnostic),
                }),
            }
        }
        self.save_snapshot(snapshot.clone())
    }

    fn setup_git_hooks(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
        config: &EnvironmentLifecycleConfig,
    ) -> CoreResult<()> {
        self.transition(
            snapshot,
            EnvironmentLifecycleState::SettingUpHooks,
            "Checking git hook setup requests.",
            None,
        )?;
        if config.git_hooks.is_empty() {
            snapshot.setup_steps.push(EnvironmentSetupStep {
                step_id: "git_hooks".into(),
                label: "No git hooks configured".into(),
                state: EnvironmentSetupStepState::Skipped,
                diagnostic: None,
            });
            self.save_snapshot(snapshot.clone())?;
            return Ok(());
        }

        for hook in &config.git_hooks {
            if !hook.config_trust.is_trusted() {
                return self
                    .fail_environment(
                        snapshot,
                        EnvironmentDiagnostic::new(
                            "agent_environment_git_hook_untrusted",
                            format!(
                                "Git hook `{}` comes from untrusted configuration.",
                                hook.hook_name
                            ),
                        )
                        .with_next_action(
                            "Approve hook setup only from trusted project or app config.",
                        ),
                    )
                    .map(drop);
            }
            if !hook.approval.is_satisfied() {
                return self.pause_for_approval(
                    snapshot,
                    "git_hook_setup",
                    &hook.hook_name,
                    format!("Git hook `{}` requires approval.", hook.hook_name),
                );
            }
            match self.executor.setup_git_hook(hook, config) {
                Ok(result) => snapshot.setup_steps.push(EnvironmentSetupStep {
                    step_id: hook.hook_name.clone(),
                    label: result.summary,
                    state: EnvironmentSetupStepState::Succeeded,
                    diagnostic: None,
                }),
                Err(diagnostic) if hook.required => {
                    return self.fail_environment(snapshot, diagnostic).map(drop);
                }
                Err(diagnostic) => snapshot.setup_steps.push(EnvironmentSetupStep {
                    step_id: hook.hook_name.clone(),
                    label: hook.script_path.clone(),
                    state: EnvironmentSetupStepState::Failed,
                    diagnostic: Some(diagnostic),
                }),
            }
        }
        self.save_snapshot(snapshot.clone())
    }

    fn pause_for_approval(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
        action_type: &str,
        action_id: &str,
        detail: String,
    ) -> CoreResult<()> {
        snapshot.setup_steps.push(EnvironmentSetupStep {
            step_id: action_id.into(),
            label: detail.clone(),
            state: EnvironmentSetupStepState::ApprovalRequired,
            diagnostic: None,
        });
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ApprovalRequired,
            trace: None,
            payload: json!({
                "actionId": format!("environment-{action_type}-{action_id}"),
                "boundaryId": "environment_lifecycle",
                "actionType": action_type,
                "title": "Environment setup approval required",
                "detail": detail,
            }),
        })?;
        self.transition(
            snapshot,
            EnvironmentLifecycleState::Paused,
            "Environment startup paused for operator approval.",
            None,
        )?;
        self.store
            .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Paused)?;
        Ok(())
    }

    fn fail_environment(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
        diagnostic: EnvironmentDiagnostic,
    ) -> CoreResult<EnvironmentLifecycleSnapshot> {
        self.transition(
            snapshot,
            EnvironmentLifecycleState::Failed,
            "Environment startup failed.",
            Some(diagnostic.clone()),
        )?;
        self.store
            .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: None,
            payload: json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
                "retryable": false,
            }),
        })?;
        Err(CoreError::invalid_request(
            "agent_environment_startup_failed",
            "Environment startup failed; inspect the lifecycle diagnostic event for details.",
        ))
    }

    fn transition(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
        state: EnvironmentLifecycleState,
        detail: &str,
        diagnostic: Option<EnvironmentDiagnostic>,
    ) -> CoreResult<()> {
        snapshot.previous_state = Some(snapshot.state);
        snapshot.state = state;
        snapshot.diagnostic = diagnostic.clone();
        snapshot.updated_at = now_timestamp();
        self.save_snapshot(snapshot.clone())?;
        self.emit_update(snapshot, Some(detail), diagnostic.as_ref())
    }

    fn deliver_pending_messages(
        &self,
        snapshot: &mut EnvironmentLifecycleSnapshot,
    ) -> CoreResult<()> {
        let key = (snapshot.project_id.clone(), snapshot.run_id.clone());
        let pending = {
            let mut state = self.lock_state()?;
            state.pending_messages.remove(&key).unwrap_or_default()
        };
        if pending.is_empty() {
            return Ok(());
        }

        for message in pending {
            self.store.append_message(NewMessageRecord {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                role: message.role,
                content: message.content.clone(),
                provider_metadata: None,
            })?;
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({ "role": "user", "text": message.content }),
            })?;
        }
        snapshot.pending_messages.clear();
        self.save_snapshot(snapshot.clone())?;
        self.emit_update(
            snapshot,
            Some("Delivered queued user messages after environment readiness."),
            None,
        )
    }

    fn emit_update(
        &self,
        snapshot: &EnvironmentLifecycleSnapshot,
        detail: Option<&str>,
        diagnostic: Option<&EnvironmentDiagnostic>,
    ) -> CoreResult<()> {
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
            trace: None,
            payload: json!({
                "schema": ENVIRONMENT_LIFECYCLE_SCHEMA,
                "environmentId": snapshot.environment_id,
                "state": snapshot.state,
                "previousState": snapshot.previous_state,
                "sandboxId": snapshot.sandbox_id,
                "sandboxGroupingPolicy": snapshot.sandbox_grouping_policy,
                "pendingMessageCount": snapshot.pending_messages.len(),
                "healthChecks": snapshot.health_checks,
                "setupSteps": snapshot.setup_steps,
                "semanticIndexRequired": snapshot.semantic_index_required,
                "semanticIndexAvailable": snapshot.semantic_index_available,
                "semanticIndexState": snapshot.semantic_index_state,
                "semanticIndexRequirementReasons": snapshot.semantic_index_requirement_reasons,
                "detail": detail,
                "diagnostic": diagnostic,
            }),
        })?;
        Ok(())
    }

    fn save_snapshot(&self, snapshot: EnvironmentLifecycleSnapshot) -> CoreResult<()> {
        let mut state = self.lock_state()?;
        state.environments.insert(
            (snapshot.project_id.clone(), snapshot.run_id.clone()),
            snapshot,
        );
        Ok(())
    }

    fn lock_state(&self) -> CoreResult<std::sync::MutexGuard<'_, EnvironmentLifecycleStateStore>> {
        self.state.lock().map_err(|_| {
            CoreError::system_fault(
                "agent_environment_lifecycle_lock_failed",
                "The environment lifecycle state lock was poisoned.",
            )
        })
    }

    fn pending_messages_for(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> CoreResult<Vec<PendingEnvironmentMessage>> {
        let state = self.lock_state()?;
        Ok(state
            .pending_messages
            .get(&(project_id.to_string(), run_id.to_string()))
            .map(|messages| messages.iter().cloned().collect())
            .unwrap_or_default())
    }
}

fn validate_environment_config(config: &EnvironmentLifecycleConfig) -> CoreResult<()> {
    for (field, value) in [
        ("environmentId", &config.environment_id),
        ("projectId", &config.project_id),
        ("runId", &config.run_id),
        ("workspaceRoot", &config.workspace_root),
    ] {
        if value.trim().is_empty() {
            return Err(CoreError::invalid_request(
                "agent_environment_required_field_missing",
                format!("`{field}` is required for environment lifecycle startup."),
            ));
        }
    }
    Ok(())
}

pub fn semantic_workspace_prompt_requirement_reasons(prompt: &str) -> Vec<String> {
    let lowered = prompt.to_lowercase();
    let mut reasons = Vec::new();
    if contains_any(
        &lowered,
        &[
            "tool:workspace_index",
            "tool:workspace_query",
            "tool:semantic_search",
            "workspace_index",
        ],
    ) {
        reasons.push("prompt invoked the workspace-index tool".into());
    }
    if contains_any(
        &lowered,
        &["related-tests", "related tests", "related_tests"],
    ) {
        reasons.push("prompt requested related-tests workspace retrieval".into());
    }
    if contains_any(
        &lowered,
        &[
            "change impact",
            "impact analysis",
            "impact mode",
            "workspace impact",
            "change_impact",
        ],
    ) {
        reasons.push("prompt requested change-impact workspace retrieval".into());
    }
    if contains_any(
        &lowered,
        &[
            "semantic workspace",
            "semantic search",
            "semantic code search",
            "workspace index",
        ],
    ) {
        reasons.push("prompt requested semantic workspace search".into());
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn collect_health_checks(config: &EnvironmentLifecycleConfig) -> Vec<EnvironmentHealthCheck> {
    vec![
        filesystem_health(config),
        git_health(config),
        required_binaries_health(config),
        provider_credentials_health(config),
        tool_packs_health(config),
        semantic_index_health(config),
    ]
}

fn filesystem_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    if Path::new(&config.workspace_root).is_dir() {
        EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::FilesystemAccessible,
            "Workspace filesystem is accessible.",
        )
    } else {
        EnvironmentHealthCheck::failed(
            EnvironmentHealthCheckKind::FilesystemAccessible,
            format!(
                "Workspace root `{}` is not accessible as a directory.",
                config.workspace_root
            ),
            EnvironmentDiagnostic::new(
                "agent_environment_workspace_unavailable",
                "The configured workspace root is not accessible.",
            )
            .with_next_action("Confirm the project path and app-data project record."),
        )
    }
}

fn git_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    let git_dir = Path::new(&config.workspace_root).join(".git");
    if git_dir.exists() {
        return EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::GitStateAvailable,
            "Git state is available.",
        );
    }
    let diagnostic = EnvironmentDiagnostic::new(
        "agent_environment_git_state_missing",
        "No `.git` directory was found at the workspace root.",
    )
    .with_next_action("Open a git-backed project or disable git-required startup policy.");
    if config.git_state_required {
        EnvironmentHealthCheck::failed(
            EnvironmentHealthCheckKind::GitStateAvailable,
            "Git state is required but unavailable.",
            diagnostic,
        )
    } else {
        EnvironmentHealthCheck::warning(
            EnvironmentHealthCheckKind::GitStateAvailable,
            "Git state is unavailable; git-aware tools may be limited.",
            diagnostic,
        )
    }
}

fn required_binaries_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    if config.required_binaries.is_empty() {
        return EnvironmentHealthCheck::skipped(
            EnvironmentHealthCheckKind::RequiredBinariesAvailable,
            "No environment-specific binaries were required.",
        );
    }
    let missing = config
        .required_binaries
        .iter()
        .filter(|binary| !binary_on_path(binary))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::RequiredBinariesAvailable,
            "Required binaries are available on PATH.",
        )
    } else {
        EnvironmentHealthCheck::failed(
            EnvironmentHealthCheckKind::RequiredBinariesAvailable,
            format!("Missing required binaries: {}.", missing.join(", ")),
            EnvironmentDiagnostic::new(
                "agent_environment_required_binary_missing",
                format!("Missing required binaries: {}.", missing.join(", ")),
            )
            .with_next_action("Install the missing tools or update trusted environment config."),
        )
    }
}

fn provider_credentials_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    if !config.provider_credentials_required {
        return EnvironmentHealthCheck::skipped(
            EnvironmentHealthCheckKind::ProviderCredentialsValid,
            "Provider credentials are not required for this startup check.",
        );
    }
    if config.provider_credentials_valid {
        EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::ProviderCredentialsValid,
            "Provider credentials are configured.",
        )
    } else {
        EnvironmentHealthCheck::failed(
            EnvironmentHealthCheckKind::ProviderCredentialsValid,
            "Provider credentials are required but unavailable.",
            EnvironmentDiagnostic::new(
                "agent_environment_provider_credentials_missing",
                "Provider credentials are required before the agent loop can start.",
            )
            .with_next_action("Open provider settings and complete login or API key setup."),
        )
    }
}

fn tool_packs_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    if config.tool_packs.is_empty() {
        EnvironmentHealthCheck::warning(
            EnvironmentHealthCheckKind::ToolPacksAvailable,
            "No tool packs were registered for this environment.",
            EnvironmentDiagnostic::new(
                "agent_environment_tool_packs_empty",
                "The environment started without any declared tool packs.",
            )
            .with_next_action("Check runtime tool registry configuration."),
        )
    } else {
        EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::ToolPacksAvailable,
            format!("{} tool pack(s) are available.", config.tool_packs.len()),
        )
    }
}

fn semantic_index_health(config: &EnvironmentLifecycleConfig) -> EnvironmentHealthCheck {
    let state = semantic_index_effective_state(config);
    if state.is_ready() {
        return EnvironmentHealthCheck::passed(
            EnvironmentHealthCheckKind::SemanticIndexStatus,
            "Semantic workspace index is ready.",
        );
    }
    let diagnostic = semantic_index_diagnostic(state);
    if config.semantic_index_required {
        EnvironmentHealthCheck::failed(
            EnvironmentHealthCheckKind::SemanticIndexStatus,
            format!(
                "Semantic workspace index is required but {}.",
                semantic_index_not_ready_phrase(state)
            ),
            diagnostic,
        )
    } else {
        EnvironmentHealthCheck::warning(
            EnvironmentHealthCheckKind::SemanticIndexStatus,
            format!(
                "Semantic workspace index is optional and {}.",
                semantic_index_not_ready_phrase(state)
            ),
            diagnostic,
        )
    }
}

fn semantic_index_ready(config: &EnvironmentLifecycleConfig) -> bool {
    semantic_index_effective_state(config).is_ready()
}

fn semantic_index_effective_state(
    config: &EnvironmentLifecycleConfig,
) -> EnvironmentSemanticIndexState {
    if config.semantic_index_state.is_ready() || config.semantic_index_available {
        EnvironmentSemanticIndexState::Ready
    } else {
        config.semantic_index_state
    }
}

fn semantic_index_not_ready_phrase(state: EnvironmentSemanticIndexState) -> &'static str {
    match state {
        EnvironmentSemanticIndexState::Ready => "ready",
        EnvironmentSemanticIndexState::Indexing => "currently indexing",
        EnvironmentSemanticIndexState::Stale => "stale",
        EnvironmentSemanticIndexState::Empty => "empty",
        EnvironmentSemanticIndexState::Failed => "failed",
        EnvironmentSemanticIndexState::Unavailable => "unavailable",
    }
}

fn semantic_index_diagnostic(state: EnvironmentSemanticIndexState) -> EnvironmentDiagnostic {
    let (code, message, next_action) = match state {
        EnvironmentSemanticIndexState::Ready => (
            "agent_environment_workspace_index_ready",
            "Semantic workspace index is ready.",
            "Continue the agent run.",
        ),
        EnvironmentSemanticIndexState::Indexing => (
            "agent_environment_workspace_index_indexing",
            "Semantic workspace index is currently rebuilding.",
            "Wait for workspace indexing to finish before starting a semantic-search-required run.",
        ),
        EnvironmentSemanticIndexState::Stale => (
            "agent_environment_workspace_index_stale",
            "Semantic workspace index is stale for the current workspace.",
            "Run workspace indexing before starting a semantic-search-required agent run.",
        ),
        EnvironmentSemanticIndexState::Empty => (
            "agent_environment_workspace_index_empty",
            "Semantic workspace index has not been built yet.",
            "Run workspace indexing before starting a semantic-search-required agent run.",
        ),
        EnvironmentSemanticIndexState::Failed => (
            "agent_environment_workspace_index_failed",
            "Semantic workspace index failed during the previous rebuild.",
            "Review workspace-index diagnostics, repair the failure, and reindex.",
        ),
        EnvironmentSemanticIndexState::Unavailable => (
            "agent_environment_workspace_index_unavailable",
            "Semantic workspace index state is unavailable.",
            "Repair app-data project state permissions and re-run workspace indexing.",
        ),
    };
    EnvironmentDiagnostic::new(code, message).with_next_action(next_action)
}

fn first_failed_health_diagnostic(
    checks: &[EnvironmentHealthCheck],
) -> Option<EnvironmentDiagnostic> {
    checks
        .iter()
        .find(|check| check.status == EnvironmentHealthStatus::Failed)
        .and_then(|check| check.diagnostic.clone())
}

fn binary_on_path(binary: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(binary).is_file())
}

fn default_project_instructions_loaded() -> bool {
    true
}

fn now_timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("unix:{seconds}")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        AgentRuntimeFacade, EnvironmentLifecycleExecutor, FakeProviderRuntime, NewRunRecord,
        ProviderSelection, RunControls, StartRunRequest,
    };

    #[derive(Debug)]
    struct FailingSetupExecutor;

    impl EnvironmentLifecycleExecutor for FailingSetupExecutor {
        fn run_setup_script(
            &self,
            script: &EnvironmentSetupScript,
            _config: &EnvironmentLifecycleConfig,
        ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
            Err(EnvironmentDiagnostic::new(
                "setup_failed",
                format!("Setup script `{}` failed.", script.script_id),
            ))
        }

        fn setup_git_hook(
            &self,
            _hook: &EnvironmentGitHookSetup,
            _config: &EnvironmentLifecycleConfig,
        ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
            Ok(EnvironmentSetupStepResult {
                summary: "hook ok".into(),
            })
        }

        fn setup_skills_plugins(
            &self,
            _config: &EnvironmentLifecycleConfig,
        ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
            Ok(EnvironmentSetupStepResult {
                summary: "skills ok".into(),
            })
        }

        fn index_workspace(
            &self,
            _config: &EnvironmentLifecycleConfig,
        ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
            Ok(EnvironmentSetupStepResult {
                summary: "index ok".into(),
            })
        }
    }

    fn run_record(project_id: &str, run_id: &str) -> NewRunRecord {
        NewRunRecord {
            trace_id: None,
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: project_id.into(),
            agent_session_id: "session-1".into(),
            run_id: run_id.into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            prompt: "test prompt".into(),
        }
    }

    fn insert_run(store: &crate::InMemoryAgentCoreStore, project_id: &str, run_id: &str) {
        store
            .insert_run(run_record(project_id, run_id))
            .expect("insert run");
    }

    fn semantic_health(snapshot: &EnvironmentLifecycleSnapshot) -> &EnvironmentHealthCheck {
        snapshot
            .health_checks
            .iter()
            .find(|check| check.kind == EnvironmentHealthCheckKind::SemanticIndexStatus)
            .expect("semantic index health check")
    }

    #[test]
    fn lifecycle_queues_pending_messages_until_ready() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store.clone());
        service
            .save_snapshot(EnvironmentLifecycleSnapshot::new(
                &EnvironmentLifecycleConfig::local("project-1", "run-1"),
            ))
            .expect("save snapshot");

        let queued = service
            .queue_user_message("project-1", "run-1", "Please add this once ready.")
            .expect("queue message");
        assert_eq!(queued.pending_messages.len(), 1);

        let ready = service
            .start_environment(EnvironmentLifecycleConfig::local("project-1", "run-1"))
            .expect("environment should become ready");
        assert_eq!(ready.state, EnvironmentLifecycleState::Ready);
        let run = store.load_run("project-1", "run-1").expect("load run");
        assert!(run
            .messages
            .iter()
            .any(|message| message.content == "Please add this once ready."));
    }

    #[test]
    fn setup_failure_marks_run_failed_before_provider_loop() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::with_executor(
            store.clone(),
            Arc::new(FailingSetupExecutor),
        );
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.setup_scripts.push(EnvironmentSetupScript {
            script_id: "setup".into(),
            label: "setup".into(),
            command: vec!["false".into()],
            cwd: None,
            config_trust: EnvironmentConfigTrust::TrustedProject,
            approval: EnvironmentActionApproval::not_required(),
            required: true,
        });

        let error = service
            .start_environment(config)
            .expect_err("setup failure should stop startup");
        assert_eq!(error.code, "agent_environment_startup_failed");
        let run = store.load_run("project-1", "run-1").expect("load run");
        assert_eq!(run.status, RunStatus::Failed);
        assert!(!run
            .events
            .iter()
            .any(|event| event.event_kind == RuntimeEventKind::ReasoningSummary));
        assert!(run
            .events
            .iter()
            .any(|event| event.event_kind == RuntimeEventKind::RunFailed));
    }

    #[test]
    fn required_empty_semantic_index_blocks_lifecycle() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store.clone());
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.semantic_index_required = true;
        config.semantic_index_state = EnvironmentSemanticIndexState::Empty;
        config.semantic_index_requirement_reasons =
            vec!["prompt requested semantic workspace search".into()];

        let error = service
            .start_environment(config)
            .expect_err("empty required semantic index should block startup");
        assert_eq!(error.code, "agent_environment_startup_failed");
        let snapshot = service
            .snapshot("project-1", "run-1")
            .expect("load lifecycle snapshot");
        assert_eq!(
            semantic_health(&snapshot).status,
            EnvironmentHealthStatus::Failed
        );
        assert_eq!(
            semantic_health(&snapshot)
                .diagnostic
                .as_ref()
                .expect("semantic diagnostic")
                .code,
            "agent_environment_workspace_index_empty"
        );
    }

    #[test]
    fn required_stale_semantic_index_blocks_lifecycle() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store);
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.semantic_index_required = true;
        config.semantic_index_state = EnvironmentSemanticIndexState::Stale;
        config.semantic_index_requirement_reasons =
            vec!["prompt requested related-tests workspace retrieval".into()];

        let error = service
            .start_environment(config)
            .expect_err("stale required semantic index should block startup");
        assert_eq!(error.code, "agent_environment_startup_failed");
        let snapshot = service
            .snapshot("project-1", "run-1")
            .expect("load lifecycle snapshot");
        assert_eq!(
            semantic_health(&snapshot)
                .diagnostic
                .as_ref()
                .expect("semantic diagnostic")
                .code,
            "agent_environment_workspace_index_stale"
        );
    }

    #[test]
    fn optional_empty_semantic_index_warns_without_blocking_lifecycle() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store);
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.semantic_index_required = false;
        config.semantic_index_state = EnvironmentSemanticIndexState::Empty;

        let snapshot = service
            .start_environment(config)
            .expect("optional empty semantic index should not block startup");
        assert_eq!(snapshot.state, EnvironmentLifecycleState::Ready);
        assert_eq!(
            semantic_health(&snapshot).status,
            EnvironmentHealthStatus::Warning
        );
    }

    #[test]
    fn required_ready_semantic_index_allows_lifecycle() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store);
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.semantic_index_required = true;
        config.semantic_index_available = true;
        config.semantic_index_state = EnvironmentSemanticIndexState::Ready;
        config.semantic_index_requirement_reasons =
            vec!["prompt requested semantic workspace search".into()];

        let snapshot = service
            .start_environment(config)
            .expect("ready required semantic index should allow startup");
        assert_eq!(snapshot.state, EnvironmentLifecycleState::Ready);
        assert_eq!(
            semantic_health(&snapshot).status,
            EnvironmentHealthStatus::Passed
        );
    }

    #[test]
    fn git_hook_setup_pauses_for_approval() {
        let store = crate::InMemoryAgentCoreStore::default();
        insert_run(&store, "project-1", "run-1");
        let service = EnvironmentLifecycleService::new(store.clone());
        let mut config = EnvironmentLifecycleConfig::local("project-1", "run-1");
        config.git_hooks.push(EnvironmentGitHookSetup {
            hook_name: "pre-commit".into(),
            script_path: ".git/hooks/pre-commit".into(),
            config_trust: EnvironmentConfigTrust::TrustedProject,
            approval: EnvironmentActionApproval::pending(),
            required: true,
        });

        let snapshot = service
            .start_environment(config)
            .expect("approval wait is a paused lifecycle state");
        assert_eq!(snapshot.state, EnvironmentLifecycleState::Paused);
        let run = store.load_run("project-1", "run-1").expect("load run");
        assert_eq!(run.status, RunStatus::Paused);
        assert!(run
            .events
            .iter()
            .any(|event| event.event_kind == RuntimeEventKind::ApprovalRequired));
    }

    #[test]
    fn fake_provider_records_environment_lifecycle_before_provider_turn() {
        let runtime = FakeProviderRuntime::default();
        let snapshot = runtime
            .start_run(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                prompt: "Start with lifecycle.".into(),
                provider: ProviderSelection {
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                },
                controls: Some(RunControls {
                    runtime_agent_id: "engineer".into(),
                    agent_definition_id: Some("engineer".into()),
                    agent_definition_version: Some(1),
                    thinking_effort: None,
                    approval_mode: "yolo".into(),
                    plan_mode_required: false,
                }),
            })
            .expect("fake provider run should complete");

        let ready_event = snapshot
            .events
            .iter()
            .find(|event| {
                event.event_kind == RuntimeEventKind::EnvironmentLifecycleUpdate
                    && event
                        .payload
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        == Some("ready")
            })
            .expect("ready lifecycle event");
        let provider_event = snapshot
            .events
            .iter()
            .find(|event| event.event_kind == RuntimeEventKind::ReasoningSummary)
            .expect("provider reasoning event");
        assert!(ready_event.id < provider_event.id);
    }
}
