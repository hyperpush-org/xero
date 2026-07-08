/**
 * Agent conversation panel.
 *
 * Renders user / assistant transcripts as polished message rows with
 * avatars, role labels, and (for assistant content) markdown + code
 * highlighting. Tool/action items render as inline activity rows and
 * failure notices get severity-aware treatment.
 *
 * The component preserves the public ARIA surface (`Agent conversation`
 * landmark, `Agent conversation turns` list) so existing tests keep
 * passing.
 */

import {
  AlertCircle,
  AlertTriangle,
  Bot,
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  Circle,
  Copy,
  FileText,
  FolderOpen,
  GitBranch,
  History,
  ImageIcon,
  Info,
  Loader2,
  MoreHorizontal,
  MousePointer2,
  PencilLine,
  Terminal,
  Undo2,
  User,
  XCircle,
} from 'lucide-react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { memo, useCallback, useEffect, useMemo, useState } from 'react'

import { cn } from '../../lib/utils'
import type {
  CodeHistoryConflictDto,
  CodePatchAvailabilityDto,
  CodePatchTextHunkDto,
  RuntimeActionAnswerShapeDto,
  RuntimeActionRequiredOptionDto,
  RuntimeSensitiveInputFieldDto,
  RuntimeAgentIdDto,
  RuntimeRunView,
  RuntimeStreamCompleteItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamIssueView,
  RuntimeStreamMediaSourceDto,
  RuntimeStreamToolItemView,
} from '../../model'
import { getRuntimeAgentLabel } from '../../model'
import { AppLogo } from '../app-logo'
import { Badge } from '../ui/badge'
import { Button } from '../ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '../ui/collapsible'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip'
import { ActionPromptCard } from './action-prompt-card'
import { Markdown } from './conversation-markdown'
import {
  AttachmentPreviewChip,
  ImageAttachmentPreview,
  ToolMediaAttachments,
  attachmentDisplayName,
} from './media-attachment-preview'
import { RoutingSuggestionCard } from './routing-suggestion-card'

export interface ConversationMessageAttachment {
  id: string
  kind: 'image' | 'document' | 'text'
  mediaType: string
  originalName: string
  sizeBytes: number
  title?: string | null
  alt?: string | null
  width?: number | null
  height?: number | null
  source?: RuntimeStreamMediaSourceDto
  renderUrl?: string | null
  /** Webview-renderable URL (e.g. via convertFileSrc) for image previews. */
  previewSrc?: string
  /** Absolute path on disk; used for click-to-open behaviour. */
  absolutePath?: string
}

export type ConversationTurn =
  | {
      id: string
      kind: 'message'
      role: 'user' | 'assistant'
      sequence: number
      text: string
      attachments?: ConversationMessageAttachment[]
    }
  | {
      id: string
      kind: 'thinking'
      sequence: number
      text: string
    }
  | {
      id: string
      kind: 'action'
      sequence: number
      toolCallId: string
      toolName: string
      title: string
      detail: string
      detailRows: Array<{ label: string; value: string }>
      mediaAttachments?: ConversationMessageAttachment[]
      state?: RuntimeStreamToolItemView['toolState'] | null
      defaultOpen?: boolean
    }
  | {
      id: string
      kind: 'action_group'
      sequence: number
      title: string
      detail: string
      state?: RuntimeStreamToolItemView['toolState'] | null
      actions: Array<{
        id: string
        sequence: number
        toolCallId: string
        toolName: string
        title: string
        detail: string
        detailRows: Array<{ label: string; value: string }>
        mediaAttachments?: ConversationMessageAttachment[]
        state: RuntimeStreamToolItemView['toolState'] | null
        defaultOpen?: boolean
      }>
    }
  | {
      id: string
      kind: 'file_change'
      runId: string
      sequence: number
      title: string
      detail: string
      operation: string
      path: string
      toPath: string | null
      changeGroupId: string | null
      workspaceEpoch: number | null
      patchAvailability: CodePatchAvailabilityDto | null
    }
  | {
      id: string
      kind: 'failure'
      sequence: number
      message: string
      code: string
    }
  | {
      id: string
      kind: 'action_prompt'
      sequence: number
      actionId: string
      runId?: string | null
      actionType: string
      title: string
      detail: string
      shape: RuntimeActionAnswerShapeDto
      options: RuntimeActionRequiredOptionDto[] | null
      allowMultiple: boolean
      sensitiveFields?: RuntimeSensitiveInputFieldDto[] | null
      intendedUse?: string | null
      pendingDecision: 'approve' | 'reject' | 'resume' | null
      isResolved: boolean
    }
  | {
      id: string
      kind: 'handoff_notice'
      sequence: number
      sourceRunId: string
      targetRunId: string
    }
  | {
      id: string
      kind: 'routing_suggestion'
      sequence: number
      targetKind: 'built_in' | 'custom'
      targetAgentId: RuntimeAgentIdDto
      targetAgentDefinitionId: string | null
      targetAgentDefinitionVersion: number | null
      targetLabel: string | null
      reason: string
      summary: string
      isResolved: boolean
      acceptedTarget: RuntimeAgentIdDto | null
      acceptedTargetAgentDefinitionId: string | null
      acceptedTargetLabel: string | null
      routingResolutionMode?: 'manual' | 'automatic' | null
    }
  | {
      id: string
      kind: 'subagent_group'
      sequence: number
      subagentId: string
      role: string | null
      roleLabel: string
      status: string
      runId: string | null
      prompt: string | null
      usedToolCalls: number | null
      maxToolCalls: number | null
      usedTokens: number | null
      maxTokens: number | null
      resultSummary: string | null
      startedAt: string | null
      completedAt: string | null
      children: ConversationTurn[]
    }

export type CodeUndoTargetKind = 'change_group' | 'file_change' | 'hunks'

export interface CodeUndoRequest {
  targetKind: CodeUndoTargetKind
  changeGroupId: string
  path: string
  filePath?: string | null
  hunkIds?: string[]
  expectedWorkspaceEpoch?: number | null
}

export interface CodeUndoUiState {
  status: 'pending' | 'succeeded' | 'failed'
  message: string
  conflictSummary?: CodeUndoConflictSummary
}

export interface CodeUndoConflictSummary {
  title: string
  targetLabel: string
  affectedPaths: string[]
  conflicts: CodeHistoryConflictDto[]
}

export type ReturnSessionToHereTargetKind = 'session_boundary' | 'run_boundary'

export interface ReturnSessionToHereUiRequest {
  targetKind: ReturnSessionToHereTargetKind
  targetId: string
  boundaryId: string
  runId?: string | null
  changeGroupId?: string | null
  expectedWorkspaceEpoch?: number | null
}

export interface ConversationSectionProps {
  runtimeRun: RuntimeRunView | null
  visibleTurns: ConversationTurn[]
  streamIssue: RuntimeStreamIssueView | null
  streamFailure: RuntimeStreamFailureItemView | null
  showActivityIndicator?: boolean
  /** Stream completion item, used to surface same-type handoff continuations. */
  streamCompletion?: RuntimeStreamCompleteItemView | null
  /** GitHub avatar URL for the signed-in user, when available. */
  accountAvatarUrl?: string | null
  /** GitHub login for the signed-in user, used as alt text. */
  accountLogin?: string | null
  /** Active run agent label used for routing-decline actions. */
  currentAgentLabel?: string | null
  /** Visual density. `dense` collapses each turn into a single PTY-style line. */
  variant?: 'default' | 'dense'
  codeUndoStates?: Record<string, CodeUndoUiState>
  returnSessionToHereStates?: Record<string, CodeUndoUiState>
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  /**
   * When provided, the inline same-session handoff notice rows and the footer
   * handoff completion notice become clickable and open the handoff context
   * summary dialog.
   */
  onOpenHandoffSummary?: (request: {
    sourceRunId?: string | null
    targetRunId?: string | null
  }) => void
  /**
   * Source run id for the footer "Run continued in a fresh session" notice.
   * The footer notice only appears while the pane is still subscribed to the
   * source run's stream (before the rebind), so this is the source run id.
   */
  footerHandoffSourceRunId?: string | null
}

export function getCodeUndoStateKey({
  targetKind,
  changeGroupId,
  filePath,
}: {
  targetKind: CodeUndoTargetKind
  changeGroupId: string
  filePath?: string | null
}): string {
  if (targetKind === 'file_change') {
    return `${changeGroupId}:file:${filePath ?? ''}`
  }

  if (targetKind === 'hunks') {
    return `${changeGroupId}:file:${filePath ?? ''}:hunks`
  }

  return changeGroupId
}

export function getReturnSessionToHereStateKey({
  targetKind,
  boundaryId,
  runId,
  changeGroupId,
}: {
  targetKind: ReturnSessionToHereTargetKind
  boundaryId: string
  runId?: string | null
  changeGroupId?: string | null
}): string {
  return [
    'return-session',
    targetKind,
    runId ?? '',
    boundaryId,
    changeGroupId ?? '',
  ].join(':')
}

/**
 * Stable detail-text marker emitted by the runtime when a same-type handoff
 * completes a source run. The runtime emits the matching summary in
 * `mark_source_run_handed_off` (see `runtime/agent_core/run.rs`); changing the
 * sentence on either side requires updating both for the user-facing notice.
 */
const HANDOFF_COMPLETION_DETAIL_MARKER = 'handed off to a same-type target run'

const BROWSER_TOOL_CONTEXT_HEADING_PATTERN =
  String.raw`Browser (?:sketch context|element inspection context)(?:\s*\(capture\s+\d+\))?:`
const LINKED_CONTEXT_HEADING_PATTERN =
  String.raw`Linked (?:project (?:file|folder)|folder) context:`
const PROMPT_CONTEXT_HEADING_PATTERN =
  String.raw`(?:${BROWSER_TOOL_CONTEXT_HEADING_PATTERN}|${LINKED_CONTEXT_HEADING_PATTERN})`
const BROWSER_TOOL_CONTEXT_MARKER_PATTERN =
  /^Browser (sketch context|element inspection context)(?:\s*\(capture\s+\d+\))?:$/
const LINKED_CONTEXT_MARKER_PATTERN =
  /^Linked (?:(project) )?(file|folder) context:$/
const PROMPT_CONTEXT_BLOCK_PATTERN = new RegExp(
  String.raw`(^|\n{2,})(${PROMPT_CONTEXT_HEADING_PATTERN}\n[\s\S]*?)(?=\n{2,}${PROMPT_CONTEXT_HEADING_PATTERN}\n|$)`,
  'g',
)

type PromptContextKind = 'sketch' | 'element' | 'file' | 'folder'

export interface BrowserToolPromptContext {
  id: string
  kind: PromptContextKind
  title: string
  subtitle: string | null
  page: string | null
  lines: string[]
  rawText: string
}

export interface BrowserToolPromptParts {
  visibleText: string
  contexts: BrowserToolPromptContext[]
}

interface BrowserToolContextAttachmentMapping {
  pairedByContextId: Map<string, ConversationMessageAttachment>
  unpairedAttachments: ConversationMessageAttachment[] | undefined
}

function pairBrowserToolContextAttachments(
  contexts: readonly BrowserToolPromptContext[],
  attachments: readonly ConversationMessageAttachment[] | null | undefined,
): BrowserToolContextAttachmentMapping {
  if (!attachments?.length || contexts.every((context) => context.kind !== 'sketch')) {
    return {
      pairedByContextId: new Map(),
      unpairedAttachments: attachments?.slice(),
    }
  }

  const imageAttachments = attachments.filter((attachment) => attachment.kind === 'image')
  const pairedByContextId = new Map<string, ConversationMessageAttachment>()
  const pairedAttachmentIds = new Set<string>()
  let imageIndex = 0

  for (const context of contexts) {
    if (context.kind !== 'sketch') continue
    const attachment = imageAttachments[imageIndex]
    imageIndex += 1
    if (!attachment) continue
    pairedByContextId.set(context.id, attachment)
    pairedAttachmentIds.add(attachment.id)
  }

  return {
    pairedByContextId,
    unpairedAttachments: attachments.filter((attachment) => !pairedAttachmentIds.has(attachment.id)),
  }
}

function isHandoffCompletion(
  completion: RuntimeStreamCompleteItemView | null | undefined,
): boolean {
  return Boolean(
    completion?.detail
      ?.toLowerCase()
      .includes(HANDOFF_COMPLETION_DETAIL_MARKER),
  )
}

export function splitBrowserToolPromptContext(text: string): BrowserToolPromptParts {
  const contexts: BrowserToolPromptContext[] = []
  const visibleParts: string[] = []
  let cursor = 0

  PROMPT_CONTEXT_BLOCK_PATTERN.lastIndex = 0
  let match = PROMPT_CONTEXT_BLOCK_PATTERN.exec(text)
  while (match !== null) {
    visibleParts.push(text.slice(cursor, match.index))

    const separator = match[1] ?? ''
    const rawText = (match[2] ?? '').trim()
    const context = parsePromptContext(rawText, contexts.length)
    if (context) {
      contexts.push(context)
      cursor = match.index + separator.length + (match[2] ?? '').length
    } else {
      visibleParts.push(separator, match[2] ?? '')
      cursor = match.index + separator.length + (match[2] ?? '').length
    }

    match = PROMPT_CONTEXT_BLOCK_PATTERN.exec(text)
  }

  visibleParts.push(text.slice(cursor))

  return {
    visibleText: visibleParts.join('').trim(),
    contexts,
  }
}

export function userVisiblePromptText(text: string): string {
  return splitBrowserToolPromptContext(text).visibleText
}

function parsePromptContext(
  rawText: string,
  index: number,
): BrowserToolPromptContext | null {
  const lines = rawText.split(/\r?\n/)
  const marker = lines[0]?.trim() ?? ''
  const markerMatch = BROWSER_TOOL_CONTEXT_MARKER_PATTERN.exec(marker)
  if (markerMatch) {
    return parseBrowserToolPromptContext(rawText, lines, markerMatch, index)
  }

  const linkedMarkerMatch = LINKED_CONTEXT_MARKER_PATTERN.exec(marker)
  if (linkedMarkerMatch) {
    return parseLinkedPromptContext(rawText, lines, linkedMarkerMatch, index)
  }

  return null
}

function parseBrowserToolPromptContext(
  rawText: string,
  lines: string[],
  markerMatch: RegExpExecArray,
  index: number,
): BrowserToolPromptContext {
  const kind = markerMatch[1] === 'sketch context' ? 'sketch' : 'element'
  const pageLine = lines.find((line) => line.trim().startsWith('Page: '))
  const page = pageLine ? pageLine.trim().replace(/^Page:\s*/, '') : null
  const bodyLines = lines
    .slice(1)
    .map((line) => line.trim())
    .filter((line) => line.length > 0)

  return {
    id: `${kind}-${index}`,
    kind,
    title: kind === 'sketch' ? 'Browser sketch context' : 'Element context',
    subtitle: browserPromptContextSubtitle(kind, page, bodyLines),
    page,
    lines: bodyLines,
    rawText,
  }
}

function parseLinkedPromptContext(
  rawText: string,
  lines: string[],
  markerMatch: RegExpExecArray,
  index: number,
): BrowserToolPromptContext {
  const kind: Extract<PromptContextKind, 'file' | 'folder'> =
    markerMatch[2] === 'file' ? 'file' : 'folder'
  const bodyLines = lines
    .slice(1)
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
  const pathText = bodyLines.find((line) => line.startsWith('- '))?.replace(/^-\s*/, '').trim() ?? ''
  const linkedPath = parseLinkedContextPath(pathText)
  const titlePath = linkedPath.displayPath || linkedPath.absolutePath || (kind === 'file' ? 'Attached file' : 'Attached folder')
  const title = pathBaseName(titlePath)
  const sourceLabel = markerMatch[1] === 'project' ? 'Project' : 'Linked'

  return {
    id: `${sourceLabel.toLowerCase()}-${kind}-${index}`,
    kind,
    title,
    subtitle: compactPromptContextText(linkedPath.absolutePath || linkedPath.displayPath || sourceLabel),
    page: null,
    lines: bodyLines,
    rawText,
  }
}

