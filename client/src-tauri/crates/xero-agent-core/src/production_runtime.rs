use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{CoreError, CoreResult};

pub const PRODUCTION_RUNTIME_CONTRACT_VERSION: u32 = 1;
pub const PRODUCTION_RUNTIME_SERVICE_ID: &str = "xero_owned_agent_production_runtime";
pub const FAKE_PROVIDER_ID: &str = "fake_provider";
pub const HEADLESS_JSON_STATE_FILE: &str = "agent-core-runs.json";
pub const PROJECT_STATE_DATABASE_FILE: &str = "state.db";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeExecutionMode {
    ProductionRealProvider,
    HarnessFakeProvider,
    ExternalAgent,
}

impl RuntimeExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProductionRealProvider => "production_real_provider",
            Self::HarnessFakeProvider => "harness_fake_provider",
            Self::ExternalAgent => "external_agent",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStoreKind {
    AppDataProjectState,
    FileBackedHeadlessJson,
    InMemoryHarness,
}

impl RuntimeStoreKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppDataProjectState => "app_data_project_state",
            Self::FileBackedHeadlessJson => "file_backed_headless_json",
            Self::InMemoryHarness => "in_memory_harness",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStoreDescriptor {
    pub kind: RuntimeStoreKind,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_file_name: Option<String>,
}

impl RuntimeStoreDescriptor {
    pub fn app_data_project_state(
        project_id: impl Into<String>,
        database_path: impl Into<PathBuf>,
    ) -> Self {
        let database_path = database_path.into();
        let root_path = database_path.parent().map(path_to_string);
        Self {
            kind: RuntimeStoreKind::AppDataProjectState,
            project_id: project_id.into(),
            root_path,
            database_path: Some(path_to_string(&database_path)),
            state_file_name: database_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
        }
    }

    pub fn file_backed_headless_json(
        project_id: impl Into<String>,
        state_file_path: impl Into<PathBuf>,
    ) -> Self {
        let state_file_path = state_file_path.into();
        Self {
            kind: RuntimeStoreKind::FileBackedHeadlessJson,
            project_id: project_id.into(),
            root_path: state_file_path.parent().map(path_to_string),
            database_path: None,
            state_file_name: state_file_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
        }
    }

