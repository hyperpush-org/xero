use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    commands::{CommandError, CommandResult},
    provider_credentials::{
        ProviderCredentialProfile, ProviderCredentialReadinessProjection,
        ProviderCredentialReadinessStatus, ProviderCredentialsView,
    },
    provider_models::{ProviderModelCatalog, ProviderModelCatalogSource},
};

use super::provider::{
    resolve_runtime_provider_identity, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID,
    BEDROCK_PROVIDER_ID, GEMINI_AI_STUDIO_PROVIDER_ID, GEMINI_RUNTIME_KIND,
    GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID,
    OPENAI_CODEX_PROVIDER_ID, OPENAI_COMPATIBLE_RUNTIME_KIND, OPENROUTER_PROVIDER_ID,
    VERTEX_PROVIDER_ID,
};

pub const XERO_DIAGNOSTIC_CONTRACT_VERSION: u32 = 1;
pub const XERO_DOCTOR_REPORT_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum XeroDiagnosticSubject {
    Dictation,
    ProviderCredential,
    ModelCatalog,
    RuntimeBinding,
    RuntimeSupervisor,
    McpRegistry,
    SettingsDependency,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum XeroDiagnosticStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum XeroDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum XeroDiagnosticRedactionClass {
    Public,
    EndpointCredential,
    LocalPath,
    Secret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroDiagnosticEndpointMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_list_strategy: Option<String>,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroDiagnosticCheck {
    pub contract_version: u32,
    pub check_id: String,
    pub subject: XeroDiagnosticSubject,
    pub status: XeroDiagnosticStatus,
    pub severity: XeroDiagnosticSeverity,
    pub retryable: bool,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<XeroDiagnosticEndpointMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    pub redaction_class: XeroDiagnosticRedactionClass,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroDiagnosticCheckInput {
    pub subject: XeroDiagnosticSubject,
    pub status: XeroDiagnosticStatus,
    pub severity: XeroDiagnosticSeverity,
    pub retryable: bool,
    pub code: String,
    pub message: String,
    pub affected_profile_id: Option<String>,
    pub affected_provider_id: Option<String>,
    pub endpoint: Option<XeroDiagnosticEndpointMetadata>,
    pub remediation: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroDoctorReportMode {
    QuickLocal,
    ExtendedNetwork,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroDoctorReportOutputMode {
    CompactHuman,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroDoctorVersionInfo {
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_supervisor_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_protocol_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroDoctorReportSummary {
    pub passed: u32,
    pub warnings: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub highest_severity: XeroDiagnosticSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroDoctorReport {
    pub contract_version: u32,
    pub report_id: String,
    pub generated_at: String,
    pub mode: XeroDoctorReportMode,
    pub versions: XeroDoctorVersionInfo,
    pub summary: XeroDoctorReportSummary,
    #[serde(default)]
    pub dictation_checks: Vec<XeroDiagnosticCheck>,
    #[serde(default)]
    pub profile_checks: Vec<XeroDiagnosticCheck>,
    #[serde(default)]
    pub model_catalog_checks: Vec<XeroDiagnosticCheck>,
    #[serde(default)]
    pub runtime_supervisor_checks: Vec<XeroDiagnosticCheck>,
    #[serde(default)]
    pub mcp_dependency_checks: Vec<XeroDiagnosticCheck>,
    #[serde(default)]
    pub settings_dependency_checks: Vec<XeroDiagnosticCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroDoctorReportInput {
    pub report_id: String,
    pub generated_at: String,
    pub mode: XeroDoctorReportMode,
    pub versions: XeroDoctorVersionInfo,
    pub dictation_checks: Vec<XeroDiagnosticCheck>,
    pub profile_checks: Vec<XeroDiagnosticCheck>,
    pub model_catalog_checks: Vec<XeroDiagnosticCheck>,
    pub runtime_supervisor_checks: Vec<XeroDiagnosticCheck>,
    pub mcp_dependency_checks: Vec<XeroDiagnosticCheck>,
    pub settings_dependency_checks: Vec<XeroDiagnosticCheck>,
}

impl XeroDiagnosticCheck {
    pub fn new(input: XeroDiagnosticCheckInput) -> CommandResult<Self> {
        let (message, message_redacted, message_class) = sanitize_diagnostic_text(&input.message);
        let (remediation, remediation_redacted, remediation_class) =
            sanitize_optional_diagnostic_text(input.remediation.as_deref());
        let (endpoint, endpoint_redacted, endpoint_class) =
            sanitize_endpoint_metadata(input.endpoint);
        let redaction_class = strongest_redaction_class(
            strongest_redaction_class(message_class, remediation_class),
            endpoint_class,
        );

        let check = Self {
            contract_version: XERO_DIAGNOSTIC_CONTRACT_VERSION,
            check_id: diagnostic_check_id(
                input.subject,
                input.affected_provider_id.as_deref(),
                input.affected_profile_id.as_deref(),
                &input.code,
            ),
            subject: input.subject,
            status: input.status,
            severity: input.severity,
            retryable: input.retryable,
            code: input.code.trim().to_owned(),
            message,
            affected_profile_id: normalize_optional_text(input.affected_profile_id),
            affected_provider_id: normalize_optional_text(input.affected_provider_id),
            endpoint,
            remediation,
            redaction_class,
            redacted: message_redacted || remediation_redacted || endpoint_redacted,
        };

        validate_diagnostic_check(check)
    }

    pub fn passed(
        subject: XeroDiagnosticSubject,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> CommandResult<Self> {
        Self::new(XeroDiagnosticCheckInput {
            subject,
            status: XeroDiagnosticStatus::Passed,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: code.into(),
            message: message.into(),
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation: None,
        })
    }

    pub fn skipped(
        subject: XeroDiagnosticSubject,
        code: impl Into<String>,
        message: impl Into<String>,
        remediation: Option<String>,
    ) -> CommandResult<Self> {
        Self::new(XeroDiagnosticCheckInput {
            subject,
            status: XeroDiagnosticStatus::Skipped,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: code.into(),
            message: message.into(),
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation,
        })
    }
}

impl XeroDoctorReport {
    pub fn new(input: XeroDoctorReportInput) -> CommandResult<Self> {
        let mut report = Self {
            contract_version: XERO_DOCTOR_REPORT_CONTRACT_VERSION,
            report_id: input.report_id.trim().to_owned(),
            generated_at: input.generated_at.trim().to_owned(),
            mode: input.mode,
            versions: XeroDoctorVersionInfo {
                app_version: sanitize_diagnostic_text(&input.versions.app_version).0,
                runtime_supervisor_version: sanitize_optional_diagnostic_text(
                    input.versions.runtime_supervisor_version.as_deref(),
                )
                .0,
                runtime_protocol_version: sanitize_optional_diagnostic_text(
                    input.versions.runtime_protocol_version.as_deref(),
                )
                .0,
            },
            summary: XeroDoctorReportSummary {
                passed: 0,
                warnings: 0,
                failed: 0,
                skipped: 0,
                total: 0,
                highest_severity: XeroDiagnosticSeverity::Info,
            },
            dictation_checks: sort_and_validate_checks(input.dictation_checks)?,
            profile_checks: sort_and_validate_checks(input.profile_checks)?,
            model_catalog_checks: sort_and_validate_checks(input.model_catalog_checks)?,
            runtime_supervisor_checks: sort_and_validate_checks(input.runtime_supervisor_checks)?,
            mcp_dependency_checks: sort_and_validate_checks(input.mcp_dependency_checks)?,
            settings_dependency_checks: sort_and_validate_checks(input.settings_dependency_checks)?,
        };
        report.summary = summarize_diagnostic_checks(report.all_checks());
        validate_doctor_report(report)
    }

    pub fn all_checks(&self) -> Vec<&XeroDiagnosticCheck> {
        self.dictation_checks
            .iter()
            .chain(self.profile_checks.iter())
            .chain(self.model_catalog_checks.iter())
            .chain(self.runtime_supervisor_checks.iter())
            .chain(self.mcp_dependency_checks.iter())
            .chain(self.settings_dependency_checks.iter())
            .collect()
    }
}

pub fn provider_readiness_diagnostic(
    profile: &ProviderCredentialProfile,
    readiness: &ProviderCredentialReadinessProjection,
) -> CommandResult<XeroDiagnosticCheck> {
    let endpoint = endpoint_metadata_from_profile(profile);
    match readiness.status {
        ProviderCredentialReadinessStatus::Ready => XeroDiagnosticCheck::new(
            XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::ProviderCredential,
                status: XeroDiagnosticStatus::Passed,
                severity: XeroDiagnosticSeverity::Info,
                retryable: false,
                code: "provider_ready".into(),
                message: format!(
                    "Provider `{}` is ready.",
                    profile.provider_id
                ),
                affected_profile_id: Some(profile.profile_id.clone()),
                affected_provider_id: Some(profile.provider_id.clone()),
                endpoint,
                remediation: None,
            },
        ),
        ProviderCredentialReadinessStatus::Missing => XeroDiagnosticCheck::new(
            XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::ProviderCredential,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: false,
                code: "provider_credentials_missing".into(),
                message: format!(
                    "Provider `{}` is missing credentials.",
                    profile.provider_id
                ),
                affected_profile_id: Some(profile.profile_id.clone()),
                affected_provider_id: Some(profile.provider_id.clone()),
                endpoint,
                remediation: Some(provider_remediation(profile)),
            },
        ),
        ProviderCredentialReadinessStatus::Malformed => XeroDiagnosticCheck::new(
            XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::ProviderCredential,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: false,
                code: "provider_credentials_malformed".into(),
                message: format!(
                    "Provider `{}` has a credential link that no longer matches its stored credential record.",
                    profile.provider_id
                ),
                affected_profile_id: Some(profile.profile_id.clone()),
                affected_provider_id: Some(profile.provider_id.clone()),
                endpoint,
                remediation: Some(
                    "Reconnect or resave this provider so Xero can rebuild the app-local credential link.".into(),
                ),
            },
        ),
    }
}

pub fn unsupported_provider_diagnostic(
    profile_id: Option<&str>,
    provider_id: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ProviderCredential,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: "provider_id_unsupported".into(),
        message: format!("Xero does not support provider id `{provider_id}`."),
        affected_profile_id: profile_id.map(str::to_owned),
        affected_provider_id: Some(provider_id.into()),
        endpoint: None,
        remediation: Some(
            "Choose a supported provider preset or configure this endpoint through an OpenAI-compatible recipe.".into(),
        ),
    })
}

pub fn invalid_base_url_diagnostic(
    profile: &ProviderCredentialProfile,
    base_url: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ProviderCredential,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: "provider_base_url_invalid".into(),
        message: format!(
            "Provider `{}` has an invalid base URL.",
            profile.provider_id
        ),
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: Some(XeroDiagnosticEndpointMetadata {
            base_url: Some(base_url.into()),
            host: None,
            api_version: profile.api_version.clone(),
            region: profile.region.clone(),
            project_id: profile.project_id.clone(),
            model_list_strategy: None,
            redacted: false,
        }),
        remediation: Some(
            "Enter a valid http or https endpoint, usually ending in `/v1` for OpenAI-compatible providers.".into(),
        ),
    })
}

pub fn stale_runtime_binding_diagnostic(
    profile_id: Option<&str>,
    provider_id: &str,
    runtime_kind: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::RuntimeBinding,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: "runtime_binding_stale".into(),
        message: format!(
            "The runtime binding for provider `{provider_id}` and runtime `{runtime_kind}` is stale."
        ),
        affected_profile_id: profile_id.map(str::to_owned),
        affected_provider_id: Some(provider_id.into()),
        endpoint: None,
        remediation: Some(
            "Restart the runtime session after resaving or reselecting the provider.".into(),
        ),
    })
}

pub fn ambient_auth_failure_diagnostic(
    profile: &ProviderCredentialProfile,
    message: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ProviderCredential,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: true,
        code: "provider_ambient_auth_failed".into(),
        message: message.into(),
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: endpoint_metadata_from_profile(profile),
        remediation: Some(
            "Refresh the ambient provider login in your shell or cloud SDK, then run diagnostics again.".into(),
        ),
    })
}

