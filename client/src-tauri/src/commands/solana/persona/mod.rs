//! Persona store + CRUD + funding orchestration.
//!
//! A `Persona` is a named keypair on a specific cluster with funding
//! metadata attached. The store owns a per-cluster index of personas and
//! delegates key material + file IO to `KeypairStore`, and SOL / SPL /
//! Metaplex work to `FundingBackend`.
//!
//! Mainnet is intentionally off-limits for create/import operations: the
//! workbench default is localnet + mainnet-fork personas. The `policy_ok`
//! function below is the single enforcement point — every Tauri command
//! that accepts a persona-bound operation routes through it.

pub mod fund;
pub mod keygen;
pub mod roles;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::cluster::ClusterKind;
use fund::{apply_delta, FundingContext};
use keygen::{KeypairStore, OsRngKeypairProvider};
use roles::PersonaRole;

pub use fund::{FundingBackend, FundingDelta, FundingReceipt};
pub use keygen::KeypairBytes;

const PERSONA_REGISTRY_VERSION: u32 = 1;
const PERSONA_REGISTRY_FILE: &str = "personas.json";

/// A persona record as surfaced to the frontend / agent. Keypair bytes
/// never cross this boundary — callers get an opaque file path instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Persona {
    pub name: String,
    pub role: PersonaRole,
    pub cluster: ClusterKind,
    pub pubkey: String,
    pub keypair_path: String,
    pub created_at_ms: u64,
    pub seed: FundingDelta,
    /// Free-form note shown in the UI; optional metadata the agent can set.
    #[serde(default)]
    pub note: Option<String>,
}

/// What the caller provides when creating a persona. The `name` is the
/// human-readable handle; every other field is optional.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaSpec {
    pub name: String,
    pub cluster: ClusterKind,
    #[serde(default = "default_role")]
    pub role: PersonaRole,
    /// If Some, overrides any per-role preset values. Missing fields inherit
    /// from the role preset. The final value is what we record and fund.
    #[serde(default)]
    pub seed_override: Option<FundingDelta>,
    #[serde(default)]
    pub note: Option<String>,
}

fn default_role() -> PersonaRole {
    PersonaRole::Custom
}

impl PersonaSpec {
    pub fn effective_seed(&self) -> FundingDelta {
        let preset = self.role.preset();
        let mut seed = FundingDelta {
            sol_lamports: preset.lamports,
            tokens: preset.tokens.clone(),
            nfts: preset.nfts.clone(),
        };
        if let Some(override_delta) = &self.seed_override {
            if override_delta.sol_lamports > 0 {
                seed.sol_lamports = override_delta.sol_lamports;
            }
            if !override_delta.tokens.is_empty() {
                seed.tokens = override_delta.tokens.clone();
            }
            if !override_delta.nfts.is_empty() {
                seed.nfts = override_delta.nfts.clone();
            }
        }
        seed
    }
}

/// Policy gate: which operations are allowed on which cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonaOp {
    Create,
    Fund,
    ImportKeypair,
    ExportKeypair,
    Delete,
}

pub fn policy_ok(op: PersonaOp, cluster: ClusterKind) -> CommandResult<()> {
    match (op, cluster) {
        // Mainnet is off-limits for every mutation path. The workbench is
        // intentionally read-only for mainnet so a stray agent run can't
        // touch production authority keypairs.
        (
            PersonaOp::Create
            | PersonaOp::Fund
            | PersonaOp::ImportKeypair
            | PersonaOp::ExportKeypair
            | PersonaOp::Delete,
            ClusterKind::Mainnet,
        ) => Err(CommandError::policy_denied(
            "Persona operations against mainnet are disabled by default. Use localnet, \
             mainnet-fork, or devnet.",
        )),
        _ => Ok(()),
    }
}

/// On-disk persistence format. Per-cluster file so a corruption in one
/// cluster's registry can't take down others.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    version: u32,
    personas: BTreeMap<String, Persona>,
}