    pub fn in_memory_harness(project_id: impl Into<String>) -> Self {
        Self {
            kind: RuntimeStoreKind::InMemoryHarness,
            project_id: project_id.into(),
            root_path: None,
            database_path: None,
            state_file_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionRuntimeContract {
    pub contract_version: u32,
    pub service_id: String,
    pub surface: String,
    pub execution_mode: RuntimeExecutionMode,
    pub project_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub store: RuntimeStoreDescriptor,
    #[serde(default)]
    pub service_boundaries: Vec<String>,
}

impl ProductionRuntimeContract {
    pub fn real_provider(
        surface: impl Into<String>,
        project_id: impl Into<String>,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        store: RuntimeStoreDescriptor,
    ) -> Self {
        Self {
            contract_version: PRODUCTION_RUNTIME_CONTRACT_VERSION,
            service_id: PRODUCTION_RUNTIME_SERVICE_ID.into(),
            surface: surface.into(),
            execution_mode: RuntimeExecutionMode::ProductionRealProvider,
            project_id: project_id.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            store,
            service_boundaries: production_runtime_boundaries(),
        }
    }

    pub fn fake_provider_harness(
        surface: impl Into<String>,
        project_id: impl Into<String>,
        model_id: impl Into<String>,
        store: RuntimeStoreDescriptor,
    ) -> Self {
        Self {
            contract_version: PRODUCTION_RUNTIME_CONTRACT_VERSION,
            service_id: PRODUCTION_RUNTIME_SERVICE_ID.into(),
            surface: surface.into(),
            execution_mode: RuntimeExecutionMode::HarnessFakeProvider,
            project_id: project_id.into(),
            provider_id: FAKE_PROVIDER_ID.into(),
            model_id: model_id.into(),
            store,
            service_boundaries: harness_runtime_boundaries(),
        }
    }
}

pub fn validate_production_runtime_contract(
    contract: &ProductionRuntimeContract,
) -> CoreResult<()> {
    validate_required(&contract.surface, "surface")?;
    validate_required(&contract.project_id, "projectId")?;
    validate_required(&contract.provider_id, "providerId")?;
    validate_required(&contract.model_id, "modelId")?;
    validate_required(&contract.store.project_id, "store.projectId")?;

    if contract.contract_version != PRODUCTION_RUNTIME_CONTRACT_VERSION {
        return Err(CoreError::invalid_request(
            "agent_core_production_contract_version_unsupported",
            format!(
                "Production runtime contract version `{}` is not supported; expected `{}`.",
                contract.contract_version, PRODUCTION_RUNTIME_CONTRACT_VERSION
            ),
        ));
    }

    if contract.store.project_id != contract.project_id {
        return Err(CoreError::invalid_request(
            "agent_core_production_project_store_mismatch",
            format!(
                "Production runtime project `{}` does not match store project `{}`.",
                contract.project_id, contract.store.project_id
            ),
        ));
    }

    match contract.execution_mode {
        RuntimeExecutionMode::ProductionRealProvider => validate_real_provider_contract(contract),
        RuntimeExecutionMode::HarnessFakeProvider => validate_fake_provider_contract(contract),
        RuntimeExecutionMode::ExternalAgent => Ok(()),
    }
}

pub fn production_runtime_trace_metadata(contract: &ProductionRuntimeContract) -> JsonValue {
    json!({
        "contractVersion": contract.contract_version,
        "serviceId": &contract.service_id,
        "surface": &contract.surface,
        "executionMode": contract.execution_mode,
        "projectId": &contract.project_id,
        "providerId": &contract.provider_id,
        "modelId": &contract.model_id,
        "store": &contract.store,
        "serviceBoundaries": &contract.service_boundaries,
    })
}

pub fn production_runtime_boundaries() -> Vec<String> {
    [
        "provider_profile_model_resolution",
        "app_data_project_store_access",
        "context_package_assembly",
        "preflight_admission",
        "provider_loop_execution",
        "tool_registry_v2_dispatch",
        "approval_policy_decisions",
        "sandbox_runner_integration",
        "trace_support_bundle_export",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub fn harness_runtime_boundaries() -> Vec<String> {
    [
        "explicit_fake_provider_selection",
        "file_backed_harness_store",
        "deterministic_fake_turn",
        "canonical_trace_export",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn validate_real_provider_contract(contract: &ProductionRuntimeContract) -> CoreResult<()> {
    if contract.provider_id == FAKE_PROVIDER_ID {
        return Err(CoreError::invalid_request(
            "agent_core_fake_provider_not_production",
            "The fake provider is harness-only and cannot satisfy the production runtime contract.",
        ));
    }
    if contract.provider_id.starts_with("external_") {
        return Err(CoreError::invalid_request(
            "agent_core_external_provider_not_owned_runtime",
            "External agent adapters must use the explicit external-agent host path, not the owned-provider production runtime.",
        ));
    }
    if contract.store.kind != RuntimeStoreKind::AppDataProjectState {
        return Err(CoreError::invalid_request(
            "agent_core_production_store_required",
            format!(
                "Real owned-provider execution requires app-data project state; `{}` is harness-only.",
                contract.store.kind.as_str()
            ),
        ));
    }
    let Some(database_path) = contract.store.database_path.as_deref() else {
        return Err(CoreError::invalid_request(
            "agent_core_production_store_database_missing",
            "Real owned-provider execution requires an app-data project database path.",
        ));
    };
    if !database_path.ends_with(PROJECT_STATE_DATABASE_FILE) {
        return Err(CoreError::invalid_request(
            "agent_core_production_store_database_invalid",
            format!(
                "Real owned-provider execution must use `{PROJECT_STATE_DATABASE_FILE}` app-data project databases, got `{database_path}`."
            ),
        ));
    }
    if contract
        .store
        .state_file_name
        .as_deref()
        .is_some_and(|name| name == HEADLESS_JSON_STATE_FILE)
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_store_rejected",
            "Real owned-provider execution cannot use `agent-core-runs.json`; that store is harness-only.",
        ));
    }
    Ok(())
}

fn validate_fake_provider_contract(contract: &ProductionRuntimeContract) -> CoreResult<()> {
    if contract.provider_id != FAKE_PROVIDER_ID {
        return Err(CoreError::invalid_request(
            "agent_core_fake_provider_explicit_required",
            format!(
                "Harness execution requires provider `{FAKE_PROVIDER_ID}`, got `{}`.",
                contract.provider_id
            ),
        ));
    }
    if !matches!(
        contract.store.kind,
        RuntimeStoreKind::FileBackedHeadlessJson | RuntimeStoreKind::InMemoryHarness
    ) {
        return Err(CoreError::invalid_request(
            "agent_core_harness_store_invalid",
            "Fake-provider harness execution must use an explicit harness store.",
        ));
    }
    Ok(())
}

fn validate_required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::invalid_request(
            "agent_core_production_contract_missing_field",
            format!("Production runtime contract field `{field}` is required."),
        ))
    } else {
        Ok(())
    }
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentRuntimeFacade, FileAgentCoreStore, HeadlessProviderExecutionConfig,
        HeadlessProviderRuntime, HeadlessRuntimeOptions, OpenAiCompatibleHeadlessConfig,
        ProviderSelection, StartRunRequest,
    };

