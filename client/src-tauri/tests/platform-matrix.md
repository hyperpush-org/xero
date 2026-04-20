# M008 / S06 Deterministic Release-Gate Verification Matrix

This artifact locks milestone **M008** and slice **S06** to one canonical release-gate proof chain for all desktop targets.

## Release-Gate Command (must match exactly on every target)

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml --test runtime_session_bridge --test autonomous_fixture_parity --test runtime_event_stream --test runtime_run_persistence --test bootstrap_contracts && pnpm --dir client test src/features/cadence/use-cadence-desktop-state.runtime-run.test.tsx src/features/cadence/live-views.test.tsx components/cadence/agent-runtime.test.tsx && cargo check --manifest-path client/src-tauri/Cargo.toml && pnpm --dir client exec tauri build --debug
```

## Proof Scope

- Provider bridge and runtime handshake: `runtime_session_bridge`
- Durable assembled parity and replay evidence: `autonomous_fixture_parity`, `runtime_event_stream`, `runtime_run_persistence`, `bootstrap_contracts`
- Agent-tab acceptance and desktop projections: `use-cadence-desktop-state.runtime-run.test.tsx`, `live-views.test.tsx`, `agent-runtime.test.tsx`
- Build integrity: `cargo check` and `pnpm --dir client exec tauri build --debug`

No platform-specific skips are allowed for this M008/S06 release-gate contract.

## macOS

- Command set: exact canonical release-gate command above (no drift)
- Result: proof must pass without platform-specific substitutions

## Linux

- Command set: exact canonical release-gate command above (no drift)
- Result: proof must pass without platform-specific substitutions

## Windows

- Command set: exact canonical release-gate command above (no drift)
- Result: proof must pass without platform-specific substitutions