#[derive(Debug)]
pub struct PersonaStore {
    root: PathBuf,
    keypairs: KeypairStore,
    funding: Mutex<Box<dyn FundingBackend>>,
    registries: RwLock<BTreeMap<ClusterKind, RegistryFile>>,
}

impl PersonaStore {
    pub fn new(
        root: impl Into<PathBuf>,
        keypairs: KeypairStore,
        funding: Box<dyn FundingBackend>,
    ) -> Self {
        let root = root.into();
        let mut registries = BTreeMap::new();
        for cluster in ClusterKind::ALL {
            let file = load_registry(&registry_path(&root, cluster))
                .unwrap_or_else(|_| RegistryFile::default());
            registries.insert(cluster, file);
        }
        Self {
            root,
            keypairs,
            funding: Mutex::new(funding),
            registries: RwLock::new(registries),
        }
    }

    pub fn with_default_root() -> CommandResult<Self> {
        let root = default_root()?;
        let keypair_root = root.join("keypairs");
        let keypairs = KeypairStore::new(keypair_root, Box::new(OsRngKeypairProvider));
        let funding = Box::new(fund::DefaultFundingBackend::new());
        Ok(Self::new(root, keypairs, funding))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self, cluster: ClusterKind) -> CommandResult<Vec<Persona>> {
        let registries = self.lock_read()?;
        let file = registries.get(&cluster).cloned().unwrap_or_default();
        let mut personas: Vec<Persona> = file.personas.into_values().collect();
        personas.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(personas)
    }

    pub fn get(&self, cluster: ClusterKind, name: &str) -> CommandResult<Option<Persona>> {
        let registries = self.lock_read()?;
        Ok(registries
            .get(&cluster)
            .and_then(|r| r.personas.get(name).cloned()))
    }

    pub fn create(
        &self,
        spec: PersonaSpec,
        rpc_url: Option<String>,
    ) -> CommandResult<(Persona, FundingReceipt)> {
        policy_ok(PersonaOp::Create, spec.cluster)?;
        validate_name(&spec.name)?;

        if self.get(spec.cluster, &spec.name)?.is_some() {
            return Err(CommandError::user_fixable(
                "solana_persona_already_exists",
                format!(
                    "Persona '{}' already exists on cluster {}.",
                    spec.name,
                    spec.cluster.as_str()
                ),
            ));
        }

        let (bytes, path) = self.keypairs.generate(spec.cluster.as_str(), &spec.name)?;
        let pubkey = bytes.pubkey_base58();
        let seed = spec.effective_seed();

        let persona = Persona {
            name: spec.name.clone(),
            role: spec.role,
            cluster: spec.cluster,
            pubkey: pubkey.clone(),
            keypair_path: path.display().to_string(),
            created_at_ms: now_ms(),
            seed: seed.clone(),
            note: spec.note.clone(),
        };

        self.insert_persona(persona.clone())?;

        let receipt = if let Some(url) = rpc_url.filter(|u| !u.is_empty()) {
            self.run_funding(&persona, &url, &seed)?
        } else {
            // No cluster running / no URL — the agent explicitly created the
            // persona without funding. Produce an empty receipt so the UI
            // can still render a success toast.
            FundingReceipt::new(&persona.name, spec.cluster.as_str())
        };

        Ok((persona, receipt))
    }

    pub fn fund(
        &self,
        cluster: ClusterKind,
        name: &str,
        delta: &FundingDelta,
        rpc_url: &str,
    ) -> CommandResult<FundingReceipt> {
        policy_ok(PersonaOp::Fund, cluster)?;
        let persona = self.get(cluster, name)?.ok_or_else(|| {
            CommandError::user_fixable(
                "solana_persona_not_found",
                format!(
                    "Persona '{}' does not exist on cluster {}.",
                    name,
                    cluster.as_str()
                ),
            )
        })?;
        self.run_funding(&persona, rpc_url, delta)
    }

