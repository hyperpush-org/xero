import { spawn } from 'node:child_process'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { loadRootDotenv } from '../../scripts/lib/env.mjs'
import { createLogger, streamRun } from '../../scripts/lib/preflight-utils.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const clientDir = resolve(scriptDir, '..')
const repoRoot = resolve(clientDir, '..')
const runner = resolve(clientDir, 'src-tauri', 'scripts', 'tauri-dev-runner.sh')
const devTauriConfig = resolve(clientDir, 'src-tauri', 'tauri.dev.conf.json')
const devAppDataDir = defaultAppDataDir('dev.sn0w.xero')
const tauriArgs = ['dev', '--config', devTauriConfig, ...process.argv.slice(2)]
const rootEnv = loadRootDotenv(repoRoot)
const logger = createLogger('tauri:dev', '\x1b[35m')
const sidecarPath = resolve(
  clientDir,
  'src-tauri',
  'target',
  'debug',
  desktopSidecarBinaryName(),
)
const sidecarSourceRoots = [
  resolve(clientDir, 'src-tauri', 'crates', 'xero-desktop-sidecar'),
  resolve(clientDir, 'src-tauri', 'crates', 'xero-desktop-control-ipc'),
]

const env = buildTauriDevEnv(rootEnv, { devAppDataDir, runner, sidecarPath })

export function buildTauriDevEnv(rootEnv, { devAppDataDir, runner, sidecarPath }) {
  return {
    ...rootEnv,
    CARGO_BUILD_JOBS: rootEnv.CARGO_BUILD_JOBS ?? '4',
    CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER: runner,
    CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER: runner,
    XERO_APP_DATA_DIR: rootEnv.XERO_APP_DATA_DIR ?? devAppDataDir,
    XERO_DESKTOP_SIDECAR_PATH: sidecarPath,
    XERO_LAUNCH_MODE: rootEnv.XERO_LAUNCH_MODE ?? 'local-source',
  }
}

async function main() {
  if (process.env.XERO_SKIP_DESKTOP_SIDECAR_BUILD === '1') {
    logger.log('Skipping debug desktop sidecar build (XERO_SKIP_DESKTOP_SIDECAR_BUILD=1).')
  } else if (sidecarIsFresh()) {
    logger.log(`Debug desktop sidecar is fresh (${sidecarPath}).`)
  } else {
    logger.log(`Building debug desktop sidecar (${sidecarPath})...`)
    await streamRun(
      'cargo',
      [
        'build',
        '--manifest-path',
        resolve(clientDir, 'src-tauri', 'Cargo.toml'),
        '--package',
        'xero-desktop-sidecar',
      ],
      { cwd: repoRoot, env },
    )
    await normalizeMacosDesktopSidecarLinkage(sidecarPath, env)
  }

  const command = process.platform === 'win32' ? 'tauri.cmd' : 'tauri'
  const child = spawn(command, tauriArgs, {
    cwd: clientDir,
    env,
    shell: process.platform === 'win32',
    stdio: 'inherit',
  })

  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal)
      return
    }

    process.exit(code ?? 1)
  })

  child.on('error', (error) => {
    console.error(`Failed to start Tauri dev: ${error.message}`)
    process.exit(1)
  })
}

if (isDirectRun()) {
  main().catch((error) => {
    logger.fail(error?.message ?? String(error))
    process.exit(1)
  })
}

function isDirectRun() {
  return Boolean(process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
}

function defaultAppDataDir(directoryName) {
  if (process.platform === 'darwin') {
    return resolve(homedir(), 'Library', 'Application Support', directoryName)
  }
  if (process.platform === 'win32') {
    return resolve(process.env.APPDATA || process.env.LOCALAPPDATA || homedir(), directoryName)
  }
  return resolve(process.env.XDG_DATA_HOME || resolve(homedir(), '.local', 'share'), directoryName)
}

function desktopSidecarBinaryName() {
  return process.platform === 'win32' ? 'xero-desktop-sidecar.exe' : 'xero-desktop-sidecar'
}

function sidecarIsFresh() {
  if (!existsSync(sidecarPath)) return false

  const binaryMtime = statMtime(sidecarPath)
  if (binaryMtime === 0) return false

  return sidecarSourceRoots.every((root) => newestMtime(root) <= binaryMtime)
}

function newestMtime(rootPath, currentMax = 0) {
  let stat
  try {
    stat = statSync(rootPath)
  } catch {
    return currentMax
  }

  if (stat.isFile()) return Math.max(currentMax, stat.mtimeMs)
  if (!stat.isDirectory()) return currentMax

  let maxMtime = currentMax
  for (const entry of readdirSync(rootPath, { withFileTypes: true })) {
    if (entry.name === 'target' || entry.name === '.git') continue
    maxMtime = newestMtime(resolve(rootPath, entry.name), maxMtime)
  }
  return maxMtime
}

function statMtime(path) {
  try {
    return statSync(path).mtimeMs
  } catch {
    return 0
  }
}

async function normalizeMacosDesktopSidecarLinkage(path, env) {
  if (process.platform !== 'darwin') return

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
      shell: process.platform === 'win32',
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
