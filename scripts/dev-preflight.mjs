#!/usr/bin/env node
// Preflight for `pnpm run dev`: make sure local toolchain commands,
// package deps, Docker/Postgres, Phoenix assets, and database schema are
// ready before the concurrently fan-out kicks in. Each step is
// idempotent: running it on a fully-prepped machine is fast and quiet.

import { spawn, spawnSync } from 'node:child_process'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { delimiter, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { homedir, platform } from 'node:os'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const composeFile = resolve(repoRoot, 'server/docker-compose.yml')
const composeScript = resolve(scriptDir, 'docker-compose.mjs')
const clientDir = resolve(repoRoot, 'client')
const landingDir = resolve(repoRoot, 'landing')
const serverDir = resolve(repoRoot, 'server')
const containerName = 'xero-postgres'
const host = platform()
const isWindows = host === 'win32'

const DOCKER_DAEMON_TIMEOUT_MS = 90_000
const DOCKER_INFO_TIMEOUT_MS = 5_000
const DOCKER_START_TIMEOUT_MS = 10_000
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
    shell: opts.shell ?? isWindows,
  })
  return result
}

function quiet(cmd, args, opts = {}) {
  return run(cmd, args, { ...opts, stdio: ['ignore', 'pipe', 'pipe'] })
}

function quietAsync(cmd, args, opts = {}) {
  return new Promise((resolveChild) => {
    let stdout = ''
    let stderr = ''
    let timedOut = false
    let settled = false
    let timer = null

    const child = spawn(cmd, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      cwd: opts.cwd,
      env: opts.env ?? process.env,
      shell: opts.shell ?? isWindows,
    })

    const finish = (result) => {
      if (settled) return
      settled = true
      if (timer) clearTimeout(timer)
      resolveChild({ stdout, stderr, timedOut, ...result })
    }

    timer = opts.timeout
      ? setTimeout(() => {
          timedOut = true
          child.kill('SIGKILL')
        }, opts.timeout)
      : null

    child.stdout?.on('data', (chunk) => {
      stdout += chunk
    })
    child.stderr?.on('data', (chunk) => {
      stderr += chunk
    })
    child.on('exit', (code, signal) => finish({ status: code, signal }))
    child.on('error', (error) => finish({ status: null, error }))
  })
}

