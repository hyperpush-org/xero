#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process'
import { realpathSync, statSync } from 'node:fs'
import { platform } from 'node:os'
import { delimiter } from 'node:path'
import { dirname, isAbsolute, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = realpathSync(resolve(scriptDir, '..'))
const composeFile = resolve(repoRoot, 'server/docker-compose.yml')
const composeScript = resolve(scriptDir, 'docker-compose.mjs')
const isWindows = platform() === 'win32'
const args = new Set(process.argv.slice(2))
const dryRun = args.has('--dry-run')
const skipDb = args.has('--no-db')
const currentPid = process.pid

const ports = [
  { port: 3000, label: 'client Vite/Tauri dev server' },
  { port: 3001, label: 'landing Next dev server' },
  { port: 3002, label: 'cloud Vite dev server' },
  { port: 4000, label: 'Phoenix relay server' },
]

const stopPatterns = [
  {
    label: 'root dev orchestrator',
    test: (command) =>
      command.includes('concurrently') &&
      command.includes('dev:server') &&
      command.includes('dev:tauri'),
  },
  {
    label: 'root pnpm dev',
    test: (command) =>
      /\bpnpm(?:\.cmd)?\b.*\brun\b.*\bdev\b/.test(command) &&
      !/\bstop:all\b/.test(command) &&
      !/\bdev:preflight\b/.test(command),
  },
  {
    label: 'Phoenix server',
    test: (command) => /\bmix(?:\.bat)?\s+phx\.server\b/.test(command),
  },
  {
    label: 'Tauri dev launcher',
    test: (command) =>
      command.includes('client/scripts/tauri-dev.mjs') ||
      command.includes('client\\scripts\\tauri-dev.mjs') ||
      /\btauri(?:\.cmd)?\b\s+dev\b/.test(command),
  },
  {
    label: 'Xero TUI dev launcher',
    test: (command) =>
      command.includes('client/scripts/xero-tui-dev.mjs') ||
      command.includes('client\\scripts\\xero-tui-dev.mjs') ||
      command.includes('--bin xero-tui'),
  },
  {
    label: 'Vite dev server',
    test: (command) =>
      /\bvite(?:\.js)?\b/.test(command) &&
      (/\b--port(?:=|\s+)3000\b/.test(command) ||
        /\b--port(?:=|\s+)3002\b/.test(command) ||
        /\bvite\s+dev\b/.test(command)),
  },
  {
    label: 'Next dev server',
    test: (command) => /\bnext(?:\.js)?\b.*\bdev\b/.test(command),
  },
  {
    label: 'Docker Postgres log tail',
    test: (command) =>
      command.includes('docker-compose.yml') &&
      command.includes('logs') &&
      command.includes('postgres'),
  },
]

main().catch((error) => {
  console.error(`[stop:all] ${error?.message ?? String(error)}`)
  process.exit(1)
})

async function main() {
  const processes = getProcessTable()
  const byPid = new Map(processes.map((processInfo) => [processInfo.pid, processInfo]))
  const targets = new Map()

  for (const portInfo of ports) {
    for (const pid of listeningPids(portInfo.port)) {
      const processInfo = byPid.get(pid) ?? describePid(pid)
      if (!processInfo) continue

      if (isProjectRelated(processInfo)) {
        addTarget(targets, processInfo, portInfo.label)
      } else {
        console.warn(
          `[stop:all] Skipping pid ${pid} on :${portInfo.port}; it does not look like a Xero project process.`,
        )
      }
    }
  }

  for (const processInfo of processes) {
    const matched = stopPatterns.find((pattern) => pattern.test(processInfo.command))
    if (!matched || !isProjectRelated(processInfo)) continue
    addTarget(targets, processInfo, matched.label)
  }

  addDescendants(targets, processes)

  const targetList = [...targets.values()].sort(
    (a, b) => descendantDepth(b, processes) - descendantDepth(a, processes),
  )

  if (dryRun) {
    printTargets(targetList, 'Would stop')
  } else {
    await stopTargets(targetList)
  }

  if (skipDb) {
    console.log('[stop:all] Skipping Docker Postgres shutdown because --no-db was provided.')
  } else if (dryRun) {
    console.log(`[stop:all] Would run Docker Compose down for ${relative(repoRoot, composeFile)}.`)
  } else {
    await stopDockerPostgres()
  }
}

function getProcessTable() {
  if (isWindows) {
    return []
  }

  const result = run('ps', ['-axo', 'pid=,ppid=,command='])
  if (result.status !== 0) return []

  return result.stdout
    .split(/\r?\n/)
    .map((line) => {
      const match = line.match(/^\s*(\d+)\s+(\d+)\s+(.*)$/)
      if (!match) return null
      return {
        pid: Number.parseInt(match[1], 10),
        ppid: Number.parseInt(match[2], 10),
        command: match[3],
      }
    })
    .filter(Boolean)
    .filter((processInfo) => processInfo.pid !== currentPid)
}

function describePid(pid) {
  if (pid === currentPid) return null
  const result = run('ps', ['-p', String(pid), '-o', 'pid=,ppid=,command='])
  if (result.status !== 0) return null
  const line = result.stdout.trim()
  const match = line.match(/^(\d+)\s+(\d+)\s+(.*)$/)
  if (!match) return null
  return {
    pid: Number.parseInt(match[1], 10),
    ppid: Number.parseInt(match[2], 10),
    command: match[3],
  }
}

function listeningPids(port) {
  if (isWindows) return []
  const result = run('lsof', ['-nP', `-tiTCP:${port}`, '-sTCP:LISTEN'])
  if (result.status !== 0 && !result.stdout.trim()) return []
  return result.stdout
    .split(/\s+/)
    .map((value) => Number.parseInt(value, 10))
    .filter(Number.isFinite)
    .filter((pid) => pid !== currentPid)
}

function isProjectRelated(processInfo) {
  const command = processInfo.command
  if (command.includes('scripts/stop-all.mjs') || command.includes('scripts\\stop-all.mjs')) {
    return false
  }

  if (command.includes(repoRoot)) return true

  const cwd = processCwd(processInfo.pid)
  return cwd ? isUnderRepo(cwd) : false
}

function processCwd(pid) {
  if (isWindows) return null
  const result = run('lsof', ['-a', '-p', String(pid), '-d', 'cwd', '-Fn'])
  if (result.status !== 0) return null
  const entry = result.stdout
    .split(/\r?\n/)
    .find((line) => line.startsWith('n'))
  if (!entry) return null

  try {
    return realpathSync(entry.slice(1))
  } catch {
    return entry.slice(1)
  }
}

function isUnderRepo(path) {
  const rel = relative(repoRoot, path)
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel))
}

