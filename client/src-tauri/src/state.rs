use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    auth::{
        ActiveAuthFlowRegistry, AnthropicAuthConfig, OpenAiCodexAuthConfig,
        OpenAiCompatibleAuthConfig, OpenRouterAuthConfig,
    },
    commands::CommandError,
    global_db::global_database_path,
    provider_models::ProviderModelCatalogRefreshRegistry,
    runtime::{AgentProviderConfig, AgentRunSupervisor, AutonomousWebConfig},
};

pub const AUTONOMOUS_SKILL_CACHE_DIRECTORY_NAME: &str = "autonomous-skills";

#[derive(Debug, Clone, Default)]
pub struct ImportFailpoints {
    pub fail_registry_write: bool,
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
    autonomous_skill_cache_dir_override: Option<PathBuf>,
    openai_auth_config_override: Option<OpenAiCodexAuthConfig>,
    openai_compatible_auth_config_override: Option<OpenAiCompatibleAuthConfig>,
    openrouter_auth_config_override: Option<OpenRouterAuthConfig>,
    anthropic_auth_config_override: Option<AnthropicAuthConfig>,
    autonomous_web_config_override: Option<AutonomousWebConfig>,
    owned_agent_provider_config_override: Option<AgentProviderConfig>,
    import_failpoints: ImportFailpoints,
    runtime_stream_failpoints: RuntimeStreamFailpoints,
    agent_run_supervisor: AgentRunSupervisor,
    provider_model_catalog_refresh_registry: ProviderModelCatalogRefreshRegistry,
    active_auth_flows: ActiveAuthFlowRegistry,
}

impl DesktopState {
    pub fn with_global_db_path_override(mut self, path: PathBuf) -> Self {
        self.global_db_path_override = Some(path);
        self
    }

    pub fn with_autonomous_skill_cache_dir_override(mut self, path: PathBuf) -> Self {
        self.autonomous_skill_cache_dir_override = Some(path);
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

    pub fn agent_run_supervisor(&self) -> &AgentRunSupervisor {
        &self.agent_run_supervisor
    }

    pub fn provider_model_catalog_refresh_registry(&self) -> &ProviderModelCatalogRefreshRegistry {
        &self.provider_model_catalog_refresh_registry
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
                format!("Xero could not resolve the app-data directory: {error}"),
            )
        })
    }

    pub fn global_db_path<R: Runtime>(&self, app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
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
}
