import { describe, expect, it } from 'vitest'
import {
  applyRepositoryStatus,
  applyRuntimeSession,
  applyRuntimeStreamIssue,
  applyWorkflowTransitionRequestSchema,
  applyWorkflowTransitionResponseSchema,
  autonomousRunStateSchema,
  autonomousToolResultPayloadSchema,
  autonomousUnitAttemptSchema,
  autonomousUnitSchema,
  composeNotificationRouteTarget,
  createRuntimeStreamFromSubscription,
  decomposeNotificationRouteTarget,
  deriveAutonomousWorkflowContext,
  getRuntimeStreamStatusLabel,
  listNotificationDispatchesResponseSchema,
  listNotificationRoutesResponseSchema,
  mapAutonomousArtifact,
  mapAutonomousUnit,
  mapProjectSnapshot,
  mapProjectSummary,
  mapRepositoryStatus,
  mapRuntimeRun,
  mapRuntimeSession,
  mergeRuntimeStreamEvent,
  mergeRuntimeUpdated,
  projectSnapshotResponseSchema,
  resolveOperatorActionRequestSchema,
  runtimeRunSchema,
  runtimeSessionSchema,
  runtimeSettingsSchema,
  runtimeStreamItemSchema,
  runtimeUpdatedPayloadSchema,
  resumeOperatorRunRequestSchema,
  resumeOperatorRunResponseSchema,
  safePercent,
  submitNotificationReplyResponseSchema,
  syncNotificationAdaptersRequestSchema,
  syncNotificationAdaptersResponseSchema,
  subscribeRuntimeStreamResponseSchema,
  toolResultSummarySchema,
  upsertNotificationRouteCredentialsRequestSchema,
  upsertNotificationRouteCredentialsResponseSchema,
  upsertNotificationRouteRequestSchema,
  upsertRuntimeSettingsRequestSchema,
  upsertWorkflowGraphRequestSchema,
  upsertWorkflowGraphResponseSchema,
  type AutonomousUnitArtifactDto,
  type ProjectSnapshotResponseDto,
  type RepositoryStatusResponseDto,
  type RuntimeSessionDto,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemDto,
} from '@/src/lib/cadence-model'

function makeSnapshot(overrides: Partial<ProjectSnapshotResponseDto> = {}): ProjectSnapshotResponseDto {
  return {
    project: {
      id: 'project-1',
      name: 'Cadence',
      description: 'Desktop shell',
      milestone: 'M001',
      totalPhases: 3,
      completedPhases: 1,
      activePhase: 2,
      branch: 'main',
      runtime: 'codex',
    },
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/Cadence',
      displayName: 'Cadence',
      branch: 'main',
      headSha: 'abc123',
      isGitRepo: true,
    },
    phases: [
      {
        id: 1,
        name: 'Import',
        description: 'Import a repo',
        status: 'complete',
        currentStep: null,
        taskCount: 1,
        completedTasks: 1,
        summary: 'Imported successfully',
      },
      {
        id: 2,
        name: 'Live state',
        description: 'Connect the desktop shell',
        status: 'active',
        currentStep: 'execute',
        taskCount: 2,
        completedTasks: 1,
        summary: null,
      },
    ],
    lifecycle: {
      stages: [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-15T17:59:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: 'active',
          actionRequired: true,
          lastTransitionAt: '2026-04-15T18:00:00Z',
        },
      ],
    },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    ...overrides,
  }
}

function makeAutonomousRun(overrides: Partial<NonNullable<ProjectSnapshotResponseDto['autonomousRun']>> = {}) {
  return {
    projectId: 'project-1',
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    supervisorKind: 'detached_pty',
    status: 'stale' as const,
    recoveryState: 'recovery_required' as const,
    activeUnitId: 'run-1:checkpoint:2',
    duplicateStartDetected: false,
    duplicateStartRunId: null,
    duplicateStartReason: null,
    startedAt: '2026-04-15T23:10:00Z',
    lastHeartbeatAt: '2026-04-15T23:10:01Z',
    lastCheckpointAt: '2026-04-15T23:10:02Z',
    pausedAt: null,
    cancelledAt: null,
    completedAt: null,
    crashedAt: '2026-04-15T23:10:03Z',
    stoppedAt: null,
    pauseReason: null,
    cancelReason: null,
    crashReason: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Cadence could not connect to the detached supervisor control endpoint.',
    },
    lastErrorCode: 'runtime_supervisor_connect_failed',
    lastError: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Cadence could not connect to the detached supervisor control endpoint.',
    },
    updatedAt: '2026-04-15T23:10:03Z',
    ...overrides,
  }
}

function makeAutonomousUnit(overrides: Partial<NonNullable<ProjectSnapshotResponseDto['autonomousUnit']>> = {}) {
  return {
    projectId: 'project-1',
    runId: 'run-1',
    unitId: 'run-1:checkpoint:2',
    sequence: 2,
    kind: 'state' as const,
    status: 'active' as const,
    summary: 'Supervisor heartbeat recorded.',
    boundaryId: null,
    startedAt: '2026-04-15T23:10:02Z',
    finishedAt: null,
    updatedAt: '2026-04-15T23:10:03Z',
    lastErrorCode: 'runtime_supervisor_connect_failed',
    lastError: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Cadence could not connect to the detached supervisor control endpoint.',
    },
    ...overrides,
  }
}

function makeHandoffPackage(projectId = 'project-1', transitionId = 'auto:txn-001') {
  return {
    id: 42,
    projectId,
    handoffTransitionId: transitionId,
    causalTransitionId: 'txn-000',
    fromNodeId: 'workflow-discussion',
    toNodeId: 'workflow-research',
    transitionKind: 'advance',
    packagePayload: '{"schemaVersion":1,"triggerTransition":{"transitionId":"auto:txn-001"}}',
    packageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
    createdAt: '2026-04-16T14:00:00Z',
  }
}

function makeNotificationDispatch(overrides: {
  id: number
  projectId?: string
  actionId?: string
  routeId?: string
  status?: 'pending' | 'sent' | 'failed' | 'claimed'
  attemptCount?: number
  lastAttemptAt?: string | null
  deliveredAt?: string | null
  claimedAt?: string | null
  lastErrorCode?: string | null
  lastErrorMessage?: string | null
  updatedAt?: string
}): NonNullable<ProjectSnapshotResponseDto['notificationDispatches']>[number] {
  const idHex = overrides.id.toString(16).padStart(32, 'a').slice(-32)

  return {
    id: overrides.id,
    projectId: overrides.projectId ?? 'project-1',
    actionId: overrides.actionId ?? 'scope:auto-dispatch:workflow-research:requires_user_input',
    routeId: overrides.routeId ?? 'telegram-primary',
    correlationKey: `nfy:${idHex}`,
    status: overrides.status ?? 'pending',
    attemptCount: overrides.attemptCount ?? 0,
    lastAttemptAt: overrides.lastAttemptAt ?? null,
    deliveredAt: overrides.deliveredAt ?? null,
    claimedAt: overrides.claimedAt ?? null,
    lastErrorCode: overrides.lastErrorCode ?? null,
    lastErrorMessage: overrides.lastErrorMessage ?? null,
    createdAt: '2026-04-16T14:00:00Z',
    updatedAt: overrides.updatedAt ?? '2026-04-16T14:00:00Z',
  }
}

function makeNotificationRoute(overrides: {
  projectId?: string
  routeId?: string
  routeKind?: 'telegram' | 'discord'
  routeTarget?: string
  enabled?: boolean
  metadataJson?: string | null
  credentialReadiness?: {
    hasBotToken: boolean
    hasChatId: boolean
    hasWebhookUrl: boolean
    ready: boolean
    status: 'ready' | 'missing' | 'malformed' | 'unavailable'
    diagnostic?: {
      code: string
      message: string
      retryable: boolean
    } | null
  } | null
  createdAt?: string
  updatedAt?: string
} = {}) {
  const routeKind = overrides.routeKind ?? 'telegram'

  return {
    projectId: overrides.projectId ?? 'project-1',
    routeId: overrides.routeId ?? 'telegram-primary',
    routeKind,
    routeTarget: overrides.routeTarget ?? 'telegram:@Cadence_ops',
    enabled: overrides.enabled ?? true,
    metadataJson: overrides.metadataJson ?? '{"channel":"ops"}',
    credentialReadiness:
      overrides.credentialReadiness ??
      (routeKind === 'telegram'
        ? {
            hasBotToken: true,
            hasChatId: true,
            hasWebhookUrl: false,
            ready: true,
            status: 'ready',
            diagnostic: null,
          }
        : {
            hasBotToken: true,
            hasChatId: false,
            hasWebhookUrl: true,
            ready: true,
            status: 'ready',
            diagnostic: null,
          }),
    createdAt: overrides.createdAt ?? '2026-04-16T14:00:00Z',
    updatedAt: overrides.updatedAt ?? '2026-04-16T14:05:00Z',
  }
}

function makeStatus(overrides: Partial<RepositoryStatusResponseDto> = {}): RepositoryStatusResponseDto {
  return {
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/Cadence',
      displayName: 'Cadence',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: {
      name: 'feature/live-state',
      headSha: null,
      detached: false,
    },
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: 'modified',
        unstaged: null,
        untracked: false,
      },
      {
        path: 'client/src/lib/cadence-model.ts',
        staged: null,
        unstaged: 'added',
        untracked: true,
      },
    ],
    hasStagedChanges: true,
    hasUnstagedChanges: true,
    hasUntrackedChanges: true,
    ...overrides,
  }
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionDto> = {}): RuntimeSessionDto {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: 'flow-1',
    sessionId: 'session-1',
    accountId: 'acct-1',
    phase: 'authenticated',
    callbackBound: true,
    authorizationUrl: 'https://auth.openai.com/oauth/authorize',
    redirectUri: 'http://127.0.0.1:1455/auth/callback',
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-13T19:33:32Z',
    ...overrides,
  }
}

const streamSubscription = subscribeRuntimeStreamResponseSchema.parse({
  projectId: 'project-1',
  runtimeKind: 'openai_codex',
  runId: 'run-1',
  sessionId: 'session-1',
  flowId: 'flow-1',
  subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
})

