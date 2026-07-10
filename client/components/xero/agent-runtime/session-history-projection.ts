import type {
  RunTranscriptSummaryDto,
  SessionTranscriptDto,
  SessionTranscriptItemDto,
} from '@/src/lib/xero-model'

import type { ConversationTurn } from '@xero/ui/components/transcript/conversation-section'
import {
  promoteActionMediaIntoFollowingAssistantMessages,
  runtimeMediaAttachmentsToConversation,
} from '@xero/ui/components/transcript/runtime-media'
import { cleanPersistedRoutingContinuationTurns } from './routing-continuation'

const HANDED_OFF_RUN_STATUS = 'handed_off'
const MAX_HISTORICAL_CONVERSATION_TURNS = 80

interface BuildHistoricalConversationTurnsOptions {
  /**
   * The runId currently driving the live runtime stream. Items belonging to
   * this run are excluded from the historical projection so the live stream
   * remains the single source of truth for the active run.
   *
   * Pass `null` when there is no active run (e.g. before the stream attaches
   * or after a terminal completion); in that case all transcript items are
   * eligible.
   */
  activeRunId: string | null
}

/**
 * Project a `SessionTranscriptDto` into the chronological `ConversationTurn[]`
 * that the conversation pane prepends ahead of the live runtime stream.
 *
 * The projection:
 *   - keeps only user/assistant message items (other kinds are surfaced by
 *     the live stream and don't belong in static history),
 *   - filters out items belonging to the active run (the live stream covers
 *     them and would otherwise duplicate),
 *   - inserts a `handoff_notice` turn between consecutive runs whenever the
 *     prior run terminated with status `handed_off`, so the user sees an
 *     inline marker where the runtime swapped runs underneath them.
 */
export function buildHistoricalConversationTurns(
  transcript: SessionTranscriptDto,
  { activeRunId }: BuildHistoricalConversationTurnsOptions,
): ConversationTurn[] {
  const runsById = new Map<string, RunTranscriptSummaryDto>()
  for (const run of transcript.runs) {
    runsById.set(run.runId, run)
  }
  const displayPolicy = buildMessageDisplayPolicy(transcript.items)

  // The successor lookup is keyed off the run order returned by the transcript
  // so we can attach a trailing handoff_notice when the active run *is* the
  // handoff target and therefore has no items in the historical projection.
  const successorByRunId = new Map<string, string>()
  for (let index = 0; index < transcript.runs.length - 1; index += 1) {
    successorByRunId.set(transcript.runs[index].runId, transcript.runs[index + 1].runId)
  }

  const eligibleItems = transcript.items
    .filter((item) => item.runId !== activeRunId)
    .filter((item) =>
      isDisplayableUserOrAssistantMessage(item, displayPolicy) ||
      isDisplayableMediaToolResult(item) ||
      isDisplayableRouteRequest(item),
    )

  const turns: ConversationTurn[] = []
  let previousRunId: string | null = null
  let lastSequence = 0

  for (const item of eligibleItems) {
    if (previousRunId && previousRunId !== item.runId) {
      const previousRun = runsById.get(previousRunId)
      if (previousRun?.status === HANDED_OFF_RUN_STATUS) {
        turns.push({
          id: `handoff_notice:${previousRunId}->${item.runId}`,
          kind: 'handoff_notice',
          sequence: item.sequence,
          sourceRunId: previousRunId,
          targetRunId: item.runId,
        })
      }
    }
    previousRunId = item.runId
    lastSequence = item.sequence

    if (isDisplayableMediaToolResult(item)) {
      turns.push(toMediaToolTurn(item))
    } else if (isDisplayableRouteRequest(item)) {
      turns.push(toRoutingSuggestionTurn(item))
    } else {
      const messageTurn = toMessageTurn(item)
      turns.push(messageTurn)
    }
  }

  // Trailing handoff_notice: covers the case where the final emitted item
  // belongs to a handed_off run whose successor is filtered out (typically
  // because the successor *is* the active run, so its items live in the live
  // stream). Without this row the user would see source-run items end abruptly
  // with no marker that the conversation continues in a different run below.
  if (previousRunId) {
    const previousRun = runsById.get(previousRunId)
    const successorRunId = successorByRunId.get(previousRunId)
    if (
      previousRun?.status === HANDED_OFF_RUN_STATUS &&
      successorRunId &&
      successorRunId === activeRunId
    ) {
      turns.push({
        id: `handoff_notice:${previousRunId}->${successorRunId}:trailing`,
        kind: 'handoff_notice',
        sequence: lastSequence + 1,
        sourceRunId: previousRunId,
        targetRunId: successorRunId,
      })
    }
  }

  const canonicalTurns = cleanPersistedRoutingContinuationTurns(turns)
  const promotedTurns = promoteActionMediaIntoFollowingAssistantMessages(canonicalTurns)

  if (promotedTurns.length <= MAX_HISTORICAL_CONVERSATION_TURNS) {
    return promotedTurns
  }

  return promotedTurns.slice(-MAX_HISTORICAL_CONVERSATION_TURNS)
}

