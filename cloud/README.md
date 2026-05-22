# Cloud

Clean TanStack Start shell for the cloud surface.

## Commands

```bash
pnpm dev
pnpm check
pnpm build
```

## Production

The production app is configured in `fly.toml` as `xero-cloud` and is intended to run at `https://cloud.xeroshell.com`.

Deploy from the repo root so the Docker build context includes `packages/ui`:

```bash
fly deploy --remote-only --config cloud/fly.toml
```

The Fly app reads `XERO_SERVER_URL` for the Phoenix relay/auth API origin.