function addTarget(targets, processInfo, reason) {
  if (processInfo.pid === currentPid) return
  const existing = targets.get(processInfo.pid)
  if (existing) {
    existing.reasons.add(reason)
    return
  }
  targets.set(processInfo.pid, { ...processInfo, reasons: new Set([reason]) })
}

function addDescendants(targets, processes) {
  let added = true
  while (added) {
    added = false
    for (const processInfo of processes) {
      if (processInfo.pid === currentPid || targets.has(processInfo.pid)) continue
      if (!targets.has(processInfo.ppid)) continue
      addTarget(targets, processInfo, 'child of matched Xero dev process')
      added = true
    }
  }
}

function descendantDepth(processInfo, processes) {
  const children = processes.filter((child) => child.ppid === processInfo.pid)
  if (!children.length) return 0
  return 1 + Math.max(...children.map((child) => descendantDepth(child, processes)))
}

function printTargets(targetList, verb) {
  if (!targetList.length) {
    console.log(`[stop:all] ${verb}: no matching Xero dev server processes.`)
    return
  }

  for (const processInfo of targetList) {
    console.log(
      `[stop:all] ${verb} pid ${processInfo.pid} (${[...processInfo.reasons].join(', ')}): ${processInfo.command}`,
    )
  }
}

async function stopTargets(targetList) {
  if (!targetList.length) {
    console.log('[stop:all] No matching Xero dev server processes were found.')
    return
  }

  printTargets(targetList, 'Stopping')
  for (const processInfo of targetList) {
    signalPid(processInfo.pid, 'SIGTERM')
  }

  const remaining = await waitForExit(
    targetList.map((processInfo) => processInfo.pid),
    5_000,
  )

  if (!remaining.length) {
    console.log('[stop:all] Project dev server processes stopped.')
    return
  }

  for (const pid of remaining) {
    console.warn(`[stop:all] pid ${pid} did not exit after SIGTERM; sending SIGKILL.`)
    signalPid(pid, 'SIGKILL')
  }
}

function signalPid(pid, signal) {
  try {
    process.kill(pid, signal)
  } catch (error) {
    if (error?.code !== 'ESRCH') {
      console.warn(`[stop:all] Could not signal pid ${pid}: ${error.message}`)
    }
  }
}

async function waitForExit(pids, timeoutMs) {
  const deadline = Date.now() + timeoutMs
  let remaining = pids.filter(isRunning)
  while (remaining.length && Date.now() < deadline) {
    await sleep(250)
    remaining = remaining.filter(isRunning)
  }
  return remaining
}

function isRunning(pid) {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

async function stopDockerPostgres() {
  if (!commandExists('docker') && !commandExists('docker-compose')) {
    console.warn('[stop:all] Docker Compose is not available; skipping Postgres container shutdown.')
    return
  }

  if (commandExists('docker') && !dockerDaemonReady()) {
    console.warn('[stop:all] Docker is not running; skipping Postgres container shutdown.')
    return
  }

  console.log('[stop:all] Stopping Docker Postgres service...')
  const code = await streamRun(process.execPath, [composeScript, '-f', composeFile, 'down'], {
    cwd: repoRoot,
  })
  if (code !== 0) {
    throw new Error(`Docker Compose down failed with exit code ${code}.`)
  }
  console.log('[stop:all] Docker Postgres service stopped.')
}

function dockerDaemonReady() {
  const probe = run('docker', ['info', '--format', '{{.ServerVersion}}'], {
    timeout: 5_000,
  })
  return probe.status === 0
}

function commandExists(command) {
  const pathDirs = (process.env.PATH ?? '').split(delimiter).filter(Boolean)
  const extensions = isWindows
    ? ['', ...(process.env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD').split(';')]
    : ['']

  return pathDirs.some((dir) =>
    extensions.some((extension) => isRunnableFile(resolve(dir, `${command}${extension}`))),
  )
}

function isRunnableFile(path) {
  try {
    const stat = statSync(path)
    if (!stat.isFile()) return false
    return isWindows || (stat.mode & 0o111) !== 0
  } catch {
    return false
  }
}

function run(command, commandArgs, options = {}) {
  return spawnSync(command, commandArgs, {
    cwd: options.cwd,
    encoding: 'utf8',
    shell: options.shell ?? false,
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: options.timeout ?? 10_000,
  })
}

function streamRun(command, commandArgs, options = {}) {
  return new Promise((resolveRun) => {
    const child = spawn(command, commandArgs, {
      cwd: options.cwd,
      shell: false,
      stdio: 'inherit',
    })

    child.on('exit', (code) => resolveRun(code ?? 1))
    child.on('error', (error) => {
      console.warn(`[stop:all] ${error.message}`)
      resolveRun(1)
    })
  })
}

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms))
}
