//! Persistent Solana RPC provider profiles.
//!
//! Profiles live under the OS app-data backed Solana state root. The public
//! command DTOs deliberately redact endpoint secrets; callers select profiles
//! by id and backend command resolution uses the private stored endpoint.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;
use super::rpc_router::{default_endpoints, EndpointSpec, RpcRouter};

const STORE_FILE: &str = "provider-profiles.json";
const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SolanaProviderKind {
    SolanaPublic,
    Helius,
    QuickNode,
    Alchemy,
    Triton,
    Chainstack,
    Localnet,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretPlacement {
    None,
    QueryParameter,
    Header,
    EmbeddedUrl,
}

impl Default for SecretPlacement {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRateLimit {
    #[serde(default)]
    pub requests_per_second: Option<u32>,
    #[serde(default)]
    pub requests_per_month: Option<u64>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub cluster: ClusterKind,
    pub label: String,
    pub provider: SolanaProviderKind,
    pub rpc_url: String,
    #[serde(default)]
    pub websocket_url: Option<String>,
    #[serde(default)]
    pub secret_placement: SecretPlacement,
    #[serde(default)]
    pub secret_name: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub allow_public_fallback: bool,
    #[serde(default)]
    pub rate_limit: Option<ProviderRateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileView {
    pub id: String,
    pub cluster: ClusterKind,
    pub label: String,
    pub provider: SolanaProviderKind,
    pub rpc_url: String,
    #[serde(default)]
    pub websocket_url: Option<String>,
    pub secret_placement: SecretPlacement,
    #[serde(default)]
    pub secret_name: Option<String>,
    pub has_secret: bool,
    pub priority: u32,
    pub enabled: bool,
    pub allow_public_fallback: bool,
    #[serde(default)]
    pub rate_limit: Option<ProviderRateLimit>,
    pub managed: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileUpsert {
    pub id: String,
    pub cluster: ClusterKind,
    pub label: String,
    pub provider: SolanaProviderKind,
    pub rpc_url: String,
    #[serde(default)]
    pub websocket_url: Option<String>,
    #[serde(default)]
    pub secret_placement: SecretPlacement,
    #[serde(default)]
    pub secret_name: Option<String>,
    /// Omitted means preserve an existing secret. Empty string means remove it.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub allow_public_fallback: bool,
    #[serde(default)]
    pub rate_limit: Option<ProviderRateLimit>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInventoryEntry {
    pub provider: SolanaProviderKind,
    pub label: String,
    pub supports_query_token: bool,
    pub supports_header_token: bool,
    pub supports_websocket: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfilesResponse {
    pub profiles: Vec<ProviderProfileView>,
    pub selected_profile_ids: BTreeMap<ClusterKind, String>,
    pub inventory: Vec<ProviderInventoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileDocument {
    schema_version: u32,
    selected_profile_ids: BTreeMap<ClusterKind, String>,
    profiles: Vec<ProviderProfile>,
}

impl Default for ProviderProfileDocument {
    fn default() -> Self {
        let mut selected_profile_ids = BTreeMap::new();
        let mut profiles = Vec::new();
        for cluster in ClusterKind::ALL {
            let defaults = default_profiles(cluster);
            if let Some(profile) = defaults.first() {
                selected_profile_ids.insert(cluster, profile.id.clone());
            }
            profiles.extend(defaults);
        }
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            selected_profile_ids,
            profiles,
        }
    }
}

#[derive(Debug)]
pub struct ProviderProfileStore {
    path: PathBuf,
    doc: RwLock<ProviderProfileDocument>,
}

impl ProviderProfileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let path = root.into().join(STORE_FILE);
        let doc = load_document(&path).unwrap_or_default();
        Self {
            path,
            doc: RwLock::new(doc),
        }
    }

    pub fn list(&self) -> ProviderProfilesResponse {
        let doc = self.doc.read().expect("provider profile store poisoned");
        response_from_document(&doc)
    }

    pub fn upsert(
        &self,
        input: ProviderProfileUpsert,
        router: &RpcRouter,
    ) -> CommandResult<ProviderProfilesResponse> {
        validate_profile_input(&input)?;
        let mut doc = self.doc.write().expect("provider profile store poisoned");
        let existing_secret = doc
            .profiles
            .iter()
            .find(|profile| profile.id == input.id)
            .and_then(|profile| profile.api_key.clone());
        let next_secret = match input.api_key {
            Some(secret) if secret.trim().is_empty() => None,
            Some(secret) => Some(secret.trim().to_string()),
            None => existing_secret,
        };
        let profile = ProviderProfile {
            id: input.id,
            cluster: input.cluster,
            label: input.label.trim().to_string(),
            provider: input.provider,
            rpc_url: input.rpc_url.trim().to_string(),
            websocket_url: input
                .websocket_url
                .and_then(|url| non_empty_optional(url.trim())),
            secret_placement: input.secret_placement,
            secret_name: input
                .secret_name
                .and_then(|name| non_empty_optional(name.trim())),
            api_key: next_secret,
            priority: input.priority,
            enabled: input.enabled,
            allow_public_fallback: input.allow_public_fallback,
            rate_limit: input.rate_limit,
        };

        if let Some(existing) = doc.profiles.iter_mut().find(|entry| entry.id == profile.id) {
            *existing = profile;
        } else {
            doc.profiles.push(profile);
        }
        ensure_selection(&mut doc);
        save_document(&self.path, &doc)?;
        apply_document_to_router(&doc, router)?;
        Ok(response_from_document(&doc))
    }

    pub fn select(
        &self,
        cluster: ClusterKind,
        profile_id: String,
        router: &RpcRouter,
    ) -> CommandResult<ProviderProfilesResponse> {
        let mut doc = self.doc.write().expect("provider profile store poisoned");
        let exists = doc.profiles.iter().any(|profile| {
            profile.cluster == cluster && profile.id == profile_id && profile.enabled
        });
        if !exists {
            return Err(CommandError::user_fixable(
                "solana_provider_profile_not_found",
                "The selected Solana provider profile does not exist for this cluster.",
            ));
        }
        doc.selected_profile_ids.insert(cluster, profile_id);
        save_document(&self.path, &doc)?;
        apply_document_to_router(&doc, router)?;
        Ok(response_from_document(&doc))
    }

    pub fn delete(
        &self,
        profile_id: String,
        router: &RpcRouter,
    ) -> CommandResult<ProviderProfilesResponse> {
        let mut doc = self.doc.write().expect("provider profile store poisoned");
        let before = doc.profiles.len();
        doc.profiles
            .retain(|profile| profile.id != profile_id || is_default_profile_id(&profile.id));
        if before == doc.profiles.len() {
            return Err(CommandError::user_fixable(
                "solana_provider_profile_not_found",
                "The Solana provider profile could not be found.",
            ));
        }
        ensure_selection(&mut doc);
        save_document(&self.path, &doc)?;
        apply_document_to_router(&doc, router)?;
        Ok(response_from_document(&doc))
    }

    pub fn apply_to_router(&self, router: &RpcRouter) -> CommandResult<()> {
        let doc = self.doc.read().expect("provider profile store poisoned");
        apply_document_to_router(&doc, router)
    }

    pub fn resolve_rpc_url(&self, cluster: ClusterKind) -> Option<String> {
        let doc = self.doc.read().expect("provider profile store poisoned");
        selected_profile(&doc, cluster).and_then(|profile| resolve_profile_url(&profile))
    }
}

fn load_document(path: &PathBuf) -> Option<ProviderProfileDocument> {
    let bytes = fs::read(path).ok()?;
    let mut doc: ProviderProfileDocument = serde_json::from_slice(&bytes).ok()?;
    if doc.schema_version != CURRENT_SCHEMA_VERSION {
        return None;
    }
    ensure_defaults(&mut doc);
    ensure_selection(&mut doc);
    Some(doc)
}

fn save_document(path: &PathBuf, doc: &ProviderProfileDocument) -> CommandResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CommandError::system_fault(
                "solana_provider_profiles_mkdir_failed",
                format!("Could not create Solana provider profile directory: {err}"),
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(doc).map_err(|err| {
        CommandError::system_fault(
            "solana_provider_profiles_encode_failed",
            format!("Could not encode Solana provider profiles: {err}"),
        )
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|err| {
        CommandError::system_fault(
            "solana_provider_profiles_write_failed",
            format!("Could not write Solana provider profiles: {err}"),
        )
    })?;
    harden_file_permissions(&tmp);
    fs::rename(&tmp, path).map_err(|err| {
        CommandError::system_fault(
            "solana_provider_profiles_commit_failed",
            format!("Could not save Solana provider profiles: {err}"),
        )
    })?;
    harden_file_permissions(path);
    Ok(())
}

#[cfg(unix)]
fn harden_file_permissions(path: &PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn harden_file_permissions(_path: &PathBuf) {}

fn validate_profile_input(input: &ProviderProfileUpsert) -> CommandResult<()> {
    if input.id.trim().is_empty() {
        return Err(CommandError::invalid_request("id"));
    }
    if is_default_profile_id(input.id.trim()) {
        return Err(CommandError::user_fixable(
            "solana_provider_profile_managed",
            "Built-in Solana provider profiles cannot be overwritten.",
        ));
    }
    if input.label.trim().is_empty() {
        return Err(CommandError::invalid_request("label"));
    }
    validate_http_url(&input.rpc_url, "rpcUrl")?;
    if let Some(ws_url) = input
        .websocket_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        validate_ws_url(ws_url, "websocketUrl")?;
    }
    if matches!(
        input.secret_placement,
        SecretPlacement::Header | SecretPlacement::QueryParameter
    ) && input.secret_name.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(CommandError::user_fixable(
            "solana_provider_profile_secret_name_missing",
            "A query parameter or header name is required for this API-key placement.",
        ));
    }
    Ok(())
}

fn validate_http_url(value: &str, field: &'static str) -> CommandResult<()> {
    let parsed = url::Url::parse(value.trim()).map_err(|_| {
        CommandError::user_fixable(
            "solana_provider_profile_bad_url",
            format!("Field `{field}` must be an absolute HTTP(S) URL."),
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CommandError::user_fixable(
            "solana_provider_profile_bad_url",
            format!("Field `{field}` must use http or https."),
        ));
    }
    Ok(())
}

fn validate_ws_url(value: &str, field: &'static str) -> CommandResult<()> {
    let parsed = url::Url::parse(value.trim()).map_err(|_| {
        CommandError::user_fixable(
            "solana_provider_profile_bad_url",
            format!("Field `{field}` must be an absolute WS(S) URL."),
        )
    })?;
    if !matches!(parsed.scheme(), "ws" | "wss") {
        return Err(CommandError::user_fixable(
            "solana_provider_profile_bad_url",
            format!("Field `{field}` must use ws or wss."),
        ));
    }
    Ok(())
}

fn apply_document_to_router(
    doc: &ProviderProfileDocument,
    router: &RpcRouter,
) -> CommandResult<()> {
    for cluster in ClusterKind::ALL {
        let mut profiles = profiles_for_cluster(doc, cluster);
        let selected = selected_profile(doc, cluster);
        if let Some(profile) = selected
            .as_ref()
            .filter(|profile| is_default_profile_id(&profile.id))
        {
            profiles.push(profile.clone());
        }
        let include_defaults = selected
            .map(|profile| profile.allow_public_fallback)
            .unwrap_or(true);
        profiles.sort_by(|a, b| {
            let selected_a = doc.selected_profile_ids.get(&cluster) == Some(&a.id);
            let selected_b = doc.selected_profile_ids.get(&cluster) == Some(&b.id);
            selected_b
                .cmp(&selected_a)
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| a.label.cmp(&b.label))
        });

        let mut endpoints: Vec<EndpointSpec> = profiles
            .into_iter()
            .filter_map(|profile| profile_to_endpoint(&profile))
            .collect();
        if include_defaults {
            let existing: HashMap<String, ()> = endpoints
                .iter()
                .map(|endpoint| (endpoint.url.clone(), ()))
                .collect();
            endpoints.extend(
                default_endpoints(cluster)
                    .into_iter()
                    .filter(|endpoint| !existing.contains_key(&endpoint.url)),
            );
        }
        router.set_endpoints(cluster, endpoints)?;
    }
    Ok(())
}

fn response_from_document(doc: &ProviderProfileDocument) -> ProviderProfilesResponse {
    let views = doc
        .profiles
        .iter()
        .map(|profile| {
            let selected = doc.selected_profile_ids.get(&profile.cluster) == Some(&profile.id);
            ProviderProfileView {
                id: profile.id.clone(),
                cluster: profile.cluster,
                label: profile.label.clone(),
                provider: profile.provider.clone(),
                rpc_url: redact_url(&profile.rpc_url),
                websocket_url: profile.websocket_url.as_deref().map(redact_url),
                secret_placement: profile.secret_placement.clone(),
                secret_name: profile.secret_name.clone(),
                has_secret: profile
                    .api_key
                    .as_deref()
                    .is_some_and(|key| !key.is_empty())
                    || matches!(profile.secret_placement, SecretPlacement::EmbeddedUrl),
                priority: profile.priority,
                enabled: profile.enabled,
                allow_public_fallback: profile.allow_public_fallback,
                rate_limit: profile.rate_limit.clone(),
                managed: is_default_profile_id(&profile.id),
                selected,
            }
        })
        .collect();
    ProviderProfilesResponse {
        profiles: views,
        selected_profile_ids: doc.selected_profile_ids.clone(),
        inventory: provider_inventory(),
    }
}

fn ensure_defaults(doc: &mut ProviderProfileDocument) {
    let mut ids: HashMap<String, ()> = doc.profiles.iter().map(|p| (p.id.clone(), ())).collect();
    for cluster in ClusterKind::ALL {
        for profile in default_profiles(cluster) {
            if !ids.contains_key(&profile.id) {
                ids.insert(profile.id.clone(), ());
                doc.profiles.push(profile);
            }
        }
    }
}

fn ensure_selection(doc: &mut ProviderProfileDocument) {
    ensure_defaults(doc);
    for cluster in ClusterKind::ALL {
        let selected_id = doc.selected_profile_ids.get(&cluster).cloned();
        let selected_valid = selected_id.as_ref().is_some_and(|id| {
            doc.profiles
                .iter()
                .any(|profile| profile.cluster == cluster && profile.id == *id && profile.enabled)
        });
        if !selected_valid {
            if let Some(profile) = doc
                .profiles
                .iter()
                .find(|profile| profile.cluster == cluster && profile.enabled)
            {
                doc.selected_profile_ids.insert(cluster, profile.id.clone());
            }
        }
    }
}

fn selected_profile(
    doc: &ProviderProfileDocument,
    cluster: ClusterKind,
) -> Option<ProviderProfile> {
    let selected_id = doc.selected_profile_ids.get(&cluster)?;
    doc.profiles
        .iter()
        .find(|profile| profile.cluster == cluster && profile.id == *selected_id && profile.enabled)
        .cloned()
}

fn profiles_for_cluster(
    doc: &ProviderProfileDocument,
    cluster: ClusterKind,
) -> Vec<ProviderProfile> {
    doc.profiles
        .iter()
        .filter(|profile| {
            profile.cluster == cluster && profile.enabled && !is_default_profile_id(&profile.id)
        })
        .cloned()
        .collect()
}

fn profile_to_endpoint(profile: &ProviderProfile) -> Option<EndpointSpec> {
    Some(EndpointSpec {
        id: profile.id.clone(),
        url: resolve_profile_url(profile)?,
        ws_url: resolve_optional_profile_url(profile.websocket_url.as_deref(), profile),
        label: Some(profile.label.clone()),
        requires_api_key: profile
            .api_key
            .as_deref()
            .is_some_and(|key| !key.is_empty())
            || matches!(profile.secret_placement, SecretPlacement::EmbeddedUrl),
    })
}

fn resolve_profile_url(profile: &ProviderProfile) -> Option<String> {
    apply_secret(&profile.rpc_url, profile)
}

fn resolve_optional_profile_url(value: Option<&str>, profile: &ProviderProfile) -> Option<String> {
    value.and_then(|url| apply_secret(url, profile))
}

fn apply_secret(value: &str, profile: &ProviderProfile) -> Option<String> {
    match profile.secret_placement {
        SecretPlacement::None | SecretPlacement::Header | SecretPlacement::EmbeddedUrl => {
            Some(value.to_string())
        }
        SecretPlacement::QueryParameter => {
            let secret = profile.api_key.as_deref()?.trim();
            let name = profile.secret_name.as_deref()?.trim();
            if secret.is_empty() || name.is_empty() {
                return Some(value.to_string());
            }
            let mut parsed = url::Url::parse(value).ok()?;
            parsed.query_pairs_mut().append_pair(name, secret);
            Some(parsed.to_string())
        }
    }
}

pub fn redact_url(value: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(value) else {
        return redact_loose(value);
    };
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("redacted"));
    }
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("redacted");
    }
    if parsed.query().is_some() {
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(key, value)| {
                let redacted = if is_secret_key(&key) || value.len() > 12 {
                    "redacted".to_string()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), redacted)
            })
            .collect();
        parsed.set_query(None);
        {
            let mut query = parsed.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }
    }
    parsed.to_string()
}

