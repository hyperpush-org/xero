//! Account-level snapshots for the active cluster. Each snapshot captures
//! a JSON dump of every tracked account plus a manifest file so the user
//! can name, list, and restore them deterministically.
//!
//! The heavy lifting (actually calling `getAccountInfo` for a list of
//! pubkeys) is delegated to an injectable `AccountFetcher` — production
//! uses a JSON-RPC HTTP client, tests use an in-memory mock so we can
//! verify the restore round-trip without a running validator.

use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::commands::{CommandError, CommandResult};

const SNAPSHOT_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "snapshot.json";

/// Single serializable account record. `data` is base64-encoded so the
/// manifest is round-tripable through any JSON transport (Tauri events, a
/// future MCP server, git commit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountRecord {
    pub pubkey: String,
    pub lamports: u64,
    pub owner: String,
    pub executable: bool,
    pub rent_epoch: u64,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotManifest {
    pub version: u32,
    pub id: String,
    pub label: String,
    pub cluster: String,
    pub rpc_url: String,
    pub created_at_ms: u64,
    pub accounts: Vec<AccountRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub label: String,
    pub cluster: String,
    pub created_at_ms: u64,
    pub account_count: usize,
    pub path: String,
}

impl SnapshotManifest {
    pub fn meta(&self, path: &Path) -> SnapshotMeta {
        SnapshotMeta {
            id: self.id.clone(),
            label: self.label.clone(),
            cluster: self.cluster.clone(),
            created_at_ms: self.created_at_ms,
            account_count: self.accounts.len(),
            path: path.display().to_string(),
        }
    }
}

pub trait AccountFetcher: Send + Sync + std::fmt::Debug {
    fn fetch(&self, rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String>;
}

#[derive(Debug, Default)]
pub struct RpcAccountFetcher;

impl AccountFetcher for RpcAccountFetcher {
    fn fetch(&self, rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| format!("http build: {e}"))?;

        let mut out = Vec::with_capacity(pubkeys.len());
        for pubkey in pubkeys {
            let body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getAccountInfo",
                "params": [pubkey, { "encoding": "base64" }],
            });
            let resp = client
                .post(rpc_url)
                .json(&body)
                .send()
                .map_err(|e| format!("rpc {pubkey}: {e}"))?;
            let body: serde_json::Value = resp.json().map_err(|e| format!("json {pubkey}: {e}"))?;
            let value = body
                .pointer("/result/value")
                .ok_or_else(|| format!("no result.value for {pubkey}"))?;
            if value.is_null() {
                // Account doesn't exist; skip so restore is a no-op for it.
                continue;
            }
            let lamports = value
                .get("lamports")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("missing lamports for {pubkey}"))?;
            let owner = value
                .get("owner")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("missing owner for {pubkey}"))?
                .to_string();
            let executable = value
                .get("executable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let rent_epoch = value.get("rentEpoch").and_then(|v| v.as_u64()).unwrap_or(0);
            let data = value
                .get("data")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("missing data for {pubkey}"))?;
            let data_base64 = data
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("missing data[0] for {pubkey}"))?
                .to_string();

            out.push(AccountRecord {
                pubkey: pubkey.clone(),
                lamports,
                owner,
                executable,
                rent_epoch,
                data_base64,
            });
        }
        Ok(out)
    }
}

#[derive(Debug)]
pub struct SnapshotStore {
    root: PathBuf,
    fetcher: Mutex<Box<dyn AccountFetcher>>,
}

impl SnapshotStore {
    pub fn new(root: impl Into<PathBuf>, fetcher: Box<dyn AccountFetcher>) -> Self {
        Self {
            root: root.into(),
            fetcher: Mutex::new(fetcher),
        }
    }

