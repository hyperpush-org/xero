import type {
  AgentPaneView,
  AgentTrustSnapshotView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  OperatorApprovalView,
  ResumeHistoryEntryView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamSkillItemView,
  RuntimeStreamStatus,
  RuntimeStreamToolItemView,
} from '@/src/lib/cadence-model'
import {
  getRuntimeRunStatusLabel,
  getRuntimeStreamStatusLabel,
} from '@/src/lib/cadence-model'

export type BadgeVariant = 'default' | 'secondary' | 'outline' | 'destructive'

type CheckpointControlLoopCard = NonNullable<AgentPaneView['checkpointControlLoop']>['items'][number]
type OperatorIntentKind = 'approve' | 'reject' | 'resume'

type PerActionResumeState = 'waiting' | 'running' | 'started' | 'failed'

export interface PerActionResumeStateMeta {
  state: PerActionResumeState
  label: string
  detail: string
  badgeVariant: BadgeVariant
  timestamp: string | null
}

interface ComposerModelOption {
  value: string
  label: string
}

interface ComposerModelGroup {
  id: string
  label: string
  items: ComposerModelOption[]
}

const SAMPLE_COMPOSER_MODEL_GROUPS: ComposerModelGroup[] = [
  {
    id: 'openai_codex',
    label: 'OpenAI Codex',
    items: [
      { value: 'openai_codex', label: 'openai_codex' },
      { value: 'codex-mini-latest', label: 'codex-mini-latest' },
    ],
  },
  {
    id: 'openai',
    label: 'OpenAI',
    items: [
      { value: 'gpt-4.1', label: 'gpt-4.1' },
      { value: 'gpt-4.1-mini', label: 'gpt-4.1-mini' },
      { value: 'o4-mini', label: 'o4-mini' },
      { value: 'o3', label: 'o3' },
      { value: 'o3-mini', label: 'o3-mini' },
    ],
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    items: [
      { value: 'claude-3-7-sonnet-latest', label: 'claude-3-7-sonnet-latest' },
      { value: 'claude-3-5-haiku-latest', label: 'claude-3-5-haiku-latest' },
    ],
  },
  {
    id: 'google',
    label: 'Google',
    items: [
      { value: 'gemini-2.5-pro', label: 'gemini-2.5-pro' },
      { value: 'gemini-2.5-flash', label: 'gemini-2.5-flash' },
    ],
  },
  {
    id: 'deepseek',
    label: 'DeepSeek',
    items: [
      { value: 'deepseek/deepseek-chat-v3-0324', label: 'deepseek-chat-v3-0324' },
      { value: 'deepseek/deepseek-r1-0528', label: 'deepseek-r1-0528' },
    ],
  },
  {
    id: 'meta_llama',
    label: 'Meta Llama',
    items: [
      { value: 'meta-llama/llama-4-maverick', label: 'llama-4-maverick' },
      { value: 'meta-llama/llama-4-scout', label: 'llama-4-scout' },
    ],
  },
  {
    id: 'mistral',
    label: 'Mistral',
    items: [
      { value: 'mistral/magistral-medium-2506', label: 'magistral-medium-2506' },
      { value: 'mistral/devstral-medium', label: 'devstral-medium' },
    ],
  },
  {
    id: 'moonshot',
    label: 'Moonshot',
    items: [{ value: 'moonshotai/kimi-k2', label: 'kimi-k2' }],
  },
  {
    id: 'x_ai',
    label: 'xAI',
    items: [
      { value: 'x-ai/grok-3-beta', label: 'grok-3-beta' },
      { value: 'x-ai/grok-3-mini-beta', label: 'grok-3-mini-beta' },
    ],
  },
]

function getTimestampValue(timestamp: string | null | undefined): number {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 0
  }

  const parsed = new Date(timestamp)
  return Number.isNaN(parsed.getTime()) ? 0 : parsed.getTime()
}

export function displayValue(value: string | null | undefined, fallback: string): string {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : fallback
}

export function sortByNewest<T>(values: T[], getTimestamp: (value: T) => string | null | undefined): T[] {
  return [...values].sort((left, right) => getTimestampValue(getTimestamp(right)) - getTimestampValue(getTimestamp(left)))
}

