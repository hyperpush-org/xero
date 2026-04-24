
import { z } from 'zod'
import type { OperatorApprovalView } from './operator-actions'
import {
  humanizeNodeId as humanizeWorkflowNodeId,
  type PlanningLifecycleStageView,
  type PlanningLifecycleView,
  type WorkflowHandoffPackageView,
} from './workflow'
import {
  humanizeSegmentedLabel as humanizeRuntimeKind,
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
  sortByNewest,
  toolResultSummarySchema,
  type ToolResultSummaryDto,
} from './shared'
import {
  runtimeRunDiagnosticSchema,
  type RuntimeRunDiagnosticDto,
} from './runtime'

export const autonomousRunStatusSchema = z.enum([
  'starting',
  'running',
  'paused',
  'cancelling',
  'cancelled',
  'stale',
  'failed',
  'stopped',
  'crashed',
  'completed',
])
export const autonomousRunRecoveryStateSchema = z.enum(['healthy', 'recovery_required', 'terminal', 'failed'])
export const autonomousUnitKindSchema = z.enum(['researcher', 'planner', 'executor', 'verifier'])
export const autonomousUnitStatusSchema = z.enum([
  'pending',
  'active',
  'blocked',
  'paused',
  'completed',
  'cancelled',
  'failed',
])
export const autonomousUnitArtifactStatusSchema = z.enum(['pending', 'recorded', 'rejected', 'redacted'])
export const autonomousToolCallStateSchema = z.enum(['pending', 'running', 'succeeded', 'failed'])
export const autonomousVerificationOutcomeSchema = z.enum(['passed', 'failed', 'blocked'])

export const autonomousLifecycleReasonSchema = z
  .object({
    code: z.string().trim().min(1),
    message: z.string().trim().min(1),
  })
  .strict()

export const autonomousCommandResultSchema = z
  .object({
    exitCode: z.number().int().nullable().optional(),
    timedOut: z.boolean(),
    summary: z.string().trim().min(1),
  })
  .strict()

export const autonomousToolResultPayloadSchema = z
  .object({
    kind: z.literal('tool_result'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    toolCallId: z.string().trim().min(1),
    toolName: z.string().trim().min(1),
    toolState: autonomousToolCallStateSchema,
    commandResult: autonomousCommandResultSchema.nullable().optional(),
    toolSummary: toolResultSummarySchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousVerificationEvidencePayloadSchema = z
  .object({
    kind: z.literal('verification_evidence'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    evidenceKind: z.string().trim().min(1),
    label: z.string().trim().min(1),
    outcome: autonomousVerificationOutcomeSchema,
    commandResult: autonomousCommandResultSchema.nullable().optional(),
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousPolicyDeniedPayloadSchema = z
  .object({
    kind: z.literal('policy_denied'),
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    diagnosticCode: z.string().trim().min(1),
    message: z.string().trim().min(1),
    toolName: nonEmptyOptionalTextSchema,
    actionId: nonEmptyOptionalTextSchema,
    boundaryId: nonEmptyOptionalTextSchema,
  })
  .strict()

export const autonomousArtifactPayloadSchema = z.discriminatedUnion('kind', [
  autonomousToolResultPayloadSchema,
  autonomousVerificationEvidencePayloadSchema,
  autonomousPolicyDeniedPayloadSchema,
])

export const autonomousRunSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    runtimeKind: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    supervisorKind: z.string().trim().min(1),
    status: autonomousRunStatusSchema,
    recoveryState: autonomousRunRecoveryStateSchema,
    activeUnitId: nonEmptyOptionalTextSchema,
    activeAttemptId: nonEmptyOptionalTextSchema,
    duplicateStartDetected: z.boolean(),
    duplicateStartRunId: nonEmptyOptionalTextSchema,
    duplicateStartReason: nonEmptyOptionalTextSchema,
    startedAt: isoTimestampSchema,
    lastHeartbeatAt: nonEmptyOptionalTextSchema,
    lastCheckpointAt: nonEmptyOptionalTextSchema,
    pausedAt: nonEmptyOptionalTextSchema,
    cancelledAt: nonEmptyOptionalTextSchema,
    completedAt: nonEmptyOptionalTextSchema,
    crashedAt: nonEmptyOptionalTextSchema,
    stoppedAt: nonEmptyOptionalTextSchema,
    pauseReason: autonomousLifecycleReasonSchema.nullable().optional(),
    cancelReason: autonomousLifecycleReasonSchema.nullable().optional(),
    crashReason: autonomousLifecycleReasonSchema.nullable().optional(),
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
    updatedAt: isoTimestampSchema,
  })
  .strict()

export const autonomousWorkflowLinkageSchema = z
  .object({
    workflowNodeId: z.string().trim().min(1),
    transitionId: z.string().trim().min(1),
    causalTransitionId: nonEmptyOptionalTextSchema,
    handoffTransitionId: z.string().trim().min(1),
    handoffPackageHash: z
      .string()
      .regex(/^[0-9a-f]{64}$/, 'Autonomous workflow linkage handoff package hashes must be lowercase 64-character hex digests.'),
  })
  .strict()

export const autonomousUnitSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    sequence: z.number().int().nonnegative(),
    kind: autonomousUnitKindSchema,
    status: autonomousUnitStatusSchema,
    summary: z.string().trim().min(1),
    boundaryId: nonEmptyOptionalTextSchema,
    workflowLinkage: autonomousWorkflowLinkageSchema.nullable().optional(),
    startedAt: isoTimestampSchema,
    finishedAt: nonEmptyOptionalTextSchema,
    updatedAt: isoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
  })
  .strict()

export const autonomousUnitAttemptSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    attemptNumber: z.number().int().nonnegative(),
    childSessionId: z.string().trim().min(1),
    status: autonomousUnitStatusSchema,
    boundaryId: nonEmptyOptionalTextSchema,
    workflowLinkage: autonomousWorkflowLinkageSchema.nullable().optional(),
    startedAt: isoTimestampSchema,
    finishedAt: nonEmptyOptionalTextSchema,
    updatedAt: isoTimestampSchema,
    lastErrorCode: nonEmptyOptionalTextSchema,
    lastError: runtimeRunDiagnosticSchema.nullable().optional(),
  })
  .strict()

