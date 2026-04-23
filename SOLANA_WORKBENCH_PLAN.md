# Solana Workbench — Implementation Plan

Production-grade integration of a Solana developer workbench as a first-class sidebar and agent-tool surface inside Cadence. Mirrors the `EmulatorSidebar` architecture: one Tauri command surface used by both the React UI and the autonomous agent runtime, bundled open-source sidecars, zero paid-provider dependencies in the default path.

The goal is to make everything a Solana dapp developer can do **on their own laptop** — fork, seed, simulate, land, deploy, upgrade, audit, index — feel native to the agent, with every capability callable programmatically via the same Tauri commands the UI uses.

---

## 1. Goal & Constraints

### Goal
Give a developer working inside Cadence a complete local-first Solana stack:

- A forked mainnet validator with state snapshots, seeded personas, and reusable scenarios.
- A transaction runtime that handles priority fees, compute-unit tuning, ALT assembly, and CPI account resolution for them.
- Build/deploy/upgrade tooling that catches account-layout regressions, syncs IDL and generated clients, and generates Squads multisig payloads.
- Static analysis, fuzzing, coverage, and exploit-replay harnesses bundled in.
- A log decoder that turns raw program logs into English using the IDL.
- All provider interactions default to free tiers (Helius free, Triton free, public RPCs, Jito free RPC) with graceful degradation when unavailable.

**LLM-agent driveable.** Every capability the workbench UI exposes to a human — start a fork, fund a persona, simulate a tx, decode a failure, land a tx, diff a program upgrade — must be callable programmatically via the same Tauri command surface, so the autonomous runtime and a future MCP server can drive it without a human at the keyboard.

### Hard constraints
1. **Free to us, free to the user.** No capability in the default path requires a paid API key. Paid providers are pluggable upgrades, never the default.
2. **Local-first.** Everything that can run on the user's machine (validator, fuzzer, analyzer, decoder, indexer scaffold) runs there. Remote calls are opt-in and cached.
3. **Bundled sidecars where feasible.** `solana-test-validator`, `anchor`, `cargo-build-sbf`, `spl-token`, and `trident` are user-provided via the Solana CLI install (we probe for them, like we do Xcode / Android SDK). `surfpool`, `lite-svm`, and `codama` CLI tools are bundled as sidecars where their licenses permit.
4. **One active validator at a time** across the whole app. Switching cluster or sidebar shuts the previous one down — same invariant as the emulator sidebar.
5. **Single titlebar button.** One `Solana` / `Orbit` icon next to `Globe` and `Gamepad`. Cluster selection (localnet / devnet / mainnet-fork) is inside the sidebar, not in the titlebar.
6. **User-provided prerequisites:** Solana CLI 1.18+, Anchor 0.30+, Rust toolchain with `cargo-build-sbf`, Node 20+. Missing-toolchain state is a first-class UX surface, identical to the `emulator-missing-sdk` panel.
7. **Platform matrix:**
   - macOS / Linux: full feature parity.
   - Windows: core features work; `solana-test-validator` runs under WSL2 if present, otherwise we surface a degraded state and fall back to LiteSVM for tests.
8. **Every automation command takes purely serializable JSON and returns purely serializable JSON.** No Tauri-only types leak through. Same rule as the emulator automation surface — this is what lets a future MCP server wrap the commands without a refactor.

### Inherited constraints (`AGENTS.md` + codebase conventions)
- ShadCN for all UI where possible.
- No debug/test UI — only user-facing surfaces.
- `python3` when Python is invoked.
- Tauri app only.
- Follow the sidebar mutex-open pattern in `App.tsx:100–114` and titlebar wiring in `shell.tsx`.

---

## 2. Architecture Overview

```
┌──────────────────────── Cadence Tauri App ─────────────────────────┐
│                                                                    │
│  Titlebar                                                          │
│   └── [Apple] [Android] [Globe] [Gamepad] [Solana] …               │
│                                                                    │
│  Main webview                                                      │
│   ├── ProjectRail                                                  │
│   ├── active view (phases / agent / execution)                     │
│   ├── BrowserSidebar                                               │
│   ├── GamesSidebar                                                 │
│   ├── IosEmulatorSidebar                                           │
│   ├── AndroidEmulatorSidebar                                       │
│   └── SolanaWorkbenchSidebar  ◄──┐                                 │
│        ├── Cluster picker        │                                 │
│        ├── Personas panel        │                                 │
│        ├── Tx inspector          │                                 │
│        ├── Program deploy panel  │                                 │
│        ├── Audit report panel    │                                 │
│        └── Log decoder feed      │                                 │
│                                  │                                 │
│   invoke("solana_*", …) ─────────┘                                 │
│                                                                    │
│  Rust backend  (src-tauri/src/commands/solana/)                    │
│   ├── SolanaState         ← single-active-cluster registry         │
│   ├── ValidatorSupervisor ← owns test-validator / surfpool child   │
│   ├── RpcRouter           ← health-checked failover across RPCs    │
│   ├── IdlRegistry         ← cached IDLs + auto-reload              │
│   ├── PersonaStore        ← named keypairs + seeded state          │
│   ├── TxPipeline          ← build → simulate → land → decode       │
│   ├── DeployManager       ← build → upgrade-safety → deploy        │
│   ├── AuditEngine         ← static analysis + fuzz + coverage      │
│   ├── LogBus              ← validator logs + decoded event stream  │
│   └── URI scheme handler  ← solana://idl/{program} etc.            │
│        │                                                           │
│        ▼                                                           │
└────────┼───────────────────────────────────────────────────────────┘
         │
         ▼  child processes (user-provided or bundled)
  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ ┌────────────┐
  │ solana-test- │  │ surfpool     │  │ anchor       │ │ trident    │
  │  validator   │  │ (forked      │  │ (build, idl, │ │ (fuzzer)   │
  │ (localnet)   │  │  mainnet)    │  │  deploy)     │ │            │
  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘ └─────┬──────┘
         │                 │                 │               │
         ▼                 ▼                 ▼               ▼
   RPC :8899 + WS :8900 + Geyser plugin sink + logs
         │
         ▼
  Rust RpcRouter multiplexes localnet / devnet / mainnet-fork / mainnet
```

