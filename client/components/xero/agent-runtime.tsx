"use client"

import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
  type WheelEvent,
} from 'react'
import {
  ArrowDown,
  Check,
  ChevronDown,
  ChevronRight,
  Loader2,
  Monitor,
  Plus,
  SplitSquareHorizontal,
  X,
} from 'lucide-react'
import { convertFileSrc } from '@tauri-apps/api/core'

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
import type { ToolCallGroupingPreference } from '@/src/features/xero/tool-call-grouping-preference'
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
  ProjectFileIndexEntryDto,
  RuntimeSessionView,
  RuntimeStreamActionRequiredItemView,
  RuntimeStreamActivityItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
  ReturnSessionToHereResponseDto,
  SelectiveUndoResponseDto,
} from '@/src/lib/xero-model'
import type { SessionContextSnapshotDto } from '@/src/lib/xero-model/session-context'
import type { AgentHandoffContextSummaryDto } from '@/src/lib/xero-model/agent-reports'
import {
  getRuntimeAgentDescriptorsForProjectOrigin,
  getRuntimeAgentLabel,
  getRuntimeRunThinkingEffortLabel,
  type AgentDefaultModelDto,
  type RuntimeLinkedPathDto,
  type RuntimeRunControlInputDto,
  type StagedAgentAttachmentDto,
} from '@/src/lib/xero-model'
import {
  classifyAttachment,
  classificationRejectionMessage,
} from '@/lib/agent-attachments'
import { ComputerUseSidebarHeader } from '@xero/ui/components/computer-use-sidebar'

import {
  AgentContextMeter,
  type AgentContextMeterStatus,
} from './agent-runtime/agent-context-meter'
import {
  buildComposerAgentSelectionKey,
  getComposerControlInput,
  getComposerApprovalOptions,
  getComposerModelGroups,
  getComposerModelOption,
  getComposerPlaceholder,
  getComposerThinkingOptions,
} from './agent-runtime/composer-helpers'
import { classifyCreditLimitFailure } from '@xero/ui/model/credit-limit'
import { getCloudProviderLabel } from '@/src/lib/xero-model/provider-presets'
import {
  ActionPromptDispatchProvider,
  type ActionPromptDecision,
  type ActionPromptDispatchValue,
} from '@xero/ui/components/transcript/action-prompt-card'
import {
  RoutingSuggestionDispatchProvider,
  type RoutingSuggestionDecision,
  type RoutingSuggestionDispatchValue,
} from '@xero/ui/components/transcript/routing-suggestion-card'
import {
  ComposerDock,
  type ComposerContextMentionOption,
  type ComposerPendingAttachment,
  type ComposerPendingContext,
} from './agent-runtime/composer-dock'
import { PlanTray } from './agent-runtime/plan-tray'
import {
  AGENT_PANE_COMPACT_WIDTH_PX,
  SIDEBAR_AGENT_PANE_COMPACT_WIDTH_PX,
} from './agent-runtime/density'
import { AgentPaneDropOverlay } from './agent-runtime/agent-pane-drop-overlay'
import { AgentCreateDraftSection } from './agent-runtime/agent-create-draft-section'
import {
  ConversationSection,
  getCodeUndoStateKey,
  getReturnSessionToHereStateKey,
  userVisiblePromptText,
  type CodeUndoRequest,
  type CodeUndoConflictSummary,
  type CodeUndoUiState,
  type ConversationMessageAttachment,
  type ConversationTurn,
  type ReturnSessionToHereUiRequest,
} from '@xero/ui/components/transcript/conversation-section'
import {
  mergeConversationAttachments,
  promoteActionMediaIntoFollowingAssistantMessages,
  runtimeMediaAttachmentsToConversation,
} from '@xero/ui/components/transcript/runtime-media'
import { EmptySessionState } from '@xero/ui/components/empty-session-state'
import {
  getToolCardTitle,
  getStreamRunId,
  getToolStateLabel,
  getToolSummaryContext,
  hasUsableRuntimeRunId,
} from './agent-runtime/runtime-stream-helpers'
import {
  HandoffContextDialog,
  type HandoffContextDialogStatus,
} from './agent-runtime/handoff-context-dialog'
import { SetupEmptyState } from './agent-runtime/setup-empty-state'
import {
  useAgentRuntimeController,
  type ActionPromptError,
} from './agent-runtime/use-agent-runtime-controller'
import type { SpeechDictationAdapter } from './agent-runtime/use-speech-dictation'
import {
  parseRoutingMarker,
  stripRoutingMarkers,
} from './agent-runtime/routing-suggestion-marker'
import {
  applyPersistedRoutingContinuationResolutions,
  applyRoutingContinuationDecision,
  filterInternalRoutingContinuationTurns,
  getRoutingDecisionTargetLabel,
  getRoutingResolutionForDecision,
  isInternalRoutingContinuationPromptText,
  parseInternalRoutingContinuationPromptText,
  type RoutingResolutionRecord,
} from './agent-runtime/routing-continuation'

export type AgentRuntimeDesktopAdapter = SpeechDictationAdapter &
  Partial<
    Pick<
      XeroDesktopAdapter,
      | 'applySelectiveUndo'
      | 'returnSessionToHere'
      | 'getSessionContextSnapshot'
      | 'getSessionTranscript'
      | 'stageAgentAttachment'
      | 'stageAgentAttachmentPath'
      | 'discardAgentAttachment'
      | 'pickComposerFolders'
      | 'listProjectFileIndex'
      | 'resumeAgentRun'
      | 'rejectAgentAction'
      | 'getAgentHandoffContextSummary'
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
    attachments?: StagedAgentAttachmentDto[]
    linkedPaths?: RuntimeLinkedPathDto[]
  }) => Promise<RuntimeRunView | null>
  onUpdateRuntimeRunControls?: (request?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
    attachments?: StagedAgentAttachmentDto[]
    linkedPaths?: RuntimeLinkedPathDto[]
    actionId?: string | null
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
  desktopAdapter?: AgentRuntimeDesktopAdapter
  /** GitHub avatar URL for the signed-in account, when available. */
  accountAvatarUrl?: string | null
  /** GitHub login for the signed-in account. */
  accountLogin?: string | null
  onCreateSession?: () => void
  isCreatingSession?: boolean
  /** Active and known custom agent definitions visible to the composer selector. */
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  /** Per-agent default models keyed by composer agent selection key. */
  agentDefaultModels?: Readonly<Record<string, AgentDefaultModelDto | null | undefined>>
  /** Open the Settings → Agents tab so the user can manage custom agents. */
  onOpenAgentManagement?: () => void
  /** Open the by-hand agent builder form. */
  onCreateAgentByHand?: () => void
  /** Move to Workflow and begin a new canvas-backed Agent Create session. */
  onStartWorkflowAgentCreate?: () => void
  /** True when Agent Create is paired with the visible definition-authoring canvas. */
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
  /** Clear the sidebar chat transcript from the Computer Use header. */
  onClearSidebarChat?: () => void | Promise<unknown>
  /** Disable the Computer Use sidebar clear-chat action. */
  sidebarChatClearDisabled?: boolean
  /** Accessible label for the Computer Use sidebar clear-chat action. */
  sidebarChatClearLabel?: string
  /** Tooltip for the Computer Use sidebar clear-chat action. */
  sidebarChatClearTitle?: string
  /** True while the Computer Use sidebar clear-chat action is pending. */
  sidebarChatClearPending?: boolean
  /** Close the sidebar from the agent header (X button). */
  onCloseSidebar?: () => void
  /**
   * Historical conversation turns from prior runs in this agent session
   * (loaded from the persisted session transcript). When provided, they are
   * prepended ahead of the live stream so a same-session handoff reads as a
   * continuous conversation. Items belonging to the active run must already
   * be excluded by the caller when possible; overlap is tolerated during
   * transcript / stream replay races.
   */
  historicalConversationTurns?: readonly ConversationTurn[]
  /** True while the persisted transcript for this session is loading. */
  historicalConversationTurnsLoading?: boolean
  /**
   * One-shot runtime agent to apply to the composer when this pane mounts or
   * when the value changes to a new non-null id. Used by "Create agent" entry
   * points that open a fresh session pre-selected on a specific agent.
   */
  pendingInitialRuntimeAgentId?: RuntimeAgentIdDto | null
  pendingInitialAgentDefinitionId?: string | null
  /** Called once the pending initial runtime agent has been applied. */
  onPendingInitialRuntimeAgentIdConsumed?: () => void
  /** One-shot text/image context to append to the visible composer without submitting it. */
  pendingComposerInsert?: AgentComposerInsert | null
  /** Called once the pending composer insert has been applied locally. */
  onPendingComposerInsertConsumed?: (id: string) => void
  /** Display preference for compacting completed tool calls. */
  toolCallGroupingPreference?: ToolCallGroupingPreference
  /** Automatically accept agent routing suggestions and continue in the suggested agent. */
  agentRoutingAutoSwitchEnabled?: boolean
}

const EMPTY_RUNTIME_STREAM_ITEMS: RuntimeStreamViewItem[] = []
const EMPTY_ACTION_REQUIRED_ITEMS: NonNullable<AgentPaneView['actionRequiredItems']> = []
const MAX_VISIBLE_RUNTIME_ACTION_TURNS = Number.POSITIVE_INFINITY
const CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX = 96
const CONVERSATION_FOLLOW_UP_ANCHOR_TOP_OFFSET_PX = 28
const CONVERSATION_LAYOUT_SETTLE_SYNC_DELAYS_MS = [80, 180, 320]
const BACKGROUND_PANE_STREAM_ITEM_LIMIT = 160
const BACKGROUND_PANE_VISIBLE_TURN_LIMIT = 48
const FOREGROUND_WORK_DEFER_MS = 32
const STREAMING_TOOL_OUTPUT_MAX_CHARS = 24_000
const CONTEXT_METER_REFRESH_IDLE_TIMEOUT_MS = 1200
const CONTEXT_METER_REFRESH_FALLBACK_DELAY_MS = 220
const PENDING_PROMPT_TRANSCRIPT_CLOCK_SKEW_MS = 120_000
const CODE_EDIT_TOOL_NAMES = new Set(['edit', 'patch', 'write', 'apply_patch', 'notebook_edit'])
const OWNED_AGENT_ACTION_PROMPT_TYPES = new Set([
  'command_approval',
  'review_plan',
  'safety_boundary',
  'subagent_resolution_required',
  'user_input_required',
  'verification_required',
])

export interface AgentPaneCloseState {
  hasRunningRun: boolean
  hasUnsavedComposerText: boolean
  sessionTitle: string
}

function ConversationLoadingState({ context }: { context: 'computer-use' | 'default' }) {
  return (
    <div
      aria-label={context === 'computer-use' ? 'Loading Computer Use chat' : 'Loading chat'}
      className="flex flex-col items-center gap-3 text-center"
      role="status"
    >
      <span className="inline-flex h-10 w-10 items-center justify-center rounded-lg border border-border/70 bg-secondary/30 text-primary">
        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
      </span>
      <p className="text-[13px] font-medium text-muted-foreground">
        {context === 'computer-use' ? 'Loading Computer Use chat...' : 'Loading chat...'}
      </p>
    </div>
  )
}

export interface AgentComposerInsert {
  id: string
  /** Text the user should see and edit in the composer. */
  prompt: string
  /** Extra context submitted with the next prompt, hidden from the composer draft. */
  hiddenPrompt?: string | null
  /** Visible indicator for hidden composer context, such as selected element metadata. */
  contextCard?: Omit<ComposerPendingContext, 'id'> | null
  image?: {
    bytes: Uint8Array
    mediaType: 'image/png'
    originalName: string
  } | null
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

export function getFollowUpAnchorScrollPlan({
  anchorTop,
  viewportHeight,
  scrollHeight,
  currentSpacerHeight,
  topOffset = CONVERSATION_FOLLOW_UP_ANCHOR_TOP_OFFSET_PX,
}: {
  anchorTop: number
  viewportHeight: number
  scrollHeight: number
  currentSpacerHeight: number
  topOffset?: number
}): { scrollTop: number; spacerHeight: number } {
  const safeAnchorTop = Number.isFinite(anchorTop) ? Math.max(0, anchorTop) : 0
  const safeViewportHeight = Number.isFinite(viewportHeight) ? Math.max(0, viewportHeight) : 0
  const safeScrollHeight = Number.isFinite(scrollHeight) ? Math.max(0, scrollHeight) : 0
  const safeCurrentSpacerHeight = Number.isFinite(currentSpacerHeight)
    ? Math.max(0, currentSpacerHeight)
    : 0
  const safeTopOffset = Number.isFinite(topOffset) ? Math.max(0, topOffset) : 0
  const scrollTop = Math.max(0, safeAnchorTop - safeTopOffset)
  const naturalScrollHeight = Math.max(0, safeScrollHeight - safeCurrentSpacerHeight)
  const naturalMaxScrollTop = Math.max(0, naturalScrollHeight - safeViewportHeight)
  const spacerHeight = Math.max(0, Math.ceil(scrollTop - naturalMaxScrollTop))

  return {
    scrollTop,
    spacerHeight,
  }
}

function findConversationTurnElement(viewport: HTMLElement, turnId: string): HTMLElement | null {
  const turns = viewport.querySelectorAll<HTMLElement>('[data-conversation-turn-id]')
  for (const turn of turns) {
    if (turn.getAttribute('data-conversation-turn-id') === turnId) {
      return turn
    }
  }

  return null
}

function getElementTopInScrollViewport(viewport: HTMLElement, element: HTMLElement): number {
  const viewportRect = viewport.getBoundingClientRect()
  const elementRect = element.getBoundingClientRect()
  const rectTop = elementRect.top - viewportRect.top + viewport.scrollTop
  if (
    Number.isFinite(rectTop) &&
    (elementRect.top !== 0 || viewportRect.top !== 0 || viewport.scrollTop !== 0)
  ) {
    return Math.max(0, rectTop)
  }

  let offsetTop = 0
  let current: HTMLElement | null = element
  while (current && current !== viewport) {
    offsetTop += current.offsetTop
    current = current.offsetParent as HTMLElement | null
  }

  return Math.max(0, offsetTop || element.offsetTop)
}

function scrollViewportTo(viewport: HTMLElement, top: number, behavior: ScrollBehavior): void {
  const nextTop = Math.max(0, top)
  if (typeof viewport.scrollTo === 'function') {
    try {
      viewport.scrollTo({ top: nextTop, behavior })
      return
    } catch {
      viewport.scrollTop = nextTop
      return
    }
  }

  viewport.scrollTop = nextTop
}

function findFollowUpAnchorTurnIndex(
  turns: readonly ConversationTurn[],
  anchorTurnId: string,
  queuedAnchorText: string | null | undefined,
): number {
  const directIndex = turns.findIndex((turn) => turn.id === anchorTurnId)
  if (directIndex >= 0) {
    return directIndex
  }

  const fallbackText = queuedAnchorText ? userVisiblePromptText(queuedAnchorText).trim() : ''
  if (!fallbackText) {
    return -1
  }

  for (let index = turns.length - 1; index >= 0; index -= 1) {
    const turn = turns[index]
    if (
      turn.kind === 'message' &&
      turn.role === 'user' &&
      userVisiblePromptText(turn.text).trim() === fallbackText
    ) {
      return index
    }
  }

  return -1
}

export function shouldReleaseFollowUpAnchorForTurns({
  turns,
  anchorTurnId,
  queuedAnchorText,
}: {
  turns: readonly ConversationTurn[]
  anchorTurnId: string
  queuedAnchorText?: string | null
}): boolean {
  const anchorTurnIndex = findFollowUpAnchorTurnIndex(turns, anchorTurnId, queuedAnchorText)
  if (anchorTurnIndex < 0) {
    return false
  }

  return turns
    .slice(anchorTurnIndex + 1)
    .some((turn) => turn.kind !== 'message' || turn.role !== 'user')
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

function isOwnedAgentActionPrompt(
  runId: string | null | undefined,
  actionType: string | null | undefined,
): boolean {
  const normalizedRunId = runId?.trim()
  const normalizedActionType = actionType?.trim()
  return Boolean(
    normalizedRunId &&
      normalizedActionType &&
      OWNED_AGENT_ACTION_PROMPT_TYPES.has(normalizedActionType),
  )
}

function ownedAgentActionResponse(
  decision: ActionPromptDecision,
  userAnswer: string | null | undefined,
): string | null {
  const trimmed = userAnswer?.trim() ?? ''
  if (trimmed.length > 0) {
    return trimmed
  }
  return decision === 'approve' || decision === 'resume' ? 'Approved.' : null
}

function ownedAgentActionPromptKey(runId: string, actionId: string): string {
  return `${runId}:${actionId}`
}

function getRuntimeActionErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }
  if (typeof error === 'string' && error.trim().length > 0) {
    return error
  }
  return fallback
}

