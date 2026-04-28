//! Tauri commands operating on the new flat `provider_credentials` table.

use tauri::{AppHandle, Runtime, State};
use url::Url;

use crate::{
    commands::{
        CommandError, CommandResult, DeleteProviderCredentialRequestDto, ProviderCredentialDto,
        ProviderCredentialKindDto, ProviderCredentialReadinessProofDto,
        ProviderCredentialsSnapshotDto, UpsertProviderCredentialRequestDto,
    },
    global_db::open_global_database,
    provider_credentials::{
        delete_provider_credential as sql_delete, load_all_provider_credentials,
        load_provider_credential, load_provider_credentials_view_or_default, readiness_proof,
        upsert_provider_credential as sql_upsert, ProviderCredentialKind,
        ProviderCredentialReadinessProof, ProviderCredentialRecord, ProviderCredentialsView,
    },
    runtime::{
        resolve_runtime_provider_identity, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, VERTEX_PROVIDER_ID,
    },
    state::DesktopState,
};

pub(crate) fn load_provider_credentials_view<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<ProviderCredentialsView> {
    let connection = open_global_database(&state.global_db_path(app)?)?;
    load_provider_credentials_view_or_default(&connection)
}

#[tauri::command]
pub fn list_provider_credentials<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<ProviderCredentialsSnapshotDto> {
    let connection = open_global_database(&state.global_db_path(&app)?)?;
    let records = load_all_provider_credentials(&connection)?;
    Ok(ProviderCredentialsSnapshotDto {
        credentials: records.iter().map(provider_credential_dto).collect(),
    })
}

#[tauri::command]
pub fn upsert_provider_credential<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertProviderCredentialRequestDto,
) -> CommandResult<ProviderCredentialsSnapshotDto> {
    let provider_id = request.provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::invalid_request("providerId"));
    }
    // Validate provider_id is one we recognize. `resolve_runtime_provider_identity`
    // returns the canonical provider identity for the id, erroring on unknown
    // values.
    resolve_runtime_provider_identity(Some(provider_id), None).map_err(|diagnostic| {
        CommandError::user_fixable("provider_credentials_invalid", diagnostic.message)
    })?;

    let kind = ProviderCredentialKind::from(request.kind);

    if request.kind == ProviderCredentialKindDto::OAuthSession {
        return Err(CommandError::user_fixable(
            "provider_credentials_oauth_via_login",
            "Cadence persists OAuth sessions through the provider login flow, not the credential upsert command.",
        ));
    }

    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let base_url = request
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let api_version = request
        .api_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let region = request
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let project_id = request
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let default_model_id = request
        .default_model_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    validate_per_provider_fields(
        provider_id,
        kind,
        api_key,
        base_url,
        api_version,
        region,
        project_id,
    )?;

    let connection = open_global_database(&state.global_db_path(&app)?)?;
    let existing = load_provider_credential(&connection, provider_id)?;

    let updated_at = match (existing.as_ref(), api_key) {
        (Some(prev), Some(next)) if prev.api_key.as_deref() == Some(next) => {
            prev.updated_at.clone()
        }
        _ => crate::auth::now_timestamp(),
    };

    let record = ProviderCredentialRecord {
        provider_id: provider_id.to_owned(),
        kind,
        api_key: api_key.map(str::to_owned),
        oauth_account_id: None,
        oauth_session_id: None,
        oauth_access_token: None,
        oauth_refresh_token: None,
        oauth_expires_at: None,
        base_url: base_url.map(str::to_owned),
        api_version: api_version.map(str::to_owned),
        region: region.map(str::to_owned),
        project_id: project_id.map(str::to_owned),
        default_model_id: default_model_id.map(str::to_owned).or_else(|| {
            existing
                .as_ref()
                .and_then(|prev| prev.default_model_id.clone())
        }),
        updated_at,
    };

    sql_upsert(&connection, &record)?;
    let records = load_all_provider_credentials(&connection)?;
    Ok(ProviderCredentialsSnapshotDto {
        credentials: records.iter().map(provider_credential_dto).collect(),
    })
}

#[tauri::command]
pub fn delete_provider_credential<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteProviderCredentialRequestDto,
) -> CommandResult<ProviderCredentialsSnapshotDto> {
    let provider_id = request.provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::invalid_request("providerId"));
    }

    let connection = open_global_database(&state.global_db_path(&app)?)?;
    sql_delete(&connection, provider_id)?;
    let records = load_all_provider_credentials(&connection)?;
    Ok(ProviderCredentialsSnapshotDto {
        credentials: records.iter().map(provider_credential_dto).collect(),
    })
}

