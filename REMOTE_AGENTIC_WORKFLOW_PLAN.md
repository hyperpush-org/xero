# Remote Agentic Workflow — Implementation Plan (v2)

Control the Tauri/TUI session running on the user's computer from a **web app at `cloud.xeroshell.com`**. The browser is a thin portal; the computer remains the source of truth for agents, sessions, tools, and storage. Connectivity is anywhere via the cloud relay. Authentication for both surfaces reuses the **existing server-owned GitHub OAuth flow** (`Xero.GitHubAuth`).

> **Status:** Part 1 of the previous plan (QR pairing + Phoenix relay) is already implemented. This v2 replaces the QR/pairing-token model with GitHub-OAuth account linking and replaces the planned Expo app with a TanStack Start web client. Parts of Part 1 are salvageable; the auth surface and a chunk of the schema must be reworked. The "what to keep / change / delete" table at the end of each backend phase makes the rewrite scope explicit.

## Architecture at a glance

```
[Browser @ cloud.xeroshell.com]  <— WSS —>  [Phoenix relay (server/)]  <— WSS —>  [Tauri/TUI desktop]
   (TanStack Start, GitHub-OAuth                       |                              |
    session cookie)                                    |                       xero-remote-bridge
                                                       |                              |
                                              identity = GitHub user            xero-agent-core runtime
```

- **Relay** stays a stateless byte pump with auth + presence. Never sees payload contents.
- **Identity is a GitHub user.** Account = one row keyed by `github_user_id`. All desktops AND all browser sessions belonging to that GitHub user share the account; no per-device pairing handshake.
- **Desktop bridge** is the existing `xero-remote-bridge` crate, hosted by `xero-agent-core` so both Tauri and TUI consume it identically. It authenticates to the relay with a desktop JWT obtained at the end of GitHub OAuth.
- **Web client** is a new TanStack Start app at `cloud.xeroshell.com`. It authenticates to the relay with a browser session JWT obtained at the end of the same GitHub OAuth flow.
- **Session visibility model**: per-session `remote_visible` flag on the desktop; default `false`. Only visible sessions are advertised to web clients. (Renamed from `mobile_visible`; covers any non-local viewer.)

## Confirmed design choices

| Decision | v1 (superseded) | v2 (this plan) |
|---|---|---|
| Connectivity | Cloud relay | Cloud relay (unchanged) |
| Remote client | Expo iOS/Android app | TanStack Start web app at `cloud.xeroshell.com` |
| Auth | QR pairing token + Ed25519 keypair, long-lived device JWT | Reuse `Xero.GitHubAuth` for both desktop and browser; server issues short-lived bearer JWTs scoped to the GitHub account |
| Account model | One account, paired devices | One account per GitHub user; any number of desktops + browser sessions auto-join |
| Per-session sharing | Toggle on desktop (`mobile_visible`) | Toggle on desktop (`remote_visible`); default off |
| Remote-started sessions | Auto-marked visible | Auto-marked `remote_visible = true` |
| Agent management on web | Out of scope | Out of scope (sessions only) |

---

# Part 1 — Backend rewrite (no UI, no TanStack Start)

The existing backend will be reworked in place. Each phase includes a **"Salvage / Change / Delete"** block so the diff against the current tree is explicit.

## Phase A — Account model & auth surface

Replace QR-pairing auth with GitHub-OAuth account linking. The relay still issues device JWTs; what changes is *how* a device gets one.

**Schema (Ecto migrations — new migrations only; do not edit historical ones)**
- `accounts` — keep, add `github_user_id BIGINT UNIQUE NOT NULL`, `github_login TEXT`, `github_avatar_url TEXT`. Account is keyed by `github_user_id`; an account is created on first OAuth success and reused for every subsequent desktop or browser sign-in by the same GitHub user.
- `devices` — keep, but `kind ENUM('desktop','web')` (was `'desktop','mobile'`). Drop `public_key` requirement (no longer used for E2E signature challenges; OAuth handles identity). Keep `name`, `last_seen`, `revoked_at`. Add nullable `user_agent` for web devices.
- `pairings` — **drop entirely**. Run a migration that drops the table. The QR pairing flow goes away.
- `github_auth_sessions` — already exists for desktop OAuth. Extend or add a parallel `web_auth_sessions` (cookie-bound) — see below.