function actionPromptTurnFromItem(item: RuntimeStreamActionRequiredItemView): ConversationTurn {
  const shape = item.answerShape ?? 'plain_text'
  return {
    id: item.id,
    kind: 'action_prompt',
    sequence: item.sequence,
    actionId: item.actionId,
    runId: item.runId,
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
    mediaAttachments: runtimeMediaAttachmentsToConversation(item.mediaAttachments),
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
  existing.mediaAttachments = mergeConversationAttachments(
    existing.mediaAttachments,
    incoming.mediaAttachments,
  )
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

function shouldUseCompactPaneWidth(width: number, compactWidthPx: number): boolean {
  return width > 0 && width < compactWidthPx
}

function useCompactPaneWidth(
  elementRef: RefObject<HTMLElement | null>,
  compactWidthPx: number,
): boolean {
  const [isCompactWidth, setIsCompactWidth] = useState(false)

  useLayoutEffect(() => {
    const element = elementRef.current
    if (!element || typeof ResizeObserver === 'undefined') {
      return
    }

    const updateWidth = (width: number) => {
      const nextCompactWidth = shouldUseCompactPaneWidth(width, compactWidthPx)
      setIsCompactWidth((current) =>
        current === nextCompactWidth ? current : nextCompactWidth,
      )
    }

    updateWidth(element.getBoundingClientRect().width)

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0]
      updateWidth(entry?.contentRect.width ?? element.getBoundingClientRect().width)
    })
    observer.observe(element)

    return () => observer.disconnect()
  }, [compactWidthPx, elementRef])

  return isCompactWidth
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
  const isSingleAction = actions.length === 1

  return {
    id: `tool-group:${firstAction.id}`,
    kind: 'action_group',
    sequence: lastAction.sequence,
    title: isSingleAction ? firstAction.title : `${actions.length} tool calls`,
    detail: isSingleAction ? firstAction.detail : summarizeActionGroup(actions),
    state: actionGroupState(actions),
    actions: actions.map((action) => ({
      id: action.id,
      sequence: action.sequence,
      toolCallId: action.toolCallId,
      toolName: action.toolName,
      title: action.title,
      detail: action.detail,
      detailRows: action.detailRows,
      mediaAttachments: action.mediaAttachments,
      state: action.state ?? null,
      ...(action.defaultOpen ? { defaultOpen: true } : {}),
    })),
  }
}

function compactActionBursts(turns: ConversationTurn[]): ConversationTurn[] {
  const compactedTurns: ConversationTurn[] = []
  let terminalActionBuffer: Extract<ConversationTurn, { kind: 'action' }>[] = []

  const flushTerminalActionBuffer = () => {
    if (terminalActionBuffer.length === 0) {
      return
    }
    compactedTurns.push(actionGroupTurnFromActions(terminalActionBuffer))
    terminalActionBuffer = []
  }

  for (const turn of turns) {
    if (turn.kind === 'action') {
      if (isCodeEditAction(turn) || !isTerminalActionState(turn.state)) {
        flushTerminalActionBuffer()
        compactedTurns.push(turn)
        continue
      }
      terminalActionBuffer.push(turn)
      continue
    }

    flushTerminalActionBuffer()
    compactedTurns.push(turn)
  }

  flushTerminalActionBuffer()
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
  attachments?: ConversationMessageAttachment[]
}

type PendingRoutingContinuation = {
  turnId: string
  decision: RoutingSuggestionDecision
  prompt: string
  controls: RuntimeRunControlInputDto | null
}

const conversationProjectionCache = new WeakMap<
  readonly RuntimeStreamViewItem[],
  Partial<Record<ToolCallGroupingPreference, ConversationProjection>>
>()

interface TurnRoutingContext {
  turns: ConversationTurn[]
  actionTurnIndexByToolCallId: Map<string, number>
}

type ComposerPendingLinkedPath = {
  id: string
  kind: RuntimeLinkedPathDto['kind']
  absolutePath: string
}

function createTurnRoutingContext(): TurnRoutingContext {
  return {
    turns: [],
    actionTurnIndexByToolCallId: new Map<string, number>(),
  }
}

function upsertRoutingSuggestionTurn(
  context: TurnRoutingContext,
  sourceTurnId: string,
  sourceSequence: number,
  parsed: NonNullable<ReturnType<typeof parseRoutingMarker>>,
): void {
  const routingTurnId = `routing_suggestion:${sourceTurnId}`
  const existingIndex = context.turns.findIndex(
    (turn) => turn.kind === 'routing_suggestion' && turn.id === routingTurnId,
  )

  const next: Extract<ConversationTurn, { kind: 'routing_suggestion' }> = {
    id: routingTurnId,
    kind: 'routing_suggestion',
    sequence: sourceSequence + 0.5,
    targetKind: parsed.targetKind,
    targetAgentId: parsed.targetAgentId,
    targetAgentDefinitionId: parsed.targetAgentDefinitionId,
    targetAgentDefinitionVersion: parsed.targetAgentDefinitionVersion,
    targetLabel: parsed.targetLabel,
    reason: parsed.reason,
    summary: parsed.summary,
    isResolved: false,
    acceptedTarget: null,
    acceptedTargetAgentDefinitionId: null,
    acceptedTargetLabel: null,
    routingResolutionMode: null,
  }

  const replaceIndex = existingIndex >= 0
    ? existingIndex
    : findEquivalentRoutingSuggestionTurnIndex(context, parsed)

  if (replaceIndex >= 0) {
    const existing = context.turns[replaceIndex]
    if (existing.kind === 'routing_suggestion') {
      next.id = existing.id
      next.isResolved = existing.isResolved
      next.acceptedTarget = existing.acceptedTarget
      next.acceptedTargetAgentDefinitionId = existing.acceptedTargetAgentDefinitionId
      next.acceptedTargetLabel = existing.acceptedTargetLabel
      next.routingResolutionMode = existing.routingResolutionMode
    }
    context.turns[replaceIndex] = next
    return
  }

  context.turns.push(next)
}

function findEquivalentRoutingSuggestionTurnIndex(
  context: TurnRoutingContext,
  parsed: NonNullable<ReturnType<typeof parseRoutingMarker>>,
): number {
  for (let index = context.turns.length - 1; index >= 0; index -= 1) {
    const turn = context.turns[index]
    if (turn.kind === 'message' && turn.role === 'user') {
      return -1
    }
    if (turn.kind === 'routing_suggestion' && routingSuggestionMatchesParsedMarker(turn, parsed)) {
      return index
    }
  }

  return -1
}

function routingSuggestionMatchesParsedMarker(
  turn: Extract<ConversationTurn, { kind: 'routing_suggestion' }>,
  parsed: NonNullable<ReturnType<typeof parseRoutingMarker>>,
): boolean {
  return (
    turn.targetKind === parsed.targetKind &&
    turn.targetAgentId === parsed.targetAgentId &&
    turn.targetAgentDefinitionId === parsed.targetAgentDefinitionId &&
    turn.targetAgentDefinitionVersion === parsed.targetAgentDefinitionVersion
  )
}

function maybeAttachRoutingSuggestion(
  context: TurnRoutingContext,
  messageTurn: Extract<ConversationTurn, { kind: 'message' }>,
): void {
  if (messageTurn.role !== 'assistant') return
  const parsed = parseRoutingMarker(messageTurn.text)
  const cleanText = stripRoutingMarkers(messageTurn.text)
  if (cleanText !== messageTurn.text) {
    messageTurn.text = cleanText
  }
  if (!parsed) return

  upsertRoutingSuggestionTurn(context, messageTurn.id, messageTurn.sequence, parsed)
}

function buildRoutingDeclineContinuationPrompt(
  decision: Extract<RoutingSuggestionDecision, { kind: 'decline' }>,
): string {
  const targetLabel = getRoutingDecisionTargetLabel(decision)
  const summary = decision.summary?.trim()
  const reason = decision.reason?.trim()
  const contextLines = [
    summary ? `Carry over: ${summary}` : null,
    reason ? `Routing reason: ${reason}` : null,
  ].filter((line): line is string => Boolean(line))

  return [
    `The user chose to stay with the current Agent instead of switching to ${targetLabel}.`,
    'Continue the original request now. Do not stop at another routing recommendation for this same request.',
    ...contextLines,
  ].join('\n\n')
}

function buildRoutingAcceptContinuationPrompt(
  decision: Extract<RoutingSuggestionDecision, { kind: 'accept' }>,
): string {
  const targetLabel = getRoutingDecisionTargetLabel(decision)
  const contextLines = [
    `Target agent: ${targetLabel}`,
    decision.summary?.trim() ? `Carry over: ${decision.summary.trim()}` : null,
    decision.reason?.trim() ? `Routing reason: ${decision.reason.trim()}` : null,
  ].filter((line): line is string => Boolean(line))

  return [
    `The user accepted the routing suggestion to switch to ${targetLabel}.`,
    'Continue the original request now in this same session.',
    ...contextLines,
  ].join('\n\n')
}

function buildRoutingAcceptDecisionFromTurn(
  turn: Extract<ConversationTurn, { kind: 'routing_suggestion' }>,
  resolutionMode: 'manual' | 'automatic' = 'manual',
): Extract<RoutingSuggestionDecision, { kind: 'accept' }> {
  return {
    kind: 'accept',
    targetAgentId: turn.targetAgentId,
    targetAgentDefinitionId: turn.targetAgentDefinitionId,
    targetAgentDefinitionVersion: turn.targetAgentDefinitionVersion,
    targetLabel: turn.targetLabel,
    reason: turn.reason,
    summary: turn.summary,
    resolutionMode,
  }
}

/**
 * Append the projected turn for a single runtime stream item into `context`,
 * preserving the assistant-transcript merge and tool-call dedupe behaviour.
 *
 * Returns true when the item is a user transcript (so the caller can update
 * top-level state like `hasUserMessage`).
 */
function routeItemIntoTurns(item: RuntimeStreamViewItem, context: TurnRoutingContext): boolean {
  if (item.kind === 'transcript') {
    if (item.role !== 'user' && item.role !== 'assistant') {
      return false
    }

    const previous = context.turns.at(-1)
    if (item.role === 'assistant' && previous?.kind === 'message' && previous.role === item.role) {
      previous.text = appendTranscriptDelta(previous.text, item.text)
      previous.sequence = item.sequence
      previous.attachments = mergeConversationAttachments(
        previous.attachments,
        runtimeMediaAttachmentsToConversation(item.mediaAttachments),
      )
      maybeAttachRoutingSuggestion(context, previous)
      return false
    }

    context.turns.push({
      id: item.id,
      kind: 'message',
      role: item.role,
      sequence: item.sequence,
      text: item.text,
      createdAt: item.createdAt,
      attachments: runtimeMediaAttachmentsToConversation(item.mediaAttachments),
    })
    if (item.role === 'assistant') {
      const justPushed = context.turns.at(-1)
      if (justPushed?.kind === 'message') {
        maybeAttachRoutingSuggestion(context, justPushed)
      }
    }
    return item.role === 'user'
  }

  if (isReasoningActivityItem(item)) {
    const text = getReasoningActivityText(item)
    const parsed = parseRoutingMarker(text)
    const cleanText = stripRoutingMarkers(text)
    if (cleanText.trim().length === 0) {
      if (parsed) {
        upsertRoutingSuggestionTurn(context, item.id, item.sequence, parsed)
      }
      return false
    }
    context.turns.push({
      id: item.id,
      kind: 'thinking',
      sequence: item.sequence,
      text: cleanText,
    })
    if (parsed) {
      upsertRoutingSuggestionTurn(context, item.id, item.sequence, parsed)
    }
    return false
  }

  if (isFileChangeActivityItem(item)) {
    context.turns.push(fileChangeTurnFromItem(item))
    return false
  }

  if (item.kind === 'action_required') {
    context.turns.push(actionPromptTurnFromItem(item))
    return false
  }

  if (!shouldShowActionItem(item)) {
    return false
  }

  if (item.kind === 'failure') {
    context.turns.push({
      id: item.id,
      kind: 'failure',
      sequence: item.sequence,
      code: item.code,
      message: item.message,
    })
    return false
  }

  if (item.kind !== 'tool') {
    return false
  }

  const incomingActionTurn = actionTurnFromItem(item)
  const existingActionTurnIndex = context.actionTurnIndexByToolCallId.get(item.toolCallId)
  const existingActionTurn =
    existingActionTurnIndex != null ? context.turns[existingActionTurnIndex] : null

  if (existingActionTurn?.kind === 'action') {
    mergeActionTurn(existingActionTurn, incomingActionTurn)
    return false
  }

  context.actionTurnIndexByToolCallId.set(item.toolCallId, context.turns.length)
  context.turns.push(incomingActionTurn)
  return false
}

interface SubagentGroupState {
  index: number
  context: TurnRoutingContext
}

function subagentLifecycleStatusOrFallback(item: RuntimeStreamViewItem): string {
  if (item.kind === 'subagent_lifecycle') {
    return item.subagentStatus
  }
  return 'running'
}

function subagentRoleLabelFor(item: RuntimeStreamViewItem): string {
  if (item.kind === 'subagent_lifecycle') {
    return (
      item.subagentRoleLabel ??
      item.subagentRole ??
      item.subagentId ??
      'Subagent'
    )
  }
  return item.subagentRoleLabel ?? item.subagentRole ?? item.subagentId ?? 'Subagent'
}

function emptySubagentGroupTurn(
  item: RuntimeStreamViewItem,
  subagentId: string,
): Extract<ConversationTurn, { kind: 'subagent_group' }> {
  return {
    id: `subagent_group:${subagentId}`,
    kind: 'subagent_group',
    sequence: item.sequence,
    subagentId,
    role: item.kind === 'subagent_lifecycle' ? item.subagentRole : item.subagentRole ?? null,
    roleLabel: subagentRoleLabelFor(item),
    status: subagentLifecycleStatusOrFallback(item),
    runId: item.kind === 'subagent_lifecycle' ? item.subagentRunId : null,
    prompt: item.kind === 'subagent_lifecycle' ? item.prompt : null,
    usedToolCalls: item.kind === 'subagent_lifecycle' ? item.usedToolCalls : null,
    maxToolCalls: item.kind === 'subagent_lifecycle' ? item.maxToolCalls : null,
    usedTokens: item.kind === 'subagent_lifecycle' ? item.usedTokens : null,
    maxTokens: item.kind === 'subagent_lifecycle' ? item.maxTokens : null,
    resultSummary: item.kind === 'subagent_lifecycle' ? item.resultSummary : null,
    startedAt: item.kind === 'subagent_lifecycle' ? item.createdAt : null,
    completedAt: null,
    children: [],
  }
}

function finalizeActionTurns(
  turns: ConversationTurn[],
  toolCallGroupingPreference: ToolCallGroupingPreference,
): ConversationTurn[] {
  const projectedTurns =
    toolCallGroupingPreference === 'grouped' ? compactActionBursts(turns) : turns
  return limitActionTurns(promoteActionMediaIntoFollowingAssistantMessages(projectedTurns))
}

