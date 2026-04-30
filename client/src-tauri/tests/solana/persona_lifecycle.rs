//! Phase 2 persona + scenario acceptance tests.
//!
//! Drives the `SolanaState` Arc-chain end-to-end using the mock-backed
//! fixture — no real validator, no real RPC, no real CLI binaries. The
//! plan's acceptance criteria for Phase 2 are:
//!
//! 1. Agent can create a "whale" persona with 10k SOL, 1M USDC, 3 NFTs on
//!    a fresh localnet in <5s.
//! 2. Persona mainnet keypair operations are blocked by policy; localnet
//!    import works.
//! 3. Scenario library is complete + runnable (self-contained scenarios
//!    execute; pipeline-required scenarios pre-stage and report pending).

use std::time::{Duration, Instant};

use xero_desktop_lib::commands::solana::{
    ClusterKind, FundingDelta, PersonaRole, PersonaSpec, ScenarioSpec, ScenarioStatus, StartOpts,
    TokenAllocation,
};

use super::support::fixture_with_persona_store;

fn whale_spec() -> PersonaSpec {
    serde_json::from_value(serde_json::json!({
        "name": "whale-1",
        "cluster": "localnet",
        "role": "whale",
        "seedOverride": null,
        "note": "phase-2 acceptance",
    }))
    .expect("whale spec parses")
}

pub fn whale_persona_created_under_budget_on_localnet() {
    let fixture = fixture_with_persona_store();
    let funding = fixture
        .funding
        .clone()
        .expect("persona fixture has funding");
    let supervisor = fixture.state.supervisor();
    supervisor
        .start(ClusterKind::Localnet, StartOpts::default())
        .expect("localnet should start under the scripted launcher");

    let deadline = Instant::now() + Duration::from_secs(5);
    let (persona, receipt) = fixture
        .state
        .personas()
        .create(whale_spec(), Some("http://127.0.0.1:8899".into()))
        .expect("whale persona should be created");

    assert!(
        Instant::now() < deadline,
        "phase-2 budget: <5s for whale creation"
    );

    // Every whale preset step should land cleanly on the mock backend.
    assert!(receipt.succeeded);
    // 1 airdrop + 3 tokens + 3 NFTs = 7 funding steps.
    assert_eq!(receipt.steps.len(), 7);
    assert_eq!(funding.airdrop_count(), 1);
    assert_eq!(funding.token_count(), 3);
    assert_eq!(funding.nft_count(), 3);

    let airdrops = funding.airdrops.lock().unwrap();
    assert_eq!(airdrops[0].1, 10_000 * 1_000_000_000);

    let tokens = funding.tokens.lock().unwrap();
    assert!(tokens.iter().any(|(persona_name, mint, amount)| {
        persona_name == "whale-1"
            && mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" // USDC
            && *amount == 1_000_000_000_000
    }));

    assert_eq!(persona.role, PersonaRole::Whale);
    assert_eq!(persona.cluster, ClusterKind::Localnet);
    assert!(!persona.pubkey.is_empty());
}

pub fn persona_mainnet_operations_are_policy_denied() {
    let fixture = fixture_with_persona_store();

    // Create is denied for mainnet no matter what role.
    let err = fixture
        .state
        .personas()
        .create(
            PersonaSpec {
                name: "mainnet-op".into(),
                cluster: ClusterKind::Mainnet,
                role: PersonaRole::NewUser,
                seed_override: None,
                note: None,
            },
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.class,
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied,
    );

    // Import keypair must be rejected on mainnet too.
    let provider = xero_desktop_lib::commands::solana::OsRngKeypairProvider;
    use xero_desktop_lib::commands::solana::persona::keygen::KeypairProvider;
    let bytes = provider.generate().unwrap();
    let err = fixture
        .state
        .personas()
        .import_keypair(
            ClusterKind::Mainnet,
            "should-fail",
            PersonaRole::Custom,
            bytes,
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.class,
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied,
    );
}

