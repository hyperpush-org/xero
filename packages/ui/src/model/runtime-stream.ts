
import { z } from 'zod'
import { estimateUtf16Bytes } from '../lib/byte-budget-cache'
import {
  codePatchAvailabilitySchema,
  type CodePatchAvailabilityDto,
} from './code-history'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
  toolResultSummarySchema,
  type ToolResultSummaryDto,
} from './shared'

export const MAX_RUNTIME_STREAM_ACTION_REQUIRED = 10
export const MAX_RUNTIME_STREAM_PLAN_ITEMS = 50
export const MAX_RUNTIME_STREAM_ACTION_REQUIRED_OPTIONS = 20
const OWNED_AGENT_REASONING_ACTIVITY_CODE = 'owned_agent_reasoning'
const OWNED_AGENT_REASONING_BOUNDARY_CODE = 'owned_agent_reasoning_boundary'

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
  'plan',
  'complete',
  'failure',
  'subagent_lifecycle',
])
export const runtimeSubagentStatusSchema = z.enum([
  'spawned',
  'pending',
  'starting',
  'running',
  'paused',
  'cancelling',
  'cancelled',
  'handed_off',
  'completed',
  'failed',
  'budget_exhausted',
])
export const runtimeActionAnswerShapeSchema = z.enum([
  'plain_text',
  'terminal_input',
  'single_choice',
  'multi_choice',
  'short_text',
  'long_text',
  'number',
  'date',
])
export const runtimeActionRequiredOptionSchema = z
  .object({
    id: z.string().trim().min(1),
    label: z.string().trim().min(1),
    description: nonEmptyOptionalTextSchema,
  })
  .strict()
export const runtimeStreamPlanItemStatusSchema = z.enum(['pending', 'in_progress', 'completed'])
export const runtimeStreamPlanItemSchema = z
  .object({
    id: z.string().trim().min(1),
    title: z.string().trim().min(1),
    notes: nonEmptyOptionalTextSchema,
    status: runtimeStreamPlanItemStatusSchema,
    updatedAt: z.string().min(1),
    phaseId: nonEmptyOptionalTextSchema,
    phaseTitle: nonEmptyOptionalTextSchema,
    sliceId: nonEmptyOptionalTextSchema,
    handoffNote: nonEmptyOptionalTextSchema,
  })
  .strict()
export const runtimeStreamTranscriptRoleSchema = z.enum(['user', 'assistant', 'system', 'tool'])

export const runtimeStreamItemSchema = z
  .object({
    kind: runtimeStreamItemKindSchema,
    runId: z.string().trim().min(1),
    sequence: z.number().int().positive(),
    updatedSequence: z.number().int().positive().nullable().optional(),
    sessionId: nonEmptyOptionalTextSchema,
    flowId: nonEmptyOptionalTextSchema,
    text: z.string().min(1).nullable().optional(),
    transcriptRole: runtimeStreamTranscriptRoleSchema.nullable().optional(),
    toolCallId: nonEmptyOptionalTextSchema,
    toolName: nonEmptyOptionalTextSchema,
    toolState: runtimeToolCallStateSchema.nullable().optional(),
    codeChangeGroupId: nonEmptyOptionalTextSchema,
    codeCommitId: nonEmptyOptionalTextSchema,
    codeWorkspaceEpoch: z.number().int().nonnegative().nullable().optional(),
    codePatchAvailability: codePatchAvailabilitySchema.nullable().optional(),
    toolSummary: toolResultSummarySchema.nullable().optional(),
    toolResultPreview: z.string().nullable().optional(),
    skillId: nonEmptyOptionalTextSchema,
    skillStage: runtimeSkillLifecycleStageSchema.nullable().optional(),
    skillResult: runtimeSkillLifecycleResultSchema.nullable().optional(),
    skillSource: runtimeSkillSourceSchema.nullable().optional(),
    skillCacheStatus: runtimeSkillCacheStatusSchema.nullable().optional(),
    skillDiagnostic: runtimeSkillDiagnosticSchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
    actionType: nonEmptyOptionalTextSchema,
    answerShape: runtimeActionAnswerShapeSchema.nullable().optional(),
    options: z
      .array(runtimeActionRequiredOptionSchema)
      .min(1)
      .max(MAX_RUNTIME_STREAM_ACTION_REQUIRED_OPTIONS)
      .nullable()
      .optional(),
    allowMultiple: z.boolean().nullable().optional(),
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    planId: nonEmptyOptionalTextSchema,
    planItems: z
      .array(runtimeStreamPlanItemSchema)
      .max(MAX_RUNTIME_STREAM_PLAN_ITEMS)
      .nullable()
      .optional(),
    planLastChangedId: nonEmptyOptionalTextSchema,
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
    retryable: z.boolean().nullable().optional(),
    subagentId: nonEmptyOptionalTextSchema,
    subagentRole: nonEmptyOptionalTextSchema,
    subagentRoleLabel: nonEmptyOptionalTextSchema,
    subagentRunId: nonEmptyOptionalTextSchema,
    subagentStatus: nonEmptyOptionalTextSchema,
    subagentUsedToolCalls: z.number().int().nonnegative().nullable().optional(),
    subagentMaxToolCalls: z.number().int().nonnegative().nullable().optional(),
    subagentUsedTokens: z.number().int().nonnegative().nullable().optional(),
    subagentMaxTokens: z.number().int().nonnegative().nullable().optional(),
    subagentUsedCostMicros: z.number().int().nonnegative().nullable().optional(),
    subagentMaxCostMicros: z.number().int().nonnegative().nullable().optional(),
    subagentResultSummary: nonEmptyOptionalTextSchema,
    subagentPrompt: nonEmptyOptionalTextSchema,
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
        if (item.answerShape === 'single_choice' || item.answerShape === 'multi_choice') {
          if (!item.options || item.options.length === 0) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: ['options'],
              message: 'Xero received a runtime choice action-required item without a non-empty options array.',
            })
          }
          if (item.answerShape === 'multi_choice' && item.allowMultiple === false) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              path: ['allowMultiple'],
              message: 'Multi-choice action-required items must have allowMultiple unset or true.',
            })
          }
        }
        if (
          item.answerShape != null &&
          item.answerShape !== 'single_choice' &&
          item.answerShape !== 'multi_choice' &&
          item.options
        ) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['options'],
            message: 'Only single_choice and multi_choice action-required items may include options.',
          })
        }
        return
      case 'plan':
        if (!item.planId) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['planId'],
            message: 'Xero received a runtime plan item without a non-empty planId field.',
          })
        }
        if (!Array.isArray(item.planItems)) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['planItems'],
            message: 'Xero received a runtime plan item without a planItems array.',
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
      case 'subagent_lifecycle':
        if (!item.subagentId) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['subagentId'],
            message: 'Xero received a subagent lifecycle item without a non-empty subagentId field.',
          })
        }
        if (!item.subagentStatus) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            path: ['subagentStatus'],
            message: 'Xero received a subagent lifecycle item without a subagentStatus value.',
          })
        }
        return
    }
  })

