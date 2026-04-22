import { describe, expect, it } from 'vitest'
import type {
  AutonomousUnitArtifactView,
  AutonomousUnitAttemptView,
  AutonomousUnitHistoryEntryView,
  AutonomousUnitView,
  NotificationBrokerActionView,
  NotificationBrokerView,
  NotificationDispatchView,
  OperatorApprovalView,
  PlanningLifecycleStageView,
  PlanningLifecycleView,
  ResumeHistoryEntryView,
  RuntimeStreamActionRequiredItemView,
  WorkflowHandoffPackageView,
} from '@/src/lib/cadence-model'
import {
  MAX_RECENT_AUTONOMOUS_UNITS,
  projectCheckpointControlLoops,
  projectRecentAutonomousUnits,
} from '@/src/features/cadence/agent-runtime-projections'

function makeStage(overrides: Partial<PlanningLifecycleStageView> = {}): PlanningLifecycleStageView {
  return {
    stage: 'research',
    stageLabel: 'Research',
    nodeId: 'workflow-research',
    nodeLabel: 'Research',
    status: 'active',
    statusLabel: 'Active',
    actionRequired: false,
    lastTransitionAt: '2026-04-16T12:01:00Z',
    ...overrides,
  }
}

function makeLifecycle(overrides: Partial<PlanningLifecycleView> = {}): PlanningLifecycleView {
  const activeStage = overrides.activeStage ?? makeStage()
  const stages = overrides.stages ?? [activeStage]

  return {
    stages,
    byStage: {
      discussion: null,
      research: activeStage.stage === 'research' ? activeStage : null,
      requirements: activeStage.stage === 'requirements' ? activeStage : null,
      roadmap: activeStage.stage === 'roadmap' ? activeStage : null,
    },
    hasStages: stages.length > 0,
    activeStage,
    actionRequiredCount: 0,
    blockedCount: 0,
    completedCount: 0,
    percentComplete: 50,
    ...overrides,
  }
}

function makeApproval(overrides: Partial<OperatorApprovalView> = {}): OperatorApprovalView {
  return {
    actionId: 'action-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_worktree',
    title: 'Review worktree changes',
    detail: 'Inspect the repo diff before continuing.',
    gateNodeId: 'workflow-research',
    gateKey: 'requires_user_input',
    transitionFromNodeId: 'workflow-discussion',
    transitionToNodeId: 'workflow-research',
    transitionKind: 'advance',
    userAnswer: null,
    status: 'pending',
    statusLabel: 'Pending',
    decisionNote: null,
    createdAt: '2026-04-16T12:00:00Z',
    updatedAt: '2026-04-16T12:00:00Z',
    resolvedAt: null,
    isPending: true,
    isResolved: false,
    canResume: false,
    isGateLinked: true,
    isRuntimeResumable: false,
    requiresUserAnswer: true,
    answerRequirementReason: 'gate_linked',
    answerRequirementLabel: 'Required',
    answerShapeKind: 'plain_text',
    answerShapeLabel: 'Plain-text response',
    answerShapeHint: 'Provide plain-text operator context.',
    answerPlaceholder: 'Provide operator input for this action.',
    ...overrides,
  }
}

function makeUnit(overrides: Partial<AutonomousUnitView> = {}): AutonomousUnitView {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'unit-1',
    sequence: 1,
    kind: 'state',
    kindLabel: 'State',
    status: 'completed',
    statusLabel: 'Completed',
    summary: 'Recovered the current autonomous boundary.',
    boundaryId: 'boundary-1',
    workflowLinkage: null,
    startedAt: '2026-04-16T12:00:00Z',
    finishedAt: '2026-04-16T12:01:00Z',
    updatedAt: '2026-04-16T12:01:00Z',
    lastErrorCode: null,
    lastError: null,
    isActive: false,
    isTerminal: true,
    isFailed: false,
    ...overrides,
  }
}

