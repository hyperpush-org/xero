import { z } from 'zod'
import { isoTimestampSchema, nonEmptyOptionalTextSchema, normalizeOptionalText, normalizeText } from './shared'
import { getRuntimeAgentLabel, runtimeAgentIdSchema, runtimeRunControlInputSchema, runtimeRunDiagnosticSchema } from './runtime'

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
  'action_required',
  'run_paused',
  'run_completed',
  'run_failed',
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

export const agentRunSchema = z
  .object({
    runtimeAgentId: runtimeAgentIdSchema,
    agentDefinitionId: z.string().trim().min(1),
    agentDefinitionVersion: z.number().int().positive(),
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
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

export const agentRunSummarySchema = agentRunSchema
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
