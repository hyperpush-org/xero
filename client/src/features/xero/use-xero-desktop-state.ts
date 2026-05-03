import { useCallback, useEffect, useMemo, useRef, useState, type SetStateAction } from 'react'
import {
  XeroDesktopError,
  XeroDesktopAdapter,
  getDesktopErrorMessage,
} from '@/src/lib/xero-desktop'
import {
  applyRuntimeRun,
  applyRuntimeSession,
  applyRepositoryStatus,
  estimateProviderModelCatalogBytes,
  estimateRuntimeStreamViewBytes,
  mapAutonomousRunInspection,
  mapProjectSnapshot,
  mapProjectSummary,
  mapRepositoryDiff,
  mapRepositoryStatus,
  mapAgentSession,
  mapRuntimeRun,
  mapRuntimeSession,
  selectAgentSessionId,
  type XeroDoctorReportDto,
  type McpImportDiagnosticDto,
  type McpRegistryDto,
  type NotificationDispatchDto,
  type NotificationRouteDto,
  type AgentSessionView,
  type Phase,
  type ProjectDetailView,
  type ProjectListItem,
  type ProjectUsageSummaryDto,
  type ProviderModelCatalogDto,
  type ProviderCredentialsSnapshotDto,
  type ProviderProfileDiagnosticsDto,
  type RepositoryDiffScope,
  type RepositoryStatusView,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RunDoctorReportRequestDto,
  type RuntimeStreamView,
  type SkillRegistryDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/xero-model'
import { mapNotificationBroker } from '@/src/lib/xero-model/notifications'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'

import {
  type BlockedNotificationSyncPollTarget,
} from './use-xero-desktop-state/notification-health'
import { useXeroDesktopMutations } from './use-xero-desktop-state/mutations'
import {
  applyAutonomousRunState,
  applyRuntimeToProjectList,
  loadNotificationRoutesForProject,
  loadProjectState,
  removeProjectRecord,
  type ProjectLoadSource,
} from './use-xero-desktop-state/project-loaders'
import {
  createRepositoryStatusDiffRevision,
  createRepositoryStatusSyncKey,
} from './use-xero-desktop-state/repository-status'
import {
  attachDesktopRuntimeListeners,
  attachRuntimeStreamSubscription,
  clearRuntimeMetadataRefresh,
  scheduleRuntimeMetadataRefresh as scheduleRuntimeMetadataRefreshHelper,
  type RuntimeMetadataRefreshSource,
} from './use-xero-desktop-state/runtime-stream'
import { trimRecordCacheToByteBudget } from './use-xero-desktop-state/memory-budget'
import {
  buildAgentView,
  buildExecutionView,
  buildWorkflowView,
} from './use-xero-desktop-state/view-builders'
import {
  createXeroHighChurnStore,
  createRuntimeStreamStoreKey,
  removeRuntimeStreamForSession,
  removeRuntimeStreamsForProject,
  selectRepositoryStatus,
  selectRuntimeStreams,
  useSelectorStoreValue,
  type XeroHighChurnStore,
} from './use-xero-desktop-state/high-churn-store'
import type {
  AgentTrustSnapshotView,
  AgentWorkspaceLayoutState,
  AgentWorkspacePaneSlot,
  AgentWorkspacePaneView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DoctorReportRunStatus,
  ExecutionPaneView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadResult,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProjectRemovalStatus,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
  ProviderModelCatalogLoadStatus,
  RefreshSource,
  RepositoryDiffState,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
  UseXeroDesktopStateOptions,
  UseXeroDesktopStateResult,
  WorkflowPaneView,
} from './use-xero-desktop-state/types'

export type {
  AgentPaneView,
  AgentWorkspaceLayoutState,
  AgentWorkspacePaneSlot,
  AgentWorkspacePaneView,
  AgentProviderModelView,
  AgentTrustSignalState,
  AgentTrustSnapshotView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DoctorReportRunStatus,
  DiffScopeSummary,
  ExecutionPaneView,
  NotificationChannelHealthView,
  NotificationRouteHealthState,
  NotificationRouteHealthView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadStatus,
  OperatorActionDecision,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProjectRemovalStatus,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
  ProviderModelCatalogLoadStatus,
  RefreshSource,
  RepositoryDiffLoadStatus,
  RepositoryDiffState,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  McpRegistryLoadStatus,
  McpRegistryMutationStatus,
  SkillRegistryLoadStatus,
  SkillRegistryMutationStatus,
  UseXeroDesktopStateOptions,
  UseXeroDesktopStateResult,
  WorkflowPaneView,
} from './use-xero-desktop-state/types'
export { BLOCKED_NOTIFICATION_SYNC_POLL_MS } from './use-xero-desktop-state/notification-health'
export {
  selectRepositoryShellStatus,
  selectRuntimeStreamForProject,
  shallowEqualObject,
  useSelectorStoreValue as useXeroHighChurnStoreValue,
  type RepositoryShellStatus,
  type XeroHighChurnStore,
} from './use-xero-desktop-state/high-churn-store'

const REPOSITORY_STATUS_POLL_MS = 5_000
const PREFETCH_FALLBACK_DELAY_MS = 250
const RUNTIME_STREAM_SESSION_CACHE_MAX_BYTES = 3 * 1024 * 1024
const RUNTIME_STREAM_SESSION_CACHE_MAX_ENTRIES = 24
const PROVIDER_MODEL_CATALOG_CACHE_MAX_BYTES = 2 * 1024 * 1024
const PROVIDER_MODEL_CATALOG_CACHE_MAX_ENTRIES = 24
const AGENT_WORKSPACE_LAYOUT_STORAGE_KEY = 'agentWorkspaceLayout'
const AGENT_WORKSPACE_LAYOUT_PERSIST_DEBOUNCE_MS = 250
const AGENT_WORKSPACE_MAX_PANES = 6

let agentWorkspacePaneIdSequence = 0

type IdleWindow = Window & {
  requestIdleCallback?: (callback: () => void, options?: { timeout?: number }) => number
  cancelIdleCallback?: (handle: number) => void
}

type DeferredTaskHandle = {
  kind: 'idle' | 'timeout'
  handle: number
}

function waitForProjectSelectionPaint(): Promise<void> {
  if (typeof window === 'undefined') {
    return Promise.resolve()
  }

  return new Promise((resolve) => {
    const finishAfterFrame = () => window.setTimeout(resolve, 0)
    if (typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(finishAfterFrame)
      return
    }

    finishAfterFrame()
  })
}

function scheduleDeferredTask(
  win: IdleWindow,
  callback: () => void,
  options: { timeoutMs: number },
): DeferredTaskHandle {
  if (win.requestIdleCallback) {
    return {
      kind: 'idle',
      handle: win.requestIdleCallback(callback, { timeout: options.timeoutMs }),
    }
  }

  return {
    kind: 'timeout',
    handle: win.setTimeout(callback, PREFETCH_FALLBACK_DELAY_MS),
  }
}

function cancelDeferredTask(win: IdleWindow, task: DeferredTaskHandle): void {
  if (task.kind === 'idle') {
    win.cancelIdleCallback?.(task.handle)
    return
  }

  win.clearTimeout(task.handle)
}

function createProjectShell(project: ProjectListItem): ProjectDetailView {
  return {
    ...project,
    phases: [],
    repository: null,
    repositoryStatus: null,
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [],
    selectedAgentSession: null,
    selectedAgentSessionId: selectAgentSessionId([]),
    notificationBroker: mapNotificationBroker(project.id, []),
    runtimeSession: null,
    runtimeRun: null,
    autonomousRun: null,
  }
}

function createAgentSessionStateKey(projectId: string, agentSessionId: string): string {
  return `${projectId}::${agentSessionId}`
}

function createDefaultAgentWorkspacePaneId(projectId: string): string {
  return `agent-pane-${projectId}`
}

function createAgentWorkspacePaneId(): string {
  agentWorkspacePaneIdSequence += 1
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `agent-pane-${crypto.randomUUID()}`
  }

  return `agent-pane-${Date.now().toString(36)}-${agentWorkspacePaneIdSequence.toString(36)}`
}

function readAgentWorkspaceLayoutStore(): Record<string, AgentWorkspaceLayoutState> {
  if (typeof window === 'undefined') {
    return {}
  }

  try {
    const raw = window.localStorage?.getItem?.(AGENT_WORKSPACE_LAYOUT_STORAGE_KEY)
    if (!raw) {
      return {}
    }

    const parsed = JSON.parse(raw) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {}
    }

    const layouts: Record<string, AgentWorkspaceLayoutState> = {}
    for (const [projectId, value] of Object.entries(parsed)) {
      const layout = sanitizeAgentWorkspaceLayout(value)
      if (layout) {
        layouts[projectId] = layout
      }
    }
    return layouts
  } catch {
    return {}
  }
}

function writeAgentWorkspaceLayoutStore(layouts: Record<string, AgentWorkspaceLayoutState>): void {
  if (typeof window === 'undefined') {
    return
  }

  try {
    window.localStorage?.setItem?.(AGENT_WORKSPACE_LAYOUT_STORAGE_KEY, JSON.stringify(layouts))
  } catch {
    // Layout persistence is best-effort app UI state.
  }
}

function sanitizeAgentWorkspaceLayout(value: unknown): AgentWorkspaceLayoutState | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null
  }

  const candidate = value as Partial<AgentWorkspaceLayoutState>
  const paneSlots: AgentWorkspacePaneSlot[] = []
  const seenPaneIds = new Set<string>()
  if (Array.isArray(candidate.paneSlots)) {
    for (const slot of candidate.paneSlots) {
      if (!slot || typeof slot !== 'object') {
        continue
      }
      const maybeSlot = slot as Partial<AgentWorkspacePaneSlot>
      const id = typeof maybeSlot.id === 'string' ? maybeSlot.id.trim() : ''
      const agentSessionId =
        maybeSlot.agentSessionId === null
          ? null
          : typeof maybeSlot.agentSessionId === 'string'
            ? maybeSlot.agentSessionId.trim()
            : ''
      if (!id || agentSessionId === '' || seenPaneIds.has(id)) {
        continue
      }

      seenPaneIds.add(id)
      paneSlots.push({ id, agentSessionId })
      if (paneSlots.length >= AGENT_WORKSPACE_MAX_PANES) {
        break
      }
    }
  }

  const focusedPaneId =
    typeof candidate.focusedPaneId === 'string' ? candidate.focusedPaneId.trim() : ''
  const splitterRatios: Record<string, number[]> = {}
  if (
    candidate.splitterRatios &&
    typeof candidate.splitterRatios === 'object' &&
    !Array.isArray(candidate.splitterRatios)
  ) {
    for (const [key, ratios] of Object.entries(candidate.splitterRatios)) {
      if (!key.trim() || !Array.isArray(ratios)) {
        continue
      }
      const sanitizedRatios = ratios.filter(
        (ratio): ratio is number => typeof ratio === 'number' && Number.isFinite(ratio) && ratio > 0,
      )
      if (sanitizedRatios.length === ratios.length && sanitizedRatios.length > 0) {
        splitterRatios[key] = sanitizedRatios
      }
    }
  }

  return {
    paneSlots,
    focusedPaneId,
    splitterRatios,
    preSpawnSidebarMode:
      candidate.preSpawnSidebarMode === 'pinned' || candidate.preSpawnSidebarMode === 'collapsed'
        ? candidate.preSpawnSidebarMode
        : null,
  }
}

function createDefaultAgentWorkspaceLayout(project: ProjectDetailView): AgentWorkspaceLayoutState {
  const selectedAgentSessionId = selectAgentSessionId(project.agentSessions)
  const paneId = createDefaultAgentWorkspacePaneId(project.id)
  return {
    paneSlots: [{ id: paneId, agentSessionId: selectedAgentSessionId || null }],
    focusedPaneId: paneId,
    splitterRatios: {},
    preSpawnSidebarMode: null,
  }
}

function reconcileAgentWorkspaceLayout(
  project: ProjectDetailView,
  layout: AgentWorkspaceLayoutState | null | undefined,
): AgentWorkspaceLayoutState {
  const selectedAgentSessionId = selectAgentSessionId(project.agentSessions)
  const activeSessionIds = new Set(
    project.agentSessions
      .filter((session) => session.isActive)
      .map((session) => session.agentSessionId),
  )
  const isKnownSession = (agentSessionId: string) =>
    activeSessionIds.size === 0
      ? agentSessionId === selectedAgentSessionId
      : activeSessionIds.has(agentSessionId)

  const sanitizedLayout = sanitizeAgentWorkspaceLayout(layout) ?? createDefaultAgentWorkspaceLayout(project)
  const paneSlots = sanitizedLayout.paneSlots.map((slot) => {
    if (!slot.agentSessionId) {
      return slot
    }

    return isKnownSession(slot.agentSessionId)
      ? slot
      : {
          ...slot,
          agentSessionId: null,
        }
  })

  if (paneSlots.length === 0) {
    return createDefaultAgentWorkspaceLayout(project)
  }

  const normalizedPaneSlots =
    paneSlots.length === 1 && paneSlots[0]?.agentSessionId && paneSlots[0].agentSessionId !== selectedAgentSessionId
      ? [{ ...paneSlots[0], agentSessionId: selectedAgentSessionId }]
      : paneSlots

  const focusedPaneId = normalizedPaneSlots.some((slot) => slot.id === sanitizedLayout.focusedPaneId)
    ? sanitizedLayout.focusedPaneId
    : normalizedPaneSlots[0].id

  return {
    ...sanitizedLayout,
    paneSlots: normalizedPaneSlots,
    focusedPaneId,
  }
}

function areAgentWorkspaceLayoutsEqual(
  left: AgentWorkspaceLayoutState | null | undefined,
  right: AgentWorkspaceLayoutState,
): boolean {
  if (!left) {
    return false
  }

  return JSON.stringify(left) === JSON.stringify(right)
}

