# Cloud Domain Deployment

The Cloud app is deployed to Fly.io as `xero-cloud` and should be reached at:

```text
https://cloud.xeroshell.com
```

`xeroshell.com` uses Vercel DNS, so DNS records are managed in Vercel while the app runtime and TLS certificate are managed in Fly.

## Repo Configuration

- Cloud Fly config: `cloud/fly.toml`
- Cloud Dockerfile: `cloud/Dockerfile`
- Landing Cloud link: `NEXT_PUBLIC_CLOUD_URL`, defaulting to `https://cloud.xeroshell.com`
- CI deploy secrets: `FLY_CLOUD_TOKEN` or fallback `FLY_API_TOKEN`

## First-Time Fly Setup

Run these from the repo root after confirming the Fly organization/scope:

```bash
fly apps create xero-cloud
fly deploy --remote-only --config cloud/fly.toml
fly certs add cloud.xeroshell.com -a xero-cloud
fly certs setup cloud.xeroshell.com -a xero-cloud
```

`fly certs setup` prints the exact DNS target Fly expects. For a subdomain, this is usually a CNAME target. Add that record in Vercel DNS for `cloud.xeroshell.com`.

If using the Vercel CLI:

```bash
vercel dns add xeroshell.com cloud CNAME <target-from-fly-certs-setup>
```

Then verify:

```bash
fly certs check cloud.xeroshell.com -a xero-cloud
curl -I https://cloud.xeroshell.com
```

## API Dependency

The Cloud app uses `XERO_SERVER_URL` for GitHub OAuth, session refresh, device APIs, and relay websockets. In production, set it to the Phoenix relay/auth API origin. The current `cloud/fly.toml` value is:

```text
https://xeroshell.com
```

If the landing site owns the apex and does not proxy `/api/*` and websocket routes to Phoenix, change `XERO_SERVER_URL` to a dedicated API hostname such as `https://api.xeroshell.com` and configure the server app with:

```text
CORS_ORIGINS=https://cloud.xeroshell.com
XERO_WEB_APP_URL=https://cloud.xeroshell.com
XERO_WEB_SESSION_COOKIE_DOMAIN=.xeroshell.com
```