export const autonomousUnitArtifactSchema = z
  .object({
    projectId: z.string().trim().min(1),
    runId: z.string().trim().min(1),
    unitId: z.string().trim().min(1),
    attemptId: z.string().trim().min(1),
    artifactId: z.string().trim().min(1),
    artifactKind: z.string().trim().min(1),
    status: autonomousUnitArtifactStatusSchema,
    summary: z.string().trim().min(1),
    contentHash: nonEmptyOptionalTextSchema,
    payload: autonomousArtifactPayloadSchema.nullable().optional(),
    createdAt: isoTimestampSchema,
    updatedAt: isoTimestampSchema,
  })
  .strict()
  .superRefine((artifact, ctx) => {
    const payload = artifact.payload
    if (!payload) {
      return
    }

    if (payload.projectId !== artifact.projectId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'projectId'],
        message: 'Autonomous artifact payload project id must match the enclosing artifact project id.',
      })
    }

    if (payload.runId !== artifact.runId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'runId'],
        message: 'Autonomous artifact payload run id must match the enclosing artifact run id.',
      })
    }

    if (payload.unitId !== artifact.unitId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'unitId'],
        message: 'Autonomous artifact payload unit id must match the enclosing artifact unit id.',
      })
    }

    if (payload.attemptId !== artifact.attemptId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'attemptId'],
        message: 'Autonomous artifact payload attempt id must match the enclosing artifact attempt id.',
      })
    }

    if (payload.artifactId !== artifact.artifactId) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['payload', 'artifactId'],
        message: 'Autonomous artifact payload artifact id must match the enclosing artifact id.',
      })
    }
  })

export const autonomousUnitHistoryEntrySchema = z
  .object({
    unit: autonomousUnitSchema,
    latestAttempt: autonomousUnitAttemptSchema.nullable().optional(),
    artifacts: z.array(autonomousUnitArtifactSchema).optional(),
  })
  .strict()
  .superRefine((entry, ctx) => {
    const latestAttempt = entry.latestAttempt ?? null

    if (latestAttempt) {
      if (latestAttempt.projectId !== entry.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'projectId'],
          message: 'Autonomous history attempt project id must match the enclosing unit project id.',
        })
      }

      if (latestAttempt.runId !== entry.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'runId'],
          message: 'Autonomous history attempt run id must match the enclosing unit run id.',
        })
      }

      if (latestAttempt.unitId !== entry.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['latestAttempt', 'unitId'],
          message: 'Autonomous history attempt unit id must match the enclosing unit id.',
        })
      }
    }

    entry.artifacts?.forEach((artifact, index) => {
      if (artifact.projectId !== entry.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'projectId'],
          message: 'Autonomous history artifacts must reference the same project as the enclosing unit.',
        })
      }

      if (artifact.runId !== entry.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'runId'],
          message: 'Autonomous history artifacts must reference the same run as the enclosing unit.',
        })
      }

      if (artifact.unitId !== entry.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'unitId'],
          message: 'Autonomous history artifacts must reference the same unit as the enclosing history entry.',
        })
      }

      if (latestAttempt && artifact.attemptId !== latestAttempt.attemptId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['artifacts', index, 'attemptId'],
          message: 'Autonomous history artifacts must reference the latest attempt id for the enclosing history entry.',
        })
      }
    })
  })

export const autonomousRunStateSchema = z
  .object({
    run: autonomousRunSchema.nullable(),
    unit: autonomousUnitSchema.nullable(),
    attempt: autonomousUnitAttemptSchema.nullable().optional(),
    history: z.array(autonomousUnitHistoryEntrySchema).optional(),
  })
  .strict()
  .superRefine((state, ctx) => {
    const attempt = state.attempt ?? null
    const history = state.history ?? []

    if (state.run && state.unit) {
      if (state.unit.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['unit', 'projectId'],
          message: 'Autonomous unit project id must match the autonomous run project id.',
        })
      }

      if (state.unit.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['unit', 'runId'],
          message: 'Autonomous unit run id must match the autonomous run run id.',
        })
      }
    }

    if (state.run && attempt) {
      if (attempt.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'projectId'],
          message: 'Autonomous attempt project id must match the autonomous run project id.',
        })
      }

      if (attempt.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'runId'],
          message: 'Autonomous attempt run id must match the autonomous run run id.',
        })
      }

      if (state.run.activeAttemptId && attempt.attemptId !== state.run.activeAttemptId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'attemptId'],
          message: 'Autonomous attempt id must match the active attempt id reported on the run.',
        })
      }
    }

    if (state.unit && attempt) {
      if (attempt.projectId !== state.unit.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'projectId'],
          message: 'Autonomous attempt project id must match the autonomous unit project id.',
        })
      }

      if (attempt.runId !== state.unit.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'runId'],
          message: 'Autonomous attempt run id must match the autonomous unit run id.',
        })
      }

      if (attempt.unitId !== state.unit.unitId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['attempt', 'unitId'],
          message: 'Autonomous attempt unit id must match the autonomous unit id.',
        })
      }
    }

    history.forEach((entry, index) => {
      if (state.run && entry.unit.projectId !== state.run.projectId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['history', index, 'unit', 'projectId'],
          message: 'Autonomous history unit project id must match the autonomous run project id.',
        })
      }

      if (state.run && entry.unit.runId !== state.run.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['history', index, 'unit', 'runId'],
          message: 'Autonomous history unit run id must match the autonomous run run id.',
        })
      }
    })
  })

