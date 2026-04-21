"use client"

import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  AgentPaneView,
  AgentTrustSnapshotView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  OperatorApprovalView,
  ResumeHistoryEntryView,
  RuntimeRunCheckpointView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamIssueView,
  RuntimeStreamSkillItemView,
  RuntimeStreamStatus,
  RuntimeStreamToolItemView,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/cadence-model'
import {
  getRuntimeRunStatusLabel,
  getRuntimeStreamStatusLabel,
} from '@/src/lib/cadence-model'
import {
  AlertCircle,
  Bot,
  LoaderCircle,
  Play,
  Send,
  ShieldCheck,
  XCircle,
} from 'lucide-react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { CenteredEmptyState } from '@/components/cadence/centered-empty-state'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { AgentFeedSection } from './agent-runtime/agent-feed-section'
import { ComposerDock } from './agent-runtime/composer-dock'
import * as runtimeHelpers from './agent-runtime/helpers'
import { RecoveredRuntimeSection } from './agent-runtime/recovered-runtime-section'
import { SetupEmptyState } from './agent-runtime/setup-empty-state'

interface AgentRuntimeProps {
  agent: AgentPaneView
  onOpenSettings?: () => void
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onStartAutonomousRun?: () => Promise<unknown>
  onInspectAutonomousRun?: () => Promise<unknown>
  onCancelAutonomousRun?: (runId: string) => Promise<unknown>
  onStartRuntimeRun?: () => Promise<RuntimeRunView | null>
  onStartRuntimeSession?: () => Promise<RuntimeSessionView | null>
  onStopRuntimeRun?: (runId: string) => Promise<RuntimeRunView | null>
  onSubmitManualCallback?: (flowId: string, manualInput: string) => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onRetryStream?: () => Promise<void>
  onResolveOperatorAction?: (
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ) => Promise<unknown>
  onResumeOperatorRun?: (actionId: string, options?: { userAnswer?: string | null }) => Promise<unknown>
  onRefreshNotificationRoutes?: (options?: { force?: boolean }) => Promise<unknown>
  onUpsertNotificationRoute?: (
    request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>,
  ) => Promise<unknown>
}

type PendingAction =
  | 'login'
  | 'browser'
  | 'reuse'
  | 'manual'
  | 'logout'
  | 'retry_stream'
  | 'refresh_routes'
  | 'save_route'
  | 'toggle_route'
  | null

type BadgeVariant = 'default' | 'secondary' | 'outline' | 'destructive'

type RecentAutonomousUnitCard = NonNullable<AgentPaneView['recentAutonomousUnits']>['items'][number]
type CheckpointControlLoopCard = NonNullable<AgentPaneView['checkpointControlLoop']>['items'][number]

type OperatorIntentKind = 'approve' | 'reject' | 'resume'

type PerActionResumeState = 'waiting' | 'running' | 'started' | 'failed'

interface PerActionResumeStateMeta {
  state: PerActionResumeState
  label: string
  detail: string
  badgeVariant: BadgeVariant
  timestamp: string | null
}

function displayValue(value: string | null | undefined, fallback: string): string {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : fallback
}

function getTimestampValue(timestamp: string | null | undefined): number {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 0
  }

  const parsed = new Date(timestamp)
  return Number.isNaN(parsed.getTime()) ? 0 : parsed.getTime()
}

function sortByNewest<T>(values: T[], getTimestamp: (value: T) => string | null | undefined): T[] {
  return [...values].sort((left, right) => getTimestampValue(getTimestamp(right)) - getTimestampValue(getTimestamp(left)))
}

function formatTimestamp(timestamp: string | null | undefined): string {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 'Unknown'
  }

  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return timestamp
  }

  return parsed.toLocaleString()
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }

  if (typeof error === 'string' && error.trim().length > 0) {
    return error
  }

  return fallback
}

function normalizeAnswerInput(value: string): string {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : ''
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

interface ComposerThinkingLevelOption {
  value: string
  label: string
}

const composerInlineSelectTriggerClassName =
  'h-8 max-w-full gap-1.5 border-0 bg-transparent px-1 text-[13px] font-normal text-muted-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0 dark:bg-transparent dark:hover:bg-transparent [&_svg]:text-muted-foreground/80'

const composerInlineSelectContentClassName =
  'max-h-72 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90'

const SAMPLE_COMPOSER_THINKING_LEVELS: ComposerThinkingLevelOption[] = [
  { value: 'low', label: 'Thinking · low' },
  { value: 'medium', label: 'Thinking · medium' },
  { value: 'high', label: 'Thinking · high' },
]

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
      { value: 'anthropic/claude-3.7-sonnet', label: 'claude-3.7-sonnet' },
      { value: 'anthropic/claude-3.5-sonnet', label: 'claude-3.5-sonnet' },
      { value: 'anthropic/claude-3.5-haiku', label: 'claude-3.5-haiku' },
    ],
  },
  {
    id: 'google',
    label: 'Google',
    items: [
      { value: 'google/gemini-2.5-pro-preview', label: 'gemini-2.5-pro-preview' },
      { value: 'google/gemini-2.5-flash-preview', label: 'gemini-2.5-flash-preview' },
      { value: 'google/gemini-2.0-flash', label: 'gemini-2.0-flash' },
    ],
  },
  {
    id: 'meta',
    label: 'Meta',
    items: [
      { value: 'meta-llama/llama-4-maverick', label: 'llama-4-maverick' },
      { value: 'meta-llama/llama-4-scout', label: 'llama-4-scout' },
      { value: 'meta-llama/llama-3.3-70b-instruct', label: 'llama-3.3-70b-instruct' },
    ],
  },
  {
    id: 'mistral',
    label: 'Mistral',
    items: [
      { value: 'mistralai/mistral-large', label: 'mistral-large' },
      { value: 'mistralai/ministral-8b', label: 'ministral-8b' },
      { value: 'mistralai/pixtral-large', label: 'pixtral-large' },
    ],
  },
  {
    id: 'xai',
    label: 'xAI',
    items: [
      { value: 'x-ai/grok-3-beta', label: 'grok-3-beta' },
      { value: 'x-ai/grok-3-mini-beta', label: 'grok-3-mini-beta' },
    ],
  },
]