interface MessageDisplayPolicy {
  runIdsWithUserMessages: Set<string>
}

function hasMessageText(item: SessionTranscriptItemDto): boolean {
  return (item.text ?? '').trim().length > 0
}

function hasMediaAttachments(item: SessionTranscriptItemDto): boolean {
  return Boolean(item.mediaAttachments?.length)
}

function buildMessageDisplayPolicy(items: readonly SessionTranscriptItemDto[]): MessageDisplayPolicy {
  const runIdsWithUserMessages = new Set<string>()

  for (const item of items) {
    if (item.kind !== 'message' || !hasMessageText(item)) {
      continue
    }

    if (item.sourceTable === 'agent_messages' && item.actor === 'user') {
      runIdsWithUserMessages.add(item.runId)
    }
  }

  return {
    runIdsWithUserMessages,
  }
}

function isDisplayableUserOrAssistantMessage(
  item: SessionTranscriptItemDto,
  policy: MessageDisplayPolicy,
): boolean {
  if (item.kind !== 'message') return false
  if (item.actor !== 'user' && item.actor !== 'assistant') return false
  if (!hasMessageText(item) && !hasMediaAttachments(item)) return false

  if (item.sourceTable === 'agent_messages') {
    return true
  }

  if (item.sourceTable === 'agent_runs' || item.sourceTable === 'runtime_runs') {
    return item.actor === 'user' && !policy.runIdsWithUserMessages.has(item.runId)
  }

  if (item.sourceTable === 'agent_events') {
    return false
  }

  return false
}

function isDisplayableMediaToolResult(item: SessionTranscriptItemDto): boolean {
  return item.kind === 'tool_result' && item.actor === 'tool' && hasMediaAttachments(item)
}

function isDisplayableRouteRequest(item: SessionTranscriptItemDto): boolean {
  return item.kind === 'route_request' && item.routeRequest?.policyDecision === 'approved'
}

function toMessageTurn(item: SessionTranscriptItemDto): Extract<ConversationTurn, { kind: 'message' }> {
  return {
    // Match the live runtime-stream transcript id so a row can move from the
    // live projection into the historical projection without remounting.
    id: `transcript:${item.runId}:${item.sequence}`,
    kind: 'message',
    role: item.actor === 'user' ? 'user' : 'assistant',
    sequence: item.sequence,
    text: item.text ?? '',
    createdAt: item.createdAt,
    attachments: runtimeMediaAttachmentsToConversation(item.mediaAttachments),
  }
}

function toMediaToolTurn(item: SessionTranscriptItemDto): Extract<ConversationTurn, { kind: 'action' }> {
  const toolName = item.toolName ?? 'tool'
  const title = item.title ?? `Tool result · ${toolName}`
  return {
    id: `tool:${item.runId}:${item.toolCallId ?? item.sourceId}:${item.sequence}`,
    kind: 'action',
    sequence: item.sequence,
    toolCallId: item.toolCallId ?? item.sourceId,
    toolName,
    title,
    detail: item.text ?? 'Image output',
    detailRows: [],
    mediaAttachments: runtimeMediaAttachmentsToConversation(item.mediaAttachments),
    state: item.toolState ?? 'succeeded',
    defaultOpen: true,
  }
}

function toRoutingSuggestionTurn(
  item: SessionTranscriptItemDto,
): Extract<ConversationTurn, { kind: 'routing_suggestion' }> {
  const request = item.routeRequest
  if (!request) {
    throw new Error('Route-request transcript item is missing typed route metadata.')
  }
  return {
    id: `routing_suggestion:route_request:${item.runId}:${request.requestId}`,
    kind: 'routing_suggestion',
    sequence: item.sequence,
    targetKind: request.targetKind,
    targetAgentId: request.targetAgentId,
    targetAgentDefinitionId: request.targetAgentDefinitionId ?? null,
    targetAgentDefinitionVersion: request.targetAgentDefinitionVersion ?? null,
    targetLabel: request.targetLabel,
    reason: request.reason,
    summary: request.summary,
    autoRoutable: request.autoRoutable,
    isResolved: true,
    acceptedTarget: null,
    acceptedTargetAgentDefinitionId: null,
    acceptedTargetLabel: null,
    routingResolutionMode: null,
  }
}