function buildConversationProjection(
  runtimeStreamItems: readonly RuntimeStreamViewItem[],
  toolCallGroupingPreference: ToolCallGroupingPreference,
): ConversationProjection {
  const topContext = createTurnRoutingContext()
  const subagentGroups = new Map<string, SubagentGroupState>()
  let hasUserMessage = false

  const ensureSubagentGroup = (
    item: RuntimeStreamViewItem,
    subagentId: string,
  ): SubagentGroupState => {
    const existing = subagentGroups.get(subagentId)
    if (existing) {
      return existing
    }
    const turn = emptySubagentGroupTurn(item, subagentId)
    const index = topContext.turns.length
    topContext.turns.push(turn)
    const state: SubagentGroupState = {
      index,
      context: createTurnRoutingContext(),
    }
    subagentGroups.set(subagentId, state)
    return state
  }

  for (const item of runtimeStreamItems) {
    if (item.kind === 'subagent_lifecycle') {
      const state = ensureSubagentGroup(item, item.subagentId)
      const groupTurn = topContext.turns[state.index]
      if (groupTurn?.kind === 'subagent_group') {
        groupTurn.status = item.subagentStatus
        if (item.subagentRole) groupTurn.role = item.subagentRole
        const roleLabel = subagentRoleLabelFor(item)
        if (roleLabel) groupTurn.roleLabel = roleLabel
        if (item.subagentRunId) groupTurn.runId = item.subagentRunId
        if (item.prompt) groupTurn.prompt = item.prompt
        if (typeof item.usedToolCalls === 'number') {
          groupTurn.usedToolCalls = item.usedToolCalls
        }
        if (typeof item.maxToolCalls === 'number') {
          groupTurn.maxToolCalls = item.maxToolCalls
        }
        if (typeof item.usedTokens === 'number') {
          groupTurn.usedTokens = item.usedTokens
        }
        if (typeof item.maxTokens === 'number') {
          groupTurn.maxTokens = item.maxTokens
        }
        if (item.resultSummary) groupTurn.resultSummary = item.resultSummary
        if (
          item.subagentStatus === 'completed' ||
          item.subagentStatus === 'failed' ||
          item.subagentStatus === 'cancelled' ||
          item.subagentStatus === 'budget_exhausted' ||
          item.subagentStatus === 'handed_off'
        ) {
          groupTurn.completedAt = item.createdAt
        }
        groupTurn.sequence = Math.max(groupTurn.sequence, item.sequence)
      }
      continue
    }

    if (item.subagentId) {
      const state = ensureSubagentGroup(item, item.subagentId)
      routeItemIntoTurns(item, state.context)
      const groupTurn = topContext.turns[state.index]
      if (groupTurn?.kind === 'subagent_group') {
        groupTurn.children = finalizeActionTurns(state.context.turns, toolCallGroupingPreference)
        groupTurn.sequence = Math.max(groupTurn.sequence, item.sequence)
      }
      continue
    }

    const sawUser = routeItemIntoTurns(item, topContext)
    if (sawUser) {
      hasUserMessage = true
    }
  }

  return {
    visibleTurns: finalizeActionTurns(topContext.turns, toolCallGroupingPreference),
    hasUserMessage,
  }
}

function getConversationProjection(
  runtimeStreamItems: readonly RuntimeStreamViewItem[],
  toolCallGroupingPreference: ToolCallGroupingPreference,
): ConversationProjection {
  const cached = conversationProjectionCache.get(runtimeStreamItems)
  const cachedProjection = cached?.[toolCallGroupingPreference]
  if (cachedProjection) {
    return cachedProjection
  }

  const projection = buildConversationProjection(runtimeStreamItems, toolCallGroupingPreference)
  conversationProjectionCache.set(runtimeStreamItems, {
    ...cached,
    [toolCallGroupingPreference]: projection,
  })
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
  return findTranscriptForPendingPrompt(runtimeStreamItems, pendingPrompt) !== null
}

function findTranscriptForPendingPrompt(
  runtimeStreamItems: readonly RuntimeStreamViewItem[],
  pendingPrompt: PendingPromptTurn | null,
): Extract<RuntimeStreamViewItem, { kind: 'transcript' }> | null {
  if (!pendingPrompt) {
    return null
  }

  const promptText = userVisiblePromptText(pendingPrompt.text).trim()
  if (!promptText) {
    return null
  }

  const queuedAtMs = Date.parse(pendingPrompt.queuedAt ?? '')
  const hasQueuedTimestamp = Number.isFinite(queuedAtMs)

  for (const item of runtimeStreamItems) {
    if (
      item.kind !== 'transcript' ||
      item.role !== 'user' ||
      userVisiblePromptText(item.text).trim() !== promptText
    ) {
      continue
    }

    if (!hasQueuedTimestamp) {
      return item
    }

    const itemCreatedAtMs = Date.parse(item.createdAt)
    if (
      Number.isFinite(itemCreatedAtMs) &&
      itemCreatedAtMs >= queuedAtMs - PENDING_PROMPT_TRANSCRIPT_CLOCK_SKEW_MS
    ) {
      return item
    }
  }

  return null
}

function getPendingPromptTurnId(pendingPrompt: PendingPromptTurn): string {
  return `pending-prompt:${pendingPrompt.id}`
}

/**
 * Whether `turn` is the persisted copy (live echo or refetched history) of a
 * prompt submitted at `queuedAt`. Text alone is not enough: an identical
 * prompt sent earlier in the session must not swallow a fresh submission, so
 * when both timestamps are known the persisted copy must not predate the
 * submission beyond clock skew.
 */
function userMessageTurnEchoesSubmittedPrompt(
  turn: ConversationTurn,
  submittedText: string,
  queuedAt: string | null,
): boolean {
  if (turn.kind !== 'message' || turn.role !== 'user') {
    return false
  }
  if (turn.id.startsWith('pending-prompt:')) {
    return false
  }

  const promptText = normalizeConversationTurnText(userVisiblePromptText(submittedText))
  if (!promptText || normalizeConversationTurnText(userVisiblePromptText(turn.text)) !== promptText) {
    return false
  }

  const queuedAtMs = Date.parse(queuedAt ?? '')
  if (!Number.isFinite(queuedAtMs)) {
    return true
  }
  const createdAtMs = Date.parse(turn.createdAt ?? '')
  if (!Number.isFinite(createdAtMs)) {
    return false
  }
  return createdAtMs >= queuedAtMs - PENDING_PROMPT_TRANSCRIPT_CLOCK_SKEW_MS
}

function pendingPromptEchoedInTurns(
  turns: readonly ConversationTurn[],
  pendingPrompt: PendingPromptTurn | null,
): boolean {
  if (!pendingPrompt) {
    return false
  }
  return turns.some((turn) =>
    userMessageTurnEchoesSubmittedPrompt(turn, pendingPrompt.text, pendingPrompt.queuedAt),
  )
}

function appendPendingPromptTurn(
  turns: ConversationTurn[],
  pendingPrompt: PendingPromptTurn | null,
): ConversationTurn[] {
  if (!pendingPrompt) {
    return turns
  }

  const text = pendingPrompt.text.trim()
  if (!text && !pendingPrompt.attachments?.length) {
    return turns
  }

  const visibleText = normalizeConversationTurnText(userVisiblePromptText(text))
  const alreadyVisible = Boolean(
    visibleText &&
      ((pendingPrompt.id.startsWith('queued:') &&
        turns.some(
          (turn) =>
            turn.kind === 'message' &&
            turn.role === 'user' &&
            normalizeConversationTurnText(userVisiblePromptText(turn.text)) === visibleText,
        )) ||
        // The persisted copy of this same submission may already be visible —
        // e.g. the historical transcript refetched after the run terminated —
        // in which case appending the pending bubble would duplicate it.
        pendingPromptEchoedInTurns(turns, pendingPrompt)),
  )
  if (alreadyVisible) {
    return turns
  }

  const latestSequence = turns.reduce((current, turn) => Math.max(current, turn.sequence), 0)
  return [
    ...turns,
    {
      id: getPendingPromptTurnId(pendingPrompt),
      kind: 'message',
      role: 'user',
      sequence: latestSequence + 0.5,
      text,
      createdAt: pendingPrompt.queuedAt,
      attachments: pendingPrompt.attachments,
    },
  ]
}

function pendingComposerAttachmentPreviewSrc(
  attachment: ComposerPendingAttachment & { absolutePath: string },
): string | undefined {
  if (attachment.kind !== 'image') {
    return attachment.previewUrl
  }

  try {
    return convertFileSrc(attachment.absolutePath)
  } catch {
    return attachment.previewUrl
  }
}

function pendingComposerAttachmentsToConversation(
  attachments: readonly ComposerPendingAttachment[],
): ConversationMessageAttachment[] | undefined {
  const readyAttachments = attachments.filter(
    (attachment): attachment is ComposerPendingAttachment & { absolutePath: string } =>
      attachment.status === 'ready' && typeof attachment.absolutePath === 'string',
  )
  if (readyAttachments.length === 0) {
    return undefined
  }

  return readyAttachments.map((attachment) => ({
    id: attachment.id,
    kind: attachment.kind,
    mediaType: attachment.mediaType,
    originalName: attachment.originalName,
    sizeBytes: attachment.sizeBytes,
    title: attachment.originalName,
    alt: attachment.kind === 'image' ? attachment.originalName : null,
    previewSrc: pendingComposerAttachmentPreviewSrc(attachment),
    absolutePath: attachment.absolutePath,
  }))
}

function composerPathName(absolutePath: string): string {
  const parts = absolutePath.split(/[\\/]+/).filter(Boolean)
  return parts.at(-1) ?? absolutePath
}

function projectPathName(path: string): string {
  const parts = path.split('/').filter(Boolean)
  return parts.at(-1) ?? path
}

function projectPathDisplay(path: string): string {
  return path.split('/').filter(Boolean).join('/') || '/'
}

function projectVirtualPathToAbsolutePath(rootPath: string | null | undefined, path: string): string | null {
  const root = rootPath?.trim()
  if (!root) return null
  const segments = path.split('/').filter(Boolean)
  if (segments.length === 0) return root
  const separator = root.includes('\\') ? '\\' : '/'
  return `${root.replace(/[\\/]+$/, '')}${separator}${segments.join(separator)}`
}

function composerContextIdForPath(absolutePath: string): string {
  let hash = 0
  for (let index = 0; index < absolutePath.length; index += 1) {
    hash = ((hash << 5) - hash + absolutePath.charCodeAt(index)) | 0
  }
  return `linked-folder-${Math.abs(hash).toString(36)}`
}

function composerContextIdForProjectPath(projectId: string, kind: 'file' | 'folder', path: string): string {
  return `linked-project-${kind}-${composerContextHash(`${projectId}:${kind}:${path}`)}`
}

function composerContextHash(value: string): string {
  let hash = 0
  for (let index = 0; index < value.length; index += 1) {
    hash = ((hash << 5) - hash + value.charCodeAt(index)) | 0
  }
  return Math.abs(hash).toString(36)
}

function linkedFolderHiddenPrompt(absolutePath: string): string {
  return [
    'Linked folder context:',
    `- ${absolutePath}`,
    'The user attached this folder for the agent to inspect if it is relevant to the request.',
  ].join('\n')
}

function linkedProjectPathHiddenPrompt(input: {
  kind: 'file' | 'folder'
  path: string
  absolutePath: string | null
}): string {
  const label = input.kind === 'folder' ? 'folder' : 'file'
  const displayPath = projectPathDisplay(input.path)
  const absolutePath = input.absolutePath ? ` (${input.absolutePath})` : ''
  return [
    `Linked project ${label} context:`,
    `- ${displayPath}${absolutePath}`,
    `The user attached this ${label} for the agent to inspect if it is relevant to the request.`,
  ].join('\n')
}

interface ComposerProjectPathCandidate {
  id: string
  kind: 'file' | 'folder'
  path: string
  title: string
}

function buildComposerProjectPathCandidates(
  projectId: string,
  files: readonly ProjectFileIndexEntryDto[],
): ComposerProjectPathCandidate[] {
  const foldersByPath = new Map<string, ComposerProjectPathCandidate>()
  const candidates: ComposerProjectPathCandidate[] = []

  const addFolder = (path: string) => {
    if (path === '/' || foldersByPath.has(path)) return
    foldersByPath.set(path, {
      id: composerContextIdForProjectPath(projectId, 'folder', path),
      kind: 'folder',
      path,
      title: projectPathName(path),
    })
  }

  for (const file of files) {
    candidates.push({
      id: composerContextIdForProjectPath(projectId, 'file', file.path),
      kind: 'file',
      path: file.path,
      title: file.name,
    })

    const parts = file.parentPath.split('/').filter(Boolean)
    for (let length = 1; length <= parts.length; length += 1) {
      addFolder(`/${parts.slice(0, length).join('/')}`)
    }
  }

  return [
    ...foldersByPath.values(),
    ...candidates,
  ].sort((left, right) => left.path.localeCompare(right.path))
}

function rankComposerProjectPathCandidates(
  candidates: readonly ComposerProjectPathCandidate[],
  query: string,
): ComposerContextMentionOption[] {
  const normalizedQuery = query.trim().replace(/^\/+/, '').toLowerCase()
  const ranked = candidates
    .map((candidate) => ({
      candidate,
      score: composerProjectPathCandidateScore(candidate, normalizedQuery),
    }))
    .filter((entry): entry is { candidate: ComposerProjectPathCandidate; score: number } =>
      Number.isFinite(entry.score),
    )
    .sort((left, right) => {
      if (left.score !== right.score) return left.score - right.score
      if (left.candidate.kind !== right.candidate.kind) {
        return left.candidate.kind === 'folder' ? -1 : 1
      }
      return left.candidate.path.localeCompare(right.candidate.path)
    })

  return ranked.slice(0, 12).map(({ candidate }) => ({
    id: candidate.id,
    kind: candidate.kind,
    title: candidate.title,
    subtitle: projectPathDisplay(candidate.path),
  }))
}

function composerProjectPathCandidateScore(
  candidate: ComposerProjectPathCandidate,
  query: string,
): number {
  if (!query) {
    return (candidate.kind === 'folder' ? 0 : 2) + candidate.path.length * 0.01
  }

  const name = candidate.title.toLowerCase()
  const path = projectPathDisplay(candidate.path).toLowerCase()
  if (name === query) return 0
  if (path === query) return 0.1
  if (name.startsWith(query)) return 1 + name.length * 0.01
  if (path.startsWith(query)) return 2 + path.length * 0.01
  if (name.includes(query)) return 3 + name.indexOf(query) * 0.05
  if (path.includes(query)) return 4 + path.indexOf(query) * 0.05

  const fuzzyName = composerFuzzyScore(query, name)
  const fuzzyPath = composerFuzzyScore(query, path)
  const fuzzy = Math.min(fuzzyName ?? Number.POSITIVE_INFINITY, fuzzyPath ?? Number.POSITIVE_INFINITY)
  return Number.isFinite(fuzzy) ? 6 + fuzzy : Number.POSITIVE_INFINITY
}

function composerFuzzyScore(query: string, candidate: string): number | null {
  let queryIndex = 0
  let score = candidate.length * 0.01
  let lastMatch = -1

  for (let index = 0; index < candidate.length && queryIndex < query.length; index += 1) {
    if (candidate[index] !== query[queryIndex]) continue
    score += lastMatch >= 0 ? Math.max(0, index - lastMatch - 1) : index * 0.15
    if (lastMatch + 1 === index) score -= 0.25
    lastMatch = index
    queryIndex += 1
  }

  return queryIndex === query.length ? score : null
}

function isPathDirectoryAttachmentError(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false
  const code = (error as { code?: unknown }).code
  return code === 'agent_attachment_source_not_file'
}

function getConversationTurnRunIdFromId(id: string): string | null {
  const toolMatch = /^tool:([^:]+):/.exec(id)
  if (toolMatch) {
    return toolMatch[1] ?? null
  }
  const match = /^(?:transcript|history|activity):(.+):[^:]+$/.exec(id)
  return match?.[1] ?? null
}

function getConversationTurnRunId(turn: ConversationTurn): string | null {
  const id = turn.kind === 'routing_suggestion'
    ? turn.id.replace(/^routing_suggestion:/, '')
    : turn.id
  return getConversationTurnRunIdFromId(id)
}

