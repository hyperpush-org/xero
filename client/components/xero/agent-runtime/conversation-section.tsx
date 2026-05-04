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

import { memo, useMemo, useState } from 'react'
import {
  AlertCircle,
  AlertTriangle,
  Brain,
  CheckCircle2,
  ChevronDown,
  Circle,
  FileText,
  Info,
  Loader2,
  Terminal,
  User,
  XCircle,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import type {
  RuntimeRunView,
  RuntimeStreamCompleteItemView,
  RuntimeStreamFailureItemView,
  RuntimeStreamIssueView,
  RuntimeStreamToolItemView,
} from '@/src/lib/xero-model'

import { AppLogo } from '../app-logo'
import { Markdown } from './conversation-markdown'
import { getToolStateLabel } from './runtime-stream-helpers'

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
        state: RuntimeStreamToolItemView['toolState'] | null
      }>
    }
  | {
      id: string
      kind: 'failure'
      sequence: number
      message: string
      code: string
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
        className="flex flex-col gap-1 font-mono text-[11px] leading-snug"
      >
        {showAnyTurn ? (
          <ol aria-label="Agent conversation turns" className="flex flex-col gap-0.5">
            {visibleTurns.map((turn) => (
              <DenseTurnItem key={turn.id} turn={turn} />
            ))}
          </ol>
        ) : null}
        {showActivityIndicator ? (
          <div className="flex items-center gap-1.5 px-1 text-[10.5px] text-muted-foreground">
            <Loader2 className="h-2.5 w-2.5 animate-spin text-primary/70" />
            <span>thinking…</span>
          </div>
        ) : null}
        {showAnyNotice ? (
          <ul aria-label="Agent run notices" className="mt-1 flex flex-col gap-1">
            {showHandoffNotice ? (
              <li className="rounded-sm border border-border/40 bg-muted/15 px-2 py-1 text-[10.5px] text-muted-foreground">
                ⤳ handed off to a fresh same-type session
              </li>
            ) : null}
            {showRunFailure ? (
              <li className="rounded-sm border border-destructive/30 bg-destructive/10 px-2 py-1 text-[10.5px] text-destructive">
                ✗ {runFailureMessage}
              </li>
            ) : null}
            {showStreamFailure && streamFailure ? (
              <li className="rounded-sm border border-destructive/30 bg-destructive/10 px-2 py-1 text-[10.5px] text-destructive">
                ✗ {describeStreamMessage(streamFailure.code, streamFailure.message)}
              </li>
            ) : null}
            {showStreamIssue && streamIssue ? (
              <li className="rounded-sm border border-border/40 bg-muted/20 px-2 py-1 text-[10.5px] text-muted-foreground">
                ⚠ {describeStreamMessage(streamIssue.code, streamIssue.message)}
              </li>
            ) : null}
          </ul>
        ) : null}
      </section>
    )
  }

  return (
    <section aria-label="Agent conversation" className="flex flex-col gap-5">
      {showAnyTurn ? (
        <ol aria-label="Agent conversation turns" className="flex flex-col gap-5">
          {visibleTurns.map((turn, index) => (
            <ConversationTurnItem
              key={turn.id}
              turn={turn}
              accountAvatarUrl={accountAvatarUrl}
              accountLogin={accountLogin}
              isStreaming={index === visibleTurns.length - 1 && isLastTurnStreamingAssistant}
            />
          ))}
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
    </section>
  )
})

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

interface ConversationTurnItemProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
}

const TURN_ENTRY_CLASS = cn(
  'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1',
  'motion-safe:duration-200 motion-safe:ease-out',
)

function ConversationTurnItem({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
}: ConversationTurnItemProps) {
  return (
    <li className={TURN_ENTRY_CLASS}>
      <ConversationTurnRow
        turn={turn}
        accountAvatarUrl={accountAvatarUrl}
        accountLogin={accountLogin}
        isStreaming={isStreaming}
      />
    </li>
  )
}

interface ConversationTurnRowProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
  isStreaming: boolean
}

function ConversationTurnRow({
  turn,
  accountAvatarUrl,
  accountLogin,
  isStreaming,
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
          <span className="px-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70">
            Agent
          </span>
          <ThinkingBlock messageId={turn.id} text={turn.text} />
        </div>
      </div>
    )
  }

  if (turn.kind === 'failure') {
    return <FailureCard message={turn.message} code={turn.code} />
  }

  if (turn.kind === 'action_group') {
    return (
      <ActionGroupCard
        title={turn.title}
        detail={turn.detail}
        state={turn.state ?? null}
        actions={turn.actions}
      />
    )
  }

  return (
    <ActionCard
      title={turn.title}
      detail={turn.detail}
      detailRows={turn.detailRows}
      state={turn.state ?? null}
    />
  )
}

