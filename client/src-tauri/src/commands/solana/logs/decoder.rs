//! IDL-driven enrichment for log entries.
//!
//! Two concerns:
//! 1. Build an `IdlErrorMap` for every program we have a cached IDL for,
//!    so the tx-decoder annotates `custom program error: 0x<code>` with
//!    the Anchor error variant name.
//! 2. Extract Anchor `emit!` events from `Program data: <base64>` lines.
//!    Without pulling in `borsh`/`anchor-lang` we only match the
//!    8-byte discriminator prefix against the IDL's declared events —
//!    the payload bytes are returned verbatim for the caller to decode
//!    in a language of its choice (the TS client knows how).

use std::collections::BTreeMap;
use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::super::idl::IdlRegistry;
use super::super::tx::decoder::{DecodedLogEntry, DecodedLogs, IdlErrorMap};

/// Event emitted via Anchor's `emit!`, extracted from a
/// `Program data: <base64>` line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorEvent {
    pub program_id: String,
    pub event_name: Option<String>,
    /// 8-byte discriminator hex, lowercase.
    pub discriminator_hex: String,
    pub payload_base64: String,
    pub payload_bytes_len: u32,
}

/// Trait used by the indexer dev runner to turn a batch of raw logs
/// into decoded `AnchorEvent`s. Kept small and stateless so the
/// indexer can instantiate it from the IDL registry handle.
pub trait LogDecoder: Send + Sync + std::fmt::Debug {
    fn events(&self, decoded_logs: &DecodedLogs) -> Vec<AnchorEvent>;
    fn error_map(&self, program_ids: &[String]) -> IdlErrorMap;
}

/// IDL-registry-backed log decoder.
#[derive(Debug, Clone)]
pub struct EntryDecoder {
    registry: Arc<IdlRegistry>,
}

impl EntryDecoder {
    pub fn new(registry: Arc<IdlRegistry>) -> Self {
        Self { registry }
    }
}

impl LogDecoder for EntryDecoder {
    fn events(&self, decoded_logs: &DecodedLogs) -> Vec<AnchorEvent> {
        extract_anchor_events(&self.registry, decoded_logs)
    }

    fn error_map(&self, program_ids: &[String]) -> IdlErrorMap {
        build_idl_error_map(&self.registry, program_ids)
    }
}

/// Build the Anchor-style error map for the programs we have IDLs for.
/// Programs without a cached IDL are silently omitted — the log
/// decoder already falls back to `program_id + code` when a variant is
/// missing.
pub fn build_idl_error_map(registry: &IdlRegistry, program_ids: &[String]) -> IdlErrorMap {
    let mut out: IdlErrorMap = BTreeMap::new();
    let seen: std::collections::BTreeSet<&str> = program_ids.iter().map(|s| s.as_str()).collect();

    // Walk the registry cache once; include both the explicitly-requested
    // program ids and any cached IDL where the IDL embeds its program id
    // (so the agent gets decoding even when the caller didn't supply a
    // hint list).
    for (_key, idl) in registry.cache_entries() {
        let pid = match idl.program_id() {
            Some(p) => p,
            None => continue,
        };
        if !seen.is_empty() && !seen.contains(pid.as_str()) {
            continue;
        }
        let entries = match idl.value.get("errors").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };
        let mut map: BTreeMap<u32, String> = BTreeMap::new();
        for entry in entries {
            let code = entry.get("code").and_then(|v| v.as_u64());
            let name = entry.get("name").and_then(|v| v.as_str());
            if let (Some(code), Some(name)) = (code, name) {
                map.insert(code as u32, name.to_string());
            }
        }
        if !map.is_empty() {
            out.insert(pid, map);
        }
    }
    out
}

