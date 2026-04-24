//! On-chain IDL fetcher.
//!
//! Anchor stores an IDL on-chain behind a deterministic address:
//! 1. `seeds_signer = Pubkey::find_program_address(&[], program_id).0`
//! 2. `idl_address = Pubkey::create_with_seed(seeds_signer, "anchor:idl", program_id)`
//!    which is `SHA-256(seeds_signer || "anchor:idl" || program_id)`.
//!
//! The account data layout is:
//! - `[0..8]`   — Anchor account discriminator (first 8 bytes of the
//!                SHA-256 of the fully-qualified `IdlAccount` type name).
//! - `[8..40]`  — authority pubkey (32 bytes).
//! - `[40..44]` — data length (u32 LE).
//! - `[44..44+data_len]` — zlib-compressed IDL JSON.
//!
//! The fetcher:
//! - Computes the IDL address via the Solana CLI shell-out (keeps the
//!   curve25519 arithmetic out of our code path — we already have a
//!   production PDA derivation in `pda::derive`, so we reuse that).
//! - Calls `getAccountInfo` through the `RpcTransport`.
//! - Base64-decodes, validates the header, zlib-inflates the body.
//!
//! Shelling out to `anchor idl fetch` is an alternative, but (a) the user
//! may not have anchor installed and (b) `anchor idl fetch` writes to a
//! temp file instead of stdout in some versions, making the capture
//! brittle. The in-process path is smaller and more robust for a
//! read-only operation.

use std::io::Read;
use std::sync::Arc;

use base64::Engine as _;
use flate2::read::ZlibDecoder;
use serde_json::{json, Value};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::solana::pda;
use crate::commands::solana::tx::RpcTransport;
use crate::commands::{CommandError, CommandResult};

use super::{FetchedIdl, IdlFetcher};

#[derive(Debug)]
pub struct RpcIdlFetcher {
    transport: Arc<dyn RpcTransport>,
}

impl RpcIdlFetcher {
    pub fn new(transport: Arc<dyn RpcTransport>) -> Self {
        Self { transport }
    }

    fn idl_address(&self, program_id: &str) -> CommandResult<String> {
        // 1. `seeds_signer = find_program_address(&[], program_id)` → base
        let base = pda::find_program_address(program_id, &[])?;
        // 2. `idl_address = SHA256(base || "anchor:idl" || program_id)`
        //    Using Pubkey::create_with_seed semantics.
        pda::create_with_seed(&base.pubkey, "anchor:idl", program_id)
    }
}

