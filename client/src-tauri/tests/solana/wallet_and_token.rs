//! Phase 8 integration tests — token create, Metaplex mint, wallet
//! scaffolds.
//!
//! Mirrors the `audit_engine.rs` / `persona_lifecycle.rs` layout —
//! scripted runners on top of a `SolanaState` fixture so none of the
//! Solana toolchain has to be installed.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cadence_desktop_lib::commands::solana::persona::fund::FundingDelta;
use cadence_desktop_lib::commands::solana::persona::roles::PersonaRole;
use cadence_desktop_lib::commands::solana::toolchain::{ToolProbe, ToolchainStatus};
use cadence_desktop_lib::commands::solana::{
    self, create_token, generate_wallet_scaffold, mint_metaplex_nft, token_extension_matrix,
    wallet_descriptors, ClusterKind, MetaplexMintInvocation, MetaplexMintOutcome,
    MetaplexMintRequest, MetaplexMintRunner, MetaplexStandard, PersonaSpec, TokenCreateInvocation,
    TokenCreateOutcome, TokenCreateRunner, TokenCreateSpec, TokenExtension, TokenExtensionConfig,
    TokenProgram, TokenServices, WalletKind, WalletScaffoldRequest,
};
use cadence_desktop_lib::commands::CommandResult;

use super::support::{fixture_with_persona_store, FixtureState};

#[derive(Debug, Default)]
struct CapturingTokenRunner {
    calls: Mutex<Vec<TokenCreateInvocation>>,
    outcome: Mutex<Option<TokenCreateOutcome>>,
}

impl CapturingTokenRunner {
    fn new(outcome: TokenCreateOutcome) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            outcome: Mutex::new(Some(outcome)),
        }
    }

    fn captured(&self) -> Vec<TokenCreateInvocation> {
        self.calls.lock().unwrap().clone()
    }
}

impl TokenCreateRunner for CapturingTokenRunner {
    fn run(&self, invocation: &TokenCreateInvocation) -> CommandResult<TokenCreateOutcome> {
        self.calls.lock().unwrap().push(invocation.clone());
        Ok(self.outcome.lock().unwrap().clone().unwrap())
    }
}

#[derive(Debug, Default)]
struct CapturingMetaplexRunner {
    calls: Mutex<Vec<MetaplexMintInvocation>>,
    outcome: Mutex<Option<MetaplexMintOutcome>>,
}

impl CapturingMetaplexRunner {
    fn new(outcome: MetaplexMintOutcome) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            outcome: Mutex::new(Some(outcome)),
        }
    }

    fn captured(&self) -> Vec<MetaplexMintInvocation> {
        self.calls.lock().unwrap().clone()
    }
}

impl MetaplexMintRunner for CapturingMetaplexRunner {
    fn run(&self, invocation: &MetaplexMintInvocation) -> CommandResult<MetaplexMintOutcome> {
        self.calls.lock().unwrap().push(invocation.clone());
        Ok(self.outcome.lock().unwrap().clone().unwrap())
    }
}

fn fixture_with_whale() -> (FixtureState, String) {
    let fixture = fixture_with_persona_store();
    // Create a whale persona on localnet so the token / metaplex
    // commands can resolve a keypair path.
    let (persona, _receipt) = fixture
        .state
        .personas()
        .create(
            PersonaSpec {
                name: "whale".into(),
                cluster: ClusterKind::Localnet,
                role: PersonaRole::Whale,
                seed_override: Some(FundingDelta::default()),
                note: None,
            },
            None,
        )
        .expect("persona create should succeed against the mock funding backend");
    let path = persona.keypair_path.clone();
    (fixture, path)
}

// ---------- Token extension matrix ----------------------------------------

