# Xero x Helius Deck Sources

Project-local evidence:

- `README.md`: Xero/Xero desktop app scope, Tauri/React/Rust stack, Solana workbench, notification routing, MCP, emulator sidebars.
- `SOLANA_WORKBENCH_PLAN.md`: local-first Solana workbench architecture, Surfpool/mainnet-fork, personas, TxPipeline, DeployManager, AuditEngine, IDL/Codama, PDA/ALT/Token-2022.
- `client/src-tauri/src/lib.rs`: 78 registered `solana_*` Tauri commands.
- `landing/components/landing/pricing.tsx`: Free, Pro, and Solana Pro pricing shape.
- `landing/components/landing/hero.tsx` and `app-window-mock.tsx`: Xero landing-page tone and desktop-window visual language.

External factual checks:

- LanaAI public site: https://www.lana.ai/
- Public r/solana launch post linking Mert's April 21, 2026 LanaAI announcement: https://www.reddit.com/r/solana/comments/1srtm4l/helius_launches_lanaai_conversational_ai_for/
- Helius docs homepage: https://www.helius.dev/docs
- Helius Sender docs: https://www.helius.dev/docs/sending-transactions/sender
- Helius Priority Fee API docs: https://www.helius.dev/docs/priority-fee
- Helius Webhooks docs: https://www.helius.dev/docs/webhooks
- Helius RPC endpoints docs: https://www.helius.dev/docs/api-reference/endpoints
- Solana Foundation 2023 developer ecosystem report: https://solana.com/news/2023-state-of-solana-developer-ecosystem
- SolanaFloor/Syndica 2025 developer report coverage: https://solanafloor.com/news/developers-on-solana-increase-almost-10-x
