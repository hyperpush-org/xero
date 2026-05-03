import { describe, expect, it, vi } from 'vitest'
import type { MutableRefObject } from 'react'
import {
  attachRuntimeStreamSubscription,
  createRuntimeStreamEventBuffer,
  mergeRuntimeStreamEvents,
  RUNTIME_STREAM_BATCH_WINDOW_MS,
} from './runtime-stream'
import {
  createRuntimeStreamView,
  estimateRuntimeStreamViewBytes,
  type RuntimeStreamEventDto,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'
import type { RuntimeSessionView } from '@/src/lib/xero-model/runtime'
import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'

function makeRuntimeStreamEvent(
  sequence: number,
  overrides: Partial<RuntimeStreamEventDto['item']> = {},
): RuntimeStreamEventDto {
  const kind = overrides.kind ?? 'transcript'

  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind,
      runId: 'run-1',
      sequence,
      sessionId: 'session-1',
      flowId: 'flow-1',
      text: kind === 'transcript' ? `message-${sequence}` : null,
      transcriptRole: kind === 'transcript' ? 'assistant' : null,
      toolCallId: null,
      toolName: null,
      toolState: null,
      toolSummary: null,
      skillId: null,
      skillStage: null,
      skillResult: null,
      skillSource: null,
      skillCacheStatus: null,
      skillDiagnostic: null,
      actionId: null,
      boundaryId: null,
      actionType: null,
      title: null,
      detail: null,
      code: null,
      message: null,
      retryable: null,
      createdAt: `2026-04-16T13:30:${String(sequence).padStart(2, '0')}Z`,
      ...overrides,
    },
  }
}

function makeRuntimeStream(): RuntimeStreamView {
  return createRuntimeStreamView({
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    status: 'live',
  })
}

function makeSessionRuntimeStreamEvent(agentSessionId: string, sequence: number): RuntimeStreamEventDto {
  const runId = `run-${agentSessionId}`
  return {
    ...makeRuntimeStreamEvent(sequence, {
      runId,
      text: `${agentSessionId}:${sequence};`,
    }),
    agentSessionId,
    runId,
    sessionId: 'runtime-session-1',
    flowId: 'flow-1',
    item: {
      ...makeRuntimeStreamEvent(sequence, {
        runId,
        text: `${agentSessionId}:${sequence};`,
      }).item,
      runId,
      sessionId: 'runtime-session-1',
      flowId: 'flow-1',
    },
  }
}

function makeRuntimeSession(): RuntimeSessionView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: 'flow-1',
    sessionId: 'runtime-session-1',
    accountId: 'acct-1',
    phase: 'authenticated',
    phaseLabel: 'Authenticated',
    runtimeLabel: 'OpenAI Codex',
    accountLabel: 'acct-1',
    sessionLabel: 'runtime-session-1',
    callbackBound: true,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-16T13:30:00Z',
    isAuthenticated: true,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: false,
    isFailed: false,
  }
}