    pub fn delete(&self, cluster: ClusterKind, name: &str) -> CommandResult<()> {
        policy_ok(PersonaOp::Delete, cluster)?;
        let mut registries = self.lock_write()?;
        let file = registries.entry(cluster).or_default();
        file.personas.remove(name);
        persist_registry(&self.root, cluster, file)?;
        drop(registries);
        self.keypairs.delete(cluster.as_str(), name)?;
        Ok(())
    }

    pub fn import_keypair(
        &self,
        cluster: ClusterKind,
        name: &str,
        role: PersonaRole,
        keypair_bytes: KeypairBytes,
        note: Option<String>,
    ) -> CommandResult<Persona> {
        policy_ok(PersonaOp::ImportKeypair, cluster)?;
        // Import policy is even stricter than create: localnet and
        // mainnet-fork only. Devnet keypair imports are blocked because the
        // user likely has one mainnet-authority keypair they accidentally
        // reuse across clusters (common footgun the scope-check catches).
        if !matches!(cluster, ClusterKind::Localnet | ClusterKind::MainnetFork) {
            return Err(CommandError::policy_denied(
                "Keypair import is restricted to local clusters (localnet, mainnet-fork). \
                 Generate a fresh keypair on devnet or mainnet-fork instead.",
            ));
        }
        validate_name(name)?;

        if self.get(cluster, name)?.is_some() {
            return Err(CommandError::user_fixable(
                "solana_persona_already_exists",
                format!(
                    "Persona '{name}' already exists on cluster {}.",
                    cluster.as_str()
                ),
            ));
        }

        let pubkey = keypair_bytes.pubkey_base58();
        let path = self
            .keypairs
            .import_bytes(cluster.as_str(), name, keypair_bytes)?;

        let persona = Persona {
            name: name.to_string(),
            role,
            cluster,
            pubkey,
            keypair_path: path.display().to_string(),
            created_at_ms: now_ms(),
            seed: FundingDelta::default(),
            note,
        };
        self.insert_persona(persona.clone())?;
        Ok(persona)
    }

    pub fn export_keypair_path(&self, cluster: ClusterKind, name: &str) -> CommandResult<PathBuf> {
        policy_ok(PersonaOp::ExportKeypair, cluster)?;
        // Belt-and-braces: only local clusters ever expose the keypair path
        // back to callers. Devnet/Mainnet personas cannot be exported even
        // if a future refactor relaxes the top-level policy.
        if !matches!(cluster, ClusterKind::Localnet | ClusterKind::MainnetFork) {
            return Err(CommandError::policy_denied(
                "Keypair export is restricted to local clusters.",
            ));
        }
        let _ = self.get(cluster, name)?.ok_or_else(|| {
            CommandError::user_fixable(
                "solana_persona_not_found",
                format!(
                    "Persona '{name}' does not exist on cluster {}.",
                    cluster.as_str()
                ),
            )
        })?;
        Ok(self.keypairs.path_for(cluster.as_str(), name))
    }

    pub fn keypair_path(&self, cluster: ClusterKind, name: &str) -> CommandResult<PathBuf> {
        // Internal accessor — does NOT go through the export policy gate
        // because we're building a FundingContext for a command this same
        // store originated. Still constrained to personas that exist.
        let _ = self.get(cluster, name)?.ok_or_else(|| {
            CommandError::user_fixable(
                "solana_persona_not_found",
                format!(
                    "Persona '{name}' does not exist on cluster {}.",
                    cluster.as_str()
                ),
            )
        })?;
        Ok(self.keypairs.path_for(cluster.as_str(), name))
    }

