import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { openUrlMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

vi.mock('../components/cadence/code-editor', () => ({
  CodeEditor: ({ filePath, onChange, onSave, value }: any) => (
    <div>
      <label>
        <span className="sr-only">Editor for {filePath}</span>
        <textarea
          aria-label={`Editor for ${filePath}`}
          onChange={(event) => onChange(event.target.value)}
          value={value}
        />
      </label>
      <button onClick={onSave} type="button">
        Trigger save
      </button>
    </div>
  ),
}))

afterEach(() => {
  openUrlMock.mockReset()
})

import { CadenceApp } from './App'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import type {
  ApplyWorkflowTransitionResponseDto,
  AutonomousRunStateDto,
  AutonomousUnitArtifactDto,
  AutonomousUnitAttemptDto,
  AutonomousUnitHistoryEntryDto,
  ImportRepositoryResponseDto,
  ListNotificationDispatchesResponseDto,
  ListNotificationRoutesResponseDto,
  ListProjectFilesResponseDto,
  ListProjectsResponseDto,
  OperatorApprovalDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  ProviderModelCatalogDto,
  ProviderProfilesDto,
  RepositoryDiffResponseDto,
  RepositoryStatusChangedPayloadDto,
  RepositoryStatusResponseDto,
  ResumeHistoryEntryDto,
  RuntimeRunControlInputDto,
  RuntimeRunDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeSettingsDto,
  RuntimeStreamEventDto,
  RuntimeUpdatedPayloadDto,
  SubscribeRuntimeStreamResponseDto,
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteRequestDto,
  UpsertRuntimeSettingsRequestDto,
  UpsertWorkflowGraphResponseDto,
} from '@/src/lib/cadence-model'

function makeProjectSummary(id: string, name: string): ListProjectsResponseDto['projects'][number] {
  return {
    id,
    name,
    description: `${name} description`,
    milestone: 'M001',
    totalPhases: 0,
    completedPhases: 0,
    activePhase: 0,
    branch: null,
    runtime: null,
  }
}

function makeSnapshot(projectId = 'project-1', name = 'Cadence'): ProjectSnapshotResponseDto {
  return {
    project: makeProjectSummary(projectId, name),
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: `/tmp/${name}`,
      displayName: name,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    phases: [],
    lifecycle: { stages: [] },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    handoffPackages: [],
    notificationDispatches: [],
    notificationReplyClaims: [],
  }
}

function makeStatus(projectId = 'project-1', name = 'Cadence'): RepositoryStatusResponseDto {
  return {
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: `/tmp/${name}`,
      displayName: name,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: null,
    entries: [],
    hasStagedChanges: false,
    hasUnstagedChanges: false,
    hasUntrackedChanges: false,
  }
}

function makeDiff(projectId = 'project-1', scope: RepositoryDiffResponseDto['scope'] = 'unstaged'): RepositoryDiffResponseDto {
  return {
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: '/tmp/Cadence',
      displayName: 'Cadence',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    scope,
    patch: '',
    truncated: false,
    baseRevision: null,
  }
}

function makeProjectFiles(projectId = 'project-1'): ListProjectFilesResponseDto {
  return {
    projectId,
    root: {
      name: 'root',
      path: '/',
      type: 'folder',
      children: [
        {
          name: 'README.md',
          path: '/README.md',
          type: 'file',
        },
        {
          name: 'src',
          path: '/src',
          type: 'folder',
          children: [
            {
              name: 'App.tsx',
              path: '/src/App.tsx',
              type: 'file',
            },
          ],
        },
      ],
    },
  }
}

function makeRuntimeSession(projectId = 'project-1', overrides: Partial<RuntimeSessionDto> = {}): RuntimeSessionDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: 'session-1',
    accountId: 'acct-1',
    phase: 'authenticated',
    callbackBound: true,
    authorizationUrl: 'https://auth.openai.com/oauth/authorize?client_id=test',
    redirectUri: 'http://127.0.0.1:1455/auth/callback',
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:00Z',
    ...overrides,
  }
}

function makeRuntimeSettings(overrides: Partial<RuntimeSettingsDto> = {}): RuntimeSettingsDto {
  return {
    providerId: 'openai_codex',
    modelId: 'openai_codex',
    openrouterApiKeyConfigured: false,
    anthropicApiKeyConfigured: false,
    ...overrides,
  }
}

function makeProviderProfilesFromRuntimeSettings(runtimeSettings: RuntimeSettingsDto): ProviderProfilesDto {
  const activeProfileId =
    runtimeSettings.providerId === 'openrouter'
      ? 'openrouter-default'
      : runtimeSettings.providerId === 'anthropic'
        ? 'anthropic-default'
        : 'openai_codex-default'

  return {
    activeProfileId,
    profiles: [
      {
        profileId: activeProfileId,
        providerId: runtimeSettings.providerId,
        label:
          runtimeSettings.providerId === 'openrouter'
            ? 'OpenRouter'
            : runtimeSettings.providerId === 'anthropic'
              ? 'Anthropic'
              : 'OpenAI Codex',
        modelId: runtimeSettings.modelId,
        active: true,
        readiness:
          runtimeSettings.providerId === 'openrouter'
            ? {
                ready: runtimeSettings.openrouterApiKeyConfigured,
                status: runtimeSettings.openrouterApiKeyConfigured ? 'ready' : 'missing',
                credentialUpdatedAt: runtimeSettings.openrouterApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
              }
            : runtimeSettings.providerId === 'anthropic'
              ? {
                  ready: runtimeSettings.anthropicApiKeyConfigured,
                  status: runtimeSettings.anthropicApiKeyConfigured ? 'ready' : 'missing',
                  credentialUpdatedAt: runtimeSettings.anthropicApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
                }
              : {
                  ready: false,
                  status: 'missing',
                  credentialUpdatedAt: null,
                },
        migratedFromLegacy: false,
        migratedAt: null,
      },
    ],
    migration: null,
  }
}

function buildProviderModelCatalog(profile: ProviderProfilesDto['profiles'][number]): ProviderModelCatalogDto {
  const isOpenRouter = profile.providerId === 'openrouter'
  const isAnthropic = profile.providerId === 'anthropic'
  const isReady = isOpenRouter || isAnthropic ? profile.readiness.ready : true

  return {
    profileId: profile.profileId,
    providerId: profile.providerId,
    configuredModelId: profile.modelId,
    source: isReady ? 'live' : 'unavailable',
    fetchedAt: isReady ? '2026-04-21T12:00:00Z' : null,
    lastSuccessAt: isReady ? '2026-04-21T12:00:00Z' : null,
    lastRefreshError:
      !isReady
        ? {
            code: isOpenRouter ? 'openrouter_credentials_missing' : 'anthropic_api_key_missing',
            message: `Configure an ${isOpenRouter ? 'OpenRouter' : 'Anthropic'} API key before refreshing provider models.`,
            retryable: false,
          }
        : null,
    models: isOpenRouter
      ? isReady
        ? [
            {
              modelId: profile.modelId,
              displayName: 'OpenRouter model',
              thinking: {
                supported: true,
                effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                defaultEffort: 'medium',
              },
            },
          ]
        : []
      : isAnthropic
        ? isReady
          ? [
              {
                modelId: 'claude-3-7-sonnet-latest',
                displayName: 'Claude 3.7 Sonnet',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high', 'x_high'],
                  defaultEffort: 'medium',
                },
              },
              {
                modelId: 'claude-3-5-haiku-latest',
                displayName: 'Claude 3.5 Haiku',
                thinking: {
                  supported: false,
                  effortOptions: [],
                  defaultEffort: null,
                },
              },
            ]
          : []
        : [
            {
              modelId: 'openai_codex',
              displayName: 'OpenAI Codex',
              thinking: {
                supported: true,
                effortOptions: ['low', 'medium', 'high'],
                defaultEffort: 'medium',
              },
            },
          ],
  }
}

function makeRuntimeRun(projectId = 'project-1', overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  const runtimeRun: RuntimeRunDto = {
    projectId,
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    supervisorKind: 'detached_pty',
    status: 'running',
    transport: {
      kind: 'tcp',
      endpoint: '127.0.0.1:4455',
      liveness: 'reachable',
    },
    controls: {
      active: {
        modelId: 'openai_codex',
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
        kind: 'bootstrap',
        summary: 'Supervisor boot recorded.',
        createdAt: '2026-04-15T20:00:01Z',
      },
    ],
    ...overrides,
  }

  return runtimeRun
}