pub fn provider_model_catalog_diagnostic(
    catalog: &ProviderModelCatalog,
) -> CommandResult<XeroDiagnosticCheck> {
    if let Some(error) = &catalog.last_refresh_error {
        let has_stale_snapshot = matches!(
            catalog.source,
            ProviderModelCatalogSource::Cache | ProviderModelCatalogSource::Manual
        ) && !catalog.models.is_empty();
        return XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ModelCatalog,
            status: if has_stale_snapshot {
                XeroDiagnosticStatus::Warning
            } else {
                XeroDiagnosticStatus::Failed
            },
            severity: if has_stale_snapshot {
                XeroDiagnosticSeverity::Warning
            } else {
                XeroDiagnosticSeverity::Error
            },
            retryable: error.retryable,
            code: error.code.clone(),
            message: error.message.clone(),
            affected_profile_id: Some(catalog.profile_id.clone()),
            affected_provider_id: Some(catalog.provider_id.clone()),
            endpoint: None,
            remediation: Some(provider_catalog_remediation(
                &catalog.provider_id,
                error.retryable,
            )),
        });
    }

    match catalog.source {
        ProviderModelCatalogSource::Live | ProviderModelCatalogSource::Cache => {
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::ModelCatalog,
                status: XeroDiagnosticStatus::Passed,
                severity: XeroDiagnosticSeverity::Info,
                retryable: false,
                code: "provider_model_catalog_ready".into(),
                message: format!(
                    "Model catalog for provider `{}` is available from {:?}.",
                    catalog.provider_id, catalog.source
                ),
                affected_profile_id: Some(catalog.profile_id.clone()),
                affected_provider_id: Some(catalog.provider_id.clone()),
                endpoint: None,
                remediation: None,
            })
        }
        ProviderModelCatalogSource::Manual => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ModelCatalog,
            status: XeroDiagnosticStatus::Skipped,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: "provider_model_catalog_manual".into(),
            message: format!(
                "Provider `{}` uses manual model catalog configuration.",
                catalog.provider_id
            ),
            affected_profile_id: Some(catalog.profile_id.clone()),
            affected_provider_id: Some(catalog.provider_id.clone()),
            endpoint: None,
            remediation: Some(
                "Confirm the configured model id is still accepted by the provider endpoint."
                    .into(),
            ),
        }),
        ProviderModelCatalogSource::Unavailable => {
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::ModelCatalog,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: true,
                code: "provider_model_catalog_unavailable".into(),
                message: format!(
                    "Model catalog for provider `{}` is unavailable.",
                    catalog.provider_id
                ),
                affected_profile_id: Some(catalog.profile_id.clone()),
                affected_provider_id: Some(catalog.provider_id.clone()),
                endpoint: None,
                remediation: Some(provider_catalog_remediation(&catalog.provider_id, true)),
            })
        }
    }
}