export function formatTimestamp(timestamp: string | null | undefined): string {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 'Unknown'
  }

  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return timestamp
  }

  return parsed.toLocaleString()
}

export function getComposerModelGroups(
  selectedProviderId: string,
  selectedProviderLabel: string,
  currentModelId: string,
): ComposerModelGroup[] {
  const groups = SAMPLE_COMPOSER_MODEL_GROUPS.map((group) => ({
    ...group,
    items: [...group.items],
  }))

  const currentExists = groups.some((group) => group.items.some((item) => item.value === currentModelId))
  if (currentExists) {
    return groups
  }

  const fallbackLabel = selectedProviderLabel.trim().length > 0 ? selectedProviderLabel : 'Selected provider'
  const fallbackGroupIndex = groups.findIndex((group) => group.id === selectedProviderId)
  const fallbackItem = { value: currentModelId, label: currentModelId }

  if (fallbackGroupIndex >= 0) {
    groups[fallbackGroupIndex] = {
      ...groups[fallbackGroupIndex],
      items: [fallbackItem, ...groups[fallbackGroupIndex].items],
    }
    return groups
  }

  return [{ id: selectedProviderId, label: fallbackLabel, items: [fallbackItem] }, ...groups]
}

export function getSelectedProviderId(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
}

export function getSelectedProviderLabel(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderLabel ?? (getSelectedProviderId(agent, runtimeSession) === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex')
}

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

export function createEmptyCheckpointControlLoop(): NonNullable<AgentPaneView['checkpointControlLoop']> {
  return {
    items: [],
    totalCount: 0,
    visibleCount: 0,
    hiddenCount: 0,
    isTruncated: false,
    windowLabel: 'No checkpoint actions are visible in the bounded control-loop window.',
    emptyTitle: 'No checkpoint control loops recorded',
    emptyBody:
      'Cadence has not observed a live or durable checkpoint boundary for this project yet. Waiting boundaries, resume outcomes, and broker fan-out will appear here once recorded.',
    missingEvidenceCount: 0,
    liveHintOnlyCount: 0,
    durableOnlyCount: 0,
    recoveredCount: 0,
  }
}

export function getCheckpointControlLoopTruthBadgeVariant(
  truthSource: CheckpointControlLoopCard['truthSource'],
): BadgeVariant {
  switch (truthSource) {
    case 'live_and_durable':
      return 'default'
    case 'live_hint_only':
      return 'secondary'
    case 'durable_only':
    case 'recovered_durable':
      return 'outline'
  }
}

export function getApprovalBadgeVariant(status: OperatorApprovalView['status']): BadgeVariant {
  switch (status) {
    case 'pending':
      return 'secondary'
    case 'approved':
      return 'default'
    case 'rejected':
      return 'destructive'
  }
}

export function getCheckpointControlLoopDurableBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.approval) {
    return getApprovalBadgeVariant(card.approval.status)
  }

  if (card.liveActionRequired) {
    return 'secondary'
  }

  return 'outline'
}

export function getCheckpointControlLoopBrokerBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.brokerAction?.hasFailures) {
    return 'destructive'
  }

  if (card.brokerAction?.hasPending) {
    return 'secondary'
  }

  return card.brokerAction ? 'default' : 'outline'
}

export function getCheckpointControlLoopEvidenceBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  return card.evidenceCount > 0 ? 'outline' : 'secondary'
}