function parseLinkedContextPath(value: string): {
  displayPath: string | null
  absolutePath: string | null
} {
  const normalized = value.trim()
  if (!normalized) {
    return { displayPath: null, absolutePath: null }
  }

  const pathWithAbsoluteMatch = /^(.*?)\s+\((.*)\)$/.exec(normalized)
  if (pathWithAbsoluteMatch?.[1]?.trim() && pathWithAbsoluteMatch[2]?.trim()) {
    return {
      displayPath: pathWithAbsoluteMatch[1].trim(),
      absolutePath: pathWithAbsoluteMatch[2].trim(),
    }
  }

  return {
    displayPath: normalized,
    absolutePath: null,
  }
}

function pathBaseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean)
  return parts.at(-1) ?? path
}

function compactPromptContextText(value: string | null | undefined, maxLength = 72): string | null {
  const normalized = value?.replace(/\s+/g, ' ').trim()
  if (!normalized) return null
  return normalized.length <= maxLength
    ? normalized
    : `${normalized.slice(0, Math.max(0, maxLength - 3)).trimEnd()}...`
}

function browserPromptContextSubtitle(
  kind: Extract<PromptContextKind, 'sketch' | 'element'>,
  page: string | null,
  lines: readonly string[],
): string | null {
  if (kind === 'sketch') {
    const drawingLine = lines.find((line) => line.startsWith('Drawing: '))
    if (drawingLine) return compactPromptContextText(drawingLine.replace(/^Drawing:\s*/, ''))
    const attachedImageLine = lines.find((line) => line.startsWith('Attached image: '))
    if (attachedImageLine) return compactPromptContextText(attachedImageLine.replace(/^Attached image:\s*/, ''))
  }

  const sourceLine = lines.find((line) => line.startsWith('- Source: '))
  if (sourceLine) {
    const source = sourceLine.replace(/^- Source:\s*/, '').split('|')[0]?.trim() ?? ''
    if (source) return compactPromptContextText(pathBaseName(source))
  }

  return compactPromptContextText(page) ?? `${lines.length} detail${lines.length === 1 ? '' : 's'}`
}

function visibleConversationCopyText(
  turns: readonly ConversationTurn[],
  currentAgentLabel?: string | null,
): string {
  return turns
    .flatMap((turn) => conversationTurnCopySections(turn, currentAgentLabel))
    .map((section) => section.trim())
    .filter(Boolean)
    .join('\n\n')
}

function conversationTurnCopySections(
  turn: ConversationTurn,
  currentAgentLabel?: string | null,
): string[] {
  switch (turn.kind) {
    case 'message': {
      if (turn.role === 'user') {
        const parsed = splitBrowserToolPromptContext(turn.text)
        const sections: string[] = []
        if (parsed.visibleText.trim().length > 0) {
          sections.push(`You:\n${parsed.visibleText.trim()}`)
        }
        for (const context of parsed.contexts) {
          sections.push(`${context.title}:\n${context.rawText}`)
        }
        return sections
      }

      return splitAssistantText(turn.text).flatMap((segment) => {
        const text = segment.text.trim()
        if (text.length === 0) return []
        return segment.kind === 'thinking' ? [`Thoughts:\n${text}`] : [`Agent:\n${text}`]
      })
    }
    case 'thinking':
      return turn.text.trim().length > 0 ? [`Thoughts:\n${turn.text.trim()}`] : []
    case 'action':
      return [formatCopyTitleAndDetail(turn.title, turn.detail)]
    case 'action_group':
      return [formatCopyTitleAndDetail(turn.title, turn.detail)]
    case 'file_change':
      return [formatCopyTitleAndDetail(turn.title, turn.detail)]
    case 'failure':
      return [formatCopyTitleAndDetail('Failure', turn.message)]
    case 'action_prompt':
      return [formatCopyTitleAndDetail(turn.title, turn.detail)]
    case 'handoff_notice':
      return ['Run continued in a fresh session']
    case 'routing_suggestion': {
      const label = turn.targetLabel ?? turn.targetAgentDefinitionId ?? getRuntimeAgentLabel(turn.targetAgentId)
      const targetDescription = turn.targetKind === 'custom' ? label : `the ${label} agent`
      const visibleLines = [`This task may be better suited for ${targetDescription}.`]
      const resolvedTargetLabel =
        turn.acceptedTargetLabel?.trim() ||
        (turn.acceptedTargetAgentDefinitionId ? 'custom agent' : null) ||
        (turn.acceptedTarget ? getRuntimeAgentLabel(turn.acceptedTarget) : null)

      if (turn.isResolved) {
        const displayCurrentAgentLabel = currentAgentLabel?.trim() || 'current agent'
        visibleLines.push(
          turn.acceptedTarget
            ? `${turn.routingResolutionMode === 'automatic' ? 'Auto-switched' : 'Switched'} to ${
                resolvedTargetLabel ?? getRuntimeAgentLabel(turn.acceptedTarget)
              } and continued.`
            : `Continued with ${displayCurrentAgentLabel}.`,
        )
      }

      return [`Routing suggestion:\n${visibleLines.join('\n')}`]
    }
    case 'subagent_group':
      return [formatCopyTitleAndDetail(turn.roleLabel, turn.resultSummary ?? turn.prompt ?? turn.status)]
    default:
      return []
  }
}

function formatCopyTitleAndDetail(title: string, detail: string | null | undefined): string {
  const normalizedTitle = title.trim()
  const normalizedDetail = detail?.trim() ?? ''
  if (!normalizedTitle) return normalizedDetail
  if (!normalizedDetail) return normalizedTitle
  return `${normalizedTitle}:\n${normalizedDetail}`
}

export const ConversationSection = memo(function ConversationSection({
  runtimeRun,
  visibleTurns,
  streamIssue,
  streamFailure,
  showActivityIndicator = false,
  streamCompletion = null,
  accountAvatarUrl = null,
  accountLogin = null,
  currentAgentLabel = null,
  variant = 'default',
  codeUndoStates = {},
  returnSessionToHereStates = {},
  onUndoChangeGroup,
  onReturnSessionToHere,
  onOpenHandoffSummary,
  footerHandoffSourceRunId = null,
}: ConversationSectionProps) {
  const runFailureCode =
    runtimeRun?.lastError?.code ?? runtimeRun?.lastErrorCode ?? null
  const runFailureMessage =
    runtimeRun?.lastError?.message ??
    (runtimeRun?.isFailed
      ? 'Xero recovered a failed agent run without a persisted diagnostic.'
      : null)
  const runFailurePresentation = runFailureMessage
    ? failurePresentation(runFailureMessage, runFailureCode ?? 'run_failed')
    : null
  const streamFailurePresentation = streamFailure
    ? failurePresentation(
        describeStreamMessage(streamFailure.code, streamFailure.message),
        streamFailure.code,
      )
    : null
  const inlineFailureDuplicatesRunFailure = Boolean(
    runFailureMessage &&
      visibleTurns.some(
        (turn) =>
          turn.kind === 'failure' &&
          failureDiagnosticsMatch(
            turn.message,
            turn.code,
            runFailureMessage,
            runFailureCode,
          ),
      ),
  )
  const inlineFailureDuplicatesStreamFailure = Boolean(
    streamFailure &&
      visibleTurns.some(
        (turn) =>
          turn.kind === 'failure' &&
          failureDiagnosticsMatch(
            turn.message,
            turn.code,
            streamFailure.message,
            streamFailure.code,
          ),
      ),
  )
  const streamFailureIsDuplicate =
    Boolean(
      streamFailure?.message &&
        failureMessagesMatch(streamFailure.message, runFailureMessage),
    ) || Boolean(streamFailure?.code && streamFailure.code === runFailureCode)
  const streamIssueIsDuplicate =
    Boolean(
      streamIssue?.message &&
        (streamIssue.message === runFailureMessage ||
          streamIssue.message === streamFailure?.message),
    ) ||
    Boolean(
      streamIssue?.code &&
        (streamIssue.code === runFailureCode ||
          streamIssue.code === streamFailure?.code),
    )

  const showRunFailure = Boolean(runFailureMessage && !inlineFailureDuplicatesRunFailure)
  const showStreamFailure = Boolean(
    streamFailure && !streamFailureIsDuplicate && !inlineFailureDuplicatesStreamFailure,
  )
  const showStreamIssue = Boolean(streamIssue && !streamIssueIsDuplicate)
  // Suppress the footer handoff notice if an inline `handoff_notice` turn is
  // already in the conversation. The inline turn is the steady-state marker
  // (rendered after the runtime run snapshot rebinds to the target run); the
  // footer only fires when the pane is still subscribed to the source run's
  // stream, which is the transient pre-rebind state.
  const hasInlineHandoffNotice = visibleTurns.some(
    (turn) => turn.kind === 'handoff_notice',
  )
  const showHandoffNotice =
    !showRunFailure &&
    !hasInlineHandoffNotice &&
    isHandoffCompletion(streamCompletion)
  const showAnyNotice =
    showRunFailure || showStreamFailure || showStreamIssue || showHandoffNotice
  const showAnyTurn = visibleTurns.length > 0

  const lastTurn =
    visibleTurns.length > 0 ? visibleTurns[visibleTurns.length - 1] : null
  const isLastTurnStreamingAssistant = Boolean(
    showActivityIndicator &&
      lastTurn &&
      lastTurn.kind === 'message' &&
      lastTurn.role === 'assistant' &&
      lastTurn.text.trim().length > 0,
  )
  const copyableVisibleConversationText = useMemo(
    () => visibleConversationCopyText(visibleTurns, currentAgentLabel),
    [currentAgentLabel, visibleTurns],
  )

  if (variant === 'dense') {
    return (
      <section
        aria-label="Agent conversation"
        className="flex flex-col gap-2 text-[12px] leading-snug select-text"
      >
        {showAnyTurn ? (
          <ol
            aria-label="Agent conversation turns"
            className="flex flex-col gap-2"
          >
            <AnimatePresence initial={false}>
              {visibleTurns.map((turn) => (
                <DenseTurnItem
                  key={turn.id}
                  turn={turn}
                  codeUndoStates={codeUndoStates}
                  returnSessionToHereStates={returnSessionToHereStates}
                  onUndoChangeGroup={onUndoChangeGroup}
                  onReturnSessionToHere={onReturnSessionToHere}
                  onOpenHandoffSummary={onOpenHandoffSummary}
                  currentAgentLabel={currentAgentLabel}
                />
              ))}
            </AnimatePresence>
          </ol>
        ) : null}
        {showActivityIndicator ? <AgentActivityIndicator /> : null}
        {showAnyNotice ? (
          <ul
            aria-label="Agent run notices"
            className="mt-2 flex flex-col gap-2"
          >
            {showHandoffNotice ? (
              <li>
                {onOpenHandoffSummary ? (
                  <button
                    type="button"
                    onClick={() =>
                      onOpenHandoffSummary({
                        sourceRunId: footerHandoffSourceRunId,
                        targetRunId: null,
                      })
                    }
                    aria-label="Run continued in a fresh session — view handoff context"
                    className="flex w-full items-center justify-between gap-2 rounded-sm border border-border/40 bg-muted/15 px-2 py-1 text-left text-[12px] text-muted-foreground transition-colors hover:bg-muted/30 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <span>⤳ handed off to a fresh same-type session</span>
                    <span className="shrink-0 text-[10.5px] font-medium text-foreground/80">
                      See what carried over →
                    </span>
                  </button>
                ) : (
                  <div className="rounded-sm border border-border/40 bg-muted/15 px-2 py-1 text-[12px] text-muted-foreground">
                    ⤳ handed off to a fresh same-type session
                  </div>
                )}
              </li>
            ) : null}
            {showRunFailure ? (
              <li
                className={cn(
                  'rounded-sm border px-2 py-1 text-[12px]',
                  runFailurePresentation?.tone === 'warning'
                    ? 'border-warning/30 bg-warning/10 text-foreground'
                    : 'border-destructive/30 bg-destructive/10 text-destructive',
                )}
              >
                {runFailurePresentation?.message ?? runFailureMessage}
              </li>
            ) : null}
            {showStreamFailure && streamFailure ? (
              <li
                className={cn(
                  'rounded-sm border px-2 py-1 text-[12px]',
                  streamFailurePresentation?.tone === 'warning'
                    ? 'border-warning/30 bg-warning/10 text-foreground'
                    : 'border-destructive/30 bg-destructive/10 text-destructive',
                )}
              >
                {streamFailurePresentation?.message ??
                  describeStreamMessage(
                    streamFailure.code,
                    streamFailure.message,
                  )}
              </li>
            ) : null}
            {showStreamIssue && streamIssue ? (
              <li className="rounded-sm border border-border/40 bg-muted/20 px-2 py-1 text-[12px] text-muted-foreground">
                ⚠ {describeStreamMessage(streamIssue.code, streamIssue.message)}
              </li>
            ) : null}
          </ul>
        ) : null}
      </section>
    )
  }

  return (
    <section
      aria-label="Agent conversation"
      className="flex flex-col gap-5 select-text"
    >
      {showAnyTurn ? (
        <ol
          aria-label="Agent conversation turns"
          className="flex flex-col gap-5"
        >
          <AnimatePresence initial={false}>
            {visibleTurns.map((turn, index) => {
              const prev = index > 0 ? visibleTurns[index - 1] : null
              const next =
                index < visibleTurns.length - 1
                  ? visibleTurns[index + 1]
                  : null
              return (
                <ConversationTurnItem
                  key={turn.id}
                  turn={turn}
                  accountAvatarUrl={accountAvatarUrl}
                  accountLogin={accountLogin}
                  isStreaming={
                    index === visibleTurns.length - 1 &&
                    isLastTurnStreamingAssistant
                  }
                  connectsTop={isToolTurnKind(prev)}
                  connectsBottom={isToolTurnKind(next)}
                  codeUndoStates={codeUndoStates}
                  returnSessionToHereStates={returnSessionToHereStates}
                  onUndoChangeGroup={onUndoChangeGroup}
                  onReturnSessionToHere={onReturnSessionToHere}
                  onOpenHandoffSummary={onOpenHandoffSummary}
                  currentAgentLabel={currentAgentLabel}
                  isLastTurn={index === visibleTurns.length - 1}
                  nextTurn={next}
                  hideCopyBeforeFooterNotice={
                    showHandoffNotice && index === visibleTurns.length - 1
                  }
                  visibleConversationCopyText={copyableVisibleConversationText}
                />
              )
            })}
          </AnimatePresence>
        </ol>
      ) : null}

      {showActivityIndicator && !isLastTurnStreamingAssistant ? (
        <AgentActivityIndicator />
      ) : null}

      {showAnyNotice ? (
        <ul aria-label="Agent run notices" className="flex flex-col gap-2.5">
          {showHandoffNotice ? (
            <NoticeListItem>
              <HandoffFooterNotice
                onOpen={
                  onOpenHandoffSummary
                    ? () =>
                        onOpenHandoffSummary({
                          sourceRunId: footerHandoffSourceRunId,
                          targetRunId: null,
                        })
                    : undefined
                }
              />
            </NoticeListItem>
          ) : null}
          {showRunFailure ? (
            <NoticeListItem>
              <NoticeRow
                tone={runFailurePresentation?.tone ?? 'destructive'}
                title={
                  runFailurePresentation?.tone === 'warning'
                    ? runFailurePresentation.title
                    : runtimeRun?.isTerminal
                      ? 'Latest saved run failed'
                      : 'Agent run failed'
                }
                message={
                  runFailurePresentation?.message ?? runFailureMessage ?? ''
                }
                code={runFailureCode}
              />
            </NoticeListItem>
          ) : null}
          {showStreamFailure && streamFailure ? (
            <NoticeListItem>
              <NoticeRow
                tone={streamFailurePresentation?.tone ?? 'destructive'}
                title={streamFailurePresentation?.title ?? 'Live stream failed'}
                message={
                  streamFailurePresentation?.message ??
                  describeStreamMessage(
                    streamFailure.code,
                    streamFailure.message,
                  )
                }
                code={streamFailure.code}
              />
            </NoticeListItem>
          ) : null}
          {showStreamIssue && streamIssue ? (
            <NoticeListItem>
              <NoticeRow
                tone={streamIssue.retryable ? 'info' : 'warning'}
                title={describeStreamTitle(
                  streamIssue.code,
                  'Live stream issue',
                )}
                message={describeStreamMessage(
                  streamIssue.code,
                  streamIssue.message,
                )}
                code={streamIssue.code}
              />
            </NoticeListItem>
          ) : null}
        </ul>
      ) : null}
    </section>
  )
})

interface CopyTextButtonProps {
  text?: string
  label: string
  copiedLabel: string
  tooltip: string
  className?: string
  iconClassName?: string
  size?: 'icon-xs' | 'icon-sm'
}

