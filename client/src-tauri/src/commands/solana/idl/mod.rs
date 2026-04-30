//! IDL registry, file watcher, on-chain fetch fallback.
//!
//! Phase 4 centre: almost every interpretive feature (decoder, CPI
//! resolver, Codama codegen, drift detection, log decoder) keys off a
//! cached IDL, so we centralise that here.
//!
//! An `Idl` is stored as `serde_json::Value` — we don't pin to a specific
//! Anchor-IDL schema version. The registry only needs to canonicalise
//! (pretty-print, strip whitespace hash), persist, and diff. Anchor's IDL
//! shape has changed across 0.29/0.30/Shinano, and the drift module is
//! responsible for parsing the subset it cares about.
//!
//! The registry supports two load paths:
//!   1. **Local file** — reading a `target/idl/*.json` off disk and
//!      tracking its mtime + hash. Used by the Tauri command surface when
//!      the caller passes an IDL path.
//!   2. **On-chain fetch** — a JSON-RPC call to fetch the account at the
//!      IDL address derived from the program id (Anchor's `idl-address`
//!      scheme). Lives behind an `IdlFetcher` trait so tests script the
//!      RPC without touching the network.
//!
//! File watching uses a mtime poller in a dedicated thread — we don't
//! pull in `notify` just for this. The polling interval is 500ms which
//! is well under the acceptance target of 2s (lib.rs edit → frontend
//! typecheck).

pub mod codama;
pub mod drift;
pub mod publish;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::cluster::ClusterKind;
use crate::commands::{CommandError, CommandResult};

pub use drift::{DriftChange, DriftReport, DriftSeverity};

const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A cached IDL. `value` is the raw JSON; `hash` is the SHA-256 over the
/// canonicalised bytes so the watcher can detect real changes (not just
/// mtime touches from `make` re-runs with unchanged output).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Idl {
    pub value: Value,
    pub hash: String,
    pub source: IdlSource,
    pub fetched_at_ms: u64,
}

impl Idl {
    pub fn from_value(value: Value, source: IdlSource) -> Self {
        let canonical = canonicalise(&value);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let hash = hex::upper(hasher.finalize().as_slice());
        Self {
            value,
            hash,
            source,
            fetched_at_ms: now_ms(),
        }
    }