export type AutonomousRunStatusDto = z.infer<typeof autonomousRunStatusSchema>
export type AutonomousRunRecoveryStateDto = z.infer<typeof autonomousRunRecoveryStateSchema>
export type AutonomousUnitKindDto = z.infer<typeof autonomousUnitKindSchema>
export type AutonomousUnitStatusDto = z.infer<typeof autonomousUnitStatusSchema>
export type AutonomousUnitArtifactStatusDto = z.infer<typeof autonomousUnitArtifactStatusSchema>
export type AutonomousWorkflowLinkageDto = z.infer<typeof autonomousWorkflowLinkageSchema>
export type AutonomousToolCallStateDto = z.infer<typeof autonomousToolCallStateSchema>
export type AutonomousVerificationOutcomeDto = z.infer<typeof autonomousVerificationOutcomeSchema>
export type AutonomousLifecycleReasonDto = z.infer<typeof autonomousLifecycleReasonSchema>
export type AutonomousCommandResultDto = z.infer<typeof autonomousCommandResultSchema>
export type AutonomousToolResultPayloadDto = z.infer<typeof autonomousToolResultPayloadSchema>
export type AutonomousVerificationEvidencePayloadDto = z.infer<typeof autonomousVerificationEvidencePayloadSchema>
export type AutonomousPolicyDeniedPayloadDto = z.infer<typeof autonomousPolicyDeniedPayloadSchema>
export type AutonomousArtifactPayloadDto = z.infer<typeof autonomousArtifactPayloadSchema>
export type AutonomousRunDto = z.infer<typeof autonomousRunSchema>
export type AutonomousUnitDto = z.infer<typeof autonomousUnitSchema>
export type AutonomousUnitAttemptDto = z.infer<typeof autonomousUnitAttemptSchema>
export type AutonomousUnitArtifactDto = z.infer<typeof autonomousUnitArtifactSchema>
export type AutonomousUnitHistoryEntryDto = z.infer<typeof autonomousUnitHistoryEntrySchema>
export type AutonomousRunStateDto = z.infer<typeof autonomousRunStateSchema>
export type AutonomousAdvancedFailureClass = 'timeout' | 'policy_permission' | 'validation_runtime'
export type AutonomousAdvancedFailureRecoveryRecommendation =
  | 'retry'
  | 'approve_resume'
  | 'fix_permissions_policy'

export interface AutonomousLifecycleReasonView {
  code: string
  message: string
}