function getComposerModelGroups(selectedProviderId: string, selectedProviderLabel: string, currentModelId: string): ComposerModelGroup[] {
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

function getRouteTargetDisplayValue(routeKind: NotificationRouteKindDto, routeTarget: string): string {
  try {
    return decomposeNotificationRouteTarget(routeKind, routeTarget).channelTarget
  } catch {
    return displayValue(routeTarget, 'Unavailable')
  }
}

function formatGateLinkage(approval: OperatorApprovalView): string | null {
  if (!approval.gateNodeId || !approval.gateKey) {
    return null
  }

  const transition =
    approval.transitionFromNodeId && approval.transitionToNodeId && approval.transitionKind
      ? `${approval.transitionFromNodeId} → ${approval.transitionToNodeId} (${approval.transitionKind})`
      : null

  return transition
    ? `${approval.gateNodeId} · ${approval.gateKey} · ${transition}`
    : `${approval.gateNodeId} · ${approval.gateKey}`
}

function getSelectedProviderId(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
}

function getSelectedProviderLabel(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderLabel ?? (getSelectedProviderId(agent, runtimeSession) === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex')
}

function getStatusMeta(runtimeSession: RuntimeSessionView | null, agent: AgentPaneView) {
  const selectedProviderId = runtimeHelpers.getSelectedProviderId(agent, runtimeSession)
  const selectedProviderLabel = runtimeHelpers.getSelectedProviderLabel(agent, runtimeSession)
  const providerMismatch = agent.providerMismatch ?? false
  const openrouterApiKeyConfigured = agent.openrouterApiKeyConfigured ?? false

  if (!runtimeSession) {
    return {
      eyebrow: 'Runtime setup',
      title:
        selectedProviderId === 'openrouter'
          ? openrouterApiKeyConfigured
            ? 'Bind OpenRouter for this project'
            : 'Configure OpenRouter in Settings'
          : 'Sign in to OpenAI for this project',
      body: agent.sessionUnavailableReason,
      badgeVariant: 'outline' as const,
    }
  }

  if (providerMismatch) {
    return {
      eyebrow: 'Needs rebind',
      title: `${selectedProviderLabel} is selected in Settings`,
      body: agent.sessionUnavailableReason,
      badgeVariant: 'destructive' as const,
    }
  }

  if (selectedProviderId === 'openrouter') {
    switch (runtimeSession.phase) {
      case 'authenticated':
        return {
          eyebrow: 'Runtime ready',
          title: 'OpenRouter runtime bound',
          body: agent.sessionUnavailableReason,
          badgeVariant: 'default' as const,
        }
      case 'starting':
      case 'refreshing':
      case 'exchanging_code':
        return {
          eyebrow: 'Bind in progress',
          title: 'Binding the saved OpenRouter runtime',
          body: agent.sessionUnavailableReason,
          badgeVariant: 'secondary' as const,
        }
      case 'awaiting_browser_callback':
      case 'awaiting_manual_input':
      case 'failed':
      case 'cancelled':
        return {
          eyebrow: 'Needs attention',
          title: 'OpenRouter runtime needs attention',
          body: agent.sessionUnavailableReason,
          badgeVariant: 'destructive' as const,
        }
      case 'idle':
        return {
          eyebrow: 'Runtime setup',
          title: openrouterApiKeyConfigured ? 'Bind OpenRouter for this project' : 'Configure OpenRouter in Settings',
          body: agent.sessionUnavailableReason,
          badgeVariant: 'outline' as const,
        }
    }
  }

  switch (runtimeSession.phase) {
    case 'authenticated':
      return {
        eyebrow: 'Runtime ready',
        title: 'OpenAI runtime session connected',
        body: agent.sessionUnavailableReason,
        badgeVariant: 'default' as const,
      }
    case 'awaiting_browser_callback':
    case 'awaiting_manual_input':
    case 'starting':
    case 'exchanging_code':
    case 'refreshing':
      return {
        eyebrow: 'Login in progress',
        title: 'Finish the OpenAI login flow',
        body: agent.sessionUnavailableReason,
        badgeVariant: 'secondary' as const,
      }
    case 'failed':
    case 'cancelled':
      return {
        eyebrow: 'Needs attention',
        title: 'Runtime session needs attention',
        body: agent.sessionUnavailableReason,
        badgeVariant: 'destructive' as const,
      }
    case 'idle':
      return {
        eyebrow: 'Runtime setup',
        title: 'Sign in to OpenAI for this project',
        body: agent.sessionUnavailableReason,
        badgeVariant: 'outline' as const,
      }
  }
}

function getStreamBadgeVariant(status: RuntimeStreamStatus): BadgeVariant {
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

function getRuntimeRunBadgeVariant(runtimeRun: RuntimeRunView | null): BadgeVariant {
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

function getAutonomousRunBadgeVariant(autonomousRun: AgentPaneView['autonomousRun'] | null): BadgeVariant {
  if (!autonomousRun) {
    return 'outline'
  }

  switch (autonomousRun.status) {
    case 'starting':
    case 'paused':
    case 'cancelling':
    case 'stale':
      return 'secondary'
    case 'running':
    case 'completed':
    case 'stopped':
    case 'cancelled':
      return 'default'
    case 'failed':
    case 'crashed':
      return 'destructive'
  }
}

function getAutonomousRecoveryBadgeVariant(
  recoveryState: NonNullable<AgentPaneView['autonomousRun']>['recoveryState'],
): BadgeVariant {
  switch (recoveryState) {
    case 'healthy':
      return 'default'
    case 'terminal':
      return 'outline'
    case 'recovery_required':
      return 'secondary'
    case 'failed':
      return 'destructive'
    default:
      return 'outline'
  }
}

function getAutonomousAttemptBadgeVariant(
  attempt: AgentPaneView['autonomousAttempt'] | null | undefined,
): BadgeVariant {
  if (!attempt) {
    return 'outline'
  }

  switch (attempt.status) {
    case 'active':
      return 'default'
    case 'blocked':
      return 'secondary'
    case 'completed':
    case 'cancelled':
      return 'outline'
    case 'failed':
      return 'destructive'
    case 'pending':
      return 'secondary'
    default:
      return 'outline'
  }
}

function getRecentAutonomousUnitBadgeVariant(status: RecentAutonomousUnitCard['status']): BadgeVariant {
  switch (status) {
    case 'active':
      return 'default'
    case 'blocked':
    case 'paused':
    case 'pending':
      return 'secondary'
    case 'failed':
      return 'destructive'
    case 'completed':
    case 'cancelled':
      return 'outline'
  }
}

function getRecentAutonomousWorkflowBadgeVariant(
  state: RecentAutonomousUnitCard['workflowState'],
): BadgeVariant {
  switch (state) {
    case 'ready':
      return 'default'
    case 'awaiting_snapshot':
      return 'secondary'
    case 'awaiting_handoff':
    case 'unlinked':
      return 'outline'
  }
}

function createEmptyRecentAutonomousUnits(): NonNullable<AgentPaneView['recentAutonomousUnits']> {
  return {
    items: [],
    totalCount: 0,
    visibleCount: 0,
    hiddenCount: 0,
    isTruncated: false,
    windowLabel: 'No durable recent units are available yet.',
    latestAttemptOnlyCopy: 'Only the latest durable attempt per unit is shown here.',
    emptyTitle: 'No recent autonomous units recorded',
    emptyBody: 'Cadence has not persisted a bounded autonomous unit history for this project yet.',
  }
}

function createEmptyCheckpointControlLoop(): NonNullable<AgentPaneView['checkpointControlLoop']> {
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

function getRecentAutonomousUnitsAlertMeta(options: {
  recentUnits: NonNullable<AgentPaneView['recentAutonomousUnits']>
  runtimeStream: AgentPaneView['runtimeStream'] | null
  messagesUnavailableReason: string
}) {
  if (options.recentUnits.items.length === 0) {
    return null
  }

  if (!options.runtimeStream || options.runtimeStream.status === 'stale' || options.runtimeStream.status === 'error') {
    return {
      title: 'Recovered durable history remains visible',
      body: options.messagesUnavailableReason,
    }
  }

  if (options.runtimeStream.status === 'subscribing' || options.runtimeStream.status === 'replaying') {
    return {
      title: 'Showing durable history while the live feed catches up',
      body: options.messagesUnavailableReason,
    }
  }

  return null
}

function getCheckpointControlLoopTruthBadgeVariant(truthSource: CheckpointControlLoopCard['truthSource']): BadgeVariant {
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

function getCheckpointControlLoopDurableBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.approval) {
    return getApprovalBadgeVariant(card.approval.status)
  }

  if (card.liveActionRequired) {
    return 'secondary'
  }

  return 'outline'
}

function getCheckpointControlLoopBrokerBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.brokerAction?.hasFailures) {
    return 'destructive'
  }

  if (card.brokerAction?.hasPending) {
    return 'secondary'
  }

  return card.brokerAction ? 'default' : 'outline'
}

function getCheckpointControlLoopEvidenceBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  return card.evidenceCount > 0 ? 'outline' : 'secondary'
}

function getCheckpointControlLoopRecoveryAlertMeta(options: {
  controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>
  trustSnapshot: AgentTrustSnapshotView
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

function getCheckpointControlLoopCoverageAlertMeta(controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>) {
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

function getAutonomousWorkflowContextBadgeVariant(
  state: NonNullable<AgentPaneView['autonomousWorkflowContext']>['state'],
): BadgeVariant {
  switch (state) {
    case 'ready':
      return 'default'
    case 'awaiting_snapshot':
      return 'secondary'
    case 'awaiting_handoff':
      return 'outline'
  }
}

function getAutonomousArtifactBadgeVariant(
  artifact: AgentPaneView['autonomousRecentArtifacts'][number],
): BadgeVariant {
  if (artifact.isPolicyDenied) {
    return 'destructive'
  }

  if (artifact.isVerificationEvidence) {
    return artifact.verificationOutcome === 'failed'
      ? 'destructive'
      : artifact.verificationOutcome === 'blocked'
        ? 'secondary'
        : 'default'
  }

  if (artifact.isToolResult) {
    return artifact.toolState === 'failed'
      ? 'destructive'
      : artifact.toolState === 'running' || artifact.toolState === 'pending'
        ? 'secondary'
        : 'outline'
  }

  return 'outline'
}

function getLatestAutonomousLifecycleReason(
  autonomousRun: AgentPaneView['autonomousRun'] | null,
): { label: string; message: string } | null {
  if (!autonomousRun) {
    return null
  }

  const candidates = [
    autonomousRun.pauseReason && autonomousRun.pausedAt
      ? { label: 'Last pause reason', message: autonomousRun.pauseReason.message, timestamp: autonomousRun.pausedAt }
      : null,
    autonomousRun.cancelReason && autonomousRun.cancelledAt
      ? { label: 'Last cancel reason', message: autonomousRun.cancelReason.message, timestamp: autonomousRun.cancelledAt }
      : null,
    autonomousRun.crashReason && autonomousRun.crashedAt
      ? { label: 'Last crash reason', message: autonomousRun.crashReason.message, timestamp: autonomousRun.crashedAt }
      : null,
  ].filter((candidate): candidate is { label: string; message: string; timestamp: string } => Boolean(candidate))

  if (candidates.length === 0) {
    return null
  }

  candidates.sort((left, right) => getTimestampValue(right.timestamp) - getTimestampValue(left.timestamp))
  return {
    label: candidates[0].label,
    message: candidates[0].message,
  }
}

function hasUsableRuntimeRunId(runtimeRun: RuntimeRunView | null): runtimeRun is RuntimeRunView {
  if (!runtimeRun) {
    return false
  }

  const runId = runtimeRun.runId.trim()
  return runId.length > 0 && runId !== 'run-unavailable'
}

function getRuntimeRunStatusText(runtimeRun: RuntimeRunView | null): string {
  if (!runtimeRun) {
    return 'No run'
  }

  return displayValue(runtimeRun.statusLabel, getRuntimeRunStatusLabel(runtimeRun.status))
}

function getPrimaryRuntimeRunActionLabel(runtimeRun: RuntimeRunView | null): string {
  if (!runtimeRun) {
    return 'Start supervised run'
  }

  if (runtimeRun.isActive || runtimeRun.isStale) {
    return 'Reconnect supervisor'
  }

  return 'Start new supervised run'
}

function getToolStateBadgeVariant(toolState: RuntimeStreamToolItemView['toolState']): BadgeVariant {
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

function getToolStateLabel(toolState: RuntimeStreamToolItemView['toolState']): string {
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

function getSkillStageLabel(stage: RuntimeStreamSkillItemView['stage']): string {
  switch (stage) {
    case 'discovery':
      return 'Discovery'
    case 'install':
      return 'Install'
    case 'invoke':
      return 'Invoke'
  }
}

function getSkillResultBadgeVariant(result: RuntimeStreamSkillItemView['result']): BadgeVariant {
  switch (result) {
    case 'succeeded':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

function getSkillResultLabel(result: RuntimeStreamSkillItemView['result']): string {
  switch (result) {
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

function getSkillCacheLabel(cacheStatus: RuntimeStreamSkillItemView['cacheStatus']): string {
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

function formatSkillSource(item: RuntimeStreamSkillItemView): string {
  return `${item.source.repo} · ${item.source.path} @ ${item.source.reference}`
}

function formatSkillTreeHash(item: RuntimeStreamSkillItemView): string {
  return item.source.treeHash.slice(0, 12)
}

function getApprovalBadgeVariant(status: OperatorApprovalView['status']): BadgeVariant {
  switch (status) {
    case 'pending':
      return 'secondary'
    case 'approved':
      return 'default'
    case 'rejected':
      return 'destructive'
  }
}

function getResumeBadgeVariant(status: ResumeHistoryEntryView['status']): BadgeVariant {
  switch (status) {
    case 'started':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

function getPerActionResumeStateMeta(options: {
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
      badgeVariant: 'default',
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

function formatSequence(sequence: number | null | undefined): string {
  return typeof sequence === 'number' && Number.isFinite(sequence) && sequence > 0 ? `#${sequence}` : 'Not observed'
}

function getStreamRunId(runtimeStream: AgentPaneView['runtimeStream'] | null, runtimeRun: RuntimeRunView | null): string {
  const streamRunId = runtimeStream?.runId
  if (typeof streamRunId === 'string' && streamRunId.trim().length > 0) {
    return streamRunId.trim()
  }

  if (hasUsableRuntimeRunId(runtimeRun)) {
    return runtimeRun.runId.trim()
  }

  return 'No active run'
}

function getComposerPlaceholder(
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
      : 'Connect a provider to start'
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

function getStreamStatusMeta(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null) {
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

function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <div className="flex items-start justify-between gap-3 text-[11px] text-muted-foreground">
      <span>{label}</span>
      <span className={mono ? 'max-w-[60%] break-all text-right font-mono text-foreground/75' : 'max-w-[60%] text-right text-foreground/75'}>
        {value}
      </span>
    </div>
  )
}

function FeedEmptyState({
  title,
  body,
}: {
  title: string
  body: string
}) {
  return (
    <div className="rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
      <p className="font-medium text-foreground/85">{title}</p>
      <p className="mt-1 leading-6">{body}</p>
    </div>
  )
}

function CountCard({
  label,
  value,
  tone = 'default',
}: {
  label: string
  value: string
  tone?: 'default' | 'success' | 'danger'
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-card/70 px-3 py-3">
      <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">{label}</p>
      <p
        className={`mt-2 text-lg font-semibold ${
          tone === 'success'
            ? 'text-emerald-600 dark:text-emerald-400'
            : tone === 'danger'
              ? 'text-destructive'
              : 'text-foreground'
        }`}
      >
        {value}
      </p>
    </div>
  )
}

export function AgentRuntime({
  agent,
  onOpenSettings,
  onStartLogin,
  onStartAutonomousRun,
  onInspectAutonomousRun,
  onCancelAutonomousRun,
  onStartRuntimeRun,
  onStartRuntimeSession,
  onStopRuntimeRun,
  onSubmitManualCallback,
  onLogout,
  onRetryStream,
  onResolveOperatorAction,
  onResumeOperatorRun,
  onRefreshNotificationRoutes,
  onUpsertNotificationRoute,
}: AgentRuntimeProps) {
  const runtimeSession = agent.runtimeSession ?? null
  const runtimeRun = agent.runtimeRun ?? null
  const autonomousRun = agent.autonomousRun ?? null
  const autonomousUnit = agent.autonomousUnit ?? null
  const autonomousAttempt = agent.autonomousAttempt ?? null
  const autonomousWorkflowContext = agent.autonomousWorkflowContext ?? null
  const autonomousRecentArtifacts = useMemo(
    () => sortByNewest(agent.autonomousRecentArtifacts ?? [], (artifact) => artifact.updatedAt || artifact.createdAt).slice(0, 5),
    [agent.autonomousRecentArtifacts],
  )
  const recentAutonomousUnits = agent.recentAutonomousUnits ?? createEmptyRecentAutonomousUnits()
  const checkpointControlLoop = agent.checkpointControlLoop ?? runtimeHelpers.createEmptyCheckpointControlLoop()
  const recentAutonomousUnitsAlert = getRecentAutonomousUnitsAlertMeta({
    recentUnits: recentAutonomousUnits,
    runtimeStream: agent.runtimeStream ?? null,
    messagesUnavailableReason: agent.messagesUnavailableReason,
  })
  const renderableRuntimeRun = runtimeHelpers.hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const hasIncompleteRuntimeRunPayload = Boolean(runtimeRun && !renderableRuntimeRun)
  const runtimeStream = agent.runtimeStream ?? null
  const streamStatus = agent.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeStreamItems = agent.runtimeStreamItems ?? runtimeStream?.items ?? []
  const activityItems = agent.activityItems ?? runtimeStream?.activityItems ?? []
  const actionRequiredItems = agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? []
  const skillItems = agent.skillItems ?? runtimeStream?.skillItems ?? []
  const transcriptItems = runtimeStream?.transcriptItems ?? []
  const toolCalls = runtimeStream?.toolCalls ?? []
  const runtimeRunErrorMessage = agent.runtimeRunErrorMessage ?? null
  const autonomousRunErrorMessage = agent.autonomousRunErrorMessage ?? null
  const latestAutonomousLifecycleReason = getLatestAutonomousLifecycleReason(autonomousRun)
  const runtimeRunCheckpoints = useMemo<RuntimeRunCheckpointView[]>(
    () => runtimeHelpers.sortByNewest(renderableRuntimeRun?.checkpoints ?? [], (checkpoint) => checkpoint.createdAt).slice(0, 4),
    [renderableRuntimeRun],
  )
  const streamStatusLabel = runtimeHelpers.displayValue(agent.runtimeStreamStatusLabel, getRuntimeStreamStatusLabel(streamStatus))
  const streamIssue: RuntimeStreamIssueView | null = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const approvalRequests = agent.approvalRequests ?? []
  const resumeHistory = agent.resumeHistory ?? []
  const notificationSyncSummary = agent.notificationSyncSummary ?? null
  const notificationSyncError = agent.notificationSyncError ?? null
  const notificationSyncPollingActive = agent.notificationSyncPollingActive ?? false
  const notificationSyncPollingActionId = agent.notificationSyncPollingActionId ?? null
  const notificationSyncPollingBoundaryId = agent.notificationSyncPollingBoundaryId ?? null
  const checkpointTrustSnapshot = agent.trustSnapshot ?? {
    syncState: notificationSyncError ? 'degraded' : 'unavailable',
    syncReason: notificationSyncError
      ? notificationSyncError.message
      : notificationSyncSummary
        ? 'Cadence is keeping the last observed sync counts visible, but hook-owned trust projection is unavailable.'
        : 'No notification adapter sync summary is available yet.',
  }
  const checkpointControlLoopRecoveryAlert = runtimeHelpers.getCheckpointControlLoopRecoveryAlertMeta({
    controlLoop: checkpointControlLoop,
    trustSnapshot: checkpointTrustSnapshot,
    autonomousRunErrorMessage,
    notificationSyncPollingActive,
    notificationSyncPollingActionId,
    notificationSyncPollingBoundaryId,
  })
  const checkpointControlLoopCoverageAlert = runtimeHelpers.getCheckpointControlLoopCoverageAlertMeta(checkpointControlLoop)
  const [autonomousRunActionMessage, setAutonomousRunActionMessage] = useState<string | null>(null)
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [operatorAnswers, setOperatorAnswers] = useState<Record<string, string>>({})
  const [pendingOperatorIntent, setPendingOperatorIntent] = useState<{
    actionId: string
    kind: OperatorIntentKind
  } | null>(null)
  const [recentRunReplacement, setRecentRunReplacement] = useState<{
    previousRunId: string
    nextRunId: string
  } | null>(null)
  const lastSeenProjectIdRef = useRef(agent.project.id)
  const lastSeenRuntimeRunIdRef = useRef<string | null>(renderableRuntimeRun?.runId ?? null)

  const selectedProviderId = runtimeHelpers.getSelectedProviderId(agent, runtimeSession)
  const selectedProviderLabel = runtimeHelpers.getSelectedProviderLabel(agent, runtimeSession)
  const selectedModelId = displayValue(agent.selectedModelId, selectedProviderId === 'openrouter' ? 'Model not configured' : 'openai_codex')
  const composerModelGroups = useMemo(
    () => runtimeHelpers.getComposerModelGroups(selectedProviderId, selectedProviderLabel, selectedModelId),
    [selectedModelId, selectedProviderId, selectedProviderLabel],
  )
  const [composerModelId, setComposerModelId] = useState(selectedModelId)
  const [composerThinkingLevel, setComposerThinkingLevel] = useState<ComposerThinkingLevelOption['value']>('medium')
  const isOpenRouterSelected = selectedProviderId === 'openrouter'
  const openrouterApiKeyConfigured = agent.openrouterApiKeyConfigured ?? false
  const providerMismatch = agent.providerMismatch ?? false
  const streamStatusMeta = useMemo(() => runtimeHelpers.getStreamStatusMeta(agent, runtimeSession), [agent, runtimeSession])
  const repositoryPath = displayValue(agent.repositoryPath, 'No repository path available')
  const repositoryLabel = displayValue(agent.repositoryLabel, agent.project.name)
  const sessionLabel = displayValue(runtimeSession?.sessionLabel, 'No session')
  const streamRunId = runtimeHelpers.getStreamRunId(runtimeStream, renderableRuntimeRun)
  const streamSequenceLabel = formatSequence(runtimeStream?.lastSequence ?? null)
  const streamSessionLabel = displayValue(runtimeStream?.sessionId, runtimeSession?.sessionLabel ?? 'No session')
  const hasStreamRunMismatch = Boolean(runtimeStream?.runId && renderableRuntimeRun && runtimeStream.runId !== renderableRuntimeRun.runId)
  const hasAttachedRun = Boolean(renderableRuntimeRun)
  const showNoRunStreamBanner = Boolean(runtimeSession?.isAuthenticated && !hasAttachedRun)
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const canStartAutonomousRun = Boolean(
    hasRepositoryBinding && typeof onStartAutonomousRun === 'function' && runtimeSession?.isAuthenticated && runtimeSession.sessionId,
  )
  const canInspectAutonomousRun = hasRepositoryBinding && typeof onInspectAutonomousRun === 'function'
  const canCancelAutonomousRun = Boolean(
    hasRepositoryBinding && autonomousRun && !autonomousRun.isTerminal && !autonomousRun.isFailed && typeof onCancelAutonomousRun === 'function',
  )
  const canStartRuntimeRun = Boolean(
    hasRepositoryBinding && typeof onStartRuntimeRun === 'function' && (runtimeSession?.isAuthenticated || renderableRuntimeRun),
  )
  const canStopRuntimeRun = Boolean(
    hasRepositoryBinding && renderableRuntimeRun && !renderableRuntimeRun.isTerminal && typeof onStopRuntimeRun === 'function',
  )
  const canResolveOperatorActions = hasRepositoryBinding && typeof onResolveOperatorAction === 'function'
  const canResumeOperatorRuns = hasRepositoryBinding && typeof onResumeOperatorRun === 'function'
  const composerPlaceholder = runtimeHelpers.getComposerPlaceholder(runtimeSession, streamStatus, renderableRuntimeRun, streamRunId, {
    selectedProviderId,
    openrouterApiKeyConfigured,
    providerMismatch,
  })
  const showAgentSetupEmptyState = Boolean(
    !providerMismatch && (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
  )
  const liveFeedCount = runtimeStreamItems.length
  const latestCompletion = runtimeStream?.completion ?? null
  const latestFailure = runtimeStream?.failure ?? null
  const operatorActionError = agent.operatorActionError
  const operatorActionStatus = agent.operatorActionStatus
  const pendingOperatorActionId = agent.pendingOperatorActionId
  const runtimeRunActionStatus = agent.runtimeRunActionStatus ?? 'idle'
  const pendingRuntimeRunAction = agent.pendingRuntimeRunAction ?? null
  const autonomousRunActionStatus = agent.autonomousRunActionStatus ?? 'idle'
  const pendingAutonomousRunAction = agent.pendingAutonomousRunAction ?? null
  const autonomousRunActionError =
    agent.autonomousRunActionError ??
    (autonomousRunActionMessage
      ? {
          code: 'autonomous_run_action_failed',
          message: autonomousRunActionMessage,
          retryable: false,
        }
      : null)
  const runtimeRunActionError =
    agent.runtimeRunActionError ??
    (runtimeRunActionMessage
      ? {
          code: 'runtime_run_action_failed',
          message: runtimeRunActionMessage,
          retryable: false,
        }
      : null)
  const hasCheckpointControlLoopSurface = Boolean(checkpointControlLoop.totalCount > 0 || operatorActionError)
  const hasAgentFeedSurface = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      runtimeSession?.isAuthenticated ||
      recentRunReplacement ||
      streamIssue ||
      transcriptItems.length > 0 ||
      activityItems.length > 0 ||
      toolCalls.length > 0 ||
      skillItems.length > 0 ||
      actionRequiredItems.length > 0 ||
      latestCompletion ||
      latestFailure,
  )
  // The Agent tab no longer renders these debug-oriented panels.
  const showAutonomousLedgerPanel = false
  const sortedApprovals = useMemo(
    () => sortByNewest(approvalRequests, (approval) => approval.updatedAt ?? approval.createdAt).slice(0, 6),
    [approvalRequests],
  )
  const pendingApprovals = useMemo(() => sortedApprovals.filter((approval) => approval.isPending), [sortedApprovals])
  const runtimeRunStatusText = runtimeHelpers.getRuntimeRunStatusText(renderableRuntimeRun)
  const primaryRuntimeRunActionLabel = runtimeHelpers.getPrimaryRuntimeRunActionLabel(renderableRuntimeRun)
  const autonomousRunActionErrorTitle =
    autonomousRunActionError?.retryable || autonomousRunActionError?.code.includes('timeout')
      ? 'Autonomous run control needs retry'
      : 'Autonomous run control failed'
  const runtimeRunActionErrorTitle =
    runtimeRunActionError?.retryable || runtimeRunActionError?.code.includes('timeout')
      ? 'Run control needs retry'
      : 'Run control failed'

  useEffect(() => {
    setComposerModelId(selectedModelId)
  }, [selectedModelId, selectedProviderId])

  useEffect(() => {
    if (agent.runtimeRunActionError) {
      setRuntimeRunActionMessage(null)
      return
    }

    if (renderableRuntimeRun?.updatedAt) {
      setRuntimeRunActionMessage(null)
    }
  }, [agent.runtimeRunActionError, renderableRuntimeRun?.runId, renderableRuntimeRun?.updatedAt])

  useEffect(() => {
    if (agent.autonomousRunActionError) {
      setAutonomousRunActionMessage(null)
      return
    }

    if (autonomousRun?.updatedAt) {
      setAutonomousRunActionMessage(null)
    }
  }, [agent.autonomousRunActionError, autonomousRun?.runId, autonomousRun?.updatedAt])

  useEffect(() => {
    if (operatorActionStatus === 'idle' && !pendingOperatorActionId) {
      setPendingOperatorIntent(null)
    }
  }, [operatorActionStatus, pendingOperatorActionId])

  useEffect(() => {
    setOperatorAnswers((currentAnswers) => {
      const nextAnswers: Record<string, string> = {}
      const knownActionIds = new Set(sortedApprovals.map((approval) => approval.actionId))

      for (const approval of sortedApprovals) {
        const existingAnswer = currentAnswers[approval.actionId]
        if (typeof existingAnswer === 'string') {
          nextAnswers[approval.actionId] = existingAnswer
          continue
        }

        if (approval.userAnswer) {
          nextAnswers[approval.actionId] = approval.userAnswer
        }
      }

      if (Object.keys(nextAnswers).length === Object.keys(currentAnswers).length) {
        const unchanged = Object.keys(nextAnswers).every(
          (actionId) => knownActionIds.has(actionId) && nextAnswers[actionId] === currentAnswers[actionId],
        )
        if (unchanged) {
          return currentAnswers
        }
      }

      return nextAnswers
    })
  }, [sortedApprovals])

  useEffect(() => {
    if (lastSeenProjectIdRef.current !== agent.project.id) {
      lastSeenProjectIdRef.current = agent.project.id
      lastSeenRuntimeRunIdRef.current = renderableRuntimeRun?.runId ?? null
      setRecentRunReplacement(null)
      return
    }

    const previousRunId = lastSeenRuntimeRunIdRef.current
    const nextRunId = renderableRuntimeRun?.runId ?? null

    if (previousRunId && nextRunId && previousRunId !== nextRunId) {
      setRecentRunReplacement({ previousRunId, nextRunId })
    }

    lastSeenRuntimeRunIdRef.current = nextRunId
  }, [agent.project.id, renderableRuntimeRun?.runId])

  useEffect(() => {
    if (!recentRunReplacement) {
      return
    }

    const currentRunId = renderableRuntimeRun?.runId ?? null
    const hasFreshItemsForReplacementRun =
      currentRunId === recentRunReplacement.nextRunId &&
      runtimeStream?.runId === recentRunReplacement.nextRunId &&
      runtimeStreamItems.some((item) => item.runId === recentRunReplacement.nextRunId)

    if (!currentRunId || currentRunId !== recentRunReplacement.nextRunId || hasFreshItemsForReplacementRun) {
      setRecentRunReplacement(null)
    }
  }, [recentRunReplacement, renderableRuntimeRun?.runId, runtimeStream?.runId, runtimeStreamItems])

  async function handleStartAutonomousRun() {
    if (!canStartAutonomousRun || !onStartAutonomousRun) {
      return
    }

    setAutonomousRunActionMessage(null)

    try {
      await onStartAutonomousRun()
    } catch (error) {
      setAutonomousRunActionMessage(getErrorMessage(error, 'Cadence could not start the autonomous run.'))
    }
  }

  async function handleInspectAutonomousRun() {
    if (!canInspectAutonomousRun || !onInspectAutonomousRun) {
      return
    }

    setAutonomousRunActionMessage(null)

    try {
      await onInspectAutonomousRun()
    } catch (error) {
      setAutonomousRunActionMessage(getErrorMessage(error, 'Cadence could not inspect autonomous run truth.'))
    }
  }

  async function handleCancelAutonomousRun() {
    if (!canCancelAutonomousRun || !onCancelAutonomousRun || !autonomousRun?.runId) {
      return
    }

    setAutonomousRunActionMessage(null)

    try {
      await onCancelAutonomousRun(autonomousRun.runId)
    } catch (error) {
      setAutonomousRunActionMessage(getErrorMessage(error, 'Cadence could not cancel the autonomous run.'))
    }
  }

  async function handleStartRuntimeRun() {
    if (!canStartRuntimeRun || !onStartRuntimeRun) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onStartRuntimeRun()
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not start the supervised run.'))
    }
  }

  async function handleStopRuntimeRun() {
    if (!canStopRuntimeRun || !onStopRuntimeRun || !renderableRuntimeRun) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onStopRuntimeRun(renderableRuntimeRun.runId)
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not stop the supervised run.'))
    }
  }

  async function handleResolveOperatorAction(
    actionId: string,
    decision: 'approve' | 'reject',
    options: { userAnswer?: string | null } = {},
  ) {
    if (!canResolveOperatorActions || !onResolveOperatorAction) {
      return
    }

    setPendingOperatorIntent({ actionId, kind: decision })

    try {
      await onResolveOperatorAction(actionId, decision, {
        userAnswer: options.userAnswer ?? null,
      })
    } catch {
      // Preserve the last truthful UI state. Hook-backed callers surface operatorActionError.
    } finally {
      setPendingOperatorIntent(null)
    }
  }

  async function handleResumeOperatorRun(actionId: string, options: { userAnswer?: string | null } = {}) {
    if (!canResumeOperatorRuns || !onResumeOperatorRun) {
      return
    }

    setPendingOperatorIntent({ actionId, kind: 'resume' })

    try {
      await onResumeOperatorRun(actionId, {
        userAnswer: options.userAnswer ?? null,
      })
    } catch {
      // Preserve the last truthful UI state. Hook-backed callers surface operatorActionError.
    } finally {
      setPendingOperatorIntent(null)
    }
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div
          className={
            showAgentSetupEmptyState
              ? 'flex flex-1 items-center justify-center overflow-y-auto scrollbar-thin px-6 py-5'
              : 'flex-1 overflow-y-auto scrollbar-thin px-4 py-4'
          }
        >
          {showAgentSetupEmptyState ? (
            <SetupEmptyState onOpenSettings={onOpenSettings} />
          ) : (
          <div className="mx-auto flex max-w-4xl flex-col gap-4">

            {showAutonomousLedgerPanel ? (
              <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
              <div className="flex flex-col gap-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Autonomous ledger</p>
                    <h2 className="mt-2 text-lg font-semibold text-foreground">Autonomous run truth</h2>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant={getAutonomousRunBadgeVariant(autonomousRun)}>
                      {autonomousRun?.statusLabel ?? 'No autonomous run'}
                    </Badge>
                    {autonomousRun ? (
                      <Badge variant={getAutonomousRecoveryBadgeVariant(autonomousRun.recoveryState)}>
                        {autonomousRun.recoveryLabel}
                      </Badge>
                    ) : null}
                    {autonomousUnit ? <Badge variant="outline">{autonomousUnit.statusLabel}</Badge> : null}
                  </div>
                </div>

                <p className="text-sm leading-6 text-muted-foreground">
                  {autonomousRun
                    ? 'Cadence is projecting the durable autonomous run and active unit boundary separately from the live runtime/session feed.'
                    : runtimeSession?.isAuthenticated && runtimeSession.sessionId
                      ? 'Start an autonomous run to bind the selected project to a durable run/unit ledger and recovery state.'
                      : 'Authenticate the selected project first so Cadence can bind an autonomous run to a stable runtime session.'}
                </p>

                {autonomousRunActionError ? (
                  <Alert variant="destructive">
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>{autonomousRunActionErrorTitle}</AlertTitle>
                    <AlertDescription>
                      <p>{autonomousRunActionError.message}</p>
                      {autonomousRunActionError.code ? (
                        <p className="font-mono text-[11px] text-destructive/80">code: {autonomousRunActionError.code}</p>
                      ) : null}
                    </AlertDescription>
                  </Alert>
                ) : null}

                {autonomousRunErrorMessage ? (
                  <Alert>
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Showing last truthful autonomous snapshot</AlertTitle>
                    <AlertDescription>{autonomousRunErrorMessage}</AlertDescription>
                  </Alert>
                ) : null}

                {autonomousRun?.duplicateStartDetected ? (
                  <Alert>
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Duplicate start prevented</AlertTitle>
                    <AlertDescription>
                      <p>{displayValue(autonomousRun.duplicateStartReason, 'Cadence reused the active autonomous run instead of launching a duplicate continuation.')}</p>
                      {autonomousRun.duplicateStartRunId ? (
                        <p className="mt-1 font-mono text-[11px] text-muted-foreground">run: {autonomousRun.duplicateStartRunId}</p>
                      ) : null}
                    </AlertDescription>
                  </Alert>
                ) : null}

                <div className="flex flex-wrap gap-2">
                  {canStartAutonomousRun ? (
                    <Button
                      disabled={autonomousRunActionStatus === 'running'}
                      onClick={() => void handleStartAutonomousRun()}
                      type="button"
                    >
                      {autonomousRunActionStatus === 'running' && pendingAutonomousRunAction === 'start' ? (
                        <LoaderCircle className="h-4 w-4 animate-spin" />
                      ) : (
                        <Play className="h-4 w-4" />
                      )}
                      Start autonomous run
                    </Button>
                  ) : null}

                  {canCancelAutonomousRun ? (
                    <Button
                      disabled={autonomousRunActionStatus === 'running' && pendingAutonomousRunAction === 'cancel'}
                      onClick={() => void handleCancelAutonomousRun()}
                      type="button"
                      variant="outline"
                    >
                      {autonomousRunActionStatus === 'running' && pendingAutonomousRunAction === 'cancel' ? (
                        <LoaderCircle className="h-4 w-4 animate-spin" />
                      ) : (
                        <XCircle className="h-4 w-4" />
                      )}
                      Cancel autonomous run
                    </Button>
                  ) : null}

                  {canInspectAutonomousRun ? (
                    <Button
                      disabled={autonomousRunActionStatus === 'running' && pendingAutonomousRunAction === 'inspect'}
                      onClick={() => void handleInspectAutonomousRun()}
                      type="button"
                      variant="secondary"
                    >
                      {autonomousRunActionStatus === 'running' && pendingAutonomousRunAction === 'inspect' ? (
                        <LoaderCircle className="h-4 w-4 animate-spin" />
                      ) : (
                        <ShieldCheck className="h-4 w-4" />
                      )}
                      Inspect truth
                    </Button>
                  ) : null}
                </div>

                {autonomousRun ? (
                  <div className="space-y-4">
                    <div className="grid gap-4 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
                      <div className="space-y-3 rounded-xl border border-border/70 bg-card/70 p-4">
                        <div className="flex flex-wrap items-center gap-2">
                          <h3 className="text-base font-semibold text-foreground">Current autonomous boundary</h3>
                          {autonomousUnit ? <Badge variant="outline">{autonomousUnit.kindLabel}</Badge> : null}
                        </div>
                        <div className="grid gap-3 sm:grid-cols-2">
                          <CountCard label="Run ID" value={autonomousRun.runId} />
                          <CountCard label="Recovery" value={autonomousRun.recoveryLabel} />
                          <CountCard label="Active unit" value={displayValue(autonomousRun.activeUnitId, 'No active unit')} />
                          <CountCard label="Unit status" value={autonomousUnit?.statusLabel ?? 'Unavailable'} />
                        </div>
                        {autonomousUnit ? (
                          <>
                            <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-sm">
                              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                                <span>{formatSequence(autonomousUnit.sequence)}</span>
                                <span>{autonomousUnit.unitId}</span>
                              </div>
                              <p className="mt-2 font-medium text-foreground">{autonomousUnit.summary}</p>
                              <p className="mt-2 text-muted-foreground">
                                Boundary {displayValue(autonomousUnit.boundaryId, 'unavailable')} · Updated {formatTimestamp(autonomousUnit.updatedAt)}
                              </p>
                            </div>

                            {autonomousWorkflowContext ? (
                              <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-sm">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="font-medium text-foreground">Linked workflow context</p>
                                  <Badge variant={getAutonomousWorkflowContextBadgeVariant(autonomousWorkflowContext.state)}>
                                    {autonomousWorkflowContext.stateLabel}
                                  </Badge>
                                  <Badge variant="outline">
                                    {autonomousWorkflowContext.linkageSource === 'attempt' ? 'Attempt linkage' : 'Unit linkage'}
                                  </Badge>
                                  {autonomousWorkflowContext.pendingApproval ? (
                                    <Badge variant="secondary">Pending approval</Badge>
                                  ) : null}
                                </div>
                                <p className="mt-2 text-muted-foreground">{autonomousWorkflowContext.detail}</p>
                                <div className="mt-3 grid gap-3 sm:grid-cols-2">
                                  <CountCard
                                    label="Workflow node"
                                    value={autonomousWorkflowContext.linkedStage?.stageLabel ?? autonomousWorkflowContext.linkedNodeLabel}
                                  />
                                  <CountCard
                                    label="Stage status"
                                    value={autonomousWorkflowContext.linkedStage?.statusLabel ?? 'Unavailable'}
                                  />
                                  <CountCard
                                    label="Handoff"
                                    value={displayValue(autonomousWorkflowContext.handoff?.handoffTransitionId, 'Pending')}
                                  />
                                  <CountCard
                                    label="Approval"
                                    value={autonomousWorkflowContext.pendingApproval?.statusLabel ?? 'None'}
                                  />
                                </div>
                                <div className="mt-3 space-y-1 text-[11px] text-muted-foreground">
                                  <InfoRow label="Linked node ID" mono value={autonomousWorkflowContext.linkage.workflowNodeId} />
                                  <InfoRow label="Transition ID" mono value={autonomousWorkflowContext.linkage.transitionId} />
                                  {autonomousWorkflowContext.linkage.causalTransitionId ? (
                                    <InfoRow
                                      label="Causal transition"
                                      mono
                                      value={autonomousWorkflowContext.linkage.causalTransitionId}
                                    />
                                  ) : null}
                                  <InfoRow
                                    label="Handoff transition"
                                    mono
                                    value={autonomousWorkflowContext.linkage.handoffTransitionId}
                                  />
                                  <InfoRow
                                    label="Handoff hash"
                                    mono
                                    value={autonomousWorkflowContext.linkage.handoffPackageHash}
                                  />
                                  {autonomousWorkflowContext.activeLifecycleStage ? (
                                    <InfoRow
                                      label="Snapshot active stage"
                                      value={autonomousWorkflowContext.activeLifecycleStage.stageLabel}
                                    />
                                  ) : null}
                                  {autonomousWorkflowContext.handoff ? (
                                    <>
                                      <InfoRow
                                        label="Persisted handoff"
                                        value={formatTimestamp(autonomousWorkflowContext.handoff.createdAt)}
                                      />
                                      <InfoRow
                                        label="From → to"
                                        mono
                                        value={`${autonomousWorkflowContext.handoff.fromNodeId} → ${autonomousWorkflowContext.handoff.toNodeId}`}
                                      />
                                      <InfoRow
                                        label="Transition kind"
                                        value={autonomousWorkflowContext.handoff.transitionKindLabel}
                                      />
                                    </>
                                  ) : null}
                                  {autonomousWorkflowContext.pendingApproval ? (
                                    <InfoRow
                                      label="Pending approval"
                                      value={autonomousWorkflowContext.pendingApproval.title}
                                    />
                                  ) : null}
                                </div>
                              </div>
                            ) : (
                              <FeedEmptyState
                                title="Workflow linkage pending"
                                body="Cadence has not persisted workflow-node and handoff linkage for this autonomous boundary yet."
                              />
                            )}
                          </>
                        ) : (
                          <FeedEmptyState
                            body="Cadence has not rehydrated an active autonomous unit boundary for this project yet."
                            title="No autonomous unit recorded"
                          />
                        )}
                      </div>

                      <div className="space-y-3 rounded-xl border border-border/70 bg-card/70 p-4">
                        <h3 className="text-base font-semibold text-foreground">Lifecycle diagnostics</h3>
                        <InfoRow label="Started" value={formatTimestamp(autonomousRun.startedAt)} />
                        <InfoRow label="Last checkpoint" value={formatTimestamp(autonomousRun.lastCheckpointAt)} />
                        <InfoRow label="Last heartbeat" value={formatTimestamp(autonomousRun.lastHeartbeatAt)} />
                        <InfoRow label="Updated" value={formatTimestamp(autonomousRun.updatedAt)} />
                        <InfoRow label="Recovery state" value={autonomousRun.recoveryLabel} />
                        {latestAutonomousLifecycleReason ? (
                          <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                            <p className="text-sm font-medium text-foreground">{latestAutonomousLifecycleReason.label}</p>
                            <p className="mt-2 text-sm leading-6 text-muted-foreground">{latestAutonomousLifecycleReason.message}</p>
                          </div>
                        ) : null}
                        {autonomousRun.lastError?.message ? (
                          <Alert variant="destructive">
                            <AlertCircle className="h-4 w-4" />
                            <AlertTitle>Last autonomous error</AlertTitle>
                            <AlertDescription>
                              <p>{autonomousRun.lastError.message}</p>
                              {autonomousRun.lastErrorCode ? (
                                <p className="font-mono text-[11px] text-destructive/80">code: {autonomousRun.lastErrorCode}</p>
                              ) : null}
                            </AlertDescription>
                          </Alert>
                        ) : null}
                      </div>
                    </div>

                    <div className="grid gap-4 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
                      <div className="space-y-3 rounded-xl border border-border/70 bg-card/70 p-4">
                        <div className="flex flex-wrap items-center gap-2">
                          <h3 className="text-base font-semibold text-foreground">Current attempt</h3>
                          {autonomousAttempt ? (
                            <Badge variant={getAutonomousAttemptBadgeVariant(autonomousAttempt)}>
                              {autonomousAttempt.statusLabel}
                            </Badge>
                          ) : null}
                        </div>
                        {autonomousAttempt ? (
                          <>
                            <div className="grid gap-3 sm:grid-cols-2">
                              <CountCard label="Attempt" value={`#${autonomousAttempt.attemptNumber}`} />
                              <CountCard label="Child session" value={autonomousAttempt.childSessionId} />
                              <CountCard label="Boundary" value={displayValue(autonomousAttempt.boundaryId, 'None')} />
                              <CountCard label="Updated" value={formatTimestamp(autonomousAttempt.updatedAt)} />
                            </div>
                            <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-sm">
                              <p className="font-medium text-foreground">Attempt {autonomousAttempt.attemptId}</p>
                              <p className="mt-2 text-muted-foreground">
                                Started {formatTimestamp(autonomousAttempt.startedAt)} · Finished {formatTimestamp(autonomousAttempt.finishedAt)}
                              </p>
                            </div>
                            {autonomousAttempt.lastError?.message ? (
                              <Alert variant="destructive">
                                <AlertCircle className="h-4 w-4" />
                                <AlertTitle>Last attempt error</AlertTitle>
                                <AlertDescription>
                                  <p>{autonomousAttempt.lastError.message}</p>
                                  {autonomousAttempt.lastErrorCode ? (
                                    <p className="font-mono text-[11px] text-destructive/80">code: {autonomousAttempt.lastErrorCode}</p>
                                  ) : null}
                                </AlertDescription>
                              </Alert>
                            ) : null}
                          </>
                        ) : (
                          <FeedEmptyState
                            title="No autonomous attempt recorded"
                            body="Cadence has not persisted an active autonomous attempt for this project yet."
                          />
                        )}
                      </div>

                      <div className="space-y-3 rounded-xl border border-border/70 bg-card/70 p-4">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <h3 className="text-base font-semibold text-foreground">Recent evidence</h3>
                          <Badge variant="outline">{autonomousRecentArtifacts.length} items</Badge>
                        </div>
                        {autonomousRecentArtifacts.length > 0 ? (
                          <div className="space-y-3">
                            {autonomousRecentArtifacts.map((artifact) => (
                              <div
                                key={artifact.artifactId}
                                className="rounded-lg border border-border/70 bg-background/70 px-3 py-3 text-sm"
                              >
                                <div className="flex flex-wrap items-center gap-2">
                                  <Badge variant={getAutonomousArtifactBadgeVariant(artifact)}>
                                    {artifact.artifactKindLabel}
                                  </Badge>
                                  <Badge variant="outline">{artifact.statusLabel}</Badge>
                                  {artifact.toolStateLabel ? <Badge variant="outline">{artifact.toolStateLabel}</Badge> : null}
                                  {artifact.verificationOutcomeLabel ? (
                                    <Badge variant="outline">{artifact.verificationOutcomeLabel}</Badge>
                                  ) : null}
                                </div>
                                <p className="mt-2 font-medium text-foreground">{artifact.summary}</p>
                                {artifact.detail ? (
                                  <p className="mt-2 text-muted-foreground">{artifact.detail}</p>
                                ) : null}
                                <div className="mt-3 space-y-1 text-[11px] text-muted-foreground">
                                  <InfoRow label="Artifact" mono value={artifact.artifactId} />
                                  <InfoRow label="Attempt" mono value={artifact.attemptId} />
                                  <InfoRow label="Updated" value={formatTimestamp(artifact.updatedAt)} />
                                  {artifact.diagnosticCode ? <InfoRow label="Diagnostic" mono value={artifact.diagnosticCode} /> : null}
                                  {artifact.actionId ? <InfoRow label="Action" mono value={artifact.actionId} /> : null}
                                  {artifact.boundaryId ? <InfoRow label="Boundary" mono value={artifact.boundaryId} /> : null}
                                  {artifact.commandResult ? (
                                    <InfoRow
                                      label="Command result"
                                      value={
                                        artifact.commandResult.exitCode === null
                                          ? artifact.commandResult.summary
                                          : `exit ${artifact.commandResult.exitCode}${artifact.commandResult.timedOut ? ' • timeout' : ''} • ${artifact.commandResult.summary}`
                                      }
                                    />
                                  ) : null}
                                </div>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <FeedEmptyState
                            title="No tool evidence recorded"
                            body="Cadence has not persisted a recent tool result, verification evidence row, or policy denial for this project yet."
                          />
                        )}
                      </div>
                    </div>

                    <div className="space-y-3 rounded-xl border border-border/70 bg-card/70 p-4">
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                          <h3 className="text-base font-semibold text-foreground">Recent autonomous units</h3>
                          <p className="mt-2 text-sm leading-6 text-muted-foreground">{recentAutonomousUnits.latestAttemptOnlyCopy}</p>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline">{recentAutonomousUnits.windowLabel}</Badge>
                          {recentAutonomousUnits.isTruncated ? (
                            <Badge variant="secondary">+{recentAutonomousUnits.hiddenCount} older unit{recentAutonomousUnits.hiddenCount === 1 ? '' : 's'}</Badge>
                          ) : null}
                        </div>
                      </div>

                      {recentAutonomousUnitsAlert ? (
                        <Alert>
                          <AlertCircle className="h-4 w-4" />
                          <AlertTitle>{recentAutonomousUnitsAlert.title}</AlertTitle>
                          <AlertDescription>{recentAutonomousUnitsAlert.body}</AlertDescription>
                        </Alert>
                      ) : null}

                      {recentAutonomousUnits.items.length > 0 ? (
                        <div className="grid gap-3 xl:grid-cols-2">
                          {recentAutonomousUnits.items.map((unit) => (
                            <div
                              key={unit.unitId}
                              className="rounded-lg border border-border/70 bg-background/70 px-4 py-4 text-sm"
                            >
                              <div className="flex flex-wrap items-center gap-2">
                                <Badge variant="outline">{unit.sequenceLabel}</Badge>
                                <Badge variant={getRecentAutonomousUnitBadgeVariant(unit.status)}>{unit.statusLabel}</Badge>
                                <Badge variant="outline">{unit.kindLabel}</Badge>
                              </div>
                              <p className="mt-3 font-medium text-foreground">{unit.summary}</p>
                              <p className="mt-2 text-muted-foreground">
                                Boundary {displayValue(unit.boundaryId, 'Unavailable')} · Updated {formatTimestamp(unit.updatedAt)}
                              </p>

                              <div className="mt-4 grid gap-3 lg:grid-cols-3">
                                <div className="rounded-lg border border-border/70 bg-card/70 px-3 py-3">
                                  <div className="flex flex-wrap items-center gap-2">
                                    <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Latest attempt</p>
                                    <Badge variant="outline">{unit.latestAttemptStatusLabel}</Badge>
                                  </div>
                                  <p className="mt-2 font-medium text-foreground">{unit.latestAttemptLabel}</p>
                                  <p className="mt-2 text-muted-foreground">{unit.latestAttemptSummary}</p>
                                  <p className="mt-2 text-[11px] text-muted-foreground">{unit.latestAttemptOnlyLabel}</p>
                                  <p className="mt-2 text-[11px] text-muted-foreground">
                                    Updated {formatTimestamp(unit.latestAttemptUpdatedAt)}
                                  </p>
                                </div>

                                <div className="rounded-lg border border-border/70 bg-card/70 px-3 py-3">
                                  <div className="flex flex-wrap items-center gap-2">
                                    <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Workflow</p>
                                    <Badge variant={getRecentAutonomousWorkflowBadgeVariant(unit.workflowState)}>
                                      {unit.workflowStateLabel}
                                    </Badge>
                                  </div>
                                  <p className="mt-2 font-medium text-foreground">{unit.workflowNodeLabel}</p>
                                  <p className="mt-2 text-muted-foreground">{unit.workflowDetail}</p>
                                  <p className="mt-2 text-[11px] text-muted-foreground">{unit.workflowLinkageLabel}</p>
                                </div>

                                <div className="rounded-lg border border-border/70 bg-card/70 px-3 py-3">
                                  <div className="flex flex-wrap items-center gap-2">
                                    <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Evidence</p>
                                    <Badge variant="outline">{unit.evidenceStateLabel}</Badge>
                                  </div>
                                  <p className="mt-2 text-muted-foreground">{unit.evidenceSummary}</p>
                                  <p className="mt-2 text-[11px] text-muted-foreground">
                                    Latest evidence {formatTimestamp(unit.latestEvidenceAt)}
                                  </p>
                                  {unit.evidencePreviews.length > 0 ? (
                                    <ul className="mt-3 space-y-2">
                                      {unit.evidencePreviews.map((artifact) => (
                                        <li key={artifact.artifactId} className="rounded-md border border-border/70 px-2 py-2 text-[11px]">
                                          <div className="flex flex-wrap items-center gap-2">
                                            <Badge variant="outline">{artifact.artifactKindLabel}</Badge>
                                            <Badge variant="outline">{artifact.statusLabel}</Badge>
                                          </div>
                                          <p className="mt-2 text-foreground/85">{artifact.summary}</p>
                                        </li>
                                      ))}
                                    </ul>
                                  ) : null}
                                </div>
                              </div>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <FeedEmptyState
                          title={recentAutonomousUnits.emptyTitle}
                          body={recentAutonomousUnits.emptyBody}
                        />
                      )}
                    </div>
                  </div>
                ) : (
                  <FeedEmptyState
                    body={
                      runtimeSession?.isAuthenticated && runtimeSession.sessionId
                        ? 'No autonomous run has been recorded for the selected project yet. Start one from this pane to populate durable run/unit truth.'
                        : 'Cadence needs an authenticated runtime session with a stable session id before it can create the first autonomous run.'
                    }
                    title="No autonomous run recorded"
                  />
                )}
              </div>
            </section>
            ) : null}

            {hasIncompleteRuntimeRunPayload || renderableRuntimeRun ? (
              <RecoveredRuntimeSection
                canStartRuntimeRun={canStartRuntimeRun}
                canStopRuntimeRun={canStopRuntimeRun}
                hasIncompleteRuntimeRunPayload={hasIncompleteRuntimeRunPayload}
                onStartRuntimeRun={() => void handleStartRuntimeRun()}
                onStopRuntimeRun={() => void handleStopRuntimeRun()}
                pendingRuntimeRunAction={pendingRuntimeRunAction}
                primaryRuntimeRunActionLabel={primaryRuntimeRunActionLabel}
                renderableRuntimeRun={renderableRuntimeRun}
                runtimeRunActionError={runtimeRunActionError}
                runtimeRunActionErrorTitle={runtimeRunActionErrorTitle}
                runtimeRunActionStatus={runtimeRunActionStatus}
                runtimeRunCheckpoints={runtimeRunCheckpoints}
                runtimeRunStatusText={runtimeRunStatusText}
                runtimeRunUnavailableReason={agent.runtimeRunUnavailableReason}
              />
            ) : null}

            {hasAgentFeedSurface ? (
              <AgentFeedSection
                activityItems={activityItems}
                messagesUnavailableReason={agent.messagesUnavailableReason}
                recentRunReplacement={recentRunReplacement}
                showNoRunStreamBanner={showNoRunStreamBanner}
                skillItems={skillItems}
                streamIssue={streamIssue}
                streamRunId={streamRunId}
                streamSequenceLabel={streamSequenceLabel}
                streamSessionLabel={streamSessionLabel}
                streamStatus={streamStatus}
                streamStatusLabel={streamStatusLabel}
                streamStatusMeta={streamStatusMeta}
                toolCalls={toolCalls}
                transcriptItems={transcriptItems}
              />
            ) : null}


            {hasCheckpointControlLoopSurface ? (
              <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
                <div className="flex flex-col gap-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
                        Operator checkpoints
                      </p>
                      <h2 className="mt-2 text-lg font-semibold text-foreground">Checkpoint control loop</h2>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant={pendingApprovals.length > 0 ? 'secondary' : 'outline'}>
                        {pendingApprovals.length} pending
                      </Badge>
                      <Badge variant="outline">{checkpointControlLoop.windowLabel}</Badge>
                      {checkpointControlLoop.isTruncated ? (
                        <Badge variant="secondary">
                          +{checkpointControlLoop.hiddenCount} older action{checkpointControlLoop.hiddenCount === 1 ? '' : 's'}
                        </Badge>
                      ) : null}
                    </div>
                  </div>

                  <p className="text-sm leading-6 text-muted-foreground">
                    Cadence correlates live action-required hints with durable approvals, broker fan-out, resume history, and
                    bounded evidence so the same action and boundary stay traceable from pause to recovery.
                  </p>

                  {operatorActionError ? (
                    <Alert variant="destructive">
                      <AlertCircle className="h-4 w-4" />
                      <AlertTitle>Operator action failed</AlertTitle>
                      <AlertDescription>
                        <p>{operatorActionError.message}</p>
                        <p className="font-mono text-[11px] text-destructive/80">code: {operatorActionError.code}</p>
                      </AlertDescription>
                    </Alert>
                  ) : null}

                  {checkpointControlLoopRecoveryAlert ? (
                    <Alert variant={checkpointControlLoopRecoveryAlert.variant}>
                      {checkpointControlLoopRecoveryAlert.variant === 'destructive' ? (
                        <AlertCircle className="h-4 w-4" />
                      ) : (
                        <ShieldCheck className="h-4 w-4" />
                      )}
                      <AlertTitle>{checkpointControlLoopRecoveryAlert.title}</AlertTitle>
                      <AlertDescription>{checkpointControlLoopRecoveryAlert.body}</AlertDescription>
                    </Alert>
                  ) : null}

                  {checkpointControlLoopCoverageAlert ? (
                    <Alert>
                      <AlertCircle className="h-4 w-4" />
                      <AlertTitle>{checkpointControlLoopCoverageAlert.title}</AlertTitle>
                      <AlertDescription>{checkpointControlLoopCoverageAlert.body}</AlertDescription>
                    </Alert>
                  ) : null}

                  {checkpointControlLoop.items.length > 0 ? (
                    <div className="space-y-3">
                      {checkpointControlLoop.items.map((card) => {
                        const approval = card.approval
                        const answerValue = operatorAnswers[card.actionId] ?? approval?.userAnswer ?? ''
                        const normalizedAnswer = normalizeAnswerInput(answerValue)
                        const requiresAnswer = approval?.requiresUserAnswer ?? false
                        const showAnswerError =
                          Boolean(approval) && requiresAnswer && answerValue.length > 0 && normalizedAnswer.length === 0
                        const actionPending = pendingOperatorIntent?.actionId === card.actionId
                        const resumeMeta = runtimeHelpers.getPerActionResumeStateMeta({
                          card,
                          operatorActionStatus,
                          pendingOperatorActionId,
                          pendingOperatorIntent,
                        })

                        return (
                          <div key={card.key} className="rounded-xl border border-border/70 bg-card/70 p-4">
                            <div className="flex flex-wrap items-start justify-between gap-3">
                              <div>
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="text-sm font-semibold text-foreground">{card.title}</p>
                                  <Badge variant={getCheckpointControlLoopTruthBadgeVariant(card.truthSource)}>
                                    {card.truthSourceLabel}
                                  </Badge>
                                  <Badge variant={getCheckpointControlLoopDurableBadgeVariant(card)}>
                                    {card.durableStateLabel}
                                  </Badge>
                                  <Badge variant={resumeMeta.badgeVariant}>{resumeMeta.label}</Badge>
                                </div>
                                <p className="mt-2 text-sm leading-6 text-muted-foreground">{card.detail}</p>
                                <p className="mt-2 text-[11px] text-muted-foreground">
                                  Action {card.actionId} · Boundary {displayValue(card.boundaryId, 'Pending durable linkage')}
                                </p>
                                {card.gateLinkageLabel ? (
                                  <p className="mt-2 text-[11px] text-muted-foreground">{card.gateLinkageLabel}</p>
                                ) : null}
                                <p className="mt-2 text-[11px] text-muted-foreground">{card.truthSourceDetail}</p>
                              </div>
                            </div>

                            <div className="mt-4 grid gap-3 xl:grid-cols-4">
                              <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Live</p>
                                  <Badge variant={card.liveActionRequired ? 'secondary' : 'outline'}>{card.liveStateLabel}</Badge>
                                </div>
                                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.liveStateDetail}</p>
                                <p className="mt-2 text-[11px] text-muted-foreground">
                                  Updated {formatTimestamp(card.liveUpdatedAt)}
                                </p>
                              </div>

                              <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Resume</p>
                                  <Badge variant={resumeMeta.badgeVariant}>{resumeMeta.label}</Badge>
                                </div>
                                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{resumeMeta.detail}</p>
                                <p className="mt-2 text-[11px] text-muted-foreground">
                                  Updated {formatTimestamp(resumeMeta.timestamp)}
                                </p>
                              </div>

                              <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Broker</p>
                                  <Badge variant={getCheckpointControlLoopBrokerBadgeVariant(card)}>
                                    {card.brokerStateLabel}
                                  </Badge>
                                </div>
                                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.brokerStateDetail}</p>
                                <p className="mt-2 text-[11px] text-muted-foreground">
                                  Updated {formatTimestamp(card.brokerLatestUpdatedAt)}
                                </p>
                                {card.brokerRoutePreviews.length > 0 ? (
                                  <ul className="mt-3 space-y-2">
                                    {card.brokerRoutePreviews.map((route) => (
                                      <li key={`${card.key}:${route.routeId}:${route.updatedAt}`} className="rounded-md border border-border/70 px-2 py-2 text-[11px]">
                                        <div className="flex flex-wrap items-center gap-2">
                                          <Badge variant="outline">{route.routeId}</Badge>
                                          <Badge variant="outline">{route.statusLabel}</Badge>
                                        </div>
                                        <p className="mt-2 text-muted-foreground">{route.detail}</p>
                                      </li>
                                    ))}
                                  </ul>
                                ) : null}
                              </div>

                              <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Evidence</p>
                                  <Badge variant={getCheckpointControlLoopEvidenceBadgeVariant(card)}>
                                    {card.evidenceStateLabel}
                                  </Badge>
                                </div>
                                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.evidenceSummary}</p>
                                <p className="mt-2 text-[11px] text-muted-foreground">
                                  Latest evidence {formatTimestamp(card.latestEvidenceAt)}
                                </p>
                                {card.evidencePreviews.length > 0 ? (
                                  <ul className="mt-3 space-y-2">
                                    {card.evidencePreviews.map((artifact) => (
                                      <li key={artifact.artifactId} className="rounded-md border border-border/70 px-2 py-2 text-[11px]">
                                        <div className="flex flex-wrap items-center gap-2">
                                          <Badge variant="outline">{artifact.artifactKindLabel}</Badge>
                                          <Badge variant="outline">{artifact.statusLabel}</Badge>
                                        </div>
                                        <p className="mt-2 text-foreground/85">{artifact.summary}</p>
                                      </li>
                                    ))}
                                  </ul>
                                ) : null}
                              </div>
                            </div>

                            {approval ? (
                              <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(240px,320px)]">
                                <div className="space-y-3">
                                  <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                    <p className="text-sm font-medium text-foreground">
                                      {requiresAnswer ? 'Required answer contract' : 'Optional answer contract'}
                                    </p>
                                    <p className="mt-2 text-[12px] text-muted-foreground">
                                      <span className="font-medium text-foreground/80">Answer shape:</span> {approval.answerShapeLabel}
                                    </p>
                                    {approval.answerShapeHint ? (
                                      <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{approval.answerShapeHint}</p>
                                    ) : null}
                                  </div>

                                  {approval.isPending || approval.canResume ? (
                                    <label className="grid gap-2 text-[12px] text-muted-foreground">
                                      <span>Operator answer</span>
                                      <Textarea
                                        aria-label={`Operator answer for ${card.actionId}`}
                                        className="min-h-24"
                                        onChange={(event) =>
                                          setOperatorAnswers((currentAnswers) => ({
                                            ...currentAnswers,
                                            [card.actionId]: event.target.value,
                                          }))
                                        }
                                        placeholder={approval.answerPlaceholder ?? 'Provide operator input for this action.'}
                                        value={answerValue}
                                      />
                                      {showAnswerError ? (
                                        <span className="text-destructive">
                                          {approval.answerRequirementReason === 'runtime_resumable'
                                            ? 'A non-empty user answer is required before approving this runtime-resumable request.'
                                            : 'A non-empty user answer is required before approving this action.'}
                                        </span>
                                      ) : null}
                                    </label>
                                  ) : null}
                                </div>

                                <div className="space-y-3 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                  <InfoRow label="Action ID" mono value={card.actionId} />
                                  <InfoRow label="Boundary" mono value={displayValue(card.boundaryId, 'Pending durable linkage')} />
                                  <InfoRow label="Updated" value={formatTimestamp(resumeMeta.timestamp)} />
                                  <p className="text-[12px] leading-5 text-muted-foreground">{resumeMeta.detail}</p>

                                  <div className="flex flex-wrap gap-2">
                                    {approval.isPending ? (
                                      <Button
                                        disabled={actionPending || (requiresAnswer && normalizedAnswer.length === 0)}
                                        onClick={() =>
                                          void handleResolveOperatorAction(card.actionId, 'approve', {
                                            userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : null,
                                          })
                                        }
                                        type="button"
                                      >
                                        {actionPending && pendingOperatorIntent?.kind === 'approve' ? (
                                          <LoaderCircle className="h-4 w-4 animate-spin" />
                                        ) : null}
                                        Approve
                                      </Button>
                                    ) : null}

                                    {approval.isPending ? (
                                      <Button
                                        disabled={actionPending}
                                        onClick={() =>
                                          void handleResolveOperatorAction(card.actionId, 'reject', {
                                            userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : null,
                                          })
                                        }
                                        type="button"
                                        variant="outline"
                                      >
                                        Reject
                                      </Button>
                                    ) : null}

                                    {approval.canResume ? (
                                      <Button
                                        disabled={actionPending}
                                        onClick={() =>
                                          void handleResumeOperatorRun(card.actionId, {
                                            userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : approval.userAnswer ?? null,
                                          })
                                        }
                                        type="button"
                                        variant="secondary"
                                      >
                                        {actionPending && pendingOperatorIntent?.kind === 'resume' ? (
                                          <LoaderCircle className="h-4 w-4 animate-spin" />
                                        ) : null}
                                        Resume run
                                      </Button>
                                    ) : null}
                                  </div>
                                </div>
                              </div>
                            ) : (
                              <div className="mt-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
                                <p className="text-sm font-medium text-foreground">Durable approval row not available</p>
                                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
                                  Cadence is keeping the live, broker, resume, and evidence truth visible for this action even though there is no actionable durable approval row in the current snapshot.
                                </p>
                              </div>
                            )}
                          </div>
                        )
                      })}
                    </div>
                  ) : (
                    <FeedEmptyState title={checkpointControlLoop.emptyTitle} body={checkpointControlLoop.emptyBody} />
                  )}
                </div>
              </section>
            ) : null}

          </div>
          )}
        </div>

        <ComposerDock
          composerModelGroups={composerModelGroups}
          composerModelId={composerModelId}
          composerThinkingLevel={composerThinkingLevel as 'low' | 'medium' | 'high'}
          onComposerModelChange={setComposerModelId}
          onComposerThinkingLevelChange={setComposerThinkingLevel}
          onStartRuntimeRun={() => void handleStartRuntimeRun()}
          placeholder={composerPlaceholder}
          runtimeRunActionStatus={runtimeRunActionStatus}
          showStartRunButton={canStartRuntimeRun && !renderableRuntimeRun}
        />
      </div>
    </div>
  )
}