function CopyTextButton({
  text,
  label,
  copiedLabel,
  tooltip,
  className,
  iconClassName,
  size = 'icon-sm',
}: CopyTextButtonProps) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!copied) return
    const timeoutId = window.setTimeout(() => setCopied(false), 1400)
    return () => window.clearTimeout(timeoutId)
  }, [copied])

  const handleCopy = useCallback(async () => {
    try {
      if (!navigator.clipboard?.writeText) return
      const resolvedText = text ?? ''
      if (resolvedText.length === 0) return
      await navigator.clipboard.writeText(resolvedText)
      setCopied(true)
    } catch {
      // Clipboard writes can be denied by WebView permissions or test runners.
    }
  }, [text])

  const Icon = copied ? Check : Copy

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size={size}
          aria-label={copied ? copiedLabel : label}
          onClick={handleCopy}
          className={cn(
            'select-none rounded-md transition-opacity',
            iconClassName ? null : '[&_svg]:size-[11px]',
            copied ? 'text-success opacity-100' : null,
            className,
          )}
        >
          <Icon
            key={copied ? 'copied' : 'idle'}
            aria-hidden="true"
            className={cn(
              iconClassName,
              copied ? 'agent-copy-icon-pop' : undefined,
            )}
          />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">{copied ? 'Copied' : tooltip}</TooltipContent>
    </Tooltip>
  )
}

function NoticeListItem({ children }: { children: React.ReactNode }) {
  return <AnimatedTranscriptListItem>{children}</AnimatedTranscriptListItem>
}

/**
 * Map well-known stream issue codes to a friendlier user-facing title.
 * Unknown codes fall back to the supplied default.
 */
function describeStreamTitle(code: string, fallback: string): string {
  switch (code) {
    case 'runtime_stream_contract_mismatch':
      return 'Skipped an out-of-order update'
    case 'runtime_stream_project_mismatch':
      return 'Stream belonged to a different project'
    default:
      return fallback
  }
}

/**
 * Map well-known stream issue codes to a friendlier user-facing message.
 * Unknown codes fall through to the original diagnostic so production
 * issues stay diagnosable.
 */
function describeStreamMessage(code: string, fallback: string): string {
  switch (code) {
    case 'runtime_stream_contract_mismatch':
      return "An update arrived out of order, so we kept the existing transcript. The agent's reply may be missing a few characters — retry if anything looks cut off."
    case 'runtime_stream_project_mismatch':
      return 'The runtime sent activity for a different project. We ignored it to keep the active session in sync.'
    default:
      return fallback
  }
}

function isToolTurnKind(turn: ConversationTurn | null): boolean {
  return (
    turn != null && (turn.kind === 'action' || turn.kind === 'action_group')
  )
}

function isHandoffBoundaryTurn(turn: ConversationTurn | null): boolean {
  return turn?.kind === 'routing_suggestion' || turn?.kind === 'handoff_notice'
}

interface ConversationTurnItemProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
  isLastTurn: boolean
  nextTurn: ConversationTurn | null
  hideCopyBeforeFooterNotice: boolean
  visibleConversationCopyText: string
  /** Previous visible turn is also a tool call; render a connector up to it. */
  connectsTop: boolean
  /** Next visible turn is also a tool call; render a connector down to it. */
  connectsBottom: boolean
  codeUndoStates: Record<string, CodeUndoUiState>
  returnSessionToHereStates: Record<string, CodeUndoUiState>
  currentAgentLabel?: string | null
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  onOpenHandoffSummary?: (request: {
    sourceRunId?: string | null
    targetRunId?: string | null
  }) => void
}

const TRANSCRIPT_MOTION_TRANSITION = {
  duration: 0.24,
  ease: [0.22, 1, 0.36, 1],
}
const TRANSCRIPT_INSTANT_TRANSITION = { duration: 0 }

function useTranscriptMotionTransition() {
  return useReducedMotion()
    ? TRANSCRIPT_INSTANT_TRANSITION
    : TRANSCRIPT_MOTION_TRANSITION
}

function AnimatedTranscriptListItem({
  children,
  className,
  style,
  turn,
}: {
  children: React.ReactNode
  className?: string
  style?: React.CSSProperties
  turn?: ConversationTurn
}) {
  const transition = useTranscriptMotionTransition()

  return (
    <motion.li
      initial={false}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={transition}
      className={className}
      style={style}
      data-conversation-turn-id={turn?.id}
      data-conversation-turn-kind={turn?.kind}
      data-conversation-turn-role={turn?.kind === 'message' ? turn.role : undefined}
    >
      {children}
    </motion.li>
  )
}

function AnimatedTranscriptPanel({
  children,
  className,
}: {
  children: React.ReactNode
  className?: string
}) {
  const transition = useTranscriptMotionTransition()

  return (
    <motion.div
      layout
      initial={false}
      animate={{ opacity: 1, height: 'auto', y: 0, scale: 1 }}
      exit={{ opacity: 0, height: 0, y: -3, scale: 0.995 }}
      transition={transition}
      className={cn('overflow-hidden', className)}
    >
      {children}
    </motion.div>
  )
}

function ConversationTurnItem({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
  isLastTurn,
  nextTurn,
  hideCopyBeforeFooterNotice,
  visibleConversationCopyText,
  connectsTop,
  connectsBottom,
  codeUndoStates,
  returnSessionToHereStates,
  currentAgentLabel,
  onUndoChangeGroup,
  onReturnSessionToHere,
  onOpenHandoffSummary,
}: ConversationTurnItemProps) {
  return (
    <AnimatedTranscriptListItem
      turn={turn}
      className={turn.kind === 'routing_suggestion' ? 'mt-1 mb-1' : undefined}
    >
      <ConversationTurnRow
        turn={turn}
        accountAvatarUrl={accountAvatarUrl}
        accountLogin={accountLogin}
        isStreaming={isStreaming}
        isLastTurn={isLastTurn}
        nextTurn={nextTurn}
        hideCopyBeforeFooterNotice={hideCopyBeforeFooterNotice}
        visibleConversationCopyText={visibleConversationCopyText}
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
        codeUndoStates={codeUndoStates}
        returnSessionToHereStates={returnSessionToHereStates}
        currentAgentLabel={currentAgentLabel}
        onUndoChangeGroup={onUndoChangeGroup}
        onReturnSessionToHere={onReturnSessionToHere}
        onOpenHandoffSummary={onOpenHandoffSummary}
      />
    </AnimatedTranscriptListItem>
  )
}

interface ConversationTurnRowProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
  isLastTurn: boolean
  nextTurn: ConversationTurn | null
  hideCopyBeforeFooterNotice: boolean
  visibleConversationCopyText: string
  connectsTop: boolean
  connectsBottom: boolean
  codeUndoStates: Record<string, CodeUndoUiState>
  returnSessionToHereStates: Record<string, CodeUndoUiState>
  currentAgentLabel?: string | null
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  onOpenHandoffSummary?: (request: {
    sourceRunId?: string | null
    targetRunId?: string | null
  }) => void
}

function ConversationTurnRow({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
  isLastTurn,
  nextTurn,
  hideCopyBeforeFooterNotice,
  visibleConversationCopyText,
  connectsTop,
  connectsBottom,
  codeUndoStates,
  returnSessionToHereStates,
  currentAgentLabel,
  onUndoChangeGroup,
  onReturnSessionToHere,
  onOpenHandoffSummary,
}: ConversationTurnRowProps) {
  if (turn.kind === 'message') {
    return turn.role === 'user' ? (
      <UserMessage
        text={turn.text}
        attachments={turn.attachments}
        accountAvatarUrl={accountAvatarUrl}
        accountLogin={accountLogin}
      />
    ) : (
      <AssistantMessage
        messageId={turn.id}
        text={turn.text}
        attachments={turn.attachments}
        isStreaming={isStreaming}
        hideCopyButton={
          isHandoffBoundaryTurn(nextTurn) ||
          (isLastTurn && hideCopyBeforeFooterNotice)
        }
        copyAction={
          isLastTurn
            ? {
                text: visibleConversationCopyText,
                label: 'Copy visible conversation',
                copiedLabel: 'Copied visible conversation',
                tooltip: 'Copy visible conversation',
              }
            : null
        }
      />
    )
  }

  if (turn.kind === 'thinking') {
    return (
      <div className="flex min-w-0 flex-col items-start gap-1.5">
        <span className="sr-only">Agent</span>
        <ThinkingBlock messageId={turn.id} text={turn.text} />
      </div>
    )
  }

  if (turn.kind === 'failure') {
    return <FailureCard message={turn.message} code={turn.code} />
  }

  if (turn.kind === 'file_change') {
    const fileUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'file_change',
            changeGroupId: turn.changeGroupId,
            filePath: fileChangeUndoFilePath(turn),
          })
        ]
      : undefined
    const changeGroupUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'change_group',
            changeGroupId: turn.changeGroupId,
          })
        ]
      : undefined
    const hunkUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'hunks',
            changeGroupId: turn.changeGroupId,
            filePath: fileChangeUndoFilePath(turn),
          })
        ]
      : undefined
    const returnSessionState = turn.changeGroupId
      ? returnSessionToHereStates[
          getReturnSessionToHereStateKey({
            targetKind: 'run_boundary',
            boundaryId: codeChangeGroupBoundaryId(turn.changeGroupId),
            runId: turn.runId,
            changeGroupId: turn.changeGroupId,
          })
        ]
      : undefined

    return (
      <FileChangeRow
        turn={turn}
        fileUndoState={fileUndoState}
        changeGroupUndoState={changeGroupUndoState}
        hunkUndoState={hunkUndoState}
        returnSessionState={returnSessionState}
        onUndoChangeGroup={onUndoChangeGroup}
        onReturnSessionToHere={onReturnSessionToHere}
      />
    )
  }

  if (turn.kind === 'action_group') {
    return (
      <ActionGroupCard
        title={turn.title}
        detail={turn.detail}
        state={turn.state ?? null}
        actions={turn.actions}
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
      />
    )
  }

  if (turn.kind === 'action_prompt') {
    return (
      <ActionPromptCard
        actionId={turn.actionId}
        runId={turn.runId ?? null}
        actionType={turn.actionType}
        title={turn.title}
        detail={turn.detail}
        shape={turn.shape}
        options={turn.options}
        allowMultiple={turn.allowMultiple}
        sensitiveFields={turn.sensitiveFields ?? null}
        intendedUse={turn.intendedUse ?? null}
        resolved={turn.isResolved}
      />
    )
  }

  if (turn.kind === 'handoff_notice') {
    const handler = onOpenHandoffSummary
    return (
      <HandoffNoticeRow
        onOpen={
          handler
            ? () =>
                handler({
                  sourceRunId: turn.sourceRunId,
                  targetRunId: turn.targetRunId,
                })
            : undefined
        }
      />
    )
  }

  if (turn.kind === 'routing_suggestion') {
    return (
      <RoutingSuggestionCard
        turnId={turn.id}
        targetKind={turn.targetKind}
        targetAgentId={turn.targetAgentId}
        targetAgentDefinitionId={turn.targetAgentDefinitionId}
        targetAgentDefinitionVersion={turn.targetAgentDefinitionVersion}
        targetLabel={turn.targetLabel}
        reason={turn.reason}
        summary={turn.summary}
        isResolved={turn.isResolved}
        acceptedTarget={turn.acceptedTarget}
        acceptedTargetAgentDefinitionId={turn.acceptedTargetAgentDefinitionId}
        acceptedTargetLabel={turn.acceptedTargetLabel}
        resolutionMode={turn.routingResolutionMode}
        currentAgentLabel={currentAgentLabel}
      />
    )
  }

  if (turn.kind === 'subagent_group') {
    return (
      <SubagentGroupCard
        turn={turn}
        accountAvatarUrl={accountAvatarUrl}
        accountLogin={accountLogin}
        isStreaming={isStreaming}
        codeUndoStates={codeUndoStates}
        returnSessionToHereStates={returnSessionToHereStates}
        onUndoChangeGroup={onUndoChangeGroup}
        onReturnSessionToHere={onReturnSessionToHere}
        onOpenHandoffSummary={onOpenHandoffSummary}
      />
    )
  }

  return (
    <ActionCard
      title={turn.title}
      detail={turn.detail}
      detailRows={turn.detailRows}
      mediaAttachments={turn.mediaAttachments}
      state={turn.state ?? null}
      defaultOpen={turn.defaultOpen ?? false}
      connectsTop={connectsTop}
      connectsBottom={connectsBottom}
    />
  )
}

function HandoffNoticeRow({ onOpen }: { onOpen?: () => void }) {
  const description = (
    <>
      <History
        className="h-3.5 w-3.5 shrink-0 text-muted-foreground/80"
        aria-hidden
      />
      <span className="min-w-0 flex-1 text-left">
        Run continued in a fresh session — context budget filled, so Xero handed
        this conversation off to a new same-type run. Earlier turns above are
        from the previous run; the conversation continues below.
      </span>
      {onOpen ? (
        <span className="ml-2 inline-flex shrink-0 items-center gap-1 text-[11px] font-medium text-foreground/80">
          See what carried over
          <span
            aria-hidden="true"
            className="inline-block transition-transform duration-200 ease-out group-hover/handoff:translate-x-0.5 group-focus-visible/handoff:translate-x-0.5"
          >
            →
          </span>
        </span>
      ) : null}
    </>
  )
  if (onOpen) {
    return (
      <button
        type="button"
        aria-label="Run continued in a fresh session — view handoff context"
        onClick={onOpen}
        className="agent-handoff-notice group/handoff flex w-full items-center gap-2.5 rounded-md border border-border/60 bg-muted/20 px-3 py-2 text-[12px] text-muted-foreground hover:bg-muted/40 hover:text-foreground hover:border-primary/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {description}
      </button>
    )
  }
  return (
    <div
      role="note"
      aria-label="Run continued in a fresh session"
      className="flex items-center gap-2.5 rounded-md border border-border/60 bg-muted/20 px-3 py-2 text-[12px] text-muted-foreground"
    >
      {description}
    </div>
  )
}

function humanizeFileChangeOperation(operation: string): string {
  return (
    operation
      .trim()
      .replace(/[._-]+/g, ' ')
      .replace(/\s+/g, ' ')
      .toLowerCase() || 'changed'
  )
}

function fileChangeTargetLabel(
  turn: Extract<ConversationTurn, { kind: 'file_change' }>,
): string {
  return turn.toPath ? `${turn.path} -> ${turn.toPath}` : turn.path
}

function fileChangeUndoFilePath(
  turn: Extract<ConversationTurn, { kind: 'file_change' }>,
): string {
  return turn.toPath ?? turn.path
}

function codeChangeGroupBoundaryId(changeGroupId: string): string {
  return `change_group:${changeGroupId}`
}

function visibleCodeUndoState(
  fileUndoState: CodeUndoUiState | undefined,
  changeGroupUndoState: CodeUndoUiState | undefined,
  hunkUndoState?: CodeUndoUiState,
  returnSessionState?: CodeUndoUiState,
): CodeUndoUiState | undefined {
  if (returnSessionState?.status === 'pending') return returnSessionState
  if (changeGroupUndoState?.status === 'pending') return changeGroupUndoState
  if (fileUndoState?.status === 'pending') return fileUndoState
  if (hunkUndoState?.status === 'pending') return hunkUndoState
  return (
    returnSessionState ?? hunkUndoState ?? fileUndoState ?? changeGroupUndoState
  )
}

function fileChangeTextHunks(
  turn: Extract<ConversationTurn, { kind: 'file_change' }>,
): CodePatchTextHunkDto[] {
  const availability = turn.patchAvailability
  if (!turn.changeGroupId || !availability?.available) return []
  if (availability.targetChangeGroupId !== turn.changeGroupId) return []

  const undoFilePath = fileChangeUndoFilePath(turn)
  return (availability.textHunks ?? [])
    .filter((hunk) => hunk.filePath === undoFilePath)
    .slice()
    .sort(
      (left, right) =>
        left.hunkIndex - right.hunkIndex ||
        left.hunkId.localeCompare(right.hunkId),
    )
}

function formatHunkLineLabel(hunk: CodePatchTextHunkDto): string {
  const useResultLines = hunk.resultLineCount > 0
  const startLine = useResultLines ? hunk.resultStartLine : hunk.baseStartLine
  const lineCount = useResultLines ? hunk.resultLineCount : hunk.baseLineCount
  const side = useResultLines ? 'new' : 'old'
  if (lineCount <= 1) {
    return `${side} line ${startLine}`
  }

  return `${side} lines ${startLine}-${startLine + lineCount - 1}`
}