export interface AutonomousRunView {
  projectId: string
  runId: string
  runtimeKind: string
  providerId: string
  runtimeLabel: string
  supervisorKind: string
  supervisorLabel: string
  status: AutonomousRunStatusDto
  statusLabel: string
  recoveryState: AutonomousRunRecoveryStateDto
  recoveryLabel: string
  activeUnitId: string | null
  activeAttemptId: string | null
  duplicateStartDetected: boolean
  duplicateStartRunId: string | null
  duplicateStartReason: string | null
  startedAt: string
  lastHeartbeatAt: string | null
  lastCheckpointAt: string | null
  pausedAt: string | null
  cancelledAt: string | null
  completedAt: string | null
  crashedAt: string | null
  stoppedAt: string | null
  pauseReason: AutonomousLifecycleReasonView | null
  cancelReason: AutonomousLifecycleReasonView | null
  crashReason: AutonomousLifecycleReasonView | null
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  updatedAt: string
  isActive: boolean
  needsRecovery: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousWorkflowLinkageView {
  workflowNodeId: string
  transitionId: string
  causalTransitionId: string | null
  handoffTransitionId: string
  handoffPackageHash: string
}

export interface AutonomousWorkflowHandoffView {
  handoffTransitionId: string
  causalTransitionId: string | null
  fromNodeId: string
  toNodeId: string
  transitionKind: string
  transitionKindLabel: string
  packageHash: string
  createdAt: string
}

export type AutonomousWorkflowLinkageSource = 'unit' | 'attempt'
export type AutonomousWorkflowContextState = 'ready' | 'awaiting_snapshot' | 'awaiting_handoff'

export interface AutonomousWorkflowContextView {
  linkage: AutonomousWorkflowLinkageView
  linkageSource: AutonomousWorkflowLinkageSource
  linkedNodeLabel: string
  linkedStage: PlanningLifecycleStageView | null
  activeLifecycleStage: PlanningLifecycleStageView | null
  handoff: AutonomousWorkflowHandoffView | null
  pendingApproval: OperatorApprovalView | null
  state: AutonomousWorkflowContextState
  stateLabel: string
  detail: string
}

export interface AutonomousUnitView {
  projectId: string
  runId: string
  unitId: string
  sequence: number
  kind: AutonomousUnitKindDto
  kindLabel: string
  status: AutonomousUnitStatusDto
  statusLabel: string
  summary: string
  boundaryId: string | null
  workflowLinkage: AutonomousWorkflowLinkageView | null
  startedAt: string
  finishedAt: string | null
  updatedAt: string
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  isActive: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousUnitAttemptView {
  projectId: string
  runId: string
  unitId: string
  attemptId: string
  attemptNumber: number
  childSessionId: string
  status: AutonomousUnitStatusDto
  statusLabel: string
  boundaryId: string | null
  workflowLinkage: AutonomousWorkflowLinkageView | null
  startedAt: string
  finishedAt: string | null
  updatedAt: string
  lastErrorCode: string | null
  lastError: RuntimeRunDiagnosticDto | null
  isActive: boolean
  isTerminal: boolean
  isFailed: boolean
}

export interface AutonomousCommandResultView {
  exitCode: number | null
  timedOut: boolean
  summary: string
}

export interface AutonomousUnitArtifactView {
  projectId: string
  runId: string
  unitId: string
  attemptId: string
  artifactId: string
  artifactKind: string
  artifactKindLabel: string
  status: AutonomousUnitArtifactStatusDto
  statusLabel: string
  summary: string
  contentHash: string | null
  payload: AutonomousArtifactPayloadDto | null
  createdAt: string
  updatedAt: string
  detail: string | null
  commandResult: AutonomousCommandResultView | null
  toolSummary: ToolResultSummaryDto | null
  toolName: string | null
  toolState: AutonomousToolCallStateDto | null
  toolStateLabel: string | null
  evidenceKind: string | null
  verificationOutcome: AutonomousVerificationOutcomeDto | null
  verificationOutcomeLabel: string | null
  diagnosticCode: string | null
  advancedFailureClass: AutonomousAdvancedFailureClass | null
  advancedFailureClassLabel: string | null
  advancedFailureDiagnosticCode: string | null
  advancedFailureRecommendation: AutonomousAdvancedFailureRecoveryRecommendation | null
  advancedFailureRecommendationLabel: string | null
  advancedFailureRecommendationDetail: string | null
  actionId: string | null
  boundaryId: string | null
  isToolResult: boolean
  isVerificationEvidence: boolean
  isPolicyDenied: boolean
}

export interface AutonomousUnitHistoryEntryView {
  unit: AutonomousUnitView
  latestAttempt: AutonomousUnitAttemptView | null
  artifacts: AutonomousUnitArtifactView[]
}

export interface AutonomousRunInspectionView {
  autonomousRun: AutonomousRunView | null
  autonomousUnit: AutonomousUnitView | null
  autonomousAttempt: AutonomousUnitAttemptView | null
  autonomousHistory: AutonomousUnitHistoryEntryView[]
  autonomousRecentArtifacts: AutonomousUnitArtifactView[]
}

function getAutonomousWorkflowContextStateLabel(state: AutonomousWorkflowContextState): string {
  switch (state) {
    case 'ready':
      return 'In sync'
    case 'awaiting_snapshot':
      return 'Snapshot lag'
    case 'awaiting_handoff':
      return 'Handoff pending'
  }
}

function mapAutonomousWorkflowHandoff(pkg: WorkflowHandoffPackageView): AutonomousWorkflowHandoffView {
  return {
    handoffTransitionId: pkg.handoffTransitionId,
    causalTransitionId: pkg.causalTransitionId,
    fromNodeId: pkg.fromNodeId,
    toNodeId: pkg.toNodeId,
    transitionKind: pkg.transitionKind,
    transitionKindLabel: humanizeRuntimeKind(pkg.transitionKind),
    packageHash: pkg.packageHash,
    createdAt: pkg.createdAt,
  }
}

export function deriveAutonomousWorkflowContext(options: {
  lifecycle: PlanningLifecycleView
  handoffPackages: WorkflowHandoffPackageView[]
  approvalRequests: OperatorApprovalView[]
  autonomousUnit: AutonomousUnitView | null
  autonomousAttempt?: AutonomousUnitAttemptView | null
}): AutonomousWorkflowContextView | null {
  const attemptLinkage = options.autonomousAttempt?.workflowLinkage ?? null
  const unitLinkage = options.autonomousUnit?.workflowLinkage ?? null
  const linkage = attemptLinkage ?? unitLinkage
  if (!linkage) {
    return null
  }

  const linkageSource: AutonomousWorkflowLinkageSource = attemptLinkage ? 'attempt' : 'unit'
  const linkedStage = options.lifecycle.stages.find((stage) => stage.nodeId === linkage.workflowNodeId) ?? null
  const activeLifecycleStage = options.lifecycle.activeStage
  const linkedNodeLabel = linkedStage?.nodeLabel ?? humanizeWorkflowNodeId(linkage.workflowNodeId)
  const matchingHandoffPackage = sortByNewest(
    options.handoffPackages.filter((pkg) => pkg.handoffTransitionId === linkage.handoffTransitionId),
    (pkg) => pkg.createdAt,
  )[0] ?? null
  const handoff = matchingHandoffPackage ? mapAutonomousWorkflowHandoff(matchingHandoffPackage) : null
  const pendingApproval =
    options.approvalRequests.find(
      (approval) => approval.isPending && approval.gateNodeId === linkage.workflowNodeId,
    ) ?? null

  const activeStageMismatch = Boolean(activeLifecycleStage && activeLifecycleStage.nodeId !== linkage.workflowNodeId)
  const handoffHashMismatch = Boolean(handoff && handoff.packageHash !== linkage.handoffPackageHash)

  let state: AutonomousWorkflowContextState
  let detail: string

  if (!linkedStage) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence has persisted autonomous workflow linkage for this boundary, but the selected project snapshot has not exposed the linked lifecycle node yet.'
  } else if (activeStageMismatch) {
    state = 'awaiting_snapshot'
    detail = `Cadence is keeping lifecycle progression anchored to snapshot truth while the linked node \`${linkedStage.stageLabel}\` waits for the active lifecycle stage to catch up.`
  } else if (handoffHashMismatch) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence found the linked handoff transition in the selected project snapshot, but the persisted handoff hash has not caught up to the autonomous linkage yet.'
  } else if (!handoff) {
    state = 'awaiting_handoff'
    detail =
      'Cadence has persisted autonomous workflow linkage for this boundary, but the linked handoff package is not visible in the selected project snapshot yet.'
  } else {
    state = 'ready'
    detail =
      'Lifecycle stage, autonomous linkage, and handoff package all agree on backend truth for this boundary.'
  }