describe('runtime stream event coalescing', () => {
  it('flushes non-urgent stream items in one buffered update', () => {
    let stream: RuntimeStreamView | null = makeRuntimeStream()
    let scheduledFlush: (() => void) | null = null
    const updateRuntimeStream = vi.fn(
      (
        _projectId: string,
        _agentSessionId: string,
        updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
      ) => {
        stream = updater(stream)
      },
    )

    const buffer = createRuntimeStreamEventBuffer({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runtimeKind: 'openai_codex',
      runId: 'run-1',
      sessionId: 'session-1',
      flowId: 'flow-1',
      subscribedItemKinds: ['transcript'],
      runtimeActionRefreshKeysRef: { current: {} },
      updateRuntimeStream,
      scheduleRuntimeMetadataRefresh: vi.fn(),
      scheduleFlush: (callback) => {
        scheduledFlush = callback
        return vi.fn()
      },
    })

    buffer.enqueue(makeRuntimeStreamEvent(1))
    buffer.enqueue(makeRuntimeStreamEvent(2))

    expect(updateRuntimeStream).not.toHaveBeenCalled()
    const flush = scheduledFlush as (() => void) | null
    flush?.()

    expect(updateRuntimeStream).toHaveBeenCalledTimes(1)
    expect(stream?.lastSequence).toBe(2)
    expect(stream?.transcriptItems[0]?.text).toBe('message-1message-2')
  })

  it('flushes pending items immediately when an action-required event arrives', () => {
    let stream: RuntimeStreamView | null = makeRuntimeStream()
    let scheduledFlush: (() => void) | null = null
    const cancelScheduledFlush = vi.fn()
    const refreshKeysRef: MutableRefObject<Record<string, Set<string>>> = { current: {} }
    const scheduleRuntimeMetadataRefresh = vi.fn()
    const updateRuntimeStream = vi.fn(
      (
        _projectId: string,
        _agentSessionId: string,
        updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
      ) => {
        stream = updater(stream)
      },
    )
    const buffer = createRuntimeStreamEventBuffer({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runtimeKind: 'openai_codex',
      runId: 'run-1',
      sessionId: 'session-1',
      flowId: 'flow-1',
      subscribedItemKinds: ['transcript', 'action_required'],
      runtimeActionRefreshKeysRef: refreshKeysRef,
      updateRuntimeStream,
      scheduleRuntimeMetadataRefresh,
      scheduleFlush: (callback) => {
        scheduledFlush = callback
        return cancelScheduledFlush
      },
    })

    buffer.enqueue(makeRuntimeStreamEvent(1))
    buffer.enqueue(
      makeRuntimeStreamEvent(2, {
        kind: 'action_required',
        text: null,
        actionId: 'action-1',
        boundaryId: 'boundary-1',
        actionType: 'terminal_input_required',
        title: 'Terminal input required',
        detail: 'The runtime needs operator input.',
      }),
    )

    expect(updateRuntimeStream).toHaveBeenCalledTimes(1)
    expect(cancelScheduledFlush).toHaveBeenCalledTimes(1)
    expect(scheduledFlush).toBeTypeOf('function')
    expect(stream?.lastSequence).toBe(2)
    expect(stream?.actionRequired[0]?.actionId).toBe('action-1')
    expect(scheduleRuntimeMetadataRefresh).toHaveBeenCalledWith(
      'project-1',
      'runtime_stream:action_required',
    )
  })

  it('dedupes repeated stream item sequences inside a batch', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1),
      makeRuntimeStreamEvent(1, { text: 'duplicate' }),
    ])

    expect(stream?.lastSequence).toBe(1)
    expect(stream?.transcriptItems).toHaveLength(1)
    expect(stream?.transcriptItems[0]?.text).toBe('message-1')
  })

  it('reports sequence gaps once while preserving the latest stream projection', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1),
      makeRuntimeStreamEvent(3),
    ])

    expect(stream?.lastSequence).toBe(3)
    expect(stream?.status).toBe('stale')
    expect(stream?.lastIssue?.code).toBe('runtime_stream_sequence_gap')
    expect(stream?.lastIssue?.message).toContain('expected 2, received 3')
  })

  it('estimates retained bytes for bounded session stream caches', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1, { text: 'x'.repeat(256) }),
      makeRuntimeStreamEvent(2, { text: 'y'.repeat(256) }),
    ])

    expect(estimateRuntimeStreamViewBytes(stream)).toBeGreaterThan(512)
    expect(estimateRuntimeStreamViewBytes(null)).toBe(0)
  })

  it('isolates six simultaneous runtime stream subscriptions by agent session', async () => {
    const sessionIds = Array.from({ length: 6 }, (_, index) => `agent-session-${index + 1}`)
    const handlers = new Map<string, (payload: RuntimeStreamEventDto) => void>()
    const unsubscribes = new Map<string, ReturnType<typeof vi.fn>>()
    const streams: Record<string, RuntimeStreamView> = {}
    const updateRuntimeStream = vi.fn(
      (
        projectId: string,
        agentSessionId: string,
        updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
      ) => {
        const key = `${projectId}:${agentSessionId}`
        const nextStream = updater(streams[key] ?? null)
        if (nextStream) {
          streams[key] = nextStream
        } else {
          delete streams[key]
        }
      },
    )
    const adapter = {
      subscribeRuntimeStream: vi.fn(
        async (projectId, agentSessionId, itemKinds, handler) => {
          handlers.set(agentSessionId, handler)
          const unsubscribe = vi.fn()
          unsubscribes.set(agentSessionId, unsubscribe)
          return {
            response: {
              projectId,
              agentSessionId,
              runtimeKind: 'openai_codex',
              runId: `run-${agentSessionId}`,
              sessionId: 'runtime-session-1',
              flowId: 'flow-1',
              subscribedItemKinds: itemKinds,
            },
            unsubscribe,
          }
        },
      ),
    } as Pick<XeroDesktopAdapter, 'subscribeRuntimeStream'> as XeroDesktopAdapter

    const cleanups = sessionIds.map((agentSessionId) =>
      attachRuntimeStreamSubscription({
        projectId: 'project-1',
        agentSessionId,
        runtimeSession: makeRuntimeSession(),
        runId: `run-${agentSessionId}`,
        adapter,
        runtimeActionRefreshKeysRef: { current: {} },
        updateRuntimeStream,
        scheduleRuntimeMetadataRefresh: vi.fn(),
      }),
    )

    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(adapter.subscribeRuntimeStream).toHaveBeenCalledTimes(6)

    for (const agentSessionId of sessionIds) {
      const handler = handlers.get(agentSessionId)
      expect(handler).toBeTypeOf('function')
      for (let sequence = 1; sequence <= 30; sequence += 1) {
        handler?.(makeSessionRuntimeStreamEvent(agentSessionId, sequence))
      }
    }

    await new Promise((resolve) => setTimeout(resolve, RUNTIME_STREAM_BATCH_WINDOW_MS + 5))

    expect(Object.keys(streams)).toHaveLength(6)
    for (const agentSessionId of sessionIds) {
      const stream = streams[`project-1:${agentSessionId}`]
      expect(stream?.agentSessionId).toBe(agentSessionId)
      expect(stream?.lastSequence).toBe(30)
      expect(stream?.transcriptItems.map((item) => item.text).join('')).toContain(`${agentSessionId}:30;`)
      for (const otherSessionId of sessionIds.filter((id) => id !== agentSessionId)) {
        expect(stream?.transcriptItems.map((item) => item.text).join('')).not.toContain(`${otherSessionId}:`)
      }
    }

    cleanups.forEach((cleanup) => cleanup())
    for (const unsubscribe of unsubscribes.values()) {
      expect(unsubscribe).toHaveBeenCalledTimes(1)
    }
  })
})
