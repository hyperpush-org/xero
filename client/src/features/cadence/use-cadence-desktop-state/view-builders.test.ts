import { describe, expect, it } from 'vitest'
import type {
  NotificationRouteDto,
  Phase,
  PlanningLifecycleView,
  ProjectDetailView,
  ProviderModelCatalogDto,
  ProviderProfilesDto,
  RepositoryStatusView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeSettingsDto,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'
import type { BlockedNotificationSyncPollTarget } from './notification-health'
import {
  buildAgentView,
  buildExecutionView,
  buildWorkflowView,
} from './view-builders'
import type {
  AgentTrustSnapshotView,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
} from './types'

function makeLifecycle(overrides: Partial<PlanningLifecycleView> = {}): PlanningLifecycleView {
  const activeStage =
    overrides.activeStage ??
    ({
      stage: 'research',
      stageLabel: 'Research',
      nodeId: 'workflow-research',
      status: 'active',
      statusLabel: 'Active',
      actionRequired: true,
      lastTransitionAt: '2026-04-20T12:00:00Z',
    } as PlanningLifecycleView['activeStage'])

  return {
    stages:
      overrides.stages ??
      [
        {
          stage: 'discussion',
          stageLabel: 'Discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          statusLabel: 'Complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-20T11:00:00Z',
        },
        activeStage,
      ],
    activeStage,
    hasStages: overrides.hasStages ?? true,
    percentComplete: overrides.percentComplete ?? 50,
    actionRequiredCount: overrides.actionRequiredCount ?? 1,
    ...overrides,
  } as PlanningLifecycleView
}

function makePhase(overrides: Partial<Phase> = {}): Phase {
  return {
    id: 2,
    name: 'Live projection',
    description: 'Project workflow truth into the shell',
    status: 'active',
    currentStep: 'verify',
    taskCount: 3,
    completedTasks: 2,
    summary: null,
    ...overrides,
  } as Phase
}

function makeProject(overrides: Partial<ProjectDetailView> = {}): ProjectDetailView {
  const phase = makePhase()

  return {
    id: 'project-1',
    name: 'Cadence',
    description: 'Desktop shell',
    milestone: 'M010',
    branch: 'main',
    branchLabel: 'main',
    runtimeLabel: 'Openai Codex · Authenticated',
    phaseProgressPercent: 67,
    phases: [phase],
    activePhase: phase.id,
    lifecycle: makeLifecycle(),
    repository: {
      id: 'repo-project-1',
      projectId: 'project-1',
      rootPath: '/tmp/cadence',
      displayName: 'Cadence',
      branch: 'main',
      branchLabel: 'main',
      headSha: 'abc1234',
      headShaLabel: 'abc1234',
      isGitRepo: true,
    },
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    verificationRecords: [],
    resumeHistory: [],
    notificationBroker: {
      projectId: 'project-1',
      dispatches: [],
      actions: [],
      latestActionAt: null,
      pendingActionCount: 0,
      failedActionCount: 0,
    },
    handoffPackages: [],
    autonomousRun: null,
    autonomousUnit: null,
    autonomousAttempt: null,
    autonomousHistory: [],
    autonomousRecentArtifacts: [],
    ...overrides,
  } as ProjectDetailView
}

function makeRepositoryStatus(overrides: Partial<RepositoryStatusView> = {}): RepositoryStatusView {
  return {
    branchLabel: 'feature/cadence',
    headShaLabel: 'def5678',
    lastCommit: null,
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: 'modified',
        unstaged: null,
        untracked: false,
      },
    ],
    stagedCount: 1,
    unstagedCount: 2,
    statusCount: 3,
    hasChanges: true,
    ...overrides,
  } as RepositoryStatusView
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    providerId: 'openai_codex',
    runtimeKind: 'openai_codex',
    runtimeLabel: 'Openai Codex · Authenticated',
    phase: 'authenticated',
    phaseLabel: 'Authenticated',
    sessionId: 'session-1',
    sessionLabel: 'session-1',
    accountLabel: 'acct-1',
    isAuthenticated: true,
    isLoginInProgress: false,
    lastError: null,
    ...overrides,
  } as RuntimeSessionView
}