function makeAttempt(overrides: Partial<AutonomousUnitAttemptView> = {}): AutonomousUnitAttemptView {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'unit-1',
    attemptId: 'unit-1:attempt-1',
    attemptNumber: 1,
    childSessionId: 'child-session-1',
    status: 'completed',
    statusLabel: 'Completed',
    boundaryId: 'boundary-1',
    workflowLinkage: {
      workflowNodeId: 'workflow-research',
      transitionId: 'transition-1',
      causalTransitionId: 'causal-1',
      handoffTransitionId: 'handoff-1',
      handoffPackageHash: 'hash-1',
    },
    startedAt: '2026-04-16T12:00:10Z',
    finishedAt: '2026-04-16T12:00:50Z',
    updatedAt: '2026-04-16T12:00:50Z',
    lastErrorCode: null,
    lastError: null,
    isActive: false,
    isTerminal: true,
    isFailed: false,
    ...overrides,
  }
}

function makeArtifact(overrides: Partial<AutonomousUnitArtifactView> = {}): AutonomousUnitArtifactView {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'unit-1',
    attemptId: 'unit-1:attempt-1',
    artifactId: 'artifact-1',
    artifactKind: 'tool_result',
    artifactKindLabel: 'Tool result',
    status: 'recorded',
    statusLabel: 'Recorded',
    summary: 'Read README.md from the repository root.',
    contentHash: 'hash',
    payload: null,
    createdAt: '2026-04-16T12:00:30Z',
    updatedAt: '2026-04-16T12:00:30Z',
    detail: 'Tool `read` succeeded.',
    commandResult: {
      exitCode: 0,
      timedOut: false,
      summary: 'read completed',
    },
    toolSummary: null,
    toolName: 'read',
    toolState: 'succeeded',
    toolStateLabel: 'Succeeded',
    evidenceKind: null,
    verificationOutcome: null,
    verificationOutcomeLabel: null,
    diagnosticCode: null,
    actionId: null,
    boundaryId: null,
    isToolResult: true,
    isVerificationEvidence: false,
    isPolicyDenied: false,
    ...overrides,
  }
}

function makeHistoryEntry(overrides: Partial<AutonomousUnitHistoryEntryView> = {}): AutonomousUnitHistoryEntryView {
  const unit = overrides.unit ?? makeUnit()
  const latestAttempt =
    overrides.latestAttempt === undefined
      ? makeAttempt({
          unitId: unit.unitId,
          attemptId: `${unit.unitId}:attempt-1`,
          boundaryId: unit.boundaryId,
        })
      : overrides.latestAttempt

  return {
    unit,
    latestAttempt,
    artifacts: overrides.artifacts ?? [],
  }
}

function makeHandoff(overrides: Partial<WorkflowHandoffPackageView> = {}): WorkflowHandoffPackageView {
  return {
    id: 1,
    projectId: 'project-1',
    handoffTransitionId: 'handoff-1',
    causalTransitionId: 'causal-1',
    fromNodeId: 'workflow-discussion',
    toNodeId: 'workflow-research',
    transitionKind: 'advance',
    packagePayload: '{"schemaVersion":1}',
    packageHash: 'hash-1',
    createdAt: '2026-04-16T12:00:20Z',
    ...overrides,
  }
}

function makeActionRequired(
  overrides: Partial<RuntimeStreamActionRequiredItemView> = {},
): RuntimeStreamActionRequiredItemView {
  return {
    id: 'action-required-1',
    kind: 'action_required',
    runId: 'run-1',
    sequence: 1,
    createdAt: '2026-04-16T12:00:40Z',
    actionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
    boundaryId: 'boundary-1',
    actionType: 'terminal_input_required',
    title: 'Terminal input required',
    detail: 'Provide terminal input before the run can continue.',
    ...overrides,
  }
}

function makeResumeEntry(overrides: Partial<ResumeHistoryEntryView> = {}): ResumeHistoryEntryView {
  return {
    id: 1,
    sourceActionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
    sessionId: 'session-1',
    status: 'started',
    statusLabel: 'Resume started',
    summary: 'Operator resumed the selected project runtime session.',
    createdAt: '2026-04-16T12:02:00Z',
    ...overrides,
  }
}