function uniqueNonEmptyStrings(values: readonly string[]): string[] {
  return Array.from(
    new Set(values.map((value) => value.trim()).filter(Boolean)),
  )
}

function humanizeConflictKind(kind: CodeHistoryConflictDto['kind']): string {
  switch (kind) {
    case 'text_overlap':
      return 'Overlapping text'
    case 'file_missing':
      return 'File missing'
    case 'file_exists':
      return 'File already exists'
    case 'content_mismatch':
      return 'Content changed'
    case 'metadata_mismatch':
      return 'Metadata changed'
    case 'unsupported_operation':
      return 'Unsupported change'
    case 'stale_workspace':
      return 'Workspace changed'
    case 'storage_error':
      return 'History storage issue'
    default:
      return 'Conflict'
  }
}

function CodeUndoConflictDetails({
  summary,
  dense = false,
}: {
  summary?: CodeUndoConflictSummary
  dense?: boolean
}) {
  const [open, setOpen] = useState(true)
  if (!summary || summary.conflicts.length === 0) return null

  const affectedPaths = uniqueNonEmptyStrings(summary.affectedPaths)

  return (
    <div
      role="alert"
      aria-label={summary.title}
      className={cn(
        'mt-2 rounded-md border border-destructive/25 bg-destructive/8 text-destructive shadow-sm',
        dense ? 'px-2 py-1.5' : 'px-2.5 py-2',
      )}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        <div className="flex items-start gap-2">
          <AlertTriangle
            aria-hidden="true"
            className={cn(
              'mt-[2px] shrink-0',
              dense ? 'h-3 w-3' : 'h-3.5 w-3.5',
            )}
          />
          <div className="min-w-0 flex-1">
            <p
              className={cn(
                'font-medium',
                dense ? 'text-[11.5px]' : 'text-[12px]',
              )}
            >
              {summary.title}
            </p>
            <p
              className={cn(
                'mt-0.5 break-words text-destructive/85',
                dense ? 'text-[11px]' : 'text-[11.5px]',
              )}
            >
              {summary.targetLabel}
            </p>
          </div>
          <CollapsibleTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={`${open ? 'Hide' : 'Show'} conflict details for ${summary.targetLabel}`}
              className={cn(
                'h-5 w-5 shrink-0 rounded-md text-destructive/70 hover:bg-destructive/10 hover:text-destructive',
                'focus-visible:ring-destructive/35 [&_svg]:size-3',
              )}
            >
              <ChevronDown
                aria-hidden="true"
                className={cn(
                  'transition-transform duration-150',
                  open && 'rotate-180',
                )}
              />
            </Button>
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent>
          <div
            className={cn(
              'mt-2 space-y-2 border-t border-destructive/15 pt-2',
              dense && 'mt-1.5 pt-1.5',
            )}
          >
            {affectedPaths.length > 0 ? (
              <div>
                <p className="text-[10.5px] font-medium uppercase text-destructive/70">
                  Affected paths
                </p>
                <div className="mt-1 flex flex-wrap gap-1">
                  {affectedPaths.map((path) => (
                    <Badge
                      key={path}
                      variant="outline"
                      className="max-w-full border-destructive/25 bg-background/50 px-1.5 py-0 font-mono text-[10.5px] text-destructive"
                      title={path}
                    >
                      <span className="truncate">{path}</span>
                    </Badge>
                  ))}
                </div>
              </div>
            ) : null}
            <ul className="space-y-1.5">
              {summary.conflicts.map((conflict, index) => {
                const hunkIds = uniqueNonEmptyStrings(conflict.hunkIds)
                return (
                  <li
                    key={`${conflict.path}:${conflict.kind}:${index}`}
                    className="rounded-md bg-background/45 px-2 py-1.5 text-destructive"
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                      <span
                        className="min-w-0 truncate font-mono text-[11px]"
                        title={conflict.path}
                      >
                        {conflict.path}
                      </span>
                      <Badge
                        variant="outline"
                        className="border-destructive/25 px-1.5 py-0 text-[10px] text-destructive"
                      >
                        {humanizeConflictKind(conflict.kind)}
                      </Badge>
                    </div>
                    <p className="mt-0.5 whitespace-pre-wrap break-words text-[11.5px] leading-relaxed text-destructive/90">
                      {conflict.message}
                    </p>
                    {hunkIds.length > 0 ? (
                      <div className="mt-1 flex flex-wrap items-center gap-1">
                        <span className="text-[10.5px] text-destructive/70">
                          Selected hunks
                        </span>
                        {hunkIds.map((hunkId) => (
                          <Badge
                            key={hunkId}
                            variant="outline"
                            className="border-destructive/20 bg-background/50 px-1.5 py-0 font-mono text-[10px] text-destructive"
                          >
                            {hunkId}
                          </Badge>
                        ))}
                      </div>
                    ) : null}
                  </li>
                )
              })}
            </ul>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

function FileChangeRow({
  turn,
  fileUndoState,
  changeGroupUndoState,
  hunkUndoState,
  returnSessionState,
  onUndoChangeGroup,
  onReturnSessionToHere,
}: {
  turn: Extract<ConversationTurn, { kind: 'file_change' }>
  fileUndoState?: CodeUndoUiState
  changeGroupUndoState?: CodeUndoUiState
  hunkUndoState?: CodeUndoUiState
  returnSessionState?: CodeUndoUiState
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
}) {
  const operationLabel = humanizeFileChangeOperation(turn.operation)
  const targetLabel = fileChangeTargetLabel(turn)
  const undoFilePath = fileChangeUndoFilePath(turn)
  const canUndo = Boolean(turn.changeGroupId && onUndoChangeGroup)
  const canReturnSessionToHere = Boolean(
    turn.changeGroupId && turn.runId && onReturnSessionToHere,
  )
  const textHunks = fileChangeTextHunks(turn)
  const undoState = visibleCodeUndoState(
    fileUndoState,
    changeGroupUndoState,
    hunkUndoState,
    returnSessionState,
  )
  const statusTone =
    undoState?.status === 'failed'
      ? 'text-destructive'
      : undoState?.status === 'succeeded'
        ? 'text-success'
        : 'text-muted-foreground'

  return (
    <div
      className={cn(
        'group/file-change flex items-start gap-2 rounded-md px-1 py-1 text-left transition-colors',
        'hover:bg-foreground/[0.03]',
      )}
    >
      <FileText
        className="mt-[3px] h-3.5 w-3.5 shrink-0 text-primary/70"
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-1.5">
          <span className="shrink-0 text-[12.5px] font-medium text-foreground">
            {operationLabel}
          </span>
          <span
            className="min-w-0 flex-1 truncate font-mono text-[12px] text-foreground/80"
            title={targetLabel}
          >
            {targetLabel}
          </span>
        </div>
        <p
          className="mt-0.5 min-w-0 truncate text-[11.5px] text-muted-foreground/75"
          title={turn.detail}
        >
          {turn.detail}
        </p>
        {undoState ? (
          <>
            <output
              className={cn('mt-1 text-[11.5px]', statusTone)}
              aria-live="polite"
            >
              {undoState.message}
            </output>
            <CodeUndoConflictDetails summary={undoState.conflictSummary} />
          </>
        ) : null}
      </div>
      <CodeUndoMenu
        path={targetLabel}
        filePath={undoFilePath}
        changeGroupId={turn.changeGroupId}
        expectedWorkspaceEpoch={turn.workspaceEpoch}
        fileState={fileUndoState}
        changeGroupState={changeGroupUndoState}
        hunkState={hunkUndoState}
        returnSessionState={returnSessionState}
        textHunks={textHunks}
        enabled={canUndo}
        canReturnSessionToHere={canReturnSessionToHere}
        runId={turn.runId}
        onUndoChangeGroup={onUndoChangeGroup}
        onReturnSessionToHere={onReturnSessionToHere}
      />
    </div>
  )
}

function CodeUndoMenu({
  path,
  filePath,
  changeGroupId,
  expectedWorkspaceEpoch,
  fileState,
  changeGroupState,
  hunkState,
  returnSessionState,
  textHunks,
  enabled,
  canReturnSessionToHere,
  runId,
  onUndoChangeGroup,
  onReturnSessionToHere,
  dense = false,
}: {
  path: string
  filePath: string
  changeGroupId: string | null
  expectedWorkspaceEpoch?: number | null
  fileState?: CodeUndoUiState
  changeGroupState?: CodeUndoUiState
  hunkState?: CodeUndoUiState
  returnSessionState?: CodeUndoUiState
  textHunks: CodePatchTextHunkDto[]
  enabled: boolean
  canReturnSessionToHere: boolean
  runId: string | null
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  dense?: boolean
}) {
  const [selectedHunkIds, setSelectedHunkIds] = useState<string[]>([])
  const visibleState = visibleCodeUndoState(
    fileState,
    changeGroupState,
    hunkState,
    returnSessionState,
  )
  const isPending =
    fileState?.status === 'pending' ||
    changeGroupState?.status === 'pending' ||
    hunkState?.status === 'pending' ||
    returnSessionState?.status === 'pending'
  const isChangeGroupSucceeded = changeGroupState?.status === 'succeeded'
  const isFileSucceeded = fileState?.status === 'succeeded'
  const isReturnSessionSucceeded = returnSessionState?.status === 'succeeded'
  const hasMenuAction = enabled || canReturnSessionToHere
  const canOpen =
    hasMenuAction &&
    Boolean(changeGroupId) &&
    !isPending &&
    !isChangeGroupSucceeded &&
    !isReturnSessionSucceeded
  const hasTextHunks = textHunks.length > 0
  const label = (() => {
    if (!hasMenuAction || !changeGroupId) return `Undo unavailable for ${path}`
    if (isPending)
      return returnSessionState?.status === 'pending'
        ? `Returning session to here for ${path}`
        : `Undoing ${path}`
    if (isReturnSessionSucceeded) return `Returned session to here for ${path}`
    if (isChangeGroupSucceeded) return `Undone ${path}`
    return `Open undo menu for ${path}`
  })()
  const tooltip = (() => {
    if (!changeGroupId) return 'Code history data unavailable'
    if (!hasMenuAction) return 'Code history unavailable'
    if (isPending)
      return returnSessionState?.status === 'pending'
        ? 'Returning session'
        : 'Undoing'
    if (isReturnSessionSucceeded) return 'Session returned'
    if (isChangeGroupSucceeded) return 'Undone'
    return 'Code history options'
  })()
  const TriggerIcon = isPending
    ? Loader2
    : isChangeGroupSucceeded || isReturnSessionSucceeded
      ? CheckCircle2
      : MoreHorizontal
  const undoActionsDisabled = !enabled || !changeGroupId || !canOpen
  const fileActionDisabled = undoActionsDisabled || isFileSucceeded
  const changeGroupActionDisabled =
    undoActionsDisabled || isChangeGroupSucceeded
  const selectedHunkCount = selectedHunkIds.length
  const hunkActionDisabled = undoActionsDisabled || selectedHunkCount === 0
  const returnSessionActionDisabled =
    !canReturnSessionToHere ||
    !changeGroupId ||
    !runId ||
    !canOpen ||
    isReturnSessionSucceeded
  const fileActionLabel =
    fileState?.status === 'failed'
      ? 'Retry undo for this file change'
      : isFileSucceeded
        ? 'File change undone'
        : 'Undo this file change'
  const changeGroupActionLabel =
    changeGroupState?.status === 'failed'
      ? 'Retry undo for entire change group'
      : isChangeGroupSucceeded
        ? 'Change group undone'
        : 'Undo entire change group'
  const hunkActionLabel =
    hunkState?.status === 'failed'
      ? 'Retry undo for selected hunks'
      : hunkState?.status === 'succeeded'
        ? 'Undo selected hunks'
        : 'Undo selected hunks'
  const returnSessionActionLabel =
    returnSessionState?.status === 'failed'
      ? 'Retry return session to here'
      : isReturnSessionSucceeded
        ? 'Session returned here'
        : 'Return this session to here'
  const menuWidthClass = dense ? 'min-w-56' : 'min-w-72'

  useEffect(() => {
    const availableIds = new Set(textHunks.map((hunk) => hunk.hunkId))
    setSelectedHunkIds((current) => {
      const next = current.filter((hunkId) => availableIds.has(hunkId))
      if (
        next.length === current.length &&
        next.every((hunkId, index) => hunkId === current[index])
      ) {
        return current
      }
      return next
    })
  }, [textHunks])

  const toggleSelectedHunk = useCallback((hunkId: string, checked: boolean) => {
    setSelectedHunkIds((current) => {
      if (checked) {
        return current.includes(hunkId) ? current : [...current, hunkId]
      }

      return current.filter((selectedHunkId) => selectedHunkId !== hunkId)
    })
  }, [])

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild disabled={!canOpen}>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={label}
          title={tooltip}
          disabled={!canOpen}
          className={cn(
            'mt-[1px] shrink-0 rounded-md text-muted-foreground/70 hover:text-foreground',
            dense ? 'h-5 w-5 [&_svg]:size-[11px]' : 'h-6 w-6 [&_svg]:size-3',
            visibleState?.status === 'failed' &&
              'text-destructive hover:text-destructive',
            (isChangeGroupSucceeded || isReturnSessionSucceeded) &&
              'text-success',
          )}
        >
          <TriggerIcon
            aria-hidden="true"
            className={cn(isPending && 'animate-spin')}
          />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className={menuWidthClass}>
        <DropdownMenuItem
          disabled={fileActionDisabled}
          onSelect={() => {
            if (!changeGroupId || fileActionDisabled) return
            onUndoChangeGroup?.({
              targetKind: 'file_change',
              changeGroupId,
              path,
              filePath,
              expectedWorkspaceEpoch,
            })
          }}
        >
          {isFileSucceeded ? (
            <CheckCircle2 aria-hidden="true" />
          ) : (
            <FileText aria-hidden="true" />
          )}
          <span>{fileActionLabel}</span>
        </DropdownMenuItem>
        {hasTextHunks ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="px-2 py-1 text-[11px] font-medium text-muted-foreground">
              Select hunks
            </DropdownMenuLabel>
            {textHunks.map((hunk) => {
              const checked = selectedHunkIds.includes(hunk.hunkId)
              return (
                <DropdownMenuCheckboxItem
                  key={hunk.hunkId}
                  checked={checked}
                  disabled={!canOpen || !enabled}
                  className="text-[12px]"
                  onCheckedChange={(nextChecked) => {
                    toggleSelectedHunk(hunk.hunkId, nextChecked === true)
                  }}
                  onSelect={(event) => event.preventDefault()}
                >
                  <span className="min-w-0 flex-1 truncate">
                    {`Hunk ${hunk.hunkIndex + 1} · ${formatHunkLineLabel(hunk)}`}
                  </span>
                </DropdownMenuCheckboxItem>
              )
            })}
            <DropdownMenuItem
              disabled={hunkActionDisabled}
              onSelect={() => {
                if (!changeGroupId || hunkActionDisabled) return
                onUndoChangeGroup?.({
                  targetKind: 'hunks',
                  changeGroupId,
                  path,
                  filePath,
                  hunkIds: selectedHunkIds,
                  expectedWorkspaceEpoch,
                })
              }}
            >
              <Undo2 aria-hidden="true" />
              <span>{hunkActionLabel}</span>
              {selectedHunkCount > 0 ? (
                <span className="ml-auto text-[11px] text-muted-foreground">
                  {selectedHunkCount}
                </span>
              ) : null}
            </DropdownMenuItem>
          </>
        ) : null}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={changeGroupActionDisabled}
          onSelect={() => {
            if (!changeGroupId || changeGroupActionDisabled) return
            onUndoChangeGroup?.({
              targetKind: 'change_group',
              changeGroupId,
              path,
              filePath,
              expectedWorkspaceEpoch,
            })
          }}
        >
          {isChangeGroupSucceeded ? (
            <CheckCircle2 aria-hidden="true" />
          ) : (
            <Undo2 aria-hidden="true" />
          )}
          <span>{changeGroupActionLabel}</span>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={returnSessionActionDisabled}
          onSelect={() => {
            if (!changeGroupId || !runId || returnSessionActionDisabled) return
            const boundaryId = codeChangeGroupBoundaryId(changeGroupId)
            onReturnSessionToHere?.({
              targetKind: 'run_boundary',
              targetId: `${runId}:${boundaryId}`,
              boundaryId,
              runId,
              changeGroupId,
              expectedWorkspaceEpoch,
            })
          }}
        >
          {isReturnSessionSucceeded ? (
            <CheckCircle2 aria-hidden="true" />
          ) : (
            <History aria-hidden="true" />
          )}
          <span>{returnSessionActionLabel}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// ---------------------------------------------------------------------------
// Action / tool row — cardless, inline row with a leading status icon. The
// running state gets a thin primary-colored left rail; the failed state gets
// destructive-colored text. No surrounding border or background — consecutive
// tool calls read as a compact log rather than a stack of cards.
// ---------------------------------------------------------------------------

interface ActionCardProps {
  title: string
  detail: string
  detailRows: Array<{ label: string; value: string }>
  mediaAttachments?: ConversationMessageAttachment[]
  state: RuntimeStreamToolItemView['toolState'] | null
  defaultOpen?: boolean
  connectsTop?: boolean
  connectsBottom?: boolean
}

function ActionCard({
  title,
  detail,
  detailRows,
  mediaAttachments,
  state,
  defaultOpen = false,
  connectsTop = false,
  connectsBottom = false,
}: ActionCardProps) {
  const hasDetails = detailRows.length > 0 || Boolean(mediaAttachments?.length)
  const [open, setOpen] = useState(() => defaultOpen && hasDetails)
  const isFailed = state === 'failed'
  const rowClass = cn(
    'flex w-full items-center gap-2 rounded-md py-0.5 text-left transition-colors',
    isFailed && 'text-destructive',
  )

  useEffect(() => {
    if (defaultOpen && hasDetails) {
      setOpen(true)
    }
  }, [defaultOpen, hasDetails])

  return (
    <div className="group/tool relative">
      <ToolChainConnectors
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
      />
      {hasDetails && open ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-[8px] top-[10px] bottom-0 w-px bg-border/25"
        />
      ) : null}
      <Collapsible open={open} onOpenChange={setOpen}>
        {hasDetails ? (
          <CollapsibleTrigger asChild>
            <button
              type="button"
              aria-label={`${open ? 'Hide' : 'Show'} tool details for ${title}`}
              className={cn(
                rowClass,
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
              )}
            >
              <ActionCardHeader
                title={title}
                detail={detail}
                state={state}
                open={open}
              />
            </button>
          </CollapsibleTrigger>
        ) : (
          <div className={rowClass}>
            <ActionCardHeader
              title={title}
              detail={detail}
              state={state}
              open={null}
            />
          </div>
        )}
        {hasDetails ? (
          <AnimatePresence initial={false}>
            {open ? (
              <AnimatedTranscriptPanel>
                <div className="ml-[22px] pl-3 pr-1 pb-1.5 pt-1">
                  <ToolDetailRows
                    rows={detailRows}
                    mediaAttachments={mediaAttachments}
                  />
                </div>
              </AnimatedTranscriptPanel>
            ) : null}
          </AnimatePresence>
        ) : null}
      </Collapsible>
    </div>
  )
}

function ActionCardHeader({
  title,
  detail,
  state,
  open,
}: {
  title: string
  detail: string
  state: RuntimeStreamToolItemView['toolState'] | null
  open: boolean | null
}) {
  const isFailed = state === 'failed'
  const hasDetail = detail.trim().length > 0
  return (
    <>
      <ToolStatusIcon state={state} className="shrink-0" />
      <span
        className={cn(
          'min-w-0 shrink truncate text-[13px] font-medium tracking-[-0.005em]',
          isFailed ? 'text-destructive' : 'text-foreground',
        )}
        title={title}
      >
        {title}
      </span>
      {hasDetail ? (
        <span
          className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground/75"
          title={detail}
        >
          {detail}
        </span>
      ) : (
        <span aria-hidden="true" className="flex-1" />
      )}
      {open !== null ? (
        <ChevronDown
          aria-hidden="true"
          className={cn(
            'h-3.5 w-3.5 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
            'group-hover/tool:text-muted-foreground/80',
            open ? 'rotate-180 text-muted-foreground/80' : 'rotate-0',
          )}
        />
      ) : null}
    </>
  )
}

/**
 * Renders the small vertical lines that visually chain consecutive tool-call
 * rows together at the status-icon column. The line bridges half of the
 * surrounding gap-5 (20px → 10px each side) and meets the next row's
 * connector in the middle of the gap.
 *
 * Positioning is hard-coded to match the row layout: outer wrapper has no
 * padding, the row has no horizontal padding, and the 16px status icon puts
 * the icon center at x = 8px (and y ≈ 10px — icon vertically centered in a
 * ~20px row with `items-center`).
 */
function ToolChainConnectors({
  connectsTop,
  connectsBottom,
}: {
  connectsTop: boolean
  connectsBottom: boolean
}) {
  if (!connectsTop && !connectsBottom) return null
  return (
    <>
      {connectsTop ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-[8px] -top-[10px] h-[20px] w-px bg-muted-foreground/35"
        />
      ) : null}
      {connectsBottom ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-[8px] top-[10px] -bottom-[10px] w-px bg-muted-foreground/35"
        />
      ) : null}
    </>
  )
}

