import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { test } from 'vitest'
import {
  buildCursorAgentCreateOptions,
  findCursorAutoCatalogAlias,
  normalizeCursorModelCatalog,
  parseCursorModelRoute,
  resolveCursorModelRequest,
} from './cursor-sdk-bridge.mjs'

const scriptPath = path.resolve(process.cwd(), 'scripts/cursor-sdk-bridge.mjs')

test('Cursor model routes distinguish Auto, Composer Latest, and explicit ids', () => {
  assert.deepEqual(parseCursorModelRoute('auto'), {
    route: 'auto',
    inputModelId: 'auto',
    storageModelId: 'cursor-auto',
  })
  assert.equal(parseCursorModelRoute('default').storageModelId, 'cursor-auto')
  assert.equal(parseCursorModelRoute(undefined).route, 'composer_latest')
  assert.equal(parseCursorModelRoute('composer-latest').route, 'composer_latest')
  assert.deepEqual(parseCursorModelRoute('claude-4-opus'), {
    route: 'explicit',
    inputModelId: 'claude-4-opus',
    storageModelId: 'claude-4-opus',
  })
})

test('Auto route omits model when local omitted-model support is enabled', async () => {
  const modelRequest = await resolveCursorModelRequest(parseCursorModelRoute('auto'), {
    runtime: 'local',
    localAutoMode: 'omit_model',
  })
  const options = buildCursorAgentCreateOptions({
    apiKey: 'cursor-key',
    modelRequest,
    local: { cwd: '/tmp/project', settingSources: [] },
    mcpServers: {},
    platform: { stateRoot: '/tmp/state', workspaceRef: '/tmp/project' },
  })

  assert.equal(modelRequest.requestedModelRoute, 'auto')
  assert.equal(modelRequest.requestedModelId, undefined)
  assert.equal(Object.hasOwn(options, 'model'), false)
})

test('Auto route maps to a catalog-confirmed alias when local omit support is disabled', async () => {
  const modelRequest = await resolveCursorModelRequest(parseCursorModelRoute('auto'), {
    runtime: 'local',
    localAutoMode: 'catalog_alias',
    modelCatalog: [
      { id: 'cursor-default-router', displayName: 'Cursor Default', aliases: ['default'] },
    ],
  })

  assert.equal(modelRequest.requestedModelRoute, 'auto')
  assert.equal(modelRequest.requestedModelId, 'default')
  assert.deepEqual(modelRequest.agentModelSelection, { id: 'default' })
})

test('Auto route fails when no safe local mapping exists', async () => {
  await assert.rejects(
    () =>
      resolveCursorModelRequest(parseCursorModelRoute('auto'), {
        runtime: 'local',
        localAutoMode: 'catalog_alias',
        modelCatalog: [
          { id: 'composer-family', displayName: 'Composer', aliases: ['composer-latest'] },
        ],
      }),
    (error) => {
      assert.equal(error.xeroCode, 'cursor_auto_unavailable')
      assert.match(error.message, /Composer Latest/)
      return true
    },
  )
})

test('Composer Latest still sends composer-latest explicitly', async () => {
  const modelRequest = await resolveCursorModelRequest(parseCursorModelRoute('composer-latest'), {
    runtime: 'local',
    localAutoMode: 'omit_model',
  })

  assert.equal(modelRequest.requestedModelRoute, 'composer_latest')
  assert.equal(modelRequest.requestedModelId, 'composer-latest')
  assert.deepEqual(modelRequest.agentModelSelection, { id: 'composer-latest' })
})

test('Cursor model catalog normalization discovers Auto aliases', () => {
  const catalog = normalizeCursorModelCatalog([
    {
      id: 'router',
      displayName: 'Router',
      aliases: ['auto', 'composer-latest'],
    },
    {
      id: '',
      displayName: 'blank',
    },
  ])

  assert.equal(catalog.length, 1)
  assert.equal(findCursorAutoCatalogAlias(catalog), 'auto')
})

test('fixture mode streams normalized JSONL without Cursor auth', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'xero-cursor-bridge-'))
  const fixturePath = path.join(tempDir, 'events.jsonl')
  fs.writeFileSync(
    fixturePath,
    [
      JSON.stringify({
        type: 'sdk_message',
        message: {
          type: 'assistant',
          agent_id: 'agent-fixture',
          run_id: 'cursor-run-fixture',
          message: {
            role: 'assistant',
            content: [{ type: 'text', text: 'hello from cursor' }],
          },
        },
      }),
      JSON.stringify({
        type: 'tool_call',
        cursorAgentId: 'agent-fixture',
        cursorRunId: 'cursor-run-fixture',
        callId: 'call-1',
        name: 'shell',
        status: 'running',
      }),
      JSON.stringify({
        type: 'completed',
        cursorAgentId: 'agent-fixture',
        cursorRunId: 'cursor-run-fixture',
        resolvedModel: 'composer-latest',
      }),
    ].join('\n'),
  )

  const result = spawnSync(
    process.execPath,
    [
      scriptPath,
      '--prompt',
      'hello',
      '--repo-root',
      tempDir,
      '--project-id',
      'project',
      '--run-id',
      'run',
      '--session-id',
      'session',
      '--xero-cli-path',
      process.execPath,
      '--xero-state-dir',
      tempDir,
      '--fixture',
      fixturePath,
    ],
    { encoding: 'utf8' },
  )

  assert.equal(result.status, 0, result.stderr)
  const events = result.stdout
    .trim()
    .split(/\r?\n/)
    .map((line) => JSON.parse(line))
  assert.equal(events[0].type, 'started')
  assert.equal(events[0].requestedModelRoute, 'composer_latest')
  assert.equal(events[0].requestedModelId, 'composer-latest')
  assert(events.some((event) => event.type === 'sdk_message'))
  assert(events.some((event) => event.type === 'delta' && event.text === 'hello from cursor'))
  assert(events.some((event) => event.type === 'tool_call' && event.name === 'shell'))
  assert.equal(events.at(-1).type, 'completed')
  assert.equal(events.at(-1).resolvedModel, 'composer-latest')
})

test('missing Cursor auth exits nonzero with structured error', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'xero-cursor-bridge-auth-'))
  const result = spawnSync(
    process.execPath,
    [
      scriptPath,
      '--prompt',
      'hello',
      '--repo-root',
      tempDir,
      '--project-id',
      'project',
      '--run-id',
      'run',
      '--session-id',
      'session',
      '--xero-cli-path',
      process.execPath,
      '--xero-state-dir',
      tempDir,
      '--api-key-env',
      'XERO_CURSOR_TEST_MISSING_KEY',
    ],
    {
      encoding: 'utf8',
      env: { ...process.env, XERO_CURSOR_TEST_MISSING_KEY: '' },
    },
  )

  assert.notEqual(result.status, 0)
  const event = JSON.parse(result.stdout.trim().split(/\r?\n/).at(-1))
  assert.equal(event.type, 'failed')
  assert.equal(event.code, 'cursor_auth_missing')
})
