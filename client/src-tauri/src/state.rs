use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    auth::{
        ActiveAuthFlowRegistry, AnthropicAuthConfig, AuthFlowError, OpenAiCodexAuthConfig,
        OpenAiCompatibleAuthConfig, OpenRouterAuthConfig,
    },
    commands::CommandError,
    global_db::global_database_path,
    provider_models::{
        ProviderModelCatalogRefreshRegistry, PROVIDER_MODEL_CATALOG_CACHE_FILE_NAME,
    },
    provider_profiles::{PROVIDER_PROFILES_FILE_NAME, PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME},
    runtime::{
        openai_codex_provider, AgentProviderConfig, AgentRunSupervisor, AutonomousWebConfig,
        ResolvedRuntimeProvider, RuntimeStreamController, RuntimeSupervisorController,
    },
};

pub const REGISTRY_FILE_NAME: &str = "project-registry.json";
pub const RUNTIME_SETTINGS_FILE_NAME: &str = "runtime-settings.json";
pub const DICTATION_SETTINGS_FILE_NAME: &str = "dictation-settings.json";
pub const MCP_REGISTRY_FILE_NAME: &str = "mcp-registry.json";
pub const SKILL_SOURCE_SETTINGS_FILE_NAME: &str = "skill-sources.json";
pub const OPENROUTER_CREDENTIAL_FILE_NAME: &str = "openrouter-credentials.json";
pub const AUTONOMOUS_SKILL_CACHE_DIRECTORY_NAME: &str = "autonomous-skills";

