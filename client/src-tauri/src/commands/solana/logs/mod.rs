//! Phase 7 — observability.
//!
//! Ties together three information streams a developer debugging a
//! Solana program has to reason about:
//!
//! 1. **Validator stdout** — the raw text the local validator prints when
//!    a transaction lands. Captured in the Tauri layer via the supervisor's
//!    child handle (plumbed later as a `ValidatorLogSource`).
//! 2. **`logsSubscribe` WebSocket / `getSignaturesForAddress` polling** —
//!    per-program invocation logs fetched from the cluster RPC. Primary
//!    source for the "last N slots" acceptance criterion. Implemented via
//!    the existing `RpcTransport` trait so the integration tests script
//!    responses without a live cluster.
//! 3. **Decoded event stream** — each raw log entry is fed through
//!    `tx::decoder::decode_logs` and the `IdlRegistry` so the frontend
//!    renders `InvalidVoteRecord` instead of `0x1770`, and `SwapEvent`
//!    instead of `Program data: <base64>`.
//!
//! The module is split into:
//! - `mod.rs` — `LogBus`, subscriptions, filter matching, entry
//!   aggregation, event sink bridge.
//! - `decoder.rs` — Anchor event discriminator extraction + IDL error
//!   map assembly.
//! - `rpc_source.rs` — RPC-backed recent-slot fetcher (uses
//!   `RpcTransport` trait).
//!
//! ### Threading
//! `LogBus` only holds data structures. It does **not** spawn background
//! threads directly — callers (the Tauri command layer) drive polling
//! via explicit `publish(...)` / `fetch_recent(...)` calls. This keeps
//! the test surface synchronous and the production code's worker thread
//! in one place (the Tauri layer).

pub mod decoder;
pub mod rpc_source;

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::commands::CommandError;

use super::cluster::ClusterKind;
use super::idl::IdlRegistry;
use super::tx::decoder::{explain_simulation, DecodedLogs, Explanation};

pub use decoder::{
    build_idl_error_map, extract_anchor_events, AnchorEvent, EntryDecoder, LogDecoder,
};
pub use rpc_source::{RpcLogSource, SystemRpcLogSource};

const DEFAULT_RING_CAPACITY: usize = 512;
const DEFAULT_SUBSCRIPTION_BACKLOG: usize = 256;

/// Filter the caller applies when subscribing to the log stream. Empty
/// `program_ids` means "any program"; non-empty narrows to those
/// invocations only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogFilter {
    /// Cluster this subscription cares about. A subscription only sees
    /// entries emitted on its cluster.
    pub cluster: ClusterKind,
    /// Program ids the caller wants invocations for. Empty = every
    /// program the source produces.
    #[serde(default)]
    pub program_ids: Vec<String>,
    /// When `true`, the subscriber receives decoded-event entries
    /// (`solana:log:decoded`). When `false`, the decoder still runs but
    /// only the raw-logs entry is dispatched. Defaults to `true` so the
    /// UI "decoded events" feed works without extra config.
    #[serde(default = "default_true")]
    pub include_decoded: bool,
}

fn default_true() -> bool {
    true
}

impl LogFilter {
    /// Construct a filter that matches any program on a cluster.
    pub fn cluster(cluster: ClusterKind) -> Self {
        Self {
            cluster,
            program_ids: Vec::new(),
            include_decoded: true,
        }
    }

    /// Construct a filter scoped to a single program.
    pub fn program(cluster: ClusterKind, program_id: impl Into<String>) -> Self {
        Self {
            cluster,
            program_ids: vec![program_id.into()],
            include_decoded: true,
        }
    }

    /// Does this filter accept the supplied entry?
    pub fn matches(&self, entry: &LogEntry) -> bool {
        if entry.cluster != self.cluster {
            return false;
        }
        if self.program_ids.is_empty() {
            return true;
        }
        // Accept the entry if any of its invoked programs overlaps with
        // the filter's program_ids set. This lets a single-program
        // filter catch cross-program invocations where the target
        // program is the invoker OR the invokee.
        let wanted: BTreeSet<&str> = self.program_ids.iter().map(String::as_str).collect();
        entry
            .programs_invoked
            .iter()
            .any(|p| wanted.contains(p.as_str()))
    }
}