export function getCheckpointControlLoopRecoveryAlertMeta(options: {
  controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>
  trustSnapshot: Pick<AgentTrustSnapshotView, 'syncState' | 'syncReason'>
  autonomousRunErrorMessage: string | null | undefined
  notificationSyncPollingActive: boolean
  notificationSyncPollingActionId: string | null
  notificationSyncPollingBoundaryId: string | null
}) {
  if (options.controlLoop.items.length === 0) {
    return null
  }

  if (options.notificationSyncPollingActive && options.trustSnapshot.syncState === 'degraded') {
    return {
      title: 'Showing last truthful checkpoint loop',
      body: `Cadence is still polling remote routes for blocked boundary ${displayValue(options.notificationSyncPollingBoundaryId, 'unknown')} and action ${displayValue(options.notificationSyncPollingActionId, 'unknown')} while preserving the last truthful sync summary. ${options.trustSnapshot.syncReason}`,
      variant: 'destructive' as const,
    }
  }

  if (options.trustSnapshot.syncState === 'degraded') {
    return {
      title: 'Showing last truthful checkpoint loop',
      body: options.trustSnapshot.syncReason,
      variant: 'destructive' as const,
    }
  }

  if (options.autonomousRunErrorMessage) {
    return {
      title: 'Recovered checkpoint state remains visible',
      body: options.autonomousRunErrorMessage,
      variant: 'default' as const,
    }
  }

  if (options.notificationSyncPollingActive) {
    return {
      title: 'Remote escalation is actively polling this checkpoint',
      body: `Cadence is polling remote routes for blocked boundary ${displayValue(options.notificationSyncPollingBoundaryId, 'unknown')} and action ${displayValue(options.notificationSyncPollingActionId, 'unknown')} while durable approval, broker, and resume truth remain visible here.`,
      variant: 'default' as const,
    }
  }

  return null
}

