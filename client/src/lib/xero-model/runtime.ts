import { z } from 'zod'
import {
  humanizeSegmentedLabel as humanizeRuntimeKind,
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
} from './shared'

export const DEFAULT_AGENT_SESSION_ID = 'agent-session-main'

export const runtimeAuthPhaseSchema = z.enum([
  'idle',
  'starting',
  'awaiting_browser_callback',
  'awaiting_manual_input',
  'exchanging_code',
  'authenticated',
  'refreshing',
  'cancelled',
  'failed',
])

export const runtimeDiagnosticSchema = z.object({
  code: z.string().trim().min(1),
  message: z.string().trim().min(1),
  retryable: z.boolean(),
})

export const runtimeProviderIdSchema = z.enum([
  'openrouter',
  'openai_codex',
  'anthropic',
  'github_models',
  'openai_api',
  'ollama',
  'azure_openai',
  'gemini_ai_studio',
  'bedrock',
  'vertex',
])
export const writableRuntimeSettingsProviderIdSchema = z.enum(['openrouter', 'openai_codex', 'anthropic'])

export const runtimeRunThinkingEffortSchema = z.enum(['minimal', 'low', 'medium', 'high', 'x_high'])
export const runtimeRunApprovalModeSchema = z.enum(['suggest', 'auto_edit', 'yolo'])
export const DEFAULT_RUNTIME_RUN_APPROVAL_MODE: RuntimeRunApprovalModeDto = 'suggest'
export const BUILTIN_RUNTIME_AGENT_IDS = ['ask', 'engineer', 'debug', 'agent_create'] as const
export const runtimeAgentIdSchema = z.enum(BUILTIN_RUNTIME_AGENT_IDS)
export const DEFAULT_RUNTIME_AGENT_ID: RuntimeAgentIdDto = 'ask'

export interface RuntimeAgentDescriptor {
  id: RuntimeAgentIdDto
  version: number
  label: string
  shortLabel: string
  description: string
  taskPurpose: string
  scope: 'built_in' | 'global_custom' | 'project_custom'
  lifecycleState: 'draft' | 'active' | 'archived'
  baseCapabilityProfile: 'observe_only' | 'engineering' | 'debugging' | 'agent_builder'
  defaultApprovalMode: RuntimeRunApprovalModeDto
  allowedApprovalModes: readonly RuntimeRunApprovalModeDto[]
  promptPolicy: 'ask' | 'engineer' | 'debug' | 'agent_create'
  toolPolicy: 'observe_only' | 'engineering' | 'agent_builder'
  outputContract: 'answer' | 'engineering_summary' | 'debug_summary' | 'agent_definition_draft'
  projectDataPolicy: {
    required: true
    recordKinds: readonly (
      | 'agent_handoff'
      | 'project_fact'
      | 'decision'
      | 'constraint'
      | 'plan'
      | 'finding'
      | 'verification'
      | 'question'
      | 'artifact'
      | 'context_note'
      | 'diagnostic'
    )[]
    structuredSchemas: readonly string[]
    unstructuredScopes: readonly ('answer_note' | 'session_summary' | 'artifact_excerpt' | 'troubleshooting_note')[]
    memoryCandidateKinds: readonly ('project_fact' | 'user_preference' | 'decision' | 'session_summary' | 'troubleshooting')[]
  }
  workflowRole: 'interactive' | 'workflow_step'
  allowPlanGate: boolean
  allowVerificationGate: boolean
  allowAutoCompact: boolean
}

