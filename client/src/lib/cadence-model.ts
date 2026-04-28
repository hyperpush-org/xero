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
  autonomousRunSchema,
  mapAutonomousRun,
  type AutonomousRunView,
} from './cadence-model/autonomous'
import {
  agentSessionSchema,
  mapAgentSession,
  selectAgentSessionId,
  type AgentSessionView,
  type RuntimeRunView,
  type RuntimeSessionView,
} from './cadence-model/runtime'

export type { Phase, PhaseStatus, Project } from '@/components/cadence/data'
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
export * from './cadence-model/runtime'
export * from './cadence-model/provider-profiles'
export * from './cadence-model/provider-credentials'
export * from './cadence-model/provider-models'
export * from './cadence-model/provider-setup'
export * from './cadence-model/diagnostics'
export * from './cadence-model/autonomous'
export * from './cadence-model/agent'
export * from './cadence-model/runtime-stream'
export * from './cadence-model/mcp'
export * from './cadence-model/skills'
export * from './cadence-model/session-context'
export * from './cadence-model/dictation'
export * from './cadence-model/usage'

export const projectSnapshotResponseSchema = z
  .object({
    project: projectSummarySchema,
    repository: repositorySummarySchema.nullable(),
    phases: z.array(phaseSummarySchema),
    approvalRequests: z.array(operatorApprovalSchema),
    verificationRecords: z.array(verificationRecordSchema),
    resumeHistory: z.array(resumeHistoryEntrySchema),
    agentSessions: z.array(agentSessionSchema),
    autonomousRun: z.lazy(() => autonomousRunSchema).nullable().optional(),
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
  })

export type ProjectSnapshotResponseDto = z.infer<typeof projectSnapshotResponseSchema>

export interface ProjectDetailView extends Project {
  branchLabel: string
  runtimeLabel: string
  phaseProgressPercent: number
  repository: RepositoryView | null
  repositoryStatus: RepositoryStatusView | null
  approvalRequests: OperatorApprovalView[]
  pendingApprovalCount: number
  latestDecisionOutcome: OperatorDecisionOutcomeView | null
  verificationRecords: VerificationRecordView[]
  resumeHistory: ResumeHistoryEntryView[]
  agentSessions: AgentSessionView[]
  selectedAgentSession: AgentSessionView | null
  selectedAgentSessionId: string
  notificationBroker: NotificationBrokerView
  runtimeSession?: RuntimeSessionView | null
  runtimeRun?: RuntimeRunView | null
  autonomousRun?: AutonomousRunView | null
}

export function mapProjectSnapshot(
  snapshot: ProjectSnapshotResponseDto,
  options: { notificationDispatches?: NotificationDispatchDto[] } = {},
): ProjectDetailView {
  const summary = mapProjectSummary(snapshot.project)
  const approvalRequests = snapshot.approvalRequests.map(mapOperatorApproval)
  const verificationRecords = snapshot.verificationRecords.map(mapVerificationRecord)
  const resumeHistory = snapshot.resumeHistory.map(mapResumeHistoryEntry)
  const agentSessions = snapshot.agentSessions
    .filter((session) => session.projectId === snapshot.project.id)
    .map(mapAgentSession)
  const selectedAgentSessionId = selectAgentSessionId(agentSessions)
  const selectedAgentSession =
    agentSessions.find((session) => session.agentSessionId === selectedAgentSessionId) ?? null
  const notificationDispatches = options.notificationDispatches ?? snapshot.notificationDispatches ?? []
  const notificationBroker = mapNotificationBroker(snapshot.project.id, notificationDispatches)

  const autonomousRun = snapshot.autonomousRun ? mapAutonomousRun(snapshot.autonomousRun) : null

  return {
    ...summary,
    phases: snapshot.phases.map(mapPhase),
    repository: snapshot.repository ? mapRepository(snapshot.repository) : null,
    repositoryStatus: null,
    approvalRequests,
    pendingApprovalCount: approvalRequests.filter((approval) => approval.isPending).length,
    latestDecisionOutcome: getLatestDecisionOutcome(approvalRequests),
    verificationRecords,
    resumeHistory,
    agentSessions,
    selectedAgentSession,
    selectedAgentSessionId,
    notificationBroker,
    runtimeSession: null,
    runtimeRun: null,
    autonomousRun,
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