#[derive(Debug, Clone, Default)]
pub struct ImportFailpoints {
    pub fail_registry_write: bool,
    pub fail_exclude_write: bool,
    pub fail_migration: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeStreamFailpoints {
    pub fail_after_tool_start: bool,
    pub invalid_tool_payload: bool,
    pub fail_pending_approval_sync: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DesktopState {
    global_db_path_override: Option<PathBuf>,
    registry_file_override: Option<PathBuf>,
    auth_store_file_override: Option<PathBuf>,
    notification_credential_store_file_override: Option<PathBuf>,
    provider_profiles_file_override: Option<PathBuf>,
    provider_profile_credential_store_file_override: Option<PathBuf>,
    provider_model_catalog_cache_file_override: Option<PathBuf>,
    runtime_settings_file_override: Option<PathBuf>,
    dictation_settings_file_override: Option<PathBuf>,
    mcp_registry_file_override: Option<PathBuf>,
    skill_source_settings_file_override: Option<PathBuf>,
    openrouter_credential_file_override: Option<PathBuf>,
    autonomous_skill_cache_dir_override: Option<PathBuf>,
    runtime_supervisor_binary_override: Option<PathBuf>,
    openai_auth_config_override: Option<OpenAiCodexAuthConfig>,
    openai_compatible_auth_config_override: Option<OpenAiCompatibleAuthConfig>,
    openrouter_auth_config_override: Option<OpenRouterAuthConfig>,
    anthropic_auth_config_override: Option<AnthropicAuthConfig>,
    autonomous_web_config_override: Option<AutonomousWebConfig>,
    owned_agent_provider_config_override: Option<AgentProviderConfig>,
    import_failpoints: ImportFailpoints,
    runtime_stream_failpoints: RuntimeStreamFailpoints,
    runtime_stream_controller: RuntimeStreamController,
    runtime_supervisor_controller: RuntimeSupervisorController,
    agent_run_supervisor: AgentRunSupervisor,
    provider_model_catalog_refresh_registry: ProviderModelCatalogRefreshRegistry,
    active_auth_flows: ActiveAuthFlowRegistry,
}

impl DesktopState {
    pub fn with_global_db_path_override(mut self, path: PathBuf) -> Self {
        self.global_db_path_override = Some(path);
        self
    }

    pub fn with_registry_file_override(mut self, path: PathBuf) -> Self {
        self.registry_file_override = Some(path);
        self
    }

    pub fn with_auth_store_file_override(mut self, path: PathBuf) -> Self {
        self.auth_store_file_override = Some(path);
        self
    }

    pub fn with_notification_credential_store_file_override(mut self, path: PathBuf) -> Self {
        self.notification_credential_store_file_override = Some(path);
        self
    }

    pub fn with_provider_profiles_file_override(mut self, path: PathBuf) -> Self {
        self.provider_profiles_file_override = Some(path);
        self
    }

    pub fn with_provider_profile_credential_store_file_override(mut self, path: PathBuf) -> Self {
        self.provider_profile_credential_store_file_override = Some(path);
        self
    }

    pub fn with_provider_model_catalog_cache_file_override(mut self, path: PathBuf) -> Self {
        self.provider_model_catalog_cache_file_override = Some(path);
        self
    }

    pub fn with_runtime_settings_file_override(mut self, path: PathBuf) -> Self {
        self.runtime_settings_file_override = Some(path);
        self
    }

    pub fn with_dictation_settings_file_override(mut self, path: PathBuf) -> Self {
        self.dictation_settings_file_override = Some(path);
        self
    }

    pub fn with_mcp_registry_file_override(mut self, path: PathBuf) -> Self {
        self.mcp_registry_file_override = Some(path);
        self
    }

    pub fn with_skill_source_settings_file_override(mut self, path: PathBuf) -> Self {
        self.skill_source_settings_file_override = Some(path);
        self
    }

    pub fn with_openrouter_credential_file_override(mut self, path: PathBuf) -> Self {
        self.openrouter_credential_file_override = Some(path);
        self
    }

    pub fn with_autonomous_skill_cache_dir_override(mut self, path: PathBuf) -> Self {
        self.autonomous_skill_cache_dir_override = Some(path);
        self
    }

    pub fn with_runtime_supervisor_binary_override(mut self, path: PathBuf) -> Self {
        self.runtime_supervisor_binary_override = Some(path);
        self
    }

    pub fn with_openai_auth_config_override(mut self, config: OpenAiCodexAuthConfig) -> Self {
        self.openai_auth_config_override = Some(config);
        self
    }

    pub fn with_openai_compatible_auth_config_override(
        mut self,
        config: OpenAiCompatibleAuthConfig,
    ) -> Self {
        self.openai_compatible_auth_config_override = Some(config);
        self
    }

    pub fn with_openrouter_auth_config_override(mut self, config: OpenRouterAuthConfig) -> Self {
        self.openrouter_auth_config_override = Some(config);
        self
    }

    pub fn with_anthropic_auth_config_override(mut self, config: AnthropicAuthConfig) -> Self {
        self.anthropic_auth_config_override = Some(config);
        self
    }

    pub fn with_autonomous_web_config_override(mut self, config: AutonomousWebConfig) -> Self {
        self.autonomous_web_config_override = Some(config);
        self
    }

    pub fn with_owned_agent_provider_config_override(
        mut self,
        config: AgentProviderConfig,
    ) -> Self {
        self.owned_agent_provider_config_override = Some(config);
        self
    }

    pub fn with_failpoints(mut self, failpoints: ImportFailpoints) -> Self {
        self.import_failpoints = failpoints;
        self
    }

    pub fn with_runtime_stream_failpoints(mut self, failpoints: RuntimeStreamFailpoints) -> Self {
        self.runtime_stream_failpoints = failpoints;
        self
    }

    pub fn import_failpoints(&self) -> &ImportFailpoints {
        &self.import_failpoints
    }

    pub fn runtime_stream_failpoints(&self) -> &RuntimeStreamFailpoints {
        &self.runtime_stream_failpoints
    }

    pub fn runtime_stream_controller(&self) -> &RuntimeStreamController {
        &self.runtime_stream_controller
    }

    pub fn runtime_supervisor_controller(&self) -> &RuntimeSupervisorController {
        &self.runtime_supervisor_controller
    }

    pub fn agent_run_supervisor(&self) -> &AgentRunSupervisor {
        &self.agent_run_supervisor
    }

    pub fn provider_model_catalog_refresh_registry(&self) -> &ProviderModelCatalogRefreshRegistry {
        &self.provider_model_catalog_refresh_registry
    }

    pub fn runtime_supervisor_binary_override(&self) -> Option<&PathBuf> {
        self.runtime_supervisor_binary_override.as_ref()
    }

    pub fn openai_auth_config(&self) -> OpenAiCodexAuthConfig {
        self.openai_auth_config_override
            .clone()
            .unwrap_or_else(OpenAiCodexAuthConfig::for_platform)
    }

    pub fn openai_compatible_auth_config(&self) -> OpenAiCompatibleAuthConfig {
        self.openai_compatible_auth_config_override
            .clone()
            .unwrap_or_else(OpenAiCompatibleAuthConfig::for_platform)
    }

    pub fn openrouter_auth_config(&self) -> OpenRouterAuthConfig {
        self.openrouter_auth_config_override
            .clone()
            .unwrap_or_else(OpenRouterAuthConfig::for_platform)
    }

    pub fn anthropic_auth_config(&self) -> AnthropicAuthConfig {
        self.anthropic_auth_config_override
            .clone()
            .unwrap_or_else(AnthropicAuthConfig::for_platform)
    }

    pub fn autonomous_web_config(&self) -> AutonomousWebConfig {
        self.autonomous_web_config_override
            .clone()
            .unwrap_or_else(AutonomousWebConfig::for_platform)
    }

    pub fn owned_agent_provider_config_override(&self) -> Option<AgentProviderConfig> {
        self.owned_agent_provider_config_override.clone()
    }

    pub fn active_auth_flows(&self) -> &ActiveAuthFlowRegistry {
        &self.active_auth_flows
    }

    pub fn app_data_dir<R: Runtime>(&self, app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
        app.path().app_data_dir().map_err(|error| {
            CommandError::system_fault(
                "app_data_dir_unavailable",
                format!("Cadence could not resolve the app-data directory: {error}"),
            )
        })
    }

    pub fn global_db_path<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.global_db_path_override {
            return Ok(path.clone());
        }

        Ok(global_database_path(&self.app_data_dir(app)?))
    }

    pub fn autonomous_skill_cache_dir<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.autonomous_skill_cache_dir_override {
            return Ok(path.clone());
        }

        Ok(self
            .app_data_dir(app)?
            .join(AUTONOMOUS_SKILL_CACHE_DIRECTORY_NAME))
    }

