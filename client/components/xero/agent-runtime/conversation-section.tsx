/**
 * Agent conversation panel.
 *
 * Renders user / assistant transcripts as polished message rows with
 * avatars, role labels, and (for assistant content) markdown + code
 * highlighting. Tool/action items render as inline activity rows and
 * failures get a distinct destructive treatment.
 *
 * The component preserves the public ARIA surface (`Agent conversation`
 * landmark, `Agent conversation turns` list) so existing tests keep
 * passing.
 */

import { memo, useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  AlertTriangle,
  Brain,
  Check,
  CheckCircle2,
  ChevronDown,
  Circle,
  Copy,
  FileText,
  Info,
  Loader2,
  Terminal,
  Undo2,
  User,
  XCircle,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import type {
  RuntimeActionAnswerShapeDto,
  RuntimeActionRequiredOptionDto,
  RuntimeRunView,
  RuntimeStreamCompleteItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamIssueView,
  RuntimeStreamToolItemView,
} from '@/src/lib/xero-model'

import { AppLogo } from '../app-logo'
import { ActionPromptCard } from './action-prompt-card'
import { Markdown } from './conversation-markdown'

export interface ConversationMessageAttachment {
  id: string
  kind: 'image' | 'document' | 'text'
  mediaType: string
  originalName: string
  sizeBytes: number
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
        title: string
        detail: string
        detailRows: Array<{ label: string; value: string }>
        state: RuntimeStreamToolItemView['toolState'] | null
      }>
    }
  | {
      id: string
      kind: 'file_change'
      sequence: number
      title: string
      detail: string
      operation: string
      path: string
      toPath: string | null
      changeGroupId: string | null
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
      actionType: string
      title: string
      detail: string
      shape: RuntimeActionAnswerShapeDto
      options: RuntimeActionRequiredOptionDto[] | null
      allowMultiple: boolean
      pendingDecision: 'approve' | 'reject' | 'resume' | null
      isResolved: boolean
    }

export interface CodeRollbackRequest {
  changeGroupId: string
  path: string
}

export interface CodeRollbackUiState {
  status: 'pending' | 'succeeded' | 'failed'
  message: string
}

interface ConversationSectionProps {
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
  /** Visual density. `dense` collapses each turn into a single PTY-style line. */
  variant?: 'default' | 'dense'
  codeRollbackStates?: Record<string, CodeRollbackUiState>
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}

/**
 * Stable detail-text marker emitted by the runtime when a same-type handoff
 * completes a source run. The runtime emits the matching summary in
 * `mark_source_run_handed_off` (see `runtime/agent_core/run.rs`); changing the
 * sentence on either side requires updating both for the user-facing notice.
 */
const HANDOFF_COMPLETION_DETAIL_MARKER = 'handed off to a same-type target run'