pub fn provider_capability_diagnostics(
    catalog: &ProviderModelCatalog,
    selected_model_id: Option<&str>,
) -> CommandResult<Vec<XeroDiagnosticCheck>> {
    let selected_model_id = selected_model_id
        .map(str::trim)
        .filter(|model_id| !model_id.is_empty())
        .unwrap_or(catalog.configured_model_id.as_str());
    let selected_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id);
    let capabilities = crate::provider_models::provider_capability_catalog_for_catalog(
        catalog,
        Some(selected_model_id),
    );
    let mut checks = Vec::new();

    let model_status = if selected_model.is_some() {
        if matches!(catalog.source, ProviderModelCatalogSource::Manual) {
            (
                XeroDiagnosticStatus::Warning,
                XeroDiagnosticSeverity::Warning,
                "provider_model_manual_unverified",
            )
        } else {
            (
                XeroDiagnosticStatus::Passed,
                XeroDiagnosticSeverity::Info,
                "provider_model_available",
            )
        }
    } else if matches!(catalog.source, ProviderModelCatalogSource::Manual) {
        (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            "provider_model_manual_unverified",
        )
    } else {
        (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            "provider_model_unavailable",
        )
    };
    checks.push(XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ModelCatalog,
        status: model_status.0,
        severity: model_status.1,
        retryable: matches!(model_status.0, XeroDiagnosticStatus::Failed),
        code: model_status.2.into(),
        message: if selected_model.is_some() {
            format!(
                "Selected model `{selected_model_id}` is present in the {:?} catalog for provider `{}`.",
                catalog.source, catalog.provider_id
            )
        } else {
            format!(
                "Selected model `{selected_model_id}` was not verified in the {:?} catalog for provider `{}`.",
                catalog.source, catalog.provider_id
            )
        },
        affected_profile_id: Some(catalog.profile_id.clone()),
        affected_provider_id: Some(catalog.provider_id.clone()),
        endpoint: None,
        remediation: match model_status.0 {
            XeroDiagnosticStatus::Passed => None,
            XeroDiagnosticStatus::Warning => Some(
                "Refresh the provider catalog or choose a model returned by live provider discovery."
                    .into(),
            ),
            _ => Some(
                "Choose another model, refresh the catalog, or repair provider credentials and endpoint metadata."
                    .into(),
            ),
        },
    })?);

    checks.push(capability_check(
        catalog,
        "provider_streaming_capability",
        "streaming",
        &capabilities.capabilities.streaming.status,
        &capabilities.capabilities.streaming.detail,
        "Choose a provider or model path with streaming support if live progress is required.",
    )?);
    checks.push(capability_check(
        catalog,
        "provider_tool_call_capability",
        "tool calls",
        &capabilities.capabilities.tool_calls.status,
        &format!(
            "{}; schema={}; parallel={}",
            capabilities.capabilities.tool_calls.strictness_behavior,
            capabilities.capabilities.tool_calls.schema_dialect,
            capabilities.capabilities.tool_calls.parallel_call_behavior
        ),
        "Choose a provider/model with tool-call support before starting an agent task.",
    )?);
    checks.push(capability_check(
        catalog,
        "provider_reasoning_capability",
        "reasoning controls",
        &capabilities.capabilities.reasoning.status,
        &format!(
            "{} effort option(s); fallback={}",
            capabilities.capabilities.reasoning.effort_levels.len(),
            capabilities.capabilities.reasoning.unsupported_model_fallback
        ),
        "Use a model with reasoning controls, or let Xero send the request without reasoning effort.",
    )?);
    checks.push(capability_check(
        catalog,
        "provider_attachment_capability",
        "attachments",
        &capabilities.capabilities.attachments.status,
        &format!(
            "image={}; document={}",
            capabilities.capabilities.attachments.image_input,
            capabilities.capabilities.attachments.document_input
        ),
        "Choose an attachment-capable provider before adding images or documents.",
    )?);
    checks.push(capability_check(
        catalog,
        "provider_context_limit_capability",
        "context limits",
        &capabilities.capabilities.context_limits.status,
        &format!(
            "source={}; confidence={}",
            capabilities.capabilities.context_limits.source,
            capabilities.capabilities.context_limits.confidence
        ),
        "Choose a model with known context limits or keep the prompt below conservative defaults.",
    )?);

    Ok(checks)
}

