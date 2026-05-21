//! Doc-grounded agent prompts — Phase 9.
//!
//! For every Solana workbench tool we surface a short, versioned
//! snippet of the relevant Solana / Anchor / Metaplex / Squads / Jito
//! documentation. The agent catalog concatenates these snippets onto
//! each tool's description so the LLM is primed with authoritative
//! copy at decision time. We don't attempt to replace the upstream
//! docs — the snippets are scoped to the tool surface and include the
//! doc URL so the agent (or a human) can follow up.
//!
//! Keeping these in-tree rather than fetched at runtime is a
//! deliberate trade — docs move, we want reproducible agent behaviour
//! and offline operation. Updates to the snippets are reviewed like
//! any other code change.

use serde::{Deserialize, Serialize};

/// Versioned doc snippet for an agent tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocSnippet {
    /// Agent-tool name this snippet grounds (matches the
    /// `AUTONOMOUS_TOOL_SOLANA_*` constants).
    pub tool: String,
    /// Short title for UI surfacing.
    pub title: String,
    /// Canonical doc URL — injected into the agent context so the LLM
    /// can cite its source.
    pub reference_url: String,
    /// Monotonic version. Bumped whenever the content changes; shows
    /// up in agent telemetry.
    pub version: u32,
    /// The actual snippet. Plain markdown, short.
    pub body: String,
}

