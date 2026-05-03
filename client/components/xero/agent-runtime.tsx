"use client"

import { memo, useCallback, useEffect, useMemo, useRef, useState, type WheelEvent } from 'react'
import { ArrowDown, ChevronRight, Loader2, Plus, SplitSquareHorizontal, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { useDebouncedValue } from '@/lib/input-priority'
import { cn } from '@/lib/utils'
import type {
  AgentPaneView,
  AgentProviderModelView,
} from '@/src/features/xero/use-xero-desktop-state'
import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type {
  AgentDefinitionSummaryDto,
  AgentSessionView,
  RuntimeRunView,
  RuntimeAutoCompactPreferenceDto,
  ProviderAuthSessionView,
  RuntimeSessionView,
  RuntimeStreamActivityItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/xero-model'
import type { SessionContextSnapshotDto } from '@/src/lib/xero-model/session-context'
import {
  getRuntimeAgentLabel,
  getRuntimeRunThinkingEffortLabel,
  type RuntimeRunControlInputDto,
  type StagedAgentAttachmentDto,
} from '@/src/lib/xero-model'
import {
  classifyAttachment,
  classificationRejectionMessage,
} from '@/lib/agent-attachments'

import {
  AgentContextMeter,
  type AgentContextMeterStatus,
} from './agent-runtime/agent-context-meter'
import {
  getComposerApprovalOptions,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerThinkingOptions,
} from './agent-runtime/composer-helpers'
import { ComposerDock, type ComposerPendingAttachment } from './agent-runtime/composer-dock'
import { AgentPaneDropOverlay } from './agent-runtime/agent-pane-drop-overlay'
import { AgentCreateDraftSection } from './agent-runtime/agent-create-draft-section'
import { ConversationSection, type ConversationTurn } from './agent-runtime/conversation-section'
import { EmptySessionState } from './agent-runtime/empty-session-state'
import {
  getToolCardTitle,
  getStreamRunId,
  getToolStateLabel,
  getToolSummaryContext,
  hasUsableRuntimeRunId,
} from './agent-runtime/runtime-stream-helpers'
import { SetupEmptyState } from './agent-runtime/setup-empty-state'
import { useAgentRuntimeController } from './agent-runtime/use-agent-runtime-controller'
import type { SpeechDictationAdapter } from './agent-runtime/use-speech-dictation'

type AgentRuntimeDesktopAdapter = SpeechDictationAdapter &
  Partial<
    Pick<
      XeroDesktopAdapter,
      'getSessionContextSnapshot' | 'stageAgentAttachment' | 'discardAgentAttachment'
    >
  >

export interface AgentRuntimeProps {
  agent: AgentPaneView
  onOpenSettings?: () => void
  onOpenDiagnostics?: () => void
  onStartLogin?: (options?: { originator?: string | null }) => Promise<ProviderAuthSessionView | null>
  onStartAutonomousRun?: () => Promise<unknown>
  onInspectAutonomousRun?: () => Promise<unknown>
  onCancelAutonomousRun?: (runId: string) => Promise<unknown>
  onStartRuntimeRun?: (options?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => Promise<RuntimeRunView | null>
  onUpdateRuntimeRunControls?: (request?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
    autoCompact?: RuntimeAutoCompactPreferenceDto | null
  }) => Promise<RuntimeRunView | null>
  onComposerControlsChange?: (controls: RuntimeRunControlInputDto | null) => void
  onStartRuntimeSession?: (options?: { providerProfileId?: string | null }) => Promise<RuntimeSessionView | null>
  onStopRuntimeRun?: (runId: string) => Promise<RuntimeRunView | null>
  onSubmitManualCallback?: (flowId: string, manualInput: string) => Promise<ProviderAuthSessionView | null>
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
  desktopAdapter?: AgentRuntimeDesktopAdapter
  /** GitHub avatar URL for the signed-in account, when available. */
  accountAvatarUrl?: string | null
  /** GitHub login for the signed-in account. */
  accountLogin?: string | null
  onCreateSession?: () => void
  isCreatingSession?: boolean
  /** Active and known custom agent definitions visible to the composer selector. */
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  /** Open the Settings → Agents tab so the user can manage custom agents. */
  onOpenAgentManagement?: () => void
  /** Visual density. Compact attaches the composer flush to bottom and moves secondary controls into a gear popover. */
  density?: 'comfortable' | 'compact'
  /** Pane id for multi-pane workspaces. */
  paneId?: string | null
  /** Pane index (1-based) shown as a chip when more than one pane is open. */
  paneNumber?: number | null
  /** Total panes currently open. */
  paneCount?: number
  /** Spawn a new pane. Disabled when at max. */
  onSpawnPane?: () => void
  /** Spawn-pane button is disabled (e.g., already at the cap). */
  spawnPaneDisabled?: boolean
  /** Close this pane. Hidden when paneCount === 1. */
  onClosePane?: () => void
  /** Focus this pane. Called when the user interacts with the pane. */
  onFocusPane?: () => void
  /** Whether this pane is currently focused. */
  isPaneFocused?: boolean
  /** Whether this pane should show the first-run compact-mode tooltip. */
  showCompactFirstRunTooltip?: boolean
  /** Acknowledge first-run tooltip. */
  onAckCompactFirstRunTooltip?: () => void
  /** Reports pane-local state that should block an immediate close. */
  onPaneCloseStateChange?: (state: AgentPaneCloseState) => void
}

const EMPTY_ACTION_REQUIRED_ITEMS: NonNullable<AgentPaneView['actionRequiredItems']> = []
const MAX_VISIBLE_RUNTIME_ACTION_TURNS = 16
const COMPACT_TOOL_BURST_THRESHOLD = 5
const CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX = 96

export interface AgentPaneCloseState {
  hasRunningRun: boolean
  hasUnsavedComposerText: boolean
  sessionTitle: string
}

export function isRuntimeConversationNearBottom(
  viewport: Pick<HTMLElement, 'scrollTop' | 'scrollHeight' | 'clientHeight'>,
  thresholdPx = CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX,
): boolean {
  if (viewport.scrollHeight <= viewport.clientHeight) {
    return true
  }

  return viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight <= thresholdPx
}

function appendTranscriptDelta(current: string, delta: string): string {
  return `${current}${delta}`
}

function shouldShowActionItem(item: RuntimeStreamViewItem): item is RuntimeStreamToolItemView | RuntimeStreamFailureItemView {
  return item.kind === 'tool' || item.kind === 'failure'
}

function isReasoningActivityItem(item: RuntimeStreamViewItem): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === 'owned_agent_reasoning'
}

function getReasoningActivityText(item: RuntimeStreamActivityItemView): string {
  return item.text ?? item.detail ?? ''
}

function appendThinkingDelta(current: string, delta: string): string {
  return `${current}${delta}`
}

function actionTurnFromItem(item: RuntimeStreamToolItemView): ConversationTurn {
  const summary = getToolSummaryContext(item)
  const detail = getActionDetail(item, summary)
  return {
    id: item.id,
    kind: 'action',
    sequence: item.sequence,
    toolCallId: item.toolCallId,
    toolName: item.toolName,
    title: getToolCardTitle(item),
    detail,
    detailRows: getActionDetailRows(item, summary),
    state: item.toolState,
  }
}

function normalizeToolCopy(value: string): string {
  return value.trim().replace(/[._-]+/g, ' ').replace(/\s+/g, ' ').toLowerCase()
}

function isGenericToolDetail(detail: string, item: RuntimeStreamToolItemView): boolean {
  const normalizedDetail = normalizeToolCopy(detail)
  return normalizedDetail === normalizeToolCopy(item.toolName) || normalizedDetail === 'tool activity recorded'
}

function getActionDetail(item: RuntimeStreamToolItemView, summary: string | null): string {
  if (item.detail && (!summary || !isGenericToolDetail(item.detail, item))) {
    return item.detail
  }

  return summary ?? item.detail ?? 'Tool activity recorded.'
}

function getActionDetailRows(
  item: RuntimeStreamToolItemView,
  summary: string | null,
): Array<{ label: string; value: string }> {
  const rows: Array<{ label: string; value: string }> = []

  if (item.detail) {
    rows.push({
      label: item.toolState === 'running' ? 'Input' : 'Outcome',
      value: item.detail,
    })
  }

  if (summary && summary !== item.detail) {
    rows.push({
      label: 'Result',
      value: summary,
    })
  }

  return rows
}

function mergeActionRows(
  existing: Array<{ label: string; value: string }>,
  incoming: Array<{ label: string; value: string }>,
): Array<{ label: string; value: string }> {
  const seen = new Set(existing.map((row) => `${row.label}\u0000${row.value}`))
  const merged = [...existing]

  for (const row of incoming) {
    const key = `${row.label}\u0000${row.value}`
    if (!seen.has(key)) {
      seen.add(key)
      merged.push(row)
    }
  }

  return merged
}

function mergeActionTurn(existing: ConversationTurn, incoming: ConversationTurn): void {
  if (existing.kind !== 'action' || incoming.kind !== 'action') {
    return
  }

  existing.sequence = incoming.sequence
  existing.state = incoming.state
  existing.detail = incoming.detail
  existing.detailRows = mergeActionRows(existing.detailRows, incoming.detailRows)

  if (incoming.title.length >= existing.title.length) {
    existing.title = incoming.title
  }
}

function isActionLikeTurn(
  turn: ConversationTurn,
): turn is Extract<ConversationTurn, { kind: 'action' | 'action_group' }> {
  return turn.kind === 'action' || turn.kind === 'action_group'
}

function actionGroupState(
  actions: Extract<ConversationTurn, { kind: 'action' }>[],
): RuntimeStreamToolItemView['toolState'] | null {
  if (actions.some((action) => action.state === 'failed')) {
    return 'failed'
  }
  if (actions.some((action) => action.state === 'running')) {
    return 'running'
  }
  if (actions.some((action) => action.state === 'pending')) {
    return 'pending'
  }
  if (actions.some((action) => action.state === 'succeeded')) {
    return 'succeeded'
  }
  return null
}

function summarizeActionGroup(
  actions: Extract<ConversationTurn, { kind: 'action' }>[],
): string {
  const stateCounts = new Map<RuntimeStreamToolItemView['toolState'], number>()
  for (const action of actions) {
    if (action.state) {
      stateCounts.set(action.state, (stateCounts.get(action.state) ?? 0) + 1)
    }
  }

  const stateSummary = (['failed', 'running', 'pending', 'succeeded'] as const)
    .map((state) => {
      const count = stateCounts.get(state) ?? 0
      return count > 0 ? `${count} ${getToolStateLabel(state).toLowerCase()}` : null
    })
    .filter((part): part is string => Boolean(part))
    .join(' · ')
  const latestAction = actions.at(-1)

  return [
    stateSummary || `${actions.length} recorded`,
    latestAction ? `latest ${latestAction.title}` : null,
  ]
    .filter((part): part is string => Boolean(part))
    .join(' · ')
}

function actionGroupTurnFromActions(
  actions: Extract<ConversationTurn, { kind: 'action' }>[],
): ConversationTurn {
  const firstAction = actions[0]
  const lastAction = actions.at(-1) ?? firstAction

  return {
    id: `tool-group:${firstAction.id}:${lastAction.id}`,
    kind: 'action_group',
    sequence: lastAction.sequence,
    title: `${actions.length} tool calls`,
    detail: summarizeActionGroup(actions),
    state: actionGroupState(actions),
    actions: actions.map((action) => ({
      id: action.id,
      title: action.title,
      detail: action.detail,
      state: action.state ?? null,
    })),
  }
}

function compactActionBursts(turns: ConversationTurn[]): ConversationTurn[] {
  const compactedTurns: ConversationTurn[] = []
  let actionBuffer: Extract<ConversationTurn, { kind: 'action' }>[] = []

  const flushActionBuffer = () => {
    if (actionBuffer.length >= COMPACT_TOOL_BURST_THRESHOLD) {
      compactedTurns.push(actionGroupTurnFromActions(actionBuffer))
    } else {
      compactedTurns.push(...actionBuffer)
    }
    actionBuffer = []
  }

  for (const turn of turns) {
    if (turn.kind === 'action') {
      actionBuffer.push(turn)
      continue
    }

    flushActionBuffer()
    compactedTurns.push(turn)
  }

  flushActionBuffer()
  return compactedTurns
}

function limitActionTurns(turns: ConversationTurn[]): ConversationTurn[] {
  const actionTurnIndexes = turns
    .map((turn, index) => (isActionLikeTurn(turn) ? index : null))
    .filter((index): index is number => index != null)

  if (actionTurnIndexes.length <= MAX_VISIBLE_RUNTIME_ACTION_TURNS) {
    return turns
  }

  const keptActionTurnIndexes = new Set(
    actionTurnIndexes.slice(actionTurnIndexes.length - MAX_VISIBLE_RUNTIME_ACTION_TURNS),
  )
  return turns.filter((turn, index) => !isActionLikeTurn(turn) || keptActionTurnIndexes.has(index))
}

function buildConversationTurns(runtimeStreamItems: RuntimeStreamViewItem[]): ConversationTurn[] {
  const turns: ConversationTurn[] = []
  const actionTurnIndexByToolCallId = new Map<string, number>()

  for (const item of runtimeStreamItems) {
    if (item.kind === 'transcript') {
      if (item.role !== 'user' && item.role !== 'assistant') {
        continue
      }

      const previous = turns.at(-1)
      if (item.role === 'assistant' && previous?.kind === 'message' && previous.role === item.role) {
        previous.text = appendTranscriptDelta(previous.text, item.text)
        previous.sequence = item.sequence
        continue
      }

      turns.push({
        id: item.id,
        kind: 'message',
        role: item.role,
        sequence: item.sequence,
        text: item.text,
      })
      continue
    }

    if (isReasoningActivityItem(item)) {
      const text = getReasoningActivityText(item)
      if (text.trim().length === 0) {
        const previousThinking = turns.at(-1)
        if (previousThinking?.kind === 'thinking') {
          previousThinking.text = appendThinkingDelta(previousThinking.text, text)
          previousThinking.sequence = item.sequence
        }
        continue
      }

      const previous = turns.at(-1)
      if (previous?.kind === 'thinking') {
        previous.text = appendThinkingDelta(previous.text, text)
        previous.sequence = item.sequence
        continue
      }

      turns.push({
        id: item.id,
        kind: 'thinking',
        sequence: item.sequence,
        text,
      })
      continue
    }

    if (!shouldShowActionItem(item)) {
      continue
    }

    if (item.kind === 'failure') {
      turns.push({
        id: item.id,
        kind: 'failure',
        sequence: item.sequence,
        code: item.code,
        message: item.message,
      })
      continue
    }

    const incomingActionTurn = actionTurnFromItem(item)
    const existingActionTurnIndex = actionTurnIndexByToolCallId.get(item.toolCallId)
    const existingActionTurn =
      existingActionTurnIndex != null ? turns[existingActionTurnIndex] : null

    if (existingActionTurn?.kind === 'action') {
      mergeActionTurn(existingActionTurn, incomingActionTurn)
      continue
    }

    actionTurnIndexByToolCallId.set(item.toolCallId, turns.length)
    turns.push(incomingActionTurn)
  }

  return limitActionTurns(compactActionBursts(turns))
}

function toContextMeterError(error: unknown): {
  code: string
  message: string
  retryable: boolean
} {
  const candidate = error as { code?: unknown; retryable?: unknown; message?: unknown } | null
  return {
    code: typeof candidate?.code === 'string' && candidate.code.trim().length > 0
      ? candidate.code
      : 'agent_context_meter_failed',
    message: typeof candidate?.message === 'string' && candidate.message.trim().length > 0
      ? candidate.message
      : error instanceof Error && error.message.trim().length > 0
        ? error.message
        : 'Xero could not refresh the context meter.',
    retryable: typeof candidate?.retryable === 'boolean' ? candidate.retryable : true,
  }
}

function useAgentContextMeterSnapshot(options: {
  adapter?: AgentRuntimeDesktopAdapter
  projectId: string
  agentSessionId: string | null
  runId: string | null
  providerId: string | null
  modelId: string | null
  pendingPrompt: string
  lifecycleKey: string
}): {
  status: AgentContextMeterStatus
  snapshot: SessionContextSnapshotDto | null
  error: ReturnType<typeof toContextMeterError> | null
  refresh: () => void
} {
  const debouncedPendingPrompt = useDebouncedValue(options.pendingPrompt, 350)
  const debouncedLifecycleKey = useDebouncedValue(options.lifecycleKey, 400)
  const [status, setStatus] = useState<AgentContextMeterStatus>('idle')
  const [snapshot, setSnapshot] = useState<SessionContextSnapshotDto | null>(null)
  const [error, setError] = useState<ReturnType<typeof toContextMeterError> | null>(null)
  const requestIdRef = useRef(0)
  const snapshotRef = useRef<SessionContextSnapshotDto | null>(null)

  useEffect(() => {
    snapshotRef.current = snapshot
  }, [snapshot])

  const refresh = useCallback(() => {
    if (!options.adapter?.getSessionContextSnapshot || !options.agentSessionId) {
      requestIdRef.current += 1
      setStatus('idle')
      setSnapshot(null)
      setError(null)
      return
    }

    const requestId = requestIdRef.current + 1
    requestIdRef.current = requestId
    if (!snapshotRef.current) {
      setStatus('loading')
    }
    setError(null)

    void options.adapter
      .getSessionContextSnapshot({
        projectId: options.projectId,
        agentSessionId: options.agentSessionId,
        runId: options.runId,
        providerId: options.providerId,
        modelId: options.modelId,
        pendingPrompt: debouncedPendingPrompt,
      })
      .then((nextSnapshot) => {
        if (requestIdRef.current !== requestId) return
        setSnapshot(nextSnapshot)
        setStatus('ready')
        setError(null)
      })
      .catch((nextError) => {
        if (requestIdRef.current !== requestId) return
        setError(toContextMeterError(nextError))
        setStatus('error')
      })
  }, [
    debouncedPendingPrompt,
    options.adapter,
    options.agentSessionId,
    options.modelId,
    options.projectId,
    options.providerId,
    options.runId,
  ])

  useEffect(() => {
    refresh()
  }, [refresh, debouncedLifecycleKey])

  return { status, snapshot, error, refresh }
}

export const AgentRuntime = memo(function AgentRuntime({
  agent,
  onOpenSettings,
  onOpenDiagnostics,
  onStartRuntimeRun,
  onUpdateRuntimeRunControls,
  onComposerControlsChange,
  onStopRuntimeRun,
  onStartRuntimeSession,
  onResolveOperatorAction,
  onResumeOperatorRun,
  desktopAdapter,
  accountAvatarUrl = null,
  accountLogin = null,
  onCreateSession,
  isCreatingSession = false,
  customAgentDefinitions = [],
  onOpenAgentManagement,
  density = 'comfortable',
  paneNumber = null,
  paneCount = 1,
  onSpawnPane,
  spawnPaneDisabled = false,
  onClosePane,
  showCompactFirstRunTooltip = false,
  onAckCompactFirstRunTooltip,
  onPaneCloseStateChange,
}: AgentRuntimeProps) {
  const runtimeSession = agent.runtimeSession ?? null
  const runtimeRun = agent.runtimeRun ?? null
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const hasIncompleteRuntimeRunPayload = Boolean(runtimeRun && !renderableRuntimeRun)
  const runtimeStream = agent.runtimeStream ?? null
  const streamStatus = agent.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeStreamItems = agent.runtimeStreamItems ?? runtimeStream?.items ?? []
  const activityItems = agent.activityItems ?? runtimeStream?.activityItems ?? []
  const skillItems = agent.skillItems ?? runtimeStream?.skillItems ?? []
  const actionRequiredItems = agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? EMPTY_ACTION_REQUIRED_ITEMS
  const transcriptItems = runtimeStream?.transcriptItems ?? []
  const toolCalls = runtimeStream?.toolCalls ?? []
  const streamIssue = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const visibleTurns = useMemo(() => buildConversationTurns(runtimeStreamItems), [runtimeStreamItems])
  const pendingRuntimeRunAction = agent.pendingRuntimeRunAction ?? null
  const runtimeRunActionStatus = agent.runtimeRunActionStatus
  const isQueueingRuntimePrompt =
    runtimeRunActionStatus === 'running' &&
    (pendingRuntimeRunAction === 'start' || pendingRuntimeRunAction === 'update_controls')
  const showAgentActivityIndicator = Boolean(
    isQueueingRuntimePrompt ||
      agent.selectedPrompt.hasQueuedPrompt ||
      (
        renderableRuntimeRun?.isActive &&
        streamStatus !== 'complete' &&
        streamStatus !== 'error' &&
        !runtimeStream?.failure
      ),
  )
  const hasUserMessage = useMemo(
    () => runtimeStreamItems.some((item) => item.kind === 'transcript' && item.role === 'user'),
    [runtimeStreamItems],
  )
  const selectedAgentSession = (agent.project.selectedAgentSession ?? null) as AgentSessionView | null
  const selectedAgentSessionId =
    selectedAgentSession?.agentSessionId ?? agent.project.selectedAgentSessionId ?? null
  const hasSelectedAgentSession = Boolean(selectedAgentSessionId?.trim())

  const selectedProviderId =
    agent.selectedModel?.providerId ?? agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
  const selectedModelId =
    agent.selectedModel?.modelId?.trim() || agent.selectedModelId?.trim() || null
  const composerModelOptionsView = useMemo<AgentProviderModelView[]>(
    () =>
      (agent.composerModelOptions ?? []).map((option) => ({
        selectionKey: option.selectionKey,
        profileId: option.profileId,
        profileLabel: null,
        providerId: option.providerId,
        providerLabel: option.providerLabel,
        modelId: option.modelId,
        label: option.modelId,
        displayName: option.displayName,
        groupId: option.providerId,
        groupLabel: option.providerLabel,
        availability: 'available',
        availabilityLabel: 'Available',
        thinkingSupported: option.thinking.supported,
        thinkingEffortOptions: option.thinkingEffortOptions,
        defaultThinkingEffort: option.defaultThinkingEffort,
      })),
    [agent.composerModelOptions],
  )
  const availableModels = composerModelOptionsView
  const hasRepositoryBinding = Boolean(agent.repositoryPath?.trim())
  const agentRuntimeBlocked = agent.agentRuntimeBlocked ?? false
  const selectedProviderReadyForSession = !agentRuntimeBlocked
  const canMutateRuntimeRun = !agentRuntimeBlocked
  const canStartRuntimeSession = Boolean(
    canMutateRuntimeRun &&
      hasSelectedAgentSession &&
      hasRepositoryBinding &&
      typeof onStartRuntimeSession === 'function' &&
      selectedProviderReadyForSession &&
      (!runtimeSession?.isAuthenticated || runtimeSession.providerId !== selectedProviderId),
  )
  const canStartRuntimeRun = Boolean(
    canMutateRuntimeRun &&
      hasSelectedAgentSession &&
      hasRepositoryBinding &&
      typeof onStartRuntimeRun === 'function' &&
      runtimeSession?.isAuthenticated,
  )
  const canStopRuntimeRun = Boolean(
    hasSelectedAgentSession &&
      hasRepositoryBinding &&
      renderableRuntimeRun &&
      !renderableRuntimeRun.isTerminal &&
      typeof onStopRuntimeRun === 'function',
  )

  const [pendingAttachments, setPendingAttachments] = useState<ComposerPendingAttachment[]>([])
  const pendingAttachmentsRef = useRef<ComposerPendingAttachment[]>([])
  pendingAttachmentsRef.current = pendingAttachments

  const stageAgentAttachment = desktopAdapter?.stageAgentAttachment
  const discardAgentAttachment = desktopAdapter?.discardAgentAttachment
  const projectIdForAttachments = agent.project.id
  const runIdForAttachments = renderableRuntimeRun?.runId ?? 'pending'

  const handleAddFiles = useCallback(
    (files: File[]) => {
      if (files.length === 0 || !stageAgentAttachment) return
      for (const file of files) {
        const classification = classifyAttachment({
          name: file.name,
          type: file.type,
          size: file.size,
        })
        if (classification.kind === null) {
          console.warn(classificationRejectionMessage(file, classification))
          continue
        }
        const id = `attachment-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
        const previewUrl =
          classification.kind === 'image' && typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function'
            ? URL.createObjectURL(file)
            : undefined
        const optimistic: ComposerPendingAttachment = {
          id,
          kind: classification.kind,
          originalName: file.name,
          mediaType: classification.mediaType,
          sizeBytes: file.size,
          status: 'staging',
          previewUrl,
        }
        setPendingAttachments((prev) => [...prev, optimistic])
        void file
          .arrayBuffer()
          .then((buffer) =>
            stageAgentAttachment({
              projectId: projectIdForAttachments,
              runId: runIdForAttachments,
              originalName: file.name,
              mediaType: classification.mediaType,
              bytes: new Uint8Array(buffer),
            }),
          )
          .then((staged) => {
            setPendingAttachments((prev) =>
              prev.map((attachment) =>
                attachment.id === id
                  ? {
                      ...attachment,
                      status: 'ready',
                      absolutePath: staged.absolutePath,
                      sizeBytes: staged.sizeBytes,
                      mediaType: staged.mediaType,
                    }
                  : attachment,
              ),
            )
          })
          .catch((error: unknown) => {
            const message = error instanceof Error ? error.message : 'Upload failed'
            setPendingAttachments((prev) =>
              prev.map((attachment) =>
                attachment.id === id
                  ? { ...attachment, status: 'error', errorMessage: message }
                  : attachment,
              ),
            )
          })
      }
    },
    [projectIdForAttachments, runIdForAttachments, stageAgentAttachment],
  )

  const handleRemoveAttachment = useCallback(
    (id: string) => {
      setPendingAttachments((prev) => {
        const next = prev.filter((attachment) => attachment.id !== id)
        const removed = prev.find((attachment) => attachment.id === id)
        if (removed?.previewUrl && typeof URL !== 'undefined' && typeof URL.revokeObjectURL === 'function') {
          URL.revokeObjectURL(removed.previewUrl)
        }
        if (removed?.absolutePath && discardAgentAttachment) {
          void discardAgentAttachment(projectIdForAttachments, removed.absolutePath).catch(() => {
            // Best-effort cleanup; swallow errors so the chip still goes away.
          })
        }
        return next
      })
    },
    [discardAgentAttachment, projectIdForAttachments],
  )

  const getPendingAttachments = useCallback((): StagedAgentAttachmentDto[] => {
    return pendingAttachmentsRef.current
      .filter(
        (attachment): attachment is ComposerPendingAttachment & { absolutePath: string } =>
          attachment.status === 'ready' && typeof attachment.absolutePath === 'string',
      )
      .map((attachment) => ({
        kind: attachment.kind,
        absolutePath: attachment.absolutePath,
        mediaType: attachment.mediaType,
        originalName: attachment.originalName,
        sizeBytes: attachment.sizeBytes,
      }))
  }, [])

  const handleSubmitAttachmentsSettled = useCallback(() => {
    setPendingAttachments((prev) => {
      for (const attachment of prev) {
        if (attachment.previewUrl && typeof URL !== 'undefined' && typeof URL.revokeObjectURL === 'function') {
          URL.revokeObjectURL(attachment.previewUrl)
        }
      }
      return []
    })
  }, [])

  useEffect(() => {
    return () => {
      for (const attachment of pendingAttachmentsRef.current) {
        if (attachment.previewUrl && typeof URL !== 'undefined' && typeof URL.revokeObjectURL === 'function') {
          URL.revokeObjectURL(attachment.previewUrl)
        }
      }
    }
  }, [])

  const controller = useAgentRuntimeController({
    projectId: agent.project.id,
    selectedModelSelectionKey: agent.selectedModelSelectionKey ?? agent.selectedModelOption?.selectionKey ?? selectedModelId,
    selectedRuntimeAgentId: agent.selectedRuntimeAgentId,
    selectedAgentDefinitionId: agent.runtimeRunActiveControls?.agentDefinitionId ?? null,
    customAgentDefinitions,
    selectedThinkingEffort: agent.selectedThinkingEffort,
    selectedApprovalMode: agent.selectedApprovalMode,
    selectedPrompt: agent.selectedPrompt,
    availableModels,
    approvalRequests: agent.approvalRequests,
    operatorActionStatus: agent.operatorActionStatus,
    pendingOperatorActionId: agent.pendingOperatorActionId,
    renderableRuntimeRun,
    runtimeStream,
    runtimeStreamItems,
    runtimeRunActionStatus: agent.runtimeRunActionStatus,
    runtimeRunActionError: agent.runtimeRunActionError,
    canStartRuntimeRun,
    canStartRuntimeSession,
    canStopRuntimeRun,
    actionRequiredItems,
    dictationAdapter: desktopAdapter,
    dictationScopeKey: `${agent.project.id}:${agent.project.selectedAgentSessionId ?? 'none'}`,
    onStartRuntimeRun,
    onStartRuntimeSession,
    onUpdateRuntimeRunControls: canMutateRuntimeRun ? onUpdateRuntimeRunControls : undefined,
    onComposerControlsChange,
    onStopRuntimeRun,
    onResolveOperatorAction,
    onResumeOperatorRun,
    getPendingAttachments,
    onSubmitAttachmentsSettled: handleSubmitAttachmentsSettled,
  })

  const selectedComposerModel = useMemo(
    () => getComposerModelOption(availableModels, controller.composerModelId),
    [availableModels, controller.composerModelId],
  )
  const composerModelGroups = useMemo(
    () => getComposerModelGroups(availableModels, controller.composerModelId),
    [availableModels, controller.composerModelId],
  )
  const composerThinkingOptions = useMemo(
    () => getComposerThinkingOptions(selectedComposerModel),
    [selectedComposerModel],
  )
  const composerApprovalOptions = useMemo(
    () => getComposerApprovalOptions(controller.composerRuntimeAgentId),
    [controller.composerRuntimeAgentId],
  )
  const streamRunId = getStreamRunId(runtimeStream, renderableRuntimeRun)
  const contextMeterState = useAgentContextMeterSnapshot({
    adapter: desktopAdapter,
    projectId: agent.project.id,
    agentSessionId: selectedAgentSessionId,
    runId: renderableRuntimeRun?.runId ?? streamRunId ?? null,
    providerId: selectedComposerModel?.providerId ?? selectedProviderId,
    modelId: selectedComposerModel?.modelId ?? selectedModelId,
    pendingPrompt: controller.draftPrompt,
    lifecycleKey: [
      renderableRuntimeRun?.runId ?? 'no-run',
      renderableRuntimeRun?.updatedAt ?? 'no-run-update',
      runtimeStream?.status ?? 'idle',
      runtimeStream?.lastItemAt ?? 'no-stream-update',
      agent.pendingRuntimeRunAction ?? 'no-action',
    ].join(':'),
  })
  const contextMeter =
    contextMeterState.status === 'idle' ? null : (
      <AgentContextMeter
        status={contextMeterState.status}
        snapshot={contextMeterState.snapshot}
        hasUserMessage={hasUserMessage}
      />
    )
  const composerThinkingPlaceholder = controller.composerThinkingEffort
    ? getRuntimeRunThinkingEffortLabel(controller.composerThinkingEffort)
    : controller.composerModelId
      ? 'Thinking unavailable'
      : 'Choose model'
  const baseComposerPlaceholder = getComposerPlaceholder(
    runtimeSession,
    streamStatus,
    renderableRuntimeRun,
    streamRunId,
    {
      selectedProviderId,
      agentRuntimeBlocked,
    },
  )
  const composerPlaceholder =
    controller.composerRuntimeAgentId === 'ask' &&
    !agentRuntimeBlocked &&
    runtimeSession?.isAuthenticated &&
    !renderableRuntimeRun?.isTerminal
      ? 'Ask about this project...'
      : controller.composerRuntimeAgentId === 'agent_create' &&
          !agentRuntimeBlocked &&
          runtimeSession?.isAuthenticated &&
          !renderableRuntimeRun?.isTerminal
        ? 'Describe the agent you want to create...'
      : baseComposerPlaceholder
  const showAgentSetupEmptyState = Boolean(
    agentRuntimeBlocked &&
      (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
  )
  const hasSessionActivity = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      controller.recentRunReplacement ||
      streamIssue ||
      showAgentActivityIndicator ||
      transcriptItems.length > 0 ||
      activityItems.length > 0 ||
      toolCalls.length > 0 ||
      skillItems.length > 0 ||
      actionRequiredItems.length > 0 ||
      runtimeStream?.completion ||
      runtimeStream?.failure,
  )
  const promptInputLabel = controller.promptInputAvailable ? 'Agent input' : 'Agent input unavailable'
  const sendButtonLabel = controller.promptInputAvailable ? 'Send message' : 'Send message unavailable'
  const isProviderLoggedIn = Boolean(
    selectedProviderReadyForSession ||
      runtimeSession?.isAuthenticated,
  )
  const showEmptySessionState = Boolean(
    !showAgentSetupEmptyState && !agentRuntimeBlocked && isProviderLoggedIn && !hasSessionActivity,
  )
  const hasConversationViewportContent = Boolean(
    !showAgentSetupEmptyState && !showEmptySessionState && hasSessionActivity,
  )
  const projectLabel =
    agent.project.repository?.displayName ?? agent.project.name ?? 'this project'
  const sessionLabel = agent.project.selectedAgentSession?.title?.trim() || 'New Chat'
  const scrollViewportRef = useRef<HTMLDivElement | null>(null)
  const bottomSentinelRef = useRef<HTMLDivElement | null>(null)
  const shouldAutoFollowRef = useRef(true)
  const [showJumpToLatest, setShowJumpToLatest] = useState(false)
  const latestVisibleTurn = visibleTurns.at(-1)
  const conversationScrollKey = [
    latestVisibleTurn?.id ?? 'none',
    latestVisibleTurn?.sequence ?? 'none',
    latestVisibleTurn?.kind === 'message'
      ? latestVisibleTurn.text.length
      : latestVisibleTurn?.kind === 'action'
        ? `${latestVisibleTurn.state ?? 'unknown'}:${latestVisibleTurn.detail.length}`
        : latestVisibleTurn?.kind === 'failure'
          ? latestVisibleTurn.message.length
          : 'none',
    runtimeStream?.completion?.id ?? 'no-completion',
    runtimeStream?.failure?.id ?? 'no-failure',
    streamIssue?.code ?? 'no-issue',
  ].join(':')
  const scrollToLatest = useCallback((behavior: ScrollBehavior = 'auto') => {
    bottomSentinelRef.current?.scrollIntoView({
      block: 'end',
      inline: 'nearest',
      behavior,
    })
  }, [])
  const handleConversationScroll = useCallback(() => {
    const viewport = scrollViewportRef.current
    if (!viewport) {
      return
    }

    const isNearBottom = isRuntimeConversationNearBottom(viewport)
    shouldAutoFollowRef.current = isNearBottom
    setShowJumpToLatest(hasConversationViewportContent && !isNearBottom)
  }, [hasConversationViewportContent])
  const pauseConversationAutoFollow = useCallback(() => {
    if (!hasConversationViewportContent) {
      return
    }

    shouldAutoFollowRef.current = false
    setShowJumpToLatest(true)
  }, [hasConversationViewportContent])
  const handleConversationWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    const viewport = scrollViewportRef.current
    if (event.deltaY < 0 && viewport && viewport.scrollHeight > viewport.clientHeight) {
      pauseConversationAutoFollow()
    }
  }, [pauseConversationAutoFollow])
  const handleJumpToLatest = useCallback(() => {
    shouldAutoFollowRef.current = true
    setShowJumpToLatest(false)
    scrollToLatest('smooth')
  }, [scrollToLatest])
  const handleSubmitDraftPrompt = useCallback(() => {
    shouldAutoFollowRef.current = true
    setShowJumpToLatest(false)
    scrollToLatest('auto')
    void controller.handleSubmitDraftPrompt().finally(() => {
      scrollToLatest('auto')
    })
  }, [controller, scrollToLatest])

  useEffect(() => {
    if (!hasConversationViewportContent) {
      shouldAutoFollowRef.current = true
      setShowJumpToLatest(false)
      return
    }

    if (shouldAutoFollowRef.current) {
      scrollToLatest('auto')
      setShowJumpToLatest(false)
      return
    }

    setShowJumpToLatest(true)
  }, [conversationScrollKey, hasConversationViewportContent, scrollToLatest])

  const isCompact = density === 'compact'
  const isDense = paneCount >= 4
  const showPaneNumberChip = paneCount > 1 && paneNumber != null
  const showCloseButton = paneCount > 1 && typeof onClosePane === 'function'
  const closeState = useMemo<AgentPaneCloseState>(
    () => ({
      hasRunningRun: Boolean(renderableRuntimeRun && !renderableRuntimeRun.isTerminal),
      hasUnsavedComposerText: controller.draftPrompt.trim().length > 0,
      sessionTitle: sessionLabel,
    }),
    [controller.draftPrompt, renderableRuntimeRun, sessionLabel],
  )

  useEffect(() => {
    onPaneCloseStateChange?.(closeState)
  }, [closeState, onPaneCloseStateChange])

  return (
    <AgentPaneDropOverlay
      enabled={Boolean(stageAgentAttachment)}
      onFilesDropped={handleAddFiles}
    >
      <div className="flex min-h-0 min-w-0 flex-1">
        <div className="relative flex min-w-0 flex-1 flex-col">
        <div className="pointer-events-none absolute inset-x-0 top-0 z-20">
          <div className="flex items-center justify-between gap-1.5 bg-background px-3.5 py-2">
            <div className="pointer-events-auto flex min-w-0 items-center gap-1.5 text-[12.5px] text-muted-foreground">
              {showPaneNumberChip ? (
                <span
                  aria-label={`Pane ${paneNumber}`}
                  className="inline-flex h-[18px] shrink-0 items-center justify-center rounded-sm border border-border/60 bg-muted/40 px-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground"
                >
                  P{paneNumber}
                </span>
              ) : null}
              <span className="truncate font-semibold text-foreground">{projectLabel}</span>
              <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground/70" />
              <span className="truncate font-medium">{sessionLabel}</span>
            </div>
            <div className="pointer-events-auto flex items-center gap-1">
              {onCreateSession && paneCount === 1 ? (
                <button
                  type="button"
                  aria-label="New session"
                  onClick={onCreateSession}
                  disabled={isCreatingSession}
                  className={cn(
                    'inline-flex h-[30px] items-center gap-1.5 rounded-md px-2 text-[12.5px] font-semibold text-muted-foreground transition-colors',
                    'hover:bg-primary/10 hover:text-primary',
                    'disabled:cursor-not-allowed disabled:opacity-50',
                  )}
                >
                  {isCreatingSession ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Plus className="h-3.5 w-3.5" />
                  )}
                  <span>New Session</span>
                </button>
              ) : null}
              {onSpawnPane ? (
                <button
                  type="button"
                  aria-label={spawnPaneDisabled ? 'Pane limit reached' : 'Spawn agent pane'}
                  title={spawnPaneDisabled ? 'Pane limit reached' : 'Spawn agent pane'}
                  onClick={onSpawnPane}
                  disabled={spawnPaneDisabled}
                  className={cn(
                    'inline-flex h-[30px] w-[30px] items-center justify-center rounded-md text-muted-foreground transition-colors',
                    'hover:bg-primary/10 hover:text-primary',
                    'disabled:cursor-not-allowed disabled:opacity-40',
                  )}
                >
                  <SplitSquareHorizontal className="h-3.5 w-3.5" />
                </button>
              ) : null}
              {showCloseButton ? (
                <button
                  type="button"
                  aria-label="Close pane"
                  onClick={onClosePane}
                  className={cn(
                    'inline-flex h-[30px] w-[30px] items-center justify-center rounded-md text-muted-foreground transition-colors',
                    'hover:bg-destructive/10 hover:text-destructive',
                  )}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              ) : null}
            </div>
          </div>
          <div
            aria-hidden="true"
            className="h-7 bg-gradient-to-b from-background to-background/0"
          />
        </div>
        <div className="relative min-h-0 flex-1">
          <div
            aria-label="Agent conversation viewport"
            ref={scrollViewportRef}
            onScroll={handleConversationScroll}
            onWheel={handleConversationWheel}
            className={cn(
              showAgentSetupEmptyState || showEmptySessionState
                ? 'flex h-full items-center justify-center overflow-y-auto scrollbar-thin'
                : 'flex h-full overflow-y-auto scrollbar-thin',
              isDense
                ? showAgentSetupEmptyState || showEmptySessionState
                  ? 'px-2 py-2'
                  : 'px-2 pt-12'
                : showAgentSetupEmptyState || showEmptySessionState
                  ? 'px-6 py-5'
                  : 'px-4 pt-20',
            )}
          >
            {showAgentSetupEmptyState ? (
              <SetupEmptyState onOpenSettings={onOpenSettings} />
            ) : showEmptySessionState ? (
              <EmptySessionState
                projectLabel={projectLabel}
                variant={isDense ? 'dense' : 'default'}
                onSelectSuggestion={(prompt) => {
                  controller.handleDraftPromptChange(prompt)
                  controller.promptInputRef.current?.focus()
                }}
              />
            ) : (
              <div
                className={cn(
                  'mx-auto flex w-full flex-col',
                  isDense ? 'max-w-full gap-1' : 'max-w-[720px] gap-4',
                )}
              >
                <ConversationSection
                  runtimeRun={renderableRuntimeRun}
                  visibleTurns={visibleTurns}
                  streamIssue={streamIssue}
                  streamFailure={runtimeStream?.failure ?? null}
                  showActivityIndicator={showAgentActivityIndicator}
                  streamCompletion={runtimeStream?.completion ?? null}
                  accountAvatarUrl={accountAvatarUrl}
                  accountLogin={accountLogin}
                  variant={isDense ? 'dense' : 'default'}
                />
                {controller.composerRuntimeAgentId === 'agent_create' ? (
                  <AgentCreateDraftSection
                    runtimeStreamItems={runtimeStreamItems}
                    pendingApprovalCount={agent.pendingApprovalCount ?? 0}
                    onOpenAgentManagement={onOpenAgentManagement}
                  />
                ) : null}
                <div ref={bottomSentinelRef} aria-hidden="true" className="h-1 shrink-0 scroll-mb-8" />
              </div>
            )}
          </div>
          {showJumpToLatest ? (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={handleJumpToLatest}
              className={cn(
                'absolute bottom-4 left-1/2 z-30 -translate-x-1/2 gap-1.5 rounded-full',
                'border border-border/60 bg-card/95 px-3 shadow-lg backdrop-blur',
                'supports-[backdrop-filter]:bg-card/80',
              )}
            >
              <ArrowDown className="h-3.5 w-3.5" />
              Jump to latest
            </Button>
          ) : null}
        </div>

        <ComposerDock
          density={density}
          showCompactFirstRunTooltip={showCompactFirstRunTooltip}
          onAckCompactFirstRunTooltip={onAckCompactFirstRunTooltip}
          composerRuntimeAgentId={controller.composerRuntimeAgentId}
          composerRuntimeAgentLabel={getRuntimeAgentLabel(controller.composerRuntimeAgentId)}
          composerAgentDefinitionId={controller.composerAgentDefinitionId}
          composerAgentSelectionKey={controller.composerAgentSelectionKey}
          customAgentDefinitions={customAgentDefinitions}
          composerApprovalMode={controller.composerApprovalMode}
          composerApprovalOptions={composerApprovalOptions}
          autoCompactEnabled={controller.autoCompactEnabled}
          composerModelGroups={composerModelGroups}
          composerModelId={controller.composerModelId}
          composerThinkingLevel={controller.composerThinkingEffort}
          composerThinkingOptions={composerThinkingOptions}
          composerThinkingPlaceholder={composerThinkingPlaceholder}
          controlsDisabled={controller.areControlsDisabled}
          runtimeAgentSwitchDisabled={controller.isRuntimeAgentSwitchDisabled}
          dictation={controller.dictation}
          contextMeter={contextMeter}
          draftPrompt={controller.draftPrompt}
          isPromptDisabled={controller.isPromptDisabled}
          isSendDisabled={!controller.canSubmitPrompt}
          onComposerApprovalModeChange={controller.handleComposerApprovalModeChange}
          onComposerRuntimeAgentChange={controller.handleComposerRuntimeAgentChange}
          onComposerAgentSelectionChange={controller.handleComposerAgentSelectionChange}
          onAutoCompactEnabledChange={controller.handleAutoCompactEnabledChange}
          onComposerModelChange={controller.handleComposerModelChange}
          onComposerThinkingLevelChange={controller.handleComposerThinkingLevelChange}
          onDraftPromptChange={controller.handleDraftPromptChange}
          onSubmitDraftPrompt={handleSubmitDraftPrompt}
          pendingAttachments={pendingAttachments}
          onRemoveAttachment={handleRemoveAttachment}
          pendingRuntimeRunAction={pendingRuntimeRunAction}
          placeholder={composerPlaceholder}
          promptInputRef={controller.promptInputRef}
          promptInputLabel={promptInputLabel}
          runtimeSessionBindInFlight={controller.runtimeSessionBindInFlight}
          runtimeRunActionError={controller.runtimeRunActionError}
          runtimeRunActionErrorTitle={controller.runtimeRunActionErrorTitle}
          runtimeRunActionStatus={runtimeRunActionStatus}
          sendButtonLabel={sendButtonLabel}
          onOpenDiagnostics={onOpenDiagnostics}
        />
        </div>
      </div>
    </AgentPaneDropOverlay>
  )
})