fn capability_check(
    catalog: &ProviderModelCatalog,
    code: &str,
    label: &str,
    status: &str,
    detail: &str,
    remediation: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    let (diagnostic_status, severity, retryable) = match status {
        "supported" | "probed" => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
        ),
        "not_applicable" => (
            XeroDiagnosticStatus::Skipped,
            XeroDiagnosticSeverity::Info,
            false,
        ),
        "unknown" => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            false,
        ),
        _ => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            false,
        ),
    };

    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ModelCatalog,
        status: diagnostic_status,
        severity,
        retryable,
        code: code.into(),
        message: format!(
            "Provider `{}` reports {label} as `{status}`. {detail}",
            catalog.provider_id
        ),
        affected_profile_id: Some(catalog.profile_id.clone()),
        affected_provider_id: Some(catalog.provider_id.clone()),
        endpoint: None,
        remediation: if diagnostic_status == XeroDiagnosticStatus::Passed {
            None
        } else {
            Some(remediation.into())
        },
    })
}

pub fn provider_validation_diagnostics(
    snapshot: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
) -> CommandResult<Vec<XeroDiagnosticCheck>> {
    let mut checks = Vec::new();
    let _ = snapshot;
    checks.push(provider_runtime_alignment_diagnostic(profile)?);
    checks.extend(provider_metadata_diagnostics(profile)?);
    checks.push(provider_readiness_diagnostic(
        profile,
        &profile.readiness(),
    )?);
    Ok(checks)
}

fn provider_runtime_alignment_diagnostic(
    profile: &ProviderCredentialProfile,
) -> CommandResult<XeroDiagnosticCheck> {
    match resolve_runtime_provider_identity(
        Some(profile.provider_id.as_str()),
        Some(profile.runtime_kind.as_str()),
    ) {
        Ok(provider) => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Passed,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: "provider_runtime_aligned".into(),
            message: format!(
                "Provider `{}` maps to runtime kind `{}`.",
                provider.provider_id, provider.runtime_kind
            ),
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: endpoint_metadata_from_profile(profile),
            remediation: None,
        }),
        Err(diagnostic) => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Failed,
            severity: XeroDiagnosticSeverity::Error,
            retryable: diagnostic.retryable,
            code: diagnostic.code,
            message: diagnostic.message,
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: endpoint_metadata_from_profile(profile),
            remediation: Some(
                "Resave this provider from Providers settings so provider and runtime metadata match."
                    .into(),
            ),
        }),
    }
}