function makeRuntimeRun(overrides: Partial<RuntimeRunView> = {}): RuntimeRunView {
  return {
    runId: 'run-project-1',
    providerId: 'openrouter',
    controls: {
      active: {
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        revision: 1,
        appliedAt: '2026-04-20T12:00:00Z',
      },
      pending: null,
      selected: {
        source: 'active',
        modelId: 'openai/gpt-4.1-mini',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        revision: 1,
        effectiveAt: '2026-04-20T12:00:00Z',
        queuedPrompt: null,
        queuedPromptAt: null,
        hasQueuedPrompt: false,
      },
      hasPendingControls: false,
    },
    isFailed: false,
    isStale: false,
    isActive: true,
    isTerminal: false,
    hasCheckpoints: true,
    lastError: null,
    ...overrides,
  } as RuntimeRunView
}

function makeRuntimeStream(overrides: Partial<RuntimeStreamView> = {}): RuntimeStreamView {
  return {
    runId: 'run-project-1',
    status: 'live',
    lastIssue: null,
    lastSequence: 4,
    actionRequired: [],
    completion: null,
    failure: null,
    items: [],
    skillItems: [],
    activityItems: [],
    ...overrides,
  } as RuntimeStreamView
}

function makeRuntimeSettings(overrides: Partial<RuntimeSettingsDto> = {}): RuntimeSettingsDto {
  return {
    providerId: 'openrouter',
    modelId: 'openai/gpt-4.1-mini',
    openrouterApiKeyConfigured: true,
    anthropicApiKeyConfigured: false,
    ...overrides,
  }
}

function makeProviderModelCatalog(
  overrides: Partial<ProviderModelCatalogDto> = {},
): ProviderModelCatalogDto {
  return {
    profileId: 'openrouter-work',
    providerId: 'openrouter',
    configuredModelId: 'openai/gpt-4.1-mini',
    source: 'live',
    fetchedAt: '2026-04-20T12:00:00Z',
    lastSuccessAt: '2026-04-20T12:00:00Z',
    lastRefreshError: null,
    models: [
      {
        modelId: 'openai/gpt-4.1-mini',
        displayName: 'openai/gpt-4.1-mini',
        thinking: {
          supported: true,
          effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
          defaultEffort: 'high',
        },
      },
      {
        modelId: 'anthropic/claude-3.5-haiku',
        displayName: 'anthropic/claude-3.5-haiku',
        thinking: {
          supported: true,
          effortOptions: ['low'],
          defaultEffort: 'low',
        },
      },
      {
        modelId: 'mistral/devstral-medium',
        displayName: 'mistral/devstral-medium',
        thinking: {
          supported: false,
          effortOptions: [],
          defaultEffort: null,
        },
      },
    ],
    ...overrides,
  } as ProviderModelCatalogDto
}

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  const activeProfileId = overrides.activeProfileId ?? 'openrouter-work'
  const profiles = overrides.profiles ?? [
    {
      profileId: 'openrouter-work',
      providerId: 'openrouter',
      label: 'OpenRouter Work',
      modelId: 'openai/gpt-4.1-mini',
      active: activeProfileId === 'openrouter-work',
      readiness: {
        ready: true,
        status: 'ready',
        credentialUpdatedAt: '2026-04-20T11:58:00Z',
      },
      migratedFromLegacy: false,
      migratedAt: null,
    },
  ]

  return {
    activeProfileId,
    profiles,
    migration: overrides.migration ?? null,
  }
}

function makeNotificationRoute(overrides: Partial<NotificationRouteDto> = {}): NotificationRouteDto {
  return {
    projectId: 'project-1',
    routeId: 'telegram-primary',
    routeKind: 'telegram',
    routeTarget: '@ops-room',
    enabled: true,
    metadataJson: null,
    credentialReadiness: {
      hasBotToken: true,
      hasChatId: true,
      hasWebhookUrl: false,
      ready: true,
      status: 'ready',
      diagnostic: null,
    },
    createdAt: '2026-04-20T12:00:00Z',
    updatedAt: '2026-04-20T12:00:00Z',
    ...overrides,
  }
}

function makeOperatorActionError(
  overrides: Partial<OperatorActionErrorView> = {},
): OperatorActionErrorView {
  return {
    code: 'operator_action_failed',
    message: 'Cadence could not finish the requested operator action.',
    retryable: false,
    ...overrides,
  }
}

