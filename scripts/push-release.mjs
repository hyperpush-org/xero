#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const tauriConfigPath = resolve(repoRoot, 'client/src-tauri/tauri.conf.json')
const xeroCliCargoPath = resolve(repoRoot, 'client/src-tauri/crates/xero-cli/Cargo.toml')

function usage() {
  console.log(`Usage: pnpm release:push <version> [--dry-run] [--remote <name>]

Pushes the current branch and a v<version> tag to GitHub. The tag push
triggers the Release workflow. The tag builds the desktop client when
<version> matches client/src-tauri/tauri.conf.json, and builds the TUI when
<version> matches client/src-tauri/crates/xero-cli/Cargo.toml. At least one
artifact version must match.

Examples:
  pnpm release:push 0.1.1
  pnpm release:push v0.1.1 --dry-run`)
}

function fail(message) {
  console.error(`[release:push] ${message}`)
  process.exit(1)
}

function runGit(args, options = {}) {
  const result = spawnSync('git', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: options.stdio ?? 'pipe',
  })

  if (result.status !== 0 && !options.allowFailure) {
    const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim()
    fail(`git ${args.join(' ')} failed${output ? `:\n${output}` : ''}`)
  }
  return result
}

function gitOutput(args, options = {}) {
  const result = runGit(args, options)
  return (result.stdout ?? '').trim()
}

function parseArgs(argv) {
  const parsed = {
    version: null,
    remote: 'origin',
    dryRun: false,
  }

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '-h' || arg === '--help') {
      usage()
      process.exit(0)
    }
    if (arg === '--dry-run') {
      parsed.dryRun = true
      continue
    }
    if (arg === '--remote') {
      const remote = argv[index + 1]
      if (!remote || remote.startsWith('-')) fail('--remote requires a remote name')
      parsed.remote = remote
      index += 1
      continue
    }
    if (arg.startsWith('-')) fail(`Unknown option: ${arg}`)
    if (parsed.version) fail('Pass exactly one version')
    parsed.version = arg
  }

  if (!parsed.version) {
    usage()
    fail('Missing release version')
  }

  parsed.version = parsed.version.replace(/^v/, '')
  return parsed
}

function ensureSemver(version) {
  const semverPattern =
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/
  if (!semverPattern.test(version)) {
    fail(`Expected a semantic version like 0.1.1, got ${version}`)
  }
}

function readCargoPackageVersion(path) {
  const cargoToml = readFileSync(path, 'utf8')
  return cargoToml.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1] ?? null
}

function resolveReleaseTargets(version) {
  const config = JSON.parse(readFileSync(tauriConfigPath, 'utf8'))
  const tauriVersion = config.version
  const tuiVersion = readCargoPackageVersion(xeroCliCargoPath)
  const buildTauri = tauriVersion === version
  const buildTui = tuiVersion === version

  if (!buildTauri && !buildTui) {
    fail(
      [
        `No release artifact version matches ${version}.`,
        `client/src-tauri/tauri.conf.json is ${tauriVersion ?? 'missing'}.`,
        `client/src-tauri/crates/xero-cli/Cargo.toml is ${tuiVersion ?? 'missing'}.`,
      ].join('\n'),
    )
  }

  return { buildTauri, buildTui, tauriVersion, tuiVersion }
}

function ensureCleanWorktree(dryRun) {
  const status = gitOutput(['status', '--porcelain', '--untracked-files=no'])
  if (!status) return
  if (dryRun) {
    console.warn('[release:push] dry run continuing with tracked worktree changes')
    return
  }
  fail('Tracked worktree changes remain. Commit or discard them before pushing a release tag.')
}

function ensureBranch() {
  const branch = gitOutput(['branch', '--show-current'])
  if (!branch) fail('Cannot release from a detached HEAD')
  return branch
}

function ensureRemote(remote) {
  runGit(['remote', 'get-url', remote])
}

function ensureTagAvailable(remote, tag) {
  const localTag = runGit(['rev-parse', '--verify', '--quiet', `refs/tags/${tag}`], {
    allowFailure: true,
  })
  if (localTag.status === 0) fail(`Local tag ${tag} already exists`)

  const remoteTag = runGit(['ls-remote', '--exit-code', '--tags', remote, `refs/tags/${tag}`], {
    allowFailure: true,
  })
  if (remoteTag.status === 0) fail(`Remote tag ${tag} already exists on ${remote}`)
  if (remoteTag.status !== 2) {
    const output = `${remoteTag.stdout ?? ''}${remoteTag.stderr ?? ''}`.trim()
    fail(`Could not check remote tag ${tag}${output ? `:\n${output}` : ''}`)
  }
}

function printStep(dryRun, args) {
  const command = `git ${args.join(' ')}`
  if (dryRun) {
    console.log(`[release:push] would run: ${command}`)
  } else {
    console.log(`[release:push] ${command}`)
  }
}

function maybeRunGit(dryRun, args) {
  printStep(dryRun, args)
  if (!dryRun) runGit(args, { stdio: 'inherit' })
}

const { version, remote, dryRun } = parseArgs(process.argv.slice(2))
const tag = `v${version}`

ensureSemver(version)
const releaseTargets = resolveReleaseTargets(version)
ensureCleanWorktree(dryRun)
const branch = ensureBranch()
ensureRemote(remote)
ensureTagAvailable(remote, tag)

console.log(
  `[release:push] ${tag} will build: ${[
    releaseTargets.buildTauri ? 'desktop client' : null,
    releaseTargets.buildTui ? 'TUI' : null,
  ]
    .filter(Boolean)
    .join(', ')}`,
)

maybeRunGit(dryRun, ['push', remote, `HEAD:${branch}`])
maybeRunGit(dryRun, ['tag', '-a', tag, '-m', `Xero ${version}`])
maybeRunGit(dryRun, ['push', remote, tag])

const actionsUrl = 'https://github.com/hyperpush-org/xero/actions/workflows/release.yml'
if (dryRun) {
  console.log(`[release:push] dry run complete for ${tag}`)
} else {
  console.log(`[release:push] pushed ${tag}. Release workflow: ${actionsUrl}`)
}
