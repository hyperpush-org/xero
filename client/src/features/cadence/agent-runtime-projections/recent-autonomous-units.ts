import type {
  AutonomousUnitArtifactView,
  AutonomousUnitHistoryEntryView,
  AutonomousUnitStatusDto,
  AutonomousWorkflowContextState,
  OperatorApprovalView,
  PlanningLifecycleStageView,
  PlanningLifecycleView,
  WorkflowHandoffPackageView,
} from '@/src/lib/cadence-model'

import { getTimestampMs, normalizeText, sortByNewest } from './shared'

export const MAX_RECENT_AUTONOMOUS_UNITS = 5
const MAX_RECENT_AUTONOMOUS_EVIDENCE_PREVIEWS = 2

export type RecentAutonomousUnitWorkflowState = AutonomousWorkflowContextState | 'unlinked'
export type RecentAutonomousUnitLinkageSource = 'attempt' | 'unit' | 'none'

export interface RecentAutonomousUnitEvidencePreview {
  artifactId: string
  artifactKindLabel: string
  statusLabel: string
  summary: string
  updatedAt: string
}

export interface RecentAutonomousUnitCardView {
  unitId: string
  sequence: number
  sequenceLabel: string
  kindLabel: string
  status: AutonomousUnitStatusDto
  statusLabel: string
  summary: string
  boundaryId: string | null
  updatedAt: string
  latestAttemptOnlyLabel: string
  latestAttemptLabel: string
  latestAttemptStatusLabel: string
  latestAttemptUpdatedAt: string | null
  latestAttemptSummary: string
  latestAttemptId: string | null
  latestAttemptNumber: number | null
  latestAttemptChildSessionId: string | null
  workflowState: RecentAutonomousUnitWorkflowState
  workflowStateLabel: string
  workflowNodeLabel: string
  workflowLinkageLabel: string
  workflowLinkageSource: RecentAutonomousUnitLinkageSource
  workflowNodeId: string | null
  workflowTransitionId: string | null
  workflowCausalTransitionId: string | null
  workflowHandoffTransitionId: string | null
  workflowHandoffPackageHash: string | null
  workflowDetail: string
  evidenceCount: number
  evidenceStateLabel: string
  evidenceSummary: string
  latestEvidenceAt: string | null
  evidencePreviews: RecentAutonomousUnitEvidencePreview[]
}

export interface RecentAutonomousUnitsProjectionView {
  items: RecentAutonomousUnitCardView[]
  totalCount: number
  visibleCount: number
  hiddenCount: number
  isTruncated: boolean
  windowLabel: string
  latestAttemptOnlyCopy: string
  emptyTitle: string
  emptyBody: string
}

interface RecentAutonomousUnitWorkflowContext {
  state: RecentAutonomousUnitWorkflowState
  stateLabel: string
  nodeLabel: string
  linkageLabel: string
  linkageSource: RecentAutonomousUnitLinkageSource
  workflowNodeId: string | null
  workflowTransitionId: string | null
  workflowCausalTransitionId: string | null
  workflowHandoffTransitionId: string | null
  workflowHandoffPackageHash: string | null
  detail: string
}

interface WorkflowProjectionMaps {
  stageByNodeId: Map<string, PlanningLifecycleStageView>
  handoffByTransitionId: Map<string, WorkflowHandoffPackageView>
  pendingApprovalByNodeId: Map<string, OperatorApprovalView>
}

function getWorkflowStateLabel(state: RecentAutonomousUnitWorkflowState): string {
  switch (state) {
    case 'ready':
      return 'In sync'
    case 'awaiting_snapshot':
      return 'Snapshot lag'
    case 'awaiting_handoff':
      return 'Handoff pending'
    case 'unlinked':
      return 'Linkage pending'
  }
}

