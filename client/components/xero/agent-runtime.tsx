"use client"

import { memo, useCallback, useEffect, useMemo, useRef, useState, type WheelEvent } from 'react'
import {
  ArrowDown,
  Check,
  ChevronDown,
  ChevronRight,
  Loader2,
  Plus,
  SplitSquareHorizontal,
  X,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
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
  CodeHistoryOperationDto,
  RuntimeAgentIdDto,
  RuntimeRunView,
  RuntimeAgentProjectOrigin,
  RuntimeAutoCompactPreferenceDto,
  ProviderAuthSessionView,
  RuntimeSessionView,
  RuntimeStreamActionRequiredItemView,
  RuntimeStreamActivityItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
  ReturnSessionToHereResponseDto,
  SelectiveUndoResponseDto,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/xero-model'
import type { SessionContextSnapshotDto } from '@/src/lib/xero-model/session-context'
import {
  getRuntimeAgentDescriptorsForProjectOrigin,
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
import {
  ActionPromptDispatchProvider,
  type ActionPromptDispatchValue,
} from './agent-runtime/action-prompt-card'
import { ComposerDock, type ComposerPendingAttachment } from './agent-runtime/composer-dock'
import { PlanTray } from './agent-runtime/plan-tray'
import { AgentPaneDropOverlay } from './agent-runtime/agent-pane-drop-overlay'
import { AgentCreateDraftSection } from './agent-runtime/agent-create-draft-section'
import {
  ConversationSection,
  getCodeUndoStateKey,
  getReturnSessionToHereStateKey,
  type CodeUndoRequest,
  type CodeUndoConflictSummary,
  type CodeUndoUiState,
  type ConversationTurn,
  type ReturnSessionToHereUiRequest,
} from './agent-runtime/conversation-section'
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

export type AgentRuntimeDesktopAdapter = SpeechDictationAdapter &
  Partial<
    Pick<
      XeroDesktopAdapter,
      | 'applySelectiveUndo'
      | 'returnSessionToHere'
      | 'getSessionContextSnapshot'
      | 'getSessionTranscript'
      | 'stageAgentAttachment'
      | 'discardAgentAttachment'
    >
  >

export interface AgentRuntimeProps {
  agent: AgentPaneView
  /** True while this pane belongs to the foreground app view. */
  active?: boolean
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
  onCodeUndoApplied?: () => Promise<unknown> | unknown
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
  /** Open the by-hand agent builder form. */
  onCreateAgentByHand?: () => void
  /** Move to Workflow and begin a new canvas-backed Agent Create session. */
  onStartWorkflowAgentCreate?: () => void
  /** True when Agent Create is paired with the visible workflow authoring canvas. */
  agentCreateCanvasIncluded?: boolean
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
  /** Reports pane-local state that should block an immediate close. */
  onPaneCloseStateChange?: (state: AgentPaneCloseState) => void
  /** Drag-handle bindings (from `useSortable`). When provided, the pane header acts as the drag activator. */
  dragHandle?: {
    setActivatorNodeRef?: (node: HTMLElement | null) => void
    attributes?: Record<string, unknown>
    listeners?: Record<string, unknown>
    isDragging?: boolean
  }
  /** Render the pane header in sidebar context: bg matches sidebar surface and the header hosts close + session controls. */
  inSidebar?: boolean
  /** Sessions to show in the header session switcher when rendered inside the sidebar. */
  sidebarSessions?: readonly AgentSessionView[]
  /** Switch to a different session from the sidebar header dropdown. */
  onSelectSidebarSession?: (agentSessionId: string) => void
  /** Close the sidebar from the agent header (X button). */
  onCloseSidebar?: () => void
  /**
   * Historical conversation turns from prior runs in this agent session
   * (loaded from the persisted session transcript). When provided, they are
   * prepended ahead of the live stream so a same-session handoff reads as a
   * continuous conversation. Items belonging to the active run must already
   * be excluded by the caller.
   */
  historicalConversationTurns?: readonly ConversationTurn[]
  /**
   * One-shot runtime agent to apply to the composer when this pane mounts or
   * when the value changes to a new non-null id. Used by "Create agent" entry
   * points that open a fresh session pre-selected on a specific agent.
   */
  pendingInitialRuntimeAgentId?: RuntimeAgentIdDto | null
  /** Called once the pending initial runtime agent has been applied. */
  onPendingInitialRuntimeAgentIdConsumed?: () => void
}

const EMPTY_RUNTIME_STREAM_ITEMS: RuntimeStreamViewItem[] = []
const EMPTY_ACTION_REQUIRED_ITEMS: NonNullable<AgentPaneView['actionRequiredItems']> = []
const MAX_VISIBLE_RUNTIME_ACTION_TURNS = Number.POSITIVE_INFINITY
const COMPACT_TOOL_BURST_THRESHOLD = 2
const CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX = 96
const BACKGROUND_PANE_STREAM_ITEM_LIMIT = 160
const BACKGROUND_PANE_VISIBLE_TURN_LIMIT = 48
const FOREGROUND_WORK_DEFER_MS = 32
const STREAMING_TOOL_OUTPUT_MAX_CHARS = 24_000
const CONTEXT_METER_REFRESH_IDLE_TIMEOUT_MS = 1200
const CONTEXT_METER_REFRESH_FALLBACK_DELAY_MS = 220
const CODE_EDIT_TOOL_NAMES = new Set(['edit', 'patch', 'write', 'apply_patch', 'notebook_edit'])

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

function isFileChangeActivityItem(item: RuntimeStreamViewItem): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === 'owned_agent_file_changed'
}

function getReasoningActivityText(item: RuntimeStreamActivityItemView): string {
  return item.text ?? item.detail ?? ''
}

function parseFileChangeActivityDetail(detail: string | null): {
  operation: string
  path: string
  toPath: string | null
} {
  const fallback = {
    operation: 'changed',
    path: 'unknown path',
    toPath: null,
  }
  if (!detail) {
    return fallback
  }

  const summary = detail.split(' · ')[0]?.trim() ?? ''
  const match = /^([^:]+):\s*(.+)$/.exec(summary)
  if (!match) {
    return {
      ...fallback,
      path: summary || fallback.path,
    }
  }

  const operation = match[1]?.trim() || fallback.operation
  const pathSegment = match[2]?.trim() || fallback.path
  const renameSeparator = ' -> '
  const renameIndex = pathSegment.indexOf(renameSeparator)
  if (renameIndex >= 0) {
    const path = pathSegment.slice(0, renameIndex).trim()
    const toPath = pathSegment.slice(renameIndex + renameSeparator.length).trim()
    return {
      operation,
      path: path || fallback.path,
      toPath: toPath || null,
    }
  }

  return {
    operation,
    path: pathSegment,
    toPath: null,
  }
}

function isCodeEditToolName(toolName: string): boolean {
  return CODE_EDIT_TOOL_NAMES.has(toolName)
}

function actionPromptTurnFromItem(item: RuntimeStreamActionRequiredItemView): ConversationTurn {
  const shape = item.answerShape ?? 'plain_text'
  return {
    id: item.id,
    kind: 'action_prompt',
    sequence: item.sequence,
    actionId: item.actionId,
    actionType: item.actionType,
    title: item.title,
    detail: item.detail,
    shape,
    options: item.options ?? null,
    allowMultiple: item.allowMultiple ?? shape === 'multi_choice',
    pendingDecision: null,
    isResolved: false,
  }
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
    defaultOpen: isCodeEditToolName(item.toolName),
  }
}

