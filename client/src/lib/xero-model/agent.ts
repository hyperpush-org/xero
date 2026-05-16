import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema, normalizeOptionalText, normalizeText } from '@xero/ui/model/shared'
import { getRuntimeAgentLabel, runtimeAgentIdSchema, runtimeRunControlInputSchema, runtimeRunDiagnosticSchema } from '@xero/ui/model/runtime'

export const agentRunStatusSchema = z.enum([
  'starting',
  'running',
  'paused',
  'cancelling',
  'cancelled',
  'handed_off',
  'completed',
  'failed',
])
export const agentMessageRoleSchema = z.enum(['system', 'developer', 'user', 'assistant', 'tool'])
export const agentRunEventKindSchema = z.enum([
  'message_delta',
  'reasoning_summary',
  'tool_started',
  'tool_delta',
  'tool_completed',
  'file_changed',
  'command_output',
  'validation_started',
  'validation_completed',
  'tool_registry_snapshot',
  'policy_decision',
  'state_transition',
  'plan_updated',
  'verification_gate',
  'environment_lifecycle_update',
  'action_required',
  'run_paused',
  'run_completed',
  'run_failed',
  'subagent_lifecycle',
])
export const agentToolCallStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const agentFileChangeOperationSchema = z.enum([
  'create',
  'write',
  'edit',
  'patch',
  'delete',
  'rename',
  'mkdir',
  'unknown',
])
export const agentCheckpointKindSchema = z.enum([
  'preflight',
  'message',
  'tool',
  'plan',
  'validation',
  'verification',
  'completion',
  'failure',
  'recovery',
])
export const agentActionRequestStatusSchema = z.enum([
  'pending',
  'approved',
  'rejected',
  'answered',
  'cancelled',
])
const sha256Schema = z.string().regex(/^[0-9a-f]{64}$/)
const traceIdSchema = z.string().regex(/^[0-9a-f]{32}$/)
const jsonObjectSchema = z.record(z.string(), z.unknown())
const agentRunLineageKindSchema = z.enum(['top_level', 'subagent_child'])
const addDuplicateStringIssues = (
  ctx: z.RefinementCtx,
  path: (string | number)[],
  values: string[],
  message: string,
) => {
  const seen = new Set<string>()
  values.forEach((value, index) => {
    if (seen.has(value)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: [...path, index],
        message,
      })
    }
    seen.add(value)
  })
}
const subagentRoleSchema = z.enum([
  'engineer',
  'debugger',
  'planner',
  'researcher',
  'reviewer',
  'agent_builder',
  'browser',
  'emulator',
  'solana',
  'database',
])

export const agentMessageAttachmentKindSchema = z.enum(['image', 'document', 'text'])

export const agentMessageAttachmentSchema = z
  .object({
    id: z.number().int(),
    messageId: z.number().int(),
    kind: agentMessageAttachmentKindSchema,
    absolutePath: z.string().min(1),
    mediaType: z.string().min(1),
    originalName: z.string().min(1),
    sizeBytes: z.number().int().nonnegative(),
    width: z.number().int().nonnegative().nullable().optional(),
    height: z.number().int().nonnegative().nullable().optional(),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const agentMessageSchema = z
  .object({
    id: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    role: agentMessageRoleSchema,
    content: z.string(),
    createdAt: isoTimestampSchema,
    attachments: z.array(agentMessageAttachmentSchema).default([]),
  })
  .strict()
  .superRefine((message, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['attachments'],
      message.attachments.map((attachment) => String(attachment.id)),
      'Agent message attachment ids must be unique.',
    )
    message.attachments.forEach((attachment, index) => {
      if (attachment.messageId !== message.id) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attachments', index, 'messageId'],
          message: 'Agent message attachments must reference the enclosing message id.',
        })
      }
    })
  })