pub(crate) fn provider_credential_dto(record: &ProviderCredentialRecord) -> ProviderCredentialDto {
    ProviderCredentialDto {
        provider_id: record.provider_id.clone(),
        kind: ProviderCredentialKindDto::from(record.kind),
        has_api_key: record
            .api_key
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        oauth_account_id: record.oauth_account_id.clone(),
        oauth_session_id: record.oauth_session_id.clone(),
        has_oauth_access_token: record
            .oauth_access_token
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        oauth_expires_at: record.oauth_expires_at,
        base_url: record.base_url.clone(),
        api_version: record.api_version.clone(),
        region: record.region.clone(),
        project_id: record.project_id.clone(),
        default_model_id: record.default_model_id.clone(),
        readiness_proof: ProviderCredentialReadinessProofDto::from(readiness_proof(record)),
        updated_at: record.updated_at.clone(),
    }
}

fn validate_per_provider_fields(
    provider_id: &str,
    kind: ProviderCredentialKind,
    api_key: Option<&str>,
    base_url: Option<&str>,
    _api_version: Option<&str>,
    region: Option<&str>,
    project_id: Option<&str>,
) -> CommandResult<()> {
    match provider_id {
        OPENAI_CODEX_PROVIDER_ID => {
            return Err(CommandError::user_fixable(
                "provider_credentials_oauth_via_login",
                "Cadence persists OpenAI Codex credentials through the OAuth login flow.",
            ));
        }
        OLLAMA_PROVIDER_ID => {
            if !matches!(kind, ProviderCredentialKind::Local) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    "Cadence requires kind=local for Ollama credentials.",
                ));
            }
            if let Some(url) = base_url {
                validate_base_url(url)?;
            }
        }
        AZURE_OPENAI_PROVIDER_ID => {
            if !matches!(kind, ProviderCredentialKind::ApiKey) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    "Cadence requires kind=api_key for Azure OpenAI credentials.",
                ));
            }
            if api_key.is_none() {
                return Err(CommandError::invalid_request("apiKey"));
            }
            let url = base_url.ok_or_else(|| CommandError::invalid_request("baseUrl"))?;
            validate_base_url(url)?;
        }
        OPENAI_API_PROVIDER_ID => {
            if !matches!(
                kind,
                ProviderCredentialKind::ApiKey | ProviderCredentialKind::Local
            ) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    "Cadence requires kind=api_key or kind=local for OpenAI-API credentials.",
                ));
            }
            if matches!(kind, ProviderCredentialKind::ApiKey) && api_key.is_none() {
                return Err(CommandError::invalid_request("apiKey"));
            }
            if let Some(url) = base_url {
                validate_base_url(url)?;
            }
        }
        BEDROCK_PROVIDER_ID => {
            if !matches!(kind, ProviderCredentialKind::Ambient) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    "Cadence requires kind=ambient for Bedrock credentials.",
                ));
            }
            if region.is_none() {
                return Err(CommandError::invalid_request("region"));
            }
        }
        VERTEX_PROVIDER_ID => {
            if !matches!(kind, ProviderCredentialKind::Ambient) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    "Cadence requires kind=ambient for Vertex credentials.",
                ));
            }
            if region.is_none() {
                return Err(CommandError::invalid_request("region"));
            }
            if project_id.is_none() {
                return Err(CommandError::invalid_request("projectId"));
            }
        }
        _ => {
            // openrouter, anthropic, github_models, gemini_ai_studio
            if !matches!(kind, ProviderCredentialKind::ApiKey) {
                return Err(CommandError::user_fixable(
                    "provider_credentials_invalid_kind",
                    format!(
                        "Cadence requires kind=api_key for provider `{provider_id}` credentials."
                    ),
                ));
            }
            if api_key.is_none() {
                return Err(CommandError::invalid_request("apiKey"));
            }
        }
    }
    Ok(())
}