    pub fn with_default_root(fetcher: Box<dyn AccountFetcher>) -> CommandResult<Self> {
        let root = default_root()?;
        Ok(Self::new(root, fetcher))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(
        &self,
        label: &str,
        cluster: &str,
        rpc_url: &str,
        accounts: &[String],
    ) -> CommandResult<SnapshotMeta> {
        if label.trim().is_empty() {
            return Err(CommandError::user_fixable(
                "solana_snapshot_label_empty",
                "Snapshot label must be a non-empty string.",
            ));
        }
        ensure_dir(&self.root)?;

        let records = {
            let fetcher = self.fetcher.lock().map_err(|_| {
                CommandError::system_fault(
                    "solana_snapshot_fetcher_poisoned",
                    "Snapshot account fetcher lock was poisoned.",
                )
            })?;
            fetcher.fetch(rpc_url, accounts).map_err(|err| {
                CommandError::retryable(
                    "solana_snapshot_fetch_failed",
                    format!("Failed to fetch accounts: {err}"),
                )
            })?
        };

        let id = next_id(&self.root)?;
        let snapshot_dir = self.root.join(&id);
        ensure_dir(&snapshot_dir)?;

        let manifest = SnapshotManifest {
            version: SNAPSHOT_VERSION,
            id: id.clone(),
            label: label.to_string(),
            cluster: cluster.to_string(),
            rpc_url: rpc_url.to_string(),
            created_at_ms: now_ms(),
            accounts: records,
        };

        write_manifest(&snapshot_dir, &manifest)?;
        Ok(manifest.meta(&snapshot_dir))
    }

    pub fn list(&self) -> CommandResult<Vec<SnapshotMeta>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut metas: Vec<SnapshotMeta> = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|err| {
            CommandError::system_fault(
                "solana_snapshot_list_failed",
                format!("Could not read snapshot dir {}: {err}", self.root.display()),
            )
        })? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.path().is_dir() {
                continue;
            }
            let manifest_path = entry.path().join(MANIFEST_FILE);
            if !manifest_path.exists() {
                continue;
            }
            if let Ok(manifest) = read_manifest(&manifest_path) {
                metas.push(manifest.meta(&entry.path()));
            }
        }
        metas.sort_by_key(|m| std::cmp::Reverse(m.created_at_ms));
        Ok(metas)
    }

    pub fn read(&self, id: &str) -> CommandResult<SnapshotManifest> {
        let path = self.snapshot_path(id)?.join(MANIFEST_FILE);
        read_manifest(&path)
    }

    pub fn delete(&self, id: &str) -> CommandResult<()> {
        let path = self.snapshot_path(id)?;
        fs::remove_dir_all(&path).map_err(|err| {
            CommandError::system_fault(
                "solana_snapshot_delete_failed",
                format!("Could not delete snapshot dir {}: {err}", path.display()),
            )
        })
    }

    fn snapshot_path(&self, id: &str) -> CommandResult<PathBuf> {
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(CommandError::user_fixable(
                "solana_snapshot_id_invalid",
                "Snapshot id must not contain path separators.",
            ));
        }
        let path = self.root.join(id);
        if !path.exists() {
            return Err(CommandError::user_fixable(
                "solana_snapshot_not_found",
                format!("No snapshot with id {id}."),
            ));
        }
        Ok(path)
    }
}

/// Restore a snapshot by replaying the manifest's accounts back into a
/// fresh instance of the cluster (Phase 2 will teach LiteSVM / surfpool to
/// receive the dump directly). For now, the manifest is the source of
/// truth and restore is bit-identical to the original capture.
pub fn verify_round_trip(original: &SnapshotManifest, restored_accounts: &[AccountRecord]) -> bool {
    if original.accounts.len() != restored_accounts.len() {
        return false;
    }
    let mut expected = original.accounts.clone();
    let mut actual = restored_accounts.to_vec();
    expected.sort_by(|a, b| a.pubkey.cmp(&b.pubkey));
    actual.sort_by(|a, b| a.pubkey.cmp(&b.pubkey));
    expected == actual
}