export const agentRunEventSchema = z
  .object({
    id: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    eventKind: agentRunEventKindSchema,
    payload: z.unknown(),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const agentToolCallSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    toolCallId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    input: z.unknown(),
    state: agentToolCallStateSchema,
    result: z.unknown().nullable().optional(),
    error: runtimeRunDiagnosticSchema.nullable().optional(),
    startedAt: isoTimestampSchema,
    completedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()

export const agentFileChangeSchema = z
  .object({
    id: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    traceId: traceIdSchema,
    topLevelRunId: z.string().trim().min(1),
    subagentId: z.string().trim().min(1).nullable().optional(),
    subagentRole: subagentRoleSchema.nullable().optional(),
    changeGroupId: z.string().trim().min(1).nullable().optional(),
    path: z.string().trim().min(1),
    operation: agentFileChangeOperationSchema,
    oldHash: sha256Schema.nullable().optional(),
    newHash: sha256Schema.nullable().optional(),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const agentCheckpointSchema = z
  .object({
    id: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    checkpointKind: agentCheckpointKindSchema,
    summary: z.string().trim().min(1),
    payload: z.unknown().nullable().optional(),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const agentActionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    actionId: z.string().trim().min(1),
    actionType: z.string().trim().min(1),
    title: z.string().trim().min(1),
    detail: z.string().trim().min(1),
    status: agentActionRequestStatusSchema,
    createdAt: isoTimestampSchema,
    resolvedAt: isoTimestampSchema.nullable().optional(),
    response: z.string().nullable().optional(),
  })
  .strict()

const agentRunBaseSchema = z
  .object({
    runtimeAgentId: runtimeAgentIdSchema,
    agentDefinitionId: z.string().trim().min(1),
    agentDefinitionVersion: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    traceId: traceIdSchema,
    lineageKind: agentRunLineageKindSchema,
    parentRunId: z.string().trim().min(1).nullable().optional(),
    parentTraceId: traceIdSchema.nullable().optional(),
    parentSubagentId: z.string().trim().min(1).nullable().optional(),
    subagentRole: subagentRoleSchema.nullable().optional(),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    status: agentRunStatusSchema,
    prompt: z.string().trim().min(1),
    systemPrompt: z.string().trim().min(1),
    startedAt: isoTimestampSchema,
    lastHeartbeatAt: isoTimestampSchema.nullable().optional(),
    completedAt: isoTimestampSchema.nullable().optional(),
    cancelledAt: isoTimestampSchema.nullable().optional(),
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
    messages: z.array(agentMessageSchema),
    events: z.array(agentRunEventSchema),
    toolCalls: z.array(agentToolCallSchema),
    fileChanges: z.array(agentFileChangeSchema),
    checkpoints: z.array(agentCheckpointSchema),
    actionRequests: z.array(agentActionRequestSchema),
  })
  .strict()

type AgentRunLineageFields = Pick<
  z.infer<typeof agentRunBaseSchema>,
  'lineageKind' | 'parentRunId' | 'parentTraceId' | 'parentSubagentId' | 'subagentRole'
>

const validateAgentRunLineage = (run: AgentRunLineageFields, ctx: z.RefinementCtx) => {
  if (run.lineageKind === 'top_level') {
    if (run.parentRunId || run.parentTraceId || run.parentSubagentId || run.subagentRole) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lineageKind'],
        message: 'Top-level agent runs must not include parent subagent lineage.',
      })
    }
  } else {
    if (!run.parentRunId || !run.parentTraceId || !run.parentSubagentId || !run.subagentRole) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['lineageKind'],
        message: 'Subagent child runs must include parent run, trace, subagent id, and role.',
      })
    }
  }
}