function applyAgentSessionToProject(
  project: ProjectDetailView,
  agentSession: AgentSessionView,
): ProjectDetailView {
  if (project.id !== agentSession.projectId) {
    return project
  }

  const existingIndex = project.agentSessions.findIndex(
    (session) => session.agentSessionId === agentSession.agentSessionId,
  )
  const agentSessions =
    existingIndex >= 0
      ? project.agentSessions.map((session) =>
          session.agentSessionId === agentSession.agentSessionId ? agentSession : session,
        )
      : [...project.agentSessions, agentSession]
  const selectedAgentSessionId = selectAgentSessionId(agentSessions)

  return {
    ...project,
    agentSessions,
    selectedAgentSessionId,
    selectedAgentSession:
      agentSessions.find((session) => session.agentSessionId === selectedAgentSessionId) ?? null,
  }
}

function hasOwnRecord<T>(record: Record<string, T>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(record, key)
}

function selectAgentSessionInProject(
  project: ProjectDetailView,
  agentSessionId: string,
  updatedSession?: AgentSessionView,
): ProjectDetailView {
  let selectedAgentSession: AgentSessionView | null = null
  let matched = false
  const agentSessions = project.agentSessions.map((session) => {
    if (session.agentSessionId === agentSessionId) {
      matched = true
      selectedAgentSession = {
        ...session,
        ...(updatedSession?.agentSessionId === agentSessionId ? updatedSession : {}),
        selected: true,
      }
      return selectedAgentSession
    }

    return session.selected ? { ...session, selected: false } : session
  })

  if (!matched && updatedSession?.projectId === project.id) {
    selectedAgentSession = {
      ...updatedSession,
      selected: true,
    }
    agentSessions.push(selectedAgentSession)
  }

  if (!selectedAgentSession) {
    return project
  }

  return {
    ...project,
    agentSessions,
    selectedAgentSession,
    selectedAgentSessionId: selectedAgentSession.agentSessionId,
  }
}

function createEmptyAgentSessionProject(project: ProjectDetailView): ProjectDetailView {
  return {
    ...project,
    agentSessions: project.agentSessions.map((session) =>
      session.selected ? { ...session, selected: false } : session,
    ),
    selectedAgentSession: null,
    selectedAgentSessionId: '',
    runtimeRun: null,
    autonomousRun: null,
  }
}

function createEmptyRepositoryDiffState(): RepositoryDiffState {
  return {
    status: 'idle',
    diff: null,
    errorMessage: null,
    projectId: null,
  }
}

function createInitialRepositoryDiffs(): Record<RepositoryDiffScope, RepositoryDiffState> {
  return {
    staged: createEmptyRepositoryDiffState(),
    unstaged: createEmptyRepositoryDiffState(),
    worktree: createEmptyRepositoryDiffState(),
  }
}

function getDefaultDiffScope(status: RepositoryStatusView | null): RepositoryDiffScope {
  if (!status) {
    return 'unstaged'
  }

  if (status.unstagedCount > 0) {
    return 'unstaged'
  }

  if (status.stagedCount > 0) {
    return 'staged'
  }

  return 'worktree'
}

function getActivePhase(project: ProjectDetailView | null): Phase | null {
  if (!project) {
    return null
  }

  return (
    project.phases.find((phase) => phase.status === 'active') ??
    project.phases.find((phase) => phase.id === project.activePhase) ??
    project.phases[0] ??
    null
  )
}

function getOperatorActionError(error: unknown, fallback: string): OperatorActionErrorView {
  if (error instanceof XeroDesktopError) {
    return {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
    }
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return {
      code: 'operator_action_failed',
      message: error.message,
      retryable: false,
    }
  }

  return {
    code: 'operator_action_failed',
    message: fallback,
    retryable: false,
  }
}

function getProviderModelCatalogRefreshId(providerId: string): string {
  return getCloudProviderDefaultProfileId(providerId) ?? providerId
}

function getProviderModelCatalogStateKeys(providerId: string): string[] {
  const profileId = getProviderModelCatalogRefreshId(providerId)
  return profileId === providerId ? [providerId] : [providerId, profileId]
}

