
import { z } from 'zod'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
  toolResultSummarySchema,
  type ToolResultSummaryDto,
} from './shared'

export const MAX_RUNTIME_STREAM_ITEMS = 40
export const MAX_RUNTIME_STREAM_TRANSCRIPTS = 20
export const MAX_RUNTIME_STREAM_TOOL_CALLS = 20
export const MAX_RUNTIME_STREAM_SKILLS = 20
export const MAX_RUNTIME_STREAM_ACTIVITY = 20
export const MAX_RUNTIME_STREAM_ACTION_REQUIRED = 10

export const runtimeToolCallStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const runtimeSkillLifecycleStageSchema = z.enum(['discovery', 'install', 'invoke'])
export const runtimeSkillLifecycleResultSchema = z.enum(['succeeded', 'failed'])
export const runtimeSkillCacheStatusSchema = z.enum(['miss', 'hit', 'refreshed'])
export const runtimeSkillSourceSchema = z
  .object({
    repo: z.string().trim().min(1),
    path: z.string().trim().min(1),
    reference: z.string().trim().min(1),
    treeHash: z.string().regex(/^[0-9a-f]{40}$/, 'Runtime skill source tree hashes must be lowercase 40-character hex digests.'),
  })
  .strict()
export const runtimeSkillDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
  })
  .strict()
export const runtimeStreamItemKindSchema = z.enum([
  'transcript',
  'tool',
  'skill',
  'activity',
  'action_required',
  'complete',
  'failure',
])
export const runtimeStreamTranscriptRoleSchema = z.enum(['user', 'assistant', 'system', 'tool'])

