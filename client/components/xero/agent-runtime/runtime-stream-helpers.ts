import type {
  AgentPaneView,
} from '@/src/features/xero/use-xero-desktop-state'
import type {
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamSkillItemView,
  RuntimeStreamStatus,
  RuntimeStreamToolItemView,
} from '@/src/lib/xero-model'
import {
  getRuntimeRunStatusLabel,
} from '@/src/lib/xero-model'

import { type BadgeVariant, displayValue } from './shared-helpers'

export function getStreamBadgeVariant(status: RuntimeStreamStatus): BadgeVariant {
  switch (status) {
    case 'live':
    case 'complete':
      return 'default'
    case 'subscribing':
    case 'replaying':
    case 'stale':
      return 'secondary'
    case 'error':
      return 'destructive'
    case 'idle':
      return 'outline'
  }
}

export function getRuntimeRunBadgeVariant(runtimeRun: RuntimeRunView | null): BadgeVariant {
  if (!runtimeRun) {
    return 'outline'
  }

  switch (runtimeRun.status) {
    case 'starting':
    case 'stale':
      return 'secondary'
    case 'running':
    case 'stopped':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

export function hasUsableRuntimeRunId(runtimeRun: RuntimeRunView | null): runtimeRun is RuntimeRunView {
  if (!runtimeRun) {
    return false
  }

  const runId = runtimeRun.runId.trim()
  return runId.length > 0 && runId !== 'run-unavailable'
}

export function getRuntimeRunStatusText(runtimeRun: RuntimeRunView | null): string {
  if (!runtimeRun) {
    return 'No run'
  }

  return displayValue(runtimeRun.statusLabel, getRuntimeRunStatusLabel(runtimeRun.status))
}

export function getPrimaryRuntimeRunActionLabel(runtimeRun: RuntimeRunView | null): string {
  if (!runtimeRun) {
    return 'Start agent run'
  }

  if (runtimeRun.isActive || runtimeRun.isStale) {
    return 'Reconnect agent run'
  }

  return 'Start new agent run'
}

export function getToolStateBadgeVariant(toolState: RuntimeStreamToolItemView['toolState']): BadgeVariant {
  switch (toolState) {
    case 'succeeded':
      return 'default'
    case 'running':
      return 'secondary'
    case 'failed':
      return 'destructive'
    case 'pending':
      return 'outline'
  }
}

export function getToolStateLabel(toolState: RuntimeStreamToolItemView['toolState']): string {
  switch (toolState) {
    case 'pending':
      return 'Queued'
    case 'running':
      return 'Running'
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

function getMcpCapabilityKindLabel(kind: 'tool' | 'resource' | 'prompt' | 'command'): string {
  switch (kind) {
    case 'tool':
      return 'Tool'
    case 'resource':
      return 'Resource'
    case 'prompt':
      return 'Prompt'
    case 'command':
      return 'Command'
  }
}

function getBrowserComputerUseSurfaceLabel(surface: 'browser' | 'computer_use'): string {
  switch (surface) {
    case 'browser':
      return 'Browser'
    case 'computer_use':
      return 'Computer use'
  }
}

function getBrowserComputerUseStatusLabel(status: 'pending' | 'running' | 'succeeded' | 'failed' | 'blocked'): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'running':
      return 'Running'
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
    case 'blocked':
      return 'Blocked'
  }
}

export function getToolSummaryContext(item: RuntimeStreamToolItemView): string | null {
  const summary = item.toolSummary
  if (!summary) {
    return null
  }

  switch (summary.kind) {
    case 'mcp_capability': {
      const capabilityLabel = displayValue(summary.capabilityName, summary.capabilityId)
      return `MCP ${getMcpCapabilityKindLabel(summary.capabilityKind)} · ${capabilityLabel} · server ${summary.serverId} · outcome ${getToolStateLabel(item.toolState)}`
    }
    case 'browser_computer_use': {
      const targetLabel = displayValue(summary.target, 'Target unavailable')
      const outcomeLabel = displayValue(summary.outcome, 'Outcome unavailable')
      return `${getBrowserComputerUseSurfaceLabel(summary.surface)} action ${summary.action} · status ${getBrowserComputerUseStatusLabel(summary.status)} · target ${targetLabel} · outcome ${outcomeLabel}`
    }
    default:
      return null
  }
}

export function getSkillStageLabel(stage: RuntimeStreamSkillItemView['stage']): string {
  switch (stage) {
    case 'discovery':
      return 'Discovery'
    case 'install':
      return 'Install'
    case 'invoke':
      return 'Invoke'
  }
}

export function getSkillResultBadgeVariant(result: RuntimeStreamSkillItemView['result']): BadgeVariant {
  switch (result) {
    case 'succeeded':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

export function getSkillResultLabel(result: RuntimeStreamSkillItemView['result']): string {
  switch (result) {
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

export function getSkillCacheLabel(cacheStatus: RuntimeStreamSkillItemView['cacheStatus']): string {
  switch (cacheStatus) {
    case 'miss':
      return 'Cache miss'
    case 'hit':
      return 'Cache hit'
    case 'refreshed':
      return 'Cache refreshed'
    case null:
      return 'Cache unavailable'
  }
}

export function formatSkillSource(item: RuntimeStreamSkillItemView): string {
  return `${item.source.repo} · ${item.source.path} @ ${item.source.reference}`
}

export function formatSkillTreeHash(item: RuntimeStreamSkillItemView): string {
  return item.source.treeHash.slice(0, 12)
}

export function getStreamRunId(
  runtimeStream: AgentPaneView['runtimeStream'] | null,
  runtimeRun: RuntimeRunView | null,
): string {
  const streamRunId = runtimeStream?.runId
  if (typeof streamRunId === 'string' && streamRunId.trim().length > 0) {
    return streamRunId.trim()
  }

  if (hasUsableRuntimeRunId(runtimeRun)) {
    return runtimeRun.runId.trim()
  }

  return 'No active run'
}

export function getStreamStatusMeta(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null) {
  const runtimeStream = agent.runtimeStream ?? null
  const runtimeRun = hasUsableRuntimeRunId(agent.runtimeRun ?? null) ? agent.runtimeRun : null
  const streamStatus = runtimeStream?.status ?? 'idle'

  if (!runtimeSession) {
    return {
      eyebrow: 'Agent feed',
      title: 'Authenticate to view live agent activity',
      body: agent.messagesUnavailableReason,
      badgeVariant: 'outline' as const,
    }
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return {
        eyebrow: 'Agent feed',
        title: 'Live agent activity will appear after sign-in finishes',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'secondary' as const,
      }
    }

    if (runtimeSession.isFailed) {
      return {
        eyebrow: 'Agent feed',
        title: 'Runtime session must recover before the live feed can resume',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'destructive' as const,
      }
    }

    return {
      eyebrow: 'Agent feed',
      title: 'Authenticate to view live agent activity',
      body: agent.messagesUnavailableReason,
      badgeVariant: 'outline' as const,
    }
  }

  if (!runtimeRun && !runtimeStream) {
    return {
      eyebrow: 'Agent feed',
      title: 'No agent run attached yet',
      body: 'Start or reconnect a Xero-owned agent run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project.',
      badgeVariant: 'outline' as const,
    }
  }

  if (runtimeRun?.isTerminal && runtimeRun.isFailed) {
    return {
      eyebrow: 'Agent feed',
      title: 'Latest saved run failed',
      body: agent.messagesUnavailableReason,
      badgeVariant: 'destructive' as const,
    }
  }

  switch (streamStatus) {
    case 'subscribing':
      return {
        eyebrow: 'Agent feed',
        title: 'Connecting the run-scoped live feed',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'secondary' as const,
      }
    case 'replaying':
      return {
        eyebrow: 'Agent feed',
        title: 'Replaying recent run-scoped activity',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'secondary' as const,
      }
    case 'live':
      return {
        eyebrow: 'Agent feed',
        title: 'Streaming run-scoped live activity',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'default' as const,
      }
    case 'complete':
      return {
        eyebrow: 'Agent feed',
        title: 'Run-scoped stream completed',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'default' as const,
      }
    case 'stale':
      return {
        eyebrow: 'Agent feed',
        title: 'Run-scoped live feed needs retry',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'secondary' as const,
      }
    case 'error':
      return {
        eyebrow: 'Agent feed',
        title: 'Run-scoped live feed failed',
        body: agent.messagesUnavailableReason,
        badgeVariant: 'destructive' as const,
      }
    case 'idle':
      return {
        eyebrow: 'Agent feed',
        title: runtimeRun ? 'Waiting for the first run-scoped event' : 'No agent run attached yet',
        body: runtimeRun
          ? agent.messagesUnavailableReason
          : 'Start or reconnect a Xero-owned agent run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project.',
        badgeVariant: 'outline' as const,
      }
  }
}
