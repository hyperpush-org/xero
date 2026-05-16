import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { AgentRuntimeDesktopAdapter } from '@/components/xero/agent-runtime'
import { useHistoricalConversationTurns } from '@/components/xero/agent-runtime/live-agent-runtime'
import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type { SessionTranscriptDto } from '@/src/lib/xero-model'

const PROJECT_ID = 'project-handoff'
const SESSION_ID = 'agent-session-handoff'

function publicRedaction() {
  return { redactionClass: 'public' as const, redacted: false, reason: null }
}

function makeAgentPane({
  activeRunId,
  runtimeRunActionStatus = 'idle',
  runtimeStreamStatus = 'idle',
  hasQueuedPrompt = false,
}: {
  activeRunId: string | null
  runtimeRunActionStatus?: AgentPaneView['runtimeRunActionStatus']
  runtimeStreamStatus?: AgentPaneView['runtimeStreamStatus']
  hasQueuedPrompt?: boolean
}): AgentPaneView {
  return {
    project: {
      id: PROJECT_ID,
      selectedAgentSessionId: SESSION_ID,
    } as AgentPaneView['project'],
    runtimeRun: activeRunId
      ? ({ runId: activeRunId } as AgentPaneView['runtimeRun'])
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

  it('returns null while no transcript fetch has settled (so the pane falls back to the live stream)', () => {
    const { adapter } = makeAdapter(makeTranscriptWithHandoff())
    const { result } = renderHook(() =>
      useHistoricalConversationTurns(makeAgentPane({ activeRunId: 'run-B' }), adapter),
    )
    expect(result.current).toBeNull()
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
})