// ---------------------------------------------------------------------------
// Action / tool card — flat, inline row with a leading status icon.
// ---------------------------------------------------------------------------

interface ActionCardProps {
  title: string
  detail: string
  detailRows: Array<{ label: string; value: string }>
  state: RuntimeStreamToolItemView['toolState'] | null
}

function ActionCard({ title, detail, detailRows, state }: ActionCardProps) {
  const [open, setOpen] = useState(false)
  const hasDetails = detailRows.length > 0
  const isFailed = state === 'failed'

  return (
    <div
      className={cn(
        'group rounded-lg border bg-card/25 transition-colors',
        isFailed
          ? 'border-destructive/30 bg-destructive/[0.04]'
          : 'border-border/40 hover:border-border/70 hover:bg-card/45',
        open && !isFailed ? 'border-border/70 bg-card/45' : null,
      )}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        <div className="flex items-start gap-2 px-3 py-2">
          <ToolStatusIcon state={state} className="mt-[2.5px]" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <span
                className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-foreground"
                title={title}
              >
                {title}
              </span>
              <ToolStateLabel state={state} size="sm" />
              {hasDetails ? (
                <CollapsibleTrigger asChild>
                  <button
                    type="button"
                    aria-label={`${open ? 'Hide' : 'Show'} tool details for ${title}`}
                    className={cn(
                      'flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded text-muted-foreground/60',
                      'hover:bg-muted/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                    )}
                  >
                    <ChevronDown
                      className={cn(
                        'h-3 w-3 transition-transform duration-200 ease-out',
                        open ? 'rotate-180' : 'rotate-0',
                      )}
                    />
                  </button>
                </CollapsibleTrigger>
              ) : null}
            </div>
            {detail.trim().length > 0 ? (
              <p
                className="mt-0.5 truncate text-[11.5px] leading-relaxed text-muted-foreground/90"
                title={detail}
              >
                {detail}
              </p>
            ) : null}
          </div>
        </div>
        {hasDetails ? (
          <CollapsibleContent
            className={cn(
              'overflow-hidden',
              'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:slide-in-from-top-1',
              'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-1',
              'data-[state=open]:duration-200 data-[state=closed]:duration-150',
            )}
          >
            <div className="border-t border-border/30 px-3 pb-2.5 pt-2">
              <ToolDetailRows rows={detailRows} />
            </div>
          </CollapsibleContent>
        ) : null}
      </Collapsible>
    </div>
  )
}

interface ToolDetailRowsProps {
  rows: Array<{ label: string; value: string }>
}

function ToolDetailRows({ rows }: ToolDetailRowsProps) {
  return (
    <dl className="grid gap-2">
      {rows.map((row, index) => {
        const isCommandLike =
          /input|command|cmd/i.test(row.label) && /^[A-Za-z][\w./-]*\s/.test(row.value.trim())
        return (
          <div key={`${row.label}:${index}`} className="grid gap-1">
            <dt className="text-[9.5px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/70">
              {row.label}
            </dt>
            <dd
              className={cn(
                'overflow-hidden rounded-md border border-border/40 bg-muted/25 px-2 py-1.5',
                'whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-foreground/85',
              )}
              title={row.value}
            >
              {isCommandLike ? (
                <span className="flex items-start gap-1.5">
                  <Terminal
                    aria-hidden="true"
                    className="mt-[1px] h-3 w-3 shrink-0 text-primary/70"
                  />
                  <span className="min-w-0 flex-1">{row.value}</span>
                </span>
              ) : (
                row.value
              )}
            </dd>
          </div>
        )
      })}
    </dl>
  )
}

interface ToolStateLabelProps {
  state: RuntimeStreamToolItemView['toolState'] | null
  size?: 'sm' | 'xs'
}

function ToolStateLabel({ state, size = 'sm' }: ToolStateLabelProps) {
  if (!state) return null
  const sizeClass = size === 'sm' ? 'text-[10.5px]' : 'text-[10px]'
  return (
    <span
      key={state}
      className={cn(
        'shrink-0 font-medium uppercase tracking-[0.05em] tabular-nums',
        sizeClass,
        getToolStateTextClass(state),
        'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-200',
      )}
    >
      {getToolStateLabel(state)}
    </span>
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
    state: RuntimeStreamToolItemView['toolState'] | null
  }>
}

function ActionGroupCard({ title, detail, state, actions }: ActionGroupCardProps) {
  const [open, setOpen] = useState(false)

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className={cn(
        'overflow-hidden rounded-lg border bg-card/25 transition-colors',
        open ? 'border-border/70 bg-card/45' : 'border-border/40 hover:border-border/70 hover:bg-card/45',
      )}
    >
      <CollapsibleTrigger asChild>
        <button
          type="button"
          aria-label={`${open ? 'Hide' : 'Show'} grouped tool details for ${title}`}
          className={cn(
            'flex w-full items-start gap-2 px-3 py-2 text-left',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
          )}
        >
          <ToolStatusIcon state={state} className="mt-[2.5px]" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <span
                className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-foreground"
                title={title}
              >
                {title}
              </span>
              <ChevronDown
                className={cn(
                  'h-3 w-3 shrink-0 text-muted-foreground/60 transition-transform duration-200 ease-out',
                  open ? 'rotate-180' : 'rotate-0',
                )}
              />
            </div>
            <p
              className="mt-0.5 truncate text-[11.5px] leading-relaxed text-muted-foreground/90"
              title={detail}
            >
              {detail}
            </p>
          </div>
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
        <ol className="divide-y divide-border/25 border-t border-border/30">
          {actions.map((action) => (
            <li
              key={action.id}
              className={cn(
                'flex items-start gap-2 px-3 py-1.5',
                'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-150 motion-safe:ease-out',
              )}
            >
              <ToolStatusIcon state={action.state} className="mt-[2.5px]" />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className="min-w-0 flex-1 truncate text-[12px] text-foreground"
                    title={action.title}
                  >
                    {action.title}
                  </span>
                  <ToolStateLabel state={action.state} size="xs" />
                </div>
                <p
                  className="mt-0.5 truncate text-[11px] leading-relaxed text-muted-foreground/85"
                  title={action.detail}
                >
                  {action.detail}
                </p>
              </div>
            </li>
          ))}
        </ol>
      </CollapsibleContent>
    </Collapsible>
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
    <div className="flex justify-end gap-2.5">
      <div className="flex min-w-0 max-w-[80%] flex-col items-end gap-1">
        <span className="px-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70">
          You
        </span>
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
              'whitespace-pre-wrap break-words text-[13px] leading-relaxed',
            )}
          >
            {text}
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
  const lastResponseIndex = (() => {
    for (let i = segments.length - 1; i >= 0; i -= 1) {
      if (segments[i].kind === 'response') return i
    }
    return -1
  })()

  return (
    <div className="flex gap-2.5">
      <AgentAvatar pulse={isStreaming} />
      <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5">
        <span className="px-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/70">
          Agent
        </span>
        <div className="flex w-full min-w-0 flex-col items-start gap-2">
          {segments.map((segment, index) =>
            segment.kind === 'thinking' ? (
              <ThinkingBlock key={index} messageId={`${messageId}:thinking:${index}`} text={segment.text} />
            ) : (
              <ResponseBlock
                key={index}
                messageId={`${messageId}:response:${index}`}
                text={segment.text}
                showCaret={isStreaming && index === lastResponseIndex}
              />
            ),
          )}
        </div>
      </div>
    </div>
  )
}