function isHandoffCompletion(
  completion: RuntimeStreamCompleteItemView | null | undefined,
): boolean {
  return Boolean(completion?.detail?.toLowerCase().includes(HANDOFF_COMPLETION_DETAIL_MARKER))
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
  variant = 'default',
  codeRollbackStates = {},
  onRollbackChangeGroup,
}: ConversationSectionProps) {
  const runFailureCode = runtimeRun?.lastError?.code ?? runtimeRun?.lastErrorCode ?? null
  const runFailureMessage =
    runtimeRun?.lastError?.message ??
    (runtimeRun?.isFailed ? 'Xero recovered a failed agent run without a persisted diagnostic.' : null)
  const streamFailureIsDuplicate =
    Boolean(streamFailure?.message && streamFailure.message === runFailureMessage) ||
    Boolean(streamFailure?.code && streamFailure.code === runFailureCode)
  const streamIssueIsDuplicate =
    Boolean(
      streamIssue?.message &&
        (streamIssue.message === runFailureMessage || streamIssue.message === streamFailure?.message),
    ) ||
    Boolean(streamIssue?.code && (streamIssue.code === runFailureCode || streamIssue.code === streamFailure?.code))

  const showRunFailure = Boolean(runFailureMessage)
  const showStreamFailure = Boolean(streamFailure && !streamFailureIsDuplicate)
  const showStreamIssue = Boolean(streamIssue && !streamIssueIsDuplicate)
  const showHandoffNotice = !showRunFailure && isHandoffCompletion(streamCompletion)
  const showAnyNotice = showRunFailure || showStreamFailure || showStreamIssue || showHandoffNotice
  const showAnyTurn = visibleTurns.length > 0
  const copyableConversationText = useMemo(
    () => formatConversationForCopy(visibleTurns),
    [visibleTurns],
  )

  const lastTurn = visibleTurns.length > 0 ? visibleTurns[visibleTurns.length - 1] : null
  const isLastTurnStreamingAssistant = Boolean(
    showActivityIndicator &&
      lastTurn &&
      lastTurn.kind === 'message' &&
      lastTurn.role === 'assistant' &&
      lastTurn.text.trim().length > 0,
  )

  if (variant === 'dense') {
    return (
      <section
        aria-label="Agent conversation"
        className="flex flex-col gap-2 text-[12px] leading-snug select-text"
      >
        {showAnyTurn ? (
          <ol aria-label="Agent conversation turns" className="flex flex-col gap-2">
            {visibleTurns.map((turn) => (
              <DenseTurnItem
                key={turn.id}
                turn={turn}
                codeRollbackStates={codeRollbackStates}
                onRollbackChangeGroup={onRollbackChangeGroup}
              />
            ))}
          </ol>
        ) : null}
        {showActivityIndicator ? (
          <div className="flex items-center gap-1.5 px-1 text-[12px] text-muted-foreground">
            <Loader2 className="h-2.5 w-2.5 animate-spin text-primary/70" />
            <span>thinking…</span>
          </div>
        ) : null}
        {showAnyNotice ? (
          <ul aria-label="Agent run notices" className="mt-2 flex flex-col gap-2">
            {showHandoffNotice ? (
              <li className="rounded-sm border border-border/40 bg-muted/15 px-2 py-1 text-[12px] text-muted-foreground">
                ⤳ handed off to a fresh same-type session
              </li>
            ) : null}
            {showRunFailure ? (
              <li className="rounded-sm border border-destructive/30 bg-destructive/10 px-2 py-1 text-[12px] text-destructive">
                ✗ {runFailureMessage}
              </li>
            ) : null}
            {showStreamFailure && streamFailure ? (
              <li className="rounded-sm border border-destructive/30 bg-destructive/10 px-2 py-1 text-[12px] text-destructive">
                ✗ {describeStreamMessage(streamFailure.code, streamFailure.message)}
              </li>
            ) : null}
            {showStreamIssue && streamIssue ? (
              <li className="rounded-sm border border-border/40 bg-muted/20 px-2 py-1 text-[12px] text-muted-foreground">
                ⚠ {describeStreamMessage(streamIssue.code, streamIssue.message)}
              </li>
            ) : null}
          </ul>
        ) : null}
        {copyableConversationText.length > 0 ? (
          <div className="mt-2 flex justify-end">
            <CopyTextButton
              text={copyableConversationText}
              label="Copy visible conversation"
              copiedLabel="Copied visible conversation"
              tooltip="Copy conversation"
              className="h-6 w-6 rounded-full text-muted-foreground/75 hover:text-foreground"
            />
          </div>
        ) : null}
      </section>
    )
  }

  return (
    <section aria-label="Agent conversation" className="flex flex-col gap-5 select-text">
      {showAnyTurn ? (
        <ol aria-label="Agent conversation turns" className="flex flex-col gap-5">
          {visibleTurns.map((turn, index) => {
            const prev = index > 0 ? visibleTurns[index - 1] : null
            const next = index < visibleTurns.length - 1 ? visibleTurns[index + 1] : null
            return (
              <ConversationTurnItem
                key={turn.id}
                turn={turn}
                accountAvatarUrl={accountAvatarUrl}
                accountLogin={accountLogin}
                isStreaming={index === visibleTurns.length - 1 && isLastTurnStreamingAssistant}
                connectsTop={isToolTurnKind(prev)}
                connectsBottom={isToolTurnKind(next)}
                codeRollbackStates={codeRollbackStates}
                onRollbackChangeGroup={onRollbackChangeGroup}
              />
            )
          })}
        </ol>
      ) : null}

      {showActivityIndicator && !isLastTurnStreamingAssistant ? (
        <AgentActivityIndicator />
      ) : null}

      {showAnyNotice ? (
        <ul aria-label="Agent run notices" className="flex flex-col gap-2.5">
          {showHandoffNotice ? (
            <NoticeListItem>
              <NoticeRow
                tone="info"
                title="Run continued in a fresh session"
                message="Xero handed this conversation off to a new same-type run because the context budget was full. Your task, prior decisions, and important context carried over — keep replying as normal."
                code={null}
              />
            </NoticeListItem>
          ) : null}
          {showRunFailure ? (
            <NoticeListItem>
              <NoticeRow
                tone="destructive"
                title={runtimeRun?.isTerminal ? 'Latest saved run failed' : 'Agent run failed'}
                message={runFailureMessage ?? ''}
                code={runFailureCode}
              />
            </NoticeListItem>
          ) : null}
          {showStreamFailure && streamFailure ? (
            <NoticeListItem>
              <NoticeRow
                tone="destructive"
                title="Live stream failed"
                message={describeStreamMessage(streamFailure.code, streamFailure.message)}
                code={streamFailure.code}
              />
            </NoticeListItem>
          ) : null}
          {showStreamIssue && streamIssue ? (
            <NoticeListItem>
              <NoticeRow
                tone={streamIssue.retryable ? 'info' : 'warning'}
                title={describeStreamTitle(streamIssue.code, 'Live stream issue')}
                message={describeStreamMessage(streamIssue.code, streamIssue.message)}
                code={streamIssue.code}
              />
            </NoticeListItem>
          ) : null}
        </ul>
      ) : null}

      {copyableConversationText.length > 0 ? (
        <div className="flex justify-end pt-1">
          <CopyTextButton
            text={copyableConversationText}
            label="Copy visible conversation"
            copiedLabel="Copied visible conversation"
            tooltip="Copy conversation"
            className={cn(
              'h-5 w-5 rounded-full text-muted-foreground/55 hover:text-foreground',
              'opacity-50 hover:opacity-100',
            )}
          />
        </div>
      ) : null}
    </section>
  )
})