const validateAgentRunCollections = (
  run: z.infer<typeof agentRunBaseSchema>,
  ctx: z.RefinementCtx,
) => {
  addDuplicateStringIssues(
    ctx,
    ['messages'],
    run.messages.map((message) => String(message.id)),
    'Agent run message ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['events'],
    run.events.map((event) => String(event.id)),
    'Agent run event ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['toolCalls'],
    run.toolCalls.map((toolCall) => toolCall.toolCallId),
    'Agent run tool call ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['fileChanges'],
    run.fileChanges.map((fileChange) => String(fileChange.id)),
    'Agent run file change ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['checkpoints'],
    run.checkpoints.map((checkpoint) => String(checkpoint.id)),
    'Agent run checkpoint ids must be unique.',
  )
  addDuplicateStringIssues(
    ctx,
    ['actionRequests'],
    run.actionRequests.map((actionRequest) => actionRequest.actionId),
    'Agent run action request ids must be unique.',
  )
  run.messages.forEach((message, index) => {
    if (message.projectId !== run.projectId || message.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['messages', index],
        message: 'Agent run messages must match the enclosing project and run id.',
      })
    }
  })
  run.events.forEach((event, index) => {
    if (event.projectId !== run.projectId || event.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['events', index],
        message: 'Agent run events must match the enclosing project and run id.',
      })
    }
  })
  run.toolCalls.forEach((toolCall, index) => {
    if (toolCall.projectId !== run.projectId || toolCall.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['toolCalls', index],
        message: 'Agent run tool calls must match the enclosing project and run id.',
      })
    }
  })
  run.fileChanges.forEach((fileChange, index) => {
    if (fileChange.projectId !== run.projectId || fileChange.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fileChanges', index],
        message: 'Agent run file changes must match the enclosing project and run id.',
      })
    }
    if (run.lineageKind === 'top_level' && fileChange.topLevelRunId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['fileChanges', index, 'topLevelRunId'],
        message: 'Top-level run file changes must identify the enclosing run as topLevelRunId.',
      })
    }
  })
  run.checkpoints.forEach((checkpoint, index) => {
    if (checkpoint.projectId !== run.projectId || checkpoint.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['checkpoints', index],
        message: 'Agent run checkpoints must match the enclosing project and run id.',
      })
    }
  })
  run.actionRequests.forEach((actionRequest, index) => {
    if (actionRequest.projectId !== run.projectId || actionRequest.runId !== run.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['actionRequests', index],
        message: 'Agent run action requests must match the enclosing project and run id.',
      })
    }
  })
}

export const agentRunSchema = agentRunBaseSchema.superRefine((run, ctx) => {
  validateAgentRunLineage(run, ctx)
  validateAgentRunCollections(run, ctx)
})

export const agentRunSummarySchema = agentRunBaseSchema
  .omit({
    systemPrompt: true,
    lastHeartbeatAt: true,
    messages: true,
    events: true,
    toolCalls: true,
    fileChanges: true,
    checkpoints: true,
    actionRequests: true,
  })
  .strict()
  .superRefine((run, ctx) => {
    validateAgentRunLineage(run, ctx)
  })

export const startAgentTaskRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    prompt: z.string().trim().min(1),
    controls: runtimeRunControlInputSchema.nullable().optional(),
  })
  .strict()

export const agentAutoCompactPreferenceSchema = z
  .object({
    enabled: z.boolean(),
    thresholdPercent: z.number().int().min(1).max(100).nullable().optional(),
    rawTailMessageCount: z.number().int().min(2).max(24).nullable().optional(),
  })
  .strict()

export const sendAgentMessageRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
    prompt: z.string().trim().min(1),
    autoCompact: agentAutoCompactPreferenceSchema.nullable().optional(),
  })
  .strict()

export const cancelAgentRunRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
  })
  .strict()

export const resumeAgentRunRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
    response: z.string().trim().min(1),
    autoCompact: agentAutoCompactPreferenceSchema.nullable().optional(),
  })
  .strict()

export const getAgentRunRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
  })
  .strict()

export const exportAgentTraceRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
    includeSupportBundle: z.boolean().default(false),
  })
  .strict()

export const agentTraceExportSchema = z
  .object({
    trace: jsonObjectSchema,
    timeline: jsonObjectSchema,
    diagnostics: jsonObjectSchema,
    qualityGates: jsonObjectSchema,
    productionReadiness: jsonObjectSchema,
    markdownSummary: z.string().trim().min(1),
    supportBundle: jsonObjectSchema.nullable().optional(),
    canonicalTrace: jsonObjectSchema,
  })
  .strict()

export const listAgentRunsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
  })
  .strict()

export const listAgentRunsResponseSchema = z
  .object({
    runs: z.array(agentRunSummarySchema),
  })
  .strict()
  .superRefine((response, ctx) => {
    addDuplicateStringIssues(
      ctx,
      ['runs'],
      response.runs.map((run) => run.runId),
      'Agent run list response run ids must be unique.',
    )
  })

export const subscribeAgentStreamRequestSchema = z
  .object({
    runId: z.string().trim().min(1),
  })
  .strict()

export const subscribeAgentStreamResponseSchema = z
  .object({
    runId: z.string().trim().min(1),
    replayedEventCount: z.number().int().nonnegative(),
  })
  .strict()

