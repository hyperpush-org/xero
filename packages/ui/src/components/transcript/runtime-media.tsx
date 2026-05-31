import type { RuntimeStreamMediaAttachmentDto } from '../../model'
import type {
  ConversationMessageAttachment,
  ConversationTurn,
} from './conversation-section'

type ActionTurn = Extract<ConversationTurn, { kind: 'action' }>
type ActionMediaCarrier = Pick<ActionTurn, 'state' | 'mediaAttachments'>

export function runtimeMediaAttachmentsToConversation(
  attachments: readonly RuntimeStreamMediaAttachmentDto[] | null | undefined,
): ConversationMessageAttachment[] | undefined {
  if (!attachments?.length) return undefined
  return attachments.map((attachment) => {
    const originalName =
      attachment.title?.trim() ||
      (attachment.source.kind === 'app_data_path'
        ? attachment.source.absolutePath.split(/[\\/]/).pop()
        : null) ||
      attachment.id
    const absolutePath =
      attachment.source.kind === 'app_data_path'
        ? attachment.source.absolutePath
        : attachment.source.kind === 'artifact'
          ? attachment.source.absolutePath ?? undefined
          : undefined
    return {
      id: attachment.id,
      kind: attachment.kind,
      mediaType: attachment.mediaType,
      originalName,
      sizeBytes: attachment.sizeBytes ?? 0,
      title: attachment.title ?? null,
      alt: attachment.alt ?? null,
      width: attachment.width ?? null,
      height: attachment.height ?? null,
      source: attachment.source,
      renderUrl: attachment.renderUrl ?? null,
      previewSrc:
        attachment.renderUrl ??
        (attachment.source.kind === 'data_url'
          ? attachment.source.dataUrl
          : undefined),
      absolutePath,
    }
  })
}

export function mergeConversationAttachments(
  existing: ConversationMessageAttachment[] | undefined,
  incoming: ConversationMessageAttachment[] | undefined,
): ConversationMessageAttachment[] | undefined {
  if (!existing?.length) return incoming
  if (!incoming?.length) return existing
  const merged = existing.slice()
  const seen = new Set(existing.map((attachment) => attachment.id))
  for (const attachment of incoming) {
    if (seen.has(attachment.id)) continue
    seen.add(attachment.id)
    merged.push(attachment)
  }
  return merged
}

export function promotableActionAttachments(
  action: ActionMediaCarrier,
): ConversationMessageAttachment[] | undefined {
  if (action.state === 'failed') return undefined
  const attachments = action.mediaAttachments?.filter(
    (attachment) => attachment.kind === 'image',
  )
  return attachments && attachments.length > 0 ? attachments : undefined
}

export function promoteActionMediaIntoFollowingAssistantMessages(
  turns: ConversationTurn[],
): ConversationTurn[] {
  let pendingAttachments: ConversationMessageAttachment[] | undefined
  for (const turn of turns) {
    if (turn.kind === 'message' && turn.role === 'assistant') {
      turn.attachments = mergeConversationAttachments(
        turn.attachments,
        pendingAttachments,
      )
      pendingAttachments = undefined
      continue
    }
    if (
      (turn.kind === 'message' && turn.role === 'user') ||
      turn.kind === 'failure'
    ) {
      pendingAttachments = undefined
      continue
    }
    if (turn.kind === 'action') {
      pendingAttachments = mergeConversationAttachments(
        pendingAttachments,
        promotableActionAttachments(turn),
      )
      continue
    }
    if (turn.kind === 'action_group') {
      for (const action of turn.actions) {
        pendingAttachments = mergeConversationAttachments(
          pendingAttachments,
          promotableActionAttachments(action),
        )
      }
      continue
    }
    if (turn.kind === 'subagent_group') {
      turn.children = promoteActionMediaIntoFollowingAssistantMessages(
        turn.children,
      )
    }
  }
  return turns
}