function formatConversationForCopy(turns: readonly ConversationTurn[]): string {
  return turns
    .map((turn) => {
      switch (turn.kind) {
        case 'message': {
          if (turn.role === 'assistant') {
            return splitAssistantText(turn.text)
              .map((segment) => {
                const segmentText = segment.text.trim()
                if (segmentText.length === 0) return ''
                return `${segment.kind === 'thinking' ? 'Thoughts' : 'Agent'}:\n${segmentText}`
              })
              .filter(Boolean)
              .join('\n\n')
          }
          const parts = [turn.text.trim()]
          if (turn.attachments && turn.attachments.length > 0) {
            const attachmentNames = turn.attachments
              .map((attachment) => attachment.originalName.trim())
              .filter(Boolean)
            if (attachmentNames.length > 0) {
              parts.push(`Attachments: ${attachmentNames.join(', ')}`)
            }
          }
          const body = parts.filter(Boolean).join('\n')
          return body.length > 0 ? `${turn.role === 'user' ? 'You' : 'Agent'}:\n${body}` : ''
        }
        case 'thinking':
          return turn.text.trim().length > 0 ? `Thoughts:\n${turn.text.trim()}` : ''
        case 'action':
          return formatActionForCopy('Tool', turn.title, turn.detail, turn.detailRows)
        case 'action_group': {
          const groupLines = [
            formatActionForCopy('Tools', turn.title, turn.detail, []),
            ...turn.actions.map((action) =>
              formatActionForCopy('Tool', action.title, action.detail, action.detailRows),
            ),
          ]
          return groupLines.filter(Boolean).join('\n\n')
        }
        case 'file_change':
          return formatActionForCopy('File change', turn.title, turn.detail, [])
        case 'failure':
          return `Agent run failed:\n${turn.message}${turn.code.trim().length > 0 ? `\nCode: ${turn.code}` : ''}`
        case 'action_prompt':
          return `Agent prompt:\n${turn.title}${turn.detail.trim().length > 0 ? `\n${turn.detail}` : ''}`
        default:
          return ''
      }
    })
    .filter(Boolean)
    .join('\n\n')
    .trim()
}

function formatActionForCopy(
  prefix: string,
  title: string,
  detail: string,
  detailRows: ReadonlyArray<{ label: string; value: string }>,
): string {
  const lines = [`${prefix}: ${title.trim() || 'activity'}`]
  if (detail.trim().length > 0) {
    lines.push(detail.trim())
  }
  for (const row of detailRows) {
    const label = row.label.trim()
    const value = row.value.trim()
    if (label.length > 0 && value.length > 0) {
      lines.push(`${label}: ${value}`)
    } else if (value.length > 0) {
      lines.push(value)
    }
  }
  return lines.join('\n')
}

interface CopyTextButtonProps {
  text: string
  label: string
  copiedLabel: string
  tooltip: string
  className?: string
}

function CopyTextButton({
  text,
  label,
  copiedLabel,
  tooltip,
  className,
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
      await navigator.clipboard.writeText(text)
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
          size="icon-sm"
          aria-label={copied ? copiedLabel : label}
          onClick={handleCopy}
          className={cn(
            'select-none rounded-md transition-opacity [&_svg]:size-[11px]',
            copied ? 'text-success opacity-100' : null,
            className,
          )}
        >
          <Icon aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">{copied ? 'Copied' : tooltip}</TooltipContent>
    </Tooltip>
  )
}

function NoticeListItem({ children }: { children: React.ReactNode }) {
  return <li className={TURN_ENTRY_CLASS}>{children}</li>
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
  return turn != null && (turn.kind === 'action' || turn.kind === 'action_group')
}

interface ConversationTurnItemProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
  /** Previous visible turn is also a tool call; render a connector up to it. */
  connectsTop: boolean
  /** Next visible turn is also a tool call; render a connector down to it. */
  connectsBottom: boolean
  codeRollbackStates: Record<string, CodeRollbackUiState>
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}

// Custom keyframes (see globals.css `.agent-turn-soft-enter`) give each new
// turn a softer landing — a small upward drift, micro scale, longer ease —
// than tailwind's stock `animate-in fade-in-0 slide-in-from-bottom-1`.
// Reduced motion is honoured globally by the `prefers-reduced-motion` rule
// at the bottom of globals.css.
const TURN_ENTRY_CLASS = 'agent-turn-soft-enter'

function ConversationTurnItem({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
  connectsTop,
  connectsBottom,
  codeRollbackStates,
  onRollbackChangeGroup,
}: ConversationTurnItemProps) {
  return (
    <li className={TURN_ENTRY_CLASS}>
      <ConversationTurnRow
        turn={turn}
        accountAvatarUrl={accountAvatarUrl}
        accountLogin={accountLogin}
        isStreaming={isStreaming}
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
        codeRollbackStates={codeRollbackStates}
        onRollbackChangeGroup={onRollbackChangeGroup}
      />
    </li>
  )
}

interface ConversationTurnRowProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
  connectsTop: boolean
  connectsBottom: boolean
  codeRollbackStates: Record<string, CodeRollbackUiState>
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}

function ConversationTurnRow({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
  connectsTop,
  connectsBottom,
  codeRollbackStates,
  onRollbackChangeGroup,
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
      <AssistantMessage messageId={turn.id} text={turn.text} isStreaming={isStreaming} />
    )
  }

  if (turn.kind === 'thinking') {
    return (
      <div className="flex gap-2.5">
        <AgentAvatar pulse={isStreaming} />
        <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5">
          <span className="sr-only">Agent</span>
          <ThinkingBlock messageId={turn.id} text={turn.text} />
        </div>
      </div>
    )
  }

  if (turn.kind === 'failure') {
    return <FailureCard message={turn.message} code={turn.code} />
  }

  if (turn.kind === 'file_change') {
    return (
      <FileChangeRow
        turn={turn}
        rollbackState={turn.changeGroupId ? codeRollbackStates[turn.changeGroupId] : undefined}
        onRollbackChangeGroup={onRollbackChangeGroup}
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
        actionType={turn.actionType}
        title={turn.title}
        detail={turn.detail}
        shape={turn.shape}
        options={turn.options}
        allowMultiple={turn.allowMultiple}
        resolved={turn.isResolved}
      />
    )
  }

  return (
      <ActionCard
        title={turn.title}
        detail={turn.detail}
        detailRows={turn.detailRows}
        state={turn.state ?? null}
        defaultOpen={turn.defaultOpen ?? false}
        connectsTop={connectsTop}
        connectsBottom={connectsBottom}
      />
  )
}