    fn run_funding(
        &self,
        persona: &Persona,
        rpc_url: &str,
        delta: &FundingDelta,
    ) -> CommandResult<FundingReceipt> {
        let ctx = FundingContext {
            persona_name: persona.name.clone(),
            cluster: persona.cluster.as_str().to_string(),
            rpc_url: rpc_url.to_string(),
            recipient_pubkey: persona.pubkey.clone(),
            keypair_path: PathBuf::from(&persona.keypair_path),
        };
        let backend = self.funding.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_persona_funding_poisoned",
                "Funding backend lock poisoned.",
            )
        })?;
        apply_delta(backend.as_ref(), &ctx, delta)
    }

    fn insert_persona(&self, persona: Persona) -> CommandResult<()> {
        let mut registries = self.lock_write()?;
        let file = registries.entry(persona.cluster).or_default();
        if file.version == 0 {
            file.version = PERSONA_REGISTRY_VERSION;
        }
        file.personas.insert(persona.name.clone(), persona.clone());
        persist_registry(&self.root, persona.cluster, file)
    }

    fn lock_read(
        &self,
    ) -> CommandResult<std::sync::RwLockReadGuard<'_, BTreeMap<ClusterKind, RegistryFile>>> {
        self.registries.read().map_err(|_| {
            CommandError::system_fault(
                "solana_persona_registry_poisoned",
                "Persona registry lock poisoned.",
            )
        })
    }

    fn lock_write(
        &self,
    ) -> CommandResult<std::sync::RwLockWriteGuard<'_, BTreeMap<ClusterKind, RegistryFile>>> {
        self.registries.write().map_err(|_| {
            CommandError::system_fault(
                "solana_persona_registry_poisoned",
                "Persona registry lock poisoned.",
            )
        })
    }
}

fn registry_path(root: &Path, cluster: ClusterKind) -> PathBuf {
    root.join("registry")
        .join(format!("{}-{}", cluster.as_str(), PERSONA_REGISTRY_FILE))
}

fn load_registry(path: &Path) -> CommandResult<RegistryFile> {
    if !path.exists() {
        return Ok(RegistryFile::default());
    }
    let bytes = fs::read(path).map_err(|err| {
        CommandError::system_fault(
            "solana_persona_registry_read_failed",
            format!("Could not read {}: {err}", path.display()),
        )
    })?;
    let parsed: RegistryFile = serde_json::from_slice(&bytes).map_err(|err| {
        CommandError::system_fault(
            "solana_persona_registry_parse_failed",
            format!("Could not parse {}: {err}", path.display()),
        )
    })?;
    Ok(parsed)
}