function ResponseBlock({
  messageId,
  text,
  showCaret = false,
}: {
  messageId: string
  text: string
  showCaret?: boolean
}) {
  return (
    <div className="w-full min-w-0 px-0.5 text-foreground">
      <Markdown messageId={messageId} text={text} trailing={showCaret ? <StreamingCaret /> : null} />
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
        'relative w-full max-w-full min-w-0 rounded-lg border border-border/40 bg-muted/15 pl-3 pr-2.5 py-1.5',
        'before:absolute before:inset-y-1.5 before:left-0 before:w-[2px] before:rounded-r-full before:bg-primary/35',
      )}
    >
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        className={cn(
          'flex w-full items-center gap-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/85',
          'hover:text-foreground focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <Brain className="h-3 w-3 text-primary/70" />
        <span>Thoughts</span>
        {!open && hiddenLineCount > 0 ? (
          <span
            key={hiddenLineCount}
            className={cn(
              'rounded-full bg-muted/70 px-1.5 py-px text-[9.5px] normal-case tracking-normal text-muted-foreground/80',
              'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:zoom-in-90 motion-safe:duration-150',
            )}
          >
            +{hiddenLineCount}
          </span>
        ) : null}
        <ChevronDown
          className={cn(
            'ml-auto h-3 w-3 transition-transform duration-200 ease-out',
            open ? 'rotate-180' : 'rotate-0',
          )}
        />
      </button>
      {open ? (
        <div
          key="open"
          className={cn(
            'mt-1.5 border-t border-border/25 pt-1.5',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-200',
          )}
        >
          <Markdown messageId={messageId ? `${messageId}:open` : null} text={text} muted compact />
        </div>
      ) : previewText.length > 0 ? (
        <div
          key={`preview:${previewText.length}`}
          className={cn(
            'mt-1',
            'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-150',
          )}
        >
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
          'mt-0.5 flex min-w-0 items-center gap-1.5 rounded-full border border-border/40 bg-card/35 px-2.5 py-1 text-[12px] font-medium text-muted-foreground shadow-sm',
          'agent-activity-indicator',
        )}
      >
        <Loader2 className="h-3 w-3 animate-spin text-primary/80" aria-hidden="true" />
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
        <p className="m-0 text-[12.5px] font-medium">{title}</p>
        <p className="mt-0.5 whitespace-pre-wrap break-words text-[12px] leading-relaxed">
          {message}
        </p>
        {code ? (
          <p className={cn('mt-1 break-words font-mono text-[10px]', toneStyles.codeText)}>
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
        <p className="m-0 text-[12.5px] font-medium">Agent run failed</p>
        <p className="mt-0.5 whitespace-pre-wrap break-words text-[12px] leading-relaxed">
          {message}
        </p>
        <p className="mt-1 break-words font-mono text-[10px] text-destructive/70">
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
        'mt-[16px] flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden rounded-full',
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
          : 'text-muted-foreground/60'

  const pop = state === 'succeeded' || state === 'failed'

  return (
    <Icon
      key={key}
      aria-hidden="true"
      className={cn(
        'h-3 w-3 shrink-0',
        tone,
        state === 'running' && 'animate-spin',
        'motion-safe:animate-in motion-safe:fade-in-0',
        pop ? 'tool-status-icon-pop' : 'motion-safe:zoom-in-95 motion-safe:duration-150',
        className,
      )}
    />
  )
}

function truncateForLine(text: string, max = 240): string {
  const collapsed = text.replace(/\s+/g, ' ').trim()
  if (collapsed.length <= max) return collapsed
  return `${collapsed.slice(0, max - 1)}…`
}

interface DenseTurnItemProps {
  turn: ConversationTurn
}

function DenseTurnItem({ turn }: DenseTurnItemProps) {
  if (turn.kind === 'message') {
    const isUser = turn.role === 'user'
    const marker = isUser ? '>' : '◆'
    const tone = isUser ? 'text-primary/85' : 'text-foreground/90'
    return (
      <li className="flex items-start gap-1.5 px-1">
        <span className={cn('shrink-0 select-none font-semibold', tone)}>{marker}</span>
        <span className="min-w-0 flex-1 truncate text-foreground/85" title={turn.text}>
          {truncateForLine(turn.text)}
        </span>
      </li>
    )
  }

  if (turn.kind === 'thinking') {
    return (
      <li className="flex items-start gap-1.5 px-1 text-muted-foreground/80">
        <span className="shrink-0 select-none">~</span>
        <span className="min-w-0 flex-1 truncate" title={turn.text}>
          {truncateForLine(turn.text)}
        </span>
      </li>
    )
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

  const isAction = turn.kind === 'action' || turn.kind === 'action_group'
  if (!isAction) return null
  const state = turn.state ?? null
  const stateClass =
    state === 'failed'
      ? 'text-destructive/90'
      : state === 'running'
        ? 'text-primary/90'
        : state === 'succeeded'
          ? 'text-success/90'
          : 'text-muted-foreground/70'
  return (
    <li className="flex items-start gap-1.5 px-1">
      <span className={cn('shrink-0 select-none font-semibold', stateClass)}>$</span>
      <span className="min-w-0 flex-1 truncate text-foreground/85" title={`${turn.title} — ${turn.detail}`}>
        {truncateForLine(turn.title)}
      </span>
      {state ? (
        <span className={cn('shrink-0 text-[9.5px] uppercase tracking-wider tabular-nums', stateClass)}>
          {getToolStateLabel(state)}
        </span>
      ) : null}
    </li>
  )
}

function getToolStateTextClass(state: RuntimeStreamToolItemView['toolState']): string {
  switch (state) {
    case 'succeeded':
      return 'text-success/90'
    case 'failed':
      return 'text-destructive/90'
    case 'running':
      return 'text-primary/90'
    case 'pending':
      return 'text-muted-foreground/80'
  }
}