export const runtimeStreamItemSchema = z
  .object({
    kind: runtimeStreamItemKindSchema,
    runId: z.string().trim().min(1),
    sequence: z.number().int().positive(),
    sessionId: nonEmptyOptionalTextSchema,
    flowId: nonEmptyOptionalTextSchema,
    text: z.string().min(1).nullable().optional(),
    transcriptRole: runtimeStreamTranscriptRoleSchema.nullable().optional(),
    toolCallId: nonEmptyOptionalTextSchema,
    toolName: nonEmptyOptionalTextSchema,
    toolState: runtimeToolCallStateSchema.nullable().optional(),
    toolSummary: toolResultSummarySchema.nullable().optional(),
    skillId: nonEmptyOptionalTextSchema,
    skillStage: runtimeSkillLifecycleStageSchema.nullable().optional(),
    skillResult: runtimeSkillLifecycleResultSchema.nullable().optional(),
    skillSource: runtimeSkillSourceSchema.nullable().optional(),
    skillCacheStatus: runtimeSkillCacheStatusSchema.nullable().optional(),
    skillDiagnostic: runtimeSkillDiagnosticSchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
    actionType: nonEmptyOptionalTextSchema,
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
    retryable: z.boolean().nullable().optional(),
    createdAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((item, ctx) => {
    const hasSkillMetadata =
      item.skillId != null
      || item.skillStage != null
      || item.skillResult != null
      || item.skillSource != null
      || item.skillCacheStatus != null
      || item.skillDiagnostic != null

    if (item.kind !== 'skill' && hasSkillMetadata) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['skillId'],
        message: `Xero received non-skill runtime item kind \`${item.kind}\` with skill lifecycle metadata.`,
      })
    }

    switch (item.kind) {
      case 'transcript':
        if (!item.text) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['text'],
            message: 'Xero received a runtime transcript item without a non-empty text field.',
          })
        }
        return
      case 'tool':
        if (!item.toolCallId) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['toolCallId'],
            message: 'Xero received a runtime tool item without a non-empty toolCallId field.',
          })
        }
        if (!item.toolName) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['toolName'],
            message: 'Xero received a runtime tool item without a non-empty toolName field.',
          })
        }
        if (!item.toolState) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['toolState'],
            message: 'Xero received a runtime tool item without a toolState value.',
          })
        }
        return
      case 'skill':
        if (!item.skillId) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillId'],
            message: 'Xero received a runtime skill item without a non-empty skillId field.',
          })
        }
        if (!item.skillStage) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillStage'],
            message: 'Xero received a runtime skill item without a skillStage value.',
          })
        }
        if (!item.skillResult) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillResult'],
            message: 'Xero received a runtime skill item without a skillResult value.',
          })
        }
        if (!item.skillSource) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillSource'],
            message: 'Xero received a runtime skill item without skillSource metadata.',
          })
        }
        if (!item.detail) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['detail'],
            message: 'Xero received a runtime skill item without a non-empty detail field.',
          })
        }
        if (item.skillResult === 'succeeded' && item.skillDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillDiagnostic'],
            message: 'Successful runtime skill items must not include skillDiagnostic payloads.',
          })
        }
        if (item.skillResult === 'failed' && !item.skillDiagnostic) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillDiagnostic'],
            message: 'Failed runtime skill items must include typed skillDiagnostic payloads.',
          })
        }
        if (item.skillStage === 'discovery' && item.skillCacheStatus) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillCacheStatus'],
            message: 'Discovery runtime skill items must omit skillCacheStatus because no install or invoke step has completed yet.',
          })
        }
        if ((item.skillStage === 'install' || item.skillStage === 'invoke') && item.skillResult === 'succeeded' && !item.skillCacheStatus) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['skillCacheStatus'],
            message: 'Successful install/invoke runtime skill items must include skillCacheStatus.',
          })
        }
        return
      case 'activity':
        if (!item.code) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['code'],
            message: 'Xero received a runtime activity item without a non-empty code field.',
          })
        }
        if (!item.title) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['title'],
            message: 'Xero received a runtime activity item without a non-empty title field.',
          })
        }
        return
      case 'action_required':
        if (!item.actionId) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['actionId'],
            message: 'Xero received a runtime action-required item without a non-empty actionId field.',
          })
        }
        if (!item.actionType) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['actionType'],
            message: 'Xero received a runtime action-required item without a non-empty actionType field.',
          })
        }
        if (!item.title) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['title'],
            message: 'Xero received a runtime action-required item without a non-empty title field.',
          })
        }
        if (!item.detail) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['detail'],
            message: 'Xero received a runtime action-required item without a non-empty detail field.',
          })
        }
        return
      case 'complete':
        if (!item.detail) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['detail'],
            message: 'Xero received a runtime completion item without a non-empty detail field.',
          })
        }
        return
      case 'failure':
        if (!item.code) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['code'],
            message: 'Xero received a runtime failure item without a non-empty code field.',
          })
        }
        if (!item.message) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['message'],
            message: 'Xero received a runtime failure item without a non-empty message field.',
          })
        }
        if (typeof item.retryable !== 'boolean') {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['retryable'],
            message: 'Xero received a runtime failure item without a retryable flag.',
          })
        }
        return
    }
  })

export const subscribeRuntimeStreamRequestSchema = z.object({
  projectId: z.string().trim().min(1),
  agentSessionId: z.string().trim().min(1),
  itemKinds: z.array(runtimeStreamItemKindSchema).min(1),
}).strict()

export const subscribeRuntimeStreamResponseSchema = z.object({
  projectId: z.string().trim().min(1),
  agentSessionId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  runId: z.string().trim().min(1),
  sessionId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  subscribedItemKinds: z.array(runtimeStreamItemKindSchema).min(1),
}).strict()