function streamRun(cmd, args, opts = {}) {
  return new Promise((resolveChild, reject) => {
    const child = spawn(cmd, args, {
      stdio: 'inherit',
      cwd: opts.cwd,
      env: opts.env ?? process.env,
      shell: opts.shell ?? isWindows,
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

function statMtimeMs(path) {
  try {
    return statSync(path).mtimeMs
  } catch {
    return 0
  }
}

function nodeModulesBinPath(dir, binName) {
  const extension = isWindows ? '.cmd' : ''
  return resolve(dir, 'node_modules', '.bin', `${binName}${extension}`)
}

function pnpmDepsNeedInstall(dir, requiredBins = []) {
  const nodeModulesDir = resolve(dir, 'node_modules')
  const installMarker = resolve(nodeModulesDir, '.modules.yaml')
  if (!existsSync(nodeModulesDir) || !existsSync(installMarker)) return true

  for (const binName of requiredBins) {
    if (!existsSync(nodeModulesBinPath(dir, binName))) return true
  }

  const installMtime = statMtimeMs(installMarker)
  const manifestMtime = statMtimeMs(resolve(dir, 'package.json'))
  const lockfileMtime = statMtimeMs(resolve(dir, 'pnpm-lock.yaml'))
  return installMtime < Math.max(manifestMtime, lockfileMtime)
}

function parseMajorVersion(versionText) {
  const match = String(versionText).match(/\d+/)
  return match ? Number.parseInt(match[0], 10) : Number.NaN
}

function isRunnableFile(path) {
  try {
    const stat = statSync(path)
    if (!stat.isFile()) return false
    return isWindows || (stat.mode & 0o111) !== 0
  } catch {
    return false
  }
}

async function dockerDaemonReady() {
  // `docker info` exits non-zero if the daemon is unreachable. We use
  // `--format` to keep the output tiny on success.
  const probe = await quietAsync('docker', ['info', '--format', '{{.ServerVersion}}'], {
    shell: false,
    timeout: DOCKER_INFO_TIMEOUT_MS,
  })
  return probe.status === 0
}

function commandExists(command) {
  if (command.includes('/') || command.includes('\\')) {
    return isRunnableFile(command)
  }

  const pathDirs = (process.env.PATH ?? '').split(delimiter).filter(Boolean)
  const extensions = isWindows
    ? ['', ...(process.env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD').split(';')]
    : ['']

  return pathDirs.some((dir) =>
    extensions.some((extension) => isRunnableFile(resolve(dir, `${command}${extension}`))),
  )
}

function requireCommand(command, help) {
  if (commandExists(command)) return
  fail(`Missing required command \`${command}\` on PATH. ${help}`)
  process.exit(1)
}

function requireMinimumMajor(command, args, minimumMajor, help) {
  const result = quiet(command, args)
  if (result.status !== 0) {
    fail(`Could not read ${command} version. ${help}`)
    process.exit(1)
  }

  const major = parseMajorVersion(result.stdout || result.stderr)
  if (!Number.isFinite(major) || major < minimumMajor) {
    fail(`Required ${command} ${minimumMajor}+ but found: ${(result.stdout || result.stderr).trim()}`)
    console.error(`${tag} ${help}`)
    process.exit(1)
  }
}

function ensureRequiredToolchain() {
  const nodeMajor = parseMajorVersion(process.versions.node)
  if (!Number.isFinite(nodeMajor) || nodeMajor < 20) {
    fail(`Required Node.js 20+ but found ${process.version}. Install a modern LTS release and retry.`)
    process.exit(1)
  }

  requireCommand('pnpm', 'Enable Corepack or install pnpm, then run `pnpm run dev` again.')
  requireCommand('git', 'Install Git so Mix can fetch git-backed dependencies.')
  requireCommand('mix', 'Install Elixir/Mix for the Phoenix sidecar server.')
  requireCommand('cargo', 'Install the Rust toolchain for the Tauri backend.')
  requireCommand('protoc', 'Install Protocol Buffers; on macOS, `brew install protobuf`.')

  requireMinimumMajor('pnpm', ['--version'], 9, 'Enable Corepack or install pnpm 9+.')

  ok('Required local toolchain commands are available.')
}

async function ensurePnpmDeps({ label, dir, requiredBins = [] }) {
  if (!existsSync(resolve(dir, 'package.json'))) {
    warn(`Skipping ${label} pnpm install because ${dir}/package.json is missing.`)
    return
  }

  if (!pnpmDepsNeedInstall(dir, requiredBins)) {
    log(`${label} pnpm deps already installed.`)
    return
  }

  const args = ['install']
  if (existsSync(resolve(dir, 'pnpm-lock.yaml'))) {
    args.push('--frozen-lockfile')
  }
  args.push('--prefer-offline')

  log(`Installing ${label} pnpm deps...`)
  await streamRun('pnpm', args, { cwd: dir })
  ok(`${label} pnpm deps ready.`)
}

async function ensureNodeDeps() {
  await ensurePnpmDeps({
    label: 'root dev tooling',
    dir: repoRoot,
    requiredBins: ['concurrently'],
  })
  await ensurePnpmDeps({
    label: 'desktop client',
    dir: clientDir,
    requiredBins: ['tauri', 'vite'],
  })
  await ensurePnpmDeps({
    label: 'landing site',
    dir: landingDir,
    requiredBins: ['next'],
  })
}

function psSingleQuoted(value) {
  return `'${value.replaceAll("'", "''")}'`
}

async function launchDockerDesktopOnWindows() {
  const candidates = [
    process.env.ProgramFiles && resolve(process.env.ProgramFiles, 'Docker', 'Docker', 'Docker Desktop.exe'),
    process.env['ProgramFiles(x86)'] &&
      resolve(process.env['ProgramFiles(x86)'], 'Docker', 'Docker', 'Docker Desktop.exe'),
    process.env.LOCALAPPDATA && resolve(process.env.LOCALAPPDATA, 'Docker', 'Docker Desktop.exe'),
  ].filter(Boolean)

  for (const candidate of candidates) {
    if (!existsSync(candidate)) continue
    const launch = await quietAsync('powershell.exe', [
      '-NoProfile',
      '-Command',
      `Start-Process -FilePath ${psSingleQuoted(candidate)}`,
    ], { shell: false, timeout: DOCKER_START_TIMEOUT_MS })
    if (launch.status === 0) return true
  }

  return false
}

async function startDockerOnLinux() {
  const attempts = []

  if (commandExists('systemctl')) {
    attempts.push(['systemctl', ['--user', 'start', 'docker-desktop']])

    if (process.getuid?.() === 0) {
      attempts.push(['systemctl', ['start', 'docker']])
    } else if (commandExists('sudo')) {
      attempts.push(['sudo', ['-n', 'systemctl', 'start', 'docker']])
    }
  }

  if (commandExists('service')) {
    if (process.getuid?.() === 0) {
      attempts.push(['service', ['docker', 'start']])
    } else if (commandExists('sudo')) {
      attempts.push(['sudo', ['-n', 'service', 'docker', 'start']])
    }
  }

  for (const [cmd, args] of attempts) {
    const start = await quietAsync(cmd, args, {
      shell: false,
      timeout: DOCKER_START_TIMEOUT_MS,
    })
    if (start.status === 0) return true
  }

  return false
}

async function ensureDockerRunning() {
  if (await dockerDaemonReady()) {
    ok('Docker daemon is running.')
    return
  }

  if (!commandExists('docker')) {
    fail('Docker CLI not found on PATH. Install Docker Desktop, Docker Engine, or a compatible Docker CLI and retry.')
    process.exit(1)
  }

  if (host === 'darwin') {
    log('Docker daemon is not running — launching Docker Desktop...')
    const launch = await quietAsync('open', ['-ga', 'Docker'], {
      shell: false,
      timeout: DOCKER_START_TIMEOUT_MS,
    })
    if (launch.status !== 0) {
      fail(
        'Could not launch Docker Desktop with `open -ga Docker`. Start it manually and retry.',
      )
      process.exit(1)
    }
  } else if (host === 'linux') {
    log('Docker daemon is not running — attempting to start a local Docker service...')
    const start = await startDockerOnLinux()
    if (!start) {
      fail(
        'Could not start Docker automatically. Start Docker Desktop, dockerd, or a compatible daemon manually and retry.',
      )
      process.exit(1)
    }
  } else if (host === 'win32') {
    log('Docker daemon is not running — launching Docker Desktop...')
    if (!(await launchDockerDesktopOnWindows())) {
      fail('Could not launch Docker Desktop automatically. Start Docker Desktop manually and retry.')
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
    if (await dockerDaemonReady()) {
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
    )}s. Start Docker Desktop, dockerd, or a compatible Docker daemon manually and retry.`,
  )
  process.exit(1)
}

async function containerExists() {
  const probe = await quietAsync('docker', [
    'ps',
    '-a',
    '--filter',
    `name=^${containerName}$`,
    '--format',
    '{{.Names}}',
  ], { shell: false, timeout: DOCKER_INFO_TIMEOUT_MS })
  return probe.status === 0 && probe.stdout.trim() === containerName
}

async function containerRunning() {
  const probe = await quietAsync('docker', [
    'ps',
    '--filter',
    `name=^${containerName}$`,
    '--format',
    '{{.Names}}',
  ], { shell: false, timeout: DOCKER_INFO_TIMEOUT_MS })
  return probe.status === 0 && probe.stdout.trim() === containerName
}

async function containerHealth() {
  const probe = await quietAsync('docker', [
    'inspect',
    '--format',
    '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}',
    containerName,
  ], { shell: false, timeout: DOCKER_INFO_TIMEOUT_MS })
  if (probe.status !== 0) return null
  return probe.stdout.trim() || null
}

async function ensurePostgresUp() {
  if (await containerRunning()) {
    log('Postgres container already running.')
  } else {
    const exists = await containerExists()
    log(
      exists
        ? 'Postgres container exists but is stopped — bringing it up...'
        : 'Postgres container does not exist — pulling image and starting it (first run takes a minute)...',
    )
    await streamRun('node', [composeScript, '-f', composeFile, 'up', '-d'], { shell: false })
  }

  const deadline = Date.now() + CONTAINER_HEALTHY_TIMEOUT_MS
  while (Date.now() < deadline) {
    const status = await containerHealth()
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

function mixDepsNeedFetch() {
  if (!depsLooksPopulated()) return true

  const depsMtime = statMtimeMs(resolve(serverDir, 'deps'))
  const mixExsMtime = statMtimeMs(resolve(serverDir, 'mix.exs'))
  const mixLockMtime = statMtimeMs(resolve(serverDir, 'mix.lock'))
  return depsMtime < Math.max(mixExsMtime, mixLockMtime)
}

function fileExistsUnder(dir, names, maxDepth) {
  if (maxDepth < 0 || !existsSync(dir)) return false

  try {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (names.includes(entry.name)) return true
      if (entry.isDirectory() && fileExistsUnder(resolve(dir, entry.name), names, maxDepth - 1)) {
        return true
      }
    }
  } catch {
    return false
  }

  return false
}

function rebarLooksInstalled() {
  const mixHome = process.env.MIX_HOME ?? resolve(homedir(), '.mix')
  const names = isWindows ? ['rebar3.exe', 'rebar.exe', 'rebar3', 'rebar'] : ['rebar3', 'rebar']
  return fileExistsUnder(mixHome, names, 4)
}

async function ensureMixBootstrapTools() {
  if (quiet('mix', ['hex.info']).status === 0) {
    log('Hex is available.')
  } else {
    log('Installing Hex for Mix...')
    await streamRun('mix', ['local.hex', '--force'])
  }

  if (rebarLooksInstalled()) {
    log('Rebar is available.')
  } else {
    log('Installing Rebar for Mix...')
    await streamRun('mix', ['local.rebar', '--force'])
  }
}

async function ensureMixDeps() {
  if (!mixDepsNeedFetch()) {
    log('Mix deps already populated — skipping `mix deps.get`.')
    return
  }
  log('Fetching mix deps (`mix deps.get`)...')
  await streamRun('mix', ['deps.get'], { cwd: serverDir })
}

function phoenixAssetToolsReady() {
  const buildDir = resolve(serverDir, '_build')
  if (!existsSync(buildDir)) return false

  try {
    const entries = readdirSync(buildDir)
    return (
      entries.some((entry) => entry.startsWith('esbuild-')) &&
      entries.some((entry) => entry.startsWith('tailwind-'))
    )
  } catch {
    return false
  }
}

async function ensurePhoenixAssets() {
  if (phoenixAssetToolsReady()) {
    log('Phoenix asset tools already installed.')
    return
  }

  log('Installing Phoenix asset tools (`mix assets.setup`)...')
  await streamRun('mix', ['assets.setup'], {
    cwd: serverDir,
    env: { ...process.env, MIX_ENV: process.env.MIX_ENV ?? 'dev' },
  })
  ok('Phoenix asset tools ready.')
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
  ensureRequiredToolchain()
  await ensureNodeDeps()
  await ensureMixBootstrapTools()
  await ensureMixDeps()
  await ensurePhoenixAssets()
  await ensureDockerRunning()
  await ensurePostgresUp()
  await ensureSchema()
  ok(`Preflight complete in ${((Date.now() - t0) / 1000).toFixed(1)}s.`)
}

main().catch((err) => {
  fail(err?.message ?? String(err))
  process.exit(1)
})
