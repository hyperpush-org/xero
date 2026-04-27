pub mod importer;
pub mod sql;
pub mod store;

use rusqlite::Connection;

use crate::commands::CommandResult;

pub use importer::import_legacy_provider_profiles;
pub use sql::{load_provider_profiles_from_db, persist_provider_profiles_to_db};

/// Loads the provider-profiles snapshot from the global database, falling back to the default
/// snapshot when no metadata row has been persisted yet.
pub fn load_provider_profiles_or_default(
    connection: &Connection,
) -> CommandResult<ProviderProfilesSnapshot> {
    Ok(load_provider_profiles_from_db(connection)?
        .unwrap_or_else(default_provider_profiles_snapshot))
}
pub use store::{
    default_provider_profiles_snapshot, AnthropicProfileCredentialEntry,
    OpenRouterProfileCredentialEntry, ProviderApiKeyCredentialEntry, ProviderProfileCredentialLink,
    ProviderProfileCredentialsFile, ProviderProfileReadinessProjection,
    ProviderProfileReadinessProof, ProviderProfileReadinessStatus, ProviderProfileRecord,
    ProviderProfilesMetadataFile, ProviderProfilesMigrationState, ProviderProfilesSnapshot,
    ANTHROPIC_DEFAULT_PROFILE_ID, GITHUB_MODELS_DEFAULT_PROFILE_ID,
    OPENAI_CODEX_DEFAULT_PROFILE_ID, OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
};

pub(crate) use store::{
    build_anthropic_default_profile, build_openai_default_profile, build_openrouter_default_profile,
};
