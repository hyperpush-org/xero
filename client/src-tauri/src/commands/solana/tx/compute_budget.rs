//! Compute-budget helpers.
//!
//! The `ComputeBudget111…` program has a fixed, well-documented binary
//! instruction layout, so we can emit the bytes directly without pulling
//! solana-sdk into the workspace. This lets the pipeline prepend a
//! `SetComputeUnitLimit` + `SetComputeUnitPrice` pair to any tx the
//! caller ships, based on the simulation-reported CU usage and the
//! priority-fee oracle's recommendation.
//!
//! Layout (discriminators are from `solana_sdk::compute_budget::ComputeBudgetInstruction`):
//!   0x00 RequestUnits (legacy; deprecated — we don't emit it)
//!   0x01 RequestHeapFrame(u32)
//!   0x02 SetComputeUnitLimit(u32)
//!   0x03 SetComputeUnitPrice(u64)
//!   0x04 SetLoadedAccountsDataSizeLimit(u32)

use serde::{Deserialize, Serialize};

pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";

/// Max CU limit the runtime will accept per tx (see `compute_budget.rs`
/// in solana-sdk). Past this value validators reject the tx outright.
pub const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// Floor for auto-tuned limits so a no-op sim doesn't leave us with 0 CU.
pub const MIN_COMPUTE_UNIT_LIMIT: u32 = 10_000;

/// Default safety margin applied on top of the simulated CU. Most programs
/// fluctuate within 5–10% between simulation and landed execution; 1.15
/// covers both and still leaves headroom before MAX_COMPUTE_UNIT_LIMIT.
pub const CU_HEADROOM: f64 = 1.15;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComputeBudgetPlan {
    pub compute_unit_limit: Option<u32>,
    pub compute_unit_price_micro_lamports: Option<u64>,
    pub rationale: String,
}

/// Build a `ComputeBudgetPlan` from a simulation report + fee recommendation.
pub fn auto_tune(
    simulated_cu: Option<u64>,
    recommended_micro_lamports: Option<u64>,
) -> ComputeBudgetPlan {
    let limit = simulated_cu.map(|cu| {
        let padded = (cu as f64 * CU_HEADROOM).ceil() as u64;
        let capped = padded.min(MAX_COMPUTE_UNIT_LIMIT as u64);
        let floored = capped.max(MIN_COMPUTE_UNIT_LIMIT as u64);
        floored as u32
    });
    let rationale = match (simulated_cu, recommended_micro_lamports) {
        (Some(cu), Some(price)) => format!(
            "simulated {cu} CU → limit {} CU ({}x headroom); priority {} µ-lamports/CU",
            limit.unwrap_or(0),
            CU_HEADROOM,
            price,
        ),
        (Some(cu), None) => format!(
            "simulated {cu} CU → limit {} CU; no priority-fee sample available",
            limit.unwrap_or(0)
        ),
        (None, Some(price)) => format!(
            "no simulation; priority {} µ-lamports/CU, default limit",
            price
        ),
        (None, None) => "no simulation or priority-fee sample; caller must supply both".to_string(),
    };
    ComputeBudgetPlan {
        compute_unit_limit: limit,
        compute_unit_price_micro_lamports: recommended_micro_lamports,
        rationale,
    }
}

/// Encode a `SetComputeUnitLimit(u32)` instruction's raw data bytes.
pub fn encode_set_compute_unit_limit(units: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.push(0x02);
    out.extend_from_slice(&units.to_le_bytes());
    out
}

/// Encode a `SetComputeUnitPrice(u64)` instruction's raw data bytes. The
/// value is interpreted as micro-lamports per CU.
pub fn encode_set_compute_unit_price(micro_lamports: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(0x03);
    out.extend_from_slice(&micro_lamports.to_le_bytes());
    out
}

/// Convenience: encode both of the typical ComputeBudget instructions the
/// pipeline prepends. Returns `(program_id, limit_bytes, price_bytes)`.
pub fn encode_plan(plan: &ComputeBudgetPlan) -> Vec<(&'static str, Vec<u8>)> {
    let mut out = Vec::with_capacity(2);
    if let Some(limit) = plan.compute_unit_limit {
        out.push((
            COMPUTE_BUDGET_PROGRAM_ID,
            encode_set_compute_unit_limit(limit),
        ));
    }
    if let Some(price) = plan.compute_unit_price_micro_lamports {
        out.push((
            COMPUTE_BUDGET_PROGRAM_ID,
            encode_set_compute_unit_price(price),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_compute_unit_limit_bytes_have_variant_0x02() {
        let bytes = encode_set_compute_unit_limit(200_000);
        assert_eq!(bytes[0], 0x02);
        assert_eq!(bytes.len(), 5);
        assert_eq!(u32::from_le_bytes(bytes[1..5].try_into().unwrap()), 200_000);
    }

    #[test]
    fn set_compute_unit_price_bytes_have_variant_0x03() {
        let bytes = encode_set_compute_unit_price(1_000);
        assert_eq!(bytes[0], 0x03);
        assert_eq!(bytes.len(), 9);
        assert_eq!(u64::from_le_bytes(bytes[1..9].try_into().unwrap()), 1_000);
    }

    #[test]
    fn auto_tune_applies_headroom_and_floor() {
        let plan = auto_tune(Some(100), Some(500));
        assert_eq!(plan.compute_unit_limit, Some(MIN_COMPUTE_UNIT_LIMIT));
        assert_eq!(plan.compute_unit_price_micro_lamports, Some(500));
    }

    #[test]
    fn auto_tune_caps_at_hard_limit() {
        let plan = auto_tune(Some(10_000_000), None);
        assert_eq!(plan.compute_unit_limit, Some(MAX_COMPUTE_UNIT_LIMIT));
    }

    #[test]
    fn auto_tune_without_sim_leaves_limit_unset() {
        let plan = auto_tune(None, Some(123));
        assert_eq!(plan.compute_unit_limit, None);
        assert_eq!(plan.compute_unit_price_micro_lamports, Some(123));
    }

    #[test]
    fn encode_plan_emits_two_instructions_when_both_fields_set() {
        let plan = auto_tune(Some(300_000), Some(42));
        let encoded = encode_plan(&plan);
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[0].0, COMPUTE_BUDGET_PROGRAM_ID);
        assert_eq!(encoded[0].1[0], 0x02);
        assert_eq!(encoded[1].1[0], 0x03);
    }

    #[test]
    fn encode_plan_emits_nothing_when_plan_is_empty() {
        let plan = auto_tune(None, None);
        assert!(encode_plan(&plan).is_empty());
    }

    #[test]
    fn headroom_multiplies_cu_before_cap() {
        // 1_200_000 × 1.15 = 1_380_000, under MAX.
        let plan = auto_tune(Some(1_200_000), None);
        assert_eq!(plan.compute_unit_limit, Some(1_380_000));
    }
}
