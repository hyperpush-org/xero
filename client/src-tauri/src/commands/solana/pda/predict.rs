//! Cross-cluster PDA prediction.
//!
//! Most Solana programs deploy to the same program id across every
//! cluster, so the derived PDA is the same everywhere. This helper is
//! still useful — the agent wants concrete per-cluster addresses in the
//! response without having to re-derive in the caller — and it sets up
//! the deliberate shape we'll extend in Phase 5 when deploy may use a
//! different program id on devnet vs mainnet.

use serde::{Deserialize, Serialize};

use crate::commands::solana::cluster::ClusterKind;
use crate::commands::CommandResult;

use super::{find_program_address, SeedPart};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterPda {
    pub cluster: ClusterKind,
    pub pubkey: String,
    pub bump: u8,
    pub program_id: String,
}

pub fn predict_cross_cluster(
    program_id: &str,
    seeds: &[SeedPart],
    clusters: &[ClusterKind],
) -> CommandResult<Vec<ClusterPda>> {
    let derived = find_program_address(program_id, seeds)?;
    let mut out = Vec::with_capacity(clusters.len());
    for cluster in clusters {
        out.push(ClusterPda {
            cluster: *cluster,
            pubkey: derived.pubkey.clone(),
            bump: derived.bump,
            program_id: derived.program_id.clone(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALLET: &str = "CuieVDEDtLo7FypA9SbLM9saXFdb1dsshEkyErMqkRQq";
    const ATA_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

    #[test]
    fn cross_cluster_prediction_mirrors_single_address() {
        let seeds = [SeedPart::Pubkey(WALLET.into())];
        let clusters = [
            ClusterKind::Localnet,
            ClusterKind::MainnetFork,
            ClusterKind::Devnet,
            ClusterKind::Mainnet,
        ];
        let predictions = predict_cross_cluster(ATA_PROGRAM, &seeds, &clusters).unwrap();
        assert_eq!(predictions.len(), 4);
        let pubkey = predictions[0].pubkey.clone();
        for p in &predictions {
            assert_eq!(p.pubkey, pubkey);
        }
    }
}
