"use client"

import { lazy, memo, Suspense, useMemo } from 'react'

import type { AgentRuntimeProps } from '@/components/xero/agent-runtime'
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
  if (!liveAgent) {
    return null
  }

  return (
    <Suspense fallback={<AgentRuntimeLoadingShell />}>
      <LazyAgentRuntime {...props} agent={liveAgent} />
    </Suspense>
  )
})