export type RuntimeToolCallStateDto = z.infer<typeof runtimeToolCallStateSchema>
export type RuntimeSkillLifecycleStageDto = z.infer<typeof runtimeSkillLifecycleStageSchema>
export type RuntimeSkillLifecycleResultDto = z.infer<typeof runtimeSkillLifecycleResultSchema>
export type RuntimeSkillCacheStatusDto = z.infer<typeof runtimeSkillCacheStatusSchema>
export type RuntimeSkillSourceDto = z.infer<typeof runtimeSkillSourceSchema>
export type RuntimeSkillDiagnosticDto = z.infer<typeof runtimeSkillDiagnosticSchema>
export type RuntimeStreamItemKindDto = z.infer<typeof runtimeStreamItemKindSchema>
export type RuntimeStreamTranscriptRoleDto = z.infer<typeof runtimeStreamTranscriptRoleSchema>
export type RuntimeStreamItemDto = z.infer<typeof runtimeStreamItemSchema>
export type SubscribeRuntimeStreamRequestDto = z.infer<typeof subscribeRuntimeStreamRequestSchema>
export type SubscribeRuntimeStreamResponseDto = z.infer<typeof subscribeRuntimeStreamResponseSchema>

export type RuntimeStreamStatus = 'idle' | 'subscribing' | 'replaying' | 'live' | 'complete' | 'stale' | 'error'

export interface RuntimeStreamIssueView {
  code: string
  message: string
  retryable: boolean
  observedAt: string
}

interface RuntimeStreamBaseItemView {
  id: string
  runId: string
  sequence: number
  createdAt: string
}

export interface RuntimeStreamTranscriptItemView extends RuntimeStreamBaseItemView {
  kind: 'transcript'
  text: string
  role: RuntimeStreamTranscriptRoleDto
}

export interface RuntimeStreamToolItemView extends RuntimeStreamBaseItemView {
  kind: 'tool'
  toolCallId: string
  toolName: string
  toolState: RuntimeToolCallStateDto
  detail: string | null
  toolSummary: ToolResultSummaryDto | null
}

export interface RuntimeStreamSkillItemView extends RuntimeStreamBaseItemView {
  kind: 'skill'
  skillId: string
  stage: RuntimeSkillLifecycleStageDto
  result: RuntimeSkillLifecycleResultDto
  detail: string
  source: RuntimeSkillSourceDto
  cacheStatus: RuntimeSkillCacheStatusDto | null
  diagnostic: RuntimeSkillDiagnosticDto | null
}

export interface RuntimeStreamActivityItemView extends RuntimeStreamBaseItemView {
  kind: 'activity'
  code: string
  title: string
  text?: string | null
  detail: string | null
}

export interface RuntimeStreamActionRequiredItemView extends RuntimeStreamBaseItemView {
  kind: 'action_required'
  actionId: string
  boundaryId: string | null
  actionType: string
  title: string
  detail: string
}

export interface RuntimeStreamCompleteItemView extends RuntimeStreamBaseItemView {
  kind: 'complete'
  detail: string
}

export interface RuntimeStreamFailureItemView extends RuntimeStreamBaseItemView {
  kind: 'failure'
  code: string
  message: string
  retryable: boolean
}

export type RuntimeStreamViewItem =
  | RuntimeStreamTranscriptItemView
  | RuntimeStreamToolItemView
  | RuntimeStreamSkillItemView
  | RuntimeStreamActivityItemView
  | RuntimeStreamActionRequiredItemView
  | RuntimeStreamCompleteItemView
  | RuntimeStreamFailureItemView

export interface RuntimeStreamEventDto {
  projectId: string
  agentSessionId: string
  runtimeKind: string
  runId: string
  sessionId: string
  flowId: string | null
  subscribedItemKinds: RuntimeStreamItemKindDto[]
  item: RuntimeStreamItemDto
}

export interface RuntimeStreamView {
  projectId: string
  agentSessionId: string
  runtimeKind: string
  runId: string | null
  sessionId: string | null
  flowId: string | null
  subscribedItemKinds: RuntimeStreamItemKindDto[]
  status: RuntimeStreamStatus
  items: RuntimeStreamViewItem[]
  transcriptItems: RuntimeStreamTranscriptItemView[]
  toolCalls: RuntimeStreamToolItemView[]
  skillItems: RuntimeStreamSkillItemView[]
  activityItems: RuntimeStreamActivityItemView[]
  actionRequired: RuntimeStreamActionRequiredItemView[]
  completion: RuntimeStreamCompleteItemView | null
  failure: RuntimeStreamFailureItemView | null
  lastIssue: RuntimeStreamIssueView | null
  lastItemAt: string | null
  lastSequence: number | null
}