fn redact_loose(value: &str) -> String {
    let mut out = value.to_string();
    for key in ["api-key", "api_key", "apikey", "key", "token"] {
        out = out.replace(&format!("{key}="), &format!("{key}=redacted"));
    }
    out
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("key") || key.contains("token") || key.contains("secret")
}

fn default_profiles(cluster: ClusterKind) -> Vec<ProviderProfile> {
    default_endpoints(cluster)
        .into_iter()
        .enumerate()
        .map(|(index, endpoint)| ProviderProfile {
            id: format!("builtin-{}", endpoint.id),
            cluster,
            label: endpoint.label.unwrap_or(endpoint.id),
            provider: classify_default(&endpoint.url),
            rpc_url: endpoint.url,
            websocket_url: endpoint.ws_url,
            secret_placement: SecretPlacement::None,
            secret_name: None,
            api_key: None,
            priority: index as u32,
            enabled: true,
            allow_public_fallback: true,
            rate_limit: None,
        })
        .collect()
}

fn classify_default(url: &str) -> SolanaProviderKind {
    let lower = url.to_ascii_lowercase();
    if lower.contains("127.0.0.1") || lower.contains("localhost") {
        SolanaProviderKind::Localnet
    } else if lower.contains("helius") {
        SolanaProviderKind::Helius
    } else if lower.contains("triton") || lower.contains("extrnode") {
        SolanaProviderKind::Triton
    } else {
        SolanaProviderKind::SolanaPublic
    }
}

