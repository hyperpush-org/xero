import type { PhaseStatus } from '@/components/cadence/data'
import { z } from 'zod'
import { phaseSummarySchema, type PhaseSummaryDto } from './project'
import {
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  phaseStatusSchema,
  phaseStepSchema,
  safePercent,
} from './shared'

const PLANNING_LIFECYCLE_STAGES = ['discussion', 'research', 'requirements', 'roadmap'] as const
const PLANNING_LIFECYCLE_STAGE_LABELS: Record<PlanningLifecycleStageKindDto, string> = {
  discussion: 'Discussion',
  research: 'Research',
  requirements: 'Requirements',
  roadmap: 'Roadmap',
}

export const planningLifecycleStageKindSchema = z.enum(PLANNING_LIFECYCLE_STAGES)

export const planningLifecycleStageSchema = z
  .object({
    stage: planningLifecycleStageKindSchema,
    nodeId: z.string().trim().min(1),
    status: phaseStatusSchema,
    actionRequired: z.boolean(),
    unblockReason: nonEmptyOptionalTextSchema,
    unblockGateKey: nonEmptyOptionalTextSchema,
    unblockActionId: nonEmptyOptionalTextSchema,
    lastTransitionAt: isoTimestampSchema.nullable().optional(),
  })
  .strict()

export const planningLifecycleProjectionSchema = z
  .object({
    stages: z.array(planningLifecycleStageSchema),
  })
  .strict()
  .superRefine((projection, ctx) => {
    const seenStages = new Set<(typeof PLANNING_LIFECYCLE_STAGES)[number]>()

    projection.stages.forEach((stage, index) => {
      if (seenStages.has(stage.stage)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['stages', index, 'stage'],
          message: `Duplicate lifecycle stage \`${stage.stage}\` is not allowed.`,
        })
        return
      }

      seenStages.add(stage.stage)
    })
  })

export const workflowHandoffPackageSchema = z
  .object({
    id: z.number().int().nonnegative(),
    projectId: z.string().trim().min(1),
    handoffTransitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    packagePayload: z.string().trim().min(1),
    packageHash: z.string().trim().min(1),
    createdAt: isoTimestampSchema,
  })
  .strict()

export const workflowGateStateSchema = z.enum(['pending', 'satisfied', 'blocked', 'skipped'])
export const workflowTransitionGateDecisionSchema = z.enum(['approved', 'rejected', 'blocked', 'not_applicable'])

export const workflowGraphNodeSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    phaseId: z.number().int().nonnegative(),
    sortOrder: z.number().int().nonnegative(),
    name: z.string().trim().min(1),
    description: z.string(),
    status: phaseStatusSchema,
    currentStep: phaseStepSchema.nullable().optional(),
    taskCount: z.number().int().nonnegative(),
    completedTasks: z.number().int().nonnegative(),
    summary: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphEdgeSchema = z
  .object({
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateRequirement: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphGateRequestSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    actionType: nonEmptyOptionalTextSchema,
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const workflowGraphGateMetadataSchema = z
  .object({
    nodeId: z.string().trim().min(1),
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    actionType: nonEmptyOptionalTextSchema,
    title: nonEmptyOptionalTextSchema,
    detail: nonEmptyOptionalTextSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const upsertWorkflowGraphRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    nodes: z.array(workflowGraphNodeSchema),
    edges: z.array(workflowGraphEdgeSchema),
    gates: z.array(workflowGraphGateRequestSchema),
  })
  .strict()

export const upsertWorkflowGraphResponseSchema = z
  .object({
    nodes: z.array(workflowGraphNodeSchema),
    edges: z.array(workflowGraphEdgeSchema),
    gates: z.array(workflowGraphGateMetadataSchema),
    phases: z.array(phaseSummarySchema),
  })
  .strict()

export const workflowTransitionGateUpdateRequestSchema = z
  .object({
    gateKey: z.string().trim().min(1),
    gateState: workflowGateStateSchema,
    decisionContext: nonEmptyOptionalTextSchema,
  })
  .strict()

export const applyWorkflowTransitionRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateDecision: workflowTransitionGateDecisionSchema,
    gateDecisionContext: nonEmptyOptionalTextSchema,
    gateUpdates: z.array(workflowTransitionGateUpdateRequestSchema),
    occurredAt: isoTimestampSchema,
  })
  .strict()

export const workflowTransitionEventSchema = z
  .object({
    id: z.number().int().nonnegative(),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    fromNodeId: z.string().trim().min(1),
    toNodeId: z.string().trim().min(1),
    transitionKind: z.string().trim().min(1),
    gateDecision: workflowTransitionGateDecisionSchema,
    gateDecisionContext: nonEmptyOptionalTextSchema,
    createdAt: isoTimestampSchema,
  })
  .strict()

export const workflowAutomaticDispatchStatusSchema = z.enum(['no_continuation', 'applied', 'replayed', 'skipped'])
export const workflowAutomaticDispatchPackageStatusSchema = z.enum(['persisted', 'replayed', 'skipped'])