export const subscribeRuntimeStreamRequestSchema = z.object({
  projectId: z.string().trim().min(1),
  agentSessionId: z.string().trim().min(1),
  itemKinds: z.array(runtimeStreamItemKindSchema).min(1),
  afterSequence: z.number().int().nonnegative().nullable().optional(),
  replayLimit: z.number().int().min(1).max(1000).nullable().optional(),
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

export const runtimeStreamStatusSchema = z.enum([
  'idle',
  'subscribing',
  'replaying',
  'live',
  'complete',
  'stale',
  'error',
])

export const runtimeStreamIssueSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    retryable: z.boolean(),
    observedAt: isoTimestampSchema,
  })
  .strict()

export const runtimeStreamViewSnapshotSchema = z
  .object({
    schema: z.literal('xero.runtime_stream_view_snapshot.v1'),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    sessionId: z.string().trim().min(1),
    flowId: nonEmptyOptionalTextSchema,
    subscribedItemKinds: z.array(runtimeStreamItemKindSchema).min(1),
    status: runtimeStreamStatusSchema,
    items: z.array(runtimeStreamItemSchema),
    transcriptItems: z.array(runtimeStreamItemSchema),
    toolCalls: z.array(runtimeStreamItemSchema),
    skillItems: z.array(runtimeStreamItemSchema),
    activityItems: z.array(runtimeStreamItemSchema),
    actionRequired: z.array(runtimeStreamItemSchema).max(MAX_RUNTIME_STREAM_ACTION_REQUIRED),
    plan: runtimeStreamItemSchema.nullable().optional(),
    completion: runtimeStreamItemSchema.nullable().optional(),
    failure: runtimeStreamItemSchema.nullable().optional(),
    lastIssue: runtimeStreamIssueSchema.nullable().optional(),
    lastItemAt: isoTimestampSchema.nullable().optional(),
    lastSequence: z.number().int().positive().nullable().optional(),
  })
  .strict()

export const runtimeStreamPatchSchema = z
  .object({
    schema: z.literal('xero.runtime_stream_patch.v1'),
    item: runtimeStreamItemSchema,
    snapshot: runtimeStreamViewSnapshotSchema,
  })
  .strict()

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
export type RuntimeStreamViewSnapshotDto = z.infer<typeof runtimeStreamViewSnapshotSchema>
export type RuntimeStreamPatchDto = z.infer<typeof runtimeStreamPatchSchema>

