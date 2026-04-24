import type { Project } from '@/components/cadence/data'
import { z } from 'zod'
import {
  mapPhase,
  mapProjectSummary,
  mapRepository,
  phaseSummarySchema,
  projectSummarySchema,
  repositorySummarySchema,
  type RepositoryStatusView,
  type RepositoryView,
} from './cadence-model/project'
import {
  getLatestDecisionOutcome,
  mapOperatorApproval,
  mapResumeHistoryEntry,
  mapVerificationRecord,
  operatorApprovalSchema,
  resumeHistoryEntrySchema,
  verificationRecordSchema,
  type OperatorApprovalView,
  type OperatorDecisionOutcomeView,
  type ResumeHistoryEntryView,
  type VerificationRecordView,
} from './cadence-model/operator-actions'
import {
  mapNotificationBroker,
  notificationDispatchSchema,
  notificationReplyClaimSchema,
  type NotificationBrokerView,
  type NotificationDispatchDto,
} from './cadence-model/notifications'
import {
  mapPlanningLifecycle,
  mapWorkflowHandoffPackage,
  planningLifecycleProjectionSchema,
  workflowHandoffPackageSchema,
  type PlanningLifecycleView,
  type WorkflowHandoffPackageView,
} from './cadence-model/workflow'
import {
  autonomousRunSchema,
  autonomousUnitSchema,
  mapAutonomousRun,
  mapAutonomousUnit,
  type AutonomousRunView,
  type AutonomousUnitArtifactView,
  type AutonomousUnitAttemptView,
  type AutonomousUnitHistoryEntryView,
  type AutonomousUnitView,
} from './cadence-model/autonomous'
import type { RuntimeRunView, RuntimeSessionView } from './cadence-model/runtime'

export type { Phase, PhaseStatus, PhaseStep, Project } from '@/components/cadence/data'
export {
  browserComputerUseActionStatusSchema,
  browserComputerUseSurfaceSchema,
  gitToolResultScopeSchema,
  mcpCapabilityKindSchema,
  safePercent,
  toolResultSummarySchema,
  webToolResultContentKindSchema,
} from './cadence-model/shared'
export type {
  BrowserComputerUseActionStatusDto,
  BrowserComputerUseSurfaceDto,
  GitToolResultScopeDto,
  McpCapabilityKindDto,
  ToolResultSummaryDto,
  WebToolResultContentKindDto,
} from './cadence-model/shared'
export * from './cadence-model/project'
export * from './cadence-model/operator-actions'
export * from './cadence-model/notifications'
export * from './cadence-model/workflow'
export * from './cadence-model/runtime'
export * from './cadence-model/provider-profiles'
export * from './cadence-model/provider-models'
export * from './cadence-model/autonomous'
export * from './cadence-model/runtime-stream'
export * from './cadence-model/mcp'

export const projectSnapshotResponseSchema = z
  .object({
    project: projectSummarySchema,
    repository: repositorySummarySchema.nullable(),
    phases: z.array(phaseSummarySchema),
    lifecycle: planningLifecycleProjectionSchema,
    approvalRequests: z.array(operatorApprovalSchema),
    verificationRecords: z.array(verificationRecordSchema),
    resumeHistory: z.array(resumeHistoryEntrySchema),
    handoffPackages: z.array(workflowHandoffPackageSchema).optional(),
    autonomousRun: z.lazy(() => autonomousRunSchema).nullable().optional(),
    autonomousUnit: z.lazy(() => autonomousUnitSchema).nullable().optional(),
    notificationDispatches: z.array(notificationDispatchSchema).optional(),
    notificationReplyClaims: z.array(notificationReplyClaimSchema).optional(),
  })
  .superRefine((snapshot, ctx) => {
    if (snapshot.autonomousRun && snapshot.autonomousRun.projectId !== snapshot.project.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['autonomousRun', 'projectId'],
        message: 'Autonomous run project id must match the selected project snapshot id.',
      })
    }

    if (snapshot.autonomousUnit && snapshot.autonomousUnit.projectId !== snapshot.project.id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['autonomousUnit', 'projectId'],
        message: 'Autonomous unit project id must match the selected project snapshot id.',
      })
    }

    if (snapshot.autonomousRun && snapshot.autonomousUnit) {
      if (snapshot.autonomousUnit.runId !== snapshot.autonomousRun.runId) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          path: ['autonomousUnit', 'runId'],
          message: 'Autonomous unit run id must match the active autonomous run id.',
        })
      }
    }
  })