fn provider_metadata_diagnostics(
    profile: &ProviderCredentialProfile,
) -> CommandResult<Vec<XeroDiagnosticCheck>> {
    let mut checks = Vec::new();

    match profile.provider_id.as_str() {
        OPENAI_CODEX_PROVIDER_ID => {
            require_absent(
                profile,
                "presetId",
                profile.preset_id.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "baseUrl", profile.base_url.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
        }
        OPENROUTER_PROVIDER_ID
        | ANTHROPIC_PROVIDER_ID
        | GITHUB_MODELS_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID => {
            require_preset_id(profile, profile.provider_id.as_str(), &mut checks)?;
            require_absent(profile, "baseUrl", profile.base_url.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
            if profile.provider_id == GEMINI_AI_STUDIO_PROVIDER_ID
                && profile.runtime_kind != GEMINI_RUNTIME_KIND
            {
                checks.push(metadata_failed(
                    profile,
                    "provider_runtime_kind_invalid",
                    "Gemini AI Studio providers must use runtime kind `gemini`.",
                    "Resave the provider so Xero can rebuild Gemini runtime metadata.",
                )?);
            }
        }
        OPENAI_API_PROVIDER_ID => {
            if let Some(base_url) = profile.base_url.as_deref() {
                require_http_base_url(profile, base_url, &mut checks)?;
                if profile
                    .preset_id
                    .as_deref()
                    .is_some_and(|preset_id| preset_id != OPENAI_API_PROVIDER_ID)
                {
                    checks.push(metadata_failed(
                        profile,
                        "provider_preset_invalid",
                        "Custom OpenAI-compatible providers may only keep presetId `openai_api`.",
                        "Choose the OpenAI-compatible preset or clear unsupported preset metadata.",
                    )?);
                }
            } else {
                require_preset_id(profile, OPENAI_API_PROVIDER_ID, &mut checks)?;
                require_absent(
                    profile,
                    "apiVersion",
                    profile.api_version.as_deref(),
                    &mut checks,
                )?;
            }
            require_absent(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
        }
        OLLAMA_PROVIDER_ID => {
            require_preset_id(profile, OLLAMA_PROVIDER_ID, &mut checks)?;
            if let Some(base_url) = profile.base_url.as_deref() {
                require_http_base_url(profile, base_url, &mut checks)?;
            }
            require_absent(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
            if profile.runtime_kind != OPENAI_COMPATIBLE_RUNTIME_KIND {
                checks.push(metadata_failed(
                    profile,
                    "provider_runtime_kind_invalid",
                    "Ollama providers must use runtime kind `openai_compatible`.",
                    "Resave the provider so Xero can rebuild local runtime metadata.",
                )?);
            }
        }
        AZURE_OPENAI_PROVIDER_ID => {
            require_preset_id(profile, AZURE_OPENAI_PROVIDER_ID, &mut checks)?;
            require_present(profile, "baseUrl", profile.base_url.as_deref(), &mut checks)?;
            if let Some(base_url) = profile.base_url.as_deref() {
                require_http_base_url(profile, base_url, &mut checks)?;
            }
            require_present(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
        }
        BEDROCK_PROVIDER_ID => {
            require_preset_id(profile, BEDROCK_PROVIDER_ID, &mut checks)?;
            require_present(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_absent(profile, "baseUrl", profile.base_url.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
            require_absent(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
        }
        VERTEX_PROVIDER_ID => {
            require_preset_id(profile, VERTEX_PROVIDER_ID, &mut checks)?;
            require_present(profile, "region", profile.region.as_deref(), &mut checks)?;
            require_present(
                profile,
                "projectId",
                profile.project_id.as_deref(),
                &mut checks,
            )?;
            require_absent(profile, "baseUrl", profile.base_url.as_deref(), &mut checks)?;
            require_absent(
                profile,
                "apiVersion",
                profile.api_version.as_deref(),
                &mut checks,
            )?;
        }
        other => {
            checks.push(unsupported_provider_diagnostic(
                Some(profile.profile_id.as_str()),
                other,
            )?);
        }
    }

    if checks.is_empty() {
        checks.push(XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Passed,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: "provider_metadata_ready".into(),
            message: format!(
                "Provider `{}` has complete provider metadata.",
                profile.provider_id
            ),
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: endpoint_metadata_from_profile(profile),
            remediation: None,
        })?);
    }

    Ok(checks)
}

fn require_preset_id(
    profile: &ProviderCredentialProfile,
    expected: &str,
    checks: &mut Vec<XeroDiagnosticCheck>,
) -> CommandResult<()> {
    if profile.preset_id.as_deref() == Some(expected) {
        return Ok(());
    }

    checks.push(metadata_failed(
        profile,
        "provider_preset_invalid",
        &format!(
            "Provider `{}` must use presetId `{expected}` for `{}`.",
            profile.provider_id, profile.provider_id
        ),
        "Resave this provider from Providers settings so Xero can restore the preset metadata.",
    )?);
    Ok(())
}

fn require_present(
    profile: &ProviderCredentialProfile,
    field: &'static str,
    value: Option<&str>,
    checks: &mut Vec<XeroDiagnosticCheck>,
) -> CommandResult<()> {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        return Ok(());
    }

    checks.push(metadata_failed(
        profile,
        "provider_metadata_missing",
        &format!(
            "Provider `{}` is missing required `{field}` metadata.",
            profile.provider_id
        ),
        "Open this provider in Providers settings, fill the required connection field, and save it again.",
    )?);
    Ok(())
}

fn require_absent(
    profile: &ProviderCredentialProfile,
    field: &'static str,
    value: Option<&str>,
    checks: &mut Vec<XeroDiagnosticCheck>,
) -> CommandResult<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        return Ok(());
    }

    checks.push(metadata_failed(
        profile,
        "provider_metadata_unexpected",
        &format!(
            "Provider `{}` has unsupported `{field}` metadata for `{}`.",
            profile.provider_id, profile.provider_id
        ),
        "Resave this provider from Providers settings so Xero can drop unsupported connection metadata.",
    )?);
    Ok(())
}

fn require_http_base_url(
    profile: &ProviderCredentialProfile,
    base_url: &str,
    checks: &mut Vec<XeroDiagnosticCheck>,
) -> CommandResult<()> {
    let parsed = Url::parse(base_url);
    let valid = parsed
        .as_ref()
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if valid {
        return Ok(());
    }

    checks.push(invalid_base_url_diagnostic(profile, base_url)?);
    Ok(())
}

fn metadata_failed(
    profile: &ProviderCredentialProfile,
    code: &str,
    message: &str,
    remediation: &str,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ProviderCredential,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: false,
        code: code.into(),
        message: message.into(),
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: endpoint_metadata_from_profile(profile),
        remediation: Some(remediation.into()),
    })
}

pub fn validate_diagnostic_check(check: XeroDiagnosticCheck) -> CommandResult<XeroDiagnosticCheck> {
    if check.contract_version != XERO_DIAGNOSTIC_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "diagnostic_contract_version_invalid",
            "Xero diagnostic checks must use the current diagnostic contract version.",
        ));
    }
    ensure_non_empty(&check.check_id, "checkId")?;
    ensure_non_empty(&check.code, "code")?;
    ensure_non_empty(&check.message, "message")?;
    if let Some(profile_id) = &check.affected_profile_id {
        ensure_non_empty(profile_id, "affectedProfileId")?;
    }
    if let Some(provider_id) = &check.affected_provider_id {
        ensure_non_empty(provider_id, "affectedProviderId")?;
    }

    match check.status {
        XeroDiagnosticStatus::Passed => {
            if check.severity != XeroDiagnosticSeverity::Info || check.retryable {
                return Err(invalid_state_combo(
                    "Passed diagnostic checks must use severity `info` and retryable=false.",
                ));
            }
            if check.remediation.is_some() {
                return Err(invalid_state_combo(
                    "Passed diagnostic checks must not include remediation text.",
                ));
            }
        }
        XeroDiagnosticStatus::Skipped => {
            if check.severity != XeroDiagnosticSeverity::Info || check.retryable {
                return Err(invalid_state_combo(
                    "Skipped diagnostic checks must use severity `info` and retryable=false.",
                ));
            }
        }
        XeroDiagnosticStatus::Warning => {
            if check.severity != XeroDiagnosticSeverity::Warning {
                return Err(invalid_state_combo(
                    "Warning diagnostic checks must use severity `warning`.",
                ));
            }
            ensure_non_empty_option(check.remediation.as_deref(), "remediation")?;
        }
        XeroDiagnosticStatus::Failed => {
            if check.severity != XeroDiagnosticSeverity::Error {
                return Err(invalid_state_combo(
                    "Failed diagnostic checks must use severity `error`.",
                ));
            }
            ensure_non_empty_option(check.remediation.as_deref(), "remediation")?;
        }
    }

    if check.redaction_class == XeroDiagnosticRedactionClass::Public && check.redacted {
        return Err(invalid_state_combo(
            "Public diagnostic checks must not be marked redacted.",
        ));
    }
    if check.redaction_class != XeroDiagnosticRedactionClass::Public && !check.redacted {
        return Err(invalid_state_combo(
            "Non-public diagnostic redaction classes must set redacted=true.",
        ));
    }

    Ok(check)
}