/// Scan decoded logs for `Program data:` entries and surface the
/// matching Anchor event name (when the discriminator matches an IDL
/// event for the invoking program).
pub fn extract_anchor_events(
    registry: &IdlRegistry,
    decoded_logs: &DecodedLogs,
) -> Vec<AnchorEvent> {
    let engine = base64::engine::general_purpose::STANDARD;
    let mut out = Vec::new();
    for entry in &decoded_logs.entries {
        let (program_id_opt, payload_base64) = match entry {
            DecodedLogEntry::Data { program_id, base64 } => (program_id.clone(), base64.clone()),
            _ => continue,
        };
        let program_id = match program_id_opt {
            Some(p) => p,
            None => continue,
        };
        let bytes = match engine.decode(payload_base64.as_bytes()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.len() < 8 {
            continue;
        }
        let discriminator = &bytes[..8];
        let discriminator_hex = discriminator_hex(discriminator);
        let event_name = lookup_event_name(registry, &program_id, discriminator);
        out.push(AnchorEvent {
            program_id,
            event_name,
            discriminator_hex,
            payload_base64,
            payload_bytes_len: bytes.len() as u32,
        });
    }
    out
}

fn lookup_event_name(
    registry: &IdlRegistry,
    program_id: &str,
    discriminator: &[u8],
) -> Option<String> {
    let idls = registry.cache_entries();
    for (_key, idl) in idls {
        let pid = idl.program_id();
        if pid.as_deref() != Some(program_id) {
            continue;
        }
        let events = idl.value.get("events").and_then(|v| v.as_array())?;
        for event in events {
            let disc = event
                .get("discriminator")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|n| n.as_u64().map(|u| u as u8))
                        .collect::<Vec<u8>>()
                })
                .unwrap_or_default();
            if disc.len() == 8 && disc == discriminator {
                if let Some(name) = event.get("name").and_then(|v| v.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn discriminator_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::cluster::ClusterKind;
    use crate::commands::solana::idl::{FetchedIdl, IdlFetcher};
    use crate::commands::solana::tx::decoder::decode_logs;
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

    fn seeded_registry(program_id: &str) -> IdlRegistry {
        let registry = IdlRegistry::new(Arc::new(NoopFetcher));
        registry
            .load_value_for_tests(json!({
                "address": program_id,
                "metadata": { "name": "example" },
                "errors": [
                    { "code": 6000, "name": "InvalidOwner", "msg": "owner mismatch" },
                    { "code": 6001, "name": "StaleAccount", "msg": "stale" }
                ],
                "events": [
                    { "name": "SwapEvent", "discriminator": [1,2,3,4,5,6,7,8] },
                    { "name": "TickEvent", "discriminator": [9,8,7,6,5,4,3,2] }
                ]
            }))
            .unwrap();
        registry
    }

    #[test]
    fn error_map_includes_programs_in_hint() {
        let registry = seeded_registry("Prog11");
        let map = build_idl_error_map(&registry, &["Prog11".into()]);
        let inner = map.get("Prog11").expect("entry present");
        assert_eq!(inner.get(&6000).unwrap(), "InvalidOwner");
        assert_eq!(inner.get(&6001).unwrap(), "StaleAccount");
    }

    #[test]
    fn error_map_skips_programs_not_in_hint_when_hint_is_specific() {
        let registry = seeded_registry("Prog11");
        let map = build_idl_error_map(&registry, &["Different".into()]);
        assert!(map.is_empty());
    }

    #[test]
    fn extract_anchor_events_finds_matching_discriminator() {
        let registry = seeded_registry("Prog11");
        let payload =
            base64::engine::general_purpose::STANDARD.encode([1, 2, 3, 4, 5, 6, 7, 8, 42, 0, 0, 0]);
        let logs = vec![
            "Program Prog11 invoke [1]".to_string(),
            format!("Program data: {payload}"),
            "Program Prog11 success".to_string(),
        ];
        let decoded = decode_logs(&logs, None);
        let events = extract_anchor_events(&registry, &decoded);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name.as_deref(), Some("SwapEvent"));
        assert_eq!(events[0].program_id, "Prog11");
        assert_eq!(events[0].payload_bytes_len, 12);
    }

    #[test]
    fn extract_anchor_events_falls_back_to_unknown_when_no_match() {
        let registry = seeded_registry("Prog11");
        let payload =
            base64::engine::general_purpose::STANDARD.encode([99, 99, 99, 99, 99, 99, 99, 99]);
        let logs = vec![
            "Program Prog11 invoke [1]".to_string(),
            format!("Program data: {payload}"),
            "Program Prog11 success".to_string(),
        ];
        let decoded = decode_logs(&logs, None);
        let events = extract_anchor_events(&registry, &decoded);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, None);
        assert_eq!(events[0].discriminator_hex, "6363636363636363");
    }

    #[test]
    fn extract_anchor_events_ignores_unparseable_data() {
        let registry = seeded_registry("Prog11");
        let logs = vec![
            "Program Prog11 invoke [1]".to_string(),
            "Program data: *** not base64 ***".to_string(),
            "Program Prog11 success".to_string(),
        ];
        let decoded = decode_logs(&logs, None);
        let events = extract_anchor_events(&registry, &decoded);
        assert!(events.is_empty());
    }

    #[test]
    fn extract_anchor_events_skips_short_payloads() {
        let registry = seeded_registry("Prog11");
        let payload = base64::engine::general_purpose::STANDARD.encode([1, 2, 3]);
        let logs = vec![
            "Program Prog11 invoke [1]".to_string(),
            format!("Program data: {payload}"),
            "Program Prog11 success".to_string(),
        ];
        let decoded = decode_logs(&logs, None);
        let events = extract_anchor_events(&registry, &decoded);
        assert!(events.is_empty());
    }
}