function makeTrustSnapshot(
  overrides: Partial<AgentTrustSnapshotView> = {},
): AgentTrustSnapshotView {
  return {
    state: 'degraded',
    stateLabel: 'Degraded',
    runtimeState: 'healthy',
    runtimeReason: 'Runtime is authenticated.',
    streamState: 'healthy',
    streamReason: 'Live stream is connected.',
    approvalsState: 'healthy',
    approvalsReason: 'No approvals are pending.',
    routesState: 'healthy',
    routesReason: 'Route health is stable.',
    credentialsState: 'degraded',
    credentialsReason: 'One route is missing credentials.',
    syncState: 'degraded',
    syncReason: 'One sync reply was rejected.',
    routeCount: 2,
    enabledRouteCount: 2,
    degradedRouteCount: 1,
    readyCredentialRouteCount: 1,
    missingCredentialRouteCount: 1,
    malformedCredentialRouteCount: 0,
    unavailableCredentialRouteCount: 0,
    pendingApprovalCount: 0,
    syncDispatchFailedCount: 0,
    syncReplyRejectedCount: 1,
    routeError: null,
    syncError: null,
    projectionError: null,
    ...overrides,
  }
}

describe('view builders', () => {
  it('buildWorkflowView keeps lifecycle progress and provider selection projection stable', () => {
    const project = makeProject()
    const activePhase = project.phases[0] ?? null

    const view = buildWorkflowView({
      project,
      activePhase,
      providerProfiles: null,
      runtimeSession: makeRuntimeSession(),
      runtimeSettings: makeRuntimeSettings(),
    })

    expect(view).toMatchObject({
      project,
      activePhase,
      lifecyclePercent: 50,
      hasLifecycle: true,
      actionRequiredLifecycleCount: 1,
      overallPercent: 67,
      hasPhases: true,
      selectedProviderId: 'openrouter',
      selectedProviderLabel: 'OpenRouter',
      selectedProviderSource: 'runtime_settings',
      selectedModelId: 'openai/gpt-4.1-mini',
      openrouterApiKeyConfigured: true,
      providerMismatch: true,
      providerMismatchReason:
        'Settings now select provider OpenRouter, but the persisted runtime session still reflects OpenAI Codex.',
      providerMismatchRecoveryCopy:
        'Rebind the selected provider so durable runtime truth matches Settings.',
    })
    expect(view?.activeLifecycleStage?.stage).toBe('research')
  })

  it('buildWorkflowView exposes the selected provider-profile identity when app-local profiles are loaded', () => {
    const project = makeProject()
    const activePhase = project.phases[0] ?? null

    const view = buildWorkflowView({
      project,
      activePhase,
      providerProfiles: makeProviderProfiles(),
      runtimeSession: makeRuntimeSession({ providerId: 'openai_codex', runtimeKind: 'openai_codex' }),
      runtimeSettings: makeRuntimeSettings(),
    })

    expect(view).toMatchObject({
      selectedProfileId: 'openrouter-work',
      selectedProfileLabel: 'OpenRouter Work',
      selectedProviderId: 'openrouter',
      selectedProviderLabel: 'OpenRouter',
      selectedProviderSource: 'provider_profiles',
      providerMismatch: true,
      providerMismatchReason:
        'Settings now select provider profile OpenRouter Work (openrouter-work), but the persisted runtime session still reflects OpenAI Codex.',
      providerMismatchRecoveryCopy:
        'Rebind the selected profile so durable runtime truth matches Settings.',
    })
  })

  it('buildWorkflowView keeps Anthropic selected-profile mismatch copy explicit', () => {
    const project = makeProject()
    const activePhase = project.phases[0] ?? null

    const view = buildWorkflowView({
      project,
      activePhase,
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'anthropic-work',
        profiles: [
          {
            profileId: 'anthropic-work',
            providerId: 'anthropic',
            label: 'Anthropic Work',
            modelId: 'claude-3-7-sonnet-latest',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-20T11:58:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        openrouterApiKeyConfigured: false,
        anthropicApiKeyConfigured: true,
      }),
    })

    expect(view).toMatchObject({
      selectedProfileId: 'anthropic-work',
      selectedProfileLabel: 'Anthropic Work',
      selectedProviderId: 'anthropic',
      selectedProviderLabel: 'Anthropic',
      selectedProviderSource: 'provider_profiles',
      providerMismatch: true,
      providerMismatchReason:
        'Settings now select provider profile Anthropic Work (anthropic-work), but the persisted runtime session still reflects OpenRouter.',
      providerMismatchRecoveryCopy:
        'Rebind the selected profile so durable runtime truth matches Settings.',
    })
    expect(view?.providerMismatchReason).not.toContain('OpenAI')
  })

  it('buildAgentView falls back to the last known trust snapshot when trust projection data is malformed', () => {
    const project = makeProject()
    const previousTrustSnapshot = makeTrustSnapshot({
      syncReplyRejectedCount: 2,
      missingCredentialRouteCount: 3,
    })
    const notificationRouteError = makeOperatorActionError({
      code: 'notification_routes_failed',
      message: 'Routes refresh failed.',
      retryable: true,
    })
    const notificationSyncError = makeOperatorActionError({
      code: 'notification_sync_failed',
      message: 'Sync refresh failed.',
      retryable: true,
    })
    const blockedNotificationSyncPollTarget: BlockedNotificationSyncPollTarget = {
      projectId: 'project-1',
      actionId: 'flow-1:run-1:terminal_input_required',
      boundaryId: 'boundary-1',
    }
    const loadStatus: NotificationRoutesLoadStatus = 'loading'

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: null,
      runtimeSession: makeRuntimeSession(),
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog(),
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: makeRuntimeRun(),
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: makeRuntimeStream(),
      notificationRoutes: [
        makeNotificationRoute({
          credentialReadiness: {
            hasBotToken: true,
            hasChatId: true,
            hasWebhookUrl: false,
            ready: false,
            status: 'ready',
            diagnostic: null,
          } as never,
        }),
      ],
      notificationRouteLoadStatus: loadStatus,
      notificationRouteError,
      notificationSyncSummary: null,
      notificationSyncError,
      blockedNotificationSyncPollTarget,
      notificationRouteMutationStatus: 'running',
      pendingNotificationRouteId: 'telegram-primary',
      notificationRouteMutationError: null,
      previousTrustSnapshot,
      operatorActionStatus: 'running',
      pendingOperatorActionId: 'flow-1:review_worktree',
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.trustSnapshot).toMatchObject({
      state: previousTrustSnapshot.state,
      missingCredentialRouteCount: previousTrustSnapshot.missingCredentialRouteCount,
      syncReplyRejectedCount: previousTrustSnapshot.syncReplyRejectedCount,
      routeError: notificationRouteError,
      syncError: notificationSyncError,
    })
    expect(result.trustSnapshot?.projectionError?.message).toMatch(/malformed/i)
    expect(result.view).toMatchObject({
      branchLabel: 'feature/cadence',
      repositoryLabel: 'Cadence',
      repositoryPath: '/tmp/cadence',
      selectedProviderId: 'openrouter',
      selectedProviderLabel: 'OpenRouter',
      selectedProviderSource: 'runtime_settings',
      selectedModelId: 'openai/gpt-4.1-mini',
      providerModelCatalog: {
        providerId: 'openrouter',
        providerLabel: 'OpenRouter',
        source: 'live',
        state: 'live',
        stateLabel: 'Live catalog',
      },
      providerMismatch: true,
      providerMismatchReason:
        'Settings now select provider OpenRouter, but the persisted runtime session still reflects OpenAI Codex.',
      providerMismatchRecoveryCopy:
        'Rebind the selected provider so durable runtime truth matches Settings.',
      runtimeStreamStatus: 'live',
      runtimeStreamStatusLabel: 'Streaming live activity',
      notificationRouteLoadStatus: loadStatus,
      notificationRouteIsRefreshing: true,
      notificationSyncPollingActive: true,
      notificationSyncPollingActionId: blockedNotificationSyncPollTarget.actionId,
      notificationSyncPollingBoundaryId: blockedNotificationSyncPollTarget.boundaryId,
      notificationRouteMutationStatus: 'running',
      pendingNotificationRouteId: 'telegram-primary',
      sessionUnavailableReason:
        'Settings now select provider OpenRouter, but the persisted runtime session still reflects OpenAI Codex. Rebind the selected provider so durable runtime truth matches Settings.',
    })
    expect(result.view?.notificationRoutes).toHaveLength(1)
    expect(result.view?.notificationChannelHealth).toHaveLength(2)
    expect(result.view?.selectedModelOption).toMatchObject({
      modelId: 'openai/gpt-4.1-mini',
      groupLabel: 'OpenAI',
      thinkingSupported: true,
      defaultThinkingEffort: 'high',
    })
    expect(result.view?.selectedModelThinkingEffortOptions).toEqual(['minimal', 'low', 'medium', 'high', 'x_high'])
    expect(result.view?.selectedModelDefaultThinkingEffort).toBe('high')
    expect(result.view?.recentAutonomousUnits?.totalCount).toBe(0)
    expect(result.view?.checkpointControlLoop?.totalCount).toBe(0)
  })

  it('buildAgentView preserves an orphaned configured model and cached catalog state when discovery cannot confirm it', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles(),
      runtimeSession: makeRuntimeSession({ providerId: 'openai_codex', runtimeKind: 'openai_codex' }),
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog({
        source: 'cache',
        lastRefreshError: {
          code: 'provider_model_catalog_failed',
          message: 'OpenRouter discovery timed out.',
          retryable: true,
        },
        models: [
          {
            modelId: 'mistral/devstral-medium',
            displayName: 'mistral/devstral-medium',
            thinking: {
              supported: false,
              effortOptions: [],
              defaultEffort: null,
            },
          },
          {
            modelId: ' ',
            displayName: 'broken-row',
            thinking: {
              supported: true,
              effortOptions: ['low'],
              defaultEffort: 'low',
            },
          } as never,
        ],
      }),
      activeProviderModelCatalogLoadStatus: 'error',
      activeProviderModelCatalogLoadError: makeOperatorActionError({
        code: 'provider_model_catalog_failed',
        message: 'OpenRouter discovery timed out.',
        retryable: true,
      }),
      runtimeRun: makeRuntimeRun(),
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: makeRuntimeStream(),
      notificationRoutes: [],
      notificationRouteLoadStatus: 'idle',
      notificationRouteError: null,
      notificationSyncSummary: null,
      notificationSyncError: null,
      blockedNotificationSyncPollTarget: null,
      notificationRouteMutationStatus: 'idle',
      pendingNotificationRouteId: null,
      notificationRouteMutationError: null,
      previousTrustSnapshot: null,
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.view?.providerModelCatalog).toMatchObject({
      state: 'stale',
      stateLabel: 'Cached catalog',
      source: 'cache',
      lastRefreshError: {
        code: 'provider_model_catalog_failed',
        message: 'OpenRouter discovery timed out.',
        retryable: true,
      },
    })
    expect(result.view?.providerModelCatalog.models).toHaveLength(2)
    expect(result.view?.providerModelCatalog.models[0]).toMatchObject({
      modelId: 'openai/gpt-4.1-mini',
      availability: 'orphaned',
      groupLabel: 'Current selection',
    })
    expect(result.view?.providerModelCatalog.models[1]).toMatchObject({
      modelId: 'mistral/devstral-medium',
      availability: 'available',
    })
    expect(result.view?.selectedModelOption).toMatchObject({
      modelId: 'openai/gpt-4.1-mini',
      availability: 'orphaned',
      thinkingSupported: false,
    })
    expect(result.view?.selectedModelThinkingEffortOptions).toEqual([])
    expect(result.view?.selectedModelDefaultThinkingEffort).toBeNull()
  })

  it('buildAgentView prefers non-terminal runtime-run controls over provider defaults and keeps active-versus-pending truth separate', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles(),
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog(),
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: makeRuntimeRun({
        providerId: 'openrouter',
        controls: {
          active: {
            modelId: 'openai/gpt-4.1-mini',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          pending: {
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            revision: 2,
            queuedAt: '2026-04-20T12:05:00Z',
            queuedPrompt: 'Review the diff before continuing.',
            queuedPromptAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          selected: {
            source: 'pending',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            revision: 2,
            effectiveAt: '2026-04-20T12:05:00Z',
            queuedPrompt: 'Review the diff before continuing.',
            queuedPromptAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          hasPendingControls: true,
        },
      }),
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: makeRuntimeStream(),
      notificationRoutes: [],
      notificationRouteLoadStatus: 'idle',
      notificationRouteError: null,
      notificationSyncSummary: null,
      notificationSyncError: null,
      blockedNotificationSyncPollTarget: null,
      notificationRouteMutationStatus: 'idle',
      pendingNotificationRouteId: null,
      notificationRouteMutationError: null,
      previousTrustSnapshot: null,
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.view).toMatchObject({
      controlTruthSource: 'runtime_run',
      selectedModelId: 'anthropic/claude-3.5-haiku',
      selectedThinkingEffort: 'low',
      selectedApprovalMode: 'yolo',
      selectedPrompt: {
        text: 'Review the diff before continuing.',
        queuedAt: '2026-04-20T12:05:00Z',
        hasQueuedPrompt: true,
      },
      runtimeRunActiveControls: {
        modelId: 'openai/gpt-4.1-mini',
        approvalMode: 'suggest',
        revision: 1,
      },
      runtimeRunPendingControls: {
        modelId: 'anthropic/claude-3.5-haiku',
        approvalMode: 'yolo',
        revision: 2,
      },
    })
    expect(result.view?.selectedModelOption).toMatchObject({
      modelId: 'anthropic/claude-3.5-haiku',
      availability: 'available',
    })
  })

  it('buildAgentView falls back to provider/catalog truth after the runtime run becomes terminal while preserving the final run snapshots', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles(),
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog(),
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: makeRuntimeRun({
        providerId: 'openrouter',
        isActive: false,
        isTerminal: true,
        status: 'stopped',
        statusLabel: 'Run stopped',
        controls: {
          active: {
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            revision: 3,
            appliedAt: '2026-04-20T12:10:00Z',
          },
          pending: null,
          selected: {
            source: 'active',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            revision: 3,
            effectiveAt: '2026-04-20T12:10:00Z',
            queuedPrompt: null,
            queuedPromptAt: null,
            hasQueuedPrompt: false,
          },
          hasPendingControls: false,
        },
      }),
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: makeRuntimeStream(),
      notificationRoutes: [],
      notificationRouteLoadStatus: 'idle',
      notificationRouteError: null,
      notificationSyncSummary: null,
      notificationSyncError: null,
      blockedNotificationSyncPollTarget: null,
      notificationRouteMutationStatus: 'idle',
      pendingNotificationRouteId: null,
      notificationRouteMutationError: null,
      previousTrustSnapshot: null,
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.view).toMatchObject({
      controlTruthSource: 'fallback',
      selectedModelId: 'openai/gpt-4.1-mini',
      selectedThinkingEffort: 'high',
      selectedApprovalMode: 'suggest',
      runtimeRunActiveControls: {
        modelId: 'anthropic/claude-3.5-haiku',
        approvalMode: 'yolo',
        revision: 3,
      },
      runtimeRunPendingControls: null,
    })
  })

  it('buildAgentView keeps GitHub Models catalog grouping and bind copy explicit for namespaced model ids', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'github-models-work',
        profiles: [
          {
            profileId: 'github-models-work',
            providerId: 'github_models',
            label: 'GitHub Models Work',
            modelId: 'openai/gpt-4.1',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-20T11:58:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
      runtimeSession: null,
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog({
        profileId: 'github-models-work',
        providerId: 'github_models',
        configuredModelId: 'openai/gpt-4.1',
        models: [
          {
            modelId: 'openai/gpt-4.1',
            displayName: 'GPT-4.1',
            thinking: {
              supported: true,
              effortOptions: ['low', 'medium', 'high'],
              defaultEffort: 'medium',
            },
          },
          {
            modelId: 'meta/llama-4-maverick',
            displayName: 'Llama 4 Maverick',
            thinking: {
              supported: false,
              effortOptions: [],
              defaultEffort: null,
            },
          },
        ],
      }),
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: null,
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: null,
      notificationRoutes: [],
      notificationRouteLoadStatus: 'idle',
      notificationRouteError: null,
      notificationSyncSummary: null,
      notificationSyncError: null,
      blockedNotificationSyncPollTarget: null,
      notificationRouteMutationStatus: 'idle',
      pendingNotificationRouteId: null,
      notificationRouteMutationError: null,
      previousTrustSnapshot: null,
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.view).toMatchObject({
      selectedProfileId: 'github-models-work',
      selectedProfileLabel: 'GitHub Models Work',
      selectedProviderId: 'github_models',
      selectedProviderLabel: 'GitHub Models',
      sessionUnavailableReason:
        'Bind GitHub Models with the selected app-local provider profile to create a project runtime session.',
      providerModelCatalog: {
        providerId: 'github_models',
        providerLabel: 'GitHub Models',
        state: 'live',
        stateLabel: 'Live catalog',
      },
      selectedModelOption: {
        modelId: 'openai/gpt-4.1',
        groupLabel: 'GitHub Models · OpenAI',
        thinkingSupported: true,
        defaultThinkingEffort: 'medium',
      },
    })
    expect(result.view?.providerModelCatalog.models.map((model) => model.groupLabel)).toEqual([
      'GitHub Models · OpenAI',
      'GitHub Models · Meta',
    ])
    expect(result.view?.messagesUnavailableReason).toContain('GitHub Models')
    expect(result.view?.messagesUnavailableReason).not.toContain('OpenAI-compatible')
  })

  it('buildAgentView keeps recovered GitHub Models run truth visible when current Settings point elsewhere', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles(),
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings(),
      activeProviderModelCatalog: makeProviderModelCatalog(),
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: makeRuntimeRun({
        providerId: 'github_models',
        controls: {
          active: {
            modelId: 'openai/gpt-4.1',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          pending: null,
          selected: {
            source: 'active',
            modelId: 'openai/gpt-4.1',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            revision: 1,
            effectiveAt: '2026-04-20T12:00:00Z',
            queuedPrompt: null,
            queuedPromptAt: null,
            hasQueuedPrompt: false,
          },
          hasPendingControls: false,
        },
      }),
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
      runtimeErrorMessage: null,
      runtimeRunErrorMessage: null,
      autonomousRunErrorMessage: null,
      runtimeStream: makeRuntimeStream(),
      notificationRoutes: [],
      notificationRouteLoadStatus: 'idle',
      notificationRouteError: null,
      notificationSyncSummary: null,
      notificationSyncError: null,
      blockedNotificationSyncPollTarget: null,
      notificationRouteMutationStatus: 'idle',
      pendingNotificationRouteId: null,
      notificationRouteMutationError: null,
      previousTrustSnapshot: null,
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      operatorActionError: null,
      autonomousRunActionStatus: 'idle',
      pendingAutonomousRunAction: null,
      autonomousRunActionError: null,
      runtimeRunActionStatus: 'idle',
      pendingRuntimeRunAction: null,
      runtimeRunActionError: null,
    })

    expect(result.view).toMatchObject({
      selectedProviderId: 'openrouter',
      selectedProviderLabel: 'OpenRouter',
      controlTruthSource: 'runtime_run',
      selectedModelId: 'openai/gpt-4.1',
      providerModelCatalog: {
        providerId: 'github_models',
        providerLabel: 'GitHub Models',
        state: 'unavailable',
        stateLabel: 'Catalog unavailable',
      },
      selectedModelOption: {
        modelId: 'openai/gpt-4.1',
        availability: 'orphaned',
        groupLabel: 'GitHub Models · Current selection',
      },
    })
    expect(result.view?.providerModelCatalog.detail).toContain('GitHub Models run-scoped control truth')
    expect(result.view?.providerModelCatalog.detail).not.toContain('provider defaults out of the live projection')
  })

  it('buildExecutionView keeps diff-scope counts and durable verification copy aligned with repository truth', () => {
    const project = makeProject({
      verificationRecords: [
        {
          id: 1,
          sourceActionId: 'flow-1:review_worktree',
          status: 'passed',
          statusLabel: 'Passed',
          summary: 'Approved operator action.',
          detail: null,
          recordedAt: '2026-04-20T12:10:00Z',
        },
      ],
      resumeHistory: [
        {
          id: 1,
          sourceActionId: 'flow-1:review_worktree',
          sessionId: 'session-1',
          status: 'started',
          statusLabel: 'Started',
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-20T12:11:00Z',
        },
      ],
    })

    const view = buildExecutionView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      operatorActionError: null,
    })

    expect(view).toMatchObject({
      branchLabel: 'feature/cadence',
      headShaLabel: 'def5678',
      statusCount: 3,
      hasChanges: true,
      verificationUnavailableReason:
        'Durable operator verification and resume history are loaded from the selected project snapshot.',
    })
    expect(view?.diffScopes).toEqual([
      { scope: 'staged', label: 'Staged', count: 1 },
      { scope: 'unstaged', label: 'Unstaged', count: 2 },
      { scope: 'worktree', label: 'Worktree', count: 3 },
    ])
  })
})