function humanizeFileChangeOperation(operation: string): string {
  return operation
    .trim()
    .replace(/[._-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .toLowerCase() || 'changed'
}

function fileChangeTargetLabel(turn: Extract<ConversationTurn, { kind: 'file_change' }>): string {
  return turn.toPath ? `${turn.path} -> ${turn.toPath}` : turn.path
}

function FileChangeRow({
  turn,
  rollbackState,
  onRollbackChangeGroup,
}: {
  turn: Extract<ConversationTurn, { kind: 'file_change' }>
  rollbackState?: CodeRollbackUiState
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}) {
  const operationLabel = humanizeFileChangeOperation(turn.operation)
  const targetLabel = fileChangeTargetLabel(turn)
  const canRollback = Boolean(turn.changeGroupId && onRollbackChangeGroup)
  const statusTone =
    rollbackState?.status === 'failed'
      ? 'text-destructive'
      : rollbackState?.status === 'succeeded'
        ? 'text-success'
        : 'text-muted-foreground'

  return (
    <div
      className={cn(
        'group/file-change flex items-start gap-2 rounded-md px-1 py-1 text-left transition-colors',
        'hover:bg-foreground/[0.03]',
      )}
    >
      <FileText className="mt-[3px] h-3.5 w-3.5 shrink-0 text-primary/70" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-1.5">
          <span className="shrink-0 text-[12.5px] font-medium text-foreground">
            {operationLabel}
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-foreground/80" title={targetLabel}>
            {targetLabel}
          </span>
        </div>
        <p className="mt-0.5 min-w-0 truncate text-[11.5px] text-muted-foreground/75" title={turn.detail}>
          {turn.detail}
        </p>
        {rollbackState ? (
          <p
            className={cn('mt-1 text-[11.5px]', statusTone)}
            role="status"
            aria-live="polite"
          >
            {rollbackState.message}
          </p>
        ) : null}
      </div>
      <CodeRollbackButton
        path={targetLabel}
        changeGroupId={turn.changeGroupId}
        state={rollbackState}
        enabled={canRollback}
        onRollbackChangeGroup={onRollbackChangeGroup}
      />
    </div>
  )
}

function CodeRollbackButton({
  path,
  changeGroupId,
  state,
  enabled,
  onRollbackChangeGroup,
  dense = false,
}: {
  path: string
  changeGroupId: string | null
  state?: CodeRollbackUiState
  enabled: boolean
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
  dense?: boolean
}) {
  const isPending = state?.status === 'pending'
  const isSucceeded = state?.status === 'succeeded'
  const canClick = enabled && Boolean(changeGroupId) && !isPending && !isSucceeded
  const label = (() => {
    if (!enabled || !changeGroupId) return `Rollback unavailable for ${path}`
    if (isPending) return `Rolling back ${path}`
    if (isSucceeded) return `Rolled back ${path}`
    if (state?.status === 'failed') return `Retry rollback for ${path}`
    return `Rollback ${path}`
  })()
  const tooltip = (() => {
    if (!changeGroupId) return 'Snapshot unavailable'
    if (!enabled) return 'Rollback unavailable'
    if (isPending) return 'Rolling back'
    if (isSucceeded) return 'Rolled back'
    if (state?.status === 'failed') return 'Retry rollback'
    return 'Rollback file change'
  })()
  const Icon = isPending ? Loader2 : isSucceeded ? CheckCircle2 : Undo2

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={label}
          disabled={!canClick}
          onClick={() => {
            if (!changeGroupId || !canClick) return
            onRollbackChangeGroup?.({ changeGroupId, path })
          }}
          className={cn(
            'mt-[1px] shrink-0 rounded-md text-muted-foreground/70 hover:text-foreground',
            dense ? 'h-5 w-5 [&_svg]:size-[11px]' : 'h-6 w-6 [&_svg]:size-3',
            state?.status === 'failed' && 'text-destructive hover:text-destructive',
            isSucceeded && 'text-success',
          )}
        >
          <Icon aria-hidden="true" className={cn(isPending && 'animate-spin')} />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">{tooltip}</TooltipContent>
    </Tooltip>
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
  state: RuntimeStreamToolItemView['toolState'] | null
  defaultOpen?: boolean
  connectsTop?: boolean
  connectsBottom?: boolean
}

function ActionCard({
  title,
  detail,
  detailRows,
  state,
  defaultOpen = false,
  connectsTop = false,
  connectsBottom = false,
}: ActionCardProps) {
  const hasDetails = detailRows.length > 0
  const [open, setOpen] = useState(() => defaultOpen && hasDetails)
  const isFailed = state === 'failed'
  const isRunning = state === 'running'
  const rowClass = cn(
    'flex w-full items-center gap-2 rounded-md px-1 py-0.5 text-left transition-colors',
    'hover:bg-foreground/[0.03]',
    isRunning && 'bg-primary/[0.025] agent-tool-running-row',
    isFailed && 'bg-destructive/[0.04]',
  )

  useEffect(() => {
    if (defaultOpen && hasDetails) {
      setOpen(true)
    }
  }, [defaultOpen, hasDetails])

  return (
    <div className="group/tool relative">
      <ToolChainConnectors connectsTop={connectsTop} connectsBottom={connectsBottom} />
      <Collapsible open={open} onOpenChange={setOpen}>
        {hasDetails ? (
          <CollapsibleTrigger asChild>
            <button
              type="button"
              aria-label={`${open ? 'Hide' : 'Show'} tool details for ${title}`}
              className={cn(rowClass, 'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60')}
            >
              <ActionCardHeader title={title} detail={detail} state={state} open={open} />
            </button>
          </CollapsibleTrigger>
        ) : (
          <div className={rowClass}>
            <ActionCardHeader title={title} detail={detail} state={state} open={null} />
          </div>
        )}
        {hasDetails ? (
          <CollapsibleContent
            className={cn(
              'overflow-hidden',
              'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:slide-in-from-top-1',
              'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-1',
              'data-[state=open]:duration-200 data-[state=closed]:duration-150',
            )}
          >
            <div className="ml-[22px] border-l border-border/25 pl-3 pr-1 pb-1.5 pt-1">
              <ToolDetailRows rows={detailRows} />
            </div>
          </CollapsibleContent>
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
          'min-w-0 shrink truncate text-[12.5px] font-medium tracking-[-0.005em]',
          isFailed ? 'text-destructive' : 'text-foreground',
        )}
        title={title}
      >
        {title}
      </span>
      {hasDetail ? (
        <span
          className="min-w-0 flex-1 truncate text-[11.5px] text-muted-foreground/75"
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
            'h-3 w-3 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
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
 * padding, the inner row uses `px-1` (4px) and a 14px status icon, putting
 * the icon center at x = 4 + 7 = 11px and y ≈ 10px (icon vertically centered
 * in a ~20px row with `items-center`).
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
          className="pointer-events-none absolute left-[11px] -top-[10px] h-[20px] w-px bg-muted-foreground/35"
        />
      ) : null}
      {connectsBottom ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-[11px] top-[10px] -bottom-[10px] w-px bg-muted-foreground/35"
        />
      ) : null}
    </>
  )
}

interface ToolDetailRowsProps {
  rows: Array<{ label: string; value: string }>
}

function ToolDetailRows({ rows }: ToolDetailRowsProps) {
  const outputRow = rows.find((row) => /output/i.test(row.label))
  if (outputRow) {
    return (
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
        <ToolOutputCopyAffordance text={outputRow.value} label="Copy tool output" />
      </div>
    )
  }

  const fallback =
    rows.find((row) => /result/i.test(row.label)) ??
    rows.find((row) => /input|outcome|cmd|command/i.test(row.label)) ??
    rows[0]
  if (!fallback) return null

  const isCommandLike = /^[A-Za-z][\w./-]*\s/.test(fallback.value.trim())
  return (
    <div className="group/output relative">
      <div
        className={cn(
          'rounded-md bg-background/40 px-3 py-2 pr-9',
          'whitespace-pre-wrap break-words font-mono text-[12px] leading-[1.6] text-foreground/80',
        )}
      >
        {isCommandLike ? (
          <span className="flex items-start gap-2">
            <Terminal aria-hidden="true" className="mt-[2px] h-3 w-3 shrink-0 text-primary/75" />
            <span className="min-w-0 flex-1">{fallback.value}</span>
          </span>
        ) : (
          fallback.value
        )}
      </div>
      <ToolOutputCopyAffordance text={fallback.value} label="Copy tool output" />
    </div>
  )
}

function ToolOutputCopyAffordance({ text, label }: { text: string; label: string }) {
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
// Action group card — collapsed summary + expandable inline list.
// ---------------------------------------------------------------------------

interface ActionGroupCardProps {
  title: string
  detail: string
  state: RuntimeStreamToolItemView['toolState'] | null
  actions: Array<{
    id: string
    title: string
    detail: string
    detailRows: Array<{ label: string; value: string }>
    state: RuntimeStreamToolItemView['toolState'] | null
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
  const isRunning = state === 'running'
  const hasDetail = detail.trim().length > 0

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="group/tool relative"
    >
      <ToolChainConnectors connectsTop={connectsTop} connectsBottom={connectsBottom} />
      <CollapsibleTrigger asChild>
        <button
          type="button"
          aria-label={`${open ? 'Hide' : 'Show'} grouped tool details for ${title}`}
          className={cn(
            'flex w-full items-center gap-2 rounded-md px-1 py-0.5 text-left transition-colors',
            'hover:bg-foreground/[0.03]',
            isRunning && 'bg-primary/[0.025] agent-tool-running-row',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
          )}
        >
          <ToolStatusIcon state={state} className="shrink-0" />
          <span
            className="min-w-0 shrink truncate text-[12.5px] font-medium tracking-[-0.005em] text-foreground"
            title={title}
          >
            {title}
          </span>
          {hasDetail ? (
            <span
              className="min-w-0 flex-1 truncate text-[11.5px] text-muted-foreground/75"
              title={detail}
            >
              {detail}
            </span>
          ) : (
            <span aria-hidden="true" className="flex-1" />
          )}
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'h-3 w-3 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
              'group-hover/tool:text-muted-foreground/80',
              open ? 'rotate-180 text-muted-foreground/80' : 'rotate-0',
            )}
          />
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent
        className={cn(
          'overflow-hidden',
          'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:slide-in-from-top-1',
          'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-1',
          'data-[state=open]:duration-200 data-[state=closed]:duration-150',
        )}
      >
        <ol className="ml-[22px] mt-0.5 flex flex-col gap-px border-l border-border/25 pl-2">
          {actions.map((action, index) => (
            <ActionGroupItem key={action.id} action={action} index={index} />
          ))}
        </ol>
      </CollapsibleContent>
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
  const hasDetails = action.detailRows.length > 0
  const rowClass = cn(
    'flex w-full items-center gap-2 rounded-md px-1 py-0.5 text-left transition-colors',
    'hover:bg-foreground/[0.03]',
    action.state === 'running' && 'bg-primary/[0.025] agent-tool-running-row',
    action.state === 'failed' && 'bg-destructive/[0.04]',
  )

  return (
    <li
      className="group/sub agent-stagger-child"
      style={{ ['--stagger-index' as string]: index }}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        {hasDetails ? (
          <CollapsibleTrigger asChild>
            <button
              type="button"
              aria-label={`${open ? 'Hide' : 'Show'} tool details for ${action.title}`}
              className={cn(rowClass, 'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60')}
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
          <CollapsibleContent
            className={cn(
              'overflow-hidden',
              'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:slide-in-from-top-1',
              'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-1',
              'data-[state=open]:duration-150 data-[state=closed]:duration-100',
            )}
          >
            <div className="ml-[20px] border-l border-border/25 pb-1.5 pl-3 pr-1 pt-1">
              <ToolDetailRows rows={action.detailRows} />
            </div>
          </CollapsibleContent>
        ) : null}
      </Collapsible>
    </li>
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
          'min-w-0 shrink truncate text-[12.5px]',
          isFailed ? 'text-destructive' : 'text-foreground/95',
        )}
        title={action.title}
      >
        {action.title}
      </span>
      {hasDetail ? (
        <span
          className="min-w-0 flex-1 truncate text-[11.5px] text-muted-foreground/75"
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
            'h-3 w-3 shrink-0 text-muted-foreground/45 transition-all duration-200 ease-out',
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

function UserMessage({ text, attachments, accountAvatarUrl, accountLogin }: UserMessageProps) {
  const hasAttachments = attachments && attachments.length > 0
  return (
    <div className="group/user flex justify-end gap-2.5">
      <div className="flex min-w-0 max-w-[80%] flex-col items-end gap-1">
        <span className="sr-only">You</span>
        {hasAttachments ? (
          <div className="flex max-w-full flex-wrap justify-end gap-1.5">
            {attachments!.map((attachment) => (
              <UserMessageAttachmentChip key={attachment.id} attachment={attachment} />
            ))}
          </div>
        ) : null}
        {text.length > 0 ? (
          <div
            className={cn(
              'rounded-2xl px-3.5 py-2',
              'bg-primary/10 text-foreground',
              'ring-1 ring-inset ring-primary/15',
              'whitespace-pre-wrap break-words text-[14px] leading-relaxed select-text',
            )}
          >
            {text}
          </div>
        ) : null}
        {text.trim().length > 0 ? (
          <div
            className={cn(
              'flex h-4 items-center justify-end pr-0.5',
              'opacity-0 transition-opacity duration-150',
              'group-hover/user:opacity-100 focus-within:opacity-100',
            )}
          >
            <CopyTextButton
              text={text}
              label="Copy your prompt"
              copiedLabel="Copied your prompt"
              tooltip="Copy"
              className="h-4 w-4 text-muted-foreground/60 hover:text-foreground"
            />
          </div>
        ) : null}
      </div>
      <UserAvatar avatarUrl={accountAvatarUrl} login={accountLogin} />
    </div>
  )
}

function UserMessageAttachmentChip({ attachment }: { attachment: ConversationMessageAttachment }) {
  if (attachment.kind === 'image' && attachment.previewSrc) {
    return (
      <div
        className="overflow-hidden rounded-md border border-border/50 bg-background shadow-sm"
        title={attachment.originalName}
      >
        <img
          src={attachment.previewSrc}
          alt={attachment.originalName}
          className="block max-h-40 max-w-[260px] object-cover"
          draggable={false}
        />
      </div>
    )
  }
  return (
    <div
      className="flex max-w-[260px] items-center gap-2 rounded-md border border-border/50 bg-muted/30 px-2 py-1 text-[11px] text-foreground"
      title={attachment.originalName}
    >
      <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="line-clamp-1 truncate">{attachment.originalName}</span>
    </div>
  )
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
  let match: RegExpExecArray | null

  THINK_PATTERN.lastIndex = 0
  while ((match = THINK_PATTERN.exec(text)) !== null) {
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
  isStreaming,
}: {
  messageId: string
  text: string
  isStreaming: boolean
}) {
  const segments = useMemo(() => splitAssistantText(text), [text])
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

  return (
    <div className="group/agent flex gap-2.5">
      <AgentAvatar pulse={isStreaming} />
      <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5">
        <span className="sr-only">Agent</span>
        <div className="flex w-full min-w-0 flex-col items-start gap-2">
          {segments.map((segment, index) =>
            segment.kind === 'thinking' ? (
              <ThinkingBlock key={index} messageId={`${messageId}:thinking:${index}`} text={segment.text} />
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
        </div>
        {responseCopyText.length > 0 ? (
          <div
            className={cn(
              'flex h-4 items-center pl-0.5',
              'opacity-0 transition-opacity duration-150',
              'group-hover/agent:opacity-100 focus-within:opacity-100',
            )}
          >
            <CopyTextButton
              text={responseCopyText}
              label="Copy agent response"
              copiedLabel="Copied agent response"
              tooltip="Copy"
              className="h-4 w-4 text-muted-foreground/60 hover:text-foreground"
            />
          </div>
        ) : null}
      </div>
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

function ThinkingBlock({ messageId, text }: { messageId?: string; text: string }) {
  const [open, setOpen] = useState(false)
  const normalizedText = text.trim()
  const allLines = normalizedText.split(/\r?\n/).filter((line) => line.trim().length > 0)
  const previewText = allLines.slice(-3).join('\n')
  const hiddenLineCount = Math.max(0, allLines.length - 3)

  return (
    <div
      className={cn(
        'relative w-full max-w-full min-w-0 rounded-lg border border-border/40 bg-muted/15 pl-3.5 pr-3 py-2',
        'before:absolute before:inset-y-2 before:left-0 before:w-[2px] before:rounded-r-full before:bg-primary/35',
      )}
    >
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        className={cn(
          'flex w-full items-center gap-1.5 text-left text-[11.5px] font-semibold uppercase tracking-[0.07em] text-muted-foreground/90',
          'hover:text-foreground focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <Brain className="h-3.5 w-3.5 text-primary/70" />
        <span>Thoughts</span>
        {!open && hiddenLineCount > 0 ? (
          <span
            key={hiddenLineCount}
            className={cn(
              'rounded-full bg-muted/70 px-1.5 py-px text-[10.5px] normal-case tracking-normal text-muted-foreground/85',
              'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:zoom-in-90 motion-safe:duration-150',
            )}
          >
            +{hiddenLineCount}
          </span>
        ) : null}
        <ChevronDown
          className={cn(
            'ml-auto h-3.5 w-3.5 transition-transform duration-200 ease-out',
            open ? 'rotate-180' : 'rotate-0',
          )}
        />
      </button>
      {open ? (
        <div
          key="open"
          className={cn(
            'mt-2 border-t border-border/25 pt-2',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-200',
          )}
        >
          <Markdown messageId={messageId ? `${messageId}:open` : null} text={text} muted />
        </div>
      ) : previewText.length > 0 ? (
        <div className="mt-1.5">
          <Markdown messageId={messageId ? `${messageId}:preview` : null} text={previewText} muted compact />
        </div>
      ) : null}
    </div>
  )
}

function AgentActivityIndicator() {
  return (
    <div
      className={cn(
        'flex items-start gap-2.5',
        'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-200 motion-safe:ease-out',
      )}
      role="status"
      aria-label="Agent is thinking"
    >
      <AgentAvatar pulse />
      <div
        className={cn(
          'mt-0.5 flex min-w-0 items-center gap-1.5 rounded-full border border-border/40 bg-card/35 px-3 py-1 text-[12.5px] font-medium text-muted-foreground shadow-sm',
          'agent-activity-indicator',
        )}
      >
        <Loader2 className="h-3.5 w-3.5 animate-spin text-primary/80" aria-hidden="true" />
        <span>Thinking</span>
        <span className="flex items-center gap-0.5" aria-hidden="true">
          <span className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/70 [animation-delay:0ms]" />
          <span className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/70 [animation-delay:160ms]" />
          <span className="agent-thinking-dot h-1 w-1 rounded-full bg-muted-foreground/70 [animation-delay:320ms]" />
        </span>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Run / stream notice (rendered inline at the foot of the conversation).
// ---------------------------------------------------------------------------

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

  const Icon = tone === 'destructive' ? AlertTriangle : tone === 'warning' ? AlertCircle : Info

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
          <p className={cn('mt-1 break-words font-mono text-[10.5px]', toneStyles.codeText)}>
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
  return (
    <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-2 text-destructive">
      <XCircle className="mt-[2px] h-3.5 w-3.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="m-0 text-[13px] font-medium">Agent run failed</p>
        <p className="mt-0.5 whitespace-pre-wrap break-words text-[12.5px] leading-relaxed">
          {message}
        </p>
        <p className="mt-1 break-words font-mono text-[10.5px] text-destructive/70">
          code: {code}
        </p>
      </div>
    </div>
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
      aria-hidden={showImage ? undefined : 'true'}
      aria-label={showImage && login ? `${login}'s avatar` : undefined}
      className={cn(
        'mt-[2px] flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden rounded-full',
        showImage
          ? 'ring-1 ring-border/50'
          : 'bg-primary/15 text-primary ring-1 ring-primary/25',
      )}
    >
      {showImage ? (
        <img
          src={avatarUrl ?? undefined}
          alt=""
          className="h-full w-full object-cover"
          onError={() => setFailed(true)}
        />
      ) : (
        <User className="h-3 w-3" />
      )}
    </span>
  )
}

function AgentAvatar({ pulse = false }: { pulse?: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'mt-[2px] relative flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-card/80 ring-1 ring-border/50',
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
  const iconSize = state === 'pending' || state === null ? 'h-3 w-3' : 'h-3.5 w-3.5'
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
          state === 'running' && 'animate-spin',
          state === 'succeeded' && 'drop-shadow-[0_0_4px_rgba(34,197,94,0.25)]',
          'motion-safe:animate-in motion-safe:fade-in-0',
          pop ? 'tool-status-icon-pop' : 'motion-safe:zoom-in-95 motion-safe:duration-150',
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
  codeRollbackStates: Record<string, CodeRollbackUiState>
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}

function DenseTurnItem({
  turn,
  codeRollbackStates,
  onRollbackChangeGroup,
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
        <span className="min-w-0 flex-1 truncate" title={`${turn.code}: ${turn.message}`}>
          {truncateForLine(turn.message)}
        </span>
      </li>
    )
  }

  if (turn.kind === 'file_change') {
    return (
      <DenseFileChangeItem
        turn={turn}
        rollbackState={turn.changeGroupId ? codeRollbackStates[turn.changeGroupId] : undefined}
        onRollbackChangeGroup={onRollbackChangeGroup}
      />
    )
  }

  if (turn.kind === 'action') {
    return (
      <DenseActionItem
        title={turn.title}
        detail={turn.detail}
        detailRows={turn.detailRows}
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
        <span className="min-w-0 flex-1 truncate" title={`${turn.title}: ${turn.detail}`}>
          {truncateForLine(turn.title)}
        </span>
      </li>
    )
  }

  return null
}

function DenseFileChangeItem({
  turn,
  rollbackState,
  onRollbackChangeGroup,
}: {
  turn: Extract<ConversationTurn, { kind: 'file_change' }>
  rollbackState?: CodeRollbackUiState
  onRollbackChangeGroup?: (request: CodeRollbackRequest) => void
}) {
  const targetLabel = fileChangeTargetLabel(turn)
  const canRollback = Boolean(turn.changeGroupId && onRollbackChangeGroup)
  const statusTone =
    rollbackState?.status === 'failed'
      ? 'text-destructive'
      : rollbackState?.status === 'succeeded'
        ? 'text-success'
        : 'text-muted-foreground'

  return (
    <li className="px-1">
      <div className="flex items-start gap-2 text-foreground/85">
        <FileText className="mt-[2px] h-3 w-3 shrink-0 text-primary/70" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <span className="block truncate" title={turn.detail}>
            {truncateForLine(`${humanizeFileChangeOperation(turn.operation)} ${targetLabel}`)}
          </span>
          {rollbackState ? (
            <span
              className={cn('mt-0.5 block truncate text-[11px]', statusTone)}
              role="status"
              aria-live="polite"
            >
              {rollbackState.message}
            </span>
          ) : null}
        </div>
        <CodeRollbackButton
          path={targetLabel}
          changeGroupId={turn.changeGroupId}
          state={rollbackState}
          enabled={canRollback}
          onRollbackChangeGroup={onRollbackChangeGroup}
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

function DenseMessageItem({ id, role, text, attachments }: DenseMessageItemProps) {
  const isUser = role === 'user'
  const [open, setOpen] = useState(!isUser)
  const marker = isUser ? '>' : '◆'
  const tone = isUser ? 'text-primary/85' : 'text-foreground/90'
  const normalized = text.trim()
  const hasAttachments = Boolean(attachments && attachments.length > 0)
  const hasMore = normalized.length > 240 || /\r?\n/.test(normalized) || hasAttachments

  return (
    <li className="px-1">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={hasMore ? open : undefined}
        aria-label={hasMore ? `${open ? 'Hide' : 'Show'} full message` : undefined}
        disabled={!hasMore}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasMore ? 'cursor-pointer hover:text-foreground' : 'cursor-default',
          'focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <span className={cn('shrink-0 select-none font-semibold', tone)}>{marker}</span>
        <span
          className="min-w-0 flex-1 truncate text-foreground/85"
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
            'ml-3 mt-1.5 border-l border-border/30 pl-2.5',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150',
          )}
        >
          {isUser ? (
            <p className="m-0 whitespace-pre-wrap break-words text-[12px] text-foreground/85">
              {normalized}
            </p>
          ) : (
            <Markdown messageId={`${id}:dense`} text={text} scale="dense" />
          )}
          {hasAttachments ? (
            <ul className="mt-1.5 flex flex-wrap gap-1 text-[11px] text-muted-foreground/80">
              {attachments!.map((attachment) => (
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
          hasMore ? 'cursor-pointer hover:text-foreground/90' : 'cursor-default',
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
  state: RuntimeStreamToolItemView['toolState'] | null
}

function DenseActionItem({ title, detail, detailRows, state }: DenseActionItemProps) {
  const [open, setOpen] = useState(false)
  const hasDetails = detailRows.length > 0 || detail.trim().length > 0

  return (
    <li className="px-1">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={hasDetails ? open : undefined}
        aria-label={hasDetails ? `${open ? 'Hide' : 'Show'} tool details for ${title}` : undefined}
        disabled={!hasDetails}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasDetails ? 'cursor-pointer hover:text-foreground' : 'cursor-default',
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
      {open && hasDetails ? (
        <div
          className={cn(
            'ml-3 mt-1.5 border-l border-border/30 pl-2.5',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150',
          )}
        >
          <DenseToolDetails detail={detail} detailRows={detailRows} />
        </div>
      ) : null}
    </li>
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
    state: RuntimeStreamToolItemView['toolState'] | null
  }>
}

function DenseActionGroupItem({ title, detail, state, actions }: DenseActionGroupItemProps) {
  const [open, setOpen] = useState(false)
  const hasChildren = actions.length > 0

  return (
    <li className="px-1">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={hasChildren ? open : undefined}
        aria-label={hasChildren ? `${open ? 'Hide' : 'Show'} grouped tool details for ${title}` : undefined}
        disabled={!hasChildren}
        className={cn(
          'flex w-full items-start gap-2 text-left',
          hasChildren ? 'cursor-pointer hover:text-foreground' : 'cursor-default',
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
        {hasChildren ? (
          <ChevronDown
            aria-hidden="true"
            className={cn(
              'mt-[3px] h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform duration-150',
              open ? 'rotate-180' : 'rotate-0',
            )}
          />
        ) : null}
      </button>
      {open && hasChildren ? (
        <ol
          className={cn(
            'ml-3 mt-1.5 flex flex-col gap-1.5 border-l border-border/30 pl-2.5',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150',
          )}
        >
          {actions.map((action) => (
            <DenseActionItem
              key={action.id}
              title={action.title}
              detail={action.detail}
              detailRows={action.detailRows}
              state={action.state}
            />
          ))}
        </ol>
      ) : null}
    </li>
  )
}

function DenseToolDetails({
  detail,
  detailRows,
}: {
  detail: string
  detailRows: Array<{ label: string; value: string }>
}) {
  const trimmedDetail = detail.trim()
  const outputRow = detailRows.find((row) => /output/i.test(row.label))
  const fallbackRow =
    outputRow ??
    detailRows.find((row) => /result/i.test(row.label)) ??
    detailRows.find((row) => /input|outcome|cmd|command/i.test(row.label)) ??
    detailRows[0] ??
    null

  if (!fallbackRow && trimmedDetail.length === 0) return null

  return (
    <div className="flex flex-col gap-1.5 text-[11.5px]">
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
