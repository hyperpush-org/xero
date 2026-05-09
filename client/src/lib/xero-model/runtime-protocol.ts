import { z } from 'zod'

export const XERO_AGENT_PROTOCOL_VERSION = 3 as const

const traceIdSchema = z
  .string()
  .regex(/^[0-9a-f]{32}$/, 'Runtime protocol trace IDs must be lowercase 32-character hex strings.')
const spanIdSchema = z
  .string()
  .regex(/^[0-9a-f]{16}$/, 'Runtime protocol span IDs must be lowercase 16-character hex strings.')
const nonEmptyTextSchema = z.string().trim().min(1)
const jsonObjectSchema = z.record(z.string(), z.unknown())

export const runtimeProtocolTraceContextSchema = z
  .object({
    traceId: traceIdSchema,
    spanId: spanIdSchema,
    parentSpanId: spanIdSchema.optional(),
    runTraceId: traceIdSchema,
    providerTurnTraceId: traceIdSchema.optional(),
    toolCallTraceId: traceIdSchema.optional(),
    approvalDecisionTraceId: traceIdSchema.optional(),
    storageWriteTraceId: traceIdSchema.optional(),
  })
  .strict()
  .superRefine((trace, ctx) => {
    if (trace.runTraceId !== trace.traceId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['runTraceId'],
        message: 'Runtime protocol runTraceId must match traceId.',
      })
    }
  })

export const runtimeProtocolRunStatusSchema = z.enum([
  'starting',
  'running',
  'paused',
  'cancelling',
  'cancelled',
  'handed_off',
  'completed',
  'failed',
])

export const runtimeProtocolMessageRoleSchema = z.enum([
  'system',
  'developer',
  'user',
  'assistant',
  'tool',
])

export const runtimeProtocolEventKindSchema = z.enum([
  'run_started',
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
  'context_manifest_recorded',
  'retrieval_performed',
  'memory_candidate_captured',
  'environment_lifecycle_update',
  'sandbox_lifecycle_update',
  'action_required',
  'approval_required',
  'tool_permission_grant',
  'provider_model_changed',
  'runtime_settings_changed',
  'run_paused',
  'run_completed',
  'run_failed',
])

const providerSelectionSchema = z
  .object({
    providerId: nonEmptyTextSchema,
    modelId: nonEmptyTextSchema,
  })
  .strict()

const runControlsSchema = z
  .object({
    runtimeAgentId: nonEmptyTextSchema,
    approvalMode: nonEmptyTextSchema,
    planModeRequired: z.boolean(),
  })
  .strict()

const startRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    agentSessionId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    prompt: nonEmptyTextSchema,
    provider: providerSelectionSchema,
    controls: runControlsSchema.nullable().optional(),
  })
  .strict()

const continueRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    prompt: nonEmptyTextSchema,
  })
  .strict()

const userInputRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    text: nonEmptyTextSchema,
  })
  .strict()

const approvalDecisionRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    actionId: nonEmptyTextSchema,
    response: z.string().nullable().optional(),
  })
  .strict()

const runIdRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
  })
  .strict()

const forkSessionRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    sourceAgentSessionId: nonEmptyTextSchema,
    targetAgentSessionId: nonEmptyTextSchema,
  })
  .strict()

const compactSessionRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    agentSessionId: nonEmptyTextSchema,
    reason: nonEmptyTextSchema,
  })
  .strict()

const providerModelChangeRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema.optional(),
    provider: providerSelectionSchema,
    reason: z.string().optional(),
  })
  .strict()

const runtimeSettingsChangeRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema.optional(),
    settings: jsonObjectSchema,
    reason: z.string().optional(),
  })
  .strict()

const toolPermissionGrantRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    grantId: nonEmptyTextSchema,
    toolName: nonEmptyTextSchema,
    expiresAt: z.string().optional(),
  })
  .strict()