function capRecent<T>(values: T[], limit: number): T[] {
  return values.length <= limit ? values : values.slice(values.length - limit)
}

function compareRuntimeStreamItemsBySequence(
  left: RuntimeStreamViewItem,
  right: RuntimeStreamViewItem,
): number {
  return left.sequence === right.sequence
    ? left.id.localeCompare(right.id)
    : left.sequence - right.sequence
}

function capRuntimeTimelineItems(
  currentItems: RuntimeStreamViewItem[],
  transcriptItems: RuntimeStreamTranscriptItemView[],
  nextItem: RuntimeStreamViewItem,
): RuntimeStreamViewItem[] {
  let nonTranscriptItems: RuntimeStreamViewItem[] = currentItems.filter((item) => item.kind !== 'transcript')

  if (nextItem.kind === 'tool') {
    nonTranscriptItems = [
      ...nonTranscriptItems.filter(
        (item) => !(item.kind === 'tool' && item.toolCallId === nextItem.toolCallId),
      ),
      nextItem,
    ]
  } else if (nextItem.kind === 'action_required') {
    nonTranscriptItems = [
      ...nonTranscriptItems.filter(
        (item) =>
          !(
            item.kind === 'action_required' &&
            item.runId === nextItem.runId &&
            item.actionId === nextItem.actionId
          ),
      ),
      nextItem,
    ]
  } else if (isReasoningActivityItem(nextItem)) {
    nonTranscriptItems = mergeReasoningActivityTimelineItem(nonTranscriptItems, nextItem)
  } else if (nextItem.kind !== 'transcript') {
    nonTranscriptItems = [...nonTranscriptItems, nextItem]
  }

  return [
    ...transcriptItems,
    ...capRecent(nonTranscriptItems, MAX_RUNTIME_STREAM_ITEMS),
  ].sort(compareRuntimeStreamItemsBySequence)
}

function isReasoningActivityItem(
  item: RuntimeStreamViewItem,
): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === 'owned_agent_reasoning'
}

function reasoningActivityText(item: RuntimeStreamActivityItemView): string {
  return item.text ?? item.detail ?? ''
}

function mergeReasoningActivityTimelineItem(
  currentItems: RuntimeStreamViewItem[],
  nextItem: RuntimeStreamActivityItemView,
): RuntimeStreamViewItem[] {
  const previousItem = currentItems.at(-1)
  if (!previousItem || !isReasoningActivityItem(previousItem) || previousItem.runId !== nextItem.runId) {
    return [...currentItems, nextItem]
  }

  const mergedText = `${reasoningActivityText(previousItem)}${reasoningActivityText(nextItem)}`
  return [
    ...currentItems.slice(0, -1),
    {
      ...previousItem,
      sequence: nextItem.sequence,
      createdAt: nextItem.createdAt,
      text: mergedText,
      detail: mergedText.trim().length > 0
        ? mergedText.trim()
        : previousItem.detail ?? nextItem.detail,
    },
  ]
}

function uniqueRuntimeStreamKinds(kinds: RuntimeStreamItemKindDto[]): RuntimeStreamItemKindDto[] {
  return Array.from(new Set(kinds))
}

function ensureRuntimeStreamText(value: string | null | undefined, field: string, kind: string): string {
  const normalized = normalizeOptionalText(value)
  if (!normalized) {
    throw new Error(`Xero received a ${kind} item without a non-empty ${field}.`)
  }

  return normalized
}

function ensureRuntimeTranscriptText(value: string | null | undefined): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error('Xero received a transcript item without non-empty text.')
  }

  return value
}

