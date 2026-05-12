# Xero × Helius Deck Sources

Project-local evidence (verified against the repo):

- `README.md`: Xero desktop app scope, Tauri/React/Rust stack, Solana workbench sidebar, MCP server registry, custom-agent harness with stages, 11 model providers.
- `SOLANA_WORKBENCH_PLAN.md`: local-first Solana workbench architecture — Surfpool/mainnet-fork, personas, TxPipeline, DeployManager, AuditEngine, IDL/Codama, PDA/ALT/Token-2022.
- `client/src-tauri/src/lib.rs:446–524`: **79** registered `solana_*` Tauri commands.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/solana.rs`: **24** distinct Solana tools mounted in the autonomous agent harness (`AUTONOMOUS_TOOL_SOLANA_*` constants).
- `client/components/xero/solana-workbench-sidebar.tsx:639–715`: 13 workbench tabs — Cluster, Personas, Scenarios, Tx, Logs, Indexer, IDL, Deploy, Audit, Token, Wallet, Safety, RPC. The proposed Lana embed becomes the 14th.
- `client/src-tauri/src/commands/solana/rpc_router.rs:377–396`: Helius free devnet/mainnet endpoints already registered as known RPC providers.
- `client/src-tauri/src/commands/solana/secrets/patterns.rs:89–97`: Helius RPC API key pattern in the secret scanner.
- `landing/components/landing/pricing.tsx`: Free / Solana Bundle ($50/mo) pricing shape.
- `landing/components/landing/hero.tsx`, `app-window-mock.tsx`: landing tone and desktop-window visual language used in slide 03 mock.

External factual checks:

- Lana (not a Helius product): https://www.lana.ai/ — meta description "Explore Solana with AI — look up wallets, tokens, transactions, and DeFi data using natural language." Creator/publisher meta = "Lana". Independent Next.js app. Deck treats Lana as an embed partner, not a Helius asset.
- Helius for Agents (the actual Helius agent stack): https://www.helius.dev/blog/how-to-use-ai-to-build-solana-apps — Helius MCP Server (60+ tools incl. `getBalance`, `parseTransaction`, `getAssetsByOwner`, `createWebhook`), Helius CLI (95+ commands), Helius Skills, Helius Claude Code plugin.
- Helius Orb (Helius's own AI explorer, separate from Lana): https://www.helius.dev/blog/orb-block-explorer — announced Oct 30 2025.
- Helius RPC endpoints: https://www.helius.dev/docs/api-reference/endpoints
- Helius Sender: https://www.helius.dev/docs/sending-transactions/sender
- Helius Priority Fee API: https://www.helius.dev/docs/priority-fee
- Helius Webhooks: https://www.helius.dev/docs/webhooks
- Solana Foundation 2023 developer ecosystem report: https://solana.com/news/2023-state-of-solana-developer-ecosystem
- SolanaFloor/Syndica 2025 developer report coverage: https://solanafloor.com/news/developers-on-solana-increase-almost-10-x

Removed from prior sources (factual issues):

- The Reddit post that previously cited "Mert's Apr 21 2026 LanaAI announcement" could not be verified. Helius's announced AI explorer is Orb, not Lana; Lana appears to be an independent product. The deck no longer implies Lana is a Helius asset.
