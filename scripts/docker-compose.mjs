#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process'
import { platform } from 'node:os'

const isWindows = platform() === 'win32'
const args = process.argv.slice(2)

function probe(cmd, probeArgs) {
  const result = spawnSync(cmd, probeArgs, {
    stdio: 'ignore',
    shell: isWindows,
    timeout: 5_000,
  })
  return result.status === 0
}

function resolveComposeCommand() {
  if (probe('docker', ['compose', 'version'])) {
    return { cmd: 'docker', argsPrefix: ['compose'] }
  }

  if (probe('docker-compose', ['version'])) {
    return { cmd: 'docker-compose', argsPrefix: [] }
  }

  console.error(
    '[docker-compose] Docker Compose not found. Install the Docker Compose plugin (`docker compose`) or legacy `docker-compose`.',
  )
  process.exit(1)
}

const compose = resolveComposeCommand()
const child = spawn(compose.cmd, [...compose.argsPrefix, ...args], {
  stdio: 'inherit',
  shell: false,
})

child.on('exit', (code, signal) => {
  if (signal) {
    process.exit(signal === 'SIGINT' ? 130 : 1)
    return
  }

  process.exit(code ?? 1)
})

child.on('error', (error) => {
  console.error(`[docker-compose] ${error.message}`)
  process.exit(1)
})