function mergeRuntimeTranscriptItems(
  currentItems: RuntimeStreamTranscriptItemView[],
  nextItem: RuntimeStreamTranscriptItemView,
): RuntimeStreamTranscriptItemView[] {
  if (nextItem.role !== 'assistant') {
    return capRecent([...currentItems, nextItem], MAX_RUNTIME_STREAM_TRANSCRIPTS)
  }

  const previousItem = currentItems.at(-1)
  if (previousItem?.runId !== nextItem.runId || previousItem.role !== nextItem.role) {
    return capRecent([...currentItems, nextItem], MAX_RUNTIME_STREAM_TRANSCRIPTS)
  }

  const mergedItem: RuntimeStreamTranscriptItemView = {
    ...previousItem,
    id: previousItem.id,
    sequence: nextItem.sequence,
    createdAt: nextItem.createdAt,
    text: `${previousItem.text}${nextItem.text}`,
  }

  return capRecent([...currentItems.slice(0, -1), mergedItem], MAX_RUNTIME_STREAM_TRANSCRIPTS)
}

function normalizeRuntimeToolSummary(summary: ToolResultSummaryDto | null | undefined): ToolResultSummaryDto | null {
  if (!summary) {
    return null
  }

  const parsedSummary = toolResultSummarySchema.safeParse(summary)
  if (!parsedSummary.success) {
    const firstIssue = parsedSummary.error.issues[0]
    const issuePath =
      firstIssue && firstIssue.path.length > 0
        ? firstIssue.path.map(String).join('.')
        : 'toolSummary'
    const issueMessage = firstIssue?.message ?? 'Invalid runtime tool summary payload.'

    throw new Error(
      `Xero received a runtime tool item with malformed toolSummary payload (${issuePath}: ${issueMessage}).`,
    )
  }

  return parsedSummary.data
}

function runtimeStreamItemId(kind: RuntimeStreamItemKindDto, runId: string, sequence: number): string {
  return `${kind}:${runId}:${sequence}`
}

function runtimeStreamActionRequiredItemId(runId: string, actionId: string): string {
  return `action_required:${runId}:${actionId}`
}

function getRecoveredRuntimeStreamStatus(base: RuntimeStreamView): RuntimeStreamStatus {
  if (base.completion) {
    return 'complete'
  }

  if (base.failure) {
    return base.failure.retryable ? 'stale' : 'error'
  }

  return 'live'
}

function mergeRuntimeStreamMetadata(
  base: RuntimeStreamView,
  event: RuntimeStreamEventDto,
  status: RuntimeStreamStatus,
): RuntimeStreamView {
  return {
    ...base,
    agentSessionId: event.agentSessionId,
    runtimeKind: normalizeText(event.runtimeKind, base.runtimeKind),
    runId: normalizeOptionalText(event.runId) ?? base.runId,
    sessionId: normalizeOptionalText(event.sessionId) ?? base.sessionId,
    flowId: normalizeOptionalText(event.flowId) ?? base.flowId,
    subscribedItemKinds: uniqueRuntimeStreamKinds(event.subscribedItemKinds),
    status,
    lastIssue: base.failure ? base.lastIssue : null,
  }
}