function buildWorkflowProjectionMaps(options: {
  lifecycle: PlanningLifecycleView
  handoffPackages: WorkflowHandoffPackageView[]
  approvalRequests: OperatorApprovalView[]
}): WorkflowProjectionMaps {
  const stageByNodeId = new Map<string, PlanningLifecycleStageView>()
  for (const stage of options.lifecycle.stages) {
    const nodeId = normalizeText(stage.nodeId)
    if (!nodeId || stageByNodeId.has(nodeId)) {
      continue
    }

    stageByNodeId.set(nodeId, stage)
  }

  const handoffByTransitionId = new Map<string, WorkflowHandoffPackageView>()
  for (const handoff of sortByNewest(options.handoffPackages, (pkg) => pkg.createdAt)) {
    const handoffTransitionId = normalizeText(handoff.handoffTransitionId)
    if (!handoffTransitionId || handoffByTransitionId.has(handoffTransitionId)) {
      continue
    }

    handoffByTransitionId.set(handoffTransitionId, handoff)
  }

  const pendingApprovalByNodeId = new Map<string, OperatorApprovalView>()
  for (const approval of sortByNewest(options.approvalRequests, (item) => item.updatedAt || item.createdAt)) {
    const gateNodeId = normalizeText(approval.gateNodeId)
    if (!approval.isPending || !gateNodeId || pendingApprovalByNodeId.has(gateNodeId)) {
      continue
    }

    pendingApprovalByNodeId.set(gateNodeId, approval)
  }

  return {
    stageByNodeId,
    handoffByTransitionId,
    pendingApprovalByNodeId,
  }
}

function getRecentUnitWorkflowContext(options: {
  entry: AutonomousUnitHistoryEntryView
  lifecycle: PlanningLifecycleView
  maps: WorkflowProjectionMaps
}): RecentAutonomousUnitWorkflowContext {
  const attemptLinkage = options.entry.latestAttempt?.workflowLinkage ?? null
  const unitLinkage = options.entry.unit.workflowLinkage ?? null
  const linkage = attemptLinkage ?? unitLinkage

  if (!linkage) {
    return {
      state: 'unlinked',
      stateLabel: getWorkflowStateLabel('unlinked'),
      nodeLabel: 'Not linked',
      linkageLabel: 'Workflow linkage missing',
      linkageSource: 'none',
      workflowNodeId: null,
      workflowTransitionId: null,
      workflowCausalTransitionId: null,
      workflowHandoffTransitionId: null,
      workflowHandoffPackageHash: null,
      detail: 'Cadence has not persisted workflow-node and handoff linkage for this unit yet.',
    }
  }

  const linkageSource: RecentAutonomousUnitLinkageSource = attemptLinkage ? 'attempt' : 'unit'
  const workflowNodeId = normalizeText(linkage.workflowNodeId) || null
  const workflowTransitionId = normalizeText(linkage.transitionId) || null
  const workflowCausalTransitionId = normalizeText(linkage.causalTransitionId) || null
  const workflowHandoffTransitionId = normalizeText(linkage.handoffTransitionId) || null
  const workflowHandoffPackageHash = normalizeText(linkage.handoffPackageHash) || null

  const linkedStage = workflowNodeId ? options.maps.stageByNodeId.get(workflowNodeId) ?? null : null
  const activeLifecycleStage = options.lifecycle.activeStage
  const handoff = workflowHandoffTransitionId
    ? options.maps.handoffByTransitionId.get(workflowHandoffTransitionId) ?? null
    : null
  const pendingApproval = workflowNodeId
    ? options.maps.pendingApprovalByNodeId.get(workflowNodeId) ?? null
    : null
  const activeStageMismatch = Boolean(activeLifecycleStage && workflowNodeId && activeLifecycleStage.nodeId !== workflowNodeId)
  const handoffHashMismatch = Boolean(handoff && workflowHandoffPackageHash && handoff.packageHash !== workflowHandoffPackageHash)

  let state: RecentAutonomousUnitWorkflowState
  let detail: string

  if (!workflowNodeId) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence persisted a workflow linkage row for this unit, but the workflow node id is missing, so lifecycle correlation stays anchored to snapshot truth.'
  } else if (!workflowTransitionId) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence persisted a workflow linkage row for this unit, but the workflow transition id is missing, so linkage identity remains pending until recovery catches up.'
  } else if (!linkedStage) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence has persisted workflow linkage for this unit, but the selected project snapshot has not exposed the linked lifecycle node yet.'
  } else if (activeStageMismatch) {
    state = 'awaiting_snapshot'
    detail = `Cadence is keeping lifecycle progression anchored to snapshot truth while the linked node \`${linkedStage.stageLabel}\` waits for the active lifecycle stage to catch up.`
  } else if (!workflowHandoffTransitionId) {
    state = 'awaiting_handoff'
    detail =
      'Cadence persisted workflow linkage for this unit, but the handoff transition id is missing, so handoff correlation remains pending.'
  } else if (!handoff) {
    state = 'awaiting_handoff'
    detail =
      'Cadence has persisted workflow linkage for this unit, but the linked handoff package is not visible in the selected project snapshot yet.'
  } else if (!workflowHandoffPackageHash) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence found the linked handoff transition in the selected project snapshot, but the persisted handoff hash is missing for this unit.'
  } else if (handoffHashMismatch) {
    state = 'awaiting_snapshot'
    detail =
      'Cadence found the linked handoff transition in the selected project snapshot, but the persisted handoff hash has not caught up to this unit yet.'
  } else {
    state = 'ready'
    detail = 'Lifecycle stage, autonomous linkage, and handoff package all agree on backend truth for this unit.'
  }

  if (pendingApproval) {
    detail = `${detail} Pending approval \`${pendingApproval.title}\` is still blocking continuation at this linked node.`
  }

  return {
    state,
    stateLabel: getWorkflowStateLabel(state),
    nodeLabel: linkedStage?.stageLabel ?? workflowNodeId ?? 'Workflow node unavailable',
    linkageLabel: linkageSource === 'attempt' ? 'Attempt linkage' : 'Unit linkage',
    linkageSource,
    workflowNodeId,
    workflowTransitionId,
    workflowCausalTransitionId,
    workflowHandoffTransitionId,
    workflowHandoffPackageHash,
    detail,
  }
}

