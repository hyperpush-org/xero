# xAI/Grok Provider Implementation Plan

Status: draft implementation plan
Date: May 20, 2026

## Goal

Add xAI/Grok as a first-class model provider in Xero, with the new user-facing connection paths now appearing in harnesses:

- Browser OAuth for eligible Grok, SuperGrok, or X subscription accounts.
- Device-code OAuth for remote/headless environments where a localhost callback is awkward.
- API key entry using an xAI API key, compatible with `XAI_API_KEY`.

Also update the landing page so xAI/Grok appears as a native provider rather than only as an OpenAI-compatible example.

## Source Notes

- xAI's public developer docs currently document `https://api.x.ai/v1` and the Responses API at `/responses`.
- xAI docs list `grok-4.3` as the recommended text model, with 1M context and configurable reasoning.
- xAI Responses examples use OpenAI-compatible SDKs, which means Xero can probably reuse much of the existing OpenAI Responses transport after provider-specific request normalization.
- xAI docs show tools such as `x_search`, web search, and code execution through the Responses API, but native server-side xAI tools should be treated as follow-up work unless explicitly included later.
- OpenClaw already implements xAI as a native provider with API key, browser OAuth, and device-code OAuth. Treat its code as reference material, not as a drop-in dependency.
- Do not copy OpenClaw's OAuth client ID into Xero without confirming that it is intentionally public and licensed for third-party use. Prefer registering or configuring a Xero-owned xAI OAuth client.

Reference links:

- xAI quickstart: https://docs.x.ai/developers/quickstart
- xAI models: https://docs.x.ai/developers/models
- xAI reasoning: https://docs.x.ai/developers/model-capabilities/text/reasoning
- xAI streaming: https://docs.x.ai/developers/model-capabilities/text/streaming
- xAI X Search: https://docs.x.ai/developers/tools/x-search
- OpenClaw repository: https://github.com/openclaw/openclaw

## Product Decisions

- Provider id: `xai`.
- Display label: `xAI / Grok`.
- Default profile id: `xai-default`.
- Default model: `grok-4.3`.
- Runtime shape: native xAI provider, not a generic OpenAI-compatible profile.
- Base URL: `https://api.x.ai/v1`.
- Initial model catalog: seed `grok-4.3`; add live catalog refresh for additional models returned by xAI after credentials are configured.
- Initial runtime support: text generation, streaming, Xero function/tool calls.
- Explicitly out of scope for this pass: xAI-native `x_search`, web search, code execution, file attachments, image generation, voice, pricing claims, and legacy `.xero/` state.

## Implementation Milestones

### 1. Provider Identity and Shared Schemas

Update the shared provider schema surface so `xai` is recognized end to end.

Expected targets:

- `packages/ui/src/model/runtime.ts`
- `client/src/lib/xero-model/provider-presets.ts`
- `client/src/lib/xero-model/provider-credentials.ts`
- `client/src-tauri/src/runtime/provider.rs`
- `client/src-tauri/src/provider_credentials/view.rs`
- `client/src-tauri/src/commands/provider_credentials.rs`

Tasks:

- Add `xai` to runtime provider id parsing and validation.
- Add a native xAI provider preset with OAuth, device-code, and API-key connection options.
- Add synthesized profile metadata for `xai-default`.
- Allow API-key credential upsert for `xai`.
- Keep OAuth session creation guarded behind auth commands, not generic credential upsert.
- Make diagnostics say `xAI / Grok` and point users to OAuth, device code, or API key setup.

### 2. Authentication and Credential Storage

Implement xAI auth as provider credential state under OS app-data. Do not use `.xero/`.

Expected targets:

- `client/src-tauri/src/auth/mod.rs`
- `client/src-tauri/src/auth/openai_codex/*` as a reusable pattern
- new `client/src-tauri/src/auth/xai/*`
- `client/src-tauri/src/auth/store.rs`
- `client/src-tauri/src/commands/start_oauth_login.rs`
- `client/src-tauri/src/commands/complete_oauth_callback.rs`
- new or extended command for device-code login

Browser OAuth tasks:

- Discover xAI auth endpoints from `https://auth.x.ai/.well-known/openid-configuration` where possible.
- Use OAuth 2.0 Authorization Code with PKCE.
- Match OpenClaw's xAI-specific behavior only after verification, including the token-exchange quirk where xAI may require PKCE fields to be repeated.
- Persist `oauth_session` credentials directly into `provider_credentials` with provider id `xai`, account identity, access token, refresh token, and expiry.
- Refresh expired or near-expired access tokens before runs.
- Redact tokens in logs, diagnostics, errors, and preflight output.

Device-code tasks:

- Add a start command that returns `verification_uri`, optional complete URI, `user_code`, polling interval, and expiry.
- Poll the token endpoint with `urn:ietf:params:oauth:grant-type:device_code`.
- Handle `authorization_pending`, `slow_down`, denied, and expired states without noisy UI.
- Persist the completed OAuth session through the same xAI credential path as browser OAuth.

API-key tasks:

- Support user-entered xAI API keys through the existing provider credential flow.
- Support `XAI_API_KEY` as ambient documentation and possible import behavior, without making environment variables the only path.
- Never display full keys after save.

### 3. Model Catalog, Capabilities, and Preflight

Add xAI to the model/capability system so the app can select Grok models and explain provider limits correctly.

Expected targets:

- `client/src-tauri/src/provider_models/mod.rs`
- `client/src-tauri/crates/xero-agent-core/src/provider_capabilities.rs`
- `client/src-tauri/src/provider_preflight.rs`
- `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs`
- `client/src-tauri/src/commands/contracts/session_context.rs`

Tasks:

- Add a static xAI catalog projection with `grok-4.3`.
- Set known context metadata for `grok-4.3`: 1,000,000 input context and documented reasoning support.
- Add a live catalog refresh path after credentials exist, so unlisted current/beta models are learned from xAI instead of hardcoded from OpenClaw.
- Add provider capability metadata for xAI Responses, streaming, function/tool calling, reasoning, and unsupported fields.
- Add an xAI preflight probe against `/responses`, not `/chat/completions`.
- Make preflight distinguish invalid API key, expired OAuth token, subscription/account ineligible, rate limit, and model unavailable where xAI error payloads allow it.

### 4. Runtime Adapter

Route owned-agent runs through xAI's Responses API.

Expected targets:

- `client/src-tauri/src/commands/runtime_support/run.rs`
- `client/src-tauri/src/runtime/agent_core/provider_adapters.rs`
- `client/src-tauri/src/runtime/diagnostics.rs`
- `client/src-tauri/src/runtime/pricing.rs` only if verified pricing is intentionally added

Tasks:

- Resolve xAI runs from either OAuth access token or API key.
- Refresh OAuth credentials before starting a run if expiry is close.
- Build requests for `POST https://api.x.ai/v1/responses`.
- Reuse the OpenAI Responses SSE parser if xAI events match; otherwise add xAI-specific event handling.
- Map Xero reasoning settings to xAI's documented values:
  - `none` -> `none`
  - `minimal` / `low` -> `low`
  - `medium` -> `medium`
  - `high` / `xhigh` -> `high`
- Send `reasoning: { "effort": ... }` for `grok-4.3` when reasoning is enabled.
- Avoid sending fields xAI rejects with reasoning models, especially `presencePenalty`, `frequencyPenalty`, and `stop`.
- Add an xAI tool-schema sanitizer if tool calls fail with unsupported JSON Schema keywords. OpenClaw strips several length/cardinality keywords, so this needs focused tests.
- Add parser hardening if xAI tool-call arguments arrive HTML-entity encoded, as OpenClaw appears to defend against that behavior.
- Keep attachments/media disabled until the runtime has explicit support and tests.

### 5. Desktop Provider UI

Expose xAI setup as real user-facing provider UI, using ShadCN components where new UI is needed.

Expected targets:

- provider settings components under `client/src/features/xero`
- model/profile picker code that consumes provider presets
- provider credential status and diagnostics surfaces

Tasks:

- Show `xAI / Grok` as a provider.
- Offer three setup actions:
  - `Sign in with xAI`
  - `Use device code`
  - `Use API key`