export type AgentRunStatusDto = z.infer<typeof agentRunStatusSchema>
export type AgentMessageRoleDto = z.infer<typeof agentMessageRoleSchema>
export type AgentRunEventKindDto = z.infer<typeof agentRunEventKindSchema>
export type AgentToolCallStateDto = z.infer<typeof agentToolCallStateSchema>
export type AgentFileChangeOperationDto = z.infer<typeof agentFileChangeOperationSchema>
export type AgentRunLineageKindDto = z.infer<typeof agentRunLineageKindSchema>
export type AgentSubagentRoleDto = z.infer<typeof subagentRoleSchema>
export type AgentCheckpointKindDto = z.infer<typeof agentCheckpointKindSchema>
export type AgentActionRequestStatusDto = z.infer<typeof agentActionRequestStatusSchema>
export type AgentMessageDto = z.infer<typeof agentMessageSchema>
export type AgentMessageAttachmentDto = z.infer<typeof agentMessageAttachmentSchema>
export type AgentMessageAttachmentKindDto = z.infer<typeof agentMessageAttachmentKindSchema>
export type AgentRunEventDto = z.infer<typeof agentRunEventSchema>
export type AgentToolCallDto = z.infer<typeof agentToolCallSchema>
export type AgentFileChangeDto = z.infer<typeof agentFileChangeSchema>
export type AgentCheckpointDto = z.infer<typeof agentCheckpointSchema>
export type AgentActionRequestDto = z.infer<typeof agentActionRequestSchema>
export type AgentRunDto = z.infer<typeof agentRunSchema>
export type AgentRunSummaryDto = z.infer<typeof agentRunSummarySchema>
export type StartAgentTaskRequestDto = z.infer<typeof startAgentTaskRequestSchema>
export type AgentAutoCompactPreferenceDto = z.infer<typeof agentAutoCompactPreferenceSchema>
export type SendAgentMessageRequestDto = z.infer<typeof sendAgentMessageRequestSchema>
export type CancelAgentRunRequestDto = z.infer<typeof cancelAgentRunRequestSchema>
export type ResumeAgentRunRequestDto = z.infer<typeof resumeAgentRunRequestSchema>
export type GetAgentRunRequestDto = z.infer<typeof getAgentRunRequestSchema>
export type ExportAgentTraceRequestDto = z.infer<typeof exportAgentTraceRequestSchema>
export type AgentTraceExportDto = z.infer<typeof agentTraceExportSchema>
export type ListAgentRunsRequestDto = z.infer<typeof listAgentRunsRequestSchema>
export type ListAgentRunsResponseDto = z.infer<typeof listAgentRunsResponseSchema>
export type SubscribeAgentStreamRequestDto = z.infer<typeof subscribeAgentStreamRequestSchema>
export type SubscribeAgentStreamResponseDto = z.infer<typeof subscribeAgentStreamResponseSchema>

export interface AgentRunView extends AgentRunDto {
  runtimeAgentLabel: string
  statusLabel: string
  providerLabel: string
  modelLabel: string
  lastErrorCode: string | null
  lastErrorMessage: string | null
  latestEvent: AgentRunEventDto | null
  isActive: boolean
  isTerminal: boolean
  isFailed: boolean
}

export function getAgentRunStatusLabel(status: AgentRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Starting'
    case 'running':
      return 'Running'
    case 'paused':
      return 'Paused'
    case 'cancelling':
      return 'Cancelling'
    case 'cancelled':
      return 'Cancelled'
    case 'handed_off':
      return 'Handed off'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
  }
}

export function mapAgentRun(run: AgentRunDto): AgentRunView {
  const latestEvent = run.events.length > 0 ? run.events[run.events.length - 1] : null
  const lastErrorCode = normalizeOptionalText(run.lastErrorCode)
  const lastErrorMessage = normalizeOptionalText(run.lastError?.message)

  return {
    ...run,
    runtimeAgentLabel: getRuntimeAgentLabel(run.runtimeAgentId),
    providerLabel: normalizeText(run.providerId, 'provider-unavailable'),
    modelLabel: normalizeText(run.modelId, 'model-unavailable'),
    statusLabel: getAgentRunStatusLabel(run.status),
    lastErrorCode,
    lastErrorMessage,
    latestEvent,
    isActive: run.status === 'starting' || run.status === 'running' || run.status === 'cancelling',
    isTerminal:
      run.status === 'cancelled' ||
      run.status === 'handed_off' ||
      run.status === 'completed' ||
      run.status === 'failed',
    isFailed: run.status === 'failed',
  }
}
