"use client"

import { lazy, memo, Suspense, useEffect, useMemo, useRef, useState } from 'react'

import type { AgentRuntimeDesktopAdapter, AgentRuntimeProps } from '@/components/xero/agent-runtime'
import type { ConversationTurn } from '@xero/ui/components/transcript/conversation-section'
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
 * ahead of the live runtime stream.
 *
 * Refetches whenever the (project, session, run) triple changes — the
 * runId-flip case is the same-type handoff path where the source run becomes
 * historical and we want it to re-appear in the conversation under a new
 * `handoff_notice` row.
 */
export function useHistoricalConversationTurnsState(
  agent: AgentPaneView | null,
  desktopAdapter: AgentRuntimeDesktopAdapter | undefined,
): { loading: boolean; turns: ConversationTurn[] | null } {
  const projectId = agent?.project.id ?? null
  const agentSessionId = agent?.project.selectedAgentSessionId ?? null
  const sessionRevision = agent?.project.selectedAgentSession?.updatedAt ?? null
  const runtimeRun = agent?.runtimeRun ?? null
  const runtimeRunIsTerminal = Boolean(runtimeRun?.isTerminal)
  const activeRunId = runtimeRun && !runtimeRun.isTerminal ? runtimeRun.runId : null
  const getSessionTranscript = desktopAdapter?.getSessionTranscript
  const [turnsByKey, setTurnsByKey] = useState<{
    sessionKey: string
    fetchKey: string
    turns: ConversationTurn[]
  } | null>(null)
  const streamStatus = agent?.runtimeStreamStatus ?? 'idle'
  const shouldDeferTranscriptFetch = Boolean(
    agent?.runtimeRunActionStatus === 'running' ||
      agent?.selectedPrompt?.hasQueuedPrompt ||
      (!runtimeRunIsTerminal &&
        (streamStatus === 'subscribing' ||
          streamStatus === 'replaying' ||
          streamStatus === 'live')),
  )

  // Keying on (project, session, run) covers the same-type handoff case: when
  // the runtime run snapshot is rebound from source -> target run, the runId
  // changes and we refetch so the source run's items show up as history.
  const sessionKey = projectId && agentSessionId
    ? `${projectId}::${agentSessionId}`
    : null
  const fetchKey = sessionKey
    ? `${sessionKey}::${activeRunId ?? ''}::${sessionRevision ?? ''}`
    : null
  const canFetchTranscript = Boolean(
    sessionKey &&
      fetchKey &&
      projectId &&
      agentSessionId &&
      getSessionTranscript &&
      !shouldDeferTranscriptFetch,
  )

  useEffect(() => {
    if (!canFetchTranscript || !sessionKey || !fetchKey || !projectId || !agentSessionId || !getSessionTranscript) {
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
        setTurnsByKey({ sessionKey, fetchKey, turns })
      })
      .catch(() => {
        if (cancelled) return
        // On failure, fall back silently to the live stream alone. The pane
        // is still functional; only the historical context is missing.
        setTurnsByKey({ sessionKey, fetchKey, turns: [] })
      })

    return () => {
      cancelled = true
    }
  }, [
    activeRunId,
    agentSessionId,
    canFetchTranscript,
    fetchKey,
    getSessionTranscript,
    projectId,
    sessionRevision,
    sessionKey,
    shouldDeferTranscriptFetch,
  ])

  // While stale-keyed (e.g. user just switched panes or the active run id just
  // arrived during project load), suppress the previous history. Keeping a
  // transcript fetched with no active run would let the current run appear once
  // as static history and again from the live/restored stream.
  if (
    !sessionKey ||
    !fetchKey ||
    !turnsByKey ||
    turnsByKey.sessionKey !== sessionKey ||
    turnsByKey.fetchKey !== fetchKey
  ) {
    return { loading: canFetchTranscript, turns: null }
  }
  return { loading: false, turns: turnsByKey.turns }
}

export function useHistoricalConversationTurns(
  agent: AgentPaneView | null,
  desktopAdapter: AgentRuntimeDesktopAdapter | undefined,
): ConversationTurn[] | null {
  return useHistoricalConversationTurnsState(agent, desktopAdapter).turns
}

interface LiveAgentRuntimeViewProps extends Omit<AgentRuntimeProps, 'agent'> {
  agent: AgentPaneView | null
  highChurnStore: XeroHighChurnStore
}

interface StableAgentRuntimeSnapshot {
  identity: string
  agent: AgentPaneView
  historicalTurns: ConversationTurn[] | null
}

function getAgentRuntimeIdentity(agent: AgentPaneView | null): string | null {
  if (!agent) return null
  return `${agent.project.id}:${agent.project.selectedAgentSessionId ?? 'none'}`
}

function isAgentRuntimeProjectShell(agent: AgentPaneView | null): boolean {
  return Boolean(
    agent &&
      !agent.repositoryPath &&
      !agent.project.repository &&
      !agent.project.selectedAgentSessionId,
  )
}

export const LiveAgentRuntimeView = memo(function LiveAgentRuntimeView({
  agent,
  highChurnStore,
  ...props
}: LiveAgentRuntimeViewProps) {
  const liveAgent = useAgentViewWithLiveRuntimeStream(agent, highChurnStore)
  const historicalConversationState = useHistoricalConversationTurnsState(liveAgent, props.desktopAdapter)
  const liveAgentIdentity = getAgentRuntimeIdentity(liveAgent)
  const incomingLooksLikeProjectShell = isAgentRuntimeProjectShell(liveAgent)
  const lastReadySnapshotRef = useRef<StableAgentRuntimeSnapshot | null>(null)

  useEffect(() => {
    if (
      !liveAgent ||
      !liveAgentIdentity ||
      historicalConversationState.loading ||
      incomingLooksLikeProjectShell
    ) {
      return
    }

    lastReadySnapshotRef.current = {
      identity: liveAgentIdentity,
      agent: liveAgent,
      historicalTurns: historicalConversationState.turns,
    }
  }, [
    historicalConversationState.loading,
    historicalConversationState.turns,
    incomingLooksLikeProjectShell,
    liveAgent,
    liveAgentIdentity,
  ])

  const lastReadySnapshot = lastReadySnapshotRef.current
  const shouldHoldPreviousRuntime =
    Boolean(
      liveAgentIdentity &&
        lastReadySnapshot &&
        lastReadySnapshot.identity !== liveAgentIdentity,
    ) && (
      historicalConversationState.loading ||
      incomingLooksLikeProjectShell
    )
  const renderedAgent = shouldHoldPreviousRuntime
    ? lastReadySnapshot?.agent ?? liveAgent
    : liveAgent
  const renderedHistoricalTurns = shouldHoldPreviousRuntime
    ? lastReadySnapshot?.historicalTurns ?? null
    : historicalConversationState.turns
  const renderedHistoricalLoading = shouldHoldPreviousRuntime
    ? false
    : historicalConversationState.loading

  if (!renderedAgent) {
    return null
  }

  return (
    <Suspense fallback={<AgentRuntimeLoadingShell />}>
      <LazyAgentRuntime
        {...props}
        agent={renderedAgent}
        historicalConversationTurns={renderedHistoricalTurns ?? undefined}
        historicalConversationTurnsLoading={renderedHistoricalLoading}
      />
    </Suspense>
  )
})