function makeRuntimeApproval(actionId: string, overrides: Partial<OperatorApprovalDto> = {}): OperatorApprovalDto {
  return {
    actionId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_command',
    title: 'Review destructive shell command',
    detail: 'Cadence blocked a destructive shell wrapper and needs approval before continuing.',
    gateNodeId: null,
    gateKey: null,
    transitionFromNodeId: null,
    transitionToNodeId: null,
    transitionKind: null,
    userAnswer: null,
    status: 'pending',
    decisionNote: null,
    createdAt: '2026-04-22T12:07:00Z',
    updatedAt: '2026-04-22T12:07:00Z',
    resolvedAt: null,
    ...overrides,
  }
}

function makeResumeHistoryEntry(actionId: string, overrides: Partial<ResumeHistoryEntryDto> = {}): ResumeHistoryEntryDto {
  return {
    id: 1,
    sourceActionId: actionId,
    sessionId: 'session-1',
    status: 'failed',
    summary: 'Operator resume failed and is waiting for corrected shell input.',
    createdAt: '2026-04-22T12:07:05Z',
    ...overrides,
  }
}

function makeRecoveredPolicyDeniedAutonomousState(
  projectId = 'project-1',
  options: {
    actionId: string
    boundaryId: string
    runtimeRunId?: string
    diagnosticCode?: string
  },
): AutonomousRunStateDto {
  const runtimeRunId = options.runtimeRunId ?? 'run-1'
  const autonomousRunId = `auto-${projectId}`
  const unitId = `${autonomousRunId}:${options.boundaryId}`
  const attemptId = `${unitId}:attempt-1`
  const baseState = makeAutonomousRunState(projectId, autonomousRunId)
  const attempt: AutonomousUnitAttemptDto = {
    projectId,
    runId: autonomousRunId,
    unitId,
    attemptId,
    attemptNumber: 1,
    childSessionId: `${runtimeRunId}:child-session-1`,
    status: 'failed',
    boundaryId: options.boundaryId,
    workflowLinkage: null,
    startedAt: '2026-04-22T12:08:00Z',
    finishedAt: '2026-04-22T12:08:10Z',
    updatedAt: '2026-04-22T12:08:10Z',
    lastErrorCode: options.diagnosticCode ?? 'policy_denied_command_cwd_outside_repo',
    lastError: {
      code: options.diagnosticCode ?? 'policy_denied_command_cwd_outside_repo',
      message: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
    },
  }
  const deniedArtifactId = `${attemptId}:policy-denied`
  const verificationArtifactId = `${attemptId}:verification`
  const deniedArtifact: AutonomousUnitArtifactDto = {
    projectId,
    runId: autonomousRunId,
    unitId,
    attemptId,
    artifactId: deniedArtifactId,
    artifactKind: 'policy_denied',
    status: 'recorded',
    summary: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
    contentHash: 'policy-denied-hash',
    payload: {
      kind: 'policy_denied',
      projectId,
      runId: autonomousRunId,
      unitId,
      attemptId,
      artifactId: deniedArtifactId,
      diagnosticCode: options.diagnosticCode ?? 'policy_denied_command_cwd_outside_repo',
      message: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      toolName: 'bash',
      actionId: options.actionId,
      boundaryId: options.boundaryId,
    },
    createdAt: '2026-04-22T12:08:10Z',
    updatedAt: '2026-04-22T12:08:10Z',
  }
  const verificationArtifact: AutonomousUnitArtifactDto = {
    projectId,
    runId: autonomousRunId,
    unitId,
    attemptId,
    artifactId: verificationArtifactId,
    artifactKind: 'verification_evidence',
    status: 'recorded',
    summary:
      'Autonomous attempt recorded stable policy denial `policy_denied_command_cwd_outside_repo` for the denied shell command.',
    contentHash: 'verification-hash',
    payload: {
      kind: 'verification_evidence',
      projectId,
      runId: autonomousRunId,
      unitId,
      attemptId,
      artifactId: verificationArtifactId,
      evidenceKind: 'policy_denial',
      label: 'Policy denial retained in durable history',
      outcome: 'failed',
      commandResult: {
        exitCode: 1,
        timedOut: false,
        summary: 'Cadence denied the shell command before execution.',
      },
      actionId: options.actionId,
      boundaryId: options.boundaryId,
    },
    createdAt: '2026-04-22T12:08:11Z',
    updatedAt: '2026-04-22T12:08:11Z',
  }
  const historyEntry: AutonomousUnitHistoryEntryDto = {
    unit: {
      ...baseState.unit!,
      runId: autonomousRunId,
      unitId,
      sequence: 2,
      kind: 'diagnostic',
      status: 'failed',
      summary: 'Cadence recorded a terminal shell-policy denial for this checkpoint boundary.',
      boundaryId: options.boundaryId,
      finishedAt: '2026-04-22T12:08:10Z',
      updatedAt: '2026-04-22T12:08:10Z',
      lastErrorCode: options.diagnosticCode ?? 'policy_denied_command_cwd_outside_repo',
      lastError: {
        code: options.diagnosticCode ?? 'policy_denied_command_cwd_outside_repo',
        message: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      },
    },
    latestAttempt: attempt,
    artifacts: [deniedArtifact, verificationArtifact],
  }

  return {
    run: {
      ...baseState.run!,
      runId: autonomousRunId,
      activeUnitId: unitId,
      updatedAt: '2026-04-22T12:08:10Z',
    },
    unit: historyEntry.unit,
    attempt,
    history: [historyEntry],
  }
}

function makeRuntimeStreamActionRequiredEvent(options: {
  actionId: string
  boundaryId: string
  detail: string
  runId?: string
  sequence?: number
  title?: string
}): RuntimeStreamEventDto {
  const runId = options.runId ?? 'run-1'

  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    runId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind: 'action_required',
      runId,
      sequence: options.sequence ?? 5,
      sessionId: 'session-1',
      flowId: 'flow-1',
      text: null,
      toolCallId: null,
      toolName: null,
      toolState: null,
      actionId: options.actionId,
      boundaryId: options.boundaryId,
      actionType: 'review_command',
      title: options.title ?? 'Review destructive shell command',
      detail: options.detail,
      code: null,
      message: null,
      retryable: null,
      createdAt: '2026-04-22T12:07:10Z',
    },
  }
}

function ensureCheckpointSection(): HTMLElement {
  const section = screen.getByRole('heading', { name: 'Checkpoint control loop' }).closest('section')
  expect(section).not.toBeNull()
  return section as HTMLElement
}

function makeAutonomousRunState(projectId = 'project-1', runId = 'auto-run-1'): AutonomousRunStateDto {
  return {
    run: {
      projectId,
      runId,
      runtimeKind: 'openai_codex',
      providerId: 'openai_codex',
      supervisorKind: 'detached_pty',
      status: 'running',
      recoveryState: 'healthy',
      activeUnitId: `${runId}:checkpoint:1`,
      duplicateStartDetected: false,
      duplicateStartRunId: null,
      duplicateStartReason: null,
      startedAt: '2026-04-16T20:00:00Z',
      lastHeartbeatAt: '2026-04-16T20:00:05Z',
      lastCheckpointAt: '2026-04-16T20:00:06Z',
      pausedAt: null,
      cancelledAt: null,
      completedAt: null,
      crashedAt: null,
      stoppedAt: null,
      pauseReason: null,
      cancelReason: null,
      crashReason: null,
      lastErrorCode: null,
      lastError: null,
      updatedAt: '2026-04-16T20:00:06Z',
    },
    unit: {
      projectId,
      runId,
      unitId: `${runId}:checkpoint:1`,
      sequence: 1,
      kind: 'state',
      status: 'active',
      summary: 'Recovered the current autonomous unit boundary.',
      boundaryId: 'checkpoint:1',
      workflowLinkage: null,
      startedAt: '2026-04-16T20:00:01Z',
      finishedAt: null,
      updatedAt: '2026-04-16T20:00:06Z',
      lastErrorCode: null,
      lastError: null,
    },
  }
}

function ensureKnownProjectId(projectId: string, knownProjectIds: string[], context: string) {
  if (knownProjectIds.includes(projectId)) {
    return
  }

  throw new Error(
    `${context} expected one of [${knownProjectIds.join(', ')}] but received projectId \`${projectId}\`.`,
  )
}