    #[test]
    fn real_provider_contract_rejects_headless_json_store() {
        let contract = ProductionRuntimeContract::real_provider(
            "cli_agent_exec",
            "project-1",
            "openai_api",
            "test-model",
            RuntimeStoreDescriptor::file_backed_headless_json(
                "project-1",
                "/tmp/xero/headless/agent-core-runs.json",
            ),
        );

        let error = validate_production_runtime_contract(&contract)
            .expect_err("headless store must be rejected");
        assert_eq!(error.code, "agent_core_production_store_required");
    }

    #[test]
    fn real_provider_contract_accepts_app_data_project_store() {
        let contract = ProductionRuntimeContract::real_provider(
            "desktop_start",
            "project-1",
            "openai_api",
            "test-model",
            RuntimeStoreDescriptor::app_data_project_state(
                "project-1",
                "/tmp/xero/projects/project-1/state.db",
            ),
        );

        validate_production_runtime_contract(&contract).expect("app-data store is valid");
    }

    #[test]
    fn fake_provider_harness_contract_requires_fake_provider() {
        let mut contract = ProductionRuntimeContract::fake_provider_harness(
            "cli_agent_exec",
            "project-1",
            "fake-model",
            RuntimeStoreDescriptor::file_backed_headless_json(
                "project-1",
                "/tmp/xero/headless/agent-core-runs.json",
            ),
        );
        contract.provider_id = "openai_api".into();

        let error = validate_production_runtime_contract(&contract)
            .expect_err("fake harness must be explicit");
        assert_eq!(error.code, "agent_core_fake_provider_explicit_required");
    }

    #[test]
    fn headless_real_provider_runtime_rejects_file_store_before_persisting() {
        let path = unique_test_state_path("headless-real-provider-rejects-file-store");
        let store = FileAgentCoreStore::open(path.clone()).expect("open file store");
        let runtime = HeadlessProviderRuntime::new(
            store,
            HeadlessProviderExecutionConfig::OpenAiCompatible(OpenAiCompatibleHeadlessConfig {
                provider_id: "openai_api".into(),
                model_id: "test-model".into(),
                base_url: "http://127.0.0.1:9/v1".into(),
                api_key: None,
                timeout_ms: 1,
                workspace_root: None,
                allow_workspace_writes: false,
            }),
            HeadlessRuntimeOptions::default(),
        );

        let error = runtime
            .start_run(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                prompt: "real provider must not use the harness store".into(),
                provider: ProviderSelection {
                    provider_id: "openai_api".into(),
                    model_id: "test-model".into(),
                },
                controls: None,
            })
            .expect_err("real headless provider must reject file store");

        assert_eq!(error.code, "agent_core_production_store_required");
        assert!(
            !path.exists(),
            "rejection should happen before the JSON state file is written"
        );
    }

    fn unique_test_state_path(label: &str) -> PathBuf {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        std::env::temp_dir()
            .join(format!("xero-{label}-{millis}-{}", std::process::id()))
            .join(HEADLESS_JSON_STATE_FILE)
    }
}
