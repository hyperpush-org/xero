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

import { useState } from 'react'
import {
  AlertCircle,
  AlertTriangle,
  Brain,
  CheckCircle2,
  ChevronDown,
  Circle,
  Info,
  Loader2,
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

export type ConversationTurn =
  | {
      id: string
      kind: 'message'
      role: 'user' | 'assistant'
      sequence: number
      text: string
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

export function ConversationSection({
  runtimeRun,
  visibleTurns,
  streamIssue,
  streamFailure,
  showActivityIndicator = false,
  streamCompletion = null,
  accountAvatarUrl = null,
  accountLogin = null,
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

  return (
    <section aria-label="Agent conversation" className="flex flex-col gap-6">
      {showAnyTurn ? (
        <ol aria-label="Agent conversation turns" className="flex flex-col gap-6">
          {visibleTurns.map((turn) => (
            <li key={turn.id}>
              <ConversationTurnRow
                turn={turn}
                accountAvatarUrl={accountAvatarUrl}
                accountLogin={accountLogin}
              />
            </li>
          ))}
        </ol>
      ) : null}

      {showActivityIndicator ? <AgentActivityIndicator /> : null}

      {showAnyNotice ? (
        <ul aria-label="Agent run notices" className="flex flex-col gap-3">
          {showHandoffNotice ? (
            <li>
              <NoticeRow
                tone="info"
                title="Run continued in a fresh session"
                message="Xero handed this conversation off to a new same-type run because the context budget was full. Your task, prior decisions, and important context carried over — keep replying as normal."
                code={null}
              />
            </li>
          ) : null}
          {showRunFailure ? (
            <li>
              <NoticeRow
                tone="destructive"
                title={runtimeRun?.isTerminal ? 'Latest saved run failed' : 'Agent run failed'}
                message={runFailureMessage ?? ''}
                code={runFailureCode}
              />
            </li>
          ) : null}
          {showStreamFailure && streamFailure ? (
            <li>
              <NoticeRow
                tone="destructive"
                title="Live stream failed"
                message={describeStreamMessage(streamFailure.code, streamFailure.message)}
                code={streamFailure.code}
              />
            </li>
          ) : null}
          {showStreamIssue && streamIssue ? (
            <li>
              <NoticeRow
                tone={streamIssue.retryable ? 'info' : 'warning'}
                title={describeStreamTitle(streamIssue.code, 'Live stream issue')}
                message={describeStreamMessage(streamIssue.code, streamIssue.message)}
                code={streamIssue.code}
              />
            </li>
          ) : null}
        </ul>
      ) : null}
    </section>
  )
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

interface ConversationTurnRowProps {
  turn: ConversationTurn
  accountAvatarUrl: string | null
  accountLogin: string | null
}

function ConversationTurnRow({ turn, accountAvatarUrl, accountLogin }: ConversationTurnRowProps) {
  if (turn.kind === 'message') {
    return turn.role === 'user' ? (
      <UserMessage text={turn.text} accountAvatarUrl={accountAvatarUrl} accountLogin={accountLogin} />
    ) : (
      <AssistantMessage text={turn.text} />
    )
  }

  if (turn.kind === 'thinking') {
    return (
      <div className="flex gap-3">
        <AgentAvatar />
        <div className="flex min-w-0 flex-1 flex-col items-start gap-2">
          <span className="px-0.5 text-[10.5px] font-medium uppercase tracking-wider text-muted-foreground/80">
            Agent
          </span>
          <ThinkingBlock text={turn.text} />
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

  return (
    <div
      className={cn(
        'group rounded-xl border border-border/40 bg-card/30 px-3.5 py-2.5 transition-colors',
        'hover:border-border/70 hover:bg-card/50',
      )}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        <div className="flex items-start gap-2.5">
          <ToolStatusIcon state={state} className="mt-[3px]" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground" title={title}>
                {title}
              </span>
              {state ? (
                <span
                  className={cn(
                    'shrink-0 text-[11px] font-medium tabular-nums',
                    getToolStateTextClass(state),
                  )}
                >
                  {getToolStateLabel(state)}
                </span>
              ) : null}
              {hasDetails ? (
                <CollapsibleTrigger asChild>
                  <button
                    type="button"
                    aria-label={`${open ? 'Hide' : 'Show'} tool details for ${title}`}
                    className={cn(
                      'flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground/70',
                      'hover:bg-muted/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                    )}
                  >
                    <ChevronDown
                      className={cn(
                        'h-3.5 w-3.5 transition-transform duration-150',
                        open ? 'rotate-180' : 'rotate-0',
                      )}
                    />
                  </button>
                </CollapsibleTrigger>
              ) : null}
            </div>
            <p
              className="mt-0.5 truncate text-[12.5px] leading-relaxed text-muted-foreground"
              title={detail}
            >
              {detail}
            </p>
            {hasDetails ? (
              <CollapsibleContent>
                <dl className="mt-2.5 grid gap-2 border-t border-border/40 pt-2.5">
                  {detailRows.map((row, index) => (
                    <div key={`${row.label}:${index}`} className="grid gap-0.5">
                      <dt className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/80">
                        {row.label}
                      </dt>
                      <dd
                        className="whitespace-pre-wrap break-words font-mono text-[11.5px] leading-relaxed text-foreground/85"
                        title={row.value}
                      >
                        {row.value}
                      </dd>
                    </div>
                  ))}
                </dl>
              </CollapsibleContent>
            ) : null}
          </div>
        </div>
      </Collapsible>
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
    state: RuntimeStreamToolItemView['toolState'] | null
  }>
}

function ActionGroupCard({ title, detail, state, actions }: ActionGroupCardProps) {
  const [open, setOpen] = useState(false)

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="overflow-hidden rounded-xl border border-border/40 bg-card/30"
    >
      <CollapsibleTrigger asChild>
        <button
          type="button"
          aria-label={`${open ? 'Hide' : 'Show'} grouped tool details for ${title}`}
          className={cn(
            'flex w-full items-start gap-2.5 px-3.5 py-2.5 text-left transition-colors',
            'hover:bg-card/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
          )}
        >
          <ToolStatusIcon state={state} className="mt-[3px]" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground" title={title}>
                {title}
              </span>
              <ChevronDown
                className={cn(
                  'h-3.5 w-3.5 shrink-0 text-muted-foreground/70 transition-transform duration-150',
                  open ? 'rotate-180' : 'rotate-0',
                )}
              />
            </div>
            <p
              className="mt-0.5 truncate text-[12.5px] leading-relaxed text-muted-foreground"
              title={detail}
            >
              {detail}
            </p>
          </div>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ol className="divide-y divide-border/30 border-t border-border/30">
          {actions.map((action) => (
            <li key={action.id} className="flex items-start gap-2.5 px-3.5 py-2">
              <ToolStatusIcon state={action.state} className="mt-[3px]" />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-[12.5px] text-foreground" title={action.title}>
                    {action.title}
                  </span>
                  {action.state ? (
                    <span
                      className={cn(
                        'shrink-0 text-[10.5px] font-medium tabular-nums',
                        getToolStateTextClass(action.state),
                      )}
                    >
                      {getToolStateLabel(action.state)}
                    </span>
                  ) : null}
                </div>
                <p
                  className="mt-0.5 truncate text-[11.5px] leading-relaxed text-muted-foreground/90"
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
  accountAvatarUrl: string | null
  accountLogin: string | null
}

function UserMessage({ text, accountAvatarUrl, accountLogin }: UserMessageProps) {
  return (
    <div className="flex justify-end gap-3">
      <div className="flex min-w-0 max-w-[82%] flex-col items-end gap-1.5">
        <span className="px-1 text-[10.5px] font-medium uppercase tracking-wider text-muted-foreground/80">
          You
        </span>
        <div
          className={cn(
            'rounded-2xl px-4 py-2.5',
            'bg-primary/10 text-foreground',
            'ring-1 ring-inset ring-primary/15',
            'whitespace-pre-wrap break-words text-sm leading-relaxed',
          )}
        >
          {text}
        </div>
      </div>
      <UserAvatar avatarUrl={accountAvatarUrl} login={accountLogin} />
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

function AssistantMessage({ text }: { text: string }) {
  const segments = splitAssistantText(text)
  return (
    <div className="flex gap-3">
      <AgentAvatar />
      <div className="flex min-w-0 flex-1 flex-col items-start gap-2">
        <span className="px-0.5 text-[10.5px] font-medium uppercase tracking-wider text-muted-foreground/80">
          Agent
        </span>
        <div className="flex w-full min-w-0 flex-col items-start gap-2.5">
          {segments.map((segment, index) =>
            segment.kind === 'thinking' ? (
              <ThinkingBlock key={index} text={segment.text} />
            ) : (
              <ResponseBlock key={index} text={segment.text} />
            ),
          )}
        </div>
      </div>
    </div>
  )
}

function ResponseBlock({ text }: { text: string }) {
  return (
    <div className="w-full min-w-0 px-0.5 text-foreground">
      <Markdown text={text} />
    </div>
  )
}

function ThinkingBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false)
  const normalizedText = text.trim()
  const allLines = normalizedText.split(/\r?\n/).filter((line) => line.trim().length > 0)
  const previewText = allLines.slice(-4).join('\n')
  const hiddenLineCount = Math.max(0, allLines.length - 4)

  return (
    <div className="w-full max-w-full min-w-0 rounded-xl border border-dashed border-border/50 bg-muted/20 px-3 py-2">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-expanded={open}
        className={cn(
          'flex w-full items-center gap-2 text-left text-[11px] font-medium uppercase tracking-wider text-muted-foreground/90',
          'hover:text-foreground focus-visible:outline-none focus-visible:text-foreground',
        )}
      >
        <Brain className="h-3.5 w-3.5 text-primary/80" />
        <span>Thoughts</span>
        {!open && hiddenLineCount > 0 ? (
          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] normal-case tracking-normal text-muted-foreground/80">
            +{hiddenLineCount}
          </span>
        ) : null}
        <ChevronDown
          className={cn(
            'ml-auto h-3.5 w-3.5 transition-transform duration-150',
            open ? 'rotate-180' : 'rotate-0',
          )}
        />
      </button>
      {open ? (
        <div className="mt-2 border-t border-border/30 pt-2">
          <Markdown text={text} muted />
        </div>
      ) : previewText.length > 0 ? (
        <div className="mt-2">
          <Markdown text={previewText} muted compact />
        </div>
      ) : null}
    </div>
  )
}

function AgentActivityIndicator() {
  return (
    <div className="flex items-start gap-3" role="status" aria-label="Agent is thinking">
      <AgentAvatar />
      <div className="mt-0.5 flex min-w-0 items-center gap-2 rounded-full border border-border/40 bg-card/35 px-3 py-1.5 text-[12.5px] font-medium text-muted-foreground shadow-sm">
        <Loader2 className="h-3.5 w-3.5 animate-spin text-primary/80" aria-hidden="true" />
        <span>Thinking</span>
        <span className="flex items-center gap-0.5" aria-hidden="true">
          <span className="h-1 w-1 animate-pulse rounded-full bg-muted-foreground/70 [animation-delay:0ms]" />
          <span className="h-1 w-1 animate-pulse rounded-full bg-muted-foreground/70 [animation-delay:120ms]" />
          <span className="h-1 w-1 animate-pulse rounded-full bg-muted-foreground/70 [animation-delay:240ms]" />
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
        'flex items-start gap-2.5 rounded-xl border px-3.5 py-2.5',
        toneStyles.card,
      )}
    >
      <Icon className={cn('mt-[2px] h-4 w-4 shrink-0', toneStyles.icon)} />
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
    <div className="flex items-start gap-2.5 rounded-xl border border-destructive/30 bg-destructive/8 px-3.5 py-2.5 text-destructive">
      <XCircle className="mt-[2px] h-4 w-4 shrink-0" />
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
        'mt-[18px] flex h-7 w-7 shrink-0 items-center justify-center overflow-hidden rounded-full',
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
        <User className="h-3.5 w-3.5" />
      )}
    </span>
  )
}

function AgentAvatar() {
  return (
    <span
      aria-hidden="true"
      className="mt-[2px] flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-card/80 ring-1 ring-border/50"
    >
      <AppLogo className="h-3.5 w-3.5" />
    </span>
  )
}

interface ToolStatusIconProps {
  state: RuntimeStreamToolItemView['toolState'] | null
  className?: string
}

function ToolStatusIcon({ state, className }: ToolStatusIconProps) {
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

  return (
    <Icon
      aria-hidden="true"
      className={cn('h-3.5 w-3.5 shrink-0', tone, state === 'running' && 'animate-spin', className)}
    />
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
