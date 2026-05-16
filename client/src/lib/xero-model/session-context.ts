import { z } from 'zod'
import { codePatchAvailabilitySchema } from '@xero/ui/model/code-history'
import { isoTimestampSchema, nonEmptyOptionalTextSchema } from '@xero/ui/model/shared'
import { agentSessionLineageSchema, agentSessionSchema } from '@xero/ui/model/runtime'

export const XERO_SESSION_CONTEXT_CONTRACT_VERSION = 1

export const sessionTranscriptScopeSchema = z.enum(['run', 'session'])
export const sessionTranscriptExportFormatSchema = z.enum(['markdown', 'json'])
export const sessionTranscriptSourceKindSchema = z.enum(['owned_agent', 'runtime_stream'])
export const sessionTranscriptItemKindSchema = z.enum([
  'message',
  'reasoning',
  'tool_call',
  'tool_result',
  'file_change',
  'code_rollback',
  'code_history_operation',
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
  'xero',
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    codeChangeGroupId: nonEmptyOptionalTextSchema,
    codeCommitId: nonEmptyOptionalTextSchema,
    codeWorkspaceEpoch: z.number().int().nonnegative().nullable().optional(),
    codePatchAvailability: codePatchAvailabilitySchema.nullable().optional(),
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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

export const compactSessionHistoryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: nonEmptyOptionalTextSchema,
    rawTailMessageCount: z.number().int().min(2).max(24).optional(),
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
  'skill_context',
  'approved_memory',
  'compaction_summary',
  'conversation_tail',
  'tool_result',
  'tool_summary',
  'tool_descriptor',
  'file_observation',
  'code_rollback',
  'code_history_operation',
  'code_history_notice',
  'code_history_mailbox_notice',
  'code_symbol',
  'dependency_metadata',
  'run_artifact',
  'provider_usage',
])
export const sessionContextTaskPhaseSchema = z.enum([
  'intake',
  'context_gather',
  'plan',
  'execute',
  'verify',
  'summarize',
  'run_artifact',
])
export const sessionContextDispositionSchema = z.enum(['include', 'summarize', 'defer', 'retrieve_on_demand'])
export const sessionContextBudgetPressureSchema = z.enum(['unknown', 'low', 'medium', 'high', 'over'])
export const sessionContextLimitSourceSchema = z.enum([
  'live_catalog',
  'app_profile',
  'built_in_registry',
  'heuristic',
  'unknown',
])
export const sessionContextLimitConfidenceSchema = z.enum(['high', 'medium', 'low', 'unknown'])
export const sessionContextLimitResolutionSchema = z
  .object({
    providerId: z.string(),
    modelId: z.string(),
    contextWindowTokens: z.number().int().positive().nullable().optional(),
    effectiveInputBudgetTokens: z.number().int().nonnegative().nullable().optional(),
    maxOutputTokens: z.number().int().nonnegative().nullable().optional(),
    outputReserveTokens: z.number().int().nonnegative(),
    safetyReserveTokens: z.number().int().nonnegative(),
    source: sessionContextLimitSourceSchema,
    confidence: sessionContextLimitConfidenceSchema,
    diagnostic: nonEmptyOptionalTextSchema,
    fetchedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
export const sessionContextBudgetSchema = z
  .object({
    budgetTokens: z.number().int().nonnegative().nullable().optional(),
    contextWindowTokens: z.number().int().positive().nullable().optional(),
    effectiveInputBudgetTokens: z.number().int().nonnegative().nullable().optional(),
    maxOutputTokens: z.number().int().nonnegative().nullable().optional(),
    outputReserveTokens: z.number().int().nonnegative(),
    safetyReserveTokens: z.number().int().nonnegative(),
    remainingTokens: z.number().int().nonnegative().nullable().optional(),
    pressurePercent: z.number().int().nonnegative().nullable().optional(),
    estimatedTokens: z.number().int().nonnegative(),
    estimationSource: sessionUsageSourceSchema,
    pressure: sessionContextBudgetPressureSchema,
    knownProviderBudget: z.boolean(),
    limitSource: sessionContextLimitSourceSchema,
    limitConfidence: sessionContextLimitConfidenceSchema,
    limitDiagnostic: nonEmptyOptionalTextSchema,
    limitFetchedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
  .superRefine((budget, ctx) => {
    if (!budget.knownProviderBudget) {
      if (budget.pressure !== 'unknown') {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['pressure'],
          message: 'Unknown context budgets must use unknown pressure.',
        })
      }
      if (budget.pressurePercent != null) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['pressurePercent'],
          message: 'Unknown context budgets must not expose pressure percent.',
        })
      }
      return
    }

    if (budget.effectiveInputBudgetTokens == null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['effectiveInputBudgetTokens'],
        message: 'Known context budgets must expose an effective input budget.',
      })
    }
    if (budget.budgetTokens !== budget.effectiveInputBudgetTokens) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['budgetTokens'],
        message: 'Budget tokens must match the effective input budget.',
      })
    }
    if (budget.remainingTokens == null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['remainingTokens'],
        message: 'Known context budgets must expose remaining tokens.',
      })
    }
    if (budget.pressurePercent == null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['pressurePercent'],
        message: 'Known context budgets must expose pressure percent.',
      })
    }
  })

