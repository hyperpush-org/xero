pub mod migration;
pub mod store;

pub use migration::load_or_migrate_provider_profiles_from_paths;
pub use store::{
    default_provider_profiles_snapshot, load_provider_profiles_from_paths,
    AnthropicProfileCredentialEntry, OpenRouterProfileCredentialEntry,
    ProviderApiKeyCredentialEntry, ProviderProfileCredentialLink, ProviderProfileCredentialsFile,
    ProviderProfileReadinessProof, ProviderProfileReadinessProjection,
    ProviderProfileReadinessStatus, ProviderProfileRecord,
    ProviderProfilesMetadataFile, ProviderProfilesMigrationState, ProviderProfilesSnapshot,
    ANTHROPIC_DEFAULT_PROFILE_ID, GITHUB_MODELS_DEFAULT_PROFILE_ID,
    OPENAI_CODEX_DEFAULT_PROFILE_ID, OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
    PROVIDER_PROFILES_FILE_NAME, PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME,
};

pub(crate) use store::{
    build_anthropic_default_profile, build_openai_default_profile,
    build_openrouter_default_profile, persist_provider_profiles_snapshot,
};
