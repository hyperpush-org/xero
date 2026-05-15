import { describe, expect, it } from 'vitest'

import { buildHistoricalConversationTurns } from '@/components/xero/agent-runtime/session-history-projection'
import type {
  RunTranscriptSummaryDto,
  SessionTranscriptDto,
  SessionTranscriptItemDto,
} from '@/src/lib/xero-model'

const PROJECT_ID = 'project-handoff'
const SESSION_ID = 'session-handoff'
const PROVIDER_ID = 'openai_codex'
const MODEL_ID = 'gpt-omega'

function publicRedaction() {
  return { redactionClass: 'public' as const, redacted: false, reason: null }
}

function makeRun(
  runId: string,
  status: 'completed' | 'handed_off' | 'failed' | 'cancelled' | 'running',
  startedAt: string,
  itemCount: number,
): RunTranscriptSummaryDto {
  return {
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    runId,
    providerId: PROVIDER_ID,
    modelId: MODEL_ID,
    status,
    startedAt,
    completedAt: status === 'running' ? null : startedAt,
    itemCount,
  }
}

function makeMessageItem(
  runId: string,
  sequence: number,
  actor: 'user' | 'assistant',
  text: string,
): SessionTranscriptItemDto {
  return {
    contractVersion: 1,
    itemId: `${runId}:msg:${sequence}`,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    runId,
    providerId: PROVIDER_ID,
    modelId: MODEL_ID,
    sourceKind: 'owned_agent',
    sourceTable: 'agent_messages',
    sourceId: `${runId}:msg:${sequence}`,
    sequence,
    createdAt: '2026-05-08T10:00:00Z',
    kind: 'message',
    actor,
    text,
    redaction: publicRedaction(),
  }
}

function makeRunPromptItem(
  runId: string,
  sequence: number,
  text: string,
): SessionTranscriptItemDto {
  return {
    contractVersion: 1,
    itemId: `${runId}:prompt`,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    runId,
    providerId: PROVIDER_ID,
    modelId: MODEL_ID,
    sourceKind: 'owned_agent',
    sourceTable: 'agent_runs',
    sourceId: runId,
    sequence,
    createdAt: '2026-05-08T10:00:00Z',
    kind: 'message',
    actor: 'user',
    title: 'Run prompt',
    text,
    redaction: publicRedaction(),
  }
}

function makeEventMessageDeltaItem(
  runId: string,
  sequence: number,
  actor: 'user' | 'assistant',
  text: string,
): SessionTranscriptItemDto {
  return {
    contractVersion: 1,
    itemId: `${runId}:event:${sequence}`,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    runId,
    providerId: PROVIDER_ID,
    modelId: MODEL_ID,
    sourceKind: 'owned_agent',
    sourceTable: 'agent_events',
    sourceId: `${sequence}`,
    sequence,
    createdAt: '2026-05-08T10:00:00Z',
    kind: 'message',
    actor,
    title: 'Message delta',
    text,
    redaction: publicRedaction(),
  }
}

function makeNonMessageItem(
  runId: string,
  sequence: number,
): SessionTranscriptItemDto {
  return {
    contractVersion: 1,
    itemId: `${runId}:tool:${sequence}`,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    runId,
    providerId: PROVIDER_ID,
    modelId: MODEL_ID,
    sourceKind: 'runtime_stream',
    sourceTable: 'agent_events',
    sourceId: `${runId}:tool:${sequence}`,
    sequence,
    createdAt: '2026-05-08T10:00:00Z',
    kind: 'tool_call',
    actor: 'tool',
    toolName: 'edit',
    redaction: publicRedaction(),
  }
}

function makeTranscript(
  runs: RunTranscriptSummaryDto[],
  items: SessionTranscriptItemDto[],
): SessionTranscriptDto {
  return {
    contractVersion: 1,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    title: 'Handoff session',
    summary: '',
    status: 'active',
    archived: false,
    archivedAt: null,
    runs,
    items,
    redaction: publicRedaction(),
  }
}

