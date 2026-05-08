import type {
  RunTranscriptSummaryDto,
  SessionTranscriptDto,
  SessionTranscriptItemDto,
} from '@/src/lib/xero-model'

import type { ConversationTurn } from './conversation-section'

const HANDED_OFF_RUN_STATUS = 'handed_off'

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

  // The successor lookup is keyed off the run order returned by the transcript
  // so we can attach a trailing handoff_notice when the active run *is* the
  // handoff target and therefore has no items in the historical projection.
  const successorByRunId = new Map<string, string>()
  for (let index = 0; index < transcript.runs.length - 1; index += 1) {
    successorByRunId.set(transcript.runs[index].runId, transcript.runs[index + 1].runId)
  }

  const eligibleItems = transcript.items
    .filter((item) => item.runId !== activeRunId)
    .filter((item) => isUserOrAssistantMessage(item))

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

    turns.push(toMessageTurn(item))
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

  return turns
}

function isUserOrAssistantMessage(item: SessionTranscriptItemDto): boolean {
  if (item.kind !== 'message') return false
  if (item.actor !== 'user' && item.actor !== 'assistant') return false
  const text = item.text ?? ''
  return text.length > 0
}

function toMessageTurn(item: SessionTranscriptItemDto): Extract<ConversationTurn, { kind: 'message' }> {
  return {
    id: item.itemId,
    kind: 'message',
    role: item.actor === 'user' ? 'user' : 'assistant',
    sequence: item.sequence,
    text: item.text ?? '',
  }
}