function normalizeRuntimeStreamItem(event: RuntimeStreamEventDto): RuntimeStreamViewItem {
  const projectId = normalizeOptionalText(event.projectId)
  if (!projectId) {
    throw new Error('Xero received a runtime stream item without a selected project id.')
  }

  const expectedRunId = normalizeOptionalText(event.runId)
  const expectedSessionId = normalizeOptionalText(event.sessionId)
  const eventFlowId = normalizeOptionalText(event.flowId)
  const itemRunId = normalizeOptionalText(event.item.runId)
  const itemSessionId = normalizeOptionalText(event.item.sessionId)
  const itemFlowId = normalizeOptionalText(event.item.flowId)

  if (!expectedRunId || !itemRunId || itemRunId !== expectedRunId) {
    throw new Error('Xero received a runtime stream item for an unexpected run id at the desktop adapter boundary.')
  }

  if (expectedSessionId && itemSessionId && itemSessionId !== expectedSessionId) {
    throw new Error(
      `Xero received a runtime stream item for an unexpected session (${itemSessionId}) while ${expectedSessionId} is active.`,
    )
  }

  if (eventFlowId && itemFlowId && itemFlowId !== eventFlowId) {
    throw new Error(`Xero received a runtime stream item for an unexpected auth flow (${itemFlowId}).`)
  }

  switch (event.item.kind) {
    case 'transcript': {
      const text = ensureRuntimeTranscriptText(event.item.text)
      return {
        id: runtimeStreamItemId('transcript', itemRunId, event.item.sequence),
        kind: 'transcript',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        text,
        role: event.item.transcriptRole ?? 'assistant',
      }
    }
    case 'tool': {
      const toolCallId = ensureRuntimeStreamText(event.item.toolCallId, 'toolCallId', 'tool')
      const toolName = ensureRuntimeStreamText(event.item.toolName, 'toolName', 'tool')
      const toolState = event.item.toolState
      if (!toolState) {
        throw new Error('Xero received a runtime tool item without a toolState value.')
      }

      return {
        id: runtimeStreamItemId('tool', itemRunId, event.item.sequence),
        kind: 'tool',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        toolCallId,
        toolName,
        toolState,
        detail: normalizeOptionalText(event.item.detail),
        toolSummary: normalizeRuntimeToolSummary(event.item.toolSummary),
      }
    }
    case 'skill': {
      const skillId = ensureRuntimeStreamText(event.item.skillId, 'skillId', 'skill')
      const stage = event.item.skillStage
      const result = event.item.skillResult
      const source = event.item.skillSource
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'skill')

      if (!stage) {
        throw new Error('Xero received a runtime skill item without a skillStage value.')
      }
      if (!result) {
        throw new Error('Xero received a runtime skill item without a skillResult value.')
      }
      if (!source) {
        throw new Error('Xero received a runtime skill item without skillSource metadata.')
      }

      return {
        id: runtimeStreamItemId('skill', itemRunId, event.item.sequence),
        kind: 'skill',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        skillId,
        stage,
        result,
        detail,
        source,
        cacheStatus: event.item.skillCacheStatus ?? null,
        diagnostic: event.item.skillDiagnostic ?? null,
      }
    }
    case 'activity': {
      const code = ensureRuntimeStreamText(event.item.code, 'code', 'activity')
      const title = ensureRuntimeStreamText(event.item.title, 'title', 'activity')
      return {
        id: runtimeStreamItemId('activity', itemRunId, event.item.sequence),
        kind: 'activity',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        code,
        title,
        text: typeof event.item.text === 'string' && event.item.text.length > 0
          ? event.item.text
          : null,
        detail: normalizeOptionalText(event.item.detail),
      }
    }
    case 'action_required': {
      const actionId = ensureRuntimeStreamText(event.item.actionId, 'actionId', 'action-required')
      const actionType = ensureRuntimeStreamText(event.item.actionType, 'actionType', 'action-required')
      const title = ensureRuntimeStreamText(event.item.title, 'title', 'action-required')
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'action-required')
      return {
        id: runtimeStreamActionRequiredItemId(itemRunId, actionId),
        kind: 'action_required',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        actionId,
        boundaryId: normalizeOptionalText(event.item.boundaryId),
        actionType,
        title,
        detail,
      }
    }
    case 'complete': {
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'complete')
      return {
        id: runtimeStreamItemId('complete', itemRunId, event.item.sequence),
        kind: 'complete',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        detail,
      }
    }
    case 'failure': {
      const code = ensureRuntimeStreamText(event.item.code, 'code', 'failure')
      const message = ensureRuntimeStreamText(event.item.message, 'message', 'failure')
      if (typeof event.item.retryable !== 'boolean') {
        throw new Error('Xero received a runtime failure item without a retryable flag.')
      }

      return {
        id: runtimeStreamItemId('failure', itemRunId, event.item.sequence),
        kind: 'failure',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        code,
        message,
        retryable: event.item.retryable,
      }
    }
  }
}