/// One transaction's worth of log output, cluster-tagged and decoded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub cluster: ClusterKind,
    pub signature: String,
    pub slot: Option<u64>,
    pub block_time_s: Option<i64>,
    /// The raw `logMessages` array from `getTransaction`.
    pub raw_logs: Vec<String>,
    /// All program ids invoked by the tx, in invocation order.
    pub programs_invoked: Vec<String>,
    /// IDL-decoded explanation (error variant names, CU accounting,
    /// summary string). Present even when the tx succeeded — the
    /// summary field is still useful for observability.
    pub explanation: Explanation,
    /// Anchor `emit!` events, already extracted from the raw logs.
    /// Empty when the program doesn't emit events or the logs contain no
    /// `Program data:` lines.
    pub anchor_events: Vec<AnchorEvent>,
    pub err: Option<serde_json::Value>,
    pub received_ms: u64,
}

impl LogEntry {
    pub fn decoded_logs(&self) -> &DecodedLogs {
        &self.explanation.decoded_logs
    }
}

/// Opaque subscription token — string so it serializes cleanly and
/// can't be confused with a u64 id from another module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct LogSubscriptionToken(pub String);

impl LogSubscriptionToken {
    fn new(id: u64) -> Self {
        Self(format!("log-sub-{id}"))
    }
}

/// Event sink the Tauri layer plugs into so `LogBus` broadcasts land on
/// the app event bus. Tests substitute a collecting sink so they can
/// assert the sequence of entries.
pub trait LogEventSink: Send + Sync + std::fmt::Debug {
    fn emit_raw(&self, token: &LogSubscriptionToken, entry: &LogEntry);
    fn emit_decoded(&self, token: &LogSubscriptionToken, entry: &LogEntry);
}

/// No-op sink, used when the bus runs without a Tauri `AppHandle`.
#[derive(Debug, Default, Clone)]
pub struct NullLogEventSink;

impl LogEventSink for NullLogEventSink {
    fn emit_raw(&self, _token: &LogSubscriptionToken, _entry: &LogEntry) {}
    fn emit_decoded(&self, _token: &LogSubscriptionToken, _entry: &LogEntry) {}
}

#[derive(Debug)]
struct Subscription {
    token: LogSubscriptionToken,
    filter: LogFilter,
    /// Bounded ring buffer per-subscription so the UI can render the
    /// most recent N entries on reconnect without re-fetching. Capped
    /// to keep memory usage predictable even for long sessions.
    ring: Mutex<Vec<LogEntry>>,
}

/// Central registry + broadcaster. Drop semantics are trivial — the
/// struct only owns data. Any background poller the Tauri layer spawns
/// must be stopped separately.
#[derive(Debug)]
pub struct LogBus {
    subscriptions: RwLock<HashMap<LogSubscriptionToken, Arc<Subscription>>>,
    next_id: AtomicU64,
    sink: RwLock<Arc<dyn LogEventSink>>,
    idl: Arc<IdlRegistry>,
    /// Global ring of entries (across every subscription) so
    /// `fetch_recent` + UI reconnect can reconstruct context even for
    /// tokens that were just created.
    global_ring: Mutex<Vec<LogEntry>>,
    ring_capacity: usize,
    subscription_backlog: usize,
}