function ensureCompatibleRuntimeRun(
  projectId: string,
  currentRuntimeRun: RuntimeRunDto | null,
  nextRuntimeRun: RuntimeRunDto | null,
  context: string,
) {
  if (!nextRuntimeRun) {
    return
  }

  if (nextRuntimeRun.projectId !== projectId) {
    throw new Error(
      `${context} expected run.projectId \`${projectId}\` but received \`${nextRuntimeRun.projectId}\`.`,
    )
  }

  if (!currentRuntimeRun || currentRuntimeRun.runId === nextRuntimeRun.runId) {
    return
  }

  throw new Error(
    `${context} expected active runId \`${currentRuntimeRun.runId}\` for project \`${projectId}\`; clear the active run before attaching \`${nextRuntimeRun.runId}\`.`,
  )
}

function cloneRuntimeRun(runtimeRun: RuntimeRunDto): RuntimeRunDto {
  return {
    ...runtimeRun,
    transport: { ...runtimeRun.transport },
    controls: {
      active: { ...runtimeRun.controls.active },
      pending: runtimeRun.controls.pending ? { ...runtimeRun.controls.pending } : null,
    },
    lastError: runtimeRun.lastError ? { ...runtimeRun.lastError } : null,
    checkpoints: runtimeRun.checkpoints.map((checkpoint) => ({ ...checkpoint })),
  }
}