function makeStreamEvent(
  item: Omit<RuntimeStreamItemDto, 'runId' | 'sequence'>,
  overrides: Partial<Omit<RuntimeStreamEventDto, 'projectId' | 'item'>> & {
    projectId?: string
    runtimeKind?: string
    runId?: string
    sessionId?: string
    flowId?: string | null
    sequence?: number
  } = {},
) {
  const sequence =
    overrides.sequence ??
    Math.max(1, Number.isFinite(Date.parse(String(item.createdAt))) ? Math.floor(Date.parse(String(item.createdAt)) / 1000) : 1)

  return {
    projectId: overrides.projectId ?? streamSubscription.projectId,
    runtimeKind: overrides.runtimeKind ?? streamSubscription.runtimeKind,
    runId: overrides.runId ?? streamSubscription.runId,
    sessionId: overrides.sessionId ?? streamSubscription.sessionId,
    flowId: overrides.flowId ?? streamSubscription.flowId ?? null,
    subscribedItemKinds: streamSubscription.subscribedItemKinds,
    item: runtimeStreamItemSchema.parse({
      runId: overrides.runId ?? streamSubscription.runId,
      sequence,
      ...item,
    }),
  }
}

describe('cadence-model', () => {
  it('maps nullable and blank project summary fields into UI-safe values', () => {
    const summary = mapProjectSummary({
      id: 'project-1',
      name: 'Cadence',
      description: '   ',
      milestone: '   ',
      totalPhases: 0,
      completedPhases: 3,
      activePhase: 0,
      branch: null,
      runtime: null,
    })

    expect(summary.description).toBe('No description provided.')
    expect(summary.milestone).toBe('No milestone assigned')
    expect(summary.branchLabel).toBe('No branch')
    expect(summary.runtimeLabel).toBe('Runtime unavailable')
    expect(summary.phaseProgressPercent).toBe(0)
  })

  it('maps persisted workflow phases into project detail views without fabricating summaries', () => {
    const project = mapProjectSnapshot(makeSnapshot())

    expect(project.phaseProgressPercent).toBe(33)
    expect(project.completedPhases).toBe(1)
    expect(project.repository?.headShaLabel).toBe('abc123')
    expect(project.lifecycle.hasStages).toBe(true)
    expect(project.lifecycle.activeStage?.stage).toBe('research')
    expect(project.lifecycle.actionRequiredCount).toBe(1)
    expect(project.phases).toHaveLength(2)
    expect(project.phases[0].summary).toBe('Imported successfully')
    expect(project.phases[1].summary).toBeUndefined()
    expect(project.phases[1].stepStatuses.execute).toBe('active')
    expect(project.phases[1].stepStatuses.verify).toBe('pending')
  })

  it('derives linked workflow context from autonomous linkage, lifecycle projection, and handoff truth', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-16T13:59:00Z',
            },
            {
              stage: 'research',
              nodeId: 'workflow-research',
              status: 'active',
              actionRequired: true,
              lastTransitionAt: '2026-04-16T14:00:00Z',
            },
          ],
        },
        approvalRequests: [
          {
            actionId: 'scope:auto-dispatch:workflow-research:requires_user_input',
            sessionId: 'session-1',
            flowId: 'flow-1',
            actionType: 'review_worktree',
            title: 'Review worktree changes',
            detail: 'Inspect the pending repository diff before continuing.',
            gateNodeId: 'workflow-research',
            gateKey: 'requires_user_input',
            transitionFromNodeId: 'workflow-discussion',
            transitionToNodeId: 'workflow-research',
            transitionKind: 'advance',
            userAnswer: null,
            status: 'pending',
            decisionNote: null,
            createdAt: '2026-04-16T14:00:00Z',
            updatedAt: '2026-04-16T14:00:00Z',
            resolvedAt: null,
          },
        ],
        handoffPackages: [makeHandoffPackage('project-1', 'auto:txn-001')],
      }),
    )

    const context = deriveAutonomousWorkflowContext({
      lifecycle: project.lifecycle,
      handoffPackages: project.handoffPackages,
      approvalRequests: project.approvalRequests,
      autonomousUnit: mapAutonomousUnit(
        autonomousUnitSchema.parse({
          ...makeAutonomousUnit(),
          workflowLinkage: {
            workflowNodeId: 'workflow-research',
            transitionId: 'auto:txn-001',
            causalTransitionId: 'txn-000',
            handoffTransitionId: 'auto:txn-001',
            handoffPackageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
          },
        }),
      ),
      autonomousAttempt: null,
    })

    expect(context).toMatchObject({
      linkageSource: 'unit',
      state: 'ready',
      stateLabel: 'In sync',
      linkedNodeLabel: 'Workflow Research',
      linkedStage: {
        stage: 'research',
        status: 'active',
      },
      handoff: {
        handoffTransitionId: 'auto:txn-001',
        packageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
      },
      pendingApproval: {
        actionId: 'scope:auto-dispatch:workflow-research:requires_user_input',
      },
    })
    expect(context?.detail).toContain('Pending approval')
  })

  it('flags linked workflow snapshot lag instead of fabricating lifecycle advancement when linkage outruns the active stage', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'active',
              actionRequired: false,
              lastTransitionAt: '2026-04-16T13:59:00Z',
            },
            {
              stage: 'research',
              nodeId: 'workflow-research',
              status: 'pending',
              actionRequired: true,
              lastTransitionAt: null,
            },
          ],
        },
      }),
    )

    const context = deriveAutonomousWorkflowContext({
      lifecycle: project.lifecycle,
      handoffPackages: project.handoffPackages,
      approvalRequests: project.approvalRequests,
      autonomousUnit: mapAutonomousUnit(
        autonomousUnitSchema.parse({
          ...makeAutonomousUnit(),
          workflowLinkage: {
            workflowNodeId: 'workflow-research',
            transitionId: 'auto:txn-002',
            causalTransitionId: 'txn-001',
            handoffTransitionId: 'auto:txn-002',
            handoffPackageHash: '1111111111111111111111111111111111111111111111111111111111111111',
          },
        }),
      ),
      autonomousAttempt: null,
    })

    expect(project.lifecycle.activeStage?.stage).toBe('discussion')
    expect(context).toMatchObject({
      state: 'awaiting_snapshot',
      stateLabel: 'Snapshot lag',
      linkedStage: {
        stage: 'research',
        status: 'pending',
      },
      activeLifecycleStage: {
        stage: 'discussion',
      },
      handoff: null,
    })
    expect(context?.detail).toContain('keeping lifecycle progression anchored to snapshot truth')
  })

  it('rejects malformed autonomous workflow linkage payloads at the adapter boundary', () => {
    expect(() =>
      autonomousUnitSchema.parse({
        ...makeAutonomousUnit(),
        workflowLinkage: {
          workflowNodeId: '   ',
          transitionId: 'auto:txn-001',
          causalTransitionId: 'txn-000',
          handoffTransitionId: 'auto:txn-001',
          handoffPackageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
        },
      }),
    ).toThrow()

    expect(() =>
      autonomousUnitAttemptSchema.parse({
        projectId: 'project-1',
        runId: 'run-1',
        unitId: 'run-1:checkpoint:2',
        attemptId: 'attempt-1',
        attemptNumber: 1,
        childSessionId: 'child-session-1',
        status: 'active',
        boundaryId: 'checkpoint:2',
        workflowLinkage: {
          workflowNodeId: 'workflow-research',
          transitionId: 'auto:txn-001',
          causalTransitionId: 'txn-000',
          handoffTransitionId: '   ',
          handoffPackageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
        },
        startedAt: '2026-04-16T14:00:00Z',
        finishedAt: null,
        updatedAt: '2026-04-16T14:00:01Z',
        lastErrorCode: null,
        lastError: null,
      }),
    ).toThrow()

    expect(() =>
      autonomousRunStateSchema.parse({
        run: makeAutonomousRun(),
        unit: {
          ...makeAutonomousUnit({ projectId: 'project-2' }),
          workflowLinkage: {
            workflowNodeId: 'workflow-research',
            transitionId: 'auto:txn-001',
            causalTransitionId: 'txn-000',
            handoffTransitionId: 'auto:txn-001',
            handoffPackageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
          },
        },
        attempt: null,
        history: [],
      }),
    ).toThrow(/Autonomous unit project id must match the autonomous run project id/)
  })

  it('maps additive handoff packages while filtering cross-project rows', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        handoffPackages: [
          makeHandoffPackage('project-1', 'auto:txn-001'),
          makeHandoffPackage('project-1', 'auto:txn-002'),
          makeHandoffPackage('project-2', 'auto:txn-ghost'),
        ],
      }),
    )

    expect(project.handoffPackages).toHaveLength(2)
    expect(project.handoffPackages.map((pkg) => pkg.handoffTransitionId)).toEqual(['auto:txn-001', 'auto:txn-002'])
    expect(project.handoffPackages[0]).toMatchObject({
      projectId: 'project-1',
      handoffTransitionId: 'auto:txn-001',
      packageHash: 'c6488be19a74f4cd78d6d0ec03f0f8ec0a8ec8e53e1fd0f96af7f3298df138f7',
    })
    expect(project.lifecycle.activeStage?.stage).toBe('research')
  })

  it('rejects malformed handoff-package payloads at the snapshot contract boundary even for cross-project rows', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          handoffPackages: [
            makeHandoffPackage('project-1', 'auto:txn-001'),
            {
              ...makeHandoffPackage('project-2', 'auto:txn-ghost'),
              packageHash: 7,
            } as unknown as ReturnType<typeof makeHandoffPackage>,
          ],
        }),
      ),
    ).toThrow()
  })

  it('projects bounded notification broker metadata while filtering cross-project dispatch rows', () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const projectDispatches = Array.from({ length: 255 }, (_, index) =>
      makeNotificationDispatch({
        id: index + 1,
        actionId,
        routeId: `route-${index + 1}`,
        status: 'pending',
        updatedAt: new Date(Date.UTC(2026, 3, 16, 14, 0, index % 60)).toISOString(),
      }),
    )

    const project = mapProjectSnapshot(
      makeSnapshot({
        notificationDispatches: [
          ...projectDispatches,
          makeNotificationDispatch({
            id: 900,
            projectId: 'project-2',
            actionId,
            routeId: 'cross-project-route',
            status: 'pending',
          }),
        ],
      }),
    )

    expect(project.notificationBroker.dispatchCount).toBe(250)
    expect(project.notificationBroker.totalBeforeTruncation).toBe(255)
    expect(project.notificationBroker.isTruncated).toBe(true)
    expect(project.notificationBroker.byActionId[actionId]?.dispatchCount).toBe(250)
    expect(project.notificationBroker.actions).toHaveLength(1)
  })

  it('rejects malformed broker payloads at snapshot/list/submit contract boundaries', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          notificationDispatches: [
            {
              ...makeNotificationDispatch({ id: 1 }),
              status: 'queued',
            },
          ] as unknown as NonNullable<ProjectSnapshotResponseDto['notificationDispatches']>,
        }),
      ),
    ).toThrow()

    expect(() =>
      listNotificationDispatchesResponseSchema.parse({
        dispatches: [
          {
            ...makeNotificationDispatch({ id: 2 }),
            correlationKey: 'invalid-correlation-key',
          },
        ],
      }),
    ).toThrow(/Correlation keys must match `nfy:<32 lowercase hex>`/)

    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const approvalRequest = {
      actionId,
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'review_worktree',
      title: 'Review worktree changes',
      detail: 'Inspect the pending repository diff before continuing.',
      gateNodeId: 'workflow-research',
      gateKey: 'requires_user_input',
      transitionFromNodeId: 'workflow-discussion',
      transitionToNodeId: 'workflow-research',
      transitionKind: 'advance',
      userAnswer: 'Proceed after validating repo changes.',
      status: 'approved' as const,
      decisionNote: 'Approved via correlated broker reply.',
      createdAt: '2026-04-16T14:00:00Z',
      updatedAt: '2026-04-16T14:02:30Z',
      resolvedAt: '2026-04-16T14:02:30Z',
    }

    const validResponse = {
      claim: {
        id: 11,
        projectId: 'project-1',
        actionId,
        routeId: 'telegram-primary',
        correlationKey: 'nfy:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa11',
        responderId: 'operator-1',
        status: 'accepted' as const,
        rejectionCode: null,
        rejectionMessage: null,
        createdAt: '2026-04-16T14:02:30Z',
      },
      dispatch: {
        ...makeNotificationDispatch({
          id: 11,
          actionId,
          routeId: 'telegram-primary',
          status: 'claimed',
          attemptCount: 1,
          lastAttemptAt: '2026-04-16T14:02:00Z',
          claimedAt: '2026-04-16T14:02:30Z',
        }),
        correlationKey: 'nfy:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa11',
      },
      resolveResult: {
        approvalRequest,
        verificationRecord: {
          id: 8,
          sourceActionId: actionId,
          status: 'passed' as const,
          summary: 'Approved operator action: Review worktree changes.',
          detail: null,
          recordedAt: '2026-04-16T14:02:31Z',
        },
      },
      resumeResult: {
        approvalRequest,
        resumeEntry: {
          id: 5,
          sourceActionId: actionId,
          sessionId: 'session-1',
          status: 'started' as const,
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-16T14:02:32Z',
        },
      },
    }

    expect(() => submitNotificationReplyResponseSchema.parse(validResponse)).not.toThrow()

    expect(() =>
      submitNotificationReplyResponseSchema.parse({
        ...validResponse,
        dispatch: {
          ...validResponse.dispatch,
          status: 'pending',
          claimedAt: null,
        },
      }),
    ).toThrow(/Accepted notification reply responses must return a claimed notification dispatch row/)

    expect(() =>
      submitNotificationReplyResponseSchema.parse({
        ...validResponse,
        claim: {
          ...validResponse.claim,
          projectId: 'project-2',
        },
      }),
    ).toThrow(/claim and dispatch must reference the same project/)
  })

  it('enforces strict notification adapter sync contracts for bounded cycle summaries and project scoping', () => {
    expect(() =>
      syncNotificationAdaptersRequestSchema.parse({
        projectId: 'project-1',
      }),
    ).not.toThrow()

    const validResponse = {
      projectId: 'project-1',
      dispatch: {
        projectId: 'project-1',
        pendingCount: 2,
        attemptedCount: 2,
        sentCount: 1,
        failedCount: 1,
        attemptLimit: 64,
        attemptsTruncated: false,
        attempts: [
          {
            dispatchId: 11,
            actionId: 'scope:auto-dispatch:workflow-research:requires_user_input',
            routeId: 'discord-fallback',
            routeKind: 'discord',
            outcomeStatus: 'sent',
            diagnosticCode: 'notification_adapter_dispatch_attempted',
            diagnosticMessage: 'Cadence sent notification dispatch `11` for route `discord-fallback`.',
            durableErrorCode: null,
            durableErrorMessage: null,
          },
        ],
        errorCodeCounts: [
          {
            code: 'notification_adapter_transport_timeout',
            count: 1,
          },
        ],
      },
      replies: {
        projectId: 'project-1',
        routeCount: 2,
        polledRouteCount: 2,
        messageCount: 2,
        acceptedCount: 1,
        rejectedCount: 1,
        attemptLimit: 256,
        attemptsTruncated: false,
        attempts: [
          {
            routeId: 'discord-fallback',
            routeKind: 'discord',
            actionId: 'scope:auto-dispatch:workflow-research:requires_user_input',
            messageId: 'm-1',
            accepted: true,
            diagnosticCode: 'notification_adapter_reply_received',
            diagnosticMessage:
              'Cadence accepted inbound reply for route `discord-fallback` and resumed the correlated action through the existing broker path.',
            replyCode: null,
            replyMessage: null,
          },
        ],
        errorCodeCounts: [
          {
            code: 'notification_reply_already_claimed',
            count: 1,
          },
        ],
      },
      syncedAt: '2026-04-16T14:02:35Z',
    }

    expect(() => syncNotificationAdaptersResponseSchema.parse(validResponse)).not.toThrow()

    expect(() =>
      syncNotificationAdaptersResponseSchema.parse({
        ...validResponse,
        dispatch: {
          ...validResponse.dispatch,
          projectId: 'project-2',
        },
      }),
    ).toThrow(/must match the sync response project id/)

    expect(() =>
      syncNotificationAdaptersResponseSchema.parse({
        ...validResponse,
        replies: {
          ...validResponse.replies,
          attempts: [
            {
              ...validResponse.replies.attempts[0],
              accepted: true,
              replyCode: 'notification_reply_already_claimed',
              replyMessage: 'duplicate',
            },
          ],
        },
      }),
    ).toThrow(/must not include reply rejection diagnostics/)
  })

  it('enforces strict notification route list/upsert contracts with fail-closed route kinds', () => {
    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [makeNotificationRoute()],
      }),
    ).not.toThrow()

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          {
            ...makeNotificationRoute(),
            credentialReadiness: undefined,
          },
        ],
      }),
    ).toThrow(/must include redacted `credentialReadiness` metadata for every route/)

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          makeNotificationRoute({
            credentialReadiness: {
              hasBotToken: true,
              hasChatId: true,
              hasWebhookUrl: true,
              ready: true,
              status: 'ready',
              diagnostic: null,
            },
          }),
        ],
      }),
    ).toThrow(/Telegram readiness rows must not report `hasWebhookUrl=true`/)

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          makeNotificationRoute({
            routeKind: 'discord',
            routeId: 'discord-fallback',
            routeTarget: 'discord:123456789012345678',
            credentialReadiness: {
              hasBotToken: false,
              hasChatId: false,
              hasWebhookUrl: true,
              ready: true,
              status: 'ready',
              diagnostic: null,
            },
          }),
        ],
      }),
    ).toThrow(/Discord readiness rows can set `ready=true` only when both `hasWebhookUrl` and `hasBotToken` are true/)

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          makeNotificationRoute({
            routeKind: 'discord',
            routeId: 'discord-fallback',
            routeTarget: 'discord:123456789012345678',
            credentialReadiness: {
              hasBotToken: false,
              hasChatId: false,
              hasWebhookUrl: true,
              ready: false,
              status: 'missing',
              diagnostic: null,
            },
          }),
        ],
      }),
    ).toThrow(/must include typed diagnostics/)

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          makeNotificationRoute({
            routeKind: 'email' as unknown as 'telegram',
          }),
        ],
      }),
    ).toThrow()

    expect(() =>
      listNotificationRoutesResponseSchema.parse({
        routes: [
          {
            ...makeNotificationRoute(),
            metadataJson: '["not-an-object"]',
          },
        ],
      }),
    ).toThrow(/Notification route metadata must be a JSON object string/)

    expect(() =>
      upsertNotificationRouteRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'discord-primary',
        routeKind: 'discord',
        routeTarget: 'discord:123456789012345678',
        enabled: true,
        metadataJson: '{"guildId":"123"}',
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).not.toThrow()

    expect(() =>
      upsertNotificationRouteRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'discord-primary',
        routeKind: 'slack',
        routeTarget: 'discord:123456789012345678',
        enabled: true,
        metadataJson: '{"guildId":"123"}',
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow()

    expect(() =>
      upsertNotificationRouteRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'discord-primary',
        routeKind: 'discord',
        routeTarget: 'discord:123456789012345678',
        enabled: true,
        metadataJson: '[]',
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow(/Notification route metadata must be a JSON object string/)

    expect(() =>
      upsertNotificationRouteRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'discord-primary',
        routeKind: 'discord',
        routeTarget: 'discord:123456789012345678',
        enabled: true,
        metadataJson: '{"guildId":"123"}',
        updatedAt: '2026-04-16T14:05:00Z',
        unexpected: true,
      }),
    ).toThrow()
  })

  it('enforces strict notification route credential upsert contracts with route-kind-specific payload rules', () => {
    expect(() =>
      upsertNotificationRouteCredentialsRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'route-telegram',
        routeKind: 'telegram',
        credentials: {
          botToken: 'telegram-bot-token',
          chatId: '123456',
          webhookUrl: null,
        },
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).not.toThrow()

    expect(() =>
      upsertNotificationRouteCredentialsRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'route-discord',
        routeKind: 'discord',
        credentials: {
          botToken: 'discord-bot-token',
          chatId: null,
          webhookUrl: 'https://discord.com/api/webhooks/1/2',
        },
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).not.toThrow()

    expect(() =>
      upsertNotificationRouteCredentialsRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'route-telegram',
        routeKind: 'telegram',
        credentials: {
          botToken: 'telegram-bot-token',
          chatId: null,
          webhookUrl: null,
        },
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow(/Telegram credentials require non-empty `chatId`/)

    expect(() =>
      upsertNotificationRouteCredentialsRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'route-discord',
        routeKind: 'discord',
        credentials: {
          botToken: null,
          chatId: '123456',
          webhookUrl: 'https://discord.com/api/webhooks/1/2',
        },
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow(/Discord credentials must not include `chatId`/)

    expect(() =>
      upsertNotificationRouteCredentialsRequestSchema.parse({
        projectId: 'project-1',
        routeId: 'route-discord',
        routeKind: 'discord',
        credentials: {
          botToken: null,
          chatId: null,
          webhookUrl: 'http://discord.com/api/webhooks/1/2',
        },
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow(/require an HTTPS `webhookUrl`/)

    expect(() =>
      upsertNotificationRouteCredentialsResponseSchema.parse({
        projectId: 'project-1',
        routeId: 'route-discord',
        routeKind: 'discord',
        credentialScope: 'app_local',
        hasBotToken: true,
        hasChatId: false,
        hasWebhookUrl: true,
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).not.toThrow()

    expect(() =>
      upsertNotificationRouteCredentialsResponseSchema.parse({
        projectId: 'project-1',
        routeId: 'route-telegram',
        routeKind: 'telegram',
        credentialScope: 'app_local',
        hasBotToken: true,
        hasChatId: false,
        hasWebhookUrl: false,
        updatedAt: '2026-04-16T14:05:00Z',
      }),
    ).toThrow(/Telegram credential acknowledgements must indicate bot token \+ chat id/)
  })

  it('composes and decomposes canonical notification route targets across form/storage seams', () => {
    expect(composeNotificationRouteTarget('telegram', '@ops-room')).toBe('telegram:@ops-room')
    expect(composeNotificationRouteTarget('telegram', ' telegram:@ops-room ')).toBe('telegram:@ops-room')
    expect(composeNotificationRouteTarget('discord', '123456789012345678')).toBe('discord:123456789012345678')

    expect(() => composeNotificationRouteTarget('telegram', 'discord:123456789012345678')).toThrow(
      /does not match the selected route kind/,
    )

    expect(decomposeNotificationRouteTarget('discord', 'discord:123456789012345678')).toEqual({
      channelTarget: '123456789012345678',
      canonicalTarget: 'discord:123456789012345678',
    })

    expect(() => decomposeNotificationRouteTarget('discord', '123456789012345678')).toThrow(
      /must use `<kind>:<channel-target>` format/,
    )
    expect(() => decomposeNotificationRouteTarget('telegram', 'discord:123456789012345678')).toThrow(
      /does not match route kind/,
    )
  })

  it('maps durable operator approvals, verification records, and resume history from the snapshot contract', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        approvalRequests: [
          {
            actionId: 'flow-1:review_worktree',
            sessionId: 'session-1',
            flowId: 'flow-1',
            actionType: 'review_worktree',
            title: 'Review worktree changes',
            detail: 'Inspect the pending repository diff before continuing.',
            gateNodeId: 'workflow-research',
            gateKey: 'requires_user_input',
            transitionFromNodeId: 'workflow-discussion',
            transitionToNodeId: 'workflow-research',
            transitionKind: 'advance',
            userAnswer: 'Proceed after validating repo changes.',
            status: 'approved',
            decisionNote: 'Looks safe to continue.',
            createdAt: '2026-04-13T20:01:00Z',
            updatedAt: '2026-04-13T20:02:00Z',
            resolvedAt: '2026-04-13T20:02:00Z',
          },
          {
            actionId: 'flow-2:confirm_resume',
            sessionId: 'session-2',
            flowId: 'flow-2',
            actionType: 'confirm_resume',
            title: 'Resume the runtime loop',
            detail: 'Approve the next execution pass.',
            gateNodeId: null,
            gateKey: null,
            transitionFromNodeId: null,
            transitionToNodeId: null,
            transitionKind: null,
            userAnswer: null,
            status: 'pending',
            decisionNote: null,
            createdAt: '2026-04-13T20:03:00Z',
            updatedAt: '2026-04-13T20:03:00Z',
            resolvedAt: null,
          },
        ],
        verificationRecords: [
          {
            id: 7,
            sourceActionId: 'flow-1:review_worktree',
            status: 'passed',
            summary: 'Approved operator action: Review worktree changes.',
            detail: 'Approval decision persisted to the selected project store.',
            recordedAt: '2026-04-13T20:02:01Z',
          },
        ],
        resumeHistory: [
          {
            id: 3,
            sourceActionId: 'flow-1:review_worktree',
            sessionId: 'session-1',
            status: 'started',
            summary: 'Operator resumed the selected project runtime session.',
            createdAt: '2026-04-13T20:03:30Z',
          },
        ],
      }),
    )

    expect(project.pendingApprovalCount).toBe(1)
    expect(project.approvalRequests[0]).toMatchObject({
      actionId: 'flow-1:review_worktree',
      status: 'approved',
      statusLabel: 'Approved',
      gateNodeId: 'workflow-research',
      gateKey: 'requires_user_input',
      userAnswer: 'Proceed after validating repo changes.',
      isGateLinked: true,
      isRuntimeResumable: false,
      requiresUserAnswer: true,
      answerRequirementReason: 'gate_linked',
      answerShapeKind: 'plain_text',
      answerShapeLabel: 'Worktree review rationale',
      canResume: true,
    })
    expect(project.approvalRequests[1]).toMatchObject({
      actionId: 'flow-2:confirm_resume',
      status: 'pending',
      isGateLinked: false,
      isRuntimeResumable: false,
      requiresUserAnswer: false,
      answerRequirementReason: 'optional',
      answerShapeKind: 'plain_text',
      answerShapeLabel: 'Resume confirmation note',
    })
    expect(project.latestDecisionOutcome).toMatchObject({
      actionId: 'flow-1:review_worktree',
      status: 'approved',
      statusLabel: 'Approved',
      gateNodeId: 'workflow-research',
      gateKey: 'requires_user_input',
      userAnswer: 'Proceed after validating repo changes.',
    })
    expect(project.verificationRecords[0]).toMatchObject({
      id: 7,
      status: 'passed',
      statusLabel: 'Passed',
    })
    expect(project.resumeHistory[0]).toMatchObject({
      id: 3,
      status: 'started',
      statusLabel: 'Resume started',
    })
  })

  it('derives runtime-resumable answer requirements and action-shape fallback metadata from snapshot approvals', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        approvalRequests: [
          {
            actionId: 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required',
            sessionId: 'session-1',
            flowId: 'flow-1',
            actionType: 'terminal_input_required',
            title: 'Provide terminal input',
            detail: 'Supply terminal input text so the runtime can resume.',
            gateNodeId: null,
            gateKey: null,
            transitionFromNodeId: null,
            transitionToNodeId: null,
            transitionKind: null,
            userAnswer: null,
            status: 'pending',
            decisionNote: null,
            createdAt: '2026-04-13T20:03:00Z',
            updatedAt: '2026-04-13T20:03:00Z',
            resolvedAt: null,
          },
          {
            actionId: 'flow-1:custom_followup',
            sessionId: 'session-1',
            flowId: 'flow-1',
            actionType: 'custom_followup',
            title: 'Custom follow-up action',
            detail: 'Review custom follow-up notes.',
            gateNodeId: null,
            gateKey: null,
            transitionFromNodeId: null,
            transitionToNodeId: null,
            transitionKind: null,
            userAnswer: null,
            status: 'pending',
            decisionNote: null,
            createdAt: '2026-04-13T20:04:00Z',
            updatedAt: '2026-04-13T20:04:00Z',
            resolvedAt: null,
          },
        ],
      }),
    )

    expect(project.pendingApprovalCount).toBe(2)
    expect(project.approvalRequests[0]).toMatchObject({
      actionId: 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required',
      isRuntimeResumable: true,
      requiresUserAnswer: true,
      answerRequirementReason: 'runtime_resumable',
      answerShapeKind: 'terminal_input',
      answerShapeLabel: 'Terminal input text',
    })
    expect(project.approvalRequests[0]?.answerRequirementLabel).toContain('runtime-resumable approvals')

    expect(project.approvalRequests[1]).toMatchObject({
      actionId: 'flow-1:custom_followup',
      isRuntimeResumable: false,
      requiresUserAnswer: false,
      answerRequirementReason: 'optional',
      answerShapeKind: 'plain_text',
    })
    expect(project.approvalRequests[1]?.answerShapeLabel).toContain('Plain-text response (Custom Followup)')
  })

  it('rejects malformed gate-linked and runtime-resumable approval payloads at the schema boundary', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow-1:review_worktree',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'review_worktree',
              title: 'Review worktree changes',
              detail: 'Inspect the pending repository diff before continuing.',
              gateNodeId: 'workflow-research',
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
              userAnswer: null,
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-13T20:03:00Z',
              updatedAt: '2026-04-13T20:03:00Z',
              resolvedAt: null,
            },
          ],
        }),
      ),
    ).toThrow(/Gate-linked approvals must include both `gateNodeId` and `gateKey`/)

    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow-1:review_worktree',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'review_worktree',
              title: 'Review worktree changes',
              detail: 'Inspect the pending repository diff before continuing.',
              gateNodeId: 'workflow-research',
              gateKey: 'requires_user_input',
              transitionFromNodeId: 'workflow-discussion',
              transitionToNodeId: 'workflow-research',
              transitionKind: null,
              userAnswer: null,
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-13T20:03:00Z',
              updatedAt: '2026-04-13T20:03:00Z',
              resolvedAt: null,
            },
          ],
        }),
      ),
    ).toThrow(/full transition continuation metadata/)

    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow-1:review_worktree',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'review_worktree',
              title: 'Review worktree changes',
              detail: 'Inspect the pending repository diff before continuing.',
              gateNodeId: 'workflow-research',
              gateKey: 'requires_user_input',
              transitionFromNodeId: 'workflow-discussion',
              transitionToNodeId: 'workflow-research',
              transitionKind: 'advance',
              userAnswer: null,
              status: 'approved',
              decisionNote: 'Looks safe to continue.',
              createdAt: '2026-04-13T20:01:00Z',
              updatedAt: '2026-04-13T20:02:00Z',
              resolvedAt: '2026-04-13T20:02:00Z',
            },
          ],
        }),
      ),
    ).toThrow(/Approved gate-linked approvals must include a non-empty `userAnswer`/)

    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow-1:review_worktree',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'review_worktree',
              title: 'Review worktree changes',
              detail: 'Inspect the pending repository diff before continuing.',
              gateNodeId: null,
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
              userAnswer: 'accidental value',
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-13T20:03:00Z',
              updatedAt: '2026-04-13T20:03:00Z',
              resolvedAt: null,
            },
          ],
        }),
      ),
    ).toThrow(/Pending approvals must not include `userAnswer`, `decisionNote`, or `resolvedAt`/)

    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'terminal_input_required',
              title: 'Provide terminal input',
              detail: 'Supply terminal input text so the runtime can resume.',
              gateNodeId: null,
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
              userAnswer: null,
              status: 'approved',
              decisionNote: 'Attempted approval without answer.',
              createdAt: '2026-04-13T20:03:00Z',
              updatedAt: '2026-04-13T20:03:00Z',
              resolvedAt: '2026-04-13T20:04:00Z',
            },
          ],
        }),
      ),
    ).toThrow(/Approved runtime-resumable approvals must include a non-empty `userAnswer`/)

    expect(() =>
      projectSnapshotResponseSchema.parse(
        makeSnapshot({
          approvalRequests: [
            {
              actionId: 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required',
              sessionId: null,
              flowId: null,
              actionType: 'terminal_input_required',
              title: 'Provide terminal input',
              detail: 'Supply terminal input text so the runtime can resume.',
              gateNodeId: null,
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
              userAnswer: null,
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-13T20:03:00Z',
              updatedAt: '2026-04-13T20:03:00Z',
              resolvedAt: null,
            },
          ],
        }),
      ),
    ).toThrow(/Runtime-scoped approvals must include consistent scope\/run\/boundary\/action metadata/)
  })

  it('validates resolve/resume userAnswer request contracts without legacy fallback keys', () => {
    expect(
      resolveOperatorActionRequestSchema.parse({
        projectId: 'project-1',
        actionId: 'flow-1:review_worktree',
        decision: 'approve',
        userAnswer: 'Proceed after confirming the diff.',
      }),
    ).toMatchObject({
      userAnswer: 'Proceed after confirming the diff.',
    })

    expect(() =>
      resolveOperatorActionRequestSchema.parse({
        projectId: 'project-1',
        actionId: 'flow-1:review_worktree',
        decision: 'approve',
        decisionNote: 'legacy field',
      }),
    ).toThrow()

    expect(() =>
      resolveOperatorActionRequestSchema.parse({
        projectId: 'project-1',
        actionId: 'flow-1:review_worktree',
        decision: 'approve',
        userAnswer: '   ',
      }),
    ).toThrow()

    expect(
      resumeOperatorRunRequestSchema.parse({
        projectId: 'project-1',
        actionId: 'flow-1:review_worktree',
        userAnswer: 'Proceed after confirming the diff.',
      }),
    ).toMatchObject({
      userAnswer: 'Proceed after confirming the diff.',
    })

    expect(() =>
      resumeOperatorRunRequestSchema.parse({
        projectId: 'project-1',
        actionId: 'flow-1:review_worktree',
        decisionNote: 'legacy field',
      }),
    ).toThrow()
  })

  it('derives phase step status and zero-safe progress from project snapshots', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        project: {
          id: 'project-1',
          name: 'Cadence',
          description: '',
          milestone: 'M001',
          totalPhases: 0,
          completedPhases: 0,
          activePhase: 0,
          branch: 'main',
          runtime: 'codex',
        },
        phases: [
          {
            id: 2,
            name: 'Live state',
            description: '',
            status: 'active',
            currentStep: 'verify',
            taskCount: 0,
            completedTasks: 0,
            summary: null,
          },
        ],
      }),
    )

    expect(project.phaseProgressPercent).toBe(0)
    expect(project.phases[0].description).toBe('No phase description provided.')
    expect(project.phases[0].stepStatuses.discuss).toBe('complete')
    expect(project.phases[0].stepStatuses.plan).toBe('complete')
    expect(project.phases[0].stepStatuses.execute).toBe('complete')
    expect(project.phases[0].stepStatuses.verify).toBe('active')
    expect(project.phases[0].stepStatuses.ship).toBe('pending')
  })

  it('maps lifecycle-first snapshots even when legacy phases are empty', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        project: {
          id: 'project-1',
          name: 'Cadence',
          description: 'Desktop shell',
          milestone: 'M001',
          totalPhases: 0,
          completedPhases: 0,
          activePhase: 0,
          branch: 'main',
          runtime: 'codex',
        },
        phases: [],
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-15T17:59:00Z',
            },
            {
              stage: 'research',
              nodeId: 'workflow-research',
              status: 'active',
              actionRequired: true,
              lastTransitionAt: '2026-04-15T18:00:00Z',
            },
            {
              stage: 'requirements',
              nodeId: 'workflow-requirements',
              status: 'pending',
              actionRequired: false,
              lastTransitionAt: null,
            },
          ],
        },
      }),
    )

    expect(project.phases).toHaveLength(0)
    expect(project.lifecycle.hasStages).toBe(true)
    expect(project.lifecycle.stages).toHaveLength(3)
    expect(project.lifecycle.activeStage?.stage).toBe('research')
    expect(project.lifecycle.byStage.requirements?.status).toBe('pending')
    expect(project.lifecycle.percentComplete).toBe(33)
    expect(project.lifecycle.actionRequiredCount).toBe(1)
  })

  it('projects auto-dispatch lifecycle advancement with deterministic stage ordering', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        phases: [],
        lifecycle: {
          stages: [
            {
              stage: 'requirements',
              nodeId: 'workflow-requirements',
              status: 'pending',
              actionRequired: false,
              lastTransitionAt: null,
            },
            {
              stage: 'research',
              nodeId: 'workflow-research',
              status: 'active',
              actionRequired: true,
              lastTransitionAt: '2026-04-16T14:01:00Z',
            },
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-16T14:00:00Z',
            },
          ],
        },
      }),
    )

    expect(project.lifecycle.hasStages).toBe(true)
    expect(project.lifecycle.stages.map((stage) => stage.stage)).toEqual(['discussion', 'research', 'requirements'])
    expect(project.lifecycle.activeStage?.stage).toBe('research')
    expect(project.lifecycle.completedCount).toBe(1)
    expect(project.lifecycle.actionRequiredCount).toBe(1)
    expect(project.lifecycle.percentComplete).toBe(33)
    expect(project.lifecycle.byStage.roadmap).toBeNull()
  })

  it('keeps legacy phase consumers compatible when lifecycle projection is empty', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        lifecycle: {
          stages: [],
        },
      }),
    )

    expect(project.phases).toHaveLength(2)
    expect(project.lifecycle.hasStages).toBe(false)
    expect(project.lifecycle.stages).toHaveLength(0)
    expect(project.lifecycle.byStage.discussion).toBeNull()
  })

  it('rejects malformed project snapshot lifecycle payloads at the contract boundary', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-15T17:59:00Z',
            },
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion-duplicate',
              status: 'active',
              actionRequired: false,
              lastTransitionAt: '2026-04-15T18:01:00Z',
            },
          ],
        },
      }),
    ).toThrow(/Duplicate lifecycle stage/)

    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        lifecycle: {
          stages: [
            {
              stage: 'unknown',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-15T17:59:00Z',
            },
          ],
        },
      }),
    ).toThrow()

    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'unknown',
              actionRequired: false,
              lastTransitionAt: '2026-04-15T17:59:00Z',
            },
          ],
        },
      }),
    ).toThrow()

    expect(() => {
      const snapshot = makeSnapshot()
      const { lifecycle: _lifecycle, ...legacySnapshot } = snapshot
      projectSnapshotResponseSchema.parse(legacySnapshot)
    }).toThrow()
  })

  it('maps autonomous run and unit truth from project snapshots independently of runtime auth state', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        autonomousRun: makeAutonomousRun({ providerId: 'azure_openai', duplicateStartDetected: true, duplicateStartRunId: 'run-1' }),
        autonomousUnit: makeAutonomousUnit(),
      }),
    )

    expect(project.autonomousRun?.runId).toBe('run-1')
    expect(project.autonomousRun?.providerId).toBe('azure_openai')
    expect(project.autonomousRun?.statusLabel).toBe('Autonomous run stale')
    expect(project.autonomousRun?.recoveryLabel).toBe('Recovery required')
    expect(project.autonomousRun?.duplicateStartDetected).toBe(true)
    expect(project.autonomousRun?.runtimeLabel).toBe('Openai Codex · Autonomous run stale')
    expect(project.autonomousUnit?.unitId).toBe('run-1:checkpoint:2')
    expect(project.autonomousUnit?.kindLabel).toBe('State')
    expect(project.autonomousUnit?.statusLabel).toBe('Active')
    expect(project.runtimeSession).toBeNull()
    expect(project.runtimeRun).toBeNull()
  })

  it('rejects malformed autonomous run/unit payloads at the contract boundary', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        autonomousRun: makeAutonomousRun(),
        autonomousUnit: makeAutonomousUnit({ runId: 'run-2' }),
      }),
    ).toThrow(/Autonomous unit run id must match/)
  })

  it('maps runtime-run provider identity independently from runtime kind labels', () => {
    const runtimeRun = mapRuntimeRun(
      runtimeRunSchema.parse({
        projectId: 'project-1',
        runId: 'run-1',
        runtimeKind: 'openai_codex',
        providerId: 'azure_openai',
        supervisorKind: 'detached_pty',
        status: 'running',
        transport: {
          kind: 'tcp',
          endpoint: '127.0.0.1:4455',
          liveness: 'reachable',
        },
        startedAt: '2026-04-15T20:00:00Z',
        lastHeartbeatAt: '2026-04-15T20:00:05Z',
        lastCheckpointSequence: 1,
        lastCheckpointAt: '2026-04-15T20:00:06Z',
        stoppedAt: null,
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-15T20:00:06Z',
        checkpoints: [
          {
            sequence: 1,
            kind: 'state',
            summary: 'Recovered repository context before reconnecting the live feed.',
            createdAt: '2026-04-15T20:00:06Z',
          },
        ],
      }),
    )

    expect(runtimeRun.providerId).toBe('azure_openai')
    expect(runtimeRun.runtimeKind).toBe('openai_codex')
    expect(runtimeRun.statusLabel).toBe('Supervisor running')
    expect(runtimeRun.runtimeLabel).toBe('Openai Codex · Supervisor running')
  })

  it('maps repository status counts and applies branch metadata onto the active project', () => {
    const project = mapProjectSnapshot(makeSnapshot())
    const status = mapRepositoryStatus(makeStatus())
    const merged = applyRepositoryStatus(project, status)

    expect(status.statusCount).toBe(2)
    expect(status.stagedCount).toBe(1)
    expect(status.unstagedCount).toBe(1)
    expect(status.untrackedCount).toBe(1)
    expect(merged.branch).toBe('feature/live-state')
    expect(merged.repositoryStatus?.headShaLabel).toBe('No HEAD')
  })

  it('maps authenticated runtime sessions into truthful agent/runtime labels', () => {
    const baseProject = applyRepositoryStatus(mapProjectSnapshot(makeSnapshot()), mapRepositoryStatus(makeStatus()))
    const runtime = mapRuntimeSession(makeRuntimeSession({ providerId: 'azure_openai' }))
    const merged = applyRuntimeSession(baseProject, runtime)

    expect(runtime.isAuthenticated).toBe(true)
    expect(runtime.providerId).toBe('azure_openai')
    expect(runtime.phaseLabel).toBe('Authenticated')
    expect(runtime.runtimeLabel).toBe('Openai Codex · Authenticated')
    expect(runtime.accountLabel).toBe('acct-1')
    expect(merged.runtimeLabel).toBe('Openai Codex · Authenticated')
    expect(merged.runtimeSession?.providerId).toBe('azure_openai')
    expect(merged.runtimeSession?.sessionLabel).toBe('session-1')
  })

  it('maps signed-out and manual-input runtime phases without fabricating session ids', () => {
    const signedOut = mapRuntimeSession(
      makeRuntimeSession({
        flowId: null,
        sessionId: null,
        accountId: null,
        phase: 'idle',
        callbackBound: null,
        authorizationUrl: null,
        redirectUri: null,
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Sign in first.',
          retryable: false,
        },
      }),
    )
    const awaitingManual = mapRuntimeSession(
      makeRuntimeSession({
        sessionId: null,
        phase: 'awaiting_manual_input',
        callbackBound: false,
      }),
    )

    expect(signedOut.isSignedOut).toBe(true)
    expect(signedOut.runtimeLabel).toBe('Runtime unavailable')
    expect(signedOut.sessionLabel).toBe('No session')
    expect(awaitingManual.needsManualInput).toBe(true)
    expect(awaitingManual.runtimeLabel).toBe('Openai Codex · Awaiting manual input')
  })

  it('merges runtime update events while preserving explicit provider identity and ignoring stale payloads', () => {
    const currentRuntime = mapRuntimeSession(
      makeRuntimeSession({
        providerId: 'openai_codex',
        phase: 'awaiting_manual_input',
        callbackBound: false,
        updatedAt: '2026-04-13T20:00:30Z',
      }),
    )

    const merged = mergeRuntimeUpdated(currentRuntime, {
      projectId: 'project-1',
      runtimeKind: 'openai_codex',
      providerId: 'azure_openai',
      flowId: 'flow-1',
      sessionId: 'session-2',
      accountId: 'acct-1',
      authPhase: 'authenticated',
      lastErrorCode: null,
      lastError: null,
      updatedAt: '2026-04-13T20:01:00Z',
    })

    expect(merged.phase).toBe('authenticated')
    expect(merged.providerId).toBe('azure_openai')
    expect(merged.sessionId).toBe('session-2')
    expect(merged.authorizationUrl).toBe('https://auth.openai.com/oauth/authorize')
    expect(merged.redirectUri).toBe('http://127.0.0.1:1455/auth/callback')

    const stale = mergeRuntimeUpdated(merged, {
      projectId: 'project-1',
      runtimeKind: 'openai_codex',
      providerId: 'stale_provider',
      flowId: 'flow-1',
      sessionId: 'session-stale',
      accountId: 'acct-stale',
      authPhase: 'idle',
      lastErrorCode: 'auth_session_not_found',
      lastError: {
        code: 'auth_session_not_found',
        message: 'Stale payload should not win.',
        retryable: false,
      },
      updatedAt: '2026-04-13T20:00:00Z',
    })

    expect(stale).toBe(merged)
    expect(stale.providerId).toBe('azure_openai')
    expect(stale.sessionId).toBe('session-2')
  })

  it('normalizes runtime stream items into capped project-owned stream state', () => {
    const subscribed = createRuntimeStreamFromSubscription(streamSubscription)
    const withTranscript = mergeRuntimeStreamEvent(
      subscribed,
      makeStreamEvent({
        kind: 'transcript',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: 'Connected to Cadence.',
        toolCallId: null,
        toolName: null,
        toolState: null,
        actionType: null,
        title: null,
        detail: null,
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:00Z',
      }),
    )
    const withTool = mergeRuntimeStreamEvent(
      withTranscript,
      makeStreamEvent({
        kind: 'tool',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: 'bootstrap-repository-context',
        toolName: 'inspect_repository_context',
        toolState: 'running',
        toolSummary: {
          kind: 'command',
          exitCode: 0,
          timedOut: false,
          stdoutTruncated: true,
          stderrTruncated: false,
          stdoutRedacted: false,
          stderrRedacted: true,
        },
        actionType: null,
        title: null,
        detail: 'Collecting repository status.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:01Z',
      }),
    )
    const withSkill = mergeRuntimeStreamEvent(
      withTool,
      makeStreamEvent({
        kind: 'skill',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        toolSummary: null,
        skillId: 'find-skills',
        skillStage: 'install',
        skillResult: 'succeeded',
        skillSource: {
          repo: 'vercel-labs/skills',
          path: 'skills/find-skills',
          reference: 'main',
          treeHash: '0123456789abcdef0123456789abcdef01234567',
        },
        skillCacheStatus: 'refreshed',
        skillDiagnostic: null,
        actionType: null,
        title: null,
        detail: 'Installed autonomous skill `find-skills` from the cached vercel-labs/skills tree.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:02Z',
      }),
    )
    const withActivity = mergeRuntimeStreamEvent(
      withSkill,
      makeStreamEvent({
        kind: 'activity',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        toolSummary: null,
        skillId: null,
        skillStage: null,
        skillResult: null,
        skillSource: null,
        skillCacheStatus: null,
        skillDiagnostic: null,
        actionType: null,
        title: 'Planning',
        detail: 'Replay buffer ready.',
        code: 'phase_progress',
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:03Z',
      }),
    )
    const withActionRequired = mergeRuntimeStreamEvent(
      withActivity,
      makeStreamEvent({
        kind: 'action_required',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        actionId: 'run-1:review_worktree',
        boundaryId: 'boundary-1',
        actionType: 'review_worktree',
        title: 'Repository has local changes',
        detail: 'Review the worktree before trusting agent actions.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:04Z',
      }),
    )
    const completed = mergeRuntimeStreamEvent(
      withActionRequired,
      makeStreamEvent({
        kind: 'complete',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        actionType: null,
        title: null,
        detail: 'Runtime bootstrap finished for project-1.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:05Z',
      }),
    )

    expect(subscribed.runId).toBe('run-1')
    expect(subscribed.status).toBe('subscribing')
    expect(withTranscript.status).toBe('live')
    expect(withTranscript.transcriptItems[0]).toMatchObject({
      runId: 'run-1',
      text: 'Connected to Cadence.',
    })
    expect(withTool.toolCalls[0]).toMatchObject({
      runId: 'run-1',
      toolCallId: 'bootstrap-repository-context',
      toolName: 'inspect_repository_context',
      toolState: 'running',
      toolSummary: {
        kind: 'command',
        stdoutTruncated: true,
        stderrRedacted: true,
      },
    })
    expect(withSkill.skillItems[0]).toMatchObject({
      runId: 'run-1',
      skillId: 'find-skills',
      stage: 'install',
      result: 'succeeded',
      cacheStatus: 'refreshed',
      source: {
        repo: 'vercel-labs/skills',
        treeHash: '0123456789abcdef0123456789abcdef01234567',
      },
    })
    expect(withActivity.activityItems[0]).toMatchObject({
      runId: 'run-1',
      code: 'phase_progress',
      title: 'Planning',
    })
    expect(withActionRequired.actionRequired[0]).toMatchObject({
      actionId: 'run-1:review_worktree',
      boundaryId: 'boundary-1',
      actionType: 'review_worktree',
    })
    expect(completed.status).toBe('complete')
    expect(completed.lastSequence).toBeGreaterThan(withActionRequired.lastSequence ?? 0)
    expect(completed.completion?.detail).toBe('Runtime bootstrap finished for project-1.')
    expect(getRuntimeStreamStatusLabel(completed.status)).toBe('Stream complete')
  })

  it('projects additive tool summaries through autonomous artifacts while rejecting malformed nested summary drift', () => {
    const fileSummary = toolResultSummarySchema.parse({
      kind: 'file',
      path: 'src/lib.rs',
      scope: 'workspace',
      lineCount: 120,
      matchCount: 4,
      truncated: true,
    })
    const gitSummary = toolResultSummarySchema.parse({
      kind: 'git',
      scope: 'worktree',
      changedFiles: 3,
      truncated: false,
      baseRevision: 'main~1',
    })
    const webSummary = toolResultSummarySchema.parse({
      kind: 'web',
      target: 'https://example.com/search?q=Cadence',
      resultCount: 5,
      finalUrl: 'https://example.com/search?q=Cadence',
      contentKind: 'html',
      contentType: 'text/html',
      truncated: false,
    })

    expect([fileSummary.kind, gitSummary.kind, webSummary.kind]).toEqual(['file', 'git', 'web'])

    const artifactPayloadBase = {
      kind: 'tool_result' as const,
      projectId: 'project-1',
      runId: 'run-1',
      unitId: 'run-1:checkpoint:2',
      attemptId: 'attempt-1',
      artifactId: 'artifact-1',
      toolCallId: 'tool-call-1',
      toolName: 'git_diff',
      toolState: 'succeeded' as const,
      commandResult: null,
      actionId: 'action-1',
      boundaryId: 'boundary-1',
    }

    const artifactWithSummary: AutonomousUnitArtifactDto = {
      projectId: 'project-1',
      runId: 'run-1',
      unitId: 'run-1:checkpoint:2',
      attemptId: 'attempt-1',
      artifactId: 'artifact-1',
      artifactKind: 'tool_result',
      status: 'recorded',
      summary: 'Git diff summary recorded.',
      contentHash: '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      payload: autonomousToolResultPayloadSchema.parse({
        ...artifactPayloadBase,
        toolSummary: gitSummary,
      }),
      createdAt: '2026-04-16T14:00:00Z',
      updatedAt: '2026-04-16T14:00:01Z',
    }

    const mappedArtifact = mapAutonomousArtifact(artifactWithSummary)
    expect(mappedArtifact.toolSummary).toEqual(gitSummary)
    expect(mappedArtifact.toolSummary?.kind).toBe('git')
    expect(mappedArtifact.toolSummary?.baseRevision).toBe('main~1')

    const legacyArtifact = mapAutonomousArtifact({
      ...artifactWithSummary,
      payload: autonomousToolResultPayloadSchema.parse(artifactPayloadBase),
    })
    expect(legacyArtifact.toolSummary).toBeNull()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'unknown',
        target: 'https://example.com',
        truncated: false,
      }),
    ).toThrow()

    expect(() =>
      autonomousToolResultPayloadSchema.parse({
        ...artifactPayloadBase,
        toolSummary: {
          kind: 'command',
          exitCode: 0,
          timedOut: false,
          stdoutTruncated: 'yes',
          stderrTruncated: false,
          stdoutRedacted: false,
          stderrRedacted: false,
        },
      }),
    ).toThrow()

    expect(() =>
      autonomousToolResultPayloadSchema.parse({
        ...artifactPayloadBase,
        toolSummary: {
          kind: 'file',
          path: 'src/lib.rs',
          scope: 'workspace',
        },
      }),
    ).toThrow()
  })

  it('dedupes replayed action-required items by run and action identity', () => {
    const first = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'action_required',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId: 'run-1:review_worktree',
          boundaryId: 'boundary-1',
          actionType: 'review_worktree',
          title: 'Repository has local changes',
          detail: 'Review the worktree before trusting agent actions.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:03Z',
        },
        { sequence: 9 },
      ),
    )

    const replayed = mergeRuntimeStreamEvent(
      first,
      makeStreamEvent(
        {
          kind: 'action_required',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId: 'run-1:review_worktree',
          boundaryId: 'boundary-1',
          actionType: 'review_worktree',
          title: 'Repository has local changes',
          detail: 'Review the worktree before trusting agent actions.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:04Z',
        },
        { sequence: 10 },
      ),
    )

    expect(first.actionRequired).toHaveLength(1)
    expect(first.items).toHaveLength(1)
    expect(replayed.actionRequired).toHaveLength(1)
    expect(replayed.items).toHaveLength(1)
    expect(replayed.actionRequired[0]).toMatchObject({
      actionId: 'run-1:review_worktree',
      sequence: 10,
    })
  })

  it('dedupes replayed events by runId plus sequence and promotes reconnecting streams back to live', () => {
    const liveStream = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        },
        { sequence: 7 },
      ),
    )

    const reconnecting = {
      ...liveStream,
      status: 'replaying' as const,
    }

    const replayed = mergeRuntimeStreamEvent(
      reconnecting,
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        },
        { sequence: 7 },
      ),
    )
    const resumedLive = mergeRuntimeStreamEvent(
      replayed,
      makeStreamEvent(
        {
          kind: 'activity',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: 'Execution',
          detail: 'Live event resumed.',
          code: 'phase_progress',
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:01Z',
        },
        { sequence: 8 },
      ),
    )

    expect(replayed.items).toHaveLength(1)
    expect(replayed.lastSequence).toBe(7)
    expect(resumedLive.status).toBe('live')
    expect(resumedLive.activityItems).toHaveLength(1)
    expect(resumedLive.lastSequence).toBe(8)
  })

  it('rejects non-monotonic sequence regressions while allowing same-sequence replay dedupe', () => {
    const liveStream = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        },
        { sequence: 7 },
      ),
    )

    const replayedSameSequence = mergeRuntimeStreamEvent(
      liveStream,
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        },
        { sequence: 7 },
      ),
    )

    expect(replayedSameSequence.items).toHaveLength(1)

    expect(() =>
      mergeRuntimeStreamEvent(
        liveStream,
        makeStreamEvent(
          {
            kind: 'activity',
            sessionId: 'session-1',
            flowId: 'flow-1',
            text: null,
            toolCallId: null,
            toolName: null,
            toolState: null,
            actionType: null,
            title: 'Out of order',
            detail: 'older sequence should fail',
            code: 'phase_progress',
            message: null,
            retryable: null,
            createdAt: '2026-04-13T20:01:01Z',
          },
          { sequence: 6 },
        ),
      ),
    ).toThrow(/non-monotonic runtime stream sequence/i)
  })

  it('ignores events from a different run so same-session run replacement cannot inherit stale items', () => {
    const firstRun = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'first run transcript',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        },
        { runId: 'run-1', sequence: 3 },
      ),
    )

    const ignoredDifferentRun = mergeRuntimeStreamEvent(
      firstRun,
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'second run transcript',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:02:00Z',
        },
        { runId: 'run-2', sequence: 1 },
      ),
    )
    const secondRun = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription({ ...streamSubscription, runId: 'run-2' }),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'second run transcript',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:02:00Z',
        },
        { runId: 'run-2', sequence: 1 },
      ),
    )

    expect(ignoredDifferentRun.runId).toBe('run-1')
    expect(ignoredDifferentRun.items).toHaveLength(1)
    expect(ignoredDifferentRun.transcriptItems[0]?.text).toBe('first run transcript')
    expect(secondRun.runId).toBe('run-2')
    expect(secondRun.items).toHaveLength(1)
    expect(secondRun.transcriptItems[0]?.text).toBe('second run transcript')
  })

  it('preserves the last truthful runtime stream state when issues or failure items arrive', () => {
    const liveStream = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent({
        kind: 'transcript',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: 'Connected to Cadence.',
        toolCallId: null,
        toolName: null,
        toolState: null,
        actionType: null,
        title: null,
        detail: null,
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:00Z',
      }),
    )

    const staleStream = applyRuntimeStreamIssue(liveStream, {
      projectId: 'project-1',
      runtimeKind: 'openai_codex',
      sessionId: 'session-1',
      flowId: 'flow-1',
      subscribedItemKinds: streamSubscription.subscribedItemKinds,
      code: 'runtime_stream_not_ready',
      message: 'Cadence marked the runtime stream stale.',
      retryable: true,
      observedAt: '2026-04-13T20:02:00Z',
    })
    const failedStream = mergeRuntimeStreamEvent(
      staleStream,
      makeStreamEvent({
        kind: 'failure',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        actionType: null,
        title: null,
        detail: null,
        code: 'runtime_stream_bootstrap_failed',
        message: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
        retryable: true,
        createdAt: '2026-04-13T20:02:01Z',
      }),
    )

    expect(staleStream.status).toBe('stale')
    expect(staleStream.items).toHaveLength(1)
    expect(staleStream.lastIssue?.message).toBe('Cadence marked the runtime stream stale.')
    expect(failedStream.status).toBe('stale')
    expect(failedStream.items).toHaveLength(2)
    expect(failedStream.failure?.code).toBe('runtime_stream_bootstrap_failed')
    expect(failedStream.lastIssue?.retryable).toBe(true)
  })

  it('rejects malformed runtime stream payloads and cross-project merges at the model boundary', () => {
    expect(() =>
      runtimeStreamItemSchema.parse({
        kind: 'transcript',
        runId: 'run-1',
        sequence: 1,
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: '   ',
        toolCallId: null,
        toolName: null,
        toolState: null,
        skillId: null,
        skillStage: null,
        skillResult: null,
        skillSource: null,
        skillCacheStatus: null,
        skillDiagnostic: null,
        actionId: null,
        actionType: null,
        title: null,
        detail: null,
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      runtimeStreamItemSchema.parse({
        kind: 'skill',
        runId: 'run-1',
        sequence: 2,
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        toolSummary: null,
        skillId: 'find-skills',
        skillStage: 'install',
        skillResult: 'failed',
        skillSource: {
          repo: 'vercel-labs/skills',
          path: 'skills/find-skills',
          reference: 'main',
        },
        skillCacheStatus: 'refreshed',
        skillDiagnostic: null,
        actionId: null,
        boundaryId: null,
        actionType: null,
        title: null,
        detail: 'Install failed while refreshing the cached skill tree.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow(/skillSource|skillDiagnostic|tree hashes/i)

    expect(() =>
      runtimeStreamItemSchema.parse({
        kind: 'skill',
        runId: 'run-1',
        sequence: 3,
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: null,
        toolName: null,
        toolState: null,
        toolSummary: null,
        skillId: 'find-skills',
        skillStage: 'discover',
        skillResult: 'succeeded',
        skillSource: {
          repo: 'vercel-labs/skills',
          path: 'skills/find-skills',
          reference: 'main',
          treeHash: '0123456789abcdef0123456789abcdef01234567',
        },
        skillCacheStatus: null,
        skillDiagnostic: null,
        actionId: null,
        boundaryId: null,
        actionType: null,
        title: null,
        detail: 'Discovery completed.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      mergeRuntimeStreamEvent(
        createRuntimeStreamFromSubscription(streamSubscription),
        makeStreamEvent(
          {
            kind: 'tool',
            sessionId: 'session-1',
            flowId: 'flow-1',
            text: null,
            toolCallId: 'tool-call-cross-project',
            toolName: 'inspect_repository_context',
            toolState: 'running',
            actionType: null,
            title: null,
            detail: null,
            code: null,
            message: null,
            retryable: null,
            createdAt: '2026-04-13T20:01:00Z',
          },
          { projectId: 'project-2' },
        ),
      ),
    ).toThrow(/project-2/)

    expect(() =>
      mergeRuntimeStreamEvent(
        createRuntimeStreamFromSubscription(streamSubscription),
        makeStreamEvent({
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: 'inspect_repository_context',
          toolState: 'running',
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        }),
      ),
    ).toThrow(/toolCallId/)
  })

  it('rejects malformed runtime payloads at the TypeScript boundary', () => {
    expect(() =>
      runtimeSessionSchema.parse({
        ...makeRuntimeSession(),
        accountId: '   ',
      }),
    ).toThrow()

    expect(() =>
      runtimeSettingsSchema.parse({
        providerId: 'openai_codex',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: false,
      }),
    ).toThrow(/openai_codex/)

    expect(() =>
      runtimeSettingsSchema.parse({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: true,
        openrouterApiKey: 'sk-or-v1-should-never-cross-the-boundary',
      }),
    ).toThrow()

    expect(() =>
      upsertRuntimeSettingsRequestSchema.parse({
        providerId: 'azure_openai',
        modelId: 'azure_openai',
      }),
    ).toThrow()

    expect(() =>
      upsertRuntimeSettingsRequestSchema.parse({
        providerId: 'openrouter',
        modelId: '   ',
      }),
    ).toThrow()

    expect(() =>
      upsertRuntimeSettingsRequestSchema.parse({
        providerId: 'openai_codex',
        modelId: 'openai/gpt-4.1-mini',
      }),
    ).toThrow(/openai_codex/)

    expect(() =>
      runtimeUpdatedPayloadSchema.parse({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: 'flow-1',
        sessionId: 'session-1',
        accountId: 'acct-1',
        authPhase: null,
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      runtimeUpdatedPayloadSchema.parse({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: '   ',
        flowId: 'flow-1',
        sessionId: 'session-1',
        accountId: 'acct-1',
        authPhase: 'authenticated',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      runtimeUpdatedPayloadSchema.parse({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: 'flow-1',
        sessionId: '   ',
        accountId: 'acct-1',
        authPhase: 'authenticated',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-13T20:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      runtimeRunSchema.parse({
        projectId: 'project-1',
        runId: 'run-1',
        runtimeKind: 'openai_codex',
        providerId: '   ',
        supervisorKind: 'detached_pty',
        status: 'running',
        transport: {
          kind: 'tcp',
          endpoint: '127.0.0.1:4455',
          liveness: 'reachable',
        },
        startedAt: '2026-04-15T20:00:00Z',
        lastHeartbeatAt: '2026-04-15T20:00:05Z',
        lastCheckpointSequence: 1,
        lastCheckpointAt: '2026-04-15T20:00:06Z',
        stoppedAt: null,
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-15T20:00:06Z',
        checkpoints: [],
      }),
    ).toThrow()

    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        autonomousRun: {
          ...makeAutonomousRun(),
          providerId: '   ',
        },
      }),
    ).toThrow()

    expect(() =>
      runtimeSessionSchema.parse({
        ...makeRuntimeSession(),
        updatedAt: 'not-a-timestamp',
      }),
    ).toThrow()
  })

  it('validates workflow graph command request/response payloads at the TS boundary', () => {
    const upsertRequest = upsertWorkflowGraphRequestSchema.parse({
      projectId: 'project-1',
      nodes: [
        {
          nodeId: 'plan',
          phaseId: 1,
          sortOrder: 1,
          name: 'Plan',
          description: 'Plan workflow',
          status: 'active',
          currentStep: 'plan',
          taskCount: 2,
          completedTasks: 1,
          summary: 'In progress',
        },
      ],
      edges: [
        {
          fromNodeId: 'plan',
          toNodeId: 'execute',
          transitionKind: 'advance',
          gateRequirement: 'execution_gate',
        },
      ],
      gates: [
        {
          nodeId: 'execute',
          gateKey: 'execution_gate',
          gateState: 'pending',
          actionType: 'approve_execution',
          title: 'Approve execution',
          detail: 'Operator approval required.',
          decisionContext: null,
        },
      ],
    })

    const upsertResponse = upsertWorkflowGraphResponseSchema.parse({
      nodes: upsertRequest.nodes,
      edges: upsertRequest.edges,
      gates: [
        {
          nodeId: 'execute',
          gateKey: 'execution_gate',
          gateState: 'pending',
          actionType: 'approve_execution',
          title: 'Approve execution',
          detail: 'Operator approval required.',
          decisionContext: null,
        },
      ],
      phases: [
        {
          id: 1,
          name: 'Plan',
          description: 'Plan workflow',
          status: 'active',
          currentStep: 'plan',
          taskCount: 2,
          completedTasks: 1,
          summary: 'In progress',
        },
      ],
    })

    const transitionRequest = applyWorkflowTransitionRequestSchema.parse({
      projectId: 'project-1',
      transitionId: 'txn-002',
      causalTransitionId: 'txn-001',
      fromNodeId: 'plan',
      toNodeId: 'execute',
      transitionKind: 'advance',
      gateDecision: 'approved',
      gateDecisionContext: 'operator-approved',
      gateUpdates: [
        {
          gateKey: 'execution_gate',
          gateState: 'satisfied',
          decisionContext: 'approved by operator',
        },
      ],
      occurredAt: '2026-04-15T18:01:00Z',
    })

    const transitionResponse = applyWorkflowTransitionResponseSchema.parse({
      transitionEvent: {
        id: 12,
        transitionId: 'txn-002',
        causalTransitionId: 'txn-001',
        fromNodeId: 'plan',
        toNodeId: 'execute',
        transitionKind: 'advance',
        gateDecision: 'approved',
        gateDecisionContext: 'operator-approved',
        createdAt: '2026-04-15T18:01:00Z',
      },
      automaticDispatch: {
        status: 'applied',
        transitionEvent: {
          id: 13,
          transitionId: 'auto:txn-003',
          causalTransitionId: 'txn-002',
          fromNodeId: 'execute',
          toNodeId: 'verify',
          transitionKind: 'advance',
          gateDecision: 'approved',
          gateDecisionContext: null,
          createdAt: '2026-04-15T18:02:00Z',
        },
        handoffPackage: {
          status: 'persisted',
          package: makeHandoffPackage('project-1', 'auto:txn-003'),
          code: null,
          message: null,
        },
        code: null,
        message: null,
      },
      phases: upsertResponse.phases,
    })

    const resumeResponse = resumeOperatorRunResponseSchema.parse({
      approvalRequest: {
        actionId: 'flow-1:review_worktree',
        sessionId: 'session-1',
        flowId: 'flow-1',
        actionType: 'review_worktree',
        title: 'Review worktree',
        detail: 'Review worktree before resume.',
        gateNodeId: null,
        gateKey: null,
        transitionFromNodeId: null,
        transitionToNodeId: null,
        transitionKind: null,
        userAnswer: 'Looks safe',
        status: 'approved',
        decisionNote: 'Looks safe',
        createdAt: '2026-04-16T12:00:00Z',
        updatedAt: '2026-04-16T12:01:00Z',
        resolvedAt: '2026-04-16T12:01:00Z',
      },
      resumeEntry: {
        id: 9,
        sourceActionId: 'flow-1:review_worktree',
        sessionId: 'session-1',
        status: 'started',
        summary: 'Operator resumed the selected project runtime session.',
        createdAt: '2026-04-16T12:01:10Z',
      },
      automaticDispatch: {
        status: 'skipped',
        transitionEvent: null,
        handoffPackage: null,
        code: 'workflow_handoff_redaction_failed',
        message: 'Cadence skipped handoff package persistence due to redaction policy.',
      },
    })

    expect(upsertResponse.nodes[0]?.nodeId).toBe('plan')
    expect(transitionRequest.gateUpdates[0]?.gateState).toBe('satisfied')
    expect(transitionResponse.transitionEvent.gateDecision).toBe('approved')
    expect(transitionResponse.automaticDispatch?.status).toBe('applied')
    expect(resumeResponse.automaticDispatch?.status).toBe('skipped')
  })

  it('rejects malformed workflow graph and transition payloads at the schema boundary', () => {
    expect(() =>
      upsertWorkflowGraphRequestSchema.parse({
        projectId: 'project-1',
        nodes: [
          {
            nodeId: 'plan',
            phaseId: 1,
            sortOrder: 1,
            name: 'Plan',
            description: 'Plan workflow',
            status: 'active',
            currentStep: 'plan',
            taskCount: 2,
            completedTasks: 1,
            summary: null,
            extra: true,
          },
        ],
        edges: [],
        gates: [],
      }),
    ).toThrow()

    expect(() =>
      applyWorkflowTransitionRequestSchema.parse({
        projectId: 'project-1',
        transitionId: 'txn-002',
        causalTransitionId: null,
        fromNodeId: 'plan',
        toNodeId: 'execute',
        transitionKind: 'advance',
        gateDecision: 'allow',
        gateDecisionContext: null,
        gateUpdates: [],
        occurredAt: '2026-04-15T18:01:00Z',
      }),
    ).toThrow()

    expect(() =>
      applyWorkflowTransitionResponseSchema.parse({
        transitionEvent: {
          id: 12,
          transitionId: 'txn-002',
          causalTransitionId: 'txn-001',
          fromNodeId: 'plan',
          toNodeId: 'execute',
          transitionKind: 'advance',
          gateDecision: 'approved',
          gateDecisionContext: 'operator-approved',
          createdAt: 'not-a-timestamp',
        },
        phases: [],
      }),
    ).toThrow()

    expect(() =>
      applyWorkflowTransitionResponseSchema.parse({
        transitionEvent: {
          id: 12,
          transitionId: 'txn-002',
          causalTransitionId: 'txn-001',
          fromNodeId: 'plan',
          toNodeId: 'execute',
          transitionKind: 'advance',
          gateDecision: 'approved',
          gateDecisionContext: 'operator-approved',
          createdAt: '2026-04-15T18:01:00Z',
        },
        automaticDispatch: {
          status: 'applied',
          transitionEvent: null,
          handoffPackage: null,
          code: null,
          message: null,
        },
        phases: [],
      }),
    ).toThrow(/Applied\/replayed automatic dispatch outcomes must include transition and handoff payloads/)

    expect(() =>
      applyWorkflowTransitionResponseSchema.parse({
        transitionEvent: {
          id: 12,
          transitionId: 'txn-002',
          causalTransitionId: 'txn-001',
          fromNodeId: 'plan',
          toNodeId: 'execute',
          transitionKind: 'advance',
          gateDecision: 'approved',
          gateDecisionContext: 'operator-approved',
          createdAt: '2026-04-15T18:01:00Z',
        },
        automaticDispatch: {
          status: 'skipped',
          transitionEvent: null,
          handoffPackage: null,
          code: 'workflow_transition_gate_unmet',
          message: null,
        },
        phases: [],
      }),
    ).toThrow(/Skipped automatic dispatch outcomes must include non-empty `code` and `message` diagnostics/)

    expect(() =>
      resumeOperatorRunResponseSchema.parse({
        approvalRequest: {
          actionId: 'flow-1:review_worktree',
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree',
          detail: 'Review worktree before resume.',
          gateNodeId: null,
          gateKey: null,
          transitionFromNodeId: null,
          transitionToNodeId: null,
          transitionKind: null,
          userAnswer: 'Looks safe',
          status: 'approved',
          decisionNote: 'Looks safe',
          createdAt: '2026-04-16T12:00:00Z',
          updatedAt: '2026-04-16T12:01:00Z',
          resolvedAt: '2026-04-16T12:01:00Z',
        },
        resumeEntry: {
          id: 9,
          sourceActionId: 'flow-1:review_worktree',
          sessionId: 'session-1',
          status: 'started',
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-16T12:01:10Z',
        },
        automaticDispatch: {
          status: 'skipped',
          transitionEvent: null,
          handoffPackage: {
            status: 'persisted',
            package: null,
            code: null,
            message: null,
          },
          code: 'workflow_handoff_redaction_failed',
          message: 'Cadence skipped handoff package persistence due to redaction policy.',
        },
      }),
    ).toThrow(/Skipped automatic dispatch outcomes must not include transition or handoff payloads/)
  })

  it('keeps percent math bounded when counts are invalid or exceed total', () => {
    expect(safePercent(1, 0)).toBe(0)
    expect(safePercent(10, 4)).toBe(100)
    expect(safePercent(-1, 4)).toBe(0)
  })
})
