import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const clientDir = resolve(scriptDir, '..')
const manifestPath = resolve(clientDir, 'src-tauri', 'Cargo.toml')
const pruneScript = resolve(scriptDir, 'prune-rust-target.mjs')
const rawArgs = process.argv.slice(2)
const fullSuite = rawArgs.includes('--xero-full')
const passthroughArgs = rawArgs.filter((arg) => arg !== '--xero-full')
const cargoArgs = buildCargoArgs(passthroughArgs, { fullSuite })

const run = (command, args, options = {}) =>
  new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      cwd: clientDir,
      shell: process.platform === 'win32',
      stdio: 'inherit',
      ...options,
    })

    child.on('exit', (code, signal) => {
      if (signal) {
        resolvePromise({ code: 1, signal })
        return
      }

      resolvePromise({ code: code ?? 1, signal: null })
    })

    child.on('error', rejectPromise)
  })

const env = {
  ...process.env,
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '4',
  CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL ?? '0',
  XERO_SKIP_COOKIE_IMPORTER: process.env.XERO_SKIP_COOKIE_IMPORTER ?? '1',
  XERO_SKIP_CURSOR_SIDECAR: process.env.XERO_SKIP_CURSOR_SIDECAR ?? '1',
  XERO_SKIP_DESKTOP_SIDECAR: process.env.XERO_SKIP_DESKTOP_SIDECAR ?? '1',
  XERO_SKIP_DICTATION_SHIM: process.env.XERO_SKIP_DICTATION_SHIM ?? '1',
  XERO_SKIP_IOS_HELPER: process.env.XERO_SKIP_IOS_HELPER ?? '1',
  XERO_SKIP_SIDECAR_FETCH: process.env.XERO_SKIP_SIDECAR_FETCH ?? '1',
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
const result = await run(cargo, cargoArgs, { env })

if (process.env.XERO_RUST_TARGET_PRUNE !== '0') {
  await run(process.execPath, [pruneScript], { env })
}

if (result.signal) {
  process.kill(process.pid, result.signal)
} else {
  process.exit(result.code)
}

function buildCargoArgs(args, { fullSuite }) {
  const cargoArgs = ['test', '--manifest-path', manifestPath]
  const hadPassthroughArgs = args.length > 0
  appendFeature(args, 'tauri-test-support')

  if (!fullSuite && !hadPassthroughArgs) {
    cargoArgs.push('--lib')
  }

  return [...cargoArgs, ...args]
}

function appendFeature(args, feature) {
  const joinedFeature = args.findIndex((arg) => arg.startsWith('--features='))
  if (joinedFeature !== -1) {
    const value = args[joinedFeature].slice('--features='.length)
    if (!value.split(/[,\s]+/).includes(feature)) {
      args[joinedFeature] = `--features=${value},${feature}`
    }
    return
  }

  const splitFeature = args.indexOf('--features')
  if (splitFeature !== -1) {
    const value = args[splitFeature + 1] ?? ''
    if (!value.split(/[,\s]+/).includes(feature)) {
      args[splitFeature + 1] = value ? `${value},${feature}` : feature
    }
    return
  }

  if (args.includes('--all-features')) return

  args.unshift('--features', feature)
}
