import { spawn } from 'node:child_process'
import { createWriteStream, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const clientDir = resolve(scriptDir, '..')
const repoRoot = resolve(clientDir, '..')
const serverDir = resolve(repoRoot, 'server')
const tauriDir = resolve(clientDir, 'src-tauri')
const preflightScript = resolve(repoRoot, 'scripts', 'dev-preflight.mjs')
const xeroTuiArgs = process.argv.slice(2)
if (xeroTuiArgs[0] === '--') {
  xeroTuiArgs.shift()
}

const relayUrl = resolveRelayUrl()
const cloudUrl = resolveCloudUrl()
const env = {
  ...process.env,
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '4',
  XERO_REMOTE_RELAY_URL: process.env.XERO_REMOTE_RELAY_URL ?? relayUrl,
}

main().catch((error) => {
  console.error(`Failed to start Xero agent TUI: ${error?.message ?? String(error)}`)
  process.exit(1)
})

async function main() {
  const relay = await ensureLocalRelay()
  const cloud = await ensureCloudApp()
  const command = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
  const child = spawn(
    command,
    ['run', '--package', 'xero-desktop', '--bin', 'xero-tui', '--', ...xeroTuiArgs],
    {
      cwd: tauriDir,
      env,
      shell: process.platform === 'win32',
      stdio: 'inherit',
    },
  )

  const cleanup = () => {
    stopManagedProcess(relay)
    stopManagedProcess(cloud)
  }

  const forward = (signal) => {
    if (!child.killed) child.kill(signal)
    cleanup()
  }

  process.once('SIGINT', () => forward('SIGINT'))
  process.once('SIGTERM', () => forward('SIGTERM'))

  child.on('exit', (code, signal) => {
    cleanup()
    if (signal) {
      process.kill(process.pid, signal)
      return
    }

    process.exit(code ?? 1)
  })

  child.on('error', (error) => {
    cleanup()
    console.error(`Failed to start Xero agent TUI: ${error.message}`)
    process.exit(1)
  })
}

function resolveRelayUrl() {
  const raw =
    process.env.XERO_REMOTE_RELAY_URL ||
    process.env.VITE_XERO_SERVER_URL ||
    'http://127.0.0.1:4000'
  return raw.replace(/\/+$/, '')
}

function resolveCloudUrl() {
  const raw = process.env.XERO_CLOUD_DEV_URL || 'http://127.0.0.1:3002'
  return raw.replace(/\/+$/, '')
}

async function ensureLocalRelay() {
  if (await relayIsReachable(relayUrl)) return null
  if (!isLocalRelay(relayUrl)) return null

  console.log(`[dev:tui] Local Xero relay is not running at ${relayUrl}; starting Phoenix...`)
  await runOnce(process.execPath, [preflightScript], { cwd: repoRoot, env: process.env })

  const logPath = resolve(tmpdir(), `xero-tui-phoenix-${process.pid}.log`)
  const log = createWriteStream(logPath, { flags: 'a' })
  const serverEnv = {
    ...process.env,
    ...serverPortEnv(relayUrl),
  }
  const server = spawn(process.platform === 'win32' ? 'mix.bat' : 'mix', ['phx.server'], {
    cwd: serverDir,
    env: serverEnv,
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  server.stdout.pipe(log)
  server.stderr.pipe(log)

  await waitForRelay(server, logPath)
  console.log(`[dev:tui] Phoenix relay is ready at ${relayUrl}. Logs: ${logPath}`)
  return { process: server, log }
}

async function ensureCloudApp() {
  if (await cloudIsReachable(cloudUrl)) return null
  if (!isLocalUrl(cloudUrl)) return null

  console.log(`[dev:tui] Cloud app is not running at ${cloudUrl}; starting TanStack Start...`)
  const logPath = resolve(tmpdir(), `xero-tui-cloud-${process.pid}.log`)
  const log = createWriteStream(logPath, { flags: 'a' })
  const cloud = spawn(pnpmCommand(), ['run', 'dev:cloud'], {
    cwd: repoRoot,
    env: process.env,
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  cloud.stdout.pipe(log)
  cloud.stderr.pipe(log)

  await waitForCloud(cloud, logPath)
  console.log(`[dev:tui] Cloud app is ready at ${cloudUrl}. Logs: ${logPath}`)
  return { process: cloud, log }
}

async function waitForRelay(server, logPath) {
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    if (server.exitCode !== null) {
      throw new Error(`Phoenix relay exited before becoming ready.\n${tailLog(logPath)}`)
    }
    if (await relayIsReachable(relayUrl)) return
    await sleep(500)
  }

  server.kill('SIGTERM')
  throw new Error(`Timed out waiting for Phoenix relay at ${relayUrl}.\n${tailLog(logPath)}`)
}

async function waitForCloud(cloud, logPath) {
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    if (cloud.exitCode !== null) {
      throw new Error(`Cloud app exited before becoming ready.\n${tailLog(logPath)}`)
    }
    if (await cloudIsReachable(cloudUrl)) return
    await sleep(500)
  }

  cloud.kill('SIGTERM')
  throw new Error(`Timed out waiting for Cloud app at ${cloudUrl}.\n${tailLog(logPath)}`)
}

async function relayIsReachable(baseUrl) {
  try {
    const healthUrl = new URL('/api/health', baseUrl)
    const response = await fetch(healthUrl, { signal: AbortSignal.timeout(2_000) })
    return response.ok
  } catch {
    return false
  }
}

async function cloudIsReachable(baseUrl) {
  try {
    const response = await fetch(baseUrl, { signal: AbortSignal.timeout(2_000) })
    return response.ok
  } catch {
    return false
  }
}

function isLocalRelay(baseUrl) {
  return isLocalUrl(baseUrl)
}

function isLocalUrl(baseUrl) {
  try {
    const url = new URL(baseUrl)
    return ['127.0.0.1', 'localhost', '::1'].includes(url.hostname)
  } catch {
    return false
  }
}

function pnpmCommand() {
  return process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm'
}

function stopManagedProcess(managed) {
  if (managed?.process && !managed.process.killed) {
    managed.process.kill('SIGTERM')
  }
  managed?.log?.end()
}

function serverPortEnv(baseUrl) {
  try {
    const url = new URL(baseUrl)
    return url.port ? { PORT: url.port } : {}
  } catch {
    return {}
  }
}

function runOnce(command, args, options) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      ...options,
      shell: process.platform === 'win32',
      stdio: 'inherit',
    })
    child.on('exit', (code, signal) => {
      if (signal) {
        rejectRun(new Error(`${command} exited via ${signal}`))
      } else if (code === 0) {
        resolveRun()
      } else {
        rejectRun(new Error(`${command} exited with code ${code}`))
      }
    })
    child.on('error', rejectRun)
  })
}

function tailLog(path) {
  try {
    const contents = readFileSync(path, 'utf8')
    const lines = contents.trimEnd().split(/\r?\n/)
    const tail = lines.slice(-20).join('\n')
    return tail ? `Last Phoenix log lines:\n${tail}` : `Phoenix log was empty: ${path}`
  } catch {
    return `Phoenix log unavailable: ${path}`
  }
}

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms))
}
