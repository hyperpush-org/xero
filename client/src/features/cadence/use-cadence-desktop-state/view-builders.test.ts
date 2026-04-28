import { describe, expect, it } from 'vitest'
import type {
  NotificationRouteDto,
  Phase,
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
    agentSessions: [],
    selectedAgentSession: null,
    selectedAgentSessionId: 'agent-session-main',
    notificationBroker: {
      projectId: 'project-1',
      dispatches: [],
      actions: [],
      latestActionAt: null,
      pendingActionCount: 0,
      failedActionCount: 0,
    },
    autonomousRun: null,
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
        planModeRequired: false,
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
        planModeRequired: false,
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
    agentSessionId: 'agent-session-main',
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
      runtimeKind: 'openrouter',
      label: 'OpenRouter Work',
      modelId: 'openai/gpt-4.1-mini',
      presetId: 'openrouter',
      active: activeProfileId === 'openrouter-work',
      readiness: {
        ready: true,
        status: 'ready',
        proof: 'stored_secret',
        proofUpdatedAt: '2026-04-20T11:58:00Z',
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





  it('buildAgentView keeps cached ambient catalog ownership and orphaned current selection attributable to Bedrock', () => {
    const project = makeProject()

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'bedrock-work',
        profiles: [
          {
            profileId: 'bedrock-work',
            providerId: 'bedrock',
            runtimeKind: 'anthropic',
            label: 'Amazon Bedrock Work',
            modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
            presetId: 'bedrock',
            region: 'us-east-1',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'ambient',
              proofUpdatedAt: '2026-04-20T11:58:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
      runtimeSession: makeRuntimeSession({ providerId: 'bedrock', runtimeKind: 'anthropic' }),
      runtimeSettings: makeRuntimeSettings({
        providerId: 'bedrock',
        modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
        openrouterApiKeyConfigured: false,
      }),
      activeProviderModelCatalog: makeProviderModelCatalog({
        profileId: 'bedrock-work',
        providerId: 'bedrock',
        configuredModelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
        source: 'cache',
        lastRefreshError: {
          code: 'provider_model_catalog_failed',
          message: 'Amazon Bedrock discovery timed out.',
          retryable: true,
        },
        models: [
          {
            modelId: 'anthropic.claude-3-haiku-20240307-v1:0',
            displayName: 'anthropic.claude-3-haiku-20240307-v1:0',
            thinking: {
              supported: false,
              effortOptions: [],
              defaultEffort: null,
            },
          },
        ],
      }),
      activeProviderModelCatalogLoadStatus: 'error',
      activeProviderModelCatalogLoadError: makeOperatorActionError({
        code: 'provider_model_catalog_failed',
        message: 'Amazon Bedrock discovery timed out.',
        retryable: true,
      }),
      runtimeRun: null,
      autonomousRun: null,
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

    expect(result.view?.providerModelCatalog).toMatchObject({
      providerId: 'bedrock',
      providerLabel: 'Amazon Bedrock',
      source: 'cache',
      state: 'stale',
      stateLabel: 'Cached catalog',
      lastRefreshError: {
        code: 'provider_model_catalog_failed',
        message: 'Amazon Bedrock discovery timed out.',
        retryable: true,
      },
    })
    expect(result.view?.providerModelCatalog.detail).toContain('Amazon Bedrock')
    expect(result.view?.providerModelCatalog.models[0]).toMatchObject({
      modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
      availability: 'orphaned',
      groupLabel: 'Amazon Bedrock Work · Amazon Bedrock',
    })
    expect(result.view?.selectedModelOption).toMatchObject({
      modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
      availability: 'orphaned',
    })
  })

  it('buildAgentView combines ready provider-profile catalogs with provider-scoped selection keys', () => {
    const project = makeProject()
    const providerProfiles = makeProviderProfiles({
      activeProfileId: 'openrouter-work',
      profiles: [
        {
          profileId: 'openrouter-work',
          providerId: 'openrouter',
          runtimeKind: 'openrouter',
          label: 'OpenRouter Work',
          modelId: 'openai/gpt-4.1-mini',
          presetId: 'openrouter',
          active: true,
          readiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T11:58:00Z',
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
        {
          profileId: 'openai-api-work',
          providerId: 'openai_api',
          runtimeKind: 'openai_compatible',
          label: 'OpenAI API Work',
          modelId: 'openai/gpt-4.1-mini',
          presetId: 'openai_api',
          active: false,
          readiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T11:59:00Z',
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
      ],
    })
    const openRouterCatalog = makeProviderModelCatalog()
    const openAiApiCatalog = makeProviderModelCatalog({
      profileId: 'openai-api-work',
      providerId: 'openai_api',
      configuredModelId: 'openai/gpt-4.1-mini',
      models: [
        {
          modelId: 'openai/gpt-4.1-mini',
          displayName: 'GPT-4.1 mini',
          thinking: {
            supported: true,
            effortOptions: ['low', 'medium'],
            defaultEffort: 'medium',
          },
        },
      ],
    })

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles,
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings(),
      providerModelCatalogs: {
        'openrouter-work': openRouterCatalog,
        'openai-api-work': openAiApiCatalog,
      },
      providerModelCatalogLoadStatuses: {
        'openrouter-work': 'ready',
        'openai-api-work': 'ready',
      },
      providerModelCatalogLoadErrors: {
        'openrouter-work': null,
        'openai-api-work': null,
      },
      activeProviderModelCatalog: openRouterCatalog,
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: null,
      autonomousRun: null,
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

    const duplicateModelOptions = result.view?.providerModelCatalog.models.filter(
      (model) => model.modelId === 'openai/gpt-4.1-mini',
    )

    expect(result.view?.providerModelCatalog.providerLabel).toBe('Configured providers')
    expect(duplicateModelOptions).toHaveLength(2)
    expect(duplicateModelOptions?.map((model) => model.selectionKey)).toEqual([
      'openrouter-work::openai%2Fgpt-4.1-mini',
      'openai-api-work::openai%2Fgpt-4.1-mini',
    ])
    expect(duplicateModelOptions?.map((model) => model.groupLabel)).toEqual([
      'OpenRouter Work · OpenRouter',
      'OpenAI API Work · OpenAI-compatible',
    ])
    expect(result.view?.selectedModelSelectionKey).toBe('openrouter-work::openai%2Fgpt-4.1-mini')
    expect(result.view?.selectedModelOption).toMatchObject({
      profileId: 'openrouter-work',
      providerId: 'openrouter',
      modelId: 'openai/gpt-4.1-mini',
    })
  })

  it('buildAgentView keeps runtime-run provider-profile selection over the active Settings profile', () => {
    const project = makeProject()
    const providerProfiles = makeProviderProfiles({
      activeProfileId: 'openrouter-work',
      profiles: [
        {
          profileId: 'openrouter-work',
          providerId: 'openrouter',
          runtimeKind: 'openrouter',
          label: 'OpenRouter Work',
          modelId: 'openai/gpt-4.1-mini',
          presetId: 'openrouter',
          active: true,
          readiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T11:58:00Z',
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
        {
          profileId: 'openrouter-personal',
          providerId: 'openrouter',
          runtimeKind: 'openrouter',
          label: 'OpenRouter Personal',
          modelId: 'openai/gpt-4.1-mini',
          presetId: 'openrouter',
          active: false,
          readiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T11:59:00Z',
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
      ],
    })
    const workCatalog = makeProviderModelCatalog()
    const personalCatalog = makeProviderModelCatalog({
      profileId: 'openrouter-personal',
      configuredModelId: 'openai/gpt-4.1-mini',
      models: [
        {
          modelId: 'openai/gpt-4.1-mini',
          displayName: 'GPT-4.1 Mini Personal',
          thinking: {
            supported: true,
            effortOptions: ['minimal', 'low'],
            defaultEffort: 'low',
          },
        },
      ],
    })

    const result = buildAgentView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      providerProfiles,
      runtimeSession: makeRuntimeSession({ providerId: 'openrouter', runtimeKind: 'openrouter' }),
      runtimeSettings: makeRuntimeSettings(),
      providerModelCatalogs: {
        'openrouter-work': workCatalog,
        'openrouter-personal': personalCatalog,
      },
      providerModelCatalogLoadStatuses: {
        'openrouter-work': 'ready',
        'openrouter-personal': 'ready',
      },
      providerModelCatalogLoadErrors: {
        'openrouter-work': null,
        'openrouter-personal': null,
      },
      activeProviderModelCatalog: workCatalog,
      activeProviderModelCatalogLoadStatus: 'ready',
      activeProviderModelCatalogLoadError: null,
      runtimeRun: makeRuntimeRun({
        providerId: 'openrouter',
        controls: {
          active: {
            providerProfileId: 'openrouter-personal',
            modelId: 'openai/gpt-4.1-mini',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          pending: null,
          selected: {
            source: 'active',
            providerProfileId: 'openrouter-personal',
            modelId: 'openai/gpt-4.1-mini',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
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
      selectedModelId: 'openai/gpt-4.1-mini',
      selectedModelSelectionKey: 'openrouter-personal::openai%2Fgpt-4.1-mini',
      selectedThinkingEffort: 'low',
      selectedModelOption: {
        profileId: 'openrouter-personal',
        profileLabel: 'OpenRouter Personal',
        modelId: 'openai/gpt-4.1-mini',
        defaultThinkingEffort: 'low',
      },
    })
    expect(result.view?.selectedModelThinkingEffortOptions).toEqual(['minimal', 'low'])
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
            providerProfileId: 'openrouter-work',
            modelId: 'openai/gpt-4.1-mini',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          pending: {
            providerProfileId: 'openrouter-work',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
            revision: 2,
            queuedAt: '2026-04-20T12:05:00Z',
            queuedPrompt: 'Review the diff before continuing.',
            queuedPromptAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          selected: {
            source: 'pending',
            providerProfileId: 'openrouter-work',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
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
            providerProfileId: 'openrouter-work',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
            revision: 3,
            appliedAt: '2026-04-20T12:10:00Z',
          },
          pending: null,
          selected: {
            source: 'active',
            providerProfileId: 'openrouter-work',
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
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