- Use copy that says "eligible Grok or X subscription account" until exact account eligibility is confirmed.
- Show which account/profile is active after OAuth.
- Keep UI state production-only; do not add temporary debug or test UI.
- Make errors actionable without exposing raw tokens or full provider responses.

### 6. Landing Page Update

Update the landing site to list xAI/Grok as a native provider.

Expected targets:

- `landing/components/landing/models.tsx`
- `landing/components/landing/brand-icons.tsx`
- `landing/lib/site.ts`

Tasks:

- Add an `xAI / Grok` provider card.
- Update the provider count in the models section.
- Change the OpenAI-compatible card copy so xAI is no longer presented as only a generic-compatible provider.
- Add `xAI` and `Grok` to site keywords.
- Keep the existing landing composition; if adding a reusable UI primitive, use ShadCN where possible.
- Do not claim xAI-native X Search, web search, code execution, image, or voice support on the landing page until those features are actually implemented.

### 7. Docs and Diagnostics

Update provider setup documentation and built-in diagnostics.

Expected targets:

- `docs/provider-setup-and-diagnostics.md`
- `README.md` provider list if present
- runtime/provider diagnostic helpers

Tasks:

- Document all three connection paths.
- Document that API keys come from the xAI console and can be represented by `XAI_API_KEY`.
- Document that OAuth eligibility depends on xAI account/subscription policy.
- Document device-code login for SSH, remote, and non-localhost contexts.
- Add troubleshooting for expired OAuth sessions, ineligible accounts, and stale app-data state.
- If stale app-data causes schema/runtime problems during development, wipe affected OS app-data state instead of adding backwards-compatible glue.

## Verification Plan

Use scoped checks. Do not run repo-wide Rust commands unless the implementation footprint makes it necessary.

TypeScript and UI:

- Run targeted unit tests for provider presets and provider credential schemas.
- Run targeted tests for provider settings UI if modified.
- Run `pnpm --dir landing lint`.
- Run `pnpm --dir landing build` if landing changes compile through Next.

Rust:

- Run one Cargo command at a time.
- Run scoped tests for provider parsing/resolution.
- Run scoped tests for provider credential storage/view conversion.
- Run scoped tests for xAI auth request building and token refresh with mocked HTTP.
- Run scoped tests for xAI model catalog and provider capabilities.
- Run scoped tests for xAI preflight request construction.
- Run scoped tests for xAI Responses request body normalization and stream parsing.

Manual or credentialed smoke tests, only when credentials are available:

- API key can run a short `grok-4.3` prompt through `/responses`.
- Browser OAuth can complete, persist, refresh, and run a short prompt.
- Device-code OAuth can complete, persist, refresh, and run a short prompt.
- Failed or ineligible accounts produce useful diagnostics.
- Secrets are not printed in logs, preflight output, UI, or error messages.

Tauri constraint:

- Do not open the app in a browser. Use Tauri/dev commands, unit tests, and any existing desktop/e2e harness only.

## Acceptance Criteria

- `xai` is selectable as a native provider in the app.
- Users can configure xAI with API key, browser OAuth, or device-code OAuth.
- `grok-4.3` appears in the model picker for xAI profiles.
- Agent runs use `https://api.x.ai/v1/responses`.
- OAuth sessions refresh without user intervention when refresh tokens remain valid.
- Provider preflight reports actionable status for configured xAI credentials.
- The landing page lists xAI/Grok as a native provider and no longer hides it under generic OpenAI-compatible support.
- New tests cover provider parsing, credential storage, auth request construction, runtime request shaping, and landing/provider schema updates.

## Open Questions Before Coding

- Does xAI provide a public OAuth client registration path for third-party desktop harnesses, or does Xero need a private client registration?
- Which subscription/account classes are officially eligible for API access through OAuth: Grok, SuperGrok, X Premium, X Premium+, or a narrower subset?
- Is xAI's device authorization endpoint publicly supported through discovery for all eligible accounts?
- Do OAuth access tokens authorize the same `/v1/responses` surface as API keys, or only a subset?
- Does xAI Responses streaming exactly match the current OpenAI Responses SSE parser?
- Which JSON Schema keywords are rejected in xAI tool definitions today?
- Should xAI-native `x_search`, web search, and code execution be a second milestone after the base provider is stable?
