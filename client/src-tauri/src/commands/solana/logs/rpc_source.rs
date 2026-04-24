//! RPC-backed log source.
//!
//! The primary way the workbench pulls recent logs for a program is
//! `getSignaturesForAddress` → `getTransaction` (one call per
//! signature). This is deliberately simpler than subscribing to
//! `logsSubscribe` over WebSocket because:
//!
//! - The same code path works for mainnet/devnet (read-only, remote)
//!   as for localnet (read-only, local). Free-tier endpoints all
//!   support `getSignaturesForAddress`, not all support websockets.
//! - The acceptance criterion — "all events from my program in the
//!   last N slots" — is naturally a polling query, not a firehose.
//! - The `RpcTransport` trait is already mocked for the entire Phase 3
//!   test matrix, so reusing it means no new mock plumbing.
//!
//! The live-subscription story falls out of this: the Tauri command
//! layer wraps `fetch_recent` in a polling thread with a configurable
//! interval when the caller invokes `solana_logs_subscribe`.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::tx::transport::{rpc_request, RpcTransport};
use crate::commands::{CommandError, CommandResult};

use super::{LogBus, RawLogBatch};

/// Stream of raw-log batches for one (cluster, program_id) pair. Kept
/// as a trait so tests can short-circuit RPC with canned responses.
pub trait RpcLogSource: Send + Sync + std::fmt::Debug {
    /// Fetch the last `limit` transactions' logs for `program_id`.
    ///
    /// Implementations must not panic on transport errors — returning
    /// an empty vec when the RPC is unreachable lets the Tauri command
    /// layer decide whether to surface the error.
    fn fetch_recent(
        &self,
        cluster: ClusterKind,
        rpc_url: &str,
        program_id: &str,
        limit: u32,
    ) -> CommandResult<Vec<RawLogBatch>>;

    /// Optional override: fetch logs for multiple programs, deduping
    /// signatures across them. The default impl calls `fetch_recent`
    /// once per program and merges.
    fn fetch_recent_many(
        &self,
        cluster: ClusterKind,
        rpc_url: &str,
        program_ids: &[String],
        limit: u32,
    ) -> CommandResult<Vec<RawLogBatch>> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out: Vec<RawLogBatch> = Vec::new();
        for pid in program_ids {
            let batches = self.fetch_recent(cluster, rpc_url, pid, limit)?;
            for batch in batches {
                if seen.insert(batch.signature.clone()) {
                    out.push(batch);
                }
            }
        }
        // Keep batches in ascending slot order so downstream consumers
        // (log bus, indexer replay) process transactions chronologically.
        out.sort_by(|a, b| {
            a.slot
                .unwrap_or(0)
                .cmp(&b.slot.unwrap_or(0))
                .then_with(|| a.signature.cmp(&b.signature))
        });
        Ok(out)
    }
}

/// Production implementation of `RpcLogSource` backed by the shared
/// `RpcTransport`. Holds no state apart from the transport handle.
#[derive(Debug)]
pub struct SystemRpcLogSource {
    transport: Arc<dyn RpcTransport>,
}

impl SystemRpcLogSource {
    pub fn new(transport: Arc<dyn RpcTransport>) -> Self {
        Self { transport }
    }

    fn fetch_signatures(
        &self,
        rpc_url: &str,
        program_id: &str,
        limit: u32,
    ) -> CommandResult<Vec<SignatureRecord>> {
        let capped = limit.clamp(1, 1_000);
        let config = json!({
            "limit": capped,
            "commitment": "confirmed",
        });
        let body = rpc_request("getSignaturesForAddress", json!([program_id, config]));
        let response = self.transport.post(rpc_url, body)?;
        let arr = response
            .get("result")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .filter_map(|entry| {
                let sig = entry.get("signature").and_then(|v| v.as_str())?.to_string();
                let slot = entry.get("slot").and_then(|v| v.as_u64());
                let block_time = entry.get("blockTime").and_then(|v| v.as_i64());
                let err = entry.get("err").cloned().filter(|v| !v.is_null());
                Some(SignatureRecord {
                    signature: sig,
                    slot,
                    block_time_s: block_time,
                    err,
                })
            })
            .collect())
    }

    fn fetch_transaction(
        &self,
        rpc_url: &str,
        signature: &str,
    ) -> CommandResult<Option<TransactionLogs>> {
        let config = json!({
            "encoding": "json",
            "commitment": "confirmed",
            "maxSupportedTransactionVersion": 0,
        });
        let body = rpc_request("getTransaction", json!([signature, config]));
        let response = self.transport.post(rpc_url, body)?;
        let result = match response.get("result") {
            Some(r) if !r.is_null() => r.clone(),
            _ => return Ok(None),
        };
        let slot = result.get("slot").and_then(|v| v.as_u64());
        let block_time_s = result.get("blockTime").and_then(|v| v.as_i64());
        let meta = result.get("meta").cloned().unwrap_or(Value::Null);
        let err = meta.get("err").cloned().filter(|v| !v.is_null());
        let logs = meta
            .get("logMessages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        Ok(Some(TransactionLogs {
            slot,
            block_time_s,
            logs,
            err,
        }))
    }
}