export const workflowAutomaticDispatchPackageOutcomeSchema = z
  .object({
    status: workflowAutomaticDispatchPackageStatusSchema,
    package: workflowHandoffPackageSchema.nullable().optional(),
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((outcome, ctx) => {
    if (outcome.status === 'skipped') {
      if (!outcome.code || !outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['code'],
          message: 'Skipped handoff package outcomes must include non-empty `code` and `message` diagnostics.',
        })
      }
      if (outcome.package) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['package'],
          message: 'Skipped handoff package outcomes must not include a persisted package payload.',
        })
      }
      return
    }

    if (!outcome.package) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['package'],
        message: 'Persisted/replayed handoff package outcomes must include a package payload.',
      })
    }

    if (outcome.code || outcome.message) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['code'],
        message: 'Persisted/replayed handoff package outcomes must not include skip diagnostics.',
      })
    }
  })

export const workflowAutomaticDispatchOutcomeSchema = z
  .object({
    status: workflowAutomaticDispatchStatusSchema,
    transitionEvent: workflowTransitionEventSchema.nullable().optional(),
    handoffPackage: workflowAutomaticDispatchPackageOutcomeSchema.nullable().optional(),
    code: nonEmptyOptionalTextSchema,
    message: nonEmptyOptionalTextSchema,
  })
  .strict()
  .superRefine((outcome, ctx) => {
    if (outcome.status === 'no_continuation') {
      if (outcome.transitionEvent || outcome.handoffPackage || outcome.code || outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['status'],
          message: 'No-continuation automatic dispatch outcomes cannot include transition, package, or diagnostic payloads.',
        })
      }
      return
    }

    if (outcome.status === 'skipped') {
      if (!outcome.code || !outcome.message) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['code'],
          message: 'Skipped automatic dispatch outcomes must include non-empty `code` and `message` diagnostics.',
        })
      }

      if (outcome.transitionEvent || outcome.handoffPackage) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['transitionEvent'],
          message: 'Skipped automatic dispatch outcomes must not include transition or handoff payloads.',
        })
      }
      return
    }

    if (!outcome.transitionEvent || !outcome.handoffPackage) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['transitionEvent'],
        message: 'Applied/replayed automatic dispatch outcomes must include transition and handoff payloads.',
      })
    }

    if (outcome.code || outcome.message) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['code'],
        message: 'Applied/replayed automatic dispatch outcomes must not include skip diagnostics.',
      })
    }
  })

export const applyWorkflowTransitionResponseSchema = z
  .object({
    transitionEvent: workflowTransitionEventSchema,
    automaticDispatch: workflowAutomaticDispatchOutcomeSchema.optional(),
    phases: z.array(phaseSummarySchema),
  })
  .strict()

export type PlanningLifecycleStageKindDto = z.infer<typeof planningLifecycleStageKindSchema>
export type PlanningLifecycleStageDto = z.infer<typeof planningLifecycleStageSchema>
export type PlanningLifecycleProjectionDto = z.infer<typeof planningLifecycleProjectionSchema>
export type WorkflowHandoffPackageDto = z.infer<typeof workflowHandoffPackageSchema>
export type WorkflowGateStateDto = z.infer<typeof workflowGateStateSchema>
export type WorkflowTransitionGateDecisionDto = z.infer<typeof workflowTransitionGateDecisionSchema>
export type WorkflowAutomaticDispatchStatusDto = z.infer<typeof workflowAutomaticDispatchStatusSchema>
export type WorkflowAutomaticDispatchPackageStatusDto = z.infer<typeof workflowAutomaticDispatchPackageStatusSchema>
export type WorkflowAutomaticDispatchPackageOutcomeDto = z.infer<typeof workflowAutomaticDispatchPackageOutcomeSchema>
export type WorkflowAutomaticDispatchOutcomeDto = z.infer<typeof workflowAutomaticDispatchOutcomeSchema>
export type WorkflowGraphNodeDto = z.infer<typeof workflowGraphNodeSchema>
export type WorkflowGraphEdgeDto = z.infer<typeof workflowGraphEdgeSchema>
export type WorkflowGraphGateRequestDto = z.infer<typeof workflowGraphGateRequestSchema>
export type WorkflowGraphGateMetadataDto = z.infer<typeof workflowGraphGateMetadataSchema>
export type UpsertWorkflowGraphRequestDto = z.infer<typeof upsertWorkflowGraphRequestSchema>
export type UpsertWorkflowGraphResponseDto = z.infer<typeof upsertWorkflowGraphResponseSchema>
export type WorkflowTransitionGateUpdateRequestDto = z.infer<typeof workflowTransitionGateUpdateRequestSchema>
export type ApplyWorkflowTransitionRequestDto = z.infer<typeof applyWorkflowTransitionRequestSchema>
export type WorkflowTransitionEventDto = z.infer<typeof workflowTransitionEventSchema>
export type ApplyWorkflowTransitionResponseDto = z.infer<typeof applyWorkflowTransitionResponseSchema>