export const RUNTIME_AGENT_DESCRIPTORS = [
  {
    id: 'ask',
    version: 1,
    label: 'Ask',
    shortLabel: 'Ask',
    description: 'Answer questions about the project without mutating files, app state, processes, or external services.',
    taskPurpose: 'Answer in chat using audited observe-only tools when grounding is needed.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'observe_only',
    defaultApprovalMode: 'suggest',
    allowedApprovalModes: ['suggest'],
    promptPolicy: 'ask',
    toolPolicy: 'observe_only',
    outputContract: 'answer',
    projectDataPolicy: {
      required: true,
      recordKinds: ['agent_handoff', 'project_fact', 'constraint', 'question', 'context_note', 'diagnostic'],
      structuredSchemas: ['xero.project_record.v1'],
      unstructuredScopes: ['answer_note', 'session_summary', 'troubleshooting_note'],
      memoryCandidateKinds: ['project_fact', 'user_preference', 'decision', 'session_summary', 'troubleshooting'],
    },
    workflowRole: 'interactive',
    allowPlanGate: false,
    allowVerificationGate: false,
    allowAutoCompact: true,
  },
  {
    id: 'engineer',
    version: 1,
    label: 'Engineer',
    shortLabel: 'Build',
    description: 'Implement repository changes with the existing software-building toolset and safety gates.',
    taskPurpose: 'Inspect, plan when needed, edit, verify, and summarize engineering work.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    defaultApprovalMode: 'suggest',
    allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
    promptPolicy: 'engineer',
    toolPolicy: 'engineering',
    outputContract: 'engineering_summary',
    projectDataPolicy: {
      required: true,
      recordKinds: [
        'agent_handoff',
        'project_fact',
        'decision',
        'constraint',
        'plan',
        'finding',
        'verification',
        'question',
        'artifact',
        'context_note',
        'diagnostic',
      ],
      structuredSchemas: ['xero.project_record.v1'],
      unstructuredScopes: ['answer_note', 'session_summary', 'artifact_excerpt', 'troubleshooting_note'],
      memoryCandidateKinds: ['project_fact', 'user_preference', 'decision', 'session_summary', 'troubleshooting'],
    },
    workflowRole: 'interactive',
    allowPlanGate: true,
    allowVerificationGate: true,
    allowAutoCompact: true,
  },
  {
    id: 'debug',
    version: 1,
    label: 'Debug',
    shortLabel: 'Debug',
    description:
      'Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.',
    taskPurpose:
      'Reproduce, gather evidence, test hypotheses, isolate root cause, fix, verify, and preserve reusable debugging knowledge.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'debugging',
    defaultApprovalMode: 'suggest',
    allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
    promptPolicy: 'debug',
    toolPolicy: 'engineering',
    outputContract: 'debug_summary',
    projectDataPolicy: {
      required: true,
      recordKinds: [
        'agent_handoff',
        'project_fact',
        'decision',
        'constraint',
        'plan',
        'finding',
        'verification',
        'question',
        'artifact',
        'context_note',
        'diagnostic',
      ],
      structuredSchemas: ['xero.project_record.v1', 'xero.project_record.debug_session.v1'],
      unstructuredScopes: ['answer_note', 'session_summary', 'artifact_excerpt', 'troubleshooting_note'],
      memoryCandidateKinds: ['project_fact', 'user_preference', 'decision', 'session_summary', 'troubleshooting'],
    },
    workflowRole: 'interactive',
    allowPlanGate: true,
    allowVerificationGate: true,
    allowAutoCompact: true,
  },
  {
    id: 'agent_create',
    version: 1,
    label: 'Agent Create',
    shortLabel: 'Create',
    description:
      'Interview the user, validate custom agent definitions, and save approved definitions without mutating repositories.',
    taskPurpose:
      'Gather intent, clarify scope, propose least-privilege capabilities, validate definitions, and persist approved custom agents.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'agent_builder',
    defaultApprovalMode: 'suggest',
    allowedApprovalModes: ['suggest'],
    promptPolicy: 'agent_create',
    toolPolicy: 'agent_builder',
    outputContract: 'agent_definition_draft',
    projectDataPolicy: {
      required: true,
      recordKinds: [
        'agent_handoff',
        'project_fact',
        'decision',
        'constraint',
        'plan',
        'question',
        'context_note',
        'diagnostic',
      ],
      structuredSchemas: ['xero.project_record.v1'],
      unstructuredScopes: ['answer_note', 'session_summary', 'troubleshooting_note'],
      memoryCandidateKinds: ['project_fact', 'user_preference', 'decision', 'session_summary', 'troubleshooting'],
    },
    workflowRole: 'interactive',
    allowPlanGate: false,
    allowVerificationGate: false,
    allowAutoCompact: true,
  },
] as const satisfies readonly RuntimeAgentDescriptor[]

export function getRuntimeAgentDescriptor(agentId: RuntimeAgentIdDto): RuntimeAgentDescriptor {
  return RUNTIME_AGENT_DESCRIPTORS.find((descriptor) => descriptor.id === agentId) ?? RUNTIME_AGENT_DESCRIPTORS[0]
}

export function getRuntimeAgentLabel(agentId: RuntimeAgentIdDto): string {
  return getRuntimeAgentDescriptor(agentId).label
}

function validateRuntimeSettingsProviderModel(
  payload: { providerId: z.infer<typeof runtimeProviderIdSchema>; modelId: string },
  ctx: z.RefinementCtx,
): void {
  if (payload.providerId === 'openai_codex' && payload.modelId.trim().length === 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['modelId'],
      message: 'Xero requires a modelId for provider `openai_codex`.',
    })
  }
}

function validateWritableRuntimeSettingsProvider(
  providerId: z.infer<typeof runtimeProviderIdSchema>,
  ctx: z.RefinementCtx,
): void {
  if (!(writableRuntimeSettingsProviderIdSchema.options as readonly RuntimeProviderIdDto[]).includes(providerId)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ['providerId'],
      message:
        'Xero only accepts runtime-settings compatibility writes for `openai_codex`, `openrouter`, or `anthropic`. Use provider profiles for other cloud providers.',
    })
  }
}

export const runtimeSettingsSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    modelId: z.string().trim().min(1),
    openrouterApiKeyConfigured: z.boolean(),
    anthropicApiKeyConfigured: z.boolean().default(false),
  })
  .strict()
  .superRefine((payload, ctx) => {
    validateRuntimeSettingsProviderModel(payload, ctx)
  })