export const sessionContextContributorSchema = z
  .object({
    contributorId: z.string().trim().min(1),
    kind: sessionContextContributorKindSchema,
    label: z.string().trim().min(1),
    promptFragmentId: nonEmptyOptionalTextSchema,
    promptFragmentPriority: z.number().int().nonnegative().nullable().optional(),
    promptFragmentHash: z.string().regex(/^[0-9a-f]{64}$/).nullable().optional(),
    promptFragmentProvenance: nonEmptyOptionalTextSchema,
    projectId: nonEmptyOptionalTextSchema,
    agentSessionId: nonEmptyOptionalTextSchema,
    runId: nonEmptyOptionalTextSchema,
    sourceId: nonEmptyOptionalTextSchema,
    sequence: z.number().int().positive(),
    estimatedTokens: z.number().int().nonnegative(),
    estimatedChars: z.number().int().nonnegative(),
    recencyScore: z.number().int().min(0).max(100),
    relevanceScore: z.number().int().min(0).max(100),
    authorityScore: z.number().int().min(0).max(100),
    rankScore: z.number().int().positive(),
    taskPhase: sessionContextTaskPhaseSchema,
    disposition: sessionContextDispositionSchema,
    included: z.boolean(),
    modelVisible: z.boolean(),
    summary: nonEmptyOptionalTextSchema,
    omittedReason: nonEmptyOptionalTextSchema,
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

export const sessionContextCodeSymbolSchema = z
  .object({
    symbolId: z.string().trim().min(1),
    name: z.string().trim().min(1),
    kind: z.string().trim().min(1),
    path: z.string().trim().min(1),
    line: z.number().int().positive(),
    estimatedTokens: z.number().int().nonnegative(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionContextDependencyManifestSchema = z
  .object({
    path: z.string().trim().min(1),
    ecosystem: z.string().trim().min(1),
    packageName: nonEmptyOptionalTextSchema,
    dependencyCount: z.number().int().nonnegative(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionContextCodeMapSchema = z
  .object({
    generatedFromRoot: z.string().trim().min(1),
    sourceRoots: z.array(z.string().trim().min(1)),
    packageManifests: z.array(sessionContextDependencyManifestSchema),
    symbols: z.array(sessionContextCodeSymbolSchema),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionContextSnapshotDiffSchema = z
  .object({
    previousSnapshotId: nonEmptyOptionalTextSchema,
    addedContributorIds: z.array(z.string().trim().min(1)),
    removedContributorIds: z.array(z.string().trim().min(1)),
    changedContributorIds: z.array(z.string().trim().min(1)),
    estimatedTokenDelta: z.number().int(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionContextPolicyDecisionKindSchema = z.enum([
  'compaction',
  'handoff',
  'memory_injection',
  'instruction_file',
  'retrieval',
])
export const sessionContextPolicyActionSchema = z.enum([
  'continue_now',
  'none',
  'compact_now',
  'recompact_now',
  'handoff_now',
  'blocked',
  'skipped',
  'inject_memory',
  'exclude_memory',
  'include_instruction',
])
export const sessionCompactionTriggerSchema = z.enum(['manual', 'auto'])
export const sessionContextPolicyDecisionSchema = z
  .object({
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
    snapshotId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: nonEmptyOptionalTextSchema,
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    generatedAt: isoTimestampSchema,
    budget: sessionContextBudgetSchema,
    providerRequestHash: z.string().regex(/^[0-9a-f]{64}$/),
    includedTokenEstimate: z.number().int().nonnegative(),
    deferredTokenEstimate: z.number().int().nonnegative(),
    codeMap: sessionContextCodeMapSchema,
    diff: sessionContextSnapshotDiffSchema.nullable().optional(),
    contributors: z.array(sessionContextContributorSchema),
    policyDecisions: z.array(sessionContextPolicyDecisionSchema),
    usageTotals: sessionUsageTotalsSchema.nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((snapshot, ctx) => {
    validateStrictSequence(snapshot.contributors, ctx, ['contributors'])
    const includedTokens = snapshot.contributors
      .filter((contributor) => contributor.included && contributor.modelVisible)
      .reduce((total, contributor) => total + contributor.estimatedTokens, 0)
    if (snapshot.includedTokenEstimate !== includedTokens) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['includedTokenEstimate'],
        message: 'Included token estimate must match model-visible contributors.',
      })
    }
    const deferredTokens = snapshot.contributors
      .filter((contributor) => !(contributor.included && contributor.modelVisible))
      .reduce((total, contributor) => total + contributor.estimatedTokens, 0)
    if (snapshot.deferredTokenEstimate !== deferredTokens) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['deferredTokenEstimate'],
        message: 'Deferred token estimate must match deferred contributors.',
      })
    }
  })

export const sessionCompactionDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const sessionCompactionRecordSchema = z
  .object({
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
    compactionId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    sourceRunId: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    summary: z.string().trim().min(1),
    coveredRunIds: z.array(z.string().trim().min(1)).min(1),
    coveredMessageStartId: z.number().int().positive().nullable().optional(),
    coveredMessageEndId: z.number().int().positive().nullable().optional(),
    coveredEventStartId: z.number().int().positive().nullable().optional(),
    coveredEventEndId: z.number().int().positive().nullable().optional(),
    sourceHash: z.string().regex(/^[0-9a-f]{64}$/),
    inputTokens: z.number().int().nonnegative(),
    summaryTokens: z.number().int().nonnegative(),
    rawTailMessageCount: z.number().int().nonnegative(),
    policyReason: z.string().trim().min(1),
    trigger: sessionCompactionTriggerSchema,
    active: z.boolean(),
    diagnostic: sessionCompactionDiagnosticSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    supersededAt: isoTimestampSchema.nullable().optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((compaction, ctx) => {
    if (
      compaction.coveredMessageStartId &&
      compaction.coveredMessageEndId &&
      compaction.coveredMessageStartId > compaction.coveredMessageEndId
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['coveredMessageEndId'],
        message: 'Compaction message range must be ordered.',
      })
    }
    if (
      compaction.coveredEventStartId &&
      compaction.coveredEventEndId &&
      compaction.coveredEventStartId > compaction.coveredEventEndId
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['coveredEventEndId'],
        message: 'Compaction event range must be ordered.',
      })
    }
  })

export const compactSessionHistoryResponseSchema = z
  .object({
    compaction: sessionCompactionRecordSchema,
    contextSnapshot: sessionContextSnapshotSchema,
  })
  .strict()

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
    contractVersion: z.literal(XERO_SESSION_CONTEXT_CONTRACT_VERSION),
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
    diagnostic: z
      .object({
        code: z.string().trim().min(1),
        message: z.string().trim().min(1),
        redaction: sessionContextRedactionSchema,
      })
      .strict()
      .nullable()
      .optional(),
    redaction: sessionContextRedactionSchema,
  })
  .strict()
  .superRefine((memory, ctx) => {
    if (memory.scope === 'project' && memory.agentSessionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['agentSessionId'],
        message: 'Project memory must not include a session id.',
      })
    }
    if (memory.scope === 'session' && !memory.agentSessionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['agentSessionId'],
        message: 'Session memory must include a session id.',
      })
    }
    if (memory.reviewState !== 'approved' && memory.enabled) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['enabled'],
        message: 'Only approved memory can be enabled.',
      })
    }
  })

