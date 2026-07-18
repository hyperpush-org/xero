import { z } from 'zod'

import { isoTimestampSchema } from '@xero/ui/model/shared'
import { runtimeRunApprovalModeSchema } from '@xero/ui/model/runtime'
import { agentRefSchema } from './workflow-agents'

const idSchema = z.string().trim().min(1).max(120).regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]*$/)
const nonEmptyTextSchema = z.string().trim().min(1)
const optionalTextSchema = z.string().trim().optional().default('')
const jsonValueSchema = z.union([
  z.string(),
  z.number(),
  z.boolean(),
  z.null(),
  z.array(z.unknown()),
  z.record(z.unknown()),
])

export const workflowNodeIdSchema = idSchema
export const workflowEdgeIdSchema = idSchema

export const workflowNodeTypeSchema = z.enum([
  'agent',
  'router',
  'gate',
  'human_checkpoint',
  'merge',
  'terminal',
  'state_read',
  'state_write',
  'state_patch',
  'state_query',
  'state_checkpoint',
  'collection_loop',
  'subgraph',
  'command',
])
export type WorkflowNodeTypeDto = z.infer<typeof workflowNodeTypeSchema>

export const workflowEdgeTypeSchema = z.enum([
  'success',
  'failure',
  'conditional',
  'loop',
  'recovery',
  'manual_override',
])
export type WorkflowEdgeTypeDto = z.infer<typeof workflowEdgeTypeSchema>

export const workflowArtifactTypeSchema = z.string().trim().min(1).max(120)
export type WorkflowArtifactTypeDto = z.infer<typeof workflowArtifactTypeSchema>

export const workflowArtifactPresetSchema = z.enum([
  'text_output',
  'task_brief',
  'delivery_plan',
  'plan',
  'implementation_summary',
  'verification_result',
  'debug_report',
  'gap_list',
  'review_findings',
  'human_decision',
  'milestone_audit',
  'command_result',
  'subgraph_result',
])
export type WorkflowArtifactPresetDto = z.infer<typeof workflowArtifactPresetSchema>

export const workflowNodeRunStatusSchema = z.enum([
  'pending',
  'eligible',
  'starting',
  'running',
  'waiting_on_gate',
  'succeeded',
  'failed',
  'stalled',
  'skipped',
  'cancelled',
])
export type WorkflowNodeRunStatusDto = z.infer<typeof workflowNodeRunStatusSchema>

export const workflowRunStatusSchema = z.enum([
  'queued',
  'running',
  'paused',
  'cancelling',
  'completed',
  'failed',
  'cancelled',
])
export type WorkflowRunStatusDto = z.infer<typeof workflowRunStatusSchema>

export const workflowTerminalStatusSchema = z.enum([
  'success',
  'failure',
  'cancelled',
  'needs_human',
])
export type WorkflowTerminalStatusDto = z.infer<typeof workflowTerminalStatusSchema>

export const workflowHumanCheckpointTypeSchema = z.enum([
  'human_verify',
  'decision',
  'human_action',
])
export type WorkflowHumanCheckpointTypeDto = z.infer<typeof workflowHumanCheckpointTypeSchema>

export const workflowAttemptScopeSchema = z.enum([
  'run',
  'source_node',
  'target_node',
  'artifact_group',
])
export type WorkflowAttemptScopeDto = z.infer<typeof workflowAttemptScopeSchema>

export const workflowCarryoverPolicySchema = z.enum([
  'all',
  'required_only',
  'none',
  'selected',
])
export type WorkflowCarryoverPolicyDto = z.infer<typeof workflowCarryoverPolicySchema>

export const workflowResetPolicySchema = z.enum([
  'never',
  'on_downstream_success',
  'on_terminal_success',
])
export type WorkflowResetPolicyDto = z.infer<typeof workflowResetPolicySchema>

export const workflowStallDetectorSchema = z.enum([
  'finding_count_not_decreasing',
  'same_failure_class_repeated',
  'no_artifact_progress',
  'runtime_activity_timeout',
  'retry_limit_exceeded',
])
export type WorkflowStallDetectorDto = z.infer<typeof workflowStallDetectorSchema>

export const workflowMergeWaitPolicySchema = z.enum([
  'all',
  'any',
  'quorum',
  'fail_fast',
])
export type WorkflowMergeWaitPolicyDto = z.infer<typeof workflowMergeWaitPolicySchema>

export const workflowResourceConflictModeSchema = z.enum([
  'allow_conflicts',
  'serialize_conflicts',
])
export type WorkflowResourceConflictModeDto = z.infer<
  typeof workflowResourceConflictModeSchema
>

export const workflowNumberCompareOperatorSchema = z.enum([
  'eq',
  'neq',
  'gt',
  'gte',
  'lt',
  'lte',
])
export type WorkflowNumberCompareOperatorDto = z.infer<
  typeof workflowNumberCompareOperatorSchema
>

export type WorkflowConditionDto =
  | { kind: 'always' }
  | { kind: 'all'; conditions: WorkflowConditionDto[] }
  | { kind: 'any'; conditions: WorkflowConditionDto[] }
  | { kind: 'not'; condition: WorkflowConditionDto }
  | { kind: 'node_status'; nodeId: string; status: WorkflowNodeRunStatusDto }
  | { kind: 'artifact_exists'; artifactRef: string }
  | { kind: 'artifact_field_equals'; artifactRef: string; path: string; value: unknown }
  | { kind: 'artifact_field_in'; artifactRef: string; path: string; values: unknown[] }
  | {
      kind: 'artifact_field_number_compare'
      artifactRef: string
      path: string
      operator: WorkflowNumberCompareOperatorDto
      value: number
    }
  | { kind: 'failure_class_is'; nodeId?: string | null; failureClass: string }
  | { kind: 'loop_attempt_lt'; loopKey: string; value: number }
  | { kind: 'loop_attempt_gte'; loopKey: string; value: number }
  | { kind: 'human_decision_is'; checkpointNodeId: string; decision: string }
  | { kind: 'state_field_equals'; stateRef: string; path: string; value: unknown }
  | {
      kind: 'state_collection_count_compare'
      stateRef: string
      operator: WorkflowNumberCompareOperatorDto
      value: number
    }

