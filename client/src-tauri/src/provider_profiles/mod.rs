pub mod sql;
pub mod store;

use rusqlite::Connection;

use crate::commands::CommandResult;
use crate::provider_credentials::load_all_provider_credentials;

/// Loads the provider-profiles snapshot. The legacy SQL tables are no longer
/// the source of truth — synthesize the snapshot from the flat
/// `provider_credentials` table so the seven legacy consumers (auth/store,
/// provider_models, runtime/provider, runtime/diagnostics, doctor_report,
/// provider_diagnostics, get_runtime_settings) keep compiling against the
/// snapshot shape they expect.
pub fn load_provider_profiles_or_default(
    connection: &Connection,
) -> CommandResult<ProviderProfilesSnapshot> {
    let records = load_all_provider_credentials(connection)?;
    Ok(store::synthesize_provider_profiles_snapshot_from_credentials(
        &records,
    ))
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