export function createRuntimeStreamView(options: {
  projectId: string
  agentSessionId: string
  runtimeKind: string
  runId?: string | null
  sessionId?: string | null
  flowId?: string | null
  subscribedItemKinds?: RuntimeStreamItemKindDto[]
  status?: RuntimeStreamStatus
}): RuntimeStreamView {
  return {
    projectId: options.projectId,
    agentSessionId: options.agentSessionId,
    runtimeKind: normalizeText(options.runtimeKind, 'openai_codex'),
    runId: normalizeOptionalText(options.runId),
    sessionId: normalizeOptionalText(options.sessionId),
    flowId: normalizeOptionalText(options.flowId),
    subscribedItemKinds: uniqueRuntimeStreamKinds(options.subscribedItemKinds ?? []),
    status: options.status ?? 'idle',
    items: [],
    transcriptItems: [],
    toolCalls: [],
    skillItems: [],
    activityItems: [],
    actionRequired: [],
    completion: null,
    failure: null,
    lastIssue: null,
    lastItemAt: null,
    lastSequence: null,
  }
}

export function createRuntimeStreamFromSubscription(
  response: SubscribeRuntimeStreamResponseDto,
  status: RuntimeStreamStatus = 'subscribing',
): RuntimeStreamView {
  return createRuntimeStreamView({
    projectId: response.projectId,
    agentSessionId: response.agentSessionId,
    runtimeKind: response.runtimeKind,
    runId: response.runId,
    sessionId: response.sessionId,
    flowId: response.flowId ?? null,
    subscribedItemKinds: response.subscribedItemKinds,
    status,
  })
}

export function mergeRuntimeStreamEvent(
  current: RuntimeStreamView | null,
  event: RuntimeStreamEventDto,
): RuntimeStreamView {
  if (current && current.projectId !== event.projectId) {
    throw new Error(
      `Xero received a runtime stream item for ${event.projectId} while ${current.projectId} is the selected project.`,
    )
  }

  if (current && current.agentSessionId !== event.agentSessionId) {
    throw new Error(
      `Xero received a runtime stream item for agent session ${event.agentSessionId} while ${current.agentSessionId} is active.`,
    )
  }

  if (current?.runId && current.runId !== event.runId) {
    return current
  }

  const base =
    current ??
    createRuntimeStreamView({
      projectId: event.projectId,
      agentSessionId: event.agentSessionId,
      runtimeKind: event.runtimeKind,
      runId: event.runId,
      sessionId: event.sessionId,
      flowId: event.flowId,
      subscribedItemKinds: event.subscribedItemKinds,
      status: 'subscribing',
    })

  if (event.item.sequence < 1) {
    throw new Error(`non-monotonic runtime stream sequence ${event.item.sequence}`)
  }

  if (base.lastSequence !== null) {
    if (event.item.sequence <= base.lastSequence) {
      return mergeRuntimeStreamMetadata(base, event, getRecoveredRuntimeStreamStatus(base))
    }
  }

  const nextItem = normalizeRuntimeStreamItem(event)
  const nextToolCalls =
    nextItem.kind === 'tool'
      ? capRecent(
          [
            ...base.toolCalls.filter((toolCall) => toolCall.toolCallId !== nextItem.toolCallId),
            nextItem,
          ],
          MAX_RUNTIME_STREAM_TOOL_CALLS,
        )
      : base.toolCalls
  const nextSkillItems =
    nextItem.kind === 'skill'
      ? capRecent([...base.skillItems, nextItem], MAX_RUNTIME_STREAM_SKILLS)
      : base.skillItems
  const nextTranscriptItems =
    nextItem.kind === 'transcript'
      ? mergeRuntimeTranscriptItems(base.transcriptItems, nextItem)
      : base.transcriptItems
  const nextItems = capRuntimeTimelineItems(base.items, nextTranscriptItems, nextItem)
  const nextActivityItems =
    nextItem.kind === 'activity'
      ? capRecent([...base.activityItems, nextItem], MAX_RUNTIME_STREAM_ACTIVITY)
      : base.activityItems
  const nextActionRequired =
    nextItem.kind === 'action_required'
      ? capRecent(
          [
            ...base.actionRequired.filter((actionRequiredItem) => actionRequiredItem.actionId !== nextItem.actionId),
            nextItem,
          ],
          MAX_RUNTIME_STREAM_ACTION_REQUIRED,
        )
      : base.actionRequired

  return {
    ...base,
    agentSessionId: event.agentSessionId,
    runtimeKind: normalizeText(event.runtimeKind, base.runtimeKind),
    runId: normalizeOptionalText(event.runId) ?? base.runId,
    sessionId: normalizeOptionalText(event.sessionId) ?? base.sessionId,
    flowId: normalizeOptionalText(event.flowId) ?? base.flowId,
    subscribedItemKinds: uniqueRuntimeStreamKinds(event.subscribedItemKinds),
    status:
      nextItem.kind === 'complete'
        ? 'complete'
        : nextItem.kind === 'failure'
          ? nextItem.retryable
            ? 'stale'
            : 'error'
          : 'live',
    items: nextItems,
    transcriptItems: nextTranscriptItems,
    toolCalls: nextToolCalls,
    skillItems: nextSkillItems,
    activityItems: nextActivityItems,
    actionRequired: nextActionRequired,
    completion: nextItem.kind === 'complete' ? nextItem : base.completion,
    failure: nextItem.kind === 'failure' ? nextItem : null,
    lastIssue:
      nextItem.kind === 'failure'
        ? {
            code: nextItem.code,
            message: nextItem.message,
            retryable: nextItem.retryable,
            observedAt: nextItem.createdAt,
          }
        : null,
    lastItemAt: nextItem.createdAt,
    lastSequence: nextItem.sequence,
  }
}

