# CI/CD and OTA Follow-Up

Reader: the maintainer preparing Xero's first production release.

Post-read action: configure the missing external services, run the first signed release, and verify that desktop clients are forced through the over-the-air update flow on launch.

## Current State

CI/CD scaffolding is in place for the landing site, cloud app, Elixir server, and Tauri desktop app.

The release pipeline is designed to publish desktop update artifacts to this repository's GitHub Releases. The app checks the latest release metadata on startup, blocks normal startup when an update is available, shows the full-screen update UI with progress, installs the update, and restarts.

The remaining work is mostly external setup: secrets, signing identities, Fly apps, and a first release rehearsal.

## Required GitHub Secrets

Add these repository secrets before running the release workflow:

- `TAURI_SIGNING_PRIVATE_KEY`: contents of the generated Tauri updater private key.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional. Leave unset if the generated updater key has no password.
- `FLY_API_TOKEN`: fallback Fly deploy token for the web/server apps.
- `FLY_SERVER_TOKEN`: optional server-specific Fly token.
- `FLY_LANDING_TOKEN`: optional landing-specific Fly token.
- `FLY_CLOUD_TOKEN`: optional cloud-app-specific Fly token.
- `APPLE_CERTIFICATE`: base64-encoded Apple Developer certificate for macOS signing.
- `APPLE_CERTIFICATE_PASSWORD`: password for the Apple certificate.
- `APPLE_ID`: Apple ID used for notarization.
- `APPLE_PASSWORD`: app-specific password for notarization.
- `APPLE_TEAM_ID`: Apple Developer team ID.
- `APPLE_SIGNING_IDENTITY`: optional explicit signing identity.
- `AZURE_CLIENT_ID`: Azure service principal client ID for Windows signing.
- `AZURE_TENANT_ID`: Azure tenant ID.
- `AZURE_SUBSCRIPTION_ID`: Azure subscription ID.

Add these repository variables for Windows Trusted Signing:

- `AZURE_TRUSTED_SIGNING_ENDPOINT`
- `AZURE_TRUSTED_SIGNING_ACCOUNT_NAME`
- `AZURE_TRUSTED_SIGNING_CERT_PROFILE_NAME`

Current repository check: the updater signing secret and macOS signing secrets are configured at the repo level. Windows signing and platform-specific deploy secrets still need to be added before the full release workflow can complete.

## macOS Signing Setup

Xero follows the normal Tauri v2 signing path for macOS: Tauri imports a base64-encoded Developer ID Application `.p12` certificate in CI, signs the bundle, notarizes with Apple credentials, and emits updater artifacts signed with the Tauri updater private key.

Use a paid Apple Developer Program account for public distribution. A free Apple Developer account can sign for development, but cannot notarize Developer ID builds for other users.

On the Mac that owns the signing identity:

1. Create or download a `Developer ID Application` certificate from Apple Developer > Certificates, IDs & Profiles.
2. Open the `.cer` so it lands in Keychain Access under `login` > `My Certificates`.
3. Confirm the identity is visible:

```bash
security find-identity -v -p codesigning
```

4. Export the private-key-backed certificate from Keychain Access as a password-protected `.p12`.
5. Add the Apple secrets with GitHub CLI:

```bash
openssl base64 -A -in /path/to/xero-developer-id.p12 | gh secret set APPLE_CERTIFICATE --repo hyperpush-org/xero
gh secret set APPLE_CERTIFICATE_PASSWORD --repo hyperpush-org/xero
gh secret set APPLE_ID --repo hyperpush-org/xero
gh secret set APPLE_PASSWORD --repo hyperpush-org/xero
gh secret set APPLE_TEAM_ID --repo hyperpush-org/xero
```

`APPLE_PASSWORD` must be an Apple app-specific password, not the account login password.

## Updater Signing Setup

The public updater key is already in `client/src-tauri/tauri.conf.json`. The matching private key must be stored as `TAURI_SIGNING_PRIVATE_KEY`; otherwise the release workflow cannot create `.sig` files for update artifacts.