function collectEvidenceByUnitId(options: {
  history: AutonomousUnitHistoryEntryView[]
  recentArtifacts: AutonomousUnitArtifactView[]
}): Map<string, AutonomousUnitArtifactView[]> {
  const validUnitIds = new Set(
    options.history
      .map((entry) => normalizeText(entry.unit.unitId))
      .filter((unitId): unitId is string => Boolean(unitId)),
  )

  const evidenceByUnitId = new Map<string, AutonomousUnitArtifactView[]>()
  const appendEvidence = (artifact: AutonomousUnitArtifactView) => {
    const unitId = normalizeText(artifact.unitId)
    const artifactId = normalizeText(artifact.artifactId)
    if (!unitId || !artifactId || !validUnitIds.has(unitId)) {
      return
    }

    const existing = evidenceByUnitId.get(unitId)
    if (existing) {
      existing.push(artifact)
      return
    }

    evidenceByUnitId.set(unitId, [artifact])
  }

  for (const entry of options.history) {
    for (const artifact of entry.artifacts ?? []) {
      appendEvidence(artifact)
    }
  }

  for (const artifact of options.recentArtifacts) {
    appendEvidence(artifact)
  }

  for (const [unitId, artifacts] of evidenceByUnitId.entries()) {
    const deduped = new Map<string, AutonomousUnitArtifactView>()
    for (const artifact of sortByNewest(artifacts, (item) => item.updatedAt || item.createdAt)) {
      const artifactId = normalizeText(artifact.artifactId)
      if (!artifactId || deduped.has(artifactId)) {
        continue
      }

      deduped.set(artifactId, artifact)
    }

    evidenceByUnitId.set(unitId, [...deduped.values()])
  }

  return evidenceByUnitId
}

function getEvidenceProjection(options: {
  entry: AutonomousUnitHistoryEntryView
  evidenceByUnitId: Map<string, AutonomousUnitArtifactView[]>
}): Pick<
  RecentAutonomousUnitCardView,
  'evidenceCount' | 'evidenceStateLabel' | 'evidenceSummary' | 'latestEvidenceAt' | 'evidencePreviews'