pub fn extension_matrix_flags_transfer_hook_on_wallet_adapter() {
    let matrix = token_extension_matrix();
    let hits = matrix.incompatibilities(&[TokenExtension::TransferHook]);
    let wallet_adapter_row = hits
        .iter()
        .find(|row| row.sdk.contains("wallet-adapter"))
        .expect("transfer_hook must flag @solana/wallet-adapter");
    assert!(!wallet_adapter_row.remediation_hint.is_empty());
    assert!(matches!(
        wallet_adapter_row.support_level,
        solana::SupportLevel::Unsupported | solana::SupportLevel::Partial,
    ));
}

// ---------- Token create --------------------------------------------------

pub fn token_create_argv_preserves_transfer_fee_config() {
    let (fixture, keypair_path) = fixture_with_whale();
    let runner = Arc::new(CapturingTokenRunner::new(TokenCreateOutcome {
        exit_code: Some(0),
        success: true,
        stdout: "Creating token 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b".into(),
        stderr: String::new(),
        mint_address: None,
    }));
    let services: Arc<TokenServices> = Arc::new(TokenServices {
        token: Arc::clone(&runner) as Arc<dyn TokenCreateRunner>,
        metaplex: Arc::new(cadence_desktop_lib::commands::solana::SystemMetaplexRunner::new()),
    });
    let state = fixture.state.with_token_services(services);

    let mut spec = TokenCreateSpec {
        cluster: ClusterKind::Localnet,
        program: TokenProgram::SplToken2022,
        authority_persona: "whale".into(),
        decimals: 6,
        mint_keypair_path: None,
        extensions: vec![TokenExtension::TransferFee],
        config: TokenExtensionConfig {
            transfer_fee_basis_points: Some(42),
            transfer_fee_maximum: Some(1_000_000),
            ..Default::default()
        },
        spl_token_cli: None,
        rpc_url: Some("http://127.0.0.1:8899".into()),
    };
    // Sanity: ensure the keypair we'll reference via persona exists.
    assert!(Path::new(&keypair_path).is_file());

    let services = state.token_services();
    let authority = state
        .personas()
        .keypair_path(spec.cluster, &spec.authority_persona)
        .expect("keypair path");
    spec.rpc_url = spec.rpc_url.or_else(|| state.resolve_rpc_url(spec.cluster));
    let report = create_token(services.token.as_ref(), &authority, spec).expect("create_token");
    assert!(report.success);
    assert!(report.argv.iter().any(|a| a == "--program-2022"));
    assert!(report.argv.iter().any(|a| a == "--transfer-fee"));
    assert!(report.argv.iter().any(|a| a == "42"));
    assert!(report.argv.iter().any(|a| a == "1000000"));
    assert_eq!(
        report.mint_address.as_deref(),
        Some("4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b")
    );
    // Runner was called exactly once with our argv.
    let captured = runner.captured();
    assert_eq!(captured.len(), 1);
    let program_name = Path::new(&captured[0].argv[0])
        .file_name()
        .and_then(|name| name.to_str());
    assert_eq!(program_name, Some("spl-token"));
}

pub fn token_create_reports_transfer_hook_incompatibilities() {
    let (fixture, _keypair) = fixture_with_whale();
    let runner = Arc::new(CapturingTokenRunner::new(TokenCreateOutcome {
        exit_code: Some(0),
        success: true,
        stdout: "Creating token 4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b".into(),
        stderr: String::new(),
        mint_address: None,
    }));
    let services: Arc<TokenServices> = Arc::new(TokenServices {
        token: Arc::clone(&runner) as Arc<dyn TokenCreateRunner>,
        metaplex: Arc::new(cadence_desktop_lib::commands::solana::SystemMetaplexRunner::new()),
    });
    let state = fixture.state.with_token_services(services);
    let authority = state
        .personas()
        .keypair_path(ClusterKind::Localnet, "whale")
        .unwrap();
    let spec = TokenCreateSpec {
        cluster: ClusterKind::Localnet,
        program: TokenProgram::SplToken2022,
        authority_persona: "whale".into(),
        decimals: 6,
        mint_keypair_path: None,
        extensions: vec![TokenExtension::TransferHook],
        config: TokenExtensionConfig {
            transfer_hook_program_id: Some("HookPr0g111111111111111111111111111111111111".into()),
            ..Default::default()
        },
        spl_token_cli: None,
        rpc_url: Some("http://127.0.0.1:8899".into()),
    };
    let report = create_token(state.token_services().token.as_ref(), &authority, spec)
        .expect("create_token");
    assert!(
        !report.incompatibilities.is_empty(),
        "transfer_hook should always produce at least one incompatibility row"
    );
    assert!(report
        .incompatibilities
        .iter()
        .any(|row| row.sdk.contains("wallet-adapter") && !row.remediation_hint.is_empty()));
}