fn provider_inventory() -> Vec<ProviderInventoryEntry> {
    vec![
        ProviderInventoryEntry {
            provider: SolanaProviderKind::SolanaPublic,
            label: "Solana public RPC".into(),
            supports_query_token: false,
            supports_header_token: false,
            supports_websocket: true,
            notes: "Bundled public fallback, suitable for low-volume read paths.".into(),
        },
        ProviderInventoryEntry {
            provider: SolanaProviderKind::Helius,
            label: "Helius".into(),
            supports_query_token: true,
            supports_header_token: false,
            supports_websocket: true,
            notes: "Common Solana paid/free provider; API keys are usually query parameters."
                .into(),
        },
        ProviderInventoryEntry {
            provider: SolanaProviderKind::QuickNode,
            label: "QuickNode".into(),
            supports_query_token: false,
            supports_header_token: false,
            supports_websocket: true,
            notes: "Endpoint URLs commonly embed the token in the subdomain/path.".into(),
        },
        ProviderInventoryEntry {
            provider: SolanaProviderKind::Alchemy,
            label: "Alchemy".into(),
            supports_query_token: true,
            supports_header_token: false,
            supports_websocket: true,
            notes: "Supports app-specific endpoint URLs and query-style keys.".into(),
        },
        ProviderInventoryEntry {
            provider: SolanaProviderKind::Chainstack,
            label: "Chainstack".into(),
            supports_query_token: false,
            supports_header_token: false,
            supports_websocket: true,
            notes: "Dedicated endpoint URLs usually carry the access token.".into(),
        },
        ProviderInventoryEntry {
            provider: SolanaProviderKind::Custom,
            label: "Custom".into(),
            supports_query_token: true,
            supports_header_token: true,
            supports_websocket: true,
            notes: "For local gateways or enterprise RPC providers.".into(),
        },
    ]
}