export function applyRuntimeStreamIssue(
  current: RuntimeStreamView | null,
  options: {
    projectId: string
    agentSessionId: string
    runtimeKind: string
    runId?: string | null
    sessionId?: string | null
    flowId?: string | null
    subscribedItemKinds?: RuntimeStreamItemKindDto[]
    code: string
    message: string
    retryable: boolean
    observedAt?: string
  },
): RuntimeStreamView {
  const observedAt = options.observedAt ?? new Date().toISOString()
  const base =
    current ??
    createRuntimeStreamView({
      projectId: options.projectId,
      agentSessionId: options.agentSessionId,
      runtimeKind: options.runtimeKind,
      runId: options.runId,
      sessionId: options.sessionId,
      flowId: options.flowId,
      subscribedItemKinds: options.subscribedItemKinds,
      status: options.retryable ? 'stale' : 'error',
    })

  return {
    ...base,
    agentSessionId: normalizeText(options.agentSessionId, base.agentSessionId),
    runtimeKind: normalizeText(options.runtimeKind, base.runtimeKind),
    runId: normalizeOptionalText(options.runId) ?? base.runId,
    sessionId: normalizeOptionalText(options.sessionId) ?? base.sessionId,
    flowId: normalizeOptionalText(options.flowId) ?? base.flowId,
    subscribedItemKinds: uniqueRuntimeStreamKinds(options.subscribedItemKinds ?? base.subscribedItemKinds),
    status: options.retryable ? 'stale' : 'error',
    lastIssue: {
      code: normalizeText(options.code, 'runtime_stream_issue'),
      message: normalizeText(options.message, 'Xero could not project runtime activity for this project.'),
      retryable: options.retryable,
      observedAt,
    },
    lastItemAt: base.lastItemAt ?? observedAt,
    lastSequence: base.lastSequence,
  }
}

export function getRuntimeStreamStatusLabel(status: RuntimeStreamStatus): string {
  switch (status) {
    case 'idle':
      return 'No live stream'
    case 'subscribing':
      return 'Connecting stream'
    case 'replaying':
      return 'Replaying recent activity'
    case 'live':
      return 'Streaming live activity'
    case 'complete':
      return 'Stream complete'
    case 'stale':
      return 'Stream stale'
    case 'error':
      return 'Stream failed'
  }
}