impl RpcLogSource for SystemRpcLogSource {
    fn fetch_recent(
        &self,
        cluster: ClusterKind,
        rpc_url: &str,
        program_id: &str,
        limit: u32,
    ) -> CommandResult<Vec<RawLogBatch>> {
        if program_id.is_empty() {
            return Err(CommandError::user_fixable(
                "solana_logs_program_required",
                "program_id is required for RPC log fetch.",
            ));
        }
        let signatures = self.fetch_signatures(rpc_url, program_id, limit)?;
        let mut batches = Vec::with_capacity(signatures.len());
        for record in signatures {
            let tx = self.fetch_transaction(rpc_url, &record.signature)?;
            let Some(tx) = tx else { continue };
            let mut batch = RawLogBatch::new(cluster, &record.signature, tx.logs)
                .with_program_hint(vec![program_id.to_string()]);
            if let Some(slot) = tx.slot.or(record.slot) {
                batch = batch.with_slot(slot);
            }
            batch.block_time_s = tx.block_time_s.or(record.block_time_s);
            if let Some(err) = tx.err.or(record.err) {
                batch = batch.with_err(err);
            }
            batches.push(batch);
        }
        Ok(batches)
    }
}

#[derive(Debug, Clone)]
struct SignatureRecord {
    signature: String,
    slot: Option<u64>,
    block_time_s: Option<i64>,
    err: Option<Value>,
}

#[derive(Debug, Clone)]
struct TransactionLogs {
    slot: Option<u64>,
    block_time_s: Option<i64>,
    logs: Vec<String>,
    err: Option<Value>,
}

