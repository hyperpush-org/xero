import { render, renderHook, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { AgentRuntimeDesktopAdapter } from '@/components/xero/agent-runtime'
import type { ConversationTurn } from '@xero/ui/components/transcript/conversation-section'
import {
  LiveAgentRuntimeView,
  useHistoricalConversationTurns,
  useHistoricalConversationTurnsState,
} from '@/components/xero/agent-runtime/live-agent-runtime'
import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type { SessionTranscriptDto } from '@/src/lib/xero-model'
import { createXeroHighChurnStore } from '@/src/features/xero/use-xero-desktop-state/high-churn-store'

vi.mock('@/components/xero/agent-runtime', () => ({
  AgentRuntime: ({
    agent,
    historicalConversationTurns,
    historicalConversationTurnsLoading,
  }: {
    agent: AgentPaneView
    historicalConversationTurns?: readonly ConversationTurn[]
    historicalConversationTurnsLoading?: boolean
  }) => (
    <div
      data-testid="agent-runtime"
      data-project-id={agent.project.id}
      data-session-id={agent.project.selectedAgentSessionId ?? ''}
      data-loading-history={historicalConversationTurnsLoading ? 'true' : 'false'}
    >
      {historicalConversationTurns
        ?.map((turn) => ('text' in turn ? turn.text : ''))
        .filter(Boolean)
        .join('\n') ?? null}
    </div>
  ),
}))

const PROJECT_ID = 'project-handoff'
const SESSION_ID = 'agent-session-handoff'

function publicRedaction() {
  return { redactionClass: 'public' as const, redacted: false, reason: null }
}

function makeAgentPane({
  activeRunId,
  projectId = PROJECT_ID,
  sessionId = SESSION_ID,
  runtimeRunIsTerminal = false,
  runtimeRunActionStatus = 'idle',
  runtimeStreamStatus = 'idle',
  sessionUpdatedAt = '2026-05-08T09:30:00Z',
  hasQueuedPrompt = false,
}: {
  activeRunId: string | null
  projectId?: string
  sessionId?: string
  runtimeRunIsTerminal?: boolean
  runtimeRunActionStatus?: AgentPaneView['runtimeRunActionStatus']
  runtimeStreamStatus?: AgentPaneView['runtimeStreamStatus']
  sessionUpdatedAt?: string
  hasQueuedPrompt?: boolean
}): AgentPaneView {
  return {
    project: {
      id: projectId,
      selectedAgentSessionId: sessionId,
      selectedAgentSession: {
        updatedAt: sessionUpdatedAt,
      },
    } as AgentPaneView['project'],
    runtimeRun: activeRunId
      ? ({
          runId: activeRunId,
          isTerminal: runtimeRunIsTerminal,
        } as AgentPaneView['runtimeRun'])
      : null,
    runtimeRunActionStatus,
    runtimeStreamStatus,
    selectedPrompt: {
      hasQueuedPrompt,
      text: hasQueuedPrompt ? 'queued prompt' : null,
      queuedAt: hasQueuedPrompt ? '2026-05-08T09:40:00Z' : null,
    },
  } as AgentPaneView
}

function makeTranscript({
  projectId = PROJECT_ID,
  sessionId = SESSION_ID,
  runId = 'run-A',
  text = 'answer from history',
}: {
  projectId?: string
  sessionId?: string
  runId?: string
  text?: string
} = {}): SessionTranscriptDto {
  return {
    contractVersion: 1,
    projectId,
    agentSessionId: sessionId,
    title: 'Session',
    summary: '',
    status: 'active',
    archived: false,
    archivedAt: null,
    runs: [
      {
        projectId,
        agentSessionId: sessionId,
        runId,
        providerId: 'p',
        modelId: 'm',
        status: 'completed',
        startedAt: '2026-05-08T09:00:00Z',
        completedAt: '2026-05-08T09:30:00Z',
        itemCount: 1,
      },
    ],
    items: [
      {
        contractVersion: 1,
        itemId: `${runId}:msg:1`,
        projectId,
        agentSessionId: sessionId,
        runId,
        providerId: 'p',
        modelId: 'm',
        sourceKind: 'owned_agent',
        sourceTable: 'agent_messages',
        sourceId: `${runId}:msg:1`,
        sequence: 1,
        createdAt: '2026-05-08T09:01:00Z',
        kind: 'message',
        actor: 'assistant',
        text,
        redaction: publicRedaction(),
      },
    ],
    redaction: publicRedaction(),
  }
}

function makeAdapter(transcript: SessionTranscriptDto): {
  adapter: AgentRuntimeDesktopAdapter
  getSessionTranscript: ReturnType<typeof vi.fn>
} {
  const getSessionTranscript = vi.fn(async () => transcript)
  return {
    getSessionTranscript,
    adapter: { getSessionTranscript } as unknown as AgentRuntimeDesktopAdapter,
  }
}

function makeTranscriptWithHandoff(): SessionTranscriptDto {
  return {
    contractVersion: 1,
    projectId: PROJECT_ID,
    agentSessionId: SESSION_ID,
    title: 'Handoff session',
    summary: '',
    status: 'active',
    archived: false,
    archivedAt: null,
    runs: [
      {
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        runId: 'run-A',
        providerId: 'p',
        modelId: 'm',
        status: 'handed_off',
        startedAt: '2026-05-08T09:00:00Z',
        completedAt: '2026-05-08T09:30:00Z',
        itemCount: 2,
      },
      {
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        runId: 'run-B',
        providerId: 'p',
        modelId: 'm',
        status: 'running',
        startedAt: '2026-05-08T09:31:00Z',
        completedAt: null,
        itemCount: 1,
      },
    ],
    items: [
      {
        contractVersion: 1,
        itemId: 'run-A:msg:1',
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        runId: 'run-A',
        providerId: 'p',
        modelId: 'm',
        sourceKind: 'owned_agent',
        sourceTable: 'agent_messages',
        sourceId: 'run-A:msg:1',
        sequence: 1,
        createdAt: '2026-05-08T09:01:00Z',
        kind: 'message',
        actor: 'user',
        text: 'long original prompt',
        redaction: publicRedaction(),
      },
      {
        contractVersion: 1,
        itemId: 'run-A:msg:2',
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        runId: 'run-A',
        providerId: 'p',
        modelId: 'm',
        sourceKind: 'owned_agent',
        sourceTable: 'agent_messages',
        sourceId: 'run-A:msg:2',
        sequence: 2,
        createdAt: '2026-05-08T09:10:00Z',
        kind: 'message',
        actor: 'assistant',
        text: 'first answer (filled context)',
        redaction: publicRedaction(),
      },
      {
        contractVersion: 1,
        itemId: 'run-B:msg:1',
        projectId: PROJECT_ID,
        agentSessionId: SESSION_ID,
        runId: 'run-B',
        providerId: 'p',
        modelId: 'm',
        sourceKind: 'owned_agent',
        sourceTable: 'agent_messages',
        sourceId: 'run-B:msg:1',
        sequence: 3,
        createdAt: '2026-05-08T09:32:00Z',
        kind: 'message',
        actor: 'assistant',
        text: 'continuation in fresh run',
        redaction: publicRedaction(),
      },
    ],
    redaction: publicRedaction(),
  }
}

describe('useHistoricalConversationTurns', () => {
  type ActiveRunHookProps = {
    activeRunId: string | null
    runtimeStreamStatus: AgentPaneView['runtimeStreamStatus']
  }

  it('keeps the previous runtime visible while a switched session transcript is loading', async () => {
    const highChurnStore = createXeroHighChurnStore()
    const { adapter } = makeAdapter(makeTranscript({ text: 'settled previous session' }))
    const { rerender } = render(
      <LiveAgentRuntimeView
        agent={makeAgentPane({ activeRunId: null })}
        highChurnStore={highChurnStore}
        desktopAdapter={adapter}
      />,
    )

    await waitFor(() => {
      expect(screen.getByTestId('agent-runtime')).toHaveAttribute(
        'data-loading-history',
        'false',
      )
    })
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-project-id', PROJECT_ID)
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-session-id', SESSION_ID)
    expect(screen.getByText('settled previous session')).toBeInTheDocument()

    rerender(
      <LiveAgentRuntimeView
        agent={makeAgentPane({
          activeRunId: null,
          projectId: 'project-shell',
          sessionId: '',
        })}
        highChurnStore={highChurnStore}
        desktopAdapter={adapter}
      />,
    )

    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-project-id', PROJECT_ID)
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-session-id', SESSION_ID)

    let resolveNextTranscript: ((transcript: SessionTranscriptDto) => void) | null = null
    const getSessionTranscript = vi.fn(
      () =>
        new Promise<SessionTranscriptDto>((resolve) => {
          resolveNextTranscript = resolve
        }),
    )
    const nextAdapter = {
      getSessionTranscript,
    } as unknown as AgentRuntimeDesktopAdapter

    rerender(
      <LiveAgentRuntimeView
        agent={makeAgentPane({
          activeRunId: null,
          projectId: 'project-next',
          sessionId: 'agent-session-next',
          sessionUpdatedAt: '2026-05-08T10:30:00Z',
        })}
        highChurnStore={highChurnStore}
        desktopAdapter={nextAdapter}
      />,
    )

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledWith({
        projectId: 'project-next',
        agentSessionId: 'agent-session-next',
        runId: null,
      })
    })
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-project-id', PROJECT_ID)
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute('data-session-id', SESSION_ID)
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute(
      'data-loading-history',
      'false',
    )

    resolveNextTranscript?.(
      makeTranscript({
        projectId: 'project-next',
        sessionId: 'agent-session-next',
        runId: 'run-next',
        text: 'settled next session',
      }),
    )

    await waitFor(() => {
      expect(screen.getByTestId('agent-runtime')).toHaveAttribute(
        'data-project-id',
        'project-next',
      )
    })
    expect(screen.getByTestId('agent-runtime')).toHaveAttribute(
      'data-session-id',
      'agent-session-next',
    )
    expect(screen.getByText('settled next session')).toBeInTheDocument()
  })

  it('returns null while no transcript fetch has settled (so the pane falls back to the live stream)', () => {
    const { adapter } = makeAdapter(makeTranscriptWithHandoff())
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(makeAgentPane({ activeRunId: 'run-B' }), adapter),
    )
    expect(result.current).toBeNull()
  })

  it('reports loading while no transcript fetch has settled', () => {
    const { adapter } = makeAdapter(makeTranscriptWithHandoff())
    const { result } = renderHook(() =>
      useHistoricalConversationTurnsState(makeAgentPane({ activeRunId: 'run-B' }), adapter),
    )
    expect(result.current).toEqual({ loading: true, turns: null })
  })

  it('fetches the session transcript and projects the source run plus a handoff_notice when the active run is the handoff target', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(makeAgentPane({ activeRunId: 'run-B' }), adapter),
    )

    await waitFor(() => {
      expect(result.current).not.toBeNull()
    })

    expect(getSessionTranscript).toHaveBeenCalledWith({
      projectId: PROJECT_ID,
      agentSessionId: SESSION_ID,
      runId: null,
    })

    const turns = result.current ?? []
    expect(turns.map((turn) => turn.kind)).toEqual([
      'message',
      'message',
      'handoff_notice',
    ])
    expect(turns[0]).toMatchObject({ role: 'user', text: 'long original prompt' })
    expect(turns[1]).toMatchObject({
      role: 'assistant',
      text: 'first answer (filled context)',
    })
    if (turns[2].kind === 'handoff_notice') {
      expect(turns[2].sourceRunId).toBe('run-A')
      expect(turns[2].targetRunId).toBe('run-B')
    }
  })

  it('keeps a terminal runtime run in historical conversation turns after reload', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter } = makeAdapter(transcript)
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(
        makeAgentPane({
          activeRunId: 'run-B',
          runtimeRunIsTerminal: true,
          runtimeStreamStatus: 'complete',
        }),
        adapter,
      ),
    )

    await waitFor(() => {
      expect(result.current).not.toBeNull()
    })

    const turns = result.current ?? []
    expect(
      turns.some((turn) => turn.kind === 'message' && turn.text === 'continuation in fresh run'),
    ).toBe(true)
  })

  it('fetches history for a terminal runtime run even when the stream still says live', async () => {
    const transcript = makeTranscript({
      runId: 'run-cancelled',
      text: 'history before the cancelled continuation',
    })
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(
        makeAgentPane({
          activeRunId: 'run-cancelled',
          runtimeRunIsTerminal: true,
          runtimeStreamStatus: 'live',
        }),
        adapter,
      ),
    )

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(1)
    })
    await waitFor(() => {
      expect(result.current).not.toBeNull()
    })

    expect(result.current).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: 'message',
          text: 'history before the cancelled continuation',
        }),
      ]),
    )
  })

  it('refetches when the active runId flips (the same-type handoff transition path)', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const { rerender } = renderHook(
      ({ activeRunId }: { activeRunId: string | null }) =>
        useHistoricalConversationTurns(makeAgentPane({ activeRunId }), adapter),
      { initialProps: { activeRunId: 'run-A' } },
    )

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(1)
    })

    rerender({ activeRunId: 'run-B' })

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(2)
    })
  })

  it('refetches when the selected session revision changes', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const { rerender } = renderHook(
      ({ sessionUpdatedAt }: { sessionUpdatedAt: string }) =>
        useHistoricalConversationTurns(
          makeAgentPane({
            activeRunId: null,
            sessionUpdatedAt,
          }),
          adapter,
        ),
      { initialProps: { sessionUpdatedAt: '2026-05-08T09:30:00Z' } },
    )

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(1)
    })

    rerender({ sessionUpdatedAt: '2026-05-08T09:35:00Z' })

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(2)
    })
  })

  it('suppresses history fetched before active run metadata arrives during stream attach', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const initialProps: ActiveRunHookProps = {
      activeRunId: null,
      runtimeStreamStatus: 'idle',
    }
    const { result, rerender } = renderHook(
      ({ activeRunId, runtimeStreamStatus }: ActiveRunHookProps) =>
        useHistoricalConversationTurns(
          makeAgentPane({
            activeRunId,
            runtimeStreamStatus,
          }),
          adapter,
        ),
      { initialProps },
    )

    await waitFor(() => {
      const includesActiveRunHistory = result.current?.some(
        (turn) => turn.kind === 'message' && turn.text === 'continuation in fresh run',
      )
      expect(includesActiveRunHistory).toBe(true)
    })

    rerender({ activeRunId: 'run-B', runtimeStreamStatus: 'replaying' })

    expect(result.current).toBeNull()
    expect(getSessionTranscript).toHaveBeenCalledTimes(1)

    rerender({ activeRunId: 'run-B', runtimeStreamStatus: 'complete' })

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(2)
    })
    await waitFor(() => {
      const includesActiveRunHistory = result.current?.some(
        (turn) => turn.kind === 'message' && turn.text === 'continuation in fresh run',
      )
      expect(includesActiveRunHistory).toBe(false)
    })
  })

  it('defers transcript fetches while a prompt or live runtime stream is active', async () => {
    const transcript = makeTranscriptWithHandoff()
    const { adapter, getSessionTranscript } = makeAdapter(transcript)
    const { result, rerender } = renderHook(
      ({ busy }: { busy: boolean }) =>
        useHistoricalConversationTurns(
          makeAgentPane({
            activeRunId: 'run-B',
            runtimeRunActionStatus: busy ? 'running' : 'idle',
            runtimeStreamStatus: busy ? 'live' : 'complete',
            hasQueuedPrompt: busy,
          }),
          adapter,
        ),
      { initialProps: { busy: true } },
    )

    expect(result.current).toBeNull()
    expect(getSessionTranscript).not.toHaveBeenCalled()

    rerender({ busy: false })

    await waitFor(() => {
      expect(getSessionTranscript).toHaveBeenCalledTimes(1)
    })
  })

  it('returns null when the desktop adapter does not expose getSessionTranscript', () => {
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(
        makeAgentPane({ activeRunId: 'run-B' }),
        {} as AgentRuntimeDesktopAdapter,
      ),
    )
    expect(result.current).toBeNull()
  })

  it('does not report loading when the desktop adapter does not expose getSessionTranscript', () => {
    const { result } = renderHook(() =>
      useHistoricalConversationTurnsState(
        makeAgentPane({ activeRunId: 'run-B' }),
        {} as AgentRuntimeDesktopAdapter,
      ),
    )
    expect(result.current).toEqual({ loading: false, turns: null })
  })
})