export const upsertRuntimeSettingsRequestSchema = z
  .object({
    providerId: runtimeProviderIdSchema,
    modelId: z.string().trim().min(1),
    openrouterApiKey: z.string().nullable().optional(),
    anthropicApiKey: z.string().nullable().optional(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    validateWritableRuntimeSettingsProvider(payload.providerId, ctx)
    validateRuntimeSettingsProviderModel(payload, ctx)
  })

export const runtimeSessionSchema = z.object({
  projectId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  providerId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  accountId: nonEmptyOptionalTextSchema,
  phase: runtimeAuthPhaseSchema,
  callbackBound: z.boolean().nullable().optional(),
  authorizationUrl: z.string().url().nullable().optional(),
  redirectUri: z.string().url().nullable().optional(),
  lastErrorCode: nonEmptyOptionalTextSchema,
  lastError: runtimeDiagnosticSchema.nullable().optional(),
  updatedAt: isoTimestampSchema,
})

export const providerAuthSessionSchema = z.object({
  runtimeKind: z.string().trim().min(1),
  providerId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  accountId: nonEmptyOptionalTextSchema,
  phase: runtimeAuthPhaseSchema,
  callbackBound: z.boolean().nullable().optional(),
  authorizationUrl: z.string().url().nullable().optional(),
  redirectUri: z.string().url().nullable().optional(),
  lastErrorCode: nonEmptyOptionalTextSchema,
  lastError: runtimeDiagnosticSchema.nullable().optional(),
  updatedAt: isoTimestampSchema,
})

export const runtimeUpdatedPayloadSchema = z.object({
  projectId: z.string().trim().min(1),
  runtimeKind: z.string().trim().min(1),
  providerId: z.string().trim().min(1),
  flowId: nonEmptyOptionalTextSchema,
  sessionId: nonEmptyOptionalTextSchema,
  accountId: nonEmptyOptionalTextSchema,
  authPhase: runtimeAuthPhaseSchema,
  lastErrorCode: nonEmptyOptionalTextSchema,
  lastError: runtimeDiagnosticSchema.nullable().optional(),
  updatedAt: isoTimestampSchema,
})

export const runtimeRunStatusSchema = z.enum(['starting', 'running', 'stale', 'stopped', 'failed'])
export const runtimeRunTransportLivenessSchema = z.enum(['unknown', 'reachable', 'unreachable'])
export const runtimeRunCheckpointKindSchema = z.enum(['bootstrap', 'state', 'tool', 'action_required', 'diagnostic'])

export const runtimeRunDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const runtimeRunTransportSchema = z
  .object({
    kind: z.string().trim().min(1),
    endpoint: z.string().trim().min(1),
    liveness: runtimeRunTransportLivenessSchema,
  })
  .strict()

export const runtimeRunCheckpointSchema = z
  .object({
    sequence: z.number().int().nonnegative(),
    kind: runtimeRunCheckpointKindSchema,
    summary: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const runtimeRunControlInputSchema = z
  .object({
    runtimeAgentId: runtimeAgentIdSchema,
    agentDefinitionId: z.string().trim().min(1).nullable().optional(),
    providerProfileId: nonEmptyOptionalTextSchema,
    modelId: z.string().trim().min(1),
    thinkingEffort: runtimeRunThinkingEffortSchema.nullable().optional(),
    approvalMode: runtimeRunApprovalModeSchema,
    planModeRequired: z.boolean().default(false),
  })
  .strict()

export const runtimeAutoCompactPreferenceSchema = z
  .object({
    enabled: z.boolean(),
    thresholdPercent: z.number().int().min(1).max(100).nullable().optional(),
    rawTailMessageCount: z.number().int().min(2).max(24).nullable().optional(),
  })
  .strict()

export const runtimeRunActiveControlSnapshotSchema = z
  .object({
    runtimeAgentId: runtimeAgentIdSchema,
    agentDefinitionId: z.string().trim().min(1).nullable().optional(),
    agentDefinitionVersion: z.number().int().positive().nullable().optional(),
    providerProfileId: nonEmptyOptionalTextSchema,
    modelId: z.string().trim().min(1),
    thinkingEffort: runtimeRunThinkingEffortSchema.nullable().optional(),
    approvalMode: runtimeRunApprovalModeSchema,
    planModeRequired: z.boolean().default(false),
    revision: z.number().int().positive(),
    appliedAt: isoTimestampSchema,
  })
  .strict()

export const runtimeRunPendingControlSnapshotSchema = z
  .object({
    runtimeAgentId: runtimeAgentIdSchema,
    agentDefinitionId: z.string().trim().min(1).nullable().optional(),
    agentDefinitionVersion: z.number().int().positive().nullable().optional(),
    providerProfileId: nonEmptyOptionalTextSchema,
    modelId: z.string().trim().min(1),
    thinkingEffort: runtimeRunThinkingEffortSchema.nullable().optional(),
    approvalMode: runtimeRunApprovalModeSchema,
    planModeRequired: z.boolean().default(false),
    revision: z.number().int().positive(),
    queuedAt: isoTimestampSchema,
    queuedPrompt: z.string().trim().min(1).nullable().optional(),
    queuedPromptAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
  .superRefine((snapshot, ctx) => {
    const hasQueuedPrompt = typeof snapshot.queuedPrompt === 'string' && snapshot.queuedPrompt.trim().length > 0
    const hasQueuedPromptAt = typeof snapshot.queuedPromptAt === 'string' && snapshot.queuedPromptAt.trim().length > 0

    if (hasQueuedPrompt !== hasQueuedPromptAt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['queuedPrompt'],
        message: 'Xero requires queuedPrompt and queuedPromptAt to be populated together.',
      })
    }
  })

export const runtimeRunControlStateSchema = z
  .object({
    active: runtimeRunActiveControlSnapshotSchema,
    pending: runtimeRunPendingControlSnapshotSchema.nullable().optional(),
  })
  .strict()
  .superRefine((state, ctx) => {
    const pendingRevision = state.pending?.revision ?? null
    if (pendingRevision !== null && pendingRevision <= state.active.revision) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['pending', 'revision'],
        message: 'Xero requires pending runtime-run revisions to be newer than the active revision.',
      })
    }
  })

export const agentSessionStatusSchema = z.enum(['active', 'archived'])

export const agentSessionLineageBoundaryKindSchema = z.enum(['run', 'message', 'checkpoint'])

export const agentSessionLineageDiagnosticSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const agentSessionLineageSchema = z
  .object({
    lineageId: z.string().trim().min(1),
    projectId: z.string().trim().min(1),
    childAgentSessionId: z.string().trim().min(1),
    sourceAgentSessionId: nonEmptyOptionalTextSchema,
    sourceRunId: nonEmptyOptionalTextSchema,
    sourceBoundaryKind: agentSessionLineageBoundaryKindSchema,
    sourceMessageId: z.number().int().positive().nullable().optional(),
    sourceCheckpointId: z.number().int().positive().nullable().optional(),
    sourceCompactionId: nonEmptyOptionalTextSchema,
    sourceTitle: z.string().trim().min(1),
    branchTitle: z.string().trim().min(1),
    replayRunId: z.string().trim().min(1),
    fileChangeSummary: z.string(),
    diagnostic: agentSessionLineageDiagnosticSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    sourceDeletedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()

export const agentSessionSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    title: z.string().trim().min(1),
    summary: z.string(),
    status: agentSessionStatusSchema,
    selected: z.boolean(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    archivedAt: nonEmptyOptionalTextSchema,
    lastRunId: nonEmptyOptionalTextSchema,
    lastRuntimeKind: nonEmptyOptionalTextSchema,
    lastProviderId: nonEmptyOptionalTextSchema,
    lineage: agentSessionLineageSchema.nullable().optional(),
  })
  .strict()

export const createAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    title: z.string().trim().min(1).nullable().optional(),
    summary: z.string().optional(),
    selected: z.boolean().optional(),
  })
  .strict()