pub fn validate_doctor_report(report: XeroDoctorReport) -> CommandResult<XeroDoctorReport> {
    if report.contract_version != XERO_DOCTOR_REPORT_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "doctor_report_contract_version_invalid",
            "Xero doctor reports must use the current doctor report contract version.",
        ));
    }
    ensure_non_empty(&report.report_id, "reportId")?;
    ensure_non_empty(&report.generated_at, "generatedAt")?;
    ensure_non_empty(&report.versions.app_version, "versions.appVersion")?;

    let checks = report.all_checks();
    for check in &checks {
        validate_diagnostic_check((*check).clone())?;
    }

    let expected = summarize_diagnostic_checks(checks);
    if report.summary != expected {
        return Err(CommandError::user_fixable(
            "doctor_report_summary_invalid",
            "Xero doctor report summary counts must match the included checks.",
        ));
    }

    Ok(report)
}

pub fn render_doctor_report(
    report: &XeroDoctorReport,
    mode: XeroDoctorReportOutputMode,
) -> CommandResult<String> {
    validate_doctor_report(report.clone())?;
    match mode {
        XeroDoctorReportOutputMode::Json => serde_json::to_string_pretty(report).map_err(|error| {
            CommandError::system_fault(
                "doctor_report_json_serialize_failed",
                format!("Xero could not serialize the doctor report: {error}"),
            )
        }),
        XeroDoctorReportOutputMode::CompactHuman => Ok(render_compact_human_report(report)),
    }
}

pub fn summarize_diagnostic_checks(checks: Vec<&XeroDiagnosticCheck>) -> XeroDoctorReportSummary {
    let mut summary = XeroDoctorReportSummary {
        passed: 0,
        warnings: 0,
        failed: 0,
        skipped: 0,
        total: checks.len() as u32,
        highest_severity: XeroDiagnosticSeverity::Info,
    };

    for check in checks {
        match check.status {
            XeroDiagnosticStatus::Passed => summary.passed += 1,
            XeroDiagnosticStatus::Warning => summary.warnings += 1,
            XeroDiagnosticStatus::Failed => summary.failed += 1,
            XeroDiagnosticStatus::Skipped => summary.skipped += 1,
        }
        summary.highest_severity = highest_severity(summary.highest_severity, check.severity);
    }

    summary
}

pub fn sanitize_diagnostic_text(value: &str) -> (String, bool, XeroDiagnosticRedactionClass) {
    let mut redacted = false;
    let mut redaction_class = XeroDiagnosticRedactionClass::Public;
    let mut redact_next = false;
    let words = value
        .split_whitespace()
        .map(|word| {
            let bare = trim_word_punctuation(word);
            let lower = bare.to_ascii_lowercase();
            if redact_next {
                if is_authorization_scheme(&lower) {
                    return word.to_owned();
                }

                redact_next = false;
                redacted = true;
                redaction_class = strongest_redaction_class(
                    redaction_class,
                    XeroDiagnosticRedactionClass::Secret,
                );
                return word.replace(bare, "[redacted]");
            }

            if lower == "authorization" || lower == "bearer" || is_sensitive_value_label(&lower) {
                redact_next = true;
                return word.to_owned();
            }

            if let Some(assignment) = redact_sensitive_assignment(bare) {
                redacted = true;
                redaction_class =
                    strongest_redaction_class(redaction_class, assignment.redaction_class);
                if assignment.redact_next {
                    redact_next = true;
                }
                return word.replace(bare, &assignment.value);
            }

            if looks_like_secret_token(bare) {
                redacted = true;
                redaction_class = strongest_redaction_class(
                    redaction_class,
                    XeroDiagnosticRedactionClass::Secret,
                );
                "[redacted]".into()
            } else if looks_like_raw_local_path(bare) {
                redacted = true;
                redaction_class = strongest_redaction_class(
                    redaction_class,
                    XeroDiagnosticRedactionClass::LocalPath,
                );
                word.replace(bare, "[redacted-path]")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>();

    (words.join(" "), redacted, redaction_class)
}

fn sort_and_validate_checks(
    checks: Vec<XeroDiagnosticCheck>,
) -> CommandResult<Vec<XeroDiagnosticCheck>> {
    let mut checks = checks
        .into_iter()
        .map(sanitize_diagnostic_check)
        .collect::<CommandResult<Vec<_>>>()?;
    checks.sort_by(|left, right| {
        (
            left.subject,
            left.affected_provider_id.as_deref().unwrap_or_default(),
            left.affected_profile_id.as_deref().unwrap_or_default(),
            left.code.as_str(),
            left.check_id.as_str(),
        )
            .cmp(&(
                right.subject,
                right.affected_provider_id.as_deref().unwrap_or_default(),
                right.affected_profile_id.as_deref().unwrap_or_default(),
                right.code.as_str(),
                right.check_id.as_str(),
            ))
    });
    Ok(checks)
}

fn sanitize_diagnostic_check(check: XeroDiagnosticCheck) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: check.subject,
        status: check.status,
        severity: check.severity,
        retryable: check.retryable,
        code: check.code,
        message: check.message,
        affected_profile_id: check.affected_profile_id,
        affected_provider_id: check.affected_provider_id,
        endpoint: check.endpoint,
        remediation: check.remediation,
    })
}