function fileChangeTurnFromItem(item: RuntimeStreamActivityItemView): ConversationTurn {
  const detail = item.detail ?? item.text ?? 'File changed.'
  const parsed = parseFileChangeActivityDetail(detail)

  return {
    id: item.id,
    kind: 'file_change',
    runId: item.runId,
    sequence: item.sequence,
    title: item.title,
    detail,
    operation: parsed.operation,
    path: parsed.path,
    toPath: parsed.toPath,
    changeGroupId: item.codeChangeGroupId ?? null,
    workspaceEpoch: item.codeWorkspaceEpoch ?? null,
    patchAvailability: item.codePatchAvailability ?? null,
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

  if (item.toolResultPreview && item.toolResultPreview !== item.detail && item.toolResultPreview !== summary) {
    rows.push({
      label: 'Output',
      value: item.toolResultPreview,
    })
  }

  return rows
}

function normalizeStreamOutputPreview(value: string): string {
  return value
    .replace(/(^|\n)(stdout|stderr|output):\n/g, '\n')
    .replace(/\n{2,}/g, '\n')
    .trim()
}

function outputPreviewContains(container: string, contained: string): boolean {
  const normalizedContainer = normalizeStreamOutputPreview(container)
  const normalizedContained = normalizeStreamOutputPreview(contained)
  return normalizedContained.length > 0 && normalizedContainer.includes(normalizedContained)
}

function mergeActionRows(
  existing: Array<{ label: string; value: string }>,
  incoming: Array<{ label: string; value: string }>,
): Array<{ label: string; value: string }> {
  const merged = existing.map((row) => ({ ...row }))
  const seen = new Set(merged.map((row) => `${row.label}\u0000${row.value}`))

  for (const row of incoming) {
    const key = `${row.label}\u0000${row.value}`
    if (/output/i.test(row.label)) {
      const outputRow = merged.find((existingRow) => /output/i.test(existingRow.label))
      if (outputRow) {
        if (outputPreviewContains(row.value, outputRow.value)) {
          outputRow.value = row.value
        } else if (!outputPreviewContains(outputRow.value, row.value) && !outputRow.value.includes(row.value)) {
          const nextValue = `${outputRow.value}\n${row.value}`.trim()
          outputRow.value =
            nextValue.length > STREAMING_TOOL_OUTPUT_MAX_CHARS
              ? nextValue.slice(nextValue.length - STREAMING_TOOL_OUTPUT_MAX_CHARS)
              : nextValue
        }
        continue
      }
    }

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
  existing.defaultOpen = Boolean(existing.defaultOpen || incoming.defaultOpen)

  if (incoming.title.length >= existing.title.length) {
    existing.title = incoming.title
  }
}

function scheduleForegroundWork(callback: () => void): () => void {
  if (typeof window === 'undefined') {
    callback()
    return () => {}
  }

  let cancelled = false
  let frameId = 0
  let timeoutId = 0
  const run = () => {
    if (!cancelled) {
      callback()
    }
  }

  if (typeof window.requestAnimationFrame === 'function') {
    frameId = window.requestAnimationFrame(() => {
      timeoutId = window.setTimeout(run, FOREGROUND_WORK_DEFER_MS)
    })
  } else {
    timeoutId = window.setTimeout(run, FOREGROUND_WORK_DEFER_MS)
  }

  return () => {
    cancelled = true
    if (frameId !== 0) {
      window.cancelAnimationFrame(frameId)
    }
    if (timeoutId !== 0) {
      window.clearTimeout(timeoutId)
    }
  }
}

function scheduleAfterNextPaint(callback: () => void): () => void {
  if (typeof window === 'undefined') {
    callback()
    return () => {}
  }

  let cancelled = false
  let frameId = 0
  let timeoutId = 0
  const run = () => {
    if (!cancelled) {
      callback()
    }
  }

  if (typeof window.requestAnimationFrame === 'function') {
    frameId = window.requestAnimationFrame(() => {
      timeoutId = window.setTimeout(run, 0)
    })
  } else {
    timeoutId = window.setTimeout(run, 0)
  }

  return () => {
    cancelled = true
    if (frameId !== 0 && typeof window.cancelAnimationFrame === 'function') {
      window.cancelAnimationFrame(frameId)
    }
    if (timeoutId !== 0) {
      window.clearTimeout(timeoutId)
    }
  }
}

function useDeferredForegroundWork(active: boolean): boolean {
  const [ready, setReady] = useState(active)

  useEffect(() => {
    if (!active) {
      setReady(false)
      return
    }

    if (ready) {
      return
    }

    return scheduleForegroundWork(() => setReady(true))
  }, [active, ready])

  return active && ready
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

function isCodeEditAction(turn: Extract<ConversationTurn, { kind: 'action' }>): boolean {
  return isCodeEditToolName(turn.toolName)
}

function isTerminalActionState(
  state: RuntimeStreamToolItemView['toolState'] | null | undefined,
): boolean {
  return state === 'succeeded' || state === 'failed'
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
      detailRows: action.detailRows,
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
      if (isCodeEditAction(turn) || !isTerminalActionState(turn.state)) {
        flushActionBuffer()
        compactedTurns.push(turn)
        continue
      }
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

interface ConversationProjection {
  visibleTurns: ConversationTurn[]
  hasUserMessage: boolean
}

interface PendingPromptTurn {
  id: string
  text: string
  queuedAt: string | null
}

const conversationProjectionCache = new WeakMap<readonly RuntimeStreamViewItem[], ConversationProjection>()

function buildConversationProjection(runtimeStreamItems: readonly RuntimeStreamViewItem[]): ConversationProjection {
  const turns: ConversationTurn[] = []
  const actionTurnIndexByToolCallId = new Map<string, number>()
  let hasUserMessage = false

  for (const item of runtimeStreamItems) {
    if (item.kind === 'transcript') {
      if (item.role !== 'user' && item.role !== 'assistant') {
        continue
      }

      if (item.role === 'user') {
        hasUserMessage = true
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

    if (isFileChangeActivityItem(item)) {
      turns.push(fileChangeTurnFromItem(item))
      continue
    }

    if (item.kind === 'action_required') {
      turns.push(actionPromptTurnFromItem(item))
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

  return {
    visibleTurns: limitActionTurns(compactActionBursts(turns)),
    hasUserMessage,
  }
}

function getConversationProjection(
  runtimeStreamItems: readonly RuntimeStreamViewItem[],
): ConversationProjection {
  const cached = conversationProjectionCache.get(runtimeStreamItems)
  if (cached) {
    return cached
  }

  const projection = buildConversationProjection(runtimeStreamItems)
  conversationProjectionCache.set(runtimeStreamItems, projection)
  return projection
}

function sliceBackgroundPaneStreamItems(
  runtimeStreamItems: RuntimeStreamViewItem[],
): RuntimeStreamViewItem[] {
  if (runtimeStreamItems.length <= BACKGROUND_PANE_STREAM_ITEM_LIMIT) {
    return runtimeStreamItems
  }
  return runtimeStreamItems.slice(-BACKGROUND_PANE_STREAM_ITEM_LIMIT)
}

function sliceBackgroundPaneTurns(visibleTurns: ConversationTurn[]): ConversationTurn[] {
  if (visibleTurns.length <= BACKGROUND_PANE_VISIBLE_TURN_LIMIT) {
    return visibleTurns
  }
  return visibleTurns.slice(-BACKGROUND_PANE_VISIBLE_TURN_LIMIT)
}

function hasTranscriptForPendingPrompt(
  runtimeStreamItems: readonly RuntimeStreamViewItem[],
  pendingPrompt: PendingPromptTurn | null,
): boolean {
  if (!pendingPrompt) {
    return false
  }

  const promptText = pendingPrompt.text.trim()
  if (!promptText) {
    return false
  }

  const queuedAtMs = Date.parse(pendingPrompt.queuedAt ?? '')
  const hasQueuedTimestamp = Number.isFinite(queuedAtMs)

  return runtimeStreamItems.some((item) => {
    if (item.kind !== 'transcript' || item.role !== 'user' || item.text.trim() !== promptText) {
      return false
    }

    if (!hasQueuedTimestamp) {
      return true
    }

    const itemCreatedAtMs = Date.parse(item.createdAt)
    return Number.isFinite(itemCreatedAtMs) && itemCreatedAtMs >= queuedAtMs - 5_000
  })
}

function appendPendingPromptTurn(
  turns: ConversationTurn[],
  pendingPrompt: PendingPromptTurn | null,
): ConversationTurn[] {
  if (!pendingPrompt) {
    return turns
  }

  const text = pendingPrompt.text.trim()
  if (!text) {
    return turns
  }

  const latestSequence = turns.reduce((current, turn) => Math.max(current, turn.sequence), 0)
  return [
    ...turns,
    {
      id: `pending-prompt:${pendingPrompt.id}`,
      kind: 'message',
      role: 'user',
      sequence: latestSequence + 0.5,
      text,
    },
  ]
}

function getContextMeterRequestKey(options: {
  projectId: string
  agentSessionId: string | null
  runId: string | null
  providerId: string | null
  modelId: string | null
  pendingPrompt: string
  lifecycleKey: string
}): string {
  return [
    options.projectId,
    options.agentSessionId ?? '',
    options.runId ?? '',
    options.providerId ?? '',
    options.modelId ?? '',
    options.pendingPrompt,
    options.lifecycleKey,
  ].join('\u0000')
}

function scheduleContextMeterRefresh(callback: () => void): () => void {
  if (typeof window === 'undefined') {
    callback()
    return () => {}
  }

  const idleWindow = window as Window & {
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number
    cancelIdleCallback?: (handle: number) => void
  }

  let cancelled = false
  const run = () => {
    if (!cancelled) {
      callback()
    }
  }

  if (typeof idleWindow.requestIdleCallback === 'function') {
    const idleHandle = idleWindow.requestIdleCallback(run, {
      timeout: CONTEXT_METER_REFRESH_IDLE_TIMEOUT_MS,
    })
    return () => {
      cancelled = true
      idleWindow.cancelIdleCallback?.(idleHandle)
    }
  }

  const timeout = window.setTimeout(run, CONTEXT_METER_REFRESH_FALLBACK_DELAY_MS)
  return () => {
    cancelled = true
    window.clearTimeout(timeout)
  }
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

function getCodeUndoErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }

  if (typeof error === 'string' && error.trim().length > 0) {
    return error
  }

  const maybeError = error as { message?: unknown } | null
  if (typeof maybeError?.message === 'string' && maybeError.message.trim().length > 0) {
    return maybeError.message
  }

  return 'Xero could not undo this change.'
}

function getAgentProjectOrigin(project: AgentPaneView['project']): RuntimeAgentProjectOrigin {
  return (project as AgentPaneView['project'] & { projectOrigin?: RuntimeAgentProjectOrigin })
    .projectOrigin ?? 'unknown'
}

function createCodeHistoryOperationId(prefix: string): string {
  const randomId = globalThis.crypto?.randomUUID?.()
  if (randomId) {
    return `${prefix}-${randomId}`
  }

  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

function formatCodeHistoryTargetLabel(operation: CodeHistoryOperationDto): string {
  switch (operation.target.targetKind) {
    case 'file_change':
      return `Selected file: ${operation.target.targetId}`
    case 'hunks': {
      const hunkLabel =
        operation.target.hunkIds.length > 0
          ? operation.target.hunkIds.join(', ')
          : operation.target.targetId
      return `Selected hunks: ${hunkLabel}`
    }
    case 'change_group':
      return `Selected change group: ${operation.target.targetId}`
    case 'run_boundary':
      return `Selected run boundary: ${operation.target.targetId}`
    case 'session_boundary':
      return `Selected session boundary: ${operation.target.targetId}`
    default:
      return `Selected target: ${operation.target.targetId}`
  }
}

function buildCodeHistoryConflictSummary(
  operation: CodeHistoryOperationDto,
  title: string,
): CodeUndoConflictSummary | undefined {
  if (operation.status !== 'conflicted' || operation.conflicts.length === 0) {
    return undefined
  }

  return {
    title,
    targetLabel: formatCodeHistoryTargetLabel(operation),
    affectedPaths: operation.affectedPaths,
    conflicts: operation.conflicts,
  }
}

function formatCodeUndoResult(response: SelectiveUndoResponseDto): CodeUndoUiState {
  const { operation } = response
  const affectedCount = operation.affectedPaths.length || 1
  if (operation.status === 'completed') {
    return {
      status: 'succeeded',
      message: `Undid ${affectedCount} ${affectedCount === 1 ? 'path' : 'paths'}.`,
    }
  }

  if (operation.status === 'conflicted') {
    const conflict = operation.conflicts[0]
    const path = conflict?.path ?? operation.affectedPaths[0] ?? 'the selected change'
    const detail = conflict?.message ? ` ${conflict.message}` : ''
    return {
      status: 'failed',
      message: `Undo blocked by conflict in ${path}.${detail}`,
      conflictSummary: buildCodeHistoryConflictSummary(operation, 'Undo conflict'),
    }
  }

  if (operation.status === 'pending' || operation.status === 'planning' || operation.status === 'applying') {
    return {
      status: 'pending',
      message: 'Undo is still applying...',
    }
  }

  return {
    status: 'failed',
    message: 'Undo failed before changing files.',
  }
}

function formatReturnSessionToHereResult(response: ReturnSessionToHereResponseDto): CodeUndoUiState {
  const { operation } = response
  if (operation.status === 'completed') {
    return {
      status: 'succeeded',
      message: 'Return session history event added. Other sessions are unchanged.',
    }
  }

  if (operation.status === 'conflicted') {
    const conflict = operation.conflicts[0]
    const path = conflict?.path ?? operation.affectedPaths[0] ?? 'the selected boundary'
    const detail = conflict?.message ? ` ${conflict.message}` : ''
    return {
      status: 'failed',
      message: `Return session blocked by conflict in ${path}.${detail}`,
      conflictSummary: buildCodeHistoryConflictSummary(operation, 'Return session conflict'),
    }
  }

  if (operation.status === 'pending' || operation.status === 'planning' || operation.status === 'applying') {
    return {
      status: 'pending',
      message: 'Return session is still applying...',
    }
  }

  return {
    status: 'failed',
    message: 'Xero could not return this session to here.',
  }
}

function useAgentContextMeterSnapshot(options: {
  enabled?: boolean
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
  const inFlightRequestKeyRef = useRef<string | null>(null)
  const settledRequestKeyRef = useRef<string | null>(null)
  const enabled = options.enabled ?? true
  const requestKey = useMemo(
    () =>
      getContextMeterRequestKey({
        projectId: options.projectId,
        agentSessionId: options.agentSessionId,
        runId: options.runId,
        providerId: options.providerId,
        modelId: options.modelId,
        pendingPrompt: debouncedPendingPrompt,
        lifecycleKey: debouncedLifecycleKey,
      }),
    [
      debouncedLifecycleKey,
      debouncedPendingPrompt,
      options.agentSessionId,
      options.modelId,
      options.projectId,
      options.providerId,
      options.runId,
    ],
  )

  useEffect(() => {
    snapshotRef.current = snapshot
  }, [snapshot])

  const runRefresh = useCallback((refreshOptions: { force?: boolean } = {}) => {
    if (!enabled) {
      requestIdRef.current += 1
      inFlightRequestKeyRef.current = null
      return
    }

    if (!options.adapter?.getSessionContextSnapshot || !options.agentSessionId) {
      requestIdRef.current += 1
      inFlightRequestKeyRef.current = null
      settledRequestKeyRef.current = null
      snapshotRef.current = null
      setStatus('idle')
      setSnapshot(null)
      setError(null)
      return
    }

    if (!refreshOptions.force) {
      if (settledRequestKeyRef.current === requestKey && snapshotRef.current) {
        setStatus((current) => (current === 'ready' ? current : 'ready'))
        return
      }

      if (inFlightRequestKeyRef.current === requestKey) {
        return
      }
    }

    const requestId = requestIdRef.current + 1
    requestIdRef.current = requestId
    inFlightRequestKeyRef.current = requestKey
    setStatus(snapshotRef.current ? 'stale' : 'loading')
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
        inFlightRequestKeyRef.current = null
        settledRequestKeyRef.current = requestKey
        snapshotRef.current = nextSnapshot
        setSnapshot(nextSnapshot)
        setStatus('ready')
        setError(null)
      })
      .catch((nextError) => {
        if (requestIdRef.current !== requestId) return
        inFlightRequestKeyRef.current = null
        settledRequestKeyRef.current = null
        setError(toContextMeterError(nextError))
        setStatus('error')
      })
  }, [
    debouncedPendingPrompt,
    enabled,
    options.adapter,
    options.agentSessionId,
    options.modelId,
    options.projectId,
    options.providerId,
    options.runId,
    requestKey,
  ])

  useEffect(() => {
    if (!enabled) {
      requestIdRef.current += 1
      inFlightRequestKeyRef.current = null
      return
    }

    if (!options.adapter?.getSessionContextSnapshot || !options.agentSessionId) {
      runRefresh({ force: true })
      return
    }

    return scheduleContextMeterRefresh(() => runRefresh())
  }, [enabled, options.adapter, options.agentSessionId, requestKey, runRefresh])

  const refresh = useCallback(() => {
    runRefresh({ force: true })
  }, [runRefresh])

  return { status, snapshot, error, refresh }
}

export const AgentRuntime = memo(function AgentRuntime({
  agent,
  active = true,
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
  onCodeUndoApplied,
  accountAvatarUrl = null,
  accountLogin = null,
  onCreateSession,
  isCreatingSession = false,
  customAgentDefinitions = [],
  onOpenAgentManagement,
  onCreateAgentByHand,
  onStartWorkflowAgentCreate,
  agentCreateCanvasIncluded = false,
  density = 'comfortable',
  paneNumber = null,
  paneCount = 1,
  onSpawnPane,
  spawnPaneDisabled = false,
  onClosePane,
  isPaneFocused,
  onPaneCloseStateChange,
  dragHandle,
  inSidebar = false,
  sidebarSessions,
  onSelectSidebarSession,
  onCloseSidebar,
  historicalConversationTurns,
  pendingInitialRuntimeAgentId = null,
  onPendingInitialRuntimeAgentIdConsumed,
}: AgentRuntimeProps) {
  const runtimeSession = agent.runtimeSession ?? null
  const runtimeRun = agent.runtimeRun ?? null
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const hasIncompleteRuntimeRunPayload = Boolean(runtimeRun && !renderableRuntimeRun)
  const runtimeStream = agent.runtimeStream ?? null
  const streamStatus = agent.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeStreamItems = agent.runtimeStreamItems ?? runtimeStream?.items ?? EMPTY_RUNTIME_STREAM_ITEMS
  const activityItems = agent.activityItems ?? runtimeStream?.activityItems ?? []
  const skillItems = agent.skillItems ?? runtimeStream?.skillItems ?? []
  const actionRequiredItems = agent.actionRequiredItems ?? runtimeStream?.actionRequired ?? EMPTY_ACTION_REQUIRED_ITEMS
  const transcriptItems = runtimeStream?.transcriptItems ?? []
  const toolCalls = runtimeStream?.toolCalls ?? []
  const streamIssue = agent.runtimeStreamError ?? runtimeStream?.lastIssue ?? null
  const isFocusedPane = paneCount <= 1 || isPaneFocused !== false
  const foregroundWorkReady = useDeferredForegroundWork(active)
  const useBackgroundPaneFastPath = paneCount >= 3 && !isFocusedPane
  const runtimeStreamItemsForTurns = useMemo(
    () =>
      useBackgroundPaneFastPath
        ? sliceBackgroundPaneStreamItems(runtimeStreamItems)
        : runtimeStreamItems,
    [runtimeStreamItems, useBackgroundPaneFastPath],
  )
  const conversationProjection = useMemo(
    () => getConversationProjection(runtimeStreamItemsForTurns),
    [runtimeStreamItemsForTurns],
  )
  const visibleTurns = conversationProjection.visibleTurns
  // Historical turns from prior runs in this agent session (loaded from the
  // persisted session transcript) are prepended so the conversation reads as
  // a continuous thread across same-session handoffs. The active run's items
  // come from the live stream and are excluded from `historicalConversationTurns`
  // by the caller, so there is no duplication to dedupe here.
  const visibleTurnsWithHistory = useMemo(
    () =>
      historicalConversationTurns && historicalConversationTurns.length > 0
        ? [...historicalConversationTurns, ...visibleTurns]
        : visibleTurns,
    [historicalConversationTurns, visibleTurns],
  )
  const visibleTurnsForDisplay = useMemo(
    () =>
      useBackgroundPaneFastPath
        ? sliceBackgroundPaneTurns(visibleTurnsWithHistory)
        : visibleTurnsWithHistory,
    [useBackgroundPaneFastPath, visibleTurnsWithHistory],
  )
  const [optimisticPromptTurn, setOptimisticPromptTurn] = useState<PendingPromptTurn | null>(null)
  const selectedQueuedPromptTurn = useMemo<PendingPromptTurn | null>(() => {
    const text = agent.selectedPrompt.text?.trim()
    if (!agent.selectedPrompt.hasQueuedPrompt || !text) {
      return null
    }

    return {
      id: `queued:${agent.selectedPrompt.queuedAt ?? text}`,
      text,
      queuedAt: agent.selectedPrompt.queuedAt ?? null,
    }
  }, [agent.selectedPrompt.hasQueuedPrompt, agent.selectedPrompt.queuedAt, agent.selectedPrompt.text])
  const pendingPromptTurn = useMemo<PendingPromptTurn | null>(() => {
    if (optimisticPromptTurn && !hasTranscriptForPendingPrompt(runtimeStreamItems, optimisticPromptTurn)) {
      return optimisticPromptTurn
    }

    if (selectedQueuedPromptTurn && !hasTranscriptForPendingPrompt(runtimeStreamItems, selectedQueuedPromptTurn)) {
      return selectedQueuedPromptTurn
    }

    return null
  }, [optimisticPromptTurn, runtimeStreamItems, selectedQueuedPromptTurn])
  const visibleTurnsWithPendingPrompt = useMemo(
    () => appendPendingPromptTurn(visibleTurnsForDisplay, pendingPromptTurn),
    [pendingPromptTurn, visibleTurnsForDisplay],
  )
  const [promptSubmissionPending, setPromptSubmissionPending] = useState(false)
  const promptSubmissionCancelRef = useRef<(() => void) | null>(null)
  useEffect(() => {
    return () => {
      promptSubmissionCancelRef.current?.()
      promptSubmissionCancelRef.current = null
    }
  }, [])
  const pendingRuntimeRunAction =
    promptSubmissionPending
      ? renderableRuntimeRun && !renderableRuntimeRun.isTerminal
        ? 'update_controls'
        : 'start'
      : agent.pendingRuntimeRunAction ?? null
  const runtimeRunActionStatus = promptSubmissionPending ? 'running' : agent.runtimeRunActionStatus
  const isQueueingRuntimePrompt =
    runtimeRunActionStatus === 'running' &&
    (pendingRuntimeRunAction === 'start' || pendingRuntimeRunAction === 'update_controls')
  const hasLiveRuntimeStream =
    streamStatus === 'subscribing' ||
    streamStatus === 'replaying' ||
    streamStatus === 'live'
  const shouldDeferContextMeterForRuntimeActivity = Boolean(
    isQueueingRuntimePrompt ||
      agent.selectedPrompt.hasQueuedPrompt ||
      (renderableRuntimeRun?.isActive && hasLiveRuntimeStream),
  )
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
  const hasUserMessage = conversationProjection.hasUserMessage || Boolean(pendingPromptTurn)
  const selectedAgentSession = (agent.project.selectedAgentSession ?? null) as AgentSessionView | null
  const selectedAgentSessionId =
    selectedAgentSession?.agentSessionId ?? agent.project.selectedAgentSessionId ?? null
  const hasSelectedAgentSession = Boolean(selectedAgentSessionId?.trim())
  const [codeUndoStates, setCodeUndoStates] = useState<Record<string, CodeUndoUiState>>({})
  const [returnSessionToHereStates, setReturnSessionToHereStates] = useState<Record<string, CodeUndoUiState>>({})

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
    dictationEnabled: foregroundWorkReady && isFocusedPane,
    dictationScopeKey: `${agent.project.id}:${agent.project.selectedAgentSessionId ?? 'none'}`,
    reportComposerControls: foregroundWorkReady && isFocusedPane,
    pendingInitialRuntimeAgentId,
    onPendingInitialRuntimeAgentIdConsumed,
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

  useEffect(() => {
    if (!optimisticPromptTurn) {
      return
    }

    const selectedQueuedText = selectedQueuedPromptTurn?.text.trim() ?? null
    if (
      hasTranscriptForPendingPrompt(runtimeStreamItems, optimisticPromptTurn) ||
      selectedQueuedText === optimisticPromptTurn.text.trim()
    ) {
      setOptimisticPromptTurn(null)
    }
  }, [optimisticPromptTurn, runtimeStreamItems, selectedQueuedPromptTurn])

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
  const agentProjectOrigin = getAgentProjectOrigin(agent.project)
  const availableRuntimeAgentIds = useMemo(
    () =>
      getRuntimeAgentDescriptorsForProjectOrigin(agentProjectOrigin).map(
        (descriptor) => descriptor.id,
      ),
    [agentProjectOrigin],
  )
  useEffect(() => {
    if (
      !controller.isRuntimeAgentSwitchDisabled &&
      !availableRuntimeAgentIds.includes(controller.composerRuntimeAgentId)
    ) {
      controller.handleComposerRuntimeAgentChange('ask')
    }
  }, [
    availableRuntimeAgentIds,
    controller.composerRuntimeAgentId,
    controller.isRuntimeAgentSwitchDisabled,
    controller.handleComposerRuntimeAgentChange,
  ])
  const streamRunId = getStreamRunId(runtimeStream, renderableRuntimeRun)
  const shouldRefreshContextMeter = Boolean(
    foregroundWorkReady &&
      isFocusedPane &&
      runtimeRunActionStatus !== 'running' &&
      !shouldDeferContextMeterForRuntimeActivity,
  )
  const contextMeterState = useAgentContextMeterSnapshot({
    enabled: shouldRefreshContextMeter,
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
        error={contextMeterState.error}
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
        ? 'Describe the agent you want to build...'
      : controller.composerRuntimeAgentId === 'crawl' &&
          !agentRuntimeBlocked &&
          runtimeSession?.isAuthenticated &&
          !renderableRuntimeRun?.isTerminal
        ? 'Map this existing repository...'
      : baseComposerPlaceholder
  const showAgentSetupEmptyState = Boolean(
    agentRuntimeBlocked &&
      (!runtimeSession || runtimeSession.isSignedOut || runtimeSession.phase === 'idle'),
  )
  const hasSessionActivity = Boolean(
    hasIncompleteRuntimeRunPayload ||
      renderableRuntimeRun ||
      controller.recentRunReplacement ||
      pendingPromptTurn ||
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
  const actionPromptDispatchValue = useMemo<ActionPromptDispatchValue>(() => {
    const pendingOperatorIntent = controller.pendingOperatorIntent
    return {
      pendingActionId: pendingOperatorIntent?.actionId ?? null,
      pendingDecision: pendingOperatorIntent?.kind ?? null,
      isResolving: agent.operatorActionStatus === 'running',
      resolveActionPrompt: async (actionId, decision, options) => {
        if (decision === 'resume') {
          if (renderableRuntimeRun && !renderableRuntimeRun.isTerminal) {
            return controller.handleResumeLiveActionRequired(actionId, {
              userAnswer: options?.userAnswer ?? null,
            })
          }
          return controller.handleResumeOperatorRun(actionId, {
            userAnswer: options?.userAnswer ?? null,
          })
        }
        return controller.handleResolveOperatorAction(actionId, decision, {
          userAnswer: options?.userAnswer ?? null,
        })
      },
    }
  }, [
    controller.pendingOperatorIntent,
    controller.handleResolveOperatorAction,
    controller.handleResumeOperatorRun,
    controller.handleResumeLiveActionRequired,
    agent.operatorActionStatus,
    renderableRuntimeRun,
  ])
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
  const scrollToLatestFrameRef = useRef<number | null>(null)
  const shouldAutoFollowRef = useRef(true)
  const [showJumpToLatest, setShowJumpToLatest] = useState(false)
  const latestVisibleTurn = visibleTurnsWithPendingPrompt.at(-1)
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
  const scrollToLatest = useCallback((behavior: ScrollBehavior = 'auto', options: { defer?: boolean } = {}) => {
    const run = () => {
      bottomSentinelRef.current?.scrollIntoView({
        block: 'end',
        inline: 'nearest',
        behavior,
      })
    }

    if (!options.defer || typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
      run()
      return
    }

    if (scrollToLatestFrameRef.current !== null && typeof window.cancelAnimationFrame === 'function') {
      window.cancelAnimationFrame(scrollToLatestFrameRef.current)
    }
    scrollToLatestFrameRef.current = window.requestAnimationFrame(() => {
      scrollToLatestFrameRef.current = null
      run()
    })
  }, [])
  useEffect(() => {
    return () => {
      if (
        scrollToLatestFrameRef.current !== null &&
        typeof window !== 'undefined' &&
        typeof window.cancelAnimationFrame === 'function'
      ) {
        window.cancelAnimationFrame(scrollToLatestFrameRef.current)
        scrollToLatestFrameRef.current = null
      }
    }
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
  const preserveConversationScrollPosition = useCallback(() => {
    const viewport = scrollViewportRef.current
    if (!viewport) {
      return () => {}
    }

    const scrollTop = viewport.scrollTop
    const wasAutoFollowing = shouldAutoFollowRef.current
    shouldAutoFollowRef.current = false

    return () => {
      const restore = () => {
        const nextViewport = scrollViewportRef.current
        if (!nextViewport) {
          return
        }

        const maxScrollTop = Math.max(0, nextViewport.scrollHeight - nextViewport.clientHeight)
        nextViewport.scrollTop = Math.min(scrollTop, maxScrollTop)
        const isNearBottom = isRuntimeConversationNearBottom(nextViewport)
        shouldAutoFollowRef.current = wasAutoFollowing && isNearBottom
        setShowJumpToLatest(hasConversationViewportContent && !isNearBottom)
      }

      if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
        window.requestAnimationFrame(restore)
        return
      }

      restore()
    }
  }, [hasConversationViewportContent])
  const handleUndoCodeChange = useCallback(async ({
    targetKind,
    changeGroupId,
    path,
    filePath,
    hunkIds = [],
    expectedWorkspaceEpoch,
  }: CodeUndoRequest) => {
    const applySelectiveUndo = desktopAdapter?.applySelectiveUndo
    const undoStateKey = getCodeUndoStateKey({ targetKind, changeGroupId, filePath })
    if (!applySelectiveUndo) {
      setCodeUndoStates((current) => ({
        ...current,
        [undoStateKey]: {
          status: 'failed',
          message: 'Undo is unavailable in this runtime.',
        },
      }))
      return
    }

    const restoreScrollPosition = preserveConversationScrollPosition()
    setCodeUndoStates((current) => ({
      ...current,
      [undoStateKey]: {
        status: 'pending',
        message:
          targetKind === 'hunks'
            ? 'Undoing selected hunks...'
            : targetKind === 'file_change'
              ? 'Undoing file change...'
              : 'Undoing change group...',
      },
    }))

    try {
      const undoFilePath = filePath ?? path
      const selectedHunkIds = Array.from(new Set(hunkIds))
      const response = await applySelectiveUndo({
        projectId: agent.project.id,
        operationId: createCodeHistoryOperationId('code-undo'),
        target:
          targetKind === 'hunks'
            ? {
                targetKind: 'hunks',
                targetId: `${changeGroupId}:${undoFilePath}:hunks`,
                changeGroupId,
                filePath: undoFilePath,
                hunkIds: selectedHunkIds,
              }
            : targetKind === 'file_change'
              ? {
                  targetKind: 'file_change',
                  targetId: undoFilePath,
                  changeGroupId,
                  filePath: undoFilePath,
                  hunkIds: [],
                }
              : {
                  targetKind: 'change_group',
                  targetId: changeGroupId,
                  changeGroupId,
                  hunkIds: [],
                },
        expectedWorkspaceEpoch: expectedWorkspaceEpoch ?? undefined,
      })
      const result = formatCodeUndoResult(response)
      setCodeUndoStates((current) => ({
        ...current,
        [undoStateKey]: result,
      }))
      if (result.status === 'succeeded') {
        await onCodeUndoApplied?.()
      }
    } catch (error) {
      setCodeUndoStates((current) => ({
        ...current,
        [undoStateKey]: {
          status: 'failed',
          message: getCodeUndoErrorMessage(error),
        },
      }))
    } finally {
      restoreScrollPosition()
    }
  }, [
    agent.project.id,
    desktopAdapter,
    onCodeUndoApplied,
    preserveConversationScrollPosition,
  ])
  const handleReturnSessionToHere = useCallback(async ({
    targetKind,
    targetId,
    boundaryId,
    runId,
    changeGroupId,
    expectedWorkspaceEpoch,
  }: ReturnSessionToHereUiRequest) => {
    const returnSessionToHere = desktopAdapter?.returnSessionToHere
    const agentSessionId = selectedAgentSessionId?.trim() ?? ''
    const stateKey = getReturnSessionToHereStateKey({ targetKind, boundaryId, runId, changeGroupId })

    if (!returnSessionToHere) {
      setReturnSessionToHereStates((current) => ({
        ...current,
        [stateKey]: {
          status: 'failed',
          message: 'Return session is unavailable in this runtime.',
        },
      }))
      return
    }

    if (!agentSessionId) {
      setReturnSessionToHereStates((current) => ({
        ...current,
        [stateKey]: {
          status: 'failed',
          message: 'Select an agent session before returning it to a code boundary.',
        },
      }))
      return
    }

    const restoreScrollPosition = preserveConversationScrollPosition()
    setReturnSessionToHereStates((current) => ({
      ...current,
      [stateKey]: {
        status: 'pending',
        message: 'Returning this session to here...',
      },
    }))

    try {
      const response = await returnSessionToHere({
        projectId: agent.project.id,
        operationId: createCodeHistoryOperationId('code-return-session'),
        target: {
          targetKind,
          targetId,
          agentSessionId,
          boundaryId,
          runId: runId ?? undefined,
          changeGroupId: changeGroupId ?? undefined,
        },
        expectedWorkspaceEpoch: expectedWorkspaceEpoch ?? undefined,
      })
      const result = formatReturnSessionToHereResult(response)
      setReturnSessionToHereStates((current) => ({
        ...current,
        [stateKey]: result,
      }))
      if (result.status === 'succeeded') {
        await onCodeUndoApplied?.()
      }
    } catch (error) {
      setReturnSessionToHereStates((current) => ({
        ...current,
        [stateKey]: {
          status: 'failed',
          message: getCodeUndoErrorMessage(error),
        },
      }))
    } finally {
      restoreScrollPosition()
    }
  }, [
    agent.project.id,
    desktopAdapter,
    onCodeUndoApplied,
    preserveConversationScrollPosition,
    selectedAgentSessionId,
  ])
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
    if (promptSubmissionPending) {
      return
    }

    const submittedText = controller.draftPrompt.trim()
    const optimisticPrompt = submittedText.length > 0
      ? {
          id: `${Date.now()}:${submittedText}`,
          text: submittedText,
          queuedAt: new Date().toISOString(),
        }
      : null

    if (optimisticPrompt) {
      setOptimisticPromptTurn(optimisticPrompt)
    }

    shouldAutoFollowRef.current = true
    setShowJumpToLatest(false)
    scrollToLatest('auto', { defer: true })
    setPromptSubmissionPending(true)
    promptSubmissionCancelRef.current?.()
    promptSubmissionCancelRef.current = scheduleAfterNextPaint(() => {
      promptSubmissionCancelRef.current = null
      void controller.handleSubmitDraftPrompt().then((submitted) => {
        if (!submitted && optimisticPrompt) {
          setOptimisticPromptTurn((current) =>
            current?.id === optimisticPrompt.id ? null : current,
          )
        }
      }).finally(() => {
        setPromptSubmissionPending(false)
        scrollToLatest('auto', { defer: true })
      })
    })
  }, [controller, promptSubmissionPending, scrollToLatest])

  useEffect(() => {
    if (!foregroundWorkReady) {
      return
    }

    if (!hasConversationViewportContent) {
      shouldAutoFollowRef.current = true
      setShowJumpToLatest(false)
      return
    }

    if (shouldAutoFollowRef.current) {
      scrollToLatest('auto', { defer: true })
      setShowJumpToLatest(false)
      return
    }

    setShowJumpToLatest(true)
  }, [conversationScrollKey, foregroundWorkReady, hasConversationViewportContent, scrollToLatest])

  const isCompact = density === 'compact'
  const isDense = isCompact || paneCount >= 4 || useBackgroundPaneFastPath
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

  const dragHandleAttributes = useMemo(() => {
    if (!dragHandle?.attributes) return null
    const { role: _role, ...rest } = dragHandle.attributes as Record<string, unknown>
    void _role
    return rest
  }, [dragHandle?.attributes])

  return (
    <AgentPaneDropOverlay
      enabled={Boolean(stageAgentAttachment)}
      onFilesDropped={handleAddFiles}
    >
      <div className="flex min-h-0 min-w-0 flex-1">
        <div className="relative flex min-w-0 flex-1 flex-col">
        <div className="pointer-events-none absolute inset-x-0 top-0 z-20">
          <div
            ref={dragHandle?.setActivatorNodeRef}
            {...(dragHandleAttributes ?? {})}
            {...(dragHandle?.listeners ?? {})}
            className={cn(
              'flex items-center justify-between gap-1.5 px-3.5',
              isDense ? 'py-1.5' : 'py-2',
              inSidebar ? 'bg-sidebar' : 'bg-background',
              dragHandle ? 'pointer-events-auto cursor-grab active:cursor-grabbing select-none' : null,
            )}
          >
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
              {inSidebar && sidebarSessions && sidebarSessions.length > 0 && onSelectSidebarSession ? (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      type="button"
                      aria-label="Switch agent session"
                      title={sessionLabel}
                      className={cn(
                        'inline-flex min-w-0 max-w-full items-center gap-1 rounded-md px-1 py-0.5 text-left font-medium text-muted-foreground transition-colors',
                        'hover:bg-secondary/50 hover:text-foreground data-[state=open]:bg-secondary/70 data-[state=open]:text-foreground',
                      )}
                    >
                      <span className="truncate">{sessionLabel}</span>
                      <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="start" className="w-64" sideOffset={6}>
                    {onCreateSession ? (
                      <>
                        <DropdownMenuItem
                          disabled={isCreatingSession}
                          onSelect={(event) => {
                            event.preventDefault()
                            onCreateSession()
                          }}
                        >
                          <Plus className="h-3.5 w-3.5" />
                          <span>New session</span>
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                      </>
                    ) : null}
                    {sidebarSessions.map((session) => {
                      const isSelected = session.agentSessionId === selectedAgentSessionId
                      const label = session.title?.trim() || 'Untitled'
                      return (
                        <DropdownMenuItem
                          key={session.agentSessionId}
                          onSelect={() => onSelectSidebarSession(session.agentSessionId)}
                        >
                          <Check
                            aria-hidden="true"
                            className={cn(
                              'h-3.5 w-3.5',
                              isSelected ? 'text-primary' : 'opacity-0',
                            )}
                          />
                          <span className="min-w-0 flex-1 truncate">{label}</span>
                        </DropdownMenuItem>
                      )
                    })}
                  </DropdownMenuContent>
                </DropdownMenu>
              ) : (
                <span className="truncate font-medium">{sessionLabel}</span>
              )}
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
                  <span>{inSidebar ? 'New' : 'New Session'}</span>
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
              {inSidebar && onCloseSidebar ? (
                <button
                  type="button"
                  aria-label="Close agent dock"
                  onClick={onCloseSidebar}
                  className={cn(
                    'inline-flex h-[30px] w-[30px] items-center justify-center rounded-md text-muted-foreground transition-colors',
                    'hover:bg-secondary/50 hover:text-foreground',
                  )}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              ) : null}
            </div>
          </div>
          <div
            aria-hidden="true"
            className={cn(
              'bg-gradient-to-b',
              isDense ? 'h-2' : 'h-7',
              inSidebar ? 'from-sidebar to-sidebar/0' : 'from-background to-background/0',
            )}
          />
        </div>
        <div className="relative min-h-0 flex-1">
          <div
            aria-label="Agent conversation viewport"
            ref={scrollViewportRef}
            onScroll={handleConversationScroll}
            onWheel={handleConversationWheel}
            className={cn(
              'select-text',
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
                context={
                  controller.composerRuntimeAgentId === 'agent_create' ? 'agent-create' : 'default'
                }
                agentCreateCanvasIncluded={agentCreateCanvasIncluded}
                onStartWorkflowAgentCreate={onStartWorkflowAgentCreate}
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
                <ActionPromptDispatchProvider value={actionPromptDispatchValue}>
                  <ConversationSection
                    runtimeRun={renderableRuntimeRun}
                    visibleTurns={visibleTurnsWithPendingPrompt}
                    streamIssue={streamIssue}
                    streamFailure={runtimeStream?.failure ?? null}
                    showActivityIndicator={showAgentActivityIndicator}
                    streamCompletion={runtimeStream?.completion ?? null}
                    accountAvatarUrl={accountAvatarUrl}
                    accountLogin={accountLogin}
                    variant={isDense ? 'dense' : 'default'}
                    codeUndoStates={codeUndoStates}
                    returnSessionToHereStates={returnSessionToHereStates}
                    onUndoChangeGroup={
                      desktopAdapter?.applySelectiveUndo ? handleUndoCodeChange : undefined
                    }
                    onReturnSessionToHere={
                      desktopAdapter?.returnSessionToHere ? handleReturnSessionToHere : undefined
                    }
                  />
                </ActionPromptDispatchProvider>
                {controller.composerRuntimeAgentId === 'agent_create' ? (
                  <AgentCreateDraftSection
                    runtimeStreamItems={runtimeStreamItems}
                    pendingApprovalCount={agent.pendingApprovalCount ?? 0}
                    onOpenAgentManagement={onOpenAgentManagement}
                    onCreateAgentByHand={onCreateAgentByHand}
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

        <PlanTray plan={runtimeStream?.plan ?? null} density={density} />

        <ComposerDock
          density={density}
          inSidebar={inSidebar}
          composerRuntimeAgentId={controller.composerRuntimeAgentId}
          composerRuntimeAgentLabel={getRuntimeAgentLabel(controller.composerRuntimeAgentId)}
          availableRuntimeAgentIds={availableRuntimeAgentIds}
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
          controlsDisabled={controller.areControlsDisabled || promptSubmissionPending}
          runtimeAgentSwitchDisabled={controller.isRuntimeAgentSwitchDisabled}
          dictation={controller.dictation}
          contextMeter={contextMeter}
          draftPrompt={controller.draftPrompt}
          isPromptDisabled={controller.isPromptDisabled || promptSubmissionPending}
          isSendDisabled={!controller.canSubmitPrompt || promptSubmissionPending}
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
