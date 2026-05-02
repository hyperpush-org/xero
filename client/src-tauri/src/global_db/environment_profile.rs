use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::runtime::redaction::find_prohibited_persistence_content;

pub const ENVIRONMENT_PROFILE_SCHEMA_VERSION: u32 = 1;

pub type EnvironmentProfileValidationResult<T> = Result<T, EnvironmentProfileValidationError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{field}: {message}")]
pub struct EnvironmentProfileValidationError {
    pub field: String,
    pub message: String,
}

impl EnvironmentProfileValidationError {
    fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentProfileStatus {
    Pending,
    Probing,
    Ready,
    Partial,
    Failed,
}

impl EnvironmentProfileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Probing => "probing",
            Self::Ready => "ready",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }
}

pub fn parse_environment_profile_status(
    value: &str,
) -> EnvironmentProfileValidationResult<EnvironmentProfileStatus> {
    match value {
        "pending" => Ok(EnvironmentProfileStatus::Pending),
        "probing" => Ok(EnvironmentProfileStatus::Probing),
        "ready" => Ok(EnvironmentProfileStatus::Ready),
        "partial" => Ok(EnvironmentProfileStatus::Partial),
        "failed" => Ok(EnvironmentProfileStatus::Failed),
        _ => Err(EnvironmentProfileValidationError::invalid(
            "status",
            format!("unknown environment profile status `{value}`"),
        )),
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentToolCategory {
    BaseDeveloperTool,
    PackageManager,
    PlatformPackageManager,
    LanguageRuntime,
    ContainerOrchestration,
    MobileTooling,
    CloudDeployment,
    DatabaseCli,
    SolanaTooling,
    AgentAiCli,
    Editor,
    BuildTool,
    Linter,
    VersionManager,
    IacTool,
    ShellUtility,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentToolSource {
    BundledToolchain,
    ManagedToolchain,
    Path,
    CommonDevDir,
    PlatformDefault,
    Unresolved,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentToolProbeStatus {
    Ok,
    Missing,
    Timeout,
    Failed,
    Skipped,
    NotRun,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentCapabilityState {
    Ready,
    Partial,
    Missing,
    Blocked,
    Unknown,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPermissionKind {
    OsPermission,
    ProtectedPath,
    NetworkAccess,
    InstallationAction,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPermissionStatus {
    Pending,
    Granted,
    Denied,
    Skipped,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPlatform {
    pub os_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPathProfile {
    pub entry_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentToolRecord {
    pub id: String,
    pub category: EnvironmentToolCategory,
    pub command: String,
    pub present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub source: EnvironmentToolSource,
    pub probe_status: EnvironmentToolProbeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentCapability {
    pub id: String,
    pub state: EnvironmentCapabilityState,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPermissionRequest {
    pub id: String,
    pub kind: EnvironmentPermissionKind,
    pub status: EnvironmentPermissionStatus,
    pub title: String,
    pub reason: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentDiagnostic {
    pub code: String,
    pub severity: EnvironmentDiagnosticSeverity,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentProfilePayload {
    pub schema_version: u32,
    pub platform: EnvironmentPlatform,
    pub path: EnvironmentPathProfile,
    #[serde(default)]
    pub tools: Vec<EnvironmentToolRecord>,
    #[serde(default)]
    pub capabilities: Vec<EnvironmentCapability>,
    #[serde(default)]
    pub permissions: Vec<EnvironmentPermissionRequest>,
    #[serde(default)]
    pub diagnostics: Vec<EnvironmentDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentToolSummary {
    pub id: String,
    pub category: EnvironmentToolCategory,
    pub present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    pub probe_status: EnvironmentToolProbeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentProfileSummary {
    pub schema_version: u32,
    pub status: EnvironmentProfileStatus,
    pub platform: EnvironmentPlatform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
    #[serde(default)]
    pub tools: Vec<EnvironmentToolSummary>,
    #[serde(default)]
    pub capabilities: Vec<EnvironmentCapability>,
    #[serde(default)]
    pub permission_requests: Vec<EnvironmentPermissionRequest>,
    #[serde(default)]
    pub diagnostics: Vec<EnvironmentDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentProfileRow {
    pub schema_version: u32,
    pub status: EnvironmentProfileStatus,
    pub os_kind: String,
    pub os_version: Option<String>,
    pub arch: String,
    pub default_shell: Option<String>,
    pub path_fingerprint: Option<String>,
    pub payload_json: String,
    pub summary_json: String,
    pub permission_requests_json: String,
    pub diagnostics_json: String,
    pub probe_started_at: Option<String>,
    pub probe_completed_at: Option<String>,
    pub refreshed_at: String,
}

pub fn validate_environment_payload_json(
    json: &str,
) -> EnvironmentProfileValidationResult<EnvironmentProfilePayload> {
    decode_and_validate_json("payloadJson", json, validate_environment_payload)
}

pub fn validate_environment_summary_json(
    json: &str,
) -> EnvironmentProfileValidationResult<EnvironmentProfileSummary> {
    decode_and_validate_json("summaryJson", json, validate_environment_summary)
}

pub fn validate_permission_requests_json(
    json: &str,
) -> EnvironmentProfileValidationResult<Vec<EnvironmentPermissionRequest>> {
    decode_and_validate_json("permissionRequestsJson", json, |permissions: &Vec<_>| {
        validate_permission_requests(permissions)
    })
}

pub fn validate_diagnostics_json(
    json: &str,
) -> EnvironmentProfileValidationResult<Vec<EnvironmentDiagnostic>> {
    decode_and_validate_json("diagnosticsJson", json, |diagnostics: &Vec<_>| {
        validate_diagnostics(diagnostics)
    })
}

pub fn validate_environment_payload(
    payload: &EnvironmentProfilePayload,
) -> EnvironmentProfileValidationResult<()> {
    validate_schema_version(payload.schema_version, "payload.schemaVersion")?;
    validate_platform(&payload.platform, "payload.platform")?;
    validate_path_profile(&payload.path, "payload.path")?;
    validate_tools(&payload.tools, "payload.tools")?;
    validate_capabilities(&payload.capabilities, "payload.capabilities")?;
    validate_permission_requests(&payload.permissions)?;
    validate_diagnostics(&payload.diagnostics)?;
    reject_secret_like_serialized_strings(payload, "payload")
}

pub fn validate_environment_summary(
    summary: &EnvironmentProfileSummary,
) -> EnvironmentProfileValidationResult<()> {
    validate_schema_version(summary.schema_version, "summary.schemaVersion")?;
    validate_platform(&summary.platform, "summary.platform")?;
    validate_tools_summary(&summary.tools, "summary.tools")?;
    validate_capabilities(&summary.capabilities, "summary.capabilities")?;
    validate_permission_requests(&summary.permission_requests)?;
    validate_diagnostics(&summary.diagnostics)?;
    validate_optional_non_empty(summary.refreshed_at.as_deref(), "summary.refreshedAt")?;
    reject_secret_like_serialized_strings(summary, "summary")
}

pub fn validate_environment_profile_row(
    row: &EnvironmentProfileRow,
) -> EnvironmentProfileValidationResult<()> {
    validate_schema_version(row.schema_version, "schemaVersion")?;
    validate_required(&row.os_kind, "osKind")?;
    validate_required(&row.arch, "arch")?;
    validate_optional_non_empty(row.os_version.as_deref(), "osVersion")?;
    validate_optional_non_empty(row.default_shell.as_deref(), "defaultShell")?;
    validate_optional_non_empty(row.path_fingerprint.as_deref(), "pathFingerprint")?;
    validate_optional_non_empty(row.probe_started_at.as_deref(), "probeStartedAt")?;
    validate_optional_non_empty(row.probe_completed_at.as_deref(), "probeCompletedAt")?;
    validate_required(&row.refreshed_at, "refreshedAt")?;

    let payload = validate_environment_payload_json(&row.payload_json)?;
    let summary = validate_environment_summary_json(&row.summary_json)?;
    let permission_requests = validate_permission_requests_json(&row.permission_requests_json)?;
    let diagnostics = validate_diagnostics_json(&row.diagnostics_json)?;

    validate_row_matches_payload(row, &payload)?;
    validate_row_matches_summary(row, &summary)?;

    if payload.permissions != permission_requests {
        return Err(EnvironmentProfileValidationError::invalid(
            "permissionRequestsJson",
            "permission request projection must match payload.permissions",
        ));
    }
    if payload.diagnostics != diagnostics {
        return Err(EnvironmentProfileValidationError::invalid(
            "diagnosticsJson",
            "diagnostic projection must match payload.diagnostics",
        ));
    }

    Ok(())
}

fn decode_and_validate_json<T>(
    field: &'static str,
    json: &str,
    validate: impl FnOnce(&T) -> EnvironmentProfileValidationResult<()>,
) -> EnvironmentProfileValidationResult<T>
where
    T: DeserializeOwned + Serialize,
{
    if json.trim().is_empty() {
        return Err(EnvironmentProfileValidationError::invalid(
            field,
            "JSON payload must not be empty",
        ));
    }

    let value: JsonValue = serde_json::from_str(json).map_err(|error| {
        EnvironmentProfileValidationError::invalid(field, format!("invalid JSON: {error}"))
    })?;
    reject_secret_like_json_strings(&value, field)?;

    let decoded = serde_json::from_value::<T>(value).map_err(|error| {
        EnvironmentProfileValidationError::invalid(field, format!("invalid shape: {error}"))
    })?;
    validate(&decoded)?;
    Ok(decoded)
}

fn validate_row_matches_payload(
    row: &EnvironmentProfileRow,
    payload: &EnvironmentProfilePayload,
) -> EnvironmentProfileValidationResult<()> {
    if row.schema_version != payload.schema_version {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.schemaVersion",
            "payload schema version must match the environment_profile row",
        ));
    }
    if row.os_kind != payload.platform.os_kind {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.platform.osKind",
            "payload OS kind must match the environment_profile row",
        ));
    }
    if row.os_version != payload.platform.os_version {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.platform.osVersion",
            "payload OS version must match the environment_profile row",
        ));
    }
    if row.arch != payload.platform.arch {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.platform.arch",
            "payload architecture must match the environment_profile row",
        ));
    }
    if row.default_shell != payload.platform.default_shell {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.platform.defaultShell",
            "payload default shell must match the environment_profile row",
        ));
    }
    if row.path_fingerprint != payload.path.fingerprint {
        return Err(EnvironmentProfileValidationError::invalid(
            "payload.path.fingerprint",
            "payload PATH fingerprint must match the environment_profile row",
        ));
    }
    Ok(())
}

fn validate_row_matches_summary(
    row: &EnvironmentProfileRow,
    summary: &EnvironmentProfileSummary,
) -> EnvironmentProfileValidationResult<()> {
    if row.schema_version != summary.schema_version {
        return Err(EnvironmentProfileValidationError::invalid(
            "summary.schemaVersion",
            "summary schema version must match the environment_profile row",
        ));
    }
    if row.status != summary.status {
        return Err(EnvironmentProfileValidationError::invalid(
            "summary.status",
            "summary status must match the environment_profile row",
        ));
    }
    if row.os_kind != summary.platform.os_kind
        || row.os_version != summary.platform.os_version
        || row.arch != summary.platform.arch
        || row.default_shell != summary.platform.default_shell
    {
        return Err(EnvironmentProfileValidationError::invalid(
            "summary.platform",
            "summary platform must match the environment_profile row",
        ));
    }
    if let Some(refreshed_at) = &summary.refreshed_at {
        if refreshed_at != &row.refreshed_at {
            return Err(EnvironmentProfileValidationError::invalid(
                "summary.refreshedAt",
                "summary refresh timestamp must match the environment_profile row",
            ));
        }
    }
    Ok(())
}

fn validate_schema_version(
    version: u32,
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    if version == 0 {
        return Err(EnvironmentProfileValidationError::invalid(
            field,
            "schema version must be greater than zero",
        ));
    }
    Ok(())
}

fn validate_platform(
    platform: &EnvironmentPlatform,
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    validate_required(&platform.os_kind, join_field(field, "osKind"))?;
    validate_required(&platform.arch, join_field(field, "arch"))?;
    validate_optional_non_empty(
        platform.os_version.as_deref(),
        join_field(field, "osVersion"),
    )?;
    validate_optional_non_empty(
        platform.default_shell.as_deref(),
        join_field(field, "defaultShell"),
    )
}

fn validate_path_profile(
    path: &EnvironmentPathProfile,
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    validate_optional_non_empty(
        path.fingerprint.as_deref(),
        join_field(field, "fingerprint"),
    )?;
    validate_non_empty_items(&path.sources, join_field(field, "sources"))
}

fn validate_tools(
    tools: &[EnvironmentToolRecord],
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    for (index, tool) in tools.iter().enumerate() {
        let base = indexed_field(field, index);
        validate_required(&tool.id, join_field(&base, "id"))?;
        validate_required(&tool.command, join_field(&base, "command"))?;
        validate_optional_non_empty(tool.path.as_deref(), join_field(&base, "path"))?;
        validate_optional_non_empty(tool.version.as_deref(), join_field(&base, "version"))?;
        if tool.present
            && matches!(
                tool.probe_status,
                EnvironmentToolProbeStatus::Missing | EnvironmentToolProbeStatus::NotRun
            )
        {
            return Err(EnvironmentProfileValidationError::invalid(
                join_field(&base, "probeStatus"),
                "present tools must not use a missing or not_run probe status",
            ));
        }
    }
    Ok(())
}

fn validate_tools_summary(
    tools: &[EnvironmentToolSummary],
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    for (index, tool) in tools.iter().enumerate() {
        let base = indexed_field(field, index);
        validate_required(&tool.id, join_field(&base, "id"))?;
        validate_optional_non_empty(tool.version.as_deref(), join_field(&base, "version"))?;
        validate_optional_non_empty(
            tool.display_path.as_deref(),
            join_field(&base, "displayPath"),
        )?;
        if let Some(display_path) = &tool.display_path {
            if looks_like_absolute_path(display_path) {
                return Err(EnvironmentProfileValidationError::invalid(
                    join_field(&base, "displayPath"),
                    "summary tool paths must be redacted or display-only, not absolute paths",
                ));
            }
        }
    }
    Ok(())
}

fn validate_capabilities(
    capabilities: &[EnvironmentCapability],
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    for (index, capability) in capabilities.iter().enumerate() {
        let base = indexed_field(field, index);
        validate_required(&capability.id, join_field(&base, "id"))?;
        validate_non_empty_items(&capability.evidence, join_field(&base, "evidence"))?;
        validate_optional_non_empty(capability.message.as_deref(), join_field(&base, "message"))?;
    }
    Ok(())
}

fn validate_permission_requests(
    permissions: &[EnvironmentPermissionRequest],
) -> EnvironmentProfileValidationResult<()> {
    for (index, permission) in permissions.iter().enumerate() {
        let base = indexed_field("permissions", index);
        validate_required(&permission.id, join_field(&base, "id"))?;
        validate_required(&permission.title, join_field(&base, "title"))?;
        validate_required(&permission.reason, join_field(&base, "reason"))?;
    }
    Ok(())
}

fn validate_diagnostics(
    diagnostics: &[EnvironmentDiagnostic],
) -> EnvironmentProfileValidationResult<()> {
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let base = indexed_field("diagnostics", index);
        validate_required(&diagnostic.code, join_field(&base, "code"))?;
        validate_required(&diagnostic.message, join_field(&base, "message"))?;
        validate_optional_non_empty(diagnostic.tool_id.as_deref(), join_field(&base, "toolId"))?;
    }
    Ok(())
}

fn validate_required(
    value: &str,
    field: impl Into<String>,
) -> EnvironmentProfileValidationResult<()> {
    if value.trim().is_empty() {
        return Err(EnvironmentProfileValidationError::invalid(
            field,
            "must not be empty",
        ));
    }
    Ok(())
}

fn validate_optional_non_empty(
    value: Option<&str>,
    field: impl Into<String>,
) -> EnvironmentProfileValidationResult<()> {
    if let Some(value) = value {
        validate_required(value, field)?;
    }
    Ok(())
}

fn validate_non_empty_items(
    values: &[String],
    field: impl Into<String>,
) -> EnvironmentProfileValidationResult<()> {
    let field = field.into();
    for (index, value) in values.iter().enumerate() {
        validate_required(value, indexed_field(&field, index))?;
    }
    Ok(())
}

fn reject_secret_like_serialized_strings<T: Serialize>(
    value: &T,
    field: &'static str,
) -> EnvironmentProfileValidationResult<()> {
    let json = serde_json::to_value(value).map_err(|error| {
        EnvironmentProfileValidationError::invalid(field, format!("could not encode JSON: {error}"))
    })?;
    reject_secret_like_json_strings(&json, field)
}

fn reject_secret_like_json_strings(
    value: &JsonValue,
    field: impl Into<String>,
) -> EnvironmentProfileValidationResult<()> {
    let field = field.into();
    match value {
        JsonValue::String(text) => {
            if let Some(reason) = find_prohibited_persistence_content(text) {
                return Err(EnvironmentProfileValidationError::invalid(
                    field,
                    format!("contains secret-like or unsafe output: {reason}"),
                ));
            }
        }
        JsonValue::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                reject_secret_like_json_strings(value, indexed_field(&field, index))?;
            }
        }
        JsonValue::Object(map) => {
            for (key, value) in map {
                reject_secret_like_json_strings(value, join_field(&field, key))?;
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
    Ok(())
}

fn looks_like_absolute_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("\\\\")
        || value.as_bytes().get(0..3).is_some_and(|prefix| {
            prefix[0].is_ascii_alphabetic() && prefix[1] == b':' && prefix[2] == b'\\'
        })
}

fn join_field(base: &str, child: &str) -> String {
    format!("{base}.{child}")
}

fn indexed_field(base: &str, index: usize) -> String {
    format!("{base}[{index}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_platform() -> EnvironmentPlatform {
        EnvironmentPlatform {
            os_kind: "macos".into(),
            os_version: Some("15.4".into()),
            arch: "aarch64".into(),
            default_shell: Some("zsh".into()),
        }
    }

    fn sample_payload() -> EnvironmentProfilePayload {
        EnvironmentProfilePayload {
            schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            platform: sample_platform(),
            path: EnvironmentPathProfile {
                entry_count: 2,
                fingerprint: Some("sha256-demo".into()),
                sources: vec!["tauri-process-path".into(), "common-dev-dirs".into()],
            },
            tools: vec![EnvironmentToolRecord {
                id: "node".into(),
                category: EnvironmentToolCategory::LanguageRuntime,
                command: "node".into(),
                present: true,
                path: Some("/opt/homebrew/bin/node".into()),
                version: Some("v20.11.1".into()),
                source: EnvironmentToolSource::Path,
                probe_status: EnvironmentToolProbeStatus::Ok,
                duration_ms: Some(18),
            }],
            capabilities: vec![EnvironmentCapability {
                id: "node_project_ready".into(),
                state: EnvironmentCapabilityState::Ready,
                evidence: vec!["node".into(), "pnpm".into()],
                message: None,
            }],
            permissions: vec![],
            diagnostics: vec![],
        }
    }

    fn sample_summary() -> EnvironmentProfileSummary {
        EnvironmentProfileSummary {
            schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            status: EnvironmentProfileStatus::Ready,
            platform: sample_platform(),
            refreshed_at: Some("2026-04-30T12:00:00Z".into()),
            tools: vec![EnvironmentToolSummary {
                id: "node".into(),
                category: EnvironmentToolCategory::LanguageRuntime,
                present: true,
                version: Some("v20.11.1".into()),
                display_path: Some("~/bin/node".into()),
                probe_status: EnvironmentToolProbeStatus::Ok,
            }],
            capabilities: vec![EnvironmentCapability {
                id: "node_project_ready".into(),
                state: EnvironmentCapabilityState::Ready,
                evidence: vec!["node".into(), "pnpm".into()],
                message: None,
            }],
            permission_requests: vec![],
            diagnostics: vec![],
        }
    }

    #[test]
    fn validates_payload_contract() {
        let payload = sample_payload();
        validate_environment_payload(&payload).expect("sample payload should validate");

        let encoded = serde_json::to_string(&payload).expect("encode payload");
        let decoded =
            validate_environment_payload_json(&encoded).expect("payload json should validate");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn rejects_invalid_json() {
        let error = validate_environment_payload_json("{").expect_err("invalid JSON is rejected");
        assert_eq!(error.field, "payloadJson");
        assert!(error.message.contains("invalid JSON"));
    }

    #[test]
    fn rejects_unknown_status_values() {
        let error = parse_environment_profile_status("finished")
            .expect_err("unknown row status is rejected");
        assert_eq!(error.field, "status");

        let mut summary = serde_json::to_value(sample_summary()).expect("summary to json");
        summary["status"] = JsonValue::String("finished".into());
        let error = validate_environment_summary_json(&summary.to_string())
            .expect_err("unknown summary status is rejected");
        assert_eq!(error.field, "summaryJson");
        assert!(error.message.contains("invalid shape"));
    }

    #[test]
    fn rejects_empty_ids() {
        let mut payload = sample_payload();
        payload.tools[0].id = " ".into();
        let error = validate_environment_payload(&payload).expect_err("empty id is rejected");
        assert_eq!(error.field, "payload.tools[0].id");
    }

    #[test]
    fn rejects_secret_like_output() {
        let mut payload = sample_payload();
        payload.tools[0].version = Some("version sk-demo".into());
        let error =
            validate_environment_payload(&payload).expect_err("secret-like output is rejected");
        assert!(error.message.contains("secret-like"));
    }

    #[test]
    fn rejects_absolute_paths_in_summary() {
        let mut summary = sample_summary();
        summary.tools[0].display_path = Some("/Users/alice/.local/bin/node".into());
        let error = validate_environment_summary(&summary).expect_err("absolute path is rejected");
        assert_eq!(error.field, "summary.tools[0].displayPath");

        summary.tools[0].display_path = Some(r"C:\Users\alice\.local\bin\node.exe".into());
        let error =
            validate_environment_summary(&summary).expect_err("Windows absolute path is rejected");
        assert_eq!(error.field, "summary.tools[0].displayPath");
    }

    #[test]
    fn validates_profile_row_consistency() {
        let payload = sample_payload();
        let summary = sample_summary();
        let row = EnvironmentProfileRow {
            schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            status: EnvironmentProfileStatus::Ready,
            os_kind: "macos".into(),
            os_version: Some("15.4".into()),
            arch: "aarch64".into(),
            default_shell: Some("zsh".into()),
            path_fingerprint: Some("sha256-demo".into()),
            payload_json: serde_json::to_string(&payload).expect("encode payload"),
            summary_json: serde_json::to_string(&summary).expect("encode summary"),
            permission_requests_json: "[]".into(),
            diagnostics_json: "[]".into(),
            probe_started_at: Some("2026-04-30T11:59:59Z".into()),
            probe_completed_at: Some("2026-04-30T12:00:00Z".into()),
            refreshed_at: "2026-04-30T12:00:00Z".into(),
        };

        validate_environment_profile_row(&row).expect("row should validate");
    }

    #[test]
    fn rejects_row_payload_mismatch() {
        let payload = sample_payload();
        let summary = sample_summary();
        let row = EnvironmentProfileRow {
            schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            status: EnvironmentProfileStatus::Ready,
            os_kind: "linux".into(),
            os_version: Some("15.4".into()),
            arch: "aarch64".into(),
            default_shell: Some("zsh".into()),
            path_fingerprint: Some("sha256-demo".into()),
            payload_json: serde_json::to_string(&payload).expect("encode payload"),
            summary_json: serde_json::to_string(&summary).expect("encode summary"),
            permission_requests_json: "[]".into(),
            diagnostics_json: "[]".into(),
            probe_started_at: None,
            probe_completed_at: None,
            refreshed_at: "2026-04-30T12:00:00Z".into(),
        };

        let error = validate_environment_profile_row(&row).expect_err("mismatch is rejected");
        assert_eq!(error.field, "payload.platform.osKind");
    }
}