fn persist_registry(root: &Path, cluster: ClusterKind, file: &RegistryFile) -> CommandResult<()> {
    let path = registry_path(root, cluster);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CommandError::system_fault(
                "solana_persona_registry_dir_failed",
                format!("Could not create {}: {err}", parent.display()),
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|err| {
        CommandError::system_fault(
            "solana_persona_registry_serialize_failed",
            format!("Could not serialize persona registry: {err}"),
        )
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|err| {
        CommandError::system_fault(
            "solana_persona_registry_write_failed",
            format!("Could not write {}: {err}", tmp.display()),
        )
    })?;
    fs::rename(&tmp, &path).map_err(|err| {
        CommandError::system_fault(
            "solana_persona_registry_rename_failed",
            format!(
                "Could not rename {} -> {}: {err}",
                tmp.display(),
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn validate_name(name: &str) -> CommandResult<()> {
    if name.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "solana_persona_name_empty",
            "Persona name must be a non-empty string.",
        ));
    }
    if name.len() > 64 {
        return Err(CommandError::user_fixable(
            "solana_persona_name_too_long",
            "Persona name must be <= 64 characters.",
        ));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(CommandError::user_fixable(
            "solana_persona_name_invalid",
            "Persona name contains control characters.",
        ));
    }
    Ok(())
}

fn default_root() -> CommandResult<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| {
        CommandError::system_fault(
            "solana_persona_no_data_dir",
            "Could not resolve OS data directory for persona store.",
        )
    })?;
    Ok(data_dir.join("cadence").join("solana").join("personas"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Re-exports for the Tauri layer.
pub use fund::{DefaultFundingBackend, FundingStep};

// Make `TokenAllocation` and `NftAllocation` reachable from outside via
// `persona::TokenAllocation` so the scenario module and the command
// surface don't need to chain through `persona::roles::…`.
pub use roles::{
    descriptors as role_descriptors, NftAllocation, RoleDescriptor, RolePreset, TokenAllocation,
};

#[cfg(test)]
mod tests {
    use super::*;
    use fund::test_support::MockFundingBackend;
    use keygen::test_support::DeterministicProvider;
    use keygen::KeypairProvider as _;
    use tempfile::TempDir;

    fn make_store(tmp: &TempDir) -> PersonaStore {
        let root = tmp.path().to_path_buf();
        let keypair_root = root.join("keypairs");
        let keypairs = KeypairStore::new(keypair_root, Box::new(DeterministicProvider::new()));
        let funding: Box<dyn FundingBackend> = Box::new(MockFundingBackend::new());
        PersonaStore::new(root, keypairs, funding)
    }

    #[test]
    fn create_localnet_persona_persists_to_disk() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let (persona, receipt) = store
            .create(
                PersonaSpec {
                    name: "alice".into(),
                    cluster: ClusterKind::Localnet,
                    role: PersonaRole::Whale,
                    seed_override: None,
                    note: Some("first persona".into()),
                },
                Some("http://127.0.0.1:8899".into()),
            )
            .unwrap();

        assert_eq!(persona.role, PersonaRole::Whale);
        assert_eq!(persona.cluster, ClusterKind::Localnet);
        assert_eq!(persona.name, "alice");
        assert!(!persona.pubkey.is_empty());
        assert!(receipt.succeeded);
        // Whale preset: airdrop + 3 tokens + 3 NFTs = 7 steps total.
        assert_eq!(receipt.steps.len(), 7);

        let fetched = store.get(ClusterKind::Localnet, "alice").unwrap();
        assert_eq!(fetched.as_ref().unwrap().pubkey, persona.pubkey);
    }

    #[test]
    fn create_persona_without_rpc_url_skips_funding() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let (_persona, receipt) = store
            .create(
                PersonaSpec {
                    name: "eve".into(),
                    cluster: ClusterKind::Localnet,
                    role: PersonaRole::Whale,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();
        assert!(receipt.succeeded);
        assert!(receipt.steps.is_empty());
    }

    #[test]
    fn create_persona_on_mainnet_is_policy_denied() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let err = store
            .create(
                PersonaSpec {
                    name: "oops".into(),
                    cluster: ClusterKind::Mainnet,
                    role: PersonaRole::Whale,
                    seed_override: None,
                    note: None,
                },
                Some("https://api.mainnet-beta.solana.com".into()),
            )
            .unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn import_keypair_is_blocked_on_non_local_clusters() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);

        // Synthesize a bytes blob by generating once.
        let provider = keygen::OsRngKeypairProvider;
        let bytes = provider.generate().unwrap();

        for forbidden in [ClusterKind::Mainnet, ClusterKind::Devnet] {
            let err = store
                .import_keypair(
                    forbidden,
                    "imported",
                    PersonaRole::Custom,
                    bytes.clone(),
                    None,
                )
                .unwrap_err();
            assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
        }
    }

    #[test]
    fn import_keypair_works_on_localnet() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let provider = keygen::OsRngKeypairProvider;
        let bytes = provider.generate().unwrap();
        let expected_pubkey = bytes.pubkey_base58();

        let persona = store
            .import_keypair(
                ClusterKind::Localnet,
                "imported",
                PersonaRole::Custom,
                bytes,
                Some("dev key".into()),
            )
            .unwrap();
        assert_eq!(persona.pubkey, expected_pubkey);
        assert_eq!(persona.cluster, ClusterKind::Localnet);
        assert!(store
            .get(ClusterKind::Localnet, "imported")
            .unwrap()
            .is_some());
    }

    #[test]
    fn duplicate_name_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let spec = PersonaSpec {
            name: "dup".into(),
            cluster: ClusterKind::Localnet,
            role: PersonaRole::NewUser,
            seed_override: None,
            note: None,
        };
        store.create(spec.clone(), None).unwrap();
        let err = store.create(spec, None).unwrap_err();
        assert_eq!(err.code, "solana_persona_already_exists");
    }

    #[test]
    fn delete_removes_keypair_and_registry_entry() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        store
            .create(
                PersonaSpec {
                    name: "bob".into(),
                    cluster: ClusterKind::Localnet,
                    role: PersonaRole::NewUser,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();
        assert!(store.get(ClusterKind::Localnet, "bob").unwrap().is_some());

        store.delete(ClusterKind::Localnet, "bob").unwrap();
        assert!(store.get(ClusterKind::Localnet, "bob").unwrap().is_none());
    }

    #[test]
    fn registry_round_trips_across_store_instances() {
        let tmp = TempDir::new().unwrap();
        {
            let store = make_store(&tmp);
            store
                .create(
                    PersonaSpec {
                        name: "persistent".into(),
                        cluster: ClusterKind::Localnet,
                        role: PersonaRole::Lp,
                        seed_override: None,
                        note: Some("survives restart".into()),
                    },
                    None,
                )
                .unwrap();
        }
        // Fresh store — should reload the persona.
        let store2 = make_store(&tmp);
        let persona = store2
            .get(ClusterKind::Localnet, "persistent")
            .unwrap()
            .expect("registry persisted");
        assert_eq!(persona.role, PersonaRole::Lp);
        assert_eq!(persona.note.as_deref(), Some("survives restart"));
    }

    #[test]
    fn fund_applies_delta_and_returns_receipt() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        store
            .create(
                PersonaSpec {
                    name: "funded".into(),
                    cluster: ClusterKind::Localnet,
                    role: PersonaRole::NewUser,
                    seed_override: None,
                    note: None,
                },
                None,
            )
            .unwrap();

        let delta = FundingDelta {
            sol_lamports: 5_000_000,
            tokens: vec![TokenAllocation::by_symbol("USDC", 10)],
            nfts: vec![],
        };
        let receipt = store
            .fund(
                ClusterKind::Localnet,
                "funded",
                &delta,
                "http://127.0.0.1:8899",
            )
            .unwrap();
        assert!(receipt.succeeded);
        assert_eq!(receipt.steps.len(), 2);
    }

    #[test]
    fn export_keypair_path_on_mainnet_denied() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let err = store
            .export_keypair_path(ClusterKind::Mainnet, "anything")
            .unwrap_err();
        assert_eq!(err.class, crate::commands::CommandErrorClass::PolicyDenied);
    }

    #[test]
    fn validate_name_rejects_empty_and_control_chars() {
        assert!(validate_name("").is_err());
        assert!(validate_name("name\nwith newline").is_err());
        assert!(validate_name("ok_name").is_ok());
    }

    #[test]
    fn seed_override_replaces_role_preset_fields() {
        let spec = PersonaSpec {
            name: "merged".into(),
            cluster: ClusterKind::Localnet,
            role: PersonaRole::Whale,
            seed_override: Some(FundingDelta {
                sol_lamports: 1,
                tokens: vec![],
                nfts: vec![NftAllocation {
                    collection: "only-nfts".into(),
                    count: 1,
                }],
            }),
            note: None,
        };
        let seed = spec.effective_seed();
        // sol overridden, tokens kept from preset (override was empty), nfts overridden.
        assert_eq!(seed.sol_lamports, 1);
        assert!(!seed.tokens.is_empty(), "empty override tokens keep preset");
        assert_eq!(seed.nfts.len(), 1);
        assert_eq!(seed.nfts[0].collection, "only-nfts");
    }
}
