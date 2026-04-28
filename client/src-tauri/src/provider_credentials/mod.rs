//! Flat per-provider credential store. Replaces the legacy
//! `provider_profiles` / `provider_profile_credentials` /
//! `openai_codex_sessions` triplet with a single row per provider.

pub mod readiness;
pub mod sql;
pub mod view;

pub use readiness::{readiness_proof, ProviderCredentialReadinessProof};
pub use sql::{
    delete_provider_credential, load_all_provider_credentials, load_provider_credential,
    upsert_provider_credential,
};
pub use view::{
    load_provider_credentials_view_or_default, ProviderApiKeyCredentialEntry,
    ProviderCredentialLink, ProviderCredentialProfile, ProviderCredentialReadinessProjection,
    ProviderCredentialReadinessStatus, ProviderCredentialsView, OPENAI_CODEX_DEFAULT_PROFILE_ID,
};

use serde::{Deserialize, Serialize};

/// How a credential row authenticates with the upstream provider. Mirrors the
/// `provider_credentials.kind` column's CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialKind {
    ApiKey,
    OAuthSession,
    Local,
    Ambient,
}

impl ProviderCredentialKind {
    pub fn as_sql_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OAuthSession => "oauth_session",
            Self::Local => "local",
            Self::Ambient => "ambient",
        }
    }

    pub fn from_sql_str(value: &str) -> Option<Self> {
        match value {
            "api_key" => Some(Self::ApiKey),
            "oauth_session" => Some(Self::OAuthSession),
            "local" => Some(Self::Local),
            "ambient" => Some(Self::Ambient),
            _ => None,
        }
    }
}

/// A single provider's credential row. Field availability is constrained by
/// `kind`:
/// - `ApiKey` populates `api_key`.
/// - `OAuthSession` populates `oauth_account_id`, `oauth_session_id`, and
///   typically the token columns.
/// - `Local` / `Ambient` populate neither — readiness is implied.
///
/// `default_model_id` is a UX hint: the last model the user picked for this
/// provider, used to seed the composer's dropdown on app open. There is no
/// "active provider" anywhere in this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialRecord {
    pub provider_id: String,
    pub kind: ProviderCredentialKind,
    pub api_key: Option<String>,
    pub oauth_account_id: Option<String>,
    pub oauth_session_id: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_expires_at: Option<i64>,
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub default_model_id: Option<String>,
    pub updated_at: String,
}

/// Snapshot of every credentialed provider. The vector is the source of truth
/// — there is no metadata wrapper because there is no metadata. Order is by
/// `provider_id` after a load.
pub type ProviderCredentialsSnapshot = Vec<ProviderCredentialRecord>;

/// `is_ready` is trivially true for any row that exists in
/// `provider_credentials`: the migration / upsert layer enforces the kind/field
/// invariants via the SQL CHECK constraint, so a loaded record is by definition
/// ready. Callers that want to know *why* it's ready should use
/// [`readiness_proof`] instead.
pub fn is_ready(_record: &ProviderCredentialRecord) -> bool {
    true
}