export type RuntimeStreamStatus = z.infer<typeof runtimeStreamStatusSchema>

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
  updatedSequence?: number
  createdAt: string
  codeChangeGroupId?: string | null
  codeCommitId?: string | null
  codeWorkspaceEpoch?: number | null
  codePatchAvailability?: CodePatchAvailabilityDto | null
  /**
   * When set, this item belongs to a subagent run that was forwarded onto the
   * parent's stream. The conversation projection groups these items under a
   * `subagent_group` turn keyed by `subagentId`.
   */
  subagentId?: string | null
  subagentRole?: string | null
  subagentRoleLabel?: string | null
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
  toolResultPreview: string | null
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

export type RuntimeActionAnswerShapeDto = z.infer<typeof runtimeActionAnswerShapeSchema>
export type RuntimeActionRequiredOptionDto = z.infer<typeof runtimeActionRequiredOptionSchema>
export type RuntimeStreamPlanItemStatusDto = z.infer<typeof runtimeStreamPlanItemStatusSchema>
export type RuntimeStreamPlanItemDto = z.infer<typeof runtimeStreamPlanItemSchema>

export interface RuntimeStreamActionRequiredItemView extends RuntimeStreamBaseItemView {
  kind: 'action_required'
  actionId: string
  boundaryId: string | null
  actionType: string
  title: string
  detail: string
  answerShape: RuntimeActionAnswerShapeDto | null
  options: RuntimeActionRequiredOptionDto[] | null
  allowMultiple: boolean | null
}

export interface RuntimeStreamPlanItemView extends RuntimeStreamBaseItemView {
  kind: 'plan'
  planId: string
  items: RuntimeStreamPlanItemDto[]
  lastChangedId: string | null
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

export type RuntimeSubagentStatusDto = z.infer<typeof runtimeSubagentStatusSchema>

export interface RuntimeStreamSubagentLifecycleItemView extends RuntimeStreamBaseItemView {
  kind: 'subagent_lifecycle'
  subagentId: string
  subagentRole: string | null
  subagentRoleLabel: string | null
  subagentRunId: string | null
  subagentStatus: string
  usedToolCalls: number | null
  maxToolCalls: number | null
  usedTokens: number | null
  maxTokens: number | null
  usedCostMicros: number | null
  maxCostMicros: number | null
  resultSummary: string | null
  prompt: string | null
  title: string | null
  detail: string | null
}

export type RuntimeStreamViewItem =
  | RuntimeStreamTranscriptItemView
  | RuntimeStreamToolItemView
  | RuntimeStreamSkillItemView
  | RuntimeStreamActivityItemView
  | RuntimeStreamActionRequiredItemView
  | RuntimeStreamPlanItemView
  | RuntimeStreamCompleteItemView
  | RuntimeStreamFailureItemView
  | RuntimeStreamSubagentLifecycleItemView

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
  plan: RuntimeStreamPlanItemView | null
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

function runtimeTimelineUpdateSequence(item: RuntimeStreamViewItem): number {
  return item.updatedSequence ?? item.sequence
}

function latestRuntimeTimelineItem(
  items: readonly RuntimeStreamViewItem[],
): RuntimeStreamViewItem | null {
  let latestItem: RuntimeStreamViewItem | null = null
  for (const item of items) {
    if (!latestItem || runtimeTimelineUpdateSequence(item) > runtimeTimelineUpdateSequence(latestItem)) {
      latestItem = item
    }
  }
  return latestItem
}

function normalizeRuntimeCodeHistoryMetadata(
  item: RuntimeStreamItemDto,
): Pick<
  RuntimeStreamBaseItemView,
  | 'updatedSequence'
  | 'codeChangeGroupId'
  | 'codeCommitId'
  | 'codeWorkspaceEpoch'
  | 'codePatchAvailability'
  | 'subagentId'
  | 'subagentRole'
  | 'subagentRoleLabel'
> {
  return {
    updatedSequence:
      typeof item.updatedSequence === 'number' ? item.updatedSequence : undefined,
    codeChangeGroupId: normalizeOptionalText(item.codeChangeGroupId),
    codeCommitId: normalizeOptionalText(item.codeCommitId),
    codeWorkspaceEpoch:
      typeof item.codeWorkspaceEpoch === 'number' ? item.codeWorkspaceEpoch : null,
    codePatchAvailability: item.codePatchAvailability ?? null,
    subagentId: normalizeOptionalText(item.subagentId),
    subagentRole: normalizeOptionalText(item.subagentRole),
    subagentRoleLabel: normalizeOptionalText(item.subagentRoleLabel),
  }
}

function mergeRuntimeTimelineToolItem(
  currentItems: RuntimeStreamViewItem[],
  nextItem: RuntimeStreamToolItemView,
): RuntimeStreamViewItem[] {
  const existingItemIndex = currentItems.findIndex(
    (item) => item.kind === 'tool' && item.toolCallId === nextItem.toolCallId,
  )

  if (existingItemIndex < 0) {
    return [...currentItems, nextItem]
  }

  const existingItem = currentItems[existingItemIndex]
  if (existingItem?.kind !== 'tool') {
    return [...currentItems, nextItem]
  }

  const mergedItem: RuntimeStreamToolItemView = {
    ...nextItem,
    id: existingItem.id,
    sequence: existingItem.sequence,
    createdAt: existingItem.createdAt,
    updatedSequence: nextItem.sequence,
  }

  return currentItems.map((item, index) =>
    index === existingItemIndex ? mergedItem : item,
  )
}

function capRuntimeTimelineItems(
  currentItems: RuntimeStreamViewItem[],
  transcriptItems: RuntimeStreamTranscriptItemView[],
  nextItem: RuntimeStreamViewItem,
): RuntimeStreamViewItem[] {
  const previousTimelineItem = latestRuntimeTimelineItem(currentItems)
  let nonTranscriptItems: RuntimeStreamViewItem[] = currentItems.filter(
    (item) => item.kind !== 'transcript',
  )

  if (nextItem.kind === 'tool') {
    nonTranscriptItems = mergeRuntimeTimelineToolItem(nonTranscriptItems, nextItem)
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
  } else if (nextItem.kind === 'plan') {
    nonTranscriptItems = [
      ...nonTranscriptItems.filter(
        (item) =>
          !(item.kind === 'plan' && item.runId === nextItem.runId && item.planId === nextItem.planId),
      ),
      nextItem,
    ]
  } else if (isReasoningActivityItem(nextItem)) {
    nonTranscriptItems = mergeReasoningActivityTimelineItem(
      nonTranscriptItems,
      nextItem,
      previousTimelineItem,
    )
  } else if (nextItem.kind !== 'transcript') {
    nonTranscriptItems = [...nonTranscriptItems, nextItem]
  }

  const reasoningItems = nonTranscriptItems.filter(isReasoningActivityItem)
  const otherNonTranscriptItems = nonTranscriptItems.filter(
    (item) => !isReasoningActivityItem(item),
  )

  return [
    ...transcriptItems,
    ...reasoningItems,
    ...otherNonTranscriptItems,
  ].sort(compareRuntimeStreamItemsBySequence)
}

function isReasoningActivityItem(
  item: RuntimeStreamViewItem,
): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === OWNED_AGENT_REASONING_ACTIVITY_CODE
}

