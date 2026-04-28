use super::{ProviderCredentialKind, ProviderCredentialRecord};

/// Why a credential row is considered ready. Under the flat credential schema,
/// a row either exists with a valid kind-specific shape or it doesn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialReadinessProof {
    OAuthSession,
    StoredSecret,
    Local,
    Ambient,
}

pub fn readiness_proof(record: &ProviderCredentialRecord) -> ProviderCredentialReadinessProof {
    match record.kind {
        ProviderCredentialKind::ApiKey => ProviderCredentialReadinessProof::StoredSecret,
        ProviderCredentialKind::OAuthSession => ProviderCredentialReadinessProof::OAuthSession,
        ProviderCredentialKind::Local => ProviderCredentialReadinessProof::Local,
        ProviderCredentialKind::Ambient => ProviderCredentialReadinessProof::Ambient,
    }
}