pub fn token_create_rejects_extensions_on_classic_program() {
    let (fixture, _keypair) = fixture_with_whale();
    let runner = Arc::new(CapturingTokenRunner::new(TokenCreateOutcome {
        exit_code: Some(0),
        success: true,
        stdout: String::new(),
        stderr: String::new(),
        mint_address: None,
    }));
    let services: Arc<TokenServices> = Arc::new(TokenServices {
        token: Arc::clone(&runner) as Arc<dyn TokenCreateRunner>,
        metaplex: Arc::new(cadence_desktop_lib::commands::solana::SystemMetaplexRunner::new()),
    });
    let state = fixture.state.with_token_services(services);
    let authority = state
        .personas()
        .keypair_path(ClusterKind::Localnet, "whale")
        .unwrap();
    let spec = TokenCreateSpec {
        cluster: ClusterKind::Localnet,
        program: TokenProgram::Spl,
        authority_persona: "whale".into(),
        decimals: 6,
        mint_keypair_path: None,
        extensions: vec![TokenExtension::TransferFee],
        config: TokenExtensionConfig {
            transfer_fee_basis_points: Some(25),
            transfer_fee_maximum: Some(1_000),
            ..Default::default()
        },
        spl_token_cli: None,
        rpc_url: Some("http://127.0.0.1:8899".into()),
    };
    let err = create_token(state.token_services().token.as_ref(), &authority, spec)
        .expect_err("extensions on classic SPL must be rejected");
    assert_eq!(
        err.code,
        "solana_token_create_extensions_require_token_2022"
    );
    // Runner should not have been invoked.
    assert!(runner.captured().is_empty());
}

// ---------- Metaplex mint -------------------------------------------------

pub fn metaplex_mint_materialises_worker_and_passes_env() {
    let (fixture, _keypair) = fixture_with_whale();
    let runner = Arc::new(CapturingMetaplexRunner::new(MetaplexMintOutcome {
        exit_code: Some(0),
        success: true,
        stdout:
            "CADENCE_MINT_RESULT {\"mint\":\"4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b\",\"signature\":\"SigXYZ\"}"
                .into(),
        stderr: String::new(),
        mint_address: None,
        signature: None,
    }));
    let services: Arc<TokenServices> = Arc::new(TokenServices {
        token: Arc::new(cadence_desktop_lib::commands::solana::SystemTokenCreateRunner::new()),
        metaplex: Arc::clone(&runner) as Arc<dyn MetaplexMintRunner>,
    });
    let tmp = tempfile::TempDir::new().unwrap();
    let state = fixture
        .state
        .with_token_services(services)
        .with_metaplex_worker_root(tmp.path().to_path_buf());
    let authority = state
        .personas()
        .keypair_path(ClusterKind::Localnet, "whale")
        .unwrap();

    let req = MetaplexMintRequest {
        cluster: ClusterKind::Localnet,
        authority_persona: "whale".into(),
        metadata_uri: "https://example.com/meta.json".into(),
        name: "Test NFT".into(),
        symbol: "TST".into(),
        recipient: None,
        collection_mint: None,
        seller_fee_bps: Some(250),
        standard: MetaplexStandard::NonFungible,
        node_bin: None,
        refresh_worker: false,
        rpc_url: Some("http://127.0.0.1:8899".into()),
    };
    let worker_root = state.metaplex_worker_root();
    let result = mint_metaplex_nft(
        state.token_services().metaplex.as_ref(),
        &worker_root,
        &authority,
        req,
    )
    .expect("metaplex mint");
    assert!(result.success);
    assert_eq!(
        result.mint_address.as_deref(),
        Some("4Rf9mGD7FeYknun5JczX5nGLTfQuS1GRjNVfkEMKE92b")
    );
    assert_eq!(result.signature.as_deref(), Some("SigXYZ"));

    let captured = runner.captured();
    assert_eq!(captured.len(), 1);
    // Worker script was materialised into the temp dir.
    assert!(captured[0].worker_path.exists());
    // Authority env var points at the persona's keypair file.
    let env_map: std::collections::BTreeMap<_, _> = captured[0].envs.iter().cloned().collect();
    assert!(env_map.contains_key::<std::ffi::OsString>(&"CADENCE_AUTHORITY".into()));
    let authority_env = env_map
        .get::<std::ffi::OsString>(&"CADENCE_AUTHORITY".into())
        .unwrap();
    assert_eq!(
        authority_env.to_string_lossy().as_ref(),
        authority.display().to_string(),
        "authority env var must be the persona's keypair path"
    );
}