export function getCheckpointControlLoopCoverageAlertMeta(
  controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>,
) {
  if (controlLoop.items.length === 0) {
    return null
  }

  const coverageNotes: string[] = []
  if (controlLoop.isTruncated) {
    coverageNotes.push(`${controlLoop.hiddenCount} older checkpoint action${controlLoop.hiddenCount === 1 ? '' : 's'} are outside this bounded window.`)
  }
  if (controlLoop.liveHintOnlyCount > 0) {
    coverageNotes.push(
      controlLoop.liveHintOnlyCount === 1
        ? '1 card is still anchored to live hints while durable rows persist.'
        : `${controlLoop.liveHintOnlyCount} cards are still anchored to live hints while durable rows persist.`,
    )
  }
  if (controlLoop.missingEvidenceCount > 0) {
    coverageNotes.push(
      controlLoop.missingEvidenceCount === 1
        ? '1 card still lacks durable evidence inside the bounded artifact window.'
        : `${controlLoop.missingEvidenceCount} cards still lack durable evidence inside the bounded artifact window.`,
    )
  }
  if (controlLoop.recoveredCount > 0) {
    coverageNotes.push(
      controlLoop.recoveredCount === 1
        ? '1 card is being shown from recovered durable history after the live row cleared.'
        : `${controlLoop.recoveredCount} cards are being shown from recovered durable history after the live row cleared.`,
    )
  }

  if (coverageNotes.length === 0) {
    return null
  }

  return {
    title: 'Bounded checkpoint coverage',
    body: coverageNotes.join(' '),
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
    return 'Start supervised run'
  }

  if (runtimeRun.isActive || runtimeRun.isStale) {
    return 'Reconnect supervisor'
  }

  return 'Start new supervised run'
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

function getResumeBadgeVariant(status: ResumeHistoryEntryView['status']): BadgeVariant {
  switch (status) {
    case 'started':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

export function getPerActionResumeStateMeta(options: {
  card: CheckpointControlLoopCard
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: { actionId: string; kind: OperatorIntentKind } | null
}): PerActionResumeStateMeta {
  const { card, operatorActionStatus, pendingOperatorActionId, pendingOperatorIntent } = options
  const approval = card.approval
  const latestResumeForAction = card.latestResume
  const isActionInFlight =
    (operatorActionStatus === 'running' && pendingOperatorActionId === card.actionId) ||
    pendingOperatorIntent?.actionId === card.actionId

  if (isActionInFlight) {
    return {
      state: 'running',
      label: 'Running',
      detail:
        pendingOperatorIntent?.kind === 'resume'
          ? 'Resume request is in flight for this action. Cadence will refresh durable state before updating this card.'
          : 'Decision persistence is in flight for this action. Cadence keeps the last durable resume state visible until refresh completes.',
      badgeVariant: 'secondary',
      timestamp: approval?.updatedAt ?? card.resumeUpdatedAt,
    }
  }

  if (latestResumeForAction?.status === 'failed') {
    return {
      state: 'failed',
      label: 'Failed',
      detail: `Latest resume failed: ${displayValue(latestResumeForAction.summary, 'Resume failed for this action.')}`,
      badgeVariant: 'destructive',
      timestamp: latestResumeForAction.createdAt,
    }
  }

  if (latestResumeForAction?.status === 'started') {
    return {
      state: 'started',
      label: 'Started',
      detail: `Latest resume started: ${displayValue(latestResumeForAction.summary, 'Resume started for this action.')}`,
      badgeVariant: getResumeBadgeVariant(latestResumeForAction.status),
      timestamp: latestResumeForAction.createdAt,
    }
  }

  if (approval?.isPending) {
    return {
      state: 'waiting',
      label: 'Waiting',
      detail: 'Waiting for operator input before this action can resume the run.',
      badgeVariant: 'outline',
      timestamp: approval.updatedAt,
    }
  }

  if (approval?.canResume) {
    return {
      state: 'waiting',
      label: 'Waiting',
      detail: 'No resume recorded yet for this action.',
      badgeVariant: 'outline',
      timestamp: approval.updatedAt,
    }
  }

  return {
    state: 'waiting',
    label: card.resumeStateLabel,
    detail: card.resumeDetail,
    badgeVariant: 'outline',
    timestamp: card.resumeUpdatedAt,
  }
}

export function formatSequence(sequence: number | null | undefined): string {
  return typeof sequence === 'number' && Number.isFinite(sequence) && sequence > 0 ? `#${sequence}` : 'Not observed'
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

export function getComposerPlaceholder(
  runtimeSession: RuntimeSessionView | null,
  streamStatus: RuntimeStreamStatus,
  runtimeRun: RuntimeRunView | null,
  streamRunId: string | undefined,
  options: { selectedProviderId: string; openrouterApiKeyConfigured: boolean; providerMismatch: boolean },
): string {
  if (!runtimeSession) {
    if (options.selectedProviderId === 'openrouter') {
      return options.openrouterApiKeyConfigured
        ? 'Bind OpenRouter from the Agent tab to start.'
        : 'Configure an OpenRouter API key in Settings to start.'
    }

    return 'Connect a provider to start.'
  }

  if (options.providerMismatch) {
    return `Rebind ${options.selectedProviderId === 'openrouter' ? 'OpenRouter' : 'the selected provider'} before trusting new live activity.`
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return options.selectedProviderId === 'openrouter'
        ? 'Finish the OpenRouter bind to continue.'
        : 'Finish the login flow to continue.'
    }

    return options.selectedProviderId === 'openrouter'
      ? options.openrouterApiKeyConfigured
        ? 'Bind OpenRouter from the Agent tab to start.'
        : 'Configure an OpenRouter API key in Settings to start.'
      : 'Connect a provider to start.'
  }

  if (!hasUsableRuntimeRunId(runtimeRun)) {
    return 'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.'
  }

  switch (streamStatus) {
    case 'live':
      return 'Live activity streaming. Composer is read-only.'
    case 'complete':
      return 'Run completed.'
    case 'stale':
      return 'Stream went stale — retry to refresh.'
    case 'error':
      return 'Stream failed — retry to restore.'
    case 'subscribing':
      return 'Connecting to the live transcript.'
    case 'replaying':
      return `Cadence is replaying recent run-scoped activity for ${displayValue(streamRunId, runtimeRun.runId)} while the live feed catches up.`
    case 'idle':
      return 'Waiting for first event…'
  }
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
      title: 'No supervised run attached yet',
      body: 'Start or reconnect a supervised run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project.',
      badgeVariant: 'outline' as const,
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
        title: runtimeRun ? 'Waiting for the first run-scoped event' : 'No supervised run attached yet',
        body: runtimeRun
          ? agent.messagesUnavailableReason
          : 'Start or reconnect a supervised run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project.',
        badgeVariant: 'outline' as const,
      }
  }
}