  if (pendingApproval) {
    detail = `${detail} Pending approval \`${pendingApproval.title}\` is still blocking continuation at this linked node.`
  }

  return {
    linkage,
    linkageSource,
    linkedNodeLabel,
    linkedStage,
    activeLifecycleStage,
    handoff,
    pendingApproval,
    state,
    stateLabel: getAutonomousWorkflowContextStateLabel(state),
    detail,
  }
}

export function getAutonomousRunStatusLabel(status: AutonomousRunStatusDto): string {
  switch (status) {
    case 'starting':
      return 'Autonomous run starting'
    case 'running':
      return 'Autonomous run active'
    case 'paused':
      return 'Autonomous run paused'
    case 'cancelling':
      return 'Autonomous run cancelling'
    case 'cancelled':
      return 'Autonomous run cancelled'
    case 'stale':
      return 'Autonomous run stale'
    case 'failed':
      return 'Autonomous run failed'
    case 'stopped':
      return 'Autonomous run stopped'
    case 'crashed':
      return 'Autonomous run crashed'
    case 'completed':
      return 'Autonomous run completed'
  }
}

export function getAutonomousRunRecoveryLabel(recoveryState: AutonomousRunRecoveryStateDto): string {
  switch (recoveryState) {
    case 'healthy':
      return 'Recovery healthy'
    case 'recovery_required':
      return 'Recovery required'
    case 'terminal':
      return 'Terminal state'
    case 'failed':
      return 'Recovery failed'
  }
}

export function getAutonomousUnitKindLabel(kind: AutonomousUnitKindDto): string {
  switch (kind) {
    case 'researcher':
      return 'Researcher worker'
    case 'planner':
      return 'Planner worker'
    case 'executor':
      return 'Executor worker'
    case 'verifier':
      return 'Verifier worker'
  }
}

export function getAutonomousUnitStatusLabel(status: AutonomousUnitStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'active':
      return 'Active'
    case 'blocked':
      return 'Blocked'
    case 'paused':
      return 'Paused'
    case 'completed':
      return 'Completed'
    case 'cancelled':
      return 'Cancelled'
    case 'failed':
      return 'Failed'
  }
}

function getAutonomousRunLabel(runtimeKind: string, status: AutonomousRunStatusDto): string {
  return `${humanizeRuntimeKind(runtimeKind)} · ${getAutonomousRunStatusLabel(status)}`
}

export function mapAutonomousRun(autonomousRun: AutonomousRunDto): AutonomousRunView {
  const runtimeKind = normalizeText(autonomousRun.runtimeKind, 'openai_codex')
  const providerId = normalizeText(autonomousRun.providerId, 'provider-unavailable')
  const supervisorKind = normalizeText(autonomousRun.supervisorKind, 'detached_pty')

  return {
    projectId: autonomousRun.projectId,
    runId: normalizeText(autonomousRun.runId, 'autonomous-run-unavailable'),
    runtimeKind,
    providerId,
    runtimeLabel: getAutonomousRunLabel(runtimeKind, autonomousRun.status),
    supervisorKind,
    supervisorLabel: humanizeRuntimeKind(supervisorKind),
    status: autonomousRun.status,
    statusLabel: getAutonomousRunStatusLabel(autonomousRun.status),
    recoveryState: autonomousRun.recoveryState,
    recoveryLabel: getAutonomousRunRecoveryLabel(autonomousRun.recoveryState),
    activeUnitId: normalizeOptionalText(autonomousRun.activeUnitId),
    activeAttemptId: normalizeOptionalText(autonomousRun.activeAttemptId),
    duplicateStartDetected: autonomousRun.duplicateStartDetected,
    duplicateStartRunId: normalizeOptionalText(autonomousRun.duplicateStartRunId),
    duplicateStartReason: normalizeOptionalText(autonomousRun.duplicateStartReason),
    startedAt: autonomousRun.startedAt,
    lastHeartbeatAt: normalizeOptionalText(autonomousRun.lastHeartbeatAt),
    lastCheckpointAt: normalizeOptionalText(autonomousRun.lastCheckpointAt),
    pausedAt: normalizeOptionalText(autonomousRun.pausedAt),
    cancelledAt: normalizeOptionalText(autonomousRun.cancelledAt),
    completedAt: normalizeOptionalText(autonomousRun.completedAt),
    crashedAt: normalizeOptionalText(autonomousRun.crashedAt),
    stoppedAt: normalizeOptionalText(autonomousRun.stoppedAt),
    pauseReason: autonomousRun.pauseReason ?? null,
    cancelReason: autonomousRun.cancelReason ?? null,
    crashReason: autonomousRun.crashReason ?? null,
    lastErrorCode: normalizeOptionalText(autonomousRun.lastErrorCode),
    lastError: autonomousRun.lastError ?? null,
    updatedAt: autonomousRun.updatedAt,
    isActive: autonomousRun.status === 'starting' || autonomousRun.status === 'running',
    needsRecovery: autonomousRun.recoveryState === 'recovery_required',
    isTerminal: ['cancelled', 'stopped', 'completed'].includes(autonomousRun.status),
    isFailed: ['failed', 'crashed'].includes(autonomousRun.status),
  }
}