function normalizeConversationTurnText(text: string): string {
  return text.trim().replace(/\s+/g, ' ')
}

const ROUTING_DECLINE_CONTINUATION_PREFIX =
  'The user chose to stay with the current Agent instead of switching to '
const ROUTING_DECLINE_CONTINUATION_BODY =
  'Continue the original request now. Do not stop at another routing recommendation for this same request.'
const ROUTING_ACCEPT_CONTINUATION_PREFIX =
  'The user accepted the routing suggestion to switch to '
const ROUTING_ACCEPT_CONTINUATION_BODY =
  'Continue the original request now in this same session.'

function conversationMessageCovers(
  coveringTurn: ConversationTurn,
  candidateTurn: ConversationTurn,
): boolean {
  if (
    coveringTurn.kind !== 'message' ||
    candidateTurn.kind !== 'message' ||
    coveringTurn.role !== candidateTurn.role
  ) {
    return false
  }

  // A pending prompt bubble carries no run id, so the run-scoped coverage
  // below can never match it. Reconcile it against the persisted copy of the
  // same submission (live echo or refetched history) by text + submit time.
  if (
    candidateTurn.role === 'user' &&
    candidateTurn.id.startsWith('pending-prompt:')
  ) {
    return userMessageTurnEchoesSubmittedPrompt(
      coveringTurn,
      candidateTurn.text,
      candidateTurn.createdAt ?? null,
    )
  }

  const coveringRunId = getConversationTurnRunId(coveringTurn)
  const candidateRunId = getConversationTurnRunId(candidateTurn)
  if (!coveringRunId || coveringRunId !== candidateRunId) {
    return false
  }

  const coveringText = normalizeConversationTurnText(coveringTurn.text)
  const candidateText = normalizeConversationTurnText(candidateTurn.text)
  if (!coveringText || !candidateText) {
    const coveringAttachments = coveringTurn.attachments?.map((attachment) => attachment.id).join('|') ?? ''
    const candidateAttachments = candidateTurn.attachments?.map((attachment) => attachment.id).join('|') ?? ''
    return Boolean(coveringAttachments && coveringAttachments === candidateAttachments)
  }

  return (
    coveringText === candidateText ||
    (candidateText.length >= 24 && coveringText.includes(candidateText))
  )
}

function conversationTurnActionKeys(turn: ConversationTurn): string[] {
  if (turn.kind === 'action') {
    const runId = getConversationTurnRunId(turn)
    return runId ? [`action:${runId}:${turn.toolCallId}`] : []
  }

  if (turn.kind === 'action_group') {
    return turn.actions
      .map((action) => {
        const runId = getConversationTurnRunIdFromId(action.id)
        return runId ? `action:${runId}:${action.toolCallId}` : null
      })
      .filter((key): key is string => Boolean(key))
  }

  return []
}

function conversationActionCovers(
  coveringTurn: ConversationTurn,
  candidateTurn: ConversationTurn,
): boolean {
  const candidateKeys = conversationTurnActionKeys(candidateTurn)
  if (candidateKeys.length === 0) {
    return false
  }

  const coveringKeys = new Set(conversationTurnActionKeys(coveringTurn))
  if (coveringKeys.size === 0) {
    return false
  }

  return candidateKeys.every((key) => coveringKeys.has(key))
}

function routingSuggestionsAreEquivalent(
  left: Extract<ConversationTurn, { kind: 'routing_suggestion' }>,
  right: Extract<ConversationTurn, { kind: 'routing_suggestion' }>,
): boolean {
  const leftRunId = getConversationTurnRunId(left)
  const rightRunId = getConversationTurnRunId(right)
  return (
    Boolean(leftRunId && rightRunId && leftRunId === rightRunId) &&
    left.targetKind === right.targetKind &&
    left.targetAgentId === right.targetAgentId &&
    left.targetAgentDefinitionId === right.targetAgentDefinitionId &&
    left.targetAgentDefinitionVersion === right.targetAgentDefinitionVersion
  )
}

function findEquivalentMergedRoutingSuggestionIndex(
  mergedTurns: readonly ConversationTurn[],
  candidateTurn: ConversationTurn,
): number {
  if (candidateTurn.kind !== 'routing_suggestion') {
    return -1
  }

  return mergedTurns.findIndex(
    (turn) =>
      turn.kind === 'routing_suggestion' &&
      routingSuggestionsAreEquivalent(turn, candidateTurn),
  )
}

function isConversationTurnCoveredByTurns(
  candidateTurn: ConversationTurn,
  coveringTurns: readonly ConversationTurn[],
): boolean {
  return coveringTurns.some((coveringTurn) =>
    conversationMessageCovers(coveringTurn, candidateTurn) ||
    conversationActionCovers(coveringTurn, candidateTurn),
  )
}

function findMergedConversationTurnIndex(
  mergedTurns: readonly ConversationTurn[],
  candidateTurn: ConversationTurn,
): number {
  const idIndex = mergedTurns.findIndex((turn) => turn.id === candidateTurn.id)
  if (idIndex >= 0) {
    return idIndex
  }

  return mergedTurns.findIndex((turn) =>
    conversationMessageCovers(turn, candidateTurn) ||
    conversationActionCovers(turn, candidateTurn),
  )
}

function findAnchoredConversationInsertionIndex(
  mergedTurns: readonly ConversationTurn[],
  currentTurns: readonly ConversationTurn[],
  currentIndex: number,
): number {
  for (let index = currentIndex - 1; index >= 0; index -= 1) {
    const anchorIndex = findMergedConversationTurnIndex(mergedTurns, currentTurns[index])
    if (anchorIndex >= 0) {
      return anchorIndex + 1
    }
  }

  for (let index = currentIndex + 1; index < currentTurns.length; index += 1) {
    const anchorIndex = findMergedConversationTurnIndex(mergedTurns, currentTurns[index])
    if (anchorIndex >= 0) {
      return anchorIndex
    }
  }

  return mergedTurns.length
}

function mergeConversationTurnsByCurrentOrder({
  baseTurns,
  currentTurns,
  replaceEquivalentPendingPrompts = false,
}: {
  baseTurns: readonly ConversationTurn[]
  currentTurns: readonly ConversationTurn[]
  replaceEquivalentPendingPrompts?: boolean
}): ConversationTurn[] {
  const previousIds = new Set(baseTurns.map((turn) => turn.id))
  const mergedTurns = baseTurns.slice() as ConversationTurn[]

  for (let currentIndex = 0; currentIndex < currentTurns.length; currentIndex += 1) {
    const currentTurn = currentTurns[currentIndex]
    if (previousIds.has(currentTurn.id)) {
      continue
    }

    const equivalentRoutingIndex = findEquivalentMergedRoutingSuggestionIndex(
      mergedTurns,
      currentTurn,
    )
    if (equivalentRoutingIndex >= 0) {
      mergedTurns[equivalentRoutingIndex] = currentTurn
      previousIds.add(currentTurn.id)
      continue
    }

    if (isConversationTurnCoveredByTurns(currentTurn, mergedTurns)) {
      continue
    }

    if (replaceEquivalentPendingPrompts) {
      const equivalentPendingPromptIndex = mergedTurns.findIndex((previousTurn) =>
        pendingPromptTurnSupersededBy(previousTurn, currentTurn),
      )
      if (equivalentPendingPromptIndex >= 0) {
        mergedTurns[equivalentPendingPromptIndex] = currentTurn
        previousIds.add(currentTurn.id)
        continue
      }
    }

    const insertionIndex = findAnchoredConversationInsertionIndex(
      mergedTurns,
      currentTurns,
      currentIndex,
    )
    mergedTurns.splice(insertionIndex, 0, currentTurn)
    previousIds.add(currentTurn.id)
  }

  return mergedTurns
}

function mergeHistoricalAndLiveTurns(
  historicalTurns: readonly ConversationTurn[] | null | undefined,
  liveTurns: readonly ConversationTurn[],
): ConversationTurn[] {
  if (!historicalTurns || historicalTurns.length === 0) {
    return liveTurns.slice()
  }

  if (liveTurns.length === 0) {
    return historicalTurns.slice()
  }

  return mergeConversationTurnsByCurrentOrder({
    baseTurns: historicalTurns,
    currentTurns: liveTurns,
  })
}

interface ConversationContinuitySnapshot {
  sessionKey: string
  turns: ConversationTurn[]
}

function mergeConversationContinuityTurns(
  previousTurns: readonly ConversationTurn[],
  currentTurns: readonly ConversationTurn[],
): ConversationTurn[] {
  return mergeConversationTurnsByCurrentOrder({
    baseTurns: previousTurns,
    currentTurns,
    replaceEquivalentPendingPrompts: true,
  })
}

function pendingPromptTurnSupersededBy(
  previousTurn: ConversationTurn,
  currentTurn: ConversationTurn,
): boolean {
  if (
    previousTurn.kind !== 'message' ||
    currentTurn.kind !== 'message' ||
    previousTurn.role !== 'user' ||
    currentTurn.role !== 'user' ||
    !previousTurn.id.startsWith('pending-prompt:')
  ) {
    return false
  }

  if (currentTurn.id.startsWith('pending-prompt:')) {
    return previousTurn.text.trim() === currentTurn.text.trim()
  }

  // The persisted copy of the same submission (live echo or refetched
  // history) replaces the stale pending bubble instead of duplicating it.
  return userMessageTurnEchoesSubmittedPrompt(
    currentTurn,
    previousTurn.text,
    previousTurn.createdAt ?? null,
  )
}