fn render_compact_human_report(report: &XeroDoctorReport) -> String {
    let mut lines = vec![
        format!("Xero doctor report {}", report.report_id),
        format!("Generated: {}", report.generated_at),
        format!("Mode: {:?}", report.mode),
        format!(
            "Summary: {} passed, {} warning(s), {} failed, {} skipped",
            report.summary.passed,
            report.summary.warnings,
            report.summary.failed,
            report.summary.skipped
        ),
    ];
    push_human_group(&mut lines, "Dictation", &report.dictation_checks);
    push_human_group(&mut lines, "Providers", &report.profile_checks);
    push_human_group(&mut lines, "Model catalogs", &report.model_catalog_checks);
    push_human_group(
        &mut lines,
        "Runtime supervisor",
        &report.runtime_supervisor_checks,
    );
    push_human_group(
        &mut lines,
        "MCP dependencies",
        &report.mcp_dependency_checks,
    );
    push_human_group(
        &mut lines,
        "Settings dependencies",
        &report.settings_dependency_checks,
    );
    lines.join("\n")
}

fn push_human_group(lines: &mut Vec<String>, label: &str, checks: &[XeroDiagnosticCheck]) {
    if checks.is_empty() {
        return;
    }
    lines.push(format!("{label}:"));
    for check in checks {
        let remediation = check
            .remediation
            .as_deref()
            .map(|value| format!(" Remediation: {value}"))
            .unwrap_or_default();
        lines.push(format!(
            "- [{:?}] {}: {}{}",
            check.status, check.code, check.message, remediation
        ));
    }
}

fn sanitize_optional_diagnostic_text(
    value: Option<&str>,
) -> (Option<String>, bool, XeroDiagnosticRedactionClass) {
    let Some(value) = value else {
        return (None, false, XeroDiagnosticRedactionClass::Public);
    };
    let (sanitized, redacted, class) = sanitize_diagnostic_text(value);
    (Some(sanitized), redacted, class)
}

fn sanitize_endpoint_metadata(
    endpoint: Option<XeroDiagnosticEndpointMetadata>,
) -> (
    Option<XeroDiagnosticEndpointMetadata>,
    bool,
    XeroDiagnosticRedactionClass,
) {
    let Some(endpoint) = endpoint else {
        return (None, false, XeroDiagnosticRedactionClass::Public);
    };

    let (base_url, base_url_redacted, base_url_class, host) = endpoint
        .base_url
        .as_deref()
        .map(sanitize_endpoint_url)
        .unwrap_or((None, false, XeroDiagnosticRedactionClass::Public, None));
    let (api_version, api_redacted, api_class) =
        sanitize_optional_diagnostic_text(endpoint.api_version.as_deref());
    let (region, region_redacted, region_class) =
        sanitize_optional_diagnostic_text(endpoint.region.as_deref());
    let (project_id, project_redacted, project_class) =
        sanitize_optional_diagnostic_text(endpoint.project_id.as_deref());
    let (model_list_strategy, model_strategy_redacted, model_strategy_class) =
        sanitize_optional_diagnostic_text(endpoint.model_list_strategy.as_deref());
    let redaction_class = [
        base_url_class,
        api_class,
        region_class,
        project_class,
        model_strategy_class,
    ]
    .into_iter()
    .fold(
        XeroDiagnosticRedactionClass::Public,
        strongest_redaction_class,
    );
    let redacted = endpoint.redacted
        || base_url_redacted
        || api_redacted
        || region_redacted
        || project_redacted
        || model_strategy_redacted;

    (
        Some(XeroDiagnosticEndpointMetadata {
            base_url,
            host: host.or_else(|| normalize_optional_text(endpoint.host)),
            api_version,
            region,
            project_id,
            model_list_strategy,
            redacted,
        }),
        redacted,
        redaction_class,
    )
}

fn sanitize_endpoint_url(
    value: &str,
) -> (
    Option<String>,
    bool,
    XeroDiagnosticRedactionClass,
    Option<String>,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return (None, false, XeroDiagnosticRedactionClass::Public, None);
    }

    let Ok(mut parsed) = Url::parse(trimmed) else {
        let (sanitized, redacted, class) = sanitize_diagnostic_text(trimmed);
        return (Some(sanitized), redacted, class, None);
    };

    let mut redacted = false;
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("redacted");
        redacted = true;
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(None);
        redacted = true;
    }

    if let Some(segments) = parsed.path_segments() {
        if segments.clone().any(looks_like_secret_path_segment) {
            parsed.set_path("/[redacted-path]");
            redacted = true;
        }
    }

    if parsed.query().is_some() {
        let pairs = parsed
            .query_pairs()
            .map(|(key, value)| {
                if is_sensitive_name(&key) && !value.is_empty() {
                    redacted = true;
                    (key.into_owned(), "[redacted]".to_owned())
                } else {
                    (key.into_owned(), value.into_owned())
                }
            })
            .collect::<Vec<_>>();
        parsed.set_query(None);
        if !pairs.is_empty() {
            let query = pairs
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("&");
            parsed.set_query(Some(&query));
        }
    }

    let host = parsed.host_str().map(str::to_owned);
    (
        Some(parsed.to_string()),
        redacted,
        if redacted {
            XeroDiagnosticRedactionClass::EndpointCredential
        } else {
            XeroDiagnosticRedactionClass::Public
        },
        host,
    )
}

fn endpoint_metadata_from_profile(
    profile: &ProviderCredentialProfile,
) -> Option<XeroDiagnosticEndpointMetadata> {
    if profile.base_url.is_none()
        && profile.api_version.is_none()
        && profile.region.is_none()
        && profile.project_id.is_none()
    {
        return None;
    }

    Some(XeroDiagnosticEndpointMetadata {
        base_url: profile.base_url.clone(),
        host: None,
        api_version: profile.api_version.clone(),
        region: profile.region.clone(),
        project_id: profile.project_id.clone(),
        model_list_strategy: None,
        redacted: false,
    })
}