> {
  const unitId = normalizeText(options.entry.unit.unitId)
  const latestAttemptId = normalizeText(options.entry.latestAttempt?.attemptId)
  const matchedEvidence = unitId ? options.evidenceByUnitId.get(unitId) ?? [] : []
  const filteredEvidence = matchedEvidence.filter((artifact) => {
    if (!latestAttemptId) {
      return true
    }

    return artifact.attemptId === latestAttemptId
  })
  const evidenceCount = filteredEvidence.length
  const evidencePreviews = filteredEvidence.slice(0, MAX_RECENT_AUTONOMOUS_EVIDENCE_PREVIEWS).map((artifact) => ({
    artifactId: artifact.artifactId,
    artifactKindLabel: artifact.artifactKindLabel,
    statusLabel: artifact.statusLabel,
    summary: artifact.summary,
    updatedAt: artifact.updatedAt || artifact.createdAt,
  }))

  if (evidenceCount === 0) {
    return {
      evidenceCount: 0,
      evidenceStateLabel: 'No durable evidence in bounded window',
      evidenceSummary: latestAttemptId
        ? 'Cadence has not retained a matching artifact for the latest attempt inside the bounded evidence window.'
        : 'Cadence has not retained a matching artifact for this unit inside the bounded evidence window.',
      latestEvidenceAt: null,
      evidencePreviews: [],
    }
  }

  return {
    evidenceCount,
    evidenceStateLabel: `${evidenceCount} recent evidence row${evidenceCount === 1 ? '' : 's'}`,
    evidenceSummary:
      evidenceCount === 1
        ? 'Showing the latest durable evidence row linked to this unit.'
        : 'Showing the newest durable evidence rows linked to this unit.',
    latestEvidenceAt: filteredEvidence[0]?.updatedAt || filteredEvidence[0]?.createdAt || null,
    evidencePreviews,
  }
}

function getLatestAttemptProjection(entry: AutonomousUnitHistoryEntryView): Pick<
  RecentAutonomousUnitCardView,
  | 'latestAttemptLabel'
  | 'latestAttemptStatusLabel'
  | 'latestAttemptUpdatedAt'
  | 'latestAttemptSummary'
  | 'latestAttemptId'
  | 'latestAttemptNumber'
  | 'latestAttemptChildSessionId'
> {
  const latestAttempt = entry.latestAttempt
  if (!latestAttempt) {
    return {
      latestAttemptLabel: 'Latest attempt unavailable',
      latestAttemptStatusLabel: 'Not recorded',
      latestAttemptUpdatedAt: null,
      latestAttemptSummary: 'Cadence has not persisted a latest-attempt row for this unit yet.',
      latestAttemptId: null,
      latestAttemptNumber: null,
      latestAttemptChildSessionId: null,
    }
  }

  const latestAttemptId = normalizeText(latestAttempt.attemptId) || null
  const latestAttemptNumber = Number.isFinite(latestAttempt.attemptNumber) ? latestAttempt.attemptNumber : null
  const latestAttemptChildSessionId = normalizeText(latestAttempt.childSessionId) || null

  return {
    latestAttemptLabel: latestAttemptNumber != null ? `Attempt #${latestAttemptNumber}` : 'Attempt unavailable',
    latestAttemptStatusLabel: latestAttempt.statusLabel,
    latestAttemptUpdatedAt: latestAttempt.updatedAt,
    latestAttemptSummary: latestAttemptChildSessionId
      ? `Latest durable attempt is ${latestAttempt.statusLabel.toLowerCase()} for child session ${latestAttemptChildSessionId}.`
      : `Latest durable attempt is ${latestAttempt.statusLabel.toLowerCase()}, but child-session linkage is unavailable.`,
    latestAttemptId,
    latestAttemptNumber,
    latestAttemptChildSessionId,
  }
}

function compareRecentHistoryEntries(
  left: AutonomousUnitHistoryEntryView,
  right: AutonomousUnitHistoryEntryView,
): number {
  const leftTime = getTimestampMs(
    left.latestAttempt?.updatedAt || left.unit.updatedAt || left.unit.finishedAt || left.unit.startedAt,
  )
  const rightTime = getTimestampMs(
    right.latestAttempt?.updatedAt || right.unit.updatedAt || right.unit.finishedAt || right.unit.startedAt,
  )

  if (leftTime !== rightTime) {
    return rightTime - leftTime
  }

  return right.unit.sequence - left.unit.sequence
}