function mapAutonomousWorkflowLinkage(
  workflowLinkage: AutonomousWorkflowLinkageDto,
): AutonomousWorkflowLinkageView {
  return {
    workflowNodeId: normalizeText(workflowLinkage.workflowNodeId, 'workflow-node-unavailable'),
    transitionId: normalizeText(workflowLinkage.transitionId, 'workflow-transition-unavailable'),
    causalTransitionId: normalizeOptionalText(workflowLinkage.causalTransitionId),
    handoffTransitionId: normalizeText(
      workflowLinkage.handoffTransitionId,
      'workflow-handoff-transition-unavailable',
    ),
    handoffPackageHash: normalizeText(
      workflowLinkage.handoffPackageHash,
      'workflow-handoff-package-hash-unavailable',
    ),
  }
}

export function mapAutonomousUnit(autonomousUnit: AutonomousUnitDto): AutonomousUnitView {
  return {
    projectId: autonomousUnit.projectId,
    runId: autonomousUnit.runId,
    unitId: normalizeText(autonomousUnit.unitId, 'autonomous-unit-unavailable'),
    sequence: autonomousUnit.sequence,
    kind: autonomousUnit.kind,
    kindLabel: getAutonomousUnitKindLabel(autonomousUnit.kind),
    status: autonomousUnit.status,
    statusLabel: getAutonomousUnitStatusLabel(autonomousUnit.status),
    summary: normalizeText(autonomousUnit.summary, 'Autonomous unit boundary recorded.'),
    boundaryId: normalizeOptionalText(autonomousUnit.boundaryId),
    workflowLinkage: autonomousUnit.workflowLinkage
      ? mapAutonomousWorkflowLinkage(autonomousUnit.workflowLinkage)
      : null,
    startedAt: autonomousUnit.startedAt,
    finishedAt: normalizeOptionalText(autonomousUnit.finishedAt),
    updatedAt: autonomousUnit.updatedAt,
    lastErrorCode: normalizeOptionalText(autonomousUnit.lastErrorCode),
    lastError: autonomousUnit.lastError ?? null,
    isActive: autonomousUnit.status === 'active',
    isTerminal: ['completed', 'cancelled', 'failed'].includes(autonomousUnit.status),
    isFailed: autonomousUnit.status === 'failed',
  }
}

function getAutonomousArtifactKindLabel(artifactKind: string): string {
  switch (artifactKind) {
    case 'tool_result':
      return 'Tool result'
    case 'verification_evidence':
      return 'Verification evidence'
    case 'policy_denied':
      return 'Policy denied'
    default:
      return humanizeRuntimeKind(artifactKind)
  }
}

function getAutonomousArtifactStatusLabel(status: AutonomousUnitArtifactStatusDto): string {
  switch (status) {
    case 'pending':
      return 'Pending'
    case 'recorded':
      return 'Recorded'
    case 'rejected':
      return 'Rejected'
    case 'redacted':
      return 'Redacted'
  }
}

