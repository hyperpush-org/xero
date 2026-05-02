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

function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`
}

function getGitScopeLabel(scope: 'staged' | 'unstaged' | 'worktree'): string {
  switch (scope) {
    case 'staged':
      return 'staged'
    case 'unstaged':
      return 'unstaged'
    case 'worktree':
      return 'worktree'
  }
}

function getWebContentKindLabel(kind: 'html' | 'plain_text'): string {
  switch (kind) {
    case 'html':
      return 'HTML'
    case 'plain_text':
      return 'plain text'
  }
}

function getToolDetailParts(parts: Array<string | null | undefined>): string {
  return parts
    .map((part) => part?.trim())
    .filter((part): part is string => Boolean(part))
    .join(' · ')
}

export function getToolSummaryContext(item: RuntimeStreamToolItemView): string | null {
  const summary = item.toolSummary
  if (!summary) {
    return null
  }

  switch (summary.kind) {
    case 'command': {
      const outcome = summary.timedOut
        ? 'timed out'
        : summary.exitCode != null
          ? `exit ${summary.exitCode}`
          : `status ${getToolStateLabel(item.toolState).toLowerCase()}`
      return getToolDetailParts([
        'Command',
        outcome,
        summary.stdoutTruncated ? 'stdout truncated' : null,
        summary.stderrTruncated ? 'stderr truncated' : null,
        summary.stdoutRedacted ? 'stdout redacted' : null,
        summary.stderrRedacted ? 'stderr redacted' : null,
      ])
    }
    case 'file': {
      return getToolDetailParts([
        'File result',
        summary.path ? `path ${summary.path}` : null,
        !summary.path && summary.scope ? `scope ${summary.scope}` : null,
        summary.lineCount != null ? pluralize(summary.lineCount, 'line') : null,
        summary.matchCount != null ? pluralize(summary.matchCount, 'match', 'matches') : null,
        summary.truncated ? 'truncated' : null,
      ])
    }
    case 'git': {
      return getToolDetailParts([
        'Git',
        summary.scope ? getGitScopeLabel(summary.scope) : null,
        pluralize(summary.changedFiles, 'changed file'),
        summary.baseRevision ? `base ${summary.baseRevision}` : null,
        summary.truncated ? 'truncated' : null,
      ])
    }
    case 'web': {
      return getToolDetailParts([
        'Web',
        summary.target,
        summary.resultCount != null ? pluralize(summary.resultCount, 'result') : null,
        summary.finalUrl && summary.finalUrl !== summary.target ? `final ${summary.finalUrl}` : null,
        summary.contentKind ? getWebContentKindLabel(summary.contentKind) : null,
        summary.contentType ?? null,
        summary.truncated ? 'truncated' : null,
      ])
    }
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

function humanizeToolName(toolName: string): string {
  return toolName
    .trim()
    .replace(/[._-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .toLowerCase()
}

function basename(path: string): string {
  const trimmed = path.trim().replace(/\/+$/, '')
  const lastSlash = trimmed.lastIndexOf('/')
  return lastSlash >= 0 ? trimmed.slice(lastSlash + 1) : trimmed
}

function compactFileTarget(path: string): string {
  const name = basename(path)
  return name || path
}

function parseToolDetail(detail: string | null): Map<string, string> {
  const values = new Map<string, string>()
  if (!detail) {
    return values
  }

  const pattern = /(?:^|,\s*)([A-Za-z][A-Za-z0-9]*):\s*([^,]+)/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(detail)) !== null) {
    const key = match[1]?.trim()
    const value = match[2]?.trim()
    if (key && value && !values.has(key)) {
      values.set(key, value)
    }
  }

  return values
}

function firstDetailValue(detailValues: Map<string, string>, keys: string[]): string | null {
  for (const key of keys) {
    const value = detailValues.get(key)
    if (value) {
      return value
    }
  }

  return null
}

function getToolActionLabel(item: RuntimeStreamToolItemView): string {
  const summary = item.toolSummary
  if (summary?.kind === 'browser_computer_use') {
    return summary.action.toLowerCase()
  }
  if (summary?.kind === 'mcp_capability') {
    return `MCP ${getMcpCapabilityKindLabel(summary.capabilityKind).toLowerCase()}`
  }

  switch (item.toolName) {
    case 'read':
      return 'read'
    case 'search':
      return 'search'
    case 'find':
      return 'find'
    case 'list':
      return 'list'
    case 'edit':
      return 'edit'
    case 'write':
      return 'write'
    case 'delete':
      return 'delete'
    case 'mkdir':
      return 'create directory'
    case 'hash':
      return 'hash'
    case 'patch':
      return 'patch'
    case 'rename':
      return 'rename'
    case 'command':
      return 'run'
    case 'command_session_start':
      return 'start command'
    case 'command_session_read':
      return 'read command session'
    case 'command_session_stop':
      return 'stop command session'
    case 'git_status':
      return 'check git status'
    case 'git_diff':
      return 'inspect git diff'
    case 'web_search':
    case 'web_search_only':
      return 'search web'
    case 'web_fetch':
      return 'fetch web page'
    default:
      return humanizeToolName(item.toolName) || 'tool'
  }
}

function getToolTargetLabel(item: RuntimeStreamToolItemView): string | null {
  const detailValues = parseToolDetail(item.detail)
  const summary = item.toolSummary

  switch (item.toolName) {
    case 'read':
    case 'edit':
    case 'write':
    case 'delete':
    case 'hash':
    case 'patch': {
      const path = summary?.kind === 'file' && summary.path
        ? summary.path
        : firstDetailValue(detailValues, ['path', 'fromPath', 'toPath'])
      return path ? compactFileTarget(path) : null
    }
    case 'rename': {
      const fromPath = firstDetailValue(detailValues, ['fromPath'])
      const toPath = firstDetailValue(detailValues, ['toPath'])
      if (fromPath && toPath) {
        return `${compactFileTarget(fromPath)} -> ${compactFileTarget(toPath)}`
      }
      return fromPath ? compactFileTarget(fromPath) : toPath ? compactFileTarget(toPath) : null
    }
    case 'list': {
      return summary?.kind === 'file' && summary.path
        ? summary.path
        : firstDetailValue(detailValues, ['path', 'scope'])
    }
    case 'search': {
      return firstDetailValue(detailValues, ['query', 'pattern', 'path'])
        ?? (summary?.kind === 'file' ? summary.scope ?? summary.path ?? null : null)
    }
    case 'find': {
      return firstDetailValue(detailValues, ['pattern', 'query', 'path'])
        ?? (summary?.kind === 'file' ? summary.scope ?? summary.path ?? null : null)
    }
    case 'command':
    case 'command_session_start':
      return firstDetailValue(detailValues, ['cmd', 'cwd'])
    case 'command_session_read':
    case 'command_session_stop':
      return firstDetailValue(detailValues, ['sessionId'])
    case 'git_diff':
      return summary?.kind === 'git' && summary.scope
        ? getGitScopeLabel(summary.scope)
        : firstDetailValue(detailValues, ['scope'])
    case 'web_search':
    case 'web_search_only':
      return summary?.kind === 'web'
        ? summary.target
        : firstDetailValue(detailValues, ['query', 'url'])
    case 'web_fetch':
      return summary?.kind === 'web'
        ? summary.finalUrl ?? summary.target
        : firstDetailValue(detailValues, ['url'])
    default:
      if (summary?.kind === 'file') {
        return summary.path ? compactFileTarget(summary.path) : summary.scope ?? null
      }
      if (summary?.kind === 'git') {
        return summary.scope ? getGitScopeLabel(summary.scope) : null
      }
      if (summary?.kind === 'web') {
        return summary.finalUrl ?? summary.target
      }
      if (summary?.kind === 'browser_computer_use') {
        return summary.target ?? null
      }
      if (summary?.kind === 'mcp_capability') {
        return displayValue(summary.capabilityName, summary.capabilityId)
      }
      return firstDetailValue(detailValues, ['path', 'pattern', 'query', 'url', 'cmd', 'scope', 'name', 'uri'])
  }
}

export function getToolCardTitle(item: RuntimeStreamToolItemView): string {
  const action = getToolActionLabel(item)
  const target = getToolTargetLabel(item)
  return target ? `${action} ${target}` : action
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