export const listSessionMemoriesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    includeDisabled: z.boolean().optional(),
    includeRejected: z.boolean().optional(),
  })
  .strict()

export const listSessionMemoriesResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    memories: z.array(sessionMemoryRecordSchema),
  })
  .strict()
  .superRefine((response, ctx) => {
    response.memories.forEach((memory, index) => {
      if (memory.projectId !== response.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['memories', index, 'projectId'],
          message: 'Listed session memories must match the response project.',
        })
      }
      if (
        memory.scope === 'session' &&
        (memory.agentSessionId ?? null) !== (response.agentSessionId ?? null)
      ) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['memories', index, 'agentSessionId'],
          message: 'Listed session memories must match the response agent session.',
        })
      }
    })
  })

export const getSessionMemoryReviewQueueRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    limit: z.number().int().positive().max(100).nullable().optional(),
  })
  .strict()

export const extractSessionMemoryCandidatesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const sessionMemoryDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
    redaction: sessionContextRedactionSchema,
  })
  .strict()

export const agentMemoryFreshnessStateSchema = z.enum([
  'current',
  'source_unknown',
  'stale',
  'source_missing',
  'superseded',
  'blocked',
])

export const agentMemoryReviewQueueItemSchema = z
  .object({
    memoryId: z.string().trim().min(1),
    scope: sessionMemoryScopeSchema,
    kind: sessionMemoryKindSchema,
    reviewState: sessionMemoryReviewStateSchema,
    enabled: z.boolean(),
    confidence: z.number().int().min(0).max(100).nullable().optional(),
    textPreview: z.string().nullable(),
    textHash: z.string().trim().min(1),
    provenance: z
      .object({
        sourceRunId: nonEmptyOptionalTextSchema,
        sourceItemIds: z.array(z.string().trim().min(1)),
        diagnostic: z
          .object({
            code: z.string().trim().min(1),
            message: z.string().trim().min(1),
          })
          .strict()
          .nullable()
          .optional(),
      })
      .strict(),
    freshness: z
      .object({
        state: agentMemoryFreshnessStateSchema,
        checkedAt: nonEmptyOptionalTextSchema,
        staleReason: nonEmptyOptionalTextSchema,
        supersedesId: nonEmptyOptionalTextSchema,
        supersededById: nonEmptyOptionalTextSchema,
        invalidatedAt: nonEmptyOptionalTextSchema,
        factKey: nonEmptyOptionalTextSchema,
      })
      .strict(),
    retrieval: z
      .object({
        eligible: z.boolean(),
        reason: z.enum([
          'pending_or_rejected_review',
          'disabled',
          'superseded',
          'invalidated',
          'stale',
          'source_missing',
          'blocked',
          'retrievable',
        ]),
      })
      .strict(),
    redaction: z
      .object({
        textPreviewRedacted: z.boolean(),
        factKeyRedacted: z.boolean(),
        rawTextHidden: z.literal(true),
      })
      .strict(),
    availableActions: z
      .object({
        canApprove: z.boolean(),
        canReject: z.boolean(),
        canDisable: z.boolean(),
        canDelete: z.boolean(),
        canEditByCorrection: z.boolean(),
      })
      .strict(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((item, ctx) => {
    if (item.redaction.textPreviewRedacted && item.textPreview !== null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['textPreview'],
        message: 'Redacted memory preview text must stay hidden.',
      })
    }
    if (!item.redaction.textPreviewRedacted && !item.textPreview) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['textPreview'],
        message: 'Unredacted memory review items must include a preview.',
      })
    }
    if (item.redaction.textPreviewRedacted && item.availableActions.canApprove) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['availableActions', 'canApprove'],
        message: 'Redacted memory review items cannot be approved directly.',
      })
    }
    if (item.retrieval.eligible !== (item.retrieval.reason === 'retrievable')) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['retrieval', 'reason'],
        message: 'Memory review retrieval eligibility must match its reason.',
      })
    }
  })

