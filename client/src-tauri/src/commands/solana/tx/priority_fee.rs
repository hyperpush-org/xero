//! Priority-fee oracle.
//!
//! Uses `getRecentPrioritizationFees` — available on every validator,
//! including the public mainnet RPC and the free Helius / Triton tiers —
//! so the workbench's default path never needs a paid key. When program
//! ids are supplied we narrow the sample to txs that touched those
//! programs; otherwise we take a global sample.
//!
//! The oracle returns percentile-aligned suggestions plus the raw sample
//! so the caller can show its own distribution. Suggestions are bucketed
//! by `SamplePercentile` (low/median/high/very_high/max) to keep the
//! agent's decision space small.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{CommandError, CommandResult};

use super::transport::{rpc_request, RpcTransport};

/// Percentile buckets the oracle returns. Keep this enum stable — the
/// autonomous runtime's tool schema references it verbatim.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SamplePercentile {
    Low,      // p25
    Median,   // p50
    High,     // p75
    VeryHigh, // p90
    Max,      // p99
}

impl SamplePercentile {
    pub fn as_fraction(self) -> f64 {
        match self {
            SamplePercentile::Low => 0.25,
            SamplePercentile::Median => 0.50,
            SamplePercentile::High => 0.75,
            SamplePercentile::VeryHigh => 0.90,
            SamplePercentile::Max => 0.99,
        }
    }
}

/// Individual sample. `prioritization_fee` is in micro-lamports per CU —
/// the same unit `SetComputeUnitPrice` expects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeeSample {
    pub slot: u64,
    pub prioritization_fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimate {
    pub samples: Vec<FeeSample>,
    pub percentiles: Vec<PercentileFee>,
    /// Recommended micro-lamports/CU price for a typical landing target.
    pub recommended_micro_lamports: u64,
    /// The percentile we picked for the recommended value.
    pub recommended_percentile: SamplePercentile,
    pub program_ids: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PercentileFee {
    pub percentile: SamplePercentile,
    pub micro_lamports: u64,
}

/// Fetch + summarize recent prioritization fees from `rpc_url`. An empty
/// `program_ids` slice asks the network for a global sample; otherwise
/// the RPC narrows to txs that touched the listed programs.
pub fn estimate_priority_fee(
    transport: &dyn RpcTransport,
    rpc_url: &str,
    program_ids: &[String],
    target: SamplePercentile,
) -> CommandResult<FeeEstimate> {
    let params = if program_ids.is_empty() {
        json!([])
    } else {
        json!([program_ids])
    };
    let body = rpc_request("getRecentPrioritizationFees", params);
    let response = transport.post(rpc_url, body)?;
    let result = response
        .get("result")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let samples = parse_samples(&result)?;
    Ok(summarize(samples, program_ids, target))
}

fn parse_samples(value: &Value) -> CommandResult<Vec<FeeSample>> {
    let array = value.as_array().ok_or_else(|| {
        CommandError::retryable(
            "solana_priority_fee_malformed_result",
            "getRecentPrioritizationFees returned a non-array result.",
        )
    })?;
    let mut out = Vec::with_capacity(array.len());
    for entry in array {
        let slot = entry.get("slot").and_then(|v| v.as_u64());
        let fee = entry.get("prioritizationFee").and_then(|v| v.as_u64());
        if let (Some(slot), Some(fee)) = (slot, fee) {
            out.push(FeeSample {
                slot,
                prioritization_fee: fee,
            });
        }
    }
    Ok(out)
}

fn summarize(
    mut samples: Vec<FeeSample>,
    program_ids: &[String],
    target: SamplePercentile,
) -> FeeEstimate {
    samples.sort_by_key(|s| s.slot);

    let mut fees: Vec<u64> = samples.iter().map(|s| s.prioritization_fee).collect();
    fees.sort_unstable();

    let percentiles = [
        SamplePercentile::Low,
        SamplePercentile::Median,
        SamplePercentile::High,
        SamplePercentile::VeryHigh,
        SamplePercentile::Max,
    ]
    .into_iter()
    .map(|p| PercentileFee {
        percentile: p,
        micro_lamports: percentile(&fees, p.as_fraction()),
    })
    .collect::<Vec<_>>();

    let recommended = percentiles
        .iter()
        .find(|p| p.percentile == target)
        .map(|p| p.micro_lamports)
        .unwrap_or(0);

    FeeEstimate {
        samples,
        percentiles,
        recommended_micro_lamports: recommended,
        recommended_percentile: target,
        program_ids: program_ids.to_vec(),
        source: "getRecentPrioritizationFees".to_string(),
    }
}

fn percentile(sorted_fees: &[u64], fraction: f64) -> u64 {
    if sorted_fees.is_empty() {
        return 0;
    }
    let idx = ((sorted_fees.len() as f64 - 1.0) * fraction).round() as usize;
    sorted_fees[idx.min(sorted_fees.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::solana::tx::transport::test_support::ScriptedTransport;

    fn samples_value(samples: &[(u64, u64)]) -> Value {
        let arr: Vec<Value> = samples
            .iter()
            .map(|(slot, fee)| {
                json!({
                    "slot": slot,
                    "prioritizationFee": fee
                })
            })
            .collect();
        json!({"jsonrpc": "2.0", "id": 1, "result": arr})
    }

    #[test]
    fn empty_sample_returns_zero_recommendation() {
        let transport = ScriptedTransport::new();
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            json!({"result": []}),
        );
        let out =
            estimate_priority_fee(&transport, "http://rpc.test", &[], SamplePercentile::Median)
                .unwrap();
        assert_eq!(out.recommended_micro_lamports, 0);
        assert_eq!(out.samples.len(), 0);
        assert_eq!(out.percentiles.len(), 5);
    }

    #[test]
    fn percentile_picks_the_right_bucket() {
        let transport = ScriptedTransport::new();
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            samples_value(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]),
        );
        let out = estimate_priority_fee(&transport, "http://rpc.test", &[], SamplePercentile::High)
            .unwrap();
        // p75 of 1..=5 → index 3 → 4
        assert_eq!(out.recommended_micro_lamports, 4);
        assert_eq!(out.percentiles[0].micro_lamports, 2); // p25
        assert_eq!(out.percentiles[1].micro_lamports, 3); // p50
        assert_eq!(out.percentiles[2].micro_lamports, 4); // p75
    }

    #[test]
    fn program_ids_are_forwarded_as_params() {
        let transport = ScriptedTransport::new();
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            samples_value(&[(1, 100)]),
        );
        let _ = estimate_priority_fee(
            &transport,
            "http://rpc.test",
            &["JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string()],
            SamplePercentile::Median,
        )
        .unwrap();
        let calls = transport.calls_for("getRecentPrioritizationFees");
        assert_eq!(calls.len(), 1);
        // Params must be [[<program>]], not [<program>], per Solana RPC.
        let params = &calls[0].1;
        assert_eq!(params[0][0], "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB");
    }

    #[test]
    fn malformed_entries_are_skipped_not_fatal() {
        let transport = ScriptedTransport::new();
        transport.set(
            "http://rpc.test",
            "getRecentPrioritizationFees",
            json!({"result": [
                {"slot": 1, "prioritizationFee": 10},
                {"slot": 2}, // missing fee
                {"foo": "bar"},
                {"slot": 3, "prioritizationFee": 30}
            ]}),
        );
        let out =
            estimate_priority_fee(&transport, "http://rpc.test", &[], SamplePercentile::Median)
                .unwrap();
        assert_eq!(out.samples.len(), 2);
    }
}