fn validate_base_url(value: &str) -> CommandResult<()> {
    let parsed = Url::parse(value).map_err(|error| {
        CommandError::user_fixable(
            "provider_credentials_invalid",
            format!("Cadence rejected baseUrl `{value}`: {error}"),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CommandError::user_fixable(
            "provider_credentials_invalid",
            format!(
                "Cadence rejected baseUrl `{value}` because scheme `{}` is not http(s).",
                parsed.scheme()
            ),
        ));
    }
    Ok(())
}

impl From<ProviderCredentialKind> for ProviderCredentialKindDto {
    fn from(value: ProviderCredentialKind) -> Self {
        match value {
            ProviderCredentialKind::ApiKey => Self::ApiKey,
            ProviderCredentialKind::OAuthSession => Self::OAuthSession,
            ProviderCredentialKind::Local => Self::Local,
            ProviderCredentialKind::Ambient => Self::Ambient,
        }
    }
}

impl From<ProviderCredentialKindDto> for ProviderCredentialKind {
    fn from(value: ProviderCredentialKindDto) -> Self {
        match value {
            ProviderCredentialKindDto::ApiKey => Self::ApiKey,
            ProviderCredentialKindDto::OAuthSession => Self::OAuthSession,
            ProviderCredentialKindDto::Local => Self::Local,
            ProviderCredentialKindDto::Ambient => Self::Ambient,
        }
    }
}

impl From<ProviderCredentialReadinessProof> for ProviderCredentialReadinessProofDto {
    fn from(value: ProviderCredentialReadinessProof) -> Self {
        match value {
            ProviderCredentialReadinessProof::OAuthSession => Self::OAuthSession,
            ProviderCredentialReadinessProof::StoredSecret => Self::StoredSecret,
            ProviderCredentialReadinessProof::Local => Self::Local,
            ProviderCredentialReadinessProof::Ambient => Self::Ambient,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_openai_codex_via_upsert() {
        let err = validate_per_provider_fields(
            OPENAI_CODEX_PROVIDER_ID,
            ProviderCredentialKind::OAuthSession,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("OpenAI Codex must use the OAuth login flow");
        assert_eq!(err.code, "provider_credentials_oauth_via_login");
    }

    #[test]
    fn validate_requires_api_key_for_openrouter() {
        let err = validate_per_provider_fields(
            "openrouter",
            ProviderCredentialKind::ApiKey,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("openrouter without api_key should fail");
        assert_eq!(err.code, "invalid_request");
    }

    #[test]
    fn validate_requires_local_kind_for_ollama() {
        let err = validate_per_provider_fields(
            OLLAMA_PROVIDER_ID,
            ProviderCredentialKind::ApiKey,
            Some("sk-test"),
            None,
            None,
            None,
            None,
        )
        .expect_err("ollama with api_key should fail");
        assert_eq!(err.code, "provider_credentials_invalid_kind");
    }

    #[test]
    fn validate_azure_openai_requires_base_url() {
        let err = validate_per_provider_fields(
            AZURE_OPENAI_PROVIDER_ID,
            ProviderCredentialKind::ApiKey,
            Some("sk-test"),
            None,
            None,
            None,
            None,
        )
        .expect_err("azure openai without base_url should fail");
        assert_eq!(err.code, "invalid_request");
    }

    #[test]
    fn validate_vertex_requires_region_and_project() {
        let err = validate_per_provider_fields(
            VERTEX_PROVIDER_ID,
            ProviderCredentialKind::Ambient,
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("vertex without region should fail");
        assert_eq!(err.code, "invalid_request");

        let err = validate_per_provider_fields(
            VERTEX_PROVIDER_ID,
            ProviderCredentialKind::Ambient,
            None,
            None,
            None,
            Some("us-east5"),
            None,
        )
        .expect_err("vertex without project_id should fail");
        assert_eq!(err.code, "invalid_request");
    }

    #[test]
    fn validate_bedrock_requires_region() {
        validate_per_provider_fields(
            BEDROCK_PROVIDER_ID,
            ProviderCredentialKind::Ambient,
            None,
            None,
            None,
            Some("us-east-1"),
            None,
        )
        .expect("bedrock with region should pass");
    }

    #[test]
    fn validate_base_url_rejects_non_http() {
        let err = validate_base_url("file:///tmp/x").expect_err("file:// must be rejected");
        assert_eq!(err.code, "provider_credentials_invalid");
    }

    #[test]
    fn dto_does_not_leak_api_key_or_tokens() {
        let record = ProviderCredentialRecord {
            provider_id: "openai_codex".into(),
            kind: ProviderCredentialKind::OAuthSession,
            api_key: None,
            oauth_account_id: Some("acct".into()),
            oauth_session_id: Some("sess".into()),
            oauth_access_token: Some("super-secret".into()),
            oauth_refresh_token: Some("refresh-secret".into()),
            oauth_expires_at: Some(1_900_000_000),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let dto = provider_credential_dto(&record);
        let serialized = serde_json::to_string(&dto).expect("serialize");
        assert!(
            !serialized.contains("super-secret") && !serialized.contains("refresh-secret"),
            "serialized DTO must not leak OAuth tokens: {serialized}"
        );
        assert!(dto.has_oauth_access_token);
        assert_eq!(dto.kind, ProviderCredentialKindDto::OAuthSession);
        assert_eq!(
            dto.readiness_proof,
            ProviderCredentialReadinessProofDto::OAuthSession
        );
    }
}