/// Convenience: run a recent fetch for each program id and mirror the
/// decoded entries onto the bus. Returns the decoded entries so the
/// caller can also pass them directly to the Tauri response.
pub fn fetch_recent_and_publish(
    source: &dyn RpcLogSource,
    bus: &LogBus,
    cluster: ClusterKind,
    rpc_url: &str,
    program_ids: &[String],
    limit: u32,
) -> CommandResult<Vec<super::LogEntry>> {
    let batches = source.fetch_recent_many(cluster, rpc_url, program_ids, limit)?;
    let mut entries = Vec::with_capacity(batches.len());
    for batch in batches {
        let entry = bus.publish_raw(batch);
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::idl::{FetchedIdl, IdlFetcher, IdlRegistry};
    use crate::commands::solana::tx::transport::test_support::ScriptedTransport;
    use std::sync::Mutex;

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

    fn bus() -> LogBus {
        LogBus::new(Arc::new(IdlRegistry::new(Arc::new(NoopFetcher))))
    }

    #[test]
    fn fetch_recent_pulls_signatures_then_transactions() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getSignaturesForAddress",
            json!({
                "result": [
                    {"signature": "sig-1", "slot": 10, "blockTime": 1_000, "err": null},
                    {"signature": "sig-2", "slot": 11, "blockTime": 1_001, "err": null}
                ]
            }),
        );
        transport.queue(
            "http://rpc.test",
            "getTransaction",
            json!({
                "result": {
                    "slot": 10,
                    "blockTime": 1_000,
                    "meta": {
                        "err": null,
                        "logMessages": [
                            "Program Target invoke [1]",
                            "Program Target success"
                        ]
                    }
                }
            }),
        );
        transport.queue(
            "http://rpc.test",
            "getTransaction",
            json!({
                "result": {
                    "slot": 11,
                    "blockTime": 1_001,
                    "meta": {
                        "err": null,
                        "logMessages": [
                            "Program Target invoke [1]",
                            "Program Target success"
                        ]
                    }
                }
            }),
        );

        let source = SystemRpcLogSource::new(transport as Arc<dyn RpcTransport>);
        let batches = source
            .fetch_recent(ClusterKind::Localnet, "http://rpc.test", "Target", 10)
            .unwrap();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].signature, "sig-1");
        assert_eq!(batches[0].slot, Some(10));
        assert_eq!(batches[1].signature, "sig-2");
    }

    #[test]
    fn fetch_recent_many_dedupes_across_programs() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getSignaturesForAddress",
            json!({
                "result": [
                    {"signature": "shared-sig", "slot": 42, "blockTime": 5_000, "err": null}
                ]
            }),
        );
        transport.set(
            "http://rpc.test",
            "getTransaction",
            json!({
                "result": {
                    "slot": 42,
                    "blockTime": 5_000,
                    "meta": {
                        "err": null,
                        "logMessages": [
                            "Program A invoke [1]",
                            "Program B invoke [2]",
                            "Program B success",
                            "Program A success"
                        ]
                    }
                }
            }),
        );
        let source = SystemRpcLogSource::new(transport as Arc<dyn RpcTransport>);
        let batches = source
            .fetch_recent_many(
                ClusterKind::Localnet,
                "http://rpc.test",
                &["A".to_string(), "B".to_string()],
                10,
            )
            .unwrap();
        assert_eq!(batches.len(), 1, "shared signature should not dupe");
        assert_eq!(batches[0].signature, "shared-sig");
    }

    #[test]
    fn fetch_recent_refuses_empty_program_id() {
        let transport = Arc::new(ScriptedTransport::new());
        let source = SystemRpcLogSource::new(transport as Arc<dyn RpcTransport>);
        let err = source
            .fetch_recent(ClusterKind::Localnet, "http://rpc.test", "", 5)
            .unwrap_err();
        assert_eq!(err.code, "solana_logs_program_required");
    }

    #[test]
    fn fetch_recent_skips_missing_transaction_results() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getSignaturesForAddress",
            json!({"result": [{"signature": "missing", "slot": 1}]}),
        );
        transport.set("http://rpc.test", "getTransaction", json!({"result": null}));
        let source = SystemRpcLogSource::new(transport as Arc<dyn RpcTransport>);
        let batches = source
            .fetch_recent(ClusterKind::Localnet, "http://rpc.test", "Target", 10)
            .unwrap();
        assert!(batches.is_empty());
    }

    #[derive(Debug, Default)]
    struct RecordingSource {
        batches: Mutex<Vec<RawLogBatch>>,
    }

    impl RpcLogSource for RecordingSource {
        fn fetch_recent(
            &self,
            cluster: ClusterKind,
            _rpc_url: &str,
            program_id: &str,
            _limit: u32,
        ) -> CommandResult<Vec<RawLogBatch>> {
            let batches = vec![
                RawLogBatch::new(
                    cluster,
                    "sig-a",
                    vec![
                        format!("Program {program_id} invoke [1]"),
                        format!("Program {program_id} success"),
                    ],
                )
                .with_slot(1)
                .with_program_hint(vec![program_id.to_string()]),
                RawLogBatch::new(
                    cluster,
                    "sig-b",
                    vec![
                        format!("Program {program_id} invoke [1]"),
                        format!("Program {program_id} success"),
                    ],
                )
                .with_slot(2)
                .with_program_hint(vec![program_id.to_string()]),
            ];
            let mut mirror = batches.clone();
            self.batches.lock().unwrap().append(&mut mirror);
            Ok(batches)
        }
    }

    #[test]
    fn fetch_recent_and_publish_mirrors_entries_onto_bus() {
        let source = RecordingSource::default();
        let bus = bus();
        let entries = fetch_recent_and_publish(
            &source,
            &bus,
            ClusterKind::Localnet,
            "http://rpc.test",
            &["Prog".to_string()],
            3,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        let recent = bus.recent(&super::super::LogFilter::cluster(ClusterKind::Localnet), 10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].signature, "sig-a");
        assert_eq!(recent[1].signature, "sig-b");
    }
}
