import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'

const scriptPath = new URL('./cursor-sdk-bridge.mjs', import.meta.url).pathname

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
  assert(events.some((event) => event.type === 'sdk_message'))
  assert(events.some((event) => event.type === 'delta' && event.text === 'hello from cursor'))
  assert(events.some((event) => event.type === 'tool_call' && event.name === 'shell'))
  assert.equal(events.at(-1).type, 'completed')
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
