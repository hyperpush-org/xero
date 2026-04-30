import { readdir, rm, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const clientDir = resolve(scriptDir, '..')
const defaultTargetDir = resolve(clientDir, 'src-tauri', 'target')

const options = {
  dryRun: false,
  maxAgeHours: Number.parseFloat(process.env.XERO_RUST_TARGET_MAX_AGE_HOURS ?? '6'),
  targetDir: process.env.CARGO_TARGET_DIR
    ? resolve(process.env.CARGO_TARGET_DIR)
    : defaultTargetDir,
}

for (const arg of process.argv.slice(2)) {
  if (arg === '--dry-run') {
    options.dryRun = true
    continue
  }

  if (arg.startsWith('--max-age-hours=')) {
    options.maxAgeHours = Number.parseFloat(arg.slice('--max-age-hours='.length))
    continue
  }

  if (arg.startsWith('--target-dir=')) {
    options.targetDir = resolve(arg.slice('--target-dir='.length))
    continue
  }

  console.error(`Unknown option: ${arg}`)
  process.exit(64)
}

if (!Number.isFinite(options.maxAgeHours) || options.maxAgeHours < 0) {
  console.error('Expected --max-age-hours to be a non-negative number.')
  process.exit(64)
}

const depsDir = join(options.targetDir, 'debug', 'deps')
const cutoffMs = Date.now() - options.maxAgeHours * 60 * 60 * 1000

const formatBytes = (bytes) => {
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0

  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }

  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`
}

const isLikelyTestExecutable = (entry, entryStat) => {
  if (!entryStat.isFile()) return false
  if ((entryStat.mode & 0o111) === 0) return false
  if (entry.name.includes('.')) return false

  // Build scripts live in target/debug/build, but keep this guard in case a
  // toolchain changes where it places helper binaries.
  return !entry.name.startsWith('build-script-build-')
}

let entries
try {
  entries = await readdir(depsDir, { withFileTypes: true })
} catch (error) {
  if (error.code === 'ENOENT') {
    console.log(`No Rust target deps directory found at ${depsDir}.`)
    process.exit(0)
  }

  throw error
}

let removedCount = 0
let removedBytes = 0

const removePath = async (path, size = 0) => {
  if (options.dryRun) {
    console.log(`[dry-run] remove ${path}`)
  } else {
    await rm(path, { force: true, recursive: true })
  }

  removedCount += 1
  removedBytes += size
}

for (const entry of entries) {
  const path = join(depsDir, entry.name)
  const entryStat = await stat(path)

  if (!isLikelyTestExecutable(entry, entryStat)) continue
  if (entryStat.mtimeMs > cutoffMs) continue

  await removePath(path, entryStat.size)

  const depInfoPath = `${path}.d`
  try {
    const depInfoStat = await stat(depInfoPath)
    await removePath(depInfoPath, depInfoStat.size)
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }

  const dsymPath = `${path}.dSYM`
  try {
    const dsymStat = await stat(dsymPath)
    await removePath(dsymPath, dsymStat.size)
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
}

const action = options.dryRun ? 'Would remove' : 'Removed'
console.log(
  `${action} ${removedCount} stale Rust test artifact${removedCount === 1 ? '' : 's'} ` +
    `from ${depsDir} (${formatBytes(removedBytes)}).`
)