export const workflowConditionSchema: z.ZodType<WorkflowConditionDto> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('always') }).strict(),
    z
      .object({
        kind: z.literal('all'),
        conditions: z.array(workflowConditionSchema).min(1),
      })
      .strict(),
    z
      .object({
        kind: z.literal('any'),
        conditions: z.array(workflowConditionSchema).min(1),
      })
      .strict(),
    z
      .object({
        kind: z.literal('not'),
        condition: workflowConditionSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('node_status'),
        nodeId: workflowNodeIdSchema,
        status: workflowNodeRunStatusSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('artifact_exists'),
        artifactRef: nonEmptyTextSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('artifact_field_equals'),
        artifactRef: nonEmptyTextSchema,
        path: nonEmptyTextSchema,
        value: jsonValueSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('artifact_field_in'),
        artifactRef: nonEmptyTextSchema,
        path: nonEmptyTextSchema,
        values: z.array(jsonValueSchema).min(1),
      })
      .strict(),
    z
      .object({
        kind: z.literal('artifact_field_number_compare'),
        artifactRef: nonEmptyTextSchema,
        path: nonEmptyTextSchema,
        operator: workflowNumberCompareOperatorSchema,
        value: z.number(),
      })
      .strict(),
    z
      .object({
        kind: z.literal('failure_class_is'),
        nodeId: workflowNodeIdSchema.nullable().optional(),
        failureClass: nonEmptyTextSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('loop_attempt_lt'),
        loopKey: nonEmptyTextSchema,
        value: z.number().int().nonnegative(),
      })
      .strict(),
    z
      .object({
        kind: z.literal('loop_attempt_gte'),
        loopKey: nonEmptyTextSchema,
        value: z.number().int().nonnegative(),
      })
      .strict(),
    z
      .object({
        kind: z.literal('human_decision_is'),
        checkpointNodeId: workflowNodeIdSchema,
        decision: nonEmptyTextSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('state_field_equals'),
        stateRef: nonEmptyTextSchema,
        path: nonEmptyTextSchema,
        value: jsonValueSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('state_collection_count_compare'),
        stateRef: nonEmptyTextSchema,
        operator: workflowNumberCompareOperatorSchema,
        value: z.number(),
      })
      .strict(),
  ]),
)

export const workflowPositionSchema = z
  .object({
    x: z.number(),
    y: z.number(),
  })
  .strict()
export type WorkflowPositionDto = z.infer<typeof workflowPositionSchema>

export const workflowRunOverrideSchema = z
  .object({
    providerProfileId: z.string().trim().min(1).nullable().optional(),
    modelId: z.string().trim().min(1).nullable().optional(),
    thinkingEffort: z.string().trim().min(1).nullable().optional(),
    approvalMode: runtimeRunApprovalModeSchema.nullable().optional(),
    promptPreface: z.string().optional().default(''),
    planModeRequired: z.boolean().default(false),
    autoCompactEnabled: z.boolean().default(true),
  })
  .strict()
export type WorkflowRunOverrideDto = z.infer<typeof workflowRunOverrideSchema>

export const workflowInputBindingSchema = z.discriminatedUnion('source', [
  z
    .object({
      source: z.literal('run_input'),
      name: nonEmptyTextSchema,
      required: z.boolean().default(true),
      path: z.string().trim().min(1).nullable().optional(),
      promptLabel: z.string().trim().min(1).nullable().optional(),
    })
    .strict(),
  z
    .object({
      source: z.literal('artifact'),
      name: nonEmptyTextSchema,
      required: z.boolean().default(true),
      artifactRef: nonEmptyTextSchema,
      path: z.string().trim().min(1).nullable().optional(),
      promptLabel: z.string().trim().min(1).nullable().optional(),
    })
    .strict(),
  z
    .object({
      source: z.literal('state'),
      name: nonEmptyTextSchema,
      required: z.boolean().default(true),
      stateRef: nonEmptyTextSchema,
      path: z.string().trim().min(1).nullable().optional(),
      promptLabel: z.string().trim().min(1).nullable().optional(),
    })
    .strict(),
])
export type WorkflowInputBindingDto = z.infer<typeof workflowInputBindingSchema>

export const workflowOutputExtractionSchema = z.enum([
  'generic_text',
  'json_object',
  'json_array',
])
export type WorkflowOutputExtractionDto = z.infer<typeof workflowOutputExtractionSchema>

export const workflowOutputContractSchema = z
  .object({
    artifactType: workflowArtifactTypeSchema,
    schemaVersion: z.number().int().positive().default(1),
    extraction: workflowOutputExtractionSchema.default('generic_text'),
    required: z.boolean().default(true),
    renderTextPath: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type WorkflowOutputContractDto = z.infer<typeof workflowOutputContractSchema>

export const workflowDeliveryStateEntityTypeSchema = z.enum([
  'delivery_project',
  'milestone',
  'requirement',
  'delivery_phase',
  'phase_context',
  'phase_plan',
  'phase_summary',
  'verification_evidence',
  'deferred_item',
  'milestone_archive',
])
export type WorkflowDeliveryStateEntityTypeDto = z.infer<
  typeof workflowDeliveryStateEntityTypeSchema
>

export const workflowStateQueryFilterSchema = z
  .object({
    path: nonEmptyTextSchema,
    operator: z.enum(['eq', 'neq', 'in', 'not_in', 'exists', 'missing']).default('eq'),
    value: jsonValueSchema.optional(),
    values: z.array(jsonValueSchema).optional().default([]),
  })
  .strict()
export type WorkflowStateQueryFilterDto = z.infer<typeof workflowStateQueryFilterSchema>

export const workflowStateQuerySchema = z
  .object({
    entityType: workflowDeliveryStateEntityTypeSchema,
    filters: z.array(workflowStateQueryFilterSchema).default([]),
    orderBy: nonEmptyTextSchema.nullable().optional(),
    limit: z.number().int().positive().nullable().optional(),
    includeArchived: z.boolean().default(false),
  })
  .strict()
export type WorkflowStateQueryDto = z.infer<typeof workflowStateQuerySchema>

export const workflowStateWriteActionSchema = z.enum([
  'create',
  'upsert',
  'update',
  'patch',
  'mark_complete',
  'archive',
])
export type WorkflowStateWriteActionDto = z.infer<typeof workflowStateWriteActionSchema>

export const workflowStateWriteOperationSchema = z
  .object({
    entityType: workflowDeliveryStateEntityTypeSchema,
    action: workflowStateWriteActionSchema,
    idempotencyKey: nonEmptyTextSchema.nullable().optional(),
    targetId: nonEmptyTextSchema.nullable().optional(),
    payload: z.record(z.unknown()).default({}),
    outputArtifactType: workflowArtifactTypeSchema.default('state_write_result'),
  })
  .strict()
export type WorkflowStateWriteOperationDto = z.infer<
  typeof workflowStateWriteOperationSchema
>

const workflowCollectionResumeInputPathSchema = z
  .string()
  .trim()
  .min(1)
  .regex(
    /^\$(?:\.[^.\[\]\s]+)*$/,
    'Collection loop resume paths must use $ or object fields such as $.phase.from; array indexes are not supported.',
  )

export const workflowCollectionLoopControlsSchema = z
  .object({
    fromInputPath: workflowCollectionResumeInputPathSchema.nullable().optional(),
    toInputPath: workflowCollectionResumeInputPathSchema.nullable().optional(),
    onlyInputPath: workflowCollectionResumeInputPathSchema.nullable().optional(),
  })
  .strict()
export type WorkflowCollectionLoopControlsDto = z.infer<
  typeof workflowCollectionLoopControlsSchema
>

export const workflowCommandParserSchema = z
  .object({
    extraction: workflowOutputExtractionSchema.default('generic_text'),
    renderTextPath: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type WorkflowCommandParserDto = z.infer<typeof workflowCommandParserSchema>

export const workflowFailureClassificationPolicySchema = z
  .object({
    runtimeActivityTimeoutSeconds: z.number().int().positive().nullable().optional(),
    quotaFailureClasses: z.array(nonEmptyTextSchema).default([]),
    transientFailureClasses: z.array(nonEmptyTextSchema).default([]),
  })
  .strict()
export type WorkflowFailureClassificationPolicyDto = z.infer<
  typeof workflowFailureClassificationPolicySchema
>

const workflowNodeBaseSchema = z.object({
  id: workflowNodeIdSchema,
  title: nonEmptyTextSchema,
  description: optionalTextSchema,
  position: workflowPositionSchema.default({ x: 0, y: 0 }),
})

export const workflowAgentNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('agent'),
    agentRef: agentRefSchema,
    displayLabel: z.string().trim().min(1).nullable().optional(),
    inputBindings: z.array(workflowInputBindingSchema).default([]),
    outputContract: workflowOutputContractSchema.default({
      artifactType: 'text_output',
      schemaVersion: 1,
      extraction: 'generic_text',
      required: true,
    }),
    runOverrides: workflowRunOverrideSchema.nullable().optional(),
    resourceScopes: z.array(nonEmptyTextSchema).default([]),
    failurePolicy: workflowFailureClassificationPolicySchema.default({
      quotaFailureClasses: [],
      transientFailureClasses: [],
    }),
  })
  .strict()
export type WorkflowAgentNodeDto = z.infer<typeof workflowAgentNodeSchema>

export const workflowRouterNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('router'),
  })
  .strict()