interface ToolDetailRowsProps {
  rows: Array<{ label: string; value: string }>
  mediaAttachments?: ConversationMessageAttachment[]
}

function ToolDetailRows({ rows, mediaAttachments }: ToolDetailRowsProps) {
  const safeMediaAttachments = mediaAttachments ?? []
  const hasMedia = safeMediaAttachments.length > 0
  const outputRow = rows.find((row) => /output/i.test(row.label))
  if (outputRow) {
    return (
      <div className="flex flex-col gap-2">
        {hasMedia ? (
          <ToolMediaAttachments attachments={safeMediaAttachments} />
        ) : null}
        <div className="group/output relative">
          <pre
            className={cn(
              'm-0 max-h-[360px] overflow-auto whitespace-pre-wrap break-words rounded-md',
              'bg-background/40 px-3 py-2.5 pr-9 font-mono text-[12px] leading-[1.6] text-foreground/85',
              'scrollbar-thin',
            )}
          >
            {outputRow.value}
          </pre>
          <ToolOutputCopyAffordance
            text={outputRow.value}
            label="Copy tool output"
          />
        </div>
      </div>
    )
  }

  const fallback =
    rows.find((row) => /result/i.test(row.label)) ??
    rows.find((row) => /input|outcome|cmd|command/i.test(row.label)) ??
    rows[0]
  if (!fallback) {
    return hasMedia ? (
      <ToolMediaAttachments attachments={safeMediaAttachments} />
    ) : null
  }

  const isCommandLike = /^[A-Za-z][\w./-]*\s/.test(fallback.value.trim())
  return (
    <div className="flex flex-col gap-2">
      {hasMedia ? (
        <ToolMediaAttachments attachments={safeMediaAttachments} />
      ) : null}
      <div className="group/output relative">
        <div
          className={cn(
            'rounded-md bg-background/40 px-3 py-2 pr-9',
            'whitespace-pre-wrap break-words font-mono text-[12px] leading-[1.6] text-foreground/80',
          )}
        >
          {isCommandLike ? (
            <span className="flex items-start gap-2">
              <Terminal
                aria-hidden="true"
                className="mt-[2px] h-3 w-3 shrink-0 text-primary/75"
              />
              <span className="min-w-0 flex-1">{fallback.value}</span>
            </span>
          ) : (
            fallback.value
          )}
        </div>
        <ToolOutputCopyAffordance
          text={fallback.value}
          label="Copy tool output"
        />
      </div>
    </div>
  )
}

function ToolOutputCopyAffordance({
  text,
  label,
}: {
  text: string
  label: string
}) {
  if (text.trim().length === 0) return null
  return (
    <div
      className={cn(
        'pointer-events-none absolute right-1 top-1',
        'opacity-0 transition-opacity duration-150',
        'group-hover/output:opacity-100 focus-within:opacity-100',
      )}
    >
      <CopyTextButton
        text={text}
        label={label}
        copiedLabel={`${label} (copied)`}
        tooltip="Copy output"
        className={cn(
          'pointer-events-auto h-5 w-5 rounded-md',
          'bg-background/85 text-muted-foreground shadow-sm ring-1 ring-border/40',
          'backdrop-blur hover:bg-background hover:text-foreground',
        )}
      />
    </div>
  )
}

// ---------------------------------------------------------------------------
// Subagent group card — header with role/status/budget + inline child transcript.
// ---------------------------------------------------------------------------

const SUBAGENT_TERMINAL_STATUSES = new Set([
  'completed',
  'failed',
  'cancelled',
  'budget_exhausted',
  'handed_off',
])

const SUBAGENT_ACTIVE_STATUSES = new Set([
  'spawned',
  'pending',
  'starting',
  'running',
  'paused',
  'cancelling',
])

function formatSubagentStatusLabel(status: string): string {
  return status.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

function formatBudgetUsage(
  used: number | null,
  max: number | null,
): string | null {
  if (used == null && max == null) return null
  if (used != null && max != null) return `${used}/${max}`
  if (used != null) return `${used}`
  return `0/${max}`
}

function formatTokenBudgetUsage(
  used: number | null,
  max: number | null,
): string | null {
  if (used == null && max == null) return null
  const fmt = (value: number) =>
    value >= 10_000
      ? `${(value / 1000).toFixed(1).replace(/\.0$/, '')}k`
      : `${value}`
  if (used != null && max != null) return `${fmt(used)}/${fmt(max)}`
  if (used != null) return fmt(used)
  return `0/${fmt(max ?? 0)}`
}

function subagentStatusToken(status: string): {
  className: string
  Icon: typeof Loader2 | null
} {
  if (status === 'completed' || status === 'handed_off') {
    return { className: 'text-success', Icon: CheckCircle2 }
  }
  if (status === 'failed' || status === 'budget_exhausted') {
    return { className: 'text-destructive', Icon: AlertCircle }
  }
  if (status === 'cancelled') {
    return { className: 'text-muted-foreground', Icon: XCircle }
  }
  if (SUBAGENT_ACTIVE_STATUSES.has(status)) {
    return { className: 'text-primary', Icon: Loader2 }
  }
  return { className: 'text-muted-foreground', Icon: Circle }
}

interface SubagentGroupCardProps {
  turn: Extract<ConversationTurn, { kind: 'subagent_group' }>
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
  codeUndoStates: Record<string, CodeUndoUiState>
  returnSessionToHereStates: Record<string, CodeUndoUiState>
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  onOpenHandoffSummary?: (request: {
    sourceRunId?: string | null
    targetRunId?: string | null
  }) => void
}

function SubagentGroupCard({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
  codeUndoStates,
  returnSessionToHereStates,
  onUndoChangeGroup,
  onReturnSessionToHere,
  onOpenHandoffSummary,
}: SubagentGroupCardProps) {
  const isTerminal = SUBAGENT_TERMINAL_STATUSES.has(turn.status)
  const isActive = SUBAGENT_ACTIVE_STATUSES.has(turn.status)
  const [open, setOpen] = useState(!isTerminal)
  const [autoCollapsed, setAutoCollapsed] = useState(false)

  useEffect(() => {
    if (isTerminal && !autoCollapsed) {
      setOpen(false)
      setAutoCollapsed(true)
    }
  }, [isTerminal, autoCollapsed])

  const { className: statusClassName, Icon: StatusIcon } = subagentStatusToken(
    turn.status,
  )
  const toolBudget = formatBudgetUsage(turn.usedToolCalls, turn.maxToolCalls)
  const tokenBudget = formatTokenBudgetUsage(turn.usedTokens, turn.maxTokens)
  const childCount = turn.children.length
  const hasChildren = childCount > 0

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="group/subagent relative"
      data-subagent-id={turn.subagentId}
      data-subagent-status={turn.status}
    >
      <CollapsibleTrigger asChild>
        <button
          type="button"
          aria-label={`${open ? 'Hide' : 'Show'} subagent ${turn.roleLabel} transcript`}
          className={cn(
            'flex w-full items-center gap-2 rounded-md border border-border/40 bg-muted/20 px-2 py-1.5 text-left transition-colors',
            'hover:bg-muted/30',
            isActive && 'border-primary/30 bg-primary/[0.04]',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
          )}
        >
          <Bot
            aria-hidden="true"
            className={cn('h-3.5 w-3.5 shrink-0', statusClassName)}
          />
          <span
            className="min-w-0 truncate text-[12.5px] font-medium tracking-[-0.005em] text-foreground"
            title={turn.roleLabel}
          >
            {turn.roleLabel}
          </span>
          <span
            className={cn(
              'inline-flex items-center gap-1 shrink-0 rounded-full px-1.5 py-px text-[10.5px] font-medium uppercase tracking-wide',
              statusClassName,
            )}
          >
            {StatusIcon ? (
              <StatusIcon
                aria-hidden="true"
                className={cn(
                  'h-2.5 w-2.5',
                  isActive && StatusIcon === Loader2 ? 'animate-spin' : '',
                )}
              />
            ) : null}
            {formatSubagentStatusLabel(turn.status)}
          </span>
          <span className="min-w-0 flex-1 truncate text-[11.5px] text-muted-foreground/75">
            {[
              toolBudget ? `Tools ${toolBudget}` : null,
              tokenBudget ? `Tokens ${tokenBudget}` : null,
              hasChildren
                ? `${childCount} ${childCount === 1 ? 'turn' : 'turns'}`
                : null,
            ]
              .filter(Boolean)
              .join(' · ')}
          </span>
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'h-3 w-3 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
              'group-hover/subagent:text-muted-foreground/80',
              open ? 'rotate-180 text-muted-foreground/80' : 'rotate-0',
            )}
          />
        </button>
      </CollapsibleTrigger>
      <AnimatePresence initial={false}>
        {open ? (
          <AnimatedTranscriptPanel>
            <div className="ml-[22px] mt-1 flex flex-col gap-2 border-l border-border/25 pl-3">
              {turn.prompt ? (
                <div className="rounded-md border border-border/40 bg-muted/15 px-2 py-1.5 text-[12px] text-muted-foreground">
                  <span className="mb-0.5 inline-flex items-center gap-1 text-[10.5px] font-medium uppercase tracking-wide text-muted-foreground/80">
                    <GitBranch aria-hidden="true" className="h-2.5 w-2.5" />
                    Subagent prompt
                  </span>
                  <div className="whitespace-pre-wrap break-words">
                    {turn.prompt}
                  </div>
                </div>
              ) : null}
              {hasChildren ? (
                <ul
                  aria-label={`${turn.roleLabel} subagent transcript`}
                  className="flex flex-col gap-2"
                >
                  <AnimatePresence initial={false}>
                    {turn.children.map((childTurn, index) => {
                      const nextChildTurn =
                        index < turn.children.length - 1 ? turn.children[index + 1] : null
                      return (
                        <AnimatedTranscriptListItem
                          key={childTurn.id}
                          className="agent-stagger-child"
                          style={{ ['--stagger-index' as string]: index }}
                        >
                          <ConversationTurnRow
                            turn={childTurn}
                            accountAvatarUrl={accountAvatarUrl}
                            accountLogin={accountLogin}
                            isStreaming={isStreaming && !isTerminal}
                            isLastTurn={false}
                            nextTurn={nextChildTurn}
                            hideCopyBeforeFooterNotice={false}
                            visibleConversationCopyText=""
                            connectsTop={false}
                            connectsBottom={false}
                            codeUndoStates={codeUndoStates}
                            returnSessionToHereStates={returnSessionToHereStates}
                            onUndoChangeGroup={onUndoChangeGroup}
                            onReturnSessionToHere={onReturnSessionToHere}
                            onOpenHandoffSummary={onOpenHandoffSummary}
                          />
                        </AnimatedTranscriptListItem>
                      )
                    })}
                  </AnimatePresence>
                </ul>
              ) : (
                <div className="text-[11.5px] italic text-muted-foreground/70">
                  {isActive
                    ? 'Subagent is starting up; transcript will appear here.'
                    : 'Subagent produced no transcript items.'}
                </div>
              )}
              {turn.resultSummary ? (
                <div
                  className={cn(
                    'rounded-md border px-2 py-1.5 text-[12px]',
                    turn.status === 'failed' ||
                      turn.status === 'budget_exhausted'
                      ? 'border-destructive/30 bg-destructive/[0.04] text-destructive'
                      : 'border-border/40 bg-muted/15 text-muted-foreground',
                  )}
                >
                  <span className="mb-0.5 inline-flex items-center gap-1 text-[10.5px] font-medium uppercase tracking-wide opacity-80">
                    <Info aria-hidden="true" className="h-2.5 w-2.5" />
                    {turn.status === 'failed' ||
                    turn.status === 'budget_exhausted'
                      ? 'Failure'
                      : 'Result summary'}
                  </span>
                  <div className="whitespace-pre-wrap break-words">
                    {turn.resultSummary}
                  </div>
                </div>
              ) : null}
            </div>
          </AnimatedTranscriptPanel>
        ) : null}
      </AnimatePresence>
    </Collapsible>
  )
}