export const listAgentSessionsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    includeArchived: z.boolean().optional(),
  })
  .strict()

export const getAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
  })
  .strict()

export const updateAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    title: z.string().trim().min(1).nullable().optional(),
    summary: z.string().nullable().optional(),
    selected: z.boolean().nullable().optional(),
  })
  .strict()

export const autoNameAgentSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    prompt: z.string().trim().min(1),
    controls: runtimeRunControlInputSchema.nullable().optional(),
  })
  .strict()

export const archiveAgentSessionRequestSchema = getAgentSessionRequestSchema

export const restoreAgentSessionRequestSchema = getAgentSessionRequestSchema

export const deleteAgentSessionRequestSchema = getAgentSessionRequestSchema

export const listAgentSessionsResponseSchema = z
  .object({
    sessions: z.array(agentSessionSchema),
  })
  .strict()

export const runtimeRunSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    supervisorKind: z.string().trim().min(1),
    status: runtimeRunStatusSchema,
    transport: runtimeRunTransportSchema,
    controls: runtimeRunControlStateSchema,
    startedAt: isoTimestampSchema,
    lastHeartbeatAt: nonEmptyOptionalTextSchema,
    lastCheckpointSequence: z.number().int().nonnegative(),
    lastCheckpointAt: nonEmptyOptionalTextSchema,
    stoppedAt: nonEmptyOptionalTextSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
    checkpoints: z.array(runtimeRunCheckpointSchema),
  })
  .strict()

export const runtimeRunUpdatedPayloadSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    run: runtimeRunSchema.nullable(),
  })
  .strict()
  .superRefine((payload, ctx) => {
    if (payload.run && payload.run.projectId !== payload.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['run', 'projectId'],
        message: 'Xero received a runtime-run update for a different project than the event envelope.',
      })
    }

    if (payload.run && payload.run.agentSessionId !== payload.agentSessionId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['run', 'agentSessionId'],
        message: 'Xero received a runtime-run update for a different agent session than the event envelope.',
      })
    }
  })

export const agentAttachmentKindSchema = z.enum(['image', 'document', 'text'])

export const stagedAgentAttachmentSchema = z
  .object({
    kind: agentAttachmentKindSchema,
    absolutePath: z.string().min(1),
    mediaType: z.string().min(1),
    originalName: z.string().min(1),
    sizeBytes: z.number().int().nonnegative(),
    width: z.number().int().nonnegative().nullable().optional(),
    height: z.number().int().nonnegative().nullable().optional(),
  })
  .strict()

export const startRuntimeRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    initialControls: runtimeRunControlInputSchema.nullable().optional(),
    initialPrompt: z.string().trim().min(1).nullable().optional(),
    initialAttachments: z.array(stagedAgentAttachmentSchema).default([]),
  })
  .strict()

export const startRuntimeSessionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    providerProfileId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const getRuntimeRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
  })
  .strict()

export const updateRuntimeRunControlsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    controls: runtimeRunControlInputSchema.nullable().optional(),
    prompt: z.string().trim().min(1).nullable().optional(),
    attachments: z.array(stagedAgentAttachmentSchema).default([]),
    autoCompact: runtimeAutoCompactPreferenceSchema.nullable().optional(),
  })
  .strict()
  .superRefine((request, ctx) => {
    if (!request.controls && !request.prompt) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['controls'],
        message: 'Xero requires a prompt or control delta before it can queue runtime-run changes.',
      })
    }
  })

export const stopRuntimeRunRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    agentSessionId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
  })
  .strict()

