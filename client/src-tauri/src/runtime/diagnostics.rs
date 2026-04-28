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

pub const CADENCE_DIAGNOSTIC_CONTRACT_VERSION: u32 = 1;
pub const CADENCE_DOCTOR_REPORT_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceDiagnosticSubject {
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
pub enum CadenceDiagnosticStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CadenceDiagnosticRedactionClass {
    Public,
    EndpointCredential,
    LocalPath,
    Secret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CadenceDiagnosticEndpointMetadata {
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
pub struct CadenceDiagnosticCheck {
    pub contract_version: u32,
    pub check_id: String,
    pub subject: CadenceDiagnosticSubject,
    pub status: CadenceDiagnosticStatus,
    pub severity: CadenceDiagnosticSeverity,
    pub retryable: bool,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<CadenceDiagnosticEndpointMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    pub redaction_class: CadenceDiagnosticRedactionClass,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CadenceDiagnosticCheckInput {
    pub subject: CadenceDiagnosticSubject,
    pub status: CadenceDiagnosticStatus,
    pub severity: CadenceDiagnosticSeverity,
    pub retryable: bool,
    pub code: String,
    pub message: String,
    pub affected_profile_id: Option<String>,
    pub affected_provider_id: Option<String>,
    pub endpoint: Option<CadenceDiagnosticEndpointMetadata>,
    pub remediation: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CadenceDoctorReportMode {
    QuickLocal,
    ExtendedNetwork,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CadenceDoctorReportOutputMode {
    CompactHuman,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CadenceDoctorVersionInfo {
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_supervisor_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_protocol_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CadenceDoctorReportSummary {
    pub passed: u32,
    pub warnings: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub highest_severity: CadenceDiagnosticSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CadenceDoctorReport {
    pub contract_version: u32,
    pub report_id: String,
    pub generated_at: String,
    pub mode: CadenceDoctorReportMode,
    pub versions: CadenceDoctorVersionInfo,
    pub summary: CadenceDoctorReportSummary,
    #[serde(default)]
    pub dictation_checks: Vec<CadenceDiagnosticCheck>,
    #[serde(default)]
    pub profile_checks: Vec<CadenceDiagnosticCheck>,
    #[serde(default)]
    pub model_catalog_checks: Vec<CadenceDiagnosticCheck>,
    #[serde(default)]
    pub runtime_supervisor_checks: Vec<CadenceDiagnosticCheck>,
    #[serde(default)]
    pub mcp_dependency_checks: Vec<CadenceDiagnosticCheck>,
    #[serde(default)]
    pub settings_dependency_checks: Vec<CadenceDiagnosticCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CadenceDoctorReportInput {
    pub report_id: String,
    pub generated_at: String,
    pub mode: CadenceDoctorReportMode,
    pub versions: CadenceDoctorVersionInfo,
    pub dictation_checks: Vec<CadenceDiagnosticCheck>,
    pub profile_checks: Vec<CadenceDiagnosticCheck>,
    pub model_catalog_checks: Vec<CadenceDiagnosticCheck>,
    pub runtime_supervisor_checks: Vec<CadenceDiagnosticCheck>,
    pub mcp_dependency_checks: Vec<CadenceDiagnosticCheck>,
    pub settings_dependency_checks: Vec<CadenceDiagnosticCheck>,
}

impl CadenceDiagnosticCheck {
    pub fn new(input: CadenceDiagnosticCheckInput) -> CommandResult<Self> {
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
            contract_version: CADENCE_DIAGNOSTIC_CONTRACT_VERSION,
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
        subject: CadenceDiagnosticSubject,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> CommandResult<Self> {
        Self::new(CadenceDiagnosticCheckInput {
            subject,
            status: CadenceDiagnosticStatus::Passed,
            severity: CadenceDiagnosticSeverity::Info,
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
        subject: CadenceDiagnosticSubject,
        code: impl Into<String>,
        message: impl Into<String>,
        remediation: Option<String>,
    ) -> CommandResult<Self> {
        Self::new(CadenceDiagnosticCheckInput {
            subject,
            status: CadenceDiagnosticStatus::Skipped,
            severity: CadenceDiagnosticSeverity::Info,
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

impl CadenceDoctorReport {
    pub fn new(input: CadenceDoctorReportInput) -> CommandResult<Self> {
        let mut report = Self {
            contract_version: CADENCE_DOCTOR_REPORT_CONTRACT_VERSION,
            report_id: input.report_id.trim().to_owned(),
            generated_at: input.generated_at.trim().to_owned(),
            mode: input.mode,
            versions: CadenceDoctorVersionInfo {
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
            summary: CadenceDoctorReportSummary {
                passed: 0,
                warnings: 0,
                failed: 0,
                skipped: 0,
                total: 0,
                highest_severity: CadenceDiagnosticSeverity::Info,
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

    pub fn all_checks(&self) -> Vec<&CadenceDiagnosticCheck> {
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
) -> CommandResult<CadenceDiagnosticCheck> {
    let endpoint = endpoint_metadata_from_profile(profile);
    match readiness.status {
        ProviderCredentialReadinessStatus::Ready => CadenceDiagnosticCheck::new(
            CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ProviderCredential,
                status: CadenceDiagnosticStatus::Passed,
                severity: CadenceDiagnosticSeverity::Info,
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
        ProviderCredentialReadinessStatus::Missing => CadenceDiagnosticCheck::new(
            CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ProviderCredential,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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
        ProviderCredentialReadinessStatus::Malformed => CadenceDiagnosticCheck::new(
            CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ProviderCredential,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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
                    "Reconnect or resave this provider so Cadence can rebuild the app-local credential link.".into(),
                ),
            },
        ),
    }
}

pub fn unsupported_provider_diagnostic(
    profile_id: Option<&str>,
    provider_id: &str,
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::ProviderCredential,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
        retryable: false,
        code: "provider_id_unsupported".into(),
        message: format!("Cadence does not support provider id `{provider_id}`."),
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
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::ProviderCredential,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
        retryable: false,
        code: "provider_base_url_invalid".into(),
        message: format!(
            "Provider `{}` has an invalid base URL.",
            profile.provider_id
        ),
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: Some(CadenceDiagnosticEndpointMetadata {
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
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::RuntimeBinding,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
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
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::ProviderCredential,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
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
) -> CommandResult<CadenceDiagnosticCheck> {
    if let Some(error) = &catalog.last_refresh_error {
        let has_stale_snapshot = matches!(
            catalog.source,
            ProviderModelCatalogSource::Cache | ProviderModelCatalogSource::Manual
        ) && !catalog.models.is_empty();
        return CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::ModelCatalog,
            status: if has_stale_snapshot {
                CadenceDiagnosticStatus::Warning
            } else {
                CadenceDiagnosticStatus::Failed
            },
            severity: if has_stale_snapshot {
                CadenceDiagnosticSeverity::Warning
            } else {
                CadenceDiagnosticSeverity::Error
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ModelCatalog,
                status: CadenceDiagnosticStatus::Passed,
                severity: CadenceDiagnosticSeverity::Info,
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
        ProviderModelCatalogSource::Manual => {
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ModelCatalog,
                status: CadenceDiagnosticStatus::Skipped,
                severity: CadenceDiagnosticSeverity::Info,
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
            })
        }
        ProviderModelCatalogSource::Unavailable => {
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::ModelCatalog,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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

pub fn provider_validation_diagnostics(
    snapshot: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
) -> CommandResult<Vec<CadenceDiagnosticCheck>> {
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
) -> CommandResult<CadenceDiagnosticCheck> {
    match resolve_runtime_provider_identity(
        Some(profile.provider_id.as_str()),
        Some(profile.runtime_kind.as_str()),
    ) {
        Ok(provider) => CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::ProviderCredential,
            status: CadenceDiagnosticStatus::Passed,
            severity: CadenceDiagnosticSeverity::Info,
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
        Err(diagnostic) => CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::ProviderCredential,
            status: CadenceDiagnosticStatus::Failed,
            severity: CadenceDiagnosticSeverity::Error,
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
) -> CommandResult<Vec<CadenceDiagnosticCheck>> {
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
                    "Resave the provider so Cadence can rebuild Gemini runtime metadata.",
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
                    "Resave the provider so Cadence can rebuild local runtime metadata.",
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
        checks.push(CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::ProviderCredential,
            status: CadenceDiagnosticStatus::Passed,
            severity: CadenceDiagnosticSeverity::Info,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
        "Resave this provider from Providers settings so Cadence can restore the preset metadata.",
    )?);
    Ok(())
}

fn require_present(
    profile: &ProviderCredentialProfile,
    field: &'static str,
    value: Option<&str>,
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
        "Resave this provider from Providers settings so Cadence can drop unsupported connection metadata.",
    )?);
    Ok(())
}

fn require_http_base_url(
    profile: &ProviderCredentialProfile,
    base_url: &str,
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject: CadenceDiagnosticSubject::ProviderCredential,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
        retryable: false,
        code: code.into(),
        message: message.into(),
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: endpoint_metadata_from_profile(profile),
        remediation: Some(remediation.into()),
    })
}

pub fn validate_diagnostic_check(
    check: CadenceDiagnosticCheck,
) -> CommandResult<CadenceDiagnosticCheck> {
    if check.contract_version != CADENCE_DIAGNOSTIC_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "diagnostic_contract_version_invalid",
            "Cadence diagnostic checks must use the current diagnostic contract version.",
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
        CadenceDiagnosticStatus::Passed => {
            if check.severity != CadenceDiagnosticSeverity::Info || check.retryable {
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
        CadenceDiagnosticStatus::Skipped => {
            if check.severity != CadenceDiagnosticSeverity::Info || check.retryable {
                return Err(invalid_state_combo(
                    "Skipped diagnostic checks must use severity `info` and retryable=false.",
                ));
            }
        }
        CadenceDiagnosticStatus::Warning => {
            if check.severity != CadenceDiagnosticSeverity::Warning {
                return Err(invalid_state_combo(
                    "Warning diagnostic checks must use severity `warning`.",
                ));
            }
            ensure_non_empty_option(check.remediation.as_deref(), "remediation")?;
        }
        CadenceDiagnosticStatus::Failed => {
            if check.severity != CadenceDiagnosticSeverity::Error {
                return Err(invalid_state_combo(
                    "Failed diagnostic checks must use severity `error`.",
                ));
            }
            ensure_non_empty_option(check.remediation.as_deref(), "remediation")?;
        }
    }

    if check.redaction_class == CadenceDiagnosticRedactionClass::Public && check.redacted {
        return Err(invalid_state_combo(
            "Public diagnostic checks must not be marked redacted.",
        ));
    }
    if check.redaction_class != CadenceDiagnosticRedactionClass::Public && !check.redacted {
        return Err(invalid_state_combo(
            "Non-public diagnostic redaction classes must set redacted=true.",
        ));
    }

    Ok(check)
}

pub fn validate_doctor_report(report: CadenceDoctorReport) -> CommandResult<CadenceDoctorReport> {
    if report.contract_version != CADENCE_DOCTOR_REPORT_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "doctor_report_contract_version_invalid",
            "Cadence doctor reports must use the current doctor report contract version.",
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
            "Cadence doctor report summary counts must match the included checks.",
        ));
    }

    Ok(report)
}

pub fn render_doctor_report(
    report: &CadenceDoctorReport,
    mode: CadenceDoctorReportOutputMode,
) -> CommandResult<String> {
    validate_doctor_report(report.clone())?;
    match mode {
        CadenceDoctorReportOutputMode::Json => {
            serde_json::to_string_pretty(report).map_err(|error| {
                CommandError::system_fault(
                    "doctor_report_json_serialize_failed",
                    format!("Cadence could not serialize the doctor report: {error}"),
                )
            })
        }
        CadenceDoctorReportOutputMode::CompactHuman => Ok(render_compact_human_report(report)),
    }
}

pub fn summarize_diagnostic_checks(
    checks: Vec<&CadenceDiagnosticCheck>,
) -> CadenceDoctorReportSummary {
    let mut summary = CadenceDoctorReportSummary {
        passed: 0,
        warnings: 0,
        failed: 0,
        skipped: 0,
        total: checks.len() as u32,
        highest_severity: CadenceDiagnosticSeverity::Info,
    };

    for check in checks {
        match check.status {
            CadenceDiagnosticStatus::Passed => summary.passed += 1,
            CadenceDiagnosticStatus::Warning => summary.warnings += 1,
            CadenceDiagnosticStatus::Failed => summary.failed += 1,
            CadenceDiagnosticStatus::Skipped => summary.skipped += 1,
        }
        summary.highest_severity = highest_severity(summary.highest_severity, check.severity);
    }

    summary
}

pub fn sanitize_diagnostic_text(value: &str) -> (String, bool, CadenceDiagnosticRedactionClass) {
    let mut redacted = false;
    let mut redaction_class = CadenceDiagnosticRedactionClass::Public;
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
                    CadenceDiagnosticRedactionClass::Secret,
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
                    CadenceDiagnosticRedactionClass::Secret,
                );
                "[redacted]".into()
            } else if looks_like_raw_local_path(bare) {
                redacted = true;
                redaction_class = strongest_redaction_class(
                    redaction_class,
                    CadenceDiagnosticRedactionClass::LocalPath,
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
    checks: Vec<CadenceDiagnosticCheck>,
) -> CommandResult<Vec<CadenceDiagnosticCheck>> {
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

fn sanitize_diagnostic_check(
    check: CadenceDiagnosticCheck,
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
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

fn render_compact_human_report(report: &CadenceDoctorReport) -> String {
    let mut lines = vec![
        format!("Cadence doctor report {}", report.report_id),
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

fn push_human_group(lines: &mut Vec<String>, label: &str, checks: &[CadenceDiagnosticCheck]) {
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
) -> (Option<String>, bool, CadenceDiagnosticRedactionClass) {
    let Some(value) = value else {
        return (None, false, CadenceDiagnosticRedactionClass::Public);
    };
    let (sanitized, redacted, class) = sanitize_diagnostic_text(value);
    (Some(sanitized), redacted, class)
}

fn sanitize_endpoint_metadata(
    endpoint: Option<CadenceDiagnosticEndpointMetadata>,
) -> (
    Option<CadenceDiagnosticEndpointMetadata>,
    bool,
    CadenceDiagnosticRedactionClass,
) {
    let Some(endpoint) = endpoint else {
        return (None, false, CadenceDiagnosticRedactionClass::Public);
    };

    let (base_url, base_url_redacted, base_url_class, host) = endpoint
        .base_url
        .as_deref()
        .map(sanitize_endpoint_url)
        .unwrap_or((None, false, CadenceDiagnosticRedactionClass::Public, None));
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
        CadenceDiagnosticRedactionClass::Public,
        strongest_redaction_class,
    );
    let redacted = endpoint.redacted
        || base_url_redacted
        || api_redacted
        || region_redacted
        || project_redacted
        || model_strategy_redacted;

    (
        Some(CadenceDiagnosticEndpointMetadata {
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
    CadenceDiagnosticRedactionClass,
    Option<String>,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return (None, false, CadenceDiagnosticRedactionClass::Public, None);
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
            CadenceDiagnosticRedactionClass::EndpointCredential
        } else {
            CadenceDiagnosticRedactionClass::Public
        },
        host,
    )
}

fn endpoint_metadata_from_profile(
    profile: &ProviderCredentialProfile,
) -> Option<CadenceDiagnosticEndpointMetadata> {
    if profile.base_url.is_none()
        && profile.api_version.is_none()
        && profile.region.is_none()
        && profile.project_id.is_none()
    {
        return None;
    }

    Some(CadenceDiagnosticEndpointMetadata {
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
    subject: CadenceDiagnosticSubject,
    provider_id: Option<&str>,
    profile_id: Option<&str>,
    code: &str,
) -> String {
    format!(
        "diagnostic:v{}:{:?}:{}:{}:{}",
        CADENCE_DIAGNOSTIC_CONTRACT_VERSION,
        subject,
        provider_id.unwrap_or("global"),
        profile_id.unwrap_or("global"),
        code.trim()
    )
    .to_ascii_lowercase()
    .replace("::", ":")
}

fn highest_severity(
    current: CadenceDiagnosticSeverity,
    next: CadenceDiagnosticSeverity,
) -> CadenceDiagnosticSeverity {
    if next > current {
        next
    } else {
        current
    }
}

fn strongest_redaction_class(
    left: CadenceDiagnosticRedactionClass,
    right: CadenceDiagnosticRedactionClass,
) -> CadenceDiagnosticRedactionClass {
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
    redaction_class: CadenceDiagnosticRedactionClass,
    redact_next: bool,
}

fn redact_sensitive_assignment(value: &str) -> Option<DiagnosticAssignmentRedaction> {
    for separator in ['=', ':'] {
        if let Some((key, secret)) = value.split_once(separator) {
            if is_sensitive_name(key) && !secret.trim().is_empty() {
                return Some(DiagnosticAssignmentRedaction {
                    value: format!("{}{}[redacted]", key, separator),
                    redaction_class: CadenceDiagnosticRedactionClass::Secret,
                    redact_next: is_authorization_scheme(
                        &trim_word_punctuation(secret).to_ascii_lowercase(),
                    ),
                });
            }

            if looks_like_raw_local_path(secret.trim()) {
                return Some(DiagnosticAssignmentRedaction {
                    value: format!("{}{}[redacted-path]", key, separator),
                    redaction_class: CadenceDiagnosticRedactionClass::LocalPath,
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
    value.starts_with("/Users/")
        || value.starts_with("/home/")
        || value.starts_with("/var/folders/")
        || value.starts_with("/tmp/")
        || value.starts_with("~/")
        || value.starts_with("\\Users\\")
        || value.contains(":\\Users\\")
}

fn looks_like_secret_path_segment(value: &str) -> bool {
    looks_like_secret_token(value)
        || (value.len() >= 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
}
