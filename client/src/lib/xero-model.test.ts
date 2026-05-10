import { describe, expect, it } from 'vitest'
import {
  applyRepositoryStatus,
  applyRuntimeSession,
  applyRuntimeStreamIssue,
  browserControlSettingsSchema,
  composeNotificationRouteTarget,
  createRuntimeStreamFromSubscription,
  decomposeNotificationRouteTarget,
  getRuntimeStreamStatusLabel,
  listNotificationDispatchesResponseSchema,
  listNotificationRoutesResponseSchema,
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
  safePercent,
  submitNotificationReplyResponseSchema,
  syncNotificationAdaptersRequestSchema,
  syncNotificationAdaptersResponseSchema,
  subscribeRuntimeStreamResponseSchema,
  toolResultSummarySchema,
  upsertBrowserControlSettingsRequestSchema,
  upsertNotificationRouteCredentialsRequestSchema,
  upsertNotificationRouteCredentialsResponseSchema,
  upsertNotificationRouteRequestSchema,
  upsertRuntimeSettingsRequestSchema,
  type ProjectSnapshotResponseDto,
  type RepositoryStatusResponseDto,
  type RuntimeSessionDto,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemDto,
} from '@/src/lib/xero-model'

const LEGACY_RUNTIME_STREAM_RECENT_ITEM_CAP = 40
const LEGACY_RUNTIME_STREAM_TRANSCRIPT_CAP = 20

