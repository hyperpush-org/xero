# Provider Profiles — Remaining Work

This is the handoff document for finishing the legacy `provider_profiles`
removal that started in the Provider Refactor Plan and continued through
the loader hot-swap landed in commit `0ac37c4`.

## Where things stand

The legacy provider-profile concept is still alive in code, but it's no
longer the source of truth:

- The frontend has been rewritten around the flat `providerCredentials`
  state slice (Phase 3 of the original plan).
- `load_provider_profiles_or_default` now synthesizes the snapshot from
  the `provider_credentials` SQLite table; the legacy SQL tables
  (`provider_profiles`, `provider_profiles_metadata`,
  `provider_profile_credentials`) are still created by migrations but
  nothing reads them.
- `persist_provider_profiles_to_db`, the JSON importer, the SQL
  roundtrip module, and ~700 LOC of validation/normalization helpers
  are gone (~2500 LOC deleted across 8 atomic commits).
- Frontend production code has zero references to legacy provider-profile
  DTOs. The legacy types live in `client/src/test/legacy-provider-profiles.ts`
  for the still-skipped legacy fixture builders.

`cargo test --no-default-features` passes (562 lib + 44+ integration
tests). `npx tsc --noEmit` is clean. `npx vitest run` passes
(268 passed / 7 skipped).

## What still depends on `&ProviderProfilesSnapshot`

The seven backend consumer modules below still take
`&ProviderProfilesSnapshot` and reach into `metadata.profiles`,
`metadata.active_profile_id`, `credentials.api_keys`, and the
`profile.readiness(&credentials)` helper. Rewriting each to read
`provider_credentials` directly is the remaining work.

| Module | LOC | Touch points |
|---|---|---|
| `client/src-tauri/src/auth/store.rs` | 408 | `load_provider_profiles_snapshot`, `validate_target_openai_profile`, `resolve_openai_profile_sync_targets`, `select_openai_profile_id`, `ensure_openai_profile_target` |
| `client/src-tauri/src/provider_models/mod.rs` | 1195 | `cache_scope`, `manual_*_projection` helpers, `openai_compatible_profile_uses_local_auth`, multiple `&ProviderProfilesSnapshot` parameters across the catalog projection layer |
| `client/src-tauri/src/runtime/provider.rs` | 752 | `bind_provider_runtime_session`, `resolve_runtime_provider_for_profile`, `select_runtime_provider_profile`, several functions with `Option<&ProviderProfilesSnapshot>` |
| `client/src-tauri/src/runtime/diagnostics.rs` | 1643 | `provider_profile_validation_diagnostics`, `provider_profile_active_selection_diagnostic`, `provider_profile_runtime_alignment_diagnostic`, `provider_profile_metadata_diagnostics`, `provider_profile_readiness_diagnostic` |
| `client/src-tauri/src/commands/doctor_report.rs` | 1527 | builds the doctor report by walking `snapshot.metadata.profiles` and reading `profile.readiness(&snapshot.credentials)` for each |
| `client/src-tauri/src/commands/provider_diagnostics.rs` | 166 | `check_provider_profile` command — maps a `profile_id` against the snapshot to produce diagnostics |
| `client/src-tauri/src/commands/get_runtime_settings.rs` | 182 | projects the snapshot down to a `RuntimeSettingsDto` for the frontend |

Plus the snapshot machinery itself in
`client/src-tauri/src/provider_profiles/{mod.rs, store.rs}` (554 LOC)
that all seven depend on.

## Suggested ramp for a fresh session

The seven consumers don't compile against
`Vec<ProviderCredentialRecord>` directly — they're written to the
`ProviderProfilesSnapshot` shape (active_profile_id, profiles list with
labels and runtime_kind, credentials linkage). A direct rewrite would
need to either:

1. Add `provider_credentials`-shaped helpers next to each consumer
   (e.g. `find_credential_for_provider`, `provider_label_for`,
   `readiness_proof_from_credential`) and convert one consumer at a
   time, deleting its `&ProviderProfilesSnapshot` parameter once all of
   its call sites use the new helpers.