impl LogBus {
    pub fn new(idl: Arc<IdlRegistry>) -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            sink: RwLock::new(Arc::new(NullLogEventSink) as Arc<dyn LogEventSink>),
            idl,
            global_ring: Mutex::new(Vec::with_capacity(DEFAULT_RING_CAPACITY)),
            ring_capacity: DEFAULT_RING_CAPACITY,
            subscription_backlog: DEFAULT_SUBSCRIPTION_BACKLOG,
        }
    }

    pub fn with_capacity(idl: Arc<IdlRegistry>, ring: usize, backlog: usize) -> Self {
        Self {
            ring_capacity: ring.max(1),
            subscription_backlog: backlog.max(1),
            ..Self::new(idl)
        }
    }

    /// Replace the event sink at runtime. Lets the Tauri layer bind an
    /// `AppHandle`-backed emitter after `SolanaState` construction.
    pub fn set_sink(&self, sink: Arc<dyn LogEventSink>) {
        *self.sink.write().expect("log sink lock poisoned") = sink;
    }

    pub fn subscribe(&self, filter: LogFilter) -> LogSubscriptionToken {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let token = LogSubscriptionToken::new(id);
        let subscription = Arc::new(Subscription {
            token: token.clone(),
            filter,
            ring: Mutex::new(Vec::with_capacity(self.subscription_backlog)),
        });
        self.subscriptions
            .write()
            .expect("log subscriptions lock poisoned")
            .insert(token.clone(), subscription);
        token
    }

    pub fn unsubscribe(&self, token: &LogSubscriptionToken) -> bool {
        self.subscriptions
            .write()
            .expect("log subscriptions lock poisoned")
            .remove(token)
            .is_some()
    }

    pub fn active_subscriptions(&self) -> Vec<(LogSubscriptionToken, LogFilter)> {
        self.subscriptions
            .read()
            .expect("log subscriptions lock poisoned")
            .values()
            .map(|s| (s.token.clone(), s.filter.clone()))
            .collect()
    }

    /// Re-broadcast the buffered entries for a subscription. Handy when
    /// the UI reconnects and needs to re-hydrate without re-fetching.
    pub fn replay(&self, token: &LogSubscriptionToken) -> Option<Vec<LogEntry>> {
        let guard = self
            .subscriptions
            .read()
            .expect("log subscriptions lock poisoned");
        let subscription = guard.get(token)?;
        let ring = subscription
            .ring
            .lock()
            .expect("log ring lock poisoned")
            .clone();
        Some(ring)
    }

    /// Pull the last N entries the bus has observed, filtered by the
    /// supplied filter. Does not mutate subscription rings. Used by
    /// `solana_logs_recent` so an agent can ask "all events from my
    /// program in the last 10 slots" without subscribing live.
    pub fn recent(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
        let guard = self.global_ring.lock().expect("log ring lock poisoned");
        let cap = limit.max(1);
        // Walk newest→oldest, keep the first `cap` that match, then
        // reverse so the returned slice is chronological. We collect
        // explicitly because `Filter<Rev<…>>` is not
        // `DoubleEndedIterator`.
        let mut out: Vec<LogEntry> = Vec::with_capacity(cap);
        for entry in guard.iter().rev() {
            if !filter.matches(entry) {
                continue;
            }
            out.push(entry.clone());
            if out.len() >= cap {
                break;
            }
        }
        out.reverse();
        out
    }

    /// Publish a pre-built `LogEntry` directly. Callers either build
    /// `LogEntry`s manually (scripted tests) or via `publish_raw`.
    pub fn publish_entry(&self, entry: LogEntry) {
        self.push_global(&entry);
        let subscribers = self
            .subscriptions
            .read()
            .expect("log subscriptions lock poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let sink = Arc::clone(&*self.sink.read().expect("log sink lock poisoned"));
        for subscription in subscribers {
            if !subscription.filter.matches(&entry) {
                continue;
            }
            self.push_subscription(&subscription, &entry);
            sink.emit_raw(&subscription.token, &entry);
            if subscription.filter.include_decoded
                && (!entry.explanation.decoded_logs.entries.is_empty()
                    || !entry.anchor_events.is_empty())
            {
                sink.emit_decoded(&subscription.token, &entry);
            }
        }
    }

    /// Decode + publish a raw tx log batch. This is the primary entry
    /// point for both the RPC source and the validator stdout source —
    /// both produce the same `RawLogBatch` shape.
    pub fn publish_raw(&self, batch: RawLogBatch) -> LogEntry {
        let entry = self.decode_batch(batch);
        self.publish_entry(entry.clone());
        entry
    }

    /// Build a `LogEntry` from raw inputs without publishing. Used by
    /// `RpcLogSource::fetch_recent` to produce a returnable batch that
    /// is also mirrored onto the bus.
    pub fn decode_batch(&self, batch: RawLogBatch) -> LogEntry {
        let idl_errors = decoder::build_idl_error_map(&self.idl, &batch.programs_hint());
        let explanation = explain_simulation(&batch.logs, batch.err.as_ref(), Some(&idl_errors));
        let programs_invoked = if explanation.affected_programs.is_empty() {
            batch.programs_hint()
        } else {
            explanation.affected_programs.clone()
        };
        let anchor_events = decoder::extract_anchor_events(&self.idl, &explanation.decoded_logs);
        LogEntry {
            cluster: batch.cluster,
            signature: batch.signature,
            slot: batch.slot,
            block_time_s: batch.block_time_s,
            raw_logs: batch.logs,
            programs_invoked,
            explanation,
            anchor_events,
            err: batch.err,
            received_ms: now_ms(),
        }
    }

    pub fn idl_registry(&self) -> Arc<IdlRegistry> {
        Arc::clone(&self.idl)
    }

    /// Diagnostic helper used in tests.
    #[cfg(test)]
    pub(crate) fn global_len(&self) -> usize {
        self.global_ring
            .lock()
            .expect("log ring lock poisoned")
            .len()
    }

    fn push_global(&self, entry: &LogEntry) {
        let mut guard = self.global_ring.lock().expect("log ring lock poisoned");
        if guard.len() >= self.ring_capacity {
            let overflow = guard.len() + 1 - self.ring_capacity;
            guard.drain(..overflow);
        }
        guard.push(entry.clone());
    }

    fn push_subscription(&self, subscription: &Subscription, entry: &LogEntry) {
        let mut guard = subscription
            .ring
            .lock()
            .expect("subscription ring lock poisoned");
        if guard.len() >= self.subscription_backlog {
            let overflow = guard.len() + 1 - self.subscription_backlog;
            guard.drain(..overflow);
        }
        guard.push(entry.clone());
    }
}