function getAutonomousToolCallStateLabel(state: AutonomousToolCallStateDto): string {
  switch (state) {
    case 'pending':
      return 'Pending'
    case 'running':
      return 'Running'
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

function getAutonomousVerificationOutcomeLabel(outcome: AutonomousVerificationOutcomeDto): string {
  switch (outcome) {
    case 'passed':
      return 'Passed'
    case 'failed':
      return 'Failed'
    case 'blocked':
      return 'Blocked'
  }
}

export function mapAutonomousAttempt(autonomousAttempt: AutonomousUnitAttemptDto): AutonomousUnitAttemptView {
  return {
    projectId: autonomousAttempt.projectId,
    runId: autonomousAttempt.runId,
    unitId: autonomousAttempt.unitId,
    attemptId: normalizeText(autonomousAttempt.attemptId, 'autonomous-attempt-unavailable'),
    attemptNumber: autonomousAttempt.attemptNumber,
    childSessionId: normalizeText(autonomousAttempt.childSessionId, 'child-session-unavailable'),
    status: autonomousAttempt.status,
    statusLabel: getAutonomousUnitStatusLabel(autonomousAttempt.status),
    boundaryId: normalizeOptionalText(autonomousAttempt.boundaryId),
    workflowLinkage: autonomousAttempt.workflowLinkage
      ? mapAutonomousWorkflowLinkage(autonomousAttempt.workflowLinkage)
      : null,
    startedAt: autonomousAttempt.startedAt,
    finishedAt: normalizeOptionalText(autonomousAttempt.finishedAt),
    updatedAt: autonomousAttempt.updatedAt,
    lastErrorCode: normalizeOptionalText(autonomousAttempt.lastErrorCode),
    lastError: autonomousAttempt.lastError ?? null,
    isActive: autonomousAttempt.status === 'active',
    isTerminal: ['completed', 'cancelled', 'failed'].includes(autonomousAttempt.status),
    isFailed: autonomousAttempt.status === 'failed',
  }
}

function mapAutonomousCommandResult(commandResult: AutonomousCommandResultDto): AutonomousCommandResultView {
  return {
    exitCode: commandResult.exitCode ?? null,
    timedOut: commandResult.timedOut,
    summary: normalizeText(commandResult.summary, 'Autonomous command result recorded.'),
  }
}

const ADVANCED_BROWSER_FAILURE_CODE_TO_CLASS: Record<string, AutonomousAdvancedFailureClass> = {
  advanced_browser_failure_timeout: 'timeout',
  advanced_browser_failure_policy_permission: 'policy_permission',
  advanced_browser_failure_validation_runtime: 'validation_runtime',
}

function parseAdvancedBrowserFailureDiagnosticCode(value: string | null | undefined): string | null {
  const normalizedValue = normalizeOptionalText(value)
  if (!normalizedValue) {
    return null
  }

  const [candidate] = normalizedValue.split(':', 1)
  const normalizedCandidate = normalizeOptionalText(candidate)
  if (!normalizedCandidate) {
    return null
  }

  return normalizedCandidate in ADVANCED_BROWSER_FAILURE_CODE_TO_CLASS ? normalizedCandidate : null
}

function getAutonomousAdvancedFailureClassLabel(failureClass: AutonomousAdvancedFailureClass): string {
  switch (failureClass) {
    case 'timeout':
      return 'Timeout'
    case 'policy_permission':
      return 'Policy / permission'
    case 'validation_runtime':
      return 'Validation / runtime'
  }
}

function getAutonomousAdvancedFailureRecommendationLabel(
  recommendation: AutonomousAdvancedFailureRecoveryRecommendation,
): string {
  switch (recommendation) {
    case 'retry':
      return 'Retry'
    case 'approve_resume':
      return 'Approve / resume'
    case 'fix_permissions_policy':
      return 'Fix permissions / policy'
  }
}

function getAutonomousAdvancedFailureRecommendationDetail(
  failureClass: AutonomousAdvancedFailureClass,
): string {
  switch (failureClass) {
    case 'timeout':
      return 'Browser/computer-use action timed out. Retry this boundary, increasing timeout if needed.'
    case 'policy_permission':
      return 'Browser/computer-use action was blocked by policy or permissions. Fix access or policy before retrying.'
    case 'validation_runtime':
      return 'Browser/computer-use action failed validation/runtime checks. Fix selector or runtime assumptions, then retry.'
  }
}

function getAutonomousAdvancedFailureEvidence(artifact: AutonomousUnitArtifactDto): {
  advancedFailureClass: AutonomousAdvancedFailureClass
  advancedFailureDiagnosticCode: string
  advancedFailureRecommendation: AutonomousAdvancedFailureRecoveryRecommendation
} | null {
  const payload = artifact.payload
  if (!payload) {
    return null
  }

  let advancedFailureDiagnosticCode: string | null = null

  if (payload.kind === 'tool_result') {
    const toolSummary = payload.toolSummary
    if (toolSummary?.kind !== 'browser_computer_use') {
      return null
    }

    if (toolSummary.status !== 'failed' && toolSummary.status !== 'blocked') {
      return null
    }

    advancedFailureDiagnosticCode = parseAdvancedBrowserFailureDiagnosticCode(toolSummary.outcome)
  }

  if (payload.kind === 'policy_denied') {
    advancedFailureDiagnosticCode = parseAdvancedBrowserFailureDiagnosticCode(payload.diagnosticCode)
  }

  if (!advancedFailureDiagnosticCode) {
    return null
  }

  const advancedFailureClass = ADVANCED_BROWSER_FAILURE_CODE_TO_CLASS[advancedFailureDiagnosticCode]

  return {
    advancedFailureClass,
    advancedFailureDiagnosticCode,
    advancedFailureRecommendation:
      advancedFailureClass === 'policy_permission' ? 'fix_permissions_policy' : 'retry',
  }
}

function getAutonomousArtifactDetail(
  artifact: AutonomousUnitArtifactDto,
  commandResult: AutonomousCommandResultView | null,
): string | null {
  const payload = artifact.payload ?? null
  if (!payload) {
    return normalizeOptionalText(artifact.summary)
  }

  switch (payload.kind) {
    case 'tool_result':
      return commandResult?.summary ?? normalizeOptionalText(artifact.summary)
    case 'verification_evidence':
      return commandResult?.summary ?? normalizeOptionalText(payload.label) ?? normalizeOptionalText(artifact.summary)
    case 'policy_denied':
      return normalizeOptionalText(payload.message) ?? normalizeOptionalText(artifact.summary)
  }
}

export function mapAutonomousArtifact(artifact: AutonomousUnitArtifactDto): AutonomousUnitArtifactView {
  const payload = artifact.payload ?? null
  const commandResult =
    payload != null
      && (payload.kind === 'tool_result' || payload.kind === 'verification_evidence')
      && payload.commandResult
      ? mapAutonomousCommandResult(payload.commandResult)
      : null

  let toolSummary: ToolResultSummaryDto | null = null
  let toolName: string | null = null
  let toolState: AutonomousToolCallStateDto | null = null
  let toolStateLabel: string | null = null
  let evidenceKind: string | null = null
  let verificationOutcome: AutonomousVerificationOutcomeDto | null = null
  let verificationOutcomeLabel: string | null = null
  let diagnosticCode: string | null = null
  let advancedFailureClass: AutonomousAdvancedFailureClass | null = null
  let advancedFailureClassLabel: string | null = null
  let advancedFailureDiagnosticCode: string | null = null
  let advancedFailureRecommendation: AutonomousAdvancedFailureRecoveryRecommendation | null = null
  let advancedFailureRecommendationLabel: string | null = null
  let advancedFailureRecommendationDetail: string | null = null
  let actionId: string | null = null
  let boundaryId: string | null = null

  switch (payload?.kind) {
    case 'tool_result':
      toolSummary = payload.toolSummary ?? null
      toolName = normalizeOptionalText(payload.toolName)
      toolState = payload.toolState
      toolStateLabel = getAutonomousToolCallStateLabel(payload.toolState)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
    case 'verification_evidence':
      evidenceKind = normalizeOptionalText(payload.evidenceKind)
      verificationOutcome = payload.outcome
      verificationOutcomeLabel = getAutonomousVerificationOutcomeLabel(payload.outcome)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
    case 'policy_denied':
      toolName = normalizeOptionalText(payload.toolName)
      diagnosticCode = normalizeOptionalText(payload.diagnosticCode)
      actionId = normalizeOptionalText(payload.actionId)
      boundaryId = normalizeOptionalText(payload.boundaryId)
      break
  }

  const advancedFailureEvidence = getAutonomousAdvancedFailureEvidence(artifact)
  if (advancedFailureEvidence) {
    advancedFailureClass = advancedFailureEvidence.advancedFailureClass
    advancedFailureClassLabel = getAutonomousAdvancedFailureClassLabel(advancedFailureClass)
    advancedFailureDiagnosticCode = advancedFailureEvidence.advancedFailureDiagnosticCode
    advancedFailureRecommendation = advancedFailureEvidence.advancedFailureRecommendation
    advancedFailureRecommendationLabel = getAutonomousAdvancedFailureRecommendationLabel(advancedFailureRecommendation)
    advancedFailureRecommendationDetail = getAutonomousAdvancedFailureRecommendationDetail(advancedFailureClass)
  }

  return {
    projectId: artifact.projectId,
    runId: artifact.runId,
    unitId: artifact.unitId,
    attemptId: artifact.attemptId,
    artifactId: normalizeText(artifact.artifactId, 'autonomous-artifact-unavailable'),
    artifactKind: artifact.artifactKind,
    artifactKindLabel: getAutonomousArtifactKindLabel(artifact.artifactKind),
    status: artifact.status,
    statusLabel: getAutonomousArtifactStatusLabel(artifact.status),
    summary: normalizeText(artifact.summary, 'Autonomous artifact recorded.'),
    contentHash: normalizeOptionalText(artifact.contentHash),
    payload,
    createdAt: artifact.createdAt,
    updatedAt: artifact.updatedAt,
    detail: getAutonomousArtifactDetail(artifact, commandResult),
    commandResult,
    toolSummary,
    toolName,
    toolState,
    toolStateLabel,
    evidenceKind,
    verificationOutcome,
    verificationOutcomeLabel,
    diagnosticCode,
    advancedFailureClass,
    advancedFailureClassLabel,
    advancedFailureDiagnosticCode,
    advancedFailureRecommendation,
    advancedFailureRecommendationLabel,
    advancedFailureRecommendationDetail,
    actionId,
    boundaryId,
    isToolResult: artifact.artifactKind === 'tool_result',
    isVerificationEvidence: artifact.artifactKind === 'verification_evidence',
    isPolicyDenied: artifact.artifactKind === 'policy_denied',
  }
}

export function mapAutonomousHistoryEntry(entry: AutonomousUnitHistoryEntryDto): AutonomousUnitHistoryEntryView {
  return {
    unit: mapAutonomousUnit(entry.unit),
    latestAttempt: entry.latestAttempt ? mapAutonomousAttempt(entry.latestAttempt) : null,
    artifacts: sortByNewest((entry.artifacts ?? []).map(mapAutonomousArtifact), (artifact) => artifact.updatedAt || artifact.createdAt),
  }
}

export function mapAutonomousRunInspection(autonomousState: AutonomousRunStateDto): AutonomousRunInspectionView {
  const autonomousHistory = (autonomousState.history ?? []).map(mapAutonomousHistoryEntry)
  const autonomousRecentArtifacts = sortByNewest(
    autonomousHistory.flatMap((entry) => entry.artifacts),
    (artifact) => artifact.updatedAt || artifact.createdAt,
  ).slice(0, 5)

  return {
    autonomousRun: autonomousState.run ? mapAutonomousRun(autonomousState.run) : null,
    autonomousUnit: autonomousState.unit ? mapAutonomousUnit(autonomousState.unit) : null,
    autonomousAttempt: autonomousState.attempt ? mapAutonomousAttempt(autonomousState.attempt) : null,
    autonomousHistory,
    autonomousRecentArtifacts,
  }
}