export const workflowGateNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('gate'),
    requiredChecks: z.array(workflowConditionSchema).default([]),
    onBlocked: z.enum(['pause', 'fail']).default('pause'),
  })
  .strict()

export const workflowHumanCheckpointNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('human_checkpoint'),
    checkpointType: workflowHumanCheckpointTypeSchema,
    prompt: nonEmptyTextSchema,
    decisionOptions: z.array(nonEmptyTextSchema).default([]),
    resumePayloadSchema: z.record(z.unknown()).nullable().optional(),
    stateUpdates: z.array(workflowStateWriteOperationSchema).default([]),
  })
  .strict()

export const workflowMergeNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('merge'),
    waitPolicy: workflowMergeWaitPolicySchema.default('all'),
    quorum: z.number().int().positive().nullable().optional(),
    failFast: z.boolean().default(false),
  })
  .strict()

export const workflowTerminalNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('terminal'),
    terminalStatus: workflowTerminalStatusSchema,
  })
  .strict()

export const workflowStateReadNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('state_read'),
    query: workflowStateQuerySchema,
    outputArtifactType: workflowArtifactTypeSchema.default('state_read_result'),
  })
  .strict()

export const workflowStateQueryNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('state_query'),
    query: workflowStateQuerySchema,
    outputArtifactType: workflowArtifactTypeSchema.default('state_query_result'),
  })
  .strict()

export const workflowStateWriteNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('state_write'),
    inputBindings: z.array(workflowInputBindingSchema).default([]),
    operation: workflowStateWriteOperationSchema,
  })
  .strict()

export const workflowStatePatchNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('state_patch'),
    inputBindings: z.array(workflowInputBindingSchema).default([]),
    operation: workflowStateWriteOperationSchema,
  })
  .strict()

export const workflowStateCheckpointNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('state_checkpoint'),
    requiredChecks: z.array(workflowConditionSchema).default([]),
    onBlocked: z.enum(['pause', 'fail']).default('pause'),
  })
  .strict()

export const workflowCollectionLoopNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('collection_loop'),
    collection: workflowStateQuerySchema,
    itemArtifactType: workflowArtifactTypeSchema.default('collection_item'),
    itemVariableName: nonEmptyTextSchema.default('item'),
    sortKey: nonEmptyTextSchema.nullable().optional(),
    afterItemRequery: z.boolean().default(true),
    maxItemCount: z.number().int().positive().default(100),
    maxRuntimeSeconds: z.number().int().positive().nullable().optional(),
    controls: workflowCollectionLoopControlsSchema.default({}),
  })
  .strict()

export const workflowSubgraphNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('subgraph'),
    subgraphId: workflowNodeIdSchema,
    inputBindings: z.array(workflowInputBindingSchema).default([]),
    outputContract: workflowOutputContractSchema.default({
      artifactType: 'subgraph_result',
      schemaVersion: 1,
      extraction: 'json_object',
      required: true,
    }),
  })
  .strict()

export const workflowCommandNodeSchema = workflowNodeBaseSchema
  .extend({
    type: z.literal('command'),
    command: nonEmptyTextSchema,
    args: z.array(z.string()).default([]),
    allowedCommands: z.array(nonEmptyTextSchema).default([]),
    workingDirectory: z.string().trim().min(1).nullable().optional(),
    timeoutSeconds: z.number().int().positive().default(120),
    successExitCodes: z.array(z.number().int()).default([0]),
    outputContract: workflowOutputContractSchema.default({
      artifactType: 'command_result',
      schemaVersion: 1,
      extraction: 'json_object',
      required: true,
    }),
    parser: workflowCommandParserSchema.default({
      extraction: 'generic_text',
    }),
  })
  .strict()

export const workflowNodeSchema = z.discriminatedUnion('type', [
  workflowAgentNodeSchema,
  workflowRouterNodeSchema,
  workflowGateNodeSchema,
  workflowHumanCheckpointNodeSchema,
  workflowMergeNodeSchema,
  workflowTerminalNodeSchema,
  workflowStateReadNodeSchema,
  workflowStateWriteNodeSchema,
  workflowStatePatchNodeSchema,
  workflowStateQueryNodeSchema,
  workflowStateCheckpointNodeSchema,
  workflowCollectionLoopNodeSchema,
  workflowSubgraphNodeSchema,
  workflowCommandNodeSchema,
])
export type WorkflowNodeDto = z.infer<typeof workflowNodeSchema>

export const workflowLoopPolicySchema = z
  .object({
    loopKey: nonEmptyTextSchema,
    maxAttempts: z.number().int().positive(),
    attemptScope: workflowAttemptScopeSchema.default('run'),
    carryoverPolicy: workflowCarryoverPolicySchema.default('all'),
    selectedArtifactRefs: z.array(nonEmptyTextSchema).default([]),
    resetPolicy: workflowResetPolicySchema.default('never'),
    stallDetector: workflowStallDetectorSchema.nullable().optional(),
    onExhausted: workflowNodeIdSchema,
  })
  .strict()
export type WorkflowLoopPolicyDto = z.infer<typeof workflowLoopPolicySchema>