export const getSessionMemoryReviewQueueResponseSchema = z
  .object({
    schema: z.literal('xero.agent_memory_review_queue.v1'),
    projectId: z.string().trim().min(1),
    agentSessionId: nonEmptyOptionalTextSchema,
    limit: z.number().int().positive().max(100),
    counts: z
      .object({
        candidate: z.number().int().nonnegative(),
        approved: z.number().int().nonnegative(),
        rejected: z.number().int().nonnegative(),
        disabled: z.number().int().nonnegative(),
        retrievableApproved: z.number().int().nonnegative(),
      })
      .strict(),
    items: z.array(agentMemoryReviewQueueItemSchema),
    actions: z
      .object({
        approve: z.string().trim().min(1),
        reject: z.string().trim().min(1),
        disable: z.string().trim().min(1),
        delete: z.string().trim().min(1),
        edit: z.string().trim().min(1),
      })
      .strict(),
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((queue, ctx) => {
    if (queue.items.length > queue.limit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['items'],
        message: 'Memory review queue items must not exceed the requested limit.',
      })
    }
    if (queue.counts.retrievableApproved > queue.counts.approved) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'retrievableApproved'],
        message: 'Retrievable approved memory count cannot exceed approved memory count.',
      })
    }
    const memoryIds = new Set<string>()
    const returnedCounts = {
      candidate: 0,
      approved: 0,
      rejected: 0,
      disabled: 0,
      retrievableApproved: 0,
    }
    queue.items.forEach((item, index) => {
      if (memoryIds.has(item.memoryId)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['items', index, 'memoryId'],
          message: 'Memory review queue item ids must be unique.',
        })
      }
      memoryIds.add(item.memoryId)
      returnedCounts[item.reviewState] += 1
      if (!item.enabled) returnedCounts.disabled += 1
      if (item.reviewState === 'approved' && item.retrieval.eligible) {
        returnedCounts.retrievableApproved += 1
      }
    })
    if (returnedCounts.candidate > queue.counts.candidate) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'candidate'],
        message: 'Returned candidate memory items cannot exceed candidate count.',
      })
    }
    if (returnedCounts.approved > queue.counts.approved) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'approved'],
        message: 'Returned approved memory items cannot exceed approved count.',
      })
    }
    if (returnedCounts.rejected > queue.counts.rejected) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'rejected'],
        message: 'Returned rejected memory items cannot exceed rejected count.',
      })
    }
    if (returnedCounts.disabled > queue.counts.disabled) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'disabled'],
        message: 'Returned disabled memory items cannot exceed disabled count.',
      })
    }
    if (returnedCounts.retrievableApproved > queue.counts.retrievableApproved) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['counts', 'retrievableApproved'],
        message: 'Returned retrievable approved memory items cannot exceed retrievable count.',
      })
    }
  })

export const extractSessionMemoryCandidatesResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    memories: z.array(sessionMemoryRecordSchema),
    createdCount: z.number().int().nonnegative(),
    skippedDuplicateCount: z.number().int().nonnegative(),
    rejectedCount: z.number().int().nonnegative(),
    diagnostics: z.array(sessionMemoryDiagnosticSchema),
  })
  .strict()
  .superRefine((response, ctx) => {
    response.memories.forEach((memory, index) => {
      if (memory.projectId !== response.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['memories', index, 'projectId'],
          message: 'Extracted memory candidates must match the response project.',
        })
      }
      if (memory.scope === 'session' && (memory.agentSessionId ?? null) !== response.agentSessionId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['memories', index, 'agentSessionId'],
          message: 'Extracted memory candidates must match the response agent session.',
        })
      }
    })
  })

export const updateSessionMemoryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    memoryId: z.string().trim().min(1),
    reviewState: sessionMemoryReviewStateSchema.nullable().optional(),
    enabled: z.boolean().nullable().optional(),
  })
  .strict()

export const correctSessionMemoryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    memoryId: z.string().trim().min(1),
    correctedText: z.string().trim().min(1),
  })
  .strict()

export const correctSessionMemoryResponseSchema = z
  .object({
    schema: z.literal('xero.agent_memory_correction_command.v1'),
    projectId: z.string().trim().min(1),
    originalMemory: sessionMemoryRecordSchema,
    correctedMemory: sessionMemoryRecordSchema,
    uiDeferred: z.literal(true),
  })
  .strict()
  .superRefine((response, ctx) => {
    if (response.originalMemory.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['originalMemory', 'projectId'],
        message: 'Corrected memory response original memory must match the response project.',
      })
    }
    if (response.correctedMemory.projectId !== response.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory', 'projectId'],
        message: 'Corrected memory response corrected memory must match the response project.',
      })
    }
    if (response.correctedMemory.scope !== response.originalMemory.scope) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory', 'scope'],
        message: 'Corrected memory must keep the original memory scope.',
      })
    }
    if (
      (response.correctedMemory.agentSessionId ?? null) !==
      (response.originalMemory.agentSessionId ?? null)
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory', 'agentSessionId'],
        message: 'Corrected memory must keep the original memory agent session.',
      })
    }
    if (response.originalMemory.memoryId === response.correctedMemory.memoryId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory', 'memoryId'],
        message: 'Corrected memory must be a new durable record.',
      })
    }
    if (
      !response.correctedMemory.sourceItemIds.includes(
        `corrected-memory:${response.originalMemory.memoryId}`,
      )
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory', 'sourceItemIds'],
        message: 'Corrected memory must cite the memory it corrects.',
      })
    }
    if (response.correctedMemory.reviewState !== 'approved' || !response.correctedMemory.enabled) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['correctedMemory'],
        message: 'Corrected memory must be approved and enabled for retrieval.',
      })
    }
  })