/// All built-in snippets. The catalog is a flat list — tools with
/// multiple relevant pages get multiple entries.
pub fn builtin_doc_catalog() -> Vec<DocSnippet> {
    vec![
        DocSnippet {
            tool: "solana_cluster".into(),
            title: "Cluster kinds".into(),
            reference_url: "https://docs.solanalabs.com/clusters/available".into(),
            version: 1,
            body: "Solana exposes four cluster kinds the workbench respects: localnet (single-node \
                   `solana-test-validator`), mainnet_fork (local clone of mainnet via surfpool or \
                   --clone), devnet (free remote with periodic resets), mainnet-beta (production). \
                   Only one validator runs locally at a time — starting a second one stops the \
                   first."
                .into(),
        },
        DocSnippet {
            tool: "solana_tx".into(),
            title: "Priority fees & compute budget".into(),
            reference_url:
                "https://docs.solana.com/developing/programming-model/runtime#compute-budget"
                    .into(),
            version: 1,
            body: "Every transaction has a compute-unit (CU) budget. Setting `ComputeBudgetInstruction::SetComputeUnitLimit` \
                   tightens it; `SetComputeUnitPrice` pays micro-lamports per CU as a priority fee. \
                   Simulate before sending — `simulateTransaction` returns `unitsConsumed` so you \
                   can size the CU limit precisely."
                .into(),
        },
        DocSnippet {
            tool: "solana_program".into(),
            title: "Program build & deploy".into(),
            reference_url: "https://docs.solanalabs.com/cli/examples/deploy-a-program".into(),
            version: 1,
            body: "BPF programs are built with `cargo build-sbf` (or `anchor build`). Deploy with \
                   `solana program deploy <.so>`. Upgrades redeploy against the same program id \
                   but must preserve the upgrade authority and fit inside the existing ProgramData \
                   allocation. Use `solana_program_upgrade_check` before deploying to mainnet."
                .into(),
        },
        DocSnippet {
            tool: "solana_deploy".into(),
            title: "Upgradeable loader & Squads".into(),
            reference_url: "https://docs.squads.so".into(),
            version: 1,
            body: "Mainnet deploys in the workbench route through a Squads v4 vault — the desktop \
                   app writes the program buffer but never signs the upgrade. Instead, a proposal \
                   is synthesised that Squads members approve in-app. Direct-keypair deploys are \
                   policy-denied for mainnet."
                .into(),
        },
        DocSnippet {
            tool: "solana_idl".into(),
            title: "Anchor IDL publishing".into(),
            reference_url: "https://www.anchor-lang.com/docs/idl".into(),
            version: 1,
            body: "Anchor ships an on-chain IDL account. `anchor idl init` writes it the first \
                   time; `anchor idl upgrade` replaces it thereafter. The workbench auto-picks \
                   based on whether the program has been deployed before."
                .into(),
        },
        DocSnippet {
            tool: "solana_codama".into(),
            title: "Codama client codegen".into(),
            reference_url: "https://github.com/codama-idl/codama".into(),
            version: 1,
            body: "Codama ingests an IDL and emits typed clients (TS, Rust, Umi). Re-run after \
                   every program build so frontend types stay in sync with account layouts and \
                   instruction discriminators."
                .into(),
        },
        DocSnippet {
            tool: "solana_pda".into(),
            title: "PDAs & canonical bumps".into(),
            reference_url:
                "https://docs.solana.com/developing/programming-model/calling-between-programs#program-derived-addresses"
                    .into(),
            version: 1,
            body: "Program-Derived Addresses (PDAs) are derived from `(program_id, seeds)` via \
                   `find_program_address`, which returns the canonical bump (highest bump yielding \
                   a valid address). Always store the bump on-chain and re-derive with \
                   `create_program_address` in handlers — never `find_program_address` inside an \
                   instruction (it's ~50x more CUs)."
                .into(),
        },
        DocSnippet {
            tool: "solana_audit_static".into(),
            title: "Anchor footgun checklist".into(),
            reference_url: "https://www.anchor-lang.com/docs/security".into(),
            version: 1,
            body: "The built-in static lints check for: missing `Signer`, missing `has_one`, \
                   unchecked `AccountInfo`, arithmetic overflow, realloc without rent resize, \
                   seed-spoof vectors, and non-canonical bump usage."
                .into(),
        },
        DocSnippet {
            tool: "solana_audit_fuzz".into(),
            title: "Trident fuzzing".into(),
            reference_url: "https://ackee.xyz/trident".into(),
            version: 1,
            body: "Trident generates fuzz harnesses from an Anchor IDL and drives them against \
                   the program under test. Longer runs find more paths; coverage delta is the \
                   signal that new input shapes are reaching new code."
                .into(),
        },
        DocSnippet {
            tool: "solana_replay".into(),
            title: "Exploit replay library".into(),
            reference_url: "https://github.com/coral-xyz/sealevel-attacks".into(),
            version: 1,
            body: "The replay library ships with reproducers for Wormhole sig-skip, Cashio \
                   fake-collateral, Mango oracle manipulation, and Nirvana flash-loan bugs. Replay \
                   runs against a forked snapshot at (or near) the exploit slot so the bad state \
                   is recreated deterministically."
                .into(),
        },
        DocSnippet {
            tool: "solana_secrets".into(),
            title: "Secret hygiene".into(),
            reference_url: "https://docs.solanalabs.com/cli/wallets/file-system".into(),
            version: 1,
            body: "Keypair JSON files (`id.json`, `*-keypair.json`) contain raw secret bytes. \
                   They must never be committed — scanner flags any occurrence as Critical. RPC \
                   API keys belong in env variables; Privy app secrets are server-side only."
                .into(),
        },
        DocSnippet {
            tool: "solana_cluster_drift".into(),
            title: "Cross-cluster drift".into(),
            reference_url: "https://developers.metaplex.com/token-metadata".into(),
            version: 1,
            body: "External programs (Metaplex Token Metadata, Jupiter, Squads, SPL Governance, \
                   Token-2022) are deployed separately per cluster. A hash mismatch between \
                   devnet and mainnet is the drift signal — the workbench compares on-chain \
                   ProgramData bytes and surfaces the difference before you ship."
                .into(),
        },
        DocSnippet {
            tool: "solana_cost".into(),
            title: "Cost snapshot".into(),
            reference_url: "https://docs.helius.dev/rpc".into(),
            version: 1,
            body: "Cost snapshot combines per-cluster tx count, lamports spent (base + priority \
                   fees), CUs consumed, and rent locked. Free-tier provider quotas are not exposed \
                   via public APIs — the workbench shows health pings and nudges you toward the \
                   provider dashboard for billing."
                .into(),
        },
        DocSnippet {
            tool: "solana_logs".into(),
            title: "RPC log subscriptions".into(),
            reference_url:
                "https://docs.solana.com/api/websocket#logssubscribe".into(),
            version: 1,
            body: "`logsSubscribe` streams per-signature logs. Combine with `getSignaturesForAddress` \
                   for backfill. The workbench decodes Anchor events from logs using the cached \
                   IDL."
                .into(),
        },
        DocSnippet {
            tool: "solana_indexer".into(),
            title: "Indexer scaffolds".into(),
            reference_url: "https://github.com/sevenlabs-hq/carbon".into(),
            version: 1,
            body: "Scaffold kinds: `carbon` (Rust, decoder-pipeline), `log_parser` (TS, standalone \
                   handler), `helius_webhook` (TS, webhook + DB write). Every scaffold is \
                   deterministic from the IDL; regenerate when the IDL changes."
                .into(),
        },
        DocSnippet {
            tool: "solana_token".into(),
            title: "Token-2022 extensions".into(),
            reference_url: "https://spl.solana.com/token-2022".into(),
            version: 1,
            body: "Token-2022 adds extensions (transfer hook, transfer fee, interest-bearing, \
                   metadata pointer, non-transferable) at mint-creation time. Not every wallet \
                   supports every extension — check the extension-matrix before committing to a \
                   config."
                .into(),
        },
    ]
}

/// Lookup helper — returns every snippet registered against the tool
/// name. Order follows `builtin_doc_catalog` for determinism.
pub fn snippets_for(tool: &str) -> Vec<DocSnippet> {
    builtin_doc_catalog()
        .into_iter()
        .filter(|s| s.tool == tool)
        .collect()
}