/// Input shape every log source produces. `programs_hint` lets the bus
/// enrich the decoder with IDL error maps even when the logs don't
/// include the full invocation trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawLogBatch {
    pub cluster: ClusterKind,
    pub signature: String,
    pub slot: Option<u64>,
    pub block_time_s: Option<i64>,
    pub logs: Vec<String>,
    #[serde(default)]
    pub err: Option<serde_json::Value>,
    /// Programs known to be touched by this tx — used as a fallback
    /// when the logs themselves don't trigger an `invoke` line (e.g.
    /// the tx failed before invocation).
    #[serde(default)]
    pub program_hint: Vec<String>,
}

impl RawLogBatch {
    pub fn new(cluster: ClusterKind, signature: impl Into<String>, logs: Vec<String>) -> Self {
        Self {
            cluster,
            signature: signature.into(),
            slot: None,
            block_time_s: None,
            logs,
            err: None,
            program_hint: Vec::new(),
        }
    }

    pub fn with_slot(mut self, slot: u64) -> Self {
        self.slot = Some(slot);
        self
    }

    pub fn with_program_hint(mut self, programs: Vec<String>) -> Self {
        self.program_hint = programs;
        self
    }

    pub fn with_err(mut self, err: serde_json::Value) -> Self {
        self.err = Some(err);
        self
    }

    fn programs_hint(&self) -> Vec<String> {
        self.program_hint.clone()
    }
}