export type RuntimeAuthPhaseDto = z.infer<typeof runtimeAuthPhaseSchema>
export type RuntimeDiagnosticDto = z.infer<typeof runtimeDiagnosticSchema>
export type RuntimeProviderIdDto = z.infer<typeof runtimeProviderIdSchema>
export type RuntimeSettingsDto = z.infer<typeof runtimeSettingsSchema>
export type UpsertRuntimeSettingsRequestDto = z.infer<typeof upsertRuntimeSettingsRequestSchema>
export type RuntimeSessionDto = z.infer<typeof runtimeSessionSchema>
export type ProviderAuthSessionDto = z.infer<typeof providerAuthSessionSchema>
export type RuntimeUpdatedPayloadDto = z.infer<typeof runtimeUpdatedPayloadSchema>
export type RuntimeRunStatusDto = z.infer<typeof runtimeRunStatusSchema>
export type RuntimeRunTransportLivenessDto = z.infer<typeof runtimeRunTransportLivenessSchema>
export type RuntimeRunCheckpointKindDto = z.infer<typeof runtimeRunCheckpointKindSchema>
export type RuntimeRunDiagnosticDto = z.infer<typeof runtimeRunDiagnosticSchema>
export type RuntimeRunTransportDto = z.infer<typeof runtimeRunTransportSchema>
export type RuntimeRunCheckpointDto = z.infer<typeof runtimeRunCheckpointSchema>
export type RuntimeRunThinkingEffortDto = z.infer<typeof runtimeRunThinkingEffortSchema>
export type RuntimeRunApprovalModeDto = z.infer<typeof runtimeRunApprovalModeSchema>
export type RuntimeAgentIdDto = z.infer<typeof runtimeAgentIdSchema>
export type RuntimeRunControlInputDto = z.infer<typeof runtimeRunControlInputSchema>
export type RuntimeAutoCompactPreferenceDto = z.infer<typeof runtimeAutoCompactPreferenceSchema>
export type RuntimeRunActiveControlSnapshotDto = z.infer<typeof runtimeRunActiveControlSnapshotSchema>
export type RuntimeRunPendingControlSnapshotDto = z.infer<typeof runtimeRunPendingControlSnapshotSchema>
export type RuntimeRunControlStateDto = z.infer<typeof runtimeRunControlStateSchema>
export type AgentSessionStatusDto = z.infer<typeof agentSessionStatusSchema>
export type AgentSessionLineageBoundaryKindDto = z.infer<typeof agentSessionLineageBoundaryKindSchema>
export type AgentSessionLineageDiagnosticDto = z.infer<typeof agentSessionLineageDiagnosticSchema>
export type AgentSessionLineageDto = z.infer<typeof agentSessionLineageSchema>
export type AgentSessionDto = z.infer<typeof agentSessionSchema>
export type CreateAgentSessionRequestDto = z.infer<typeof createAgentSessionRequestSchema>
export type ListAgentSessionsRequestDto = z.infer<typeof listAgentSessionsRequestSchema>
export type GetAgentSessionRequestDto = z.infer<typeof getAgentSessionRequestSchema>
export type UpdateAgentSessionRequestDto = z.infer<typeof updateAgentSessionRequestSchema>
export type AutoNameAgentSessionRequestDto = z.infer<typeof autoNameAgentSessionRequestSchema>
export type ArchiveAgentSessionRequestDto = z.infer<typeof archiveAgentSessionRequestSchema>
export type RestoreAgentSessionRequestDto = z.infer<typeof restoreAgentSessionRequestSchema>
export type DeleteAgentSessionRequestDto = z.infer<typeof deleteAgentSessionRequestSchema>
export type ListAgentSessionsResponseDto = z.infer<typeof listAgentSessionsResponseSchema>
export type RuntimeRunDto = z.infer<typeof runtimeRunSchema>
export type RuntimeRunUpdatedPayloadDto = z.infer<typeof runtimeRunUpdatedPayloadSchema>
export type GetRuntimeRunRequestDto = z.infer<typeof getRuntimeRunRequestSchema>
export type StartRuntimeRunRequestDto = z.infer<typeof startRuntimeRunRequestSchema>
export type StartRuntimeSessionRequestDto = z.infer<typeof startRuntimeSessionRequestSchema>
export type UpdateRuntimeRunControlsRequestDto = z.infer<typeof updateRuntimeRunControlsRequestSchema>
export type StopRuntimeRunRequestDto = z.infer<typeof stopRuntimeRunRequestSchema>
export type AgentAttachmentKindDto = z.infer<typeof agentAttachmentKindSchema>
export type StagedAgentAttachmentDto = z.infer<typeof stagedAgentAttachmentSchema>

export interface RuntimeSessionView {
  projectId: string
  runtimeKind: string
  providerId: string
  flowId: string | null
  sessionId: string | null
  accountId: string | null
  phase: RuntimeAuthPhaseDto
  phaseLabel: string
  runtimeLabel: string
  accountLabel: string
  sessionLabel: string
  callbackBound: boolean | null
  authorizationUrl: string | null
  redirectUri: string | null
  lastErrorCode: string | null
  lastError: RuntimeDiagnosticDto | null
  updatedAt: string
  isAuthenticated: boolean
  isLoginInProgress: boolean
  needsManualInput: boolean
  isSignedOut: boolean
  isFailed: boolean
}

export interface ProviderAuthSessionView {
  runtimeKind: string
  providerId: string
  flowId: string | null
  sessionId: string | null
  accountId: string | null
  phase: RuntimeAuthPhaseDto
  phaseLabel: string
  runtimeLabel: string
  accountLabel: string
  sessionLabel: string
  callbackBound: boolean | null
  authorizationUrl: string | null
  redirectUri: string | null
  lastErrorCode: string | null
  lastError: RuntimeDiagnosticDto | null
  updatedAt: string
  isAuthenticated: boolean
  isLoginInProgress: boolean
  needsManualInput: boolean
  isSignedOut: boolean
  isFailed: boolean
}

export interface RuntimeRunTransportView {
  kind: string
  endpoint: string
  liveness: RuntimeRunTransportLivenessDto
  livenessLabel: string
}

export interface RuntimeRunCheckpointView {
  sequence: number
  kind: RuntimeRunCheckpointKindDto
  kindLabel: string
  summary: string
  createdAt: string
}

export interface RuntimeRunControlInputView {
  runtimeAgentId: RuntimeAgentIdDto
  agentDefinitionId: string | null
  agentDefinitionVersion: number | null
  runtimeAgentLabel: string
  providerProfileId: string | null
  modelId: string
  thinkingEffort: RuntimeRunThinkingEffortDto | null
  thinkingEffortLabel: string
  approvalMode: RuntimeRunApprovalModeDto
  approvalModeLabel: string
  planModeRequired: boolean
}

export interface RuntimeRunActiveControlSnapshotView extends RuntimeRunControlInputView {
  revision: number
  appliedAt: string
}

export interface RuntimeRunPendingControlSnapshotView extends RuntimeRunControlInputView {
  revision: number
  queuedAt: string
  queuedPrompt: string | null
  queuedPromptAt: string | null
  hasQueuedPrompt: boolean
}

