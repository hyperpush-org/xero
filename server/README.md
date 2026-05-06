# Xero Server

Phoenix service used by the Xero desktop development workflow. It currently backs web callback/shared backend features such as GitHub auth sessions, Oban jobs, rate limiting, and local service endpoints.

## Local Setup

From the repository root, the usual path is:

```bash
pnpm run dev
```

That runs `scripts/dev-preflight.mjs`, starts Docker/Postgres, fetches missing Mix deps, creates the dev database, applies migrations, and then launches Phoenix alongside the Tauri app and landing site.

To run the server directly:

```bash
docker compose -f docker-compose.yml up -d
mix setup
mix phx.server
```

The dev endpoint listens on `http://localhost:4000`.

## Environment

Copy `.env.example` to `.env` for local secrets or overrides. Dev defaults use:

- `DATABASE_URL=ecto://postgres:postgres@localhost/xero_dev`
- `PORT=4000`
- CORS origins for the Vite/Tauri dev frontend

## Commands

```bash
mix setup       # deps, database setup, assets
mix phx.server  # run Phoenix
mix test        # server tests
mix precommit   # warnings-as-errors, format, tests
```