describe('buildHistoricalConversationTurns', () => {
  it('returns an empty array when the transcript only contains the active run', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'running', '2026-05-08T10:00:00Z', 2)],
      [
        makeMessageItem('run-A', 1, 'user', 'hello'),
        makeMessageItem('run-A', 2, 'assistant', 'hi'),
      ],
    )

    expect(
      buildHistoricalConversationTurns(transcript, { activeRunId: 'run-A' }),
    ).toEqual([])
  })

  it('projects user and assistant messages from non-active runs in sequence order', () => {
    const transcript = makeTranscript(
      [
        makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 2),
        makeRun('run-B', 'running', '2026-05-08T10:00:00Z', 1),
      ],
      [
        makeMessageItem('run-A', 1, 'user', 'first prompt'),
        makeMessageItem('run-A', 2, 'assistant', 'first answer'),
        makeMessageItem('run-B', 3, 'user', 'second prompt'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, {
      activeRunId: 'run-B',
    })

    expect(turns).toHaveLength(2)
    expect(turns[0]).toMatchObject({
      kind: 'message',
      role: 'user',
      text: 'first prompt',
      sequence: 1,
    })
    expect(turns[1]).toMatchObject({
      kind: 'message',
      role: 'assistant',
      text: 'first answer',
      sequence: 2,
    })
  })

  it('uses durable agent messages for historical chat and drops raw owned-agent message deltas', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 7)],
      [
        makeRunPromptItem('run-A', 1, 'project overview request'),
        makeMessageItem('run-A', 2, 'user', 'project overview request'),
        makeEventMessageDeltaItem('run-A', 3, 'assistant', 'project overview request'),
        makeEventMessageDeltaItem('run-A', 4, 'assistant', 'This'),
        makeEventMessageDeltaItem('run-A', 5, 'assistant', ' project'),
        makeEventMessageDeltaItem('run-A', 6, 'assistant', ' is Xero.'),
        makeMessageItem('run-A', 7, 'assistant', 'This project is Xero.'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, { activeRunId: null })

    expect(turns).toHaveLength(2)
    expect(turns[0]).toMatchObject({
      kind: 'message',
      role: 'user',
      text: 'project overview request',
    })
    expect(turns[1]).toMatchObject({
      kind: 'message',
      role: 'assistant',
      text: 'This project is Xero.',
    })
  })

  it('falls back to the run prompt when a run has no persisted user message row', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 2)],
      [
        makeRunPromptItem('run-A', 1, 'legacy-free prompt fallback'),
        makeMessageItem('run-A', 2, 'assistant', 'answer'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, { activeRunId: null })

    expect(turns.map((turn) => turn.kind)).toEqual(['message', 'message'])
    expect(turns[0]).toMatchObject({
      role: 'user',
      text: 'legacy-free prompt fallback',
    })
    expect(turns[1]).toMatchObject({ role: 'assistant', text: 'answer' })
  })

  it('does not surface orphan raw message deltas as historical chat turns', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'failed', '2026-05-08T09:00:00Z', 3)],
      [
        makeRunPromptItem('run-A', 1, 'prompt before failure'),
        makeEventMessageDeltaItem('run-A', 2, 'assistant', 'partial'),
        makeEventMessageDeltaItem('run-A', 3, 'assistant', ' answer'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, { activeRunId: null })

    expect(turns).toEqual([
      expect.objectContaining({
        kind: 'message',
        role: 'user',
        text: 'prompt before failure',
      }),
    ])
  })

  it('inserts a handoff_notice turn between runs when the prior run handed off', () => {
    const transcript = makeTranscript(
      [
        makeRun('run-A', 'handed_off', '2026-05-08T09:00:00Z', 2),
        makeRun('run-B', 'running', '2026-05-08T10:00:00Z', 1),
      ],
      [
        makeMessageItem('run-A', 1, 'user', 'long prompt'),
        makeMessageItem('run-A', 2, 'assistant', 'long answer'),
        makeMessageItem('run-B', 3, 'user', 'long prompt'),
        makeMessageItem('run-B', 4, 'assistant', 'continuation'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, {
      activeRunId: null,
    })

    expect(turns.map((turn) => turn.kind)).toEqual([
      'message',
      'message',
      'handoff_notice',
      'message',
      'message',
    ])
    const handoff = turns[2]
    expect(handoff.kind).toBe('handoff_notice')
    if (handoff.kind === 'handoff_notice') {
      expect(handoff.sourceRunId).toBe('run-A')
      expect(handoff.targetRunId).toBe('run-B')
    }
  })

  it('does not insert a handoff_notice when the prior run completed normally', () => {
    const transcript = makeTranscript(
      [
        makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 1),
        makeRun('run-B', 'running', '2026-05-08T10:00:00Z', 1),
      ],
      [
        makeMessageItem('run-A', 1, 'assistant', 'done'),
        makeMessageItem('run-B', 2, 'user', 'next ask'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, {
      activeRunId: null,
    })

    expect(turns.map((turn) => turn.kind)).toEqual(['message', 'message'])
  })

  it('skips active-run items and emits a trailing handoff_notice when the active run is the handoff target', () => {
    const transcript = makeTranscript(
      [
        makeRun('run-A', 'handed_off', '2026-05-08T09:00:00Z', 2),
        makeRun('run-B', 'running', '2026-05-08T10:00:00Z', 2),
      ],
      [
        makeMessageItem('run-A', 1, 'user', 'over budget prompt'),
        makeMessageItem('run-A', 2, 'assistant', 'final source answer'),
        makeMessageItem('run-B', 3, 'user', 'over budget prompt'),
        makeMessageItem('run-B', 4, 'assistant', 'fresh target answer'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, {
      activeRunId: 'run-B',
    })

    expect(turns.map((turn) => turn.kind)).toEqual([
      'message',
      'message',
      'handoff_notice',
    ])
    expect(turns[0]).toMatchObject({ role: 'user', text: 'over budget prompt' })
    expect(turns[1]).toMatchObject({ role: 'assistant', text: 'final source answer' })
    if (turns[2].kind === 'handoff_notice') {
      expect(turns[2].sourceRunId).toBe('run-A')
      expect(turns[2].targetRunId).toBe('run-B')
    }
  })

  it('ignores non-message items so historical context only carries user/assistant turns', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 2)],
      [
        makeNonMessageItem('run-A', 1),
        makeMessageItem('run-A', 2, 'user', 'kept'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, {
      activeRunId: null,
    })

    expect(turns).toEqual([
      expect.objectContaining({ kind: 'message', role: 'user', text: 'kept' }),
    ])
  })

  it('extracts routing-suggestion markers from assistant messages into routing_suggestion turns', () => {
    const markerText =
      '<xero-routing-suggestion target="plan" reason="multi-file refactor" summary="rewrite routing layer"/>\n\nI think Plan would handle this better.'
    const transcript = makeTranscript(
      [makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 2)],
      [
        makeMessageItem('run-A', 1, 'user', 'refactor everything'),
        makeMessageItem('run-A', 2, 'assistant', markerText),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, { activeRunId: null })

    expect(turns.map((turn) => turn.kind)).toEqual([
      'message',
      'message',
      'routing_suggestion',
    ])
    expect(turns[1]).toMatchObject({
      role: 'assistant',
      text: 'I think Plan would handle this better.',
    })
    const routing = turns[2]
    if (routing.kind === 'routing_suggestion') {
      expect(routing.targetAgentId).toBe('plan')
      expect(routing.reason).toBe('multi-file refactor')
      expect(routing.summary).toBe('rewrite routing layer')
      expect(routing.isResolved).toBe(true)
    }
  })

  it('leaves messages without the marker untouched', () => {
    const transcript = makeTranscript(
      [makeRun('run-A', 'completed', '2026-05-08T09:00:00Z', 2)],
      [
        makeMessageItem('run-A', 1, 'user', 'simple question'),
        makeMessageItem('run-A', 2, 'assistant', 'a plain answer'),
      ],
    )

    const turns = buildHistoricalConversationTurns(transcript, { activeRunId: null })

    expect(turns.map((turn) => turn.kind)).toEqual(['message', 'message'])
    expect(turns[1]).toMatchObject({ text: 'a plain answer' })
  })
})