fn is_default_profile_id(id: &str) -> bool {
    id.starts_with("builtin-")
}

fn non_empty_optional(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn list_redacts_query_secrets() {
        let dir = TempDir::new().unwrap();
        let store = ProviderProfileStore::new(dir.path());
        let router = RpcRouter::new_with_default_pool();
        let response = store
            .upsert(
                ProviderProfileUpsert {
                    id: "helius-devnet".into(),
                    cluster: ClusterKind::Devnet,
                    label: "Helius devnet".into(),
                    provider: SolanaProviderKind::Helius,
                    rpc_url: "https://devnet.helius-rpc.com/?api-key=abc123abc123abc123".into(),
                    websocket_url: None,
                    secret_placement: SecretPlacement::EmbeddedUrl,
                    secret_name: None,
                    api_key: None,
                    priority: 0,
                    enabled: true,
                    allow_public_fallback: true,
                    rate_limit: None,
                },
                &router,
            )
            .unwrap();
        let profile = response
            .profiles
            .iter()
            .find(|profile| profile.id == "helius-devnet")
            .unwrap();
        assert!(profile.rpc_url.contains("api-key=redacted"));
        assert!(!profile.rpc_url.contains("abc123"));
    }

    #[test]
    fn query_parameter_secret_is_applied_only_inside_router() {
        let dir = TempDir::new().unwrap();
        let store = ProviderProfileStore::new(dir.path());
        let router = RpcRouter::new_with_default_pool();
        store
            .upsert(
                ProviderProfileUpsert {
                    id: "paid-devnet".into(),
                    cluster: ClusterKind::Devnet,
                    label: "Paid devnet".into(),
                    provider: SolanaProviderKind::Custom,
                    rpc_url: "https://rpc.example.test".into(),
                    websocket_url: None,
                    secret_placement: SecretPlacement::QueryParameter,
                    secret_name: Some("api-key".into()),
                    api_key: Some("secret-token".into()),
                    priority: 0,
                    enabled: true,
                    allow_public_fallback: false,
                    rate_limit: None,
                },
                &router,
            )
            .unwrap();
        store
            .select(ClusterKind::Devnet, "paid-devnet".into(), &router)
            .unwrap();
        let endpoint = router.pick_healthy(ClusterKind::Devnet).unwrap();
        assert_eq!(
            endpoint.url,
            "https://rpc.example.test/?api-key=secret-token"
        );
        let view = store.list();
        let profile = view
            .profiles
            .iter()
            .find(|profile| profile.id == "paid-devnet")
            .unwrap();
        assert_eq!(profile.rpc_url, "https://rpc.example.test/");
    }

    #[test]
    fn selecting_builtin_profile_promotes_that_endpoint() {
        let dir = TempDir::new().unwrap();
        let store = ProviderProfileStore::new(dir.path());
        let router = RpcRouter::new_with_default_pool();

        store
            .select(
                ClusterKind::Mainnet,
                "builtin-mainnet-helius-free".into(),
                &router,
            )
            .unwrap();

        let endpoints = router.endpoints_for(ClusterKind::Mainnet);
        assert_eq!(
            endpoints.first().unwrap().url,
            "https://mainnet.helius-rpc.com"
        );
        assert_eq!(
            endpoints
                .iter()
                .filter(|endpoint| endpoint.url == "https://mainnet.helius-rpc.com")
                .count(),
            1
        );
    }
}