export type ProjectSnapshotResponseDto = z.infer<typeof projectSnapshotResponseSchema>

export interface ProjectDetailView extends Project {
  branchLabel: string
  runtimeLabel: string
  phaseProgressPercent: number
  lifecycle: PlanningLifecycleView
  repository: RepositoryView | null
  repositoryStatus: RepositoryStatusView | null
  approvalRequests: OperatorApprovalView[]
  pendingApprovalCount: number
  latestDecisionOutcome: OperatorDecisionOutcomeView | null
  verificationRecords: VerificationRecordView[]
  resumeHistory: ResumeHistoryEntryView[]
  handoffPackages: WorkflowHandoffPackageView[]
  notificationBroker: NotificationBrokerView
  runtimeSession?: RuntimeSessionView | null
  runtimeRun?: RuntimeRunView | null
  autonomousRun?: AutonomousRunView | null
  autonomousUnit?: AutonomousUnitView | null
  autonomousAttempt?: AutonomousUnitAttemptView | null
  autonomousHistory: AutonomousUnitHistoryEntryView[]
  autonomousRecentArtifacts: AutonomousUnitArtifactView[]
}

export function mapProjectSnapshot(
  snapshot: ProjectSnapshotResponseDto,
  options: { notificationDispatches?: NotificationDispatchDto[] } = {},
): ProjectDetailView {
  const summary = mapProjectSummary(snapshot.project)
  const approvalRequests = snapshot.approvalRequests.map(mapOperatorApproval)
  const verificationRecords = snapshot.verificationRecords.map(mapVerificationRecord)
  const resumeHistory = snapshot.resumeHistory.map(mapResumeHistoryEntry)
  const handoffPackages = (snapshot.handoffPackages ?? [])
    .filter((pkg) => pkg.projectId === snapshot.project.id)
    .map(mapWorkflowHandoffPackage)
  const notificationDispatches = options.notificationDispatches ?? snapshot.notificationDispatches ?? []
  const notificationBroker = mapNotificationBroker(snapshot.project.id, notificationDispatches)

  if (!snapshot.lifecycle) {
    throw new Error('Cadence received a project snapshot without the required lifecycle projection.')
  }

  const autonomousRun = snapshot.autonomousRun ? mapAutonomousRun(snapshot.autonomousRun) : null
  const autonomousUnit = snapshot.autonomousUnit ? mapAutonomousUnit(snapshot.autonomousUnit) : null

  return {
    ...summary,
    phases: snapshot.phases.map(mapPhase),
    lifecycle: mapPlanningLifecycle(snapshot.lifecycle),
    repository: snapshot.repository ? mapRepository(snapshot.repository) : null,
    repositoryStatus: null,
    approvalRequests,
    pendingApprovalCount: approvalRequests.filter((approval) => approval.isPending).length,
    latestDecisionOutcome: getLatestDecisionOutcome(approvalRequests),
    verificationRecords,
    resumeHistory,
    handoffPackages,
    notificationBroker,
    runtimeSession: null,
    runtimeRun: null,
    autonomousRun,
    autonomousUnit,
    autonomousAttempt: null,
    autonomousHistory: [],
    autonomousRecentArtifacts: [],
  }
}

export function applyRuntimeSession(
  project: ProjectDetailView,
  runtimeSession: RuntimeSessionView | null,
): ProjectDetailView {
  if (!runtimeSession) {
    return {
      ...project,
      runtimeSession: null,
    }
  }

  return {
    ...project,
    runtime: runtimeSession.runtimeLabel,
    runtimeLabel: runtimeSession.runtimeLabel,
    runtimeSession,
  }
}

export function applyRuntimeRun(
  project: ProjectDetailView,
  runtimeRun: RuntimeRunView | null,
): ProjectDetailView {
  return {
    ...project,
    runtimeRun: runtimeRun ?? null,
  }
}
