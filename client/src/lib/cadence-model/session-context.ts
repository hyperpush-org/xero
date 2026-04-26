import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema } from './shared'

export const CADENCE_SESSION_CONTEXT_CONTRACT_VERSION = 1

export const sessionTranscriptScopeSchema = z.enum(['run', 'session'])
export const sessionTranscriptExportFormatSchema = z.enum(['markdown', 'json'])
export const sessionTranscriptSourceKindSchema = z.enum(['owned_agent', 'runtime_stream'])
export const sessionTranscriptItemKindSchema = z.enum([
  'message',
  'reasoning',
  'tool_call',
  'tool_result',
  'file_change',
  'checkpoint',
  'action_request',
  'activity',
  'complete',
  'failure',
  'usage',
])
export const sessionTranscriptActorSchema = z.enum([
  'system',
  'developer',
  'user',
  'assistant',
  'tool',
  'runtime',
  'cadence',
  'operator',
])
export const sessionTranscriptToolStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const sessionContextRedactionClassSchema = z.enum(['public', 'local_path', 'secret', 'raw_payload', 'transcript'])

export const sessionContextRedactionSchema = z
  .object({
    redactionClass: sessionContextRedactionClassSchema,
    redacted: z.boolean(),
    reason: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((redaction, ctx) => {
    if (redaction.redactionClass === 'public' && redaction.redacted) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['redacted'],
        message: 'Public session-context payloads must not be marked redacted.',
      })
    }
    if (redaction.redactionClass !== 'public' && !redaction.redacted) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['redacted'],
        message: 'Non-public session-context payloads must set redacted=true.',
      })
    }
    if (redaction.redacted && !redaction.reason) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['reason'],
        message: 'Redacted session-context payloads must explain the redaction reason.',
      })
    }
  })

export const sessionUsageSourceSchema = z.enum(['provider', 'estimated', 'mixed', 'unavailable'])
export const sessionUsageTotalsSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    inputTokens: z.number().int().nonnegative(),
    outputTokens: z.number().int().nonnegative(),
    totalTokens: z.number().int().nonnegative(),
    estimatedCostMicros: z.number().int().nonnegative(),
    source: sessionUsageSourceSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const sessionTranscriptItemSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    itemId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    sourceKind: sessionTranscriptSourceKindSchema,
    sourceTable: z.string().trim().min(1),
    sourceId: z.string().trim().min(1),
    sequence: z.number().int().positive(),
    createdAt: isoTimestampSchema,
    kind: sessionTranscriptItemKindSchema,
    actor: sessionTranscriptActorSchema,
    title: nonEmptyOptionalTextSchema,
    text: nonEmptyOptionalTextSchema,
    summary: nonEmptyOptionalTextSchema,
    toolCallId: nonEmptyOptionalTextSchema,
    toolName: nonEmptyOptionalTextSchema,
    toolState: sessionTranscriptToolStateSchema.nullable().optional(),
    filePath: nonEmptyOptionalTextSchema,
    checkpointKind: nonEmptyOptionalTextSchema,
    actionId: nonEmptyOptionalTextSchema,
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const runTranscriptSummarySchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    status: z.string().trim().min(1),
    startedAt: isoTimestampSchema,
    completedAt: isoTimestampSchema.nullable().optional(),
    itemCount: z.number().int().nonnegative(),
    usageTotals: sessionUsageTotalsSchema.nullable().optional(),
  })
  .strict()

export const runTranscriptSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    status: z.string().trim().min(1),
    sourceKind: sessionTranscriptSourceKindSchema,
    startedAt: isoTimestampSchema,
    completedAt: isoTimestampSchema.nullable().optional(),
    items: z.array(sessionTranscriptItemSchema),
    usageTotals: sessionUsageTotalsSchema.nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((transcript, ctx) => {
    validateStrictSequence(transcript.items, ctx, ['items'])
    transcript.items.forEach((item, index) => {
      const path = ['items', index]
      if (item.projectId !== transcript.projectId) addMismatch(ctx, path, 'project id')
      if (item.agentSessionId !== transcript.agentSessionId) addMismatch(ctx, path, 'agent session id')
      if (item.runId !== transcript.runId) addMismatch(ctx, path, 'run id')
      if (item.providerId !== transcript.providerId || item.modelId !== transcript.modelId) {
        addMismatch(ctx, path, 'provider/model')
      }
    })
  })