fn provider_remediation(profile: &ProviderCredentialProfile) -> String {
    match profile.provider_id.as_str() {
        "ollama" => {
            "Start Ollama or update the local endpoint, then check the connection again.".into()
        }
        "bedrock" | "vertex" => {
            "Configure ambient cloud credentials and confirm the region/project metadata.".into()
        }
        "openai_codex" => "Sign in to OpenAI Codex from Providers settings.".into(),
        _ => "Add credentials or choose another ready provider in Providers settings.".into(),
    }
}

fn provider_catalog_remediation(provider_id: &str, retryable: bool) -> String {
    if retryable {
        format!("Check network access and credentials for `{provider_id}`, then refresh the model catalog.")
    } else {
        format!(
            "Review the saved provider for `{provider_id}` before refreshing the model catalog."
        )
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn ensure_non_empty(value: &str, field: &'static str) -> CommandResult<()> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn ensure_non_empty_option(value: Option<&str>, field: &'static str) -> CommandResult<()> {
    let Some(value) = value else {
        return Err(CommandError::invalid_request(field));
    };
    ensure_non_empty(value, field)
}

fn invalid_state_combo(message: impl Into<String>) -> CommandError {
    CommandError::user_fixable("diagnostic_state_invalid", message)
}

fn diagnostic_check_id(
    subject: XeroDiagnosticSubject,
    provider_id: Option<&str>,
    profile_id: Option<&str>,
    code: &str,
) -> String {
    format!(
        "diagnostic:v{}:{:?}:{}:{}:{}",
        XERO_DIAGNOSTIC_CONTRACT_VERSION,
        subject,
        provider_id.unwrap_or("global"),
        profile_id.unwrap_or("global"),
        code.trim()
    )
    .to_ascii_lowercase()
    .replace("::", ":")
}

fn highest_severity(
    current: XeroDiagnosticSeverity,
    next: XeroDiagnosticSeverity,
) -> XeroDiagnosticSeverity {
    if next > current {
        next
    } else {
        current
    }
}

fn strongest_redaction_class(
    left: XeroDiagnosticRedactionClass,
    right: XeroDiagnosticRedactionClass,
) -> XeroDiagnosticRedactionClass {
    if right > left {
        right
    } else {
        left
    }
}

fn trim_word_punctuation(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(
            character,
            ',' | ';' | ':' | '.' | ')' | '(' | '[' | ']' | '"' | '\''
        )
    })
}

struct DiagnosticAssignmentRedaction {
    value: String,
    redaction_class: XeroDiagnosticRedactionClass,
    redact_next: bool,
}

fn redact_sensitive_assignment(value: &str) -> Option<DiagnosticAssignmentRedaction> {
    for separator in ['=', ':'] {
        if let Some((key, secret)) = value.split_once(separator) {
            if is_sensitive_name(key) && !secret.trim().is_empty() {
                return Some(DiagnosticAssignmentRedaction {
                    value: format!("{}{}[redacted]", key, separator),
                    redaction_class: XeroDiagnosticRedactionClass::Secret,
                    redact_next: is_authorization_scheme(
                        &trim_word_punctuation(secret).to_ascii_lowercase(),
                    ),
                });
            }

            if looks_like_raw_local_path(secret.trim()) {
                return Some(DiagnosticAssignmentRedaction {
                    value: format!("{}{}[redacted-path]", key, separator),
                    redaction_class: XeroDiagnosticRedactionClass::LocalPath,
                    redact_next: false,
                });
            }
        }
    }
    None
}

fn is_sensitive_name(value: &str) -> bool {
    let normalized = value
        .trim()
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .replace('-', "_");
    matches!(
        normalized.as_str(),
        "access_token"
            | "api_key"
            | "apikey"
            | "anthropic_api_key"
            | "authorization"
            | "aws_access_key_id"
            | "aws_secret_access_key"
            | "aws_session_token"
            | "auth_token"
            | "bearer"
            | "client_secret"
            | "github_token"
            | "google_oauth_access_token"
            | "openai_api_key"
            | "password"
            | "private_key"
            | "refresh_token"
            | "secret"
            | "session_id"
            | "session_token"
            | "token"
            | "x_api_key"
    )
}

fn is_authorization_scheme(value: &str) -> bool {
    matches!(value, "bearer" | "basic" | "token")
}

fn is_sensitive_value_label(value: &str) -> bool {
    matches!(
        value,
        "access_token"
            | "api_key"
            | "apikey"
            | "anthropic_api_key"
            | "aws_access_key_id"
            | "aws_secret_access_key"
            | "aws_session_token"
            | "auth_token"
            | "client_secret"
            | "github_token"
            | "google_oauth_access_token"
            | "openai_api_key"
            | "password"
            | "private_key"
            | "refresh_token"
            | "session_id"
            | "session_token"
            | "x_api_key"
    )
}

fn looks_like_secret_token(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("sk-")
        || normalized.contains("github_pat_")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("ya29.")
        || normalized.contains("-----begin")
        || normalized.starts_with("akia")
}

fn looks_like_raw_local_path(value: &str) -> bool {
    let windows = value.replace('/', "\\").to_ascii_lowercase();
    value.starts_with("/Users/")
        || value.starts_with("/home/")
        || value.starts_with("/var/folders/")
        || value.starts_with("/tmp/")
        || value.starts_with("~/")
        || value.starts_with("\\Users\\")
        || value.contains(":\\Users\\")
        || windows.contains(":\\programdata\\")
        || windows.contains(":\\windows\\temp\\")
        || windows.starts_with("%appdata%\\")
        || windows.starts_with("%localappdata%\\")
}

fn looks_like_secret_path_segment(value: &str) -> bool {
    looks_like_secret_token(value)
        || (value.len() >= 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
}