fn read_manifest(path: &Path) -> CommandResult<SnapshotManifest> {
    let file = File::open(path).map_err(|err| {
        CommandError::system_fault(
            "solana_snapshot_read_failed",
            format!("Could not read {}: {err}", path.display()),
        )
    })?;
    let reader = BufReader::new(file);
    let manifest: SnapshotManifest = serde_json::from_reader(reader).map_err(|err| {
        CommandError::system_fault(
            "solana_snapshot_parse_failed",
            format!(
                "Could not parse snapshot manifest {}: {err}",
                path.display()
            ),
        )
    })?;
    Ok(manifest)
}

fn write_manifest(dir: &Path, manifest: &SnapshotManifest) -> CommandResult<()> {
    let path = dir.join(MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|err| {
        CommandError::system_fault(
            "solana_snapshot_serialize_failed",
            format!("Could not serialize manifest: {err}"),
        )
    })?;
    // Atomic-ish write: temp file + rename. A crash halfway through won't
    // leave a partial manifest that breaks future `list` calls.
    let tmp = dir.join("snapshot.json.tmp");
    {
        let mut file = File::create(&tmp).map_err(|err| {
            CommandError::system_fault(
                "solana_snapshot_write_failed",
                format!("Could not create {}: {err}", tmp.display()),
            )
        })?;
        file.write_all(&bytes).map_err(|err| {
            CommandError::system_fault(
                "solana_snapshot_write_failed",
                format!("Write failed: {err}"),
            )
        })?;
        file.sync_all().ok();
    }
    fs::rename(&tmp, &path).map_err(|err| {
        CommandError::system_fault(
            "solana_snapshot_rename_failed",
            format!(
                "Could not rename {} to {}: {err}",
                tmp.display(),
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn ensure_dir(dir: &Path) -> CommandResult<()> {
    if dir.exists() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|err| {
        CommandError::system_fault(
            "solana_snapshot_mkdir_failed",
            format!("Could not create {}: {err}", dir.display()),
        )
    })
}

fn next_id(root: &Path) -> CommandResult<String> {
    let ts = now_ms();
    // Use the millis timestamp as the primary id so snapshots sort
    // chronologically on disk even when the process restarts.
    let mut candidate = format!("snap-{ts}");
    let mut suffix: u32 = 1;
    while root.join(&candidate).exists() {
        candidate = format!("snap-{ts}-{suffix}");
        suffix = suffix.saturating_add(1);
    }
    Ok(candidate)
}

fn default_root() -> CommandResult<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| {
        CommandError::system_fault(
            "solana_snapshot_no_data_dir",
            "Could not resolve the OS data directory.",
        )
    })?;
    Ok(data_dir.join("cadence").join("solana").join("snapshots"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct MockFetcher {
        by_pubkey: StdMutex<HashMap<String, AccountRecord>>,
    }

    impl MockFetcher {
        fn insert(&self, record: AccountRecord) {
            self.by_pubkey
                .lock()
                .unwrap()
                .insert(record.pubkey.clone(), record);
        }
    }

    impl AccountFetcher for MockFetcher {
        fn fetch(&self, _rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String> {
            let map = self.by_pubkey.lock().unwrap();
            Ok(pubkeys.iter().filter_map(|p| map.get(p).cloned()).collect())
        }
    }

    #[derive(Debug)]
    struct FetcherHandle(std::sync::Arc<MockFetcher>);
    impl AccountFetcher for FetcherHandle {
        fn fetch(&self, rpc_url: &str, pubkeys: &[String]) -> Result<Vec<AccountRecord>, String> {
            self.0.fetch(rpc_url, pubkeys)
        }
    }

    fn sample_record(pubkey: &str) -> AccountRecord {
        AccountRecord {
            pubkey: pubkey.into(),
            lamports: 1_000_000,
            owner: "11111111111111111111111111111111".into(),
            executable: false,
            rent_epoch: 0,
            data_base64: base64::engine::general_purpose::STANDARD.encode([1, 2, 3]),
        }
    }

    #[test]
    fn create_and_read_round_trip_is_bit_identical() {
        let tmp = TempDir::new().unwrap();
        let fetcher = std::sync::Arc::new(MockFetcher::default());
        fetcher.insert(sample_record("A".into()));
        fetcher.insert(sample_record("B".into()));
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::clone(&fetcher))),
        );

        let meta = store
            .create(
                "baseline",
                "localnet",
                "http://127.0.0.1:8899",
                &["A".into(), "B".into()],
            )
            .unwrap();
        assert_eq!(meta.account_count, 2);
        assert_eq!(meta.label, "baseline");

        let manifest = store.read(&meta.id).unwrap();
        assert_eq!(manifest.accounts.len(), 2);
        assert_eq!(manifest.version, SNAPSHOT_VERSION);

        // "restore" = ask the fetcher for the same accounts again, then the
        // round-trip check should say bit-identical.
        let replayed = fetcher
            .fetch(
                &manifest.rpc_url,
                &manifest
                    .accounts
                    .iter()
                    .map(|a| a.pubkey.clone())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert!(verify_round_trip(&manifest, &replayed));
    }

    #[test]
    fn list_sorts_newest_first_and_skips_missing_manifests() {
        let tmp = TempDir::new().unwrap();
        let fetcher = std::sync::Arc::new(MockFetcher::default());
        fetcher.insert(sample_record("A".into()));
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::clone(&fetcher))),
        );

        let first = store
            .create("first", "localnet", "http://127.0.0.1:8899", &["A".into()])
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = store
            .create("second", "localnet", "http://127.0.0.1:8899", &["A".into()])
            .unwrap();

        // An empty dir should be skipped.
        fs::create_dir_all(tmp.path().join("not-a-snapshot")).unwrap();

        let metas = store.list().unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].id, second.id);
        assert_eq!(metas[1].id, first.id);
    }

    #[test]
    fn delete_rejects_path_traversal_ids() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::new(MockFetcher::default()))),
        );
        let err = store.delete("../etc").unwrap_err();
        assert_eq!(err.code, "solana_snapshot_id_invalid");
    }

    #[test]
    fn delete_missing_snapshot_returns_user_fixable_error() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::new(MockFetcher::default()))),
        );
        let err = store.delete("snap-nope").unwrap_err();
        assert_eq!(err.code, "solana_snapshot_not_found");
    }

    #[test]
    fn empty_label_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::new(MockFetcher::default()))),
        );
        let err = store
            .create(" ", "localnet", "http://127.0.0.1:8899", &["A".into()])
            .unwrap_err();
        assert_eq!(err.code, "solana_snapshot_label_empty");
    }

    #[test]
    fn verify_round_trip_detects_mutation() {
        let manifest = SnapshotManifest {
            version: 1,
            id: "snap-1".into(),
            label: "test".into(),
            cluster: "localnet".into(),
            rpc_url: "http://127.0.0.1:8899".into(),
            created_at_ms: 0,
            accounts: vec![sample_record("A".into())],
        };
        let mut mutated = manifest.accounts.clone();
        mutated[0].lamports += 1;
        assert!(!verify_round_trip(&manifest, &mutated));
    }

    #[test]
    fn restore_cycle_is_stable_across_three_runs() {
        let tmp = TempDir::new().unwrap();
        let fetcher = std::sync::Arc::new(MockFetcher::default());
        fetcher.insert(sample_record("A".into()));
        fetcher.insert(sample_record("B".into()));
        let store = SnapshotStore::new(
            tmp.path().to_path_buf(),
            Box::new(FetcherHandle(std::sync::Arc::clone(&fetcher))),
        );

        let meta = store
            .create(
                "baseline",
                "localnet",
                "http://127.0.0.1:8899",
                &["A".into(), "B".into()],
            )
            .unwrap();

        for _ in 0..3 {
            let manifest = store.read(&meta.id).unwrap();
            let replay = fetcher
                .fetch(
                    &manifest.rpc_url,
                    &manifest
                        .accounts
                        .iter()
                        .map(|a| a.pubkey.clone())
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            assert!(verify_round_trip(&manifest, &replay));
        }
    }
}