export const agentSessionTranscriptStatusSchema = z.enum(['active', 'archived'])
export const sessionTranscriptSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    title: z.string().trim().min(1),
    summary: z.string(),
    status: agentSessionTranscriptStatusSchema,
    archived: z.boolean(),
    archivedAt: isoTimestampSchema.nullable().optional(),
    runs: z.array(runTranscriptSummarySchema),
    items: z.array(sessionTranscriptItemSchema),
    usageTotals: sessionUsageTotalsSchema.nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((transcript, ctx) => {
    if (transcript.status === 'archived' && !transcript.archived) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['archived'],
        message: 'Archived session transcripts must set archived=true.',
      })
    }
    if (transcript.archived && !transcript.archivedAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['archivedAt'],
        message: 'Archived session transcripts must include archivedAt.',
      })
    }
    validateStrictSequence(transcript.items, ctx, ['items'])
  })

export const sessionTranscriptExportPayloadSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    exportId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    scope: sessionTranscriptScopeSchema,
    format: sessionTranscriptExportFormatSchema,
    transcript: sessionTranscriptSchema,
    contextSnapshot: z.lazy(() => sessionContextSnapshotSchema).nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionTranscriptSearchResultSnippetSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    resultId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    itemId: z.string().trim().min(1),
    archived: z.boolean(),
    rank: z.number().int().nonnegative(),
    matchedFields: z.array(z.string().trim().min(1)),
    snippet: z.string().trim().min(1),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const getSessionTranscriptRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const getSessionContextSnapshotRequestSchema = getSessionTranscriptRequestSchema
  .extend({
    providerId: nonEmptyOptionalTextSchema,
    modelId: nonEmptyOptionalTextSchema,
    pendingPrompt: z.string().nullable().optional(),
  })
  .strict()

export const exportSessionTranscriptRequestSchema = getSessionTranscriptRequestSchema
  .extend({
    format: sessionTranscriptExportFormatSchema,
  })
  .strict()

export const sessionTranscriptExportResponseSchema = z
  .object({
    payload: sessionTranscriptExportPayloadSchema,
    content: z.string().min(1),
    mimeType: z.string().trim().min(1),
    suggestedFileName: z.string().trim().min(1),
  })
  .strict()

export const saveSessionTranscriptExportRequestSchema = z
  .object({
    path: z.string().trim().min(1),
    content: z.string().min(1),
  })
  .strict()

export const searchSessionTranscriptsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    query: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    runId: nonEmptyOptionalTextSchema,
    includeArchived: z.boolean().optional(),
    limit: z.number().int().positive().max(100).optional(),
  })
  .strict()

export const searchSessionTranscriptsResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    query: z.string().trim().min(1),
    results: z.array(sessionTranscriptSearchResultSnippetSchema),
    total: z.number().int().nonnegative(),
    truncated: z.boolean(),
  })
  .strict()

export const sessionContextContributorKindSchema = z.enum([
  'system_prompt',
  'instruction_file',
  'approved_memory',
  'compaction_summary',
  'conversation_tail',
  'tool_result',
  'tool_descriptor',
  'provider_usage',
])
export const sessionContextBudgetPressureSchema = z.enum(['unknown', 'low', 'medium', 'high', 'over'])
export const sessionContextBudgetSchema = z
  .object({
    budgetTokens: z.number().int().positive().nullable().optional(),
    estimatedTokens: z.number().int().nonnegative(),
    estimationSource: sessionUsageSourceSchema,
    pressure: sessionContextBudgetPressureSchema,
    knownProviderBudget: z.boolean(),
  })
  .strict()

export const sessionContextContributorSchema = z
  .object({
    contributorId: z.string().trim().min(1),
    kind: sessionContextContributorKindSchema,
    label: z.string().trim().min(1),
    projectId: nonEmptyOptionalTextSchema,
    agentSessionId: nonEmptyOptionalTextSchema,
    runId: nonEmptyOptionalTextSchema,
    sourceId: nonEmptyOptionalTextSchema,
    sequence: z.number().int().positive(),
    estimatedTokens: z.number().int().nonnegative(),
    estimatedChars: z.number().int().nonnegative(),
    included: z.boolean(),
    modelVisible: z.boolean(),
    text: nonEmptyOptionalTextSchema,
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((contributor, ctx) => {
    if (contributor.modelVisible && !contributor.included) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['included'],
        message: 'Model-visible context contributors must also be included.',
      })
    }
  })

