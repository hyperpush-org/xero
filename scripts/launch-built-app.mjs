#!/usr/bin/env node
// Launches the production-built Xero binary with `XERO_LAUNCH_MODE=local-source`
// so the frontend can show the local-environment onboarding step. Streams
// stdout/stderr from the running binary and exits with its code.

import { spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { createLogger, host } from './lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const clientDir = resolve(repoRoot, 'client')
const serverDir = resolve(repoRoot, 'server')

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

function readEnvKey(envPath, key, fallback) {
  if (!existsSync(envPath)) return fallback
  const contents = readFileSync(envPath, 'utf8')
  const re = new RegExp(`^${key}=(.*)$`, 'm')
  const match = contents.match(re)
  if (!match) return fallback
  const value = match[1].trim()
  return value.length > 0 ? value : fallback
}

function main() {
  ensurePlatformSupported()

  const binary = builtBinaryPath()
  if (!existsSync(binary)) {
    logger.fail(`Built Tauri binary not found at ${binary}. Run \`pnpm start:build\` first.`)
    process.exit(1)
  }

  const envFile = resolve(serverDir, '.env')
  const port = readEnvKey(envFile, 'PORT', '4000')
  const phxHost = readEnvKey(envFile, 'PHX_HOST', '127.0.0.1')

  const env = {
    ...process.env,
    XERO_LAUNCH_MODE: 'local-source',
    XERO_LOCAL_ENV_FILE: envFile,
    VITE_XERO_SERVER_URL: `http://${phxHost}:${port}`,
  }

  logger.log(`Launching Xero (${binary})`)
  const child = spawn(binary, [], { stdio: 'inherit', env })

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
