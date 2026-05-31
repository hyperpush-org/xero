#!/usr/bin/env node
// Builds the Tauri production binary unless an up-to-date one already
// exists. "Up-to-date" means the binary's mtime is newer than the newest
// tracked source file under the client tree. Pass `--rebuild` to force.

import { existsSync, readdirSync, statSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { loadRootDotenv } from './lib/env.mjs'
import { createLogger, host, streamRun } from './lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const clientDir = resolve(repoRoot, 'client')
const rootEnv = loadRootDotenv(repoRoot)

const logger = createLogger('start:build')

const FORCE_REBUILD = process.argv.includes('--rebuild')

const SOURCE_ROOTS = [
  resolve(clientDir, 'src-tauri', 'src'),
  resolve(clientDir, 'src-tauri', 'crates', 'xero-desktop-control-ipc'),
  resolve(clientDir, 'src-tauri', 'crates', 'xero-desktop-sidecar'),
  resolve(clientDir, 'src-tauri', 'Cargo.toml'),
  resolve(clientDir, 'src-tauri', 'tauri.conf.json'),
  resolve(clientDir, 'src'),
  resolve(clientDir, 'components'),
  resolve(clientDir, 'lib'),
  resolve(clientDir, 'hooks'),
  resolve(clientDir, 'package.json'),
  resolve(clientDir, 'pnpm-lock.yaml'),
]

const IGNORED_DIRS = new Set(['node_modules', 'target', 'dist', '.next', '.turbo', '.git'])

function ensurePlatformSupported() {
  if (host !== 'darwin') {
    logger.fail(
      "`pnpm start` currently supports macOS only — use `pnpm dev` on this platform for now.",
    )
    process.exit(1)
  }
}

function builtBinaryPath() {
  return resolve(
    clientDir,
    'src-tauri',
    'target',
    'release',
    'bundle',
    'macos',
    'Xero.app',
    'Contents',
    'MacOS',
    'xero-desktop',
  )
}

function bundledDesktopSidecarPath() {
  return resolve(
    clientDir,
    'src-tauri',
    'target',
    'release',
    'bundle',
    'macos',
    'Xero.app',
    'Contents',
    'Resources',
    'resources',
    desktopSidecarBinaryName(),
  )
}

function releaseDesktopSidecarPath() {
  return resolve(clientDir, 'src-tauri', 'target', 'release', desktopSidecarBinaryName())
}

function desktopSidecarBinaryName() {
  return host === 'win32' ? 'xero-desktop-sidecar.exe' : 'xero-desktop-sidecar'
}

function newestMtime(rootPath, currentMax = 0) {
  let maxMtime = currentMax
  let stat
  try {
    stat = statSync(rootPath)
  } catch {
    return maxMtime
  }

  if (stat.isFile()) {
    return Math.max(maxMtime, stat.mtimeMs)
  }

  if (!stat.isDirectory()) return maxMtime

  let entries
  try {
    entries = readdirSync(rootPath, { withFileTypes: true })
  } catch {
    return maxMtime
  }

  for (const entry of entries) {
    if (IGNORED_DIRS.has(entry.name)) continue
    if (entry.name.startsWith('.') && entry.name !== '.env.example') continue
    const childPath = resolve(rootPath, entry.name)
    if (entry.isDirectory()) {
      maxMtime = newestMtime(childPath, maxMtime)
    } else if (entry.isFile()) {
      try {
        maxMtime = Math.max(maxMtime, statSync(childPath).mtimeMs)
      } catch {
        // ignore
      }
    }
  }
  return maxMtime
}

function shouldRebuild() {
  if (FORCE_REBUILD) return { rebuild: true, reason: '--rebuild flag' }
  const binary = builtBinaryPath()
  if (!existsSync(binary)) return { rebuild: true, reason: 'no built binary found' }

  const bundledSidecar = bundledDesktopSidecarPath()
  if (!existsSync(bundledSidecar)) {
    return { rebuild: true, reason: 'no bundled desktop sidecar found' }
  }

  const binaryMtime = statSync(binary).mtimeMs
  const bundledSidecarMtime = statSync(bundledSidecar).mtimeMs
  let sourceMtime = 0
  for (const root of SOURCE_ROOTS) {
    sourceMtime = newestMtime(root, sourceMtime)
  }

  if (sourceMtime > Math.min(binaryMtime, bundledSidecarMtime)) {
    return { rebuild: true, reason: 'sources newer than built binary' }
  }
  return { rebuild: false, reason: null }
}

async function main() {
  ensurePlatformSupported()

  const { rebuild, reason } = shouldRebuild()
  if (!rebuild) {
    logger.log(`Built Tauri binary is fresh — skipping build. (${builtBinaryPath()})`)
    return
  }

  logger.log(`Building Tauri release binary (${reason})...`)
  // Ad-hoc signing parity with the existing dev runner; users can override
  // by setting TAURI_SIGNING_IDENTITY before invoking `pnpm start`.
  const env = {
    ...rootEnv,
    TAURI_SIGNING_IDENTITY: rootEnv.TAURI_SIGNING_IDENTITY ?? '-',
    CARGO_BUILD_JOBS: rootEnv.CARGO_BUILD_JOBS ?? '4',
  }

  logger.log(`Building release desktop sidecar (${releaseDesktopSidecarPath()})...`)
  await streamRun(
    'cargo',
    [
      'build',
      '--manifest-path',
      resolve(clientDir, 'src-tauri', 'Cargo.toml'),
      '--package',
      'xero-desktop-sidecar',
      '--release',
    ],
    { cwd: repoRoot, env },
  )
  if (!existsSync(releaseDesktopSidecarPath())) {
    logger.fail(`Sidecar build finished but no binary at ${releaseDesktopSidecarPath()}.`)
    process.exit(1)
  }
  await normalizeMacosDesktopSidecarLinkage(releaseDesktopSidecarPath(), env)

  await streamRun('pnpm', ['exec', 'tauri', 'build', ...localTauriBuildArgs(env)], {
    cwd: clientDir,
    env,
  })

  if (!existsSync(builtBinaryPath())) {
    logger.fail(`Build finished but no binary at ${builtBinaryPath()}. Inspect tauri output above.`)
    process.exit(1)
  }
  logger.ok(`Built ${builtBinaryPath()}`)
}

main().catch((err) => {
  logger.fail(err?.message ?? String(err))
  process.exit(1)
})

async function normalizeMacosDesktopSidecarLinkage(path, env) {
  if (host !== 'darwin') return

  const linkOutput = await commandOutput('otool', ['-L', path], { env })
  if (!linkOutput.includes('@rpath/libswift_Concurrency.dylib')) return

  await streamRun(
    '/usr/bin/install_name_tool',
    [
      '-change',
      '@rpath/libswift_Concurrency.dylib',
      '/usr/lib/swift/libswift_Concurrency.dylib',
      path,
    ],
    { cwd: repoRoot, env },
  )
}

function commandOutput(command, args, options = {}) {
  return new Promise((resolveOutput, reject) => {
    let stdout = ''
    let stderr = ''
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      shell: host === 'win32',
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    child.stdout?.on('data', (chunk) => {
      stdout += chunk
    })
    child.stderr?.on('data', (chunk) => {
      stderr += chunk
    })
    child.on('exit', (code) => {
      if (code === 0) {
        resolveOutput(stdout)
      } else {
        reject(new Error(`${command} ${args.join(' ')} exited with code ${code}: ${stderr}`))
      }
    })
    child.on('error', reject)
  })
}

function localTauriBuildArgs(env) {
  if (env.TAURI_SIGNING_PRIVATE_KEY) return []

  logger.log('No TAURI_SIGNING_PRIVATE_KEY set; skipping local updater artifacts.')
  return ['--config', JSON.stringify({ bundle: { createUpdaterArtifacts: false } })]
}
