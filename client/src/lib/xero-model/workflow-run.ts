import { z } from 'zod'

import { isoTimestampSchema } from '@xero/ui/model/shared'
import {
  workflowArtifactTypeSchema,
  workflowConditionSchema,
  workflowDefinitionSchema,
  workflowEdgeIdSchema,
  workflowHumanCheckpointTypeSchema,
  workflowNodeIdSchema,
  workflowNodeRunStatusSchema,
  workflowRunStatusSchema,
  workflowStateQuerySchema,
  workflowStateWriteOperationSchema,
  workflowTerminalStatusSchema,
  type WorkflowDefinitionDto,
} from './workflow-definition'

const nonEmptyTextSchema = z.string().trim().min(1)

export const workflowArtifactRecordSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    producerNodeRunId: nonEmptyTextSchema,
    artifactType: workflowArtifactTypeSchema,
    schemaVersion: z.number().int().positive(),
    payload: z.unknown(),
    renderText: z.string().nullable().optional(),
    createdAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowArtifactRecordDto = z.infer<typeof workflowArtifactRecordSchema>

export const workflowRunNodeSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    nodeId: workflowNodeIdSchema,
    nodeType: nonEmptyTextSchema,
    status: workflowNodeRunStatusSchema,
    attemptNumber: z.number().int().nonnegative(),
    runtimeRunId: z.string().trim().min(1).nullable().optional(),
    agentSessionId: z.string().trim().min(1).nullable().optional(),
    failureClass: z.string().trim().min(1).nullable().optional(),
    startedAt: isoTimestampSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
    completedAt: isoTimestampSchema.nullable().optional(),
    idempotencyKey: nonEmptyTextSchema,
  })
  .strict()
export type WorkflowRunNodeDto = z.infer<typeof workflowRunNodeSchema>

export const workflowRunEdgeDecisionSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    fromNodeId: workflowNodeIdSchema,
    toNodeId: workflowNodeIdSchema,
    edgeId: workflowEdgeIdSchema,
    matched: z.boolean(),
    condition: workflowConditionSchema,
    evidence: z.unknown(),
    createdAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowRunEdgeDecisionDto = z.infer<
  typeof workflowRunEdgeDecisionSchema
>

export const workflowLoopAttemptSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    loopKey: nonEmptyTextSchema,
    attemptCount: z.number().int().nonnegative(),
    lastNodeRunId: z.string().trim().min(1).nullable().optional(),
    exhausted: z.boolean(),
    updatedAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowLoopAttemptDto = z.infer<typeof workflowLoopAttemptSchema>

export const workflowGateDecisionSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    nodeRunId: nonEmptyTextSchema,
    checkpointType: workflowHumanCheckpointTypeSchema,
    decision: nonEmptyTextSchema,
    decisionPayload: z.unknown().nullable().optional(),
    decidedAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowGateDecisionDto = z.infer<typeof workflowGateDecisionSchema>

export const workflowEventSchema = z
  .object({
    id: nonEmptyTextSchema,
    workflowRunId: nonEmptyTextSchema,
    nodeRunId: z.string().trim().min(1).nullable().optional(),
    eventType: nonEmptyTextSchema,
    event: z.unknown(),
    createdAt: isoTimestampSchema,
  })
  .strict()
export type WorkflowEventDto = z.infer<typeof workflowEventSchema>

export const workflowRunSchema = z
  .object({
    id: nonEmptyTextSchema,
    projectId: nonEmptyTextSchema,
    workflowVersionId: nonEmptyTextSchema,
    workflowId: nonEmptyTextSchema,
    workflowVersionNumber: z.number().int().positive(),
    status: workflowRunStatusSchema,
    terminalStatus: workflowTerminalStatusSchema.nullable().optional(),
    definitionSnapshot: workflowDefinitionSchema,
    initialInput: z.unknown().nullable().optional(),
    startedAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
    completedAt: isoTimestampSchema.nullable().optional(),
    cancellationReason: z.string().nullable().optional(),
    nodes: z.array(workflowRunNodeSchema),
    edgeDecisions: z.array(workflowRunEdgeDecisionSchema),
    artifacts: z.array(workflowArtifactRecordSchema),
    gateDecisions: z.array(workflowGateDecisionSchema),
    loopAttempts: z.array(workflowLoopAttemptSchema),
    events: z.array(workflowEventSchema),
  })
  .strict()
export type WorkflowRunDto = z.infer<typeof workflowRunSchema>

export const createWorkflowDefinitionRequestSchema = z
  .object({
    definition: workflowDefinitionSchema,
  })
  .strict()
export type CreateWorkflowDefinitionRequestDto = z.infer<
  typeof createWorkflowDefinitionRequestSchema
>

export const updateWorkflowDefinitionRequestSchema = z
  .object({
    workflowId: nonEmptyTextSchema,
    expectedVersion: z.number().int().positive(),
    definition: workflowDefinitionSchema,
  })
  .strict()
  .superRefine((request, context) => {
    if (request.definition.version !== request.expectedVersion) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['expectedVersion'],
        message: 'expectedVersion must match definition.version.',
      })
    }
  })
export type UpdateWorkflowDefinitionRequestDto = z.infer<
  typeof updateWorkflowDefinitionRequestSchema
>

export const getWorkflowDefinitionRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    workflowId: nonEmptyTextSchema,
  })
  .strict()
export type GetWorkflowDefinitionRequestDto = z.infer<
  typeof getWorkflowDefinitionRequestSchema
>

export const listWorkflowDefinitionsRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
  })
  .strict()
export type ListWorkflowDefinitionsRequestDto = z.infer<
  typeof listWorkflowDefinitionsRequestSchema
