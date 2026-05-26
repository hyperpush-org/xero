#!/usr/bin/env node
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const BRIDGE_VERSION = 'xero-cursor-sdk-bridge.v1'
const DEFAULT_MODEL = 'composer-latest'
const CURSOR_AUTO_MODEL_SENTINEL = 'cursor-auto'
const DEFAULT_MCP_MODE = 'observe-only'
const AUTO_MODEL_INPUTS = new Set(['auto', 'default', CURSOR_AUTO_MODEL_SENTINEL, 'cursor_auto'])
const AUTO_CATALOG_ALIASES = ['auto', 'default', CURSOR_AUTO_MODEL_SENTINEL, 'cursor_auto']
const DEFAULT_LOCAL_AUTO_MODE = 'catalog_alias'

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

  if (args.listModels) {
    await listModels(args)
    return
  }

  const prompt = required(args.prompt, 'prompt')
  const repoRoot = path.resolve(required(args.repoRoot, 'repo-root'))
  const projectId = required(args.projectId, 'project-id')
  const runId = required(args.runId, 'run-id')
  const sessionId = required(args.sessionId, 'session-id')
  const modelRoute = parseCursorModelRoute(args.model)
  const xeroCliPath = required(args.xeroCliPath, 'xero-cli-path')
  const xeroStateDir = path.resolve(required(args.xeroStateDir, 'xero-state-dir'))
  const mcpMode = args.mcpMode || DEFAULT_MCP_MODE

  if (args.fixture) {
    await streamFixture(path.resolve(args.fixture), {
      projectId,
      runId,
      sessionId,
      ...cursorModelRouteEventMetadata(modelRoute, null),
      model: modelRoute.storageModelId,
    })
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

  const { Agent, Cursor } = await importCursorSdk()
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
  let modelRequest
  try {
    modelRequest = await resolveCursorModelRequest(modelRoute, {
      Cursor,
      apiKey,
      runtime: 'local',
      localAutoMode: args.localAutoMode,
    })
    agent = await Agent.create(buildCursorAgentCreateOptions({
      apiKey,
      modelRequest,
      local: {
        cwd: repoRoot,
        settingSources: [],
      },
      mcpServers,
      platform,
    }))
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
      model: modelRoute.storageModelId,
      sdkVersion,
      runtime: 'local',
      mcpMode,
      ...cursorModelRouteEventMetadata(modelRoute, modelRequest),
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
      resolvedModel: resolvedCursorModel(result.model ?? cursorRun.model ?? agent.model),
      ...cursorModelRouteEventMetadata(modelRoute, modelRequest),
      durationMs: result.durationMs,
      git: result.git,
    })
  } catch (error) {
    emit('failed', {
      code: classifyBridgeError(error),
      message: error instanceof Error ? error.message : String(error),
      error: serializeError(error),
      sdkVersion,
      ...cursorModelRouteEventMetadata(modelRoute, modelRequest),
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

async function listModels(args) {
  const apiKey = readApiKey(args.apiKeyEnv)
  if (!apiKey) {
    emit('failed', {
      code: 'cursor_auth_missing',
      message: `Cursor requires ${args.apiKeyEnv || 'CURSOR_API_KEY'} before Xero can refresh Cursor models.`,
    })
    process.exitCode = 1
    return
  }

  const { Cursor } = await importCursorSdk()
  try {
    const models = await listCursorModels(Cursor, apiKey)
    const normalized = normalizeCursorModelCatalog(models)
    emit('completed', {
      runtime: 'local',
      catalogSource: 'cursor.models.list',
      modelCount: normalized.length,
      autoAliases: findCursorAutoCatalogAliases(normalized),
      composerAliases: findCursorComposerCatalogAliases(normalized),
      models: normalized,
    })
  } catch (error) {
    emit('failed', {
      code: 'cursor_model_catalog_unavailable',
      message:
        error instanceof Error
          ? `Cursor model listing failed: ${error.message}`
          : `Cursor model listing failed: ${String(error)}`,
      error: serializeError(error),
    })
    process.exitCode = 1
  }
}

export function parseCursorModelRoute(input) {
  const rawModelId =
    typeof input === 'string' && input.trim().length > 0 ? input.trim() : DEFAULT_MODEL
  const normalized = rawModelId.toLowerCase()
  if (AUTO_MODEL_INPUTS.has(normalized)) {
    return {
      route: 'auto',
      inputModelId: rawModelId,
      storageModelId: CURSOR_AUTO_MODEL_SENTINEL,
    }
  }
  if (normalized === DEFAULT_MODEL) {
    return {
      route: 'composer_latest',
      inputModelId: rawModelId,
      storageModelId: DEFAULT_MODEL,
    }
  }
  return {
    route: 'explicit',
    inputModelId: rawModelId,
    storageModelId: rawModelId,
  }
}

export async function resolveCursorModelRequest(route, options = {}) {
  if (route.route === 'composer_latest' || route.route === 'explicit') {
    return {
      requestedModelRoute: route.route,
      requestedModelId: route.storageModelId,
      agentModelSelection: { id: route.storageModelId },
      resolution: 'explicit_model',
    }
  }

  const runtime = options.runtime ?? 'local'
  const localAutoMode = normalizeLocalAutoMode(options.localAutoMode)
  if (runtime !== 'local' || localAutoMode === 'omit_model') {
    return {
      requestedModelRoute: 'auto',
      requestedModelId: undefined,
      agentModelSelection: undefined,
      resolution: 'omitted_model',
    }
  }

  if (localAutoMode === 'disabled') {
    throw cursorAutoUnavailableError(
      'Cursor Auto/default routing is not documented for the local SDK runtime yet. Select Composer Latest or a concrete Cursor model.',
    )
  }

  let catalog
  try {
    catalog =
      options.modelCatalog ??
      (await listCursorModels(requiredCursorNamespace(options.Cursor), required(options.apiKey, 'api-key')))
  } catch (error) {
    const wrapped = new Error(
      error instanceof Error
        ? `Cursor model listing failed while validating Auto/default routing: ${error.message}`
        : `Cursor model listing failed while validating Auto/default routing: ${String(error)}`,
    )
    wrapped.xeroCode = 'cursor_model_catalog_unavailable'
    wrapped.cause = error
    throw wrapped
  }

  const normalizedCatalog = normalizeCursorModelCatalog(catalog)
  const alias = findCursorAutoCatalogAlias(normalizedCatalog)
  if (!alias) {
    throw cursorAutoUnavailableError(
      'Cursor did not return a catalog-backed Auto/default model alias for this account. Select Composer Latest, select a concrete model, or refresh Cursor models after the account has access.',
    )
  }

  return {
    requestedModelRoute: 'auto',
    requestedModelId: alias,
    agentModelSelection: { id: alias },
    resolution: 'catalog_alias',
    catalogAlias: alias,
  }
}

export function buildCursorAgentCreateOptions({ apiKey, modelRequest, local, mcpServers, platform }) {
  return stripUndefined({
    apiKey,
    model: modelRequest.agentModelSelection,
    local,
    mcpServers,
    platform,
  })
}

export async function listCursorModels(Cursor, apiKey) {
  return await requiredCursorNamespace(Cursor).models.list({ apiKey })
}

export function normalizeCursorModelCatalog(models) {
  if (!Array.isArray(models)) {
    return []
  }

  return models
    .map((model) => {
      if (!model || typeof model !== 'object') {
        return null
      }
      const id = stringOrNull(model.id)
      if (!id) {
        return null
      }
      return stripUndefined({
        id,
        displayName: stringOrNull(model.displayName) ?? id,
        description: stringOrNull(model.description) ?? undefined,
        aliases: stringArray(model.aliases),
        parameters: Array.isArray(model.parameters) ? model.parameters : undefined,
        variants: Array.isArray(model.variants) ? model.variants : undefined,
      })
    })
    .filter(Boolean)
}

export function findCursorAutoCatalogAlias(models) {
  return findCatalogAlias(models, AUTO_CATALOG_ALIASES)
}

export function findCursorAutoCatalogAliases(models) {
  return findCatalogAliases(models, AUTO_CATALOG_ALIASES)
}

export function findCursorComposerCatalogAliases(models) {
  return findCatalogAliases(models, [DEFAULT_MODEL])
}

function findCatalogAlias(models, candidates) {
  return findCatalogAliases(models, candidates)[0] ?? null
}

function findCatalogAliases(models, candidates) {
  const normalizedCandidates = new Set(candidates.map((candidate) => candidate.toLowerCase()))
  const out = []
  for (const model of normalizeCursorModelCatalog(models)) {
    for (const value of [model.id, ...(model.aliases ?? [])]) {
      const trimmed = typeof value === 'string' ? value.trim() : ''
      if (trimmed.length > 0 && normalizedCandidates.has(trimmed.toLowerCase()) && !out.includes(trimmed)) {
        out.push(trimmed)
      }
    }
  }
  return out
}

function cursorModelRouteEventMetadata(route, modelRequest) {
  return stripUndefined({
    requestedModelRoute: modelRequest?.requestedModelRoute ?? route.route,
    requestedModelId:
      modelRequest?.requestedModelId ??
      (route.route === 'auto' ? undefined : route.storageModelId),
    requestedModelInput: route.inputModelId,
    cursorModelSentinel: route.route === 'auto' ? route.storageModelId : undefined,
    modelResolution: modelRequest?.resolution,
    runtime: 'local',
  })
}

function normalizeLocalAutoMode(input) {
  const value = (input ?? process.env.XERO_CURSOR_LOCAL_AUTO_MODE ?? DEFAULT_LOCAL_AUTO_MODE)
    .trim()
    .toLowerCase()
    .replace(/-/g, '_')
  if (value === 'omit' || value === 'omit_model' || value === 'omitted_model') {
    return 'omit_model'
  }
  if (value === 'catalog' || value === 'catalog_alias') {
    return 'catalog_alias'
  }
  if (value === 'disabled' || value === 'fail' || value === 'unavailable') {
    return 'disabled'
  }

  const error = new Error(
    `Unknown Cursor local Auto mode \`${input}\`. Use omit_model, catalog_alias, or disabled.`,
  )
  error.xeroCode = 'cursor_sdk_bridge_failed'
  throw error
}

function requiredCursorNamespace(Cursor) {
  if (!Cursor?.models || typeof Cursor.models.list !== 'function') {
    const error = new Error('Cursor.models.list is unavailable in this Cursor SDK build.')
    error.xeroCode = 'cursor_model_catalog_unavailable'
    throw error
  }
  return Cursor
}

function cursorAutoUnavailableError(message) {
  const error = new Error(message)
  error.xeroCode = 'cursor_auto_unavailable'
  return error
}

function resolvedCursorModel(value) {
  if (typeof value === 'string') {
    return value.trim() || undefined
  }
  if (value && typeof value === 'object') {
    return stringOrNull(value.id) ?? stringOrNull(value.modelId) ?? stringOrNull(value.model)
  }
  return undefined
}

function stringOrNull(value) {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null
}

function stringArray(value) {
  if (!Array.isArray(value)) {
    return []
  }
  return value.map(stringOrNull).filter(Boolean)
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
  let resolvedModel
  for (const event of events) {
    if (event?.type === 'sdk_message') {
      emitSdkMessage(event.message)
    } else if (event?.type) {
      resolvedModel = resolvedModel ?? resolvedCursorModel(event.resolvedModel ?? event.model)
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
    resolvedModel,
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
    } else if (arg === '--list-models') {
      parsed.listModels = true
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
    } else if (arg === '--local-auto-mode') {
      parsed.localAutoMode = nextValue(argv, ++index, arg)
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
    'Usage: node cursor-sdk-bridge.mjs --prompt PROMPT --repo-root PATH --project-id ID --run-id ID --session-id ID --xero-cli-path PATH --xero-state-dir PATH [--model MODEL] [--api-key-env ENV] [--mcp-mode MODE] [--local-auto-mode omit_model|catalog_alias|disabled]',
    '       node cursor-sdk-bridge.mjs --list-models [--api-key-env ENV]',
    '',
    'Streams newline-delimited JSON events for Xero Cursor SDK runs.',
  ].join('\n')
}

function isMainModule() {
  return process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
}

if (isMainModule()) {
  await main()
}
