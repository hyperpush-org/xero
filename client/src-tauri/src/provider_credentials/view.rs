use rusqlite::Connection;

use crate::{
    commands::CommandResult,
    runtime::{
        normalize_openai_codex_model_id, ANTHROPIC_PROVIDER_ID, ANTHROPIC_RUNTIME_KIND,
        AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID, CURSOR_DEFAULT_MODEL_ID, CURSOR_PROVIDER_ID,
        CURSOR_RUNTIME_KIND, DEEPSEEK_PROVIDER_ID, DEEPSEEK_RUNTIME_KIND,
        GEMINI_AI_STUDIO_PROVIDER_ID, GEMINI_RUNTIME_KIND, GITHUB_MODELS_PROVIDER_ID,
        OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID,
        OPENAI_COMPATIBLE_RUNTIME_KIND, OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID,
        XAI_DEFAULT_MODEL_ID, XAI_PROVIDER_ID, XAI_RUNTIME_KIND,
    },
};

use super::{
    load_all_provider_credentials, ProviderCredentialKind, ProviderCredentialReadinessProof,
    ProviderCredentialRecord,
};

pub const OPENAI_CODEX_DEFAULT_PROFILE_ID: &str = "openai_codex-default";
pub const OPENROUTER_DEFAULT_PROFILE_ID: &str = "openrouter-default";
pub const ANTHROPIC_DEFAULT_PROFILE_ID: &str = "anthropic-default";
pub const GITHUB_MODELS_DEFAULT_PROFILE_ID: &str = "github_models-default";
pub const DEEPSEEK_DEFAULT_PROFILE_ID: &str = "deepseek-default";
pub const XAI_DEFAULT_PROFILE_ID: &str = "xai-default";
pub const CURSOR_DEFAULT_PROFILE_ID: &str = "external_cursor_sdk-default";
pub const OPENROUTER_FALLBACK_MODEL_ID: &str = "openai/gpt-4.1-mini";
pub const DEEPSEEK_FALLBACK_MODEL_ID: &str = "deepseek-v4-pro";