// ---------------------------------------------------------------------------
// Action group card — collapsed summary + expandable inline list.
// ---------------------------------------------------------------------------

interface ActionGroupCardProps {
  title: string
  detail: string
  state: RuntimeStreamToolItemView['toolState'] | null
  actions: Array<{
    id: string
    sequence: number
    toolCallId: string
    toolName: string
    title: string
    detail: string
    detailRows: Array<{ label: string; value: string }>
    mediaAttachments?: ConversationMessageAttachment[]
    state: RuntimeStreamToolItemView['toolState'] | null
    defaultOpen?: boolean
  }>
  connectsTop?: boolean
  connectsBottom?: boolean
}

function ActionGroupCard({
  title,
  detail,
  state,
  actions,
  connectsTop = false,
  connectsBottom = false,
}: ActionGroupCardProps) {
  const [open, setOpen] = useState(false)
  const hasDetail = detail.trim().length > 0
  const singleAction = actions.length === 1 ? actions[0] : null
  const singleActionHasDetails = Boolean(
    singleAction &&
      (singleAction.detailRows.length > 0 ||
        Boolean(singleAction.mediaAttachments?.length)),
  )
  const hasExpandableContent = !singleAction || singleActionHasDetails
  const detailsLabel =
    actions.length === 1 ? 'compact tool details' : 'grouped tool details'
  const rowClassName = cn(
    'flex w-full items-center gap-2 rounded-md py-0.5 text-left transition-colors',
    hasExpandableContent &&
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
  )
  const rowContent = (
    <>
      <ToolStatusIcon state={state} className="shrink-0" />
      <span
        className="min-w-0 shrink truncate text-[13px] font-medium tracking-[-0.005em] text-foreground"
        title={title}
      >
        {title}
      </span>
      {hasDetail ? (
        <span
          className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground/75"
          title={detail}
        >
          {detail}
        </span>
      ) : (
        <span aria-hidden="true" className="flex-1" />
      )}
      {hasExpandableContent ? (
        <ChevronDown
          aria-hidden="true"
          className={cn(
            'h-3.5 w-3.5 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
            'group-hover/tool:text-muted-foreground/80',
            open ? 'rotate-180 text-muted-foreground/80' : 'rotate-0',
          )}
        />
      ) : null}
    </>
  )

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="group/tool relative"
    >
      <ToolChainConnectors
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
      />
      {open && hasExpandableContent ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-[8px] top-[10px] bottom-0 w-px bg-border/60"
        />
      ) : null}
      {hasExpandableContent ? (
        <CollapsibleTrigger asChild>
          <button
            type="button"
            aria-label={`${open ? 'Hide' : 'Show'} ${detailsLabel} for ${title}`}
            className={rowClassName}
          >
            {rowContent}
          </button>
        </CollapsibleTrigger>
      ) : (
        <div className={rowClassName}>{rowContent}</div>
      )}
      <AnimatePresence initial={false}>
        {open && hasExpandableContent ? (
          <AnimatedTranscriptPanel>
            {singleAction ? (
              <div className="ml-[22px] pl-3 pr-1 pb-1.5 pt-1">
                <ToolDetailRows
                  rows={singleAction.detailRows}
                  mediaAttachments={singleAction.mediaAttachments}
                />
              </div>
            ) : (
              <ol className="ml-[12px] mt-0.5 flex flex-col gap-0.5 pl-3">
                <AnimatePresence initial={false}>
                  {actions.map((action, index) => (
                    <ActionGroupItem
                      key={action.id}
                      action={action}
                      index={index}
                    />
                  ))}
                </AnimatePresence>
              </ol>
            )}
          </AnimatedTranscriptPanel>
        ) : null}
      </AnimatePresence>
    </Collapsible>
  )
}

function ActionGroupItem({
  action,
  index = 0,
}: {
  action: ActionGroupCardProps['actions'][number]
  index?: number
}) {
  const [open, setOpen] = useState(false)
  const hasDetails =
    action.detailRows.length > 0 || Boolean(action.mediaAttachments?.length)
  const rowClass = cn(
    'flex w-full items-center gap-2 rounded-md py-0.5 text-left transition-colors',
    action.state === 'failed' && 'text-destructive',
  )

  return (
    <AnimatedTranscriptListItem
      className="group/sub agent-stagger-child relative"
      style={{ ['--stagger-index' as string]: index }}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        {hasDetails && open ? (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute left-[8px] top-[10px] bottom-0 w-px bg-border/25"
          />
        ) : null}
        {hasDetails ? (
          <CollapsibleTrigger asChild>
            <button
              type="button"
              aria-label={`${open ? 'Hide' : 'Show'} tool details for ${action.title}`}
              className={cn(
                rowClass,
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
              )}
            >
              <ActionGroupItemHeader action={action} open={open} />
            </button>
          </CollapsibleTrigger>
        ) : (
          <div className={rowClass}>
            <ActionGroupItemHeader action={action} open={null} />
          </div>
        )}
        {hasDetails ? (
          <AnimatePresence initial={false}>
            {open ? (
              <AnimatedTranscriptPanel>
                <div className="ml-[20px] pb-1.5 pl-3 pr-1 pt-1">
                  <ToolDetailRows
                    rows={action.detailRows}
                    mediaAttachments={action.mediaAttachments}
                  />
                </div>
              </AnimatedTranscriptPanel>
            ) : null}
          </AnimatePresence>
        ) : null}
      </Collapsible>
    </AnimatedTranscriptListItem>
  )
}

function ActionGroupItemHeader({
  action,
  open,
}: {
  action: ActionGroupCardProps['actions'][number]
  open: boolean | null
}) {
  const isFailed = action.state === 'failed'
  const hasDetail = action.detail.trim().length > 0
  return (
    <>
      <ToolStatusIcon state={action.state} className="shrink-0" />
      <span
        className={cn(
          'min-w-0 shrink truncate text-[13px]',
          isFailed ? 'text-destructive' : 'text-foreground/95',
        )}
        title={action.title}
      >
        {action.title}
      </span>
      {hasDetail ? (
        <span
          className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground/75"
          title={action.detail}
        >
          {action.detail}
        </span>
      ) : (
        <span aria-hidden="true" className="flex-1" />
      )}
      {open !== null ? (
        <ChevronDown
          aria-hidden="true"
          className={cn(
            'h-3.5 w-3.5 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
            open ? 'rotate-180 text-muted-foreground/80' : 'rotate-0',
          )}
        />
      ) : null}
    </>
  )
}

// ---------------------------------------------------------------------------
// User message — right-aligned bubble with a soft primary tint.
// ---------------------------------------------------------------------------

interface UserMessageProps {
  text: string
  attachments?: ConversationMessageAttachment[]
  accountAvatarUrl: string | null
  accountLogin: string | null
}

function UserMessage({
  text,
  attachments,
  accountAvatarUrl,
  accountLogin,
}: UserMessageProps) {
  const promptParts = useMemo(() => splitBrowserToolPromptContext(text), [text])
  const visibleText = promptParts.visibleText
  const promptContexts = promptParts.contexts
  const promptContextAttachments = useMemo(
    () => pairBrowserToolContextAttachments(promptContexts, attachments),
    [attachments, promptContexts],
  )
  const visibleAttachments = promptContextAttachments.unpairedAttachments
  const hasAttachments = Boolean(visibleAttachments?.length)
  const hasPromptContexts = promptContexts.length > 0
  const isTouch = useIsTouchDevice()
  const [tapCopied, setTapCopied] = useState(false)

  useEffect(() => {
    if (!tapCopied) return
    const timeoutId = window.setTimeout(() => setTapCopied(false), 1400)
    return () => window.clearTimeout(timeoutId)
  }, [tapCopied])

  const trimmedLength = visibleText.trim().length
  const canTapCopy = isTouch && trimmedLength > 0

  const handleBubbleTap = useCallback(async () => {
    if (!canTapCopy) return
    try {
      if (!navigator.clipboard?.writeText) return
      await navigator.clipboard.writeText(visibleText)
      setTapCopied(true)
    } catch {
      // Clipboard writes can be denied by WebView permissions or test runners.
    }
  }, [canTapCopy, visibleText])

  const bubbleClassName = cn(
    'rounded-2xl px-3.5 py-2',
    'bg-primary/10 text-foreground',
    'ring-1 ring-inset transition-[box-shadow,background-color] duration-150',
    tapCopied ? 'ring-success/70 bg-success/10' : 'ring-primary/40',
    'whitespace-pre-wrap break-words text-[14px] leading-relaxed select-text',
    'agent-user-bubble-enter',
    canTapCopy
      ? 'cursor-pointer text-left active:bg-primary/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/70'
      : null,
  )

  return (
    <div className="group/user flex justify-end gap-2.5">
      <div className="flex min-w-0 max-w-[80%] flex-col items-end gap-1">
        <span className="sr-only">You</span>
        {visibleText.length > 0 ? (
          canTapCopy ? (
            <button
              type="button"
              onClick={() => void handleBubbleTap()}
              aria-label={
                tapCopied ? 'Copied your prompt' : 'Tap to copy your prompt'
              }
              className={bubbleClassName}
            >
              {visibleText}
            </button>
          ) : (
            <div className={bubbleClassName}>{visibleText}</div>
          )
        ) : null}
        {hasAttachments || hasPromptContexts ? (
          <div className="mt-2 flex max-w-full flex-wrap justify-end gap-1.5">
            {promptContexts.map((context) => (
              <PromptContextCard
                key={context.id}
                context={context}
                attachment={promptContextAttachments.pairedByContextId.get(context.id)}
              />
            ))}
            {visibleAttachments?.map((attachment) => (
              <AttachmentPreviewChip
                key={attachment.id}
                attachment={attachment}
              />
            ))}
          </div>
        ) : null}
        {!isTouch && trimmedLength > 0 ? (
          <div
            className={cn(
              'flex h-3 translate-y-1.5 items-center justify-end pr-0.5',
              'opacity-0 transition-opacity duration-150',
              'group-hover/user:opacity-100 focus-within:opacity-100',
            )}
          >
            <CopyTextButton
              text={visibleText}
              label="Copy your prompt"
              copiedLabel="Copied your prompt"
              tooltip="Copy"
              size="icon-xs"
              className="!size-[26px] text-muted-foreground/55 hover:text-foreground"
              iconClassName="size-[14px]"
            />
          </div>
        ) : null}
        {canTapCopy && tapCopied ? (
          <div className="flex h-3 translate-y-1.5 items-center justify-end pr-0.5">
            <span className="inline-flex items-center gap-1 text-[10px] font-medium text-success">
              <Check
                className="size-[11px] agent-copy-icon-pop"
                aria-hidden="true"
              />
              Copied
            </span>
          </div>
        ) : null}
      </div>
      <UserAvatar avatarUrl={accountAvatarUrl} login={accountLogin} />
    </div>
  )
}

function PromptContextCard({
  attachment,
  context,
}: {
  attachment?: ConversationMessageAttachment
  context: BrowserToolPromptContext
}) {
  const [open, setOpen] = useState(false)
  const Icon =
    context.kind === 'sketch'
      ? PencilLine
      : context.kind === 'element'
        ? MousePointer2
        : context.kind === 'folder'
          ? FolderOpen
          : FileText
  const shouldShowAttachment = context.kind === 'sketch' && attachment?.kind === 'image'
  const subtitle =
    context.subtitle ??
    context.page ??
    `${context.lines.length} detail${context.lines.length === 1 ? '' : 's'}`
  const hasDetails = context.lines.length > 0

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <article
        role="note"
        aria-label={`${context.title} attached to prompt`}
        className="w-[260px] max-w-full overflow-hidden rounded-lg border border-border/50 bg-muted/30 text-left text-foreground shadow-sm"
      >
        <div className="flex items-center gap-2 p-1.5">
          {shouldShowAttachment && attachment ? (
            <div className="shrink-0">
              <ImageAttachmentPreview
                attachment={attachment}
                className="h-12 w-16"
                variant="card"
              />
            </div>
          ) : (
            <span className="flex h-12 w-12 shrink-0 items-center justify-center rounded-md bg-background text-primary/80 ring-1 ring-border/40">
              <Icon
                aria-hidden="true"
                className="h-4 w-4"
              />
            </span>
          )}
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-2 rounded-sm py-0.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              aria-label={`${open ? 'Collapse' : 'Expand'} ${context.title}`}
              disabled={!hasDetails}
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12px] font-medium leading-tight" title={context.title}>
                  {context.title}
                </span>
                <span className="mt-0.5 block truncate text-[10.5px] leading-tight text-muted-foreground" title={subtitle}>
                  {subtitle}
                </span>
              </span>
              {hasDetails ? (
                <ChevronDown
                  aria-hidden="true"
                  className={cn(
                    'h-3.5 w-3.5 shrink-0 text-muted-foreground/70 transition-transform duration-150',
                    open ? 'rotate-180' : 'rotate-0',
                  )}
                />
              ) : null}
            </button>
          </CollapsibleTrigger>
        </div>
        {hasDetails ? (
          <CollapsibleContent>
            <div className="max-h-48 space-y-0.5 overflow-y-auto border-t border-border/40 px-2.5 py-2 text-[11.5px] leading-relaxed text-muted-foreground">
              {context.lines.map((line, index) => (
                <p key={`${context.id}:line-${index}`} className="m-0 whitespace-pre-wrap break-words">
                  {line}
                </p>
              ))}
            </div>
          </CollapsibleContent>
        ) : null}
      </article>
    </Collapsible>
  )
}

