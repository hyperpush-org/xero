#!/usr/bin/env node
// Builds the Tauri production binary unless an up-to-date one already
// exists. "Up-to-date" means the binary's mtime is newer than the newest
// tracked source file under the client tree. Pass `--rebuild` to force.

import { existsSync, readdirSync, statSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { createLogger, host, streamRun } from './lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const clientDir = resolve(repoRoot, 'client')

const logger = createLogger('start:build')

const FORCE_REBUILD = process.argv.includes('--rebuild')

const SOURCE_ROOTS = [
  resolve(clientDir, 'src-tauri', 'src'),
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

  const binaryMtime = statSync(binary).mtimeMs
  let sourceMtime = 0
  for (const root of SOURCE_ROOTS) {
    sourceMtime = newestMtime(root, sourceMtime)
  }

  if (sourceMtime > binaryMtime) {
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
    ...process.env,
    TAURI_SIGNING_IDENTITY: process.env.TAURI_SIGNING_IDENTITY ?? '-',
    CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '4',
  }
  await streamRun('pnpm', ['exec', 'tauri', 'build'], { cwd: clientDir, env })

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