export interface WorkflowHandoffPackageView {
  id: number
  projectId: string
  handoffTransitionId: string
  causalTransitionId: string | null
  fromNodeId: string
  toNodeId: string
  transitionKind: string
  packagePayload: string
  packageHash: string
  createdAt: string
}

export interface PlanningLifecycleUnblockReasonView {
  reason: string
  gateKey: string
  actionId: string | null
}

export interface PlanningLifecycleStageView {
  stage: PlanningLifecycleStageKindDto
  stageLabel: string
  nodeId: string
  nodeLabel: string
  status: PhaseStatus
  statusLabel: string
  actionRequired: boolean
  unblock: PlanningLifecycleUnblockReasonView | null
  lastTransitionAt: string | null
}

export interface PlanningLifecycleView {
  stages: PlanningLifecycleStageView[]
  byStage: Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView | null>
  hasStages: boolean
  activeStage: PlanningLifecycleStageView | null
  actionRequiredCount: number
  blockedCount: number
  completedCount: number
  percentComplete: number
}

function getPhaseStatusLabel(status: PhaseStatus): string {
  switch (status) {
    case 'complete':
      return 'Complete'
    case 'active':
      return 'Active'
    case 'pending':
      return 'Pending'
    case 'blocked':
      return 'Blocked'
  }
}

export function humanizeNodeId(nodeId: string): string {
  return nodeId
    .split(/[_\-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function createPlanningLifecycleUnblockReason(stage: PlanningLifecycleStageDto): PlanningLifecycleUnblockReasonView | null {
  if (!stage.actionRequired) {
    return null
  }

  const reason = normalizeOptionalText(stage.unblockReason)
  const gateKey = normalizeOptionalText(stage.unblockGateKey)
  const actionId = normalizeOptionalText(stage.unblockActionId)

  if (!reason || !gateKey) {
    return null
  }

  return {
    reason,
    gateKey,
    actionId,
  }
}

function createEmptyPlanningLifecycleByStage(): Record<PlanningLifecycleStageKindDto, PlanningLifecycleStageView | null> {
  return {
    discussion: null,
    research: null,
    requirements: null,
    roadmap: null,
  }
}

export function createEmptyPlanningLifecycle(): PlanningLifecycleView {
  return {
    stages: [],
    byStage: createEmptyPlanningLifecycleByStage(),
    hasStages: false,
    activeStage: null,
    actionRequiredCount: 0,
    blockedCount: 0,
    completedCount: 0,
    percentComplete: 0,
  }
}

export function mapPlanningLifecycle(projection: PlanningLifecycleProjectionDto): PlanningLifecycleView {
  const byStage = createEmptyPlanningLifecycleByStage()
  const stageByKind = new Map(projection.stages.map((stage) => [stage.stage, stage]))
  const stages: PlanningLifecycleStageView[] = []

  PLANNING_LIFECYCLE_STAGES.forEach((stageKind) => {
    const stage = stageByKind.get(stageKind)
    if (!stage) {
      return
    }

    const mappedStage: PlanningLifecycleStageView = {
      stage: stage.stage,
      stageLabel: PLANNING_LIFECYCLE_STAGE_LABELS[stage.stage],
      nodeId: stage.nodeId,
      nodeLabel: humanizeNodeId(stage.nodeId),
      status: stage.status,
      statusLabel: getPhaseStatusLabel(stage.status),
      actionRequired: stage.actionRequired,
      unblock: createPlanningLifecycleUnblockReason(stage),
      lastTransitionAt: normalizeOptionalText(stage.lastTransitionAt),
    }

    byStage[stage.stage] = mappedStage
    stages.push(mappedStage)
  })

  const completedCount = stages.filter((stage) => stage.status === 'complete').length
  const blockedCount = stages.filter((stage) => stage.status === 'blocked').length
  const actionRequiredCount = stages.filter((stage) => stage.actionRequired).length

  return {
    stages,
    byStage,
    hasStages: stages.length > 0,
    activeStage: stages.find((stage) => stage.status === 'active') ?? null,
    actionRequiredCount,
    blockedCount,
    completedCount,
    percentComplete: safePercent(completedCount, stages.length),
  }
}

export function mapWorkflowHandoffPackage(pkg: WorkflowHandoffPackageDto): WorkflowHandoffPackageView {
  return {
    id: pkg.id,
    projectId: pkg.projectId,
    handoffTransitionId: pkg.handoffTransitionId,
    causalTransitionId: normalizeOptionalText(pkg.causalTransitionId),
    fromNodeId: pkg.fromNodeId,
    toNodeId: pkg.toNodeId,
    transitionKind: pkg.transitionKind,
    packagePayload: pkg.packagePayload,
    packageHash: pkg.packageHash,
    createdAt: pkg.createdAt,
  }
}
