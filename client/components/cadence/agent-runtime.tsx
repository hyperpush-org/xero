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
  ChevronRight,
  ExternalLink,
  GitBranch,
  LoaderCircle,
  LogIn,
  LogOut,
  Play,
  Send,
  ShieldCheck,
  Terminal,
  XCircle,
} from 'lucide-react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'

interface AgentRuntimeProps {
  agent: AgentPaneView
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

function getStatusMeta(runtimeSession: RuntimeSessionView | null, agent: AgentPaneView) {
  if (!runtimeSession) {
    return {
      eyebrow: 'Runtime setup',
      title: 'Sign in to OpenAI for this project',
      body: agent.sessionUnavailableReason,
      badgeVariant: 'outline' as const,
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
  recoveryState: AgentPaneView['autonomousRun'] extends { recoveryState: infer T } ? T : never,
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
  approval: OperatorApprovalView
  latestResumeForAction: ResumeHistoryEntryView | null
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: { actionId: string; kind: OperatorIntentKind } | null
}): PerActionResumeStateMeta {
  const { approval, latestResumeForAction, operatorActionStatus, pendingOperatorActionId, pendingOperatorIntent } = options
  const isActionInFlight =
    (operatorActionStatus === 'running' && pendingOperatorActionId === approval.actionId) ||
    pendingOperatorIntent?.actionId === approval.actionId

  if (isActionInFlight) {
    return {
      state: 'running',
      label: 'Running',
      detail:
        pendingOperatorIntent?.kind === 'resume'
          ? 'Resume request is in flight for this action. Cadence will refresh durable state before updating this card.'
          : 'Decision persistence is in flight for this action. Cadence keeps the last durable resume state visible until refresh completes.',
      badgeVariant: 'secondary',
      timestamp: approval.updatedAt,
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

  if (approval.isPending) {
    return {
      state: 'waiting',
      label: 'Waiting',
      detail: 'Waiting for operator input before this action can resume the run.',
      badgeVariant: 'outline',
      timestamp: approval.updatedAt,
    }
  }

  if (approval.canResume) {
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
    label: 'Waiting',
    detail: 'No resume recorded yet for this action. Rejected decisions remain audit-only and cannot resume the run.',
    badgeVariant: 'outline',
    timestamp: approval.updatedAt,
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
  streamRunId?: string,
): string {
  if (!runtimeSession) {
    return 'Sign in with OpenAI to start.'
  }

  if (!runtimeSession.isAuthenticated) {
    return runtimeSession.isLoginInProgress
      ? 'Finish the login flow to continue.'
      : 'Sign in with OpenAI to start.'
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
      body: 'Start or reconnect a supervised run to populate the run-scoped transcript, tool, and activity lanes for this selected project.',
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
          : 'Start or reconnect a supervised run to populate the run-scoped transcript, tool, and activity lanes for this selected project.',
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
  const autonomousRecentArtifacts = useMemo(
    () => sortByNewest(agent.autonomousRecentArtifacts ?? [], (artifact) => artifact.updatedAt || artifact.createdAt).slice(0, 5),
    [agent.autonomousRecentArtifacts],
  )
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const hasIncompleteRuntimeRunPayload = Boolean(runtimeRun && !renderableRuntimeRun)
  const runtimeStream = agent.runtimeStream ?? null
  const streamStatus = agent.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeStreamItems = agent.runtimeStreamItems ?? runtimeStream?.items ?? []
  const activityItems = agent.activityItems ?? runtimeStream?.activityItems ?? []
  const actionRequiredItems = agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? []
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
  const streamIssue = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const approvalRequests = agent.approvalRequests ?? []
  const resumeHistory = agent.resumeHistory ?? []
  const notificationRoutes = agent.notificationRoutes ?? []
  const notificationChannelHealth = agent.notificationChannelHealth ?? []
  const notificationRouteLoadStatus = agent.notificationRouteLoadStatus ?? 'idle'
  const notificationRouteError = agent.notificationRouteError ?? null
  const notificationSyncSummary = agent.notificationSyncSummary ?? null
  const notificationSyncError = agent.notificationSyncError ?? null
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

    const runtimeState: AgentTrustSignalState = !renderableRuntimeRun
      ? 'unavailable'
      : renderableRuntimeRun.isFailed || renderableRuntimeRun.isStale
        ? 'degraded'
        : renderableRuntimeRun.isActive && runtimeSession?.isAuthenticated
          ? 'healthy'
          : renderableRuntimeRun.isActive
            ? 'degraded'
            : 'unavailable'

    const streamState: AgentTrustSignalState = !runtimeStream
      ? 'unavailable'
      : runtimeStream.status === 'error' || runtimeStream.status === 'stale'
        ? 'degraded'
        : runtimeStream.status === 'live' || runtimeStream.status === 'complete'
          ? 'healthy'
          : 'unavailable'

    const approvalsState: AgentTrustSignalState = pendingApprovalCount > 0 ? 'degraded' : 'healthy'

    const routesState: AgentTrustSignalState = notificationRoutes.length === 0
      ? notificationRouteError
        ? 'degraded'
        : 'unavailable'
      : notificationRouteError || degradedRouteCount > 0
        ? 'degraded'
        : 'healthy'

    const credentialsState: AgentTrustSignalState = enabledRoutes.length === 0
      ? 'unavailable'
      : missingCredentialRouteCount > 0 || malformedCredentialRouteCount > 0 || unavailableCredentialRouteCount > 0
        ? 'degraded'
        : 'healthy'

    const syncDispatchFailedCount = notificationSyncSummary?.dispatch.failedCount ?? 0
    const syncReplyRejectedCount = notificationSyncSummary?.replies.rejectedCount ?? 0
    const syncState: AgentTrustSignalState = notificationSyncError
      ? 'degraded'
      : !notificationSyncSummary
        ? 'unavailable'
        : syncDispatchFailedCount > 0 || syncReplyRejectedCount > 0
          ? 'degraded'
          : 'healthy'

    const signalStates: AgentTrustSignalState[] = [
      runtimeState,
      streamState,
      approvalsState,
      routesState,
      credentialsState,
      syncState,
    ]
    const state: AgentTrustSignalState = signalStates.includes('degraded')
      ? 'degraded'
      : signalStates.every((value) => value === 'healthy')
        ? 'healthy'
        : 'unavailable'

    return {
      state,
      stateLabel: getTrustSignalLabel(state),
      runtimeState,
      runtimeReason: agent.runtimeRunUnavailableReason,
      streamState,
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
          : degradedRouteCount > 0
            ? `${degradedRouteCount} route(s) show degraded or pending dispatch health.`
            : 'Notification route health is stable for configured channels.',
      credentialsState,
      credentialsReason: enabledRoutes.length === 0
        ? 'No enabled routes require app-local credential readiness checks.'
        : missingCredentialRouteCount > 0
          ? `${missingCredentialRouteCount} enabled route(s) are missing required app-local credentials.`
          : malformedCredentialRouteCount > 0
            ? `${malformedCredentialRouteCount} enabled route(s) have malformed app-local credential state.`
            : unavailableCredentialRouteCount > 0
              ? `${unavailableCredentialRouteCount} enabled route(s) could not read app-local credential state.`
              : 'All enabled routes report fully configured app-local credentials.',
      syncState,
      syncReason: notificationSyncError
        ? notificationSyncError.message
        : !notificationSyncSummary
          ? 'No notification adapter sync summary is available yet.'
          : syncDispatchFailedCount > 0 || syncReplyRejectedCount > 0
            ? `Latest sync cycle reported ${syncDispatchFailedCount} failed dispatch(es) and ${syncReplyRejectedCount} rejected repl${syncReplyRejectedCount === 1 ? 'y' : 'ies'}.`
            : 'Latest notification adapter sync cycle completed without failed dispatches or rejected replies.',
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
        code: 'trust_snapshot_unavailable',
        message: 'Cadence rendered a derived fallback trust snapshot because the typed projection payload is unavailable.',
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
    renderableRuntimeRun,
    runtimeSession?.isAuthenticated,
    runtimeStream,
  ])
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

  const statusMeta = useMemo(() => getStatusMeta(runtimeSession, agent), [agent, runtimeSession])
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
  const canStartLogin = hasRepositoryBinding && typeof onStartLogin === 'function'
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
  const canResumeRuntimeSession = hasRepositoryBinding && typeof onStartRuntimeSession === 'function'
  const canSubmitManualInput = hasRepositoryBinding && typeof onSubmitManualCallback === 'function'
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
    runtimeSession?.isLoginInProgress || hasAuthorizationUrl || browserMessage || runtimeSession?.needsManualInput,
  )
  const showReuseButton = !runtimeSession || runtimeSession.isSignedOut || runtimeSession.isFailed
  const showLogoutButton = Boolean(runtimeSession && !runtimeSession.isLoginInProgress && !runtimeSession.isSignedOut)
  const composerPlaceholder = getComposerPlaceholder(runtimeSession, streamStatus, renderableRuntimeRun, streamRunId)
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
  const hasDurableOperatorState = approvalRequests.length > 0 || resumeHistory.length > 0 || Boolean(agent.latestDecisionOutcome)
  const hasAgentFeedSurface = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      runtimeSession?.isAuthenticated ||
      recentRunReplacement ||
      streamIssue ||
      transcriptItems.length > 0 ||
      activityItems.length > 0 ||
      toolCalls.length > 0 ||
      actionRequiredItems.length > 0 ||
      latestCompletion ||
      latestFailure,
  )
  const hasOperatorStateSurface = Boolean(hasDurableOperatorState || operatorActionError)

  const sortedApprovals = useMemo(
    () => sortByNewest(approvalRequests, (approval) => approval.updatedAt ?? approval.createdAt).slice(0, 6),
    [approvalRequests],
  )
  const pendingApprovals = useMemo(() => sortedApprovals.filter((approval) => approval.isPending), [sortedApprovals])
  const sortedResumeHistory = useMemo(
    () => sortByNewest(resumeHistory, (entry) => entry.createdAt).slice(0, 6),
    [resumeHistory],
  )
  const latestResumeByActionId = useMemo(() => {
    const entriesByActionId = new Map<string, ResumeHistoryEntryView>()

    for (const entry of sortedResumeHistory) {
      const sourceActionId = entry.sourceActionId?.trim()
      if (!sourceActionId || entriesByActionId.has(sourceActionId)) {
        continue
      }

      entriesByActionId.set(sourceActionId, entry)
    }

    return entriesByActionId
  }, [sortedResumeHistory])
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
        title: 'Runtime liveness',
        state: trustSnapshot.runtimeState,
        summary: `Run ${runtimeRunStatusText} · Stream ${streamStatusLabel}`,
        reason: trustSnapshot.runtimeReason,
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
        title: 'Notification routes',
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
        title: 'Latest sync diagnostics',
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
  const trustRecoveryActions = useMemo(() => {
    const nextActions: string[] = []

    if (trustSnapshot.runtimeState !== 'healthy') {
      if (!runtimeSession?.isAuthenticated) {
        nextActions.push('Sign in with OpenAI from Settings before trusting autonomous execution.')
      }

      if (!renderableRuntimeRun || renderableRuntimeRun.isStale || renderableRuntimeRun.isFailed) {
        nextActions.push('Start or reconnect the supervised run so runtime liveness is durable and current.')
      }

      if (streamStatus === 'error' || streamStatus === 'stale') {
        nextActions.push('Retry the live feed after runtime reconnection so run-scoped telemetry is current.')
      }
    }

    if (trustSnapshot.approvalsState === 'degraded') {
      nextActions.push('Resolve pending operator approvals so autonomous continuation is no longer blocked.')
    }

    if (trustSnapshot.credentialsState === 'degraded') {
      nextActions.push('Configure missing or malformed app-local route credentials in Settings before dispatch.')
    }

    if (trustSnapshot.routesState !== 'healthy' || trustSnapshot.syncState !== 'healthy') {
      nextActions.push('Refresh route health from Settings after credential updates.')
    }

    if (trustPrimaryErrorCode) {
      nextActions.push(`Inspect error code ${trustPrimaryErrorCode} before considering this project trust state healthy.`)
    }

    return Array.from(new Set(nextActions))
  }, [renderableRuntimeRun, runtimeSession?.isAuthenticated, streamStatus, trustPrimaryErrorCode, trustSnapshot])
  const hasTrustRecoveryActions = trustRecoveryActions.length > 0

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
      setActionMessage(getErrorMessage(error, 'Cadence could not reuse the app-local runtime session for this project.'))
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
      setActionMessage(getErrorMessage(error, 'Cadence could not remove the OpenAI runtime session for this project.'))
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
        <div className="shrink-0 border-b border-border bg-card/30 px-4 py-[10px]">
          <div className="flex items-center gap-3 text-[12px]">
            <span className="text-muted-foreground">Phase</span>
            <ChevronRight className="h-3 w-3 text-muted-foreground/40" />
            <span className="font-medium text-foreground/80">{agent.activePhase?.name ?? 'None active'}</span>
            <div className="ml-auto flex items-center gap-3 text-[10px] font-mono text-muted-foreground">
              <div className="flex items-center gap-1">
                <GitBranch className="h-3.5 w-3.5" />
                <span>{agent.branchLabel}</span>
              </div>
              <div className="flex items-center gap-1">
                <Terminal className="h-3.5 w-3.5" />
                <span>{agent.runtimeLabel}</span>
              </div>
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto scrollbar-thin px-4 py-4">
          <div className="mx-auto flex max-w-4xl flex-col gap-4">
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
                    body="Start or reconnect a supervised run to populate the run-scoped transcript, tool, and activity lanes for this selected project."
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

                <div className="grid gap-4 lg:grid-cols-3">
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
                </div>
              </div>
            </section>
            ) : null}

            {hasOperatorStateSurface ? (
              <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
              <div className="flex flex-col gap-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Operator state</p>
                    <h2 className="mt-2 text-lg font-semibold text-foreground">Durable approvals and resume checkpoints</h2>
                  </div>
                  <Badge variant={pendingApprovals.length > 0 ? 'secondary' : 'outline'}>{pendingApprovals.length} pending</Badge>
                </div>

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

                {hasDurableOperatorState ? (
                  <div className="space-y-3">
                    {sortedApprovals.map((approval) => {
                      const answerValue = operatorAnswers[approval.actionId] ?? ''
                      const normalizedAnswer = normalizeAnswerInput(answerValue)
                      const requiresAnswer = approval.requiresUserAnswer
                      const showAnswerError = requiresAnswer && answerValue.length > 0 && normalizedAnswer.length === 0
                      const actionPending = pendingOperatorIntent?.actionId === approval.actionId
                      const resumeMeta = getPerActionResumeStateMeta({
                        approval,
                        latestResumeForAction: latestResumeByActionId.get(approval.actionId) ?? null,
                        operatorActionStatus,
                        pendingOperatorActionId,
                        pendingOperatorIntent,
                      })
                      const gateLinkage = formatGateLinkage(approval)

                      return (
                        <div key={approval.actionId} className="rounded-xl border border-border/70 bg-card/70 p-4">
                          <div className="flex flex-wrap items-start justify-between gap-3">
                            <div>
                              <div className="flex flex-wrap items-center gap-2">
                                <p className="text-sm font-semibold text-foreground">{approval.title}</p>
                                <Badge variant={getApprovalBadgeVariant(approval.status)}>{approval.statusLabel}</Badge>
                                <Badge variant={resumeMeta.badgeVariant}>{resumeMeta.label}</Badge>
                              </div>
                              <p className="mt-2 text-sm leading-6 text-muted-foreground">{approval.detail}</p>
                              {gateLinkage ? <p className="mt-2 text-[11px] text-muted-foreground">{gateLinkage}</p> : null}
                            </div>
                          </div>

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
                                    aria-label={`Operator answer for ${approval.actionId}`}
                                    className="min-h-24"
                                    onChange={(event) =>
                                      setOperatorAnswers((currentAnswers) => ({
                                        ...currentAnswers,
                                        [approval.actionId]: event.target.value,
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
                              <InfoRow label="Action ID" mono value={approval.actionId} />
                              <InfoRow label="Updated" value={formatTimestamp(resumeMeta.timestamp)} />
                              <p className="text-[12px] leading-5 text-muted-foreground">{resumeMeta.detail}</p>

                              <div className="flex flex-wrap gap-2">
                                {approval.isPending ? (
                                  <Button
                                    disabled={actionPending || (requiresAnswer && normalizedAnswer.length === 0)}
                                    onClick={() =>
                                      void handleResolveOperatorAction(approval.actionId, 'approve', {
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
                                      void handleResolveOperatorAction(approval.actionId, 'reject', {
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
                                      void handleResumeOperatorRun(approval.actionId, {
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
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <FeedEmptyState body="Cadence has not recorded any durable operator approvals or resume checkpoints for this project yet." title="No operator checkpoints yet" />
                )}
              </div>
            </section>
            ) : null}

          </div>
        </div>

        <div className="shrink-0 px-4 py-4">
          <div className="flex items-end gap-2">
            <div className="flex flex-1 items-end gap-2 rounded-lg bg-secondary/50 px-3 py-2">
              <textarea
                aria-label="Agent input unavailable"
                className="max-h-32 flex-1 resize-none bg-transparent text-[13px] leading-relaxed text-foreground placeholder:text-muted-foreground/50 outline-none"
                disabled
                placeholder={composerPlaceholder}
                rows={1}
                value=""
              />
              <button className="shrink-0 rounded-md bg-foreground/90 p-1.5 text-background opacity-40" disabled type="button">
                <Send className="h-3.5 w-3.5" />
              </button>
            </div>
            {canStartRuntimeRun && !renderableRuntimeRun && (
              <button
                className="shrink-0 flex items-center gap-1.5 rounded-lg border border-border bg-card/80 px-3 py-2 text-[12px] font-medium text-foreground hover:bg-card hover:border-border/80 transition-colors disabled:opacity-50"
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
