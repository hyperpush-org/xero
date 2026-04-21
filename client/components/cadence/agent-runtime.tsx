"use client"

import { useEffect, useMemo, useRef, useState } from 'react'
import { z } from 'zod'
import { openUrl } from '@tauri-apps/plugin-opener'
import type {
  AgentPaneView,
  AgentTrustSignalState,
  AgentTrustSnapshotView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  NotificationRouteKindDto,
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
  composeNotificationRouteTarget,
  decomposeNotificationRouteTarget,
  getRuntimeRunStatusLabel,
  getRuntimeStreamStatusLabel,
  notificationRouteKindSchema,
} from '@/src/lib/cadence-model'
import {
  AlertCircle,
  Bot,
  ExternalLink,
  LoaderCircle,
  LogIn,
  LogOut,
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

const notificationRouteFormSchema = z
  .object({
    routeId: z.string().trim().min(1, 'Route ID is required.'),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1, 'Route target is required.'),
    enabled: z.boolean(),
    metadataJson: z.string().optional().default(''),
  })
  .strict()
  .superRefine((value, ctx) => {
    try {
      composeNotificationRouteTarget(value.routeKind, value.routeTarget)
    } catch (error) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['routeTarget'],
        message: getErrorMessage(
          error,
          'Route target must resolve to the `<kind>:<channel-target>` storage contract before saving.',
        ),
      })
    }

    const metadataText = value.metadataJson.trim()
    if (!metadataText) {
      return
    }

    let parsedMetadata: unknown
    try {
      parsedMetadata = JSON.parse(metadataText)
    } catch {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['metadataJson'],
        message: 'Metadata must be valid JSON.',
      })
      return
    }

    if (typeof parsedMetadata !== 'object' || parsedMetadata === null || Array.isArray(parsedMetadata)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['metadataJson'],
        message: 'Metadata JSON must be an object.',
      })
    }
  })

type NotificationRouteFormValues = z.input<typeof notificationRouteFormSchema>
type NotificationRouteFieldErrorKey = 'routeId' | 'routeKind' | 'routeTarget' | 'metadataJson'
type NotificationRouteFormErrors = Partial<Record<NotificationRouteFieldErrorKey | 'form', string>>

function isNotificationRouteFieldErrorKey(value: unknown): value is NotificationRouteFieldErrorKey {
  return value === 'routeId' || value === 'routeKind' || value === 'routeTarget' || value === 'metadataJson'
}

function getNotificationHealthBadgeVariant(health: AgentPaneView['notificationRoutes'][number]['health']): BadgeVariant {
  switch (health) {
    case 'healthy':
      return 'default'
    case 'pending':
      return 'secondary'
    case 'degraded':
      return 'destructive'
    case 'idle':
    case 'disabled':
      return 'outline'
  }
}

function getTrustSignalBadgeVariant(state: AgentTrustSignalState): BadgeVariant {
  switch (state) {
    case 'healthy':
      return 'default'
    case 'degraded':
      return 'destructive'
    case 'unavailable':
      return 'outline'
  }
}

function getTrustSignalLabel(state: AgentTrustSignalState): string {
  switch (state) {
    case 'healthy':
      return 'Healthy'
    case 'degraded':
      return 'Needs attention'
    case 'unavailable':
      return 'Unavailable'
  }
}

const TRUST_PERMISSION_SCOPE_DETAIL =
  'Packaged desktop permissions stay least-privilege: core runtime, native dialog access, and an auth opener scope limited to https://auth.openai.com/*.'
const TRUST_STORAGE_BOUNDARY_DETAIL =
  'Durable runtime + operator state stays bound to the selected repository, while notification credentials remain app-local and never render raw secret values here.'

function parseRouteFormErrors(error: unknown): NotificationRouteFormErrors {
  if (!(error instanceof z.ZodError)) {
    return {
      form: getErrorMessage(error, 'Cadence could not validate the notification route form.'),
    }
  }

  const nextErrors: NotificationRouteFormErrors = {}
  for (const issue of error.issues) {
    const path = issue.path[0]
    if (isNotificationRouteFieldErrorKey(path)) {
      if (!nextErrors[path]) {
        nextErrors[path] = issue.message
      }
      continue
    }

    if (!nextErrors.form) {
      nextErrors.form = issue.message
    }
  }

  return nextErrors
}

function toNotificationRouteRequest(formValues: NotificationRouteFormValues): Omit<UpsertNotificationRouteRequestDto, 'projectId'> {
  const parsedForm = notificationRouteFormSchema.parse(formValues)
  const metadataText = parsedForm.metadataJson.trim()
  const routeTarget = composeNotificationRouteTarget(parsedForm.routeKind, parsedForm.routeTarget)

  return {
    routeId: parsedForm.routeId,
    routeKind: parsedForm.routeKind,
    routeTarget,
    enabled: parsedForm.enabled,
    metadataJson: metadataText.length > 0 ? metadataText : null,
    updatedAt: new Date().toISOString(),
  }
}

function createDefaultRouteFormValues(routeKind: NotificationRouteKindDto = 'telegram'): NotificationRouteFormValues {
  return {
    routeId: '',
    routeKind,
    routeTarget: '',
    enabled: true,
    metadataJson: '',
  }
}

