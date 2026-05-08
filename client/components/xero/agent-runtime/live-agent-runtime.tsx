"use client"

import { lazy, memo, Suspense, useEffect, useMemo, useState } from 'react'

import type { AgentRuntimeDesktopAdapter, AgentRuntimeProps } from '@/components/xero/agent-runtime'
import type { ConversationTurn } from '@/components/xero/agent-runtime/conversation-section'
import { buildHistoricalConversationTurns } from '@/components/xero/agent-runtime/session-history-projection'
import { Skeleton } from '@/components/ui/skeleton'
import {
  selectRuntimeStreamForProject,
  useXeroHighChurnStoreValue,
  type AgentPaneView,
  type XeroHighChurnStore,
} from '@/src/features/xero/use-xero-desktop-state'
import { getAgentMessagesUnavailableCredentialReason } from '@/src/features/xero/use-xero-desktop-state/runtime-provider'
import { getRuntimeStreamStatusLabel } from '@/src/lib/xero-model/runtime-stream'

const LazyAgentRuntime = lazy(() =>
  import('@/components/xero/agent-runtime').then((module) => ({ default: module.AgentRuntime })),
)

function AgentRuntimeLoadingShell() {
  return (
    <div
      role="status"
      aria-label="Loading agent runtime"
      className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-background px-3 py-2"
    >
      <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 pb-2">
        <div className="flex min-w-0 items-center gap-2">
          <Skeleton className="h-6 w-6 rounded-md" />
          <Skeleton className="h-3 w-32" />
        </div>
        <Skeleton className="h-7 w-24" />
      </div>
      <div className="flex min-h-0 flex-1 flex-col justify-end gap-3 py-4">
        <Skeleton className="h-16 w-[72%] max-w-xl self-start" />
        <Skeleton className="h-20 w-[78%] max-w-2xl self-end" />
        <Skeleton className="h-12 w-[56%] max-w-lg self-start" />
      </div>
      <Skeleton className="h-16 shrink-0 rounded-lg" />
    </div>
  )
}

export function useAgentViewWithLiveRuntimeStream(
  agent: AgentPaneView | null,
  highChurnStore: XeroHighChurnStore,
): AgentPaneView | null {
  const projectId = agent?.project.id ?? null
  const agentSessionId = agent?.project.selectedAgentSessionId ?? null
  const streamSelector = useMemo(
    () => selectRuntimeStreamForProject(projectId, agentSessionId),
    [agentSessionId, projectId],
  )
  const runtimeStream = useXeroHighChurnStoreValue(highChurnStore, streamSelector)

  return useMemo(() => {
    if (!agent) {
      return null
    }

    const streamStatus = runtimeStream?.status ?? 'idle'
    return {
      ...agent,
      runtimeStream,
      runtimeStreamStatus: streamStatus,
      runtimeStreamStatusLabel: getRuntimeStreamStatusLabel(streamStatus),
      runtimeStreamError: runtimeStream?.lastIssue ?? null,
      runtimeStreamItems: runtimeStream?.items ?? [],
      skillItems: runtimeStream?.skillItems ?? [],
      activityItems: runtimeStream?.activityItems ?? [],
      actionRequiredItems: runtimeStream?.actionRequired ?? [],
      messagesUnavailableReason: getAgentMessagesUnavailableCredentialReason(
        agent.runtimeSession ?? null,
        runtimeStream,
        agent.runtimeRun ?? null,
        agent.agentRuntimeBlocked ?? false,
      ),
    }
  }, [agent, runtimeStream])
}

/**
 * Fetches the persisted session transcript for the given pane and projects it
 * into the historical `ConversationTurn[]` that the conversation pane renders
 * ahead of the live runtime stream. Returns `null` while the request is
 * in-flight on first load (so the UI falls back to the live stream alone).
 *
 * Refetches whenever the (project, session, run) triple changes — the
 * runId-flip case is the same-type handoff path where the source run becomes
 * historical and we want it to re-appear in the conversation under a new
 * `handoff_notice` row.
 */
export function useHistoricalConversationTurns(
  agent: AgentPaneView | null,
  desktopAdapter: AgentRuntimeDesktopAdapter | undefined,
): ConversationTurn[] | null {
  const projectId = agent?.project.id ?? null
  const agentSessionId = agent?.project.selectedAgentSessionId ?? null
  const activeRunId = agent?.runtimeRun?.runId ?? null
  const getSessionTranscript = desktopAdapter?.getSessionTranscript
  const [turnsByKey, setTurnsByKey] = useState<{ key: string; turns: ConversationTurn[] } | null>(null)

  // Keying on (project, session, run) covers the same-type handoff case: when
  // the runtime run snapshot is rebound from source -> target run, the runId
  // changes and we refetch so the source run's items show up as history.
  const fetchKey = projectId && agentSessionId
    ? `${projectId}::${agentSessionId}::${activeRunId ?? ''}`
    : null

  useEffect(() => {
    if (!fetchKey || !projectId || !agentSessionId || !getSessionTranscript) {
      return
    }

    let cancelled = false
    void getSessionTranscript({
      projectId,
      agentSessionId,
      runId: null,
    })
      .then((transcript) => {
        if (cancelled) return
        const turns = buildHistoricalConversationTurns(transcript, { activeRunId })
        setTurnsByKey({ key: fetchKey, turns })
      })
      .catch(() => {
        if (cancelled) return
        // On failure, fall back silently to the live stream alone. The pane
        // is still functional; only the historical context is missing.
        setTurnsByKey({ key: fetchKey, turns: [] })
      })

    return () => {
      cancelled = true
    }
  }, [activeRunId, agentSessionId, fetchKey, getSessionTranscript, projectId])

  // While stale-keyed (e.g. user just switched panes), suppress the previous
  // pane's history rather than briefly flashing it under the new pane.
  if (!turnsByKey || turnsByKey.key !== fetchKey) {
    return null
  }
  return turnsByKey.turns
}

interface LiveAgentRuntimeViewProps extends Omit<AgentRuntimeProps, 'agent'> {
  agent: AgentPaneView | null
  highChurnStore: XeroHighChurnStore
}

export const LiveAgentRuntimeView = memo(function LiveAgentRuntimeView({
  agent,
  highChurnStore,
  ...props
}: LiveAgentRuntimeViewProps) {
  const liveAgent = useAgentViewWithLiveRuntimeStream(agent, highChurnStore)
  const historicalConversationTurns = useHistoricalConversationTurns(liveAgent, props.desktopAdapter)
  if (!liveAgent) {
    return null
  }

  return (
    <Suspense fallback={<AgentRuntimeLoadingShell />}>
      <LazyAgentRuntime
        {...props}
        agent={liveAgent}
        historicalConversationTurns={historicalConversationTurns ?? undefined}
      />
    </Suspense>
  )
})