export function projectRecentAutonomousUnits(options: {
  autonomousHistory: AutonomousUnitHistoryEntryView[]
  autonomousRecentArtifacts: AutonomousUnitArtifactView[]
  lifecycle: PlanningLifecycleView
  handoffPackages: WorkflowHandoffPackageView[]
  approvalRequests: OperatorApprovalView[]
  limit?: number
}): RecentAutonomousUnitsProjectionView {
  const limit = Math.max(1, options.limit ?? MAX_RECENT_AUTONOMOUS_UNITS)
  const history = options.autonomousHistory
    .filter((entry) => Boolean(normalizeText(entry.unit.unitId)))
    .sort(compareRecentHistoryEntries)
  const visibleHistory = history.slice(0, limit)
  const maps = buildWorkflowProjectionMaps({
    lifecycle: options.lifecycle,
    handoffPackages: options.handoffPackages,
    approvalRequests: options.approvalRequests,
  })
  const evidenceByUnitId = collectEvidenceByUnitId({
    history,
    recentArtifacts: options.autonomousRecentArtifacts,
  })

  const items = visibleHistory.map((entry) => {
    const workflow = getRecentUnitWorkflowContext({
      entry,
      lifecycle: options.lifecycle,
      maps,
    })
    const evidence = getEvidenceProjection({
      entry,
      evidenceByUnitId,
    })
    const latestAttempt = getLatestAttemptProjection(entry)

    return {
      unitId: entry.unit.unitId,
      sequence: entry.unit.sequence,
      sequenceLabel: entry.unit.sequence > 0 ? `#${entry.unit.sequence}` : 'Not observed',
      kindLabel: entry.unit.kindLabel,
      status: entry.unit.status,
      statusLabel: entry.unit.statusLabel,
      summary: entry.unit.summary,
      boundaryId: entry.unit.boundaryId,
      updatedAt: entry.unit.updatedAt,
      latestAttemptOnlyLabel: 'Only the latest attempt is shown for this unit.',
      latestAttemptLabel: latestAttempt.latestAttemptLabel,
      latestAttemptStatusLabel: latestAttempt.latestAttemptStatusLabel,
      latestAttemptUpdatedAt: latestAttempt.latestAttemptUpdatedAt,
      latestAttemptSummary: latestAttempt.latestAttemptSummary,
      latestAttemptId: latestAttempt.latestAttemptId,
      latestAttemptNumber: latestAttempt.latestAttemptNumber,
      latestAttemptChildSessionId: latestAttempt.latestAttemptChildSessionId,
      workflowState: workflow.state,
      workflowStateLabel: workflow.stateLabel,
      workflowNodeLabel: workflow.nodeLabel,
      workflowLinkageLabel: workflow.linkageLabel,
      workflowLinkageSource: workflow.linkageSource,
      workflowNodeId: workflow.workflowNodeId,
      workflowTransitionId: workflow.workflowTransitionId,
      workflowCausalTransitionId: workflow.workflowCausalTransitionId,
      workflowHandoffTransitionId: workflow.workflowHandoffTransitionId,
      workflowHandoffPackageHash: workflow.workflowHandoffPackageHash,
      workflowDetail: workflow.detail,
      evidenceCount: evidence.evidenceCount,
      evidenceStateLabel: evidence.evidenceStateLabel,
      evidenceSummary: evidence.evidenceSummary,
      latestEvidenceAt: evidence.latestEvidenceAt,
      evidencePreviews: evidence.evidencePreviews,
    } satisfies RecentAutonomousUnitCardView
  })

  const totalCount = history.length
  const visibleCount = items.length
  const hiddenCount = Math.max(0, totalCount - visibleCount)
  const isTruncated = hiddenCount > 0

  return {
    items,
    totalCount,
    visibleCount,
    hiddenCount,
    isTruncated,
    windowLabel: isTruncated
      ? `Showing ${visibleCount} of ${totalCount} durable units in the bounded recent-history window.`
      : visibleCount > 0
        ? `Showing ${visibleCount} durable unit${visibleCount === 1 ? '' : 's'} from the recent-history window.`
        : 'No durable recent units are available yet.',
    latestAttemptOnlyCopy: 'Only the latest durable attempt per unit is shown here.',
    emptyTitle: 'No recent autonomous units recorded',
    emptyBody: 'Cadence has not persisted a bounded autonomous unit history for this project yet.',
  }
}