/// Tauri-facing error for malformed caller input.
pub fn invalid_last_n(last: u64) -> CommandError {
    CommandError::user_fixable(
        "solana_logs_invalid_last_n",
        format!("last_n must be between 1 and 1024 (got {last})."),
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Collecting sink used in both unit and integration tests.
#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub struct CollectingLogSink {
    pub raw: Arc<Mutex<Vec<(LogSubscriptionToken, LogEntry)>>>,
    pub decoded: Arc<Mutex<Vec<(LogSubscriptionToken, LogEntry)>>>,
}

#[cfg(test)]
impl LogEventSink for CollectingLogSink {
    fn emit_raw(&self, token: &LogSubscriptionToken, entry: &LogEntry) {
        self.raw
            .lock()
            .unwrap()
            .push((token.clone(), entry.clone()));
    }
    fn emit_decoded(&self, token: &LogSubscriptionToken, entry: &LogEntry) {
        self.decoded
            .lock()
            .unwrap()
            .push((token.clone(), entry.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::idl::{FetchedIdl, IdlFetcher};
    use crate::commands::CommandResult;
    use serde_json::json;

    #[derive(Debug, Default)]
    struct NoopFetcher;

    impl IdlFetcher for NoopFetcher {
        fn fetch(
            &self,
            _cluster: ClusterKind,
            _rpc_url: &str,
            _program_id: &str,
        ) -> CommandResult<Option<FetchedIdl>> {
            Ok(None)
        }
    }

    fn sample_idl(program_id: &str) -> serde_json::Value {
        json!({
            "address": program_id,
            "metadata": { "name": "example", "version": "0.1.0" },
            "errors": [
                { "code": 6000, "name": "InvalidOwner", "msg": "not the owner" }
            ],
            "events": [
                { "name": "SwapEvent", "discriminator": [1,2,3,4,5,6,7,8] }
            ]
        })
    }

    fn make_bus() -> LogBus {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        LogBus::with_capacity(Arc::new(registry), 8, 4)
    }

    #[test]
    fn subscribe_and_receive_matching_entry() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        let token = bus.subscribe(LogFilter::program(ClusterKind::Localnet, "P111"));
        let batch = RawLogBatch::new(
            ClusterKind::Localnet,
            "sig-1",
            vec![
                "Program P111 invoke [1]".into(),
                "Program log: hello".into(),
                "Program P111 consumed 1234 of 200000 compute units".into(),
                "Program P111 success".into(),
            ],
        );
        bus.publish_raw(batch);

        let raw = sink.raw.lock().unwrap().clone();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].0, token);
        assert_eq!(raw[0].1.signature, "sig-1");
        assert!(raw[0].1.explanation.ok);
    }

    #[test]
    fn filter_excludes_entries_for_other_programs() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        bus.subscribe(LogFilter::program(ClusterKind::Localnet, "OnlyMe"));
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig-a",
            vec![
                "Program OtherProg invoke [1]".into(),
                "Program OtherProg success".into(),
            ],
        ));
        assert!(sink.raw.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_program_filter_matches_any_invocation() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        bus.subscribe(LogFilter::cluster(ClusterKind::Localnet));
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig-a",
            vec![
                "Program ProgX invoke [1]".into(),
                "Program ProgX success".into(),
            ],
        ));
        assert_eq!(sink.raw.lock().unwrap().len(), 1);
    }

    #[test]
    fn filter_excludes_entries_from_other_clusters() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        bus.subscribe(LogFilter::cluster(ClusterKind::Localnet));
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Devnet,
            "sig-a",
            vec![
                "Program ProgX invoke [1]".into(),
                "Program ProgX success".into(),
            ],
        ));
        assert!(sink.raw.lock().unwrap().is_empty());
    }

    #[test]
    fn include_decoded_false_skips_decoded_events() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        bus.subscribe(LogFilter {
            cluster: ClusterKind::Localnet,
            program_ids: vec![],
            include_decoded: false,
        });
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig-a",
            vec![
                "Program ProgX invoke [1]".into(),
                "Program ProgX success".into(),
            ],
        ));
        assert_eq!(sink.raw.lock().unwrap().len(), 1);
        assert!(sink.decoded.lock().unwrap().is_empty());
    }

    #[test]
    fn replay_returns_buffered_entries_for_subscription() {
        let bus = make_bus();
        let token = bus.subscribe(LogFilter::cluster(ClusterKind::Localnet));

        for i in 0..3 {
            bus.publish_raw(RawLogBatch::new(
                ClusterKind::Localnet,
                format!("sig-{i}"),
                vec![
                    "Program ProgX invoke [1]".into(),
                    "Program ProgX success".into(),
                ],
            ));
        }
        let entries = bus.replay(&token).expect("token still active");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].signature, "sig-0");
        assert_eq!(entries[2].signature, "sig-2");
    }

    #[test]
    fn subscription_ring_enforces_backlog_cap() {
        let bus = make_bus(); // backlog=4
        let token = bus.subscribe(LogFilter::cluster(ClusterKind::Localnet));
        for i in 0..10 {
            bus.publish_raw(RawLogBatch::new(
                ClusterKind::Localnet,
                format!("sig-{i}"),
                vec![
                    "Program ProgX invoke [1]".into(),
                    "Program ProgX success".into(),
                ],
            ));
        }
        let entries = bus.replay(&token).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].signature, "sig-6");
        assert_eq!(entries[3].signature, "sig-9");
    }

    #[test]
    fn global_ring_enforces_capacity() {
        let bus = make_bus(); // ring=8
        for i in 0..20 {
            bus.publish_raw(RawLogBatch::new(
                ClusterKind::Localnet,
                format!("sig-{i}"),
                vec![
                    "Program ProgX invoke [1]".into(),
                    "Program ProgX success".into(),
                ],
            ));
        }
        assert_eq!(bus.global_len(), 8);
    }

    #[test]
    fn recent_returns_last_n_matching_entries_in_order() {
        let bus = make_bus();
        for i in 0..5 {
            bus.publish_raw(RawLogBatch::new(
                ClusterKind::Localnet,
                format!("sig-{i}"),
                vec![
                    "Program Target invoke [1]".into(),
                    "Program Target success".into(),
                ],
            ));
        }
        // Interleave with an entry for a different program.
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig-other",
            vec![
                "Program Other invoke [1]".into(),
                "Program Other success".into(),
            ],
        ));

        let recent = bus.recent(&LogFilter::program(ClusterKind::Localnet, "Target"), 3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].signature, "sig-2");
        assert_eq!(recent[2].signature, "sig-4");
    }

    #[test]
    fn unsubscribe_stops_future_deliveries() {
        let bus = make_bus();
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        let token = bus.subscribe(LogFilter::cluster(ClusterKind::Localnet));
        assert!(bus.unsubscribe(&token));

        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig",
            vec!["Program X invoke [1]".into(), "Program X success".into()],
        ));
        assert!(sink.raw.lock().unwrap().is_empty());
    }

    #[test]
    fn unsubscribe_returns_false_for_unknown_token() {
        let bus = make_bus();
        assert!(!bus.unsubscribe(&LogSubscriptionToken("nope".into())));
    }

    #[test]
    fn decoded_entry_annotates_failure_with_idl_variant() {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        registry
            .load_value_for_tests(sample_idl("Prog111"))
            .unwrap();
        let bus = LogBus::with_capacity(Arc::new(registry), 8, 4);
        let sink = CollectingLogSink::default();
        bus.set_sink(Arc::new(sink.clone()) as Arc<dyn LogEventSink>);

        bus.subscribe(LogFilter::program(ClusterKind::Localnet, "Prog111"));
        bus.publish_raw(RawLogBatch::new(
            ClusterKind::Localnet,
            "sig",
            vec![
                "Program Prog111 invoke [1]".into(),
                "Program Prog111 failed: custom program error: 0x1770".into(),
            ],
        ));
        let raw = sink.raw.lock().unwrap().clone();
        assert_eq!(raw.len(), 1);
        let entry = &raw[0].1;
        assert!(!entry.explanation.ok);
        let err = entry
            .explanation
            .primary_error
            .as_ref()
            .expect("failure should be decoded");
        assert_eq!(err.idl_variant.as_deref(), Some("InvalidOwner"));
    }
}