pub fn metaplex_mint_rejects_overlong_symbol() {
    let (fixture, _keypair) = fixture_with_whale();
    let runner = Arc::new(CapturingMetaplexRunner::new(MetaplexMintOutcome {
        exit_code: Some(0),
        success: true,
        stdout: String::new(),
        stderr: String::new(),
        mint_address: None,
        signature: None,
    }));
    let services: Arc<TokenServices> = Arc::new(TokenServices {
        token: Arc::new(cadence_desktop_lib::commands::solana::SystemTokenCreateRunner::new()),
        metaplex: Arc::clone(&runner) as Arc<dyn MetaplexMintRunner>,
    });
    let tmp = tempfile::TempDir::new().unwrap();
    let state = fixture
        .state
        .with_token_services(services)
        .with_metaplex_worker_root(tmp.path().to_path_buf());
    let authority = state
        .personas()
        .keypair_path(ClusterKind::Localnet, "whale")
        .unwrap();
    let req = MetaplexMintRequest {
        cluster: ClusterKind::Localnet,
        authority_persona: "whale".into(),
        metadata_uri: "https://example.com/meta.json".into(),
        name: "Test".into(),
        symbol: "WAYTOOLONGSYM".into(),
        recipient: None,
        collection_mint: None,
        seller_fee_bps: Some(0),
        standard: MetaplexStandard::NonFungible,
        node_bin: None,
        refresh_worker: false,
        rpc_url: Some("http://127.0.0.1:8899".into()),
    };
    let err = mint_metaplex_nft(
        state.token_services().metaplex.as_ref(),
        &state.metaplex_worker_root(),
        &authority,
        req,
    )
    .expect_err("symbol > 10 bytes must be rejected");
    assert_eq!(err.code, "solana_metaplex_mint_bad_symbol");
    assert!(runner.captured().is_empty());
}

// ---------- Wallet scaffold -----------------------------------------------

fn full_toolchain() -> ToolchainStatus {
    ToolchainStatus {
        node: ToolProbe {
            present: true,
            path: Some("/usr/local/bin/node".into()),
            version: Some("v20.11.1".into()),
        },
        pnpm: ToolProbe {
            present: true,
            path: Some("/usr/local/bin/pnpm".into()),
            version: Some("9.0.0".into()),
        },
        ..ToolchainStatus::default()
    }
}

pub fn wallet_descriptors_cover_every_kind() {
    let descs = wallet_descriptors();
    let kinds: std::collections::BTreeSet<_> = descs.iter().map(|d| d.kind).collect();
    for kind in WalletKind::ALL {
        assert!(
            kinds.contains(&kind),
            "descriptor catalog missing {:?}",
            kind
        );
    }
}