function createAdapter(options?: {
  projects?: ListProjectsResponseDto['projects']
  snapshot?: ProjectSnapshotResponseDto
  status?: RepositoryStatusResponseDto
  diff?: RepositoryDiffResponseDto
  runtimeSession?: RuntimeSessionDto
  runtimeSettings?: RuntimeSettingsDto
  providerProfiles?: ProviderProfilesDto
  runtimeRun?: RuntimeRunDto | null
  autonomousState?: AutonomousRunStateDto | null
  notificationRoutes?: ListNotificationRoutesResponseDto['routes']
  projectFiles?: ListProjectFilesResponseDto
  pickedRepositoryPath?: string | null
}) {
  let currentSnapshot = options?.snapshot ?? makeSnapshot()
  let currentStatus = options?.status ?? makeStatus()
  let currentDiff = options?.diff ?? makeDiff()
  let currentRuntimeSession = options?.runtimeSession ?? makeRuntimeSession()
  let currentRuntimeSettings = options?.runtimeSettings ?? makeRuntimeSettings()
  let currentProviderProfiles = options?.providerProfiles ?? makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
  let currentProviderModelCatalogs: Record<string, ProviderModelCatalogDto> = Object.fromEntries(
    currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
  )
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentAutonomousState = options?.autonomousState ?? null
  let currentNotificationRoutes = options?.notificationRoutes ?? []
  let currentProjects = options?.projects ?? [makeProjectSummary('project-1', 'Cadence')]
  let currentProjectFiles = options?.projectFiles ?? makeProjectFiles()
  const pickedRepositoryPath = options?.pickedRepositoryPath ?? null
  const currentFileContents: Record<string, string> = {
    '/README.md': '# Cadence\n',
    '/src/App.tsx': 'export default function App() {\n  return <main>Cadence</main>\n}\n',
  }
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  const streamSubscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: CadenceDesktopError) => void) | null
    unsubscribe: () => void
  }> = []

  const rebuildProviderModelCatalogs = () => {
    currentProviderModelCatalogs = Object.fromEntries(
      currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
    )
  }

  const syncRuntimeSettingsFromActiveProfile = () => {
    const activeProfile =
      currentProviderProfiles.profiles.find((profile) => profile.profileId === currentProviderProfiles.activeProfileId) ??
      currentProviderProfiles.profiles[0] ??
      null

    if (!activeProfile) {
      return
    }

    currentRuntimeSettings = {
      providerId: activeProfile.providerId,
      modelId: activeProfile.modelId,
      openrouterApiKeyConfigured: activeProfile.providerId === 'openrouter' ? activeProfile.readiness.ready : false,
      anthropicApiKeyConfigured: activeProfile.providerId === 'anthropic' ? activeProfile.readiness.ready : false,
    }
  }

  const cloneRuntimeRunActiveControls = (runtimeRun: RuntimeRunDto) => ({
    ...runtimeRun.controls.active,
  })

  const cloneRuntimeRunPendingControls = (runtimeRun: RuntimeRunDto) =>
    runtimeRun.controls.pending ? { ...runtimeRun.controls.pending } : null

  const getRuntimeKindForProvider = (providerId: RuntimeSettingsDto['providerId']) =>
    providerId === 'openrouter' || providerId === 'anthropic' ? providerId : 'openai_codex'

  const buildRuntimeRunControls = (options: {
    base: RuntimeRunDto['controls']['active']
    nextControls?: RuntimeRunControlInputDto | null
    revision: number
    appliedAt?: string
    queuedAt?: string
    queuedPrompt?: string | null
    queuedPromptAt?: string | null
  }) => {
    const modelId = options.nextControls?.modelId ?? options.base.modelId
    const thinkingEffort = options.nextControls?.thinkingEffort ?? options.base.thinkingEffort
    const approvalMode = options.nextControls?.approvalMode ?? options.base.approvalMode

    return {
      active: {
        modelId,
        thinkingEffort,
        approvalMode,
        revision: options.revision,
        appliedAt: options.appliedAt ?? options.base.appliedAt,
      },
      pending:
        options.queuedAt || options.queuedPrompt != null
          ? {
              modelId,
              thinkingEffort,
              approvalMode,
              revision: options.revision,
              queuedAt: options.queuedAt ?? options.appliedAt ?? options.base.appliedAt,
              queuedPrompt: options.queuedPrompt ?? null,
              queuedPromptAt: options.queuedPromptAt ?? null,
            }
          : null,
    }
  }

  const mergePendingRuntimeRunControls = (runtimeRun: RuntimeRunDto, request?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null }) => {
    const activeControls = cloneRuntimeRunActiveControls(runtimeRun)
    const existingPending = cloneRuntimeRunPendingControls(runtimeRun)
    const nextRevision = existingPending?.revision ?? activeControls.revision + 1
    const nextQueuedAt = existingPending?.queuedAt ?? '2026-04-22T12:05:00Z'
    const nextQueuedPrompt = request?.prompt ?? existingPending?.queuedPrompt ?? null
    const nextQueuedPromptAt = request?.prompt ? '2026-04-22T12:05:30Z' : existingPending?.queuedPromptAt ?? null

    return buildRuntimeRunControls({
      base: activeControls,
      nextControls: request?.controls ?? existingPending,
      revision: nextRevision,
      queuedAt: nextQueuedAt,
      queuedPrompt: nextQueuedPrompt,
      queuedPromptAt: nextQueuedPromptAt,
    })
  }

  const queuePendingRuntimeRunSnapshot = (request?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null }) => {
    currentRuntimeRun = currentRuntimeRun
      ? makeRuntimeRun('project-1', {
          ...currentRuntimeRun,
          runtimeKind: getRuntimeKindForProvider(currentRuntimeSettings.providerId),
          providerId: currentRuntimeSettings.providerId,
          controls: mergePendingRuntimeRunControls(currentRuntimeRun, request),
          lastHeartbeatAt: '2026-04-22T12:05:30Z',
          updatedAt: '2026-04-22T12:05:30Z',
        })
      : makeRuntimeRun('project-1')

    return currentRuntimeRun
  }

  const startRuntimeRunSnapshot = (options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null }) => {
    const activeControls = buildRuntimeRunControls({
      base: makeRuntimeRun('project-1').controls.active,
      nextControls: {
        modelId: options?.initialControls?.modelId ?? currentRuntimeSettings.modelId,
        thinkingEffort: options?.initialControls?.thinkingEffort ?? 'medium',
        approvalMode: options?.initialControls?.approvalMode ?? 'suggest',
      },
      revision: 1,
      appliedAt: '2026-04-22T12:00:00Z',
      queuedAt: options?.initialPrompt ? '2026-04-22T12:00:30Z' : undefined,
      queuedPrompt: options?.initialPrompt ?? null,
      queuedPromptAt: options?.initialPrompt ? '2026-04-22T12:00:30Z' : null,
    })

    currentRuntimeRun = makeRuntimeRun('project-1', {
      runtimeKind: getRuntimeKindForProvider(currentRuntimeSettings.providerId),
      providerId: currentRuntimeSettings.providerId,
      controls: activeControls,
      startedAt: '2026-04-22T12:00:00Z',
      lastHeartbeatAt: '2026-04-22T12:00:05Z',
      lastCheckpointAt: '2026-04-22T12:00:06Z',
      updatedAt: '2026-04-22T12:00:06Z',
    })

    return currentRuntimeRun
  }

  const getKnownProjectIds = () => {
    const projectIds = new Set<string>(currentProjects.map((project) => project.id))
    projectIds.add(currentSnapshot.project.id)
    projectIds.add(currentRuntimeSession.projectId)
    if (currentRuntimeRun) {
      projectIds.add(currentRuntimeRun.projectId)
    }
    if (currentAutonomousState?.run) {
      projectIds.add(currentAutonomousState.run.projectId)
    }
    if (currentAutonomousState?.unit) {
      projectIds.add(currentAutonomousState.unit.projectId)
    }
    return Array.from(projectIds)
  }

  const applyProjectUpdatedPayload = (payload: ProjectUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.project.id, getKnownProjectIds(), 'emitProjectUpdated')
    currentProjects = currentProjects.map((project) =>
      project.id === payload.project.id ? payload.project : project,
    )

    if (currentSnapshot.project.id === payload.project.id) {
      const currentRepository =
        currentSnapshot.repository ?? {
          id: `repo-${payload.project.id}`,
          projectId: payload.project.id,
          rootPath: `/tmp/${payload.project.name}`,
          displayName: payload.project.name,
          branch: null,
          headSha: null,
          isGitRepo: true,
        }

      currentSnapshot = {
        ...currentSnapshot,
        project: payload.project,
        repository: {
          ...currentRepository,
          projectId: payload.project.id,
          displayName: payload.project.name,
        },
      }
    }
  }

  const applyRuntimeUpdatedPayload = (payload: RuntimeUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.projectId, getKnownProjectIds(), 'emitRuntimeUpdated')
    currentRuntimeSession = makeRuntimeSession(payload.projectId, {
      runtimeKind: payload.runtimeKind,
      providerId: payload.providerId,
      flowId: payload.flowId,
      sessionId: payload.sessionId,
      accountId: payload.accountId,
      phase: payload.authPhase,
      callbackBound: currentRuntimeSession.callbackBound,
      authorizationUrl: currentRuntimeSession.authorizationUrl,
      redirectUri: currentRuntimeSession.redirectUri,
      lastErrorCode: payload.lastErrorCode,
      lastError: payload.lastError,
      updatedAt: payload.updatedAt,
    })
  }

  const applyRuntimeRunUpdatedPayload = (payload: RuntimeRunUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.projectId, getKnownProjectIds(), 'emitRuntimeRunUpdated')
    ensureCompatibleRuntimeRun(payload.projectId, currentRuntimeRun, payload.run, 'emitRuntimeRunUpdated')
    currentRuntimeRun = payload.run ? cloneRuntimeRun(payload.run) : null
  }

  const upsertRuntimeSettings = vi.fn(async (request: UpsertRuntimeSettingsRequestDto) => {
    currentRuntimeSettings = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured:
        request.providerId === 'openrouter'
          ? request.openrouterApiKey == null
            ? currentRuntimeSettings.openrouterApiKeyConfigured
            : request.openrouterApiKey.trim().length > 0
          : false,
      anthropicApiKeyConfigured:
        request.providerId === 'anthropic'
          ? request.anthropicApiKey == null
            ? currentRuntimeSettings.anthropicApiKeyConfigured
            : request.anthropicApiKey.trim().length > 0
          : false,
    }
    currentProviderProfiles = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
    rebuildProviderModelCatalogs()
    return currentRuntimeSettings
  })

  const upsertProviderProfile = vi.fn(async (request: {
    profileId: string
    providerId: 'openai_codex' | 'openrouter' | 'anthropic'
    label: string
    modelId: string
    openrouterApiKey?: string | null
    anthropicApiKey?: string | null
    activate?: boolean
  }) => {
    currentRuntimeSettings = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured:
        request.providerId === 'openrouter'
          ? request.openrouterApiKey == null
            ? currentRuntimeSettings.openrouterApiKeyConfigured
            : request.openrouterApiKey.trim().length > 0
          : false,
      anthropicApiKeyConfigured:
        request.providerId === 'anthropic'
          ? request.anthropicApiKey == null
            ? currentRuntimeSettings.anthropicApiKeyConfigured
            : request.anthropicApiKey.trim().length > 0
          : false,
    }
    currentProviderProfiles = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
    rebuildProviderModelCatalogs()
    return currentProviderProfiles
  })

  const setActiveProviderProfile = vi.fn(async (profileId: string) => {
    const nextProfiles = currentProviderProfiles.profiles.map((profile) => ({
      ...profile,
      active: profile.profileId === profileId,
    }))
    currentProviderProfiles = {
      ...currentProviderProfiles,
      activeProfileId: profileId,
      profiles: nextProfiles,
    }
    syncRuntimeSettingsFromActiveProfile()
    rebuildProviderModelCatalogs()
    return currentProviderProfiles
  })

  const upsertNotificationRoute = vi.fn(async (request: UpsertNotificationRouteRequestDto) => {
    const route = {
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      routeTarget: request.routeTarget,
      enabled: request.enabled,
      metadataJson: request.metadataJson ?? null,
      credentialReadiness: null,
      createdAt: '2026-04-16T12:59:00Z',
      updatedAt: request.updatedAt,
    }

    currentNotificationRoutes = [
      ...currentNotificationRoutes.filter((item) => item.routeId !== route.routeId),
      route,
    ]

    return { route }
  })

  const startRuntimeRun = vi.fn(async (_projectId: string, options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null }) =>
    startRuntimeRunSnapshot(options),
  )

  const updateRuntimeRunControls = vi.fn(async (request?: {
    projectId: string
    runId: string
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => queuePendingRuntimeRunSnapshot(request))

  const startAutonomousRun = vi.fn(async () => {
    currentAutonomousState = makeAutonomousRunState('project-1')
    return currentAutonomousState
  })

  const pickRepositoryFolder = vi.fn(async () => pickedRepositoryPath)
  const importRepository = vi.fn(async (_path: string): Promise<ImportRepositoryResponseDto> => {
    const project = makeProjectSummary('project-1', 'Cadence')
    currentProjects = [project]
    return {
      project,
      repository: makeStatus().repository,
    }
  })
  const onProjectUpdated = vi.fn(
    async (
      handler: (payload: ProjectUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      projectUpdatedHandler = handler
      projectUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeUpdated = vi.fn(
    async (
      handler: (payload: RuntimeUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeUpdatedHandler = handler
      runtimeUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeRunUpdated = vi.fn(
    async (
      handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeRunUpdatedHandler = handler
      runtimeRunUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    importRepository,
    listProjects: async () => ({ projects: currentProjects }),
    removeProject: async (projectId) => {
      currentProjects = currentProjects.filter((project) => project.id !== projectId)
      return { projects: currentProjects }
    },
    getProjectSnapshot: async () => currentSnapshot,
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async (_projectId, scope) => ({ ...currentDiff, scope }),
    listProjectFiles: async () => currentProjectFiles,
    readProjectFile: async (projectId, path) => ({
      projectId,
      path,
      content: currentFileContents[path] ?? '',
    }),
    writeProjectFile: async (projectId, path, content) => {
      currentFileContents[path] = content
      return { projectId, path }
    },
    createProjectEntry: async (request) => {
      currentFileContents[request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`] = ''
      return {
        projectId: request.projectId,
        path: request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`,
      }
    },
    renameProjectEntry: async (request) => ({
      projectId: request.projectId,
      path: request.path.split('/').slice(0, -1).filter(Boolean).length
        ? `/${request.path.split('/').slice(0, -1).filter(Boolean).join('/')}/${request.newName}`
        : `/${request.newName}`,
    }),
    deleteProjectEntry: async (projectId, path) => ({ projectId, path }),
    searchProject: async (request) => ({
      projectId: request.projectId,
      totalMatches: 0,
      totalFiles: 0,
      truncated: false,
      files: [],
    }),
    replaceInProject: async (request) => ({
      projectId: request.projectId,
      filesChanged: 0,
      totalReplacements: 0,
    }),
    getAutonomousRun: async () => currentAutonomousState ?? { run: null, unit: null },
    getRuntimeRun: async () => currentRuntimeRun,
    getRuntimeSettings: async () => currentRuntimeSettings,
    getProviderModelCatalog: async (profileId, options) => {
      const currentProfile = currentProviderProfiles.profiles.find((profile) => profile.profileId === profileId)
      if (!currentProfile) {
        throw new Error(`Missing provider profile ${profileId}`)
      }

      const currentCatalog = currentProviderModelCatalogs[profileId]
      if (!options?.forceRefresh && currentCatalog) {
        return currentCatalog
      }

      const nextCatalog = buildProviderModelCatalog(currentProfile)
      currentProviderModelCatalogs[profileId] = nextCatalog
      return nextCatalog
    },
    getProviderProfiles: async () => currentProviderProfiles,
    getRuntimeSession: async () => currentRuntimeSession,
    startOpenAiLogin: async (_projectId, _options) => {
      currentRuntimeSession = makeRuntimeSession('project-1', {
        phase: 'awaiting_browser_callback',
        flowId: 'flow-1',
      })
      return currentRuntimeSession
    },
    submitOpenAiCallback: async (_projectId, _flowId, _options) => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    startAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    startRuntimeSession: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    stopRuntimeRun: async (_projectId, runId) => {
      currentRuntimeRun = makeRuntimeRun('project-1', {
        runId,
        status: 'stopped',
        stoppedAt: '2026-04-15T20:10:00Z',
      })
      return currentRuntimeRun
    },
    cancelAutonomousRun: async (_projectId, runId) => {
      currentAutonomousState = {
        run: {
          ...makeAutonomousRunState('project-1', runId).run!,
          status: 'cancelled',
          recoveryState: 'terminal',
          cancelledAt: '2026-04-16T20:10:00Z',
          cancelReason: {
            code: 'operator_cancelled',
            message: 'Operator cancelled the autonomous run from the desktop shell.',
          },
          updatedAt: '2026-04-16T20:10:00Z',
        },
        unit: {
          ...makeAutonomousRunState('project-1', runId).unit!,
          status: 'cancelled',
          finishedAt: '2026-04-16T20:10:00Z',
          updatedAt: '2026-04-16T20:10:00Z',
        },
      }
      return currentAutonomousState
    },
    logoutRuntimeSession: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      })
      return currentRuntimeSession
    },
    resolveOperatorAction: async () => {
      throw new Error('not used')
    },
    resumeOperatorRun: async () => {
      throw new Error('not used')
    },
    listNotificationRoutes: async () => ({ routes: currentNotificationRoutes }),
    listNotificationDispatches: async (): Promise<ListNotificationDispatchesResponseDto> => ({ dispatches: [] }),
    upsertNotificationRoute,
    upsertNotificationRouteCredentials: async (request) => ({
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      credentialScope: 'app_local',
      hasBotToken: Boolean(request.credentials.botToken),
      hasChatId: Boolean(request.credentials.chatId),
      hasWebhookUrl: Boolean(request.credentials.webhookUrl),
      updatedAt: request.updatedAt,
    }),
    recordNotificationDispatchOutcome: async (request) => ({ dispatch: request as never }),
    submitNotificationReply: async (request) => ({
      claim: {
        id: 1,
        projectId: request.projectId,
        actionId: request.actionId,
        routeId: request.routeId,
        correlationKey: request.correlationKey,
        responderId: request.responderId ?? null,
        status: request.decision === 'approve' ? 'accepted' : 'rejected',
        rejectionCode: request.decision === 'approve' ? null : 'notification_reply_rejected',
        rejectionMessage: request.decision === 'approve' ? null : 'Operator rejected the notification reply.',
        createdAt: request.receivedAt,
      },
      dispatch: {
        id: 1,
        projectId: request.projectId,
        actionId: request.actionId,
        routeId: request.routeId,
        correlationKey: request.correlationKey,
        status: request.decision === 'approve' ? 'claimed' : 'failed',
        attemptCount: 1,
        lastAttemptAt: request.receivedAt,
        deliveredAt: request.decision === 'approve' ? request.receivedAt : null,
        claimedAt: request.decision === 'approve' ? request.receivedAt : null,
        lastErrorCode: request.decision === 'approve' ? null : 'notification_reply_rejected',
        lastErrorMessage: request.decision === 'approve' ? null : 'Operator rejected the notification reply.',
        createdAt: request.receivedAt,
        updatedAt: request.receivedAt,
      },
      resolveResult: {
        approvalRequest: {
          actionId: request.actionId,
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the pending repository diff before continuing.',
          status: request.decision === 'approve' ? 'approved' : 'rejected',
          decisionNote: null,
          createdAt: request.receivedAt,
          updatedAt: request.receivedAt,
          resolvedAt: request.receivedAt,
        },
        verificationRecord: {
          id: 1,
          sourceActionId: request.actionId,
          status: request.decision === 'approve' ? 'passed' : 'failed',
          summary: request.decision === 'approve' ? 'Approved operator action.' : 'Rejected operator action.',
          detail: null,
          recordedAt: request.receivedAt,
        },
      },
      resumeResult: null,
    }),
    syncNotificationAdapters: async (_projectId): Promise<SyncNotificationAdaptersResponseDto> => ({
      projectId: 'project-1',
      dispatch: {
        projectId: 'project-1',
        pendingCount: 0,
        attemptedCount: 0,
        sentCount: 0,
        failedCount: 0,
        attemptLimit: 64,
        attemptsTruncated: false,
        attempts: [],
        errorCodeCounts: [],
      },
      replies: {
        projectId: 'project-1',
        routeCount: 0,
        polledRouteCount: 0,
        messageCount: 0,
        acceptedCount: 0,
        rejectedCount: 0,
        attemptLimit: 256,
        attemptsTruncated: false,
        attempts: [],
        errorCodeCounts: [],
      },
      syncedAt: '2026-04-16T13:00:00Z',
    }),
    upsertWorkflowGraph: async (request): Promise<UpsertWorkflowGraphResponseDto> => ({
      nodes: request.nodes,
      edges: request.edges,
      gates: request.gates,
      phases: [],
    }),
    applyWorkflowTransition: async (request): Promise<ApplyWorkflowTransitionResponseDto> => ({
      transitionEvent: {
        id: 1,
        transitionId: request.transitionId,
        causalTransitionId: request.causalTransitionId ?? null,
        fromNodeId: request.fromNodeId,
        toNodeId: request.toNodeId,
        transitionKind: request.transitionKind,
        gateDecision: request.gateDecision,
        gateDecisionContext: request.gateDecisionContext ?? null,
        createdAt: request.occurredAt,
      },
      automaticDispatch: undefined,
      phases: [],
    }),
    browserEval: async () => undefined,
    browserCurrentUrl: async () => null,
    browserScreenshot: async () => '',
    browserNavigate: async () => undefined,
    browserBack: async () => undefined,
    browserForward: async () => undefined,
    browserReload: async () => undefined,
    browserStop: async () => undefined,
    browserClick: async () => undefined,
    browserType: async () => undefined,
    browserScroll: async () => undefined,
    browserPressKey: async () => undefined,
    browserReadText: async () => undefined,
    browserQuery: async () => undefined,
    browserWaitForSelector: async () => undefined,
    browserWaitForLoad: async () => undefined,
    browserHistoryState: async () => undefined,
    browserCookiesGet: async () => undefined,
    browserCookiesSet: async () => undefined,
    browserStorageRead: async () => undefined,
    browserStorageWrite: async () => undefined,
    browserStorageClear: async () => undefined,
    browserTabList: async () => [],
    browserTabFocus: async () => ({
      id: 'tab-1',
      label: 'cadence-browser',
      title: null,
      url: null,
      loading: false,
      canGoBack: false,
      canGoForward: false,
      active: true,
    }),
    browserTabClose: async () => [],
    onBrowserUrlChanged: async () => () => undefined,
    onBrowserLoadState: async () => () => undefined,
    onBrowserConsole: async () => () => undefined,
    onBrowserTabUpdated: async () => () => undefined,
    subscribeRuntimeStream: async (
      projectId: string,
      itemKinds: RuntimeStreamEventDto['subscribedItemKinds'],
      handler: (payload: RuntimeStreamEventDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      const subscription = {
        projectId,
        handler,
        onError: onError ?? null,
        unsubscribe: () => {
          const index = streamSubscriptions.indexOf(subscription)
          if (index >= 0) {
            streamSubscriptions.splice(index, 1)
          }
        },
      }
      streamSubscriptions.push(subscription)

      return {
        response: {
          projectId,
          runtimeKind: 'openai_codex',
          runId: currentRuntimeRun?.runId ?? 'run-1',
          sessionId: currentRuntimeSession.sessionId ?? 'session-1',
          flowId: currentRuntimeSession.flowId ?? null,
          subscribedItemKinds: itemKinds,
        } satisfies SubscribeRuntimeStreamResponseDto,
        unsubscribe: subscription.unsubscribe,
      }
    },
    onProjectUpdated,
    onRepositoryStatusChanged: async (_handler: (payload: RepositoryStatusChangedPayloadDto) => void) => () => {},
    onRuntimeUpdated,
    onRuntimeRunUpdated,
  }

  return {
    adapter,
    streamSubscriptions,
    upsertNotificationRoute,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    importRepository,
    pickRepositoryFolder,
    startRuntimeRun,
    updateRuntimeRunControls,
    startAutonomousRun,
    onProjectUpdated,
    onRuntimeUpdated,
    onRuntimeRunUpdated,
    setSnapshot(snapshot: ProjectSnapshotResponseDto) {
      currentSnapshot = snapshot
    },
    setAutonomousState(state: AutonomousRunStateDto | null) {
      currentAutonomousState = state
    },
    emitProjectUpdated(payload: ProjectUpdatedPayloadDto) {
      applyProjectUpdatedPayload(payload)
      projectUpdatedHandler?.(payload)
    },
    emitProjectUpdatedError(error: CadenceDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      applyRuntimeUpdatedPayload(payload)
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeUpdatedError(error: CadenceDesktopError) {
      runtimeUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      applyRuntimeRunUpdatedPayload(payload)
      runtimeRunUpdatedHandler?.(payload)
    },
    emitRuntimeRunUpdatedError(error: CadenceDesktopError) {
      runtimeRunUpdatedErrorHandler?.(error)
    },
    emitRuntimeStream(index: number, payload: RuntimeStreamEventDto) {
      streamSubscriptions[index]?.handler(payload)
    },
    emitRuntimeStreamError(index: number, error: CadenceDesktopError) {
      streamSubscriptions[index]?.onError?.(error)
    },
  }
}

function getProviderCard(label: string): HTMLElement {
  const card = screen
    .getAllByText(label)
    .map((node) => node.closest('.rounded-lg'))
    .find((value): value is HTMLElement => value instanceof HTMLElement)

  if (!card) {
    throw new Error(`Could not find provider card for ${label}`)
  }

  return card
}

describe('CadenceApp current UI', () => {
  it('shows the onboarding flow on a cold-start empty state', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    expect(await screen.findByRole('heading', { name: /Welcome to Cadence/i })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Get started' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Skip setup' })).toBeVisible()
  })

  it('falls through to the legacy empty state when onboarding is dismissed', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Skip setup' }))

    expect(await screen.findByRole('heading', { name: 'Add your first project' })).toBeVisible()
    expect(screen.getAllByRole('button', { name: /Import repository/ }).length).toBeGreaterThanOrEqual(1)
  })

  it('reflects real provider settings in onboarding and disables unsupported providers', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))

    expect(await screen.findByRole('heading', { name: 'Configure providers' })).toBeVisible()
    expect(screen.getByText('Provider setup is app-wide. Choose the active profile for new runtime binds without rewriting project runtime history.')).toBeVisible()
    expect(screen.getByText('Active')).toBeVisible()
    expect(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Set up' })).toBeVisible()
    expect(screen.getAllByText('Unavailable')).toHaveLength(1)
  })

  it('keeps onboarding provider review truthful before OpenAI is connected', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.getByText('OpenAI Codex · active profile')).toBeVisible()
  })

  it('keeps onboarding provider review truthful for Anthropic API-key readiness', async () => {
    const { adapter: missingKeyAdapter } = createAdapter({
      projects: [],
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKeyConfigured: false,
      }),
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'anthropic',
        runtimeKind: 'anthropic',
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={missingKeyAdapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.getByText('Anthropic · API key required')).toBeVisible()
  })

  it('keeps onboarding provider review truthful when an Anthropic API key is already saved', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKeyConfigured: true,
      }),
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'anthropic',
        runtimeKind: 'anthropic',
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.getByText('Anthropic · API key saved')).toBeVisible()
  })

  it('saves OpenRouter provider settings from onboarding', async () => {
    const { adapter, upsertProviderProfile } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Set up' }))
    fireEvent.change(screen.getByLabelText('Model'), { target: { value: 'openai/gpt-4.1-mini' } })
    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-or-v1-test-secret' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(upsertProviderProfile).toHaveBeenCalledTimes(1))
    expect(upsertProviderProfile).toHaveBeenCalledWith({
      profileId: 'openrouter-default',
      providerId: 'openrouter',
      label: 'OpenRouter',
      modelId: 'openai/gpt-4.1-mini',
      openrouterApiKey: 'sk-or-v1-test-secret',
      activate: false,
    })
  })

  it('saves Anthropic provider settings from onboarding', async () => {
    const { adapter, upsertProviderProfile } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Set up' }))
    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-ant-test-secret' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(upsertProviderProfile).toHaveBeenCalledTimes(1))
    expect(upsertProviderProfile).toHaveBeenCalledWith({
      profileId: 'anthropic-default',
      providerId: 'anthropic',
      label: 'Anthropic',
      modelId: 'claude-3-7-sonnet-latest',
      anthropicApiKey: 'sk-ant-test-secret',
      activate: false,
    })
  })

  it('imports a project and creates a notification route from onboarding', async () => {
    const { adapter, pickRepositoryFolder, importRepository, upsertNotificationRoute } = createAdapter({
      projects: [],
      pickedRepositoryPath: '/tmp/Cadence',
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { name: 'Add a project' })).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: /Choose a folder/i }))

    await waitFor(() => expect(pickRepositoryFolder).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(importRepository).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByText('/tmp/Cadence')).toBeVisible())

    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click((await screen.findAllByRole('button', { name: 'Add route' }))[0])
    fireEvent.change(screen.getByPlaceholderText('Chat ID or @channel'), { target: { value: '@ops-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save route' }))

    await waitFor(() => expect(upsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(upsertNotificationRoute).toHaveBeenCalledWith(
      expect.objectContaining({
        routeId: 'telegram-primary',
        routeKind: 'telegram',
        routeTarget: 'telegram:@ops-room',
        enabled: true,
      }),
    )
  })

  it('renders the current workflow empty state for an imported project', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.getByText('No milestone assigned')).toBeVisible()
  })

  it('renders live git and runtime footer data from desktop state while leaving mock-only fields untouched', async () => {
    const runtimeSettings = makeRuntimeSettings({
      providerId: 'openrouter',
      modelId: 'openai/gpt-4.1-mini',
      openrouterApiKeyConfigured: true,
    })
    const providerProfiles = makeProviderProfilesFromRuntimeSettings(runtimeSettings)
    const { adapter } = createAdapter({
      status: {
        repository: {
          id: 'repo-project-1',
          projectId: 'project-1',
          rootPath: '/tmp/Cadence',
          displayName: 'Cadence',
          branch: 'feature/footer-live-data',
          headSha: '1234567890abcdef1234567890abcdef12345678',
          isGitRepo: true,
        },
        branch: {
          name: 'feature/footer-live-data',
          headSha: '1234567890abcdef1234567890abcdef12345678',
          detached: false,
        },
        lastCommit: {
          sha: '1234567890abcdef1234567890abcdef12345678',
          summary: 'fix: use live head commit metadata',
          committedAt: '2026-04-22T17:55:00Z',
        },
        entries: [
          {
            path: 'src/App.tsx',
            staged: 'modified',
            unstaged: null,
            untracked: false,
          },
          {
            path: 'src-tauri/src/main.rs',
            staged: null,
            unstaged: 'modified',
            untracked: false,
          },
        ],
        hasStagedChanges: true,
        hasUnstagedChanges: true,
        hasUntrackedChanges: false,
      },
      runtimeSettings,
      providerProfiles,
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
      }),
      runtimeRun: makeRuntimeRun('project-1', {
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
        status: 'running',
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.getByRole('contentinfo', { name: 'Status bar' })).toBeVisible()
    expect(screen.getByText('feature/footer-live-data')).toBeVisible()
    expect(screen.getByText('2 changes')).toBeVisible()
    expect(screen.getByText('1234567')).toBeVisible()
    expect(screen.getByText('fix: use live head commit metadata')).toBeVisible()
    expect(screen.getByText('OpenRouter')).toBeVisible()
    expect(screen.getByText('running')).toBeVisible()
  })

  it('collapses the project rail into a compact icon strip from the titlebar toggle', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    const collapseButton = screen.getByRole('button', { name: 'Collapse project sidebar' })
    fireEvent.click(collapseButton)

    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()
    expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull()
    expect(screen.queryByRole('button', { name: 'Project actions for cadence' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /cadence/i })).toBeVisible()
  })

  it('auto-collapses the project rail in Editor and restores it when leaving if it started expanded', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(document.querySelector('aside[data-collapsed="false"]')).not.toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Workflow' }))

    await waitFor(() => expect(document.querySelector('aside[data-collapsed="false"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Collapse project sidebar' })).toBeVisible()
  })

  it('keeps the project rail collapsed after leaving Editor when it was already collapsed', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Collapse project sidebar' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()
  })

  it('switches to Agent without rendering the removed debug panels', async () => {
    const { adapter } = createAdapter({ runtimeRun: null, autonomousState: null })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start run' })).toBeVisible()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
  })

  it('rehydrates the recovered runtime snapshot after reload without rendering the removed debug panels', async () => {
    const recoveredAutonomousState = makeAutonomousRunState('project-1', 'auto-run-1')
    recoveredAutonomousState.run = {
      ...recoveredAutonomousState.run!,
      recoveryState: 'recovery_required',
      activeUnitId: 'auto-run-1:checkpoint:2',
      duplicateStartDetected: true,
      duplicateStartRunId: 'auto-run-1',
      duplicateStartReason:
        'Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.',
      crashedAt: '2026-04-16T20:03:00Z',
      crashReason: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      lastErrorCode: 'runtime_supervisor_connect_failed',
      lastError: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      updatedAt: '2026-04-16T20:03:00Z',
    }
    recoveredAutonomousState.unit = {
      ...recoveredAutonomousState.unit!,
      unitId: 'auto-run-1:checkpoint:2',
      sequence: 2,
      summary: 'Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.',
      boundaryId: 'checkpoint:2',
      updatedAt: '2026-04-16T20:03:00Z',
    }

    const { adapter } = createAdapter({
      snapshot: {
        ...makeSnapshot(),
        autonomousRun: recoveredAutonomousState.run,
        autonomousUnit: recoveredAutonomousState.unit,
      },
      runtimeRun: makeRuntimeRun('project-1', {
        status: 'stale',
        transport: {
          kind: 'tcp',
          endpoint: '127.0.0.1:4455',
          liveness: 'unreachable',
        },
        lastCheckpointSequence: 2,
        checkpoints: [
          {
            sequence: 1,
            kind: 'bootstrap',
            summary: 'Supervisor boot recorded.',
            createdAt: '2026-04-15T20:00:01Z',
          },
          {
            sequence: 2,
            kind: 'state',
            summary: 'Recovered repository context before reconnecting the live feed.',
            createdAt: '2026-04-15T20:00:06Z',
          },
        ],
        lastErrorCode: 'runtime_supervisor_connect_failed',
        lastError: {
          code: 'runtime_supervisor_connect_failed',
          message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
        },
      }),
      autonomousState: recoveredAutonomousState,
    })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByRole('heading', { name: 'Recovered run snapshot' })).toBeVisible()
    expect(
      screen.queryByText('Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.'),
    ).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Inspect truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
  })

  it('refreshes active project metadata from project:updated events without rerendering the app root', async () => {
    const setup = createAdapter()

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )
    await waitFor(() => expect(setup.onProjectUpdated).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('button', { name: 'Project actions for Cadence' })).toBeVisible()

    act(() => {
      setup.emitProjectUpdated({
        project: makeProjectSummary('project-1', 'Cadence Prime'),
        reason: 'metadata_changed',
      })
    })

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Project actions for Cadence Prime' })).toBeVisible(),
    )
    expect(screen.queryByRole('button', { name: 'Project actions for Cadence' })).not.toBeInTheDocument()
  })

  it('refreshes provider auth UI from runtime:updated events without rerendering', async () => {
    const setup = createAdapter({
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Sign in with OpenAI to create a runtime session for this project.',
          retryable: false,
        },
      }),
      runtimeRun: null,
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeVisible()
    await waitFor(() => expect(setup.onRuntimeUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: 'flow-1',
        sessionId: 'session-1',
        accountId: 'acct-1',
        authPhase: 'authenticated',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-22T12:00:00Z',
      })
    })

    await waitFor(() => expect(screen.getByText('Connected')).toBeVisible())
    expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible()

    act(() => {
      setup.emitRuntimeUpdatedError(
        new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Event runtime:updated returned an unexpected payload shape.',
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByText('Event runtime:updated returned an unexpected payload shape.')).toBeVisible(),
    )
  })

  it('refreshes the Agent pane from runtime_run:updated events and rejects mismatched payloads', async () => {
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        lastErrorCode: null,
        lastError: null,
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    await waitFor(() => expect(setup.onRuntimeRunUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-1',
          startedAt: '2026-04-20T12:00:00Z',
          lastHeartbeatAt: '2026-04-20T12:00:05Z',
          lastCheckpointAt: '2026-04-20T12:00:06Z',
          updatedAt: '2026-04-20T12:00:06Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'suggest',
              revision: 1,
              appliedAt: '2026-04-20T12:00:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    expect((await screen.findAllByRole('heading', { name: 'Recovered run snapshot' })).length).toBeGreaterThan(0)
    expect(screen.getByText('Approval active · Suggest')).toBeVisible()

    const pendingRun = makeRuntimeRun('project-1', {
      runId: 'run-live-1',
      startedAt: '2026-04-20T12:00:00Z',
      lastHeartbeatAt: '2026-04-20T12:05:00Z',
      lastCheckpointSequence: 2,
      lastCheckpointAt: '2026-04-20T12:05:00Z',
      updatedAt: '2026-04-20T12:05:00Z',
      controls: {
        active: {
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          revision: 1,
          appliedAt: '2026-04-20T12:00:00Z',
        },
        pending: {
          modelId: 'anthropic/claude-3.5-haiku',
          thinkingEffort: 'low',
          approvalMode: 'yolo',
          revision: 2,
          queuedAt: '2026-04-20T12:05:00Z',
          queuedPrompt: 'Review the diff before continuing.',
          queuedPromptAt: '2026-04-20T12:05:00Z',
        },
      },
    })

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: pendingRun,
      })
    })

    await waitFor(() => expect(screen.getByText('Model pending · anthropic/claude-3.5-haiku')).toBeVisible())
    expect(screen.getByText('Thinking pending · Low')).toBeVisible()
    expect(screen.getByText('Approval pending · YOLO')).toBeVisible()
    expect(screen.getByText('Queued prompt pending the next model-call boundary.')).toBeVisible()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: pendingRun,
      })
    })

    expect(screen.getAllByText('Approval pending · YOLO')).toHaveLength(1)
    expect(() =>
      setup.emitRuntimeRunUpdated({
        projectId: 'project-2',
        run: makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      }),
    ).toThrowError(/expected one of \[project-1\]/)
    expect(() =>
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', { runId: 'run-live-2' }),
      }),
    ).toThrowError(/clear the active run before attaching `run-live-2`/)

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-1',
          startedAt: '2026-04-20T12:00:00Z',
          lastHeartbeatAt: '2026-04-20T12:06:00Z',
          lastCheckpointSequence: 3,
          lastCheckpointAt: '2026-04-20T12:06:00Z',
          updatedAt: '2026-04-20T12:06:00Z',
          controls: {
            active: {
              modelId: 'anthropic/claude-3.5-haiku',
              thinkingEffort: 'low',
              approvalMode: 'yolo',
              revision: 2,
              appliedAt: '2026-04-20T12:06:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByText('Approval active · YOLO')).toBeVisible())
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: null,
      })
    })

    await waitFor(() => expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible())

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-2',
          startedAt: '2026-04-20T12:07:00Z',
          lastHeartbeatAt: '2026-04-20T12:07:05Z',
          lastCheckpointSequence: 4,
          lastCheckpointAt: '2026-04-20T12:07:06Z',
          updatedAt: '2026-04-20T12:07:06Z',
          controls: {
            active: {
              modelId: 'anthropic/claude-3.5-haiku',
              thinkingEffort: 'low',
              approvalMode: 'yolo',
              revision: 2,
              appliedAt: '2026-04-20T12:07:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() =>
      expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThan(0),
    )
    expect(screen.getByText('Approval active · YOLO')).toBeVisible()

    act(() => {
      setup.emitRuntimeRunUpdatedError(
        new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Event runtime_run:updated returned an unexpected payload shape.',
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByText('Event runtime_run:updated returned an unexpected payload shape.')).toBeVisible(),
    )
  })

  it('proves auth, provider-backed model truth, and pending-to-active boundary application through the shipped Agent path', async () => {
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Sign in with OpenAI to create a runtime session for this project.',
          retryable: false,
        },
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('heading', { name: 'Providers' })).toBeVisible()
    expect(screen.getAllByText('OpenAI Codex').length).toBeGreaterThan(0)
    expect(screen.getByText('Active')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeVisible()
    await waitFor(() => expect(setup.onRuntimeUpdated).toHaveBeenCalledTimes(1))

    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }))

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: 'flow-1',
        sessionId: 'session-1',
        accountId: 'acct-1',
        authPhase: 'authenticated',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-22T12:00:00Z',
      })
    })

    await waitFor(() => expect(screen.getByText('Connected')).toBeVisible())
    fireEvent.click(screen.getByRole('button', { name: 'Close' }))

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('openai_codex')
    expect(screen.getByRole('combobox', { name: 'Thinking level selector' })).toHaveTextContent('Thinking · medium')
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toHaveTextContent('Approval · suggest')
    expect(screen.getAllByText('Live catalog').length).toBeGreaterThan(0)
    expect(screen.getByText(/Showing 1 discovered model/)).toBeVisible()
    expect(screen.getByText(/Thinking supports Low, Medium, High\. Default: Medium\./)).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))

    await waitFor(() =>
      expect(setup.startRuntimeRun).toHaveBeenCalledWith('project-1', {
        initialControls: {
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
        },
        initialPrompt: null,
      }),
    )
    await waitFor(() => expect(screen.getByText('Approval active · Suggest')).toBeVisible())

    fireEvent.keyDown(screen.getByRole('combobox', { name: 'Approval mode selector' }), { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'Approval · yolo' }))

    await waitFor(() =>
      expect(setup.updateRuntimeRunControls).toHaveBeenNthCalledWith(1, {
        projectId: 'project-1',
        runId: 'run-1',
        controls: {
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'yolo',
        },
        prompt: null,
      }),
    )
    await waitFor(() => expect(screen.getByText('Approval pending · YOLO')).toBeVisible())
    expect(screen.getByText(/Pending YOLO does not apply until the next model-call boundary\./)).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Review the diff before continuing.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.updateRuntimeRunControls).toHaveBeenNthCalledWith(2, {
        projectId: 'project-1',
        runId: 'run-1',
        controls: null,
        prompt: 'Review the diff before continuing.',
      }),
    )
    await waitFor(() => expect(screen.getByText('Queued prompt pending the next model-call boundary.')).toBeVisible())
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toHaveValue(''))

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:06:00Z',
          lastCheckpointSequence: 2,
          lastCheckpointAt: '2026-04-22T12:06:00Z',
          updatedAt: '2026-04-22T12:06:00Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'yolo',
              revision: 2,
              appliedAt: '2026-04-22T12:06:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByText('Approval active · YOLO')).toBeVisible())
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).not.toBeDisabled()
  })

  it('shows live review-required checkpoint truth only after YOLO becomes active on the shipped Agent surface', async () => {
    const reviewActionId = 'flow:flow-1:run:run-1:boundary:boundary-review-1:review_command'
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        flowId: 'flow-1',
        lastErrorCode: null,
        lastError: null,
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))
    await waitFor(() => expect(screen.getByText('Approval active · Suggest')).toBeVisible())
    await waitFor(() => expect(setup.streamSubscriptions.length).toBeGreaterThan(0))

    fireEvent.keyDown(screen.getByRole('combobox', { name: 'Approval mode selector' }), { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'Approval · yolo' }))

    await waitFor(() => expect(screen.getByText('Approval pending · YOLO')).toBeVisible())
    expect(screen.getByText(/Pending YOLO does not apply until the next model-call boundary\./)).toBeVisible()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:07:00Z',
          lastCheckpointSequence: 3,
          lastCheckpointAt: '2026-04-22T12:07:00Z',
          updatedAt: '2026-04-22T12:07:00Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'yolo',
              revision: 2,
              appliedAt: '2026-04-22T12:07:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByText('Approval active · YOLO')).toBeVisible())
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()

    setup.setSnapshot({
      ...makeSnapshot(),
      approvalRequests: [makeRuntimeApproval(reviewActionId)],
      resumeHistory: [],
      notificationDispatches: [],
      notificationReplyClaims: [],
    })

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeRuntimeStreamActionRequiredEvent({
          actionId: reviewActionId,
          boundaryId: 'boundary-review-1',
          detail: 'Cadence blocked a destructive shell wrapper and needs operator review before continuing.',
        }),
      )
    })

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible())
    const checkpointQueries = () => within(ensureCheckpointSection())

    await waitFor(() => expect(checkpointQueries().getByText('Live + durable')).toBeVisible())
    await waitFor(() => expect(checkpointQueries().getByText('Pending approval')).toBeVisible())
    expect(checkpointQueries().getByText('Review destructive shell command')).toBeVisible()
    expect(checkpointQueries().getByText('Live action required')).toBeVisible()
    expect(
      checkpointQueries().getAllByText('Waiting for operator input before this action can resume the run.').length,
    ).toBeGreaterThan(0)
    expect(
      checkpointQueries().getByText(
        `Action ${reviewActionId} · Boundary boundary-review-1`,
      ),
    ).toBeVisible()
    expect(checkpointQueries().getAllByText(/Cadence blocked a destructive shell wrapper/).length).toBeGreaterThan(0)
  })

  it('keeps recovered durable policy denials understandable on the shipped Agent surface after the live row clears', async () => {
    const deniedActionId = 'flow:flow-1:run:run-1:boundary:boundary-denied-1:review_command'
    const setup = createAdapter({
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        flowId: 'flow-1',
        lastErrorCode: null,
        lastError: null,
      }),
      runtimeRun: makeRuntimeRun('project-1', {
        runId: 'run-1',
        controls: {
          active: {
            modelId: 'openai_codex',
            thinkingEffort: 'medium',
            approvalMode: 'yolo',
            revision: 2,
            appliedAt: '2026-04-22T12:07:00Z',
          },
          pending: null,
        },
        updatedAt: '2026-04-22T12:08:00Z',
      }),
      autonomousState: makeRecoveredPolicyDeniedAutonomousState('project-1', {
        actionId: deniedActionId,
        boundaryId: 'boundary-denied-1',
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    await waitFor(() => expect(screen.getByText('Approval active · YOLO')).toBeVisible())
    await waitFor(() => expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible())

    const checkpointQueries = within(ensureCheckpointSection())
    expect(checkpointQueries.getByText('Recovered durable denial')).toBeVisible()
    expect(checkpointQueries.getAllByText('Policy denied').length).toBeGreaterThan(0)
    expect(checkpointQueries.getAllByText('Not resumable').length).toBeGreaterThan(0)
    expect(checkpointQueries.getByText('No live review row')).toBeVisible()
    expect(
      checkpointQueries.getAllByText(
        'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      ).length,
    ).toBeGreaterThan(0)
    expect(
      checkpointQueries.getByText(
        `Action ${deniedActionId} · Boundary boundary-denied-1`,
      ),
    ).toBeVisible()
    expect(checkpointQueries.queryByRole('button', { name: 'Approve' })).not.toBeInTheDocument()
    expect(checkpointQueries.queryByRole('button', { name: 'Resume run' })).not.toBeInTheDocument()
  })

  it('opens Settings and runs the current provider and notification flows', async () => {
    const { adapter, upsertNotificationRoute } = createAdapter({
      runtimeRun: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Sign in with OpenAI to create a runtime session for this project.',
          retryable: false,
        },
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('heading', { name: 'Providers' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }))

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))
    expect(await screen.findByText('Telegram')).toBeVisible()

    fireEvent.click(screen.getAllByRole('button', { name: 'Add route' })[0])
    fireEvent.change(screen.getByLabelText('Route name'), { target: { value: 'ops-alerts' } })
    fireEvent.change(screen.getByLabelText('Target'), { target: { value: '@ops-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create route' }))

    await waitFor(() => expect(upsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(upsertNotificationRoute.mock.calls[0][0]).toMatchObject({
      routeId: 'ops-alerts',
      routeKind: 'telegram',
      routeTarget: 'telegram:@ops-room',
      enabled: true,
    })
  })

  it('switches to Editor and loads the selected project files', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    expect(await screen.findByText('README.md')).toBeVisible()
    expect(screen.getByText('Explorer')).toBeVisible()
    expect(screen.getByLabelText('Search files')).toBeVisible()
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
    expect(screen.queryByText('No execution activity yet')).not.toBeInTheDocument()
  })

  it('keeps open editor tabs and unsaved edits when switching away from Editor and back', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    fireEvent.click(await screen.findByRole('button', { name: 'README.md' }))

    const editor = await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(editor, { target: { value: '# Draft changes\n' } })

    fireEvent.click(screen.getByRole('button', { name: 'Workflow' }))
    expect(await screen.findByText('No milestone assigned')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    const restoredEditor = await screen.findByLabelText('Editor for /README.md')
    expect(restoredEditor).toBeVisible()
    expect(restoredEditor).toHaveValue('# Draft changes\n')
    expect(screen.getByRole('button', { name: 'Close README.md' })).toBeVisible()
  })
})
