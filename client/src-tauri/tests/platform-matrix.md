# Desktop Backend Cross-Platform Verification Matrix

This artifact locks the slice **S07** backend verification contract to one canonical command set for all desktop targets.

## Verification Command (must match on every target)

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_fixture_parity --test bootstrap_contracts --test openai_oauth_auth_flow --test runtime_run_persistence --test runtime_supervisor --test runtime_event_stream --test runtime_run_bridge --test notification_route_credentials --test notification_channel_dispatch --test notification_channel_replies && pnpm --dir client test -- src/lib/cadence-model.test.ts src/features/cadence/use-cadence-desktop-state.runtime-run.test.tsx src/features/cadence/live-views.test.tsx components/cadence/agent-runtime.test.tsx src/App.test.tsx && pnpm --dir client exec tauri build --debug
```

No platform-specific skips are allowed for this S07 backend contract.

## macOS

- Command set: exact canonical command above (no drift)
- Result: pending local execution evidence for the S07 parity chain

## Linux

- Command set: exact canonical command above (no drift)
- Result: pending CI/host execution evidence

## Windows

- Command set: exact canonical command above (no drift)
- Result: pending CI/host execution evidence