export const workflowEdgeSchema = z
  .object({
    id: workflowEdgeIdSchema,
    fromNodeId: workflowNodeIdSchema,
    toNodeId: workflowNodeIdSchema,
    type: workflowEdgeTypeSchema,
    label: z.string().trim().max(80).optional().default(''),
    priority: z.number().int().min(0).default(100),
    condition: workflowConditionSchema.default({ kind: 'always' }),
    loopPolicy: workflowLoopPolicySchema.nullable().optional(),
  })
  .strict()
export type WorkflowEdgeDto = z.infer<typeof workflowEdgeSchema>

export const workflowArtifactContractSchema = z
  .object({
    artifactType: workflowArtifactTypeSchema,
    schemaVersion: z.number().int().positive().default(1),
    jsonSchema: z.record(z.unknown()).nullable().optional(),
    displayName: nonEmptyTextSchema,
    description: optionalTextSchema,
  })
  .strict()
export type WorkflowArtifactContractDto = z.infer<typeof workflowArtifactContractSchema>

export const workflowRunPolicySchema = z
  .object({
    defaultProviderProfileId: z.string().trim().min(1).nullable().optional(),
    defaultModelId: z.string().trim().min(1).nullable().optional(),
    approvalMode: runtimeRunApprovalModeSchema.nullable().optional(),
    concurrencyLimit: z.number().int().positive().max(16).default(1),
    nodeTimeoutSeconds: z.number().int().positive().nullable().optional(),
    resourceConflictPolicy: z
      .object({
        mode: workflowResourceConflictModeSchema.default('serialize_conflicts'),
        defaultScopes: z.array(nonEmptyTextSchema).default([]),
      })
      .strict()
      .default({
        mode: 'serialize_conflicts',
        defaultScopes: [],
      }),
    recoveryDefaults: z
      .object({
        debugMaxAttempts: z.number().int().nonnegative().default(2),
        gapClosureMaxAttempts: z.number().int().nonnegative().default(2),
        reviewFixMaxAttempts: z.number().int().nonnegative().default(3),
      })
      .strict()
      .default({
        debugMaxAttempts: 2,
        gapClosureMaxAttempts: 2,
        reviewFixMaxAttempts: 3,
      }),
  })
  .strict()
export type WorkflowRunPolicyDto = z.infer<typeof workflowRunPolicySchema>

export const workflowSubgraphSchema = z
  .object({
    id: workflowNodeIdSchema,
    title: nonEmptyTextSchema,
    description: optionalTextSchema,
    startNodeId: workflowNodeIdSchema,
    nodes: z.array(workflowNodeSchema).min(1),
    edges: z.array(workflowEdgeSchema).default([]),
    inputBindings: z.array(workflowInputBindingSchema).default([]),
    outputContract: workflowOutputContractSchema.default({
      artifactType: 'subgraph_result',
      schemaVersion: 1,
      extraction: 'json_object',
      required: true,
    }),
  })
  .strict()
export type WorkflowSubgraphDto = z.infer<typeof workflowSubgraphSchema>