function makeDispatch(overrides: Partial<NotificationDispatchView> = {}): NotificationDispatchView {
  return {
    id: 1,
    projectId: 'project-1',
    actionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
    routeId: 'telegram-primary',
    correlationKey: 'corr-1',
    status: 'failed',
    statusLabel: 'Failed',
    attemptCount: 2,
    lastAttemptAt: '2026-04-16T12:01:20Z',
    deliveredAt: null,
    claimedAt: null,
    lastErrorCode: 'notification_dispatch_http_500',
    lastErrorMessage: 'Telegram returned HTTP 500.',
    createdAt: '2026-04-16T12:01:00Z',
    updatedAt: '2026-04-16T12:01:20Z',
    isPending: false,
    isSent: false,
    isFailed: true,
    isClaimed: false,
    hasFailureDiagnostics: true,
    ...overrides,
  }
}

function makeBrokerAction(overrides: Partial<NotificationBrokerActionView> = {}): NotificationBrokerActionView {
  const dispatches = overrides.dispatches ?? [makeDispatch()]

  return {
    actionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
    dispatches,
    dispatchCount: dispatches.length,
    pendingCount: dispatches.filter((dispatch) => dispatch.isPending).length,
    sentCount: dispatches.filter((dispatch) => dispatch.isSent).length,
    failedCount: dispatches.filter((dispatch) => dispatch.isFailed).length,
    claimedCount: dispatches.filter((dispatch) => dispatch.isClaimed).length,
    latestUpdatedAt: dispatches[0]?.updatedAt ?? null,
    hasFailures: dispatches.some((dispatch) => dispatch.isFailed),
    hasPending: dispatches.some((dispatch) => dispatch.isPending),
    hasClaimed: dispatches.some((dispatch) => dispatch.isClaimed),
    ...overrides,
  }
}

function makeBrokerView(overrides: Partial<NotificationBrokerView> = {}): NotificationBrokerView {
  const actions = overrides.actions ?? []

  return {
    dispatches: overrides.dispatches ?? actions.flatMap((action) => action.dispatches),
    actions,
    routes: overrides.routes ?? [],
    byActionId:
      overrides.byActionId ??
      Object.fromEntries(actions.map((action) => [action.actionId, action] as const)),
    byRouteId: overrides.byRouteId ?? {},
    dispatchCount: overrides.dispatchCount ?? actions.reduce((total, action) => total + action.dispatchCount, 0),
    routeCount: overrides.routeCount ?? 0,
    pendingCount: overrides.pendingCount ?? actions.reduce((total, action) => total + action.pendingCount, 0),
    sentCount: overrides.sentCount ?? actions.reduce((total, action) => total + action.sentCount, 0),
    failedCount: overrides.failedCount ?? actions.reduce((total, action) => total + action.failedCount, 0),
    claimedCount: overrides.claimedCount ?? actions.reduce((total, action) => total + action.claimedCount, 0),
    latestUpdatedAt: overrides.latestUpdatedAt ?? actions[0]?.latestUpdatedAt ?? null,
    isTruncated: overrides.isTruncated ?? false,
    totalBeforeTruncation: overrides.totalBeforeTruncation ?? actions.reduce((total, action) => total + action.dispatchCount, 0),
  }
}

