#!/usr/bin/env node
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import process from 'node:process'

const BRIDGE_VERSION = 'xero-cursor-sdk-bridge.v1'
const DEFAULT_MODEL = 'composer-latest'
const DEFAULT_MCP_MODE = 'observe-only'

function main() {
  return run().catch((error) => {
    emit('failed', {
      code: classifyBridgeError(error),
      message: error instanceof Error ? error.message : String(error),
      error: serializeError(error),
    })
    process.exitCode = 1
  })
}

async function run() {
  const args = parseArgs(process.argv.slice(2))
  if (args.help) {
    process.stdout.write(usage())
    return
  }

  if (args.selfTest) {
    const sdk = await importCursorSdk()
    const ok = typeof sdk.Agent?.create === 'function'
    emit(ok ? 'completed' : 'failed', {
      code: ok ? undefined : 'cursor_sdk_bridge_failed',
      message: ok ? 'Cursor SDK bridge import self-test passed.' : 'Cursor SDK Agent.create is unavailable.',
      sdkVersion: await readCursorSdkVersion(),
    })
    process.exitCode = ok ? 0 : 1
    return
  }

  const prompt = required(args.prompt, 'prompt')
  const repoRoot = path.resolve(required(args.repoRoot, 'repo-root'))
  const projectId = required(args.projectId, 'project-id')
  const runId = required(args.runId, 'run-id')
  const sessionId = required(args.sessionId, 'session-id')
  const model = args.model || DEFAULT_MODEL
  const xeroCliPath = required(args.xeroCliPath, 'xero-cli-path')
  const xeroStateDir = path.resolve(required(args.xeroStateDir, 'xero-state-dir'))
  const mcpMode = args.mcpMode || DEFAULT_MCP_MODE

  if (args.fixture) {
    await streamFixture(path.resolve(args.fixture), { projectId, runId, sessionId, model })
    return
  }

  const apiKey = readApiKey(args.apiKeyEnv)
  if (!apiKey) {
    emit('failed', {
      code: 'cursor_auth_missing',
      message: `Cursor requires ${args.apiKeyEnv || 'CURSOR_API_KEY'} before Xero can start a Cursor-backed run.`,
    })
    process.exitCode = 1
    return
  }

  const { Agent } = await importCursorSdk()
  const sdkVersion = await readCursorSdkVersion()
  const mcpServers = {
    xero: {
      type: 'stdio',
      command: xeroCliPath,
      args: [
        '--state-dir',
        xeroStateDir,
        'mcp',
        'serve-tools',
        '--project-id',
        projectId,
        '--run-id',
        runId,
        '--session-id',
        sessionId,
        '--repo',
        repoRoot,
        '--mode',
        mcpMode,
      ],
      cwd: repoRoot,
      env: sanitizedMcpEnv(),
    },
  }
  const platform = {
    stateRoot: path.join(xeroStateDir, 'cursor-sdk', safePathSegment(projectId)),
    workspaceRef: repoRoot,
  }

  let agent
  try {
    agent = await Agent.create({
      apiKey,
      model: { id: model },
      local: {
        cwd: repoRoot,
        settingSources: [],
      },
      mcpServers,
      platform,
    })
    const cursorRun = await agent.send(prompt, {
      mcpServers,
      onDelta: ({ update }) => emit('delta', { update }),
      onStep: ({ step }) => emit('step', { step }),
    })
    emit('started', {
      projectId,
      runId,
      sessionId,
      cursorAgentId: agent.agentId,
      cursorRunId: cursorRun.id,
      model,
      sdkVersion,
      runtime: 'local',
      mcpMode,
    })

    for await (const message of cursorRun.stream()) {
      emitSdkMessage(message)
    }

    const result = await cursorRun.wait()
    emit('completed', {
      projectId,
      runId,
      sessionId,
      cursorAgentId: agent.agentId,
      cursorRunId: cursorRun.id,
      status: result.status,
      result: result.result,
      model: result.model,
      durationMs: result.durationMs,
      git: result.git,
    })
  } catch (error) {
    emit('failed', {
      code: classifyBridgeError(error),
      message: error instanceof Error ? error.message : String(error),
      error: serializeError(error),
      sdkVersion,
    })
    process.exitCode = 1
  } finally {
    try {
      await agent?.[Symbol.asyncDispose]?.()
    } catch {
      agent?.close?.()
    }
  }
}