export interface RuntimeRunControlSelectionView extends RuntimeRunControlInputView {
  source: 'active' | 'pending'
  revision: number
  effectiveAt: string
  queuedPrompt: string | null
  queuedPromptAt: string | null
  hasQueuedPrompt: boolean
}

export interface RuntimeRunControlStateView {
  active: RuntimeRunActiveControlSnapshotView
  pending: RuntimeRunPendingControlSnapshotView | null
  selected: RuntimeRunControlSelectionView
  hasPendingControls: boolean
}

export interface RuntimeRunView {
  projectId: string
  agentSessionId: string
  runId: string
  runtimeKind: string
  providerId: string
  runtimeLabel: string
  supervisorKind: string
  supervisorLabel: string
  status: RuntimeRunStatusDto
  statusLabel: string
  transport: RuntimeRunTransportView
  controls: RuntimeRunControlStateView
  startedAt: string
  lastHeartbeatAt: string | null
  lastCheckpointSequence: number
  lastCheckpointAt: string | null
  stoppedAt: string | null
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  updatedAt: string
  checkpoints: RuntimeRunCheckpointView[]
  latestCheckpoint: RuntimeRunCheckpointView | null
  checkpointCount: number
  hasCheckpoints: boolean
  isActive: boolean
  isTerminal: boolean
  isStale: boolean
  isFailed: boolean
}

export interface AgentSessionView {
  projectId: string
  agentSessionId: string
  title: string
  summary: string
  status: AgentSessionStatusDto
  statusLabel: string
  selected: boolean
  createdAt: string
  updatedAt: string
  archivedAt: string | null
  lastRunId: string | null
  lastRuntimeKind: string | null
  lastProviderId: string | null
  lineage: AgentSessionLineageDto | null
  isActive: boolean
  isArchived: boolean
}

function timestampToSortValue(value: string | null): number {
  if (!value) {
    return Number.NEGATIVE_INFINITY
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : Number.NEGATIVE_INFINITY
}

function getRuntimePhaseLabel(phase: RuntimeAuthPhaseDto): string {
  switch (phase) {
    case 'idle':
      return 'Signed out'
    case 'starting':
      return 'Starting login'
    case 'awaiting_browser_callback':
      return 'Awaiting browser'
    case 'awaiting_manual_input':
      return 'Awaiting manual input'
    case 'exchanging_code':
      return 'Signing in'
    case 'authenticated':
      return 'Authenticated'
    case 'refreshing':
      return 'Refreshing session'
    case 'cancelled':
      return 'Login cancelled'
    case 'failed':
      return 'Login failed'
  }
}

function getRuntimeLabel(runtimeKind: string, phase: RuntimeAuthPhaseDto): string {
  if (phase === 'idle' || phase === 'failed' || phase === 'cancelled') {
    return 'Runtime unavailable'
  }

  return `${humanizeRuntimeKind(runtimeKind)} · ${getRuntimePhaseLabel(phase)}`
}

export function getRuntimeRunStatusLabel(status: RuntimeRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Agent starting'
    case 'running':
      return 'Agent running'
    case 'stale':
      return 'Agent stale'
    case 'stopped':
      return 'Run stopped'
    case 'failed':
      return 'Run failed'
  }
}

export function getRuntimeRunTransportLivenessLabel(liveness: RuntimeRunTransportLivenessDto): string {
  switch (liveness) {
    case 'unknown':
      return 'Runtime liveness unknown'
    case 'reachable':
      return 'Runtime reachable'
    case 'unreachable':
      return 'Runtime unreachable'
  }
}

export function getRuntimeRunCheckpointKindLabel(kind: RuntimeRunCheckpointKindDto): string {
  switch (kind) {
    case 'bootstrap':
      return 'Bootstrap'
    case 'state':
      return 'State'
    case 'tool':
      return 'Tool'
    case 'action_required':
      return 'Action required'
    case 'diagnostic':
      return 'Diagnostic'
  }
}

export function getRuntimeRunApprovalModeLabel(mode: RuntimeRunApprovalModeDto): string {
  switch (mode) {
    case 'suggest':
      return 'Suggest'
    case 'auto_edit':
      return 'Auto edit'
    case 'yolo':
      return 'YOLO'
  }
}

export function getRuntimeRunThinkingEffortLabel(effort: RuntimeRunThinkingEffortDto | null | undefined): string {
  switch (effort) {
    case 'minimal':
      return 'Minimal'
    case 'low':
      return 'Low'
    case 'medium':
      return 'Medium'
    case 'high':
      return 'High'
    case 'x_high':
      return 'Very high'
    default:
      return 'Thinking unavailable'
  }
}

function getRuntimeRunLabel(runtimeKind: string, status: RuntimeRunStatusDto): string {
  return `${humanizeRuntimeKind(runtimeKind)} · ${getRuntimeRunStatusLabel(status)}`
}

function getAgentSessionStatusLabel(status: AgentSessionStatusDto): string {
  switch (status) {
    case 'active':
      return 'Active'
    case 'archived':
      return 'Archived'
  }
}

function mapRuntimeRunControlInput(control: RuntimeRunControlInputDto): RuntimeRunControlInputView {
  const runtimeAgentId = control.runtimeAgentId ?? DEFAULT_RUNTIME_AGENT_ID
  const controlSnapshot = control as RuntimeRunControlInputDto & {
    agentDefinitionVersion?: number | null
  }
  return {
    runtimeAgentId,
    agentDefinitionId: normalizeOptionalText(control.agentDefinitionId),
    agentDefinitionVersion: controlSnapshot.agentDefinitionVersion ?? null,
    runtimeAgentLabel: getRuntimeAgentLabel(runtimeAgentId),
    providerProfileId: normalizeOptionalText(control.providerProfileId),
    modelId: normalizeText(control.modelId, 'model-unavailable'),
    thinkingEffort: control.thinkingEffort ?? null,
    thinkingEffortLabel: getRuntimeRunThinkingEffortLabel(control.thinkingEffort ?? null),
    approvalMode: control.approvalMode,
    approvalModeLabel: getRuntimeRunApprovalModeLabel(control.approvalMode),
    planModeRequired: control.planModeRequired ?? false,
  }
}