pub fn privy_scaffold_writes_compileable_tree_with_api_key_env() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toolchain = full_toolchain();
    let request = WalletScaffoldRequest {
        kind: WalletKind::Privy,
        output_dir: tmp.path().display().to_string(),
        project_slug: Some("my-privy-app".into()),
        cluster: ClusterKind::Devnet,
        rpc_url: None,
        app_name: Some("Demo".into()),
        app_id: Some("priv-test-id".into()),
        overwrite: false,
    };
    let result = generate_wallet_scaffold(&toolchain, &request).expect("scaffold");
    assert_eq!(result.api_key_env.as_deref(), Some("PRIVY_APP_ID"));
    assert!(result.files.iter().any(|f| f.path == ".env.example"));
    assert!(result.files.iter().any(|f| f.path == "package.json"));
    assert!(result.files.iter().any(|f| f.path == "src/main.tsx"));
    let env_example =
        std::fs::read_to_string(std::path::Path::new(&result.root).join(".env.example")).unwrap();
    assert!(env_example.contains("VITE_PRIVY_APP_ID"));
    assert!(env_example.contains("priv-test-id"));
}

pub fn wallet_adapter_scaffold_bakes_rpc_url_and_reports_free_tier() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toolchain = full_toolchain();
    let request = WalletScaffoldRequest {
        kind: WalletKind::WalletAdapter,
        output_dir: tmp.path().display().to_string(),
        project_slug: Some("my-adapter".into()),
        cluster: ClusterKind::Mainnet,
        rpc_url: Some("https://rpc.example/free".into()),
        app_name: Some("Demo".into()),
        app_id: None,
        overwrite: false,
    };
    let result = generate_wallet_scaffold(&toolchain, &request).expect("scaffold");
    assert!(result.api_key_env.is_none());
    assert_eq!(result.rpc_url, "https://rpc.example/free");
    let providers =
        std::fs::read_to_string(std::path::Path::new(&result.root).join("src/WalletProviders.tsx"))
            .unwrap();
    assert!(providers.contains("https://rpc.example/free"));
    assert!(providers.contains("LedgerWalletAdapter"));
}

pub fn mwa_scaffold_writes_phone_testing_checklist() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toolchain = full_toolchain();
    let request = WalletScaffoldRequest {
        kind: WalletKind::MwaStub,
        output_dir: tmp.path().display().to_string(),
        project_slug: Some("mwa-demo".into()),
        cluster: ClusterKind::Devnet,
        rpc_url: None,
        app_name: Some("MWA Demo".into()),
        app_id: None,
        overwrite: false,
    };
    let result = generate_wallet_scaffold(&toolchain, &request).expect("scaffold");
    assert!(result
        .files
        .iter()
        .any(|f| f.path == "PHONE_TESTING_CHECKLIST.md"));
    let checklist = std::fs::read_to_string(
        std::path::Path::new(&result.root).join("PHONE_TESTING_CHECKLIST.md"),
    )
    .unwrap();
    assert!(checklist.to_lowercase().contains("phone"));
    assert!(checklist.to_lowercase().contains("expo"));
}

pub fn wallet_scaffold_refuses_missing_node() {
    let tmp = tempfile::TempDir::new().unwrap();
    let toolchain = ToolchainStatus::default();
    let request = WalletScaffoldRequest {
        kind: WalletKind::WalletAdapter,
        output_dir: tmp.path().display().to_string(),
        project_slug: None,
        cluster: ClusterKind::Devnet,
        rpc_url: None,
        app_name: None,
        app_id: None,
        overwrite: false,
    };
    let err = generate_wallet_scaffold(&toolchain, &request).unwrap_err();
    assert_eq!(err.code, "solana_wallet_scaffold_requires_node");
}