function makeSnapshot(overrides: Partial<ProjectSnapshotResponseDto> = {}): ProjectSnapshotResponseDto {
  return {
    project: {
      id: 'project-1',
      name: 'Xero',
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
      rootPath: '/tmp/Xero',
      displayName: 'Xero',
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
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [
      {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        title: 'Main session',
        summary: 'Primary project session',
        status: 'active',
        selected: true,
        createdAt: '2026-04-15T17:55:00Z',
        updatedAt: '2026-04-15T17:55:00Z',
        archivedAt: null,
        lastRunId: null,
        lastRuntimeKind: null,
        lastProviderId: null,
      },
    ],
    ...overrides,
  }
}

function makeAutonomousRun(overrides: Partial<NonNullable<ProjectSnapshotResponseDto['autonomousRun']>> = {}) {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    supervisorKind: 'owned_agent',
    status: 'stale' as const,
    recoveryState: 'recovery_required' as const,
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
      message: 'Xero could not connect to the agent runtime control endpoint.',
    },
    lastErrorCode: 'runtime_supervisor_connect_failed',
    lastError: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Xero could not connect to the agent runtime control endpoint.',
    },
    updatedAt: '2026-04-15T23:10:03Z',
    ...overrides,
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
    routeTarget: overrides.routeTarget ?? 'telegram:@Xero_ops',
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
      rootPath: '/tmp/Xero',
      displayName: 'Xero',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: {
      name: 'feature/live-state',
      headSha: null,
      detached: false,
      upstream: {
        name: 'origin/feature/live-state',
        ahead: 3,
        behind: 1,
      },
    },
    lastCommit: {
      sha: 'c3e529f1c4e2a7d0d4cf759f9080e7f507dc9f4a',
      summary: 'fix: use live commit metadata in the footer',
      committedAt: '2026-04-22T17:55:00Z',
    },
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: 'modified',
        unstaged: null,
        untracked: false,
      },
      {
        path: 'client/src/lib/xero-model.ts',
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
  agentSessionId: 'agent-session-main',
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
    agentSessionId?: string
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
    agentSessionId: overrides.agentSessionId ?? streamSubscription.agentSessionId,
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

function makeTranscriptStreamEvent(
  text: string,
  options: {
    sequence: number
    role?: RuntimeStreamItemDto['transcriptRole']
    createdAt?: string
  },
) {
  return makeStreamEvent(
    {
      kind: 'transcript',
      sessionId: 'session-1',
      flowId: 'flow-1',
      text,
      transcriptRole: options.role ?? 'assistant',
      toolCallId: null,
      toolName: null,
      toolState: null,
      actionType: null,
      title: null,
      detail: null,
      code: null,
      message: null,
      retryable: null,
      createdAt: options.createdAt ?? `2026-04-13T20:03:${String(options.sequence).padStart(2, '0')}Z`,
    },
    { sequence: options.sequence },
  )
}

describe('xero-model', () => {
  it('maps nullable and blank project summary fields into UI-safe values', () => {
    const summary = mapProjectSummary({
      id: 'project-1',
      name: 'Xero',
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

  it('maps persisted phase snapshots into project detail views without fabricating workflow state', () => {
    const project = mapProjectSnapshot(makeSnapshot())

    expect(project.phaseProgressPercent).toBe(33)
    expect(project.completedPhases).toBe(1)
    expect(project.repository?.headShaLabel).toBe('abc123')
    expect(project.phases).toHaveLength(2)
    expect(project.phases[0].summary).toBe('Imported successfully')
    expect(project.phases[1].summary).toBeUndefined()
    expect(project.phases[1].currentStep).toBe('execute')
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

    const actionId = 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required'
    const approvalRequest = {
      actionId,
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'terminal_input_required',
      title: 'Terminal input required',
      detail: 'Inspect the pending repository diff before continuing.',
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
            diagnosticMessage: 'Xero sent notification dispatch `11` for route `discord-fallback`.',
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
              'Xero accepted inbound reply for route `discord-fallback` and resumed the correlated action through the existing broker path.',
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

    expect(() => composeNotificationRouteTarget('telegram', '   ')).toThrow(/Route target is required/)
    expect(() => composeNotificationRouteTarget('telegram', 'discord:123456789012345678')).toThrow(
      /does not match the selected route kind/,
    )

    expect(decomposeNotificationRouteTarget('discord', 'discord:123456789012345678')).toEqual({
      channelTarget: '123456789012345678',
      canonicalTarget: 'discord:123456789012345678',
    })

    expect(() => decomposeNotificationRouteTarget('discord', '   ')).toThrow(/Route target is required/)
    expect(() => decomposeNotificationRouteTarget('discord', '123456789012345678')).toThrow(
      /must use `<kind>:<channel-target>` format/,
    )
    expect(() => decomposeNotificationRouteTarget('telegram', 'discord:123456789012345678')).toThrow(
      /does not match route kind/,
    )
  })

  it('maps durable operator approvals, verification records, and resume history from the snapshot contract', () => {
    const runtimeActionId = 'flow:flow-1:run:run-7:boundary:boundary-2:terminal_input_required'
    const project = mapProjectSnapshot(
      makeSnapshot({
        approvalRequests: [
          {
            actionId: runtimeActionId,
            sessionId: 'session-1',
            flowId: 'flow-1',
            actionType: 'terminal_input_required',
            title: 'Terminal input required',
            detail: 'Provide terminal input so the runtime can continue.',
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
            sourceActionId: runtimeActionId,
            status: 'passed',
            summary: 'Approved operator action: Terminal input required.',
            detail: 'Approval decision persisted to the selected project store.',
            recordedAt: '2026-04-13T20:02:01Z',
          },
        ],
        resumeHistory: [
          {
            id: 3,
            sourceActionId: runtimeActionId,
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
      actionId: runtimeActionId,
      status: 'approved',
      statusLabel: 'Approved',
      userAnswer: 'Proceed after validating repo changes.',
      isRuntimeResumable: true,
      requiresUserAnswer: true,
      answerRequirementReason: 'runtime_resumable',
      answerShapeKind: 'terminal_input',
      canResume: true,
    })
    expect(project.approvalRequests[1]).toMatchObject({
      actionId: 'flow-2:confirm_resume',
      status: 'pending',
      isRuntimeResumable: false,
      requiresUserAnswer: false,
      answerRequirementReason: 'optional',
      answerShapeKind: 'plain_text',
      answerShapeLabel: 'Resume confirmation note',
    })
    expect(project.latestDecisionOutcome).toMatchObject({
      actionId: runtimeActionId,
      status: 'approved',
      statusLabel: 'Approved',
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

  it('rejects malformed runtime-resumable approval payloads at the schema boundary', () => {
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

  it('keeps phase current-step strings generic and zero-safe progress bounded', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        project: {
          id: 'project-1',
          name: 'Xero',
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
    expect(project.phases[0].currentStep).toBe('verify')
  })

  it('maps autonomous run truth from project snapshots independently of runtime auth state', () => {
    const project = mapProjectSnapshot(
      makeSnapshot({
        autonomousRun: makeAutonomousRun({ providerId: 'azure_openai', duplicateStartDetected: true, duplicateStartRunId: 'run-1' }),
      }),
    )

    expect(project.autonomousRun?.runId).toBe('run-1')
    expect(project.autonomousRun?.providerId).toBe('azure_openai')
    expect(project.autonomousRun?.statusLabel).toBe('Stale')
    expect(project.autonomousRun?.recoveryLabel).toBe('Recovery required')
    expect(project.autonomousRun?.duplicateStartDetected).toBe(true)
    expect(project.autonomousRun?.runtimeLabel).toBe('openai_codex · Stale')
    expect(project.runtimeSession).toBeNull()
    expect(project.runtimeRun).toBeNull()
  })

  it('rejects malformed autonomous run payloads at the contract boundary', () => {
    expect(() =>
      projectSnapshotResponseSchema.parse({
        ...makeSnapshot(),
        autonomousRun: makeAutonomousRun({ providerId: '   ' }),
      }),
    ).toThrow()
  })

  it('maps runtime-run provider identity independently from runtime kind labels', () => {
    const runtimeRun = mapRuntimeRun(
      runtimeRunSchema.parse({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-1',
        runtimeKind: 'openai_codex',
        providerId: 'azure_openai',
        supervisorKind: 'owned_agent',
        status: 'running',
        transport: {
          kind: 'internal',
          endpoint: 'xero://owned-agent',
          liveness: 'reachable',
        },
        controls: {
          active: {
            runtimeAgentId: 'engineer',
            modelId: 'azure-openai/gpt-4.1-mini',
            thinkingEffort: 'medium',
            approvalMode: 'suggest',
            revision: 1,
            appliedAt: '2026-04-15T20:00:00Z',
          },
          pending: null,
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
    expect(runtimeRun.statusLabel).toBe('Agent running')
    expect(runtimeRun.runtimeLabel).toBe('Openai Codex · Agent running')
  })

  it('maps repository status counts and applies branch metadata onto the active project', () => {
    const project = mapProjectSnapshot(makeSnapshot())
    const status = mapRepositoryStatus(makeStatus())
    const merged = applyRepositoryStatus(project, status)

    expect(status.statusCount).toBe(2)
    expect(status.stagedCount).toBe(1)
    expect(status.unstagedCount).toBe(1)
    expect(status.untrackedCount).toBe(1)
    expect(status.upstream).toEqual({
      name: 'origin/feature/live-state',
      ahead: 3,
      behind: 1,
    })
    expect(status.lastCommit?.shortShaLabel).toBe('c3e529f')
    expect(status.lastCommit?.summary).toBe('fix: use live commit metadata in the footer')
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
        text: 'Connected to Xero.',
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
      text: 'Connected to Xero.',
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

  it('keeps stream transcripts outside the non-transcript timeline cap and dedupes tool transitions', () => {
    let stream = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Please inspect the runtime.',
          transcriptRole: 'user',
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
        { sequence: 1 },
      ),
    )

    stream = mergeRuntimeStreamEvent(
      stream,
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Reading the relevant files.',
          transcriptRole: 'assistant',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:01Z',
        },
        { sequence: 2 },
      ),
    )

    stream = mergeRuntimeStreamEvent(
      stream,
      makeStreamEvent(
        {
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'read-runtime',
          toolName: 'read',
          toolState: 'running',
          actionType: null,
          title: null,
          detail: 'path: client/components/xero/agent-runtime.tsx',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:02Z',
        },
        { sequence: 3 },
      ),
    )

    stream = mergeRuntimeStreamEvent(
      stream,
      makeStreamEvent(
        {
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'read-runtime',
          toolName: 'read',
          toolState: 'succeeded',
          actionType: null,
          title: null,
          detail: 'Read 80 line(s).',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:03Z',
        },
        { sequence: 4 },
      ),
    )

    expect(stream.items.filter((item) => item.kind === 'tool' && item.toolCallId === 'read-runtime')).toHaveLength(1)
    expect(stream.items.find((item) => item.kind === 'tool' && item.toolCallId === 'read-runtime')).toMatchObject({
      sequence: 3,
      updatedSequence: 4,
      toolState: 'succeeded',
    })

    for (let index = 0; index < LEGACY_RUNTIME_STREAM_RECENT_ITEM_CAP + 5; index += 1) {
      stream = mergeRuntimeStreamEvent(
        stream,
        makeStreamEvent(
          {
            kind: 'tool',
            sessionId: 'session-1',
            flowId: 'flow-1',
            text: null,
            toolCallId: `burst-tool-${index}`,
            toolName: 'read',
            toolState: 'succeeded',
            actionType: null,
            title: null,
            detail: `Read file ${index}.`,
            code: null,
            message: null,
            retryable: null,
            createdAt: '2026-04-13T20:02:00Z',
          },
          { sequence: 5 + index },
        ),
      )
    }

    expect(stream.items.filter((item) => item.kind === 'transcript').map((item) => item.text)).toEqual([
      'Please inspect the runtime.',
      'Reading the relevant files.',
    ])
    expect(stream.items.filter((item) => item.kind !== 'transcript')).toHaveLength(
      LEGACY_RUNTIME_STREAM_RECENT_ITEM_CAP + 6,
    )
    expect(stream.toolCalls).toHaveLength(LEGACY_RUNTIME_STREAM_RECENT_ITEM_CAP + 6)
  })

  it('preserves whitespace-bearing assistant transcript deltas while compacting the live turn', () => {
    let stream = createRuntimeStreamFromSubscription(streamSubscription)

    for (const [index, text] of ['Repository', ' ', 'instructions', ' - ', 'In this repo.'].entries()) {
      stream = mergeRuntimeStreamEvent(
        stream,
        makeTranscriptStreamEvent(text, { sequence: index + 1 }),
      )
    }

    expect(stream.transcriptItems).toHaveLength(1)
    expect(stream.transcriptItems[0]?.text).toBe('Repository instructions - In this repo.')
    expect(stream.items.filter((item) => item.kind === 'transcript').map((item) => item.text)).toEqual([
      'Repository instructions - In this repo.',
    ])
  })

  it('keeps a long streamed assistant reply as one expanding transcript instead of a capped delta window', () => {
    let stream = createRuntimeStreamFromSubscription(streamSubscription)
    const deltas = Array.from(
      { length: LEGACY_RUNTIME_STREAM_TRANSCRIPT_CAP + 12 },
      (_, index) => (index === 0 ? 'Chunk' : ` ${index}`),
    )

    deltas.forEach((text, index) => {
      stream = mergeRuntimeStreamEvent(
        stream,
        makeTranscriptStreamEvent(text, { sequence: index + 1 }),
      )
    })

    expect(stream.transcriptItems).toHaveLength(1)
    expect(stream.transcriptItems[0]?.text).toBe(deltas.join(''))
    expect(stream.items.filter((item) => item.kind === 'transcript')).toHaveLength(1)
  })

  it('keeps replayed transcript turns beyond the old recent-tail cap', () => {
    let stream = createRuntimeStreamFromSubscription(streamSubscription)
    const turnCount = LEGACY_RUNTIME_STREAM_TRANSCRIPT_CAP + 5

    for (let index = 0; index < turnCount; index += 1) {
      stream = mergeRuntimeStreamEvent(
        stream,
        makeTranscriptStreamEvent(`turn-${index}`, {
          sequence: index + 1,
          role: 'user',
        }),
      )
    }

    expect(stream.transcriptItems).toHaveLength(turnCount)
    expect(stream.items.filter((item) => item.kind === 'transcript')).toHaveLength(turnCount)
    expect(stream.transcriptItems[0]?.text).toBe('turn-0')
    expect(stream.transcriptItems.at(-1)?.text).toBe(`turn-${turnCount - 1}`)
  })

  it('keeps streamed reasoning activity deltas as one expanding timeline item', () => {
    let stream = createRuntimeStreamFromSubscription(streamSubscription)

    for (const [index, text] of ['I should inspect', ' the failing test', '\n\n'].entries()) {
      stream = mergeRuntimeStreamEvent(
        stream,
        makeStreamEvent(
          {
            kind: 'activity',
            sessionId: 'session-1',
            flowId: 'flow-1',
            text,
            toolCallId: null,
            toolName: null,
            toolState: null,
            actionType: null,
            title: 'Reasoning',
            detail: text.trim() || 'Owned agent reasoning summary updated.',
            code: 'owned_agent_reasoning',
            message: null,
            retryable: null,
            createdAt: `2026-04-13T20:03:${String(index + 1).padStart(2, '0')}Z`,
          },
          { sequence: index + 1 },
        ),
      )
    }

    const reasoningItems = stream.items.filter(
      (item) => item.kind === 'activity' && item.code === 'owned_agent_reasoning',
    )
    expect(reasoningItems).toHaveLength(1)
    expect(reasoningItems[0]).toMatchObject({
      text: 'I should inspect the failing test',
      detail: 'I should inspect the failing test',
    })
    expect(stream.items).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: 'activity',
          code: 'owned_agent_reasoning_boundary',
          sequence: 3,
        }),
      ]),
    )
  })

  it('projects browser/computer-use summaries through runtime tool rows and rejects malformed follow-up payloads', () => {
    const subscribed = createRuntimeStreamFromSubscription(streamSubscription)
    const withBrowserTool = mergeRuntimeStreamEvent(
      subscribed,
      makeStreamEvent({
        kind: 'tool',
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: null,
        toolCallId: 'browser-click-1',
        toolName: 'browser.click',
        toolState: 'succeeded',
        toolSummary: {
          kind: 'browser_computer_use',
          surface: 'browser',
          action: 'click',
          status: 'succeeded',
          target: 'button[type=submit]',
          outcome: 'Clicked submit and advanced to confirmation.',
        },
        actionType: null,
        title: null,
        detail: 'Browser click action reached the confirmation banner.',
        code: null,
        message: null,
        retryable: null,
        createdAt: '2026-04-13T20:01:01Z',
      }),
    )

    expect(withBrowserTool.toolCalls).toHaveLength(1)
    expect(withBrowserTool.toolCalls[0]).toMatchObject({
      toolCallId: 'browser-click-1',
      toolName: 'browser.click',
      toolState: 'succeeded',
      toolSummary: {
        kind: 'browser_computer_use',
        surface: 'browser',
        action: 'click',
        status: 'succeeded',
        target: 'button[type=submit]',
        outcome: 'Clicked submit and advanced to confirmation.',
      },
    })

    expect(() =>
      mergeRuntimeStreamEvent(
        withBrowserTool,
        makeStreamEvent({
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'browser-click-2',
          toolName: 'browser.click',
          toolState: 'failed',
          toolSummary: {
            kind: 'browser_computer_use',
            surface: 'browser',
            action: 'click',
            status: 'done',
            target: 'button[type=submit]',
            outcome: 'Malformed browser summary.',
          },
          actionType: null,
          title: null,
          detail: 'Malformed browser summary.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:02Z',
        } as unknown as RuntimeStreamItemDto),
      ),
    ).toThrow(/Invalid enum value/)
  })

  it('accepts additive tool summary contracts while rejecting malformed nested summary drift', () => {
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
      target: 'https://example.com/search?q=Xero',
      resultCount: 5,
      finalUrl: 'https://example.com/search?q=Xero',
      contentKind: 'html',
      contentType: 'text/html',
      truncated: false,
    })
    const browserComputerUseSummary = toolResultSummarySchema.parse({
      kind: 'browser_computer_use',
      surface: 'browser',
      action: 'click',
      status: 'succeeded',
      target: 'button[type=submit]',
      outcome: 'Clicked submit and advanced to confirmation.',
    })
    const mcpSummary = toolResultSummarySchema.parse({
      kind: 'mcp_capability',
      serverId: 'linear',
      capabilityKind: 'prompt',
      capabilityId: 'summarize_context',
      capabilityName: 'Summarize Context',
    })

    expect([
      fileSummary.kind,
      gitSummary.kind,
      webSummary.kind,
      browserComputerUseSummary.kind,
      mcpSummary.kind,
    ]).toEqual(['file', 'git', 'web', 'browser_computer_use', 'mcp_capability'])

    expect(gitSummary.kind).toBe('git')
    if (gitSummary.kind !== 'git') {
      throw new Error('Expected a git summary kind.')
    }
    expect(gitSummary.baseRevision).toBe('main~1')

    expect(mcpSummary.kind).toBe('mcp_capability')
    if (mcpSummary.kind !== 'mcp_capability') {
      throw new Error('Expected an MCP capability summary kind.')
    }
    expect(mcpSummary.capabilityKind).toBe('prompt')
    expect(mcpSummary.capabilityName).toBe('Summarize Context')

    expect(browserComputerUseSummary.kind).toBe('browser_computer_use')
    if (browserComputerUseSummary.kind !== 'browser_computer_use') {
      throw new Error('Expected a browser/computer-use summary kind.')
    }
    expect(browserComputerUseSummary.surface).toBe('browser')
    expect(browserComputerUseSummary.action).toBe('click')
    expect(browserComputerUseSummary.status).toBe('succeeded')

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'unknown',
        target: 'https://example.com',
        truncated: false,
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'browser_computer_use',
        surface: 'browser',
        action: 'click',
        target: 'button[type=submit]',
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'browser_computer_use',
        surface: 'tab',
        action: 'click',
        status: 'succeeded',
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'browser_computer_use',
        surface: 'computer_use',
        action: 'click',
        status: 'done',
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'mcp_capability',
        capabilityKind: 'tool',
        capabilityId: 'list_projects',
        capabilityName: 'List Projects',
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'mcp_capability',
        serverId: 'linear',
        capabilityKind: 'unsupported_kind',
        capabilityId: 'list_projects',
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'mcp_capability',
        serverId: 'linear',
        capabilityKind: 'tool',
        capabilityId: null,
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'command',
        exitCode: 0,
        timedOut: false,
        stdoutTruncated: 'yes',
        stderrTruncated: false,
        stdoutRedacted: false,
        stderrRedacted: false,
      }),
    ).toThrow()

    expect(() =>
      toolResultSummarySchema.parse({
        kind: 'file',
        path: 'src/lib.rs',
        scope: 'workspace',
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
          text: 'Connected to Xero.',
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
          text: 'Connected to Xero.',
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

  it('ignores stale replayed sequence regressions while preserving the latest transcript', () => {
    const liveStream = mergeRuntimeStreamEvent(
      createRuntimeStreamFromSubscription(streamSubscription),
      makeStreamEvent(
        {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Xero.',
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
          text: 'Connected to Xero.',
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
    expect(replayedSameSequence.status).toBe('live')

    const staleReplay = mergeRuntimeStreamEvent(
      {
        ...liveStream,
        status: 'replaying',
        lastIssue: {
          code: 'runtime_stream_contract_mismatch',
          message: 'Older replay events should not keep the stream degraded.',
          retryable: false,
          observedAt: '2026-04-13T20:01:02Z',
        },
      },
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
          detail: 'older replay should be ignored',
          code: 'phase_progress',
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:01Z',
        },
        { sequence: 6 },
      ),
    )

    expect(staleReplay.items).toHaveLength(1)
    expect(staleReplay.transcriptItems[0]?.text).toBe('Connected to Xero.')
    expect(staleReplay.lastSequence).toBe(7)
    expect(staleReplay.status).toBe('live')
    expect(staleReplay.lastIssue).toBeNull()
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
        text: 'Connected to Xero.',
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
      agentSessionId: 'agent-session-main',
      runtimeKind: 'openai_codex',
      sessionId: 'session-1',
      flowId: 'flow-1',
      subscribedItemKinds: streamSubscription.subscribedItemKinds,
      code: 'runtime_stream_not_ready',
      message: 'Xero marked the runtime stream stale.',
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
        message: 'Xero lost the runtime bootstrap stream while collecting repository context.',
        retryable: true,
        createdAt: '2026-04-13T20:02:01Z',
      }),
    )

    expect(staleStream.status).toBe('stale')
    expect(staleStream.items).toHaveLength(1)
    expect(staleStream.lastIssue?.message).toBe('Xero marked the runtime stream stale.')
    expect(failedStream.status).toBe('stale')
    expect(failedStream.items).toHaveLength(2)
    expect(failedStream.failure?.code).toBe('runtime_stream_bootstrap_failed')
    expect(failedStream.lastIssue?.retryable).toBe(true)
  })

  it('rejects malformed runtime stream payloads and cross-project merges at the model boundary', () => {
    expect(
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
      }).text,
    ).toBe('   ')

    expect(() =>
      runtimeStreamItemSchema.parse({
        kind: 'transcript',
        runId: 'run-1',
        sequence: 1,
        sessionId: 'session-1',
        flowId: 'flow-1',
        text: '',
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
    expect(
      browserControlSettingsSchema.parse({
        preference: 'default',
        updatedAt: null,
      }),
    ).toEqual({
      preference: 'default',
      updatedAt: null,
    })

    expect(
      upsertBrowserControlSettingsRequestSchema.parse({
        preference: 'native_browser',
      }),
    ).toEqual({
      preference: 'native_browser',
    })

    expect(() =>
      browserControlSettingsSchema.parse({
        preference: 'device_browser',
      }),
    ).toThrow(/preference/)

    expect(() =>
      runtimeSessionSchema.parse({
        ...makeRuntimeSession(),
        accountId: '   ',
      }),
    ).toThrow()

    expect(() =>
      runtimeSettingsSchema.parse({
        providerId: 'openai_codex',
        modelId: '   ',
        openrouterApiKeyConfigured: false,
      }),
    ).toThrow(/modelId/)

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
        modelId: '   ',
      }),
    ).toThrow()

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
        supervisorKind: 'owned_agent',
        status: 'running',
        transport: {
          kind: 'internal',
          endpoint: 'xero://owned-agent',
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


  it('keeps percent math bounded when counts are invalid or exceed total', () => {
    expect(safePercent(1, 0)).toBe(0)
    expect(safePercent(10, 4)).toBe(100)
    expect(safePercent(-1, 4)).toBe(0)
  })
})