pub fn localnet_keypair_import_works() {
    let fixture = fixture_with_persona_store();
    let provider = xero_desktop_lib::commands::solana::OsRngKeypairProvider;
    use xero_desktop_lib::commands::solana::persona::keygen::KeypairProvider;
    let bytes = provider.generate().unwrap();
    let pubkey = bytes.pubkey_base58();

    let persona = fixture
        .state
        .personas()
        .import_keypair(
            ClusterKind::Localnet,
            "imported-dev-key",
            PersonaRole::Custom,
            bytes,
            Some("just for dev".into()),
        )
        .expect("localnet import should succeed");
    assert_eq!(persona.pubkey, pubkey);
    assert_eq!(persona.cluster, ClusterKind::Localnet);

    // Re-importing with the same name must error cleanly.
    let bytes2 = provider.generate().unwrap();
    let err = fixture
        .state
        .personas()
        .import_keypair(
            ClusterKind::Localnet,
            "imported-dev-key",
            PersonaRole::Custom,
            bytes2,
            None,
        )
        .unwrap_err();
    assert_eq!(err.code, "solana_persona_already_exists");
}

pub fn self_contained_scenario_runs_end_to_end() {
    let fixture = fixture_with_persona_store();
    fixture
        .state
        .supervisor()
        .start(ClusterKind::Localnet, StartOpts::default())
        .unwrap();

    fixture
        .state
        .personas()
        .create(
            PersonaSpec {
                name: "mint-caller".into(),
                cluster: ClusterKind::Localnet,
                role: PersonaRole::NewUser,
                seed_override: None,
                note: None,
            },
            None,
        )
        .unwrap();

    let run = fixture
        .state
        .scenarios()
        .run(ScenarioSpec {
            id: "metaplex_mint_list".into(),
            cluster: ClusterKind::Localnet,
            persona: "mint-caller".into(),
            params: serde_json::json!({ "count": 2, "collection": "acceptance" }),
        })
        .unwrap();

    assert_eq!(run.status, ScenarioStatus::Succeeded);
    // One airdrop step, two NFT mint steps.
    assert!(!run.signatures.is_empty());
    assert!(run.funding_receipts.iter().all(|r| r.succeeded));
}

pub fn pipeline_scenario_pre_stages_on_mainnet_fork() {
    let fixture = fixture_with_persona_store();
    let funding = fixture.funding.clone().unwrap();
    fixture
        .state
        .supervisor()
        .start(ClusterKind::MainnetFork, StartOpts::default())
        .unwrap();

    fixture
        .state
        .personas()
        .create(
            PersonaSpec {
                name: "whaley".into(),
                cluster: ClusterKind::MainnetFork,
                role: PersonaRole::Whale,
                seed_override: None,
                note: None,
            },
            None,
        )
        .unwrap();

    let before_tokens = funding.token_count();
    let run = fixture
        .state
        .scenarios()
        .run(ScenarioSpec {
            id: "swap_jupiter".into(),
            cluster: ClusterKind::MainnetFork,
            persona: "whaley".into(),
            params: serde_json::Value::Null,
        })
        .unwrap();

    assert_eq!(run.status, ScenarioStatus::PendingPipeline);
    assert!(run.pipeline_hint.is_some());
    // Pre-stage must at least have funded some tokens (the Jupiter
    // scenario's required USDC + mSOL allocation).
    assert!(funding.token_count() > before_tokens);
}

pub fn fund_command_rejects_empty_delta() {
    let fixture = fixture_with_persona_store();
    fixture
        .state
        .personas()
        .create(
            PersonaSpec {
                name: "fund-me".into(),
                cluster: ClusterKind::Localnet,
                role: PersonaRole::NewUser,
                seed_override: None,
                note: None,
            },
            None,
        )
        .unwrap();

    let empty = FundingDelta::default();
    let result = fixture.state.personas().fund(
        ClusterKind::Localnet,
        "fund-me",
        &empty,
        "http://127.0.0.1:8899",
    );
    // The store itself is happy to run an empty delta (no-op) — but the
    // Tauri command layer rejects it for ergonomics. Here we only assert
    // the store accepts it and produces an empty successful receipt.
    let receipt = result.unwrap();
    assert!(receipt.succeeded);
    assert!(receipt.steps.is_empty());

    // An explicit delta should still land cleanly.
    let delta = FundingDelta {
        sol_lamports: 100,
        tokens: vec![TokenAllocation::by_symbol("USDC", 50)],
        nfts: vec![],
    };
    let receipt = fixture
        .state
        .personas()
        .fund(
            ClusterKind::Localnet,
            "fund-me",
            &delta,
            "http://127.0.0.1:8899",
        )
        .unwrap();
    assert!(receipt.succeeded);
    assert_eq!(receipt.steps.len(), 2);
}
