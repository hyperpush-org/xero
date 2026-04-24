//! Jito bundle submission.
//!
//! Uses Jito's public block-engine RPC — `mainnet.block-engine.jito.wtf` —
//! which is free and does not require an API key. The bundle is a JSON
//! array of base58-encoded transactions; each transaction is signed and
//! fee-paid by the caller. The last tx in the bundle *must* include a
//! tip transfer to one of the Jito tip accounts (the pipeline helper
//! `tip_accounts()` returns the canonical list).
//!
//! We submit via `sendBundle` and return the bundle id. The caller then
//! polls `getBundleStatuses` to determine whether the bundle landed. The
//! transport trait is the same one the rest of Phase 3 uses so the
//! pipeline tests can script bundle submissions without hitting the real
//! block engine.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{CommandError, CommandResult};

use super::transport::{rpc_request, RpcTransport};

pub const JITO_DEFAULT_BLOCK_ENGINE_URL: &str =
    "https://mainnet.block-engine.jito.wtf/api/v1/bundles";

/// Canonical Jito tip accounts (mainnet). A bundle must transfer tip
/// lamports to one of these accounts in its tail tx or the block engine
/// rejects it. The list is stable per Jito's docs.
pub const TIP_ACCOUNTS: &[&str] = &[
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pivKeVBBXhjwvUhANTd",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

pub fn tip_accounts() -> &'static [&'static str] {
    TIP_ACCOUNTS
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BundleSubmission {
    pub bundle_id: String,
    pub block_engine_url: String,
    pub tx_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BundleStatus {
    pub bundle_id: String,
    pub status: String,
    pub landed_slot: Option<u64>,
    pub transactions: Vec<String>,
}

/// Submit a bundle of base58-encoded signed transactions. Jito expects
/// base58 specifically (not base64) as of block-engine v1.
pub fn submit_bundle(
    transport: &dyn RpcTransport,
    block_engine_url: &str,
    signed_base58_txs: &[String],
) -> CommandResult<BundleSubmission> {
    if signed_base58_txs.is_empty() {
        return Err(CommandError::user_fixable(
            "solana_jito_bundle_empty",
            "Cannot submit an empty bundle — include at least one signed transaction.",
        ));
    }
    if signed_base58_txs.len() > 5 {
        return Err(CommandError::user_fixable(
            "solana_jito_bundle_too_big",
            "Jito bundles are capped at 5 transactions.",
        ));
    }
    let body = rpc_request("sendBundle", json!([signed_base58_txs]));
    let response = transport.post(block_engine_url, body)?;
    let bundle_id = response
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            CommandError::retryable(
                "solana_jito_bundle_malformed",
                format!("Jito bundle response had no string result: {}", response),
            )
        })?
        .to_string();
    Ok(BundleSubmission {
        bundle_id,
        block_engine_url: block_engine_url.to_string(),
        tx_count: signed_base58_txs.len(),
    })
}

pub fn bundle_status(
    transport: &dyn RpcTransport,
    block_engine_url: &str,
    bundle_ids: &[String],
) -> CommandResult<Vec<BundleStatus>> {
    if bundle_ids.is_empty() {
        return Ok(Vec::new());
    }
    let body = rpc_request("getBundleStatuses", json!([bundle_ids]));
    let response = transport.post(block_engine_url, body)?;
    let value = response
        .pointer("/result/value")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let array = value.as_array().cloned().unwrap_or_default();
    let out = array
        .into_iter()
        .map(|entry| BundleStatus {
            bundle_id: entry
                .get("bundle_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: entry
                .get("confirmation_status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            landed_slot: entry.get("slot").and_then(|v| v.as_u64()),
            transactions: entry
                .get("transactions")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::tx::transport::test_support::ScriptedTransport;

    #[test]
    fn empty_bundle_is_user_fixable_error() {
        let transport = ScriptedTransport::new();
        let err = submit_bundle(&transport, JITO_DEFAULT_BLOCK_ENGINE_URL, &[]).unwrap_err();
        assert_eq!(err.code, "solana_jito_bundle_empty");
    }

    #[test]
    fn oversized_bundle_is_user_fixable_error() {
        let transport = ScriptedTransport::new();
        let txs: Vec<String> = (0..6).map(|i| format!("tx{i}")).collect();
        let err = submit_bundle(&transport, JITO_DEFAULT_BLOCK_ENGINE_URL, &txs).unwrap_err();
        assert_eq!(err.code, "solana_jito_bundle_too_big");
    }

    #[test]
    fn submit_returns_bundle_id_from_result() {
        let transport = ScriptedTransport::new();
        transport.set(
            JITO_DEFAULT_BLOCK_ENGINE_URL,
            "sendBundle",
            json!({"result": "bundle123"}),
        );
        let resp = submit_bundle(
            &transport,
            JITO_DEFAULT_BLOCK_ENGINE_URL,
            &["tx1".into(), "tx2".into()],
        )
        .unwrap();
        assert_eq!(resp.bundle_id, "bundle123");
        assert_eq!(resp.tx_count, 2);
    }

    #[test]
    fn submit_passes_tx_list_as_single_array_param() {
        let transport = ScriptedTransport::new();
        transport.set(
            JITO_DEFAULT_BLOCK_ENGINE_URL,
            "sendBundle",
            json!({"result": "b"}),
        );
        let _ = submit_bundle(&transport, JITO_DEFAULT_BLOCK_ENGINE_URL, &["tx1".into()]);
        let calls = transport.calls_for("sendBundle");
        assert_eq!(calls.len(), 1);
        // Jito expects params = [[tx1, tx2, ...]]
        assert!(calls[0].1[0].is_array());
        assert_eq!(calls[0].1[0][0], "tx1");
    }

    #[test]
    fn bundle_status_parses_result_values() {
        let transport = ScriptedTransport::new();
        transport.set(
            JITO_DEFAULT_BLOCK_ENGINE_URL,
            "getBundleStatuses",
            json!({
                "result": {
                    "value": [
                        {
                            "bundle_id": "b1",
                            "confirmation_status": "confirmed",
                            "slot": 12345,
                            "transactions": ["tx-a", "tx-b"]
                        }
                    ]
                }
            }),
        );
        let out = bundle_status(
            &transport,
            JITO_DEFAULT_BLOCK_ENGINE_URL,
            &["b1".to_string()],
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bundle_id, "b1");
        assert_eq!(out[0].status, "confirmed");
        assert_eq!(out[0].landed_slot, Some(12345));
        assert_eq!(out[0].transactions, vec!["tx-a", "tx-b"]);
    }

    #[test]
    fn bundle_status_with_empty_input_short_circuits() {
        let transport = ScriptedTransport::new();
        let out = bundle_status(&transport, JITO_DEFAULT_BLOCK_ENGINE_URL, &[]).unwrap();
        assert!(out.is_empty());
        assert!(transport.calls_for("getBundleStatuses").is_empty());
    }

    #[test]
    fn tip_accounts_are_fixed_list() {
        assert!(!tip_accounts().is_empty());
        assert!(tip_accounts().iter().all(|a| !a.is_empty()));
    }
}