**HTTP endpoints**
- `POST /api/github/login` — already exists. Keep; this is the OAuth kickoff. Add a `kind=desktop|web` query/body param so the callback knows what to issue.
- `GET /auth/github/callback` — already exists. Modify the success path to:
  1. Upsert an `accounts` row keyed by `github_user_id`.
  2. Upsert a `devices` row (kind = `desktop` or `web`) for this surface.
  3. Issue a relay JWT (`account_id`, `device_id`, `kind`, short TTL — say 30 min) and:
     - For `kind=desktop`: return it via the existing desktop polling endpoint (`GET /api/github/session`), unchanged contract.
     - For `kind=web`: set an HttpOnly secure session cookie on `cloud.xeroshell.com` plus a CSRF token, then 302 back to the web app.
- `POST /api/relay/token/refresh` — **new.** Body: existing valid relay JWT (or web session cookie). Returns a fresh relay JWT. Both desktop and web call this before expiry. Replaces the long-lived device-JWT model.
- `DELETE /api/github/session` — already exists for desktop. Keep. Add web equivalent that clears the cookie and revokes the corresponding `devices` row.
- `POST /api/devices/:id/revoke` — keep (desktop or web can revoke any of the account's devices). Authed via relay JWT.
- `GET /api/devices` — keep, returns the account's devices.

**Channels**
- Socket `/socket/desktop` — keep. JWT in connect params (now short-lived, refreshed on the fly). On connect, joins `desktop:<computer_id>` where `computer_id` = `device_id` for the desktop row.
- Socket `/socket/web` — **rename** of `/socket/mobile`. JWT in connect params (or read from the session cookie via a websocket auth handshake). On connect, joins `account:<account_id>`. To attach to a session: `phx_join` on `session:<computer_id>:<session_id>`, forwarded to the owning desktop for authorization (must be `remote_visible = true`).
- Payload frames remain opaque blobs forwarded by `session_id`/`computer_id`.

**CSRF / cookie hygiene for the web flow**
- Cookie scope: `Domain=.xeroshell.com`, `Secure`, `HttpOnly`, `SameSite=Lax` (Lax allows the OAuth-callback redirect to land with the cookie attached).
- CSRF token issued alongside the cookie, required on all state-changing endpoints from the web app.

**Other**
- Relay JWT signing key in Phoenix config, rotated via env var. (Unchanged.)
- `Xero.RateLimiter` continues to throttle per-channel; coalesce if slow consumer. (Unchanged.)
- Telemetry counters: now `oauth_logins_total{kind}` instead of `pairings_per_sec`.

**Acceptance**
- Two CLI clients (one playing desktop, one playing web) can each complete the OAuth flow against a stub GitHub (use `Xero.GitHubAuth`'s existing test seam), receive a relay JWT, send opaque frames in both directions, reconnect, refresh JWTs, resume.
- Two desktops linked to the same GitHub user appear under one account; a web client sees both.
- Revoked device cannot reconnect.
- `mix test` covers all OAuth paths including: existing user, brand-new user, GitHub error, expired token, refresh.

**Salvage / Change / Delete vs. current tree**

| File | Action |
|---|---|
| `server/lib/xero/github_auth.ex`, `server/lib/xero/github_auth/session.ex` | **Salvage.** Extend `complete/2` to also upsert account+device and mint a relay JWT. Add `kind` param. |
| `server/lib/xero_web/controllers/github_auth_controller.ex` | **Change.** Handle `kind=web` callback path (cookie + redirect) in addition to the existing desktop polling path. |
| `server/lib/xero/remote/account.ex` | **Change.** Drop pairing-token columns from schema; add `github_user_id`, `github_login`, `github_avatar_url`. |
| `server/lib/xero/remote/device.ex` | **Change.** `kind` becomes `desktop|web`. Drop `public_key` requirement. |
| `server/lib/xero/remote/pairing.ex` | **Delete.** No more pairing tokens. |
| `server/lib/xero/remote/jwt.ex` | **Salvage.** Keep token shape; shorten TTL; add refresh helper. |
| `server/lib/xero_web/controllers/remote_pair_controller.ex` | **Delete.** Whole controller goes. |
| `server/lib/xero_web/controllers/remote_device_controller.ex` | **Salvage.** Just remove pairing-specific code. |
| `server/lib/xero_web/plugs/remote_auth_plug.ex` | **Salvage.** Accepts the new relay JWT shape. |
| `server/lib/xero_web/channels/remote_mobile_socket.ex`, `remote_mobile_account_channel.ex` | **Rename** to `remote_web_socket.ex` / `remote_web_account_channel.ex`. Auth changes from device JWT to relay JWT (which is itself derived from OAuth). |
| `server/lib/xero_web/channels/remote_desktop_socket.ex`, `remote_desktop_channel.ex`, `remote_session_channel.ex` | **Salvage.** Re-point at new auth shape; payload forwarding unchanged. |
| `server/lib/xero_web/router.ex` | **Change.** Drop pair routes; add `/api/relay/token/refresh`; update OAuth callback to branch on `kind`. |
| Existing tests for pairing | **Delete.** Replace with OAuth-flow tests. |

---

## Phase B — Desktop bridge rewrite (`xero-remote-bridge`)

The crate exists. Strip the QR-pairing machinery, repoint it at the new OAuth-derived JWT flow. Keep the WSS plumbing, envelopes, and the in-process API surface.

**Responsibilities**
- Trigger and complete the existing desktop GitHub OAuth flow (already wired via the Tauri shell + `Xero.GitHubAuth`'s polling endpoint). On success, store the relay JWT in the OS keychain (`keyring` crate) and fall back to an encrypted file in `~/.config/xero/` on headless Linux.
- Maintain one outbound WSS to `wss://cloud.xeroshell.com/socket/desktop` (or whatever the relay URL ends up being). Auto-reconnect with jittered backoff. Heartbeat every 30s. **Refresh the relay JWT before it expires** via `POST /api/relay/token/refresh`.
- In-process API for the rest of the app:
  - `sign_in_with_github() -> AuthStatus` (starts the existing OAuth flow if not already signed in).
  - `sign_out()` — clears stored JWT and calls `DELETE /api/github/session`.
  - `set_session_visibility(session_id, bool)` — flips `remote_visible`.
  - `list_paired_devices()` → renamed `list_account_devices()`.
  - `revoke_device(device_id)`.
  - `subscribe_inbound() -> Stream<InboundCommand>`.
  - `forward(session_id, runtime_event)`.
- Internally listens to the existing `subscribe_runtime_stream` firehose; for every event whose `session_id` has `remote_visible = true`, wrap in the envelope below and push to the channel topic.
- Persist `remote_visible` per session in the existing project-state store.

**Wire envelope (msgpack — unchanged shape, field renames)**
```
{
  v: 1,
  seq: u64,
  computer_id: string,
  session_id: string,
  kind: "snapshot" | "event" | "presence" | "session_added" | "session_removed",
  payload: <reuse existing runtime event types from subscribe_runtime_stream>
}
```
Inbound from web: `kind = "send_message" | "start_session" | "resolve_operator_action" | "cancel_run"`.

**Tauri commands (called by future UI; defined now)**
- `bridge_status() -> { connected, relay_url, signed_in, account: { github_login, avatar_url }, devices: [...] }`
- `bridge_sign_in()` — starts the GitHub OAuth flow.
- `bridge_sign_out()`
- `bridge_revoke_device(device_id)`
- `set_session_remote_visibility(session_id, bool)`

**TUI parity**
- Same commands exposed via `xero-cli`:
  - `xero-cli remote login` (kicks off GitHub OAuth, prints the URL the user opens in a browser)
  - `xero-cli remote logout`
  - `xero-cli remote devices [list|revoke <id>]`
  - `xero-cli remote visibility <session> on|off`

**No UI in this phase.** Commands return data; rendering comes later.

**Acceptance**
- `cargo test -p xero-remote-bridge` covers envelope encode/decode, reconnect, JWT refresh, `remote_visible` gating.
- Integration test: spin up the relay locally, complete the OAuth flow against a `Xero.GitHubAuth` test seam, start a runtime with one session, flip `remote_visible = true`, observe frames on the relay channel via a CLI client.

**Salvage / Change / Delete vs. current crate**

| Piece | Action |
|---|---|
| WSS client, reconnect logic, heartbeat | **Salvage.** |
| Msgpack envelope encode/decode | **Salvage.** Rename `mobile_*` fields to `remote_*` where they appear. |
| Ed25519 keypair handling, pairing-payload assembly | **Delete.** No more pairing. |
| Keychain storage | **Salvage.** Store the relay JWT (and refresh token if we add one) instead of a private key. |
| `pair_initiate` API and any pairing payload formatting | **Delete.** |
| `subscribe_runtime_stream` integration | **Salvage.** |

---

## Phase C — End-to-end auth flow (no UI)

Prove the GitHub-OAuth → relay-JWT → live channel flow works for both desktop and web before any browser screen is wired.

**Desktop**
- The existing OAuth flow already works in the Tauri shell — extend it so a successful callback also yields a relay JWT and the bridge auto-connects.
- `xero-cli remote login` prints the GitHub authorize URL; user opens it; CLI polls `GET /api/github/session` (existing) until the relay JWT lands; CLI then connects to `/socket/desktop`.

**Web-simulator CLI** (lives in `xero-cli`, replaces the v1 `mock-phone` binary)
- New small Rust binary: `xero-cli mock-web ...`. It pretends to be the future browser app.
  - `mock-web login` — starts an OAuth flow with `kind=web`, prints the URL, polls a session endpoint, stores the resulting cookie (or relay JWT) in a tmp file.
  - `mock-web connect` — opens a WSS to `/socket/web`.
  - `mock-web list-sessions` — lists sessions visible across all desktops in the account.
  - `mock-web attach <computer_id> <session_id>` — joins `session:*` and streams events.
  - `mock-web send <computer_id> <session_id> <message>` — pushes a `send_message` frame.
  - `mock-web start <computer_id> <agent> <prompt>` — pushes `start_session`.
  - `mock-web devices` / `mock-web revoke <id>` — manage account devices.
- This binary is **the** integration-test harness; it stays in tree as the canonical CLI debugger for the web side.

**Acceptance**
- E2E happy path: desktop runs `xero-cli remote login`, OAuth completes, relay connection up → in another shell `xero-cli mock-web login` (same GitHub user), `mock-web list-sessions` sees the desktop, `mock-web attach` streams events from a real session.
- Two-desktops-one-account: a second desktop signing in with the same GitHub user joins the same account; web client sees both desktops' sessions.
- Negative paths: expired relay JWT auto-refreshes; revoked device's stored JWT fails to reconnect; mismatched `kind` on a socket is rejected; web client without `remote_visible` for a target session gets a clean 403.

---

## Phase D — Session forwarding & command routing

Same as v1's Phase D conceptually; only the auth identity changes. The bridge already forwards `remote_visible` events outbound (Phase B). This phase completes the inbound path and the "start a session from the web" flow.

**Inbound command routing**
- Bridge subscribes to all `session:<computer_id>:*` topics it owns.
- Inbound `send_message` → calls the same handler the desktop UI uses. Reuse, don't reimplement.
- Inbound `resolve_operator_action` → reuses `resolve_operator_action.rs`.
- Inbound `cancel_run` → reuses `stop_runtime_run.rs`.
- Inbound `start_session` → calls `start_runtime_session.rs` then sets `remote_visible = true` for the new session.

**Authorization**
- For every inbound command, the bridge re-verifies that the originating relay JWT belongs to a non-revoked device on the same account as the desktop, and that the target session is `remote_visible = true`. Relay alone cannot impersonate — the bridge re-checks.

**Snapshot-on-attach**
- When a web client joins `session:*`, the desktop emits a `snapshot` frame with the current transcript (last N events or full, depending on size), followed by live `event` frames keyed off the snapshot's `seq`.
- On reconnect, web client supplies `last_seq`; desktop replays from buffer if available, otherwise sends a fresh snapshot.

**Multi-client**
- Supported by construction (Phoenix Channels fan-out). All browser tabs across all of the user's devices see the same events. Inbound commands are accepted from any of them.

**Acceptance**
- `mock-web start <computer_id> <agent> <prompt>` creates a session on the chosen desktop, returns its id, immediately attaches, streams the full agent turn including tool calls, and the new session shows up in the desktop's normal session list with `remote_visible = true`.
- `mock-web send` while the desktop UI has the same session open: both surfaces see the message and the agent's reply.
- Operator-action prompt sent from an agent surfaces on both desktop and `mock-web`; resolving on either dismisses it on both.

---

## Backend exit criteria

By the end of Phase D, without any browser UI work:
- A user can drive the entire system from `xero-cli mock-web` on any second machine: sign in with GitHub, list desktops/sessions, attach, send messages, start new sessions, approve tool calls — all routed through the production relay against a real Tauri/TUI runtime, identity-checked end-to-end against GitHub.
- All happy and negative auth paths covered by tests.
- Telemetry visible for relay throughput, OAuth logins, and JWT refresh rates.

Ship the backend rewrite (Phases A–D) before any browser work begins.

---

# Part 2 — UI (desktop devices UX + TanStack Start web app)

UI phases assume Part 1 v2 is complete and stable. They only render data backend commands already return.

## Phase E — Desktop "Cloud account" UI

Replace the "Pair a phone" idea with a simple sign-in panel.

- Settings → **"Cloud account"** section.
- If signed out: a **"Sign in with GitHub"** button → invokes `bridge_sign_in()`, which uses the existing OAuth flow → on success the panel shows the GitHub login + avatar + a list of the account's devices (this desktop highlighted).
- If signed in: GitHub login + avatar, list of devices (other desktops and browser sessions, with last-seen and revoke), and a "Sign out" button.
- ShadCN components per `CLAUDE.md`.
- Same surface in TUI (text list of devices + `xero-cli remote login` / `remote logout` / `remote devices` commands — already exist from Phase C, just promote to user-facing).

## Phase F — Per-session "Share to web" toggle

- Add a small cloud icon to each session row in the existing session list (Tauri + TUI). Click toggles `set_session_remote_visibility`.
- Active/visible state reflected as a subtle row indicator.
- No new screens; decoration on existing rows.

## Phase G — Web app scaffold + auth (`cloud/`)

New top-level `cloud/` directory. Deployed to `cloud.xeroshell.com`.

**Stack**
- TanStack Start (Vinxi-based, React, file-based routing).
- TanStack Router for type-safe routes.
- TanStack Query for cache/invalidation around the WS event stream.
- ShadCN UI (mirror the desktop's component set so visuals match).
- `phoenix` npm package for Channels.
- Auth: server-issued HttpOnly cookie scoped to `.xeroshell.com`; CSRF token in a non-HttpOnly cookie or response header.

**Deploy**
- Fly.io app alongside the Phoenix relay (or a sibling app), CNAME `cloud.xeroshell.com`.
- TLS via Fly's managed certs.
- Same `fly.toml` pattern as the existing `landing/` app — reuse that template.

**Routes (initial)**
- `/` — if not signed in, marketing-lite landing with a "Sign in with GitHub" button → `POST /api/github/login?kind=web` → 302 to GitHub → callback → 302 back to `/sessions`.
- `/signin` — explicit sign-in page (same button), used as the OAuth return-to fallback.
- `/settings` — account info (GitHub login/avatar), device list, "Sign out of this browser" (clears cookie + revokes the corresponding `devices` row).

Hard limit on screen count in this phase to keep the surface tight.

## Phase H — Web app: sessions & agents

- `/sessions` — list of all `remote_visible` sessions across every desktop on the account. Each row shows the originating desktop, session name, last activity, and a presence dot for the desktop. Pull-to-refresh equivalent (manual refresh + auto via WS).
- `/sessions/[computerId]/[sessionId]` — chat-style transcript reusing the existing event vocabulary from `subscribe_runtime_stream`. Composer at the bottom. Inline operator-action prompts (approve/deny inline). Snapshot-on-attach + delta streaming via `last_seq`.
- `/agents` — read-only list of agent definitions per desktop on the account. Tap → "Start session with prompt" → backend creates the session on the chosen desktop and the new id appears in `/sessions`.
- All data flows through the relay WS; REST is only used for auth/device management.

## Phase I — Notifications

- Web Push (VAPID) subscriptions registered with the relay on web sign-in.
- Relay sends a push when an `operator_action_required` event arrives for a `remote_visible` session and no live web socket is currently connected for that account.
- Click notification → deep-link into `/sessions/[computerId]/[sessionId]`.
- Defer until Phase H is solid. Web Push requires VAPID keys and SW registration but no app-store overhead, which makes it materially cheaper than the v1 Expo Push plan.

---

# Operational notes

- **Relay hosting**: deploy via the existing `server/fly.toml` to Fly.io. Phoenix Channels handle horizontal scale with PG-backed presence.
- **Web app hosting**: new Fly app for `cloud.xeroshell.com`. Same region as the relay.
- **DNS**: add the `cloud` CNAME to the `xeroshell.com` zone. Confirm cert issuance via Fly.
- **Cookie domain**: scope auth cookie to `.xeroshell.com` so the marketing site (`xeroshell.com`) and the cloud app share auth state if we ever want one-click handoff. SameSite=Lax.
- **GitHub OAuth app**: the existing OAuth client already covers desktop. Add `https://cloud.xeroshell.com/auth/github/callback` as an additional authorized callback in the GitHub OAuth app, *or* keep a single callback on the API host and have it 302 to either desktop polling or `cloud.xeroshell.com` based on the `kind` param stored against the OAuth flow id.
- **Multi-account future-proofing**: still keyed by `github_user_id`, so a single GitHub user is a single account. Org-level accounts can layer on later via a `memberships` table.
- **Keychain availability** (desktop side): unchanged — `keyring` crate covers macOS/Linux/Windows with an encrypted-file fallback for headless Linux.
- **No backwards compatibility constraints** per project policy. Drop the pairing tables in the same migration that adds the new columns.

# Open questions

1. **Relay payload limits** — large tool outputs (e.g., a 200KB file read) shouldn't blow up the browser. Suggest: truncate to ~64KB on the bridge side for the web channel with a "view full on desktop" sentinel.
2. **Session deletion from web** — out of scope for v1; web can hide but not delete. (Unchanged.)
3. **Anonymous-session-on-web** (i.e., viewing a session without signing in via a share link) — defer. v1 requires GitHub sign-in.
4. **GitHub-org gating** — should access be limited to specific GitHub orgs or invite-only? Default to "any GitHub user can sign in" for now; an env-configured allowlist is one line if we need it later.
