import type { Project } from '@/components/xero/data'
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
} from './xero-model/project'
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
} from './xero-model/operator-actions'
import {
  mapNotificationBroker,
  notificationDispatchSchema,
  notificationReplyClaimSchema,
  type NotificationBrokerView,
  type NotificationDispatchDto,
} from './xero-model/notifications'
import {
  autonomousRunSchema,
  mapAutonomousRun,
  type AutonomousRunView,
} from './xero-model/autonomous'
import {
  agentSessionSchema,
  mapAgentSession,
  selectAgentSessionId,
  type AgentSessionView,
  type RuntimeRunView,
  type RuntimeSessionView,
} from './xero-model/runtime'

export type { Phase, PhaseStatus, Project } from '@/components/xero/data'
export {
  browserComputerUseActionStatusSchema,
  browserComputerUseSurfaceSchema,
  gitToolResultScopeSchema,
  mcpCapabilityKindSchema,
  safePercent,
  toolResultSummarySchema,
  webToolResultContentKindSchema,
} from './xero-model/shared'
export type {
  BrowserComputerUseActionStatusDto,
  BrowserComputerUseSurfaceDto,
  GitToolResultScopeDto,
  McpCapabilityKindDto,
  ToolResultSummaryDto,
  WebToolResultContentKindDto,
} from './xero-model/shared'
export * from './xero-model/project'
export * from './xero-model/operator-actions'
export * from './xero-model/notifications'
export * from './xero-model/runtime'
export * from './xero-model/provider-credentials'
export * from './xero-model/provider-models'
export * from './xero-model/diagnostics'
export * from './xero-model/autonomous'
export * from './xero-model/agent'
export * from './xero-model/agent-definition'
export * from './xero-model/runtime-stream'
export * from './xero-model/mcp'
export * from './xero-model/skills'
export * from './xero-model/session-context'
export * from './xero-model/dictation'
export * from './xero-model/browser'
export * from './xero-model/soul'
export * from './xero-model/usage'
export * from './xero-model/environment'
export * from './xero-model/developer-storage'

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