    pub fn registry_file<R: Runtime>(&self, app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.registry_file_override {
            return Ok(path.clone());
        }

        // Phase 2.5: project + repository state lives in the global database.
        self.global_db_path(app)
    }

    pub fn notification_credential_store_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.notification_credential_store_file_override {
            return Ok(path.clone());
        }

        // Phase 2.3: notification credentials live in the global database. Callers that
        // construct a FileNotificationCredentialStore from this path now operate on cadence.db.
        self.global_db_path(app)
    }

    pub fn runtime_settings_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.runtime_settings_file_override {
            return Ok(path.clone());
        }

        Ok(self.app_data_dir(app)?.join(RUNTIME_SETTINGS_FILE_NAME))
    }

    pub fn dictation_settings_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.dictation_settings_file_override {
            return Ok(path.clone());
        }

        // Phase 2.4: dictation settings live in the global database.
        self.global_db_path(app)
    }

    pub fn mcp_registry_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.mcp_registry_file_override {
            return Ok(path.clone());
        }

        // Phase 2.4: MCP servers live in the global database.
        self.global_db_path(app)
    }

    pub fn skill_source_settings_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.skill_source_settings_file_override {
            return Ok(path.clone());
        }

        // Phase 2.4: skill source settings live in the global database.
        self.global_db_path(app)
    }

    pub fn provider_profiles_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.provider_profiles_file_override {
            return Ok(path.clone());
        }

        Ok(self.app_data_dir(app)?.join(PROVIDER_PROFILES_FILE_NAME))
    }

    pub fn provider_profile_credential_store_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.provider_profile_credential_store_file_override {
            return Ok(path.clone());
        }

        Ok(self
            .app_data_dir(app)?
            .join(PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME))
    }

    pub fn provider_model_catalog_cache_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.provider_model_catalog_cache_file_override {
            return Ok(path.clone());
        }

        // Phase 2.4: provider-model catalog cache lives in the global database.
        self.global_db_path(app)
    }

    pub fn openrouter_credential_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.openrouter_credential_file_override {
            return Ok(path.clone());
        }

        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CommandError::system_fault(
                "app_data_dir_unavailable",
                format!("Cadence could not resolve the app-data directory: {error}"),
            )
        })?;

        Ok(app_data_dir.join(OPENROUTER_CREDENTIAL_FILE_NAME))
    }

    pub fn auth_store_file_for_provider<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        _provider: ResolvedRuntimeProvider,
    ) -> Result<PathBuf, AuthFlowError> {
        if let Some(path) = &self.auth_store_file_override {
            return Ok(path.clone());
        }

        self.global_db_path(app).map_err(|error| {
            AuthFlowError::terminal(
                error.code,
                crate::commands::RuntimeAuthPhase::Failed,
                error.message,
            )
        })
    }

    pub fn auth_store_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, AuthFlowError> {
        self.auth_store_file_for_provider(app, openai_codex_provider())
    }
}