export function useXeroDesktopState(
  options: UseXeroDesktopStateOptions = {},
): UseXeroDesktopStateResult {
  const adapter = options.adapter ?? XeroDesktopAdapter
  const highChurnStoreRef = useRef<XeroHighChurnStore | null>(null)
  if (!highChurnStoreRef.current) {
    highChurnStoreRef.current = createXeroHighChurnStore()
  }
  const highChurnStore = highChurnStoreRef.current
  const repositoryStatus = useSelectorStoreValue(
    highChurnStore,
    selectRepositoryStatus,
    Object.is,
    { disabled: options.subscribeRepositoryStatus === false },
  )
  const runtimeStreams = useSelectorStoreValue(
    highChurnStore,
    selectRuntimeStreams,
    Object.is,
    { disabled: options.subscribeRuntimeStreams === false },
  )
  const [projects, setProjects] = useState<ProjectListItem[]>([])
  const [activeProject, setActiveProject] = useState<ProjectDetailView | null>(null)
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null)
  const [agentWorkspaceLayouts, setAgentWorkspaceLayouts] =
    useState<Record<string, AgentWorkspaceLayoutState>>(readAgentWorkspaceLayoutStore)
  const [repositoryDiffs, setRepositoryDiffs] = useState<Record<RepositoryDiffScope, RepositoryDiffState>>(
    createInitialRepositoryDiffs,
  )
  const [runtimeSessions, setRuntimeSessions] = useState<Record<string, RuntimeSessionView>>({})
  const [runtimeRuns, setRuntimeRuns] = useState<Record<string, RuntimeRunView>>({})
  const [autonomousRuns, setAutonomousRuns] = useState<Record<string, NonNullable<ProjectDetailView['autonomousRun']>>>({})
  const [notificationRoutes, setNotificationRoutes] = useState<Record<string, NotificationRouteDto[]>>({})
  const [notificationRouteLoadStatuses, setNotificationRouteLoadStatuses] = useState<
    Record<string, NotificationRoutesLoadStatus>
  >({})
  const [notificationRouteLoadErrors, setNotificationRouteLoadErrors] = useState<
    Record<string, OperatorActionErrorView | null>
  >({})
  const [notificationSyncSummaries, setNotificationSyncSummaries] = useState<
    Record<string, SyncNotificationAdaptersResponseDto | null>
  >({})
  const [notificationSyncErrors, setNotificationSyncErrors] = useState<
    Record<string, OperatorActionErrorView | null>
  >({})
  const [runtimeLoadErrors, setRuntimeLoadErrors] = useState<Record<string, string | null>>({})
  const [runtimeRunLoadErrors, setRuntimeRunLoadErrors] = useState<Record<string, string | null>>({})
  const [autonomousRunLoadErrors, setAutonomousRunLoadErrors] = useState<Record<string, string | null>>({})
  const [activeDiffScope, setActiveDiffScope] = useState<RepositoryDiffScope>('unstaged')
  const [isLoading, setIsLoading] = useState(true)
  const [isProjectLoading, setIsProjectLoading] = useState(false)
  const [pendingProjectSelectionId, setPendingProjectSelectionId] = useState<string | null>(null)
  const [isImporting, setIsImporting] = useState(false)
  const [projectRemovalStatus, setProjectRemovalStatus] = useState<ProjectRemovalStatus>('idle')
  const [pendingProjectRemovalId, setPendingProjectRemovalId] = useState<string | null>(null)
  const [operatorActionStatus, setOperatorActionStatus] = useState<OperatorActionStatus>('idle')
  const [pendingOperatorActionId, setPendingOperatorActionId] = useState<string | null>(null)
  const [operatorActionError, setOperatorActionError] = useState<OperatorActionErrorView | null>(null)
  const [autonomousRunActionStatus, setAutonomousRunActionStatus] = useState<AutonomousRunActionStatus>('idle')
  const [pendingAutonomousRunAction, setPendingAutonomousRunAction] = useState<AutonomousRunActionKind | null>(null)
  const [autonomousRunActionError, setAutonomousRunActionError] = useState<OperatorActionErrorView | null>(null)
  const [runtimeRunActionStatus, setRuntimeRunActionStatus] = useState<RuntimeRunActionStatus>('idle')
  const [pendingRuntimeRunAction, setPendingRuntimeRunAction] = useState<RuntimeRunActionKind | null>(null)
  const [runtimeRunActionError, setRuntimeRunActionError] = useState<OperatorActionErrorView | null>(null)
  const [notificationRouteMutationStatus, setNotificationRouteMutationStatus] =
    useState<NotificationRouteMutationStatus>('idle')
  const [pendingNotificationRouteId, setPendingNotificationRouteId] = useState<string | null>(null)
  const [notificationRouteMutationError, setNotificationRouteMutationError] =
    useState<OperatorActionErrorView | null>(null)
  const [providerCredentials, setProviderCredentials] =
    useState<ProviderCredentialsSnapshotDto | null>(null)
  const [providerCredentialsLoadStatus, setProviderCredentialsLoadStatus] =
    useState<ProviderCredentialsLoadStatus>('idle')
  const [providerCredentialsLoadError, setProviderCredentialsLoadError] =
    useState<OperatorActionErrorView | null>(null)
  const [providerCredentialsSaveStatus, setProviderCredentialsSaveStatus] =
    useState<ProviderCredentialsSaveStatus>('idle')
  const [providerCredentialsSaveError, setProviderCredentialsSaveError] =
    useState<OperatorActionErrorView | null>(null)
  const [providerModelCatalogs, setProviderModelCatalogs] = useState<Record<string, ProviderModelCatalogDto>>({})
  const [providerModelCatalogLoadStatuses, setProviderModelCatalogLoadStatuses] = useState<
    Record<string, ProviderModelCatalogLoadStatus>
  >({})
  const [providerModelCatalogLoadErrors, setProviderModelCatalogLoadErrors] = useState<
    Record<string, OperatorActionErrorView | null>
  >({})
  const [doctorReport, setDoctorReport] = useState<XeroDoctorReportDto | null>(null)
  const [doctorReportStatus, setDoctorReportStatus] = useState<DoctorReportRunStatus>('idle')
  const [doctorReportError, setDoctorReportError] = useState<OperatorActionErrorView | null>(null)
  const [mcpRegistry, setMcpRegistry] = useState<McpRegistryDto | null>(null)
  const [mcpImportDiagnostics, setMcpImportDiagnostics] = useState<McpImportDiagnosticDto[]>([])
  const [mcpRegistryLoadStatus, setMcpRegistryLoadStatus] = useState<McpRegistryLoadStatus>('idle')
  const [mcpRegistryLoadError, setMcpRegistryLoadError] = useState<OperatorActionErrorView | null>(null)
  const [mcpRegistryMutationStatus, setMcpRegistryMutationStatus] =
    useState<McpRegistryMutationStatus>('idle')
  const [pendingMcpServerId, setPendingMcpServerId] = useState<string | null>(null)
  const [mcpRegistryMutationError, setMcpRegistryMutationError] = useState<OperatorActionErrorView | null>(null)
  const [skillRegistry, setSkillRegistry] = useState<SkillRegistryDto | null>(null)
  const [skillRegistryLoadStatus, setSkillRegistryLoadStatus] = useState<SkillRegistryLoadStatus>('idle')
  const [skillRegistryLoadError, setSkillRegistryLoadError] = useState<OperatorActionErrorView | null>(null)
  const [skillRegistryMutationStatus, setSkillRegistryMutationStatus] =
    useState<SkillRegistryMutationStatus>('idle')
  const [pendingSkillSourceId, setPendingSkillSourceId] = useState<string | null>(null)
  const [skillRegistryMutationError, setSkillRegistryMutationError] =
    useState<OperatorActionErrorView | null>(null)
  const [usageSummaries, setUsageSummaries] =
    useState<Record<string, ProjectUsageSummaryDto>>({})
  const [usageSummaryLoadErrors, setUsageSummaryLoadErrors] =
    useState<Record<string, string | null>>({})
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [refreshSource, setRefreshSource] = useState<RefreshSource>(null)
  const [runtimeStreamRetryToken, setRuntimeStreamRetryToken] = useState(0)
  const [agentSessionRuntimeCacheRevision, setAgentSessionRuntimeCacheRevision] = useState(0)
  const activeProjectRef = useRef<ProjectDetailView | null>(null)
  const activeProjectIdRef = useRef<string | null>(null)
  const projectDetailsRef = useRef<Record<string, ProjectDetailView>>({})
  const projectSelectionRequestRef = useRef(0)
  const projectPrefetchInFlightRef = useRef<Partial<Record<string, Promise<void>>>>({})
  const repositoryStatusRef = useRef<RepositoryStatusView | null>(null)
  const repositoryStatusSyncKeyRef = useRef('none')
  const repositoryDiffRevisionRef = useRef(createRepositoryStatusDiffRevision(null))
  const repositoryStatusRefreshInFlightRef = useRef(false)
  const latestLoadRequestRef = useRef(0)
  const latestDiffRequestRef = useRef<Record<RepositoryDiffScope, number>>({
    staged: 0,
    unstaged: 0,
    worktree: 0,
  })
  const repositoryDiffsRef = useRef<Record<RepositoryDiffScope, RepositoryDiffState>>(createInitialRepositoryDiffs())
  const runtimeSessionsRef = useRef<Record<string, RuntimeSessionView>>({})
  const runtimeRunsRef = useRef<Record<string, RuntimeRunView>>({})
  const autonomousRunsRef = useRef<Record<string, NonNullable<ProjectDetailView['autonomousRun']>>>({})
  const runtimeRunsBySessionRef = useRef<Record<string, RuntimeRunView | null>>({})
  const autonomousRunsBySessionRef = useRef<Record<string, ProjectDetailView['autonomousRun']>>({})
  const runtimeStreamsBySessionRef = useRef<Record<string, RuntimeStreamView>>({})
  const agentSessionRuntimePrefetchInFlightRef = useRef<Partial<Record<string, Promise<void>>>>({})
  const agentWorkspaceLayoutsRef = useRef<Record<string, AgentWorkspaceLayoutState>>(agentWorkspaceLayouts)
  const pendingSpawnPaneIdsRef = useRef<Set<string>>(new Set())
  const notificationRoutesRef = useRef<Record<string, NotificationRouteDto[]>>({})
  const notificationRouteLoadStatusesRef = useRef<Record<string, NotificationRoutesLoadStatus>>({})
  const notificationRouteLoadErrorsRef = useRef<Record<string, OperatorActionErrorView | null>>({})
  const notificationRouteLoadRequestRef = useRef<Record<string, number>>({})
  const notificationRouteLoadInFlightRef = useRef<Record<string, Promise<NotificationRoutesLoadResult>>>({})
  const notificationSyncSummariesRef = useRef<Record<string, SyncNotificationAdaptersResponseDto | null>>({})
  const notificationDispatchesRef = useRef<Record<string, NotificationDispatchDto[]>>({})
  const trustSnapshotRef = useRef<Record<string, AgentTrustSnapshotView>>({})
  const providerCredentialsRef = useRef<ProviderCredentialsSnapshotDto | null>(null)
  const providerCredentialsLoadInFlightRef = useRef<Promise<ProviderCredentialsSnapshotDto> | null>(
    null,
  )
  const providerModelCatalogsRef = useRef<Record<string, ProviderModelCatalogDto>>({})
  const providerModelCatalogLoadStatusesRef = useRef<Record<string, ProviderModelCatalogLoadStatus>>({})
  const providerModelCatalogLoadErrorsRef = useRef<Record<string, OperatorActionErrorView | null>>({})
  const providerModelCatalogLoadRequestRef = useRef<Record<string, number>>({})
  const providerModelCatalogLoadInFlightRef = useRef<
    Record<string, { requestKey: string; promise: Promise<ProviderModelCatalogDto> }>
  >({})
  const mcpRegistryRef = useRef<McpRegistryDto | null>(null)
  const mcpRegistryLoadInFlightRef = useRef<Promise<McpRegistryDto> | null>(null)
  const skillRegistryRef = useRef<SkillRegistryDto | null>(null)
  const skillRegistryLoadInFlightRef = useRef<Promise<SkillRegistryDto> | null>(null)
  const runtimeRefreshTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingRuntimeRefreshRef = useRef<{
    projectId: string
    source: Extract<RefreshSource, 'runtime_run:updated' | 'runtime_stream:action_required'>
  } | null>(null)
  const runtimeActionRefreshKeysRef = useRef<Record<string, Set<string>>>({})
  const runtimeRunRefreshKeyRef = useRef<Record<string, string>>({})
  const previousRuntimeAuthRef = useRef<Record<string, boolean>>({})

  const trimRuntimeStreamSessionCache = useCallback((protectedKey: string | null = null) => {
    runtimeStreamsBySessionRef.current = trimRecordCacheToByteBudget(
      runtimeStreamsBySessionRef.current,
      {
        estimateBytes: estimateRuntimeStreamViewBytes,
        maxBytes: RUNTIME_STREAM_SESSION_CACHE_MAX_BYTES,
        maxEntries: RUNTIME_STREAM_SESSION_CACHE_MAX_ENTRIES,
        protectedKeys: [protectedKey],
      },
    ).records
  }, [])

  const setRepositoryStatus = useCallback(
    (action: SetStateAction<RepositoryStatusView | null>) => {
      const nextStatus = highChurnStore.setRepositoryStatus(action)
      repositoryStatusRef.current = nextStatus
      repositoryStatusSyncKeyRef.current = createRepositoryStatusSyncKey(nextStatus)
      return nextStatus
    },
    [highChurnStore],
  )

  const setRuntimeStreams = useCallback(
    (action: SetStateAction<Record<string, RuntimeStreamView>>) => {
      const nextStreams = highChurnStore.setRuntimeStreams(action)
      for (const runtimeStream of Object.values(nextStreams)) {
        const cacheKey = createAgentSessionStateKey(runtimeStream.projectId, runtimeStream.agentSessionId)
        runtimeStreamsBySessionRef.current[cacheKey] = runtimeStream
        trimRuntimeStreamSessionCache(cacheKey)
      }
      return nextStreams
    },
    [highChurnStore, trimRuntimeStreamSessionCache],
  )

  useEffect(() => {
    activeProjectRef.current = activeProject
    if (activeProject) {
      projectDetailsRef.current[activeProject.id] = activeProject
    }
  }, [activeProject])

  useEffect(() => {
    agentWorkspaceLayoutsRef.current = agentWorkspaceLayouts
  }, [agentWorkspaceLayouts])

  useEffect(() => {
    if (typeof window === 'undefined') {
      return
    }

    const handle = window.setTimeout(() => {
      writeAgentWorkspaceLayoutStore(agentWorkspaceLayoutsRef.current)
    }, AGENT_WORKSPACE_LAYOUT_PERSIST_DEBOUNCE_MS)

    return () => window.clearTimeout(handle)
  }, [agentWorkspaceLayouts])

  useEffect(() => {
    if (!activeProject) {
      return
    }

    setAgentWorkspaceLayouts((currentLayouts) => {
      const nextLayout = reconcileAgentWorkspaceLayout(
        activeProject,
        currentLayouts[activeProject.id],
      )
      if (areAgentWorkspaceLayoutsEqual(currentLayouts[activeProject.id], nextLayout)) {
        return currentLayouts
      }

      return {
        ...currentLayouts,
        [activeProject.id]: nextLayout,
      }
    })
  }, [activeProject])

  useEffect(() => {
    if (isLoading) {
      return
    }

    const liveProjectIds = new Set(projects.map((project) => project.id))
    for (const projectId of Object.keys(projectDetailsRef.current)) {
      if (!liveProjectIds.has(projectId)) {
        delete projectDetailsRef.current[projectId]
      }
    }
    const removeStaleSessionRecords = (record: Record<string, unknown>) => {
      for (const key of Object.keys(record)) {
        const [projectId] = key.split('::', 1)
        if (!liveProjectIds.has(projectId)) {
          delete record[key]
        }
      }
    }
    removeStaleSessionRecords(runtimeRunsBySessionRef.current)
    removeStaleSessionRecords(autonomousRunsBySessionRef.current)
    removeStaleSessionRecords(runtimeStreamsBySessionRef.current)
    removeStaleSessionRecords(agentSessionRuntimePrefetchInFlightRef.current)
    setAgentWorkspaceLayouts((currentLayouts) => {
      let changed = false
      const nextLayouts = { ...currentLayouts }
      for (const projectId of Object.keys(nextLayouts)) {
        if (!liveProjectIds.has(projectId)) {
          delete nextLayouts[projectId]
          changed = true
        }
      }
      return changed ? nextLayouts : currentLayouts
    })
  }, [isLoading, projects])

  useEffect(() => {
    activeProjectIdRef.current = activeProjectId
  }, [activeProjectId])

  useEffect(() => {
    repositoryStatusRef.current = repositoryStatus
    repositoryStatusSyncKeyRef.current = createRepositoryStatusSyncKey(repositoryStatus)
  }, [repositoryStatus])

  useEffect(() => {
    repositoryDiffsRef.current = repositoryDiffs
  }, [repositoryDiffs])

  useEffect(() => {
    runtimeSessionsRef.current = runtimeSessions
  }, [runtimeSessions])

  useEffect(() => {
    runtimeRunsRef.current = runtimeRuns
  }, [runtimeRuns])

  useEffect(() => {
    autonomousRunsRef.current = autonomousRuns
  }, [autonomousRuns])

  useEffect(() => {
    if (!activeProject || !activeProjectId) {
      return
    }

    const agentSessionId = activeProject.selectedAgentSessionId
    if (!activeProject.agentSessions.some((session) => session.agentSessionId === agentSessionId)) {
      return
    }

    const cacheKey = createAgentSessionStateKey(activeProjectId, agentSessionId)
    const runtimeRun = runtimeRuns[activeProjectId] ?? activeProject.runtimeRun ?? null
    runtimeRunsBySessionRef.current[cacheKey] =
      runtimeRun?.agentSessionId === agentSessionId ? runtimeRun : null

    const autonomousRun = autonomousRuns[activeProjectId] ?? activeProject.autonomousRun ?? null
    autonomousRunsBySessionRef.current[cacheKey] =
      autonomousRun?.agentSessionId === agentSessionId ? autonomousRun : null

    const runtimeStream =
      runtimeStreams[createRuntimeStreamStoreKey(activeProjectId, agentSessionId)] ??
      runtimeStreams[activeProjectId] ??
      null
    if (runtimeStream?.agentSessionId === agentSessionId) {
      runtimeStreamsBySessionRef.current[cacheKey] = runtimeStream
      trimRuntimeStreamSessionCache(cacheKey)
    }
  }, [activeProject, activeProjectId, autonomousRuns, runtimeRuns, runtimeStreams, trimRuntimeStreamSessionCache])

  useEffect(() => {
    notificationRoutesRef.current = notificationRoutes
  }, [notificationRoutes])

  useEffect(() => {
    notificationRouteLoadStatusesRef.current = notificationRouteLoadStatuses
  }, [notificationRouteLoadStatuses])

  useEffect(() => {
    notificationRouteLoadErrorsRef.current = notificationRouteLoadErrors
  }, [notificationRouteLoadErrors])

  useEffect(() => {
    notificationSyncSummariesRef.current = notificationSyncSummaries
  }, [notificationSyncSummaries])


  useEffect(() => {
    providerCredentialsRef.current = providerCredentials
  }, [providerCredentials])

  useEffect(() => {
    providerModelCatalogsRef.current = providerModelCatalogs
  }, [providerModelCatalogs])

  useEffect(() => {
    const protectedCatalogKeys = new Set<string>()
    for (const credential of providerCredentials?.credentials ?? []) {
      for (const key of getProviderModelCatalogStateKeys(credential.providerId)) {
        protectedCatalogKeys.add(key)
      }
    }

    const trimmed = trimRecordCacheToByteBudget(providerModelCatalogs, {
      estimateBytes: estimateProviderModelCatalogBytes,
      maxBytes: PROVIDER_MODEL_CATALOG_CACHE_MAX_BYTES,
      maxEntries: PROVIDER_MODEL_CATALOG_CACHE_MAX_ENTRIES,
      protectedKeys: protectedCatalogKeys,
    })

    if (trimmed.records === providerModelCatalogs) {
      return
    }

    providerModelCatalogsRef.current = trimmed.records
    setProviderModelCatalogs(trimmed.records)
    const evictedKeys = new Set(trimmed.evictedKeys)
    setProviderModelCatalogLoadStatuses((currentStatuses) => {
      const nextStatuses = { ...currentStatuses }
      for (const key of evictedKeys) {
        delete nextStatuses[key]
      }
      return nextStatuses
    })
    setProviderModelCatalogLoadErrors((currentErrors) => {
      const nextErrors = { ...currentErrors }
      for (const key of evictedKeys) {
        delete nextErrors[key]
      }
      return nextErrors
    })
  }, [providerCredentials, providerModelCatalogs])

  useEffect(() => {
    providerModelCatalogLoadStatusesRef.current = providerModelCatalogLoadStatuses
  }, [providerModelCatalogLoadStatuses])

  useEffect(() => {
    providerModelCatalogLoadErrorsRef.current = providerModelCatalogLoadErrors
  }, [providerModelCatalogLoadErrors])


  useEffect(() => {
    mcpRegistryRef.current = mcpRegistry
  }, [mcpRegistry])

  useEffect(() => {
    skillRegistryRef.current = skillRegistry
  }, [skillRegistry])

  const supersedeInFlightProjectLoad = useCallback((projectId: string) => {
    if (activeProjectIdRef.current !== projectId) {
      return
    }

    latestLoadRequestRef.current += 1
    setIsProjectLoading(false)
  }, [])

  const updateRuntimeStream = useCallback(
    (
      projectId: string,
      agentSessionId: string,
      updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
    ) => {
      setRuntimeStreams((currentStreams) => {
        const currentProject = activeProjectRef.current
        const activeAgentSessionId =
          currentProject?.id === projectId ? selectAgentSessionId(currentProject.agentSessions) : null
        const currentStream =
          currentStreams[createRuntimeStreamStoreKey(projectId, agentSessionId)] ??
          (activeAgentSessionId === agentSessionId ? currentStreams[projectId] ?? null : null)
        const nextStream = updater(currentStream)
        if (!nextStream) {
          return removeRuntimeStreamForSession(currentStreams, projectId, agentSessionId)
        }

        const nextKey = createRuntimeStreamStoreKey(projectId, nextStream.agentSessionId)
        runtimeStreamsBySessionRef.current[
          createAgentSessionStateKey(projectId, nextStream.agentSessionId)
        ] = nextStream

        const nextStreams = {
          ...currentStreams,
          [nextKey]: nextStream,
        }

        if (nextStream.agentSessionId === activeAgentSessionId) {
          nextStreams[projectId] = nextStream
        } else if (currentStreams[projectId]?.agentSessionId === nextStream.agentSessionId) {
          delete nextStreams[projectId]
        }

        return nextStreams
      })
    },
    [],
  )

  const resetRepositoryDiffs = useCallback((status: RepositoryStatusView | null) => {
    const nextRevision = status?.diffRevision ?? createRepositoryStatusDiffRevision(status)
    if (repositoryDiffRevisionRef.current === nextRevision) {
      return
    }

    repositoryDiffRevisionRef.current = nextRevision
    setRepositoryDiffs(createInitialRepositoryDiffs())
    setActiveDiffScope(getDefaultDiffScope(status))
  }, [])

  const handleAdapterEventError = useCallback((error: XeroDesktopError) => {
    setErrorMessage(getDesktopErrorMessage(error))
  }, [])

  const refreshUsageSummary = useCallback(
    async (projectId: string): Promise<ProjectUsageSummaryDto | null> => {
      try {
        const summary = await adapter.getProjectUsageSummary(projectId)
        setUsageSummaries((current) => ({
          ...current,
          [projectId]: summary,
        }))
        setUsageSummaryLoadErrors((current) => ({
          ...current,
          [projectId]: null,
        }))
        return summary
      } catch (error) {
        setUsageSummaryLoadErrors((current) => ({
          ...current,
          [projectId]: getDesktopErrorMessage(error),
        }))
        return null
      }
    },
    [adapter],
  )

  const syncRepositoryStatus = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId || repositoryStatusRefreshInFlightRef.current) {
      return null
    }

    repositoryStatusRefreshInFlightRef.current = true

    try {
      const response = await adapter.getRepositoryStatus(projectId)
      if (activeProjectIdRef.current !== projectId) {
        return null
      }

      const nextStatus = mapRepositoryStatus(response)
      const nextStatusKey = createRepositoryStatusSyncKey(nextStatus)
      if (repositoryStatusSyncKeyRef.current === nextStatusKey) {
        return nextStatus
      }

      repositoryStatusRef.current = nextStatus
      repositoryStatusSyncKeyRef.current = nextStatusKey
      setRefreshSource('repository:status_changed')
      setRepositoryStatus(nextStatus)
      resetRepositoryDiffs(nextStatus)
      return nextStatus
    } catch {
      return null
    } finally {
      repositoryStatusRefreshInFlightRef.current = false
    }
  }, [adapter, resetRepositoryDiffs])

  const applyRuntimeSessionUpdate = useCallback(
    (runtimeSession: RuntimeSessionView, options: { clearGlobalError?: boolean } = {}) => {
      supersedeInFlightProjectLoad(runtimeSession.projectId)

      setRuntimeSessions((currentRuntimeSessions) => ({
        ...currentRuntimeSessions,
        [runtimeSession.projectId]: runtimeSession,
      }))
      setRuntimeLoadErrors((currentErrors) => ({
        ...currentErrors,
        [runtimeSession.projectId]: null,
      }))
      setProjects((currentProjects) =>
        currentProjects.map((project) =>
          project.id === runtimeSession.projectId ? applyRuntimeToProjectList(project, runtimeSession) : project,
        ),
      )
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === runtimeSession.projectId
          ? applyRuntimeSession(currentProject, runtimeSession)
          : currentProject,
      )

      if ((runtimeSession.isSignedOut || runtimeSession.isFailed) && runtimeSession.projectId) {
        setRuntimeStreams((currentStreams) => removeRuntimeStreamsForProject(currentStreams, runtimeSession.projectId))
      }

      if (options.clearGlobalError ?? true) {
        setErrorMessage(null)
      }

      return runtimeSession
    },
    [supersedeInFlightProjectLoad],
  )

  const applyRuntimeRunUpdate = useCallback(
    (
      projectId: string,
      runtimeRun: RuntimeRunView | null,
      options: { clearGlobalError?: boolean; loadError?: string | null } = {},
    ) => {
      supersedeInFlightProjectLoad(projectId)
      const agentSessionId =
        runtimeRun?.agentSessionId ??
        (activeProjectRef.current?.id === projectId
          ? selectAgentSessionId(activeProjectRef.current.agentSessions)
          : null)
      if (agentSessionId) {
        runtimeRunsBySessionRef.current[
          createAgentSessionStateKey(projectId, agentSessionId)
        ] = runtimeRun
        setAgentSessionRuntimeCacheRevision((revision) => revision + 1)
      }
      const isSelectedSession = Boolean(
        agentSessionId &&
          activeProjectRef.current?.id === projectId &&
          selectAgentSessionId(activeProjectRef.current.agentSessions) === agentSessionId,
      )

      if (isSelectedSession) {
        setRuntimeRuns((currentRuntimeRuns) => {
          if (!runtimeRun) {
            return removeProjectRecord(currentRuntimeRuns, projectId)
          }

          return {
            ...currentRuntimeRuns,
            [projectId]: runtimeRun,
          }
        })
      }
      setRuntimeRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: options.loadError ?? null,
      }))
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === projectId && isSelectedSession
          ? applyRuntimeRun(currentProject, runtimeRun)
          : currentProject,
      )

      if (options.clearGlobalError ?? false) {
        setErrorMessage(options.loadError ?? null)
      }

      return runtimeRun
    },
    [supersedeInFlightProjectLoad],
  )

  const applyAutonomousRunStateUpdate = useCallback(
    (
      projectId: string,
      inspection: {
        autonomousRun: ProjectDetailView['autonomousRun']
      },
      options: { clearGlobalError?: boolean; loadError?: string | null } = {},
    ) => {
      supersedeInFlightProjectLoad(projectId)
      const agentSessionId =
        inspection.autonomousRun?.agentSessionId ??
        (activeProjectRef.current?.id === projectId
          ? selectAgentSessionId(activeProjectRef.current.agentSessions)
          : null)
      if (agentSessionId) {
        autonomousRunsBySessionRef.current[
          createAgentSessionStateKey(projectId, agentSessionId)
        ] = inspection.autonomousRun ?? null
        setAgentSessionRuntimeCacheRevision((revision) => revision + 1)
      }
      const isSelectedSession = Boolean(
        agentSessionId &&
          activeProjectRef.current?.id === projectId &&
          selectAgentSessionId(activeProjectRef.current.agentSessions) === agentSessionId,
      )

      if (isSelectedSession) {
        setAutonomousRuns((currentRuns) => {
          if (!inspection.autonomousRun) {
            return removeProjectRecord(currentRuns, projectId)
          }

          return {
            ...currentRuns,
            [projectId]: inspection.autonomousRun,
          }
        })
      }
      setAutonomousRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: options.loadError ?? null,
      }))
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === projectId && isSelectedSession
          ? applyAutonomousRunState(
              currentProject,
              inspection.autonomousRun,
            )
          : currentProject,
      )

      if (options.clearGlobalError ?? false) {
        setErrorMessage(options.loadError ?? null)
      }

      return inspection.autonomousRun
    },
    [supersedeInFlightProjectLoad],
  )

  const syncRuntimeSession = useCallback(
    async (projectId: string) => {
      const response = await adapter.getRuntimeSession(projectId)
      return applyRuntimeSessionUpdate(mapRuntimeSession(response), { clearGlobalError: false })
    },
    [adapter, applyRuntimeSessionUpdate],
  )

  const syncRuntimeRun = useCallback(
    async (projectId: string) => {
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )
      const response = await adapter.getRuntimeRun(projectId, agentSessionId)
      const runtimeRun = response ? mapRuntimeRun(response) : null
      return applyRuntimeRunUpdate(projectId, runtimeRun?.agentSessionId === agentSessionId ? runtimeRun : null, {
        clearGlobalError: false,
        loadError: null,
      })
    },
    [adapter, applyRuntimeRunUpdate],
  )

  const syncAutonomousRun = useCallback(
    async (projectId: string) => {
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )
      const response = await adapter.getAutonomousRun(projectId, agentSessionId)
      const inspection = mapAutonomousRunInspection(response)
      if (inspection.autonomousRun?.agentSessionId !== agentSessionId) {
        inspection.autonomousRun = null
      }
      applyAutonomousRunStateUpdate(projectId, inspection, {
        clearGlobalError: false,
        loadError: null,
      })
      return inspection.autonomousRun
    },
    [adapter, applyAutonomousRunStateUpdate],
  )

  const loadNotificationRoutes = useCallback(
    async (projectId: string, options: { force?: boolean } = {}): Promise<NotificationRoutesLoadResult> =>
      loadNotificationRoutesForProject({
        adapter,
        projectId,
        force: options.force,
        notificationRoutesRef,
        notificationRouteLoadErrorsRef,
        notificationRouteLoadRequestRef,
        notificationRouteLoadInFlightRef,
        setNotificationRoutes,
        setNotificationRouteLoadStatuses,
        setNotificationRouteLoadErrors,
        getOperatorActionError,
      }),
    [adapter],
  )

  const applyAgentSessionRuntimeState = useCallback(
    (
      projectId: string,
      agentSessionId: string,
      runtimeRun: RuntimeRunView | null,
      autonomousRun: ProjectDetailView['autonomousRun'],
      runtimeStream: RuntimeStreamView | null,
    ) => {
      runtimeRunsBySessionRef.current[
        createAgentSessionStateKey(projectId, agentSessionId)
      ] = runtimeRun
      autonomousRunsBySessionRef.current[
        createAgentSessionStateKey(projectId, agentSessionId)
      ] = autonomousRun ?? null
      setAgentSessionRuntimeCacheRevision((revision) => revision + 1)

      setRuntimeStreams((currentStreams) => {
        if (!runtimeStream) {
          return removeRuntimeStreamForSession(currentStreams, projectId, agentSessionId)
        }

        const nextStreams = {
          ...currentStreams,
          [createRuntimeStreamStoreKey(projectId, agentSessionId)]: runtimeStream,
        }

        if (selectAgentSessionId(activeProjectRef.current?.agentSessions) === agentSessionId) {
          nextStreams[projectId] = runtimeStream
        }

        return nextStreams
      })

      const currentProject = activeProjectRef.current
      if (!currentProject || currentProject.id !== projectId) {
        return null
      }

      const selectedSessionId = selectAgentSessionId(currentProject.agentSessions)
      if (selectedSessionId !== agentSessionId) {
        return currentProject
      }

      const runtimeRunRecords = runtimeRun
        ? { ...runtimeRunsRef.current, [projectId]: runtimeRun }
        : removeProjectRecord(runtimeRunsRef.current, projectId)
      runtimeRunsRef.current = runtimeRunRecords
      setRuntimeRuns(runtimeRunRecords)

      const autonomousRunRecords = autonomousRun
        ? { ...autonomousRunsRef.current, [projectId]: autonomousRun }
        : removeProjectRecord(autonomousRunsRef.current, projectId)
      autonomousRunsRef.current = autonomousRunRecords
      setAutonomousRuns(autonomousRunRecords)

      const nextProject = applyAutonomousRunState(
        applyRuntimeRun(currentProject, runtimeRun),
        autonomousRun,
      )
      activeProjectRef.current = nextProject
      projectDetailsRef.current[projectId] = nextProject
      setActiveProject(nextProject)
      return nextProject
    },
    [],
  )

  const optimisticallySelectAgentSession = useCallback(
    (agentSessionId: string) => {
      const currentProject = activeProjectRef.current
      const projectId = activeProjectIdRef.current
      if (!currentProject || !projectId || currentProject.id !== projectId) {
        return null
      }

      const nextSelection = selectAgentSessionInProject(currentProject, agentSessionId)
      if (nextSelection === currentProject) {
        return { projectId, previousProject: currentProject }
      }

      latestLoadRequestRef.current += 1
      setIsProjectLoading(false)
      setRefreshSource('selection')
      setErrorMessage(null)
      setRuntimeRunActionError(null)
      setPendingRuntimeRunAction(null)
      setRuntimeRunActionStatus('idle')
      setAutonomousRunActionError(null)
      setPendingAutonomousRunAction(null)
      setAutonomousRunActionStatus('idle')

      const cacheKey = createAgentSessionStateKey(projectId, agentSessionId)
      const cachedRuntimeRun = hasOwnRecord(runtimeRunsBySessionRef.current, cacheKey)
        ? runtimeRunsBySessionRef.current[cacheKey]
        : null
      const cachedAutonomousRun = hasOwnRecord(autonomousRunsBySessionRef.current, cacheKey)
        ? autonomousRunsBySessionRef.current[cacheKey]
        : null
      const cachedRuntimeStream = runtimeStreamsBySessionRef.current[cacheKey] ?? null
      const nextRuntimeStream =
        cachedRuntimeStream && (!cachedRuntimeRun || cachedRuntimeStream.runId === cachedRuntimeRun.runId)
          ? cachedRuntimeStream
          : null
      const nextProject = applyAutonomousRunState(
        applyRuntimeRun(nextSelection, cachedRuntimeRun ?? null),
        cachedAutonomousRun ?? null,
      )

      activeProjectRef.current = nextProject
      projectDetailsRef.current[projectId] = nextProject
      setActiveProject(nextProject)
      applyAgentSessionRuntimeState(
        projectId,
        agentSessionId,
        cachedRuntimeRun ?? null,
        cachedAutonomousRun ?? null,
        nextRuntimeStream,
      )

      return { projectId, previousProject: currentProject }
    },
    [applyAgentSessionRuntimeState],
  )

  const applyAgentSessionSelection = useCallback((agentSession: AgentSessionView) => {
    const currentProject = activeProjectRef.current
    if (!currentProject || currentProject.id !== agentSession.projectId) {
      return null
    }

    const nextProject = selectAgentSessionInProject(
      currentProject,
      agentSession.agentSessionId,
      agentSession,
    )
    activeProjectRef.current = nextProject
    projectDetailsRef.current[nextProject.id] = nextProject
    setActiveProject(nextProject)
    return nextProject
  }, [])

  const rollbackAgentSessionSelection = useCallback(
    (previousProject: ProjectDetailView | null) => {
      if (!previousProject || activeProjectIdRef.current !== previousProject.id) {
        return
      }

      const agentSessionId = previousProject.selectedAgentSessionId
      const cacheKey = createAgentSessionStateKey(previousProject.id, agentSessionId)
      const cachedRuntimeStream = runtimeStreamsBySessionRef.current[cacheKey] ?? null

      activeProjectRef.current = previousProject
      projectDetailsRef.current[previousProject.id] = previousProject
      setActiveProject(previousProject)
      applyAgentSessionRuntimeState(
        previousProject.id,
        agentSessionId,
        previousProject.runtimeRun ?? null,
        previousProject.autonomousRun ?? null,
        cachedRuntimeStream,
      )
    },
    [applyAgentSessionRuntimeState],
  )

  const hydrateAgentSessionRuntimeState = useCallback(
    async (
      projectId: string,
      agentSessionId: string,
      options: { force?: boolean } = {},
    ): Promise<ProjectDetailView | null> => {
      const cacheKey = createAgentSessionStateKey(projectId, agentSessionId)
      const hasRuntimeRunCache = hasOwnRecord(runtimeRunsBySessionRef.current, cacheKey)
      const hasAutonomousRunCache = hasOwnRecord(autonomousRunsBySessionRef.current, cacheKey)
      if (
        !options.force &&
        hasRuntimeRunCache &&
        hasAutonomousRunCache
      ) {
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      }

      const inFlight = agentSessionRuntimePrefetchInFlightRef.current[cacheKey]
      if (inFlight) {
        await inFlight
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      }

      const requestPromise = (async () => {
        const previousRuntimeRun = hasRuntimeRunCache
          ? runtimeRunsBySessionRef.current[cacheKey]
          : null
        const previousAutonomousRun = hasAutonomousRunCache
          ? autonomousRunsBySessionRef.current[cacheKey]
          : null
        const [runtimeRunResult, autonomousRunResult] = await Promise.all([
          adapter
            .getRuntimeRun(projectId, agentSessionId)
            .then((response) => {
              const runtimeRun = response ? mapRuntimeRun(response) : null
              return {
                runtimeRun: runtimeRun?.agentSessionId === agentSessionId ? runtimeRun : null,
                error: null as string | null,
              }
            })
            .catch((error) => ({
              runtimeRun: previousRuntimeRun,
              error: getDesktopErrorMessage(error),
            })),
          adapter
            .getAutonomousRun(projectId, agentSessionId)
            .then((response) => {
              const autonomousRun = mapAutonomousRunInspection(response).autonomousRun
              return {
                autonomousRun: autonomousRun?.agentSessionId === agentSessionId ? autonomousRun : null,
                error: null as string | null,
              }
            })
            .catch((error) => ({
              autonomousRun: previousAutonomousRun,
              error: getDesktopErrorMessage(error),
            })),
        ])

        runtimeRunsBySessionRef.current[cacheKey] = runtimeRunResult.runtimeRun
        autonomousRunsBySessionRef.current[cacheKey] = autonomousRunResult.autonomousRun ?? null

        if (
          activeProjectIdRef.current !== projectId ||
          selectAgentSessionId(activeProjectRef.current?.agentSessions) !== agentSessionId
        ) {
          return
        }

        const cachedRuntimeStream = runtimeStreamsBySessionRef.current[cacheKey] ?? null
        const nextRuntimeStream =
          cachedRuntimeStream &&
          (!runtimeRunResult.runtimeRun || cachedRuntimeStream.runId === runtimeRunResult.runtimeRun.runId)
            ? cachedRuntimeStream
            : null
        applyAgentSessionRuntimeState(
          projectId,
          agentSessionId,
          runtimeRunResult.runtimeRun,
          autonomousRunResult.autonomousRun ?? null,
          nextRuntimeStream,
        )
        setRuntimeRunLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: runtimeRunResult.error,
        }))
        setAutonomousRunLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: autonomousRunResult.error,
        }))
        if (runtimeRunResult.error || autonomousRunResult.error) {
          setErrorMessage(
            [runtimeRunResult.error, autonomousRunResult.error]
              .filter((message): message is string => Boolean(message))
              .join('\n'),
          )
        } else {
          setErrorMessage(null)
        }
      })()
        .catch(() => undefined)
        .finally(() => {
          if (agentSessionRuntimePrefetchInFlightRef.current[cacheKey] === requestPromise) {
            delete agentSessionRuntimePrefetchInFlightRef.current[cacheKey]
          }
        })

      agentSessionRuntimePrefetchInFlightRef.current[cacheKey] = requestPromise
      await requestPromise
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [adapter, applyAgentSessionRuntimeState],
  )

  const prefetchProject = useCallback(
    (projectId: string) => {
      const trimmedProjectId = projectId.trim()
      if (
        !trimmedProjectId ||
        trimmedProjectId === activeProjectIdRef.current ||
        projectDetailsRef.current[trimmedProjectId] ||
        projectPrefetchInFlightRef.current[trimmedProjectId]
      ) {
        return
      }

      const requestPromise = (async () => {
        const statusPromise = adapter
          .getRepositoryStatus(trimmedProjectId)
          .then(mapRepositoryStatus)
          .catch(() => null)
        const snapshotResponse = await adapter.getProjectSnapshot(trimmedProjectId)
        const dispatches =
          notificationDispatchesRef.current[trimmedProjectId] ??
          snapshotResponse.notificationDispatches ??
          []
        const snapshotProject = mapProjectSnapshot(snapshotResponse, {
          notificationDispatches: dispatches,
        })
        const cachedRuntime = runtimeSessionsRef.current[trimmedProjectId] ?? null
        const cachedRuntimeRun = runtimeRunsRef.current[trimmedProjectId] ?? null
        const cachedAutonomousRun =
          autonomousRunsRef.current[trimmedProjectId] ?? snapshotProject.autonomousRun ?? null
        const prefetchedProject = applyAutonomousRunState(
          applyRuntimeRun(
            applyRuntimeSession(snapshotProject, cachedRuntime),
            cachedRuntimeRun,
          ),
          cachedAutonomousRun,
        )

        projectDetailsRef.current[trimmedProjectId] = prefetchedProject

        const status = await statusPromise
        if (status) {
          projectDetailsRef.current[trimmedProjectId] = applyRepositoryStatus(
            projectDetailsRef.current[trimmedProjectId] ?? prefetchedProject,
            status,
          )
        }
      })()
        .catch(() => undefined)
        .finally(() => {
          if (projectPrefetchInFlightRef.current[trimmedProjectId] === requestPromise) {
            delete projectPrefetchInFlightRef.current[trimmedProjectId]
          }
        })

      projectPrefetchInFlightRef.current[trimmedProjectId] = requestPromise
    },
    [adapter],
  )

  useEffect(() => {
    if (isLoading || projects.length < 2 || typeof window === 'undefined') {
      return
    }

    const candidates = projects
      .map((project) => project.id)
      .filter((projectId) => projectId !== activeProjectId && !projectDetailsRef.current[projectId])
    if (candidates.length === 0) {
      return
    }

    const win = window as IdleWindow

    let cancelled = false
    let scheduledHandle: DeferredTaskHandle | null = null

    function runNext() {
      scheduledHandle = null
      if (cancelled) {
        return
      }

      const nextProjectId = candidates.shift()
      if (!nextProjectId) {
        return
      }

      const inFlight = projectPrefetchInFlightRef.current[nextProjectId] ?? null
      prefetchProject(nextProjectId)
      void (inFlight ?? projectPrefetchInFlightRef.current[nextProjectId] ?? Promise.resolve()).finally(scheduleNext)
    }

    function scheduleNext() {
      if (cancelled) {
        return
      }

      scheduledHandle = scheduleDeferredTask(win, runNext, { timeoutMs: 1_500 })
    }

    scheduleNext()

    return () => {
      cancelled = true
      if (scheduledHandle === null) {
        return
      }

      cancelDeferredTask(win, scheduledHandle)
    }
  }, [activeProjectId, isLoading, prefetchProject, projects])

  useEffect(() => {
    if (!activeProjectId || !activeProject || isLoading || isProjectLoading || typeof window === 'undefined') {
      return
    }

    const warmProjectId = activeProjectId
    const candidates = activeProject.agentSessions
      .filter((session) => session.isActive)
      .map((session) => session.agentSessionId)
      .filter((agentSessionId) => {
        const cacheKey = createAgentSessionStateKey(warmProjectId, agentSessionId)
        return (
          !hasOwnRecord(runtimeRunsBySessionRef.current, cacheKey) ||
          !hasOwnRecord(autonomousRunsBySessionRef.current, cacheKey)
        )
      })

    if (candidates.length === 0) {
      return
    }

    const win = window as IdleWindow

    let cancelled = false
    let scheduledHandle: DeferredTaskHandle | null = null

    function runNext() {
      scheduledHandle = null
      if (cancelled) {
        return
      }

      const nextAgentSessionId = candidates.shift()
      if (!nextAgentSessionId) {
        return
      }

      void hydrateAgentSessionRuntimeState(warmProjectId, nextAgentSessionId).finally(scheduleNext)
    }

    function scheduleNext() {
      if (cancelled) {
        return
      }

      scheduledHandle = scheduleDeferredTask(win, runNext, { timeoutMs: 1_500 })
    }

    scheduleNext()

    return () => {
      cancelled = true
      if (scheduledHandle === null) {
        return
      }

      cancelDeferredTask(win, scheduledHandle)
    }
  }, [activeProject, activeProjectId, hydrateAgentSessionRuntimeState, isLoading, isProjectLoading])

  const refreshProviderModelCatalog = useCallback(
    async (profileId: string, options: { force?: boolean } = {}): Promise<ProviderModelCatalogDto> => {
      const trimmedProfileId = profileId.trim()
      const requestDependencyKey = `missing:${trimmedProfileId}`
      const requestKey = `${options.force ? 'force' : 'cached'}:${requestDependencyKey}`
      const inFlight = providerModelCatalogLoadInFlightRef.current[trimmedProfileId]
      if (inFlight && inFlight.requestKey === requestKey) {
        return inFlight.promise
      }

      const cachedCatalog = providerModelCatalogsRef.current[trimmedProfileId] ?? null
      const cachedStatus = providerModelCatalogLoadStatusesRef.current[trimmedProfileId] ?? 'idle'
      if (!options.force && cachedCatalog && cachedStatus === 'ready') {
        return cachedCatalog
      }

      const requestId = (providerModelCatalogLoadRequestRef.current[trimmedProfileId] ?? 0) + 1
      providerModelCatalogLoadRequestRef.current[trimmedProfileId] = requestId

      setProviderModelCatalogLoadStatuses((currentStatuses) => ({
        ...currentStatuses,
        [trimmedProfileId]: 'loading',
      }))
      setProviderModelCatalogLoadErrors((currentErrors) => ({
        ...currentErrors,
        [trimmedProfileId]: null,
      }))

      const loadPromise = (async () => {
        try {
          const response = await adapter.getProviderModelCatalog(trimmedProfileId, {
            forceRefresh: options.force ?? false,
          })

          if (providerModelCatalogLoadRequestRef.current[trimmedProfileId] !== requestId) {
            return response
          }

          setProviderModelCatalogs((currentCatalogs) => ({
            ...currentCatalogs,
            [trimmedProfileId]: response,
          }))
          setProviderModelCatalogLoadStatuses((currentStatuses) => ({
            ...currentStatuses,
            [trimmedProfileId]: 'ready',
          }))
          setProviderModelCatalogLoadErrors((currentErrors) => ({
            ...currentErrors,
            [trimmedProfileId]: null,
          }))
          return response
        } catch (error) {
          if (providerModelCatalogLoadRequestRef.current[trimmedProfileId] === requestId) {
            setProviderModelCatalogLoadStatuses((currentStatuses) => ({
              ...currentStatuses,
              [trimmedProfileId]: 'error',
            }))
            setProviderModelCatalogLoadErrors((currentErrors) => ({
              ...currentErrors,
              [trimmedProfileId]: getOperatorActionError(
                error,
                `Xero could not load the provider-model catalog for profile \`${trimmedProfileId}\`.`,
              ),
            }))
          }

          throw error
        } finally {
          const activeLoad = providerModelCatalogLoadInFlightRef.current[trimmedProfileId]
          if (activeLoad?.requestKey === requestKey) {
            delete providerModelCatalogLoadInFlightRef.current[trimmedProfileId]
          }
        }
      })()

      providerModelCatalogLoadInFlightRef.current[trimmedProfileId] = {
        requestKey,
        promise: loadPromise,
      }
      return loadPromise
    },
    [adapter],
  )

  const checkProviderProfile = useCallback(
    async (
      profileId: string,
      options: { includeNetwork?: boolean } = {},
    ): Promise<ProviderProfileDiagnosticsDto> => {
      const trimmedProfileId = profileId.trim()
      const response = await adapter.checkProviderProfile(trimmedProfileId, {
        includeNetwork: options.includeNetwork ?? true,
      })

      const modelCatalog = response.modelCatalog
      if (modelCatalog) {
        setProviderModelCatalogs((currentCatalogs) => ({
          ...currentCatalogs,
          [response.profileId]: modelCatalog,
        }))
        setProviderModelCatalogLoadStatuses((currentStatuses) => ({
          ...currentStatuses,
          [response.profileId]: 'ready',
        }))
        setProviderModelCatalogLoadErrors((currentErrors) => ({
          ...currentErrors,
          [response.profileId]: null,
        }))
      }

      return response
    },
    [adapter],
  )

  const runDoctorReport = useCallback(
    async (request: Partial<RunDoctorReportRequestDto> = {}): Promise<XeroDoctorReportDto> => {
      setDoctorReportStatus('running')
      setDoctorReportError(null)

      try {
        const report = await adapter.runDoctorReport({
          mode: request.mode ?? 'quick_local',
        })
        setDoctorReport(report)
        setDoctorReportStatus('ready')
        return report
      } catch (error) {
        const nextError = getOperatorActionError(
          error,
          'Xero could not generate the doctor report.',
        )
        setDoctorReportError(nextError)
        setDoctorReportStatus('error')
        throw error
      }
    },
    [adapter],
  )

  const loadProject = useCallback(
    async (projectId: string, source: ProjectLoadSource) =>
      loadProjectState({
        adapter,
        projectId,
        source,
        refs: {
          latestLoadRequestRef,
          projectDetailsRef,
          runtimeSessionsRef,
          runtimeRunsRef,
          autonomousRunsRef,
          notificationSyncSummariesRef,
          notificationDispatchesRef,
          notificationRoutesRef,
        },
        setters: {
          setProjects,
          setActiveProject,
          setActiveProjectId,
          setRepositoryStatus,
          setRuntimeSessions,
          setRuntimeRuns,
          setAutonomousRuns,
          setNotificationSyncSummaries,
          setNotificationSyncErrors,
          setRuntimeLoadErrors,
          setRuntimeRunLoadErrors,
          setAutonomousRunLoadErrors,
          setIsProjectLoading,
          setRefreshSource,
          setErrorMessage,
          setOperatorActionError,
          setPendingOperatorActionId,
          setOperatorActionStatus,
          setRuntimeRunActionError,
          setPendingRuntimeRunAction,
          setRuntimeRunActionStatus,
          setAutonomousRunActionError,
          setPendingAutonomousRunAction,
          setAutonomousRunActionStatus,
          setNotificationRouteMutationError,
        },
        resetRepositoryDiffs,
        loadNotificationRoutes,
        getOperatorActionError,
      }),
    [adapter, loadNotificationRoutes, resetRepositoryDiffs],
  )

  const scheduleRuntimeMetadataRefresh = useCallback(
    (projectId: string, source: RuntimeMetadataRefreshSource) => {
      scheduleRuntimeMetadataRefreshHelper({
        projectId,
        source,
        refs: {
          activeProjectIdRef,
          pendingRuntimeRefreshRef,
          runtimeRefreshTimeoutRef,
        },
        loadProject,
      })
    },
    [loadProject],
  )

  useEffect(() => {
    return () => {
      clearRuntimeMetadataRefresh({
        pendingRuntimeRefreshRef,
        runtimeRefreshTimeoutRef,
      })
    }
  }, [])

  const bootstrap = useCallback(async (source: 'startup' | 'remove' = 'startup') => {
    setIsLoading(true)
    setRefreshSource(source)
    setErrorMessage(null)

    try {
      const response = await adapter.listProjects()
      const nextProjects = response.projects.map(mapProjectSummary)
      setProjects(nextProjects)

      if (nextProjects.length === 0) {
        setActiveProjectId(null)
        setActiveProject(null)
        setRepositoryStatus(null)
        setRuntimeRuns({})
        setAutonomousRuns({})
        setNotificationRoutes({})
        setNotificationRouteLoadStatuses({})
        setNotificationRouteLoadErrors({})
        setNotificationSyncSummaries({})
        setNotificationSyncErrors({})
        setNotificationRouteMutationStatus('idle')
        setPendingNotificationRouteId(null)
        setNotificationRouteMutationError(null)
        setRuntimeStreams({})
        setRuntimeLoadErrors({})
        setRuntimeRunLoadErrors({})
        setAutonomousRunLoadErrors({})
        trustSnapshotRef.current = {}
        resetRepositoryDiffs(null)
        return
      }

      const preferredProjectId = activeProjectIdRef.current
      const nextProjectId =
        preferredProjectId && nextProjects.some((project) => project.id === preferredProjectId)
          ? preferredProjectId
          : nextProjects[0].id

      await loadProject(nextProjectId, source)
    } catch (error) {
      setErrorMessage(getDesktopErrorMessage(error))
    } finally {
      setIsLoading(false)
    }
  }, [adapter, loadProject, resetRepositoryDiffs])

  useEffect(() => {
    let disposeListeners: () => void = () => {}
    let effectDisposed = false

    void bootstrap()

    void attachDesktopRuntimeListeners({
      adapter,
      refs: {
        activeProjectIdRef,
        runtimeSessionsRef,
        runtimeRunRefreshKeyRef,
        repositoryStatusSyncKeyRef,
      },
      setters: {
        setProjects,
        setRefreshSource,
        setRepositoryStatus,
        setActiveProject,
        setRuntimeSessions,
        setRuntimeLoadErrors,
        setRuntimeStreams,
        setErrorMessage,
      },
      handleAdapterEventError,
      applyRuntimeRunUpdate,
      loadProject,
      resetRepositoryDiffs,
      scheduleRuntimeMetadataRefresh,
    }).then((nextDispose) => {
      if (effectDisposed) {
        nextDispose()
        return
      }

      disposeListeners = nextDispose
    })

    return () => {
      effectDisposed = true
      disposeListeners()
    }
  }, [adapter, applyRuntimeRunUpdate, bootstrap, handleAdapterEventError, loadProject, resetRepositoryDiffs, scheduleRuntimeMetadataRefresh])

  useEffect(() => {
    if (!activeProjectId || typeof window === 'undefined' || typeof document === 'undefined') {
      return
    }

    const refreshIfVisible = () => {
      if (document.visibilityState === 'hidden') {
        return
      }

      void syncRepositoryStatus()
    }

    const pollHandle = window.setInterval(refreshIfVisible, REPOSITORY_STATUS_POLL_MS)
    window.addEventListener('focus', refreshIfVisible)
    document.addEventListener('visibilitychange', refreshIfVisible)

    return () => {
      window.clearInterval(pollHandle)
      window.removeEventListener('focus', refreshIfVisible)
      document.removeEventListener('visibilitychange', refreshIfVisible)
    }
  }, [activeProjectId, syncRepositoryStatus])

  // Fetch the active project's usage summary on mount and whenever the
  // selected project changes. Runs even on the first render so the footer
  // populates without waiting for an agent run to complete.
  useEffect(() => {
    if (!activeProjectId) {
      return
    }
    void refreshUsageSummary(activeProjectId)
  }, [activeProjectId, refreshUsageSummary])

  // Live-refresh totals when the provider loop persists a usage row. We only
  // trigger a re-fetch for the project that emitted (no-op for others).
  useEffect(() => {
    let unlisten: (() => void) | null = null
    let cancelled = false

    void adapter
      .onAgentUsageUpdated(
        (payload) => {
          void refreshUsageSummary(payload.projectId)
        },
        handleAdapterEventError,
      )
      .then((dispose) => {
        if (cancelled) {
          dispose()
          return
        }
        unlisten = dispose
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [adapter, handleAdapterEventError, refreshUsageSummary])

  const showRepositoryDiff = useCallback(
    async (scope: RepositoryDiffScope, options: { force?: boolean } = {}) => {
      setActiveDiffScope(scope)

      const projectId = activeProjectIdRef.current
      if (!projectId) {
        return
      }

      const currentDiffState = repositoryDiffsRef.current[scope]
      if (
        !options.force &&
        currentDiffState.projectId === projectId &&
        (currentDiffState.status === 'ready' || currentDiffState.status === 'loading')
      ) {
        return
      }

      const requestId = latestDiffRequestRef.current[scope] + 1
      latestDiffRequestRef.current[scope] = requestId

      setRepositoryDiffs((currentDiffs) => ({
        ...currentDiffs,
        [scope]: {
          ...currentDiffs[scope],
          status: 'loading',
          errorMessage: null,
          projectId,
        },
      }))

      try {
        const response = await adapter.getRepositoryDiff(projectId, scope)
        if (activeProjectIdRef.current !== projectId || latestDiffRequestRef.current[scope] !== requestId) {
          return
        }

        const nextDiff = mapRepositoryDiff(response)
        setRepositoryDiffs((currentDiffs) => ({
          ...currentDiffs,
          [scope]: {
            status: 'ready',
            diff: nextDiff,
            errorMessage: null,
            projectId,
          },
        }))
      } catch (error) {
        if (activeProjectIdRef.current !== projectId || latestDiffRequestRef.current[scope] !== requestId) {
          return
        }

        const nextMessage = getDesktopErrorMessage(error)
        setRepositoryDiffs((currentDiffs) => ({
          ...currentDiffs,
          [scope]: {
            ...currentDiffs[scope],
            status: 'error',
            errorMessage: nextMessage,
            projectId,
          },
        }))
      }
    },
    [adapter],
  )

  const selectProject = useCallback(
    async (projectId: string) => {
      if (projectId === activeProjectIdRef.current && !errorMessage) {
        return
      }

      const requestId = projectSelectionRequestRef.current + 1
      projectSelectionRequestRef.current = requestId
      setPendingProjectSelectionId(projectId)

      const cachedProject = projectDetailsRef.current[projectId] ?? null
      if (cachedProject) {
        setRepositoryStatus(cachedProject.repositoryStatus)
        setActiveProjectId(projectId)
        setActiveProject(cachedProject)
        resetRepositoryDiffs(cachedProject.repositoryStatus)
      } else {
        const projectSummary = projects.find((project) => project.id === projectId)
        if (projectSummary) {
          const projectShell = createProjectShell(projectSummary)
          projectDetailsRef.current[projectId] = projectShell
          setRepositoryStatus(null)
          setActiveProjectId(projectId)
          setActiveProject(projectShell)
          resetRepositoryDiffs(null)
        }
      }

      await waitForProjectSelectionPaint()

      try {
        await loadProject(projectId, 'selection')
      } finally {
        if (projectSelectionRequestRef.current === requestId) {
          setPendingProjectSelectionId(null)
        }
      }
    },
    [errorMessage, loadProject, projects, resetRepositoryDiffs],
  )

  const retry = useCallback(async () => {
    if (activeProjectIdRef.current) {
      const projectId = activeProjectIdRef.current
      delete runtimeActionRefreshKeysRef.current[projectId]
      delete runtimeRunRefreshKeyRef.current[projectId]
      await loadProject(projectId, 'selection')
      setRuntimeStreamRetryToken((current) => current + 1)
      return
    }

    await bootstrap()
  }, [bootstrap, loadProject])

  const retryActiveRepositoryDiff = useCallback(async () => {
    await showRepositoryDiff(activeDiffScope, { force: true })
  }, [activeDiffScope, showRepositoryDiff])

  const {
    importProject,
    createProject,
    removeProject,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    completeOAuthCallback,
    refreshMcpRegistry,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    refreshSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    refreshNotificationRoutes,
    upsertNotificationRoute,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
  } = useXeroDesktopMutations({
    adapter,
    refs: {
      activeProjectIdRef,
      activeProjectRef,
      runtimeRunsRef,
      providerCredentialsRef,
      providerCredentialsLoadInFlightRef,
      mcpRegistryRef,
      mcpRegistryLoadInFlightRef,
      skillRegistryRef,
      skillRegistryLoadInFlightRef,
    },
    setters: {
      setProjects,
      setIsImporting,
      setProjectRemovalStatus,
      setPendingProjectRemovalId,
      setRefreshSource,
      setErrorMessage,
      setOperatorActionStatus,
      setPendingOperatorActionId,
      setOperatorActionError,
      setAutonomousRunActionStatus,
      setPendingAutonomousRunAction,
      setAutonomousRunActionError,
      setRuntimeRunActionStatus,
      setPendingRuntimeRunAction,
      setRuntimeRunActionError,
      setNotificationRoutes,
      setNotificationRouteLoadStatuses,
      setNotificationRouteLoadErrors,
      setNotificationRouteMutationStatus,
      setPendingNotificationRouteId,
      setNotificationRouteMutationError,
      setProviderCredentials,
      setProviderCredentialsLoadStatus,
      setProviderCredentialsLoadError,
      setProviderCredentialsSaveStatus,
      setProviderCredentialsSaveError,
      setMcpRegistry,
      setMcpImportDiagnostics,
      setMcpRegistryLoadStatus,
      setMcpRegistryLoadError,
      setMcpRegistryMutationStatus,
      setPendingMcpServerId,
      setMcpRegistryMutationError,
      setSkillRegistry,
      setSkillRegistryLoadStatus,
      setSkillRegistryLoadError,
      setSkillRegistryMutationStatus,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
    },
    operations: {
      bootstrap,
      loadProject,
      loadNotificationRoutes,
      syncRuntimeSession,
      syncRuntimeRun,
      syncAutonomousRun,
      optimisticallySelectAgentSession,
      applyAgentSessionSelection,
      rollbackAgentSessionSelection,
      hydrateAgentSessionRuntimeState,
      applyRuntimeSessionUpdate,
      applyRuntimeRunUpdate,
      applyAutonomousRunStateUpdate,
    },
    providerCredentialsLoadStatus,
    mcpRegistryLoadStatus,
    skillRegistryLoadStatus,
  })

  const spawnPane = useCallback(async (): Promise<AgentWorkspaceLayoutState | null> => {
    const projectId = activeProjectIdRef.current
    const currentProject = activeProjectRef.current
    if (!projectId || !currentProject || currentProject.id !== projectId) {
      return null
    }

    const currentLayout = reconcileAgentWorkspaceLayout(
      currentProject,
      agentWorkspaceLayoutsRef.current[projectId],
    )
    const reusablePaneIndex = currentLayout.paneSlots.findIndex(
      (slot) => !slot.agentSessionId && !pendingSpawnPaneIdsRef.current.has(slot.id),
    )
    if (currentLayout.paneSlots.length >= AGENT_WORKSPACE_MAX_PANES && reusablePaneIndex < 0) {
      return currentLayout
    }

    const paneId = createAgentWorkspacePaneId()
    const targetPaneId =
      reusablePaneIndex >= 0 ? currentLayout.paneSlots[reusablePaneIndex]?.id ?? paneId : paneId
    const pendingPaneSlots =
      reusablePaneIndex >= 0
        ? currentLayout.paneSlots
        : [
            ...currentLayout.paneSlots,
            { id: paneId, agentSessionId: null },
          ]
    const pendingLayout: AgentWorkspaceLayoutState = {
      ...currentLayout,
      paneSlots: pendingPaneSlots,
      focusedPaneId: targetPaneId,
    }
    const pendingLayouts = {
      ...agentWorkspaceLayoutsRef.current,
      [projectId]: pendingLayout,
    }

    agentWorkspaceLayoutsRef.current = pendingLayouts
    pendingSpawnPaneIdsRef.current.add(targetPaneId)
    setAgentWorkspaceLayouts(pendingLayouts)

    let createdSession: AgentSessionView
    try {
      createdSession = {
        ...mapAgentSession(
          await adapter.createAgentSession({
            projectId,
            title: null,
            summary: '',
            selected: false,
          }),
        ),
        selected: false,
      }
    } catch (error) {
      pendingSpawnPaneIdsRef.current.delete(targetPaneId)
      if (reusablePaneIndex < 0) {
        setAgentWorkspaceLayouts((currentLayouts) => {
          const activeProject = activeProjectRef.current
          if (!activeProject || activeProject.id !== projectId) {
            return currentLayouts
          }

          const layout = reconcileAgentWorkspaceLayout(activeProject, currentLayouts[projectId])
          const targetSlot = layout.paneSlots.find((slot) => slot.id === targetPaneId)
          if (!targetSlot || targetSlot.agentSessionId) {
            return currentLayouts
          }

          const paneIndex = layout.paneSlots.findIndex((slot) => slot.id === targetPaneId)
          const paneSlots = layout.paneSlots.filter((slot) => slot.id !== targetPaneId)
          const nextLayout =
            paneSlots.length > 0
              ? {
                  ...layout,
                  paneSlots,
                  focusedPaneId:
                    layout.focusedPaneId === targetPaneId
                      ? paneSlots[Math.max(0, paneIndex - 1)]?.id ?? paneSlots[0].id
                      : layout.focusedPaneId,
                }
              : createDefaultAgentWorkspaceLayout(activeProject)
          const nextLayouts = {
            ...currentLayouts,
            [projectId]: nextLayout,
          }
          agentWorkspaceLayoutsRef.current = nextLayouts
          return nextLayouts
        })
      }
      setErrorMessage(getDesktopErrorMessage(error))
      throw error
    }
    pendingSpawnPaneIdsRef.current.delete(targetPaneId)

    setActiveProject((project) => {
      if (!project || project.id !== projectId) {
        return project
      }
      const nextProject = applyAgentSessionToProject(project, createdSession)
      activeProjectRef.current = nextProject
      projectDetailsRef.current[projectId] = nextProject
      return nextProject
    })
    setAgentWorkspaceLayouts((currentLayouts) => {
      const project = activeProjectRef.current
      if (!project || project.id !== projectId) {
        return currentLayouts
      }

      const layout = reconcileAgentWorkspaceLayout(project, currentLayouts[projectId])
      if (!layout.paneSlots.some((slot) => slot.id === targetPaneId)) {
        return currentLayouts
      }

      const paneSlots = layout.paneSlots.map((slot) =>
        slot.id === targetPaneId
          ? { ...slot, agentSessionId: createdSession.agentSessionId }
          : slot,
      )
      const nextLayout: AgentWorkspaceLayoutState = {
        ...layout,
        paneSlots,
        focusedPaneId: targetPaneId,
      }
      const nextLayouts = {
        ...currentLayouts,
        [projectId]: nextLayout,
      }
      agentWorkspaceLayoutsRef.current = nextLayouts
      return nextLayouts
    })
    void hydrateAgentSessionRuntimeState(projectId, createdSession.agentSessionId).catch(() => undefined)

    return {
      ...pendingLayout,
      paneSlots: pendingLayout.paneSlots.map((slot) =>
        slot.id === targetPaneId
          ? { ...slot, agentSessionId: createdSession.agentSessionId }
          : slot,
      ),
    }
  }, [adapter, hydrateAgentSessionRuntimeState])

  const closePane = useCallback((paneId: string) => {
    const projectId = activeProjectIdRef.current
    const currentProject = activeProjectRef.current
    if (!projectId || !currentProject || currentProject.id !== projectId) {
      return
    }

    setAgentWorkspaceLayouts((currentLayouts) => {
      const currentLayout = reconcileAgentWorkspaceLayout(
        currentProject,
        currentLayouts[projectId],
      )
      if (currentLayout.paneSlots.length <= 1) {
        return currentLayouts
      }

      const paneIndex = currentLayout.paneSlots.findIndex((slot) => slot.id === paneId)
      if (paneIndex < 0) {
        return currentLayouts
      }

      const paneSlots = currentLayout.paneSlots.filter((slot) => slot.id !== paneId)
      const nextFocusedPaneId =
        currentLayout.focusedPaneId === paneId
          ? paneSlots[Math.max(0, paneIndex - 1)]?.id ?? paneSlots[0].id
          : currentLayout.focusedPaneId
      return {
        ...currentLayouts,
        [projectId]: {
          ...currentLayout,
          paneSlots,
          focusedPaneId: nextFocusedPaneId,
        },
      }
    })
  }, [])

  const focusPane = useCallback((paneId: string) => {
    const projectId = activeProjectIdRef.current
    const currentProject = activeProjectRef.current
    if (!projectId || !currentProject || currentProject.id !== projectId) {
      return
    }

    setAgentWorkspaceLayouts((currentLayouts) => {
      const currentLayout = reconcileAgentWorkspaceLayout(
        currentProject,
        currentLayouts[projectId],
      )
      if (
        currentLayout.focusedPaneId === paneId ||
        !currentLayout.paneSlots.some((slot) => slot.id === paneId)
      ) {
        return currentLayouts
      }

      return {
        ...currentLayouts,
        [projectId]: {
          ...currentLayout,
          focusedPaneId: paneId,
        },
      }
    })
  }, [])

  const setSplitterRatios = useCallback((arrangementKey: string, ratios: number[]) => {
    const key = arrangementKey.trim()
    const projectId = activeProjectIdRef.current
    const currentProject = activeProjectRef.current
    if (!projectId || !key || !currentProject || currentProject.id !== projectId) {
      return
    }

    const sanitizedRatios = ratios.filter((ratio) => Number.isFinite(ratio) && ratio > 0)
    if (sanitizedRatios.length !== ratios.length || sanitizedRatios.length === 0) {
      return
    }

    setAgentWorkspaceLayouts((currentLayouts) => {
      const currentLayout = reconcileAgentWorkspaceLayout(
        currentProject,
        currentLayouts[projectId],
      )
      return {
        ...currentLayouts,
        [projectId]: {
          ...currentLayout,
          splitterRatios: {
            ...currentLayout.splitterRatios,
            [key]: sanitizedRatios,
          },
        },
      }
    })
  }, [])

  useEffect(() => {
    if (providerCredentialsLoadStatus !== 'idle') {
      return
    }

    void refreshProviderCredentials().catch(() => undefined)
  }, [providerCredentialsLoadStatus, refreshProviderCredentials])

  useEffect(() => {
    if (providerCredentialsLoadStatus !== 'ready' || !providerCredentials) {
      return
    }

    for (const credential of providerCredentials.credentials) {
      const catalogKeys = getProviderModelCatalogStateKeys(credential.providerId)
      const hasCatalog = catalogKeys.some((key) => providerModelCatalogsRef.current[key])
      const hasActiveLoad = catalogKeys.some(
        (key) =>
          providerModelCatalogLoadStatusesRef.current[key] === 'loading' ||
          Boolean(providerModelCatalogLoadInFlightRef.current[key]),
      )

      if (hasCatalog || hasActiveLoad) {
        continue
      }

      void refreshProviderModelCatalog(
        getProviderModelCatalogRefreshId(credential.providerId),
      ).catch(() => undefined)
    }
  }, [providerCredentials, providerCredentialsLoadStatus, refreshProviderModelCatalog])

  useEffect(() => {
    if (mcpRegistryLoadStatus !== 'idle') {
      return
    }

    void refreshMcpRegistry().catch(() => undefined)
  }, [mcpRegistryLoadStatus, refreshMcpRegistry])

  useEffect(() => {
    if (skillRegistryLoadStatus !== 'idle') {
      return
    }

    void refreshSkillRegistry().catch(() => undefined)
  }, [refreshSkillRegistry, skillRegistryLoadStatus])

  useEffect(() => {
    if (skillRegistryLoadStatus !== 'ready') {
      return
    }

    const currentProjectId = skillRegistryRef.current?.projectId ?? null
    if (currentProjectId === activeProjectId) {
      return
    }

    void refreshSkillRegistry({ force: true }).catch(() => undefined)
  }, [activeProjectId, refreshSkillRegistry, skillRegistryLoadStatus])

  const activeProviderModelCatalog: ProviderModelCatalogDto | null = null
  const activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus = 'idle'
  const activeProviderModelCatalogLoadError: OperatorActionErrorView | null = null

  const activeRuntimeSession = activeProjectId
    ? runtimeSessions[activeProjectId] ?? activeProject?.runtimeSession ?? null
    : null
  const activeAgentSessionId = activeProject ? selectAgentSessionId(activeProject.agentSessions) : null
  const activeRuntimeRunCandidate = activeProjectId ? runtimeRuns[activeProjectId] ?? activeProject?.runtimeRun ?? null : null
  const activeRuntimeRun =
    activeRuntimeRunCandidate?.agentSessionId === activeAgentSessionId ? activeRuntimeRunCandidate : null
  const activeAutonomousRunCandidate = activeProjectId
    ? autonomousRuns[activeProjectId] ?? activeProject?.autonomousRun ?? null
    : null
  const activeAutonomousRun =
    activeAutonomousRunCandidate?.agentSessionId === activeAgentSessionId ? activeAutonomousRunCandidate : null
  const activeAutonomousRunErrorMessage = activeProjectId ? autonomousRunLoadErrors[activeProjectId] ?? null : null
  const activeRuntimeSessionId = activeRuntimeSession?.sessionId ?? null
  const activeRuntimeSessionFlowId = activeRuntimeSession?.flowId ?? null
  const activeRuntimeSessionKind = activeRuntimeSession?.runtimeKind ?? null
  const activeRuntimeSessionAuthenticated = activeRuntimeSession?.isAuthenticated ?? false
  const activeRuntimeSubscriptionSession = useMemo(
    () => activeRuntimeSession,
    [
      activeRuntimeSessionAuthenticated,
      activeRuntimeSessionFlowId,
      activeRuntimeSessionId,
      activeRuntimeSessionKind,
    ],
  )
  const agentWorkspaceLayout = useMemo(
    () =>
      activeProject
        ? reconcileAgentWorkspaceLayout(activeProject, agentWorkspaceLayouts[activeProject.id])
        : null,
    [activeProject, agentWorkspaceLayouts],
  )
  const activeRuntimeSubscriptionTargets = useMemo(() => {
    if (
      !activeProjectId ||
      !activeProject ||
      !activeRuntimeSubscriptionSession?.isAuthenticated ||
      !activeRuntimeSubscriptionSession.sessionId
    ) {
      return []
    }

    const seenSessionIds = new Set<string>()
    const slots = agentWorkspaceLayout?.paneSlots ?? [
      { id: createDefaultAgentWorkspacePaneId(activeProjectId), agentSessionId: activeAgentSessionId },
    ]

    return slots.flatMap((slot) => {
      const agentSessionId = slot.agentSessionId
      if (!agentSessionId || seenSessionIds.has(agentSessionId)) {
        return []
      }
      seenSessionIds.add(agentSessionId)

      const cacheKey = createAgentSessionStateKey(activeProjectId, agentSessionId)
      const runtimeRun =
        agentSessionId === activeAgentSessionId
          ? activeRuntimeRun
          : runtimeRunsBySessionRef.current[cacheKey] ?? null
      const runId = runtimeRun?.runId ?? null
      if (!runId) {
        return []
      }

      return [{
        key: [
          activeProjectId,
          agentSessionId,
          activeRuntimeSubscriptionSession.runtimeKind,
          activeRuntimeSubscriptionSession.sessionId,
          activeRuntimeSubscriptionSession.flowId ?? 'none',
          runId,
          runtimeStreamRetryToken,
        ].join(':'),
        projectId: activeProjectId,
        agentSessionId,
        runId,
        runtimeSession: activeRuntimeSubscriptionSession,
      }]
    })
  }, [
    activeAgentSessionId,
    activeProject,
    activeProjectId,
    activeRuntimeRun,
    activeRuntimeSubscriptionSession,
    agentSessionRuntimeCacheRevision,
    agentWorkspaceLayout,
    runtimeStreamRetryToken,
  ])
  const activeRuntimeSubscriptionKey = activeRuntimeSubscriptionTargets
    .map((target) => target.key)
    .join('|')

  useEffect(() => {
    const cleanups = activeRuntimeSubscriptionTargets.map((target) =>
      attachRuntimeStreamSubscription({
        projectId: target.projectId,
        agentSessionId: target.agentSessionId,
        runtimeSession: target.runtimeSession,
        runId: target.runId,
        adapter,
        runtimeActionRefreshKeysRef,
        updateRuntimeStream,
        scheduleRuntimeMetadataRefresh,
      }),
    )

    return () => {
      for (const cleanup of cleanups) {
        cleanup()
      }
    }
  }, [
    activeRuntimeSubscriptionKey,
    adapter,
    scheduleRuntimeMetadataRefresh,
    updateRuntimeStream,
  ])

  useEffect(() => {
    const previous = previousRuntimeAuthRef.current
    const next: Record<string, boolean> = {}
    let shouldRefresh = false
    for (const [projectId, session] of Object.entries(runtimeSessions)) {
      const authenticated = Boolean(session?.isAuthenticated)
      next[projectId] = authenticated
      if (authenticated && previous[projectId] === false) {
        shouldRefresh = true
      }
    }
    previousRuntimeAuthRef.current = next
    if (shouldRefresh) {
      void refreshProviderCredentials({ force: true }).catch(() => undefined)
    }
  }, [refreshProviderCredentials, runtimeSessions])

  const activePhase = useMemo(() => getActivePhase(activeProject), [activeProject])
  const activeRuntimeErrorMessage = activeProject ? runtimeLoadErrors[activeProject.id] ?? null : null
  const activeRuntimeRunErrorMessage = activeProject ? runtimeRunLoadErrors[activeProject.id] ?? null : null
  const activeRuntimeStreamCandidate =
    activeProject && activeAgentSessionId
      ? runtimeStreams[createRuntimeStreamStoreKey(activeProject.id, activeAgentSessionId)] ?? runtimeStreams[activeProject.id] ?? null
      : activeProject
        ? runtimeStreams[activeProject.id] ?? null
        : null
  const activeRuntimeStream =
    activeRuntimeStreamCandidate?.agentSessionId === activeAgentSessionId
      ? activeRuntimeStreamCandidate
      : null
  const activeNotificationRoutes = activeProject
    ? (notificationRoutes[activeProject.id] ?? []).filter(
        (route) => route.projectId === activeProject.id && route.routeId.trim().length > 0,
      )
    : []
  const activeNotificationRouteLoadStatus: NotificationRoutesLoadStatus = activeProject
    ? notificationRouteLoadStatuses[activeProject.id] ?? 'idle'
    : 'idle'
  const activeNotificationRouteLoadError = activeProject
    ? notificationRouteLoadErrors[activeProject.id] ?? null
    : null
  const activeNotificationSyncSummary = activeProject
    ? notificationSyncSummaries[activeProject.id] ?? null
    : null
  const activeNotificationSyncError = activeProject
    ? notificationSyncErrors[activeProject.id] ?? null
    : null
  const activeBlockedNotificationSyncPollTarget: BlockedNotificationSyncPollTarget | null = null

  const workflowView = useMemo<WorkflowPaneView | null>(
    () =>
      buildWorkflowView({
        project: activeProject,
        activePhase,
        providerCredentials,
        runtimeSession: activeRuntimeSession,
      }),
    [activePhase, activeProject, activeRuntimeSession, providerCredentials],
  )

  const agentViewProjection = useMemo(
    () =>
      buildAgentView({
        project: activeProject,
        activePhase,
        repositoryStatus,
        providerCredentials,
        runtimeSession: activeRuntimeSession,
        providerModelCatalogs,
        providerModelCatalogLoadStatuses,
        providerModelCatalogLoadErrors,
        activeProviderModelCatalog,
        activeProviderModelCatalogLoadStatus,
        activeProviderModelCatalogLoadError,
        runtimeRun: activeRuntimeRun,
        autonomousRun: activeAutonomousRun,
        runtimeErrorMessage: activeRuntimeErrorMessage,
        runtimeRunErrorMessage: activeRuntimeRunErrorMessage,
        autonomousRunErrorMessage: activeAutonomousRunErrorMessage,
        runtimeStream: activeRuntimeStream,
        notificationRoutes: activeNotificationRoutes,
        notificationRouteLoadStatus: activeNotificationRouteLoadStatus,
        notificationRouteError: activeNotificationRouteLoadError,
        notificationSyncSummary: activeNotificationSyncSummary,
        notificationSyncError: activeNotificationSyncError,
        blockedNotificationSyncPollTarget: activeBlockedNotificationSyncPollTarget,
        notificationRouteMutationStatus,
        pendingNotificationRouteId,
        notificationRouteMutationError,
        previousTrustSnapshot: activeProject ? trustSnapshotRef.current[activeProject.id] ?? null : null,
        operatorActionStatus,
        pendingOperatorActionId,
        operatorActionError,
        autonomousRunActionStatus,
        pendingAutonomousRunAction,
        autonomousRunActionError,
        runtimeRunActionStatus,
        pendingRuntimeRunAction,
        runtimeRunActionError,
      }),
    [
      activeNotificationRouteLoadError,
      activeNotificationRouteLoadStatus,
      activeNotificationRoutes,
      activeNotificationSyncError,
      activeNotificationSyncSummary,
      activePhase,
      activeProject,
      providerModelCatalogs,
      providerModelCatalogLoadErrors,
      providerModelCatalogLoadStatuses,
      activeProviderModelCatalog,
      activeProviderModelCatalogLoadError,
      activeProviderModelCatalogLoadStatus,
      activeAutonomousRun,
      activeAutonomousRunErrorMessage,
      activeBlockedNotificationSyncPollTarget,
      activeRuntimeErrorMessage,
      activeRuntimeRun,
      activeRuntimeRunErrorMessage,
      activeRuntimeSession,
      activeRuntimeStream,
      notificationRouteMutationError,
      notificationRouteMutationStatus,
      operatorActionError,
      operatorActionStatus,
      pendingAutonomousRunAction,
      pendingNotificationRouteId,
      pendingOperatorActionId,
      pendingRuntimeRunAction,
      repositoryStatus,
      autonomousRunActionError,
      autonomousRunActionStatus,
      runtimeRunActionError,
      runtimeRunActionStatus,
      providerCredentials,
    ],
  )
  const agentView = agentViewProjection.view
  const agentWorkspacePanes = useMemo<AgentWorkspacePaneView[]>(() => {
    if (!activeProject || !agentWorkspaceLayout) {
      return []
    }

    return agentWorkspaceLayout.paneSlots.flatMap<AgentWorkspacePaneView>((slot) => {
      if (!slot.agentSessionId) {
        const projection = buildAgentView({
          project: createEmptyAgentSessionProject(activeProject),
          activePhase,
          repositoryStatus,
          providerCredentials,
          runtimeSession: activeRuntimeSession,
          providerModelCatalogs,
          providerModelCatalogLoadStatuses,
          providerModelCatalogLoadErrors,
          activeProviderModelCatalog,
          activeProviderModelCatalogLoadStatus,
          activeProviderModelCatalogLoadError,
          runtimeRun: null,
          autonomousRun: null,
          runtimeErrorMessage: activeRuntimeErrorMessage,
          runtimeRunErrorMessage: null,
          autonomousRunErrorMessage: null,
          runtimeStream: null,
          notificationRoutes: activeNotificationRoutes,
          notificationRouteLoadStatus: activeNotificationRouteLoadStatus,
          notificationRouteError: activeNotificationRouteLoadError,
          notificationSyncSummary: activeNotificationSyncSummary,
          notificationSyncError: activeNotificationSyncError,
          blockedNotificationSyncPollTarget: activeBlockedNotificationSyncPollTarget,
          notificationRouteMutationStatus,
          pendingNotificationRouteId,
          notificationRouteMutationError,
          previousTrustSnapshot: null,
          operatorActionStatus,
          pendingOperatorActionId,
          operatorActionError,
          autonomousRunActionStatus,
          pendingAutonomousRunAction,
          autonomousRunActionError,
          runtimeRunActionStatus,
          pendingRuntimeRunAction,
          runtimeRunActionError,
        }).view

        return projection
          ? [{
              paneId: slot.id,
              agentSessionId: null,
              agent: projection,
            }]
          : []
      }

      if (slot.agentSessionId === activeAgentSessionId && agentView) {
        return [{
          paneId: slot.id,
          agentSessionId: slot.agentSessionId,
          agent: agentView,
        }]
      }

      const paneProject = selectAgentSessionInProject(activeProject, slot.agentSessionId)
      const cacheKey = createAgentSessionStateKey(activeProject.id, slot.agentSessionId)
      const cachedRuntimeRun = hasOwnRecord(runtimeRunsBySessionRef.current, cacheKey)
        ? runtimeRunsBySessionRef.current[cacheKey]
        : null
      const cachedAutonomousRun = hasOwnRecord(autonomousRunsBySessionRef.current, cacheKey)
        ? autonomousRunsBySessionRef.current[cacheKey]
        : null
      const cachedRuntimeStream = runtimeStreamsBySessionRef.current[cacheKey] ?? null
      const projection = buildAgentView({
        project: paneProject,
        activePhase,
        repositoryStatus,
        providerCredentials,
        runtimeSession: activeRuntimeSession,
        providerModelCatalogs,
        providerModelCatalogLoadStatuses,
        providerModelCatalogLoadErrors,
        activeProviderModelCatalog,
        activeProviderModelCatalogLoadStatus,
        activeProviderModelCatalogLoadError,
        runtimeRun: cachedRuntimeRun,
        autonomousRun: cachedAutonomousRun,
        runtimeErrorMessage: activeRuntimeErrorMessage,
        runtimeRunErrorMessage: activeRuntimeRunErrorMessage,
        autonomousRunErrorMessage: activeAutonomousRunErrorMessage,
        runtimeStream: cachedRuntimeStream,
        notificationRoutes: activeNotificationRoutes,
        notificationRouteLoadStatus: activeNotificationRouteLoadStatus,
        notificationRouteError: activeNotificationRouteLoadError,
        notificationSyncSummary: activeNotificationSyncSummary,
        notificationSyncError: activeNotificationSyncError,
        blockedNotificationSyncPollTarget: activeBlockedNotificationSyncPollTarget,
        notificationRouteMutationStatus,
        pendingNotificationRouteId,
        notificationRouteMutationError,
        previousTrustSnapshot: null,
        operatorActionStatus,
        pendingOperatorActionId,
        operatorActionError,
        autonomousRunActionStatus,
        pendingAutonomousRunAction,
        autonomousRunActionError,
        runtimeRunActionStatus,
        pendingRuntimeRunAction,
        runtimeRunActionError,
      }).view

      if (!projection) {
        return []
      }

      return [{
        paneId: slot.id,
        agentSessionId: slot.agentSessionId,
        agent: projection,
      }]
    })
  }, [
    activeAgentSessionId,
    activeAutonomousRunErrorMessage,
    activeBlockedNotificationSyncPollTarget,
    activeNotificationRouteLoadError,
    activeNotificationRouteLoadStatus,
    activeNotificationRoutes,
    activeNotificationSyncError,
    activeNotificationSyncSummary,
    activePhase,
    activeProject,
    activeProviderModelCatalog,
    activeProviderModelCatalogLoadError,
    activeProviderModelCatalogLoadStatus,
    activeRuntimeErrorMessage,
    activeRuntimeRunErrorMessage,
    activeRuntimeSession,
    agentView,
    agentWorkspaceLayout,
    autonomousRunActionError,
    autonomousRunActionStatus,
    notificationRouteMutationError,
    notificationRouteMutationStatus,
    operatorActionError,
    operatorActionStatus,
    pendingAutonomousRunAction,
    pendingNotificationRouteId,
    pendingOperatorActionId,
    pendingRuntimeRunAction,
    providerCredentials,
    providerModelCatalogLoadErrors,
    providerModelCatalogLoadStatuses,
    providerModelCatalogs,
    repositoryStatus,
    runtimeRunActionError,
    runtimeRunActionStatus,
  ])

  useEffect(() => {
    if (!activeProject || !agentViewProjection.trustSnapshot) {
      return
    }

    trustSnapshotRef.current[activeProject.id] = agentViewProjection.trustSnapshot
  }, [activeProject, agentViewProjection.trustSnapshot])

  const executionView = useMemo<ExecutionPaneView | null>(
    () =>
      buildExecutionView({
        project: activeProject,
        activePhase,
        repositoryStatus,
        operatorActionError,
      }),
    [activePhase, activeProject, operatorActionError, repositoryStatus],
  )

  return {
    highChurnStore,
    projects,
    activeProject,
    activeProjectId,
    pendingProjectSelectionId,
    repositoryStatus,
    workflowView,
    agentView,
    agentWorkspaceLayout,
    agentWorkspacePanes,
    executionView,
    repositoryDiffs,
    activeDiffScope,
    activeRepositoryDiff: repositoryDiffs[activeDiffScope],
    isLoading,
    isProjectLoading,
    isImporting,
    projectRemovalStatus,
    pendingProjectRemovalId,
    errorMessage,
    providerCredentials,
    providerCredentialsLoadStatus,
    providerCredentialsLoadError,
    providerCredentialsSaveStatus,
    providerCredentialsSaveError,
    providerModelCatalogs,
    providerModelCatalogLoadStatuses,
    providerModelCatalogLoadErrors,
    activeProviderModelCatalog,
    activeProviderModelCatalogLoadStatus,
    activeProviderModelCatalogLoadError,
    doctorReport,
    doctorReportStatus,
    doctorReportError,
    mcpRegistry,
    mcpImportDiagnostics,
    mcpRegistryLoadStatus,
    mcpRegistryLoadError,
    mcpRegistryMutationStatus,
    pendingMcpServerId,
    mcpRegistryMutationError,
    skillRegistry,
    skillRegistryLoadStatus,
    skillRegistryLoadError,
    skillRegistryMutationStatus,
    pendingSkillSourceId,
    skillRegistryMutationError,
    refreshSource,
    isDesktopRuntime: adapter.isDesktopRuntime(),
    operatorActionStatus,
    pendingOperatorActionId,
    operatorActionError,
    autonomousRunActionStatus,
    pendingAutonomousRunAction,
    autonomousRunActionError,
    runtimeRunActionStatus,
    pendingRuntimeRunAction,
    runtimeRunActionError,
    selectProject,
    prefetchProject,
    importProject,
    createProject,
    removeProject,
    retry,
    showRepositoryDiff,
    retryActiveRepositoryDiff,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshProviderModelCatalog,
    checkProviderProfile,
    runDoctorReport,
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    completeOAuthCallback,
    refreshMcpRegistry,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    refreshSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    refreshNotificationRoutes,
    upsertNotificationRoute,
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
    spawnPane,
    closePane,
    focusPane,
    setSplitterRatios,
    usageSummaries,
    activeUsageSummary: activeProjectId ? (usageSummaries[activeProjectId] ?? null) : null,
    activeUsageSummaryLoadError: activeProjectId
      ? (usageSummaryLoadErrors[activeProjectId] ?? null)
      : null,
    refreshUsageSummary,
  }
}