### Data flow per transaction
1. Agent or UI calls `solana_tx_send` with a description: `{ cluster, persona, instructions, table_hints, priority_strategy }`.
2. `TxPipeline` resolves personas → signers, resolves CPI accounts, packs into a v0 transaction, attaches `ComputeBudget` instructions auto-tuned via simulation.
3. `RpcRouter` picks the best healthy endpoint for the target cluster, applies backoff, sends through Jito bundle client if strategy calls for it.
4. Result goes through the **Decoder**: signature → logs → IDL error map → human-readable trace with affected accounts highlighted.
5. Result emitted on `solana:tx` event; persisted to a per-cluster history.

### Data flow per program deploy
1. `DeployManager` runs `anchor build` (or `cargo build-sbf`) and hashes the resulting `.so`.
2. Upgrade-safety pass: diff account layouts in the Anchor IDL against the deployed program's on-chain IDL; fail closed if the diff is breaking.
3. If cluster's program authority is a multisig, synthesize a Squads proposal payload instead of calling `solana program deploy` directly.
4. After deploy, publish IDL on-chain via `anchor idl init`/`upgrade` and regenerate clients via Codama.

### Why this architecture
- **Same pattern as EmulatorSidebar.** Single-active invariant, sidecar-driven, URI scheme for large payloads, serializable command surface shared with the agent. No new paradigms for contributors to learn.
- **Local-first by default.** The "happy path" (localnet + forked mainnet + seeded personas) never leaves the laptop, so cost = $0 regardless of scale.
- **Pluggable paid tier.** `RpcRouter` accepts API keys from `SolanaSettings`, upgrading without changing command shape.
- **IDL is load-bearing.** Almost every interpretive feature (decoder, CPI resolver, PDA checker, Codama codegen) keys off a cached IDL, so we centralize that in `IdlRegistry`.

---

## 3. Bundled Binaries & User-Provided Prerequisites