function emitSdkMessage(message) {
  emit('sdk_message', { message })
  if (!message || typeof message !== 'object') {
    return
  }
  if (message.type === 'assistant') {
    const content = Array.isArray(message.message?.content) ? message.message.content : []
    for (const block of content) {
      if (block?.type === 'text' && typeof block.text === 'string' && block.text.length > 0) {
        emit('delta', {
          role: 'assistant',
          text: block.text,
          cursorAgentId: message.agent_id,
          cursorRunId: message.run_id,
        })
      } else if (block?.type === 'tool_use') {
        emit('tool_call', {
          cursorAgentId: message.agent_id,
          cursorRunId: message.run_id,
          callId: block.id,
          name: block.name,
          status: 'running',
          args: block.input,
        })
      }
    }
  } else if (message.type === 'tool_call') {
    emit('tool_call', {
      cursorAgentId: message.agent_id,
      cursorRunId: message.run_id,
      callId: message.call_id,
      name: message.name,
      status: message.status,
      args: message.args,
      result: message.result,
      truncated: message.truncated,
    })
  } else if (message.type === 'thinking') {
    emit('step', {
      kind: 'thinking',
      cursorAgentId: message.agent_id,
      cursorRunId: message.run_id,
      text: message.text,
      thinkingDurationMs: message.thinking_duration_ms,
    })
  } else if (message.type === 'status') {
    emit('step', {
      kind: 'status',
      cursorAgentId: message.agent_id,
      cursorRunId: message.run_id,
      status: message.status,
      message: message.message,
    })
  } else if (message.type === 'task') {
    emit('step', {
      kind: 'task',
      cursorAgentId: message.agent_id,
      cursorRunId: message.run_id,
      status: message.status,
      text: message.text,
    })
  }
}

async function streamFixture(fixturePath, defaults) {
  const raw = fs.readFileSync(fixturePath, 'utf8')
  const events = raw.trim().startsWith('[')
    ? JSON.parse(raw)
    : raw
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line))

  emit('started', {
    ...defaults,
    cursorAgentId: 'fixture-agent',
    cursorRunId: 'fixture-run',
    sdkVersion: await readCursorSdkVersion().catch(() => undefined),
    runtime: 'fixture',
    mcpMode: DEFAULT_MCP_MODE,
  })
  for (const event of events) {
    if (event?.type === 'sdk_message') {
      emitSdkMessage(event.message)
    } else if (event?.type) {
      emit(event.type, event)
    } else {
      emitSdkMessage(event)
    }
  }
  emit('completed', {
    ...defaults,
    cursorAgentId: 'fixture-agent',
    cursorRunId: 'fixture-run',
    status: 'finished',
    runtime: 'fixture',
  })
}

async function importCursorSdk() {
  try {
    return await import('@cursor/sdk')
  } catch (error) {
    error.xeroCode = 'cursor_sdk_bridge_failed'
    throw error
  }
}

async function readCursorSdkVersion() {
  try {
    const pkgPath = requireResolve('@cursor/sdk/package.json')
    return JSON.parse(fs.readFileSync(pkgPath, 'utf8')).version
  } catch {
    return undefined
  }
}

function requireResolve(specifier) {
  return createRequire(import.meta.url).resolve(specifier)
}

function readApiKey(apiKeyEnv) {
  const envName = apiKeyEnv || 'CURSOR_API_KEY'
  return (process.env[envName] || '').trim()
}

