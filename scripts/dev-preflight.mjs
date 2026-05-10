#!/usr/bin/env node
// Preflight for `pnpm run dev`: make sure local toolchain commands,
// package deps, Docker/Postgres, Phoenix assets, and database schema are
// ready before the concurrently fan-out kicks in. Each step is
// idempotent: running it on a fully-prepped machine is fast and quiet.

import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  createLogger,
  ensureDockerRunning,
  ensureMixBootstrapTools,
  ensureMixDeps,
  ensurePhoenixAssets,
  ensurePnpmDeps,
  ensurePostgresUp,
  ensureRequiredToolchain,
  ensureSchema,
} from './lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const composeFile = resolve(repoRoot, 'server/docker-compose.yml')
const composeScript = resolve(scriptDir, 'docker-compose.mjs')
const clientDir = resolve(repoRoot, 'client')
const landingDir = resolve(repoRoot, 'landing')
const serverDir = resolve(repoRoot, 'server')
const containerName = 'xero-postgres'

const logger = createLogger('preflight')

async function ensureNodeDeps() {
  await ensurePnpmDeps(logger, {
    label: 'root dev tooling',
    dir: repoRoot,
    requiredBins: ['concurrently'],
  })
  await ensurePnpmDeps(logger, {
    label: 'desktop client',
    dir: clientDir,
    requiredBins: ['tauri', 'vite'],
  })
  await ensurePnpmDeps(logger, {
    label: 'landing site',
    dir: landingDir,
    requiredBins: ['next'],
  })
}

async function main() {
  const t0 = Date.now()
  ensureRequiredToolchain(logger)
  await ensureNodeDeps()
  await ensureMixBootstrapTools(logger)
  await ensureMixDeps(logger, { serverDir })
  await ensurePhoenixAssets(logger, { serverDir })
  await ensureDockerRunning(logger)
  await ensurePostgresUp(logger, { containerName, composeFile, composeScript })
  await ensureSchema(logger, { serverDir })
  logger.ok(`Preflight complete in ${((Date.now() - t0) / 1000).toFixed(1)}s.`)
}

main().catch((err) => {
  logger.fail(err?.message ?? String(err))
  process.exit(1)
})