describe('projectRecentAutonomousUnits', () => {
  it('projects newest-first units with linked workflow truth and per-unit evidence summaries', () => {
    const lifecycle = makeLifecycle()
    const history = [
      makeHistoryEntry({
        unit: makeUnit({
          unitId: 'unit-older',
          sequence: 1,
          summary: 'Older durable unit.',
          updatedAt: '2026-04-16T12:00:00Z',
        }),
        latestAttempt: makeAttempt({
          unitId: 'unit-older',
          attemptId: 'unit-older:attempt-1',
          childSessionId: 'child-session-older',
          updatedAt: '2026-04-16T12:00:00Z',
        }),
      }),
      makeHistoryEntry({
        unit: makeUnit({
          unitId: 'unit-newer',
          sequence: 2,
          status: 'blocked',
          statusLabel: 'Blocked',
          summary: 'Blocked on operator boundary.',
          updatedAt: '2026-04-16T12:02:00Z',
        }),
        latestAttempt: makeAttempt({
          unitId: 'unit-newer',
          attemptId: 'unit-newer:attempt-2',
          attemptNumber: 2,
          status: 'blocked',
          statusLabel: 'Blocked',
          childSessionId: 'child-session-newer',
          updatedAt: '2026-04-16T12:02:00Z',
        }),
      }),
    ]

    const projection = projectRecentAutonomousUnits({
      autonomousHistory: history,
      autonomousRecentArtifacts: [
        makeArtifact({
          unitId: 'unit-newer',
          attemptId: 'unit-newer:attempt-2',
          artifactId: 'artifact-newer',
          summary: 'Captured operator approval evidence.',
          updatedAt: '2026-04-16T12:02:10Z',
        }),
        makeArtifact({
          unitId: 'ghost-unit',
          attemptId: 'ghost:attempt-1',
          artifactId: 'artifact-orphan',
          summary: 'Orphaned artifact should be ignored.',
          updatedAt: '2026-04-16T12:03:00Z',
        }),
      ],
      lifecycle,
      handoffPackages: [makeHandoff()],
      approvalRequests: [makeApproval()],
    })

    expect(projection.items).toHaveLength(2)
    expect(projection.items[0].unitId).toBe('unit-newer')
    expect(projection.items[0].workflowStateLabel).toBe('In sync')
    expect(projection.items[0].workflowLinkageLabel).toBe('Attempt linkage')
    expect(projection.items[0].evidenceCount).toBe(1)
    expect(projection.items[0].evidencePreviews[0]?.summary).toBe('Captured operator approval evidence.')
    expect(projection.items[0].latestAttemptSummary).toContain('child-session-newer')
    expect(projection.latestAttemptOnlyCopy).toBe('Only the latest durable attempt per unit is shown here.')
    expect(projection.windowLabel).toContain('Showing 2 durable units')
    expect(projection.items[1].evidenceStateLabel).toBe('No durable evidence in bounded window')
  })

  it('truncates to the bounded window, drops blank unit ids, and keeps missing latest attempts truthful', () => {
    const history = Array.from({ length: MAX_RECENT_AUTONOMOUS_UNITS + 2 }, (_, index) => {
      const unitIndex = index + 1
      return makeHistoryEntry({
        unit: makeUnit({
          unitId: unitIndex === 1 ? '   ' : `unit-${unitIndex}`,
          sequence: unitIndex,
          summary: `Durable unit ${unitIndex}`,
          updatedAt: `2026-04-16T12:${String(unitIndex).padStart(2, '0')}:00Z`,
        }),
        latestAttempt:
          unitIndex === MAX_RECENT_AUTONOMOUS_UNITS + 2
            ? null
            : makeAttempt({
                unitId: `unit-${unitIndex}`,
                attemptId: `unit-${unitIndex}:attempt-1`,
                updatedAt: `2026-04-16T12:${String(unitIndex).padStart(2, '0')}:10Z`,
              }),
      })
    })

    const projection = projectRecentAutonomousUnits({
      autonomousHistory: history,
      autonomousRecentArtifacts: [
        makeArtifact({
          unitId: 'ghost-unit',
          attemptId: 'ghost:attempt-1',
          artifactId: 'artifact-orphan',
          summary: 'This orphan artifact must not leak.',
        }),
      ],
      lifecycle: makeLifecycle(),
      handoffPackages: [makeHandoff()],
      approvalRequests: [],
    })

    expect(projection.totalCount).toBe(MAX_RECENT_AUTONOMOUS_UNITS + 1)
    expect(projection.visibleCount).toBe(MAX_RECENT_AUTONOMOUS_UNITS)
    expect(projection.isTruncated).toBe(true)
    expect(projection.hiddenCount).toBe(1)
    expect(projection.windowLabel).toContain(`Showing ${MAX_RECENT_AUTONOMOUS_UNITS} of ${MAX_RECENT_AUTONOMOUS_UNITS + 1}`)
    expect(projection.items.some((item) => item.unitId.trim().length === 0)).toBe(false)
    expect(projection.items.some((item) => item.evidencePreviews.some((artifact) => artifact.summary.includes('orphan')))).toBe(false)

    const missingAttemptUnit = projection.items.find(
      (item) => item.unitId === `unit-${MAX_RECENT_AUTONOMOUS_UNITS + 2}`,
    )
    expect(missingAttemptUnit?.latestAttemptStatusLabel).toBe('Not recorded')
    expect(missingAttemptUnit?.latestAttemptSummary).toBe(
      'Cadence has not persisted a latest-attempt row for this unit yet.',
    )
  })

  it('marks snapshot lag and handoff pending states without inventing linked-stage truth', () => {
    const projection = projectRecentAutonomousUnits({
      autonomousHistory: [
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-snapshot-lag',
            sequence: 2,
            updatedAt: '2026-04-16T12:02:00Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-snapshot-lag',
            attemptId: 'unit-snapshot-lag:attempt-1',
            workflowLinkage: {
              workflowNodeId: 'workflow-requirements',
              transitionId: 'transition-2',
              causalTransitionId: 'causal-2',
              handoffTransitionId: 'handoff-2',
              handoffPackageHash: 'hash-2',
            },
          }),
        }),
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-handoff-pending',
            sequence: 1,
            updatedAt: '2026-04-16T12:01:00Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-handoff-pending',
            attemptId: 'unit-handoff-pending:attempt-1',
            workflowLinkage: {
              workflowNodeId: 'workflow-research',
              transitionId: 'transition-3',
              causalTransitionId: 'causal-3',
              handoffTransitionId: 'handoff-missing',
              handoffPackageHash: 'hash-missing',
            },
          }),
        }),
      ],
      autonomousRecentArtifacts: [],
      lifecycle: makeLifecycle({
        stages: [
          makeStage({
            stage: 'research',
            stageLabel: 'Research',
            nodeId: 'workflow-research',
            nodeLabel: 'Research',
          }),
        ],
        activeStage: makeStage({
          stage: 'research',
          stageLabel: 'Research',
          nodeId: 'workflow-research',
          nodeLabel: 'Research',
        }),
      }),
      handoffPackages: [makeHandoff({ handoffTransitionId: 'handoff-2', packageHash: 'hash-2' })],
      approvalRequests: [],
    })

    const snapshotLagUnit = projection.items.find((item) => item.unitId === 'unit-snapshot-lag')
    const handoffPendingUnit = projection.items.find((item) => item.unitId === 'unit-handoff-pending')

    expect(snapshotLagUnit?.workflowStateLabel).toBe('Snapshot lag')
    expect(snapshotLagUnit?.workflowNodeLabel).toBe('workflow-requirements')
    expect(snapshotLagUnit?.workflowDetail).toContain('selected project snapshot has not exposed the linked lifecycle node yet')
    expect(handoffPendingUnit?.workflowStateLabel).toBe('Handoff pending')
    expect(handoffPendingUnit?.workflowDetail).toContain('linked handoff package is not visible')
  })
})

