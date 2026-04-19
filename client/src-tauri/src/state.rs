use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    auth::{ActiveAuthFlowRegistry, AuthFlowError, OpenAiCodexAuthConfig},
    commands::CommandError,
    notifications::NOTIFICATION_CREDENTIAL_STORE_FILE_NAME,
    runtime::{
        openai_codex_provider, ResolvedRuntimeProvider, RuntimeStreamController,
        RuntimeSupervisorController,
    },
};

pub const REGISTRY_FILE_NAME: &str = "project-registry.json";

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
    registry_file_override: Option<PathBuf>,
    auth_store_file_override: Option<PathBuf>,
    notification_credential_store_file_override: Option<PathBuf>,
    runtime_supervisor_binary_override: Option<PathBuf>,
    openai_auth_config_override: Option<OpenAiCodexAuthConfig>,
    import_failpoints: ImportFailpoints,
    runtime_stream_failpoints: RuntimeStreamFailpoints,
    runtime_stream_controller: RuntimeStreamController,
    runtime_supervisor_controller: RuntimeSupervisorController,
    active_auth_flows: ActiveAuthFlowRegistry,
}

impl DesktopState {
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

    pub fn with_runtime_supervisor_binary_override(mut self, path: PathBuf) -> Self {
        self.runtime_supervisor_binary_override = Some(path);
        self
    }

    pub fn with_openai_auth_config_override(mut self, config: OpenAiCodexAuthConfig) -> Self {
        self.openai_auth_config_override = Some(config);
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

    pub fn runtime_supervisor_binary_override(&self) -> Option<&PathBuf> {
        self.runtime_supervisor_binary_override.as_ref()
    }

    pub fn openai_auth_config(&self) -> OpenAiCodexAuthConfig {
        self.openai_auth_config_override
            .clone()
            .unwrap_or_else(OpenAiCodexAuthConfig::for_platform)
    }

    pub fn active_auth_flows(&self) -> &ActiveAuthFlowRegistry {
        &self.active_auth_flows
    }

    pub fn registry_file<R: Runtime>(&self, app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.registry_file_override {
            return Ok(path.clone());
        }

        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CommandError::system_fault(
                "app_data_dir_unavailable",
                format!("Cadence could not resolve the app-data directory: {error}"),
            )
        })?;

        Ok(app_data_dir.join(REGISTRY_FILE_NAME))
    }

    pub fn notification_credential_store_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, CommandError> {
        if let Some(path) = &self.notification_credential_store_file_override {
            return Ok(path.clone());
        }

        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            CommandError::system_fault(
                "app_data_dir_unavailable",
                format!("Cadence could not resolve the app-data directory: {error}"),
            )
        })?;

        Ok(app_data_dir.join(NOTIFICATION_CREDENTIAL_STORE_FILE_NAME))
    }

    pub fn auth_store_file_for_provider<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        provider: ResolvedRuntimeProvider,
    ) -> Result<PathBuf, AuthFlowError> {
        if let Some(path) = &self.auth_store_file_override {
            return Ok(path.clone());
        }

        let app_data_dir = app.path().app_data_dir().map_err(|error| {
            AuthFlowError::terminal(
                "app_data_dir_unavailable",
                crate::commands::RuntimeAuthPhase::Failed,
                format!("Cadence could not resolve the app-data directory: {error}"),
            )
        })?;

        Ok(app_data_dir.join(provider.auth_store_file_name))
    }

    pub fn auth_store_file<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<PathBuf, AuthFlowError> {
        self.auth_store_file_for_provider(app, openai_codex_provider())
    }
}