2. Or replace `ProviderProfilesSnapshot` with a thinner credentials-only
   wrapper that just exposes `Vec<ProviderCredentialRecord>` plus the
   helpers each consumer needs, then progressively delete the legacy
   field accesses.

Approach (1) is more incremental but adds two abstractions in flight
at once. Approach (2) is bigger per commit but ends up with a single
type.

### Proposed commit sequence (Approach 2)

1. **Define a `ProviderCredentialsView` struct** in
   `provider_credentials/mod.rs` with the methods the seven consumers
   need: `active_provider_id() -> Option<&str>`,
   `find(provider_id) -> Option<&Record>`,
   `readiness_for(provider_id) -> ReadinessProjection`,
   `label_for(provider_id) -> &str`. Keep `ProviderProfilesSnapshot`
   alongside for now.
2. **Convert `commands/provider_diagnostics.rs`** (smallest, 166 LOC) to
   take `&ProviderCredentialsView` instead. Update
   `commands/doctor_report.rs`'s call site since it routes through this.
3. **Convert `commands/get_runtime_settings.rs`** (182 LOC).
4. **Convert `runtime/provider.rs`** (752 LOC) — careful, it's the OAuth
   bind path.
5. **Convert `auth/store.rs`** (408 LOC).
6. **Convert `provider_models/mod.rs`** (1195 LOC) — the largest single
   consumer.
7. **Convert `runtime/diagnostics.rs`** (1643 LOC) — last because the
   doctor report routes through it.
8. **Convert `commands/doctor_report.rs`** (1527 LOC).
9. **Delete `provider_profiles/`** entirely + the
   `synthesize_provider_profiles_snapshot_from_credentials` function.
10. **Drop the legacy SQL tables** from `global_db/migrations.rs` —
    requires a new forward-only migration that drops
    `provider_profiles`, `provider_profiles_metadata`,
    `provider_profile_credentials` along with the `idx_provider_profiles_*`
    index. Update the `expected_tables` assertion in
    `global_db/mod.rs:189`.

Each step should land as a separate commit with `cargo test
--no-default-features` and `cargo check` clean, and frontend `tsc + vitest`
clean. The full sequence is ~1–2 days of careful work.

## Risk notes

- **OAuth flow:** `auth/store.rs::sync_openai_profile_link` is the only
  remaining writer that touches the snapshot in memory. The legacy
  persist call is gone (commit `36d7594`), so the in-memory mutations
  there are already dead — that's why the function simplifies cleanly
  when converted.
- **Doctor report payload:** the frontend consumes the doctor JSON
  shape; if you change the keys (`affectedProfileId`, etc.), the UI
  needs to be updated in lockstep.
- **`provider_models` catalog cache:** the cache key currently includes
  `profile_id`. After the rewrite the key should be `provider_id` since
  there's no profile concept anymore. This is a cache-invalidation event;
  consider bumping the cache schema version.
- **Migration design:** `provider_credentials` was filled by an
  `INSERT ... SELECT FROM provider_profiles` migration. When you delete
  the legacy tables, that migration's text doesn't change (migrations
  are immutable once applied) but the new schema migration needs to
  `DROP TABLE` the legacy tables behind a guard that won't fail on
  fresh installs (use `DROP TABLE IF EXISTS`).

## Files of interest

- `client/src-tauri/src/provider_profiles/store.rs` — current synthesizer
  is the reference for how to project credentials → snapshot fields.
- `client/src-tauri/src/provider_credentials/{mod.rs, sql.rs, readiness.rs}`
  — the source of truth, ready to be exposed via the new helper layer.
- `client/src-tauri/src/runtime/provider.rs:222` —
  `resolve_runtime_provider_identity` and the per-provider runtime_kind
  table; useful as the basis for `label_for(provider_id)`.

## Audit doc note

`LEGACY_COMPATIBILITY_AUDIT.md` (in repo root) was inadvertently
included in commit `36d7594` via `git add -A`. It was a pre-existing
untracked file from earlier audit work, not part of this refactor.
Decide whether to keep it or revert it in a follow-up commit.
