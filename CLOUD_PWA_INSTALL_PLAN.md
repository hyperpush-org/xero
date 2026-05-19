# Cloud PWA Installability Plan

## Reader And Goal

This plan is for the engineer making the Xero cloud app installable as a Progressive Web App. After reading it, they should be able to add manifest metadata, install-ready assets, a conservative service worker, and user-facing install affordances without touching the Tauri desktop install path or adding temporary/debug UI.

The target outcome is simple: a user visiting Xero Cloud on a supported device can install it from the browser and later launch it from their dock, home screen, launcher, or app list as a standalone app.

## Research Summary

Modern PWA installability is browser-specific, but the durable baseline is a web app manifest with correct app identity, icons, scope, start URL, and display mode. A service worker is still useful for a reliable PWA experience, but Xero Cloud must treat offline support carefully because authenticated session data is private and realtime relay traffic is network-bound.

Chromium browsers can expose the `beforeinstallprompt` event after the browser decides the app is installable. That event is not a cross-browser standard and can only be used once per fired event, so the UI must be progressive enhancement rather than the only install path.

iPhone and iPad users generally install through Safari's share sheet and "Add to Home Screen" / "Open as Web App" path. Chrome and Edge on iOS cannot show a Chromium-style PWA install prompt, so Xero should show platform-specific instructions there instead of an inert install button.

Sources:

- MDN, Making PWAs installable: https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Guides/Making_PWAs_installable
- MDN, Trigger installation from your PWA: https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/How_to/Trigger_install_prompt
- web.dev, Installation prompt: https://web.dev/learn/pwa/installation-prompt/
- Apple Support, Turn a website into an app in Safari on iPhone: https://support.apple.com/guide/iphone/turn-a-website-into-an-app-iphea86e5236/ios

## Current App Baseline

Xero Cloud is a TanStack Start app running on Vite and Nitro. It already uses the shared ShadCN-compatible `@xero/ui` component package and has a dedicated cloud package with build, test, format, lint, and check scripts.

The cloud app does not currently appear to have PWA manifest files, service worker registration, install prompt handling, or app icon asset coverage under its own static assets. The landing app has several icon assets that may be reusable as visual source material, but the cloud app should own the generated PWA assets it serves.

## Product Decisions

Ship installability first, not full offline product behavior. The installed app should launch cleanly, preserve the app-like standalone surface, and show a useful offline state when the network is unavailable. It should not pretend remote sessions are usable offline.

Keep the PWA scope on the cloud origin only. This must not alter Tauri desktop packaging, `.xero/` legacy state, or desktop app-data storage.

Do not cache private session content. Static hashed assets and a non-sensitive offline fallback are allowed. Auth responses, API responses, OAuth callback output, relay endpoints, session transcripts, and any request using credentials must stay network-only.

Expose install UI as a normal user feature. Use ShadCN components where possible, keep it polished, and do not add temporary banners, debug panels, or developer-only install state UI.

## Scope

In scope:

- Web app manifest for Xero Cloud.
- Raster app icons and maskable icons for desktop, Android, and iOS home screen usage.
- Apple-specific home screen metadata.
- Service worker registration with conservative static asset caching.
- User-facing install affordance for Chromium-style prompt support.
- User-facing manual instructions for Safari/iOS and unsupported prompt cases.
- Tests for install state detection, prompt flow, registration behavior, and cache exclusions.
- Production/deployment header checks for manifest, icons, and service worker files.

Out of scope:

- Push notifications.
- Background sync.
- Offline transcript editing.
- Native app store distribution.
- Changes to the Tauri desktop app packaging.
- Any backwards-compatibility layer for stale cloud PWA state. This is new functionality.

## Phase A - Manifest And Assets

Add a cloud-owned web app manifest with:

- `id`: stable cloud app identity for the deployed origin.
- `name`: `Xero Cloud`.
- `short_name`: `Xero`.
- `description`: concise install prompt copy.
- `start_url`: a stable cloud route with a small install-source query parameter if analytics needs it.
- `scope`: cloud root.
- `display`: `standalone`.
- `display_override`: include `window-controls-overlay` only if the UI is verified in that mode; otherwise keep it conservative.
- `theme_color` and `background_color`: match the Dusk theme tokens.
- `icons`: include at least 192x192 and 512x512 PNGs, with maskable variants.
- `screenshots`: optional but recommended for richer Chromium install dialogs after the baseline is passing.

Generate or adapt branded raster assets:

- Favicon sizes for browser tabs.
- Apple touch icon.
- Android/Chromium app icons.
- Maskable icon with safe padding so it survives circular, squircle, and rounded-square crops.

Wire the manifest and icon metadata into the cloud root document head. Include Apple mobile web app tags and status bar metadata so the installed iOS experience has the expected standalone behavior.

Acceptance:

- The built cloud HTML includes manifest and icon links.
- Manifest JSON validates and contains no placeholder names, placeholder colors, or broken icon references.
- All icon files are cloud-owned static files and load with image content types.
- App title and installed name show as Xero Cloud/Xero rather than the generic `Cloud`.

## Phase B - Service Worker

Start with a hand-written service worker or a small generated service worker only after confirming it works cleanly with TanStack Start, Vite, Nitro, and the current Vite version. Avoid a plugin if it forces broad SSR build changes.

Register the service worker from the cloud client entry only in browser contexts. In development, either disable registration or use a clearly separated dev registration path so stale local workers do not mask runtime issues.

Caching strategy:

- Precache versioned static build assets and non-sensitive icon/manifest assets.
- Cache the offline fallback page.
- Use network-only behavior for OAuth, auth session checks, API calls, relay token refresh, WebSocket-adjacent endpoints, and any request with credentials.
- Use network-first for top-level navigations only if the fallback is a static offline page that contains no user data.
- Never cache SSR HTML that may include authenticated session state.
- On activate, clean up old Xero Cloud cache names.

Update behavior:

- Detect service worker updates and surface a ShadCN toast or compact top-bar action when a new version is ready.
- Let the user reload into the new version from that action.
- Do not force a mid-session reload while a run is active unless the user chooses it.

Acceptance:

- Production build registers a valid service worker.
- Refreshing the installed app loads normally.
- Losing network shows the offline fallback instead of a broken white screen.
- Dev builds are not polluted by stale production service workers.
- Tests prove auth/session/relay URLs are excluded from service worker caches.

## Phase C - Install UX

Add a small PWA install state module or hook that tracks:

- `beforeinstallprompt` availability.
- Whether the app is already running in standalone display mode.
- The `appinstalled` event where available.
- iOS Safari manual install eligibility.
- Unsupported or already-installed states.

Use this state to expose install affordances in durable product UI:

- On the signed-out/login screen, show an install action only when install is available or manual instructions are useful.
- In the signed-in app, add a compact install action in the top bar or account/device menu.
- Hide install actions when the app is already installed or running standalone.
- Never show an install prompt during GitHub OAuth redirect or while a blocking auth action is in flight.

Chromium flow:

- Capture `beforeinstallprompt`.
- Store the event in memory only.
- Trigger `prompt()` from a direct user action.
- Record accepted/dismissed state in local client state only if needed for suppressing immediate repeat prompts.
- Clear the stored event after one use.

iOS/Safari flow:

- Show manual instructions in a ShadCN Dialog or Drawer.
- Keep instructions short and device-specific.
- Explain the Safari share-sheet path and "Open as Web App" step.
- Do not render these instructions in standalone mode.

Acceptance:

- A supported Chromium browser can install Xero Cloud from the user-facing install action.
- iOS Safari users see useful manual instructions instead of a dead prompt button.
- The install UI disappears in standalone mode.
- The UI uses shared ShadCN components and contains no debug-only state readouts.

## Phase D - Routing, Auth, And Standalone Behavior

Verify installed-app launch behavior across the auth surface:

- Launching from the installed icon should open the cloud start URL and route signed-in users to sessions.
- Signed-out users should land on the normal GitHub sign-in surface.
- OAuth redirects should continue to complete in browser context and return to the cloud app.
- The canonical loopback redirect behavior used during local development should not leak into production install URLs.

Standalone layout checks:

- Ensure safe-area padding works on iPhone/iPad home screen launches.
- Keep top-bar controls reachable without browser chrome.
- Preserve responsive session navigation at phone, tablet, and desktop installed-app sizes.
- Avoid any text overflow in install dialogs, menu items, or update prompts.

Acceptance:

- Installed launch works for signed-in and signed-out users.
- OAuth does not get trapped in the installed app or lose its session cookie.
- Routes under the app scope load directly after installation.
- Mobile standalone mode has no clipped top or bottom controls.

## Phase E - Deployment Headers

Confirm the cloud deployment serves:

- Manifest with `application/manifest+json` or compatible JSON content type.
- Service worker JavaScript with a short or no-cache policy so updates are discovered promptly.
- Icons with long-lived immutable caching when filenames are content-stable or versioned.
- HTML with normal app cache headers that do not fight the service worker update model.

The deployed origin must be HTTPS. Localhost remains acceptable only for local verification.

Acceptance:

- Manifest, service worker, and icons load correctly from the deployed cloud origin.
- Browser installability audits report no missing manifest or icon issues.
- A service worker update reaches an already-installed app without requiring users to uninstall.

## Test Plan

Unit tests:

- Install hook captures and clears `beforeinstallprompt`.
- Install hook reports installed/standalone state from display-mode and iOS standalone signals.
- Install hook handles `appinstalled`.
- Install action does nothing when no prompt is available.
- iOS manual instruction state is shown only in browser mode.
- Service worker route classifier excludes auth, API, relay, OAuth, credentialed, and non-GET requests from caches.

Component tests:

- Login screen renders install affordance only when appropriate.
- Signed-in top-bar/menu renders install affordance only when appropriate.
- Manual install dialog text fits mobile widths.
- Update-ready toast/action renders and calls the reload path.

Build and static checks:

- `pnpm --dir ./cloud run test`
- `pnpm --dir ./cloud run check`
- `pnpm --dir ./cloud run build`

Manual verification:

- Chromium desktop: install from the in-app action, launch from the OS app list, confirm standalone mode.
- Android Chrome: install from the browser prompt or menu, launch from home screen, confirm icon and name.
- iOS Safari: add to home screen, enable Open as Web App, launch, confirm standalone mode.
- Offline: launch installed app without network and confirm only the non-sensitive offline fallback appears.
- Auth: sign in, close, relaunch installed app, and confirm normal session routing.

## Rollout Notes

Ship behind normal cloud deployment. There is no need for a desktop feature flag because this is cloud-only.

Treat service worker cache changes as security-sensitive. Any future expansion beyond static assets must include tests proving private remote-session data cannot be served from cache to the wrong browser state.

After the baseline is stable, consider follow-ups for richer install screenshots, installed-app analytics, push notifications, and optional notification permission onboarding. Those should be separate product decisions, not bundled into the first installability pass.