export const runtimeSubmissionSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('start_run'), payload: startRunRequestSchema }).strict(),
  z.object({ kind: z.literal('continue_run'), payload: continueRunRequestSchema }).strict(),
  z.object({ kind: z.literal('user_message'), payload: userInputRequestSchema }).strict(),
  z.object({ kind: z.literal('approval_decision'), payload: approvalDecisionRequestSchema }).strict(),
  z.object({ kind: z.literal('tool_permission_grant'), payload: toolPermissionGrantRequestSchema }).strict(),
  z.object({ kind: z.literal('cancel'), payload: runIdRequestSchema }).strict(),
  z.object({ kind: z.literal('resume'), payload: z.object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    response: nonEmptyTextSchema,
  }).strict() }).strict(),
  z.object({ kind: z.literal('fork'), payload: forkSessionRequestSchema }).strict(),
  z.object({ kind: z.literal('compact'), payload: compactSessionRequestSchema }).strict(),
  z.object({ kind: z.literal('export_trace'), payload: runIdRequestSchema }).strict(),
  z.object({ kind: z.literal('provider_model_change'), payload: providerModelChangeRequestSchema }).strict(),
  z.object({ kind: z.literal('runtime_settings_change'), payload: runtimeSettingsChangeRequestSchema }).strict(),
])

export const runtimeSubmissionEnvelopeSchema = z
  .object({
    protocolVersion: z.literal(XERO_AGENT_PROTOCOL_VERSION),
    submissionId: nonEmptyTextSchema,
    trace: runtimeProtocolTraceContextSchema,
    submittedAt: nonEmptyTextSchema,
    submission: runtimeSubmissionSchema,
  })
  .strict()

const runtimePlanItemSchema = z
  .object({
    id: nonEmptyTextSchema,
    text: z.string(),
    status: nonEmptyTextSchema,
    phaseId: nonEmptyTextSchema.optional(),
    phaseTitle: nonEmptyTextSchema.optional(),
    sliceId: nonEmptyTextSchema.optional(),
    handoffNote: nonEmptyTextSchema.optional(),
  })
  .strict()

const environmentLifecycleStateSchema = z.enum([
  'created',
  'waiting_for_sandbox',
  'preparing_repository',
  'loading_project_instructions',
  'running_setup_scripts',
  'setting_up_hooks',
  'setting_up_skills_plugins',
  'indexing_workspace',
  'starting_conversation',
  'ready',
  'failed',
  'paused',
  'archived',
])

const sandboxGroupingPolicySchema = z.enum([
  'none',
  'reuse_newest',
  'reuse_least_busy',
  'reuse_by_project',
  'dedicated_per_session',
])

const environmentDiagnosticSchema = z
  .object({
    code: nonEmptyTextSchema,
    message: z.string(),
    nextAction: z.string().optional(),
  })
  .strict()

const environmentHealthCheckSchema = z
  .object({
    kind: z.enum([
      'filesystem_accessible',
      'git_state_available',
      'required_binaries_available',
      'provider_credentials_valid',
      'tool_packs_available',
      'semantic_index_status',
    ]),
    status: z.enum(['passed', 'warning', 'failed', 'skipped']),
    summary: z.string(),
    diagnostic: environmentDiagnosticSchema.optional(),
    checkedAt: nonEmptyTextSchema,
  })
  .strict()

const environmentSetupStepSchema = z
  .object({
    stepId: nonEmptyTextSchema,
    label: z.string(),
    state: z.enum(['pending', 'running', 'succeeded', 'failed', 'skipped', 'approval_required']),
    diagnostic: environmentDiagnosticSchema.optional(),
  })
  .strict()

export const runtimeProtocolEventPayloadSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('run_started'), payload: z.object({
    status: runtimeProtocolRunStatusSchema,
    providerId: z.string(),
    modelId: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('run_completed'), payload: z.object({
    summary: z.string(),
    state: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('run_failed'), payload: z.object({
    code: nonEmptyTextSchema,
    message: z.string(),
    retryable: z.boolean(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('state_transition'), payload: z.object({
    from: z.string(),
    to: z.string(),
    reason: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('message_delta'), payload: z.object({
    role: runtimeProtocolMessageRoleSchema,
    text: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('reasoning_summary'), payload: z.object({ text: z.string() }).strict() }).strict(),
  z.object({ kind: z.literal('tool_started'), payload: z.object({
    toolCallId: z.string(),
    toolName: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('tool_delta'), payload: z.object({
    toolCallId: z.string(),
    text: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('tool_completed'), payload: z.object({
    toolCallId: z.string(),
    outcome: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('policy_decision'), payload: z.object({
    subject: z.string(),
    decision: z.string(),
    reason: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('approval_required'), payload: z.object({
    actionId: z.string(),
    boundaryId: z.string().nullable().optional(),
    actionType: z.string(),
    title: z.string(),
    detail: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('plan_updated'), payload: z.object({
    summary: z.string().nullable().optional(),
    items: z.array(runtimePlanItemSchema),
  }).strict() }).strict(),
  z.object({ kind: z.literal('verification_gate'), payload: z.object({
    status: z.string(),
    summary: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('context_manifest_recorded'), payload: z.object({
    manifestId: z.string(),
    contextHash: z.string(),
    turnIndex: z.number().int().nonnegative(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('retrieval_performed'), payload: z.object({
    query: z.string(),
    resultCount: z.number().int().nonnegative(),
    source: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('memory_candidate_captured'), payload: z.object({
    candidateId: z.string(),
    candidateKind: z.string(),
    confidence: z.number().int().min(0).max(100),
  }).strict() }).strict(),
  z.object({ kind: z.literal('environment_lifecycle_update'), payload: z.object({
    environmentId: z.string(),
    state: environmentLifecycleStateSchema,
    previousState: environmentLifecycleStateSchema.nullable().optional(),
    sandboxId: z.string().nullable().optional(),
    sandboxGroupingPolicy: sandboxGroupingPolicySchema,
    pendingMessageCount: z.number().int().nonnegative(),
    healthChecks: z.array(environmentHealthCheckSchema),
    setupSteps: z.array(environmentSetupStepSchema),
    semanticIndexRequired: z.boolean().optional(),
    semanticIndexAvailable: z.boolean().optional(),
    semanticIndexState: z
      .enum(['ready', 'indexing', 'stale', 'empty', 'failed', 'unavailable'])
      .optional(),
    semanticIndexRequirementReasons: z.array(z.string()).optional(),
    detail: z.string().nullable().optional(),
    diagnostic: environmentDiagnosticSchema.nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('sandbox_lifecycle_update'), payload: z.object({
    sandboxId: z.string().nullable().optional(),
    phase: z.string(),
    detail: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('validation_started'), payload: z.object({ label: z.string() }).strict() }).strict(),
  z.object({ kind: z.literal('validation_completed'), payload: z.object({
    label: z.string(),
    outcome: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('tool_registry_snapshot'), payload: z.object({
    toolCount: z.number().int().nonnegative(),
    toolNames: z.array(z.string()),
  }).strict() }).strict(),
  z.object({ kind: z.literal('file_changed'), payload: z.object({
    path: z.string(),
    operation: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('command_output'), payload: z.object({
    toolCallId: z.string().nullable().optional(),
    stream: z.string(),
    text: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('tool_permission_grant'), payload: z.object({
    grantId: z.string(),
    toolName: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('provider_model_changed'), payload: z.object({
    providerId: z.string(),
    modelId: z.string(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('runtime_settings_changed'), payload: z.object({ summary: z.string() }).strict() }).strict(),
  z.object({ kind: z.literal('run_paused'), payload: z.object({
    reason: z.string().nullable().optional(),
  }).strict() }).strict(),
  z.object({ kind: z.literal('untyped'), payload: z.object({
    eventKind: runtimeProtocolEventKindSchema,
    payload: z.unknown(),
  }).strict() }).strict(),
])

export const runtimeProtocolEventSchema = z
  .object({
    protocolVersion: z.literal(XERO_AGENT_PROTOCOL_VERSION),
    eventId: z.number().int().positive(),
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    eventKind: runtimeProtocolEventKindSchema,
    trace: runtimeProtocolTraceContextSchema,
    payload: runtimeProtocolEventPayloadSchema,
    occurredAt: nonEmptyTextSchema,
  })
  .strict()
  .superRefine((event, ctx) => {
    if (event.payload.kind === 'untyped') {
      if (event.payload.payload.eventKind !== event.eventKind) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['payload', 'payload', 'eventKind'],
          message: 'Untyped runtime protocol event payload kind must match eventKind.',
        })
      }
      return
    }
    if (event.payload.kind !== event.eventKind) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'kind'],
        message: 'Runtime protocol event payload kind must match eventKind.',
      })
    }
  })

export type RuntimeProtocolTraceContextDto = z.infer<typeof runtimeProtocolTraceContextSchema>
export type RuntimeSubmissionEnvelopeDto = z.infer<typeof runtimeSubmissionEnvelopeSchema>
export type RuntimeProtocolEventDto = z.infer<typeof runtimeProtocolEventSchema>