function useIsTouchDevice(): boolean {
  const [isTouch, setIsTouch] = useState(false)
  useEffect(() => {
    if (
      typeof window === 'undefined' ||
      typeof window.matchMedia !== 'function'
    ) {
      return
    }
    const mediaQuery = window.matchMedia('(hover: none) and (pointer: coarse)')
    setIsTouch(mediaQuery.matches)
    const handleChange = (event: MediaQueryListEvent) =>
      setIsTouch(event.matches)
    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [])
  return isTouch
}

// ---------------------------------------------------------------------------
// Assistant message — left-aligned with reasoning split out from response.
// ---------------------------------------------------------------------------

const THINK_PATTERN = /<(think(?:ing)?)>([\s\S]*?)<\/\1>/gi

interface AssistantSegment {
  kind: 'thinking' | 'response'
  text: string
}

function splitAssistantText(text: string): AssistantSegment[] {
  const segments: AssistantSegment[] = []
  let lastIndex = 0

  THINK_PATTERN.lastIndex = 0
  let match = THINK_PATTERN.exec(text)
  while (match !== null) {
    if (match.index > lastIndex) {
      const before = text.slice(lastIndex, match.index)
      if (before.trim().length > 0) {
        segments.push({ kind: 'response', text: before })
      }
    }
    const inner = match[2]
    if (inner.trim().length > 0) {
      segments.push({ kind: 'thinking', text: inner })
    }
    lastIndex = match.index + match[0].length
    match = THINK_PATTERN.exec(text)
  }

  if (lastIndex < text.length) {
    const tail = text.slice(lastIndex)
    if (tail.trim().length > 0) {
      segments.push({ kind: 'response', text: tail })
    }
  }

  if (segments.length === 0 && text.trim().length > 0) {
    segments.push({ kind: 'response', text })
  }

  return segments
}

function AssistantMessage({
  messageId,
  text,
  attachments,
  isStreaming,
  hideCopyButton,
  copyAction,
}: {
  messageId: string
  text: string
  attachments?: ConversationMessageAttachment[]
  isStreaming: boolean
  hideCopyButton?: boolean
  copyAction?: {
    text: string
    label: string
    copiedLabel: string
    tooltip: string
  } | null
}) {
  const segments = useMemo(() => splitAssistantText(text), [text])
  const hasAttachments = Boolean(attachments?.length)
  const isTouch = useIsTouchDevice()
  const responseCopyText = useMemo(
    () =>
      segments
        .filter((segment) => segment.kind === 'response')
        .map((segment) => segment.text.trim())
        .filter(Boolean)
        .join('\n\n'),
    [segments],
  )
  const lastResponseIndex = (() => {
    for (let i = segments.length - 1; i >= 0; i -= 1) {
      if (segments[i].kind === 'response') return i
    }
    return -1
  })()
  const resolvedCopyAction = copyAction ?? {
    text: responseCopyText,
    label: 'Copy agent response',
    copiedLabel: 'Copied agent response',
    tooltip: 'Copy',
  }

  return (
    <div className="group/agent flex min-w-0 flex-col items-start gap-1.5">
      <span className="sr-only">Agent</span>
      <div className="flex w-full min-w-0 flex-col items-start gap-2">
        {segments.map((segment, index) =>
          segment.kind === 'thinking' ? (
            <ThinkingBlock
              key={index}
              messageId={`${messageId}:thinking:${index}`}
              text={segment.text}
            />
          ) : (
            <ResponseBlock
              key={index}
              messageId={`${messageId}:response:${index}`}
              text={segment.text}
              isStreaming={isStreaming && index === lastResponseIndex}
              showCaret={isStreaming && index === lastResponseIndex}
            />
          ),
        )}
        {hasAttachments ? (
          <ToolMediaAttachments
            attachments={attachments ?? []}
            variant="response"
          />
        ) : null}
      </div>
      {!hideCopyButton && resolvedCopyAction.text.length > 0 ? (
        <div
          className={cn(
            'mt-2 flex h-4 items-center pl-0.5 transition-opacity duration-150',
            isTouch
              ? 'opacity-100'
              : 'opacity-0 group-hover/agent:opacity-100 focus-within:opacity-100',
          )}
        >
          <CopyTextButton
            text={resolvedCopyAction.text}
            label={resolvedCopyAction.label}
            copiedLabel={resolvedCopyAction.copiedLabel}
            tooltip={resolvedCopyAction.tooltip}
            className="h-4 w-4 text-muted-foreground/60 hover:text-foreground"
          />
        </div>
      ) : null}
    </div>
  )
}

function ResponseBlock({
  messageId,
  text,
  isStreaming = false,
  showCaret = false,
}: {
  messageId: string
  text: string
  isStreaming?: boolean
  showCaret?: boolean
}) {
  return (
    <div className="w-full min-w-0 px-0.5 text-foreground select-text">
      <Markdown
        messageId={messageId}
        text={text}
        streaming={isStreaming}
        trailing={showCaret ? <StreamingCaret /> : null}
      />
    </div>
  )
}

function StreamingCaret() {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'agent-stream-caret ml-0.5 inline-block h-[0.95em] w-[2px] translate-y-[2px] rounded-sm bg-primary/80 align-text-bottom',
      )}
    />
  )
}

function ThinkingBlock({
  messageId,
  text,
}: {
  messageId?: string
  text: string
}) {
  return (
    <div className="w-full max-w-full min-w-0">
      <div className="flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-[0.07em] text-muted-foreground/90">
        <Brain className="h-3.5 w-3.5 text-primary/70" />
        <span>Thoughts</span>
      </div>
      <div className="mt-1.5">
        <Markdown messageId={messageId ?? null} text={text} muted />
      </div>
    </div>
  )
}

function AgentActivityIndicator() {
  return (
    <output
      className={cn(
        'flex items-center gap-2',
        'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-200 motion-safe:ease-out',
      )}
      aria-label="Agent is thinking"
    >
      <AgentAvatar pulse />
      <span className="ml-2 flex items-center gap-1.5" aria-hidden="true">
        <span
          className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/80"
          style={{ animationDelay: '0ms' }}
        />
        <span
          className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/80"
          style={{ animationDelay: '200ms' }}
        />
        <span
          className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/80"
          style={{ animationDelay: '400ms' }}
        />
      </span>
    </output>
  )
}

// ---------------------------------------------------------------------------
// Run / stream notice (rendered inline at the foot of the conversation).
// ---------------------------------------------------------------------------

function HandoffFooterNotice({ onOpen }: { onOpen?: () => void }) {
  const title = 'Run continued in a fresh session'
  const message =
    'Xero handed this conversation off to a new same-type run because the context budget was full. Your task, prior decisions, and important context carried over — keep replying as normal.'
  if (onOpen) {
    return (
      <button
        type="button"
        onClick={onOpen}
        aria-label={`${title} — view handoff context`}
        className="agent-handoff-notice group flex w-full items-start gap-2 rounded-lg border border-border/50 bg-muted/30 px-3 py-2 text-left text-foreground hover:bg-muted/45 hover:border-primary/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Info
          className="mt-[2px] h-3.5 w-3.5 shrink-0 text-primary/80"
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <p className="m-0 text-[13px] font-medium">{title}</p>
          <p className="mt-0.5 whitespace-pre-wrap break-words text-[12.5px] leading-relaxed">
            {message}
          </p>
          <p className="mt-1 inline-flex items-center gap-1 text-[11px] font-medium text-foreground/80">
            See what carried over
            <span
              aria-hidden="true"
              className="inline-block transition-transform duration-200 ease-out group-hover:translate-x-0.5 group-focus-visible:translate-x-0.5"
            >
              →
            </span>
          </p>
        </div>
      </button>
    )
  }
  return <NoticeRow tone="info" title={title} message={message} code={null} />
}

interface NoticeRowProps {
  tone: 'info' | 'warning' | 'destructive'
  title: string
  message: string
  code: string | null
}

function NoticeRow({ tone, title, message, code }: NoticeRowProps) {
  const toneStyles =
    tone === 'destructive'
      ? {
          card: 'border-destructive/30 bg-destructive/8 text-destructive',
          icon: 'text-destructive',
          codeText: 'text-destructive/70',
        }
      : tone === 'warning'
        ? {
            card: 'border-warning/30 bg-warning/8 text-foreground',
            icon: 'text-warning',
            codeText: 'text-muted-foreground',
          }
        : {
            card: 'border-border/50 bg-muted/30 text-foreground',
            icon: 'text-primary/80',
            codeText: 'text-muted-foreground',
          }

  const Icon =
    tone === 'destructive'
      ? AlertTriangle
      : tone === 'warning'
        ? AlertCircle
        : Info

  return (
    <div
      className={cn(
        'flex items-start gap-2 rounded-lg border px-3 py-2',
        toneStyles.card,
      )}
    >
      <Icon className={cn('mt-[2px] h-3.5 w-3.5 shrink-0', toneStyles.icon)} />
      <div className="min-w-0 flex-1">
        <p className="m-0 text-[13px] font-medium">{title}</p>
        <p className="mt-0.5 whitespace-pre-wrap break-words text-[12.5px] leading-relaxed">
          {message}
        </p>
        {code ? (
          <p
            className={cn(
              'mt-1 break-words font-mono text-[10.5px]',
              toneStyles.codeText,
            )}
          >
            code: {code}
          </p>
        ) : null}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Failure card (inline conversation turn).
// ---------------------------------------------------------------------------

function FailureCard({ message, code }: { message: string; code: string }) {
  const presentation = failurePresentation(message, code)
  const warning = presentation.tone === 'warning'
  const Icon = warning ? AlertCircle : XCircle
  return (
    <div
      className={cn(
        'flex items-start gap-2 rounded-lg border px-3 py-2',
        warning
          ? 'border-warning/30 bg-warning/8 text-foreground'
          : 'agent-failure-shake border-destructive/30 bg-destructive/8 text-destructive',
      )}
    >
      <Icon
        className={cn(
          'mt-[2px] h-3.5 w-3.5 shrink-0',
          warning ? 'text-warning' : 'text-destructive',
        )}
      />
      <div className="min-w-0 flex-1">
        <p className="m-0 text-[13px] font-medium">{presentation.title}</p>
        <p className="mt-0.5 whitespace-pre-wrap break-words text-[12.5px] leading-relaxed">
          {presentation.message}
        </p>
        <p
          className={cn(
            'mt-1 break-words font-mono text-[10.5px]',
            warning ? 'text-muted-foreground' : 'text-destructive/70',
          )}
        >
          code: {code}
        </p>
      </div>
    </div>
  )
}

function failurePresentation(
  message: string,
  code: string,
): {
  tone: 'warning' | 'destructive'
  title: string
  message: string
} {
  if (code === 'browser_not_open') {
    return {
      tone: 'warning',
      title: 'In-app browser not open',
      message:
        'The in-app browser is not open yet. Open the built-in browser, or continue from the visible desktop or another open app.',
    }
  }
  if (code === 'autonomous_tool_edit_expected_text_mismatch') {
    return {
      tone: 'destructive',
      title: 'Edit could not be applied',
      message:
        'The edit tool could not apply the patch because the exact line snapshot did not match. The agent should reread the file and retry against the current contents.',
    }
  }
  return {
    tone: 'destructive',
    title: 'Agent run failed',
    message: compactFailureMessage(message),
  }
}

function compactFailureMessage(message: string): string {
  const normalized = message.trim()
  if (normalized.length <= 900) return normalized

  const firstLine = normalized.split(/\r?\n/, 1)[0]?.trim()
  if (firstLine) {
    return `${firstLine}\n\nDetailed diagnostics are saved with the run.`
  }

  return `${normalized.slice(0, 900).trimEnd()}\n\nDetailed diagnostics are saved with the run.`
}

function normalizeFailureMessage(message: string | null | undefined): string {
  return (message ?? '').trim().replace(/\s+/g, ' ')
}

function failureMessagesMatch(
  left: string | null | undefined,
  right: string | null | undefined,
): boolean {
  const normalizedLeft = normalizeFailureMessage(left)
  const normalizedRight = normalizeFailureMessage(right)
  return Boolean(
    normalizedLeft &&
      normalizedRight &&
      (normalizedLeft === normalizedRight ||
        normalizedLeft.includes(normalizedRight) ||
        normalizedRight.includes(normalizedLeft)),
  )
}

function failureDiagnosticsMatch(
  leftMessage: string | null | undefined,
  leftCode: string | null | undefined,
  rightMessage: string | null | undefined,
  rightCode: string | null | undefined,
): boolean {
  return Boolean(
    (leftCode && rightCode && leftCode === rightCode) ||
      failureMessagesMatch(leftMessage, rightMessage),
  )
}

// ---------------------------------------------------------------------------
// Avatars and small leading icons.
// ---------------------------------------------------------------------------

interface UserAvatarProps {
  avatarUrl: string | null
  login: string | null
}

function UserAvatar({ avatarUrl, login }: UserAvatarProps) {
  const [failed, setFailed] = useState(false)
  const showImage = Boolean(avatarUrl) && !failed

  return (
    <span
      className={cn(
        'mt-[2px] flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden rounded-full',
        showImage
          ? 'ring-1 ring-primary/40'
          : 'bg-primary/15 text-primary ring-1 ring-primary/40',
      )}
    >
      {showImage ? (
        <img
          src={avatarUrl ?? undefined}
          alt={login ? `${login}'s avatar` : ''}
          className="h-full w-full object-cover"
          onError={() => setFailed(true)}
        />
      ) : (
        <User className="h-3 w-3" aria-hidden="true" />
      )}
    </span>
  )
}

function AgentAvatar({ pulse = false }: { pulse?: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'mt-[2px] relative flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-card/80 ring-1 ring-primary/40',
        pulse && 'agent-avatar-pulse',
      )}
    >
      <AppLogo className="h-3 w-3" />
    </span>
  )
}

interface ToolStatusIconProps {
  state: RuntimeStreamToolItemView['toolState'] | null
  className?: string
}

function ToolStatusIcon({ state, className }: ToolStatusIconProps) {
  const key = state ?? 'pending'
  const Icon =
    state === 'running'
      ? Loader2
      : state === 'failed'
        ? XCircle
        : state === 'succeeded'
          ? CheckCircle2
          : Circle

  const tone =
    state === 'running'
      ? 'text-primary'
      : state === 'failed'
        ? 'text-destructive'
        : state === 'succeeded'
          ? 'text-success'
          : 'text-muted-foreground/55'

  const pop = state === 'succeeded' || state === 'failed'
  const iconSize =
    state === 'pending' || state === null ? 'h-3.5 w-3.5' : 'h-4 w-4'
  // Replays a soft halo only when the tool finishes — keyed on `state` so a
  // re-render with the same terminal state doesn't restart the animation,
  // but a transition (running → succeeded) does.
  const flashTone =
    state === 'succeeded' ? 'success' : state === 'failed' ? 'failure' : null

  return (
    <span
      aria-hidden="true"
      className={cn(
        'relative inline-flex items-center justify-center shrink-0',
        state === 'running' && 'animate-spin',
        className,
      )}
    >
      <Icon
        key={key}
        aria-hidden="true"
        className={cn(
          iconSize,
          'shrink-0',
          tone,
          state === 'succeeded' && 'drop-shadow-[0_0_4px_rgba(34,197,94,0.25)]',
          'motion-safe:animate-in motion-safe:fade-in-0',
          pop
            ? 'tool-status-icon-pop'
            : 'motion-safe:zoom-in-95 motion-safe:duration-150',
        )}
      />
      {flashTone ? (
        <span
          key={`${key}-flash`}
          className="agent-tool-flash"
          data-tone={flashTone}
        />
      ) : null}
    </span>
  )
}

function truncateForLine(text: string, max = 240): string {
  const collapsed = text.replace(/\s+/g, ' ').trim()
  if (collapsed.length <= max) return collapsed
  return `${collapsed.slice(0, max - 1)}…`
}

interface DenseTurnItemProps {
  turn: ConversationTurn
  codeUndoStates: Record<string, CodeUndoUiState>
  returnSessionToHereStates: Record<string, CodeUndoUiState>
  currentAgentLabel?: string | null
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
  onOpenHandoffSummary?: (request: {
    sourceRunId?: string | null
    targetRunId?: string | null
  }) => void
}

function DenseTurnItem({
  turn,
  codeUndoStates,
  returnSessionToHereStates,
  onUndoChangeGroup,
  onReturnSessionToHere,
  onOpenHandoffSummary,
}: DenseTurnItemProps) {
  if (turn.kind === 'message') {
    return (
      <DenseMessageItem
        id={turn.id}
        role={turn.role}
        text={turn.text}
        attachments={turn.attachments}
      />
    )
  }

  if (turn.kind === 'thinking') {
    return <DenseThinkingItem id={turn.id} text={turn.text} />
  }

  if (turn.kind === 'failure') {
    return (
      <li className="flex items-start gap-1.5 px-1 text-destructive">
        <span className="shrink-0 select-none">✗</span>
        <span
          className="min-w-0 flex-1 truncate"
          title={`${turn.code}: ${turn.message}`}
        >
          {truncateForLine(turn.message)}
        </span>
      </li>
    )
  }

  if (turn.kind === 'file_change') {
    const fileUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'file_change',
            changeGroupId: turn.changeGroupId,
            filePath: fileChangeUndoFilePath(turn),
          })
        ]
      : undefined
    const changeGroupUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'change_group',
            changeGroupId: turn.changeGroupId,
          })
        ]
      : undefined
    const hunkUndoState = turn.changeGroupId
      ? codeUndoStates[
          getCodeUndoStateKey({
            targetKind: 'hunks',
            changeGroupId: turn.changeGroupId,
            filePath: fileChangeUndoFilePath(turn),
          })
        ]
      : undefined
    const returnSessionState = turn.changeGroupId
      ? returnSessionToHereStates[
          getReturnSessionToHereStateKey({
            targetKind: 'run_boundary',
            boundaryId: codeChangeGroupBoundaryId(turn.changeGroupId),
            runId: turn.runId,
            changeGroupId: turn.changeGroupId,
          })
        ]
      : undefined

    return (
      <DenseFileChangeItem
        turn={turn}
        fileUndoState={fileUndoState}
        changeGroupUndoState={changeGroupUndoState}
        hunkUndoState={hunkUndoState}
        returnSessionState={returnSessionState}
        onUndoChangeGroup={onUndoChangeGroup}
        onReturnSessionToHere={onReturnSessionToHere}
      />
    )
  }

  if (turn.kind === 'action') {
    return (
      <DenseActionItem
        title={turn.title}
        detail={turn.detail}
        detailRows={turn.detailRows}
        mediaAttachments={turn.mediaAttachments}
        state={turn.state ?? null}
      />
    )
  }

  if (turn.kind === 'action_group') {
    return (
      <DenseActionGroupItem
        title={turn.title}
        detail={turn.detail}
        state={turn.state ?? null}
        actions={turn.actions}
      />
    )
  }

  if (turn.kind === 'action_prompt') {
    return (
      <li className="flex items-start gap-1.5 px-1 text-foreground/90">
        <span className="shrink-0 select-none text-primary/80">?</span>
        <span
          className="min-w-0 flex-1 truncate"
          title={`${turn.title}: ${turn.detail}`}
        >
          {truncateForLine(turn.title)}
        </span>
      </li>
    )
  }

  if (turn.kind === 'handoff_notice') {
    const handler = onOpenHandoffSummary
    if (handler) {
      return (
        <li>
          <button
            type="button"
            onClick={() =>
              handler({
                sourceRunId: turn.sourceRunId,
                targetRunId: turn.targetRunId,
              })
            }
            aria-label="Run continued in a fresh session — view handoff context"
            className="flex w-full items-start gap-1.5 px-1 text-left text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <span className="shrink-0 select-none">⤳</span>
            <span
              className="min-w-0 flex-1 truncate"
              title="Run continued in a fresh session"
            >
              handed off to a fresh same-type session
            </span>
          </button>
        </li>
      )
    }
    return (
      <li className="flex items-start gap-1.5 px-1 text-muted-foreground">
        <span className="shrink-0 select-none">⤳</span>
        <span
          className="min-w-0 flex-1 truncate"
          title="Run continued in a fresh session"
        >
          handed off to a fresh same-type session
        </span>
      </li>
    )
  }

  if (turn.kind === 'routing_suggestion') {
    const label = turn.targetLabel ?? turn.targetAgentDefinitionId ?? turn.targetAgentId
    return (
      <li className="flex items-start gap-1.5 px-1 text-muted-foreground">
        <span className="shrink-0 select-none">↪</span>
        <span
          className="min-w-0 flex-1 truncate"
          title={`Routing suggestion → ${label}`}
        >
          suggested routing to {label}
        </span>
      </li>
    )
  }

  return null
}