    pub fn program_name(&self) -> Option<String> {
        self.value
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            .or_else(|| self.value.get("name").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
    }

    pub fn program_id(&self) -> Option<String> {
        self.value
            .get("address")
            .and_then(|v| v.as_str())
            .or_else(|| {
                self.value
                    .get("metadata")
                    .and_then(|m| m.get("address"))
                    .and_then(|v| v.as_str())
            })
            .map(|s| s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum IdlSource {
    /// Loaded from a local file — the path the user pointed us at.
    File { path: String },
    /// Pulled from the cluster via `getAccountInfo` at the IDL address.
    Chain {
        cluster: ClusterKind,
        idl_address: String,
    },
    /// Constructed synthetically (tests).
    Synthetic,
}

/// Trait the registry uses to fetch an IDL from chain. Production impl
/// wraps the `tx::RpcTransport`; tests provide a scripted impl.
pub trait IdlFetcher: Send + Sync + std::fmt::Debug {
    fn fetch(
        &self,
        cluster: ClusterKind,
        rpc_url: &str,
        program_id: &str,
    ) -> CommandResult<Option<FetchedIdl>>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FetchedIdl {
    pub value: Value,
    pub idl_address: String,
}

/// Events emitted through the registry's own channel and bridged by the
/// Tauri layer onto `solana:idl:changed`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdlChangedEvent {
    pub token: String,
    pub path: String,
    pub program_id: Option<String>,
    pub program_name: Option<String>,
    pub hash: String,
    pub ts_ms: u64,
    pub phase: IdlChangePhase,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdlChangePhase {
    /// Initial snapshot when the watch starts.
    Initial,
    /// File contents changed (hash differs from previous snapshot).
    Updated,
    /// File disappeared after being watched.
    Removed,
    /// Parse failure — reported back so the UI can surface the error.
    Invalid,
}

/// Opaque subscription token — a `String` so it serializes cleanly and
/// can't be confused with a u64 id from another module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct IdlSubscriptionToken(pub String);

impl IdlSubscriptionToken {
    fn new(id: u64) -> Self {
        Self(format!("idl-watch-{id}"))
    }
}

/// Public trait for consumers of IDL watcher events.
pub trait IdlEventSink: Send + Sync + std::fmt::Debug {
    fn emit(&self, event: IdlChangedEvent);
}

/// No-op sink used by tests and when the watcher runs without a Tauri
/// `AppHandle`.
#[derive(Debug, Default, Clone)]
pub struct NullIdlEventSink;

impl IdlEventSink for NullIdlEventSink {
    fn emit(&self, _event: IdlChangedEvent) {}
}

/// Collecting sink used in tests to assert the sequence of events.
#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub struct CollectingIdlEventSink(pub Arc<Mutex<Vec<IdlChangedEvent>>>);

#[cfg(test)]
impl IdlEventSink for CollectingIdlEventSink {
    fn emit(&self, event: IdlChangedEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[derive(Debug)]
struct WatchHandle {
    path: PathBuf,
    stop: Arc<Mutex<bool>>,
    thread: Option<JoinHandle<()>>,
    /// Shared with the poller thread; only accessed through
    /// `poll_once_for_tests` in non-production builds, which is why the
    /// compiler flags it as unread in the release profile.
    #[allow(dead_code)]
    last_hash: Arc<Mutex<Option<String>>>,
}

/// Runtime cache + file watcher.
///
/// Threading note: the watcher owns a single poller thread per watched
/// path — the acceptance target ("2s from edit to codegen") is met by a
/// 500ms poll and a background codegen run, which is plenty; we don't
/// need inotify/kqueue integration here and picking up a new dep for it
/// isn't worth the cross-platform tax.
#[derive(Debug)]
pub struct IdlRegistry {
    cache: RwLock<HashMap<String, Idl>>,
    fetcher: Arc<dyn IdlFetcher>,
    sink: RwLock<Arc<dyn IdlEventSink>>,
    watches: Mutex<HashMap<IdlSubscriptionToken, WatchHandle>>,
    next_watch_id: AtomicU64,
}

impl IdlRegistry {
    pub fn new(fetcher: Arc<dyn IdlFetcher>) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            fetcher,
            sink: RwLock::new(Arc::new(NullIdlEventSink) as Arc<dyn IdlEventSink>),
            watches: Mutex::new(HashMap::new()),
            next_watch_id: AtomicU64::new(1),
        }
    }

    pub fn with_sink(fetcher: Arc<dyn IdlFetcher>, sink: Arc<dyn IdlEventSink>) -> Self {
        let me = Self::new(fetcher);
        *me.sink.write().unwrap() = sink;
        me
    }

    /// Replace the event sink at runtime. Used by the Tauri layer to
    /// bind an `AppHandle`-backed emitter after state construction.
    pub fn set_sink(&self, sink: Arc<dyn IdlEventSink>) {
        *self.sink.write().unwrap() = sink;
    }

    /// Register an IDL value directly into the cache, skipping disk I/O.
    /// Primarily used by the log bus tests to seed an IDL that the log
    /// decoder can look up — real code paths should prefer `load_file`
    /// or `fetch_on_chain`.
    pub fn load_value_for_tests(&self, value: Value) -> CommandResult<Idl> {
        let idl = Idl::from_value(value, IdlSource::Synthetic);
        let key = format!(
            "synthetic::{}::{}",
            idl.program_id().unwrap_or_else(|| "<no-id>".into()),
            idl.hash
        );
        self.insert(&key, idl.clone());
        Ok(idl)
    }

    pub fn load_file(&self, path: &Path) -> CommandResult<Idl> {
        let bytes = fs::read(path).map_err(|err| read_error(path, err))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|err| {
            CommandError::user_fixable(
                "solana_idl_parse_failed",
                format!("IDL at {} is not valid JSON: {err}", path.display()),
            )
        })?;
        let idl = Idl::from_value(
            value,
            IdlSource::File {
                path: path.display().to_string(),
            },
        );
        self.insert(&cache_key_file(path), idl.clone());
        Ok(idl)
    }

    /// Fetch an IDL from the cluster via the injected fetcher. Returns
    /// `None` if the program has no IDL account on that cluster.
    pub fn fetch_on_chain(
        &self,
        cluster: ClusterKind,
        rpc_url: &str,
        program_id: &str,
    ) -> CommandResult<Option<Idl>> {
        let fetched = self.fetcher.fetch(cluster, rpc_url, program_id)?;
        Ok(fetched.map(|payload| {
            let idl = Idl::from_value(
                payload.value,
                IdlSource::Chain {
                    cluster,
                    idl_address: payload.idl_address,
                },
            );
            self.insert(&cache_key_chain(cluster, program_id), idl.clone());
            idl
        }))
    }

    /// Get the highest-priority cached IDL for a program across the
    /// registry (file > cluster-specific).
    pub fn get_cached(&self, program_id: &str, cluster: Option<ClusterKind>) -> Option<Idl> {
        let cache = self.cache.read().ok()?;
        if let Some(c) = cluster {
            if let Some(idl) = cache.get(&cache_key_chain(c, program_id)) {
                return Some(idl.clone());
            }
        }
        // Scan files for a matching program id.
        cache
            .iter()
            .filter_map(|(_key, idl)| {
                if idl.program_id().as_deref() == Some(program_id) {
                    Some(idl.clone())
                } else {
                    None
                }
            })
            .next()
    }

    pub fn cache_entries(&self) -> Vec<(String, Idl)> {
        self.cache
            .read()
            .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    pub fn watch_path(&self, path: &Path) -> CommandResult<IdlSubscriptionToken> {
        let canonical = resolve_path(path)?;
        let id = self.next_watch_id.fetch_add(1, Ordering::SeqCst);
        let token = IdlSubscriptionToken::new(id);
        let stop = Arc::new(Mutex::new(false));
        let last_hash: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let sink = Arc::clone(&*self.sink.read().unwrap());
        let sink_thread = Arc::clone(&sink);
        let thread_stop = Arc::clone(&stop);
        let thread_hash = Arc::clone(&last_hash);
        let token_clone = token.clone();
        let path_for_thread = canonical.clone();

        let thread = thread::Builder::new()
            .name(format!("xero-solana-idl-watch-{id}"))
            .spawn(move || {
                watcher_loop(
                    token_clone,
                    path_for_thread,
                    sink_thread,
                    thread_stop,
                    thread_hash,
                );
            })
            .map_err(|err| {
                CommandError::system_fault(
                    "solana_idl_watch_spawn_failed",
                    format!("Could not spawn IDL watcher thread: {err}"),
                )
            })?;

        let handle = WatchHandle {
            path: canonical,
            stop,
            thread: Some(thread),
            last_hash,
        };
        self.watches
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "solana_idl_watch_poisoned",
                    "IDL watch registry lock poisoned.",
                )
            })?
            .insert(token.clone(), handle);
        Ok(token)
    }

    pub fn unwatch(&self, token: &IdlSubscriptionToken) -> CommandResult<bool> {
        let mut guard = self.watches.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_idl_watch_poisoned",
                "IDL watch registry lock poisoned.",
            )
        })?;
        if let Some(mut handle) = guard.remove(token) {
            *handle.stop.lock().expect("stop flag poisoned") = true;
            if let Some(thread) = handle.thread.take() {
                drop(guard);
                let _ = thread.join();
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn active_watches(&self) -> Vec<(IdlSubscriptionToken, PathBuf)> {
        self.watches
            .lock()
            .map(|guard| {
                guard
                    .iter()
                    .map(|(k, v)| (k.clone(), v.path.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Synchronously re-read a watched path and emit one event. Lets the
    /// caller verify the watcher pipeline without waiting for the poll.
    #[cfg(test)]
    pub fn poll_once_for_tests(&self, token: &IdlSubscriptionToken) -> CommandResult<()> {
        let guard = self
            .watches
            .lock()
            .map_err(|_| CommandError::system_fault("solana_idl_watch_poisoned", "poisoned"))?;
        let handle = guard.get(token).ok_or_else(|| {
            CommandError::user_fixable("solana_idl_watch_not_found", "no such token")
        })?;
        let path = handle.path.clone();
        let last_hash = Arc::clone(&handle.last_hash);
        drop(guard);
        poll_once(
            token,
            &path,
            &*self.sink.read().unwrap().clone(),
            &last_hash,
        );
        Ok(())
    }

    fn insert(&self, key: &str, idl: Idl) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key.to_string(), idl);
        }
    }
}

impl Drop for IdlRegistry {
    fn drop(&mut self) {
        let mut guard = match self.watches.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        for (_, mut handle) in guard.drain() {
            if let Ok(mut flag) = handle.stop.lock() {
                *flag = true;
            }
            if let Some(thread) = handle.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

fn watcher_loop(
    token: IdlSubscriptionToken,
    path: PathBuf,
    sink: Arc<dyn IdlEventSink>,
    stop: Arc<Mutex<bool>>,
    last_hash: Arc<Mutex<Option<String>>>,
) {
    // Emit an initial snapshot immediately so the caller doesn't have to
    // race the first poll.
    poll_once_internal(&token, &path, sink.as_ref(), &last_hash, true);
    let start = Instant::now();
    loop {
        if let Ok(flag) = stop.lock() {
            if *flag {
                return;
            }
        }
        // Sleep in small slices so shutdown is responsive even with a
        // 500ms poll interval.
        let deadline = Instant::now() + WATCH_POLL_INTERVAL;
        while Instant::now() < deadline {
            if let Ok(flag) = stop.lock() {
                if *flag {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(50));
            // Safety rail: watcher terminates itself after 7 days to
            // avoid zombie threads if shutdown never fires. Nothing
            // should be watching a file for longer in practice.
            if start.elapsed() > Duration::from_secs(7 * 24 * 3600) {
                return;
            }
        }
        poll_once_internal(&token, &path, sink.as_ref(), &last_hash, false);
    }
}

#[cfg(test)]
fn poll_once(
    token: &IdlSubscriptionToken,
    path: &Path,
    sink: &dyn IdlEventSink,
    last_hash: &Arc<Mutex<Option<String>>>,
) {
    poll_once_internal(token, path, sink, last_hash, false);
}

fn poll_once_internal(
    token: &IdlSubscriptionToken,
    path: &Path,
    sink: &dyn IdlEventSink,
    last_hash: &Arc<Mutex<Option<String>>>,
    is_initial: bool,
) {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let mut guard = last_hash.lock().expect("hash lock poisoned");
            if guard.is_some() {
                *guard = None;
                drop(guard);
                sink.emit(IdlChangedEvent {
                    token: token.0.clone(),
                    path: path.display().to_string(),
                    program_id: None,
                    program_name: None,
                    hash: String::new(),
                    ts_ms: now_ms(),
                    phase: IdlChangePhase::Removed,
                });
            }
            return;
        }
        Err(_) => return,
    };
    let parsed: Result<Value, _> = serde_json::from_slice(&bytes);
    let value = match parsed {
        Ok(v) => v,
        Err(err) => {
            sink.emit(IdlChangedEvent {
                token: token.0.clone(),
                path: path.display().to_string(),
                program_id: None,
                program_name: None,
                hash: format!("invalid: {err}"),
                ts_ms: now_ms(),
                phase: IdlChangePhase::Invalid,
            });
            return;
        }
    };
    let canonical = canonicalise(&value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let hash = hex::upper(hasher.finalize().as_slice());

    let mut guard = last_hash.lock().expect("hash lock poisoned");
    let should_emit = match guard.as_ref() {
        None => true,
        Some(prev) => prev != &hash,
    };
    if should_emit {
        *guard = Some(hash.clone());
        drop(guard);
        let idl = Idl::from_value(
            value,
            IdlSource::File {
                path: path.display().to_string(),
            },
        );
        sink.emit(IdlChangedEvent {
            token: token.0.clone(),
            path: path.display().to_string(),
            program_id: idl.program_id(),
            program_name: idl.program_name(),
            hash,
            ts_ms: now_ms(),
            phase: if is_initial {
                IdlChangePhase::Initial
            } else {
                IdlChangePhase::Updated
            },
        });
    }
}

pub(crate) fn canonicalise(value: &Value) -> String {
    // Canonical form: pretty-print with sorted keys is overkill here —
    // `serde_json` already sorts maps when we round-trip through
    // `to_string`. We use compact form so the hash is stable across
    // formatting changes to the source file that serde doesn't alter.
    serde_json::to_string(value).unwrap_or_default()
}

fn cache_key_file(path: &Path) -> String {
    format!("file::{}", path.display())
}

fn cache_key_chain(cluster: ClusterKind, program_id: &str) -> String {
    format!("chain::{}::{}", cluster.as_str(), program_id)
}

fn resolve_path(path: &Path) -> CommandResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|err| {
            CommandError::system_fault(
                "solana_idl_resolve_cwd_failed",
                format!("Could not resolve cwd for relative IDL path: {err}"),
            )
        })
}

fn read_error(path: &Path, err: io::Error) -> CommandError {
    if err.kind() == io::ErrorKind::NotFound {
        CommandError::user_fixable(
            "solana_idl_not_found",
            format!("IDL file not found: {}", path.display()),
        )
    } else {
        CommandError::system_fault(
            "solana_idl_read_failed",
            format!("Could not read {}: {err}", path.display()),
        )
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

mod hex {
    pub fn upper(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        out
    }
}

/// RPC-backed `IdlFetcher` used in production. Reads the account at the
/// Anchor IDL address (the PDA of `seeds = ["anchor:idl"]` + program_id),
/// strips the 44-byte Anchor IDL account header, and zlib-inflates the
/// remaining bytes into the IDL JSON.
pub mod chain_fetcher;

pub use chain_fetcher::RpcIdlFetcher;

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct ScriptedIdlFetcher {
        pub responses: Mutex<HashMap<String, Option<FetchedIdl>>>,
        pub calls: Mutex<Vec<(ClusterKind, String, String)>>,
    }

    impl ScriptedIdlFetcher {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set(&self, program_id: &str, payload: Option<FetchedIdl>) {
            self.responses
                .lock()
                .unwrap()
                .insert(program_id.to_string(), payload);
        }
    }

    impl IdlFetcher for ScriptedIdlFetcher {
        fn fetch(
            &self,
            cluster: ClusterKind,
            rpc_url: &str,
            program_id: &str,
        ) -> CommandResult<Option<FetchedIdl>> {
            self.calls
                .lock()
                .unwrap()
                .push((cluster, rpc_url.to_string(), program_id.to_string()));
            Ok(self
                .responses
                .lock()
                .unwrap()
                .get(program_id)
                .cloned()
                .unwrap_or(None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedIdlFetcher;
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn sample_idl() -> Value {
        json!({
            "metadata": { "name": "my_program", "version": "0.1.0", "address": "Prog11111111111111111111111111111111111111" },
            "instructions": [
                { "name": "initialize", "accounts": [], "args": [] }
            ],
            "accounts": [
                { "name": "State", "discriminator": [1,2,3,4,5,6,7,8] }
            ],
            "errors": [
                { "code": 6000, "name": "InvalidSeed", "msg": "invalid seed provided" }
            ]
        })
    }

    #[test]
    fn hash_is_stable_for_identical_value_content() {
        let a = Idl::from_value(sample_idl(), IdlSource::Synthetic);
        let b = Idl::from_value(sample_idl(), IdlSource::Synthetic);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn load_file_populates_cache() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("idl.json");
        fs::write(&path, serde_json::to_vec(&sample_idl()).unwrap()).unwrap();
        let registry = IdlRegistry::new(Arc::new(ScriptedIdlFetcher::new()));
        let idl = registry.load_file(&path).unwrap();
        assert_eq!(
            idl.program_id().as_deref(),
            Some("Prog11111111111111111111111111111111111111")
        );
        let cached = registry.get_cached("Prog11111111111111111111111111111111111111", None);
        assert!(cached.is_some());
    }

    #[test]
    fn fetch_on_chain_persists_per_cluster() {
        let fetcher = ScriptedIdlFetcher::new();
        fetcher.set(
            "Prog11111111111111111111111111111111111111",
            Some(FetchedIdl {
                value: sample_idl(),
                idl_address: "IdlAddr1".to_string(),
            }),
        );
        let registry = IdlRegistry::new(Arc::new(fetcher));
        let got = registry
            .fetch_on_chain(
                ClusterKind::Devnet,
                "https://api.devnet.solana.com",
                "Prog11111111111111111111111111111111111111",
            )
            .unwrap();
        assert!(got.is_some());
        let entries = registry.cache_entries();
        assert!(entries
            .iter()
            .any(|(k, _)| k.starts_with("chain::devnet::")));
    }

    #[test]
    fn fetch_on_chain_returns_none_when_missing() {
        let registry = IdlRegistry::new(Arc::new(ScriptedIdlFetcher::new()));
        let got = registry
            .fetch_on_chain(ClusterKind::Devnet, "http://rpc.test", "Missing1111")
            .unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn watcher_emits_initial_snapshot() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("idl.json");
        fs::write(&path, serde_json::to_vec(&sample_idl()).unwrap()).unwrap();
        let sink = CollectingIdlEventSink::default();
        let sink_arc: Arc<dyn IdlEventSink> = Arc::new(sink.clone());
        let registry = IdlRegistry::with_sink(Arc::new(ScriptedIdlFetcher::new()), sink_arc);
        let token = registry.watch_path(&path).unwrap();
        // Give the watcher thread a moment to run its first poll.
        thread::sleep(Duration::from_millis(200));
        let events = sink.0.lock().unwrap().clone();
        assert!(!events.is_empty(), "watcher should emit at least once");
        assert_eq!(events[0].phase, IdlChangePhase::Initial);
        registry.unwatch(&token).unwrap();
    }

    #[test]
    fn watcher_detects_update_via_manual_poll() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("idl.json");
        fs::write(&path, serde_json::to_vec(&sample_idl()).unwrap()).unwrap();
        let sink = CollectingIdlEventSink::default();
        let sink_arc: Arc<dyn IdlEventSink> = Arc::new(sink.clone());
        let registry = IdlRegistry::with_sink(Arc::new(ScriptedIdlFetcher::new()), sink_arc);
        let token = registry.watch_path(&path).unwrap();
        thread::sleep(Duration::from_millis(200));

        // Modify the file, then nudge the poll without waiting for the
        // next 500ms tick.
        let mut edited = sample_idl();
        edited["instructions"][0]["name"] = json!("initialize_v2");
        fs::write(&path, serde_json::to_vec(&edited).unwrap()).unwrap();
        registry.poll_once_for_tests(&token).unwrap();

        let events = sink.0.lock().unwrap().clone();
        assert!(events.iter().any(|e| e.phase == IdlChangePhase::Updated));
        registry.unwatch(&token).unwrap();
    }

    #[test]
    fn watcher_reports_missing_file_after_removal() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("idl.json");
        fs::write(&path, serde_json::to_vec(&sample_idl()).unwrap()).unwrap();
        let sink = CollectingIdlEventSink::default();
        let sink_arc: Arc<dyn IdlEventSink> = Arc::new(sink.clone());
        let registry = IdlRegistry::with_sink(Arc::new(ScriptedIdlFetcher::new()), sink_arc);
        let token = registry.watch_path(&path).unwrap();
        thread::sleep(Duration::from_millis(200));

        fs::remove_file(&path).unwrap();
        registry.poll_once_for_tests(&token).unwrap();

        let events = sink.0.lock().unwrap().clone();
        assert!(events.iter().any(|e| e.phase == IdlChangePhase::Removed));
        registry.unwatch(&token).unwrap();
    }

    #[test]
    fn unwatch_returns_false_for_unknown_token() {
        let registry = IdlRegistry::new(Arc::new(ScriptedIdlFetcher::new()));
        let ok = registry
            .unwatch(&IdlSubscriptionToken("idl-watch-999".into()))
            .unwrap();
        assert!(!ok);
    }
}
