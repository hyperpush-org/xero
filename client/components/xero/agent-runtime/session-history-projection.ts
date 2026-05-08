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

  const eligibleItems = transcript.items
    .filter((item) => item.runId !== activeRunId)
    .filter((item) => isUserOrAssistantMessage(item))

  const turns: ConversationTurn[] = []
  let previousRunId: string | null = null

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

    turns.push(toMessageTurn(item))
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