function DenseFileChangeItem({
  turn,
  fileUndoState,
  changeGroupUndoState,
  hunkUndoState,
  returnSessionState,
  onUndoChangeGroup,
  onReturnSessionToHere,
}: {
  turn: Extract<ConversationTurn, { kind: 'file_change' }>
  fileUndoState?: CodeUndoUiState
  changeGroupUndoState?: CodeUndoUiState
  hunkUndoState?: CodeUndoUiState
  returnSessionState?: CodeUndoUiState
  onUndoChangeGroup?: (request: CodeUndoRequest) => void
  onReturnSessionToHere?: (request: ReturnSessionToHereUiRequest) => void
}) {
  const targetLabel = fileChangeTargetLabel(turn)
  const undoFilePath = fileChangeUndoFilePath(turn)
  const canUndo = Boolean(turn.changeGroupId && onUndoChangeGroup)
  const canReturnSessionToHere = Boolean(
    turn.changeGroupId && turn.runId && onReturnSessionToHere,
  )
  const textHunks = fileChangeTextHunks(turn)
  const undoState = visibleCodeUndoState(
    fileUndoState,
    changeGroupUndoState,
    hunkUndoState,
    returnSessionState,
  )
  const statusTone =
    undoState?.status === 'failed'
      ? 'text-destructive'
      : undoState?.status === 'succeeded'
        ? 'text-success'
        : 'text-muted-foreground'

  return (
    <li className="px-1">
      <div className="flex items-start gap-2 text-foreground/85">
        <FileText
          className="mt-[2px] h-3 w-3 shrink-0 text-primary/70"
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <span className="block truncate" title={turn.detail}>
            {truncateForLine(
              `${humanizeFileChangeOperation(turn.operation)} ${targetLabel}`,
            )}
          </span>
          {undoState ? (
            <>
              <output
                className={cn('mt-0.5 block truncate text-[11px]', statusTone)}
                aria-live="polite"
              >
                {undoState.message}
              </output>
              <CodeUndoConflictDetails
                summary={undoState.conflictSummary}
                dense
              />
            </>
          ) : null}
        </div>
        <CodeUndoMenu
          path={targetLabel}
          filePath={undoFilePath}
          changeGroupId={turn.changeGroupId}
          expectedWorkspaceEpoch={turn.workspaceEpoch}
          fileState={fileUndoState}
          changeGroupState={changeGroupUndoState}
          hunkState={hunkUndoState}
          returnSessionState={returnSessionState}
          textHunks={textHunks}
          enabled={canUndo}
          canReturnSessionToHere={canReturnSessionToHere}
          runId={turn.runId}
          onUndoChangeGroup={onUndoChangeGroup}
          onReturnSessionToHere={onReturnSessionToHere}
          dense
        />
      </div>
    </li>
  )
}

interface DenseMessageItemProps {
  id: string
  role: 'user' | 'assistant'
  text: string
  attachments?: ConversationMessageAttachment[]
}

function DenseMessageItem({
  id,
  role,
  text,
  attachments,
}: DenseMessageItemProps) {
  const isUser = role === 'user'
  const [open, setOpen] = useState(!isUser)
  const marker = isUser ? '>' : '◆'
  const tone = isUser ? 'text-primary/85' : 'text-foreground/90'
  const promptParts = useMemo(
    () => (isUser ? splitBrowserToolPromptContext(text) : null),
    [isUser, text],
  )
  const displayText = promptParts?.visibleText ?? text
  const promptContexts = promptParts?.contexts ?? []
  const promptContextAttachments = useMemo(
    () => pairBrowserToolContextAttachments(promptContexts, attachments),
    [attachments, promptContexts],
  )
  const visibleAttachments = promptContextAttachments.unpairedAttachments
  const normalized = displayText.trim()
  const hasAttachments = Boolean(visibleAttachments && visibleAttachments.length > 0)
  const hasMore =
    normalized.length > 240 ||
    /\r?\n/.test(normalized) ||
    hasAttachments ||
    promptContexts.length > 0
  const summaryText =
    normalized || (promptContexts.length > 0 ? promptContexts[0]?.title ?? 'Prompt context' : '')

  return (
    <li
      className="px-1"
      data-conversation-turn-id={id}
      data-conversation-turn-kind="message"
      data-conversation-turn-role={role}
    >
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={hasMore ? open : undefined}
        aria-label={
          hasMore ? `${open ? 'Hide' : 'Show'} full message` : undefined
        }
        disabled={!hasMore}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasMore ? 'cursor-pointer hover:text-foreground' : 'cursor-default',
          'focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <span className={cn('shrink-0 select-none font-semibold', tone)}>
          {marker}
        </span>
        <span
          className="min-w-0 flex-1 truncate text-foreground/85"
          title={hasMore && !open ? summaryText : undefined}
        >
          {truncateForLine(summaryText)}
        </span>
        {hasMore ? (
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'mt-[3px] h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform duration-150',
              open ? 'rotate-180' : 'rotate-0',
            )}
          />
        ) : null}
      </button>
      {open && hasMore ? (
        <div
          className={cn(
            'ml-3 mt-1.5 border-l border-border/30 pl-2.5',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150',
          )}
        >
          {isUser ? (
            normalized.length > 0 ? (
            <p className="m-0 whitespace-pre-wrap break-words text-[12px] text-foreground/85">
              {normalized}
            </p>
            ) : null
          ) : (
            <Markdown messageId={`${id}:dense`} text={text} scale="dense" />
          )}
          {promptContexts.length > 0 ? (
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {promptContexts.map((context) => (
                <PromptContextCard
                  key={`${id}:${context.id}`}
                  context={context}
                  attachment={promptContextAttachments.pairedByContextId.get(context.id)}
                />
              ))}
            </div>
          ) : null}
          {hasAttachments ? (
            <ul className="mt-1.5 flex flex-wrap gap-1 text-[11px] text-muted-foreground/80">
              {visibleAttachments?.map((attachment) => (
                <li
                  key={attachment.id}
                  className="rounded-sm border border-border/40 bg-muted/20 px-1.5 py-0.5"
                  title={attachment.originalName}
                >
                  {attachment.originalName}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </li>
  )
}

function DenseThinkingItem({ id, text }: { id: string; text: string }) {
  const [open, setOpen] = useState(false)
  const normalized = text.trim()
  const hasMore = normalized.length > 240 || /\r?\n/.test(normalized)
  return (
    <li className="px-1 text-muted-foreground/80">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        aria-label={`${open ? 'Hide' : 'Show'} reasoning`}
        disabled={!hasMore}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasMore
            ? 'cursor-pointer hover:text-foreground/90'
            : 'cursor-default',
          'focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <span className="shrink-0 select-none">~</span>
        <span
          className="min-w-0 flex-1 truncate"
          title={hasMore && !open ? text : undefined}
        >
          {truncateForLine(text)}
        </span>
        {hasMore ? (
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'mt-[3px] h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform duration-150',
              open ? 'rotate-180' : 'rotate-0',
            )}
          />
        ) : null}
      </button>
      {open && hasMore ? (
        <div
          className={cn(
            'ml-3 mt-1.5 border-l border-border/30 pl-2.5 text-muted-foreground/85',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150',
          )}
        >
          <Markdown messageId={`${id}:dense`} text={text} muted scale="dense" />
        </div>
      ) : null}
    </li>
  )
}

interface DenseActionItemProps {
  title: string
  detail: string
  detailRows: Array<{ label: string; value: string }>
  mediaAttachments?: ConversationMessageAttachment[]
  state: RuntimeStreamToolItemView['toolState'] | null
}

function DenseActionItem({
  title,
  detail,
  detailRows,
  mediaAttachments,
  state,
}: DenseActionItemProps) {
  const [open, setOpen] = useState(false)
  const hasDetails =
    detailRows.length > 0 ||
    detail.trim().length > 0 ||
    Boolean(mediaAttachments?.length)

  return (
    <AnimatedTranscriptListItem className="px-1">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={hasDetails ? open : undefined}
        aria-label={
          hasDetails
            ? `${open ? 'Hide' : 'Show'} tool details for ${title}`
            : undefined
        }
        disabled={!hasDetails}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasDetails
            ? 'cursor-pointer hover:text-foreground'
            : 'cursor-default',
          'focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <ToolStatusIcon state={state} className="mt-[1px]" />
        <span
          className="min-w-0 flex-1 truncate text-foreground/85"
          title={`${title}${detail ? ` — ${detail}` : ''}`}
        >
          {truncateForLine(title)}
        </span>
        {hasDetails ? (
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'mt-[3px] h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform duration-150',
              open ? 'rotate-180' : 'rotate-0',
            )}
          />
        ) : null}
      </button>
      <AnimatePresence initial={false}>
        {open && hasDetails ? (
          <AnimatedTranscriptPanel>
            <div className="ml-3 mt-1.5 border-l border-border/30 pl-2.5">
              <DenseToolDetails
                detail={detail}
                detailRows={detailRows}
                mediaAttachments={mediaAttachments}
              />
            </div>
          </AnimatedTranscriptPanel>
        ) : null}
      </AnimatePresence>
    </AnimatedTranscriptListItem>
  )
}

interface DenseActionGroupItemProps {
  title: string
  detail: string
  state: RuntimeStreamToolItemView['toolState'] | null
  actions: Array<{
    id: string
    title: string
    detail: string
    detailRows: Array<{ label: string; value: string }>
    mediaAttachments?: ConversationMessageAttachment[]
    state: RuntimeStreamToolItemView['toolState'] | null
  }>
}

function DenseActionGroupItem({
  title,
  detail,
  state,
  actions,
}: DenseActionGroupItemProps) {
  const [open, setOpen] = useState(false)
  const singleAction = actions.length === 1 ? actions[0] : null
  const singleActionHasDetails = Boolean(
    singleAction &&
      (singleAction.detailRows.length > 0 ||
        singleAction.detail.trim().length > 0 ||
        Boolean(singleAction.mediaAttachments?.length)),
  )
  const hasExpandableContent = actions.length > 1 || singleActionHasDetails
  const detailsLabel =
    actions.length === 1 ? 'compact tool details' : 'grouped tool details'
  const rowClassName = cn(
    'flex w-full items-start gap-2 text-left',
    hasExpandableContent
      ? 'cursor-pointer hover:text-foreground'
      : 'cursor-default',
    'focus-visible:outline-none focus-visible:text-foreground',
  )
  const rowContent = (
    <>
      <ToolStatusIcon state={state} className="mt-[1px]" />
      <span
        className="min-w-0 flex-1 truncate text-foreground/85"
        title={`${title}${detail ? ` — ${detail}` : ''}`}
      >
        {truncateForLine(title)}
      </span>
      {hasExpandableContent ? (
        <ChevronDown
          aria-hidden="true"
          className={cn(
            'mt-[3px] h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform duration-150',
            open ? 'rotate-180' : 'rotate-0',
          )}
        />
      ) : null}
    </>
  )

  return (
    <AnimatedTranscriptListItem className="px-1">
      {hasExpandableContent ? (
        <button
          type="button"
          onClick={() => setOpen((prev) => !prev)}
          aria-expanded={open}
          aria-label={`${open ? 'Hide' : 'Show'} ${detailsLabel} for ${title}`}
          className={rowClassName}
        >
          {rowContent}
        </button>
      ) : (
        <div className={rowClassName}>{rowContent}</div>
      )}
      <AnimatePresence initial={false}>
        {open && hasExpandableContent ? (
          <AnimatedTranscriptPanel>
            {singleAction ? (
              <div className="ml-3 mt-1.5 border-l border-border/30 pl-2.5">
                <DenseToolDetails
                  detail={singleAction.detail}
                  detailRows={singleAction.detailRows}
                  mediaAttachments={singleAction.mediaAttachments}
                />
              </div>
            ) : (
              <ol className="ml-3 mt-1.5 flex flex-col gap-1.5 border-l border-border/30 pl-2.5">
                <AnimatePresence initial={false}>
                  {actions.map((action) => (
                    <DenseActionItem
                      key={action.id}
                      title={action.title}
                      detail={action.detail}
                      detailRows={action.detailRows}
                      mediaAttachments={action.mediaAttachments}
                      state={action.state}
                    />
                  ))}
                </AnimatePresence>
              </ol>
            )}
          </AnimatedTranscriptPanel>
        ) : null}
      </AnimatePresence>
    </AnimatedTranscriptListItem>
  )
}

function DenseToolDetails({
  detail,
  detailRows,
  mediaAttachments,
}: {
  detail: string
  detailRows: Array<{ label: string; value: string }>
  mediaAttachments?: ConversationMessageAttachment[]
}) {
  const trimmedDetail = detail.trim()
  const hasMedia = Boolean(mediaAttachments?.length)
  const outputRow = detailRows.find((row) => /output/i.test(row.label))
  const fallbackRow =
    outputRow ??
    detailRows.find((row) => /result/i.test(row.label)) ??
    detailRows.find((row) => /input|outcome|cmd|command/i.test(row.label)) ??
    detailRows[0] ??
    null

  if (!fallbackRow && trimmedDetail.length === 0 && !hasMedia) return null

  return (
    <div className="flex flex-col gap-1.5 text-[11.5px]">
      {hasMedia ? (
        <ul className="flex flex-wrap gap-1 text-[11px] text-muted-foreground/80">
          {mediaAttachments?.map((attachment) => (
            <li
              key={attachment.id}
              className="inline-flex max-w-[220px] items-center gap-1 rounded-sm border border-border/40 bg-muted/20 px-1.5 py-0.5"
              title={attachmentDisplayName(attachment)}
            >
              <ImageIcon className="h-3 w-3 shrink-0" aria-hidden="true" />
              <span className="truncate">
                {attachmentDisplayName(attachment)}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      {trimmedDetail.length > 0 ? (
        <p className="m-0 whitespace-pre-wrap break-words text-muted-foreground/85">
          {trimmedDetail}
        </p>
      ) : null}
      {fallbackRow ? (
        <pre
          className={cn(
            'm-0 max-h-[260px] overflow-auto whitespace-pre-wrap break-words rounded-sm',
            'bg-background/40 px-2 py-1.5 font-mono text-[11.5px] leading-snug text-foreground/85 scrollbar-thin',
          )}
        >
          {fallbackRow.value}
        </pre>
      ) : null}
    </div>
  )
}