impl IdlFetcher for RpcIdlFetcher {
    fn fetch(
        &self,
        _cluster: ClusterKind,
        rpc_url: &str,
        program_id: &str,
    ) -> CommandResult<Option<FetchedIdl>> {
        let idl_address = self.idl_address(program_id)?;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                idl_address,
                {"encoding": "base64", "commitment": "confirmed"}
            ],
        });
        let response = self.transport.post(rpc_url, body)?;
        let value = response
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null);
        if value.is_null() {
            return Ok(None);
        }
        let data_tuple = value
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                CommandError::system_fault(
                    "solana_idl_bad_account_shape",
                    "getAccountInfo returned data in an unexpected shape.",
                )
            })?;
        let encoded = data_tuple.first().and_then(|v| v.as_str()).ok_or_else(|| {
            CommandError::system_fault(
                "solana_idl_bad_account_data",
                "getAccountInfo data array was empty.",
            )
        })?;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|err| {
                CommandError::system_fault(
                    "solana_idl_base64_decode_failed",
                    format!("Could not base64-decode IDL account: {err}"),
                )
            })?;
        if raw.len() < 44 {
            return Err(CommandError::user_fixable(
                "solana_idl_account_too_small",
                format!(
                    "IDL account at {idl_address} is {} bytes; expected at least 44 for the Anchor header.",
                    raw.len()
                ),
            ));
        }
        let data_len = u32::from_le_bytes([raw[40], raw[41], raw[42], raw[43]]) as usize;
        let zlib_end = 44usize.checked_add(data_len).ok_or_else(|| {
            CommandError::system_fault(
                "solana_idl_bad_data_len",
                "IDL account data_len overflows usize.",
            )
        })?;
        if zlib_end > raw.len() {
            return Err(CommandError::user_fixable(
                "solana_idl_truncated",
                format!(
                    "IDL account data_len={data_len} exceeds account size {}",
                    raw.len()
                ),
            ));
        }
        let mut decoder = ZlibDecoder::new(&raw[44..zlib_end]);
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).map_err(|err| {
            CommandError::user_fixable(
                "solana_idl_decompress_failed",
                format!("Could not zlib-inflate IDL account: {err}"),
            )
        })?;
        let value: Value = serde_json::from_slice(&decoded).map_err(|err| {
            CommandError::user_fixable(
                "solana_idl_parse_failed",
                format!("IDL bytes from {idl_address} are not valid JSON: {err}"),
            )
        })?;
        Ok(Some(FetchedIdl { value, idl_address }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::tx::transport::test_support::ScriptedTransport;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn build_idl_account(program_id: &str, payload: &[u8]) -> String {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(payload).unwrap();
        let compressed = enc.finish().unwrap();

        let mut out = Vec::new();
        out.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]); // discriminator
        out.extend_from_slice(&[0u8; 32]); // authority (pubkey)
        let data_len = compressed.len() as u32;
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&compressed);
        let _ = program_id; // arg kept for symmetry with the real layout
        base64::engine::general_purpose::STANDARD.encode(&out)
    }

    #[test]
    fn fetch_decodes_and_inflates_idl_payload() {
        let transport = Arc::new(ScriptedTransport::new());
        let idl_json = r#"{"metadata":{"name":"my_program","address":"Prog11111111111111111111111111111111111111"}}"#;
        let account_b64 = build_idl_account(
            "Prog11111111111111111111111111111111111111",
            idl_json.as_bytes(),
        );
        transport.set(
            "http://rpc.test",
            "getAccountInfo",
            json!({
                "result": {
                    "value": {
                        "data": [account_b64, "base64"],
                        "executable": false,
                        "lamports": 0,
                        "owner": "SomeOwner111",
                        "rentEpoch": 0
                    }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let fetcher = RpcIdlFetcher::new(transport_dyn);
        let got = fetcher
            .fetch(
                ClusterKind::Devnet,
                "http://rpc.test",
                "Prog11111111111111111111111111111111111111",
            )
            .unwrap()
            .expect("payload should decode");
        assert_eq!(
            got.value.pointer("/metadata/name").and_then(|v| v.as_str()),
            Some("my_program")
        );
    }

    #[test]
    fn fetch_returns_none_when_account_absent() {
        let transport = Arc::new(ScriptedTransport::new());
        transport.set(
            "http://rpc.test",
            "getAccountInfo",
            json!({"result": {"value": null}}),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let fetcher = RpcIdlFetcher::new(transport_dyn);
        let got = fetcher
            .fetch(
                ClusterKind::Devnet,
                "http://rpc.test",
                "Prog11111111111111111111111111111111111111",
            )
            .unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn fetch_errors_on_truncated_account() {
        let transport = Arc::new(ScriptedTransport::new());
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 10]);
        transport.set(
            "http://rpc.test",
            "getAccountInfo",
            json!({
                "result": {
                    "value": { "data": [short, "base64"], "executable": false, "lamports": 0, "owner": "x", "rentEpoch": 0 }
                }
            }),
        );
        let transport_dyn: Arc<dyn RpcTransport> = transport.clone();
        let fetcher = RpcIdlFetcher::new(transport_dyn);
        let err = fetcher
            .fetch(
                ClusterKind::Devnet,
                "http://rpc.test",
                "Prog11111111111111111111111111111111111111",
            )
            .unwrap_err();
        assert_eq!(err.code, "solana_idl_account_too_small");
    }
}
