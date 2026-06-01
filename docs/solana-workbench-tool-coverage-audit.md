# Solana Workbench Tool Coverage Audit

## Reader And Action

Reader: an engineer landing cold on the Solana workbench.

Post-read action: decide whether a Solana workbench action or autonomous Solana tool is wired, covered by tests, and safe to extend without re-auditing the whole surface.

## Scope

This audit covers the current Solana workbench surface from three angles:

- Human UI: sidebar tabs, panels, and hook actions.
- Desktop backend: Tauri command names and Rust Solana command modules.
- Agent surface: autonomous Solana tools, domain tool-pack discovery, request DTO names, policy expansion, and runtime executor routing.

It does not require opening the app in a browser. Verification should use Tauri/Rust tests, React component tests, and mocked localnet/devnet-safe fixtures.

## Human Workbench Inventory

| Tab | User workflow | Hook actions | Backend command families | Current focused coverage |
| --- | --- | --- | --- | --- |
| Cluster | Toolchain status, cluster list/status, start/stop localnet or fork, RPC health, snapshots | refreshToolchain, installToolchain, refreshClusters, refreshStatus, refreshRpcHealth, refreshSnapshots, start, stop | solana_toolchain_*, solana_cluster_*, solana_rpc_health, solana_snapshot_* | Sidebar smoke tests; Rust spin/restore and RPC failover tests |
| Personas | Role presets, persona list/create/fund/delete | refreshPersonaRoles, refreshPersonas, createPersona, fundPersona, deletePersona | solana_persona_* | Sidebar smoke tests; Rust persona lifecycle and mainnet policy tests |
| Scenarios | Scenario catalog and safe scenario runs | refreshScenarios, runScenario | solana_scenario_* | Rust persona/scenario lifecycle tests |
| Tx | Build, simulate, send, explain, priority fee, CPI, ALT resolution | buildTx, simulateTx, sendTx, explainTx, estimatePriorityFee, resolveCpi, resolveAlt | solana_tx_*, solana_priority_fee_estimate, solana_cpi_resolve, solana_alt_resolve | Backend command module tests; agent catalog contract for tx/simulate/explain/alt |
| Logs | Active subscriptions, subscribe/unsubscribe, recent logs, local feed view | refreshActiveLogSubscriptions, subscribeLogs, unsubscribeLogs, fetchRecentLogs, refreshLogView, clearLogFeed | solana_logs_* | React log feed test; backend log/indexer command tests |
| Indexer | Scaffold local indexers and run event projections | scaffoldIndexer, runIndexer | solana_indexer_* | Backend indexer unit tests; agent catalog contract |
| IDL | Load/fetch/drift/publish IDLs, Codama generation, watch/unwatch | loadIdl, fetchIdl, driftIdl, publishIdl, generateCodama, startIdlWatch, stopIdlWatch | solana_idl_*, solana_codama_generate | Backend IDL/Codama unit tests; sidebar smoke coverage |
| Deploy | Build, upgrade safety, deploy, rollback, Squads proposal, verified build | buildProgram, upgradeCheck, deployProgram, rollbackProgram, createSquadsProposal, submitVerifiedBuild | solana_program_*, solana_squads_*, solana_verified_build_submit | Rust safety and deploy gate tests |
| Audit | Static, external, fuzz, fuzz scaffold, coverage, replay | runStaticAudit, runExternalAudit, runFuzzAudit, scaffoldFuzzHarness, runCoverageAudit, runReplay | solana_audit_*, solana_replay_* | Rust audit engine tests |
| Token | Token extension matrix, token create, Metaplex mint | refreshExtensionMatrix, createToken, mintMetaplex | solana_token_*, solana_metaplex_mint | Rust wallet/token tests |
| Wallet | Wallet scaffold catalog and generation | refreshWalletDescriptors, generateWalletScaffold | solana_wallet_scaffold_* | Rust wallet scaffold tests |
| Safety | Secret patterns/scan, scope check, cluster drift, cost snapshot/reset, doc catalog | scanSecrets, refreshSecretPatterns, runScopeCheck, refreshTrackedPrograms, checkClusterDrift, refreshCostSnapshot, resetCostLedger, refreshDocCatalog | solana_secrets_*, solana_cluster_drift_*, solana_cost_*, solana_doc_catalog | Rust safety tests; agent catalog contract |
| RPC | Endpoint health display | refreshRpcHealth | solana_rpc_health | Sidebar smoke coverage; Rust RPC failover tests |

## Autonomous Tool Inventory

The Solana domain tool pack exposes exactly these autonomous tools:

`solana_cluster`, `solana_logs`, `solana_tx`, `solana_simulate`, `solana_explain`, `solana_alt`, `solana_idl`, `solana_codama`, `solana_pda`, `solana_program`, `solana_deploy`, `solana_upgrade_check`, `solana_squads`, `solana_verified_build`, `solana_audit_static`, `solana_audit_external`, `solana_audit_fuzz`, `solana_audit_coverage`, `solana_replay`, `solana_indexer`, `solana_secrets`, `solana_cluster_drift`, `solana_cost`, and `solana_docs`.

The current contract test verifies four invariants for that list:

- Every expected tool appears in the deferred autonomous tool catalog as a Solana entry.
- The Solana domain tool pack contains exactly the same set.
- A custom agent policy that explicitly allows the Solana pack expands to allow each tool for the engineering runtime agent.
- Representative request DTO variants report the expected tool names.

This closes the high-risk discovery and policy-regression gap. It does not yet prove every tool can successfully execute representative happy-path inputs through the autonomous runtime.

## Safety And State Findings

- Solana state should live under the OS app-data namespace used by the desktop app, not under repo-local `.xero` state and not under a loose `xero` sibling directory.
- Snapshot and persona default roots now share the same app-data namespace as the main Solana state object.
- Mainnet persona creation/import remains policy-denied.
- Deploy gates and secret scans have Rust tests for committed mainnet keypair hazards.
- The current UI text keeps per-run gating language out of Solana-specific user-facing labels; the workbench does not introduce legacy workflow terminology for per-run stages.

## Remaining Gaps

- Add autonomous runtime fixture tests that execute each of the 24 Solana tools with representative valid input and assert useful output shape.
- Add negative runtime tests for invalid/disallowed Solana tool calls, especially mutation-adjacent deploy, send, verified build, and publish paths.
- Expand focused component tests for the non-smoke panels so each panel-level action invokes the expected hook handler with sanitized arguments.
- Add redaction assertions for exported diagnostics and agent-visible tool results where keypair paths, RPC tokens, and wallet material could appear.

## Verification Commands

Use scoped commands. Do not run broad repo-wide Rust tests for this audit unless storage and time budgets are explicitly available.

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml --lib runtime::autonomous_tool_runtime::tests::solana_catalog_pack_policy_and_request_names_cover_issue_15_inventory
cargo test --manifest-path client/src-tauri/Cargo.toml --lib commands::solana::tests::app_data_state_roots_solana_stores_together
```