function mapRuntimeRunActiveControlSnapshot(
  snapshot: RuntimeRunActiveControlSnapshotDto,
): RuntimeRunActiveControlSnapshotView {
  return {
    ...mapRuntimeRunControlInput(snapshot),
    revision: snapshot.revision,
    appliedAt: snapshot.appliedAt,
  }
}

function mapRuntimeRunPendingControlSnapshot(
  snapshot: RuntimeRunPendingControlSnapshotDto,
): RuntimeRunPendingControlSnapshotView {
  const queuedPrompt = normalizeOptionalText(snapshot.queuedPrompt)
  return {
    ...mapRuntimeRunControlInput(snapshot),
    revision: snapshot.revision,
    queuedAt: snapshot.queuedAt,
    queuedPrompt,
    queuedPromptAt: normalizeOptionalText(snapshot.queuedPromptAt),
    hasQueuedPrompt: queuedPrompt !== null,
  }
}

function mapRuntimeRunControlState(controls: RuntimeRunControlStateDto): RuntimeRunControlStateView {
  const active = mapRuntimeRunActiveControlSnapshot(controls.active)
  const pending = controls.pending ? mapRuntimeRunPendingControlSnapshot(controls.pending) : null

  return {
    active,
    pending,
    selected: pending
      ? {
          ...mapRuntimeRunControlInput(pending),
          source: 'pending',
          revision: pending.revision,
          effectiveAt: pending.queuedAt,
          queuedPrompt: pending.queuedPrompt,
          queuedPromptAt: pending.queuedPromptAt,
          hasQueuedPrompt: pending.hasQueuedPrompt,
        }
      : {
          ...mapRuntimeRunControlInput(active),
          source: 'active',
          revision: active.revision,
          effectiveAt: active.appliedAt,
          queuedPrompt: null,
          queuedPromptAt: null,
          hasQueuedPrompt: false,
        },
    hasPendingControls: pending !== null,
  }
}

export function mapRuntimeSession(runtime: RuntimeSessionDto): RuntimeSessionView {
  const runtimeKind = normalizeText(runtime.runtimeKind, 'openai_codex')
  const providerId = normalizeText(runtime.providerId, 'provider-unavailable')
  const accountId = normalizeOptionalText(runtime.accountId)
  const sessionId = normalizeOptionalText(runtime.sessionId)

  return {
    projectId: runtime.projectId,
    runtimeKind,
    providerId,
    flowId: normalizeOptionalText(runtime.flowId),
    sessionId,
    accountId,
    phase: runtime.phase,
    phaseLabel: getRuntimePhaseLabel(runtime.phase),
    runtimeLabel: getRuntimeLabel(runtimeKind, runtime.phase),
    accountLabel: accountId ?? 'Not signed in',
    sessionLabel: sessionId ?? 'No session',
    callbackBound: runtime.callbackBound ?? null,
    authorizationUrl: normalizeOptionalText(runtime.authorizationUrl),
    redirectUri: normalizeOptionalText(runtime.redirectUri),
    lastErrorCode: normalizeOptionalText(runtime.lastErrorCode),
    lastError: runtime.lastError ?? null,
    updatedAt: runtime.updatedAt,
    isAuthenticated: runtime.phase === 'authenticated',
    isLoginInProgress: [
      'starting',
      'awaiting_browser_callback',
      'awaiting_manual_input',
      'exchanging_code',
      'refreshing',
    ].includes(runtime.phase),
    needsManualInput: runtime.phase === 'awaiting_manual_input',
    isSignedOut: runtime.phase === 'idle',
    isFailed: runtime.phase === 'failed' || runtime.phase === 'cancelled',
  }
}

export function mapProviderAuthSession(session: ProviderAuthSessionDto): ProviderAuthSessionView {
  const runtimeKind = normalizeText(session.runtimeKind, 'openai_codex')
  const providerId = normalizeText(session.providerId, 'provider-unavailable')
  const accountId = normalizeOptionalText(session.accountId)
  const sessionId = normalizeOptionalText(session.sessionId)

  return {
    runtimeKind,
    providerId,
    flowId: normalizeOptionalText(session.flowId),
    sessionId,
    accountId,
    phase: session.phase,
    phaseLabel: getRuntimePhaseLabel(session.phase),
    runtimeLabel: getRuntimeLabel(runtimeKind, session.phase),
    accountLabel: accountId ?? 'Not signed in',
    sessionLabel: sessionId ?? 'No session',
    callbackBound: session.callbackBound ?? null,
    authorizationUrl: normalizeOptionalText(session.authorizationUrl),
    redirectUri: normalizeOptionalText(session.redirectUri),
    lastErrorCode: normalizeOptionalText(session.lastErrorCode),
    lastError: session.lastError ?? null,
    updatedAt: session.updatedAt,
    isAuthenticated: session.phase === 'authenticated',
    isLoginInProgress: [
      'starting',
      'awaiting_browser_callback',
      'awaiting_manual_input',
      'exchanging_code',
      'refreshing',
    ].includes(session.phase),
    needsManualInput: session.phase === 'awaiting_manual_input',
    isSignedOut: session.phase === 'idle',
    isFailed: session.phase === 'failed' || session.phase === 'cancelled',
  }
}

