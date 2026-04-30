#!/usr/bin/env node
// Preflight for `pnpm run dev` — make sure Docker, Postgres, deps,
// and database schema are ready before the concurrently fan-out kicks
// in. Each step is idempotent: running it on a fully-prepped machine is
// fast and silent.

import { spawn, spawnSync } from 'node:child_process'
import { existsSync, readdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform } from 'node:os'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const composeFile = resolve(repoRoot, 'server/docker-compose.yml')
const serverDir = resolve(repoRoot, 'server')
const containerName = 'xero-postgres'

const DOCKER_DAEMON_TIMEOUT_MS = 90_000
const CONTAINER_HEALTHY_TIMEOUT_MS = 90_000
const POLL_INTERVAL_MS = 1500

const colors = {
  reset: '\x1b[0m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
}

const tag = `${colors.bold}${colors.cyan}[preflight]${colors.reset}`
const log = (msg) => console.log(`${tag} ${msg}`)
const warn = (msg) => console.warn(`${tag} ${colors.yellow}${msg}${colors.reset}`)
const fail = (msg) => console.error(`${tag} ${colors.red}${msg}${colors.reset}`)
const ok = (msg) => console.log(`${tag} ${colors.green}${msg}${colors.reset}`)

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, {
    stdio: opts.stdio ?? 'pipe',
    encoding: 'utf8',
    cwd: opts.cwd,
    env: opts.env ?? process.env,
    timeout: opts.timeout,
  })
  return result
}

function quiet(cmd, args, opts = {}) {
  return run(cmd, args, { ...opts, stdio: ['ignore', 'pipe', 'pipe'] })
}