| Binary | Source | License | Bundled? | Reason |
|---|---|---|---|---|
| `surfpool` | [txtx/surfpool](https://github.com/txtx/surfpool) | MIT | Yes (per-platform) | Forked-mainnet validator, tiny binary, BSD-like license |
| `codama` CLI | [codama-idl/codama](https://github.com/codama-idl/codama) | MIT | Yes (node script + pinned version) | IDL → TS/Rust client codegen |
| `trident` / `trident-cli` | [Ackee-Blockchain/trident](https://github.com/Ackee-Blockchain/trident) | Apache-2.0 | Yes (cargo-install-on-first-run, cached) | Anchor fuzzer |
| `litesvm` | [LiteSVM/litesvm](https://github.com/LiteSVM/litesvm) | Apache-2.0 | Vendored crate | In-process validator for fast tests |
| `solana-verify` (verified-builds CLI) | Solana Foundation | Apache-2.0 | Yes | Verified-build submission |
| `mucho` | [Solana Foundation] | Apache-2.0 | Optional | Convenience wrapper around CLI tools |

User-provided prerequisites (NOT bundled):
- Solana CLI 1.18+ (`solana`, `solana-test-validator`, `solana-keygen`, `spl-token`).
- Anchor 0.30+ (`anchor`) — required only when the opened project contains `Anchor.toml`.
- Rust toolchain with `cargo-build-sbf` — required only for Rust program projects.
- Node 20+ and pnpm — already required elsewhere in the app.

Detection: startup probe using `which`/`where`, version parsing via `--version`, results cached in `SolanaState.toolchain` and surfaced to frontend via `solana_toolchain_status`. Same shape as `emulator_sdk_status`.

---

## 4. Rust Module Layout

New tree under `client/src-tauri/src/commands/`:

```
solana/
├── mod.rs                      // SolanaState, Tauri command registration, URI scheme
├── toolchain.rs                // CLI discovery, version parsing, missing-tool UX
├── rpc_router.rs               // multi-endpoint router, health checks, backoff, free-tier pool
├── validator/
│   ├── mod.rs                  // ValidatorSupervisor entry point
│   ├── test_validator.rs       // wraps solana-test-validator (--clone, --reset, --bpf-program)
│   ├── surfpool.rs             // wraps surfpool for forked-mainnet
│   ├── litesvm.rs              // in-process LiteSVM for test harnesses
│   ├── snapshot.rs             // account dump/restore via getAccountInfo + atomic write
│   └── events.rs               // solana:validator:{status,log,progress}
├── persona/
│   ├── mod.rs                  // PersonaStore: named keypairs + metadata
│   ├── roles.rs                // built-in roles: whale, lp, voter, liquidator, new_user
│   ├── fund.rs                 // airdrop, SPL-Token mint/transfer, Metaplex NFT seed
│   └── import.rs               // import keypair from file / mnemonic (localnet only)
├── tx/
│   ├── mod.rs                  // TxPipeline: build → simulate → land → decode
│   ├── compute_budget.rs       // auto-tune CU price + limit from simulation
│   ├── priority_fee.rs         // free-tier fee oracle (Helius free, Triton free, local percentile)
│   ├── alt.rs                  // Address Lookup Table create/extend/activate helpers
│   ├── cpi_resolver.rs         // known-program account map (SPL, Jupiter, Metaplex, etc.)
│   ├── jito.rs                 // Jito bundle submission via free public RPC
│   └── decoder.rs              // logs + IDL + error codes → human trace
├── idl/
│   ├── mod.rs                  // IdlRegistry: cache, reload on file change, on-chain fetch
│   ├── codama.rs               // run Codama codegen, emit updates
│   ├── publish.rs              // anchor idl init / upgrade orchestration
│   └── drift.rs                // detect deployed-IDL vs local-IDL drift per cluster
├── program/
│   ├── mod.rs                  // DeployManager entry
│   ├── build.rs                // anchor build / cargo build-sbf with captured diagnostics
│   ├── deploy.rs               // solana program deploy / upgrade (direct + Squads path)
│   ├── upgrade_safety.rs       // account-layout diff, size check, authority check
│   ├── squads.rs               // Squads v4 proposal payload synthesis
│   └── verified_build.rs       // solana-verify submission
├── audit/
│   ├── mod.rs                  // AuditEngine entry
│   ├── static_lints.rs         // Anchor footgun checks: missing Signer, missing owner, etc.
│   ├── sec3.rs                 // wrap sec3/soteria if installed; otherwise no-op
│   ├── trident.rs              // Trident fuzz harness gen + run
│   ├── coverage.rs             // cargo-llvm-cov orchestration, lcov parse
│   └── replay.rs               // exploit replay library: Wormhole, Mango, Cashio, Nirvana
├── pda/
│   ├── mod.rs                  // PDA derivation helpers + canonical-bump checker
│   ├── seed_scan.rs            // scan program source for hardcoded seeds, find collisions
│   └── predict.rs              // cross-cluster deterministic PDA prediction
├── indexer/
│   ├── mod.rs                  // scaffold generator
│   ├── carbon.rs               // Carbon indexer scaffold from IDL
│   ├── log_parser.rs           // lightweight log-based event extractor
│   └── webhook.rs              // Helius free-tier webhook handler scaffold
├── token/
│   ├── mod.rs                  // SPL + Token-2022 helpers
│   ├── extensions.rs           // Token-2022 extension support matrix + warnings
│   └── metaplex.rs             // Umi-based NFT mint/list fixtures for seeding
├── secrets/
│   ├── mod.rs                  // scoped keypair store, cluster-bound
│   ├── gitleaks.rs             // wrap gitleaks if installed; fallback to built-in patterns
│   └── rotation.rs             // rotation reminders tied to deploy events
├── logs/
│   ├── mod.rs                  // validator + RPC subscription multiplexer
│   └── decoder_stream.rs       // decoded event stream (uses tx/decoder.rs)
└── cost/
    ├── mod.rs                  // meter aggregation across free-tier providers
    └── providers.rs            // Helius usage API, Triton usage API, local rent/CU tally
```

### `SolanaState`

```rust
pub struct SolanaState {
    active: Mutex<Option<ActiveCluster>>, // single-validator invariant
    rpc_router: Arc<RpcRouter>,
    idl_registry: Arc<IdlRegistry>,
    personas: Arc<PersonaStore>,
    tx_history: Arc<TxHistory>,
    toolchain: OnceLock<ToolchainProbe>,
    log_bus: Arc<LogBus>,
    settings: Arc<SolanaSettings>, // paid-provider API keys if user supplied
}

enum ActiveCluster {
    Localnet(TestValidatorSession),
    MainnetFork(SurfpoolSession),
    Devnet,    // remote only
    Mainnet,   // remote only, read-path default
}
```

Registered in `lib.rs` alongside `BrowserState` and the emulator state.

### Tauri command surface

Grouped by purpose. Every command is JSON-in/JSON-out, agent-callable.

**Toolchain & cluster lifecycle** — Phase 1:
```rust
solana_toolchain_status() -> ToolchainStatus
    // { solana_cli: { present, version }, anchor: { present, version },
    //   cargo_build_sbf, rust, node, pnpm, surfpool, trident, codama,
    //   solana_verify, wsl2 (windows only) }

solana_cluster_list() -> Vec<ClusterDescriptor>
    // localnet, mainnet_fork, devnet, mainnet + any custom user endpoints

solana_cluster_start(kind: ClusterKind, opts: StartOpts) -> ClusterHandle
    // kind: localnet | mainnet_fork
    // opts: clone_programs, clone_accounts, seed_personas, snapshot_id, reset, limit_ledger
    // idempotent: stops any other active cluster first

solana_cluster_stop() -> ()

solana_cluster_status() -> ClusterStatus
    // { kind, slot, block_height, tps, uptime_s, rpc_url, ws_url, geyser? }

solana_snapshot_create(label: String) -> SnapshotId
solana_snapshot_restore(id: SnapshotId) -> ()
solana_snapshot_list() -> Vec<SnapshotMeta>
solana_snapshot_delete(id: SnapshotId) -> ()
```

**RPC router** — Phase 1:
```rust
solana_rpc_health() -> Vec<EndpointHealth>
    // per-endpoint latency, success rate, last error, rate-limit window

solana_rpc_endpoints_set(cluster, endpoints: Vec<EndpointSpec>) -> ()
    // free-tier list (public + Helius free + Triton free) is the default
```

**Personas & seeding** — Phase 2:
```rust
solana_persona_create(spec: PersonaSpec) -> Persona
    // { role: "whale" | "lp" | "voter" | "liquidator" | "new_user" | "custom",
    //   name, sol, tokens: [{mint, amount}], nfts: [{collection, count}] }

solana_persona_list(cluster) -> Vec<Persona>
solana_persona_fund(name, delta: FundingDelta) -> ()
solana_persona_import_keypair(name, path) -> Persona   // localnet only
solana_persona_export_keypair(name) -> Path            // localnet only, behind confirm

solana_scenario_list() -> Vec<ScenarioDescriptor>
    // built-in library: swap_jupiter, add_liquidity_orca, governance_vote,
    // liquidation_kamino, metaplex_mint_list, token2022_transfer_hook, ...

solana_scenario_run(name, params: JsonValue, as_persona: PersonaRef) -> ScenarioRun
    // returns list of signatures + decoded results
```

**Transaction pipeline** — Phase 3:
```rust
solana_tx_build(spec: TxSpec) -> UnsignedTx
    // spec: { instructions, signers: [PersonaRef], table_hints, cluster }
    // returns: { v0_message, required_alts, simulated_cu, predicted_fee }

solana_tx_simulate(tx: UnsignedTx | SignedTx) -> SimulationResult
    // { logs, decoded_logs, cu_consumed, err?, affected_accounts }

solana_tx_send(spec: TxSpec, strategy: LandingStrategy) -> TxResult
    // strategy: { priority_percentile, use_jito, max_retries, confirmation: "processed"|"confirmed"|"finalized" }
    // Pipeline: build → simulate (auto-tune CU) → send → confirm → decode

solana_tx_explain(signature | logs) -> Explanation
    // LLM-ready structured explanation of a failed or successful tx

solana_alt_create(addresses: Vec<Pubkey>, authority) -> Pubkey
solana_alt_extend(alt, addresses) -> ()
solana_alt_resolve(tx_spec) -> Vec<Pubkey>   // suggest ALT entries for a given tx

solana_cpi_resolve(program_id, instruction_name, args) -> Vec<AccountMeta>
    // uses known-program map + IDL fallback

solana_priority_fee_estimate(program_ids: Vec<Pubkey>) -> FeeEstimate
    // free-tier oracle: Helius free API or local percentile from recent slots
```

**IDL & clients** — Phase 4:
```rust
solana_idl_get(program_id, cluster) -> Idl
solana_idl_watch(path) -> SubscriptionToken
    // emits solana:idl:changed when target/idl/*.json changes

solana_idl_publish(program_id, cluster, path) -> Signature
    // anchor idl init / upgrade orchestration

solana_idl_drift(program_id) -> DriftReport
    // compare local IDL vs on-chain IDL per cluster, highlight breaking changes

solana_codama_generate(idl_path, targets: ["ts", "rust", "umi"]) -> GenerationReport
    // runs Codama; returns list of files written + typecheck result
```

**Program deploy & upgrade** — Phase 5:
```rust
solana_program_build(manifest_path, profile: "dev" | "release") -> BuildReport
    // wraps anchor build / cargo build-sbf, captures diagnostics, returns .so hash + size

solana_program_upgrade_check(manifest_path, cluster, program_id) -> UpgradeSafetyReport
    // { ok, account_layout_diff, size_ok, authority_ok, breaking_changes: [...] }

solana_program_deploy(spec: DeploySpec) -> DeployResult
    // spec: { manifest_path, cluster, authority: DirectKeypair | SquadsVault,
    //         buffer_strategy, idl_publish: bool }
    // If SquadsVault: synthesize proposal payload + link; no direct deploy.

solana_program_rollback(program_id, previous_hash) -> RollbackResult
    // best-effort via buffer redeploy of previous .so (if archived locally)

solana_verified_build_submit(program_id, manifest_path, github_url) -> SubmissionResult
    // solana-verify workflow

solana_squads_proposal_create(vault, instructions) -> ProposalDescriptor
```

**Audit & quality** — Phase 6:
```rust
solana_audit_static(manifest_path) -> AuditReport
    // { findings: [{ severity, rule_id, file, line, message, fix_hint }] }
    // Built-in Anchor footgun lints + wrap sec3/soteria/aderyn if installed

solana_audit_fuzz(target: String, duration_s, corpus?: Path) -> FuzzReport
    // wraps Trident; returns crashes, coverage delta, reproducer commands

solana_audit_coverage(manifest_path) -> CoverageReport
    // cargo-llvm-cov + lcov parse, per-instruction coverage

solana_replay_exploit(name: String, target_program: Pubkey) -> ReplayReport
    // built-in library: wormhole_sig_skip, cashio_fake_collateral,
    // mango_oracle_manip, nirvana_flash_loan, and user-contributed
```

**PDA tooling** — Phase 4:
```rust
solana_pda_derive(program_id, seeds: Vec<SeedPart>) -> { pubkey, bump, canonical }
solana_pda_scan(manifest_path) -> Vec<PdaSite>
    // scans program source, finds every PDA derivation, reports canonical-bump use
solana_pda_predict(program_id, seeds, clusters: Vec<Cluster>) -> Vec<ClusterPda>
    // show address per cluster for deterministic programs
```

**Indexer scaffolds** — Phase 7:
```rust
solana_indexer_scaffold(kind: "carbon" | "log_parser" | "helius_webhook", idl_path) -> ScaffoldResult
solana_indexer_run(path) -> IndexerHandle     // local-only dev runner
```

**Token / Metaplex** — Phase 7:
```rust
solana_token_create(spec: TokenSpec) -> Pubkey
    // includes Token-2022 extension flags: transfer_hook, transfer_fee,
    // metadata_pointer, interest_bearing, non_transferable, etc.

solana_token_extension_matrix() -> ExtensionMatrix
    // which SDKs/wallets support which Token-2022 extensions today

solana_metaplex_mint(collection, metadata_uri, recipient) -> MintResult
    // Umi-based fixture for seeding; DAS-indexable
```

**Secrets & hygiene** — Phase 9:
```rust
solana_secrets_scan(path) -> Vec<SecretFinding>
solana_secrets_scope_check() -> Vec<ScopeWarning>
    // keypair bound to mainnet authority accidentally loaded on devnet, etc.
solana_cluster_drift_check(manifest_path) -> DriftReport
    // compares pinned external program versions (Metaplex, Jupiter, …) per cluster
```

**Logs & decoded stream** — Phase 7:
```rust
solana_logs_subscribe(filter: LogFilter) -> SubscriptionToken
    // emits solana:log and solana:log:decoded events (the latter via IdlRegistry)
solana_logs_unsubscribe(token) -> ()
```

**Cost governance** — Phase 9:
```rust
solana_cost_snapshot() -> CostSnapshot
    // aggregates free-tier provider usage + local rent/CU tallies
```

Events emitted:
- `solana:validator:status` — `{ kind, phase: booting|ready|stopped|error, message? }`
- `solana:validator:log` — `{ level, message, ts }`
- `solana:tx` — `{ signature, kind: "sent"|"confirmed"|"failed", decoded }`
- `solana:idl:changed` — `{ program_id, path }`
- `solana:deploy:progress` — `{ phase, detail }`
- `solana:audit:finding` — streaming findings during long audit runs
- `solana:cost:update` — periodic cost deltas

### URI scheme

Registered once:

```rust
.register_asynchronous_uri_scheme_protocol("solana", move |_app, request, responder| {
    // solana://idl/{program_id}           -> JSON IDL bytes
    // solana://program/{program_id}.so    -> latest built binary bytes
    // solana://snapshot/{id}              -> tarball of account snapshot
    // solana://tx/{signature}/trace       -> decoded trace JSON
})
```

Same pattern as `emulator://frame?t={seq}`. Keeps large payloads out of Tauri events.

---

## 5. Agent Runtime Integration (`autonomous_tool_runtime`)

Every capability above is exposed to the autonomous agent by adding a thin wrapper in `client/src-tauri/src/runtime/autonomous_tool_runtime/`, matching the existing `browser.rs` pattern.

New files:
```
src-tauri/src/runtime/autonomous_tool_runtime/
├── solana.rs             // tool definitions + dispatch
├── solana_validator.rs   // start/stop/snapshot/status
├── solana_persona.rs     // persona CRUD + fund + scenarios
├── solana_tx.rs          // build/simulate/send/explain
├── solana_program.rs     // build/upgrade-check/deploy/squads
├── solana_audit.rs       // static/fuzz/coverage/replay
└── solana_idl.rs         // get/watch/publish/drift + codama
```

### Tool registry (constants)

```rust
pub const AUTONOMOUS_TOOL_SOLANA_CLUSTER: &str = "solana_cluster";
pub const AUTONOMOUS_TOOL_SOLANA_PERSONA: &str = "solana_persona";
pub const AUTONOMOUS_TOOL_SOLANA_SCENARIO: &str = "solana_scenario";
pub const AUTONOMOUS_TOOL_SOLANA_TX: &str = "solana_tx";
pub const AUTONOMOUS_TOOL_SOLANA_SIMULATE: &str = "solana_simulate";
pub const AUTONOMOUS_TOOL_SOLANA_EXPLAIN: &str = "solana_explain";
pub const AUTONOMOUS_TOOL_SOLANA_PROGRAM: &str = "solana_program";
pub const AUTONOMOUS_TOOL_SOLANA_DEPLOY: &str = "solana_deploy";
pub const AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK: &str = "solana_upgrade_check";
pub const AUTONOMOUS_TOOL_SOLANA_SQUADS: &str = "solana_squads";
pub const AUTONOMOUS_TOOL_SOLANA_IDL: &str = "solana_idl";
pub const AUTONOMOUS_TOOL_SOLANA_CODAMA: &str = "solana_codama";
pub const AUTONOMOUS_TOOL_SOLANA_PDA: &str = "solana_pda";
pub const AUTONOMOUS_TOOL_SOLANA_ALT: &str = "solana_alt";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC: &str = "solana_audit_static";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ: &str = "solana_audit_fuzz";
pub const AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE: &str = "solana_audit_coverage";
pub const AUTONOMOUS_TOOL_SOLANA_REPLAY: &str = "solana_replay";
pub const AUTONOMOUS_TOOL_SOLANA_LOGS: &str = "solana_logs";
pub const AUTONOMOUS_TOOL_SOLANA_INDEXER: &str = "solana_indexer";
pub const AUTONOMOUS_TOOL_SOLANA_COST: &str = "solana_cost";
pub const AUTONOMOUS_TOOL_SOLANA_SECRETS: &str = "solana_secrets";
pub const AUTONOMOUS_TOOL_SOLANA_DRIFT: &str = "solana_cluster_drift";
```

### Policy integration

All `solana_*` tools go through the same `autonomous_tool_runtime/policy.rs` gate as `command` and `browser`. In particular:

- **Destructive actions require approval mode upgrade:** any tool that calls `solana_cluster_stop`, `solana_snapshot_delete`, `solana_program_deploy` to non-localnet clusters, `solana_persona_export_keypair`, or `solana_squads_proposal_create` returns `RequiresApproval` when the runtime is in read-only mode.
- **Mainnet actions are double-gated.** A separate policy layer blocks every tool whose `cluster` argument is `mainnet` unless the user has explicitly enabled mainnet agent actions in settings. Default: off.
- **Keypair export and raw key material** never cross the agent wire. Tools return opaque `PersonaRef` handles instead of private keys.

### Agent-facing docs

Each tool gets a JSON schema + short markdown description shipped with the agent tool catalog, matching how `AutonomousBrowserAction` surfaces its schema today. The catalog is what the LLM sees when deciding which tool to call.

---

## 6. Frontend Component Layout

New files under `client/components/cadence/`:

```
solana-workbench-sidebar.tsx          // shell: resize handle, width persistence, cluster tab
solana-cluster-picker.tsx             // localnet / mainnet-fork / devnet / mainnet
solana-validator-panel.tsx            // start/stop, snapshot list, slot ticker, logs
solana-persona-panel.tsx              // persona CRUD, funding, scenario launcher
solana-tx-inspector.tsx               // paste signature or tx bytes → decoded trace
solana-deploy-panel.tsx               // build status, upgrade-safety report, Squads path
solana-audit-panel.tsx                // static findings + fuzz runs + coverage + replay
solana-idl-panel.tsx                  // IDL viewer, drift warnings, Codama regen button
solana-log-feed.tsx                   // live decoded log stream
solana-missing-toolchain.tsx          // first-run panel when CLIs missing
solana-cost-panel.tsx                 // usage across free-tier providers
```

State hook:
```
client/src/features/solana/use-solana-workbench.ts
```

Mirrors `use-cadence-desktop-state.ts` and the emulator `use-emulator-session.ts` — owns `listen` subscriptions for `solana:*` events, exposes an action API.

### Integration points in existing code

**`client/components/cadence/shell.tsx`:**
- Add `solanaOpen` + `onToggleSolana` to `CadenceShellProps`.
- New `SolanaBtn` next to `BrowserBtn`/`IosBtn`/`AndroidBtn`.
- Always visible on all platforms.

**`client/src/App.tsx`:**
- Add `solanaOpen` to the sidebar mutex group (`App.tsx:100–114`).
- Opening any one closes all others; opening the workbench does not auto-stop the validator (we keep it running across sidebar close/open, only stopped explicitly via panel or `solana_cluster_stop`).
- Render `<SolanaWorkbenchSidebar open={solanaOpen} />` alongside the emulators.

---

## 7. Phases

Each phase is a shippable increment: at the end of every phase the workbench is usable on its own, both from the UI and from the agent.

---

### Phase 1 — Foundation: toolchain, cluster, RPC router

**Deliverables**
- `SolanaState` + `ValidatorSupervisor` + `RpcRouter` scaffolding.
- `solana_toolchain_status` and `solana-missing-toolchain.tsx` panel.
- `solana_cluster_start` / `_stop` / `_status` for `localnet` (via `solana-test-validator`) and `mainnet_fork` (via `surfpool`).
- `solana_snapshot_create` / `_restore` / `_list` using `getAccountInfo` dumps + ledger snapshots.
- `solana_rpc_health` + default free-tier endpoint pool (public + Helius free + Triton free).
- Sidebar shell, cluster picker, validator panel (start / stop / slot ticker).

**Agent tools** registered: `solana_cluster`, basic `solana_logs`.

**Acceptance**
- Agent can spin up a forked-mainnet validator from a cold start in <30s, run `getSlot`, and stop it.
- Snapshot-restore is bit-identical for a seeded set of accounts.
- Failover: kill the primary RPC mid-session; next call lands on the next healthy endpoint.
- Missing-toolchain state renders correctly on a clean VM.

**Exits**
- Unit tests in `tests/solana/validator_supervisor.rs` mirroring `runtime_supervisor.rs`.
- Integration test: spin-restore-spin cycle, 3 consecutive runs pass.

---

### Phase 2 — Personas & Scenarios

**Deliverables**
- `PersonaStore` with built-in roles: `whale`, `lp`, `voter`, `liquidator`, `new_user`.
- `solana_persona_*` commands, per-persona funding (SOL airdrop + SPL mint + Metaplex NFT fixtures).
- Built-in scenario library: `swap_jupiter` (localnet-cloned Jupiter program), `add_liquidity_orca`, `governance_vote` (SPL Governance), `metaplex_mint_list`, `token2022_transfer_hook`.
- `solana_scenario_run` end-to-end.
- Persona panel in sidebar, scenario launcher.

**Agent tools** registered: `solana_persona`, `solana_scenario`.

**Acceptance**
- Agent can create a "whale" persona with 10k SOL, 1M USDC, 3 Metaplex NFTs on a fresh localnet in <5s.
- `swap_jupiter` scenario runs end-to-end against forked mainnet with a cloned Jupiter program.
- Persona mainnet keypair import is blocked by policy; localnet import works.

---

### Phase 3 — Transaction pipeline

**Deliverables**
- `TxPipeline`: build → simulate → auto-tune CU → land → decode.
- Free-tier priority-fee oracle (local percentile from recent slots; Helius free API when key present).
- ALT create/extend/activate + auto-resolver suggesting entries for an in-progress tx.
- CPI account resolver with known-program map (SPL Token, Token-2022, Metaplex, Jupiter, Orca, Raydium, SPL Governance, Squads).
- Jito bundle submission via Jito's public free RPC.
- Decoder: logs + IDL + error codes → structured English trace.
- Tx inspector panel (paste signature / bytes → decoded trace).

**Agent tools** registered: `solana_tx`, `solana_simulate`, `solana_explain`, `solana_alt`.

**Acceptance**
- Agent lands a tx on congested-fork scenario (simulated high-CU-price state) by auto-tuning priority fee — 10/10 landing rate in soak test.
- Tx with 70 accounts auto-packs into v0 + ALT without manual setup.
- `solana_explain` against a known failed signature produces a trace that names the failing instruction, affected accounts, and the IDL error variant.

---

### Phase 4 — IDL, Codama, PDA tooling

**Deliverables**
- `IdlRegistry` with file-watcher on `target/idl/*.json` and on-chain IDL fetch fallback.
- `solana_idl_watch` → `solana:idl:changed` events.
- `solana_codama_generate` running the bundled Codama CLI; writes to `clients/ts/` / `clients/rust/` per project convention.
- `solana_idl_drift` comparing local vs on-chain IDL, classifying breaking vs non-breaking changes.
- `solana_idl_publish` wrapping `anchor idl init` / `anchor idl upgrade` with progress events.
- `solana_pda_derive`, `solana_pda_scan` (source parser finds every PDA derivation site, flags non-canonical bumps), `solana_pda_predict` cross-cluster.
- IDL panel in sidebar.

**Agent tools** registered: `solana_idl`, `solana_codama`, `solana_pda`.

**Acceptance**
- Changing a single account field in `lib.rs` → Codama regenerates TS client within 2s → frontend typecheck catches the break.
- PDA scanner finds every `Pubkey::find_program_address` call in a reference Anchor program (e.g. SPL Governance fork) and reports canonical-bump usage.
- Drift report correctly classifies an added-optional-field as non-breaking and a removed-required-field as breaking.

---

### Phase 5 — Deploy, upgrade safety, Squads, verified builds

**Deliverables**
- `solana_program_build` wrapping `anchor build` / `cargo build-sbf` with captured diagnostics.
- `solana_program_upgrade_check`: account-layout diff from IDL, `.so` size check (<10MB default program size, ProgramData max), upgrade-authority check.
- `solana_program_deploy` with two authority modes: direct keypair (localnet/devnet) and `SquadsVault` (mainnet default).
- `solana_squads_proposal_create` synthesizing a Squads v4 proposal with the correct upgrade instructions.
- `solana_verified_build_submit` wrapping `solana-verify`.
- Post-deploy hook: auto-call `solana_idl_publish` and `solana_codama_generate`.
- Deploy panel in sidebar showing every phase and the safety report gate.

**Agent tools** registered: `solana_program`, `solana_deploy`, `solana_upgrade_check`, `solana_squads`.

**Acceptance**
- Breaking account-layout change blocks deploy with a human-readable diff.
- Mainnet deploy with `SquadsVault` authority produces a working Squads proposal link; no direct `solana program deploy` call is made.
- Verified-build submission succeeds end-to-end against a sample program.

---

### Phase 6 — Security & quality

**Deliverables**
- `AuditEngine` with built-in static lints (Anchor footgun checklist: missing `Signer`, missing `owner`, missing `has_one`, unchecked `AccountInfo`, arithmetic overflow, realloc-without-rent, seed-spoof).
- Pluggable external analyzers: wrap Sec3/Soteria, Aderyn if installed; otherwise rules-only mode.
- Trident fuzz harness generator from IDL + run command; findings surfaced as streaming events.
- `cargo-llvm-cov` coverage orchestration + lcov parse; per-instruction coverage viewer.
- Exploit replay library seeded with Wormhole sig-skip, Cashio fake-collateral, Mango oracle manipulation, Nirvana flash-loan; runs against forked mainnet at the exploit block.
- Audit panel with severity filters + findings stream.

**Agent tools** registered: `solana_audit_static`, `solana_audit_fuzz`, `solana_audit_coverage`, `solana_replay`.

**Acceptance**
- Agent runs full static audit on a 20-instruction Anchor program in <10s and produces structured findings.
- Trident fuzz run for 60s on a reference target produces non-zero coverage delta and a reproducer command.
- Wormhole exploit replay against a freshly forked snapshot of that block produces the expected bad state.

---

### Phase 7 — Observability & indexing

**Deliverables**
- `LogBus` multiplexing validator stdout + RPC `logsSubscribe` + Geyser plugin (when using surfpool with plugin enabled).
- `solana_logs_subscribe` with decoded-log variant using `IdlRegistry`.
- `solana_indexer_scaffold` for three kinds: Carbon, log-parser (standalone), Helius free-tier webhook.
- Local indexer dev runner that replays the last N slots and executes the handler.
- Log-feed panel + decoded-event filter chips.

**Agent tools** registered: `solana_logs`, `solana_indexer`.

**Acceptance**
- Agent asks for "all events from my program in the last 10 slots" and receives structured decoded events.
- Carbon scaffold compiles and runs against localnet without edits.

---

### Phase 8 — Client & wallet integration scaffolds

**Deliverables**
- Wallet-adapter scaffolds: `@solana/wallet-adapter` legacy, `@wallet-standard/react`, Privy free-tier, Dynamic free-tier, Mobile Wallet Adapter stub (desktop stub with "test on phone" checklist).
- `solana_token_create` with full Token-2022 extension support + `solana_token_extension_matrix` (which SDKs support what today, updated from a bundled JSON manifest).
- `solana_metaplex_mint` with Umi.
- Frontend wallet panel: preview which scaffold to generate, copy-to-project action.

**Agent tools** registered: (none new — these are codegen actions routed through the existing `write`/`edit` tools, gated by `solana_toolchain_status`).

**Acceptance**
- Agent instruction "add Privy free-tier wallet connect" produces working files in `client/` that compile and render a connect button.
- Token-2022 extension matrix flags `transfer_hook` as unsupported in X wallet/SDK versions with a concrete remediation hint.

---

### Phase 9 — Hygiene, cost governance, doc grounding

**Deliverables**
- `solana_secrets_scan` with built-in Solana-specific patterns (`id.json`, Jito tip accounts, RPC API keys, Privy app secrets).
- `solana_secrets_scope_check` warning on mainnet keypairs loaded in devnet contexts or vice versa.
- `solana_cluster_drift_check` comparing pinned external program versions across clusters (Metaplex, Jupiter, Squads, SPL Governance).
- `solana_cost_snapshot` aggregating free-tier usage from Helius/Triton public usage endpoints + local rent/CU tally.
- Doc-grounded agent prompts: for every tool, a versioned snippet of the relevant Solana/Anchor/Metaplex docs is fed into the agent catalog so hallucinations drop.
- Cost panel + secrets scan results surfaced under a "Safety" tab in the sidebar.

**Agent tools** registered: `solana_secrets`, `solana_cluster_drift`, `solana_cost`.

**Acceptance**
- Scanner catches a committed `id.json` with a mainnet authority keypair and blocks deploy.
- Drift check correctly flags Metaplex Token Metadata version delta between devnet and mainnet.
- Cost snapshot for a 1-hour workbench session lines up within 5% of each provider's own dashboard.

---

## 8. Open Questions & Decisions to Revisit

1. **Surfpool vs vanilla test-validator `--clone`.** Surfpool's forking UX is strictly better, but dependency on a young project. Default to surfpool for `mainnet_fork`, fall back to `solana-test-validator --clone` when surfpool is absent.
2. **Trident bundling.** Trident takes ~3min to `cargo install` on a cold machine. Decision: install on first fuzz run with a progress panel, not at app start.
3. **LiteSVM vs Bankrun.** LiteSVM is Rust-native, Bankrun is TS-native. We include LiteSVM for Rust tests and surface Bankrun via the test-scaffold codegen when the project is TS.
4. **Squads v3 vs v4.** Build against v4 only; reject v3 vaults with a clear error.
5. **Agent mainnet write-path.** Default off. Even when on, the policy layer requires a per-session confirmation hash and logs every mainnet action.
6. **Indexer storage.** For the local dev runner: SQLite by default (aligns with existing Tauri SQLite usage). Advanced storage is a Phase 10 follow-up.
7. **Program rollback.** Only possible if we archive every deployed `.so` locally. Add an opt-in `program_archive_dir` setting in Phase 5.

---

## 9. Non-Goals (for this plan)

- No custom L1/L2 support (Eclipse, Sonic, etc.) beyond "point the RPC router at it and pray." Add later.
- No hosted preview environments. Local-first only.
- No paid-tier exclusive features. Paid providers are pluggable upgrades, not gates.
- No Solana Mobile Stack (Saga, Seeker) host emulation — out of scope; users test on-device via MWA stub.
- No AI-written Rust programs. Codegen surface is limited to clients, scaffolds, fuzz harnesses, and indexer handlers — not business logic.

---

## 10. Summary

Nine phases, each shippable, each adding a concrete capability to both the UI and the agent tool surface. At the end of Phase 9 the workbench can:

- Spin up a forked-mainnet validator in under 30 seconds.
- Seed any persona with any asset mix in a handful of commands.
- Run a library of realistic scenarios (swaps, governance, liquidations, NFT mints).
- Build, simulate, auto-tune CU, land, and decode transactions on every cluster.
- Build, safety-check, deploy, and Squads-propose program upgrades.
- Audit, fuzz, cover, and exploit-replay programs.
- Scaffold indexers and clients from IDL.
- Catch secret leaks, cluster drift, and Token-2022 incompatibilities before they ship.
- Surface cost across every free-tier provider in one place.

Every one of those capabilities is a JSON-in/JSON-out Tauri command, registered in the autonomous tool runtime, gated by the same policy layer the browser and emulator go through. The agent drives it the same way a developer does — through the command surface.