const NOTIFICATION_ROUTE_KIND_OPTIONS: Array<{
  value: NotificationRouteKindDto
  label: string
  targetPlaceholder: string
  targetHelp: string
}> = [
  {
    value: 'telegram',
    label: 'Telegram',
    targetPlaceholder: '@channel_or_chat_id',
    targetHelp: 'Use a Telegram chat id (for example -100123...) or a channel handle.',
  },
  {
    value: 'discord',
    label: 'Discord',
    targetPlaceholder: 'channel-id',
    targetHelp: 'Use the Discord channel id where route notifications should be delivered.',
  },
]

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
  const selectedProviderId = getSelectedProviderId(agent, runtimeSession)
  const selectedProviderLabel = getSelectedProviderLabel(agent, runtimeSession)
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
  const checkpointControlLoop = agent.checkpointControlLoop ?? createEmptyCheckpointControlLoop()
  const recentAutonomousUnitsAlert = getRecentAutonomousUnitsAlertMeta({
    recentUnits: recentAutonomousUnits,
    runtimeStream: agent.runtimeStream ?? null,
    messagesUnavailableReason: agent.messagesUnavailableReason,
  })
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
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
    () => sortByNewest(renderableRuntimeRun?.checkpoints ?? [], (checkpoint) => checkpoint.createdAt).slice(0, 4),
    [renderableRuntimeRun],
  )
  const streamStatusLabel = displayValue(agent.runtimeStreamStatusLabel, getRuntimeStreamStatusLabel(streamStatus))
  const streamIssue: RuntimeStreamIssueView | null = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const approvalRequests = agent.approvalRequests ?? []
  const resumeHistory = agent.resumeHistory ?? []
  const notificationRoutes = agent.notificationRoutes ?? []
  const notificationChannelHealth = agent.notificationChannelHealth ?? []
  const notificationRouteLoadStatus = agent.notificationRouteLoadStatus ?? 'idle'
  const notificationRouteError = agent.notificationRouteError ?? null
  const notificationSyncSummary = agent.notificationSyncSummary ?? null
  const notificationSyncError = agent.notificationSyncError ?? null
  const notificationSyncPollingActive = agent.notificationSyncPollingActive ?? false
  const notificationSyncPollingActionId = agent.notificationSyncPollingActionId ?? null
  const notificationSyncPollingBoundaryId = agent.notificationSyncPollingBoundaryId ?? null
  const notificationRouteMutationStatus = agent.notificationRouteMutationStatus ?? 'idle'
  const pendingNotificationRouteId = agent.pendingNotificationRouteId ?? null
  const notificationRouteMutationError = agent.notificationRouteMutationError ?? null
  const trustSnapshot = useMemo<AgentTrustSnapshotView>(() => {
    if (agent.trustSnapshot) {
      return agent.trustSnapshot
    }

    const enabledRoutes = notificationRoutes.filter((route) => route.enabled)
    const degradedRouteCount = notificationRoutes.filter(
      (route) => route.health === 'degraded' || route.health === 'pending',
    ).length
    const pendingApprovalCount = agent.pendingApprovalCount ?? approvalRequests.filter((approval) => approval.isPending).length

    let readyCredentialRouteCount = 0
    let missingCredentialRouteCount = 0
    let malformedCredentialRouteCount = 0
    let unavailableCredentialRouteCount = 0

    for (const route of enabledRoutes) {
      const readinessStatus = route.credentialReadiness?.status ?? 'unavailable'
      switch (readinessStatus) {
        case 'ready':
          readyCredentialRouteCount += 1
          break
        case 'missing':
          missingCredentialRouteCount += 1
          break
        case 'malformed':
          malformedCredentialRouteCount += 1
          break
        case 'unavailable':
        default:
          unavailableCredentialRouteCount += 1
      }
    }

    const syncDispatchFailedCount = notificationSyncSummary?.dispatch.failedCount ?? 0
    const syncReplyRejectedCount = notificationSyncSummary?.replies.rejectedCount ?? 0
    const approvalsState: AgentTrustSignalState = pendingApprovalCount > 0 ? 'degraded' : 'healthy'
    const routesState: AgentTrustSignalState = notificationRouteError ? 'degraded' : 'unavailable'
    const syncState: AgentTrustSignalState = notificationSyncError ? 'degraded' : 'unavailable'
    const state: AgentTrustSignalState =
      notificationRouteError || notificationSyncError || pendingApprovalCount > 0 ? 'degraded' : 'unavailable'

    return {
      state,
      stateLabel: getTrustSignalLabel(state),
      runtimeState: 'unavailable',
      runtimeReason: agent.runtimeRunUnavailableReason,
      streamState: 'unavailable',
      streamReason: agent.messagesUnavailableReason,
      approvalsState,
      approvalsReason:
        pendingApprovalCount > 0
          ? `There are ${pendingApprovalCount} pending operator approval gate(s) waiting for action.`
          : 'No pending operator approvals are blocking autonomous continuation.',
      routesState,
      routesReason: notificationRouteError
        ? notificationRouteError.message
        : notificationRoutes.length === 0
          ? 'No notification routes are configured for the selected project.'
          : 'Cadence is keeping route counts visible, but hook-owned trust projection is unavailable.',
      credentialsState: enabledRoutes.length === 0 ? 'unavailable' : 'degraded',
      credentialsReason: enabledRoutes.length === 0
        ? 'No enabled routes require app-local credential readiness checks.'
        : 'Cadence is keeping credential-readiness counts visible, but hook-owned trust projection is unavailable.',
      syncState,
      syncReason: notificationSyncError
        ? notificationSyncError.message
        : notificationSyncSummary
          ? 'Cadence is keeping the last observed sync counts visible, but hook-owned trust projection is unavailable.'
          : 'No notification adapter sync summary is available yet.',
      routeCount: notificationRoutes.length,
      enabledRouteCount: enabledRoutes.length,
      degradedRouteCount,
      readyCredentialRouteCount,
      missingCredentialRouteCount,
      malformedCredentialRouteCount,
      unavailableCredentialRouteCount,
      pendingApprovalCount,
      syncDispatchFailedCount,
      syncReplyRejectedCount,
      routeError: notificationRouteError,
      syncError: notificationSyncError,
      projectionError: {
        code: 'trust_snapshot_missing',
        message:
          'Cadence did not receive a hook-composed trust snapshot for this view. Showing fail-closed trust state until the selected-project projection recovers.',
        retryable: true,
      },
    }
  }, [
    agent.messagesUnavailableReason,
    agent.pendingApprovalCount,
    agent.runtimeRunUnavailableReason,
    agent.trustSnapshot,
    approvalRequests,
    notificationRouteError,
    notificationRoutes,
    notificationSyncError,
    notificationSyncSummary,
  ])
  const checkpointControlLoopRecoveryAlert = getCheckpointControlLoopRecoveryAlertMeta({
    controlLoop: checkpointControlLoop,
    trustSnapshot,
    autonomousRunErrorMessage,
    notificationSyncPollingActive,
    notificationSyncPollingActionId,
    notificationSyncPollingBoundaryId,
  })
  const checkpointControlLoopCoverageAlert = getCheckpointControlLoopCoverageAlertMeta(checkpointControlLoop)
  const [manualInput, setManualInput] = useState('')
  const [pendingAction, setPendingAction] = useState<PendingAction>(null)
  const [actionMessage, setActionMessage] = useState<string | null>(null)
  const [autonomousRunActionMessage, setAutonomousRunActionMessage] = useState<string | null>(null)
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [routePanelMessage, setRoutePanelMessage] = useState<string | null>(null)
  const [browserMessage, setBrowserMessage] = useState<string | null>(null)
  const [operatorAnswers, setOperatorAnswers] = useState<Record<string, string>>({})
  const [routeFormValues, setRouteFormValues] = useState<NotificationRouteFormValues>(() => createDefaultRouteFormValues())
  const [routeFormErrors, setRouteFormErrors] = useState<NotificationRouteFormErrors>({})
  const [editingRouteId, setEditingRouteId] = useState<string | null>(null)
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

  const selectedProviderId = getSelectedProviderId(agent, runtimeSession)
  const selectedProviderLabel = getSelectedProviderLabel(agent, runtimeSession)
  const selectedModelId = displayValue(agent.selectedModelId, selectedProviderId === 'openrouter' ? 'Model not configured' : 'openai_codex')
  const composerModelGroups = useMemo(
    () => getComposerModelGroups(selectedProviderId, selectedProviderLabel, selectedModelId),
    [selectedModelId, selectedProviderId, selectedProviderLabel],
  )
  const [composerModelId, setComposerModelId] = useState(selectedModelId)
  const [composerThinkingLevel, setComposerThinkingLevel] = useState<ComposerThinkingLevelOption['value']>('medium')
  const isOpenRouterSelected = selectedProviderId === 'openrouter'
  const isOpenAiSelected = selectedProviderId === 'openai_codex'
  const openrouterApiKeyConfigured = agent.openrouterApiKeyConfigured ?? false
  const providerMismatch = agent.providerMismatch ?? false
  const streamStatusMeta = useMemo(() => getStreamStatusMeta(agent, runtimeSession), [agent, runtimeSession])
  const repositoryPath = displayValue(agent.repositoryPath, 'No repository path available')
  const repositoryLabel = displayValue(agent.repositoryLabel, agent.project.name)
  const sessionLabel = displayValue(runtimeSession?.sessionLabel, 'No session')
  const streamRunId = getStreamRunId(runtimeStream, renderableRuntimeRun)
  const streamSequenceLabel = formatSequence(runtimeStream?.lastSequence ?? null)
  const streamSessionLabel = displayValue(runtimeStream?.sessionId, runtimeSession?.sessionLabel ?? 'No session')
  const hasStreamRunMismatch = Boolean(runtimeStream?.runId && renderableRuntimeRun && runtimeStream.runId !== renderableRuntimeRun.runId)
  const hasAttachedRun = Boolean(renderableRuntimeRun)
  const showNoRunStreamBanner = Boolean(runtimeSession?.isAuthenticated && !hasAttachedRun)
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const canStartLogin = hasRepositoryBinding && isOpenAiSelected && typeof onStartLogin === 'function'
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
  const canResumeRuntimeSession =
    hasRepositoryBinding && typeof onStartRuntimeSession === 'function' && (!isOpenRouterSelected || openrouterApiKeyConfigured)
  const canSubmitManualInput = hasRepositoryBinding && isOpenAiSelected && typeof onSubmitManualCallback === 'function'
  const canStopRuntimeRun = Boolean(
    hasRepositoryBinding && renderableRuntimeRun && !renderableRuntimeRun.isTerminal && typeof onStopRuntimeRun === 'function',
  )
  const canLogout = hasRepositoryBinding && typeof onLogout === 'function'
  const canRetryStream = Boolean(hasRepositoryBinding && runtimeSession?.isAuthenticated && typeof onRetryStream === 'function')
  const canResolveOperatorActions = hasRepositoryBinding && typeof onResolveOperatorAction === 'function'
  const canResumeOperatorRuns = hasRepositoryBinding && typeof onResumeOperatorRun === 'function'
  const canRefreshNotificationRoutes = hasRepositoryBinding && typeof onRefreshNotificationRoutes === 'function'
  const canMutateNotificationRoutes = hasRepositoryBinding && typeof onUpsertNotificationRoute === 'function'
  const hasAuthorizationUrl = Boolean(runtimeSession?.authorizationUrl)
  const hasActiveFlow = Boolean(runtimeSession?.flowId)
  const showManualFallback = Boolean(
    isOpenAiSelected && (runtimeSession?.isLoginInProgress || hasAuthorizationUrl || browserMessage || runtimeSession?.needsManualInput),
  )
  const showReuseButton = !runtimeSession || runtimeSession.isSignedOut || runtimeSession.isFailed || providerMismatch
  const showLogoutButton = Boolean(runtimeSession && !runtimeSession.isLoginInProgress && !runtimeSession.isSignedOut)
  const runtimeSessionActionLabel = isOpenRouterSelected
    ? providerMismatch || runtimeSession?.isAuthenticated
      ? 'Rebind OpenRouter runtime'
      : 'Bind OpenRouter runtime'
    : 'Reuse app-local runtime session'
  const composerPlaceholder = getComposerPlaceholder(runtimeSession, streamStatus, renderableRuntimeRun, streamRunId, {
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
  const showRemoteEscalationPanel = false
  const sortedApprovals = useMemo(
    () => sortByNewest(approvalRequests, (approval) => approval.updatedAt ?? approval.createdAt).slice(0, 6),
    [approvalRequests],
  )
  const pendingApprovals = useMemo(() => sortedApprovals.filter((approval) => approval.isPending), [sortedApprovals])
  const runtimeRunStatusText = getRuntimeRunStatusText(renderableRuntimeRun)
  const primaryRuntimeRunActionLabel = getPrimaryRuntimeRunActionLabel(renderableRuntimeRun)
  const autonomousRunActionErrorTitle =
    autonomousRunActionError?.retryable || autonomousRunActionError?.code.includes('timeout')
      ? 'Autonomous run control needs retry'
      : 'Autonomous run control failed'
  const runtimeRunActionErrorTitle =
    runtimeRunActionError?.retryable || runtimeRunActionError?.code.includes('timeout')
      ? 'Run control needs retry'
      : 'Run control failed'
  const routeKindOption =
    NOTIFICATION_ROUTE_KIND_OPTIONS.find((option) => option.value === routeFormValues.routeKind) ??
    NOTIFICATION_ROUTE_KIND_OPTIONS[0]
  const isRouteListLoading = notificationRouteLoadStatus === 'loading' && notificationRoutes.length === 0
  const isRouteListRefreshing = agent.notificationRouteIsRefreshing || (notificationRouteLoadStatus === 'loading' && notificationRoutes.length > 0)
  const isRouteMutationRunning = notificationRouteMutationStatus === 'running'
  const trustSignalCards = useMemo(
    () => [
      {
        id: 'runtime',
        title: 'Run status',
        state: trustSnapshot.runtimeState,
        summary: runtimeRunStatusText,
        reason: trustSnapshot.runtimeReason,
      },
      {
        id: 'stream',
        title: 'Live feed',
        state: trustSnapshot.streamState,
        summary: streamStatusLabel,
        reason: trustSnapshot.streamReason,
      },
      {
        id: 'approvals',
        title: 'Approval backlog',
        state: trustSnapshot.approvalsState,
        summary: `${trustSnapshot.pendingApprovalCount} pending gate${trustSnapshot.pendingApprovalCount === 1 ? '' : 's'}`,
        reason: trustSnapshot.approvalsReason,
      },
      {
        id: 'routes',
        title: 'Route health',
        state: trustSnapshot.routesState,
        summary: `${trustSnapshot.routeCount} total · ${trustSnapshot.degradedRouteCount} degraded/pending`,
        reason: trustSnapshot.routesReason,
      },
      {
        id: 'credentials',
        title: 'Credential readiness',
        state: trustSnapshot.credentialsState,
        summary: `${trustSnapshot.readyCredentialRouteCount} ready · ${trustSnapshot.missingCredentialRouteCount} missing · ${trustSnapshot.malformedCredentialRouteCount} malformed`,
        reason: trustSnapshot.credentialsReason,
      },
      {
        id: 'sync',
        title: 'Route sync',
        state: trustSnapshot.syncState,
        summary: `${trustSnapshot.syncDispatchFailedCount} dispatch failures · ${trustSnapshot.syncReplyRejectedCount} rejected replies`,
        reason: trustSnapshot.syncReason,
      },
    ],
    [streamStatusLabel, trustSnapshot, runtimeRunStatusText],
  )
  const trustPrimaryErrorCode =
    trustSnapshot.projectionError?.code ??
    trustSnapshot.routeError?.code ??
    trustSnapshot.syncError?.code ??
    runtimeRunActionError?.code ??
    streamIssue?.code ??
    runtimeSession?.lastErrorCode ??
    renderableRuntimeRun?.lastErrorCode ??
    null
  const trustActionApprovals = useMemo(
    () => sortedApprovals.filter((approval) => approval.isPending || approval.canResume).slice(0, 2),
    [sortedApprovals],
  )
  const trustHiddenActionCount = Math.max(
    sortedApprovals.filter((approval) => approval.isPending || approval.canResume).length - trustActionApprovals.length,
    0,
  )
  const trustPrimaryIssue = useMemo(() => {
    if (trustSnapshot.projectionError) {
      return {
        title: 'Showing last truthful trust snapshot',
        message: trustSnapshot.projectionError.message,
        code: trustSnapshot.projectionError.code,
        destructive: true,
      }
    }

    if (providerMismatch) {
      return {
        title: 'Provider mismatch',
        message: agent.sessionUnavailableReason,
        code: null,
        destructive: true,
      }
    }

    if (runtimeRunActionError) {
      return {
        title: runtimeRunActionErrorTitle,
        message: runtimeRunActionError.message,
        code: runtimeRunActionError.code,
        destructive: true,
      }
    }

    if (streamIssue) {
      return {
        title: 'Live feed failed',
        message: streamIssue.message,
        code: streamIssue.code,
        destructive: true,
      }
    }

    if (runtimeSession?.lastError) {
      return {
        title: 'Runtime session needs attention',
        message: runtimeSession.lastError.message,
        code: runtimeSession.lastErrorCode,
        destructive: true,
      }
    }

    if (renderableRuntimeRun?.lastError) {
      return {
        title: 'Recovered run needs attention',
        message: renderableRuntimeRun.lastError.message,
        code: renderableRuntimeRun.lastErrorCode,
        destructive: true,
      }
    }

    if (trustSnapshot.approvalsState === 'degraded') {
      return {
        title: 'Operator approval required',
        message: trustSnapshot.approvalsReason,
        code: null,
        destructive: false,
      }
    }

    if (notificationSyncPollingActive) {
      return {
        title: trustSnapshot.syncState === 'degraded' ? 'Showing last truthful route sync' : 'Route sync polling active',
        message:
          trustSnapshot.syncState === 'degraded'
            ? `Cadence is still polling configured routes for blocked boundary ${displayValue(notificationSyncPollingBoundaryId, 'unknown')} and action ${displayValue(notificationSyncPollingActionId, 'unknown')} while preserving the last truthful sync summary. ${trustSnapshot.syncReason}`
            : `Cadence is polling configured routes for blocked boundary ${displayValue(notificationSyncPollingBoundaryId, 'unknown')} and pending operator action ${displayValue(notificationSyncPollingActionId, 'unknown')}.`,
        code: trustSnapshot.syncError?.code ?? null,
        destructive: trustSnapshot.syncState === 'degraded',
      }
    }

    if (trustSnapshot.runtimeState !== 'healthy') {
      return {
        title: 'Run status needs attention',
        message: trustSnapshot.runtimeReason,
        code: trustPrimaryErrorCode,
        destructive: false,
      }
    }

    if (trustSnapshot.streamState !== 'healthy') {
      return {
        title: 'Live feed needs attention',
        message: trustSnapshot.streamReason,
        code: trustPrimaryErrorCode,
        destructive: false,
      }
    }

    if (trustSnapshot.routesState !== 'healthy') {
      return {
        title: 'Route health needs attention',
        message: trustSnapshot.routesReason,
        code: trustSnapshot.routeError?.code ?? null,
        destructive: false,
      }
    }

    if (trustSnapshot.credentialsState !== 'healthy') {
      return {
        title: 'Credential readiness needs attention',
        message: trustSnapshot.credentialsReason,
        code: null,
        destructive: false,
      }
    }

    if (trustSnapshot.syncState !== 'healthy') {
      return {
        title: 'Route sync needs attention',
        message: trustSnapshot.syncReason,
        code: trustSnapshot.syncError?.code ?? null,
        destructive: false,
      }
    }

    return null
  }, [
    agent.sessionUnavailableReason,
    notificationSyncPollingActionId,
    notificationSyncPollingActive,
    notificationSyncPollingBoundaryId,
    providerMismatch,
    renderableRuntimeRun?.lastError,
    renderableRuntimeRun?.lastErrorCode,
    runtimeRunActionError,
    runtimeRunActionErrorTitle,
    runtimeSession?.lastError,
    runtimeSession?.lastErrorCode,
    streamIssue,
    trustPrimaryErrorCode,
    trustSnapshot,
  ])
  const trustRecoveryActions = useMemo(() => {
    const nextActions: string[] = []

    if (trustSnapshot.runtimeState !== 'healthy') {
      if (isOpenRouterSelected) {
        if (!openrouterApiKeyConfigured) {
          nextActions.push('Configure the OpenRouter API key in Settings before trusting autonomous execution.')
        } else if (!runtimeSession?.isAuthenticated || providerMismatch) {
          nextActions.push('Bind or rebind OpenRouter so provider identity and diagnostics reflect the selected Settings provider.')
        }
      } else if (!runtimeSession?.isAuthenticated) {
        nextActions.push('Sign in with OpenAI before trusting autonomous execution.')
      }

      if (!renderableRuntimeRun || renderableRuntimeRun.isStale || renderableRuntimeRun.isFailed) {
        nextActions.push('Start or reconnect the supervised run so runtime liveness is durable and current.')
      }
    }

    if (trustSnapshot.streamState !== 'healthy') {
      nextActions.push('Retry the live feed after runtime reconnection so run-scoped telemetry is current.')
    }

    if (trustSnapshot.approvalsState === 'degraded') {
      nextActions.push('Resolve pending operator approvals so autonomous continuation is no longer blocked.')
    }

    if (trustSnapshot.credentialsState === 'degraded') {
      nextActions.push('Configure missing or malformed app-local route credentials in Settings before dispatch.')
    }

    if (trustSnapshot.routesState !== 'healthy' || trustSnapshot.syncState !== 'healthy') {
      nextActions.push('Refresh route health from the Agent tab after credential updates.')
    }

    if (trustPrimaryErrorCode) {
      nextActions.push(`Inspect error code ${trustPrimaryErrorCode} before considering this project trust state healthy.`)
    }

    return Array.from(new Set(nextActions))
  }, [
    isOpenRouterSelected,
    openrouterApiKeyConfigured,
    providerMismatch,
    renderableRuntimeRun,
    runtimeSession?.isAuthenticated,
    trustPrimaryErrorCode,
    trustSnapshot,
  ])
  const hasTrustRecoveryActions = trustRecoveryActions.length > 0
  const hasNotificationTrustSurface =
    notificationSyncPollingActive ||
    notificationRoutes.length > 0 ||
    notificationChannelHealth.length > 0 ||
    notificationSyncSummary !== null ||
    notificationSyncError !== null ||
    trustSnapshot.pendingApprovalCount > 0
  const showTrustSurface =
    runtimeSession !== null ||
    renderableRuntimeRun !== null ||
    streamIssue !== null ||
    providerMismatch ||
    trustSnapshot.projectionError !== null ||
    hasNotificationTrustSurface
  const latestSyncTimestampLabel = notificationSyncSummary
    ? formatTimestamp(notificationSyncSummary.syncedAt)
    : 'No successful sync recorded yet.'

  useEffect(() => {
    setComposerModelId(selectedModelId)
  }, [selectedModelId, selectedProviderId])

  useEffect(() => {
    setActionMessage(null)

    if (runtimeSession?.isAuthenticated) {
      setManualInput('')
      setBrowserMessage(null)
      return
    }

    if (!runtimeSession?.flowId) {
      setManualInput('')
      setBrowserMessage(null)
    }
  }, [runtimeSession?.flowId, runtimeSession?.isAuthenticated, runtimeSession?.updatedAt])

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
    setRouteFormValues(createDefaultRouteFormValues())
    setRouteFormErrors({})
    setRoutePanelMessage(null)
    setEditingRouteId(null)
  }, [agent.project.id])

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

  async function openAuthorizationUrl(url: string) {
    try {
      await openUrl(url)
      setBrowserMessage(null)
    } catch (error) {
      setBrowserMessage(
        getErrorMessage(
          error,
          'Cadence could not open the system browser automatically. Use the authorization URL below and finish login with the pasted redirect fallback.',
        ),
      )
    }
  }

  async function handleStartLogin() {
    if (!canStartLogin || !onStartLogin) {
      return
    }

    setPendingAction('login')
    setActionMessage(null)
    setBrowserMessage(null)

    try {
      const nextRuntime = await onStartLogin()
      if (!nextRuntime?.authorizationUrl) {
        setBrowserMessage(
          'Cadence started the OpenAI login flow, but the authorization URL was missing. Start login again to create a fresh flow.',
        )
        return
      }

      await openAuthorizationUrl(nextRuntime.authorizationUrl)
    } catch (error) {
      setActionMessage(getErrorMessage(error, 'Cadence could not start the OpenAI login flow for this project.'))
    } finally {
      setPendingAction(null)
    }
  }

  async function handleOpenBrowserAgain() {
    const url = runtimeSession?.authorizationUrl
    if (!url) {
      setBrowserMessage('Cadence no longer has an authorization URL for this login flow. Start login again.')
      return
    }

    setPendingAction('browser')
    setActionMessage(null)

    try {
      await openAuthorizationUrl(url)
    } finally {
      setPendingAction(null)
    }
  }

  async function handleResumeRuntimeSession() {
    if (!canResumeRuntimeSession || !onStartRuntimeSession) {
      return
    }

    setPendingAction('reuse')
    setActionMessage(null)

    try {
      await onStartRuntimeSession()
    } catch (error) {
      setActionMessage(
        getErrorMessage(
          error,
          isOpenRouterSelected
            ? 'Cadence could not bind or rebind the OpenRouter runtime for this project.'
            : 'Cadence could not reuse the app-local runtime session for this project.',
        ),
      )
    } finally {
      setPendingAction(null)
    }
  }

  async function handleStartAutonomousRun() {
    if (!canStartAutonomousRun || !onStartAutonomousRun) {
      return
    }

    setAutonomousRunActionMessage(null)

    try {
      await onStartAutonomousRun()
    } catch (error) {
      setAutonomousRunActionMessage(
        getErrorMessage(error, 'Cadence could not start the autonomous run for this project.'),
      )
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
      setAutonomousRunActionMessage(
        getErrorMessage(error, 'Cadence could not inspect the autonomous run truth for this project.'),
      )
    }
  }

  async function handleCancelAutonomousRun() {
    if (!canCancelAutonomousRun || !onCancelAutonomousRun || !autonomousRun) {
      return
    }

    setAutonomousRunActionMessage(null)

    try {
      await onCancelAutonomousRun(autonomousRun.runId)
    } catch (error) {
      setAutonomousRunActionMessage(
        getErrorMessage(error, 'Cadence could not cancel the autonomous run for this project.'),
      )
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
      setRuntimeRunActionMessage(
        getErrorMessage(error, 'Cadence could not start or reconnect the supervised runtime run for this project.'),
      )
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
      setRuntimeRunActionMessage(
        getErrorMessage(error, 'Cadence could not stop the supervised runtime run for this project.'),
      )
    }
  }

  async function handleSubmitManualCallback() {
    if (!canSubmitManualInput || !onSubmitManualCallback) {
      return
    }

    const trimmedInput = manualInput.trim()
    if (!trimmedInput) {
      setActionMessage('Paste the full OpenAI redirect URL before submitting the manual fallback.')
      return
    }

    if (!runtimeSession?.flowId) {
      setActionMessage('Cadence no longer has an active OpenAI login flow for this project. Start login again.')
      return
    }

    setPendingAction('manual')
    setActionMessage(null)

    try {
      await onSubmitManualCallback(runtimeSession.flowId, trimmedInput)
      setManualInput('')
      setBrowserMessage(null)
    } catch (error) {
      setActionMessage(getErrorMessage(error, 'Cadence could not complete the pasted OpenAI redirect URL.'))
    } finally {
      setPendingAction(null)
    }
  }

  async function handleLogout() {
    if (!canLogout || !onLogout) {
      return
    }

    setPendingAction('logout')
    setActionMessage(null)
    setBrowserMessage(null)

    try {
      await onLogout()
    } catch (error) {
      setActionMessage(
        getErrorMessage(
          error,
          isOpenRouterSelected
            ? 'Cadence could not clear the OpenRouter runtime binding for this project.'
            : 'Cadence could not remove the OpenAI runtime session for this project.',
        ),
      )
    } finally {
      setPendingAction(null)
    }
  }

  async function handleRetryStream() {
    if (!canRetryStream || !onRetryStream) {
      return
    }

    setPendingAction('retry_stream')
    setActionMessage(null)

    try {
      await onRetryStream()
    } catch (error) {
      setActionMessage(getErrorMessage(error, 'Cadence could not retry the live runtime stream for this project.'))
    } finally {
      setPendingAction(null)
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

  async function handleRefreshNotificationRoutes(options: { force?: boolean } = {}) {
    if (!canRefreshNotificationRoutes || !onRefreshNotificationRoutes) {
      return
    }

    setPendingAction('refresh_routes')
    setRoutePanelMessage(null)

    try {
      await onRefreshNotificationRoutes({ force: options.force ?? true })
    } catch (error) {
      setRoutePanelMessage(getErrorMessage(error, 'Cadence could not refresh notification route health for this project.'))
    } finally {
      setPendingAction(null)
    }
  }

  function handleRouteFieldChange<Field extends keyof NotificationRouteFormValues>(
    field: Field,
    value: NotificationRouteFormValues[Field],
  ) {
    setRouteFormValues((currentValues) => ({
      ...currentValues,
      [field]: value,
    }))

    setRouteFormErrors((currentErrors) => {
      const errorField: NotificationRouteFieldErrorKey | null = isNotificationRouteFieldErrorKey(field) ? field : null

      if ((!errorField || !currentErrors[errorField]) && !currentErrors.form) {
        return currentErrors
      }

      const nextErrors = { ...currentErrors }
      if (errorField) {
        delete nextErrors[errorField]
      }
      delete nextErrors.form
      return nextErrors
    })
  }

  function handleRouteKindChange(nextRouteKind: string) {
    const routeKind = NOTIFICATION_ROUTE_KIND_OPTIONS.find((option) => option.value === nextRouteKind)?.value

    if (!routeKind) {
      setRouteFormErrors((currentErrors) => ({
        ...currentErrors,
        routeKind: 'Route kind must be Telegram or Discord.',
      }))
      return
    }

    handleRouteFieldChange('routeKind', routeKind)
  }

  function handleEditRoute(route: AgentPaneView['notificationRoutes'][number]) {
    let routeTargetValue = route.routeTarget
    let routeTargetError: string | null = null

    try {
      routeTargetValue = decomposeNotificationRouteTarget(route.routeKind, route.routeTarget).channelTarget
    } catch (error) {
      routeTargetError = getErrorMessage(
        error,
        'Saved route target is malformed. Provide a channel target so Cadence can persist canonical `<kind>:<channel-target>` values.',
      )
    }

    setEditingRouteId(route.routeId)
    setRouteFormValues({
      routeId: route.routeId,
      routeKind: route.routeKind,
      routeTarget: routeTargetValue,
      enabled: route.enabled,
      metadataJson: route.metadataJson ?? '',
    })
    setRouteFormErrors(routeTargetError ? { routeTarget: routeTargetError } : {})
    setRoutePanelMessage(null)
  }

  function handleStartNewRoute(routeKind: NotificationRouteKindDto = routeFormValues.routeKind) {
    setEditingRouteId(null)
    setRouteFormValues(createDefaultRouteFormValues(routeKind))
    setRouteFormErrors({})
    setRoutePanelMessage(null)
  }

  async function handleSaveNotificationRoute() {
    if (!canMutateNotificationRoutes || !onUpsertNotificationRoute) {
      return
    }

    let request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>
    try {
      request = toNotificationRouteRequest(routeFormValues)
      setRouteFormErrors({})
    } catch (error) {
      setRouteFormErrors(parseRouteFormErrors(error))
      return
    }

    setPendingAction('save_route')
    setRoutePanelMessage(null)

    try {
      await onUpsertNotificationRoute(request)
      setEditingRouteId(request.routeId)
      setRoutePanelMessage(`Saved route ${request.routeId} (${request.routeKind}).`)
    } catch (error) {
      setRoutePanelMessage(getErrorMessage(error, 'Cadence could not save this notification route.'))
    } finally {
      setPendingAction(null)
    }
  }

  async function handleToggleRoute(route: AgentPaneView['notificationRoutes'][number]) {
    if (!canMutateNotificationRoutes || !onUpsertNotificationRoute) {
      return
    }

    setPendingAction('toggle_route')
    setRoutePanelMessage(null)

    try {
      await onUpsertNotificationRoute({
        routeId: route.routeId,
        routeKind: route.routeKind,
        routeTarget: route.routeTarget,
        enabled: !route.enabled,
        metadataJson: route.metadataJson ?? null,
        updatedAt: new Date().toISOString(),
      })
      setRoutePanelMessage(`${route.routeId} ${route.enabled ? 'disabled' : 'enabled'}.`)
    } catch (error) {
      setRoutePanelMessage(getErrorMessage(error, `Cadence could not update route ${route.routeId}.`))
    } finally {
      setPendingAction(null)
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
            <CenteredEmptyState
              description="Open Settings to choose a provider and model before using the agent tab for this imported project."
              icon={Bot}
              title="Configure agent runtime"
              action={
                onOpenSettings ? (
                  <div className="flex flex-wrap items-center justify-center gap-2">
                    <Button onClick={onOpenSettings} type="button">
                      Configure
                    </Button>
                  </div>
                ) : undefined
              }
            />
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
              <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
                <div className="flex flex-col gap-4">
                  {hasIncompleteRuntimeRunPayload ? (
                    <>
                      <div className="flex flex-wrap items-center gap-2">
                        <h2 className="text-lg font-semibold text-foreground">Durable run snapshot unavailable</h2>
                        <Badge variant="destructive">Unavailable</Badge>
                      </div>
                      <p className="text-sm leading-6 text-muted-foreground">Durable run snapshot is incomplete</p>
                    </>
                  ) : renderableRuntimeRun ? (
                    <>
                      <div className="flex flex-wrap items-center justify-between gap-3">
                        <div>
                          <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Durable runtime</p>
                          <h2 className="mt-2 text-lg font-semibold text-foreground">Recovered run snapshot</h2>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant={getRuntimeRunBadgeVariant(renderableRuntimeRun)}>{runtimeRunStatusText}</Badge>
                          <Badge variant="outline">{displayValue(renderableRuntimeRun.statusLabel, runtimeRunStatusText)}</Badge>
                        </div>
                      </div>

                      <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                        <h3 className="text-base font-semibold text-foreground">
                          {renderableRuntimeRun.isStale
                            ? 'Supervisor heartbeat is stale'
                            : renderableRuntimeRun.isTerminal
                              ? 'Supervisor stopped cleanly'
                              : 'Recovered run snapshot'}
                        </h3>
                        <p className="mt-2 text-sm leading-6 text-muted-foreground">{agent.runtimeRunUnavailableReason}</p>
                        <div className="mt-4 grid gap-3 sm:grid-cols-2">
                          <CountCard label="Run ID" value={renderableRuntimeRun.runId} />
                          <CountCard label="Checkpoint count" value={String(renderableRuntimeRun.checkpointCount)} />
                        </div>
                      </div>

                      {runtimeRunActionError ? (
                        <Alert variant="destructive">
                          <AlertCircle className="h-4 w-4" />
                          <AlertTitle>{runtimeRunActionErrorTitle}</AlertTitle>
                          <AlertDescription>
                            <p>{runtimeRunActionError.message}</p>
                            {runtimeRunActionError.code ? (
                              <p className="font-mono text-[11px] text-destructive/80">code: {runtimeRunActionError.code}</p>
                            ) : null}
                          </AlertDescription>
                        </Alert>
                      ) : null}

                      <div className="flex flex-wrap gap-2">
                        {canStartRuntimeRun ? (
                          <Button disabled={runtimeRunActionStatus === 'running'} onClick={() => void handleStartRuntimeRun()} type="button">
                            {runtimeRunActionStatus === 'running' && pendingRuntimeRunAction !== 'stop' ? (
                              <LoaderCircle className="h-4 w-4 animate-spin" />
                            ) : (
                              <Play className="h-4 w-4" />
                            )}
                            {primaryRuntimeRunActionLabel}
                          </Button>
                        ) : null}

                        {canStopRuntimeRun ? (
                          <Button disabled={runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'stop'} onClick={() => void handleStopRuntimeRun()} type="button" variant="outline">
                            {runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'stop' ? (
                              <LoaderCircle className="h-4 w-4 animate-spin" />
                            ) : (
                              <XCircle className="h-4 w-4" />
                            )}
                            Stop run
                          </Button>
                        ) : null}
                      </div>

                      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
                        {runtimeRunCheckpoints.length > 0 ? (
                          runtimeRunCheckpoints.map((checkpoint) => (
                            <div key={`${checkpoint.kind}-${checkpoint.sequence}-${checkpoint.createdAt}`} className="rounded-xl border border-border/70 bg-card/70 p-4">
                              <div className="flex flex-wrap items-center gap-2">
                                <Badge variant="outline">{formatSequence(checkpoint.sequence)}</Badge>
                                <Badge variant="outline">{checkpoint.kindLabel}</Badge>
                              </div>
                              <p className="mt-3 text-sm leading-6 text-foreground/90">
                                {displayValue(checkpoint.summary, 'Durable checkpoint recorded.')}
                              </p>
                              <p className="mt-2 text-[11px] text-muted-foreground">{formatTimestamp(checkpoint.createdAt)}</p>
                            </div>
                          ))
                        ) : (
                          <FeedEmptyState body="Cadence has not recorded a durable checkpoint for this run yet." title="No checkpoints recorded" />
                        )}
                      </div>
                    </>
                  ) : null}
                </div>
              </section>
            ) : null}

            {hasAgentFeedSurface ? (
              <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
              <div className="flex flex-col gap-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Agent feed</p>
                    <h2 className="mt-2 text-lg font-semibold text-foreground">{streamStatusMeta.title}</h2>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant={streamStatusMeta.badgeVariant}>{streamStatusLabel}</Badge>
                    <Badge variant={getStreamBadgeVariant(streamStatus)}>{streamStatus === 'replaying' ? 'Replaying recent activity' : streamStatusLabel}</Badge>
                  </div>
                </div>

                <p className="text-sm leading-6 text-muted-foreground">{streamStatusMeta.body}</p>

                {recentRunReplacement ? (
                  <Alert>
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Switched to a new supervised run</AlertTitle>
                    <AlertDescription>
                      <p>{recentRunReplacement.previousRunId} → {recentRunReplacement.nextRunId}</p>
                    </AlertDescription>
                  </Alert>
                ) : null}

                {streamStatus === 'subscribing' ? (
                  <Alert>
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                    <AlertTitle>Connecting to the live transcript</AlertTitle>
                    <AlertDescription>{agent.messagesUnavailableReason}</AlertDescription>
                  </Alert>
                ) : null}

                {streamStatus === 'replaying' ? (
                  <Alert>
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                    <AlertTitle>Replaying recent run-scoped backlog</AlertTitle>
                    <AlertDescription>{agent.messagesUnavailableReason}</AlertDescription>
                  </Alert>
                ) : null}

                {showNoRunStreamBanner ? (
                  <FeedEmptyState
                    body="Start or reconnect a supervised run to populate the run-scoped transcript, tool, skill, and activity lanes for this selected project."
                    title="No supervised run is attached"
                  />
                ) : null}

                {streamIssue ? (
                  <Alert variant="destructive">
                    <AlertCircle className="h-4 w-4" />
                    <AlertTitle>Live feed issue</AlertTitle>
                    <AlertDescription>
                      <p>{streamIssue.message}</p>
                      <p className="font-mono text-[11px] text-destructive/80">code: {streamIssue.code}</p>
                    </AlertDescription>
                  </Alert>
                ) : null}

                <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="text-base font-semibold text-foreground">Transcript</h3>
                      <Badge variant="outline">{streamRunId}</Badge>
                    </div>
                    <div className="mt-4 space-y-3">
                      {transcriptItems.length > 0 ? (
                        transcriptItems.map((item) => (
                          <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                              <span>{formatSequence(item.sequence)}</span>
                              <span>{item.runId}</span>
                            </div>
                            <p className="mt-2 text-sm leading-6 text-foreground/90">{item.text}</p>
                          </div>
                        ))
                      ) : (
                        <FeedEmptyState body="Cadence has not received transcript lines for this run yet." title="No transcript yet" />
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="text-base font-semibold text-foreground">Runtime activity</h3>
                      <Badge variant="outline">{streamSequenceLabel}</Badge>
                    </div>
                    <div className="mt-4 space-y-3">
                      {activityItems.length > 0 ? (
                        activityItems.map((item) => (
                          <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                              <span>{formatSequence(item.sequence)}</span>
                              <span>{item.runId}</span>
                            </div>
                            <p className="mt-2 text-sm font-medium text-foreground">{item.title}</p>
                            <p className="mt-1 text-sm leading-6 text-muted-foreground">
                              {displayValue(item.detail, 'Cadence recorded this activity without additional detail.')}
                            </p>
                          </div>
                        ))
                      ) : (
                        <FeedEmptyState body="Cadence has not recorded any runtime activity rows for this run yet." title="No runtime activity yet" />
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="text-base font-semibold text-foreground">Tool lane</h3>
                      <Badge variant="outline">{streamSessionLabel}</Badge>
                    </div>
                    <div className="mt-4 space-y-3">
                      {toolCalls.length > 0 ? (
                        toolCalls.map((item) => (
                          <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                              <span>{formatSequence(item.sequence)}</span>
                              <span>{item.runId}</span>
                              <Badge variant={getToolStateBadgeVariant(item.toolState)}>{getToolStateLabel(item.toolState)}</Badge>
                            </div>
                            <p className="mt-2 text-sm font-medium text-foreground">{item.toolName}</p>
                            <p className="mt-1 text-sm leading-6 text-muted-foreground">
                              {displayValue(item.detail, 'Cadence has not recorded tool detail for this call yet.')}
                            </p>
                          </div>
                        ))
                      ) : (
                        <FeedEmptyState body="Cadence has not observed any tool calls for this run yet." title="No tool calls yet" />
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="text-base font-semibold text-foreground">Skill lane</h3>
                      <Badge variant="outline">{skillItems.length} item{skillItems.length === 1 ? '' : 's'}</Badge>
                    </div>
                    <div className="mt-4 space-y-3">
                      {skillItems.length > 0 ? (
                        skillItems.map((item) => (
                          <div key={item.id} className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
                            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                              <span>{formatSequence(item.sequence)}</span>
                              <span>{item.runId}</span>
                              <Badge variant="outline">{getSkillStageLabel(item.stage)}</Badge>
                              <Badge variant={getSkillResultBadgeVariant(item.result)}>{getSkillResultLabel(item.result)}</Badge>
                              {item.cacheStatus ? <Badge variant="secondary">{getSkillCacheLabel(item.cacheStatus)}</Badge> : null}
                            </div>
                            <p className="mt-2 text-sm font-medium text-foreground">{item.skillId}</p>
                            <p className="mt-1 text-sm leading-6 text-muted-foreground">{item.detail}</p>
                            <div className="mt-3 space-y-1 text-[11px] text-muted-foreground">
                              <p>{formatSkillSource(item)}</p>
                              <p className="font-mono text-[10px]">tree {formatSkillTreeHash(item)}</p>
                            </div>
                            {item.diagnostic ? (
                              <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/5 px-2 py-2 text-[11px] text-destructive/90">
                                <p className="font-medium">{item.diagnostic.message}</p>
                                <p className="mt-1 font-mono">
                                  code: {item.diagnostic.code}
                                  {item.diagnostic.retryable ? ' · retryable' : ' · terminal'}
                                </p>
                              </div>
                            ) : null}
                          </div>
                        ))
                      ) : (
                        <FeedEmptyState
                          body="Cadence has not observed any skill discovery, install, or invoke lifecycle rows for this run yet."
                          title="No skill activity yet"
                        />
                      )}
                    </div>
                  </div>
                </div>
              </div>
            </section>
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
                        const resumeMeta = getPerActionResumeStateMeta({
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

        <div className="relative shrink-0 px-4 pb-7 pt-10">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 -top-14 h-24 bg-gradient-to-b from-background/0 via-background/86 to-background"
          />
          <div className="relative mx-auto flex w-full max-w-[880px] items-end justify-center gap-3">
            <div className="w-full max-w-[620px]">
              <div className="relative overflow-hidden rounded-xl border border-border/70 bg-card/95 shadow-[0_18px_50px_rgba(0,0,0,0.2)] backdrop-blur supports-[backdrop-filter]:bg-card/80">
                <Textarea
                  aria-label="Agent input unavailable"
                  className="max-h-56 min-h-[120px] resize-none border-0 bg-transparent px-4 pb-12 pt-4 text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/55 shadow-none outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100"
                  disabled
                  placeholder={composerPlaceholder}
                  rows={4}
                  value=""
                />
                <div className="absolute bottom-2 left-3 right-14 flex max-w-[calc(100%-5rem)] flex-wrap items-center gap-3">
                  <Select value={composerModelId} onValueChange={setComposerModelId}>
                    <SelectTrigger
                      aria-label="Model selector"
                      className={composerInlineSelectTriggerClassName}
                      size="sm"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className={composerInlineSelectContentClassName}>
                      {composerModelGroups.map((group, index) => (
                        <div key={group.id}>
                          {index > 0 ? <SelectSeparator /> : null}
                          <SelectGroup>
                            <SelectLabel>{group.label}</SelectLabel>
                            {group.items.map((model) => (
                              <SelectItem key={model.value} value={model.value}>
                                {model.label}
                              </SelectItem>
                            ))}
                          </SelectGroup>
                        </div>
                      ))}
                    </SelectContent>
                  </Select>
                  <Select value={composerThinkingLevel} onValueChange={setComposerThinkingLevel}>
                    <SelectTrigger
                      aria-label="Thinking level selector"
                      className={composerInlineSelectTriggerClassName}
                      size="sm"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent className={composerInlineSelectContentClassName}>
                      {SAMPLE_COMPOSER_THINKING_LEVELS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <button
                  aria-label="Send message unavailable"
                  className="absolute bottom-3 right-3 inline-flex h-8 w-8 items-center justify-center rounded-lg bg-foreground/90 text-background opacity-40 shadow-sm"
                  disabled
                  type="button"
                >
                  <Send className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
            {canStartRuntimeRun && !renderableRuntimeRun && (
              <button
                className="shrink-0 flex items-center gap-1.5 rounded-lg border border-border bg-card/80 px-3 py-2 text-[12px] font-medium text-foreground transition-colors hover:border-border/80 hover:bg-card disabled:opacity-50"
                disabled={runtimeRunActionStatus === 'running'}
                onClick={() => void handleStartRuntimeRun()}
                type="button"
              >
                {runtimeRunActionStatus === 'running' ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                ) : (
                  <Play className="h-3.5 w-3.5" />
                )}
                Start run
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