export const workflowDefinitionSchema = z
  .object({
    schema: z.literal('xero.workflow_definition.v1').default('xero.workflow_definition.v1'),
    id: workflowNodeIdSchema,
    projectId: nonEmptyTextSchema,
    name: nonEmptyTextSchema,
    description: optionalTextSchema,
    version: z.number().int().positive().default(1),
    startNodeId: workflowNodeIdSchema,
    nodes: z.array(workflowNodeSchema).min(1),
    edges: z.array(workflowEdgeSchema).default([]),
    subgraphs: z.array(workflowSubgraphSchema).default([]),
    artifactContracts: z.array(workflowArtifactContractSchema).default([]),
    runPolicy: workflowRunPolicySchema.default({
      concurrencyLimit: 1,
      resourceConflictPolicy: {
        mode: 'serialize_conflicts',
        defaultScopes: [],
      },
      recoveryDefaults: {
        debugMaxAttempts: 2,
        gapClosureMaxAttempts: 2,
        reviewFixMaxAttempts: 3,
      },
    }),
    createdAt: isoTimestampSchema.nullable().optional(),
    updatedAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()
export type WorkflowDefinitionDto = z.infer<typeof workflowDefinitionSchema>

export const workflowDefinitionSummarySchema = z
  .object({
    id: nonEmptyTextSchema,
    projectId: nonEmptyTextSchema,
    name: nonEmptyTextSchema,
    description: z.string(),
    activeVersionId: nonEmptyTextSchema,
    activeVersionNumber: z.number().int().positive(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowDefinitionSummaryDto = z.infer<
  typeof workflowDefinitionSummarySchema
>

export const workflowValidationSeveritySchema = z.enum(['error', 'warning'])
export type WorkflowValidationSeverityDto = z.infer<
  typeof workflowValidationSeveritySchema
>

export const workflowValidationDiagnosticSchema = z
  .object({
    severity: workflowValidationSeveritySchema,
    code: nonEmptyTextSchema,
    path: nonEmptyTextSchema,
    message: nonEmptyTextSchema,
  })
  .strict()
export type WorkflowValidationDiagnosticDto = z.infer<
  typeof workflowValidationDiagnosticSchema
>

export const workflowValidationReportSchema = z
  .object({
    status: z.enum(['valid', 'invalid']),
    diagnostics: z.array(workflowValidationDiagnosticSchema),
  })
  .strict()
export type WorkflowValidationReportDto = z.infer<typeof workflowValidationReportSchema>

export function validateWorkflowDefinition(input: unknown): WorkflowValidationReportDto {
  const parsed = workflowDefinitionSchema.safeParse(input)
  if (!parsed.success) {
    return {
      status: 'invalid',
      diagnostics: parsed.error.issues.map((issue) => ({
        severity: 'error',
        code: 'schema_invalid',
        path: issue.path.length > 0 ? issue.path.join('.') : '$',
        message: issue.message,
      })),
    }
  }

  const diagnostics = validateWorkflowDefinitionGraph(parsed.data)
  return {
    status: diagnostics.some((diagnostic) => diagnostic.severity === 'error')
      ? 'invalid'
      : 'valid',
    diagnostics,
  }
}

function validateWorkflowDefinitionGraph(
  definition: WorkflowDefinitionDto,
  graph: Pick<WorkflowDefinitionDto, 'startNodeId' | 'nodes' | 'edges'> = definition,
  pathPrefix = '',
  subgraphContext?: { id: string },
): WorkflowValidationDiagnosticDto[] {
  const diagnostics: WorkflowValidationDiagnosticDto[] = []
  const nodeIds = new Set<string>()
  const edgeIds = new Set<string>()
  const producedArtifactRefs = new Set<string>()
  const artifactContractByRef = new Map<string, WorkflowArtifactContractDto>()

  graph.nodes.forEach((node, index) => {
    const nodePath = `${pathPrefix}nodes.${index}`
    if (nodeIds.has(node.id)) {
      diagnostics.push(error('duplicate_node_id', `${nodePath}.id`, `Node id \`${node.id}\` is duplicated.`))
    }
    nodeIds.add(node.id)
    const producedArtifactType = producedArtifactTypeForNode(node)
    const artifactRef = producedArtifactType ? `${node.id}.${producedArtifactType}` : null
    if (artifactRef) {
      producedArtifactRefs.add(artifactRef)
    }
    if (node.type === 'agent' || node.type === 'command' || node.type === 'subgraph') {
      const contract = definition.artifactContracts.find(
        (candidate) =>
          candidate.artifactType === node.outputContract.artifactType &&
          candidate.schemaVersion === node.outputContract.schemaVersion,
      )
      if (!contract && node.outputContract.extraction !== 'generic_text') {
        diagnostics.push(error(
          'artifact_contract_missing',
          `${nodePath}.outputContract`,
          `JSON artifact \`${node.outputContract.artifactType}\` v${node.outputContract.schemaVersion} must declare an artifact contract.`,
        ))
      }
      if (contract && artifactRef) artifactContractByRef.set(artifactRef, contract)
      if (
        node.outputContract.renderTextPath &&
        !node.outputContract.renderTextPath.trim().startsWith('$')
      ) {
        diagnostics.push(error(
          'render_text_path_invalid',
          `${nodePath}.outputContract.renderTextPath`,
          'Render paths must use a JSON path that starts with `$`.',
        ))
      } else if (
        node.outputContract.renderTextPath &&
        contract?.jsonSchema &&
        !jsonSchemaAllowsPath(contract.jsonSchema, node.outputContract.renderTextPath)
      ) {
        diagnostics.push(error(
          'render_text_path_not_in_schema',
          `${nodePath}.outputContract.renderTextPath`,
          `Render path \`${node.outputContract.renderTextPath}\` is not allowed by the \`${node.outputContract.artifactType}\` artifact schema.`,
        ))
      }
    } else if (artifactRef && producedArtifactType) {
      const contract = definition.artifactContracts.find(
        (candidate) => candidate.artifactType === producedArtifactType && candidate.schemaVersion === 1,
      )
      if (contract) artifactContractByRef.set(artifactRef, contract)
    }
  })

  const subgraphIds = new Set<string>()
  definition.subgraphs.forEach((subgraph, index) => {
    if (!subgraphContext && subgraphIds.has(subgraph.id)) {
      diagnostics.push(error('duplicate_subgraph_id', `subgraphs.${index}.id`, `Subgraph id \`${subgraph.id}\` is duplicated.`))
    }
    subgraphIds.add(subgraph.id)
    if (subgraphContext) return
    if (subgraph.nodes.length === 0) {
      diagnostics.push(error('subgraph_nodes_empty', `subgraphs.${index}.nodes`, `Subgraph \`${subgraph.id}\` must contain at least one node.`))
      return
    }
    diagnostics.push(...validateWorkflowDefinitionGraph(
      definition,
      subgraph,
      `subgraphs.${index}.`,
      { id: subgraph.id },
    ))
  })

  if (!nodeIds.has(graph.startNodeId)) {
    diagnostics.push(subgraphContext
      ? error('subgraph_start_node_missing', `${pathPrefix}startNodeId`, `Subgraph \`${subgraphContext.id}\` references a missing start node.`)
      : error('start_node_missing', 'startNodeId', 'The start node must exist.'))
  }

  const outgoingDefaults = new Map<string, string>()
  const outgoingEdges = new Map<string, WorkflowEdgeDto[]>()

  const loopKeys = new Set(graph.edges.flatMap((edge) => edge.loopPolicy ? [edge.loopPolicy.loopKey] : []))
  graph.edges.forEach((edge, index) => {
    const edgePath = `${pathPrefix}edges.${index}`
    if (edgeIds.has(edge.id)) {
      diagnostics.push(error('duplicate_edge_id', `${edgePath}.id`, `Edge id \`${edge.id}\` is duplicated.`))
    }
    edgeIds.add(edge.id)
    if (!nodeIds.has(edge.fromNodeId)) {
      diagnostics.push(error(subgraphContext ? 'subgraph_edge_source_missing' : 'edge_source_missing', `${edgePath}.fromNodeId`, `Edge \`${edge.id}\` references a missing source node.`))
    }
    if (!nodeIds.has(edge.toNodeId)) {
      diagnostics.push(error(subgraphContext ? 'subgraph_edge_target_missing' : 'edge_target_missing', `${edgePath}.toNodeId`, `Edge \`${edge.id}\` references a missing target node.`))
    }
    if (edge.condition.kind === 'always') {
      const buckets = defaultEdgeBuckets(edge.type)
      const conflicts = buckets.some((bucket) =>
        outgoingDefaults.has(`${edge.fromNodeId}:${bucket}`),
      )
      if (conflicts) {
        diagnostics.push(error('duplicate_default_edge', `${edgePath}.condition`, `Node \`${edge.fromNodeId}\` has more than one default else edge.`))
      } else {
        buckets.forEach((bucket) => outgoingDefaults.set(`${edge.fromNodeId}:${bucket}`, edge.id))
      }
    }
    if (edge.type === 'loop' || edge.loopPolicy) {
      if (!edge.loopPolicy) {
        diagnostics.push(error('loop_policy_missing', `${edgePath}.loopPolicy`, `Loop edge \`${edge.id}\` must declare a loop policy.`))
      } else {
        if (edge.loopPolicy.maxAttempts <= 0) {
          diagnostics.push(error('loop_max_attempts_invalid', `${edgePath}.loopPolicy.maxAttempts`, `Loop edge \`${edge.id}\` must allow at least one attempt.`))
        }
        if (!nodeIds.has(edge.loopPolicy.onExhausted)) {
          diagnostics.push(error('loop_exhaustion_target_missing', `${edgePath}.loopPolicy.onExhausted`, `Loop edge \`${edge.id}\` must route exhaustion to an existing node.`))
        }
        edge.loopPolicy.selectedArtifactRefs.forEach((artifactRef, artifactIndex) => {
          if (!producedArtifactRefs.has(artifactRef)) {
            diagnostics.push(error('loop_artifact_ref_missing', `${edgePath}.loopPolicy.selectedArtifactRefs.${artifactIndex}`, `Loop policy references missing artifact \`${artifactRef}\`.`))
          }
        })
      }
    }
    validateConditionSemantics(edge.condition, `${edgePath}.condition`, nodeIds, producedArtifactRefs, artifactContractByRef, loopKeys, diagnostics)
    const entries = outgoingEdges.get(edge.fromNodeId) ?? []
    entries.push(edge)
    outgoingEdges.set(edge.fromNodeId, entries)
  })

  outgoingEdges.forEach((nodeEdges, nodeId) => {
    const conditionalBuckets = new Set<string>()
    nodeEdges.forEach((edge) => {
      if (edge.condition.kind !== 'always') {
        defaultEdgeBuckets(edge.type).forEach((bucket) => conditionalBuckets.add(bucket))
      }
    })
    conditionalBuckets.forEach((bucket) => {
      if (
        !outgoingDefaults.has(`${nodeId}:all`) &&
        !outgoingDefaults.has(`${nodeId}:${bucket}`)
      ) {
        diagnostics.push(error(
          'conditional_route_fallback_missing',
          `${pathPrefix}nodes.${nodeId}`,
          `Node \`${nodeId}\` has conditional ${bucket} routes but no \`always\` fallback for that outcome. Route the fallback to a Human Checkpoint or Terminal node.`,
        ))
      }
    })
  })

  graph.nodes.forEach((node, index) => {
    const nodePath = `${pathPrefix}nodes.${index}`
    if (
      subgraphContext &&
      node.type === 'terminal' &&
      node.terminalStatus === 'needs_human'
    ) {
      diagnostics.push(error(
        'subgraph_needs_human_terminal_unsupported',
        `${nodePath}.terminalStatus`,
        'Subgraphs cannot pause through a Needs Human Terminal. Route to a Human Checkpoint so the paused run has a resumable gate.',
      ))
    }
    if (
      node.type === 'agent' ||
      node.type === 'state_write' ||
      node.type === 'state_patch' ||
      node.type === 'subgraph'
    ) {
      node.inputBindings.forEach((binding, bindingIndex) => {
        const bindingPath = `${nodePath}.inputBindings.${bindingIndex}`
        if (binding.path && !binding.path.trim().startsWith('$')) {
          diagnostics.push(error('input_binding_path_invalid', `${bindingPath}.path`, 'Input binding paths must use a JSON path that starts with `$`.'))
        }
        if (binding.source === 'artifact' && !producedArtifactRefs.has(binding.artifactRef)) {
          diagnostics.push(error('artifact_ref_missing', `${bindingPath}.artifactRef`, `Artifact reference \`${binding.artifactRef}\` is not produced by any agent node.`))
        }
        if (binding.source === 'state' && !producedArtifactRefs.has(binding.stateRef)) {
          diagnostics.push(error('state_ref_missing', `${bindingPath}.stateRef`, `State reference \`${binding.stateRef}\` is not produced by any state-capable node.`))
        }
      })
    }
    if (node.type === 'state_read' || node.type === 'state_query') {
      validateStateQuery(node.query, `${nodePath}.query`, diagnostics)
    }
    if (node.type === 'state_write' || node.type === 'state_patch') {
      validateStateWriteOperation(node.operation, `${nodePath}.operation`, diagnostics, true)
    }
    if (node.type === 'collection_loop') {
      validateStateQuery(node.collection, `${nodePath}.collection`, diagnostics)
      if (node.maxItemCount <= 0) {
        diagnostics.push(error('collection_loop_max_item_count_invalid', `${nodePath}.maxItemCount`, 'Collection loops must allow at least one item.'))
      }
      if (node.sortKey && !node.sortKey.trim().startsWith('$')) {
        diagnostics.push(error('collection_loop_sort_path_invalid', `${nodePath}.sortKey`, 'Collection loop sort keys must use a JSON path that starts with `$`.'))
      }
    }
    if (node.type === 'subgraph' && !subgraphIds.has(node.subgraphId)) {
      diagnostics.push(error('subgraph_ref_missing', `${nodePath}.subgraphId`, `Subgraph node references missing subgraph \`${node.subgraphId}\`.`))
    }
    if (node.type === 'command') {
      if (!node.command.trim()) {
        diagnostics.push(error('command_empty', `${nodePath}.command`, 'Command nodes must declare a command.'))
      }
      if (node.timeoutSeconds <= 0) {
        diagnostics.push(error('command_timeout_invalid', `${nodePath}.timeoutSeconds`, 'Command node timeout must be at least one second.'))
      }
      if (node.allowedCommands.length === 0) {
        diagnostics.push(error('command_allowlist_empty', `${nodePath}.allowedCommands`, 'Command nodes must declare an allowlist.'))
      } else if (!node.allowedCommands.includes(node.command)) {
        diagnostics.push(error('command_not_in_allowlist', `${nodePath}.allowedCommands`, `Command \`${node.command}\` must appear in the command node allowlist.`))
      }
      validateWorkflowCommandPolicy(node, nodePath, diagnostics)
    }
    if (node.type === 'human_checkpoint') {
      const seen = new Set<string>()
      node.decisionOptions.forEach((option, optionIndex) => {
        const trimmed = option.trim()
        if (!trimmed) {
          diagnostics.push(error('checkpoint_decision_empty', `${nodePath}.decisionOptions.${optionIndex}`, 'Human checkpoint decision options cannot be blank.'))
        } else if (seen.has(trimmed)) {
          diagnostics.push(error('checkpoint_decision_duplicate', `${nodePath}.decisionOptions.${optionIndex}`, `Human checkpoint decision \`${trimmed}\` is duplicated.`))
        }
        seen.add(trimmed)
      })
      if (node.resumePayloadSchema !== null && node.resumePayloadSchema !== undefined && !isRecord(node.resumePayloadSchema)) {
        diagnostics.push(error('checkpoint_payload_schema_invalid', `${nodePath}.resumePayloadSchema`, 'Human checkpoint resume payload schemas must be JSON Schema objects.'))
      }
      node.stateUpdates.forEach((operation, operationIndex) => {
        validateStateWriteOperation(operation, `${nodePath}.stateUpdates.${operationIndex}`, diagnostics, false)
      })
    }
    if (node.type === 'merge' && node.waitPolicy === 'quorum' && !node.quorum) {
      diagnostics.push(error('merge_quorum_missing', `${nodePath}.quorum`, 'Quorum merge nodes must declare a quorum.'))
    }
    if (node.type === 'gate' || node.type === 'state_checkpoint') {
      node.requiredChecks.forEach((condition, checkIndex) => {
        validateConditionSemantics(condition, `${nodePath}.requiredChecks.${checkIndex}`, nodeIds, producedArtifactRefs, artifactContractByRef, loopKeys, diagnostics)
      })
    }
  })

  diagnostics.push(...detectUnboundedCycles(graph.nodes, graph.startNodeId, outgoingEdges, `${pathPrefix}edges`))
  if (!subgraphContext) validateSubgraphInvocationCycles(definition, diagnostics)
  return diagnostics
}

const approvedGitStatusOptions = new Set([
  '--short', '-s', '--porcelain', '--porcelain=v1', '--untracked-files=no',
  '--untracked-files=normal', '--untracked-files=all', '-uno', '-unormal', '-uall', '-z',
  '--null',
])

function validateWorkflowCommandPolicy(
  node: Extract<WorkflowNodeDto, { type: 'command' }>,
  nodePath: string,
  diagnostics: WorkflowValidationDiagnosticDto[],
): void {
  if (!node.command.trim()) return
  if (node.command !== 'git') {
    diagnostics.push(error(
      'workflow_command_not_allowed_by_app_policy',
      `${nodePath}.command`,
      `Workflow command \`${node.command}\` is not approved by Xero's command policy. Command nodes currently support only a constrained \`git status\` operation.`,
    ))
    return
  }
  if (node.args[0] !== 'status') {
    diagnostics.push(error(
      'workflow_command_arguments_not_allowed_by_app_policy',
      `${nodePath}.args`,
      'Workflow command nodes currently support only the `git status` subcommand.',
    ))
    return
  }

  let afterPathSeparator = false
  for (const argument of node.args.slice(1)) {
    if (afterPathSeparator) {
      const pathSegments = argument.split('/')
      const invalidPath = !argument
        || argument.includes('\0')
        || argument.startsWith(':')
        || argument.startsWith('/')
        || argument.startsWith('\\')
        || argument.includes('\\')
        || /^[A-Za-z]:/.test(argument)
        || pathSegments.includes('..')
      if (invalidPath) {
        diagnostics.push(error(
          'workflow_command_arguments_not_allowed_by_app_policy',
          `${nodePath}.args`,
          `Workflow command pathspec \`${argument}\` must be a plain repo-relative path.`,
        ))
        return
      }
      continue
    }
    if (argument === '--') {
      afterPathSeparator = true
      continue
    }
    if (approvedGitStatusOptions.has(argument)) {
      continue
    }
    diagnostics.push(error(
      'workflow_command_arguments_not_allowed_by_app_policy',
      `${nodePath}.args`,
      `Workflow \`git status\` argument \`${argument}\` is outside Xero's read-only command policy. Put repo-relative pathspecs after \`--\`.`,
    ))
    return
  }
}

function producedArtifactTypeForNode(node: WorkflowNodeDto): string | null {
  switch (node.type) {
    case 'agent':
    case 'command':
    case 'subgraph':
      return node.outputContract.artifactType
    case 'state_read':
    case 'state_query':
      return node.outputArtifactType
    case 'state_write':
    case 'state_patch':
      return node.operation.outputArtifactType
    case 'collection_loop':
      return node.itemArtifactType
    default:
      return null
  }
}

function validateStateQuery(
  query: WorkflowStateQueryDto,
  path: string,
  diagnostics: WorkflowValidationDiagnosticDto[],
): void {
  query.filters.forEach((filter, index) => {
    if (!filter.path.trim().startsWith('$')) {
      diagnostics.push(error('state_query_filter_path_invalid', `${path}.filters.${index}.path`, 'State query filter paths must use a JSON path that starts with `$`.'))
    }
  })
  if (query.orderBy && !query.orderBy.trim().startsWith('$')) {
    diagnostics.push(error('state_query_order_path_invalid', `${path}.orderBy`, 'State query order paths must use a JSON path that starts with `$`.'))
  }
}

function validateStateWriteOperation(
  operation: WorkflowStateWriteOperationDto,
  path: string,
  diagnostics: WorkflowValidationDiagnosticDto[],
  requireOutputArtifact: boolean,
): void {
  if (requireOutputArtifact && !operation.outputArtifactType.trim()) {
    diagnostics.push(error('state_write_output_artifact_empty', `${path}.outputArtifactType`, 'State write nodes must name their output artifact.'))
  }
  if (operation.idempotencyKey !== null && operation.idempotencyKey !== undefined && !operation.idempotencyKey.trim()) {
    diagnostics.push(error('state_write_idempotency_key_empty', `${path}.idempotencyKey`, 'State write idempotency keys cannot be blank.'))
  }
  if (operation.targetId !== null && operation.targetId !== undefined && !operation.targetId.trim()) {
    diagnostics.push(error('state_write_target_id_empty', `${path}.targetId`, 'State write target ids cannot be blank.'))
  }
}

function defaultEdgeBuckets(type: WorkflowEdgeTypeDto): string[] {
  switch (type) {
    case 'success':
      return ['success']
    case 'failure':
    case 'recovery':
      return ['failure']
    case 'conditional':
    case 'loop':
    case 'manual_override':
      return ['all']
  }
}

function validateConditionShape(
  condition: WorkflowConditionDto,
  path: string,
  diagnostics: WorkflowValidationDiagnosticDto[],
): void {
  if (condition.kind === 'all' || condition.kind === 'any') {
    if (condition.conditions.length === 0) {
      diagnostics.push(error('condition_children_empty', path, 'Composite Workflow conditions must contain at least one child condition.'))
    }
    condition.conditions.forEach((child, index) => validateConditionShape(child, `${path}.conditions.${index}`, diagnostics))
    return
  }
  if (condition.kind === 'not') {
    validateConditionShape(condition.condition, `${path}.condition`, diagnostics)
    return
  }
  if (
    condition.kind === 'artifact_field_equals' ||
    condition.kind === 'artifact_field_in' ||
    condition.kind === 'artifact_field_number_compare' ||
    condition.kind === 'state_field_equals'
  ) {
    if (!condition.path.trim().startsWith('$')) {
      diagnostics.push(error('condition_json_path_invalid', path, 'Workflow field conditions must use a JSON path that starts with `$`.'))
    }
  }
}

function validateConditionSemantics(
  condition: WorkflowConditionDto,
  path: string,
  nodeIds: Set<string>,
  producedArtifactRefs: Set<string>,
  artifactContractByRef: Map<string, WorkflowArtifactContractDto>,
  loopKeys: Set<string>,
  diagnostics: WorkflowValidationDiagnosticDto[],
): void {
  validateConditionShape(condition, path, diagnostics)
  collectConditionArtifactRefs(condition).forEach((artifactRef) => {
    if (!producedArtifactRefs.has(artifactRef)) {
      diagnostics.push(error('condition_artifact_ref_missing', path, `Condition references missing artifact \`${artifactRef}\`.`))
    }
  })
  collectConditionArtifactFieldRefs(condition).forEach(({ artifactRef, path: jsonPath }) => {
    const contract = artifactContractByRef.get(artifactRef)
    if (contract?.jsonSchema && !jsonSchemaAllowsPath(contract.jsonSchema, jsonPath)) {
      diagnostics.push(error(
        'condition_artifact_path_not_in_schema',
        path,
        `Condition references \`${artifactRef}${jsonPath}\`, but that field is not allowed by the artifact schema.`,
      ))
    }
  })
  collectConditionNodeRefs(condition).forEach((nodeId) => {
    if (!nodeIds.has(nodeId)) {
      diagnostics.push(error('condition_node_ref_missing', path, `Condition references missing node \`${nodeId}\`.`))
    }
  })
  collectConditionStateRefs(condition).forEach((stateRef) => {
    if (!producedArtifactRefs.has(stateRef)) {
      diagnostics.push(error('condition_state_ref_missing', path, `Condition references missing state value \`${stateRef}\`.`))
    }
  })
  collectConditionLoopKeys(condition).forEach((loopKey) => {
    if (!loopKeys.has(loopKey)) {
      diagnostics.push(error('condition_loop_key_missing', path, `Condition references missing loop key \`${loopKey}\`.`))
    }
  })
}

function collectConditionArtifactFieldRefs(
  condition: WorkflowConditionDto,
): Array<{ artifactRef: string; path: string }> {
  switch (condition.kind) {
    case 'artifact_field_equals':
    case 'artifact_field_in':
    case 'artifact_field_number_compare':
      return [{ artifactRef: condition.artifactRef, path: condition.path }]
    case 'all':
    case 'any':
      return condition.conditions.flatMap(collectConditionArtifactFieldRefs)
    case 'not':
      return collectConditionArtifactFieldRefs(condition.condition)
    default:
      return []
  }
}

function jsonSchemaAllowsPath(schema: unknown, path: string): boolean {
  if (path === '$') return true
  if (!path.startsWith('$.')) return false
  let cursor: unknown = schema
  for (const rawSegment of path.slice(2).split('.')) {
    const field = rawSegment.replace(/\[\d+\]/g, '')
    if (!isRecord(cursor)) return false
    const schemaType = cursor.type
    if (schemaType && !schemaTypeAllowsObject(schemaType)) return false
    const properties = isRecord(cursor.properties) ? cursor.properties : null
    if (!properties || !(field in properties)) return false
    cursor = properties[field]
    const arrayIndexes = rawSegment.match(/\[\d+\]/g) ?? []
    for (const _index of arrayIndexes) {
      if (!isRecord(cursor)) return false
      const itemSchema = cursor.items
      if (!itemSchema) return false
      cursor = itemSchema
    }
  }
  return true
}

function schemaTypeAllowsObject(type: unknown): boolean {
  if (typeof type === 'string') return type === 'object'
  return Array.isArray(type) ? type.includes('object') : true
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function detectUnboundedCycles(
  nodes: WorkflowNodeDto[],
  startNodeId: string,
  outgoingEdges: Map<string, WorkflowEdgeDto[]>,
  edgesPath: string,
): WorkflowValidationDiagnosticDto[] {
  const diagnostics: WorkflowValidationDiagnosticDto[] = []
  const visiting = new Set<string>()
  const visited = new Set<string>()
  const edgeStack: WorkflowEdgeDto[] = []

  const visit = (nodeId: string) => {
    if (visiting.has(nodeId)) {
      const cycleStart = edgeStack.findIndex((edge) => edge.fromNodeId === nodeId)
      const cycle = cycleStart >= 0 ? edgeStack.slice(cycleStart) : edgeStack
      const hasBoundedLoop = cycle.some((edge) => edge.type === 'loop' && edge.loopPolicy)
      if (!hasBoundedLoop) {
        diagnostics.push(error('cycle_without_loop_policy', edgesPath, `Cycle \`${cycle.map((edge) => edge.id).join(' -> ')}\` must include an explicit bounded loop edge.`))
      }
      return
    }
    if (visited.has(nodeId)) return
    visiting.add(nodeId)
    for (const edge of outgoingEdges.get(nodeId) ?? []) {
      edgeStack.push(edge)
      visit(edge.toNodeId)
      edgeStack.pop()
    }
    visiting.delete(nodeId)
    visited.add(nodeId)
  }

  if (nodes.some((node) => node.id === startNodeId)) {
    visit(startNodeId)
  }
  return diagnostics
}

function collectConditionArtifactRefs(condition: WorkflowConditionDto): string[] {
  switch (condition.kind) {
    case 'artifact_exists':
    case 'artifact_field_equals':
    case 'artifact_field_in':
    case 'artifact_field_number_compare':
      return [condition.artifactRef]
    case 'all':
    case 'any':
      return condition.conditions.flatMap(collectConditionArtifactRefs)
    case 'not':
      return collectConditionArtifactRefs(condition.condition)
    default:
      return []
  }
}

function collectConditionStateRefs(condition: WorkflowConditionDto): string[] {
  switch (condition.kind) {
    case 'state_field_equals':
    case 'state_collection_count_compare':
      return [condition.stateRef]
    case 'all':
    case 'any':
      return condition.conditions.flatMap(collectConditionStateRefs)
    case 'not':
      return collectConditionStateRefs(condition.condition)
    default:
      return []
  }
}

function collectConditionNodeRefs(condition: WorkflowConditionDto): string[] {
  switch (condition.kind) {
    case 'node_status':
      return [condition.nodeId]
    case 'failure_class_is':
      return condition.nodeId ? [condition.nodeId] : []
    case 'human_decision_is':
      return [condition.checkpointNodeId]
    case 'all':
    case 'any':
      return condition.conditions.flatMap(collectConditionNodeRefs)
    case 'not':
      return collectConditionNodeRefs(condition.condition)
    default:
      return []
  }
}

function collectConditionLoopKeys(condition: WorkflowConditionDto): string[] {
  switch (condition.kind) {
    case 'loop_attempt_lt':
    case 'loop_attempt_gte':
      return [condition.loopKey]
    case 'all':
    case 'any':
      return condition.conditions.flatMap(collectConditionLoopKeys)
    case 'not':
      return collectConditionLoopKeys(condition.condition)
    default:
      return []
  }
}

function validateSubgraphInvocationCycles(
  definition: WorkflowDefinitionDto,
  diagnostics: WorkflowValidationDiagnosticDto[],
): void {
  definition.subgraphs.forEach((subgraph, subgraphIndex) => {
    subgraph.nodes.forEach((node, nodeIndex) => {
      if (node.type !== 'subgraph') return
      if (!definition.subgraphs.some((candidate) => candidate.id === node.subgraphId)) return
      if (subgraphInvokesTarget(definition, node.subgraphId, subgraph.id, new Set())) {
        diagnostics.push(error(
          'recursive_subgraph_invocation',
          `subgraphs.${subgraphIndex}.nodes.${nodeIndex}.subgraphId`,
          `Subgraph \`${subgraph.id}\` recursively invokes \`${node.subgraphId}\`; recursive subgraph invocation is unsupported.`,
        ))
      }
    })
  })
}

function subgraphInvokesTarget(
  definition: WorkflowDefinitionDto,
  currentId: string,
  targetId: string,
  visiting: Set<string>,
): boolean {
  if (currentId === targetId) return true
  if (visiting.has(currentId)) return false
  visiting.add(currentId)
  const subgraph = definition.subgraphs.find((candidate) => candidate.id === currentId)
  const reachesTarget = subgraph?.nodes.some((node) =>
    node.type === 'subgraph' && subgraphInvokesTarget(definition, node.subgraphId, targetId, visiting)) ?? false
  visiting.delete(currentId)
  return reachesTarget
}

function error(
  code: string,
  path: string,
  message: string,
): WorkflowValidationDiagnosticDto {
  return { severity: 'error', code, path, message }
}