>

export const listWorkflowDefinitionsResponseSchema = z
  .object({
    definitions: z.array(
      z
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
        .strict(),
    ),
  })
  .strict()
export type ListWorkflowDefinitionsResponseDto = z.infer<
  typeof listWorkflowDefinitionsResponseSchema
>

export const startWorkflowRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    workflowId: nonEmptyTextSchema,
    idempotencyKey: nonEmptyTextSchema.max(200),
    initialInput: z.unknown().nullable().optional(),
  })
  .strict()
export type StartWorkflowRunRequestDto = z.infer<typeof startWorkflowRunRequestSchema>

export const getWorkflowRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
  })
  .strict()
export type GetWorkflowRunRequestDto = z.infer<typeof getWorkflowRunRequestSchema>

export const explainWorkflowRunBlockerRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
  })
  .strict()
export type ExplainWorkflowRunBlockerRequestDto = z.infer<
  typeof explainWorkflowRunBlockerRequestSchema
>

export const exportWorkflowRunBundleRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
  })
  .strict()
export type ExportWorkflowRunBundleRequestDto = z.infer<
  typeof exportWorkflowRunBundleRequestSchema
>

export const resumeWorkflowNextIncompletePhaseRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    idempotencyKey: nonEmptyTextSchema.max(200),
  })
  .strict()
export type ResumeWorkflowNextIncompletePhaseRequestDto = z.infer<
  typeof resumeWorkflowNextIncompletePhaseRequestSchema
>

export const listWorkflowRunsRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    workflowId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type ListWorkflowRunsRequestDto = z.infer<typeof listWorkflowRunsRequestSchema>

export const listWorkflowRunsResponseSchema = z
  .object({
    runs: z.array(workflowRunSchema),
  })
  .strict()
export type ListWorkflowRunsResponseDto = z.infer<
  typeof listWorkflowRunsResponseSchema
>

export const cancelWorkflowRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    reason: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type CancelWorkflowRunRequestDto = z.infer<typeof cancelWorkflowRunRequestSchema>

export const retryWorkflowNodeRunRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    nodeRunId: nonEmptyTextSchema,
  })
  .strict()
export type RetryWorkflowNodeRunRequestDto = z.infer<
  typeof retryWorkflowNodeRunRequestSchema
>

export const skipWorkflowBranchRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    nodeRunId: nonEmptyTextSchema,
    reason: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type SkipWorkflowBranchRequestDto = z.infer<typeof skipWorkflowBranchRequestSchema>

export const resumeWorkflowCheckpointRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    runId: nonEmptyTextSchema,
    nodeRunId: nonEmptyTextSchema,
    decision: nonEmptyTextSchema,
    payload: z.unknown().nullable().optional(),
  })
  .strict()
export type ResumeWorkflowCheckpointRequestDto = z.infer<
  typeof resumeWorkflowCheckpointRequestSchema
>

export const readWorkflowDeliveryStateRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    query: workflowStateQuerySchema,
  })
  .strict()
export type ReadWorkflowDeliveryStateRequestDto = z.infer<
  typeof readWorkflowDeliveryStateRequestSchema
>

export const writeWorkflowDeliveryStateRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    operation: workflowStateWriteOperationSchema,
  })
  .strict()
export type WriteWorkflowDeliveryStateRequestDto = z.infer<
  typeof writeWorkflowDeliveryStateRequestSchema
>

export const exportWorkflowDeliveryStateRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
  })
  .strict()
export type ExportWorkflowDeliveryStateRequestDto = z.infer<
  typeof exportWorkflowDeliveryStateRequestSchema
>

export const wipeWorkflowDeliveryStateRequestSchema = z
  .object({
    projectId: nonEmptyTextSchema,
  })
  .strict()
export type WipeWorkflowDeliveryStateRequestDto = z.infer<
  typeof wipeWorkflowDeliveryStateRequestSchema
>

export const workflowDeliveryStateResponseSchema = z
  .object({
    state: z.unknown(),
  })
  .strict()
export type WorkflowDeliveryStateResponseDto = z.infer<
  typeof workflowDeliveryStateResponseSchema
>

export const workflowDefinitionResponseSchema = z
  .object({
    definition: workflowDefinitionSchema,
  })
  .strict()
export type WorkflowDefinitionResponseDto = z.infer<
  typeof workflowDefinitionResponseSchema
>

export const workflowRunResponseSchema = z
  .object({
    run: workflowRunSchema,
  })
  .strict()
export type WorkflowRunResponseDto = z.infer<typeof workflowRunResponseSchema>

export const workflowRunUpdatedPayloadSchema = z
  .object({
    projectId: nonEmptyTextSchema,
    run: workflowRunSchema,
  })
  .strict()
export type WorkflowRunUpdatedPayloadDto = z.infer<
  typeof workflowRunUpdatedPayloadSchema
>

export const workflowRunBlockerResponseSchema = z
  .object({
    status: nonEmptyTextSchema,
    summary: nonEmptyTextSchema,
    nodeId: z.string().trim().min(1).nullable().optional(),
    nodeRunId: z.string().trim().min(1).nullable().optional(),
    failureClass: z.string().trim().min(1).nullable().optional(),
    event: z.unknown().nullable().optional(),
  })
  .strict()
export type WorkflowRunBlockerResponseDto = z.infer<
  typeof workflowRunBlockerResponseSchema
>

export const workflowRunBundleResponseSchema = z
  .object({
    bundle: z.unknown(),
  })
  .strict()
export type WorkflowRunBundleResponseDto = z.infer<
  typeof workflowRunBundleResponseSchema
>

export type WorkflowDefinitionSnapshotDto = WorkflowDefinitionDto
