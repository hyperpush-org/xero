#!/usr/bin/env node
// Launches the production-built Xero binary. Streams stdout/stderr from the
// running binary and exits with its code.

import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { createLogger, host } from './lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const clientDir = resolve(repoRoot, 'client')

const logger = createLogger('start:app', '\x1b[35m')

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

function main() {
  ensurePlatformSupported()

  const binary = builtBinaryPath()
  if (!existsSync(binary)) {
    logger.fail(`Built Tauri binary not found at ${binary}. Run \`pnpm start:build\` first.`)
    process.exit(1)
  }

  logger.log(`Launching Xero (${binary})`)
  const child = spawn(binary, [], { stdio: 'inherit' })

  const forward = (signal) => () => {
    if (!child.killed) child.kill(signal)
  }
  process.on('SIGINT', forward('SIGINT'))
  process.on('SIGTERM', forward('SIGTERM'))

  child.on('exit', (code, signal) => {
    if (signal) {
      logger.warn(`Xero exited via ${signal}.`)
      process.exit(0)
    }
    process.exit(code ?? 0)
  })
  child.on('error', (err) => {
    logger.fail(err.message)
    process.exit(1)
  })
}

main()