export const sessionContextPolicyDecisionKindSchema = z.enum(['compaction', 'memory_injection', 'instruction_file'])
export const sessionContextPolicyActionSchema = z.enum([
  'none',
  'compact_now',
  'blocked',
  'skipped',
  'inject_memory',
  'exclude_memory',
  'include_instruction',
])
export const sessionCompactionTriggerSchema = z.enum(['manual', 'auto'])
export const sessionContextPolicyDecisionSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    decisionId: z.string().trim().min(1),
    kind: sessionContextPolicyDecisionKindSchema,
    action: sessionContextPolicyActionSchema,
    trigger: sessionCompactionTriggerSchema.nullable().optional(),
    reasonCode: z.string().trim().min(1),
    message: z.string().trim().min(1),
    rawTranscriptPreserved: z.boolean(),
    modelVisible: z.boolean(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((decision, ctx) => {
    if (decision.kind === 'compaction' && !decision.rawTranscriptPreserved) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['rawTranscriptPreserved'],
        message: 'Compaction policy decisions must preserve raw transcript rows.',
      })
    }
  })

export const sessionContextSnapshotSchema = z
  .object({
    contractVersion: z.literal(CADENCE_SESSION_CONTEXT_CONTRACT_VERSION),
    snapshotId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: nonEmptyOptionalTextSchema,
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    budget: sessionContextBudgetSchema,
    contributors: z.array(sessionContextContributorSchema),
    policyDecisions: z.array(sessionContextPolicyDecisionSchema),
    usageTotals: sessionUsageTotalsSchema.nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((snapshot, ctx) => {
    validateStrictSequence(snapshot.contributors, ctx, ['contributors'])
  })

export const sessionMemoryScopeSchema = z.enum(['project', 'session'])
export const sessionMemoryKindSchema = z.enum([
  'project_fact',
  'user_preference',
  'decision',
  'session_summary',
  'troubleshooting',
])
export const sessionMemoryReviewStateSchema = z.enum(['candidate', 'approved', 'rejected'])
export const sessionMemoryRecordSchema = z
  .object({
    memoryId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    scope: sessionMemoryScopeSchema,
    kind: sessionMemoryKindSchema,
    text: z.string().trim().min(1),
    reviewState: sessionMemoryReviewStateSchema,
    enabled: z.boolean(),
    confidence: z.number().int().min(0).max(100).nullable().optional(),
    sourceRunId: nonEmptyOptionalTextSchema,
    sourceItemIds: z.array(z.string().trim().min(1)),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export type SessionTranscriptScopeDto = z.infer<typeof sessionTranscriptScopeSchema>
export type SessionTranscriptExportFormatDto = z.infer<typeof sessionTranscriptExportFormatSchema>
export type SessionTranscriptSourceKindDto = z.infer<typeof sessionTranscriptSourceKindSchema>
export type SessionTranscriptItemKindDto = z.infer<typeof sessionTranscriptItemKindSchema>
export type SessionTranscriptActorDto = z.infer<typeof sessionTranscriptActorSchema>
export type SessionTranscriptToolStateDto = z.infer<typeof sessionTranscriptToolStateSchema>
export type SessionContextRedactionClassDto = z.infer<typeof sessionContextRedactionClassSchema>
export type SessionContextRedactionDto = z.infer<typeof sessionContextRedactionSchema>
export type SessionUsageSourceDto = z.infer<typeof sessionUsageSourceSchema>
export type SessionUsageTotalsDto = z.infer<typeof sessionUsageTotalsSchema>
export type SessionTranscriptItemDto = z.infer<typeof sessionTranscriptItemSchema>
export type RunTranscriptSummaryDto = z.infer<typeof runTranscriptSummarySchema>
export type RunTranscriptDto = z.infer<typeof runTranscriptSchema>
export type AgentSessionTranscriptStatusDto = z.infer<typeof agentSessionTranscriptStatusSchema>
export type SessionTranscriptDto = z.infer<typeof sessionTranscriptSchema>
export type SessionTranscriptExportPayloadDto = z.infer<typeof sessionTranscriptExportPayloadSchema>
export type SessionTranscriptSearchResultSnippetDto = z.infer<typeof sessionTranscriptSearchResultSnippetSchema>
export type GetSessionTranscriptRequestDto = z.infer<typeof getSessionTranscriptRequestSchema>
export type GetSessionContextSnapshotRequestDto = z.infer<typeof getSessionContextSnapshotRequestSchema>
export type ExportSessionTranscriptRequestDto = z.infer<typeof exportSessionTranscriptRequestSchema>
export type SessionTranscriptExportResponseDto = z.infer<typeof sessionTranscriptExportResponseSchema>
export type SaveSessionTranscriptExportRequestDto = z.infer<typeof saveSessionTranscriptExportRequestSchema>
export type SearchSessionTranscriptsRequestDto = z.infer<typeof searchSessionTranscriptsRequestSchema>
export type SearchSessionTranscriptsResponseDto = z.infer<typeof searchSessionTranscriptsResponseSchema>
export type SessionContextContributorKindDto = z.infer<typeof sessionContextContributorKindSchema>
export type SessionContextBudgetPressureDto = z.infer<typeof sessionContextBudgetPressureSchema>
export type SessionContextBudgetDto = z.infer<typeof sessionContextBudgetSchema>
export type SessionContextContributorDto = z.infer<typeof sessionContextContributorSchema>
export type SessionContextPolicyDecisionKindDto = z.infer<typeof sessionContextPolicyDecisionKindSchema>
export type SessionContextPolicyActionDto = z.infer<typeof sessionContextPolicyActionSchema>
export type SessionCompactionTriggerDto = z.infer<typeof sessionCompactionTriggerSchema>
export type SessionContextPolicyDecisionDto = z.infer<typeof sessionContextPolicyDecisionSchema>
export type SessionContextSnapshotDto = z.infer<typeof sessionContextSnapshotSchema>
export type SessionMemoryScopeDto = z.infer<typeof sessionMemoryScopeSchema>
export type SessionMemoryKindDto = z.infer<typeof sessionMemoryKindSchema>
export type SessionMemoryReviewStateDto = z.infer<typeof sessionMemoryReviewStateSchema>
export type SessionMemoryRecordDto = z.infer<typeof sessionMemoryRecordSchema>

export function createPublicSessionContextRedaction(): SessionContextRedactionDto {
  return { redactionClass: 'public', redacted: false, reason: null }
}

export function createRedactedSessionContextText(
  value: string,
): { value: string; redaction: SessionContextRedactionDto } {
  const reason = sensitiveSessionContextReason(value)
  if (!reason) {
    return { value, redaction: createPublicSessionContextRedaction() }
  }

  return {
    value: 'Cadence redacted sensitive session-context text.',
    redaction: {
      redactionClass: reason === 'tool raw payload data' ? 'raw_payload' : 'secret',
      redacted: true,
      reason,
    },
  }
}

export function createContextBudget(estimatedTokens: number, budgetTokens?: number | null): SessionContextBudgetDto {
  if (!budgetTokens || budgetTokens <= 0) {
    return {
      budgetTokens: null,
      estimatedTokens,
      estimationSource: 'estimated',
      pressure: 'unknown',
      knownProviderBudget: false,
    }
  }
  const percent = Math.floor((estimatedTokens / budgetTokens) * 100)
  const pressure: SessionContextBudgetPressureDto =
    percent < 50 ? 'low' : percent < 80 ? 'medium' : percent <= 100 ? 'high' : 'over'
  return { budgetTokens, estimatedTokens, estimationSource: 'estimated', pressure, knownProviderBudget: true }
}

function validateStrictSequence(
  items: Array<{ sequence: number }>,
  ctx: z.RefinementCtx,
  pathPrefix: (string | number)[],
): void {
  let previous = 0
  items.forEach((item, index) => {
    if (item.sequence <= previous) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: [...pathPrefix, index, 'sequence'],
        message: 'Session-context sequences must be strictly increasing.',
      })
    }
    previous = item.sequence
  })
}

function addMismatch(ctx: z.RefinementCtx, path: (string | number)[], label: string): void {
  ctx.addIssue({
    code: z.ZodIssueCode.custom,
    path,
    message: `Session-context item ${label} must match its parent transcript.`,
  })
}

function sensitiveSessionContextReason(value: string): string | null {
  const normalized = value.toLowerCase()
  if (
    normalized.includes('sk-') ||
    normalized.includes('bearer ') ||
    normalized.includes('access_token') ||
    normalized.includes('refresh_token') ||
    normalized.includes('api_key') ||
    normalized.includes('client_secret') ||
    normalized.includes('session_token') ||
    normalized.includes('github_pat_') ||
    normalized.includes('ghp_') ||
    normalized.includes('xoxb-') ||
    normalized.includes('ya29.')
  ) {
    return 'OAuth or API token material'
  }
  if (normalized.includes('tool_payload') || normalized.includes('raw payload')) {
    return 'tool raw payload data'
  }
  return null
}