function sanitizedMcpEnv() {
  const env = {}
  for (const key of ['PATH', 'HOME']) {
    if (process.env[key]) {
      env[key] = process.env[key]
    }
  }
  return env
}

function emit(type, payload = {}) {
  const event = {
    type,
    bridgeVersion: BRIDGE_VERSION,
    timestamp: new Date().toISOString(),
    ...stripUndefined(payload),
  }
  process.stdout.write(`${JSON.stringify(event)}\n`)
}

function stripUndefined(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return value
  }
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined))
}

function classifyBridgeError(error) {
  if (error?.xeroCode) {
    return error.xeroCode
  }
  const name = error?.name || ''
  const message = error instanceof Error ? error.message.toLowerCase() : String(error).toLowerCase()
  if (name === 'AuthenticationError' || message.includes('auth') || message.includes('api key')) {
    return 'cursor_auth_failed'
  }
  if (message.includes('model')) {
    return 'cursor_model_unavailable'
  }
  if (message.includes('mcp')) {
    return 'cursor_mcp_server_failed'
  }
  return 'cursor_sdk_bridge_failed'
}

function serializeError(error) {
  if (!(error instanceof Error)) {
    return { message: String(error) }
  }
  return stripUndefined({
    name: error.name,
    message: error.message,
    code: error.code,
    stack: process.env.XERO_CURSOR_BRIDGE_INCLUDE_STACK === '1' ? error.stack : undefined,
  })
}

function required(value, name) {
  if (typeof value !== 'string' || value.trim() === '') {
    const error = new Error(`Missing required --${name}.`)
    error.xeroCode = 'cursor_sdk_bridge_failed'
    throw error
  }
  return value
}

function safePathSegment(value) {
  return value.replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 120) || 'project'
}

function parseArgs(argv) {
  const parsed = {}
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '--help' || arg === '-h') {
      parsed.help = true
    } else if (arg === '--self-test') {
      parsed.selfTest = true
    } else if (arg === '--prompt') {
      parsed.prompt = nextValue(argv, ++index, arg)
    } else if (arg === '--repo-root') {
      parsed.repoRoot = nextValue(argv, ++index, arg)
    } else if (arg === '--project-id') {
      parsed.projectId = nextValue(argv, ++index, arg)
    } else if (arg === '--run-id') {
      parsed.runId = nextValue(argv, ++index, arg)
    } else if (arg === '--session-id') {
      parsed.sessionId = nextValue(argv, ++index, arg)
    } else if (arg === '--model') {
      parsed.model = nextValue(argv, ++index, arg)
    } else if (arg === '--api-key-env') {
      parsed.apiKeyEnv = nextValue(argv, ++index, arg)
    } else if (arg === '--xero-cli-path') {
      parsed.xeroCliPath = nextValue(argv, ++index, arg)
    } else if (arg === '--xero-state-dir') {
      parsed.xeroStateDir = nextValue(argv, ++index, arg)
    } else if (arg === '--mcp-mode') {
      parsed.mcpMode = nextValue(argv, ++index, arg)
    } else if (arg === '--fixture') {
      parsed.fixture = nextValue(argv, ++index, arg)
    } else {
      const error = new Error(`Unknown argument ${arg}.`)
      error.xeroCode = 'cursor_sdk_bridge_failed'
      throw error
    }
  }
  return parsed
}

function nextValue(argv, index, flag) {
  const value = argv[index]
  if (!value || value.startsWith('--')) {
    const error = new Error(`${flag} requires a value.`)
    error.xeroCode = 'cursor_sdk_bridge_failed'
    throw error
  }
  return value
}

function usage() {
  return [
    'Usage: node cursor-sdk-bridge.mjs --prompt PROMPT --repo-root PATH --project-id ID --run-id ID --session-id ID --xero-cli-path PATH --xero-state-dir PATH [--model MODEL] [--api-key-env ENV] [--mcp-mode MODE]',
    '',
    'Streams newline-delimited JSON events for Xero Cursor SDK runs.',
  ].join('\n')
}

await main()