export const deleteSessionMemoryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    memoryId: z.string().trim().min(1),
  })
  .strict()

export const branchAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    sourceAgentSessionId: z.string().trim().min(1),
    sourceRunId: z.string().trim().min(1),
    title: z.string().trim().min(1).nullable().optional(),
    selected: z.boolean().optional(),
  })
  .strict()

export const rewindAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    sourceAgentSessionId: z.string().trim().min(1),
    sourceRunId: z.string().trim().min(1),
    boundaryKind: z.enum(['message', 'checkpoint']),
    sourceMessageId: z.number().int().positive().nullable().optional(),
    sourceCheckpointId: z.number().int().positive().nullable().optional(),
    title: z.string().trim().min(1).nullable().optional(),
    selected: z.boolean().optional(),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (request.boundaryKind === 'message' && !request.sourceMessageId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['sourceMessageId'],
        message: 'Message rewind requests require sourceMessageId.',
      })
    }
    if (request.boundaryKind === 'message' && request.sourceCheckpointId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['sourceCheckpointId'],
        message: 'Message rewind requests must not include sourceCheckpointId.',
      })
    }
    if (request.boundaryKind === 'checkpoint' && !request.sourceCheckpointId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['sourceCheckpointId'],
        message: 'Checkpoint rewind requests require sourceCheckpointId.',
      })
    }
    if (request.boundaryKind === 'checkpoint' && request.sourceMessageId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['sourceMessageId'],
        message: 'Checkpoint rewind requests must not include sourceMessageId.',
      })
    }
  })