describe('projectCheckpointControlLoops', () => {
  it('correlates live, durable, broker, resume, and evidence truth for the same action boundary', () => {
    const liveAndDurableActionId = 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required'
    const recoveredActionId = 'flow:flow-1:run:run-1:boundary:boundary-2:terminal_input_required'

    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [
        makeActionRequired({
          actionId: liveAndDurableActionId,
          boundaryId: 'boundary-1',
          createdAt: '2026-04-16T12:01:30Z',
        }),
      ],
      approvalRequests: [
        makeApproval({
          actionId: liveAndDurableActionId,
          actionType: 'terminal_input_required',
          title: 'Terminal input required',
          detail: 'Provide terminal input before the run can continue.',
          gateNodeId: null,
          gateKey: null,
          transitionFromNodeId: null,
          transitionToNodeId: null,
          transitionKind: null,
          answerShapeHint: 'Provide terminal input to continue this boundary.',
          answerPlaceholder: 'Provide terminal input.',
        }),
      ],
      resumeHistory: [
        makeResumeEntry({
          id: 1,
          sourceActionId: recoveredActionId,
          status: 'started',
          statusLabel: 'Resume started',
          summary: 'Operator resumed the recovered checkpoint boundary.',
          createdAt: '2026-04-16T12:03:00Z',
        }),
      ],
      notificationBroker: makeBrokerView({
        actions: [
          makeBrokerAction({
            actionId: liveAndDurableActionId,
            dispatches: [
              makeDispatch({
                actionId: liveAndDurableActionId,
                routeId: 'telegram-primary',
                updatedAt: '2026-04-16T12:01:20Z',
              }),
              makeDispatch({
                id: 2,
                actionId: liveAndDurableActionId,
                routeId: 'discord-fallback',
                status: 'pending',
                statusLabel: 'Pending',
                attemptCount: 1,
                lastAttemptAt: null,
                lastErrorCode: null,
                lastErrorMessage: null,
                updatedAt: '2026-04-16T12:01:10Z',
                isPending: true,
                isSent: false,
                isFailed: false,
                isClaimed: false,
                hasFailureDiagnostics: false,
              }),
            ],
          }),
        ],
      }),
      autonomousHistory: [
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-boundary-1',
            boundaryId: 'boundary-1',
            updatedAt: '2026-04-16T12:01:00Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-boundary-1',
            attemptId: 'unit-boundary-1:attempt-1',
            boundaryId: 'boundary-1',
            updatedAt: '2026-04-16T12:01:00Z',
          }),
          artifacts: [
            makeArtifact({
              artifactId: 'artifact-boundary-1',
              unitId: 'unit-boundary-1',
              attemptId: 'unit-boundary-1:attempt-1',
              summary: 'Captured terminal input evidence.',
              actionId: liveAndDurableActionId,
              boundaryId: 'boundary-1',
              updatedAt: '2026-04-16T12:01:40Z',
            }),
          ],
        }),
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-boundary-2',
            boundaryId: 'boundary-2',
            updatedAt: '2026-04-16T12:02:30Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-boundary-2',
            attemptId: 'unit-boundary-2:attempt-1',
            boundaryId: 'boundary-2',
            updatedAt: '2026-04-16T12:02:30Z',
          }),
          artifacts: [
            makeArtifact({
              artifactId: 'artifact-boundary-2a',
              unitId: 'unit-boundary-2',
              attemptId: 'unit-boundary-2:attempt-1',
              summary: 'Resume evidence row 1.',
              actionId: recoveredActionId,
              boundaryId: 'boundary-2',
              updatedAt: '2026-04-16T12:03:10Z',
            }),
            makeArtifact({
              artifactId: 'artifact-boundary-2b',
              unitId: 'unit-boundary-2',
              attemptId: 'unit-boundary-2:attempt-1',
              summary: 'Resume evidence row 2.',
              actionId: recoveredActionId,
              boundaryId: 'boundary-2',
              updatedAt: '2026-04-16T12:03:05Z',
            }),
          ],
        }),
      ],
      autonomousRecentArtifacts: [],
    })

    expect(projection.items).toHaveLength(2)
    expect(projection.items[0].actionId).toBe(recoveredActionId)
    expect(projection.items[0].truthSource).toBe('recovered_durable')
    expect(projection.items[0].resumeStateLabel).toBe('Resume started')
    expect(projection.items[0].durableStateLabel).toBe('Approval cleared from durable snapshot')
    expect(projection.items[0].evidenceCount).toBe(2)
    expect(projection.items[0].evidencePreviews.map((artifact) => artifact.summary)).toEqual([
      'Resume evidence row 1.',
      'Resume evidence row 2.',
    ])

    expect(projection.items[1].actionId).toBe(liveAndDurableActionId)
    expect(projection.items[1].truthSource).toBe('live_and_durable')
    expect(projection.items[1].liveStateLabel).toBe('Live action required')
    expect(projection.items[1].durableStateLabel).toBe('Pending')
    expect(projection.items[1].brokerStateLabel).toBe('1 broker failure')
    expect(projection.items[1].brokerRoutePreviews).toHaveLength(2)
    expect(projection.items[1].evidenceStateLabel).toBe('1 durable evidence row')
    expect(projection.missingEvidenceCount).toBe(0)
    expect(projection.liveHintOnlyCount).toBe(0)
    expect(projection.recoveredCount).toBe(1)
  })

  it('ignores malformed rows, keeps broker mismatches isolated, and shows live-hint-only cards truthfully', () => {
    const actionId = 'flow:flow-1:run:run-1:boundary:boundary-3:terminal_input_required'

    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [
        makeActionRequired({
          actionId,
          boundaryId: 'boundary-3',
          title: 'Awaiting terminal input',
          detail: 'The live runtime stream still reports this boundary as blocked.',
          createdAt: '2026-04-16T12:05:00Z',
        }),
        makeActionRequired({
          id: 'malformed-live',
          actionId: '   ',
          boundaryId: 'boundary-ignored',
        }),
        makeActionRequired({
          id: 'missing-boundary',
          actionId: 'flow:flow-1:run:run-1:boundary:boundary-ignored:terminal_input_required',
          boundaryId: null,
        }),
      ],
      approvalRequests: [
        makeApproval({
          actionId: '   ',
          actionType: 'terminal_input_required',
        }),
      ],
      resumeHistory: [
        makeResumeEntry({
          sourceActionId: '   ',
        }),
      ],
      notificationBroker: makeBrokerView({
        actions: [
          makeBrokerAction({
            actionId: 'ghost-action',
            dispatches: [
              makeDispatch({
                actionId: 'ghost-action',
                routeId: 'ghost-route',
                status: 'sent',
                statusLabel: 'Sent',
                attemptCount: 1,
                lastAttemptAt: '2026-04-16T12:04:00Z',
                deliveredAt: '2026-04-16T12:04:00Z',
                lastErrorCode: null,
                lastErrorMessage: null,
                updatedAt: '2026-04-16T12:04:00Z',
                isPending: false,
                isSent: true,
                isFailed: false,
                isClaimed: false,
                hasFailureDiagnostics: false,
              }),
            ],
          }),
        ],
      }),
      autonomousHistory: [
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-boundary-3',
            boundaryId: 'boundary-3',
            updatedAt: '2026-04-16T12:04:30Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-boundary-3',
            attemptId: 'unit-boundary-3:attempt-1',
            boundaryId: 'boundary-3',
            updatedAt: '2026-04-16T12:04:30Z',
          }),
          artifacts: [
            makeArtifact({
              artifactId: 'artifact-without-action',
              unitId: 'unit-boundary-3',
              attemptId: 'unit-boundary-3:attempt-1',
              actionId: null,
              boundaryId: 'boundary-3',
              updatedAt: '2026-04-16T12:04:40Z',
            }),
          ],
        }),
      ],
      autonomousRecentArtifacts: [
        makeArtifact({
          artifactId: 'recent-artifact-without-action',
          actionId: null,
          boundaryId: 'boundary-3',
          updatedAt: '2026-04-16T12:04:50Z',
        }),
      ],
    })

    expect(projection.items).toHaveLength(1)
    expect(projection.items[0].actionId).toBe(actionId)
    expect(projection.items[0].truthSource).toBe('live_hint_only')
    expect(projection.items[0].liveStateLabel).toBe('Live action required')
    expect(projection.items[0].durableStateLabel).toBe('Durable approval pending refresh')
    expect(projection.items[0].brokerStateLabel).toBe('Broker diagnostics unavailable')
    expect(projection.items[0].evidenceStateLabel).toBe('No durable evidence in bounded window')
    expect(projection.liveHintOnlyCount).toBe(1)
    expect(projection.missingEvidenceCount).toBe(1)
    expect(projection.windowLabel).toContain('Showing 1 checkpoint action')
  })

  it('keeps recovered durable policy denials understandable without inventing approval or resume state', () => {
    const actionId = 'flow:flow-1:run:run-1:boundary:boundary-4:review_command'

    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [],
      approvalRequests: [],
      resumeHistory: [],
      notificationBroker: makeBrokerView(),
      autonomousHistory: [
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-boundary-4',
            boundaryId: 'boundary-4',
            updatedAt: '2026-04-16T12:06:00Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-boundary-4',
            attemptId: 'unit-boundary-4:attempt-1',
            boundaryId: 'boundary-4',
            updatedAt: '2026-04-16T12:06:00Z',
          }),
          artifacts: [
            makeArtifact({
              artifactId: 'artifact-policy-denied',
              unitId: 'unit-boundary-4',
              attemptId: 'unit-boundary-4:attempt-1',
              artifactKind: 'policy_denied',
              artifactKindLabel: 'Policy denied',
              summary: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
              detail: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
              diagnosticCode: 'policy_denied_command_cwd_outside_repo',
              actionId,
              boundaryId: 'boundary-4',
              isToolResult: false,
              isVerificationEvidence: false,
              isPolicyDenied: true,
              updatedAt: '2026-04-16T12:06:30Z',
            }),
            makeArtifact({
              artifactId: 'artifact-policy-denied-verification',
              unitId: 'unit-boundary-4',
              attemptId: 'unit-boundary-4:attempt-1',
              artifactKind: 'verification_evidence',
              artifactKindLabel: 'Verification evidence',
              summary: 'Autonomous attempt recorded stable policy denial `policy_denied_command_cwd_outside_repo`.',
              verificationOutcome: 'failed',
              verificationOutcomeLabel: 'Failed',
              actionId,
              boundaryId: 'boundary-4',
              isToolResult: false,
              isVerificationEvidence: true,
              isPolicyDenied: false,
              updatedAt: '2026-04-16T12:06:20Z',
            }),
          ],
        }),
      ],
      autonomousRecentArtifacts: [],
    })

    expect(projection.items).toHaveLength(1)
    expect(projection.items[0]).toMatchObject({
      actionId,
      truthSource: 'recovered_durable',
      truthSourceLabel: 'Recovered durable denial',
      liveStateLabel: 'No live review row',
      durableStateLabel: 'Policy denied',
      resumeStateLabel: 'Not resumable',
      evidenceCount: 2,
    })
    expect(projection.items[0].durableStateDetail).toContain('cwd escapes the imported repository root')
    expect(projection.recoveredCount).toBe(1)
  })

  it('ignores malformed durable policy denials without stable diagnostic codes or boundary linkage', () => {
    const actionId = 'flow:flow-1:run:run-1:boundary:boundary-ignored:review_command'

    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [],
      approvalRequests: [],
      resumeHistory: [],
      notificationBroker: makeBrokerView(),
      autonomousHistory: [
        makeHistoryEntry({
          unit: makeUnit({
            unitId: 'unit-boundary-ignored',
            boundaryId: 'boundary-ignored',
            updatedAt: '2026-04-16T12:07:00Z',
          }),
          latestAttempt: makeAttempt({
            unitId: 'unit-boundary-ignored',
            attemptId: 'unit-boundary-ignored:attempt-1',
            boundaryId: 'boundary-ignored',
            updatedAt: '2026-04-16T12:07:00Z',
          }),
          artifacts: [
            makeArtifact({
              artifactId: 'artifact-policy-denied-missing-diagnostic',
              unitId: 'unit-boundary-ignored',
              attemptId: 'unit-boundary-ignored:attempt-1',
              artifactKind: 'policy_denied',
              artifactKindLabel: 'Policy denied',
              summary: 'Cadence denied the shell command but did not persist a stable diagnostic code.',
              detail: 'Cadence denied the shell command but did not persist a stable diagnostic code.',
              diagnosticCode: null,
              actionId,
              boundaryId: 'boundary-ignored',
              isToolResult: false,
              isVerificationEvidence: false,
              isPolicyDenied: true,
              updatedAt: '2026-04-16T12:07:10Z',
            }),
            makeArtifact({
              artifactId: 'artifact-policy-denied-missing-boundary',
              unitId: 'unit-boundary-ignored',
              attemptId: 'unit-boundary-ignored:attempt-1',
              artifactKind: 'policy_denied',
              artifactKindLabel: 'Policy denied',
              summary: 'Cadence denied the shell command but did not persist a stable boundary id.',
              detail: 'Cadence denied the shell command but did not persist a stable boundary id.',
              diagnosticCode: 'policy_denied_command_cwd_outside_repo',
              actionId,
              boundaryId: null,
              isToolResult: false,
              isVerificationEvidence: false,
              isPolicyDenied: true,
              updatedAt: '2026-04-16T12:07:11Z',
            }),
          ],
        }),
      ],
      autonomousRecentArtifacts: [],
    })

    expect(projection.items).toHaveLength(0)
    expect(projection.recoveredCount).toBe(0)
  })
})
