import { describe, expect, it, vi } from 'vitest'
import type { MutableRefObject } from 'react'
import {
  ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
  attachRuntimeStreamSubscription,
  createRuntimeStreamEventBuffer,
  mergeRuntimeStreamEvents,
  RUNTIME_STREAM_BATCH_WINDOW_MS,
} from './runtime-stream'
import {
  MAX_RUNTIME_STREAM_ITEMS,
  createRuntimeStreamView,
  estimateRuntimeStreamViewBytes,
  type RuntimeStreamEventDto,
  type RuntimeStreamActivityItemView,
  type RuntimeStreamToolItemView,
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
      toolResultPreview: null,
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

function makeReasoningRuntimeStreamEvent(sequence: number, text: string): RuntimeStreamEventDto {
  return makeRuntimeStreamEvent(sequence, {
    kind: 'activity',
    text,
    code: 'owned_agent_reasoning',
    title: 'Reasoning',
    detail: text.trim() || 'Owned agent reasoning summary updated.',
  })
}

function isReasoningActivityItem(
  item: RuntimeStreamView['items'][number],
): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === 'owned_agent_reasoning'
}

function makeToolRuntimeStreamEvent(
  sequence: number,
  toolCallId = `call-${sequence}`,
  overrides: Partial<RuntimeStreamEventDto['item']> = {},
): RuntimeStreamEventDto {
  return makeRuntimeStreamEvent(sequence, {
    kind: 'tool',
    text: null,
    toolCallId,
    toolName: 'read',
    toolState: 'succeeded',
    detail: `Read file ${sequence}.`,
    ...overrides,
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

  it('accepts sparse stream sequences while preserving the latest stream projection', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1),
      makeRuntimeStreamEvent(3),
    ])

    expect(stream?.lastSequence).toBe(3)
    expect(stream?.status).toBe('live')
    expect(stream?.lastIssue).toBeNull()
    expect(stream?.transcriptItems[0]?.text).toBe('message-1message-3')
  })

  it('keeps reasoning bubbles and earlier tools in one ordered timeline', () => {
    const toolBurst = Array.from({ length: MAX_RUNTIME_STREAM_ITEMS + 8 }, (_, index) =>
      makeToolRuntimeStreamEvent(index + 3, `call-read-${index}`),
    )

    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1, { transcriptRole: 'user', text: 'Please inspect the repo.' }),
      makeReasoningRuntimeStreamEvent(2, 'I should inspect the files first.'),
      ...toolBurst,
    ])

    const retainedReasoning = stream?.items.find(
      (item) => item.kind === 'activity' && item.code === 'owned_agent_reasoning',
    )
    const retainedTools = stream?.items.filter((item) => item.kind === 'tool') ?? []

    expect(retainedReasoning).toMatchObject({
      kind: 'activity',
      text: 'I should inspect the files first.',
    })
    expect(retainedTools).toHaveLength(MAX_RUNTIME_STREAM_ITEMS + 8)
    expect(retainedTools[0]?.toolCallId).toBe('call-read-0')
  })

  it('does not merge reasoning across intervening transcript turns', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeReasoningRuntimeStreamEvent(1, 'First thought.'),
      makeRuntimeStreamEvent(2, { text: 'Visible assistant update.' }),
      makeReasoningRuntimeStreamEvent(3, 'Second thought.'),
    ])

    const reasoningItems = stream?.items.filter(isReasoningActivityItem) ?? []

    expect(reasoningItems.map((item) => item.text)).toEqual([
      'First thought.',
      'Second thought.',
    ])
    expect(stream?.items.map((item) => `${item.kind}:${item.sequence}`)).toEqual([
      'activity:1',
      'transcript:2',
      'activity:3',
    ])
  })

  it('uses whitespace reasoning deltas as summary boundaries', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeReasoningRuntimeStreamEvent(1, 'Inspecting the repo'),
      makeReasoningRuntimeStreamEvent(2, '\n\n'),
      makeReasoningRuntimeStreamEvent(3, 'Inspecting files for details'),
    ])

    const renderedReasoningItems = stream?.items.filter(isReasoningActivityItem) ?? []

    expect(renderedReasoningItems.map((item) => item.text)).toEqual([
      'Inspecting the repo',
      'Inspecting files for details',
    ])
    expect(stream?.items.map((item) => `${item.kind}:${item.sequence}:${item.kind === 'activity' ? item.code : ''}`)).toEqual([
      'activity:1:owned_agent_reasoning',
      'activity:2:owned_agent_reasoning_boundary',
      'activity:3:owned_agent_reasoning',
    ])
  })

  it('does not merge assistant transcript deltas across intervening tool calls', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeRuntimeStreamEvent(1, { text: 'Before the tool. ' }),
      makeToolRuntimeStreamEvent(2, 'call-read'),
      makeRuntimeStreamEvent(3, { text: 'After the tool.' }),
    ])

    expect(stream?.transcriptItems.map((item) => item.text)).toEqual([
      'Before the tool. ',
      'After the tool.',
    ])
    expect(stream?.items.map((item) => `${item.kind}:${item.sequence}`)).toEqual([
      'transcript:1',
      'tool:2',
      'transcript:3',
    ])
  })

  it('keeps tool lifecycle updates at their first timeline position', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeReasoningRuntimeStreamEvent(1, 'Inspecting project details'),
      makeToolRuntimeStreamEvent(2, 'call-read-index', {
        toolState: 'running',
        detail: 'Reading client/src/index.ts.',
      }),
      makeReasoningRuntimeStreamEvent(3, 'Structuring my research approach'),
      makeToolRuntimeStreamEvent(4, 'call-read-index', {
        toolState: 'succeeded',
        detail: 'Read client/src/index.ts.',
        toolResultPreview: 'export { App } from "./App"',
      }),
    ])

    expect(stream?.items.map((item) => `${item.kind}:${item.sequence}`)).toEqual([
      'activity:1',
      'tool:2',
      'activity:3',
    ])

    const toolItem = stream?.items.find(
      (item): item is RuntimeStreamToolItemView => item.kind === 'tool',
    )

    expect(toolItem).toMatchObject({
      id: 'tool:run-1:2',
      sequence: 2,
      createdAt: '2026-04-16T13:30:02Z',
      toolCallId: 'call-read-index',
      toolState: 'succeeded',
      detail: 'Read client/src/index.ts.',
      toolResultPreview: 'export { App } from "./App"',
    })
  })

  it('does not merge reasoning across a later tool lifecycle update', () => {
    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeReasoningRuntimeStreamEvent(1, 'Inspecting project details'),
      makeToolRuntimeStreamEvent(2, 'call-read-index', {
        toolState: 'running',
        detail: 'Reading client/src/index.ts.',
      }),
      makeReasoningRuntimeStreamEvent(3, 'Inspecting structure deeper'),
      makeToolRuntimeStreamEvent(4, 'call-read-index', {
        toolState: 'succeeded',
        detail: 'Read client/src/index.ts.',
      }),
      makeReasoningRuntimeStreamEvent(5, 'Organizing a response'),
    ])

    const reasoningItems = stream?.items.filter(isReasoningActivityItem) ?? []

    expect(reasoningItems.map((item) => item.text)).toEqual([
      'Inspecting project details',
      'Inspecting structure deeper',
      'Organizing a response',
    ])
    expect(stream?.items.map((item) => `${item.kind}:${item.sequence}`)).toEqual([
      'activity:1',
      'tool:2',
      'activity:3',
      'activity:5',
    ])
  })

  it('does not evict earlier visible tools when later tool bursts exceed the raw cap', () => {
    const firstToolBurst = Array.from({ length: 7 }, (_, index) =>
      makeToolRuntimeStreamEvent(index + 2, `call-first-${index}`),
    )
    const secondToolBurst = Array.from({ length: MAX_RUNTIME_STREAM_ITEMS + 4 }, (_, index) =>
      makeToolRuntimeStreamEvent(index + 10, `call-second-${index}`),
    )

    const stream = mergeRuntimeStreamEvents(makeRuntimeStream(), [
      makeReasoningRuntimeStreamEvent(1, 'Inspecting project structure'),
      ...firstToolBurst,
      makeReasoningRuntimeStreamEvent(9, 'Inspecting structure deeper'),
      ...secondToolBurst,
    ])

    const firstThoughtIndex = stream?.items.findIndex(
      (item) => item.kind === 'activity' && item.text === 'Inspecting project structure',
    ) ?? -1
    const firstToolIndex = stream?.items.findIndex(
      (item) => item.kind === 'tool' && item.toolCallId === 'call-first-0',
    ) ?? -1
    const secondThoughtIndex = stream?.items.findIndex(
      (item) => item.kind === 'activity' && item.text === 'Inspecting structure deeper',
    ) ?? -1
    const secondToolIndex = stream?.items.findIndex(
      (item) => item.kind === 'tool' && item.toolCallId === 'call-second-0',
    ) ?? -1

    expect(firstThoughtIndex).toBeGreaterThanOrEqual(0)
    expect(firstToolIndex).toBeGreaterThan(firstThoughtIndex)
    expect(secondThoughtIndex).toBeGreaterThan(firstToolIndex)
    expect(secondToolIndex).toBeGreaterThan(secondThoughtIndex)
    expect(stream?.items.filter((item) => item.kind === 'tool')).toHaveLength(
      firstToolBurst.length + secondToolBurst.length,
    )
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

  it('requests a full persisted replay on a cold runtime stream subscription', async () => {
    let stream: RuntimeStreamView | null = null
    const adapter = {
      subscribeRuntimeStream: vi.fn(
        async (projectId, agentSessionId, itemKinds) => ({
          response: {
            projectId,
            agentSessionId,
            runtimeKind: 'openai_codex',
            runId: 'run-1',
            sessionId: 'runtime-session-1',
            flowId: 'flow-1',
            subscribedItemKinds: itemKinds,
          },
          unsubscribe: vi.fn(),
        }),
      ),
    } as Pick<XeroDesktopAdapter, 'subscribeRuntimeStream'> as XeroDesktopAdapter
    const updateRuntimeStream = vi.fn(
      (
        _projectId: string,
        _agentSessionId: string,
        updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
      ) => {
        stream = updater(stream)
      },
    )

    const cleanup = attachRuntimeStreamSubscription({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runtimeSession: makeRuntimeSession(),
      runId: 'run-1',
      adapter,
      runtimeActionRefreshKeysRef: { current: {} },
      updateRuntimeStream,
      scheduleRuntimeMetadataRefresh: vi.fn(),
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(adapter.subscribeRuntimeStream).toHaveBeenCalledWith(
      'project-1',
      'agent-session-main',
      ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      expect.any(Function),
      expect.any(Function),
      {
        afterSequence: null,
        replayLimit: null,
      },
    )

    cleanup()
  })

  it('requests a bounded incremental replay when resubscribing an existing stream', async () => {
    let stream: RuntimeStreamView | null = {
      ...makeRuntimeStream(),
      lastSequence: 42,
      items: [
        {
          id: 'transcript:run-1:42',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 42,
          createdAt: '2026-04-16T13:30:42Z',
          role: 'assistant',
          text: 'Existing stream item.',
        },
      ],
    }
    const adapter = {
      subscribeRuntimeStream: vi.fn(
        async (projectId, agentSessionId, itemKinds) => ({
          response: {
            projectId,
            agentSessionId,
            runtimeKind: 'openai_codex',
            runId: 'run-1',
            sessionId: 'runtime-session-1',
            flowId: 'flow-1',
            subscribedItemKinds: itemKinds,
          },
          unsubscribe: vi.fn(),
        }),
      ),
    } as Pick<XeroDesktopAdapter, 'subscribeRuntimeStream'> as XeroDesktopAdapter
    const updateRuntimeStream = vi.fn(
      (
        _projectId: string,
        _agentSessionId: string,
        updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
      ) => {
        stream = updater(stream)
      },
    )

    const cleanup = attachRuntimeStreamSubscription({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runtimeSession: makeRuntimeSession(),
      runId: 'run-1',
      adapter,
      runtimeActionRefreshKeysRef: { current: {} },
      updateRuntimeStream,
      scheduleRuntimeMetadataRefresh: vi.fn(),
    })

    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(adapter.subscribeRuntimeStream).toHaveBeenCalledWith(
      'project-1',
      'agent-session-main',
      ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      expect.any(Function),
      expect.any(Function),
      {
        afterSequence: 42,
        replayLimit: 200,
      },
    )

    cleanup()
  })
})