function useContinuousConversationTurns(
  turns: ConversationTurn[],
  {
    sessionKey,
    preserveDuringTransition,
    preserveSameSession,
  }: {
    sessionKey: string
    preserveDuringTransition: boolean
    preserveSameSession: boolean
  },
): ConversationTurn[] {
  const continuityRef = useRef<ConversationContinuitySnapshot | null>(null)
  const visibleTurns = useMemo(() => {
    const previous = continuityRef.current
    const currentIds = new Set(turns.map((turn) => turn.id))
    const previousTurns = previous?.turns ?? []
    const sharedTurnCount = previousTurns.reduce(
      (count, turn) => count + (currentIds.has(turn.id) ? 1 : 0),
      0,
    )
    const missingPreviousTurnCount = previousTurns.length - sharedTurnCount
    const looksLikeRuntimeReset =
      missingPreviousTurnCount > 0 && (sharedTurnCount === 0 || sharedTurnCount <= 2)
    if (
      (preserveDuringTransition || preserveSameSession) &&
      previous?.sessionKey === sessionKey &&
      previous.turns.length > 0 &&
      looksLikeRuntimeReset
    ) {
      return mergeConversationContinuityTurns(previous.turns, turns)
    }

    return turns
  }, [preserveDuringTransition, preserveSameSession, sessionKey, turns])

  useEffect(() => {
    if (visibleTurns.length === 0 && !preserveDuringTransition) {
      continuityRef.current = null
      return
    }

    if (visibleTurns.length > 0) {
      continuityRef.current = {
        sessionKey,
        turns: visibleTurns,
      }
    }
  }, [preserveDuringTransition, sessionKey, visibleTurns])

  return visibleTurns
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
  agentDefaultModels = {},
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
  onClearSidebarChat,
  sidebarChatClearDisabled = false,
  sidebarChatClearLabel = 'Clear Computer Use chat',
  sidebarChatClearTitle,
  sidebarChatClearPending = false,
  onCloseSidebar,
  historicalConversationTurns,
  historicalConversationTurnsLoading = false,
  pendingInitialRuntimeAgentId = null,
  pendingInitialAgentDefinitionId = null,
  onPendingInitialRuntimeAgentIdConsumed,
  pendingComposerInsert = null,
  onPendingComposerInsertConsumed,
  toolCallGroupingPreference = 'grouped',
  agentRoutingAutoSwitchEnabled = false,
}: AgentRuntimeProps) {
  const paneRootRef = useRef<HTMLDivElement | null>(null)
  const compactPaneWidthPx = inSidebar
    ? SIDEBAR_AGENT_PANE_COMPACT_WIDTH_PX
    : AGENT_PANE_COMPACT_WIDTH_PX
  const isCompactWidth = useCompactPaneWidth(paneRootRef, compactPaneWidthPx)
  const effectiveDensity: NonNullable<AgentRuntimeProps['density']> =
    density === 'compact' || isCompactWidth ? 'compact' : 'comfortable'
  const runtimeSession = agent.runtimeSession ?? null
  const runtimeRun = agent.runtimeRun ?? null
  const renderableRuntimeRun = hasUsableRuntimeRunId(runtimeRun) ? runtimeRun : null
  const currentAgentLabelForRouting =
    renderableRuntimeRun?.controls?.active.runtimeAgentLabel.trim() ||
    agent.runtimeRunActiveControls?.runtimeAgentLabel.trim() ||
    renderableRuntimeRun?.controls?.selected.runtimeAgentLabel.trim() ||
    agent.selectedRuntimeAgentLabel.trim() ||
    getRuntimeAgentLabel(agent.selectedRuntimeAgentId)
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
    () => getConversationProjection(runtimeStreamItemsForTurns, toolCallGroupingPreference),
    [runtimeStreamItemsForTurns, toolCallGroupingPreference],
  )
  const visibleTurns = conversationProjection.visibleTurns
  // Historical turns from prior runs in this agent session (loaded from the
  // persisted session transcript) are prepended so the conversation reads as
  // a continuous thread across same-session handoffs. The active run's items
  // come from the live stream; the merge also guards the transient reload /
  // session-switch case where a stale transcript fetch briefly overlaps with
  // the live replay for the same run.
  const visibleTurnsWithHistory = useMemo(
    () => mergeHistoricalAndLiveTurns(historicalConversationTurns, visibleTurns),
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
    if (
      !agent.selectedPrompt.hasQueuedPrompt ||
      !text ||
      isInternalRoutingContinuationPromptText(text)
    ) {
      return null
    }

    return {
      id: `queued:${agent.selectedPrompt.queuedAt ?? text}`,
      text,
      queuedAt: agent.selectedPrompt.queuedAt ?? null,
    }
  }, [agent.selectedPrompt.hasQueuedPrompt, agent.selectedPrompt.queuedAt, agent.selectedPrompt.text])
  const pendingPromptTurn = useMemo<PendingPromptTurn | null>(() => {
    // A pending prompt is superseded by its persisted copy wherever that copy
    // lives: the live stream echo, or the historical transcript once a
    // stopped/failed run's items are refetched into history.
    if (
      optimisticPromptTurn &&
      !hasTranscriptForPendingPrompt(runtimeStreamItems, optimisticPromptTurn) &&
      !pendingPromptEchoedInTurns(visibleTurnsForDisplay, optimisticPromptTurn)
    ) {
      return optimisticPromptTurn
    }

    if (
      selectedQueuedPromptTurn &&
      !hasTranscriptForPendingPrompt(runtimeStreamItems, selectedQueuedPromptTurn) &&
      !pendingPromptEchoedInTurns(visibleTurnsForDisplay, selectedQueuedPromptTurn)
    ) {
      return selectedQueuedPromptTurn
    }

    return null
  }, [optimisticPromptTurn, runtimeStreamItems, selectedQueuedPromptTurn, visibleTurnsForDisplay])
  const rawVisibleTurnsWithPendingPrompt = useMemo(
    () => appendPendingPromptTurn(visibleTurnsForDisplay, pendingPromptTurn),
    [pendingPromptTurn, visibleTurnsForDisplay],
  )
  const submittedPromptTurnIdOverridesRef = useRef<{
    sessionKey: string
    byTranscriptId: Map<string, string>
  } | null>(null)
  const conversationSessionKey = `${agent.project.id}:${agent.project.selectedAgentSessionId ?? 'none'}`
  if (submittedPromptTurnIdOverridesRef.current?.sessionKey !== conversationSessionKey) {
    submittedPromptTurnIdOverridesRef.current = {
      sessionKey: conversationSessionKey,
      byTranscriptId: new Map<string, string>(),
    }
  }
  const pendingPromptForStableId = optimisticPromptTurn ?? selectedQueuedPromptTurn
  const visibleTurnsWithStableSubmittedPromptIds = useMemo(() => {
    const overrides = submittedPromptTurnIdOverridesRef.current?.byTranscriptId
    if (!overrides) {
      return rawVisibleTurnsWithPendingPrompt
    }

    const matchedTranscript = findTranscriptForPendingPrompt(runtimeStreamItems, pendingPromptForStableId)
    if (matchedTranscript && pendingPromptForStableId) {
      if (!overrides.has(matchedTranscript.id)) {
        overrides.set(matchedTranscript.id, getPendingPromptTurnId(pendingPromptForStableId))
      }
    }

    if (overrides.size === 0) {
      return rawVisibleTurnsWithPendingPrompt
    }

    let changed = false
    const stableTurns = rawVisibleTurnsWithPendingPrompt.map((turn) => {
      if (turn.kind !== 'message' || turn.role !== 'user') {
        return turn
      }

      const stableId = overrides.get(turn.id)
      if (!stableId || stableId === turn.id) {
        return turn
      }

      changed = true
      return {
        ...turn,
        id: stableId,
      }
    })

    return changed ? stableTurns : rawVisibleTurnsWithPendingPrompt
  }, [pendingPromptForStableId, rawVisibleTurnsWithPendingPrompt, runtimeStreamItems])
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
        hasLiveRuntimeStream &&
        !runtimeStream?.failure
      ),
  )
  const preserveConversationDuringRuntimeTransition = Boolean(
    isQueueingRuntimePrompt ||
      promptSubmissionPending ||
      agent.selectedPrompt.hasQueuedPrompt ||
      streamStatus === 'subscribing' ||
      streamStatus === 'replaying' ||
      streamStatus === 'live' ||
      streamStatus === 'complete' ||
      (renderableRuntimeRun?.isActive && runtimeStreamItems.length === 0),
  )
  const conversationContinuityKey = `${conversationSessionKey}:${toolCallGroupingPreference}`
  const continuousVisibleTurnsWithPendingPrompt = useContinuousConversationTurns(
    visibleTurnsWithStableSubmittedPromptIds,
    {
      sessionKey: conversationContinuityKey,
      preserveDuringTransition: preserveConversationDuringRuntimeTransition,
      preserveSameSession: historicalConversationTurnsLoading,
    },
  )
  const visibleTurnsWithPersistedRoutingResolutions = useMemo(
    () => {
      const persistedTurns = applyPersistedRoutingContinuationResolutions(continuousVisibleTurnsWithPendingPrompt)
      const selectedPromptText = agent.selectedPrompt.hasQueuedPrompt
        ? agent.selectedPrompt.text
        : null
      const selectedPromptDecision = selectedPromptText
        ? parseInternalRoutingContinuationPromptText(selectedPromptText)
        : null
      const shouldApplySelectedPromptDecision = Boolean(
        selectedPromptDecision &&
          (
            isQueueingRuntimePrompt ||
            promptSubmissionPending ||
            (renderableRuntimeRun && !renderableRuntimeRun.isTerminal)
          ),
      )

      return shouldApplySelectedPromptDecision && selectedPromptDecision
        ? applyRoutingContinuationDecision(persistedTurns, selectedPromptDecision, persistedTurns.length)
        : persistedTurns
    },
    [
      agent.selectedPrompt.hasQueuedPrompt,
      agent.selectedPrompt.text,
      continuousVisibleTurnsWithPendingPrompt,
      isQueueingRuntimePrompt,
      promptSubmissionPending,
      renderableRuntimeRun,
    ],
  )
  const visibleTurnsWithPendingPrompt = useMemo(
    () => filterInternalRoutingContinuationTurns(visibleTurnsWithPersistedRoutingResolutions),
    [visibleTurnsWithPersistedRoutingResolutions],
  )
  const [optimisticResolvedOwnedActionPrompts, setOptimisticResolvedOwnedActionPrompts] = useState<
    ReadonlySet<string>
  >(() => new Set())
  const visibleTurnsWithOwnedActionPromptState = useMemo(() => {
    if (optimisticResolvedOwnedActionPrompts.size === 0) {
      return visibleTurnsWithPendingPrompt
    }

    let changed = false
    const turns = visibleTurnsWithPendingPrompt.map((turn) => {
      if (
        turn.kind !== 'action_prompt' ||
        !turn.runId ||
        turn.isResolved ||
        !optimisticResolvedOwnedActionPrompts.has(
          ownedAgentActionPromptKey(turn.runId, turn.actionId),
        )
      ) {
        return turn
      }

      changed = true
      return {
        ...turn,
        isResolved: true,
      }
    })

    return changed ? turns : visibleTurnsWithPendingPrompt
  }, [optimisticResolvedOwnedActionPrompts, visibleTurnsWithPendingPrompt])
  useEffect(() => {
    setOptimisticResolvedOwnedActionPrompts((current) =>
      current.size === 0 ? current : new Set(),
    )
  }, [conversationSessionKey])
  const hasUserMessage =
    visibleTurnsWithPendingPrompt.some((turn) => turn.kind === 'message' && turn.role === 'user')
  const selectedAgentSession = (agent.project.selectedAgentSession ?? null) as AgentSessionView | null
  const selectedAgentSessionId =
    selectedAgentSession?.agentSessionId ?? agent.project.selectedAgentSessionId ?? null
  const hasSelectedAgentSession = Boolean(selectedAgentSessionId?.trim())
  const isComputerUseSession = Boolean(selectedAgentSession?.isComputerUse)
  const isComputerUseSidebar = inSidebar && isComputerUseSession
  const [codeUndoStates, setCodeUndoStates] = useState<Record<string, CodeUndoUiState>>({})
  const [returnSessionToHereStates, setReturnSessionToHereStates] = useState<Record<string, CodeUndoUiState>>({})

  const [handoffSummaryOpen, setHandoffSummaryOpen] = useState(false)
  const [handoffSummaryStatus, setHandoffSummaryStatus] =
    useState<HandoffContextDialogStatus>('idle')
  const [handoffSummaryError, setHandoffSummaryError] = useState<string | null>(null)
  const [handoffSummary, setHandoffSummary] =
    useState<AgentHandoffContextSummaryDto | null>(null)
  const [handoffSummaryRequest, setHandoffSummaryRequest] = useState<{
    sourceRunId: string | null
    targetRunId: string | null
  } | null>(null)
  const getAgentHandoffContextSummaryFn = desktopAdapter?.getAgentHandoffContextSummary
  const handoffSummaryProjectId = agent.project.id
  const fetchHandoffSummary = useCallback(
    async (request: { sourceRunId: string | null; targetRunId: string | null }) => {
      if (!getAgentHandoffContextSummaryFn) return
      setHandoffSummaryStatus('loading')
      setHandoffSummaryError(null)
      try {
        const result = await getAgentHandoffContextSummaryFn({
          projectId: handoffSummaryProjectId,
          targetRunId: request.targetRunId ?? null,
          sourceRunId: request.targetRunId ? null : request.sourceRunId ?? null,
        })
        setHandoffSummary(result)
        setHandoffSummaryStatus('ready')
      } catch (error) {
        setHandoffSummaryStatus('error')
        setHandoffSummaryError(
          error instanceof Error
            ? error.message
            : 'Failed to load handoff context summary.',
        )
      }
    },
    [getAgentHandoffContextSummaryFn, handoffSummaryProjectId],
  )
  const handleOpenHandoffSummary = useCallback(
    (request: { sourceRunId?: string | null; targetRunId?: string | null }) => {
      const normalized = {
        sourceRunId: request.sourceRunId ?? null,
        targetRunId: request.targetRunId ?? null,
      }
      setHandoffSummaryRequest(normalized)
      setHandoffSummaryOpen(true)
      setHandoffSummary(null)
      void fetchHandoffSummary(normalized)
    },
    [fetchHandoffSummary],
  )
  const refreshHandoffSummary = useCallback(() => {
    if (!handoffSummaryRequest) return
    void fetchHandoffSummary(handoffSummaryRequest)
  }, [fetchHandoffSummary, handoffSummaryRequest])
  const footerHandoffSourceRunId =
    renderableRuntimeRun?.runId ?? runtimeStream?.runId ?? null

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
        inputModalities: option.inputModalities ?? [],
        thinkingEffortOptions: option.thinkingEffortOptions,
        defaultThinkingEffort: option.defaultThinkingEffort,
        contextWindowTokens: option.contextWindowTokens ?? null,
        maxOutputTokens: option.maxOutputTokens ?? null,
        capabilities: option.capabilities ?? null,
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
  const [pendingContextCards, setPendingContextCards] = useState<ComposerPendingContext[]>([])
  const [pendingLinkedPaths, setPendingLinkedPaths] = useState<ComposerPendingLinkedPath[]>([])
  const [composerContextMentionQuery, setComposerContextMentionQuery] = useState<string | null>(null)
  const [composerPathIndexState, setComposerPathIndexState] = useState<{
    status: 'idle' | 'loading' | 'ready' | 'error'
    files: ProjectFileIndexEntryDto[]
    error: string | null
  }>({ status: 'idle', files: [], error: null })
  const pendingAttachmentsRef = useRef<ComposerPendingAttachment[]>([])
  pendingAttachmentsRef.current = pendingAttachments
  const pendingLinkedPathsRef = useRef<ComposerPendingLinkedPath[]>([])
  pendingLinkedPathsRef.current = pendingLinkedPaths
  const consumedComposerInsertIdsRef = useRef<Set<string>>(new Set())
  const composerPathIndexStatusRef = useRef(composerPathIndexState.status)
  composerPathIndexStatusRef.current = composerPathIndexState.status

  const stageAgentAttachment = desktopAdapter?.stageAgentAttachment
  const stageAgentAttachmentPath = desktopAdapter?.stageAgentAttachmentPath
  const discardAgentAttachment = desktopAdapter?.discardAgentAttachment
  const pickComposerFolders = desktopAdapter?.pickComposerFolders
  const listProjectFileIndex = desktopAdapter?.listProjectFileIndex
  const projectIdForAttachments = agent.project.id
  const runIdForAttachments = renderableRuntimeRun?.runId ?? 'pending'

  useEffect(() => {
    setComposerContextMentionQuery(null)
    setComposerPathIndexState({ status: 'idle', files: [], error: null })
  }, [projectIdForAttachments])

  useEffect(() => {
    if (composerContextMentionQuery === null || !listProjectFileIndex) return
    if (
      composerPathIndexStatusRef.current === 'loading' ||
      composerPathIndexStatusRef.current === 'ready'
    ) return

    let cancelled = false
    setComposerPathIndexState((current) => ({ ...current, status: 'loading', error: null }))
    void listProjectFileIndex({
      projectId: projectIdForAttachments,
      includeHidden: false,
      limit: 6000,
    })
      .then((response) => {
        if (cancelled) return
        setComposerPathIndexState({ status: 'ready', files: response.files, error: null })
      })
      .catch((error: unknown) => {
        if (cancelled) return
        setComposerPathIndexState({
          status: 'error',
          files: [],
          error: error instanceof Error ? error.message : 'Project paths unavailable',
        })
      })

    return () => {
      cancelled = true
    }
  }, [
    composerContextMentionQuery,
    listProjectFileIndex,
    projectIdForAttachments,
  ])

  const composerContextMentionCandidates = useMemo(
    () => buildComposerProjectPathCandidates(projectIdForAttachments, composerPathIndexState.files),
    [composerPathIndexState.files, projectIdForAttachments],
  )
  const composerContextMentionOptions = useMemo(
    () =>
      composerContextMentionQuery === null
        ? []
        : rankComposerProjectPathCandidates(
            composerContextMentionCandidates,
            composerContextMentionQuery,
          ),
    [composerContextMentionCandidates, composerContextMentionQuery],
  )

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

  const stageExternalComposerAttachment = useCallback(
    (insertId: string, image: NonNullable<AgentComposerInsert['image']>) => {
      if (!stageAgentAttachment) return

      const classification = classifyAttachment({
        name: image.originalName,
        type: image.mediaType,
        size: image.bytes.byteLength,
      })
      if (classification.kind === null) return

      const id = `composer-insert-${insertId}`
      const previewUrl =
        classification.kind === 'image' && typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function'
          ? URL.createObjectURL(new Blob([image.bytes.slice().buffer as ArrayBuffer], { type: image.mediaType }))
          : undefined
      const optimistic: ComposerPendingAttachment = {
        id,
        kind: classification.kind,
        originalName: image.originalName,
        mediaType: classification.mediaType,
        sizeBytes: image.bytes.byteLength,
        status: 'staging',
        previewUrl,
      }

      setPendingAttachments((prev) =>
        prev.some((attachment) => attachment.id === id) ? prev : [...prev, optimistic],
      )

      void stageAgentAttachment({
        projectId: projectIdForAttachments,
        runId: runIdForAttachments,
        originalName: image.originalName,
        mediaType: classification.mediaType,
        bytes: image.bytes,
      })
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

  const getPendingLinkedPaths = useCallback((): RuntimeLinkedPathDto[] => {
    return pendingLinkedPathsRef.current.map((path) => ({
      kind: path.kind,
      absolutePath: path.absolutePath,
    }))
  }, [])

  const handleSubmitAttachmentsSettled = useCallback(() => {
    setPendingContextCards([])
    setPendingLinkedPaths([])
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
    agentDefaultModels,
    selectedThinkingEffort: agent.selectedThinkingEffort,
    selectedApprovalMode: agent.selectedApprovalMode,
    selectedAutoCompactEnabled: agent.selectedAutoCompactEnabled,
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
    lockedRuntimeAgentId: isComputerUseSession ? 'computer_use' : null,
    pendingInitialRuntimeAgentId,
    pendingInitialAgentDefinitionId,
    onPendingInitialRuntimeAgentIdConsumed,
    onStartRuntimeRun,
    onStartRuntimeSession,
    onUpdateRuntimeRunControls: canMutateRuntimeRun ? onUpdateRuntimeRunControls : undefined,
    onComposerControlsChange,
    onStopRuntimeRun,
    onResolveOperatorAction,
    onResumeOperatorRun,
    getPendingAttachments,
    getPendingLinkedPaths,
    onSubmitAttachmentsSettled: handleSubmitAttachmentsSettled,
  })

  const handleRemoveContextCard = useCallback(
    (id: string) => {
      setPendingContextCards((prev) => prev.filter((context) => context.id !== id))
      setPendingLinkedPaths((prev) => prev.filter((path) => path.id !== id))
      controller.handleRemoveHiddenDraftPrompt(id)
    },
    [controller],
  )

  const handleAddLinkedFolders = useCallback(
    (paths: readonly string[]) => {
      const folders = paths
        .map((path) => path.trim())
        .filter((path) => path.length > 0)
        .map((path) => ({
          id: composerContextIdForPath(path),
          path,
          name: composerPathName(path),
        }))
      if (folders.length === 0) return

      for (const folder of folders) {
        controller.handleAppendHiddenDraftPrompt(linkedFolderHiddenPrompt(folder.path), folder.id)
      }

      setPendingLinkedPaths((prev) => {
        const existingIds = new Set(prev.map((path) => path.id))
        const additions = folders
          .filter((folder) => !existingIds.has(folder.id))
          .map((folder): ComposerPendingLinkedPath => ({
            id: folder.id,
            kind: 'folder',
            absolutePath: folder.path,
          }))
        return additions.length > 0 ? [...prev, ...additions] : prev
      })

      setPendingContextCards((prev) => {
        const existingIds = new Set(prev.map((context) => context.id))
        const additions = folders
          .filter((folder) => !existingIds.has(folder.id))
          .map((folder): ComposerPendingContext => ({
            id: folder.id,
            kind: 'folder',
            title: folder.name,
            subtitle: folder.path,
          }))
        return additions.length > 0 ? [...prev, ...additions] : prev
      })
    },
    [controller],
  )

  const handleSelectComposerContextMention = useCallback(
    (option: ComposerContextMentionOption) => {
      const candidate = composerContextMentionCandidates.find((item) => item.id === option.id)
      if (!candidate) return

      const absolutePath = projectVirtualPathToAbsolutePath(agent.repositoryPath, candidate.path)
      controller.handleAppendHiddenDraftPrompt(
        linkedProjectPathHiddenPrompt({
          kind: candidate.kind,
          path: candidate.path,
          absolutePath,
        }),
        candidate.id,
      )

      if (absolutePath) {
        setPendingLinkedPaths((prev) => {
          if (prev.some((path) => path.id === candidate.id)) return prev
          return [
            ...prev,
            {
              id: candidate.id,
              kind: candidate.kind,
              absolutePath,
            },
          ]
        })
      }

      setPendingContextCards((prev) => {
        if (prev.some((context) => context.id === candidate.id)) return prev
        return [
          ...prev,
          {
            id: candidate.id,
            kind: candidate.kind,
            title: candidate.title,
            subtitle: candidate.path,
          },
        ]
      })
    },
    [agent.repositoryPath, composerContextMentionCandidates, controller],
  )

  const handlePickComposerFolders = useCallback(() => {
    if (!pickComposerFolders) return
    void pickComposerFolders()
      .then((paths) => handleAddLinkedFolders(paths))
      .catch((error: unknown) => {
        console.warn(error instanceof Error ? error.message : 'Folder selection failed')
      })
  }, [handleAddLinkedFolders, pickComposerFolders])

  const handleDroppedPaths = useCallback(
    (paths: string[]) => {
      if (!stageAgentAttachmentPath) {
        handleAddLinkedFolders(paths)
        return
      }

      for (const rawPath of paths) {
        const absolutePath = rawPath.trim()
        if (absolutePath.length === 0) continue

        void stageAgentAttachmentPath({
          projectId: projectIdForAttachments,
          runId: runIdForAttachments,
          absolutePath,
        })
          .then((staged) => {
            const id = `attachment-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
            let previewUrl: string | undefined
            if (staged.kind === 'image') {
              try {
                previewUrl = convertFileSrc(staged.absolutePath)
              } catch {
                previewUrl = undefined
              }
            }
            setPendingAttachments((prev) => [
              ...prev,
              {
                id,
                kind: staged.kind,
                originalName: staged.originalName,
                mediaType: staged.mediaType,
                sizeBytes: staged.sizeBytes,
                status: 'ready',
                absolutePath: staged.absolutePath,
                previewUrl,
              },
            ])
          })
          .catch((error: unknown) => {
            if (isPathDirectoryAttachmentError(error)) {
              handleAddLinkedFolders([absolutePath])
              return
            }
            console.warn(error instanceof Error ? error.message : 'Path attachment failed')
          })
      }
    },
    [
      handleAddLinkedFolders,
      projectIdForAttachments,
      runIdForAttachments,
      stageAgentAttachmentPath,
    ],
  )

  useEffect(() => {
    if (!pendingComposerInsert) {
      return
    }
    if (consumedComposerInsertIdsRef.current.has(pendingComposerInsert.id)) {
      return
    }

    consumedComposerInsertIdsRef.current.add(pendingComposerInsert.id)
    const hiddenPrompt = pendingComposerInsert.hiddenPrompt?.trim() ?? ''
    const hiddenPromptId = `composer-context-${pendingComposerInsert.id}`
    controller.handleAppendDraftPrompt(pendingComposerInsert.prompt)
    if (hiddenPrompt) {
      controller.handleAppendHiddenDraftPrompt(hiddenPrompt, hiddenPromptId)
      const contextCard = pendingComposerInsert.contextCard
      const contextCardCoveredByImage =
        contextCard?.kind === 'sketch' && Boolean(pendingComposerInsert.image)
      if (contextCard && !contextCardCoveredByImage) {
        setPendingContextCards((prev) =>
          prev.some((context) => context.id === hiddenPromptId)
            ? prev
            : [
                ...prev,
                {
                  id: hiddenPromptId,
                  kind: contextCard.kind,
                  title: contextCard.title,
                  subtitle: contextCard.subtitle,
                },
              ],
        )
      }
    }
    if (pendingComposerInsert.image) {
      stageExternalComposerAttachment(pendingComposerInsert.id, pendingComposerInsert.image)
    }
    onPendingComposerInsertConsumed?.(pendingComposerInsert.id)

    const focusComposer = () => {
      controller.promptInputRef.current?.focus()
    }
    if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(focusComposer)
      return
    }
    window.setTimeout(focusComposer, 0)
  }, [controller, onPendingComposerInsertConsumed, pendingComposerInsert, stageExternalComposerAttachment])

  useEffect(() => {
    if (!optimisticPromptTurn) {
      return
    }

    if (
      hasTranscriptForPendingPrompt(runtimeStreamItems, optimisticPromptTurn) ||
      // A run that ends before the stream echoes the prompt (manual stop,
      // provider preflight failure) surfaces the persisted copy through the
      // refetched historical transcript instead — release the optimistic
      // bubble then too, or it would linger and duplicate the prompt.
      pendingPromptEchoedInTurns(visibleTurnsWithHistory, optimisticPromptTurn)
    ) {
      setOptimisticPromptTurn(null)
    }
  }, [optimisticPromptTurn, runtimeStreamItems, visibleTurnsWithHistory])

  const selectedComposerModel = useMemo(
    () => getComposerModelOption(availableModels, controller.composerModelId),
    [availableModels, controller.composerModelId],
  )
  // A credit/billing limit failure is presented as a dedicated card docked above
  // the composer (see ComposerDock) instead of a red run-failure error. Once the
  // user picks a different model to resolve it (tracked by the failed run id),
  // the card is dismissed.
  const [dismissedCreditLimitRunId, setDismissedCreditLimitRunId] = useState<string | null>(
    null,
  )
  const creditLimitNotice = useMemo(() => {
    const run = renderableRuntimeRun
    if (!run || !run.isFailed) return null
    if (dismissedCreditLimitRunId === run.runId) return null
    return classifyCreditLimitFailure({
      code: run.lastError?.code ?? run.lastErrorCode ?? null,
      message: run.lastError?.message ?? null,
      providerId: run.providerId,
      providerLabel: getCloudProviderLabel(run.providerId),
      modelLabel:
        selectedComposerModel?.providerId === run.providerId
          ? (selectedComposerModel?.displayName ?? null)
          : null,
    })
  }, [renderableRuntimeRun, selectedComposerModel, dismissedCreditLimitRunId])
  // Switching to a different model resolves the out-of-credits situation, so
  // dismiss the credit-limit card for the current failed run when the model
  // selection actually changes.
  const handleComposerModelChangeWithCreditDismiss = useCallback(
    (value: string) => {
      const run = renderableRuntimeRun
      if (run && value !== controller.composerModelId) {
        setDismissedCreditLimitRunId(run.runId)
      }
      controller.handleComposerModelChange(value)
    },
    [renderableRuntimeRun, controller.composerModelId, controller.handleComposerModelChange],
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
  const availableRuntimeAgentIds = useMemo<readonly RuntimeAgentIdDto[]>(
    () =>
      isComputerUseSession
        ? ['computer_use']
        : getRuntimeAgentDescriptorsForProjectOrigin(agentProjectOrigin)
            .map((descriptor) => descriptor.id)
            .filter((id) => id !== 'computer_use'),
    [agentProjectOrigin, isComputerUseSession],
  )
  useEffect(() => {
    if (isComputerUseSession) {
      return
    }
    if (
      !controller.isRuntimeAgentSwitchDisabled &&
      !availableRuntimeAgentIds.includes(controller.composerRuntimeAgentId)
    ) {
      controller.handleComposerRuntimeAgentChange('generalist')
    }
  }, [
    availableRuntimeAgentIds,
    controller.composerRuntimeAgentId,
    controller.isRuntimeAgentSwitchDisabled,
    controller.handleComposerRuntimeAgentChange,
    isComputerUseSession,
  ])

  const [resolvedRoutingTurns, setResolvedRoutingTurns] = useState<Record<string, RoutingResolutionRecord>>({})
  const autoResolvedRoutingTurnIdsRef = useRef<Set<string>>(new Set())
  useEffect(() => {
    autoResolvedRoutingTurnIdsRef.current.clear()
  }, [conversationSessionKey])
  const hasInternalRoutingQueuedPrompt = Boolean(
    agent.selectedPrompt.hasQueuedPrompt &&
      agent.selectedPrompt.text &&
      isInternalRoutingContinuationPromptText(agent.selectedPrompt.text),
  )
  const hasBlockingQueuedPrompt =
    agent.selectedPrompt.hasQueuedPrompt && !hasInternalRoutingQueuedPrompt
  const routingSuggestionActionUnavailableReason =
    promptSubmissionPending || runtimeRunActionStatus === 'running' || controller.runtimeSessionBindInFlight
      ? 'A continuation is already being queued.'
      : hasBlockingQueuedPrompt
        ? 'A continuation prompt is already queued.'
        : !controller.promptInputAvailable
          ? 'Start or reconnect the runtime before continuing.'
          : null
  const recordRoutingResolution = useCallback((turnId: string, decision: RoutingSuggestionDecision) => {
    setResolvedRoutingTurns((previous) => ({
      ...previous,
      [turnId]: getRoutingResolutionForDecision(decision),
    }))
  }, [])
  const applyAcceptedRoutingSelection = useCallback(
    (decision: RoutingSuggestionDecision) => {
      if (decision.kind !== 'accept') {
        return
      }

      if (decision.targetAgentDefinitionId) {
        controller.handleComposerAgentSelectionChange(
          buildComposerAgentSelectionKey(
            decision.targetAgentId,
            decision.targetAgentDefinitionId,
          ),
        )
        return
      }

      controller.handleComposerRuntimeAgentChange(decision.targetAgentId)
    },
    [
      controller.handleComposerAgentSelectionChange,
      controller.handleComposerRuntimeAgentChange,
    ],
  )
  const submitRoutingContinuation = useCallback(
    async (continuation: PendingRoutingContinuation) => {
      const submitted = await controller.handleSubmitExplicitPrompt(continuation.prompt, {
        ...(continuation.controls ? { controls: continuation.controls } : {}),
        promptVisibility: 'internal',
        replaceQueuedPrompt: true,
      })

      if (!submitted) {
        return false
      }

      recordRoutingResolution(continuation.turnId, continuation.decision)
      if (!renderableRuntimeRun || renderableRuntimeRun.isTerminal) {
        applyAcceptedRoutingSelection(continuation.decision)
      }
      return true
    },
    [
      applyAcceptedRoutingSelection,
      controller.handleSubmitExplicitPrompt,
      recordRoutingResolution,
      renderableRuntimeRun,
    ],
  )
  const routingSuggestionDispatchValue = useMemo<RoutingSuggestionDispatchValue>(() => {
    return {
      getRoutingSuggestionActionAvailability: () => ({
        disabled: routingSuggestionActionUnavailableReason !== null,
        reason: routingSuggestionActionUnavailableReason,
      }),
      resolveRoutingSuggestion: (turnId, decision) => {
        if (routingSuggestionActionUnavailableReason !== null) {
          return
        }

        if (decision.kind === 'decline') {
          const continuation: PendingRoutingContinuation = {
            turnId,
            decision,
            prompt: buildRoutingDeclineContinuationPrompt(decision),
            controls: null,
          }

          void submitRoutingContinuation(continuation)
          return
        }

        const targetControls = getComposerControlInput({
          runtimeAgentId: decision.targetAgentId,
          agentDefinitionId: decision.targetAgentDefinitionId ?? null,
          models: availableModels,
          selectionKey: controller.composerModelId,
          thinkingEffort: controller.composerThinkingEffort,
          approvalMode: controller.composerApprovalMode,
          autoCompactEnabled: controller.autoCompactEnabled,
        })
        if (!targetControls) return

        const continuation: PendingRoutingContinuation = {
          turnId,
          decision,
          prompt: buildRoutingAcceptContinuationPrompt(decision),
          controls: targetControls,
        }

        void submitRoutingContinuation(continuation)
      },
    }
  }, [
    availableModels,
    controller.autoCompactEnabled,
    controller.composerApprovalMode,
    controller.composerModelId,
    controller.composerThinkingEffort,
    recordRoutingResolution,
    routingSuggestionActionUnavailableReason,
    submitRoutingContinuation,
  ])
  useEffect(() => {
    if (!agentRoutingAutoSwitchEnabled) return
    if (routingSuggestionActionUnavailableReason !== null) return

    const routingTurn = visibleTurnsWithPendingPrompt.find(
      (turn): turn is Extract<ConversationTurn, { kind: 'routing_suggestion' }> =>
        turn.kind === 'routing_suggestion' &&
        !turn.isResolved &&
        !resolvedRoutingTurns[turn.id] &&
        !autoResolvedRoutingTurnIdsRef.current.has(turn.id),
    )
    if (!routingTurn) return

    autoResolvedRoutingTurnIdsRef.current.add(routingTurn.id)
    routingSuggestionDispatchValue.resolveRoutingSuggestion(
      routingTurn.id,
      buildRoutingAcceptDecisionFromTurn(routingTurn, 'automatic'),
    )
  }, [
    agentRoutingAutoSwitchEnabled,
    resolvedRoutingTurns,
    routingSuggestionActionUnavailableReason,
    routingSuggestionDispatchValue,
    visibleTurnsWithPendingPrompt,
  ])
  function applyRoutingResolutions(turns: ConversationTurn[]): ConversationTurn[] {
    if (Object.keys(resolvedRoutingTurns).length === 0) return turns
    return turns.map((turn) => {
      if (turn.kind !== 'routing_suggestion') return turn
      const resolution = resolvedRoutingTurns[turn.id]
      if (!resolution) return turn
      return {
        ...turn,
        isResolved: true,
        acceptedTarget: resolution.acceptedTarget,
        acceptedTargetAgentDefinitionId: resolution.acceptedTargetAgentDefinitionId,
        acceptedTargetLabel: resolution.acceptedTargetLabel,
        routingResolutionMode: resolution.routingResolutionMode,
      }
    })
  }
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
      selectedRuntimeAgentId: controller.composerRuntimeAgentId,
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
        ? 'Describe the agent...'
      : controller.composerRuntimeAgentId === 'crawl' &&
          !agentRuntimeBlocked &&
          runtimeSession?.isAuthenticated &&
          !renderableRuntimeRun?.isTerminal
        ? 'Map this repository...'
      : controller.composerRuntimeAgentId === 'computer_use' &&
          !agentRuntimeBlocked &&
          runtimeSession?.isAuthenticated &&
          !renderableRuntimeRun?.isTerminal
        ? 'Tell Xero what to do on this computer...'
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
      runtimeStream?.failure ||
      visibleTurnsWithPendingPrompt.length > 0,
  )
  const promptInputLabel = controller.promptInputAvailable ? 'Agent input' : 'Agent input unavailable'
  const sendButtonLabel = controller.promptInputAvailable ? 'Send message' : 'Send message unavailable'
  const [pendingOwnedAgentActionIntent, setPendingOwnedAgentActionIntent] = useState<{
    actionId: string
    kind: ActionPromptDecision
  } | null>(null)
  const [ownedAgentActionPromptError, setOwnedAgentActionPromptError] =
    useState<ActionPromptError | null>(null)
  const latestActionPromptError = ownedAgentActionPromptError ?? controller.operatorActionPromptError
  const composerRuntimeRunActionError =
    controller.runtimeRunActionError ??
    (latestActionPromptError
      ? {
          code: 'agent_action_failed',
          message: latestActionPromptError.message,
          retryable: true,
        }
      : null)
  const composerRuntimeRunActionErrorTitle = controller.runtimeRunActionError
    ? controller.runtimeRunActionErrorTitle
    : latestActionPromptError
      ? 'Action failed'
      : controller.runtimeRunActionErrorTitle
  const actionPromptDispatchValue = useMemo<ActionPromptDispatchValue>(() => {
    const pendingOperatorIntent = pendingOwnedAgentActionIntent ?? controller.pendingOperatorIntent
    return {
      pendingActionId: pendingOperatorIntent?.actionId ?? null,
      pendingDecision: pendingOperatorIntent?.kind ?? null,
      isResolving: agent.operatorActionStatus === 'running' || pendingOwnedAgentActionIntent !== null,
      actionError: latestActionPromptError,
      resolveActionPrompt: async (actionId, decision, options) => {
        const runId = options?.runId?.trim() ?? ''
        const actionType = options?.actionType?.trim() ?? ''
        if (isOwnedAgentActionPrompt(runId, actionType)) {
          setPendingOwnedAgentActionIntent({ actionId, kind: decision })
          setOwnedAgentActionPromptError(null)
          const resolvedActionKey = ownedAgentActionPromptKey(runId, actionId)
          let optimisticPromptId: string | null = null
          try {
            const response = ownedAgentActionResponse(decision, options?.userAnswer ?? null)
            setOptimisticResolvedOwnedActionPrompts((current) => {
              if (current.has(resolvedActionKey)) {
                return current
              }
              const next = new Set(current)
              next.add(resolvedActionKey)
              return next
            })
            if (response) {
              optimisticPromptId = `owned-action:${resolvedActionKey}:${Date.now()}`
              setOptimisticPromptTurn({
                id: optimisticPromptId,
                text: response,
                queuedAt: new Date().toISOString(),
              })
            }
            if (decision === 'reject') {
              if (!desktopAdapter?.rejectAgentAction) {
                throw new Error('Xero cannot reject this owned-agent action in the current runtime.')
              }
              await desktopAdapter.rejectAgentAction(runId, actionId, { response })
              setOwnedAgentActionPromptError(null)
              return
            }
            if (onUpdateRuntimeRunControls) {
              await onUpdateRuntimeRunControls({
                prompt: response ?? 'Approved.',
                actionId,
              })
              setOwnedAgentActionPromptError(null)
              return
            }
            if (!desktopAdapter?.resumeAgentRun) {
              throw new Error('Xero cannot resume this owned-agent action in the current runtime.')
            }
            await desktopAdapter.resumeAgentRun(runId, response ?? 'Approved.', { actionId })
            setOwnedAgentActionPromptError(null)
          } catch (error) {
            setOwnedAgentActionPromptError({
              actionId,
              message: getRuntimeActionErrorMessage(
                error,
                'Xero could not resolve this owned-agent action.',
              ),
            })
            setOptimisticResolvedOwnedActionPrompts((current) => {
              if (!current.has(resolvedActionKey)) {
                return current
              }
              const next = new Set(current)
              next.delete(resolvedActionKey)
              return next
            })
            if (optimisticPromptId) {
              setOptimisticPromptTurn((current) =>
                current?.id === optimisticPromptId ? null : current,
              )
            }
          } finally {
            setPendingOwnedAgentActionIntent((current) =>
              current?.actionId === actionId && current.kind === decision ? null : current,
            )
          }
          return
        }
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
    desktopAdapter,
    onUpdateRuntimeRunControls,
    agent.operatorActionStatus,
    latestActionPromptError,
    pendingOwnedAgentActionIntent,
    renderableRuntimeRun,
  ])
  const isProviderLoggedIn = Boolean(
    selectedProviderReadyForSession ||
      runtimeSession?.isAuthenticated,
  )
  const showConversationLoadingState = Boolean(
    !showAgentSetupEmptyState &&
      !agentRuntimeBlocked &&
      isProviderLoggedIn &&
      !hasSessionActivity &&
      historicalConversationTurnsLoading,
  )
  const showEmptySessionState = Boolean(
    !showAgentSetupEmptyState &&
      !showConversationLoadingState &&
      !agentRuntimeBlocked &&
      isProviderLoggedIn &&
      !hasSessionActivity,
  )
  const hasConversationViewportContent = Boolean(
    !showAgentSetupEmptyState &&
      !showConversationLoadingState &&
      !showEmptySessionState &&
      hasSessionActivity,
  )
  const conversationSurfaceKey = [
    agent.project.id,
    selectedAgentSessionId ?? 'none',
    'conversation',
  ].join(':')
  const projectLabel =
    agent.project.repository?.displayName ?? agent.project.name ?? 'this project'
  const sessionLabel = agent.project.selectedAgentSession?.title?.trim() || 'New Chat'
  const scrollViewportRef = useRef<HTMLDivElement | null>(null)
  const bottomSentinelRef = useRef<HTMLDivElement | null>(null)
  const scrollToLatestFrameRef = useRef<number | null>(null)
  const scrollToFollowUpAnchorFrameRef = useRef<number | null>(null)
  const conversationMeasurementFrameRef = useRef<number | null>(null)
  const conversationMeasurementTimeoutRefs = useRef<number[]>([])
  const followUpAnchorPendingBehaviorRef = useRef<ScrollBehavior | null>(null)
  const followUpAnchorSpacerHeightRef = useRef(0)
  const shouldAutoFollowRef = useRef(true)
  const [showJumpToLatest, setShowJumpToLatest] = useState(false)
  const showJumpToLatestRef = useRef(false)
  const setConversationJumpToLatest = useCallback((nextValue: boolean) => {
    if (showJumpToLatestRef.current === nextValue) {
      return
    }

    showJumpToLatestRef.current = nextValue
    setShowJumpToLatest(nextValue)
  }, [])
  const [followUpAnchorTurnId, setFollowUpAnchorTurnId] = useState<string | null>(null)
  const [followUpAnchorSpacerHeight, setFollowUpAnchorSpacerHeightState] = useState(0)
  const setFollowUpAnchorSpacerHeight = useCallback((height: number) => {
    const nextHeight = Math.max(0, Math.ceil(height))
    followUpAnchorSpacerHeightRef.current = nextHeight
    setFollowUpAnchorSpacerHeightState((currentHeight) =>
      currentHeight === nextHeight ? currentHeight : nextHeight,
    )
  }, [])
  const clearFollowUpAnchor = useCallback(() => {
    followUpAnchorPendingBehaviorRef.current = null
    setFollowUpAnchorTurnId(null)
    setFollowUpAnchorSpacerHeight(0)
  }, [setFollowUpAnchorSpacerHeight])
  const conversationRunScrollKey = [
    agent.project.id,
    selectedAgentSessionId ?? 'none',
    renderableRuntimeRun?.runId ?? runtimeStream?.runId ?? 'no-run',
  ].join(':')
  const conversationSessionScrollKey = [
    agent.project.id,
    selectedAgentSessionId ?? 'none',
  ].join(':')
  const conversationRunScrollKeyRef = useRef<string | null>(null)
  const conversationSessionScrollKeyRef = useRef<string | null>(null)
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
  useLayoutEffect(() => {
    const sessionChanged = conversationSessionScrollKeyRef.current !== conversationSessionScrollKey
    if (
      conversationRunScrollKeyRef.current === conversationRunScrollKey &&
      !sessionChanged
    ) {
      return
    }

    conversationSessionScrollKeyRef.current = conversationSessionScrollKey
    conversationRunScrollKeyRef.current = conversationRunScrollKey
    if (sessionChanged) {
      clearFollowUpAnchor()
    }
    if (!sessionChanged && followUpAnchorTurnId) {
      shouldAutoFollowRef.current = false
      setConversationJumpToLatest(false)
      return
    }

    shouldAutoFollowRef.current = true
    setConversationJumpToLatest(false)
    const viewport = scrollViewportRef.current
    if (viewport) {
      viewport.scrollTop = viewport.scrollHeight
    }
  }, [
    clearFollowUpAnchor,
    conversationRunScrollKey,
    conversationSessionScrollKey,
    followUpAnchorTurnId,
    setConversationJumpToLatest,
  ])
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
  const getFollowUpAnchorPlan = useCallback((
    turnId: string,
  ): { viewport: HTMLElement; scrollTop: number; spacerHeight: number } | null => {
    const viewport = scrollViewportRef.current
    if (!viewport) {
      return null
    }

    const turn = findConversationTurnElement(viewport, turnId)
    if (!turn) {
      return null
    }

    const plan = getFollowUpAnchorScrollPlan({
      anchorTop: getElementTopInScrollViewport(viewport, turn),
      viewportHeight: viewport.clientHeight,
      scrollHeight: viewport.scrollHeight,
      currentSpacerHeight: followUpAnchorSpacerHeightRef.current,
    })

    return {
      viewport,
      ...plan,
    }
  }, [])
  const scrollToFollowUpAnchor = useCallback((
    turnId: string,
    behavior: ScrollBehavior = 'auto',
    options: { defer?: boolean } = {},
  ) => {
    const run = () => {
      const plan = getFollowUpAnchorPlan(turnId)
      if (!plan) return
      scrollViewportTo(plan.viewport, plan.scrollTop, behavior)
    }

    if (!options.defer || typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
      run()
      return
    }

    if (
      scrollToFollowUpAnchorFrameRef.current !== null &&
      typeof window.cancelAnimationFrame === 'function'
    ) {
      window.cancelAnimationFrame(scrollToFollowUpAnchorFrameRef.current)
    }
    scrollToFollowUpAnchorFrameRef.current = window.requestAnimationFrame(() => {
      scrollToFollowUpAnchorFrameRef.current = null
      run()
    })
  }, [getFollowUpAnchorPlan])
  const clearConversationMeasurementTimeouts = useCallback(() => {
    if (typeof window === 'undefined') {
      conversationMeasurementTimeoutRefs.current = []
      return
    }

    for (const timeoutId of conversationMeasurementTimeoutRefs.current) {
      window.clearTimeout(timeoutId)
    }
    conversationMeasurementTimeoutRefs.current = []
  }, [])
  useEffect(() => {
    return () => {
      clearConversationMeasurementTimeouts()
      if (
        scrollToLatestFrameRef.current !== null &&
        typeof window !== 'undefined' &&
        typeof window.cancelAnimationFrame === 'function'
      ) {
        window.cancelAnimationFrame(scrollToLatestFrameRef.current)
        scrollToLatestFrameRef.current = null
      }
      if (
        scrollToFollowUpAnchorFrameRef.current !== null &&
        typeof window !== 'undefined' &&
        typeof window.cancelAnimationFrame === 'function'
      ) {
        window.cancelAnimationFrame(scrollToFollowUpAnchorFrameRef.current)
        scrollToFollowUpAnchorFrameRef.current = null
      }
      if (
        conversationMeasurementFrameRef.current !== null &&
        typeof window !== 'undefined' &&
        typeof window.cancelAnimationFrame === 'function'
      ) {
        window.cancelAnimationFrame(conversationMeasurementFrameRef.current)
        conversationMeasurementFrameRef.current = null
      }
    }
  }, [clearConversationMeasurementTimeouts])
  const syncConversationScrollState = useCallback(() => {
    const viewport = scrollViewportRef.current
    if (!viewport) {
      return
    }

    const isNearBottom = isRuntimeConversationNearBottom(viewport)
    if (followUpAnchorTurnId) {
      if (isNearBottom && followUpAnchorSpacerHeightRef.current === 0) {
        clearFollowUpAnchor()
        shouldAutoFollowRef.current = true
        setConversationJumpToLatest(false)
        return
      }

      shouldAutoFollowRef.current = false
      setConversationJumpToLatest(hasConversationViewportContent && !isNearBottom)
      return
    }

    shouldAutoFollowRef.current = isNearBottom
    setConversationJumpToLatest(hasConversationViewportContent && !isNearBottom)
  }, [clearFollowUpAnchor, followUpAnchorTurnId, hasConversationViewportContent, setConversationJumpToLatest])
  const scheduleConversationScrollStateSync = useCallback(() => {
    if (typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
      syncConversationScrollState()
      return
    }

    if (
      conversationMeasurementFrameRef.current !== null &&
      typeof window.cancelAnimationFrame === 'function'
    ) {
      window.cancelAnimationFrame(conversationMeasurementFrameRef.current)
    }
    conversationMeasurementFrameRef.current = window.requestAnimationFrame(() => {
      conversationMeasurementFrameRef.current = null
      syncConversationScrollState()
    })
  }, [syncConversationScrollState])
  const scheduleConversationLayoutSettledSync = useCallback(() => {
    scheduleConversationScrollStateSync()
    clearConversationMeasurementTimeouts()

    if (typeof window === 'undefined' || typeof window.setTimeout !== 'function') {
      return
    }

    conversationMeasurementTimeoutRefs.current =
      CONVERSATION_LAYOUT_SETTLE_SYNC_DELAYS_MS.map((delayMs) =>
        window.setTimeout(() => {
          scheduleConversationScrollStateSync()
        }, delayMs),
      )
  }, [
    clearConversationMeasurementTimeouts,
    scheduleConversationScrollStateSync,
  ])
  const handleConversationScroll = useCallback(() => {
    syncConversationScrollState()
  }, [syncConversationScrollState])
  useLayoutEffect(() => {
    if (!hasConversationViewportContent) {
      return
    }

    const viewport = scrollViewportRef.current
    const content = bottomSentinelRef.current?.parentElement
    if (!viewport || !content) {
      return
    }

    const resizeObserver =
      typeof ResizeObserver === 'undefined'
        ? null
        : new ResizeObserver(() => {
            scheduleConversationLayoutSettledSync()
          })
    resizeObserver?.observe(viewport)
    resizeObserver?.observe(content)

    const mutationObserver =
      typeof MutationObserver === 'undefined'
        ? null
        : new MutationObserver(() => {
            scheduleConversationLayoutSettledSync()
          })
    mutationObserver?.observe(content, {
      childList: true,
      subtree: true,
    })
    scheduleConversationLayoutSettledSync()

    return () => {
      resizeObserver?.disconnect()
      mutationObserver?.disconnect()
      clearConversationMeasurementTimeouts()
      if (
        conversationMeasurementFrameRef.current !== null &&
        typeof window !== 'undefined' &&
        typeof window.cancelAnimationFrame === 'function'
      ) {
        window.cancelAnimationFrame(conversationMeasurementFrameRef.current)
        conversationMeasurementFrameRef.current = null
      }
    }
  }, [
    clearConversationMeasurementTimeouts,
    hasConversationViewportContent,
    scheduleConversationLayoutSettledSync,
  ])
  const pauseConversationAutoFollow = useCallback(() => {
    if (!hasConversationViewportContent) {
      return
    }

    shouldAutoFollowRef.current = false
    setConversationJumpToLatest(true)
  }, [hasConversationViewportContent, setConversationJumpToLatest])
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
        setConversationJumpToLatest(hasConversationViewportContent && !isNearBottom)
      }

      if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
        window.requestAnimationFrame(restore)
        return
      }

      restore()
    }
  }, [hasConversationViewportContent, setConversationJumpToLatest])
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
      if (followUpAnchorTurnId) {
        clearFollowUpAnchor()
      }
      pauseConversationAutoFollow()
    }
  }, [clearFollowUpAnchor, followUpAnchorTurnId, pauseConversationAutoFollow])
  const handleJumpToLatest = useCallback(() => {
    const hadFollowUpAnchor = followUpAnchorTurnId !== null
    shouldAutoFollowRef.current = true
    setConversationJumpToLatest(false)
    clearFollowUpAnchor()
    scrollToLatest('smooth', hadFollowUpAnchor ? { defer: true } : {})
  }, [clearFollowUpAnchor, followUpAnchorTurnId, scrollToLatest, setConversationJumpToLatest])
  const handleSubmitDraftPrompt = useCallback(() => {
    if (promptSubmissionPending) {
      return
    }

    const submittedText = controller.getDraftPromptWithHiddenContext().trim()
    const submittedAttachments = pendingComposerAttachmentsToConversation(pendingAttachmentsRef.current)
    const optimisticPrompt = submittedText.length > 0
      ? {
          id: `${Date.now()}:${submittedText}`,
          text: submittedText,
          queuedAt: new Date().toISOString(),
          attachments: submittedAttachments,
        }
      : null
    const shouldAnchorSubmittedPrompt = Boolean(
      optimisticPrompt &&
        hasConversationViewportContent &&
        hasUserMessage,
    )
    const followUpAnchorId = optimisticPrompt ? getPendingPromptTurnId(optimisticPrompt) : null

    if (optimisticPrompt) {
      setOptimisticPromptTurn(optimisticPrompt)
    }

    if (shouldAnchorSubmittedPrompt && followUpAnchorId) {
      shouldAutoFollowRef.current = false
      setConversationJumpToLatest(false)
      followUpAnchorPendingBehaviorRef.current = 'smooth'
      setFollowUpAnchorTurnId(followUpAnchorId)
    } else {
      clearFollowUpAnchor()
      shouldAutoFollowRef.current = true
      setConversationJumpToLatest(false)
      scrollToLatest('auto', { defer: true })
    }
    setPromptSubmissionPending(true)
    promptSubmissionCancelRef.current?.()
    let cancelled = false
    const cancelSubmission = () => {
      cancelled = true
    }
    promptSubmissionCancelRef.current = cancelSubmission
    void controller.handleSubmitDraftPrompt().then((submitted) => {
      if (!cancelled) {
        if (!submitted && optimisticPrompt) {
          setOptimisticPromptTurn((current) =>
            current?.id === optimisticPrompt.id ? null : current,
          )
          if (followUpAnchorId) {
            followUpAnchorPendingBehaviorRef.current = null
            setFollowUpAnchorTurnId((current) =>
              current === followUpAnchorId ? null : current,
            )
            setFollowUpAnchorSpacerHeight(0)
          }
        }
      }
    }).finally(() => {
      if (!cancelled) {
        setPromptSubmissionPending(false)
        if (shouldAnchorSubmittedPrompt && followUpAnchorId) {
          followUpAnchorPendingBehaviorRef.current ??= 'smooth'
        } else {
          scrollToLatest('auto', { defer: true })
        }
      }
      if (promptSubmissionCancelRef.current === cancelSubmission) {
        promptSubmissionCancelRef.current = null
      }
    })
  }, [
    clearFollowUpAnchor,
    controller,
    hasConversationViewportContent,
    hasUserMessage,
    promptSubmissionPending,
    scrollToLatest,
    setConversationJumpToLatest,
    setFollowUpAnchorSpacerHeight,
  ])

  useLayoutEffect(() => {
    if (!foregroundWorkReady || !followUpAnchorTurnId) {
      return
    }

    const queuedAnchorText = agent.selectedPrompt.hasQueuedPrompt
      ? agent.selectedPrompt.text?.trim()
      : null
    const anchorTurnIndex = findFollowUpAnchorTurnIndex(
      visibleTurnsWithPendingPrompt,
      followUpAnchorTurnId,
      queuedAnchorText,
    )
    if (anchorTurnIndex < 0) {
      if (!promptSubmissionPending && !agent.selectedPrompt.hasQueuedPrompt) {
        clearFollowUpAnchor()
      }
      return
    }

    if (
      shouldReleaseFollowUpAnchorForTurns({
        turns: visibleTurnsWithPendingPrompt,
        anchorTurnId: followUpAnchorTurnId,
        queuedAnchorText,
      })
    ) {
      clearFollowUpAnchor()
      shouldAutoFollowRef.current = true
      setConversationJumpToLatest(false)
      scrollToLatest('auto', { defer: true })
      return
    }

    const plan = getFollowUpAnchorPlan(followUpAnchorTurnId)
    if (!plan) {
      return
    }

    shouldAutoFollowRef.current = false
    if (plan.spacerHeight !== followUpAnchorSpacerHeight) {
      setFollowUpAnchorSpacerHeight(plan.spacerHeight)
      return
    }

    const behavior = followUpAnchorPendingBehaviorRef.current
    if (!behavior) {
      return
    }

    followUpAnchorPendingBehaviorRef.current = null
    scrollToFollowUpAnchor(followUpAnchorTurnId, behavior, { defer: true })
  }, [
    agent.selectedPrompt.hasQueuedPrompt,
    clearFollowUpAnchor,
    conversationScrollKey,
    followUpAnchorTurnId,
    followUpAnchorSpacerHeight,
    foregroundWorkReady,
    getFollowUpAnchorPlan,
    promptSubmissionPending,
    scrollToLatest,
    scrollToFollowUpAnchor,
    setConversationJumpToLatest,
    setFollowUpAnchorSpacerHeight,
    visibleTurnsWithPendingPrompt,
  ])

  useEffect(() => {
    if (!foregroundWorkReady) {
      return
    }

    if (!hasConversationViewportContent) {
      shouldAutoFollowRef.current = true
      setConversationJumpToLatest(false)
      return
    }

    if (followUpAnchorTurnId) {
      shouldAutoFollowRef.current = false
      const viewport = scrollViewportRef.current
      const isNearBottom = viewport ? isRuntimeConversationNearBottom(viewport) : false
      setConversationJumpToLatest(hasConversationViewportContent && !isNearBottom)
      return
    }

    if (shouldAutoFollowRef.current) {
      scrollToLatest('auto', { defer: true })
      setConversationJumpToLatest(false)
      return
    }

    const viewport = scrollViewportRef.current
    const isNearBottom = viewport ? isRuntimeConversationNearBottom(viewport) : false
    setConversationJumpToLatest(hasConversationViewportContent && !isNearBottom)
  }, [
    conversationScrollKey,
    followUpAnchorTurnId,
    foregroundWorkReady,
    hasConversationViewportContent,
    scrollToLatest,
    setConversationJumpToLatest,
  ])

  const isCompact = effectiveDensity === 'compact'
  const isDense = isCompact || paneCount >= 4 || useBackgroundPaneFastPath
  const showPaneNumberChip = paneCount > 1 && paneNumber != null
  const showCloseButton = paneCount > 1 && typeof onClosePane === 'function'
  const useCompactHeaderChrome = isCompactWidth
  const showIconOnlyNewSessionButton = useCompactHeaderChrome
  const isStopComposerMode = Boolean(
    controller.canStopRuntimeRun &&
      renderableRuntimeRun?.isActive &&
      !renderableRuntimeRun.isTerminal &&
      hasLiveRuntimeStream,
  )
  const isStoppingRuntimeRun = runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'stop'
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
      enabled={Boolean(stageAgentAttachment || stageAgentAttachmentPath)}
      onFilesDropped={handleAddFiles}
      onPathsDropped={stageAgentAttachmentPath ? handleDroppedPaths : undefined}
    >
      <div ref={paneRootRef} className="flex min-h-0 min-w-0 flex-1">
        <div className="relative flex min-w-0 flex-1 flex-col">
          <div className="pointer-events-none absolute inset-x-0 top-0 z-20">
            {isComputerUseSidebar ? (
              <ComputerUseSidebarHeader
                label={sessionLabel}
                clearDisabled={sidebarChatClearDisabled}
                clearLabel={sidebarChatClearLabel}
                clearPending={sidebarChatClearPending}
                clearTitle={sidebarChatClearTitle}
                onClear={onClearSidebarChat}
                closeLabel="Close Computer Use"
                onClose={onCloseSidebar}
                className="pointer-events-auto"
              />
            ) : (
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
                  {useCompactHeaderChrome ? null : (
                    <>
                      <span className="truncate font-semibold text-foreground">{projectLabel}</span>
                      <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground/70" />
                    </>
                  )}
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
                          {isComputerUseSession ? (
                            <span className="inline-flex h-[18px] shrink-0 items-center gap-1 rounded-sm border border-primary/20 bg-primary/10 px-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-primary">
                              <Monitor className="h-3 w-3" />
                              Computer
                            </span>
                          ) : null}
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
                              {session.isComputerUse ? (
                                <Monitor className="h-3.5 w-3.5 text-primary" />
                              ) : null}
                            </DropdownMenuItem>
                          )
                        })}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  ) : (
                    <span className="inline-flex min-w-0 items-center gap-1">
                      <span className="truncate font-medium">{sessionLabel}</span>
                      {isComputerUseSession ? (
                        <span className="inline-flex h-[18px] shrink-0 items-center gap-1 rounded-sm border border-primary/20 bg-primary/10 px-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-primary">
                          <Monitor className="h-3 w-3" />
                          Computer
                        </span>
                      ) : null}
                    </span>
                  )}
                </div>
                <div className="pointer-events-auto flex items-center gap-1">
                  {onCreateSession && paneCount === 1 ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size={showIconOnlyNewSessionButton ? 'icon-sm' : 'sm'}
                      aria-label="New session"
                      title={showIconOnlyNewSessionButton ? 'New session' : undefined}
                      onClick={onCreateSession}
                      disabled={isCreatingSession}
                      className={cn(
                        'h-[30px] rounded-md text-[12.5px] font-semibold text-muted-foreground',
                        'hover:bg-primary/10 hover:text-primary',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                        showIconOnlyNewSessionButton ? 'w-[30px] p-0' : 'gap-1.5 px-2',
                      )}
                    >
                      {isCreatingSession ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Plus className="h-3.5 w-3.5" />
                      )}
                      {showIconOnlyNewSessionButton ? null : (
                        <span>{inSidebar ? 'New' : 'New Session'}</span>
                      )}
                    </Button>
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
            )}
          <div
            aria-hidden="true"
            className={cn(
              'bg-gradient-to-b',
              isDense || isComputerUseSidebar ? 'h-2' : 'h-7',
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
              showAgentSetupEmptyState || showConversationLoadingState || showEmptySessionState
                ? 'flex h-full items-center justify-center overflow-y-auto scrollbar-thin'
                : 'flex h-full overflow-y-auto scrollbar-thin',
              isDense
                ? showAgentSetupEmptyState || showConversationLoadingState || showEmptySessionState
                  ? 'px-2 py-2'
                  : isComputerUseSidebar
                    ? 'px-2 pt-12'
                    : 'px-2 pt-12'
                : showAgentSetupEmptyState || showConversationLoadingState || showEmptySessionState
                  ? 'px-6 py-5'
                  : isComputerUseSidebar
                    ? 'px-4 pt-14'
                    : 'px-4 pt-20',
            )}
          >
            {showAgentSetupEmptyState ? (
              <SetupEmptyState onOpenSettings={onOpenSettings} />
            ) : showConversationLoadingState ? (
              <ConversationLoadingState
                context={isComputerUseSession ? 'computer-use' : 'default'}
              />
            ) : showEmptySessionState ? (
              <EmptySessionState
                context={
                  isComputerUseSession
                    ? 'computer-use'
                    : controller.composerRuntimeAgentId === 'agent_create'
                      ? 'agent-create'
                      : 'default'
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
                key={conversationSurfaceKey}
                className={cn(
                  'agent-session-surface-enter mx-auto flex w-full flex-col',
                  isDense ? 'max-w-full gap-1' : 'max-w-[720px] gap-4',
                )}
              >
                <ActionPromptDispatchProvider value={actionPromptDispatchValue}>
                  <RoutingSuggestionDispatchProvider value={routingSuggestionDispatchValue}>
                    <ConversationSection
                      runtimeRun={renderableRuntimeRun}
                      visibleTurns={applyRoutingResolutions(visibleTurnsWithOwnedActionPromptState)}
                      streamIssue={streamIssue}
                      streamFailure={runtimeStream?.failure ?? null}
                      showActivityIndicator={showAgentActivityIndicator}
                      streamCompletion={runtimeStream?.completion ?? null}
                      accountAvatarUrl={accountAvatarUrl}
                      accountLogin={accountLogin}
                      currentAgentLabel={currentAgentLabelForRouting}
                      variant={isDense ? 'dense' : 'default'}
                      codeUndoStates={codeUndoStates}
                      returnSessionToHereStates={returnSessionToHereStates}
                      onUndoChangeGroup={
                        desktopAdapter?.applySelectiveUndo ? handleUndoCodeChange : undefined
                      }
                      onReturnSessionToHere={
                        desktopAdapter?.returnSessionToHere ? handleReturnSessionToHere : undefined
                      }
                      onOpenHandoffSummary={
                        getAgentHandoffContextSummaryFn ? handleOpenHandoffSummary : undefined
                      }
                      footerHandoffSourceRunId={footerHandoffSourceRunId}
                    />
                  </RoutingSuggestionDispatchProvider>
                </ActionPromptDispatchProvider>
                {controller.composerRuntimeAgentId === 'agent_create' ? (
                  <AgentCreateDraftSection
                    runtimeStreamItems={runtimeStreamItems}
                    pendingApprovalCount={agent.pendingApprovalCount ?? 0}
                    onOpenAgentManagement={onOpenAgentManagement}
                    onCreateAgentByHand={onCreateAgentByHand}
                  />
                ) : null}
                {followUpAnchorSpacerHeight > 0 ? (
                  <div
                    aria-hidden="true"
                    data-conversation-follow-up-spacer="true"
                    className="shrink-0"
                    style={{ height: followUpAnchorSpacerHeight }}
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

        <PlanTray plan={runtimeStream?.plan ?? null} density={effectiveDensity} />

        <ComposerDock
          density={effectiveDensity}
          inSidebar={inSidebar}
          composerRuntimeAgentId={controller.composerRuntimeAgentId}
          composerRuntimeAgentLabel={getRuntimeAgentLabel(controller.composerRuntimeAgentId)}
          availableRuntimeAgentIds={availableRuntimeAgentIds}
          hideAgentSelector={isComputerUseSession}
          hideAutoCompact={isComputerUseSession}
          hideContextMeter={isComputerUseSession}
          hideDictation={isComputerUseSession}
          runtimeAgentLockReason={
            isComputerUseSession
              ? 'Computer Use sessions always run with the Computer Use agent.'
              : undefined
          }
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
          isStopVisible={isStopComposerMode}
          isStopDisabled={isStoppingRuntimeRun}
          onStopRuntimeRun={() => void controller.handleStopRuntimeRun()}
          onComposerApprovalModeChange={controller.handleComposerApprovalModeChange}
          onComposerRuntimeAgentChange={controller.handleComposerRuntimeAgentChange}
          onComposerAgentSelectionChange={controller.handleComposerAgentSelectionChange}
          onAutoCompactEnabledChange={controller.handleAutoCompactEnabledChange}
          onComposerModelChange={handleComposerModelChangeWithCreditDismiss}
          onComposerThinkingLevelChange={controller.handleComposerThinkingLevelChange}
          onDraftPromptChange={controller.handleDraftPromptChange}
          onSubmitDraftPrompt={handleSubmitDraftPrompt}
          pendingAttachments={pendingAttachments}
          pendingContexts={pendingContextCards}
          attachmentCompatibility={selectedComposerModel}
          onAddFiles={handleAddFiles}
          onAddFolders={pickComposerFolders ? handlePickComposerFolders : undefined}
          onRemoveAttachment={handleRemoveAttachment}
          onRemoveContext={handleRemoveContextCard}
          contextMentionOptions={composerContextMentionOptions}
          contextMentionStatus={composerPathIndexState.status}
          contextMentionError={composerPathIndexState.error}
          onContextMentionQueryChange={listProjectFileIndex ? setComposerContextMentionQuery : undefined}
          onSelectContextMention={listProjectFileIndex ? handleSelectComposerContextMention : undefined}
          pendingRuntimeRunAction={pendingRuntimeRunAction}
          placeholder={composerPlaceholder}
          promptInputRef={controller.promptInputRef}
          promptInputLabel={promptInputLabel}
          runtimeSessionBindInFlight={controller.runtimeSessionBindInFlight}
          runtimeRunActionError={composerRuntimeRunActionError}
          runtimeRunActionErrorTitle={composerRuntimeRunActionErrorTitle}
          creditLimitNotice={creditLimitNotice}
          runtimeRunActionStatus={runtimeRunActionStatus}
          sendButtonLabel={sendButtonLabel}
          onOpenDiagnostics={onOpenDiagnostics}
        />
        </div>
      </div>
      <HandoffContextDialog
        open={handoffSummaryOpen}
        onOpenChange={setHandoffSummaryOpen}
        status={handoffSummaryStatus}
        errorMessage={handoffSummaryError}
        summary={handoffSummary}
        onRefresh={refreshHandoffSummary}
      />
    </AgentPaneDropOverlay>
  )
})