If the matching private key is available:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo hyperpush-org/xero < /path/to/xero-updater.key
```

The current `~/.tauri/xero.key` was generated without a password, so leave `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` unset.

If the private key is lost and no public release has shipped yet, generate a new updater key and replace the public key in `client/src-tauri/tauri.conf.json` before the first release. If a public release has shipped, do not rotate the key without a bridge release signed by the old key.

## macOS CI Smoke Test

After the Apple secrets are configured, run the focused signed macOS workflow before running the full release workflow:

```bash
gh workflow run "macOS Signed Build" --repo hyperpush-org/xero
```

This workflow builds both `aarch64-apple-darwin` and `x86_64-apple-darwin`, notarizes the app, verifies `codesign`, verifies Gatekeeper assessment with `spctl`, checks that the bundled `idb_companion` resource is present, and uploads the signed `.dmg`, updater `.app.tar.gz`, and `.sig` artifacts.

## Required Production Secrets

Set the server's production runtime secrets in Fly before the first deploy:

- `DATABASE_URL`
- `SECRET_KEY_BASE`
- GitHub OAuth client values used by the app
- Any provider API keys, model credentials, or runtime secrets required by production configuration

Generate `SECRET_KEY_BASE` with the Phoenix secret generator from the server project. Use a production Postgres database URL, not a local development database.

## Fly Setup

Create or verify the Fly apps before the release workflow deploys:

- Server app: `xero-server`
- Landing app: `xero-landing`
- Cloud app: `xero-cloud`

Confirm the server app has:

- A reachable production Postgres database
- Required secrets set
- Public hostname configured
- Health checks passing at `/api/health`

Confirm the landing app has:

- Correct Fly app name
- Public hostname configured
- Any production environment variables needed by the landing build

Confirm the cloud app has:

- Fly app `xero-cloud` created
- Public hostname `cloud.xeroshell.com` attached in Fly certificates
- Vercel DNS record for `cloud.xeroshell.com` pointing at the Fly target shown by `fly certs setup`
- `XERO_SERVER_URL` pointing at the production Phoenix relay/auth API
- PWA resources reachable over HTTPS (`/manifest.webmanifest`, `/sw.js`)

## First Release Rehearsal

Run the first release from a clean commit after all secrets are configured.

1. Confirm the desktop app version is the version you want to ship.
2. Run `pnpm release:push X.Y.Z` to push the current branch and a matching `vX.Y.Z` tag.
3. Let the release workflow create a draft GitHub Release.
4. Confirm the workflow uploads installers, updater archives, signatures, and `latest.json` to the same repository release.
5. Confirm the workflow publishes the release only after all platform artifacts are uploaded.
6. Download each platform installer from the release and smoke-test installation.

If the workflow is run manually, use the version that matches the desktop app version.

## OTA Update Validation

After one signed release exists, validate over-the-air updates with a second version.

1. Install the first released desktop build.
2. Bump the desktop app version.
3. Create and publish a second signed release.
4. Open the first installed app.
5. Confirm the app shows the full-screen update screen before normal startup.
6. Confirm the progress bar moves while the update downloads and installs.
7. Confirm the app restarts into the new version.
8. Confirm the app does not show the update screen again after it is current.

Test this on macOS Apple Silicon, macOS Intel, Windows, and Linux before treating OTA as production-ready.

## Signing Checks

Before shipping publicly:

- Verify macOS builds are signed and notarized.
- Verify Windows installers are Authenticode-signed.
- Verify the Tauri updater signature is generated after Windows signing, so the updater signature matches the final installer bytes.
- Verify Linux AppImage update artifacts are present and referenced in `latest.json`.

## Deployment Checks

After each release:

- Open the landing production URL and confirm it points at the production server.
- Open `https://cloud.xeroshell.com` and confirm the Cloud app loads over Fly, not the Vercel fallback 404.
- Check the server health endpoint.
- Check server logs for boot errors and migration errors.
- Confirm desktop app startup can reach the production API.
- Confirm the GitHub Release contains one `latest.json` with entries for every supported platform.

## Notes for Future Changes

When adding a new desktop platform or installer format, update the release workflow and the updater metadata generation together. The updater will only offer builds that are present in `latest.json`.

When rotating the Tauri updater key, update the public key in the desktop app config, ship a bridge release signed by the old key, then use the new private key for subsequent releases. Otherwise existing clients may be unable to verify updates.