function streamRun(cmd, args, opts = {}) {
  return new Promise((resolveChild, reject) => {
    const child = spawn(cmd, args, {
      stdio: 'inherit',
      cwd: opts.cwd,
      env: opts.env ?? process.env,
    })
    child.on('exit', (code) => {
      if (code === 0) {
        resolveChild()
      } else {
        reject(new Error(`${cmd} ${args.join(' ')} exited with code ${code}`))
      }
    })
    child.on('error', reject)
  })
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

function dockerDaemonReady() {
  // `docker info` exits non-zero if the daemon is unreachable. We use
  // `--format` to keep the output tiny on success.
  const probe = quiet('docker', ['info', '--format', '{{.ServerVersion}}'])
  return probe.status === 0
}

async function ensureDockerRunning() {
  if (dockerDaemonReady()) {
    ok('Docker daemon is running.')
    return
  }

  const which = quiet('which', ['docker'])
  if (which.status !== 0) {
    fail('Docker CLI not found on PATH. Install Docker Desktop and retry.')
    process.exit(1)
  }

  const host = platform()
  if (host === 'darwin') {
    log('Docker daemon is not running — launching Docker Desktop...')
    const launch = quiet('open', ['-ga', 'Docker'])
    if (launch.status !== 0) {
      fail(
        'Could not launch Docker Desktop with `open -ga Docker`. Start it manually and retry.',
      )
      process.exit(1)
    }
  } else if (host === 'linux') {
    log('Docker daemon is not running — attempting `systemctl start docker`...')
    const start = quiet('sudo', ['-n', 'systemctl', 'start', 'docker'])
    if (start.status !== 0) {
      fail(
        'Could not start dockerd via systemctl (sudo may have prompted). Start Docker manually and retry.',
      )
      process.exit(1)
    }
  } else {
    fail(
      `Unsupported platform "${host}" for auto-starting Docker. Start the daemon manually and retry.`,
    )
    process.exit(1)
  }

  const deadline = Date.now() + DOCKER_DAEMON_TIMEOUT_MS
  let dots = 0
  while (Date.now() < deadline) {
    if (dockerDaemonReady()) {
      ok('Docker daemon is up.')
      return
    }
    await sleep(POLL_INTERVAL_MS)
    dots = (dots + 1) % 4
    process.stdout.write(`\r${tag} waiting for Docker daemon${'.'.repeat(dots).padEnd(3, ' ')}`)
  }
  process.stdout.write('\n')
  fail(
    `Docker daemon did not become ready within ${Math.round(
      DOCKER_DAEMON_TIMEOUT_MS / 1000,
    )}s. Open Docker Desktop manually and retry.`,
  )
  process.exit(1)
}

function containerExists() {
  const probe = quiet('docker', [
    'ps',
    '-a',
    '--filter',
    `name=^${containerName}$`,
    '--format',
    '{{.Names}}',
  ])
  return probe.status === 0 && probe.stdout.trim() === containerName
}

function containerRunning() {
  const probe = quiet('docker', [
    'ps',
    '--filter',
    `name=^${containerName}$`,
    '--format',
    '{{.Names}}',
  ])
  return probe.status === 0 && probe.stdout.trim() === containerName
}

function containerHealth() {
  const probe = quiet('docker', [
    'inspect',
    '--format',
    '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}',
    containerName,
  ])
  if (probe.status !== 0) return null
  return probe.stdout.trim() || null
}

async function ensurePostgresUp() {
  if (containerRunning()) {
    log('Postgres container already running.')
  } else {
    log(
      containerExists()
        ? 'Postgres container exists but is stopped — bringing it up...'
        : 'Postgres container does not exist — pulling image and starting it (first run takes a minute)...',
    )
    await streamRun('docker', ['compose', '-f', composeFile, 'up', '-d'])
  }

  const deadline = Date.now() + CONTAINER_HEALTHY_TIMEOUT_MS
  while (Date.now() < deadline) {
    const status = containerHealth()
    if (status === 'healthy' || status === 'running') {
      // `running` covers the moment between healthcheck rounds during
      // startup; a follow-up loop will catch the unhealthy case.
      if (status === 'healthy') {
        ok('Postgres is healthy.')
        return
      }
    }
    if (status === 'unhealthy') {
      fail('Postgres healthcheck reports unhealthy. Inspect with `docker logs xero-postgres`.')
      process.exit(1)
    }
    await sleep(POLL_INTERVAL_MS)
  }

  warn(
    `Postgres did not report healthy within ${Math.round(
      CONTAINER_HEALTHY_TIMEOUT_MS / 1000,
    )}s. Continuing — Phoenix will retry the connection at startup.`,
  )
}

function depsLooksPopulated() {
  const depsDir = resolve(serverDir, 'deps')
  if (!existsSync(depsDir)) return false
  try {
    return readdirSync(depsDir).length > 5
  } catch {
    return false
  }
}

async function ensureMixDeps() {
  if (depsLooksPopulated()) {
    log('Mix deps already populated — skipping `mix deps.get`.')
    return
  }
  log('Fetching mix deps (`mix deps.get`)...')
  await streamRun('mix', ['deps.get'], { cwd: serverDir })
}

async function ensureSchema() {
  // Both ecto.create and ecto.migrate are idempotent; running them on
  // every dev start guarantees migrations stay in sync without forcing
  // the user to remember a separate "setup" command.
  log('Ensuring database is created and migrations are applied...')
  // ecto.create exits 0 with a "already up" notice if the DB exists.
  await streamRun('mix', ['ecto.create', '--quiet'], {
    cwd: serverDir,
    env: { ...process.env, MIX_ENV: process.env.MIX_ENV ?? 'dev' },
  })
  await streamRun('mix', ['ecto.migrate', '--all'], {
    cwd: serverDir,
    env: { ...process.env, MIX_ENV: process.env.MIX_ENV ?? 'dev' },
  })
  ok('Database schema ready.')
}

async function main() {
  const t0 = Date.now()
  await ensureDockerRunning()
  await ensurePostgresUp()
  await ensureMixDeps()
  await ensureSchema()
  ok(`Preflight complete in ${((Date.now() - t0) / 1000).toFixed(1)}s.`)
}

main().catch((err) => {
  fail(err?.message ?? String(err))
  process.exit(1)
})