export const agentSessionBranchResponseSchema = z
  .object({
    session: agentSessionSchema,
    lineage: agentSessionLineageSchema,
    replayRunId: z.string().trim().min(1),
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
export type CompactSessionHistoryRequestDto = z.infer<typeof compactSessionHistoryRequestSchema>
export type ExportSessionTranscriptRequestDto = z.infer<typeof exportSessionTranscriptRequestSchema>
export type SessionTranscriptExportResponseDto = z.infer<typeof sessionTranscriptExportResponseSchema>
export type SaveSessionTranscriptExportRequestDto = z.infer<typeof saveSessionTranscriptExportRequestSchema>
export type SearchSessionTranscriptsRequestDto = z.infer<typeof searchSessionTranscriptsRequestSchema>
export type SearchSessionTranscriptsResponseDto = z.infer<typeof searchSessionTranscriptsResponseSchema>
export type SessionContextContributorKindDto = z.infer<typeof sessionContextContributorKindSchema>
export type SessionContextTaskPhaseDto = z.infer<typeof sessionContextTaskPhaseSchema>
export type SessionContextDispositionDto = z.infer<typeof sessionContextDispositionSchema>
export type SessionContextBudgetPressureDto = z.infer<typeof sessionContextBudgetPressureSchema>
export type SessionContextLimitSourceDto = z.infer<typeof sessionContextLimitSourceSchema>
export type SessionContextLimitConfidenceDto = z.infer<typeof sessionContextLimitConfidenceSchema>
export type SessionContextLimitResolutionDto = z.infer<typeof sessionContextLimitResolutionSchema>
export type SessionContextBudgetDto = z.infer<typeof sessionContextBudgetSchema>
export type SessionContextContributorDto = z.infer<typeof sessionContextContributorSchema>
export type SessionContextCodeSymbolDto = z.infer<typeof sessionContextCodeSymbolSchema>
export type SessionContextDependencyManifestDto = z.infer<typeof sessionContextDependencyManifestSchema>
export type SessionContextCodeMapDto = z.infer<typeof sessionContextCodeMapSchema>
export type SessionContextSnapshotDiffDto = z.infer<typeof sessionContextSnapshotDiffSchema>
export type SessionContextPolicyDecisionKindDto = z.infer<typeof sessionContextPolicyDecisionKindSchema>
export type SessionContextPolicyActionDto = z.infer<typeof sessionContextPolicyActionSchema>
export type SessionCompactionTriggerDto = z.infer<typeof sessionCompactionTriggerSchema>
export type SessionContextPolicyDecisionDto = z.infer<typeof sessionContextPolicyDecisionSchema>
export type SessionContextSnapshotDto = z.infer<typeof sessionContextSnapshotSchema>
export type SessionCompactionDiagnosticDto = z.infer<typeof sessionCompactionDiagnosticSchema>
export type SessionCompactionRecordDto = z.infer<typeof sessionCompactionRecordSchema>
export type CompactSessionHistoryResponseDto = z.infer<typeof compactSessionHistoryResponseSchema>
export type SessionMemoryScopeDto = z.infer<typeof sessionMemoryScopeSchema>
export type SessionMemoryKindDto = z.infer<typeof sessionMemoryKindSchema>
export type SessionMemoryReviewStateDto = z.infer<typeof sessionMemoryReviewStateSchema>
export type SessionMemoryRecordDto = z.infer<typeof sessionMemoryRecordSchema>
export type ListSessionMemoriesRequestDto = z.infer<typeof listSessionMemoriesRequestSchema>
export type ListSessionMemoriesResponseDto = z.infer<typeof listSessionMemoriesResponseSchema>
export type GetSessionMemoryReviewQueueRequestDto = z.infer<typeof getSessionMemoryReviewQueueRequestSchema>
export type AgentMemoryFreshnessStateDto = z.infer<typeof agentMemoryFreshnessStateSchema>
export type AgentMemoryReviewQueueItemDto = z.infer<typeof agentMemoryReviewQueueItemSchema>
export type GetSessionMemoryReviewQueueResponseDto = z.infer<typeof getSessionMemoryReviewQueueResponseSchema>
export type SessionMemoryDiagnosticDto = z.infer<typeof sessionMemoryDiagnosticSchema>
export type ExtractSessionMemoryCandidatesRequestDto = z.infer<typeof extractSessionMemoryCandidatesRequestSchema>
export type ExtractSessionMemoryCandidatesResponseDto = z.infer<typeof extractSessionMemoryCandidatesResponseSchema>
export type UpdateSessionMemoryRequestDto = z.infer<typeof updateSessionMemoryRequestSchema>
export type CorrectSessionMemoryRequestDto = z.infer<typeof correctSessionMemoryRequestSchema>
export type CorrectSessionMemoryResponseDto = z.infer<typeof correctSessionMemoryResponseSchema>
export type DeleteSessionMemoryRequestDto = z.infer<typeof deleteSessionMemoryRequestSchema>
export type BranchAgentSessionRequestDto = z.infer<typeof branchAgentSessionRequestSchema>
export type RewindAgentSessionRequestDto = z.infer<typeof rewindAgentSessionRequestSchema>
export type AgentSessionBranchResponseDto = z.infer<typeof agentSessionBranchResponseSchema>

export function createPublicSessionContextRedaction(): SessionContextRedactionDto {
  return { redactionClass: 'public', redacted: false, reason: null }
}

export function createRedactedSessionContextText(
  value: string,
): { value: string; redaction: SessionContextRedactionDto } {
  const classification = sensitiveSessionContextClassification(value)
  if (!classification) {
    return { value, redaction: createPublicSessionContextRedaction() }
  }

  return {
    value:
      classification.redactionClass === 'local_path'
        ? '[redacted-path]'
        : 'Xero redacted sensitive session-context text.',
    redaction: {
      redactionClass: classification.redactionClass,
      redacted: true,
      reason: classification.reason,
    },
  }
}

export function createContextBudget(estimatedTokens: number, budgetTokens?: number | null): SessionContextBudgetDto {
  if (!budgetTokens || budgetTokens <= 0) {
    return {
      budgetTokens: null,
      contextWindowTokens: null,
      effectiveInputBudgetTokens: null,
      maxOutputTokens: 4096,
      outputReserveTokens: 4096,
      safetyReserveTokens: 0,
      remainingTokens: null,
      pressurePercent: null,
      estimatedTokens,
      estimationSource: 'estimated',
      pressure: 'unknown',
      knownProviderBudget: false,
      limitSource: 'unknown',
      limitConfidence: 'unknown',
      limitDiagnostic: "Xero can estimate the next request size, but the selected model's context window is unknown.",
      limitFetchedAt: null,
    }
  }
  const percent = Math.ceil((estimatedTokens / budgetTokens) * 100)
  const pressure: SessionContextBudgetPressureDto =
    percent < 50 ? 'low' : percent < 75 ? 'medium' : percent <= 100 ? 'high' : 'over'
  return {
    budgetTokens,
    contextWindowTokens: budgetTokens,
    effectiveInputBudgetTokens: budgetTokens,
    maxOutputTokens: null,
    outputReserveTokens: 0,
    safetyReserveTokens: 0,
    remainingTokens: Math.max(0, budgetTokens - estimatedTokens),
    pressurePercent: percent,
    estimatedTokens,
    estimationSource: 'estimated',
    pressure,
    knownProviderBudget: true,
    limitSource: 'heuristic',
    limitConfidence: 'low',
    limitDiagnostic: 'Legacy budget value supplied without context-window metadata.',
    limitFetchedAt: null,
  }
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

function sensitiveSessionContextClassification(value: string): {
  reason: string
  redactionClass: SessionContextRedactionClassDto
} | null {
  const normalized = value.toLowerCase()
  if (looksLikePromptInjectionText(normalized)) {
    return { reason: 'prompt-injection-shaped memory text', redactionClass: 'transcript' }
  }
  if (looksLikeEndpointCredential(value)) {
    return { reason: 'endpoint credential material', redactionClass: 'secret' }
  }
  if (
    normalized.includes('sk-') ||
    normalized.includes('bearer ') ||
    normalized.includes('bearer:') ||
    normalized.includes('authorization=') ||
    normalized.includes('authorization:') ||
    normalized.includes('access_token') ||
    normalized.includes('refresh_token') ||
    normalized.includes('api_key') ||
    normalized.includes('anthropic_api_key') ||
    normalized.includes('aws_secret_access_key') ||
    normalized.includes('aws_session_token') ||
    normalized.includes('client_secret') ||
    normalized.includes('github_token') ||
    normalized.includes('google_oauth_access_token') ||
    normalized.includes('openai_api_key') ||
    normalized.includes('session_id=') ||
    normalized.includes('session_token') ||
    normalized.includes('token=') ||
    normalized.includes('token:') ||
    normalized.includes('"token"') ||
    normalized.includes('github_pat_') ||
    normalized.includes('ghp_') ||
    normalized.includes('xoxb-') ||
    normalized.includes('ya29.')
  ) {
    return { reason: 'OAuth or API token material', redactionClass: 'secret' }
  }
  if (normalized.includes('tool_payload') || normalized.includes('raw payload')) {
    return { reason: 'tool raw payload data', redactionClass: 'raw_payload' }
  }
  if (looksLikeSecretBearingPath(value)) {
    return { reason: 'local secret-bearing path', redactionClass: 'local_path' }
  }
  return null
}

function looksLikePromptInjectionText(normalized: string): boolean {
  return [
    'ignore previous instructions',
    'ignore all previous instructions',
    'disregard previous instructions',
    'override the system prompt',
    'override system instructions',
    'reveal the system prompt',
    'reveal hidden instructions',
    'treat this memory as higher priority',
    'developer message override',
    'system message override',
  ].some((marker) => normalized.includes(marker))
}

function looksLikeEndpointCredential(value: string): boolean {
  return value.split(/\s+/).some((word) => {
    const token = word.replace(/^[,;()[\]"'`]+|[,;()[\]"'`]+$/g, '')
    const schemeIndex = token.indexOf('://')
    if (schemeIndex < 0) {
      return false
    }
    const rest = token.slice(schemeIndex + 3)
    const authority = rest.split(/[/?#]/)[0] ?? ''
    if (authority.includes('@')) {
      return true
    }
    const query = token.includes('?') ? token.slice(token.indexOf('?') + 1) : ''
    return query
      .split('&')
      .map((pair) => pair.split('='))
      .some(([key, secret]) => Boolean(secret) && isSensitiveContextName(key ?? ''))
  })
}

function isSensitiveContextName(value: string): boolean {
  const normalized = value.trim().replace(/^-+/, '').toLowerCase().replace(/-/g, '_')
  return [
    'access_token',
    'api_key',
    'apikey',
    'anthropic_api_key',
    'authorization',
    'aws_access_key_id',
    'aws_secret_access_key',
    'aws_session_token',
    'auth_token',
    'bearer',
    'client_secret',
    'github_token',
    'google_oauth_access_token',
    'openai_api_key',
    'password',
    'private_key',
    'refresh_token',
    'secret',
    'session_id',
    'session_token',
    'token',
    'x_api_key',
  ].includes(normalized)
}

function looksLikeSecretBearingPath(value: string): boolean {
  const normalized = value.replace(/\\/g, '/').toLowerCase()
  return (
    normalized.includes('/.ssh/') ||
    normalized.includes('/.aws/') ||
    normalized.includes('/.config/') ||
    normalized.includes(':/programdata/') ||
    normalized.includes(':/windows/temp/') ||
    normalized.startsWith('%appdata%/') ||
    normalized.startsWith('%localappdata%/') ||
    normalized.includes('.env') ||
    normalized.includes('credentials') ||
    normalized.includes('keychain')
  )
}