const OPENAI_CODEX_DEFAULT_PROFILE_LABEL: &str = "OpenAI Codex";
const OPENROUTER_DEFAULT_PROFILE_LABEL: &str = "OpenRouter";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialsView {
    records: Vec<ProviderCredentialRecord>,
    profiles: Vec<ProviderCredentialProfile>,
    api_keys: Vec<ProviderApiKeyCredentialEntry>,
    active_profile_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialProfile {
    pub profile_id: String,
    pub provider_id: String,
    pub runtime_kind: String,
    pub label: String,
    pub model_id: String,
    pub preset_id: Option<String>,
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub credential_link: Option<ProviderCredentialLink>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialLink {
    OpenAiCodex {
        account_id: String,
        session_id: String,
        updated_at: String,
    },
    Xai {
        account_id: String,
        session_id: String,
        updated_at: String,
    },
    ApiKey {
        updated_at: String,
    },
    Local {
        updated_at: String,
    },
    Ambient {
        updated_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiKeyCredentialEntry {
    pub profile_id: String,
    pub api_key: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialReadinessStatus {
    Ready,
    Missing,
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialReadinessProjection {
    pub ready: bool,
    pub status: ProviderCredentialReadinessStatus,
    pub proof: Option<ProviderCredentialReadinessProof>,
    pub proof_updated_at: Option<String>,
}

pub fn load_provider_credentials_view_or_default(
    connection: &Connection,
) -> CommandResult<ProviderCredentialsView> {
    let records = load_all_provider_credentials(connection)?;
    Ok(ProviderCredentialsView::from_records(records))
}

impl ProviderCredentialsView {
    pub fn from_records(records: Vec<ProviderCredentialRecord>) -> Self {
        let mut profiles = Vec::new();
        let mut api_keys = Vec::new();

        for record in &records {
            let Some(synthesized) = synthesize_profile_from_credential(record) else {
                continue;
            };
            if let Some(api_key_entry) = synthesized.api_key_entry {
                api_keys.push(api_key_entry);
            }
            profiles.push(synthesized.profile);
        }

        let active_profile_id = profiles
            .iter()
            .find(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
            .map(|profile| profile.profile_id.clone())
            .or_else(|| profiles.first().map(|profile| profile.profile_id.clone()))
            .unwrap_or_default();

        Self {
            records,
            profiles,
            api_keys,
            active_profile_id,
        }
    }

    #[doc(hidden)]
    pub fn from_projected_profiles_for_tests(
        active_profile_id: String,
        profiles: Vec<ProviderCredentialProfile>,
        api_keys: Vec<ProviderApiKeyCredentialEntry>,
    ) -> Self {
        Self {
            records: Vec::new(),
            profiles,
            api_keys,
            active_profile_id,
        }
    }

    pub fn active_profile_id(&self) -> &str {
        &self.active_profile_id
    }

    pub fn with_active_profile_id(mut self, profile_id: String) -> Self {
        self.active_profile_id = profile_id;
        self
    }

    pub fn profiles(&self) -> &[ProviderCredentialProfile] {
        &self.profiles
    }

    pub fn records(&self) -> &[ProviderCredentialRecord] {
        &self.records
    }

    pub fn active_profile(&self) -> Option<&ProviderCredentialProfile> {
        self.profile(&self.active_profile_id)
    }

    pub fn profile(&self, profile_id: &str) -> Option<&ProviderCredentialProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
    }

    pub fn record_for_provider(&self, provider_id: &str) -> Option<&ProviderCredentialRecord> {
        self.records
            .iter()
            .find(|record| record.provider_id == provider_id)
    }

    pub fn matched_api_key_credential_for_profile(
        &self,
        profile_id: &str,
    ) -> Option<&ProviderApiKeyCredentialEntry> {
        self.api_key_credential(profile_id)
    }

    fn api_key_credential(&self, profile_id: &str) -> Option<&ProviderApiKeyCredentialEntry> {
        self.api_keys
            .iter()
            .find(|entry| entry.profile_id == profile_id)
    }
}

impl ProviderCredentialProfile {
    pub fn readiness(&self) -> ProviderCredentialReadinessProjection {
        match &self.credential_link {
            Some(ProviderCredentialLink::OpenAiCodex { updated_at, .. }) => {
                ProviderCredentialReadinessProjection {
                    ready: true,
                    status: ProviderCredentialReadinessStatus::Ready,
                    proof: Some(ProviderCredentialReadinessProof::OAuthSession),
                    proof_updated_at: Some(updated_at.clone()),
                }
            }
            Some(ProviderCredentialLink::Xai { updated_at, .. }) => {
                ProviderCredentialReadinessProjection {
                    ready: true,
                    status: ProviderCredentialReadinessStatus::Ready,
                    proof: Some(ProviderCredentialReadinessProof::OAuthSession),
                    proof_updated_at: Some(updated_at.clone()),
                }
            }
            Some(ProviderCredentialLink::ApiKey { updated_at }) => {
                ProviderCredentialReadinessProjection {
                    ready: true,
                    status: ProviderCredentialReadinessStatus::Ready,
                    proof: Some(ProviderCredentialReadinessProof::StoredSecret),
                    proof_updated_at: Some(updated_at.clone()),
                }
            }
            Some(ProviderCredentialLink::Local { updated_at }) => {
                ProviderCredentialReadinessProjection {
                    ready: true,
                    status: ProviderCredentialReadinessStatus::Ready,
                    proof: Some(ProviderCredentialReadinessProof::Local),
                    proof_updated_at: Some(updated_at.clone()),
                }
            }
            Some(ProviderCredentialLink::Ambient { updated_at }) => {
                ProviderCredentialReadinessProjection {
                    ready: true,
                    status: ProviderCredentialReadinessStatus::Ready,
                    proof: Some(ProviderCredentialReadinessProof::Ambient),
                    proof_updated_at: Some(updated_at.clone()),
                }
            }
            None => ProviderCredentialReadinessProjection {
                ready: false,
                status: ProviderCredentialReadinessStatus::Missing,
                proof: None,
                proof_updated_at: None,
            },
        }
    }
}

struct SynthesizedProfile {
    profile: ProviderCredentialProfile,
    api_key_entry: Option<ProviderApiKeyCredentialEntry>,
}

fn synthesize_profile_from_credential(
    record: &ProviderCredentialRecord,
) -> Option<SynthesizedProfile> {
    let provider_id = record.provider_id.as_str();
    let (profile_id, label, runtime_kind, preset_id) = synthesized_profile_metadata(provider_id);

    let credential_link = match record.kind {
        ProviderCredentialKind::OAuthSession => {
            let account_id = record.oauth_account_id.clone()?;
            let session_id = record.oauth_session_id.clone()?;
            match provider_id {
                OPENAI_CODEX_PROVIDER_ID => Some(ProviderCredentialLink::OpenAiCodex {
                    account_id,
                    session_id,
                    updated_at: record.updated_at.clone(),
                }),
                XAI_PROVIDER_ID => Some(ProviderCredentialLink::Xai {
                    account_id,
                    session_id,
                    updated_at: record.updated_at.clone(),
                }),
                _ => None,
            }
        }
        ProviderCredentialKind::ApiKey => Some(ProviderCredentialLink::ApiKey {
            updated_at: record.updated_at.clone(),
        }),
        ProviderCredentialKind::Local => Some(ProviderCredentialLink::Local {
            updated_at: record.updated_at.clone(),
        }),
        ProviderCredentialKind::Ambient => Some(ProviderCredentialLink::Ambient {
            updated_at: record.updated_at.clone(),
        }),
    };

    let model_id = record
        .default_model_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if provider_id == OPENAI_CODEX_PROVIDER_ID {
                normalize_openai_codex_model_id(OPENAI_CODEX_PROVIDER_ID)
            } else if provider_id == OPENROUTER_PROVIDER_ID {
                OPENROUTER_FALLBACK_MODEL_ID.into()
            } else if provider_id == DEEPSEEK_PROVIDER_ID {
                DEEPSEEK_FALLBACK_MODEL_ID.into()
            } else if provider_id == XAI_PROVIDER_ID {
                XAI_DEFAULT_MODEL_ID.into()
            } else if provider_id == CURSOR_PROVIDER_ID {
                CURSOR_DEFAULT_MODEL_ID.into()
            } else {
                provider_id.to_owned()
            }
        });

    let api_key_entry = match (record.kind, record.api_key.as_ref()) {
        (ProviderCredentialKind::ApiKey, Some(api_key)) => Some(ProviderApiKeyCredentialEntry {
            profile_id: profile_id.clone(),
            api_key: api_key.clone(),
            updated_at: record.updated_at.clone(),
        }),
        _ => None,
    };

    let profile = ProviderCredentialProfile {
        profile_id,
        provider_id: provider_id.to_owned(),
        runtime_kind: runtime_kind.to_owned(),
        label,
        model_id,
        preset_id,
        base_url: record.base_url.clone(),
        api_version: record.api_version.clone(),
        region: record.region.clone(),
        project_id: record.project_id.clone(),
        credential_link,
        updated_at: record.updated_at.clone(),
    };

    Some(SynthesizedProfile {
        profile,
        api_key_entry,
    })
}

fn synthesized_profile_metadata(
    provider_id: &str,
) -> (String, String, &'static str, Option<String>) {
    match provider_id {
        OPENAI_CODEX_PROVIDER_ID => (
            OPENAI_CODEX_DEFAULT_PROFILE_ID.into(),
            OPENAI_CODEX_DEFAULT_PROFILE_LABEL.into(),
            OPENAI_CODEX_PROVIDER_ID,
            None,
        ),
        OPENROUTER_PROVIDER_ID => (
            OPENROUTER_DEFAULT_PROFILE_ID.into(),
            OPENROUTER_DEFAULT_PROFILE_LABEL.into(),
            OPENROUTER_PROVIDER_ID,
            Some(OPENROUTER_PROVIDER_ID.into()),
        ),
        ANTHROPIC_PROVIDER_ID => (
            ANTHROPIC_DEFAULT_PROFILE_ID.into(),
            "Anthropic".into(),
            ANTHROPIC_PROVIDER_ID,
            Some(ANTHROPIC_PROVIDER_ID.into()),
        ),
        GITHUB_MODELS_PROVIDER_ID => (
            GITHUB_MODELS_DEFAULT_PROFILE_ID.into(),
            "GitHub Models".into(),
            OPENAI_COMPATIBLE_RUNTIME_KIND,
            Some(GITHUB_MODELS_PROVIDER_ID.into()),
        ),
        DEEPSEEK_PROVIDER_ID => (
            DEEPSEEK_DEFAULT_PROFILE_ID.into(),
            "DeepSeek".into(),
            DEEPSEEK_RUNTIME_KIND,
            Some(DEEPSEEK_PROVIDER_ID.into()),
        ),
        XAI_PROVIDER_ID => (
            XAI_DEFAULT_PROFILE_ID.into(),
            "xAI / Grok".into(),
            XAI_RUNTIME_KIND,
            Some(XAI_PROVIDER_ID.into()),
        ),
        CURSOR_PROVIDER_ID => (
            CURSOR_DEFAULT_PROFILE_ID.into(),
            "Cursor".into(),
            CURSOR_RUNTIME_KIND,
            Some(CURSOR_PROVIDER_ID.into()),
        ),
        OPENAI_API_PROVIDER_ID => (
            format!("{OPENAI_API_PROVIDER_ID}-default"),
            "OpenAI API".into(),
            OPENAI_COMPATIBLE_RUNTIME_KIND,
            Some(OPENAI_API_PROVIDER_ID.into()),
        ),
        OLLAMA_PROVIDER_ID => (
            format!("{OLLAMA_PROVIDER_ID}-default"),
            "Ollama".into(),
            OPENAI_COMPATIBLE_RUNTIME_KIND,
            Some(OLLAMA_PROVIDER_ID.into()),
        ),
        AZURE_OPENAI_PROVIDER_ID => (
            format!("{AZURE_OPENAI_PROVIDER_ID}-default"),
            "Azure OpenAI".into(),
            OPENAI_COMPATIBLE_RUNTIME_KIND,
            Some(AZURE_OPENAI_PROVIDER_ID.into()),
        ),
        GEMINI_AI_STUDIO_PROVIDER_ID => (
            format!("{GEMINI_AI_STUDIO_PROVIDER_ID}-default"),
            "Gemini".into(),
            GEMINI_RUNTIME_KIND,
            Some(GEMINI_AI_STUDIO_PROVIDER_ID.into()),
        ),
        BEDROCK_PROVIDER_ID => (
            format!("{BEDROCK_PROVIDER_ID}-default"),
            "Amazon Bedrock".into(),
            ANTHROPIC_RUNTIME_KIND,
            Some(BEDROCK_PROVIDER_ID.into()),
        ),
        VERTEX_PROVIDER_ID => (
            format!("{VERTEX_PROVIDER_ID}-default"),
            "Vertex AI".into(),
            ANTHROPIC_RUNTIME_KIND,
            Some(VERTEX_PROVIDER_ID.into()),
        ),
        other => (
            format!("{other}-default"),
            other.to_owned(),
            OPENAI_COMPATIBLE_RUNTIME_KIND,
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_api_key_profile_is_synthesized_as_native_provider() {
        let record = ProviderCredentialRecord {
            provider_id: XAI_PROVIDER_ID.into(),
            kind: ProviderCredentialKind::ApiKey,
            api_key: Some("xai-key".into()),
            oauth_account_id: None,
            oauth_session_id: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires_at: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-05-20T12:00:00Z".into(),
        };

        let synthesized = synthesize_profile_from_credential(&record).expect("xAI profile");

        assert_eq!(synthesized.profile.profile_id, XAI_DEFAULT_PROFILE_ID);
        assert_eq!(synthesized.profile.provider_id, XAI_PROVIDER_ID);
        assert_eq!(synthesized.profile.runtime_kind, XAI_RUNTIME_KIND);
        assert_eq!(synthesized.profile.label, "xAI / Grok");
        assert_eq!(synthesized.profile.model_id, XAI_DEFAULT_MODEL_ID);
        assert_eq!(
            synthesized.profile.preset_id.as_deref(),
            Some(XAI_PROVIDER_ID)
        );
        assert!(matches!(
            synthesized.profile.credential_link,
            Some(ProviderCredentialLink::ApiKey { .. })
        ));
        assert_eq!(
            synthesized
                .api_key_entry
                .as_ref()
                .map(|entry| entry.profile_id.as_str()),
            Some(XAI_DEFAULT_PROFILE_ID)
        );
    }

    #[test]
    fn xai_oauth_profile_keeps_account_session_link() {
        let record = ProviderCredentialRecord {
            provider_id: XAI_PROVIDER_ID.into(),
            kind: ProviderCredentialKind::OAuthSession,
            api_key: None,
            oauth_account_id: Some("acct-1".into()),
            oauth_session_id: Some("sess-1".into()),
            oauth_access_token: Some("access".into()),
            oauth_refresh_token: Some("refresh".into()),
            oauth_expires_at: Some(1_779_984_000),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-05-20T12:00:00Z".into(),
        };

        let synthesized = synthesize_profile_from_credential(&record).expect("xAI profile");

        assert!(synthesized.api_key_entry.is_none());
        assert!(matches!(
            synthesized.profile.credential_link,
            Some(ProviderCredentialLink::Xai { ref account_id, ref session_id, .. })
                if account_id == "acct-1" && session_id == "sess-1"
        ));
    }

    #[test]
    fn cursor_api_key_profile_is_synthesized_as_external_provider() {
        let record = ProviderCredentialRecord {
            provider_id: CURSOR_PROVIDER_ID.into(),
            kind: ProviderCredentialKind::ApiKey,
            api_key: Some("cursor-key".into()),
            oauth_account_id: None,
            oauth_session_id: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires_at: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-05-23T12:00:00Z".into(),
        };

        let synthesized = synthesize_profile_from_credential(&record).expect("Cursor profile");

        assert_eq!(synthesized.profile.profile_id, CURSOR_DEFAULT_PROFILE_ID);
        assert_eq!(synthesized.profile.provider_id, CURSOR_PROVIDER_ID);
        assert_eq!(synthesized.profile.runtime_kind, CURSOR_RUNTIME_KIND);
        assert_eq!(synthesized.profile.label, "Cursor");
        assert_eq!(synthesized.profile.model_id, CURSOR_DEFAULT_MODEL_ID);
        assert_eq!(
            synthesized.profile.preset_id.as_deref(),
            Some(CURSOR_PROVIDER_ID)
        );
        assert!(matches!(
            synthesized.profile.credential_link,
            Some(ProviderCredentialLink::ApiKey { .. })
        ));
        assert_eq!(
            synthesized
                .api_key_entry
                .as_ref()
                .map(|entry| entry.profile_id.as_str()),
            Some(CURSOR_DEFAULT_PROFILE_ID)
        );
    }
}