export function mapRuntimeRunCheckpoint(checkpoint: RuntimeRunCheckpointDto): RuntimeRunCheckpointView {
  return {
    sequence: checkpoint.sequence,
    kind: checkpoint.kind,
    kindLabel: getRuntimeRunCheckpointKindLabel(checkpoint.kind),
    summary: normalizeText(checkpoint.summary, 'Durable checkpoint recorded.'),
    createdAt: checkpoint.createdAt,
  }
}

export function mapRuntimeRun(runtimeRun: RuntimeRunDto): RuntimeRunView {
  const runtimeKind = normalizeText(runtimeRun.runtimeKind, 'openai_codex')
  const providerId = normalizeText(runtimeRun.providerId, 'provider-unavailable')
  const supervisorKind = normalizeText(runtimeRun.supervisorKind, 'owned_agent')
  const checkpoints = runtimeRun.checkpoints
    .map(mapRuntimeRunCheckpoint)
    .sort((left, right) => left.sequence - right.sequence)
  const latestCheckpoint = checkpoints[checkpoints.length - 1] ?? null

  return {
    projectId: runtimeRun.projectId,
    agentSessionId: runtimeRun.agentSessionId,
    runId: normalizeText(runtimeRun.runId, 'run-unavailable'),
    runtimeKind,
    providerId,
    runtimeLabel: getRuntimeRunLabel(runtimeKind, runtimeRun.status),
    supervisorKind,
    supervisorLabel: humanizeRuntimeKind(supervisorKind),
    status: runtimeRun.status,
    statusLabel: getRuntimeRunStatusLabel(runtimeRun.status),
    transport: {
      kind: normalizeText(runtimeRun.transport.kind, 'internal'),
      endpoint: normalizeText(runtimeRun.transport.endpoint, 'Unavailable'),
      liveness: runtimeRun.transport.liveness,
      livenessLabel: getRuntimeRunTransportLivenessLabel(runtimeRun.transport.liveness),
    },
    controls: mapRuntimeRunControlState(runtimeRun.controls),
    startedAt: runtimeRun.startedAt,
    lastHeartbeatAt: normalizeOptionalText(runtimeRun.lastHeartbeatAt),
    lastCheckpointSequence: runtimeRun.lastCheckpointSequence,
    lastCheckpointAt: normalizeOptionalText(runtimeRun.lastCheckpointAt),
    stoppedAt: normalizeOptionalText(runtimeRun.stoppedAt),
    lastErrorCode: normalizeOptionalText(runtimeRun.lastErrorCode),
    lastError: runtimeRun.lastError ?? null,
    updatedAt: runtimeRun.updatedAt,
    checkpoints,
    latestCheckpoint,
    checkpointCount: checkpoints.length,
    hasCheckpoints: checkpoints.length > 0,
    isActive: runtimeRun.status === 'starting' || runtimeRun.status === 'running',
    isTerminal: runtimeRun.status === 'stopped' || runtimeRun.status === 'failed',
    isStale: runtimeRun.status === 'stale',
    isFailed: runtimeRun.status === 'failed',
  }
}

export function mapAgentSession(agentSession: AgentSessionDto): AgentSessionView {
  return {
    projectId: agentSession.projectId,
    agentSessionId: normalizeText(agentSession.agentSessionId, DEFAULT_AGENT_SESSION_ID),
    title: normalizeText(agentSession.title, 'Agent session'),
    summary: agentSession.summary,
    status: agentSession.status,
    statusLabel: getAgentSessionStatusLabel(agentSession.status),
    selected: agentSession.selected,
    createdAt: agentSession.createdAt,
    updatedAt: agentSession.updatedAt,
    archivedAt: normalizeOptionalText(agentSession.archivedAt),
    lastRunId: normalizeOptionalText(agentSession.lastRunId),
    lastRuntimeKind: normalizeOptionalText(agentSession.lastRuntimeKind),
    lastProviderId: normalizeOptionalText(agentSession.lastProviderId),
    lineage: agentSession.lineage ?? null,
    isActive: agentSession.status === 'active',
    isArchived: agentSession.status === 'archived',
  }
}

export function selectAgentSessionId(agentSessions: readonly AgentSessionView[] | null | undefined): string {
  return (
    agentSessions?.find((session) => session.selected && session.isActive)?.agentSessionId ??
    agentSessions?.find((session) => session.isActive)?.agentSessionId ??
    DEFAULT_AGENT_SESSION_ID
  )
}

export function mergeRuntimeUpdated(
  currentRuntime: RuntimeSessionView | null,
  payload: RuntimeUpdatedPayloadDto,
): RuntimeSessionView {
  if (currentRuntime && timestampToSortValue(payload.updatedAt) < timestampToSortValue(currentRuntime.updatedAt)) {
    return currentRuntime
  }

  const nextFlowId = normalizeOptionalText(payload.flowId)
  const currentFlowId = currentRuntime?.flowId ?? null

  return mapRuntimeSession({
    projectId: payload.projectId,
    runtimeKind: payload.runtimeKind,
    providerId: payload.providerId,
    flowId: nextFlowId,
    sessionId: normalizeOptionalText(payload.sessionId),
    accountId: normalizeOptionalText(payload.accountId),
    phase: payload.authPhase,
    callbackBound: currentFlowId === nextFlowId ? currentRuntime?.callbackBound ?? null : null,
    authorizationUrl: currentFlowId === nextFlowId ? currentRuntime?.authorizationUrl ?? null : null,
    redirectUri: currentFlowId === nextFlowId ? currentRuntime?.redirectUri ?? null : null,
    lastErrorCode: normalizeOptionalText(payload.lastErrorCode),
    lastError: payload.lastError ?? null,
    updatedAt: payload.updatedAt,
  })
}
