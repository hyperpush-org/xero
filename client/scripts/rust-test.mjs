import { spawn } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const clientDir = resolve(scriptDir, '..')
const manifestPath = resolve(clientDir, 'src-tauri', 'Cargo.toml')
const pruneScript = resolve(scriptDir, 'prune-rust-target.mjs')
const cargoArgs = ['test', '--manifest-path', manifestPath, ...process.argv.slice(2)]

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
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
const result = await run(cargo, cargoArgs, { env })

if (process.env.CADENCE_RUST_TARGET_PRUNE !== '0') {
  await run(process.execPath, [pruneScript], { env })
}

if (result.signal) {
  process.kill(process.pid, result.signal)
} else {
  process.exit(result.code)
}