function reasoningActivityText(item: RuntimeStreamActivityItemView): string {
  return item.text ?? item.detail ?? ''
}

function mergeReasoningActivityTimelineItem(
  currentItems: RuntimeStreamViewItem[],
  nextItem: RuntimeStreamActivityItemView,
  previousTimelineItem: RuntimeStreamViewItem | null,
): RuntimeStreamViewItem[] {
  if (
    !previousTimelineItem ||
    !isReasoningActivityItem(previousTimelineItem) ||
    previousTimelineItem.runId !== nextItem.runId
  ) {
    return [...currentItems, nextItem]
  }

  const previousItemIndex = currentItems.findIndex((item) => item.id === previousTimelineItem.id)
  if (previousItemIndex < 0) {
    return [...currentItems, nextItem]
  }

  const previousItem = currentItems[previousItemIndex]
  if (!isReasoningActivityItem(previousItem)) {
    return [...currentItems, nextItem]
  }

  const mergedText = `${reasoningActivityText(previousItem)}${reasoningActivityText(nextItem)}`
  return currentItems.map((item, index) =>
    index === previousItemIndex
      ? {
          ...previousItem,
          updatedSequence: nextItem.sequence,
          text: mergedText,
          detail: mergedText.trim().length > 0
            ? mergedText.trim()
            : previousItem.detail ?? nextItem.detail,
        }
      : item,
  )
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
  previousTimelineItem: RuntimeStreamViewItem | null,
): RuntimeStreamTranscriptItemView[] {
  if (nextItem.role !== 'assistant') {
    return [...currentItems, nextItem]
  }

  const previousItem = currentItems.at(-1)
  if (
    !previousItem ||
    previousItem.runId !== nextItem.runId ||
    previousItem.role !== nextItem.role ||
    previousTimelineItem?.kind !== 'transcript' ||
    previousTimelineItem.id !== previousItem.id
  ) {
    return [...currentItems, nextItem]
  }

  const mergedItem: RuntimeStreamTranscriptItemView = {
    ...previousItem,
    id: previousItem.id,
    updatedSequence: nextItem.sequence,
    text: `${previousItem.text}${nextItem.text}`,
  }

  return [...currentItems.slice(0, -1), mergedItem]
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

function runtimeStreamPlanItemId(runId: string, planId: string): string {
  return `plan:${runId}:${planId}`
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
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('transcript', itemRunId, event.item.sequence),
        kind: 'transcript',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
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

      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('tool', itemRunId, event.item.sequence),
        kind: 'tool',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        toolCallId,
        toolName,
        toolState,
        detail: normalizeOptionalText(event.item.detail),
        toolSummary: normalizeRuntimeToolSummary(event.item.toolSummary),
        toolResultPreview: normalizeOptionalText(event.item.toolResultPreview),
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

      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('skill', itemRunId, event.item.sequence),
        kind: 'skill',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
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
      const text = typeof event.item.text === 'string' && event.item.text.length > 0
        ? event.item.text
        : null
      const isReasoningBoundary =
        code === OWNED_AGENT_REASONING_ACTIVITY_CODE &&
        text != null &&
        text.trim().length === 0
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('activity', itemRunId, event.item.sequence),
        kind: 'activity',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        code: isReasoningBoundary ? OWNED_AGENT_REASONING_BOUNDARY_CODE : code,
        title,
        text,
        detail: normalizeOptionalText(event.item.detail),
      }
    }
    case 'action_required': {
      const actionId = ensureRuntimeStreamText(event.item.actionId, 'actionId', 'action-required')
      const actionType = ensureRuntimeStreamText(event.item.actionType, 'actionType', 'action-required')
      const title = ensureRuntimeStreamText(event.item.title, 'title', 'action-required')
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'action-required')
      const options = event.item.options ?? null
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamActionRequiredItemId(itemRunId, actionId),
        kind: 'action_required',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        actionId,
        boundaryId: normalizeOptionalText(event.item.boundaryId),
        actionType,
        title,
        detail,
        answerShape: event.item.answerShape ?? null,
        options: options && options.length > 0
          ? options.map((option) => ({
              id: option.id,
              label: option.label,
              description: option.description ?? null,
            }))
          : null,
        allowMultiple:
          typeof event.item.allowMultiple === 'boolean' ? event.item.allowMultiple : null,
      }
    }
    case 'plan': {
      const planId = ensureRuntimeStreamText(event.item.planId, 'planId', 'plan')
      const planItems = Array.isArray(event.item.planItems) ? event.item.planItems : []
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamPlanItemId(itemRunId, planId),
        kind: 'plan',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        planId,
        items: planItems.map((planItem) => ({
          id: planItem.id,
          title: planItem.title,
          notes: planItem.notes ?? null,
          status: planItem.status,
          updatedAt: planItem.updatedAt,
          phaseId: planItem.phaseId ?? null,
          phaseTitle: planItem.phaseTitle ?? null,
          sliceId: planItem.sliceId ?? null,
          handoffNote: planItem.handoffNote ?? null,
        })),
        lastChangedId: normalizeOptionalText(event.item.planLastChangedId),
      }
    }
    case 'complete': {
      const detail = ensureRuntimeStreamText(event.item.detail, 'detail', 'complete')
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('complete', itemRunId, event.item.sequence),
        kind: 'complete',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        detail,
      }
    }
    case 'failure': {
      const code = ensureRuntimeStreamText(event.item.code, 'code', 'failure')
      const message = ensureRuntimeStreamText(event.item.message, 'message', 'failure')
      if (typeof event.item.retryable !== 'boolean') {
        throw new Error('Xero received a runtime failure item without a retryable flag.')
      }

      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: runtimeStreamItemId('failure', itemRunId, event.item.sequence),
        kind: 'failure',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        code,
        message,
        retryable: event.item.retryable,
      }
    }
    case 'subagent_lifecycle': {
      const subagentId = ensureRuntimeStreamText(
        event.item.subagentId,
        'subagentId',
        'subagent-lifecycle',
      )
      const subagentStatus = ensureRuntimeStreamText(
        event.item.subagentStatus,
        'subagentStatus',
        'subagent-lifecycle',
      )
      const codeHistory = normalizeRuntimeCodeHistoryMetadata(event.item)
      return {
        id: `subagent_lifecycle:${itemRunId}:${subagentId}:${event.item.sequence}`,
        kind: 'subagent_lifecycle',
        runId: itemRunId,
        sequence: event.item.sequence,
        createdAt: event.item.createdAt,
        ...codeHistory,
        subagentId,
        subagentRole: normalizeOptionalText(event.item.subagentRole),
        subagentRoleLabel: normalizeOptionalText(event.item.subagentRoleLabel),
        subagentRunId: normalizeOptionalText(event.item.subagentRunId),
        subagentStatus,
        usedToolCalls:
          typeof event.item.subagentUsedToolCalls === 'number'
            ? event.item.subagentUsedToolCalls
            : null,
        maxToolCalls:
          typeof event.item.subagentMaxToolCalls === 'number'
            ? event.item.subagentMaxToolCalls
            : null,
        usedTokens:
          typeof event.item.subagentUsedTokens === 'number'
            ? event.item.subagentUsedTokens
            : null,
        maxTokens:
          typeof event.item.subagentMaxTokens === 'number'
            ? event.item.subagentMaxTokens
            : null,
        usedCostMicros:
          typeof event.item.subagentUsedCostMicros === 'number'
            ? event.item.subagentUsedCostMicros
            : null,
        maxCostMicros:
          typeof event.item.subagentMaxCostMicros === 'number'
            ? event.item.subagentMaxCostMicros
            : null,
        resultSummary: normalizeOptionalText(event.item.subagentResultSummary),
        prompt: normalizeOptionalText(event.item.subagentPrompt),
        title: normalizeOptionalText(event.item.title),
        detail: normalizeOptionalText(event.item.detail),
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
    plan: null,
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

function normalizeProjectedRuntimeStreamItem(
  snapshot: RuntimeStreamViewSnapshotDto,
  item: RuntimeStreamItemDto,
): RuntimeStreamViewItem {
  return normalizeRuntimeStreamItem({
    projectId: snapshot.projectId,
    agentSessionId: snapshot.agentSessionId,
    runtimeKind: snapshot.runtimeKind,
    runId: snapshot.runId,
    sessionId: snapshot.sessionId,
    flowId: snapshot.flowId ?? null,
    subscribedItemKinds: snapshot.subscribedItemKinds,
    item,
  })
}

function normalizeProjectedRuntimeStreamItemOfKind<
  TKind extends RuntimeStreamViewItem['kind'],
>(
  snapshot: RuntimeStreamViewSnapshotDto,
  item: RuntimeStreamItemDto | null | undefined,
  kind: TKind,
): Extract<RuntimeStreamViewItem, { kind: TKind }> | null {
  if (!item) {
    return null
  }

  const normalizedItem = normalizeProjectedRuntimeStreamItem(snapshot, item)
  if (normalizedItem.kind !== kind) {
    throw new Error(
      `Xero received a runtime stream snapshot ${kind} slot containing ${normalizedItem.kind}.`,
    )
  }

  return normalizedItem as Extract<RuntimeStreamViewItem, { kind: TKind }>
}

function normalizeProjectedRuntimeStreamItemsOfKind<
  TKind extends RuntimeStreamViewItem['kind'],
>(
  snapshot: RuntimeStreamViewSnapshotDto,
  items: RuntimeStreamItemDto[],
  kind: TKind,
): Extract<RuntimeStreamViewItem, { kind: TKind }>[] {
  return items.map((item) => {
    const normalizedItem = normalizeProjectedRuntimeStreamItemOfKind(snapshot, item, kind)
    if (!normalizedItem) {
      throw new Error(`Xero received an empty runtime stream snapshot ${kind} item.`)
    }
    return normalizedItem
  })
}

export function createRuntimeStreamViewFromSnapshot(
  snapshot: RuntimeStreamViewSnapshotDto,
): RuntimeStreamView {
  const items = snapshot.items.map((item) => normalizeProjectedRuntimeStreamItem(snapshot, item))
  const transcriptItems = normalizeProjectedRuntimeStreamItemsOfKind(
    snapshot,
    snapshot.transcriptItems,
    'transcript',
  )
  const toolCalls = normalizeProjectedRuntimeStreamItemsOfKind(snapshot, snapshot.toolCalls, 'tool')
  const skillItems = normalizeProjectedRuntimeStreamItemsOfKind(snapshot, snapshot.skillItems, 'skill')
  const activityItems = normalizeProjectedRuntimeStreamItemsOfKind(
    snapshot,
    snapshot.activityItems,
    'activity',
  )
  const actionRequired = normalizeProjectedRuntimeStreamItemsOfKind(
    snapshot,
    snapshot.actionRequired,
    'action_required',
  )

  return {
    projectId: snapshot.projectId,
    agentSessionId: snapshot.agentSessionId,
    runtimeKind: normalizeText(snapshot.runtimeKind, 'openai_codex'),
    runId: normalizeOptionalText(snapshot.runId),
    sessionId: normalizeOptionalText(snapshot.sessionId),
    flowId: normalizeOptionalText(snapshot.flowId),
    subscribedItemKinds: uniqueRuntimeStreamKinds(snapshot.subscribedItemKinds),
    status: snapshot.status,
    items,
    transcriptItems,
    toolCalls,
    skillItems,
    activityItems,
    actionRequired,
    plan: normalizeProjectedRuntimeStreamItemOfKind(snapshot, snapshot.plan, 'plan'),
    completion: normalizeProjectedRuntimeStreamItemOfKind(snapshot, snapshot.completion, 'complete'),
    failure: normalizeProjectedRuntimeStreamItemOfKind(snapshot, snapshot.failure, 'failure'),
    lastIssue: snapshot.lastIssue
      ? {
          code: snapshot.lastIssue.code,
          message: snapshot.lastIssue.message,
          retryable: snapshot.lastIssue.retryable,
          observedAt: snapshot.lastIssue.observedAt,
        }
      : null,
    lastItemAt: snapshot.lastItemAt ?? null,
    lastSequence: snapshot.lastSequence ?? null,
  }
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
      ? [
          ...base.toolCalls.filter((toolCall) => toolCall.toolCallId !== nextItem.toolCallId),
          nextItem,
        ]
      : base.toolCalls
  const nextSkillItems =
    nextItem.kind === 'skill'
      ? [...base.skillItems, nextItem]
      : base.skillItems
  const nextTranscriptItems =
    nextItem.kind === 'transcript'
      ? mergeRuntimeTranscriptItems(
          base.transcriptItems,
          nextItem,
          latestRuntimeTimelineItem(base.items),
        )
      : base.transcriptItems
  const nextItems = capRuntimeTimelineItems(base.items, nextTranscriptItems, nextItem)
  const nextActivityItems =
    nextItem.kind === 'activity'
      ? [...base.activityItems, nextItem]
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
  const nextPlan = nextItem.kind === 'plan' ? nextItem : base.plan

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
    plan: nextPlan,
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

function estimateOptionalTextBytes(value: string | null | undefined): number {
  return value ? estimateUtf16Bytes(value) : 0
}

function estimateRuntimeStreamItemBytes(item: RuntimeStreamViewItem): number {
  let bytes = 96
  bytes += estimateUtf16Bytes(item.id)
  bytes += estimateUtf16Bytes(item.runId)
  bytes += estimateUtf16Bytes(item.createdAt)
  bytes += estimateUtf16Bytes(item.kind)
  bytes += estimateOptionalTextBytes(item.codeChangeGroupId)
  bytes += estimateOptionalTextBytes(item.codeCommitId)
  if (item.codeWorkspaceEpoch != null) {
    bytes += 8
  }
  if (item.codePatchAvailability) {
    bytes += estimateUtf16Bytes(item.codePatchAvailability.projectId)
    bytes += estimateUtf16Bytes(item.codePatchAvailability.targetChangeGroupId)
    bytes += estimateOptionalTextBytes(item.codePatchAvailability.unavailableReason)
    for (const path of item.codePatchAvailability.affectedPaths) {
      bytes += estimateUtf16Bytes(path)
    }
    for (const hunk of item.codePatchAvailability.textHunks ?? []) {
      bytes += 40
      bytes += estimateUtf16Bytes(hunk.hunkId)
      bytes += estimateOptionalTextBytes(hunk.patchFileId)
      bytes += estimateUtf16Bytes(hunk.filePath)
    }
  }

  switch (item.kind) {
    case 'transcript':
      bytes += estimateUtf16Bytes(item.text)
      bytes += estimateUtf16Bytes(item.role)
      return bytes
    case 'tool':
      bytes += estimateUtf16Bytes(item.toolCallId)
      bytes += estimateUtf16Bytes(item.toolName)
      bytes += estimateUtf16Bytes(item.toolState)
      bytes += estimateOptionalTextBytes(item.detail)
      if (item.toolSummary) {
        bytes += estimateUtf16Bytes(item.toolSummary.kind)
        switch (item.toolSummary.kind) {
          case 'file':
            bytes += estimateOptionalTextBytes(item.toolSummary.path)
            bytes += estimateOptionalTextBytes(item.toolSummary.scope)
            break
          case 'git':
            bytes += estimateOptionalTextBytes(item.toolSummary.scope)
            bytes += estimateOptionalTextBytes(item.toolSummary.baseRevision)
            break
          case 'web':
            bytes += estimateUtf16Bytes(item.toolSummary.target)
            bytes += estimateOptionalTextBytes(item.toolSummary.finalUrl)
            bytes += estimateOptionalTextBytes(item.toolSummary.contentKind)
            bytes += estimateOptionalTextBytes(item.toolSummary.contentType)
            break
          case 'browser_computer_use':
            bytes += estimateUtf16Bytes(item.toolSummary.surface)
            bytes += estimateUtf16Bytes(item.toolSummary.action)
            bytes += estimateUtf16Bytes(item.toolSummary.status)
            bytes += estimateOptionalTextBytes(item.toolSummary.target)
            bytes += estimateOptionalTextBytes(item.toolSummary.outcome)
            break
          case 'mcp_capability':
            bytes += estimateUtf16Bytes(item.toolSummary.serverId)
            bytes += estimateUtf16Bytes(item.toolSummary.capabilityKind)
            bytes += estimateUtf16Bytes(item.toolSummary.capabilityId)
            bytes += estimateOptionalTextBytes(item.toolSummary.capabilityName)
            break
          case 'command':
            break
        }
      }
      return bytes
    case 'skill':
      bytes += estimateUtf16Bytes(item.skillId)
      bytes += estimateUtf16Bytes(item.stage)
      bytes += estimateUtf16Bytes(item.result)
      bytes += estimateUtf16Bytes(item.detail)
      bytes += estimateUtf16Bytes(item.source.repo)
      bytes += estimateUtf16Bytes(item.source.path)
      bytes += estimateUtf16Bytes(item.source.reference)
      bytes += estimateUtf16Bytes(item.source.treeHash)
      bytes += estimateOptionalTextBytes(item.cacheStatus)
      if (item.diagnostic) {
        bytes += estimateUtf16Bytes(item.diagnostic.code)
        bytes += estimateUtf16Bytes(item.diagnostic.message)
      }
      return bytes
    case 'activity':
      bytes += estimateUtf16Bytes(item.code)
      bytes += estimateUtf16Bytes(item.title)
      bytes += estimateOptionalTextBytes(item.text)
      bytes += estimateOptionalTextBytes(item.detail)
      return bytes
    case 'action_required':
      bytes += estimateUtf16Bytes(item.actionId)
      bytes += estimateOptionalTextBytes(item.boundaryId)
      bytes += estimateUtf16Bytes(item.actionType)
      bytes += estimateUtf16Bytes(item.title)
      bytes += estimateUtf16Bytes(item.detail)
      bytes += estimateOptionalTextBytes(item.answerShape)
      if (item.options) {
        for (const option of item.options) {
          bytes += estimateUtf16Bytes(option.id)
          bytes += estimateUtf16Bytes(option.label)
          bytes += estimateOptionalTextBytes(option.description)
        }
      }
      return bytes
    case 'plan':
      bytes += estimateUtf16Bytes(item.planId)
      bytes += estimateOptionalTextBytes(item.lastChangedId)
      for (const planItem of item.items) {
        bytes += estimateUtf16Bytes(planItem.id)
        bytes += estimateUtf16Bytes(planItem.title)
        bytes += estimateOptionalTextBytes(planItem.notes)
        bytes += estimateUtf16Bytes(planItem.status)
        bytes += estimateUtf16Bytes(planItem.updatedAt)
        bytes += estimateOptionalTextBytes(planItem.phaseId)
        bytes += estimateOptionalTextBytes(planItem.phaseTitle)
        bytes += estimateOptionalTextBytes(planItem.sliceId)
        bytes += estimateOptionalTextBytes(planItem.handoffNote)
      }
      return bytes
    case 'complete':
      bytes += estimateUtf16Bytes(item.detail)
      return bytes
    case 'failure':
      bytes += estimateUtf16Bytes(item.code)
      bytes += estimateUtf16Bytes(item.message)
      return bytes
    case 'subagent_lifecycle':
      bytes += estimateUtf16Bytes(item.subagentId)
      bytes += estimateOptionalTextBytes(item.subagentRole)
      bytes += estimateOptionalTextBytes(item.subagentRoleLabel)
      bytes += estimateOptionalTextBytes(item.subagentRunId)
      bytes += estimateUtf16Bytes(item.subagentStatus)
      bytes += estimateOptionalTextBytes(item.resultSummary)
      bytes += estimateOptionalTextBytes(item.prompt)
      bytes += estimateOptionalTextBytes(item.title)
      bytes += estimateOptionalTextBytes(item.detail)
      bytes += 64
      return bytes
  }
}

function estimateRuntimeStreamItemsBytes(items: readonly RuntimeStreamViewItem[]): number {
  const seenIds = new Set<string>()
  let bytes = 0
  for (const item of items) {
    if (seenIds.has(item.id)) continue
    seenIds.add(item.id)
    bytes += estimateRuntimeStreamItemBytes(item)
  }
  return bytes
}

export function estimateRuntimeStreamViewBytes(stream: RuntimeStreamView | null | undefined): number {
  if (!stream) {
    return 0
  }

  let bytes = 192
  bytes += estimateUtf16Bytes(stream.projectId)
  bytes += estimateUtf16Bytes(stream.agentSessionId)
  bytes += estimateUtf16Bytes(stream.runtimeKind)
  bytes += estimateOptionalTextBytes(stream.runId)
  bytes += estimateOptionalTextBytes(stream.sessionId)
  bytes += estimateOptionalTextBytes(stream.flowId)
  bytes += estimateUtf16Bytes(stream.status)
  bytes += estimateOptionalTextBytes(stream.lastItemAt)
  for (const kind of stream.subscribedItemKinds) {
    bytes += estimateUtf16Bytes(kind)
  }
  if (stream.lastIssue) {
    bytes += estimateUtf16Bytes(stream.lastIssue.code)
    bytes += estimateUtf16Bytes(stream.lastIssue.message)
    bytes += estimateUtf16Bytes(stream.lastIssue.observedAt)
  }
  bytes += estimateRuntimeStreamItemsBytes([
    ...stream.items,
    ...stream.transcriptItems,
    ...stream.toolCalls,
    ...stream.skillItems,
    ...stream.activityItems,
    ...stream.actionRequired,
    ...(stream.plan ? [stream.plan] : []),
    ...(stream.completion ? [stream.completion] : []),
    ...(stream.failure ? [stream.failure] : []),
  ])
  return bytes
}
