import {
  Activity,
  useCallback,
  useEffect,
  lazy,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  Suspense,
  type ReactNode,
} from 'react'
import type {
  AgentComposerInsert,
  AgentPaneCloseState,
  AgentRuntimeProps,
  ComposerWorkflowTarget,
} from '@/components/xero/agent-runtime'
import {
  browserLaunchTargetMatchesUrl,
  type BrowserLaunchTarget,
} from '@/components/xero/browser-launch-targets'
import { SetupEmptyState } from '@/components/xero/agent-runtime/setup-empty-state'
import { AgentWorkspace } from '@/components/xero/agent-workspace'
import { AgentSessionsSidebar } from '@/components/xero/agent-sessions-sidebar'
import { AgentWorkspaceDndProvider } from '@/components/xero/agent-runtime/agent-workspace-dnd-provider'
import { AgentCommandPalette } from '@/components/xero/agent-runtime/agent-command-palette'
import {
  buildComposerAgentSelectionKey,
  runtimeAgentIdForCustomBaseCapability,
} from '@/components/xero/agent-runtime/composer-helpers'
import { type View } from '@/components/xero/data'
import { LoadingScreen } from '@/components/xero/loading-screen'
import { NoProjectEmptyState } from '@/components/xero/no-project-empty-state'
import { OnboardingFlow } from '@/components/xero/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/xero/project-load-error-state'
import { PhaseView } from '@/components/xero/phase-view'
import { ProjectAddDialog } from '@/components/xero/project-add-dialog'
import { ProjectRail } from '@/components/xero/project-rail'
import { UpdateScreen } from '@/components/xero/update-screen'
import { createSafeTauriUnlisten } from '@/src/lib/tauri-events'
import {
  browserIdempotencyStorage,
  ContinuationIdempotencyCoordinator,
} from '@/src/features/xero/continuation-idempotency'
import {
  XeroShell,
  detectPlatform,
  MOBILE_EMULATOR_SURFACES_ENABLED,
  type PlatformVariant,
  type SurfacePreloadTarget,
} from '@/components/xero/shell'
import { isTauri } from '@tauri-apps/api/core'
import type { StatusFooterProps } from '@/components/xero/status-footer'
import type { SettingsSection } from '@/components/xero/settings-dialog'
import type { TerminalSidebarHandle } from '@/components/xero/terminal-sidebar'
import type { StartTargetsModelOption } from '@/components/xero/start-targets-editor'
import type { VcsCommitMessageModel } from '@/components/xero/vcs-sidebar'
import type { EditorTerminalTaskRequest } from '@/components/xero/execution-view/editor-tasks'
import {
  buildEditorAgentActivities,
  type EditorAgentContextRequest,
} from '@/components/xero/execution-view/agent-aware-editor-hooks'
import { BaseAlertDialog } from '@xero/ui/components/base-dialog'
import { XeroDesktopAdapter as DefaultXeroDesktopAdapter, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import {
  applyRuntimeRun,
  applyRuntimeSession,
  mapProjectSnapshot,
  mapRuntimeRun,
  mapRuntimeSession,
  type ProjectDetailView,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeStreamView,
} from '@/src/lib/xero-model'
import {
  mapAgentSession,
  type RuntimeAgentIdDto,
  type RuntimeLinkedPathDto,
  type RuntimeRunControlInputDto,
  type StagedAgentAttachmentDto,
} from '@/src/lib/xero-model/runtime'
import {
  canonicalCustomAgentDefinitionSchema,
  type AgentDefaultModelDto,
  type AgentDefinitionWriteResponseDto,
  type AgentDefinitionSummaryDto,
} from '@/src/lib/xero-model/agent-definition'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringAttachableSkillDto,
  AgentAuthoringSkillSearchResultDto,
  AgentToolPackCatalogDto,
  SearchAgentAuthoringSkillsResponseDto,
  AgentRefDto,
  WorkflowAgentDetailDto,
  WorkflowAgentSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'
import type {
  WorkflowDefinitionDto,
  WorkflowDefinitionSummaryDto,
} from '@/src/lib/xero-model/workflow-definition'
import { getWorkflowStartInputPlan } from '@/src/lib/xero-model/workflow-start-input'
import type {
  WorkflowRunBlockerResponseDto,
  WorkflowRunBundleResponseDto,
  WorkflowRunDto,
} from '@/src/lib/xero-model/workflow-run'
import {
  instantiateBlankWorkflow,
  instantiateWorkflowTemplate,
  type WorkflowTemplateIdDto,
} from '@/src/lib/xero-model/workflow-templates'
import { useWorkflowAgentInspector } from '@/src/features/xero/use-workflow-agent-inspector'
import {
  persistToolCallGroupingPreference,
  readStoredToolCallGroupingPreference,
  readToolCallGroupingPreference,
  writeStoredToolCallGroupingPreference,
  type ToolCallGroupingPreference,
} from '@/src/features/xero/tool-call-grouping-preference'
import {
  persistAgentRoutingAutoSwitchPreference,
  readAgentRoutingAutoSwitchPreference,
  readStoredAgentRoutingAutoSwitchPreference,
  writeStoredAgentRoutingAutoSwitchPreference,
} from '@/src/features/xero/agent-routing-auto-switch-preference'
import type {
  SessionTranscriptSearchResultSnippetDto,
} from '@/src/lib/xero-model/session-context'
import { type RepositoryDiffScope } from '@/src/lib/xero-model/project'
import { summarizeProjectUsageSpend } from '@/src/lib/xero-model/usage'
import type {
  EnvironmentDiscoveryStatusDto,
  EnvironmentProbeReportDto,
  EnvironmentProfileSummaryDto,
  VerifyUserToolRequestDto,
  VerifyUserToolResponseDto,
} from '@/src/lib/xero-model/environment'
import {
  useXeroDesktopState,
  type AgentPaneView,
  type AgentWorkspaceLayoutState,
  type AgentWorkspacePaneView,
  type OperatorActionErrorView,
  type RefreshSource,
  type RuntimeRunActionKind,
  type RuntimeRunActionStatus,
} from '@/src/features/xero/use-xero-desktop-state'
import {
  buildAgentView,
} from '@/src/features/xero/use-xero-desktop-state/view-builders'
import {
  createRuntimeStreamStoreKey,
  removeRuntimeStreamForSession,
} from '@/src/features/xero/use-xero-desktop-state/high-churn-store'
import {
  attachRuntimeStreamSubscription,
  type RuntimeMetadataRefreshSource,
} from '@/src/features/xero/use-xero-desktop-state/runtime-stream'
import { getOperatorActionError } from '@/src/features/xero/use-xero-desktop-state/mutation-support'
import { useForcedAppUpdate } from '@/src/features/updates/use-forced-app-update'
import {
  clearProjectSelectionPreview,
  previewProjectSelection,
} from '@/src/features/xero/project-selection-preview'
import { useGitHubAuth } from '@/src/lib/github-auth'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'
import { SHORTCUT_DEFINITIONS, type ShortcutId } from '@/src/features/shortcuts/shortcuts-definitions'
import { useShortcutListener } from '@/src/features/shortcuts/use-shortcut-listener'
import {
  loadSourceControlSettings,
  subscribeSourceControlSettings,
  type SourceControlModelSelection,
} from '@/components/xero/source-control-settings'
import { startLayoutShiftGuard } from '@/lib/layout-shift-guard'
import {
  SIDEBAR_WIDTH_DURATION_MS,
  useSidebarOpenMotion,
  useSidebarWidthMotion,
} from '@/lib/sidebar-motion'
import { cn } from '@/lib/utils'
import { FloatingRightSidebarFrame } from '@/components/xero/floating-right-sidebar-frame'
import { SessionNotificationsSidebar } from '@/components/xero/session-notifications-sidebar'
import { SignInReminderToast } from '@/components/xero/sign-in-reminder-toast'
import type { BrowserAgentContextRequest } from '@/components/xero/browser-tool-injection'
import { DesktopControlBanner } from '@/components/xero/desktop-control-banner'
import { checkAttachmentModelCompatibility } from '@/lib/agent-attachments'
import { WORKFLOWS_ENABLED } from '@/src/features/xero/workflows-feature-flag'
import { WorkflowStartIdempotencyCoordinator } from '@/src/features/xero/workflow-start-idempotency'

export interface XeroAppProps {
  adapter?: XeroDesktopAdapter
}

interface AgentWorkspaceDisplaySnapshot {
  project: ProjectDetailView
  agentView: AgentPaneView | null
  layout: AgentWorkspaceLayoutState | null
  panes: AgentWorkspacePaneView[]
}

const loadAgentRuntime = () => import('@/components/xero/agent-runtime')
const loadExecutionView = () => import('@/components/xero/execution-view')
const loadBrowserSidebar = () => import('@/components/xero/browser-sidebar')
const loadIosEmulatorSidebar = () => import('@/components/xero/ios-emulator-sidebar')
const loadSettingsDialog = () => import('@/components/xero/settings-dialog')
const loadUsageStatsSidebar = () => import('@/components/xero/usage-stats-sidebar')
const loadVcsSidebar = () => import('@/components/xero/vcs-sidebar')
const loadWorkflowsSidebar = () => import('@/components/xero/workflows-sidebar')
const loadAgentDockSidebar = () => import('@/components/xero/agent-dock-sidebar')
const loadTerminalSidebar = () => import('@/components/xero/terminal-sidebar')
const loadStartTargetsDialog = () => import('@/components/xero/start-targets-dialog')

const ACTIVE_VIEW_APP_STATE_KEY = 'app.activeView.v1'
const ONBOARDING_COMPLETED_APP_STATE_KEY = 'app.onboarding.completed.v1'
const GLOBAL_BROWSER_PROJECT_KEY = '__global_browser__'
const GLOBAL_COMPUTER_USE_PROJECT_ID = 'global-computer-use'
const GLOBAL_COMPUTER_USE_AGENT_SESSION_ID = 'agent-session-global-computer-use'

type AppSidebarSurface =
  | 'agentDock'
  | 'browser'
  | 'computerUse'
  | 'ios'
  | 'notifications'
  | 'terminal'
  | 'usage'
  | 'vcs'
  | 'workflows'

interface ComputerUseLoadResult {
  project: ProjectDetailView
  runtimeSession: RuntimeSessionView | null
  runtimeRun: RuntimeRunView | null
}

interface BrowserSidebarProjectState {
  open: boolean
  fullWidth: boolean
}

const DEFAULT_BROWSER_SIDEBAR_PROJECT_STATE: BrowserSidebarProjectState = {
  open: false,
  fullWidth: false,
}

function browserSidebarProjectKey(projectId: string | null): string {
  return projectId ?? GLOBAL_BROWSER_PROJECT_KEY
}

function createRuntimeContinuationRequestId(): string {
  const randomUuid = globalThis.crypto?.randomUUID?.()
  if (!randomUuid) {
    throw new Error('Xero cannot create a continuation request id in this runtime.')
  }
  return `computer-use-prompt-${randomUuid}`
}

function createWorkflowStartIdempotencyKey(): string {
  const randomUuid = globalThis.crypto?.randomUUID?.()
  if (!randomUuid) {
    throw new Error('Xero cannot create a Workflow start idempotency key in this runtime.')
  }
  return `workflow-start-${randomUuid}`
}

function normalizePersistedActiveView(value: unknown): View | null {
  if (value === 'workflow') {
    return 'phases'
  }

  if (value === 'agent') {
    return 'agent'
  }

  if (value === 'editor') {
    return 'execution'
  }

  return null
}

function persistedActiveViewValue(view: View): 'workflow' | 'agent' | 'editor' {
  if (view === 'agent') {
    return 'agent'
  }

  if (view === 'execution') {
    return 'editor'
  }

  return 'workflow'
}

async function readPersistedActiveView(adapter: XeroDesktopAdapter): Promise<View | null> {
  if (!adapter.readAppUiState) {
    return null
  }

  try {
    const response = await adapter.readAppUiState({
      key: ACTIVE_VIEW_APP_STATE_KEY,
    })
    return normalizePersistedActiveView(response.value)
  } catch {
    return null
  }
}

async function persistActiveView(adapter: XeroDesktopAdapter, view: View): Promise<void> {
  if (!adapter.writeAppUiState) {
    return
  }

  await adapter.writeAppUiState({
    key: ACTIVE_VIEW_APP_STATE_KEY,
    value: persistedActiveViewValue(view),
  })
}

async function readPersistedOnboardingCompleted(adapter: XeroDesktopAdapter): Promise<boolean> {
  if (!adapter.readAppUiState) {
    return false
  }

  try {
    const response = await adapter.readAppUiState({
      key: ONBOARDING_COMPLETED_APP_STATE_KEY,
    })
    return response.value === true
  } catch {
    return false
  }
}

async function persistOnboardingCompleted(adapter: XeroDesktopAdapter): Promise<void> {
  if (!adapter.writeAppUiState) {
    return
  }

  await adapter.writeAppUiState({
    key: ONBOARDING_COMPLETED_APP_STATE_KEY,
    value: true,
  })
}

const warmedSurfaceChunks = new Set<SurfacePreloadTarget>()

const BASE_IDLE_SURFACE_PRELOAD_SEQUENCE: SurfacePreloadTarget[] = [
  'agent-dock',
  'terminal',
  'workflows',
  'vcs',
  'browser',
  'settings',
  'usage',
]

const AGENT_DOCK_WIDTH_STORAGE_KEY = 'xero.agentDock.width'
const AGENT_DOCK_MIN_WIDTH = 320
const AGENT_DOCK_DEFAULT_WIDTH = 560
const AGENT_DOCK_MAX_WIDTH = 720
// Matches ProjectRail's fixed `w-12`; browser focus fills the workspace to its right.
const PROJECT_RAIL_WIDTH = 48
const BROWSER_FOCUS_MIN_WIDTH = 320
const STARTUP_SURFACE_PREWARM_SETTLE_MS = 120
const HEAVY_SURFACE_MOUNT_AFTER_REVEAL_MS = 180
const BASE_STARTUP_SURFACE_PRELOAD_TARGETS: SurfacePreloadTarget[] = []

type BrowserComposerInsertTarget = 'agent-view' | 'agent-dock'

interface PendingBrowserComposerInsert {
  target: BrowserComposerInsertTarget
  insert: AgentComposerInsert
}

interface PendingBrowserOpenUrl {
  id: string
  url: string
}

function shouldIncludeIosSurface(): boolean {
  return MOBILE_EMULATOR_SURFACES_ENABLED && detectPlatform() === 'macos'
}

function withPlatformSurfacePreloads(
  targets: readonly SurfacePreloadTarget[],
): SurfacePreloadTarget[] {
  return shouldIncludeIosSurface() ? [...targets, 'ios'] : [...targets]
}

function readPersistedAgentDockWidth(): number {
  if (typeof window === 'undefined') {
    return AGENT_DOCK_DEFAULT_WIDTH
  }

  try {
    const raw = window.localStorage?.getItem?.(AGENT_DOCK_WIDTH_STORAGE_KEY)
    if (!raw) {
      return AGENT_DOCK_DEFAULT_WIDTH
    }
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed)) {
      return AGENT_DOCK_DEFAULT_WIDTH
    }
    return Math.max(AGENT_DOCK_MIN_WIDTH, Math.min(AGENT_DOCK_MAX_WIDTH, parsed))
  } catch {
    return AGENT_DOCK_DEFAULT_WIDTH
  }
}

function readBrowserFocusWidth(): number | null {
  if (typeof window === 'undefined') {
    return null
  }

  return Math.max(BROWSER_FOCUS_MIN_WIDTH, Math.round(window.innerWidth - PROJECT_RAIL_WIDTH))
}

function preloadSurfaceChunk(target: SurfacePreloadTarget): void {
  if (import.meta.env.MODE === 'test') {
    return
  }

  if (target === 'ios' && !shouldIncludeIosSurface()) {
    return
  }

  if (warmedSurfaceChunks.has(target)) {
    return
  }
  warmedSurfaceChunks.add(target)

  if (target === 'browser') {
    void loadBrowserSidebar()
    return
  }
  if (target === 'ios') {
    void loadIosEmulatorSidebar()
    return
  }
  if (target === 'terminal') {
    void loadTerminalSidebar()
    return
  }
  if (target === 'settings') {
    void loadSettingsDialog().then((module) =>
      module.preloadSettingsSectionChunks().catch(() => undefined),
    )
    return
  }
  if (target === 'usage') {
    void loadUsageStatsSidebar()
    return
  }
  if (target === 'vcs') {
    void loadVcsSidebar()
    return
  }
  if (target === 'workflows') {
    void loadWorkflowsSidebar()
    return
  }
  if (target === 'agent-dock') {
    void Promise.all([loadAgentDockSidebar(), loadAgentRuntime()])
  }
}

function scheduleIdlePreload(callback: () => void, timeout: number): () => void {
  if (typeof window === 'undefined') {
    return () => {}
  }

  const idleWindow = window as Window & {
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number
    cancelIdleCallback?: (handle: number) => void
  }

  if (typeof idleWindow.requestIdleCallback === 'function') {
    const handle = idleWindow.requestIdleCallback(callback, { timeout })
    return () => idleWindow.cancelIdleCallback?.(handle)
  }

  const handle = window.setTimeout(callback, Math.min(timeout, 200))
  return () => window.clearTimeout(handle)
}

function scheduleAfterNextPaint(callback: () => void): () => void {
  if (typeof window === 'undefined') {
    callback()
    return () => {}
  }

  let cancelled = false
  let timeout: number | null = null

  const run = () => {
    timeout = window.setTimeout(() => {
      if (!cancelled) {
        callback()
      }
    }, 0)
  }

  if (typeof window.requestAnimationFrame === 'function') {
    const frame = window.requestAnimationFrame(run)
    return () => {
      cancelled = true
      window.cancelAnimationFrame?.(frame)
      if (timeout !== null) {
        window.clearTimeout(timeout)
      }
    }
  }

  run()
  return () => {
    cancelled = true
    if (timeout !== null) {
      window.clearTimeout(timeout)
    }
  }
}

function waitForStartupPrewarmPaints(): Promise<void> {
  if (typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
    return Promise.resolve()
  }

  return new Promise((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => resolve())
    })
  })
}

async function waitForStartupPreloadSettle(): Promise<void> {
  await waitForStartupPrewarmPaints()
  if (typeof window !== 'undefined') {
    await new Promise<void>((resolve) => {
      window.setTimeout(resolve, STARTUP_SURFACE_PREWARM_SETTLE_MS)
    })
  }
  await waitForStartupPrewarmPaints()
}

async function preloadStartupSurfaceChunks(): Promise<void> {
  const preloads: Array<Promise<unknown>> = [
    loadAgentRuntime(),
    loadExecutionView(),
  ]

  await Promise.all(preloads)
  withPlatformSurfacePreloads(BASE_STARTUP_SURFACE_PRELOAD_TARGETS).forEach((target) =>
    warmedSurfaceChunks.add(target),
  )
}

function useStartupSurfacePrewarm(enabled: boolean): {
  ready: boolean
  shouldMount: boolean
} {
  const [ready, setReady] = useState(() => import.meta.env.MODE === 'test' || !enabled)
  const [shouldMount, setShouldMount] = useState(false)
  const startedRef = useRef(false)

  useEffect(() => {
    if (import.meta.env.MODE === 'test') {
      setReady(true)
      setShouldMount(false)
      return
    }

    if (!enabled) {
      setShouldMount(false)
      return
    }

    if (startedRef.current) {
      return
    }

    let cancelled = false
    startedRef.current = true
    setReady(false)
    setShouldMount(true)

    void preloadStartupSurfaceChunks()
      .catch(() => undefined)
      .then(() => waitForStartupPreloadSettle())
      .then(() => {
        if (!cancelled) {
          setShouldMount(false)
          setReady(true)
        }
      })

    return () => {
      cancelled = true
      setShouldMount(false)
    }
  }, [enabled])

  return {
    ready: import.meta.env.MODE === 'test' || !enabled ? true : ready && startedRef.current,
    shouldMount:
      import.meta.env.MODE === 'test' || !enabled
        ? false
        : shouldMount && startedRef.current,
  }
}

function useIdleSurfacePreloads(enabled: boolean): void {
  useEffect(() => {
    if (!enabled || import.meta.env.MODE === 'test' || typeof window === 'undefined') {
      return
    }

    let cancelled = false
    let cancelScheduled: (() => void) | null = null
    const queue = withPlatformSurfacePreloads(BASE_IDLE_SURFACE_PRELOAD_SEQUENCE)

    const warmNext = () => {
      if (cancelled) {
        return
      }
      const next = queue.shift()
      if (!next) {
        return
      }

      preloadSurfaceChunk(next)
      cancelScheduled = scheduleIdlePreload(warmNext, 1200)
    }

    cancelScheduled = scheduleIdlePreload(warmNext, 1800)

    return () => {
      cancelled = true
      cancelScheduled?.()
    }
  }, [enabled])
}

const LazyExecutionView = lazy(() =>
  loadExecutionView().then((module) => ({ default: module.ExecutionView })),
)
const LazyBrowserSidebar = lazy(() =>
  loadBrowserSidebar().then((module) => ({ default: module.BrowserSidebar })),
)
const LazyIosEmulatorSidebar = lazy(() =>
  loadIosEmulatorSidebar().then((module) => ({ default: module.IosEmulatorSidebar })),
)
const LazySettingsDialog = lazy(() =>
  loadSettingsDialog().then((module) => ({ default: module.SettingsDialog })),
)
const LazyUsageStatsSidebar = lazy(() =>
  loadUsageStatsSidebar().then((module) => ({ default: module.UsageStatsSidebar })),
)
const LazyVcsSidebar = lazy(() =>
  loadVcsSidebar().then((module) => ({ default: module.VcsSidebar })),
)
const LazyWorkflowsSidebar = lazy(() =>
  loadWorkflowsSidebar().then((module) => ({ default: module.WorkflowsSidebar })),
)
const LazyAgentDockSidebar = lazy(() =>
  loadAgentDockSidebar().then((module) => ({ default: module.AgentDockSidebar })),
)
const LazyTerminalSidebar = lazy(() =>
  loadTerminalSidebar().then((module) => ({ default: module.TerminalSidebar })),
)
const LazyStartTargetsDialog = lazy(() =>
  loadStartTargetsDialog().then((module) => ({ default: module.StartTargetsDialog })),
)

function preloadViewChunk(view: View): void {
  if (view === 'agent') {
    void loadAgentRuntime()
    return
  }

  if (view === 'execution') {
    void loadExecutionView()
  }
}

function getVcsCommitMessageModel(
  agent: AgentPaneView | null,
  composerControls: RuntimeRunControlInputDto | null,
  sourceControlModelSelection: SourceControlModelSelection | null = null,
): VcsCommitMessageModel | null {
  const configuredModelId = sourceControlModelSelection?.modelId?.trim() || null
  const modelId =
    configuredModelId || composerControls?.modelId?.trim() || agent?.selectedModelId?.trim() || null
  if (!agent || !modelId) {
    return null
  }

  const providerId =
    sourceControlModelSelection?.providerId ??
    agent.selectedModel?.providerId ??
    agent.selectedProviderId ??
    null
  const selectedModelOption =
    agent.providerModelCatalog.models.find(
      (model) =>
        model.modelId === modelId &&
        (!sourceControlModelSelection?.providerProfileId ||
          model.profileId === sourceControlModelSelection.providerProfileId) &&
        (!composerControls?.providerProfileId || configuredModelId || model.profileId === composerControls.providerProfileId),
    ) ??
    agent.providerModelCatalog.models.find(
      (model) => model.modelId === modelId || model.selectionKey === `${providerId}:${modelId}`,
    ) ?? (configuredModelId ? null : agent.selectedModelOption)
  const providerProfileId =
    sourceControlModelSelection?.providerProfileId ??
    (configuredModelId ? selectedModelOption?.profileId : composerControls?.providerProfileId) ??
    agent.runtimeRunActiveControls?.providerProfileId ??
    agent.runtimeRunPendingControls?.providerProfileId ??
    selectedModelOption?.profileId ??
    getCloudProviderDefaultProfileId(providerId) ??
    null

  return {
    providerProfileId,
    modelId,
    thinkingEffort: configuredModelId
      ? sourceControlModelSelection?.thinkingEffort ?? null
      : composerControls?.thinkingEffort ??
        agent.selectedThinkingEffort ??
        agent.selectedModelDefaultThinkingEffort ??
        null,
    label: selectedModelOption?.label ?? modelId,
  }
}

function getProjectRunnerModelOptions(
  agent: AgentPaneView | null,
): StartTargetsModelOption[] {
  return (agent?.composerModelOptions ?? []).map((option) => ({
    selectionKey: option.selectionKey,
    providerId: option.providerId,
    providerProfileId: option.profileId,
    providerLabel: option.providerLabel,
    modelId: option.modelId,
    label: option.displayName,
    thinkingEffortOptions: option.thinkingEffortOptions,
    defaultThinkingEffort: option.defaultThinkingEffort,
  }))
}

function projectRunnerSuggestControlsFromRequest(
  request: {
    modelId: string
    providerProfileId?: string | null
    runtimeAgentId: RuntimeAgentIdDto | null
    thinkingEffort: RuntimeRunControlInputDto['thinkingEffort']
  },
  currentControls: RuntimeRunControlInputDto | null,
): RuntimeRunControlInputDto | null {
  const modelId = normalizeComposerSettingsText(request.modelId)
  if (!modelId) return null

  return {
    runtimeAgentId: request.runtimeAgentId ?? currentControls?.runtimeAgentId ?? 'ask',
    agentDefinitionId: currentControls?.agentDefinitionId ?? null,
    providerProfileId: request.providerProfileId ?? null,
    modelId,
    thinkingEffort: request.thinkingEffort ?? null,
    approvalMode: currentControls?.approvalMode ?? 'suggest',
    planModeRequired: false,
    autoCompactEnabled: currentControls?.autoCompactEnabled ?? true,
  }
}

function sameRuntimeRunControlInput(
  left: RuntimeRunControlInputDto | null,
  right: RuntimeRunControlInputDto | null,
): boolean {
  if (left === right) return true
  if (!left || !right) return left === right

  return (
    (left.providerProfileId ?? null) === (right.providerProfileId ?? null) &&
    left.runtimeAgentId === right.runtimeAgentId &&
    (left.agentDefinitionId ?? null) === (right.agentDefinitionId ?? null) &&
    left.modelId === right.modelId &&
    (left.thinkingEffort ?? null) === (right.thinkingEffort ?? null) &&
    left.approvalMode === right.approvalMode &&
    Boolean(left.planModeRequired) === Boolean(right.planModeRequired) &&
    left.autoCompactEnabled === right.autoCompactEnabled
  )
}

const COMPOSER_SETTINGS_APP_STATE_KEY = 'xero.agent.composer.settings.v1'
const COMPOSER_SETTINGS_UPDATED_EVENT = 'agent:composer_settings_updated'
const COMPOSER_SETTINGS_VERSION = 1
const COMPOSER_THINKING_LEVELS = ['none', 'minimal', 'low', 'medium', 'high', 'x_high'] as const
const COMPOSER_APPROVAL_MODES = ['suggest', 'auto_edit', 'yolo'] as const
const COMPOSER_RUNTIME_AGENT_IDS: readonly RuntimeAgentIdDto[] = [
  'generalist',
  'ask',
  'computer_use',
  'plan',
  'engineer',
  'debug',
  'crawl',
  'agent_create',
]

function isComposerSettingsRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function normalizeComposerSettingsText(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function isComposerRuntimeAgentId(value: unknown): value is RuntimeAgentIdDto {
  return (
    typeof value === 'string' &&
    COMPOSER_RUNTIME_AGENT_IDS.includes(value as RuntimeAgentIdDto)
  )
}

function isComposerThinkingEffort(
  value: unknown,
): value is NonNullable<RuntimeRunControlInputDto['thinkingEffort']> {
  return (
    typeof value === 'string' &&
    COMPOSER_THINKING_LEVELS.includes(
      value as NonNullable<RuntimeRunControlInputDto['thinkingEffort']>,
    )
  )
}

function isComposerApprovalMode(
  value: unknown,
): value is RuntimeRunControlInputDto['approvalMode'] {
  return (
    typeof value === 'string' &&
    COMPOSER_APPROVAL_MODES.includes(value as RuntimeRunControlInputDto['approvalMode'])
  )
}

function composerSettingsValueFromControls(
  controls: RuntimeRunControlInputDto | null,
): Record<string, unknown> | null {
  const modelId = normalizeComposerSettingsText(controls?.modelId)
  if (!controls || !modelId) return null

  return {
    version: COMPOSER_SETTINGS_VERSION,
    modelId,
    providerProfileId: normalizeComposerSettingsText(controls.providerProfileId),
    runtimeAgentId: controls.runtimeAgentId,
    agentDefinitionId: normalizeComposerSettingsText(controls.agentDefinitionId),
    thinkingEffort: controls.thinkingEffort ?? null,
    approvalMode: controls.approvalMode,
    autoCompactEnabled: controls.autoCompactEnabled,
    updatedAt: new Date().toISOString(),
  }
}

function parseComposerModelSelectionKey(value: unknown): {
  providerId: string | null
  modelId: string | null
} {
  const selectionKey = normalizeComposerSettingsText(value)
  if (!selectionKey) {
    return { providerId: null, modelId: null }
  }

  const separatorIndex = selectionKey.indexOf(':')
  if (separatorIndex <= 0 || separatorIndex === selectionKey.length - 1) {
    return { providerId: null, modelId: selectionKey }
  }

  return {
    providerId: selectionKey.slice(0, separatorIndex),
    modelId: selectionKey.slice(separatorIndex + 1),
  }
}

function composerModelRouteFromSettingsValue(value: unknown): {
  modelId: string
  providerProfileId: string | null
  providerId: string | null
} | null {
  if (!isComposerSettingsRecord(value) || value.version !== COMPOSER_SETTINGS_VERSION) {
    return null
  }

  const selection = parseComposerModelSelectionKey(value.modelSelectionKey)
  const storedModelId = normalizeComposerSettingsText(value.modelId)
  const modelId = storedModelId ?? selection.modelId
  if (!modelId) {
    return null
  }

  const selectionProviderId =
    selection.modelId === modelId ? selection.providerId : null
  const providerProfileId =
    normalizeComposerSettingsText(value.providerProfileId) ??
    getCloudProviderDefaultProfileId(selectionProviderId)

  return {
    modelId,
    providerProfileId,
    providerId: selectionProviderId,
  }
}

function runtimeControlsFromComposerSettingsValue(
  value: unknown,
): RuntimeRunControlInputDto | null {
  if (!isComposerSettingsRecord(value) || value.version !== COMPOSER_SETTINGS_VERSION) {
    return null
  }
  const route = composerModelRouteFromSettingsValue(value)
  if (!route || !isComposerRuntimeAgentId(value.runtimeAgentId)) {
    return null
  }

  return {
    runtimeAgentId: value.runtimeAgentId,
    agentDefinitionId: normalizeComposerSettingsText(value.agentDefinitionId),
    providerProfileId: route.providerProfileId,
    modelId: route.modelId,
    thinkingEffort: isComposerThinkingEffort(value.thinkingEffort)
      ? value.thinkingEffort
      : null,
    approvalMode: isComposerApprovalMode(value.approvalMode)
      ? value.approvalMode
      : 'suggest',
    planModeRequired: false,
    autoCompactEnabled:
      typeof value.autoCompactEnabled === 'boolean' ? value.autoCompactEnabled : true,
  }
}

export function projectRunnerSuggestRequestFromStoredComposerSettings(value: unknown): {
  modelId: string
  providerId?: string | null
  providerProfileId: string | null
  runtimeAgentId: RuntimeAgentIdDto | null
  thinkingEffort: NonNullable<RuntimeRunControlInputDto['thinkingEffort']> | null
} | null {
  if (!isComposerSettingsRecord(value) || value.version !== COMPOSER_SETTINGS_VERSION) {
    return null
  }

  const route = composerModelRouteFromSettingsValue(value)
  if (!route) {
    return null
  }

  const runtimeAgentId =
    isComposerRuntimeAgentId(value.runtimeAgentId) && value.runtimeAgentId !== 'computer_use'
      ? value.runtimeAgentId
      : null

  return {
    modelId: route.modelId,
    providerId: route.providerId,
    providerProfileId: route.providerProfileId,
    runtimeAgentId,
    thinkingEffort: isComposerThinkingEffort(value.thinkingEffort)
      ? value.thinkingEffort
      : null,
  }
}

function mirrorComposerSettingsToLocalStorage(value: unknown): void {
  if (!isComposerSettingsRecord(value) || value.version !== COMPOSER_SETTINGS_VERSION) {
    return
  }
  if (typeof window === 'undefined') return

  try {
    window.localStorage?.setItem?.(
      COMPOSER_SETTINGS_APP_STATE_KEY,
      JSON.stringify(value),
    )
    if (typeof value.autoCompactEnabled === 'boolean') {
      window.localStorage?.setItem?.(
        'xero.agent.autoCompact.enabled',
        value.autoCompactEnabled ? '1' : '0',
      )
    }
  } catch {
    /* Keep the in-memory selection when localStorage is unavailable. */
  }
}

interface PendingInitialAgentSelection {
  agentSessionId: string
  runtimeAgentId: RuntimeAgentIdDto
  agentDefinitionId: string | null
}

interface PendingInitialWorkflowSelection {
  projectId: string
  agentSessionId: string
  target: ComposerWorkflowTarget
}

interface PendingWorkflowPaneSelection {
  projectId: string
  paneId: string
  target: ComposerWorkflowTarget
}

type RuntimeAgentSelection = Omit<PendingInitialAgentSelection, 'agentSessionId'>

function runtimeAgentSelectionFromRef(
  ref: AgentRefDto,
  customDefinitions: readonly AgentDefinitionSummaryDto[],
  workflowAgents: readonly WorkflowAgentSummaryDto[],
): RuntimeAgentSelection | null {
  if (ref.kind === 'built_in') {
    return {
      runtimeAgentId: ref.runtimeAgentId,
      agentDefinitionId: null,
    }
  }

  const customDefinition = customDefinitions.find(
    (definition) => definition.definitionId === ref.definitionId,
  )
  const workflowAgent = workflowAgents.find(
    (agent) => agent.ref.kind === 'custom' && agent.ref.definitionId === ref.definitionId,
  )
  const baseCapabilityProfile =
    customDefinition?.baseCapabilityProfile ?? workflowAgent?.baseCapabilityProfile
  if (!baseCapabilityProfile) return null

  return {
    runtimeAgentId: runtimeAgentIdForCustomBaseCapability(baseCapabilityProfile),
    agentDefinitionId: ref.definitionId,
  }
}

function shouldConfirmPaneClose(state: AgentPaneCloseState | null | undefined): state is AgentPaneCloseState {
  return Boolean(state?.hasRunningRun || state?.hasUnsavedComposerText)
}

function getPaneCloseConfirmationCopy(state: AgentPaneCloseState | null | undefined): string {
  const reasons = [
    state?.hasRunningRun ? 'The agent is still running' : null,
    state?.hasUnsavedComposerText ? 'The composer has unsent text' : null,
  ].filter((reason): reason is string => Boolean(reason))

  if (reasons.length === 0) {
    return 'This pane can close now.'
  }

  return `${reasons.join(' and ')}.`
}

export function useActivatedSurface(active: boolean, prewarm = false) {
  const [activated, setActivated] = useState(active)

  useEffect(() => {
    if (active) {
      setActivated(true)
    }
  }, [active])

  return active || prewarm || activated
}

export function useStickyPrewarmedSurface(active: boolean, prewarm = false) {
  const [activated, setActivated] = useState(active || prewarm)

  useEffect(() => {
    if (active || prewarm) {
      setActivated(true)
    }
  }, [active, prewarm])

  return active || prewarm || activated
}

interface LazyActivityPaneProps {
  active: boolean
  children: ReactNode
  className?: string
  name: string
  prewarm?: boolean
}

function useFrozenSurfaceChildren(
  children: ReactNode,
  options: { active: boolean; prewarm?: boolean },
): ReactNode {
  const { active, prewarm = false } = options
  const previousActiveRef = useRef(active)
  const renderedChildrenRef = useRef(children)
  const activeChanged = previousActiveRef.current !== active

  if (active || prewarm || activeChanged) {
    renderedChildrenRef.current = children
  }
  previousActiveRef.current = active

  return renderedChildrenRef.current
}

function LazyActivityPane({
  active,
  children,
  className,
  name,
  prewarm = false,
}: LazyActivityPaneProps) {
  const shouldMount = useActivatedSurface(active, prewarm)
  const renderedChildren = useFrozenSurfaceChildren(children, {
    active,
    prewarm,
  })

  if (!shouldMount) {
    return null
  }

  const pane = (
    <div
      aria-hidden={!active}
      className={className}
      inert={!active ? true : undefined}
    >
      {renderedChildren}
    </div>
  )

  return (
    <Activity mode={active ? 'visible' : 'hidden'} name={name}>
      {pane}
    </Activity>
  )
}

function LazyMountedPane({
  active,
  children,
  className,
  prewarm = false,
}: Omit<LazyActivityPaneProps, 'name'>) {
  const shouldMount = useActivatedSurface(active, prewarm)
  const renderedChildren = useFrozenSurfaceChildren(children, {
    active,
    prewarm,
  })

  if (!shouldMount) {
    return null
  }

  return (
    <div
      aria-hidden={!active}
      className={className}
      inert={!active ? true : undefined}
    >
      {renderedChildren}
    </div>
  )
}

interface LazyPrerenderedSurfaceProps {
  children: ReactNode
  open: boolean
  prewarm?: boolean
  stickyPrewarm?: boolean
}

function LazyPrerenderedSurface({
  children,
  open,
  prewarm = false,
  stickyPrewarm = false,
}: LazyPrerenderedSurfaceProps) {
  const activatedMount = useActivatedSurface(open, prewarm)
  const stickyMount = useStickyPrewarmedSurface(open, prewarm)
  const shouldMount = stickyPrewarm ? stickyMount : activatedMount
  const renderedChildren = useFrozenSurfaceChildren(children, {
    active: open,
    prewarm,
  })

  if (!shouldMount) {
    return null
  }

  return <>{renderedChildren}</>
}

function isForegroundProjectLoad(source: RefreshSource): boolean {
  return source === 'startup' || source === 'selection' || source === 'import' || source === 'remove'
}

function InlineSidebarLoadingShell({
  instantOpen = false,
  label,
  open,
  width = 420,
}: {
  instantOpen?: boolean
  label: string
  open: boolean
  width?: number
}) {
  const motionOpen = useSidebarOpenMotion(open, { instantOpen })
  const targetWidth = motionOpen ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth)

  return (
    <aside
      aria-busy={open}
      aria-hidden={!open}
      aria-label={`Loading ${label}`}
      className={cn(
        widthMotion.islandClassName,
        'relative flex shrink-0 flex-col overflow-hidden bg-sidebar',
        open ? 'border-l border-border/80' : 'border-l-0',
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div className="flex h-full min-w-0 shrink-0 flex-col" style={{ width }}>
        <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 pl-3 pr-2">
          <div className="min-w-0 truncate text-[11px] font-semibold text-foreground">
            {label}
          </div>
          <div className="h-3 w-3 shrink-0 animate-spin rounded-full border border-primary/30 border-t-primary" />
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-3 py-3">
          <div className="h-7 w-40 max-w-[70%] rounded-md bg-secondary/50" />
          <div className="h-24 rounded-md border border-border/60 bg-background/35" />
          <div className="space-y-2">
            <div className="h-3 w-3/4 rounded bg-secondary/45" />
            <div className="h-3 w-1/2 rounded bg-secondary/35" />
          </div>
        </div>
      </div>
    </aside>
  )
}

function OverlaySidebarLoadingShell({
  label,
  open,
  width = 420,
}: {
  label: string
  open: boolean
  width?: number
}) {
  return (
    <FloatingRightSidebarFrame
      ariaBusy={open}
      label={`Loading ${label}`}
      open={open}
      width={width}
    >
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 pl-3 pr-2">
          <div className="min-w-0 truncate text-[11px] font-semibold text-foreground">
            {label}
          </div>
          <div className="h-3 w-3 shrink-0 animate-spin rounded-full border border-primary/30 border-t-primary" />
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-3 py-3">
          <div className="h-7 w-40 max-w-[70%] rounded-md bg-secondary/50" />
          <div className="h-24 rounded-md border border-border/60 bg-background/35" />
          <div className="space-y-2">
            <div className="h-3 w-3/4 rounded bg-secondary/45" />
            <div className="h-3 w-1/2 rounded bg-secondary/35" />
          </div>
        </div>
      </div>
    </FloatingRightSidebarFrame>
  )
}

function ModalLoadingShell({ open }: { open: boolean }) {
  if (!open) {
    return null
  }

  return (
    <div className="fixed inset-0 z-[2147483600] bg-background">
      <LoadingScreen className="h-screen w-screen" />
    </div>
  )
}

function useDeferredSurfaceActivation(open: boolean, prewarm: boolean): boolean {
  const [active, setActive] = useState(prewarm)

  useEffect(() => {
    if (prewarm) {
      setActive(true)
      return
    }

    if (!open || active) {
      return
    }

    let cancelAfterPaint: (() => void) | null = null
    let timeout: number | null = null

    cancelAfterPaint = scheduleAfterNextPaint(() => {
      if (typeof window === 'undefined') {
        setActive(true)
        return
      }

      timeout = window.setTimeout(() => {
        setActive(true)
      }, HEAVY_SURFACE_MOUNT_AFTER_REVEAL_MS)
    })

    return () => {
      cancelAfterPaint?.()
      if (timeout !== null) {
        window.clearTimeout(timeout)
      }
    }
  }, [active, open, prewarm])

  return active
}

function IosEmulatorSurface({ open, prewarm = false }: { open: boolean; prewarm?: boolean }) {
  const shouldMount = useStickyPrewarmedSurface(open, prewarm)
  const readyForSidebar = useDeferredSurfaceActivation(open, prewarm)

  if (!shouldMount) {
    return null
  }

  if (!readyForSidebar) {
    return <InlineSidebarLoadingShell label="iOS Simulator" open={open} width={640} />
  }

  return (
    <Suspense
      fallback={
        <InlineSidebarLoadingShell
          instantOpen={open}
          label="iOS Simulator"
          open={open}
          width={640}
        />
      }
    >
      <LazyIosEmulatorSidebar open={open} openImmediately={open} />
    </Suspense>
  )
}

export const APP_BOOT_LOADING_EXIT_MS = 160

function DismissingLoadingOverlay({
  active,
  className,
  loadingClassName,
}: {
  active: boolean
  className?: string
  loadingClassName?: string
}) {
  const [rendered, setRendered] = useState(active)

  useEffect(() => {
    if (active) {
      setRendered(true)
      return
    }

    if (!rendered) {
      return
    }

    const timeout = window.setTimeout(() => {
      setRendered(false)
    }, APP_BOOT_LOADING_EXIT_MS)

    return () => {
      window.clearTimeout(timeout)
    }
  }, [active, rendered])

  if (!rendered) {
    return null
  }

  const closing = !active

  return (
    <div
      aria-hidden={closing}
      className={cn(
        closing && 'pointer-events-none',
        className,
      )}
      data-state={closing ? 'closed' : 'open'}
      inert={closing ? true : undefined}
    >
      <LoadingScreen className={loadingClassName} state={closing ? 'closed' : 'open'} />
    </div>
  )
}

export function AppWideLoadingOverlay({ active }: { active: boolean }) {
  return (
    <DismissingLoadingOverlay
      active={active}
      className="absolute inset-0 z-40"
      loadingClassName="h-full w-full"
    />
  )
}

export function AppBootLoadingOverlay({ active }: { active: boolean }) {
  // Rendered as an app-root sibling of XeroShell; the shell main row uses
  // paint containment, which would otherwise clip fixed descendants.
  return (
    <DismissingLoadingOverlay
      active={active}
      className="fixed inset-0 z-[2147483647]"
      loadingClassName="h-screen w-screen"
    />
  )
}

export function XeroApp({ adapter }: XeroAppProps) {
  const resolvedAdapter = adapter ?? DefaultXeroDesktopAdapter
  const [activeView, setActiveViewRaw] = useState<View>('agent')
  const [activeViewHydrated, setActiveViewHydrated] = useState(() => !resolvedAdapter.readAppUiState)

  // Tab switches simultaneously trigger the cross-fade of view panes AND the
  // auto-collapse of the project rail / sessions sidebar (both via useEffect
  // below). Animating the sidebar widths at the same time as heavy view
  // contents (CodeMirror, agent UI, phase view) re-layout produces visible
  // jitter on slower hosts, so we mark the document as `data-layout-shifting`
  // for one frame around the change. CSS in globals.css disables the
  // `.sidebar-motion-island` width transitions while the attribute is set —
  // sidebars snap to their new widths instantly, leaving only the cheap
  // GPU-driven pane cross-fade animating on the main thread. User-initiated
  // toggles (e.g. clicking the rail collapse button) still animate normally.
  const activeViewRef = useRef(activeView)
  const cancelLayoutShiftGuardRef = useRef<(() => void) | null>(null)
  const setActiveView = useCallback((view: View) => {
    if (activeViewRef.current === view) {
      return
    }

    preloadViewChunk(view)
    activeViewRef.current = view
    cancelLayoutShiftGuardRef.current?.()
    cancelLayoutShiftGuardRef.current = startLayoutShiftGuard()
    setActiveViewRaw(view)
  }, [])

  useEffect(() => {
    if (!resolvedAdapter.readAppUiState) {
      setActiveViewHydrated(true)
      return
    }

    let disposed = false
    setActiveViewHydrated(false)
    void readPersistedActiveView(resolvedAdapter)
      .then((view) => {
        if (disposed || !view) {
          return
        }
        setActiveView(view)
      })
      .catch(() => undefined)
      .finally(() => {
        if (disposed) {
          return
        }
        setActiveViewHydrated(true)
      })

    return () => {
      disposed = true
    }
  }, [resolvedAdapter, setActiveView])

  useEffect(() => {
    if (!activeViewHydrated) {
      return
    }

    void persistActiveView(resolvedAdapter, activeView).catch(() => undefined)
  }, [activeView, activeViewHydrated, resolvedAdapter])

  useEffect(() => {
    return () => {
      cancelLayoutShiftGuardRef.current?.()
      cancelLayoutShiftGuardRef.current = null
    }
  }, [])

  useShortcutListener(
    useCallback(
      (id: ShortcutId) => {
        const def = SHORTCUT_DEFINITIONS.find((entry) => entry.id === id)
        if (def?.view) {
          setActiveView(def.view)
        }
      },
      [setActiveView],
    ),
  )

  const {
    highChurnStore,
    projects,
    activeProject,
    activeProjectId,
    runningAgentProjectIds,
    pendingProjectSelectionId,
    repositoryStatus,
    workflowView,
    agentView,
    agentWorkspaceLayout,
    agentWorkspacePanes,
    executionView,
    isLoading,
    isProjectLoading,
    refreshSource,
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
    isDesktopRuntime,
    selectProject,
    prefetchProject,
    importProject,
    createProject,
    removeProject,
    retry,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    revokeProjectAssetTokens,
    openProjectFileExternal,
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
    runDoctorReport,
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
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
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    activeUsageSummary,
    refreshUsageSummary,
    spawnPane,
    closePane,
    focusPane,
    reorderPanes,
    openSessionInNewPane,
    setSplitterRatios,
    acknowledgeCompletedAgentSessions,
    unreadCompletedSessionCount,
    unreadCompletedSessionNotifications,
  } = useXeroDesktopState({ adapter, subscribeRuntimeStreams: false })

  const completedSessionCountsByProject = useMemo<ReadonlyMap<string, number>>(() => {
    const counts = new Map<string, number>()

    for (const notification of unreadCompletedSessionNotifications) {
      counts.set(notification.projectId, (counts.get(notification.projectId) ?? 0) + 1)
    }

    return counts
  }, [unreadCompletedSessionNotifications])

  const {
    session: githubSession,
    status: githubAuthStatus,
    error: githubAuthError,
    login: loginWithGithub,
    logout: logoutGithub,
  } = useGitHubAuth()

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsInitialSection, setSettingsInitialSection] = useState<SettingsSection>('account')
  const [toolCallGroupingPreference, setToolCallGroupingPreference] =
    useState<ToolCallGroupingPreference>(() => readStoredToolCallGroupingPreference())
  const [agentRoutingAutoSwitchEnabled, setAgentRoutingAutoSwitchEnabled] =
    useState<boolean>(() => readStoredAgentRoutingAutoSwitchPreference())
  const [pendingAgentSessionId, setPendingAgentSessionId] = useState<string | null>(null)
  const [paneCloseStates, setPaneCloseStates] = useState<Record<string, AgentPaneCloseState>>({})
  const [pendingPaneCloseId, setPendingPaneCloseId] = useState<string | null>(null)
  const [agentComposerControls, setAgentComposerControls] =
    useState<RuntimeRunControlInputDto | null>(null)
  const [sourceControlSettings, setSourceControlSettings] = useState(
    loadSourceControlSettings,
  )
  const persistComposerSettings = useCallback(
    (controls: RuntimeRunControlInputDto | null) => {
      const value = composerSettingsValueFromControls(controls)
      if (!value) return
      mirrorComposerSettingsToLocalStorage(value)
      void resolvedAdapter.writeAppUiState?.({
        key: COMPOSER_SETTINGS_APP_STATE_KEY,
        value,
      }).catch(() => undefined)
    },
    [resolvedAdapter],
  )
  useEffect(() => {
    let disposed = false
    void resolvedAdapter.readAppUiState?.({ key: COMPOSER_SETTINGS_APP_STATE_KEY })
      .then((response) => {
        if (disposed) return
        const value = response.value ?? null
        mirrorComposerSettingsToLocalStorage(value)
        const controls = runtimeControlsFromComposerSettingsValue(value)
        if (controls?.runtimeAgentId && controls.runtimeAgentId !== 'computer_use') {
          setAgentComposerControls((current) =>
            sameRuntimeRunControlInput(current, controls) ? current : controls,
          )
        }
      })
      .catch(() => undefined)

    return () => {
      disposed = true
    }
  }, [resolvedAdapter])
  useEffect(
    () =>
      subscribeSourceControlSettings((nextSettings) => {
        setSourceControlSettings(nextSettings)
      }),
    [],
  )
  useEffect(() => {
    if (!isTauri()) return
    let disposed = false
    let unlisten: (() => void) | null = null

    void import('@tauri-apps/api/event')
      .then(({ listen }) =>
        listen<unknown>(COMPOSER_SETTINGS_UPDATED_EVENT, (event) => {
          const value = event.payload
          mirrorComposerSettingsToLocalStorage(value)
          const controls = runtimeControlsFromComposerSettingsValue(value)
          if (controls?.runtimeAgentId && controls.runtimeAgentId !== 'computer_use') {
            setAgentComposerControls((current) =>
              sameRuntimeRunControlInput(current, controls) ? current : controls,
            )
          }
        }),
      )
      .then((nextUnlisten) => {
        const safeUnlisten = createSafeTauriUnlisten(nextUnlisten)
        if (disposed) {
          safeUnlisten()
          return
        }
        unlisten = safeUnlisten
      })
      .catch(() => undefined)

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])
  const [isCreatingAgentSession, setIsCreatingAgentSession] = useState(false)
  const [projectAddOpen, setProjectAddOpen] = useState(false)
  const browserProjectKey = browserSidebarProjectKey(activeProjectId)
  const [browserSidebarStateByProject, setBrowserSidebarStateByProject] = useState<
    Record<string, BrowserSidebarProjectState>
  >({})
  const browserSidebarState =
    browserSidebarStateByProject[browserProjectKey] ?? DEFAULT_BROWSER_SIDEBAR_PROJECT_STATE
  const browserOpen = browserSidebarState.open
  const browserFullWidth = browserSidebarState.fullWidth
  const updateBrowserSidebarState = useCallback(
    (update: (current: BrowserSidebarProjectState) => BrowserSidebarProjectState) => {
      setBrowserSidebarStateByProject((current) => {
        const previous =
          current[browserProjectKey] ?? DEFAULT_BROWSER_SIDEBAR_PROJECT_STATE
        const next = update(previous)
        if (next.open === previous.open && next.fullWidth === previous.fullWidth) {
          return current
        }
        return { ...current, [browserProjectKey]: next }
      })
    },
    [browserProjectKey],
  )
  const setBrowserOpen = useCallback(
    (open: boolean) => {
      updateBrowserSidebarState((current) => ({
        ...current,
        open,
      }))
    },
    [updateBrowserSidebarState],
  )
  const setBrowserFullWidth = useCallback(
    (fullWidth: boolean) => {
      updateBrowserSidebarState((current) => ({
        ...current,
        fullWidth,
      }))
    },
    [updateBrowserSidebarState],
  )
  const [browserFullWidthTarget, setBrowserFullWidthTarget] =
    useState<number | null>(readBrowserFocusWidth)
  const [browserLaunchTargets, setBrowserLaunchTargets] = useState<BrowserLaunchTarget[]>([])
  const activeBrowserLaunchTargets = useMemo(
    () =>
      browserLaunchTargets.filter((target) =>
        activeProjectId ? target.projectId === activeProjectId : !target.projectId,
      ),
    [activeProjectId, browserLaunchTargets],
  )
  const [pendingBrowserOpenUrl, setPendingBrowserOpenUrl] = useState<PendingBrowserOpenUrl | null>(null)
  const [iosOpen, setIosOpen] = useState(false)
  const [vcsOpen, setVcsOpen] = useState(false)
  const [workflowsOpen, setWorkflowsOpen] = useState(false)
  const [workflowDefinitions, setWorkflowDefinitions] = useState<WorkflowDefinitionSummaryDto[]>([])
  const [workflowDefinitionsStatus, setWorkflowDefinitionsStatus] =
    useState<'idle' | 'loading' | 'ready' | 'error'>('idle')
  const [workflowDefinitionsError, setWorkflowDefinitionsError] = useState<Error | null>(null)
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRunDto[]>([])
  const [workflowRunsStatus, setWorkflowRunsStatus] =
    useState<'idle' | 'loading' | 'ready' | 'error'>('idle')
  const [selectedWorkflowDefinition, setSelectedWorkflowDefinition] =
    useState<WorkflowDefinitionDto | null>(null)
  const [selectedWorkflowRun, setSelectedWorkflowRun] = useState<WorkflowRunDto | null>(null)
  const [selectedWorkflowIsDraft, setSelectedWorkflowIsDraft] = useState(false)
  const [selectedWorkflowTemplatePreviewId, setSelectedWorkflowTemplatePreviewId] =
    useState<WorkflowTemplateIdDto | null>(null)
  const [workflowActionRunning, setWorkflowActionRunning] = useState(false)
  const workflowDefinitionsRequestSequenceRef = useRef(0)
  const workflowRunsRequestSequenceRef = useRef(0)
  const workflowSelectionRequestSequenceRef = useRef(0)
  const workflowStartRequestSequenceRef = useRef(0)
  const composerTemplateMaterializationRef = useRef<{
    projectId: string
    templateId: WorkflowTemplateIdDto
    definition: WorkflowDefinitionDto
  } | null>(null)
  const [workflowStartRequest, setWorkflowStartRequest] = useState<{
    token: number
    workflowId: string
  } | null>(null)
  const [workflowStartIdempotency] = useState(
    () => new WorkflowStartIdempotencyCoordinator(createWorkflowStartIdempotencyKey),
  )
  const workflowAgentInspector = useWorkflowAgentInspector({
    adapter: resolvedAdapter,
    projectId: activeProjectId,
  })
  const [usageOpen, setUsageOpen] = useState(false)
  const [notificationsOpen, setNotificationsOpen] = useState(false)
  const [agentDockOpen, setAgentDockOpen] = useState(false)
  const [computerUseOpen, setComputerUseOpen] = useState(false)
  const [pendingBrowserComposerInsert, setPendingBrowserComposerInsert] =
    useState<PendingBrowserComposerInsert | null>(null)
  const [computerUseProject, setComputerUseProject] = useState<ProjectDetailView | null>(null)
  const [computerUseRuntimeSession, setComputerUseRuntimeSession] =
    useState<RuntimeSessionView | null>(null)
  const [computerUseRuntimeRun, setComputerUseRuntimeRun] = useState<RuntimeRunView | null>(null)
  const [computerUseRuntimeRunActionStatus, setComputerUseRuntimeRunActionStatus] =
    useState<RuntimeRunActionStatus>('idle')
  const [computerUsePendingRuntimeRunAction, setComputerUsePendingRuntimeRunAction] =
    useState<RuntimeRunActionKind | null>(null)
  const [computerUseRuntimeRunActionError, setComputerUseRuntimeRunActionError] =
    useState<OperatorActionErrorView | null>(null)
  const [computerUseClearChatPending, setComputerUseClearChatPending] = useState(false)
  const computerUseRuntimeActionRefreshKeysRef = useRef<Record<string, Set<string>>>({})
  const computerUseRuntimeMetadataRefreshTimeoutRef = useRef<number | null>(null)
  const computerUseProjectLoadPromiseRef = useRef<Promise<ComputerUseLoadResult> | null>(null)
  const pendingAgentDockOpenTimeoutRef = useRef<number | null>(null)
  const pendingComputerUseOpenTimeoutRef = useRef<number | null>(null)
  const [terminalOpen, setTerminalOpen] = useState(false)
  const [startTargetsDialogOpen, setStartTargetsDialogOpen] = useState(false)
  const [pendingInitialRuntimeAgent, setPendingInitialRuntimeAgent] =
    useState<PendingInitialAgentSelection | null>(null)
  const [pendingInitialWorkflow, setPendingInitialWorkflow] =
    useState<PendingInitialWorkflowSelection | null>(null)
  const [pendingWorkflowPaneSelection, setPendingWorkflowPaneSelection] =
    useState<PendingWorkflowPaneSelection | null>(null)
  const [agentAuthoringSession, setAgentAuthoringSession] = useState<{
    projectId: string
    mode: 'create' | 'edit' | 'duplicate'
    initialDetail: WorkflowAgentDetailDto | null
  } | null>(null)
  const [agentAuthoringLoading, setAgentAuthoringLoading] = useState(false)
  const [agentAuthoringCatalogBinding, setAgentAuthoringCatalogBinding] = useState<{
    projectId: string
    catalog: AgentAuthoringCatalogDto
  } | null>(null)
  const [agentToolPackCatalogBinding, setAgentToolPackCatalogBinding] = useState<{
    projectId: string
    catalog: AgentToolPackCatalogDto
  } | null>(null)
  const activeProjectIdRef = useRef(activeProjectId)
  const computerUseContinuationCoordinatorRef =
    useRef<ContinuationIdempotencyCoordinator | null>(null)
  const computerUseContinuationCoordinator =
    computerUseContinuationCoordinatorRef.current ??
    new ContinuationIdempotencyCoordinator(createRuntimeContinuationRequestId, {
      storage: browserIdempotencyStorage(),
    })
  computerUseContinuationCoordinatorRef.current = computerUseContinuationCoordinator
  useLayoutEffect(() => {
    activeProjectIdRef.current = activeProjectId
    workflowDefinitionsRequestSequenceRef.current += 1
    workflowRunsRequestSequenceRef.current += 1
    setWorkflowDefinitions([])
    setWorkflowRuns([])
  }, [activeProjectId])
  const agentAuthoringDetailRequestRef = useRef(0)
  const activeAgentAuthoringSession =
    agentAuthoringSession?.projectId === activeProjectId ? agentAuthoringSession : null
  const agentAuthoringCatalog =
    agentAuthoringCatalogBinding?.projectId === activeProjectId
      ? agentAuthoringCatalogBinding.catalog
      : null
  const agentToolPackCatalog =
    agentToolPackCatalogBinding?.projectId === activeProjectId
      ? agentToolPackCatalogBinding.catalog
      : null
  const [environmentDiscoveryStatus, setEnvironmentDiscoveryStatus] =
    useState<EnvironmentDiscoveryStatusDto | null>(null)
  const [environmentProfileSummary, setEnvironmentProfileSummary] =
    useState<EnvironmentProfileSummaryDto>(null)
  const environmentDiscoveryCheckedRef = useRef(false)
  const [customAgentDefinitions, setCustomAgentDefinitions] = useState<
    readonly AgentDefinitionSummaryDto[]
  >([])
  const [customAgentDefinitionsRevision, setCustomAgentDefinitionsRevision] = useState(0)
  const refreshCustomAgentDefinitions = useCallback(() => {
    setCustomAgentDefinitionsRevision((current) => current + 1)
  }, [])
  useEffect(() => {
    agentAuthoringDetailRequestRef.current += 1
    setAgentAuthoringSession(null)
    setAgentAuthoringCatalogBinding(null)
    setAgentToolPackCatalogBinding(null)
    setAgentAuthoringLoading(false)
  }, [activeProjectId])
  const editorAgentActivities = useMemo(
    () =>
      buildEditorAgentActivities(
        agentWorkspacePanes.map((pane) => ({
          paneId: pane.paneId,
          sessionTitle: pane.agent.project.selectedAgentSession?.title ?? null,
          runtimeStreamItems: pane.agent.runtimeStreamItems ?? [],
        })),
      ),
    [agentWorkspacePanes],
  )
  const agentDefaultModels = useMemo(() => {
    const defaults: Record<string, AgentDefaultModelDto | null> = {}
    for (const agent of workflowAgentInspector.agents) {
      if (agent.ref.kind === 'built_in') {
        defaults[buildComposerAgentSelectionKey(agent.ref.runtimeAgentId, null)] =
          agent.defaultModel ?? null
      } else {
        defaults[
          buildComposerAgentSelectionKey(
            runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile),
            agent.ref.definitionId,
          )
        ] = agent.defaultModel ?? null
      }
    }
    for (const definition of customAgentDefinitions) {
      defaults[
        buildComposerAgentSelectionKey(
          runtimeAgentIdForCustomBaseCapability(definition.baseCapabilityProfile),
          definition.definitionId,
        )
      ] = definition.defaultModel ?? null
    }
    return defaults
  }, [customAgentDefinitions, workflowAgentInspector.agents])
  const refreshWorkflowDefinitions = useCallback(async () => {
    workflowDefinitionsRequestSequenceRef.current += 1
    const requestSequence = workflowDefinitionsRequestSequenceRef.current
    if (!WORKFLOWS_ENABLED || !activeProjectId || !resolvedAdapter.listWorkflowDefinitions) {
      setWorkflowDefinitions([])
      setWorkflowDefinitionsStatus('idle')
      setWorkflowDefinitionsError(null)
      return
    }
    const requestProjectId = activeProjectId
    setWorkflowDefinitionsStatus('loading')
    setWorkflowDefinitionsError(null)
    try {
      const response = await resolvedAdapter.listWorkflowDefinitions({ projectId: activeProjectId })
      if (
        activeProjectIdRef.current !== requestProjectId ||
        workflowDefinitionsRequestSequenceRef.current !== requestSequence
      ) return
      setWorkflowDefinitions(response.definitions)
      setWorkflowDefinitionsStatus('ready')
    } catch (error) {
      if (
        activeProjectIdRef.current !== requestProjectId ||
        workflowDefinitionsRequestSequenceRef.current !== requestSequence
      ) return
      setWorkflowDefinitionsError(error instanceof Error ? error : new Error(String(error)))
      setWorkflowDefinitionsStatus('error')
    }
  }, [activeProjectId, resolvedAdapter])

  const refreshWorkflowRuns = useCallback(async () => {
    workflowRunsRequestSequenceRef.current += 1
    const requestSequence = workflowRunsRequestSequenceRef.current
    if (!WORKFLOWS_ENABLED || !activeProjectId || !resolvedAdapter.listWorkflowRuns) {
      setWorkflowRuns([])
      setWorkflowRunsStatus('idle')
      return
    }
    const requestProjectId = activeProjectId
    setWorkflowRunsStatus('loading')
    try {
      const response = await resolvedAdapter.listWorkflowRuns({ projectId: activeProjectId })
      if (
        activeProjectIdRef.current !== requestProjectId ||
        workflowRunsRequestSequenceRef.current !== requestSequence
      ) return
      setWorkflowRuns(response.runs)
      setWorkflowRunsStatus('ready')
      setSelectedWorkflowRun((current) =>
        current ? response.runs.find((run) => run.id === current.id) ?? current : current,
      )
    } catch {
      if (
        activeProjectIdRef.current !== requestProjectId ||
        workflowRunsRequestSequenceRef.current !== requestSequence
      ) return
      setWorkflowRunsStatus('error')
    }
  }, [activeProjectId, resolvedAdapter])

  useEffect(() => {
    workflowSelectionRequestSequenceRef.current += 1
    workflowStartRequestSequenceRef.current += 1
    composerTemplateMaterializationRef.current = null
    setPendingInitialWorkflow(null)
    setPendingWorkflowPaneSelection(null)
    setWorkflowStartRequest(null)
    setSelectedWorkflowDefinition(null)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(false)
    setSelectedWorkflowTemplatePreviewId(null)
    void refreshWorkflowDefinitions()
    void refreshWorkflowRuns()
  }, [activeProjectId, refreshWorkflowDefinitions, refreshWorkflowRuns])

  useEffect(() => {
    if (!pendingWorkflowPaneSelection) return
    if (pendingWorkflowPaneSelection.projectId !== activeProjectId) {
      setPendingWorkflowPaneSelection(null)
      return
    }

    const pane = agentWorkspaceLayout?.paneSlots.find(
      (slot) => slot.id === pendingWorkflowPaneSelection.paneId,
    )
    if (!pane) {
      setPendingWorkflowPaneSelection(null)
      return
    }
    if (!pane.agentSessionId) return

    setPendingInitialWorkflow({
      projectId: pendingWorkflowPaneSelection.projectId,
      agentSessionId: pane.agentSessionId,
      target: pendingWorkflowPaneSelection.target,
    })
    setPendingWorkflowPaneSelection(null)
  }, [activeProjectId, agentWorkspaceLayout, pendingWorkflowPaneSelection])

  useEffect(() => {
    if (!WORKFLOWS_ENABLED || !activeProjectId || !resolvedAdapter.onWorkflowRunUpdated) return
    let cancelled = false
    let unlisten: (() => void) | null = null
    void resolvedAdapter
      .onWorkflowRunUpdated((payload) => {
        if (cancelled || payload.projectId !== activeProjectId) return
        setWorkflowRuns((current) => {
          const index = current.findIndex((run) => run.id === payload.run.id)
          if (index === -1) return [payload.run, ...current]
          const next = current.slice()
          next[index] = payload.run
          return next
        })
        setSelectedWorkflowRun((current) =>
          current?.id === payload.run.id ? payload.run : current,
        )
      })
      .then((dispose) => {
        if (cancelled) {
          dispose()
        } else {
          unlisten = dispose
        }
      })
      .catch((error) => {
        console.error('Failed to subscribe to workflow run updates', error)
      })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [activeProjectId, resolvedAdapter])
  const shouldRestoreExplorerFromAutoCollapseRef = useRef(false)
  const previousBrowserOpenRef = useRef<boolean>(browserOpen)

  useEffect(() => {
    if (typeof window === 'undefined') return

    const updateBrowserFullWidthTarget = () => {
      setBrowserFullWidthTarget(readBrowserFocusWidth())
    }

    updateBrowserFullWidthTarget()
    window.addEventListener('resize', updateBrowserFullWidthTarget)
    return () => window.removeEventListener('resize', updateBrowserFullWidthTarget)
  }, [])

  useEffect(() => {
    if (browserOpen) return
    setBrowserFullWidth(false)
  }, [browserOpen, setBrowserFullWidth])

  useEffect(() => {
    let cancelled = false
    void readToolCallGroupingPreference(resolvedAdapter)
      .then((preference) => {
        if (cancelled) return
        setToolCallGroupingPreference(preference)
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
    }
  }, [resolvedAdapter])

  useEffect(() => {
    let cancelled = false
    void readAgentRoutingAutoSwitchPreference(resolvedAdapter)
      .then((enabled) => {
        if (cancelled) return
        setAgentRoutingAutoSwitchEnabled(enabled)
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
    }
  }, [resolvedAdapter])

  const handleToolCallGroupingPreferenceChange = useCallback(
    async (preference: ToolCallGroupingPreference) => {
      const previousPreference = toolCallGroupingPreference
      setToolCallGroupingPreference(preference)
      try {
        await persistToolCallGroupingPreference(resolvedAdapter, preference)
      } catch (error) {
        setToolCallGroupingPreference(previousPreference)
        writeStoredToolCallGroupingPreference(previousPreference)
        throw error
      }
    },
    [resolvedAdapter, toolCallGroupingPreference],
  )

  const handleAgentRoutingAutoSwitchChange = useCallback(
    async (enabled: boolean) => {
      const previousPreference = agentRoutingAutoSwitchEnabled
      setAgentRoutingAutoSwitchEnabled(enabled)
      try {
        await persistAgentRoutingAutoSwitchPreference(resolvedAdapter, enabled)
      } catch (error) {
        setAgentRoutingAutoSwitchEnabled(previousPreference)
        writeStoredAgentRoutingAutoSwitchPreference(previousPreference)
        throw error
      }
    },
    [agentRoutingAutoSwitchEnabled, resolvedAdapter],
  )

  useEffect(() => {
    setAgentComposerControls(null)
  }, [activeProjectId])

  useEffect(() => {
    if (!activeProjectId) {
      setCustomAgentDefinitions([])
      return
    }

    let cancelled = false
    void resolvedAdapter
      .listAgentDefinitions({ projectId: activeProjectId, includeArchived: false })
      .then((response) => {
        if (cancelled) return
        const customs = response.definitions.filter((definition) => !definition.isBuiltIn)
        setCustomAgentDefinitions(customs)
      })
      .catch(() => {
        if (cancelled) return
        setCustomAgentDefinitions([])
      })

    return () => {
      cancelled = true
    }
  }, [activeProjectId, customAgentDefinitionsRevision, resolvedAdapter])

  const openSettings = useCallback((section: SettingsSection = 'account') => {
    preloadSurfaceChunk('settings')
    setSettingsInitialSection(section)
    setSettingsOpen(true)
  }, [])

  const refreshEnvironmentDiscovery = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (!resolvedAdapter.getEnvironmentDiscoveryStatus) {
        return null
      }

      let status =
        options.force && resolvedAdapter.refreshEnvironmentDiscovery
          ? await resolvedAdapter.refreshEnvironmentDiscovery()
          : options.force && resolvedAdapter.startEnvironmentDiscovery
            ? await resolvedAdapter.startEnvironmentDiscovery()
            : await resolvedAdapter.getEnvironmentDiscoveryStatus()

      if (
        !options.force &&
        status.shouldStart &&
        resolvedAdapter.startEnvironmentDiscovery
      ) {
        status = await resolvedAdapter.startEnvironmentDiscovery()
      }

      setEnvironmentDiscoveryStatus(status)
      if (resolvedAdapter.getEnvironmentProfileSummary) {
        const summary = await resolvedAdapter.getEnvironmentProfileSummary()
        setEnvironmentProfileSummary(summary)
      }
      return status
    },
    [resolvedAdapter],
  )

  const verifyUserEnvironmentTool = useCallback(
    async (request: VerifyUserToolRequestDto): Promise<VerifyUserToolResponseDto | null> => {
      if (!resolvedAdapter.verifyUserEnvironmentTool) {
        return null
      }
      return resolvedAdapter.verifyUserEnvironmentTool(request)
    },
    [resolvedAdapter],
  )

  const saveUserEnvironmentTool = useCallback(
    async (request: VerifyUserToolRequestDto): Promise<EnvironmentProbeReportDto | null> => {
      if (!resolvedAdapter.saveUserEnvironmentTool) {
        return null
      }
      const report = await resolvedAdapter.saveUserEnvironmentTool(request)
      setEnvironmentProfileSummary(report.summary)
      if (resolvedAdapter.getEnvironmentDiscoveryStatus) {
        const status = await resolvedAdapter.getEnvironmentDiscoveryStatus()
        setEnvironmentDiscoveryStatus(status)
      }
      return report
    },
    [resolvedAdapter],
  )

  const removeUserEnvironmentTool = useCallback(
    async (id: string): Promise<EnvironmentProbeReportDto | null> => {
      if (!resolvedAdapter.removeUserEnvironmentTool) {
        return null
      }
      const report = await resolvedAdapter.removeUserEnvironmentTool(id)
      setEnvironmentProfileSummary(report.summary)
      if (resolvedAdapter.getEnvironmentDiscoveryStatus) {
        const status = await resolvedAdapter.getEnvironmentDiscoveryStatus()
        setEnvironmentDiscoveryStatus(status)
      }
      return report
    },
    [resolvedAdapter],
  )

  const resolveEnvironmentPermissions = useCallback(
    async (
      decisions: Array<{
        id: string
        status: 'granted' | 'denied' | 'skipped'
      }>,
    ) => {
      if (!resolvedAdapter.resolveEnvironmentPermissionRequests) {
        return null
      }
      const status = await resolvedAdapter.resolveEnvironmentPermissionRequests({ decisions })
      setEnvironmentDiscoveryStatus(status)
      if (resolvedAdapter.getEnvironmentProfileSummary) {
        const summary = await resolvedAdapter.getEnvironmentProfileSummary()
        setEnvironmentProfileSummary(summary)
      }
      return status
    },
    [resolvedAdapter],
  )

  const clearPendingAgentDockOpen = useCallback(() => {
    let clearedComputerUseOpen = false
    if (pendingAgentDockOpenTimeoutRef.current === null) {
      if (pendingComputerUseOpenTimeoutRef.current !== null) {
        window.clearTimeout(pendingComputerUseOpenTimeoutRef.current)
        pendingComputerUseOpenTimeoutRef.current = null
        clearedComputerUseOpen = true
      }
      if (clearedComputerUseOpen) {
        setIsCreatingAgentSession(false)
      }
      return
    }
    window.clearTimeout(pendingAgentDockOpenTimeoutRef.current)
    pendingAgentDockOpenTimeoutRef.current = null
    if (pendingComputerUseOpenTimeoutRef.current !== null) {
      window.clearTimeout(pendingComputerUseOpenTimeoutRef.current)
      pendingComputerUseOpenTimeoutRef.current = null
      clearedComputerUseOpen = true
    }
    if (clearedComputerUseOpen) {
      setIsCreatingAgentSession(false)
    }
  }, [])

  useEffect(() => clearPendingAgentDockOpen, [clearPendingAgentDockOpen])

  const closeSidebarsExcept = useCallback((except: AppSidebarSurface | null = null) => {
    if (except !== 'browser') setBrowserOpen(false)
    if (except !== 'ios') setIosOpen(false)
    if (except !== 'vcs') setVcsOpen(false)
    if (except !== 'workflows') setWorkflowsOpen(false)
    if (except !== 'usage') setUsageOpen(false)
    if (except !== 'notifications') setNotificationsOpen(false)
    if (except !== 'agentDock') setAgentDockOpen(false)
    if (except !== 'computerUse') setComputerUseOpen(false)
    if (except !== 'terminal') setTerminalOpen(false)
  }, [setBrowserOpen])

  const toggleBrowser = useCallback(() => {
    clearPendingAgentDockOpen()
    if (browserOpen) {
      setBrowserFullWidth(false)
      setBrowserOpen(false)
      return
    }
    closeSidebarsExcept('browser')
    setBrowserFullWidth(false)
    setBrowserOpen(true)
  }, [
    browserOpen,
    clearPendingAgentDockOpen,
    closeSidebarsExcept,
    setBrowserFullWidth,
    setBrowserOpen,
  ])

  const revealBrowserSidebar = useCallback(() => {
    clearPendingAgentDockOpen()
    closeSidebarsExcept('browser')
    setBrowserOpen(true)
  }, [clearPendingAgentDockOpen, closeSidebarsExcept, setBrowserOpen])

  const handleBrowserFullWidthChange = useCallback(
    (nextFullWidth: boolean) => {
      clearPendingAgentDockOpen()
      if (nextFullWidth) {
        closeSidebarsExcept('browser')
        setBrowserOpen(true)
      }
      setBrowserFullWidth(nextFullWidth)
    },
    [clearPendingAgentDockOpen, closeSidebarsExcept, setBrowserFullWidth, setBrowserOpen],
  )

  const handleOpenUrlInBrowser = useCallback(
    (url: string) => {
      const id = `browser-open-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
      setPendingBrowserOpenUrl({ id, url })
      revealBrowserSidebar()
    },
    [revealBrowserSidebar],
  )

  const handleBrowserLaunchTargetDetected = useCallback((target: BrowserLaunchTarget) => {
    setBrowserLaunchTargets((current) => {
      const next = current.filter(
        (entry) => entry.id !== target.id || entry.projectId !== target.projectId,
      )
      next.unshift(target)
      return next.slice(0, 24)
    })
  }, [])

  const handleBrowserLaunchTargetUnavailable = useCallback((url: string) => {
    setBrowserLaunchTargets((current) =>
      current.filter(
        (target) =>
          target.projectId !== activeProjectId || !browserLaunchTargetMatchesUrl(target, url),
      ),
    )
  }, [activeProjectId])

  const handlePendingBrowserOpenUrlConsumed = useCallback((id: string) => {
    setPendingBrowserOpenUrl((current) => (current?.id === id ? null : current))
  }, [])

  const toggleIos = useCallback(() => {
    clearPendingAgentDockOpen()
    if (iosOpen) {
      setIosOpen(false)
      return
    }
    closeSidebarsExcept('ios')
    setIosOpen(true)
  }, [clearPendingAgentDockOpen, closeSidebarsExcept, iosOpen])

  const toggleVcs = useCallback(() => {
    clearPendingAgentDockOpen()
    if (vcsOpen) {
      setVcsOpen(false)
      return
    }
    closeSidebarsExcept('vcs')
    setVcsOpen(true)
  }, [clearPendingAgentDockOpen, closeSidebarsExcept, vcsOpen])

  const toggleWorkflows = useCallback(() => {
    clearPendingAgentDockOpen()
    if (workflowsOpen) {
      setWorkflowsOpen(false)
      return
    }
    closeSidebarsExcept('workflows')
    setWorkflowsOpen(true)
  }, [clearPendingAgentDockOpen, closeSidebarsExcept, workflowsOpen])

  const toggleAgentDock = useCallback(() => {
    clearPendingAgentDockOpen()
    if (agentDockOpen) {
      setAgentDockOpen(false)
      return
    }
    if (activeView === 'phases' && activeProject?.selectedAgentSessionId) {
      setPendingInitialRuntimeAgent({
        agentSessionId: activeProject.selectedAgentSessionId,
        runtimeAgentId: 'agent_create',
        agentDefinitionId: null,
      })
    }
    closeSidebarsExcept('agentDock')

    if (computerUseOpen) {
      setComputerUseOpen(false)
      setAgentDockOpen(false)
      pendingAgentDockOpenTimeoutRef.current = window.setTimeout(() => {
        pendingAgentDockOpenTimeoutRef.current = null
        setAgentDockOpen(true)
      }, SIDEBAR_WIDTH_DURATION_MS)
      return
    }

    setComputerUseOpen(false)
    setAgentDockOpen(true)
  }, [
    activeProject?.selectedAgentSessionId,
    activeView,
    agentDockOpen,
    clearPendingAgentDockOpen,
    closeSidebarsExcept,
    computerUseOpen,
  ])

  const loadComputerUseProject = useCallback(async (): Promise<ComputerUseLoadResult> => {
    await resolvedAdapter.ensureGlobalComputerUseSession?.()

    const bundle = resolvedAdapter.getProjectLoadBundle
      ? await resolvedAdapter.getProjectLoadBundle({
          projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
        })
      : null
    const snapshot = bundle
      ? bundle.projectSnapshot
      : await resolvedAdapter.getProjectSnapshot(GLOBAL_COMPUTER_USE_PROJECT_ID)
    const runtimeSession = bundle?.runtimeSession
      ? mapRuntimeSession(bundle.runtimeSession)
      : await resolvedAdapter
          .getRuntimeSession(GLOBAL_COMPUTER_USE_PROJECT_ID)
          .then(mapRuntimeSession)
          .catch(() => null)
    const runtimeRun = bundle?.runtimeRun
      ? mapRuntimeRun(bundle.runtimeRun)
      : await resolvedAdapter
          .getRuntimeRun(GLOBAL_COMPUTER_USE_PROJECT_ID, GLOBAL_COMPUTER_USE_AGENT_SESSION_ID)
          .then((run) => (run ? mapRuntimeRun(run) : null))
          .catch(() => null)
    const project = applyRuntimeRun(
      applyRuntimeSession(
        mapProjectSnapshot(snapshot),
        runtimeSession,
      ),
      runtimeRun,
    )

    setComputerUseProject(project)
    setComputerUseRuntimeSession(runtimeSession)
    setComputerUseRuntimeRun(runtimeRun)
    return { project, runtimeSession, runtimeRun }
  }, [resolvedAdapter])

  const preloadComputerUseProject = useCallback((): Promise<ComputerUseLoadResult> => {
    if (computerUseProject) {
      return Promise.resolve({
        project: computerUseProject,
        runtimeSession: computerUseRuntimeSession,
        runtimeRun: computerUseRuntimeRun,
      })
    }

    if (computerUseProjectLoadPromiseRef.current) {
      return computerUseProjectLoadPromiseRef.current
    }

    const loadPromise = loadComputerUseProject()
    computerUseProjectLoadPromiseRef.current = loadPromise
    loadPromise.then(
      () => {
        if (computerUseProjectLoadPromiseRef.current === loadPromise) {
          computerUseProjectLoadPromiseRef.current = null
        }
      },
      () => {
        if (computerUseProjectLoadPromiseRef.current === loadPromise) {
          computerUseProjectLoadPromiseRef.current = null
        }
      },
    )
    return loadPromise
  }, [
    computerUseProject,
    computerUseRuntimeRun,
    computerUseRuntimeSession,
    loadComputerUseProject,
  ])

  const refreshComputerUseRuntimeMetadata = useCallback(async () => {
    const [runtimeSession, runtimeRun] = await Promise.all([
      resolvedAdapter
        .getRuntimeSession(GLOBAL_COMPUTER_USE_PROJECT_ID)
        .then(mapRuntimeSession)
        .catch(() => null),
      resolvedAdapter
        .getRuntimeRun(GLOBAL_COMPUTER_USE_PROJECT_ID, GLOBAL_COMPUTER_USE_AGENT_SESSION_ID)
        .then((run) => (run ? mapRuntimeRun(run) : null))
        .catch(() => null),
    ])
    setComputerUseRuntimeSession(runtimeSession)
    setComputerUseRuntimeRun(runtimeRun)
    setComputerUseProject((current) =>
      current ? applyRuntimeRun(applyRuntimeSession(current, runtimeSession), runtimeRun) : current,
    )
    return { runtimeSession, runtimeRun }
  }, [resolvedAdapter])

  const scheduleComputerUseRuntimeMetadataRefresh = useCallback(
    (_projectId: string, _source: RuntimeMetadataRefreshSource) => {
      if (computerUseRuntimeMetadataRefreshTimeoutRef.current) {
        window.clearTimeout(computerUseRuntimeMetadataRefreshTimeoutRef.current)
      }
      computerUseRuntimeMetadataRefreshTimeoutRef.current = window.setTimeout(() => {
        computerUseRuntimeMetadataRefreshTimeoutRef.current = null
        void refreshComputerUseRuntimeMetadata()
      }, 24)
    },
    [refreshComputerUseRuntimeMetadata],
  )

  useEffect(() => {
    return () => {
      if (computerUseRuntimeMetadataRefreshTimeoutRef.current) {
        window.clearTimeout(computerUseRuntimeMetadataRefreshTimeoutRef.current)
        computerUseRuntimeMetadataRefreshTimeoutRef.current = null
      }
    }
  }, [])

  const updateComputerUseRuntimeStream = useCallback(
    (
      projectId: string,
      agentSessionId: string,
      updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null,
    ) => {
      highChurnStore.setRuntimeStreams((currentStreams) => {
        const sessionKey = createRuntimeStreamStoreKey(projectId, agentSessionId)
        const currentStream = currentStreams[sessionKey] ?? currentStreams[projectId] ?? null
        const nextStream = updater(currentStream)
        if (!nextStream) {
          return removeRuntimeStreamForSession(currentStreams, projectId, agentSessionId)
        }
        return {
          ...currentStreams,
          [sessionKey]: nextStream,
          [projectId]: nextStream,
        }
      })
    },
    [highChurnStore],
  )

  useEffect(() => {
    if (!computerUseOpen || !computerUseRuntimeSession || !computerUseRuntimeRun?.runId) {
      return
    }

    return attachRuntimeStreamSubscription({
      projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
      agentSessionId: GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
      runtimeSession: computerUseRuntimeSession,
      runId: computerUseRuntimeRun.runId,
      adapter: resolvedAdapter,
      runtimeActionRefreshKeysRef: computerUseRuntimeActionRefreshKeysRef,
      updateRuntimeStream: updateComputerUseRuntimeStream,
      scheduleRuntimeMetadataRefresh: scheduleComputerUseRuntimeMetadataRefresh,
    })
  }, [
    computerUseOpen,
    computerUseRuntimeRun?.runId,
    computerUseRuntimeSession,
    resolvedAdapter,
    scheduleComputerUseRuntimeMetadataRefresh,
    updateComputerUseRuntimeStream,
  ])

  const computerUseProjectForView = useMemo(
    () =>
      computerUseProject
        ? applyRuntimeRun(
            applyRuntimeSession(computerUseProject, computerUseRuntimeSession),
            computerUseRuntimeRun,
          )
        : null,
    [computerUseProject, computerUseRuntimeRun, computerUseRuntimeSession],
  )

  const computerUseAgentView = useMemo(
    () =>
      buildAgentView({
        project: computerUseProjectForView,
        activePhase: null,
        repositoryStatus: null,
        fallbackRuntimeAgentId: 'computer_use',
        providerCredentials,
        runtimeSession: computerUseRuntimeSession,
        providerModelCatalogs,
        providerModelCatalogLoadStatuses,
        providerModelCatalogLoadErrors,
        runtimeRun: computerUseRuntimeRun,
        autonomousRun: null,
        runtimeErrorMessage: null,
        runtimeRunErrorMessage: computerUseRuntimeRunActionError?.message ?? null,
        autonomousRunErrorMessage: null,
        runtimeStream: null,
        previousTrustSnapshot: null,
        operatorActionStatus: 'idle',
        pendingOperatorActionId: null,
        operatorActionError: null,
        autonomousRunActionStatus: 'idle',
        pendingAutonomousRunAction: null,
        autonomousRunActionError: null,
        runtimeRunActionStatus: computerUseRuntimeRunActionStatus,
        pendingRuntimeRunAction: computerUsePendingRuntimeRunAction,
        runtimeRunActionError: computerUseRuntimeRunActionError,
      }).view,
    [
      computerUsePendingRuntimeRunAction,
      computerUseProjectForView,
      computerUseRuntimeRun,
      computerUseRuntimeRunActionError,
      computerUseRuntimeRunActionStatus,
      computerUseRuntimeSession,
      providerCredentials,
      providerModelCatalogLoadErrors,
      providerModelCatalogLoadStatuses,
      providerModelCatalogs,
    ],
  )

  const computerUseRunning = Boolean(
    computerUseRuntimeRun?.isActive && !computerUseRuntimeRun.isTerminal,
  )

  const clearComputerUseChat = useCallback(async () => {
    if (
      !resolvedAdapter.resetGlobalComputerUseSession ||
      computerUseRunning ||
      computerUseClearChatPending
    ) {
      return
    }

    setComputerUseClearChatPending(true)
    setComputerUseRuntimeRunActionError(null)
    try {
      await resolvedAdapter.resetGlobalComputerUseSession()
      computerUseRuntimeActionRefreshKeysRef.current = {}
      highChurnStore.setRuntimeStreams((currentStreams) =>
        removeRuntimeStreamForSession(
          currentStreams,
          GLOBAL_COMPUTER_USE_PROJECT_ID,
          GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
        ),
      )
      setComputerUseRuntimeRun(null)
      await loadComputerUseProject()
    } catch (error) {
      setComputerUseRuntimeRunActionError(
        getOperatorActionError(error, 'Xero could not clear the Computer Use chat.'),
      )
    } finally {
      setComputerUseClearChatPending(false)
    }
  }, [
    computerUseClearChatPending,
    computerUseRunning,
    highChurnStore,
    loadComputerUseProject,
    resolvedAdapter,
  ])

  const canClearComputerUseChat = Boolean(resolvedAdapter.resetGlobalComputerUseSession) &&
    !computerUseRunning &&
    !computerUseClearChatPending
  const clearComputerUseChatTitle = !resolvedAdapter.resetGlobalComputerUseSession
    ? 'Clear chat is unavailable in this build.'
    : computerUseRunning
      ? 'Stop the current run before clearing chat'
      : computerUseClearChatPending
        ? 'Clearing Computer Use chat'
        : undefined

  const closeComputerUse = useCallback(() => {
    setComputerUseOpen(false)
  }, [])

  const toggleComputerUse = useCallback(() => {
    clearPendingAgentDockOpen()
    if (computerUseOpen) {
      closeComputerUse()
      return
    }

    preloadSurfaceChunk('agent-dock')
    setSelectedWorkflowDefinition(null)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(false)
    setSelectedWorkflowTemplatePreviewId(null)
    closeSidebarsExcept('computerUse')
    setIsCreatingAgentSession(true)
    const openComputerUse = () => {
      void (async () => {
        await preloadComputerUseProject()
        setComputerUseOpen(true)
      })()
        .catch(() => undefined)
        .finally(() => {
          setIsCreatingAgentSession(false)
        })
    }

    if (agentDockOpen) {
      pendingComputerUseOpenTimeoutRef.current = window.setTimeout(() => {
        pendingComputerUseOpenTimeoutRef.current = null
        openComputerUse()
      }, SIDEBAR_WIDTH_DURATION_MS)
      return
    }

    openComputerUse()
  }, [
    agentDockOpen,
    clearPendingAgentDockOpen,
    closeComputerUse,
    closeSidebarsExcept,
    computerUseOpen,
    preloadComputerUseProject,
  ])

  const toggleTerminal = useCallback(() => {
    clearPendingAgentDockOpen()
    if (terminalOpen) {
      setTerminalOpen(false)
      return
    }
    closeSidebarsExcept('terminal')
    setTerminalOpen(true)
  }, [clearPendingAgentDockOpen, closeSidebarsExcept, terminalOpen])
  useEffect(() => {
    if (activeView === 'agent' && agentDockOpen) {
      setAgentDockOpen(false)
    }
  }, [activeView, agentDockOpen])

  useEffect(() => {
    setBrowserLaunchTargets([])
    setPendingBrowserOpenUrl(null)
  }, [activeProjectId])

  // Imperative handle published by <TerminalSidebar>. We use it to spawn a
  // tab and write the project's start_command when the user clicks Play.
  const terminalSidebarHandleRef = useRef<TerminalSidebarHandle | null>(null)

  // Stable array reference so the start-targets editor's sync effect doesn't
  // clobber in-flight edits when App re-renders for unrelated reasons.
  const activeProjectStartTargets = useMemo(
    () => activeProject?.startTargets ?? [],
    [activeProject?.startTargets],
  )

  const handleEditProjectStartTargets = useCallback(() => {
    setStartTargetsDialogOpen(true)
  }, [])

  const revealTerminalSidebar = useCallback(() => {
    closeSidebarsExcept('terminal')
    setTerminalOpen(true)
  }, [closeSidebarsExcept])

  const waitForTerminalSidebarHandle = useCallback(async () => {
    for (let attempt = 0; attempt < 30; attempt += 1) {
      if (terminalSidebarHandleRef.current) return terminalSidebarHandleRef.current
      await new Promise((resolve) => window.setTimeout(resolve, 50))
    }
    return terminalSidebarHandleRef.current
  }, [])

  const handleRunTarget = useCallback(
    async (targetId: string) => {
      if (!activeProjectId) return
      const target = activeProjectStartTargets.find((entry) => entry.id === targetId)
      if (!target) {
        setStartTargetsDialogOpen(true)
        return
      }
      revealTerminalSidebar()
      const handle = await waitForTerminalSidebarHandle()
      if (!handle) return
      await handle.spawnTabWithCommand(target.command, {
        label: target.name,
        browserSupported: target.browserSupported,
        source: {
          kind: 'start-target',
          targetId: target.id,
          targetName: target.name,
        },
      })
    },
    [activeProjectId, activeProjectStartTargets, revealTerminalSidebar, waitForTerminalSidebarHandle],
  )

  const handleRunAllTargets = useCallback(async () => {
    if (!activeProjectId) return
    const targets = activeProjectStartTargets
    if (targets.length === 0) {
      setStartTargetsDialogOpen(true)
      return
    }
    revealTerminalSidebar()
    const handle = await waitForTerminalSidebarHandle()
    if (!handle) return
    for (const target of targets) {
      await handle.spawnTabWithCommand(target.command, {
        label: target.name,
        browserSupported: target.browserSupported,
        source: {
          kind: 'start-target',
          targetId: target.id,
          targetName: target.name,
        },
      })
    }
  }, [activeProjectId, activeProjectStartTargets, revealTerminalSidebar, waitForTerminalSidebarHandle])

  const handleRunEditorTerminalTask = useCallback(
    async (request: EditorTerminalTaskRequest) => {
      if (!activeProjectId) return null
      revealTerminalSidebar()
      const handle = await waitForTerminalSidebarHandle()
      if (!handle) return null
      return handle.spawnTabWithCommand(request.command, {
        label: request.label,
        exitWhenDone: request.exitWhenDone ?? true,
        source: {
          kind: 'editor-task',
          label: request.label,
        },
        onData: request.onData,
        onExit: request.onExit,
      })
    },
    [activeProjectId, revealTerminalSidebar, waitForTerminalSidebarHandle],
  )

  const handleUpdateProjectStartTargets = useCallback(
    async (
      targets: { id?: string | null; name: string; command: string; browserSupported?: boolean }[],
    ) => {
      if (!activeProjectId) return
      await resolvedAdapter.updateProjectStartTargets?.({
        projectId: activeProjectId,
        targets,
      })
      void prefetchProject?.(activeProjectId)
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleSuggestProjectStartTargets = useCallback(
    async (request: {
      modelId: string
      providerId?: string | null
      providerProfileId: string | null
      runtimeAgentId: RuntimeAgentIdDto | null
      thinkingEffort:
        | 'none'
        | 'minimal'
        | 'low'
        | 'medium'
        | 'high'
        | 'x_high'
        | null
    }) => {
      if (!activeProjectId) {
        throw new Error('No active project.')
      }
      if (!resolvedAdapter.suggestProjectStartTargets) {
        throw new Error('AI suggest is unavailable.')
      }
      const controls = projectRunnerSuggestControlsFromRequest(
        request,
        agentComposerControls,
      )
      if (controls) {
        persistComposerSettings(controls)
        setAgentComposerControls((current) =>
          sameRuntimeRunControlInput(current, controls) ? current : controls,
        )
      }
      const result = await resolvedAdapter.suggestProjectStartTargets({
        projectId: activeProjectId,
        modelId: request.modelId,
        providerId: request.providerId ?? null,
        providerProfileId: request.providerProfileId,
        runtimeAgentId: request.runtimeAgentId,
        thinkingEffort: request.thinkingEffort,
      })
      return { targets: result.targets }
    },
    [
      activeProjectId,
      agentComposerControls,
      persistComposerSettings,
      resolvedAdapter,
      setAgentComposerControls,
    ],
  )

  // Build the suggest request from the best available model route. The
  // provider profile travels with the model so an xAI model cannot be sent
  // through the OpenAI Codex profile when the agent pane has not mounted yet.
  const resolveProjectRunnerSuggestRequest = useCallback(() => {
    const controls = agentComposerControls
    if (controls && controls.modelId) {
      return {
        modelId: controls.modelId,
        providerId: null,
        providerProfileId: controls.providerProfileId ?? null,
        runtimeAgentId: controls.runtimeAgentId,
        thinkingEffort: controls.thinkingEffort ?? null,
      }
    }
    const selectedModelOption = agentView?.selectedModelOption
    if (selectedModelOption?.modelId) {
      return {
        modelId: selectedModelOption.modelId,
        providerId: selectedModelOption.providerId,
        providerProfileId:
          selectedModelOption.profileId ??
          getCloudProviderDefaultProfileId(selectedModelOption.providerId),
        runtimeAgentId: agentView?.selectedRuntimeAgentId ?? null,
        thinkingEffort:
          agentView?.selectedThinkingEffort ??
          selectedModelOption.defaultThinkingEffort ??
          null,
      }
    }

    try {
      if (typeof window !== 'undefined') {
        const raw = window.localStorage?.getItem?.('xero.agent.composer.settings.v1')
        if (raw) {
          const parsed = JSON.parse(raw) as Record<string, unknown>
          const request = projectRunnerSuggestRequestFromStoredComposerSettings(parsed)
          if (request) {
            return request
          }
        }
      }
    } catch {
      /* localStorage unavailable — fall through to empty defaults. */
    }
    return {
      modelId: '',
      providerId: null,
      providerProfileId: null,
      runtimeAgentId: null,
      thinkingEffort: null,
    }
  }, [
    agentComposerControls,
    agentView?.selectedModelOption,
    agentView?.selectedRuntimeAgentId,
    agentView?.selectedThinkingEffort,
  ])

  // ── Properties-panel ↔ sidebar coordination ──────────────────────────────
  // The inline node properties / details panel sits over the canvas on the
  // left. When it appears we collapse whichever right-side sidebar happens to
  // be open (sidebars are mutually exclusive, so at most one) so the panel
  // has a clean stage. We remember the collapsed one and reopen it when the
  // panel goes away — unless the user has since opened a different sidebar
  // themselves, in which case we leave their choice alone.
  const [phaseNodePanelOpen, setPhaseNodePanelOpen] = useState(false)
  const handlePhaseSelectedNodeChange = useCallback((hasSelection: boolean) => {
    setPhaseNodePanelOpen(hasSelection)
  }, [])
  const phaseSidebarRestoreKeyRef = useRef<
    AppSidebarSurface | null
  >(null)
  const phaseSidebarStateRef = useRef({
    browserOpen,
    computerUseOpen,
    iosOpen,
    notificationsOpen,
    terminalOpen,
    vcsOpen,
    workflowsOpen,
    usageOpen,
    agentDockOpen,
  })
  useEffect(() => {
    phaseSidebarStateRef.current = {
      browserOpen,
      computerUseOpen,
      iosOpen,
      notificationsOpen,
      terminalOpen,
      vcsOpen,
      workflowsOpen,
      usageOpen,
      agentDockOpen,
    }
  }, [
    agentDockOpen,
    browserOpen,
    computerUseOpen,
    iosOpen,
    notificationsOpen,
    terminalOpen,
    usageOpen,
    vcsOpen,
    workflowsOpen,
  ])
  useEffect(() => {
    if (phaseNodePanelOpen) {
      // Don't double-collapse if we already stashed a sidebar for this open.
      if (phaseSidebarRestoreKeyRef.current !== null) return
      const snapshot = phaseSidebarStateRef.current
      if (snapshot.browserOpen) {
        phaseSidebarRestoreKeyRef.current = 'browser'
        setBrowserOpen(false)
      } else if (snapshot.computerUseOpen) {
        phaseSidebarRestoreKeyRef.current = 'computerUse'
        setComputerUseOpen(false)
      } else if (snapshot.iosOpen) {
        phaseSidebarRestoreKeyRef.current = 'ios'
        setIosOpen(false)
      } else if (snapshot.notificationsOpen) {
        phaseSidebarRestoreKeyRef.current = 'notifications'
        setNotificationsOpen(false)
      } else if (snapshot.terminalOpen) {
        phaseSidebarRestoreKeyRef.current = 'terminal'
        setTerminalOpen(false)
      } else if (snapshot.vcsOpen) {
        phaseSidebarRestoreKeyRef.current = 'vcs'
        setVcsOpen(false)
      } else if (snapshot.workflowsOpen) {
        phaseSidebarRestoreKeyRef.current = 'workflows'
        setWorkflowsOpen(false)
      } else if (snapshot.usageOpen) {
        phaseSidebarRestoreKeyRef.current = 'usage'
        setUsageOpen(false)
      } else if (snapshot.agentDockOpen) {
        phaseSidebarRestoreKeyRef.current = 'agentDock'
        setAgentDockOpen(false)
      }
      return
    }
    const key = phaseSidebarRestoreKeyRef.current
    phaseSidebarRestoreKeyRef.current = null
    if (!key) return
    const snapshot = phaseSidebarStateRef.current
    const anySidebarOpen =
      snapshot.browserOpen ||
      snapshot.computerUseOpen ||
      snapshot.iosOpen ||
      snapshot.notificationsOpen ||
      snapshot.terminalOpen ||
      snapshot.vcsOpen ||
      snapshot.workflowsOpen ||
      snapshot.usageOpen ||
      snapshot.agentDockOpen
    // If the user has opened a different sidebar in the meantime, respect
    // that and don't reintroduce the auto-collapsed one (would break the
    // single-open invariant the toggle handlers maintain).
    if (anySidebarOpen) return
    if (key === 'browser') setBrowserOpen(true)
    else if (key === 'computerUse') setComputerUseOpen(true)
    else if (key === 'ios') setIosOpen(true)
    else if (key === 'notifications') setNotificationsOpen(true)
    else if (key === 'terminal') setTerminalOpen(true)
    else if (key === 'vcs') setVcsOpen(true)
    else if (key === 'workflows') setWorkflowsOpen(true)
    else if (key === 'usage') setUsageOpen(true)
    else if (key === 'agentDock') setAgentDockOpen(true)
  }, [phaseNodePanelOpen])
  const [explorerMode, setExplorerMode] = useState<'pinned' | 'collapsed'>(() => {
    if (typeof window === 'undefined') return 'pinned'
    try {
      const raw = window.localStorage.getItem('xero.explorer.collapsed')
      if (raw === '1' || raw === 'collapsed') return 'collapsed'
      return 'pinned'
    } catch {
      return 'pinned'
    }
  })
  const explorerCollapsed = explorerMode === 'collapsed'
  const setExplorerCollapsed = useCallback((next: boolean) => {
    setExplorerMode(next ? 'collapsed' : 'pinned')
  }, [])
  const [explorerPeeking, setExplorerPeeking] = useState(false)
  const peekTimerRef = useRef<number | null>(null)
  const clearPeekTimer = useCallback(() => {
    if (peekTimerRef.current !== null) {
      window.clearTimeout(peekTimerRef.current)
      peekTimerRef.current = null
    }
  }, [])
  const requestExplorerPeek = useCallback(() => {
    clearPeekTimer()
    setExplorerPeeking(true)
  }, [clearPeekTimer])
  const releaseExplorerPeek = useCallback(() => {
    clearPeekTimer()
    peekTimerRef.current = window.setTimeout(() => {
      peekTimerRef.current = null
      setExplorerPeeking(false)
    }, 150)
  }, [clearPeekTimer])
  const collapseExplorer = useCallback(() => {
    setExplorerCollapsed(true)
  }, [setExplorerCollapsed])
  const pinExplorer = useCallback(() => {
    clearPeekTimer()
    setExplorerPeeking(false)
    setExplorerMode('pinned')
  }, [clearPeekTimer])
  useEffect(() => () => clearPeekTimer(), [clearPeekTimer])
  useEffect(() => {
    if ((activeView !== 'agent' || explorerMode === 'pinned') && explorerPeeking) {
      clearPeekTimer()
      setExplorerPeeking(false)
    }
  }, [activeView, clearPeekTimer, explorerMode, explorerPeeking])

  useEffect(() => {
    if (typeof window === 'undefined') return
    try {
      window.localStorage.setItem(
        'xero.explorer.collapsed',
        explorerMode === 'collapsed' ? 'collapsed' : 'pinned',
      )
    } catch {
      /* storage unavailable — revert silently */
    }
  }, [explorerMode])

  const [platformOverride, setPlatformOverride] = useState<PlatformVariant | null>(null)
  const [onboardingDismissed, setOnboardingDismissed] = useState(false)
  const [onboardingCompletionHydrated, setOnboardingCompletionHydrated] = useState(
    () => !resolvedAdapter.readAppUiState,
  )
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const completeOnboarding = useCallback(() => {
    setOnboardingDismissed(true)
    setOnboardingOpen(false)
    void persistOnboardingCompleted(resolvedAdapter).catch(() => undefined)
  }, [resolvedAdapter])

  useEffect(() => {
    if (!resolvedAdapter.readAppUiState) {
      setOnboardingCompletionHydrated(true)
      return
    }

    let disposed = false
    setOnboardingCompletionHydrated(false)
    void readPersistedOnboardingCompleted(resolvedAdapter)
      .then((completed) => {
        if (disposed || !completed) {
          return
        }
        setOnboardingDismissed(true)
        setOnboardingOpen(false)
      })
      .catch(() => undefined)
      .finally(() => {
        if (disposed) {
          return
        }
        setOnboardingCompletionHydrated(true)
      })

    return () => {
      disposed = true
    }
  }, [resolvedAdapter])
  useEffect(() => {
    const wasBrowserOpen = previousBrowserOpenRef.current

    if (activeView === 'agent' && browserOpen && !wasBrowserOpen) {
      shouldRestoreExplorerFromAutoCollapseRef.current = !explorerCollapsed
      if (!explorerCollapsed) {
        setExplorerCollapsed(true)
      }
    } else if (
      !browserOpen &&
      wasBrowserOpen &&
      shouldRestoreExplorerFromAutoCollapseRef.current
    ) {
      shouldRestoreExplorerFromAutoCollapseRef.current = false
      if (explorerCollapsed) {
        setExplorerCollapsed(false)
      }
    }

    previousBrowserOpenRef.current = browserOpen
  }, [activeView, browserOpen, explorerCollapsed])

  const footerRepositoryStatus = repositoryStatus ?? activeProject?.repositoryStatus ?? null
  const footerLastCommit = footerRepositoryStatus?.lastCommit ?? null
  const footerSpend = summarizeProjectUsageSpend(activeUsageSummary)
  const statusFooter: StatusFooterProps = {
    git: activeProject
      ? {
          branch:
            footerRepositoryStatus?.branchLabel ??
            activeProject.repository?.branchLabel ??
            activeProject.branchLabel,
          upstream: footerRepositoryStatus?.upstream ?? null,
          hasChanges: footerRepositoryStatus?.hasChanges ?? false,
          changedFiles: footerRepositoryStatus?.statusCount ?? 0,
          lastCommit: footerLastCommit
            ? {
                sha: footerLastCommit.sha,
                message: footerLastCommit.summary,
                committedAt: footerLastCommit.committedAt,
              }
            : null,
        }
      : null,
    spend: footerSpend
      ? {
          totalTokens: footerSpend.totalTokens,
          totalCostMicros: footerSpend.totalCostMicros,
        }
      : null,
    notifications: unreadCompletedSessionCount,
    notificationsActive: notificationsOpen,
    onNotificationsClick: () => {
      if (notificationsOpen) {
        setNotificationsOpen(false)
        return
      }
      closeSidebarsExcept('notifications')
      setNotificationsOpen(true)
    },
    spendActive: usageOpen,
    onSpendClick: activeProjectId
      ? () => {
          preloadSurfaceChunk('usage')
          if (usageOpen) {
            setUsageOpen(false)
            return
          }
          closeSidebarsExcept('usage')
          setUsageOpen(true)
        }
      : undefined,
  }
  const vcsCommitMessageModel = useMemo(
    () =>
      getVcsCommitMessageModel(
        agentView,
        agentComposerControls,
        sourceControlSettings.commitMessageModelSelection,
      ),
    [agentComposerControls, agentView, sourceControlSettings.commitMessageModelSelection],
  )
  const projectRunnerModelOptions = useMemo(
    () => getProjectRunnerModelOptions(agentView),
    [agentView],
  )
  const memoryAdapter = useMemo(() => {
    if (
      !resolvedAdapter.getSessionMemoryItems ||
      !resolvedAdapter.updateSessionMemory ||
      !resolvedAdapter.correctSessionMemory ||
      !resolvedAdapter.deleteSessionMemory
    ) {
      return null
    }
    return {
      getQueue: resolvedAdapter.getSessionMemoryItems.bind(resolvedAdapter),
      updateMemory: resolvedAdapter.updateSessionMemory.bind(resolvedAdapter),
      correctMemory: resolvedAdapter.correctSessionMemory.bind(resolvedAdapter),
      deleteMemory: resolvedAdapter.deleteSessionMemory.bind(resolvedAdapter),
    }
  }, [resolvedAdapter])
  const projectStateAdapter = useMemo(() => {
    if (
      !resolvedAdapter.listProjectStateBackups ||
      !resolvedAdapter.createProjectStateBackup ||
      !resolvedAdapter.restoreProjectStateBackup ||
      !resolvedAdapter.repairProjectState
    ) {
      return null
    }
    return {
      listBackups: resolvedAdapter.listProjectStateBackups.bind(resolvedAdapter),
      createBackup: resolvedAdapter.createProjectStateBackup.bind(resolvedAdapter),
      restoreBackup: resolvedAdapter.restoreProjectStateBackup.bind(resolvedAdapter),
      repairProjectState: resolvedAdapter.repairProjectState.bind(resolvedAdapter),
    }
  }, [resolvedAdapter])
  const dangerAdapter = useMemo(() => {
    if (!resolvedAdapter.wipeProjectData || !resolvedAdapter.wipeAllXeroData) {
      return null
    }
    const wipeProject = resolvedAdapter.wipeProjectData.bind(resolvedAdapter)
    const wipeAll = resolvedAdapter.wipeAllXeroData.bind(resolvedAdapter)
    return {
      wipeProject: async (projectId: string) => {
        const response = await wipeProject({ projectId })
        void retry()
        return response
      },
      wipeAll: async () => {
        const response = await wipeAll()
        void retry()
        return response
      },
    }
  }, [resolvedAdapter, retry])
  const workflowAgentCreateActive =
    activeView === 'phases' && activeAgentAuthoringSession?.mode === 'create'
  const agentCreateCanvasIncluded =
    activeView === 'phases' &&
    (activeAgentAuthoringSession?.mode === 'create' || selectedWorkflowDefinition !== null)
  const pendingAgentDockSelection =
    workflowAgentCreateActive && isCreatingAgentSession
      ? ({ runtimeAgentId: 'agent_create', agentDefinitionId: null } satisfies RuntimeAgentSelection)
      : pendingInitialRuntimeAgent &&
          pendingInitialRuntimeAgent.agentSessionId === activeProject?.selectedAgentSessionId
        ? {
            runtimeAgentId: pendingInitialRuntimeAgent.runtimeAgentId,
            agentDefinitionId: pendingInitialRuntimeAgent.agentDefinitionId,
          }
        : null
  const pendingAgentDockRuntimeAgentId: RuntimeAgentIdDto | null =
    pendingAgentDockSelection?.runtimeAgentId ?? null
  const pendingAgentDockAgentDefinitionId: string | null =
    pendingAgentDockSelection?.agentDefinitionId ?? null

  useEffect(() => {
    if (
      onboardingCompletionHydrated &&
      !onboardingDismissed &&
      !isLoading &&
      projects.length === 0
    ) {
      setOnboardingOpen(true)
    }
  }, [isLoading, onboardingCompletionHydrated, onboardingDismissed, projects.length])

  const selectedAgentSessionId = activeProject?.selectedAgentSessionId ?? null
  const currentAgentWorkspaceDisplay = useMemo<AgentWorkspaceDisplaySnapshot | null>(() => {
    if (!activeProject) {
      return null
    }

    return {
      project: activeProject,
      agentView,
      layout: agentWorkspaceLayout,
      panes: agentWorkspacePanes,
    }
  }, [activeProject, agentView, agentWorkspaceLayout, agentWorkspacePanes])
  const shouldHoldAgentWorkspaceForProjectSelection =
    activeView === 'agent' && pendingProjectSelectionId !== null
  const lastStableAgentWorkspaceDisplayRef = useRef<AgentWorkspaceDisplaySnapshot | null>(
    currentAgentWorkspaceDisplay,
  )

  useEffect(() => {
    if (shouldHoldAgentWorkspaceForProjectSelection || !currentAgentWorkspaceDisplay) {
      return
    }

    lastStableAgentWorkspaceDisplayRef.current = currentAgentWorkspaceDisplay
  }, [currentAgentWorkspaceDisplay, shouldHoldAgentWorkspaceForProjectSelection])

  const displayedAgentWorkspace =
    shouldHoldAgentWorkspaceForProjectSelection &&
      lastStableAgentWorkspaceDisplayRef.current
      ? lastStableAgentWorkspaceDisplayRef.current
      : currentAgentWorkspaceDisplay
  const displayedActiveProject = displayedAgentWorkspace?.project ?? activeProject
  const displayedAgentView = displayedAgentWorkspace?.agentView ?? agentView
  const displayedAgentWorkspaceLayout =
    displayedAgentWorkspace?.layout ?? agentWorkspaceLayout
  const displayedAgentWorkspacePanes =
    displayedAgentWorkspace?.panes ?? agentWorkspacePanes
  const displayedSelectedAgentSessionId =
    displayedActiveProject?.selectedAgentSessionId ?? selectedAgentSessionId
  const visibleAgentSessionIds = useMemo(() => {
    if (activeView !== 'agent') {
      return []
    }

    const paneSessionIds =
      displayedAgentWorkspaceLayout?.paneSlots
        .map((slot) => slot.agentSessionId)
        .filter((agentSessionId): agentSessionId is string => Boolean(agentSessionId)) ?? []

    return paneSessionIds.length > 0
      ? Array.from(new Set(paneSessionIds))
      : displayedSelectedAgentSessionId
        ? [displayedSelectedAgentSessionId]
        : []
  }, [activeView, displayedAgentWorkspaceLayout, displayedSelectedAgentSessionId])
  useEffect(() => {
    if (visibleAgentSessionIds.length === 0) {
      return
    }

    acknowledgeCompletedAgentSessions(visibleAgentSessionIds)
  }, [acknowledgeCompletedAgentSessions, unreadCompletedSessionCount, visibleAgentSessionIds])
  const handleOpenNotificationSession = useCallback(
    (projectId: string, agentSessionId: string) => {
      void (async () => {
        closeSidebarsExcept(null)
        setActiveView('agent')

        if (projectId !== activeProjectId) {
          await selectProject(projectId)
        }

        await selectAgentSession(agentSessionId)
        acknowledgeCompletedAgentSessions([agentSessionId], { projectId })
      })().catch(() => undefined)
    },
    [
      acknowledgeCompletedAgentSessions,
      activeProjectId,
      closeSidebarsExcept,
      selectAgentSession,
      selectProject,
    ],
  )
  const resolvePaneAgentSessionId = useCallback(
    (paneId: string): string | null => {
      const slot = agentWorkspaceLayout?.paneSlots.find((candidate) => candidate.id === paneId)
      if (slot) {
        return slot.agentSessionId
      }
      return selectedAgentSessionId
    },
    [agentWorkspaceLayout, selectedAgentSessionId],
  )
  const ensurePaneAgentSessionSelected = useCallback(
    async (paneId: string): Promise<boolean> => {
      if (!activeProjectId) return false
      const agentSessionId = resolvePaneAgentSessionId(paneId)
      if (!agentSessionId) return false
      if (agentSessionId === selectedAgentSessionId) return true
      await selectAgentSession(agentSessionId)
      return true
    },
    [activeProjectId, resolvePaneAgentSessionId, selectAgentSession, selectedAgentSessionId],
  )
  const paneCount = displayedAgentWorkspaceLayout?.paneSlots.length ?? 1
  const isMultiPane = paneCount > 1
  useEffect(() => {
    const livePaneIds = new Set(displayedAgentWorkspaceLayout?.paneSlots.map((slot) => slot.id) ?? [])
    setPaneCloseStates((current) => {
      let changed = false
      const next: Record<string, AgentPaneCloseState> = {}
      for (const [paneId, state] of Object.entries(current)) {
        if (livePaneIds.has(paneId)) {
          next[paneId] = state
        } else {
          changed = true
        }
      }
      return changed ? next : current
    })
    setPendingPaneCloseId((current) => (current && livePaneIds.has(current) ? current : null))
  }, [displayedAgentWorkspaceLayout])
  const sessionPaneAssignments = useMemo<Record<string, number>>(() => {
    const map: Record<string, number> = {}
    if (!displayedAgentWorkspaceLayout) return map
    displayedAgentWorkspaceLayout.paneSlots.forEach((slot, index) => {
      if (slot.agentSessionId) {
        map[slot.agentSessionId] = index + 1
      }
    })
    return map
  }, [displayedAgentWorkspaceLayout])
  const dndPaneSlots = useMemo(() => {
    if (!displayedAgentWorkspaceLayout || !displayedActiveProject) return []
    const projectLabel = displayedActiveProject.name ?? null
    return displayedAgentWorkspaceLayout.paneSlots.map((slot, index) => {
      const session = slot.agentSessionId
        ? displayedActiveProject.agentSessions.find(
            (entry) => entry.agentSessionId === slot.agentSessionId,
          ) ?? null
        : null
      return {
        id: slot.id,
        agentSessionId: slot.agentSessionId,
        title: session?.title?.trim() || (slot.agentSessionId ? 'New Chat' : 'Empty pane'),
        projectLabel,
        index,
      }
    })
  }, [displayedActiveProject, displayedAgentWorkspaceLayout])
  const agentCommandPalettePanes = useMemo(() => {
    if (!displayedAgentWorkspaceLayout || !displayedActiveProject) return []
    return displayedAgentWorkspaceLayout.paneSlots.map((slot, index) => {
      const session = displayedActiveProject.agentSessions.find(
        (candidate) => candidate.agentSessionId === slot.agentSessionId,
      )
      return {
        paneId: slot.id,
        paneNumber: index + 1,
        sessionTitle: session?.title ?? 'Untitled',
        isFocused: slot.id === displayedAgentWorkspaceLayout.focusedPaneId,
      }
    })
  }, [displayedActiveProject, displayedAgentWorkspaceLayout])
  const preSpawnExplorerModeRef = useRef<'pinned' | 'collapsed' | null>(null)
  useLayoutEffect(() => {
    if (activeView !== 'agent') return
    if (isMultiPane) {
      if (preSpawnExplorerModeRef.current === null) {
        preSpawnExplorerModeRef.current = explorerMode
      }
      if (explorerMode === 'pinned') {
        setExplorerMode('collapsed')
      }
    } else if (preSpawnExplorerModeRef.current !== null) {
      const restoreMode = preSpawnExplorerModeRef.current
      preSpawnExplorerModeRef.current = null
      if (restoreMode !== explorerMode) {
        setExplorerMode(restoreMode)
      }
    }
  }, [activeView, explorerMode, isMultiPane])
  const handleSelectAgentSession = useCallback(
    (agentSessionId: string) => {
      if (!activeProjectId) return
      const loadedPaneId = agentWorkspaceLayout?.paneSlots.find(
        (slot) => slot.agentSessionId === agentSessionId,
      )?.id ?? null
      if (loadedPaneId && loadedPaneId !== agentWorkspaceLayout?.focusedPaneId) {
        focusPane(loadedPaneId)
      }
      if (agentSessionId === selectedAgentSessionId) return
      void selectAgentSession(agentSessionId)
    },
    [activeProjectId, agentWorkspaceLayout, focusPane, selectAgentSession, selectedAgentSessionId],
  )

  const handleSpawnPane = useCallback(() => {
    if (!activeProjectId) return
    void spawnPane().catch(() => undefined)
  }, [activeProjectId, spawnPane])
  const getPaneCloseState = useCallback(
    (paneId: string): AgentPaneCloseState | null => {
      const reportedState = paneCloseStates[paneId]
      if (reportedState) {
        return reportedState
      }

      const pane = displayedAgentWorkspacePanes.find((candidate) => candidate.paneId === paneId)
      if (!pane) {
        return null
      }

      return {
        hasRunningRun: Boolean(pane.agent?.runtimeRun && !pane.agent.runtimeRun.isTerminal),
        hasUnsavedComposerText: false,
        sessionTitle: pane.agent?.project.selectedAgentSession?.title?.trim() || 'New Chat',
      }
    },
    [displayedAgentWorkspacePanes, paneCloseStates],
  )
  const handleClosePane = useCallback(
    (paneId: string) => {
      const closeState = getPaneCloseState(paneId)
      if (shouldConfirmPaneClose(closeState)) {
        setPendingPaneCloseId(paneId)
        return
      }
      closePane(paneId)
    },
    [closePane, getPaneCloseState],
  )
  const handleConfirmClosePane = useCallback(() => {
    if (!pendingPaneCloseId) {
      return
    }

    closePane(pendingPaneCloseId)
    setPendingPaneCloseId(null)
  }, [closePane, pendingPaneCloseId])
  const handlePaneCloseStateChange = useCallback(
    (paneId: string, state: AgentPaneCloseState) => {
      setPaneCloseStates((current) => {
        const previous = current[paneId]
        if (
          previous &&
          previous.hasRunningRun === state.hasRunningRun &&
          previous.hasUnsavedComposerText === state.hasUnsavedComposerText &&
          previous.sessionTitle === state.sessionTitle
        ) {
          return current
        }

        return {
          ...current,
          [paneId]: state,
        }
      })
    },
    [],
  )
  const pendingPaneCloseState = pendingPaneCloseId ? getPaneCloseState(pendingPaneCloseId) : null
  const pendingPaneCloseCopy = getPaneCloseConfirmationCopy(pendingPaneCloseState)
  const handleFocusPane = useCallback(
    (paneId: string) => {
      focusPane(paneId)
    },
    [focusPane],
  )
  const handleSplitterRatiosChange = useCallback(
    (arrangementKey: string, ratios: number[]) => {
      setSplitterRatios(arrangementKey, ratios)
    },
    [setSplitterRatios],
  )

  const focusPaneByIndex = useCallback(
    (index: number) => {
      const slot = agentWorkspaceLayout?.paneSlots[index]
      if (!slot) return
      focusPane(slot.id)
    },
    [agentWorkspaceLayout, focusPane],
  )
  const cycleFocusPane = useCallback(
    (delta: number) => {
      if (!agentWorkspaceLayout || agentWorkspaceLayout.paneSlots.length < 2) return
      const slots = agentWorkspaceLayout.paneSlots
      const currentIndex = slots.findIndex((slot) => slot.id === agentWorkspaceLayout.focusedPaneId)
      const nextIndex = (currentIndex + delta + slots.length) % slots.length
      const nextSlot = slots[nextIndex]
      if (nextSlot) focusPane(nextSlot.id)
    },
    [agentWorkspaceLayout, focusPane],
  )
  useEffect(() => {
    if (typeof window === 'undefined') return
    if (activeView !== 'agent') return

    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const isEditableTarget = Boolean(
        target &&
          (target.tagName === 'INPUT' ||
            target.tagName === 'TEXTAREA' ||
            target.isContentEditable),
      )
      const meta = event.metaKey || event.ctrlKey
      // ⌘1–⌘6 focus pane N (skip when editable focused so users keep number entry)
      if (meta && !event.shiftKey && !event.altKey && /^Digit[1-6]$/.test(event.code)) {
        if (isEditableTarget) return
        const index = Number(event.code.slice(-1)) - 1
        if (agentWorkspaceLayout && index < agentWorkspaceLayout.paneSlots.length) {
          event.preventDefault()
          focusPaneByIndex(index)
        }
        return
      }
      // ⌘⇧N spawn pane
      if (meta && event.shiftKey && (event.key === 'N' || event.key === 'n')) {
        event.preventDefault()
        handleSpawnPane()
        return
      }
      // ⌘W close focused pane
      if (meta && !event.shiftKey && (event.key === 'w' || event.key === 'W')) {
        if (!agentWorkspaceLayout || agentWorkspaceLayout.paneSlots.length <= 1) return
        const focusedId = agentWorkspaceLayout.focusedPaneId
        if (!focusedId) return
        event.preventDefault()
        handleClosePane(focusedId)
        return
      }
      // ⌥←/→ cycle focus
      if (event.altKey && !meta && !event.shiftKey) {
        if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
          if (isEditableTarget) return
          if (agentWorkspaceLayout && agentWorkspaceLayout.paneSlots.length > 1) {
            event.preventDefault()
            cycleFocusPane(1)
          }
        } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
          if (isEditableTarget) return
          if (agentWorkspaceLayout && agentWorkspaceLayout.paneSlots.length > 1) {
            event.preventDefault()
            cycleFocusPane(-1)
          }
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    activeView,
    agentWorkspaceLayout,
    cycleFocusPane,
    focusPaneByIndex,
    handleClosePane,
    handleSpawnPane,
  ])

  const handleCreateAgentSession = useCallback(() => {
    if (!activeProjectId) return
    setIsCreatingAgentSession(true)
    void createAgentSession().finally(() => {
      setIsCreatingAgentSession(false)
    })
  }, [activeProjectId, createAgentSession])

  const handleCreateAgent = useCallback(
    () => {
      if (!activeProjectId) return
      preloadSurfaceChunk('agent-dock')
      setSelectedWorkflowDefinition(null)
      setSelectedWorkflowRun(null)
      setSelectedWorkflowIsDraft(false)
      setSelectedWorkflowTemplatePreviewId(null)
      closeSidebarsExcept('agentDock')
      setActiveView('phases')
      agentAuthoringDetailRequestRef.current += 1
      setAgentAuthoringSession({
        projectId: activeProjectId,
        mode: 'create',
        initialDetail: null,
      })
      setAgentDockOpen(true)
      setIsCreatingAgentSession(true)
      void createAgentSession()
        .then((updatedProject) => {
          const newSessionId = updatedProject?.selectedAgentSessionId
          if (newSessionId) {
            setPendingInitialRuntimeAgent({
              agentSessionId: newSessionId,
              runtimeAgentId: 'agent_create',
              agentDefinitionId: null,
            })
          }
        })
        .finally(() => {
          setIsCreatingAgentSession(false)
        })
    },
    [activeProjectId, closeSidebarsExcept, createAgentSession],
  )

  const handleClearPendingInitialRuntimeAgent = useCallback(
    (agentSessionId: string) => {
      setPendingInitialRuntimeAgent((current) =>
        current?.agentSessionId === agentSessionId ? null : current,
      )
    },
    [],
  )

  const handleClearPendingInitialWorkflow = useCallback((agentSessionId: string) => {
    setPendingInitialWorkflow((current) =>
      current?.agentSessionId === agentSessionId ? null : current,
    )
  }, [])

  // Lazy-load the baseline authoring catalog once a session opens. Skills can
  // be expanded later by query-scoped online search from the picker.
  useEffect(() => {
    if (!activeAgentAuthoringSession) return
    if (!activeProjectId) return
    if (agentAuthoringCatalogBinding?.projectId === activeProjectId) return
    const projectId = activeProjectId
    let cancelled = false
    void resolvedAdapter
      .getAgentAuthoringCatalog({ projectId })
      .then((catalog) => {
        if (cancelled || activeProjectIdRef.current !== projectId) return
        setAgentAuthoringCatalogBinding({ projectId, catalog })
      })
      .catch((error: unknown) => {
        if (cancelled || activeProjectIdRef.current !== projectId) return
        console.error('Failed to load agent authoring catalog', error)
      })
    return () => {
      cancelled = true
    }
  }, [activeAgentAuthoringSession, activeProjectId, agentAuthoringCatalogBinding, resolvedAdapter])

  // Lazy-load the tool-pack catalog the same way. The granular policy editor
  // in the canvas properties panel consumes it for the pack picker and to
  // expand `allowedToolPacks` entries into the tools they grant.
  useEffect(() => {
    if (!activeAgentAuthoringSession) return
    if (!activeProjectId) return
    if (agentToolPackCatalogBinding?.projectId === activeProjectId) return
    if (!resolvedAdapter.getAgentToolPackCatalog) return
    const projectId = activeProjectId
    let cancelled = false
    void resolvedAdapter
      .getAgentToolPackCatalog({ projectId })
      .then((catalog) => {
        if (cancelled || activeProjectIdRef.current !== projectId) return
        setAgentToolPackCatalogBinding({ projectId, catalog })
      })
      .catch((error: unknown) => {
        if (cancelled || activeProjectIdRef.current !== projectId) return
        console.error('Failed to load agent tool-pack catalog', error)
      })
    return () => {
      cancelled = true
    }
  }, [activeAgentAuthoringSession, activeProjectId, agentToolPackCatalogBinding, resolvedAdapter])

  const handleSearchAttachableSkills = useCallback(
    async (params: {
      query: string
      offset: number
      limit: number
    }): Promise<SearchAgentAuthoringSkillsResponseDto> => {
      const emptyResponse: SearchAgentAuthoringSkillsResponseDto = {
        entries: [],
        offset: params.offset,
        limit: params.limit,
        nextOffset: null,
        hasMore: false,
      }
      if (!activeProjectId || !resolvedAdapter.searchAgentAuthoringSkills) return emptyResponse
      return resolvedAdapter.searchAgentAuthoringSkills({
        projectId: activeProjectId,
        query: params.query,
        offset: params.offset,
        limit: params.limit,
      })
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleResolveAttachableSkill = useCallback(
    async (result: AgentAuthoringSkillSearchResultDto): Promise<AgentAuthoringAttachableSkillDto> => {
      if (!activeProjectId || !resolvedAdapter.resolveAgentAuthoringSkill) {
        throw new Error('Online skill resolution is unavailable.')
      }
      const projectId = activeProjectId
      const skill = await resolvedAdapter.resolveAgentAuthoringSkill({
        projectId,
        source: result.source,
        skillId: result.skillId,
      })
      if (activeProjectIdRef.current !== projectId) return skill
      setAgentAuthoringCatalogBinding((current) => {
        if (!current || current.projectId !== projectId) return current
        const bySourceId = new Map<string, AgentAuthoringAttachableSkillDto>()
        for (const skill of current.catalog.attachableSkills) {
          bySourceId.set(skill.sourceId, skill)
        }
        bySourceId.set(skill.sourceId, skill)
        return {
          projectId,
          catalog: {
            ...current.catalog,
            attachableSkills: [...bySourceId.values()],
          },
        }
      })
      return skill
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleStartAgentAuthoringCreate = useCallback(() => {
    if (!activeProjectId) return
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setActiveView('phases')
    agentAuthoringDetailRequestRef.current += 1
    setAgentAuthoringSession({
      projectId: activeProjectId,
      mode: 'create',
      initialDetail: null,
    })
  }, [activeProjectId, setActiveView])

  const handleStartWorkflowAgentCreate = useCallback(() => {
    if (!activeProjectId) return
    preloadSurfaceChunk('agent-dock')
    const definition = instantiateBlankWorkflow({
      projectId: activeProjectId,
      name: 'New workflow',
    })
    setSelectedWorkflowDefinition(definition)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(true)
    setSelectedWorkflowTemplatePreviewId(null)
    setAgentAuthoringSession(null)
    workflowAgentInspector.selectAgent(null)
    closeSidebarsExcept('agentDock')
    setActiveView('phases')
    setAgentDockOpen(true)
    const selectAgentCreate = (agentSessionId: string) => {
      setPendingInitialRuntimeAgent({
        agentSessionId,
        runtimeAgentId: 'agent_create',
        agentDefinitionId: null,
      })
    }
    if (activeProject?.selectedAgentSessionId) {
      selectAgentCreate(activeProject.selectedAgentSessionId)
      return
    }
    setIsCreatingAgentSession(true)
    void createAgentSession()
      .then((updatedProject) => {
        const newSessionId = updatedProject?.selectedAgentSessionId
        if (newSessionId) selectAgentCreate(newSessionId)
      })
      .finally(() => {
        setIsCreatingAgentSession(false)
      })
  }, [
    activeProject?.selectedAgentSessionId,
    activeProjectId,
    closeSidebarsExcept,
    createAgentSession,
    setActiveView,
    workflowAgentInspector.selectAgent,
  ])

  const handleStartAgentAuthoringFromRef = useCallback(
    async (mode: 'edit' | 'duplicate', ref: AgentRefDto) => {
      if (!activeProjectId) return
      const projectId = activeProjectId
      const requestId = agentAuthoringDetailRequestRef.current + 1
      agentAuthoringDetailRequestRef.current = requestId
      setAgentAuthoringLoading(true)
      try {
        const detail = await resolvedAdapter.getWorkflowAgentDetail({
          projectId,
          ref,
        })
        if (
          agentAuthoringDetailRequestRef.current !== requestId ||
          activeProjectIdRef.current !== projectId
        ) {
          return
        }
        setWorkflowsOpen(false)
        setAgentDockOpen(false)
        setActiveView('phases')
        setAgentAuthoringSession({ projectId, mode, initialDetail: detail })
      } catch (error) {
        if (
          agentAuthoringDetailRequestRef.current !== requestId ||
          activeProjectIdRef.current !== projectId
        ) {
          return
        }
        console.error('Failed to load agent definition for authoring', error)
      } finally {
        if (
          agentAuthoringDetailRequestRef.current === requestId &&
          activeProjectIdRef.current === projectId
        ) {
          setAgentAuthoringLoading(false)
        }
      }
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleCreateAgentFromTemplate = useCallback(
    (ref: AgentRefDto) => {
      void handleStartAgentAuthoringFromRef('duplicate', ref)
    },
    [handleStartAgentAuthoringFromRef],
  )

  const handleCloseAgentAuthoring = useCallback(() => {
    agentAuthoringDetailRequestRef.current += 1
    setAgentAuthoringLoading(false)
    setAgentAuthoringSession(null)
  }, [])

  const handleAgentAuthoringSubmit = useCallback(
    async ({
      snapshot,
      mode,
      definitionId,
      dryRun,
    }: {
      snapshot: Record<string, unknown>
      mode: 'create' | 'edit' | 'duplicate'
      definitionId?: string
      dryRun?: boolean
    }) => {
      const authoringProjectId = activeAgentAuthoringSession?.projectId ?? null
      if (!activeProjectId || !authoringProjectId || authoringProjectId !== activeProjectId) {
        throw new Error('Select a project before saving an agent definition.')
      }
      const definition = canonicalCustomAgentDefinitionSchema.parse(snapshot)
      if (mode === 'edit' && definitionId) {
        return resolvedAdapter.updateAgentDefinition({
          projectId: authoringProjectId,
          definitionId,
          definition,
          dryRun: dryRun ?? false,
        })
      }
      return resolvedAdapter.saveAgentDefinition({
        projectId: authoringProjectId,
        definition,
        dryRun: dryRun ?? false,
      })
    },
    [activeAgentAuthoringSession, activeProjectId, resolvedAdapter],
  )

  const handleAgentAuthoringSaved = useCallback(() => {
    refreshCustomAgentDefinitions()
    void workflowAgentInspector.refreshAgents()
  }, [refreshCustomAgentDefinitions, workflowAgentInspector.refreshAgents])

  const handleClearWorkflowAgentSelection = useCallback(() => {
    workflowAgentInspector.selectAgent(null)
  }, [workflowAgentInspector.selectAgent])

  const handleSelectWorkflowDefinition = useCallback(
    async (workflowId: string, options?: { activateView?: boolean }) => {
      if (!activeProjectId || !resolvedAdapter.getWorkflowDefinition) return null
      const requestProjectId = activeProjectId
      workflowSelectionRequestSequenceRef.current += 1
      const selectionRequestSequence = workflowSelectionRequestSequenceRef.current
      try {
        const response = await resolvedAdapter.getWorkflowDefinition({
          projectId: activeProjectId,
          workflowId,
        })
        if (
          selectionRequestSequence !== workflowSelectionRequestSequenceRef.current ||
          activeProjectIdRef.current !== requestProjectId
        ) {
          return null
        }
        setWorkflowStartRequest(null)
        setSelectedWorkflowDefinition(response.definition)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        setAgentAuthoringSession(null)
        setSelectedWorkflowRun((current) =>
          current?.workflowId === workflowId ? current : workflowRuns.find((run) => run.workflowId === workflowId) ?? null,
        )
        workflowAgentInspector.selectAgent(null)
        if (options?.activateView !== false) {
          setActiveView('phases')
        }
        return response.definition
      } catch (error) {
        console.error('Failed to load workflow definition', error)
        return null
      }
    },
    [activeProjectId, resolvedAdapter, setActiveView, workflowAgentInspector, workflowRuns],
  )

  const requestWorkflowStartInput = useCallback((workflowId: string) => {
    workflowStartRequestSequenceRef.current += 1
    setWorkflowStartRequest({
      token: workflowStartRequestSequenceRef.current,
      workflowId,
    })
  }, [])

  const handleRequestWorkflowStart = useCallback(
    async (workflowId: string) => {
      const definition = await handleSelectWorkflowDefinition(workflowId)
      if (!definition) return
      requestWorkflowStartInput(definition.id)
    },
    [handleSelectWorkflowDefinition, requestWorkflowStartInput],
  )

  const handleWorkflowStartRequestHandled = useCallback((requestToken: number) => {
    setWorkflowStartRequest((currentRequest) =>
      currentRequest?.token === requestToken ? null : currentRequest,
    )
  }, [])

  const handleSelectWorkflowRun = useCallback(
    async (runId: string) => {
      if (!activeProjectId || !resolvedAdapter.getWorkflowRun) return
      const requestProjectId = activeProjectId
      workflowSelectionRequestSequenceRef.current += 1
      const selectionRequestSequence = workflowSelectionRequestSequenceRef.current
      try {
        const response = await resolvedAdapter.getWorkflowRun({ projectId: activeProjectId, runId })
        if (
          selectionRequestSequence !== workflowSelectionRequestSequenceRef.current ||
          activeProjectIdRef.current !== requestProjectId
        ) {
          return
        }
        setWorkflowStartRequest(null)
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        setAgentAuthoringSession(null)
        workflowAgentInspector.selectAgent(null)
        setActiveView('phases')
      } catch (error) {
        console.error('Failed to load workflow run', error)
      }
    },
    [activeProjectId, resolvedAdapter, setActiveView, workflowAgentInspector],
  )

  const handleCreateWorkflow = useCallback(() => {
    if (!activeProjectId) {
      closeSidebarsExcept('workflows')
      setWorkflowsOpen(true)
      return
    }
    workflowSelectionRequestSequenceRef.current += 1
    setWorkflowStartRequest(null)
    const definition = instantiateBlankWorkflow({
      projectId: activeProjectId,
      name: 'New workflow',
    })
    setSelectedWorkflowDefinition(definition)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(true)
    setSelectedWorkflowTemplatePreviewId(null)
    setAgentAuthoringSession(null)
    workflowAgentInspector.selectAgent(null)
    closeSidebarsExcept(null)
    setActiveView('phases')
  }, [
    activeProjectId,
    closeSidebarsExcept,
    setActiveView,
    workflowAgentInspector,
  ])

  const handleCreateWorkflowFromTemplate = useCallback(
    async (templateId: WorkflowTemplateIdDto) => {
      if (!activeProjectId) return
      workflowSelectionRequestSequenceRef.current += 1
      setWorkflowStartRequest(null)
      const definition = instantiateWorkflowTemplate({
        projectId: activeProjectId,
        templateId,
        agents: workflowAgentInspector.agents,
      })
      setSelectedWorkflowDefinition(definition)
      setSelectedWorkflowRun(null)
      setSelectedWorkflowIsDraft(true)
      setSelectedWorkflowTemplatePreviewId(null)
      setAgentAuthoringSession(null)
      workflowAgentInspector.selectAgent(null)
      closeSidebarsExcept(null)
      setActiveView('phases')
    },
    [
      activeProjectId,
      closeSidebarsExcept,
      setActiveView,
      workflowAgentInspector,
      workflowAgentInspector.agents,
    ],
  )

  const handlePreviewWorkflowTemplate = useCallback(
    (templateId: WorkflowTemplateIdDto) => {
      if (!activeProjectId) return
      workflowSelectionRequestSequenceRef.current += 1
      setWorkflowStartRequest(null)
      const definition = instantiateWorkflowTemplate({
        projectId: activeProjectId,
        templateId,
        agents: workflowAgentInspector.agents,
      })
      setSelectedWorkflowDefinition(definition)
      setSelectedWorkflowRun(null)
      setSelectedWorkflowIsDraft(false)
      setSelectedWorkflowTemplatePreviewId(templateId)
      setAgentAuthoringSession(null)
      workflowAgentInspector.selectAgent(null)
      setActiveView('phases')
    },
    [
      activeProjectId,
      setActiveView,
      workflowAgentInspector,
      workflowAgentInspector.agents,
    ],
  )

  const handleSaveWorkflowDefinition = useCallback(
    async (definition: WorkflowDefinitionDto) => {
      if (!activeProjectId) {
        throw new Error('Select a project before saving a Workflow.')
      }
      setWorkflowActionRunning(true)
      try {
        const response = selectedWorkflowIsDraft
          ? await resolvedAdapter.createWorkflowDefinition?.({ definition })
          : await resolvedAdapter.updateWorkflowDefinition?.({
              workflowId: definition.id,
              expectedVersion: definition.version,
              definition,
            })
        if (!response) {
          throw new Error('Workflow persistence is unavailable in this build.')
        }
        setSelectedWorkflowDefinition(response.definition)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        await refreshWorkflowDefinitions()
        return response.definition
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowDefinitions, resolvedAdapter, selectedWorkflowIsDraft],
  )

  const handleCancelWorkflowEditing = useCallback(() => {
    if (!selectedWorkflowIsDraft) return
    setSelectedWorkflowDefinition(null)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(false)
    setSelectedWorkflowTemplatePreviewId(null)
  }, [selectedWorkflowIsDraft])

  const handleClearWorkflowSelection = useCallback(() => {
    workflowSelectionRequestSequenceRef.current += 1
    setWorkflowStartRequest(null)
    setSelectedWorkflowDefinition(null)
    setSelectedWorkflowRun(null)
    setSelectedWorkflowIsDraft(false)
    setSelectedWorkflowTemplatePreviewId(null)
  }, [])

  const handleStartWorkflowDefinitionRun = useCallback(
    async (
      workflowId: string,
      initialInput: unknown,
      options?: { selectionRequestSequence?: number },
    ) => {
      if (!activeProjectId || !resolvedAdapter.startWorkflowRun) {
        throw new Error('Select a project before starting a Workflow.')
      }
      setWorkflowActionRunning(true)
      try {
        const response = await workflowStartIdempotency.run(
          { projectId: activeProjectId, workflowId, initialInput },
          (idempotencyKey) =>
            resolvedAdapter.startWorkflowRun!({
              projectId: activeProjectId,
              workflowId,
              idempotencyKey,
              initialInput,
            }),
        )
        if (activeProjectIdRef.current !== activeProjectId) {
          return response.run
        }
        await refreshWorkflowRuns()
        const selectionStillCurrentAfterRefresh =
          options?.selectionRequestSequence === undefined ||
          options.selectionRequestSequence === workflowSelectionRequestSequenceRef.current
        if (
          activeProjectIdRef.current !== activeProjectId ||
          !selectionStillCurrentAfterRefresh
        ) {
          return response.run
        }
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter, workflowStartIdempotency],
  )

  const handleStartWorkflowFromComposer = useCallback(
    async (target: ComposerWorkflowTarget) => {
      const launchProjectId = activeProjectId
      let definition: WorkflowDefinitionDto | null
      let launchSelectionRequestSequence: number | null = null

      if (target.kind === 'definition') {
        composerTemplateMaterializationRef.current = null
        definition = await handleSelectWorkflowDefinition(target.workflowId, {
          activateView: false,
        })
        if (definition) {
          launchSelectionRequestSequence = workflowSelectionRequestSequenceRef.current
        }
      } else {
        if (!activeProjectId || !resolvedAdapter.createWorkflowDefinition) {
          throw new Error('Workflow template creation is unavailable for this project.')
        }
        workflowSelectionRequestSequenceRef.current += 1
        const selectionRequestSequence = workflowSelectionRequestSequenceRef.current
        launchSelectionRequestSequence = selectionRequestSequence
        const materializedTemplate = composerTemplateMaterializationRef.current
        const reusableDefinition =
          materializedTemplate?.projectId === activeProjectId &&
          materializedTemplate.templateId === target.templateId
            ? materializedTemplate.definition
            : null

        if (reusableDefinition) {
          definition = reusableDefinition
          setWorkflowStartRequest(null)
          setSelectedWorkflowDefinition(definition)
          setSelectedWorkflowRun(null)
          setSelectedWorkflowIsDraft(false)
          setSelectedWorkflowTemplatePreviewId(null)
          setAgentAuthoringSession(null)
          workflowAgentInspector.selectAgent(null)
        } else {
          composerTemplateMaterializationRef.current = null
          const templateDefinition = instantiateWorkflowTemplate({
            projectId: activeProjectId,
            templateId: target.templateId,
            agents: workflowAgentInspector.agents,
          })

          setWorkflowActionRunning(true)
          try {
            const response = await resolvedAdapter.createWorkflowDefinition({
              definition: templateDefinition,
            })
            if (
              selectionRequestSequence !== workflowSelectionRequestSequenceRef.current ||
              activeProjectIdRef.current !== activeProjectId
            ) {
              return null
            }
            definition = response.definition
            composerTemplateMaterializationRef.current = {
              projectId: activeProjectId,
              templateId: target.templateId,
              definition,
            }
            setWorkflowStartRequest(null)
            setSelectedWorkflowDefinition(definition)
            setSelectedWorkflowRun(null)
            setSelectedWorkflowIsDraft(false)
            setSelectedWorkflowTemplatePreviewId(null)
            setAgentAuthoringSession(null)
            workflowAgentInspector.selectAgent(null)
            await refreshWorkflowDefinitions()
          } finally {
            setWorkflowActionRunning(false)
          }
        }
      }

      if (!definition) {
        throw new Error('Xero could not load this Workflow. Try again.')
      }
      if (
        !launchProjectId ||
        activeProjectIdRef.current !== launchProjectId ||
        launchSelectionRequestSequence === null ||
        workflowSelectionRequestSequenceRef.current !== launchSelectionRequestSequence
      ) {
        return null
      }

      if (getWorkflowStartInputPlan(definition).fields.length > 0) {
        composerTemplateMaterializationRef.current = null
        setActiveView('phases')
        requestWorkflowStartInput(definition.id)
        return null
      }

      const run = await handleStartWorkflowDefinitionRun(definition.id, {}, {
        selectionRequestSequence: launchSelectionRequestSequence,
      })
      if (
        activeProjectIdRef.current !== launchProjectId ||
        workflowSelectionRequestSequenceRef.current !== launchSelectionRequestSequence
      ) {
        return run
      }
      composerTemplateMaterializationRef.current = null
      setActiveView('phases')
      return run
    },
    [
      activeProjectId,
      handleSelectWorkflowDefinition,
      handleStartWorkflowDefinitionRun,
      refreshWorkflowDefinitions,
      requestWorkflowStartInput,
      resolvedAdapter,
      setActiveView,
      workflowAgentInspector.agents,
      workflowAgentInspector.selectAgent,
    ],
  )

  const handleCancelWorkflowRun = useCallback(
    async (runId: string) => {
      if (!activeProjectId || !resolvedAdapter.cancelWorkflowRun) return
      setWorkflowActionRunning(true)
      try {
        const response = await resolvedAdapter.cancelWorkflowRun({
          projectId: activeProjectId,
          runId,
          reason: 'Cancelled by user.',
        })
        setSelectedWorkflowRun(response.run)
        await refreshWorkflowRuns()
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter],
  )

  const handleRetryWorkflowNodeRun = useCallback(
    async (runId: string, nodeRunId: string) => {
      if (!activeProjectId || !resolvedAdapter.retryWorkflowNodeRun) return
      setWorkflowActionRunning(true)
      try {
        const response = await resolvedAdapter.retryWorkflowNodeRun({
          projectId: activeProjectId,
          runId,
          nodeRunId,
        })
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        await refreshWorkflowRuns()
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter],
  )

  const handleSkipWorkflowBranch = useCallback(
    async (runId: string, nodeRunId: string, reason = 'Skipped by user.') => {
      if (!activeProjectId || !resolvedAdapter.skipWorkflowBranch) return
      setWorkflowActionRunning(true)
      try {
        const response = await resolvedAdapter.skipWorkflowBranch({
          projectId: activeProjectId,
          runId,
          nodeRunId,
          reason,
        })
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        await refreshWorkflowRuns()
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter],
  )

  const handleResumeWorkflowCheckpoint = useCallback(
    async (runId: string, nodeRunId: string, decision: string, payload: unknown = null) => {
      if (!activeProjectId || !resolvedAdapter.resumeWorkflowCheckpoint) return
      setWorkflowActionRunning(true)
      try {
        const response = await resolvedAdapter.resumeWorkflowCheckpoint({
          projectId: activeProjectId,
          runId,
          nodeRunId,
          decision,
          payload,
        })
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        await refreshWorkflowRuns()
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter],
  )

  const handleExplainWorkflowRunBlocker = useCallback(
    async (runId: string): Promise<WorkflowRunBlockerResponseDto | void> => {
      if (!activeProjectId || !resolvedAdapter.explainWorkflowRunBlocker) return
      setWorkflowActionRunning(true)
      try {
        return await resolvedAdapter.explainWorkflowRunBlocker({
          projectId: activeProjectId,
          runId,
        })
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleExportWorkflowRunBundle = useCallback(
    async (runId: string): Promise<WorkflowRunBundleResponseDto | void> => {
      if (!activeProjectId || !resolvedAdapter.exportWorkflowRunBundle) return
      setWorkflowActionRunning(true)
      try {
        return await resolvedAdapter.exportWorkflowRunBundle({
          projectId: activeProjectId,
          runId,
        })
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleResumeWorkflowNextIncompletePhase = useCallback(
    async (runId: string): Promise<WorkflowRunDto | void> => {
      if (!activeProjectId || !resolvedAdapter.resumeWorkflowNextIncompletePhase) return
      setWorkflowActionRunning(true)
      try {
        const response = await workflowStartIdempotency.run(
          {
            projectId: activeProjectId,
            workflowId: `resume-next-incomplete-phase:${runId}`,
            initialInput: { sourceRunId: runId },
          },
          (idempotencyKey) =>
            resolvedAdapter.resumeWorkflowNextIncompletePhase!({
              projectId: activeProjectId,
              runId,
              idempotencyKey,
            }),
        )
        setSelectedWorkflowRun(response.run)
        setSelectedWorkflowDefinition(response.run.definitionSnapshot)
        setSelectedWorkflowIsDraft(false)
        setSelectedWorkflowTemplatePreviewId(null)
        await refreshWorkflowRuns()
        return response.run
      } finally {
        setWorkflowActionRunning(false)
      }
    },
    [activeProjectId, refreshWorkflowRuns, resolvedAdapter, workflowStartIdempotency],
  )

  const handleInspectWorkflowAgent = useCallback(
    (ref: AgentRefDto) => {
      workflowAgentInspector.selectAgent(ref)
      setSelectedWorkflowDefinition(null)
      setSelectedWorkflowRun(null)
      setSelectedWorkflowIsDraft(false)
      setSelectedWorkflowTemplatePreviewId(null)
      setActiveView('phases')
    },
    [setActiveView, workflowAgentInspector.selectAgent],
  )

  const focusedChatPane = agentWorkspaceLayout?.paneSlots.find(
    (slot) => slot.id === agentWorkspaceLayout.focusedPaneId,
  )
  const focusedChatAgentSessionId =
    focusedChatPane?.agentSessionId ?? activeProject?.selectedAgentSessionId ?? null

  const handleUseWorkflowAgentInChat = useCallback(
    (ref: AgentRefDto) => {
      if (!activeProjectId) return
      const selection = runtimeAgentSelectionFromRef(
        ref,
        customAgentDefinitions,
        workflowAgentInspector.agents,
      )
      if (!selection) {
        console.error('Failed to resolve agent for chat selection', ref)
        return
      }

      const applySelection = (agentSessionId: string) => {
        setPendingInitialRuntimeAgent({
          agentSessionId,
          ...selection,
        })
      }

      setPendingInitialWorkflow(null)
      setPendingWorkflowPaneSelection(null)
      closeSidebarsExcept(null)
      setActiveView('agent')

      if (focusedChatAgentSessionId) {
        applySelection(focusedChatAgentSessionId)
        return
      }

      setIsCreatingAgentSession(true)
      void createAgentSession()
        .then((updatedProject) => {
          const newSessionId = updatedProject?.selectedAgentSessionId
          if (newSessionId) applySelection(newSessionId)
        })
        .finally(() => {
          setIsCreatingAgentSession(false)
        })
    },
    [
      activeProjectId,
      closeSidebarsExcept,
      createAgentSession,
      customAgentDefinitions,
      focusedChatAgentSessionId,
      workflowAgentInspector.agents,
    ],
  )

  const handleUseWorkflowInChat = useCallback(
    (target: ComposerWorkflowTarget) => {
      if (!activeProjectId) return
      const projectId = activeProjectId
      const applySelection = (agentSessionId: string) => {
        if (activeProjectIdRef.current !== projectId) return
        setPendingInitialWorkflow({ projectId, agentSessionId, target })
      }

      setPendingInitialRuntimeAgent(null)
      setPendingInitialWorkflow(null)
      setPendingWorkflowPaneSelection(null)
      closeSidebarsExcept(null)
      setActiveView('agent')

      if (focusedChatPane && !focusedChatPane.agentSessionId) {
        setPendingWorkflowPaneSelection({
          projectId,
          paneId: focusedChatPane.id,
          target,
        })
        return
      }

      if (focusedChatAgentSessionId) {
        applySelection(focusedChatAgentSessionId)
        return
      }

      setIsCreatingAgentSession(true)
      void createAgentSession()
        .then((updatedProject) => {
          const newSessionId = updatedProject?.selectedAgentSessionId
          if (newSessionId) applySelection(newSessionId)
        })
        .finally(() => {
          setIsCreatingAgentSession(false)
        })
    },
    [
      activeProjectId,
      closeSidebarsExcept,
      createAgentSession,
      focusedChatAgentSessionId,
      focusedChatPane,
    ],
  )

  const handleSetWorkflowAgentDefaultModel = useCallback(
    async (agent: WorkflowAgentSummaryDto, defaultModel: AgentDefaultModelDto | null) => {
      if (!activeProjectId) {
        throw new Error('Select a project before setting an agent default model.')
      }
      await resolvedAdapter.setAgentDefaultModel({
        projectId: activeProjectId,
        ref: agent.ref,
        defaultModel,
      })
      refreshCustomAgentDefinitions()
      void workflowAgentInspector.refreshAgents()
    },
    [activeProjectId, refreshCustomAgentDefinitions, resolvedAdapter, workflowAgentInspector],
  )

  const handlePhaseAuthoringSaved = useCallback(
    (response: AgentDefinitionWriteResponseDto) => {
      handleAgentAuthoringSaved()
      if (response.applied) handleCloseAgentAuthoring()
    },
    [handleAgentAuthoringSaved, handleCloseAgentAuthoring],
  )

  const handlePreviewEffectiveRuntime = useCallback(
    async ({
      snapshot,
      definitionId,
    }: {
      snapshot: Record<string, unknown>
      definitionId: string | null
    }) => {
      if (!activeProjectId) {
        throw new Error('Select a project before previewing an agent definition.')
      }
      const definition = canonicalCustomAgentDefinitionSchema.parse(snapshot)
      return resolvedAdapter.previewAgentDefinition({
        projectId: activeProjectId,
        definitionId,
        definition,
      })
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleReadProjectUiState = useCallback(
    async (key: string): Promise<unknown | null> => {
      if (!activeProjectId || !resolvedAdapter.readProjectUiState) return null
      const response = await resolvedAdapter.readProjectUiState({ projectId: activeProjectId, key })
      return response.value ?? null
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleWriteProjectUiState = useCallback(
    async (key: string, value: unknown | null): Promise<void> => {
      if (!activeProjectId || !resolvedAdapter.writeProjectUiState) return
      await resolvedAdapter.writeProjectUiState({ projectId: activeProjectId, key, value })
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleArchiveAgentDefinition = useCallback(
    async (ref: AgentRefDto) => {
      if (!activeProjectId) return
      if (ref.kind !== 'custom') return
      try {
        await resolvedAdapter.archiveAgentDefinition({
          projectId: activeProjectId,
          definitionId: ref.definitionId,
          expectedCurrentVersion: ref.version,
        })
        refreshCustomAgentDefinitions()
        await workflowAgentInspector.refreshAgents()
      } catch (error) {
        console.error('Failed to archive agent definition', error)
      }
    },
    [activeProjectId, refreshCustomAgentDefinitions, resolvedAdapter, workflowAgentInspector],
  )

  const handleArchiveAgentSession = useCallback((agentSessionId: string) => {
    setPendingAgentSessionId(agentSessionId)
    void archiveAgentSession(agentSessionId).finally(() => {
      setPendingAgentSessionId(null)
    })
  }, [archiveAgentSession])

  const handleOpenSearchResult = useCallback((result: SessionTranscriptSearchResultSnippetDto) => {
    if (!activeProject) return
    setActiveView('agent')
    if (!result.archived && result.agentSessionId !== activeProject.selectedAgentSessionId) {
      handleSelectAgentSession(result.agentSessionId)
    }
  }, [activeProject, handleSelectAgentSession, setActiveView])
  const handleLoadArchivedAgentSessions = useCallback(
    async (projectId: string) => {
      const response = await resolvedAdapter.listAgentSessions({
        projectId,
        includeArchived: true,
      })
      return response.sessions
        .filter((session) => session.status === 'archived')
        .map(mapAgentSession)
    },
    [resolvedAdapter],
  )
  const handleRestoreAgentSession = useCallback(
    async (agentSessionId: string) => {
      await restoreAgentSession(agentSessionId)
      await selectAgentSession(agentSessionId)
    },
    [restoreAgentSession, selectAgentSession],
  )
  const handleDeleteAgentSession = useCallback(
    async (agentSessionId: string) => {
      await deleteAgentSession(agentSessionId)
    },
    [deleteAgentSession],
  )
  const handleSearchAgentSessions = useCallback(
    async (query: string) => {
      if (!activeProjectId || !resolvedAdapter.searchSessionTranscripts) {
        return []
      }

      const response = await resolvedAdapter.searchSessionTranscripts({
        projectId: activeProjectId,
        query,
        includeArchived: true,
        limit: 12,
      })
      return response.results
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleOpenAgentManagement = useCallback(() => openSettings('agents'), [openSettings])
  const handleOpenAgentProviderSettings = useCallback(
    () => openSettings('providers'),
    [openSettings],
  )
  const handleOpenAgentDiagnostics = useCallback(
    () => openSettings('diagnostics'),
    [openSettings],
  )
  const handleAgentStartAutonomousRun = useCallback(async (paneId: string) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return startAutonomousRun()
  }, [ensurePaneAgentSessionSelected, startAutonomousRun])
  const handleAgentInspectAutonomousRun = useCallback(async (paneId: string) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return inspectAutonomousRun()
  }, [ensurePaneAgentSessionSelected, inspectAutonomousRun])
  const handleAgentCancelAutonomousRun = useCallback(async (paneId: string, runId: string) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return cancelAutonomousRun(runId)
  }, [cancelAutonomousRun, ensurePaneAgentSessionSelected])
  const handleAgentStartLogin = useCallback(
    (
      _paneId: string,
      options?: Parameters<NonNullable<AgentRuntimeProps['onStartLogin']>>[0],
    ) => startOpenAiLogin(options),
    [startOpenAiLogin],
  )
  const handleAgentStartRuntimeRun = useCallback(async (
    paneId: string,
    options?: Parameters<NonNullable<AgentRuntimeProps['onStartRuntimeRun']>>[0],
  ) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return startRuntimeRun(options)
  }, [ensurePaneAgentSessionSelected, startRuntimeRun])
  const handleAgentUpdateRuntimeRunControls = useCallback(async (
    paneId: string,
    request?: Parameters<NonNullable<AgentRuntimeProps['onUpdateRuntimeRunControls']>>[0],
  ) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return updateRuntimeRunControls(request)
  }, [ensurePaneAgentSessionSelected, updateRuntimeRunControls])
  const handleSendEditorContextToAgent = useCallback(
    async (request: EditorAgentContextRequest) => {
      if (!activeProject) {
        throw new Error('Select a project before sending editor context to an agent.')
      }

      const activeRuntimeRun = agentView?.runtimeRun ?? null
      if (activeRuntimeRun && !activeRuntimeRun.isTerminal) {
        await updateRuntimeRunControls({ prompt: request.prompt })
      } else {
        await startRuntimeRun({
          controls: agentComposerControls,
          prompt: request.prompt,
        })
      }
      setActiveView('agent')
    },
    [
      activeProject,
      agentComposerControls,
      agentView?.runtimeRun,
      setActiveView,
      startRuntimeRun,
      updateRuntimeRunControls,
    ],
  )
  const browserPenToolDisabledReason = useMemo(() => {
    const compatibility = checkAttachmentModelCompatibility(
      { kind: 'image', mediaType: 'image/png' },
      agentView?.selectedModelOption ?? null,
    )
    if (compatibility.supported) return null
    return `${compatibility.message} Choose a model with image input to use the pen tool.`
  }, [agentView?.selectedModelOption])
  const handleAddBrowserContextToAgentComposer = useCallback(
    async (request: BrowserAgentContextRequest) => {
      if (!activeProject) {
        throw new Error('Select a project before adding browser context to an agent.')
      }
      if (request.image && browserPenToolDisabledReason) {
        throw new Error(browserPenToolDisabledReason)
      }

      const target: BrowserComposerInsertTarget =
        activeView === 'agent' ? 'agent-view' : 'agent-dock'
      const insertId = `browser-context-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
      setPendingBrowserComposerInsert({
        target,
        insert: {
          id: insertId,
          prompt: request.visiblePrompt,
          hiddenPrompt: request.prompt,
          contextCard: request.contextCard,
          image: request.image,
        },
      })

      if (target === 'agent-dock') {
        preloadSurfaceChunk('agent-dock')
        closeSidebarsExcept('agentDock')
        setAgentDockOpen(true)
      }
    },
    [
      activeProject,
      activeView,
      browserPenToolDisabledReason,
      closeSidebarsExcept,
    ],
  )
  const handleBrowserComposerInsertConsumed = useCallback((id: string) => {
    setPendingBrowserComposerInsert((current) =>
      current?.insert.id === id ? null : current,
    )
  }, [])
  const handleAgentComposerControlsChange = useCallback((
    _paneId: string,
    controls: RuntimeRunControlInputDto | null,
  ) => {
    persistComposerSettings(controls)
    setAgentComposerControls((current) =>
      sameRuntimeRunControlInput(current, controls) ? current : controls,
    )
  }, [persistComposerSettings])
  const handleAgentStartRuntimeSession = useCallback(
    (
      _paneId: string,
      options?: Parameters<NonNullable<AgentRuntimeProps['onStartRuntimeSession']>>[0],
    ) => startRuntimeSession(options),
    [startRuntimeSession],
  )
  const handleAgentStopRuntimeRun = useCallback(async (paneId: string, runId: string) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    return stopRuntimeRun(runId)
  }, [ensurePaneAgentSessionSelected, stopRuntimeRun])
  const handleAgentLogout = useCallback(
    (_paneId: string) => logoutRuntimeSession(),
    [logoutRuntimeSession],
  )

  const startComputerUseRuntimeRun = useCallback(
    async (options?: {
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
      attachments?: StagedAgentAttachmentDto[]
      linkedPaths?: RuntimeLinkedPathDto[]
    }) => {
      const isPromptSubmission =
        Boolean(options?.prompt?.trim()) || Boolean(options?.attachments?.length)
      if (!isPromptSubmission) {
        setComputerUseRuntimeRunActionStatus('running')
        setComputerUsePendingRuntimeRunAction('start')
      }
      setComputerUseRuntimeRunActionError(null)

      try {
        await loadComputerUseProject()
        const response = await resolvedAdapter.startRuntimeRun(
          GLOBAL_COMPUTER_USE_PROJECT_ID,
          GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
          {
            initialControls: options?.controls ?? null,
            initialPrompt: options?.prompt ?? null,
            initialAttachments: options?.attachments ?? [],
            initialLinkedPaths: options?.linkedPaths ?? [],
          },
        )
        const runtimeRun = mapRuntimeRun(response)
        setComputerUseRuntimeRun(runtimeRun)
        setComputerUseProject((current) => (current ? applyRuntimeRun(current, runtimeRun) : current))
        return runtimeRun
      } catch (error) {
        setComputerUseRuntimeRunActionError(
          getOperatorActionError(
            error,
            'Xero could not start the Computer Use run.',
          ),
        )
        throw error
      } finally {
        if (!isPromptSubmission) {
          setComputerUseRuntimeRunActionStatus('idle')
          setComputerUsePendingRuntimeRunAction(null)
        }
      }
    },
    [loadComputerUseProject, resolvedAdapter],
  )

  const updateComputerUseRuntimeRunControls = useCallback(
    async (request: {
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
      attachments?: StagedAgentAttachmentDto[]
      linkedPaths?: RuntimeLinkedPathDto[]
    } = {}) => {
      const isPromptSubmission =
        Boolean(request.prompt?.trim()) || Boolean(request.attachments?.length)
      if (!isPromptSubmission) {
        setComputerUseRuntimeRunActionStatus('running')
        setComputerUsePendingRuntimeRunAction('update_controls')
      }
      setComputerUseRuntimeRunActionError(null)

      try {
        const runId =
          computerUseRuntimeRun?.runId ??
          (await refreshComputerUseRuntimeMetadata()).runtimeRun?.runId ??
          null
        if (!runId) {
          throw new Error('Xero cannot queue Computer Use controls until a run exists.')
        }
        const payload = {
          controls: request.controls ?? null,
          prompt: request.prompt ?? null,
          attachments: request.attachments ?? [],
          linkedPaths: request.linkedPaths ?? [],
        }
        const response = await computerUseContinuationCoordinator.run(
          {
            channel: 'runtime-control',
            targetId: runId,
            payload,
          },
          (continuationRequestId) =>
            resolvedAdapter.updateRuntimeRunControls({
              projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
              agentSessionId: GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
              runId,
              continuationRequestId,
              ...payload,
            }),
        )
        const runtimeRun = mapRuntimeRun(response)
        setComputerUseRuntimeRun(runtimeRun)
        setComputerUseProject((current) => (current ? applyRuntimeRun(current, runtimeRun) : current))
        return runtimeRun
      } catch (error) {
        setComputerUseRuntimeRunActionError(
          getOperatorActionError(
            error,
            'Xero could not queue Computer Use control changes.',
          ),
        )
        throw error
      } finally {
        if (!isPromptSubmission) {
          setComputerUseRuntimeRunActionStatus('idle')
          setComputerUsePendingRuntimeRunAction(null)
        }
      }
    },
    [
      computerUseContinuationCoordinator,
      computerUseRuntimeRun?.runId,
      refreshComputerUseRuntimeMetadata,
      resolvedAdapter,
    ],
  )

  const startComputerUseRuntimeSession = useCallback(
    async (options?: { providerProfileId?: string | null }) => {
      const response = await resolvedAdapter.startRuntimeSession(
        GLOBAL_COMPUTER_USE_PROJECT_ID,
        options,
      )
      const runtimeSession = mapRuntimeSession(response)
      setComputerUseRuntimeSession(runtimeSession)
      setComputerUseProject((current) =>
        current ? applyRuntimeSession(current, runtimeSession) : current,
      )
      return runtimeSession
    },
    [resolvedAdapter],
  )

  const stopComputerUseRuntimeRun = useCallback(
    async (runId: string) => {
      setComputerUseRuntimeRunActionStatus('running')
      setComputerUsePendingRuntimeRunAction('stop')
      setComputerUseRuntimeRunActionError(null)
      try {
        const response = await resolvedAdapter.stopRuntimeRun(
          GLOBAL_COMPUTER_USE_PROJECT_ID,
          GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
          runId,
        )
        const runtimeRun = response ? mapRuntimeRun(response) : null
        setComputerUseRuntimeRun(runtimeRun)
        setComputerUseProject((current) =>
          current ? applyRuntimeRun(current, runtimeRun) : current,
        )
        return runtimeRun
      } catch (error) {
        setComputerUseRuntimeRunActionError(
          getOperatorActionError(error, 'Xero could not stop the Computer Use run.'),
        )
        throw error
      } finally {
        setComputerUseRuntimeRunActionStatus('idle')
        setComputerUsePendingRuntimeRunAction(null)
      }
    },
    [resolvedAdapter],
  )

  const logoutComputerUseRuntimeSession = useCallback(async () => {
    const response = await resolvedAdapter.logoutRuntimeSession(GLOBAL_COMPUTER_USE_PROJECT_ID)
    const runtimeSession = mapRuntimeSession(response)
    setComputerUseRuntimeSession(runtimeSession)
    setComputerUseProject((current) =>
      current ? applyRuntimeSession(current, runtimeSession) : current,
    )
    return runtimeSession
  }, [resolvedAdapter])

  const handleAgentResolveOperatorAction = useCallback(async (
    paneId: string,
    actionId: string,
    decision: 'approve' | 'reject',
    options?: Parameters<NonNullable<AgentRuntimeProps['onResolveOperatorAction']>>[2],
  ) => {
    if (!(await ensurePaneAgentSessionSelected(paneId))) return null
    const result = await resolveOperatorAction(actionId, decision, {
      userAnswer: options?.userAnswer ?? null,
    })
    if (decision === 'approve') {
      refreshCustomAgentDefinitions()
    }
    return result
  }, [ensurePaneAgentSessionSelected, refreshCustomAgentDefinitions, resolveOperatorAction])
  const handleAgentResumeOperatorRun = useCallback(
    async (
      paneId: string,
      actionId: string,
      options?: Parameters<NonNullable<AgentRuntimeProps['onResumeOperatorRun']>>[1],
    ) => {
      if (!(await ensurePaneAgentSessionSelected(paneId))) return null
      return resumeOperatorRun(actionId, { userAnswer: options?.userAnswer ?? null })
    },
    [ensurePaneAgentSessionSelected, resumeOperatorRun],
  )
  const handleAgentSubmitManualCallback = useCallback(
    (
      _paneId: string,
      flowId: string,
      manualInput: string,
    ) => submitOpenAiCallback(flowId, { manualInput }),
    [submitOpenAiCallback],
  )
  const handleAgentCodeUndoApplied = useCallback(() => retry(), [retry])

  const handleSelectProject = useCallback(
    (projectId: string) => {
      void selectProject(projectId)
    },
    [selectProject],
  )

  const handlePreviewProject = useCallback(
    (projectId: string) => {
      const project = projects.find((candidate) => candidate.id === projectId)
      if (!project) {
        return
      }

      previewProjectSelection(project.id, project.name)
    },
    [projects],
  )

  useEffect(() => {
    clearProjectSelectionPreview(activeProjectId)
  }, [activeProjectId])

  const handleRemoveProject = useCallback(
    (projectId: string) => {
      void removeProject(projectId)
    },
    [removeProject],
  )
  const closeVcs = useCallback(() => setVcsOpen(false), [])
  const refreshVcsStatus = useCallback(() => {
    if (activeProjectId) {
      return retry()
    }
    return undefined
  }, [activeProjectId, retry])
  const loadRepositoryDiff = useCallback(
    (projectId: string, scope: RepositoryDiffScope) => resolvedAdapter.getRepositoryDiff(projectId, scope),
    [resolvedAdapter],
  )
  const generateCommitMessage = useCallback(
    (projectId: string, model: VcsCommitMessageModel) =>
      resolvedAdapter.gitGenerateCommitMessage({
        projectId,
        providerProfileId: model.providerProfileId,
        modelId: model.modelId,
        thinkingEffort: model.thinkingEffort,
      }),
    [resolvedAdapter],
  )
  const stagePaths = useCallback(
    (projectId: string, paths: string[]) => resolvedAdapter.gitStagePaths(projectId, paths),
    [resolvedAdapter],
  )
  const unstagePaths = useCallback(
    (projectId: string, paths: string[]) => resolvedAdapter.gitUnstagePaths(projectId, paths),
    [resolvedAdapter],
  )
  const discardChanges = useCallback(
    (projectId: string, paths: string[]) => resolvedAdapter.gitDiscardChanges(projectId, paths),
    [resolvedAdapter],
  )
  const commitChanges = useCallback(
    (projectId: string, message: string) => resolvedAdapter.gitCommit(projectId, message),
    [resolvedAdapter],
  )
  const fetchRepository = useCallback(
    (projectId: string) => resolvedAdapter.gitFetch(projectId),
    [resolvedAdapter],
  )
  const pullRepository = useCallback(
    (projectId: string) => resolvedAdapter.gitPull(projectId),
    [resolvedAdapter],
  )
  const pushRepository = useCallback(
    (projectId: string) => resolvedAdapter.gitPush(projectId),
    [resolvedAdapter],
  )

  const renderBody = () => {
    if (isLoading && !activeProject) {
      return <LoadingScreen />
    }

    if (!activeProject && errorMessage) {
      return <ProjectLoadErrorState message={errorMessage} onRetry={() => void retry()} />
    }

    if (!activeProject) {
      if (activeView === 'agent') {
        const hasReadyProvider = (providerCredentials?.credentials.length ?? 0) > 0
        return (
          <div className="flex flex-1 items-center justify-center overflow-y-auto scrollbar-thin px-6 py-5">
            <SetupEmptyState
              kind={hasReadyProvider ? 'no-project' : 'no-provider'}
              onOpenSettings={() => openSettings('providers')}
              onImportProject={() => void importProject()}
              isImportingProject={isImporting}
              isDesktopRuntime={isDesktopRuntime}
            />
          </div>
        )
      }

      return (
        <NoProjectEmptyState
          isDesktopRuntime={isDesktopRuntime}
          isImporting={isImporting}
          onImport={() => void importProject()}
        />
      )
    }

    const shouldRenderExecutionPanel = Boolean(executionView && activeProjectId)

    const isExecutionVisible = activeView === 'execution'
    const getViewPaneClassName = (
      visible: boolean,
      options: { heavySwitchSurface?: boolean; workflowSurface?: boolean } = {},
    ) =>
      cn(
        'view-pane absolute inset-0 flex min-h-0 min-w-0 overflow-hidden motion-standard',
        options.workflowSurface && 'workflow-view-pane',
        options.heavySwitchSurface
          ? 'transition-opacity'
          : 'transform-gpu transition-[opacity,transform]',
        visible
          ? 'z-10 opacity-100'
          : cn(
              'pointer-events-none z-0 opacity-0',
              options.heavySwitchSurface ? 'invisible' : 'translate-x-2',
            ),
        !options.heavySwitchSurface && visible ? 'translate-x-0' : null,
      )
    const agentRenderProject = displayedActiveProject ?? activeProject
    const agentRenderView = displayedAgentView
    const agentRenderWorkspaceLayout = displayedAgentWorkspaceLayout
    const agentRenderWorkspacePanes = displayedAgentWorkspacePanes
    const sessionsPeekAvailable = activeView === 'agent' && explorerMode === 'collapsed'
    const agentUsesHeavySwitchSurface = paneCount >= 3

    return (
      <AgentWorkspaceDndProvider
        paneSlots={dndPaneSlots}
        onReorderPanes={reorderPanes}
        onOpenSessionInNewPane={openSessionInNewPane}
      >
        <AgentSessionsSidebar
          projectId={agentRenderProject.id}
          sessions={agentRenderProject.agentSessions}
          selectedSessionId={agentRenderProject.selectedAgentSessionId}
          onSelectSession={handleSelectAgentSession}
          onCreateSession={handleCreateAgentSession}
          onArchiveSession={handleArchiveAgentSession}
          onLoadArchivedSessions={handleLoadArchivedAgentSessions}
          onRestoreSession={handleRestoreAgentSession}
          onDeleteSession={handleDeleteAgentSession}
          onSearchSessions={
            resolvedAdapter.searchSessionTranscripts
              ? handleSearchAgentSessions
              : undefined
          }
          onOpenSearchResult={handleOpenSearchResult}
          onReadProjectUiState={
            resolvedAdapter.readProjectUiState ? handleReadProjectUiState : undefined
          }
          onWriteProjectUiState={
            resolvedAdapter.writeProjectUiState ? handleWriteProjectUiState : undefined
          }
          pendingSessionId={pendingAgentSessionId}
          isCreating={isCreatingAgentSession}
          collapsed={activeView !== 'agent' || explorerCollapsed}
          mode={activeView === 'agent' ? explorerMode : 'pinned'}
          peeking={sessionsPeekAvailable ? explorerPeeking : false}
          onCollapse={collapseExplorer}
          onPin={pinExplorer}
          onRequestPeek={sessionsPeekAvailable ? requestExplorerPeek : undefined}
          onReleasePeek={sessionsPeekAvailable ? releaseExplorerPeek : undefined}
          sessionPaneAssignments={isMultiPane ? sessionPaneAssignments : undefined}
        />
        <AgentCommandPalette
          enabled={activeView === 'agent' && Boolean(agentRenderWorkspaceLayout)}
          panes={agentCommandPalettePanes}
          spawnDisabled={paneCount >= 6}
          onSpawnPane={handleSpawnPane}
          onClosePane={handleClosePane}
          onFocusPane={handleFocusPane}
          onCycleFocus={cycleFocusPane}
        />
        <BaseAlertDialog
          open={pendingPaneCloseId !== null}
          onOpenChange={(open) => {
            if (!open) {
              setPendingPaneCloseId(null)
            }
          }}
          variant="destructive-confirmation"
          title="Close agent pane?"
          description={
            <>
              {pendingPaneCloseCopy} Closing keeps the session in the sidebar, but this pane will stop showing it.
            </>
          }
          cancelAction={{ label: 'Cancel' }}
          action={{
            label: 'Close pane',
            className: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
            destructive: false,
            onClick: handleConfirmClosePane,
          }}
        />
        <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
          {workflowView ? (
            <LazyActivityPane
              active={activeView === 'phases'}
              className={getViewPaneClassName(activeView === 'phases', {
                workflowSurface: true,
              })}
              name="workflow-pane"
              prewarm={startupSurfacePrewarm.shouldMount}
            >
              <PhaseView
                active={activeView === 'phases'}
                projectId={activeProjectId}
                workflow={workflowView}
                onToggleWorkflows={toggleWorkflows}
                workflowsOpen={workflowsOpen}
                onCreateAgent={handleCreateAgent}
                onCreateAgentFromTemplate={handleCreateAgentFromTemplate}
                onEditAgentFromWorkflow={(ref) => handleStartAgentAuthoringFromRef('edit', ref)}
                selectedWorkflowDefinition={WORKFLOWS_ENABLED ? selectedWorkflowDefinition : null}
                selectedWorkflowRun={WORKFLOWS_ENABLED ? selectedWorkflowRun : null}
                selectedWorkflowIsDraft={WORKFLOWS_ENABLED ? selectedWorkflowIsDraft : false}
                selectedWorkflowIsTemplatePreview={
                  WORKFLOWS_ENABLED ? selectedWorkflowTemplatePreviewId !== null : false
                }
                workflowActionRunning={WORKFLOWS_ENABLED ? workflowActionRunning : false}
                onCreateWorkflow={WORKFLOWS_ENABLED ? handleCreateWorkflow : undefined}
                onCreateWorkflowWithAgentCreate={
                  WORKFLOWS_ENABLED ? handleStartWorkflowAgentCreate : undefined
                }
                onCreateWorkflowFromTemplate={
                  WORKFLOWS_ENABLED ? handleCreateWorkflowFromTemplate : undefined
                }
                onSaveWorkflowDefinition={WORKFLOWS_ENABLED ? handleSaveWorkflowDefinition : undefined}
                onCancelWorkflowEditing={WORKFLOWS_ENABLED ? handleCancelWorkflowEditing : undefined}
                onClearWorkflowSelection={WORKFLOWS_ENABLED ? handleClearWorkflowSelection : undefined}
                onStartWorkflowDefinitionRun={
                  WORKFLOWS_ENABLED ? handleStartWorkflowDefinitionRun : undefined
                }
                workflowStartRequestToken={
                  WORKFLOWS_ENABLED &&
                  selectedWorkflowDefinition &&
                  workflowStartRequest?.workflowId === selectedWorkflowDefinition.id
                    ? workflowStartRequest.token
                    : 0
                }
                onWorkflowStartRequestHandled={
                  WORKFLOWS_ENABLED ? handleWorkflowStartRequestHandled : undefined
                }
                onCancelWorkflowRun={WORKFLOWS_ENABLED ? handleCancelWorkflowRun : undefined}
                onRetryWorkflowNodeRun={WORKFLOWS_ENABLED ? handleRetryWorkflowNodeRun : undefined}
                onSkipWorkflowBranch={WORKFLOWS_ENABLED ? handleSkipWorkflowBranch : undefined}
                onResumeWorkflowCheckpoint={
                  WORKFLOWS_ENABLED ? handleResumeWorkflowCheckpoint : undefined
                }
                onExplainWorkflowRunBlocker={
                  WORKFLOWS_ENABLED ? handleExplainWorkflowRunBlocker : undefined
                }
                onExportWorkflowRunBundle={WORKFLOWS_ENABLED ? handleExportWorkflowRunBundle : undefined}
                onResumeWorkflowNextIncompletePhase={
                  WORKFLOWS_ENABLED ? handleResumeWorkflowNextIncompletePhase : undefined
                }
                templates={workflowAgentInspector.agents}
                templatesLoading={workflowAgentInspector.agentsStatus === 'loading'}
                templatesError={workflowAgentInspector.agentsError}
                agentDetail={workflowAgentInspector.detail}
                agentDetailStatus={workflowAgentInspector.detailStatus}
                agentDetailError={workflowAgentInspector.detailError}
                onClearAgentSelection={handleClearWorkflowAgentSelection}
                onReloadAgentDetail={workflowAgentInspector.reloadDetail}
                authoringSession={activeAgentAuthoringSession}
                authoringCatalog={agentAuthoringCatalog}
                toolPackCatalog={agentToolPackCatalog}
                onSearchAttachableSkills={handleSearchAttachableSkills}
                onResolveAttachableSkill={handleResolveAttachableSkill}
                onAuthoringSubmit={handleAgentAuthoringSubmit}
                onAuthoringSaved={handlePhaseAuthoringSaved}
                onAuthoringCancel={handleCloseAgentAuthoring}
                onReadProjectUiState={
                  resolvedAdapter.readProjectUiState ? handleReadProjectUiState : undefined
                }
                onWriteProjectUiState={
                  resolvedAdapter.writeProjectUiState ? handleWriteProjectUiState : undefined
                }
                onSelectedNodeChange={handlePhaseSelectedNodeChange}
                onPreviewEffectiveRuntime={
                  activeProjectId ? handlePreviewEffectiveRuntime : undefined
                }
              />
            </LazyActivityPane>
          ) : null}

          {agentRenderView ? (
            <LazyActivityPane
              active={activeView === 'agent'}
              className={getViewPaneClassName(activeView === 'agent', {
                heavySwitchSurface: agentUsesHeavySwitchSurface,
              })}
              name="agent-pane"
              prewarm={startupSurfacePrewarm.shouldMount}
            >
              <AgentWorkspace
                active={activeView === 'agent'}
                layout={agentRenderWorkspaceLayout}
                panes={agentRenderWorkspacePanes}
                highChurnStore={highChurnStore}
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                toolCallGroupingPreference={toolCallGroupingPreference}
                agentRoutingAutoSwitchEnabled={agentRoutingAutoSwitchEnabled}
                customAgentDefinitions={customAgentDefinitions}
                agentDefaultModels={agentDefaultModels}
                workflowDefinitions={WORKFLOWS_ENABLED ? workflowDefinitions : []}
                onStartWorkflowFromComposer={
                  WORKFLOWS_ENABLED ? handleStartWorkflowFromComposer : undefined
                }
                onOpenAgentManagement={handleOpenAgentManagement}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onStartWorkflowAgentCreate={
                  WORKFLOWS_ENABLED ? handleStartWorkflowAgentCreate : undefined
                }
                onCreateSession={handleCreateAgentSession}
                pendingInitialRuntimeAgent={pendingInitialRuntimeAgent}
                onClearPendingInitialRuntimeAgent={handleClearPendingInitialRuntimeAgent}
                pendingInitialWorkflow={pendingInitialWorkflow}
                onClearPendingInitialWorkflow={handleClearPendingInitialWorkflow}
                isCreatingSession={isCreatingAgentSession}
                onSpawnPane={handleSpawnPane}
                onClosePane={handleClosePane}
                onFocusPane={handleFocusPane}
                onSplitterRatiosChange={handleSplitterRatiosChange}
                onPaneCloseStateChange={handlePaneCloseStateChange}
                onLogout={handleAgentLogout}
                onOpenSettings={handleOpenAgentProviderSettings}
                onOpenDiagnostics={handleOpenAgentDiagnostics}
                onResolveOperatorAction={handleAgentResolveOperatorAction}
                onResumeOperatorRun={handleAgentResumeOperatorRun}
                onRetryStream={retry}
                onStartLogin={handleAgentStartLogin}
                onStartAutonomousRun={handleAgentStartAutonomousRun}
                onInspectAutonomousRun={handleAgentInspectAutonomousRun}
                onCancelAutonomousRun={handleAgentCancelAutonomousRun}
                onStartRuntimeRun={handleAgentStartRuntimeRun}
                onUpdateRuntimeRunControls={handleAgentUpdateRuntimeRunControls}
                onComposerControlsChange={handleAgentComposerControlsChange}
                onStartRuntimeSession={handleAgentStartRuntimeSession}
                onStopRuntimeRun={handleAgentStopRuntimeRun}
                onSubmitManualCallback={handleAgentSubmitManualCallback}
                onCodeUndoApplied={handleAgentCodeUndoApplied}
                pendingComposerInsert={
                  pendingBrowserComposerInsert?.target === 'agent-view'
                    ? pendingBrowserComposerInsert.insert
                    : null
                }
                onPendingComposerInsertConsumed={handleBrowserComposerInsertConsumed}
              />
            </LazyActivityPane>
          ) : null}

          {shouldRenderExecutionPanel && executionView ? (
            <LazyMountedPane
              active={isExecutionVisible}
              className={getViewPaneClassName(isExecutionVisible)}
              prewarm={startupSurfacePrewarm.shouldMount}
            >
              <Suspense fallback={<LoadingScreen />}>
                <LazyExecutionView
                  active={isExecutionVisible}
                  execution={executionView}
                  listProjectFileIndex={resolvedAdapter.listProjectFileIndex}
                  listProjectFiles={listProjectFiles}
                  readProjectFile={readProjectFile}
                  writeProjectFile={writeProjectFile}
                  statProjectFiles={resolvedAdapter.statProjectFiles}
                  readProjectUiState={resolvedAdapter.readProjectUiState}
                  writeProjectUiState={resolvedAdapter.writeProjectUiState}
                  runProjectTypecheck={resolvedAdapter.runProjectTypecheck}
                  formatProjectDocument={resolvedAdapter.formatProjectDocument}
                  runProjectLint={resolvedAdapter.runProjectLint}
                  getRepositoryDiff={resolvedAdapter.getRepositoryDiff}
                  gitRevertPatch={resolvedAdapter.gitRevertPatch}
                  runEditorTerminalTask={handleRunEditorTerminalTask}
                  revokeProjectAssetTokens={revokeProjectAssetTokens}
                  openProjectFileExternal={openProjectFileExternal}
                  createProjectEntry={createProjectEntry}
                  renameProjectEntry={renameProjectEntry}
                  moveProjectEntry={moveProjectEntry}
                  deleteProjectEntry={deleteProjectEntry}
                  searchProject={searchProject}
                  replaceInProject={replaceInProject}
                  agentActivities={editorAgentActivities}
                  onSendEditorContextToAgent={handleSendEditorContextToAgent}
                />
              </Suspense>
            </LazyMountedPane>
          ) : null}
        </div>
      </AgentWorkspaceDndProvider>
    )
  }

  const onboardingProject = activeProject
    ? {
        name: activeProject.name,
        path: activeProject.repository?.rootPath ?? activeProject.name,
      }
    : null
  const shouldAutoOpenOnboarding =
    onboardingCompletionHydrated && !onboardingDismissed && !isLoading && projects.length === 0
  const showOnboarding =
    (onboardingOpen || shouldAutoOpenOnboarding) &&
    !onboardingDismissed &&
    !isLoading &&
    onboardingCompletionHydrated &&
    activeViewHydrated
  const isForegroundProjectSelection = pendingProjectSelectionId !== null
  const pendingProjectSelectionName = pendingProjectSelectionId
    ? projects.find((project) => project.id === pendingProjectSelectionId)?.name ?? null
    : null
  const shellProjectName = pendingProjectSelectionName ?? activeProject?.name
  const agentDockSurfaceOpen = agentDockOpen || computerUseOpen
  const browserFocusMode = browserOpen && browserFullWidth
  const agentDockSurfaceAgent = computerUseOpen ? computerUseAgentView : agentView
  const agentDockSurfaceSessions = computerUseOpen
    ? (computerUseProjectForView?.agentSessions ?? [])
    : (activeProject?.agentSessions ?? [])
  const agentDockSurfaceSelectedSessionId = computerUseOpen
    ? GLOBAL_COMPUTER_USE_AGENT_SESSION_ID
    : (activeProject?.selectedAgentSessionId ?? null)
  const foregroundProjectLoad = isForegroundProjectLoad(refreshSource)
  const isBlockingProjectLoading =
    isProjectLoading &&
    foregroundProjectLoad &&
    !isForegroundProjectSelection &&
    !activeProject
  const isProjectSelectionShellPending =
    pendingProjectSelectionId !== null && activeProjectId !== pendingProjectSelectionId
  const startupSurfacePrewarm = useStartupSurfacePrewarm(
    activeViewHydrated && !showOnboarding && !isLoading && !isBlockingProjectLoading,
  )
  useIdleSurfacePreloads(
    activeViewHydrated &&
      !showOnboarding &&
      !isLoading &&
      !isBlockingProjectLoading &&
      startupSurfacePrewarm.ready,
  )
  const showStartupSurfacePrewarm = !startupSurfacePrewarm.ready
  const showAppBootLoading = !showOnboarding && (
    !activeViewHydrated ||
    !onboardingCompletionHydrated ||
    isLoading ||
    isBlockingProjectLoading ||
    showStartupSurfacePrewarm
  )
  const signInReminderToast = (
    <SignInReminderToast enabled={!showOnboarding && !showAppBootLoading} />
  )

  useEffect(() => {
    if (
      import.meta.env.MODE === 'test' ||
      showAppBootLoading ||
      !startupSurfacePrewarm.ready
    ) {
      return
    }

    return scheduleIdlePreload(() => {
      void preloadComputerUseProject().catch(() => undefined)
    }, 900)
  }, [preloadComputerUseProject, showAppBootLoading, startupSurfacePrewarm.ready])

  useEffect(() => {
    if (environmentDiscoveryCheckedRef.current) {
      return
    }
    if (!resolvedAdapter.getEnvironmentDiscoveryStatus) {
      environmentDiscoveryCheckedRef.current = true
      return
    }

    let cancelled = false
    environmentDiscoveryCheckedRef.current = true

    const startEnvironmentDiscovery = async () => {
      try {
        const status = await refreshEnvironmentDiscovery()
        if (cancelled || !status) return
      } catch {
        // Startup remains non-blocking; diagnostics can surface discovery failures later.
      }
    }

    void startEnvironmentDiscovery()

    return () => {
      cancelled = true
    }
  }, [refreshEnvironmentDiscovery, resolvedAdapter.getEnvironmentDiscoveryStatus])

  if (showOnboarding) {
    return (
      <>
        {signInReminderToast}
        <XeroShell
          activeView={activeView}
          onViewChange={setActiveView}
          onViewPreload={preloadViewChunk}
          onSurfacePreload={preloadSurfaceChunk}
          projectId={activeProjectId}
          projectName={shellProjectName}
          onToggleBrowser={toggleBrowser}
          browserOpen={browserOpen}
          onToggleIos={toggleIos}
          iosOpen={iosOpen}
          onToggleVcs={toggleVcs}
          vcsOpen={vcsOpen}
          onToggleWorkflows={toggleWorkflows}
          workflowsOpen={workflowsOpen}
          onToggleAgentDock={toggleAgentDock}
          agentDockOpen={agentDockOpen}
          agentDockDisabled={activeView === 'agent' || !activeProject}
          onToggleComputerUse={toggleComputerUse}
          computerUseOpen={computerUseOpen}
          computerUseRunning={computerUseRunning}
          vcsChangeCount={repositoryStatus?.statusCount ?? 0}
          vcsAdditions={repositoryStatus?.additions ?? 0}
          vcsDeletions={repositoryStatus?.deletions ?? 0}
          platformOverride={platformOverride}
          footer={statusFooter}
          chromeOnly
          hideFooter
        >
          <OnboardingFlow
            providerCredentials={providerCredentials}
            providerCredentialsLoadStatus={providerCredentialsLoadStatus}
            providerCredentialsLoadError={providerCredentialsLoadError}
            providerCredentialsSaveStatus={providerCredentialsSaveStatus}
            providerCredentialsSaveError={providerCredentialsSaveError}
            runtimeSession={agentView?.runtimeSession ?? null}
            project={onboardingProject}
            isImporting={isImporting}
            isProjectLoading={isProjectLoading}
            projectErrorMessage={errorMessage}
            environmentPermissionRequests={environmentDiscoveryStatus?.permissionRequests ?? []}
            onResolveEnvironmentPermissions={resolveEnvironmentPermissions}
            onImportProject={async () => {
              await importProject()
            }}
            onRefreshProviderCredentials={(options) => refreshProviderCredentials(options)}
            onUpsertProviderCredential={(request) => upsertProviderCredential(request)}
            onDeleteProviderCredential={(providerId) => deleteProviderCredential(providerId)}
            onStartOAuthLogin={(request) => startOAuthLogin(request)}
            onComplete={completeOnboarding}
            onDismiss={completeOnboarding}
          />
        </XeroShell>
      </>
    )
  }

  return (
    <>
      {signInReminderToast}
      <div
        aria-hidden={showAppBootLoading}
        className={cn('h-screen w-screen', showAppBootLoading && 'invisible')}
        inert={showAppBootLoading ? true : undefined}
      >
        <XeroShell
          activeView={activeView}
          onViewChange={setActiveView}
          onViewPreload={preloadViewChunk}
          onSurfacePreload={preloadSurfaceChunk}
          projectId={activeProjectId}
          projectName={shellProjectName}
          onToggleBrowser={toggleBrowser}
          browserOpen={browserOpen}
          onToggleIos={toggleIos}
          iosOpen={iosOpen}
          onToggleVcs={toggleVcs}
          vcsOpen={vcsOpen}
          onToggleWorkflows={toggleWorkflows}
          workflowsOpen={workflowsOpen}
          onToggleAgentDock={toggleAgentDock}
          agentDockOpen={agentDockOpen}
          agentDockDisabled={activeView === 'agent' || !activeProject}
          onToggleComputerUse={toggleComputerUse}
          computerUseOpen={computerUseOpen}
          computerUseRunning={computerUseRunning}
          onToggleTerminal={toggleTerminal}
          terminalOpen={terminalOpen}
          projectRunning={false}
          projectStartTargets={activeProjectStartTargets}
          onEditProjectStartTargets={handleEditProjectStartTargets}
          onRunTarget={handleRunTarget}
          onRunAllTargets={handleRunAllTargets}
          onStopProject={() => {
            /* PTY stops via Ctrl+C inside the terminal tab. */
          }}
          vcsChangeCount={repositoryStatus?.statusCount ?? 0}
          vcsAdditions={repositoryStatus?.additions ?? 0}
          vcsDeletions={repositoryStatus?.deletions ?? 0}
          platformOverride={platformOverride}
          footer={statusFooter}
        >
          <ProjectRail
            activeProjectId={activeProjectId}
            errorMessage={errorMessage}
            isImporting={isImporting}
            isLoading={isLoading || (isProjectLoading && foregroundProjectLoad)}
            onImportProject={() => setProjectAddOpen(true)}
            onOpenSettings={() => openSettings()}
            onPreloadProject={prefetchProject}
            onPreviewProject={handlePreviewProject}
            onRemoveProject={handleRemoveProject}
            onSelectProject={handleSelectProject}
            pendingProjectSelectionId={pendingProjectSelectionId}
            pendingProjectRemovalId={pendingProjectRemovalId}
            projectRemovalStatus={projectRemovalStatus}
            projects={projects}
            completedSessionCountsByProject={completedSessionCountsByProject}
            runningProjectIds={runningAgentProjectIds}
            onSessionsHoverEnter={
              activeView === 'agent' && explorerCollapsed && Boolean(activeProject)
                ? requestExplorerPeek
                : undefined
            }
            onSessionsHoverLeave={
              activeView === 'agent' && explorerCollapsed && Boolean(activeProject)
                ? releaseExplorerPeek
                : undefined
            }
          />
          <div
            aria-hidden={browserFocusMode ? true : undefined}
            className={cn(
              'relative flex min-h-0 min-w-0 flex-1 overflow-hidden transition-[max-width,opacity,transform] motion-standard',
              browserFocusMode
                ? 'pointer-events-none max-w-0 -translate-x-2 opacity-0'
                : 'max-w-full translate-x-0 opacity-100',
            )}
            inert={browserFocusMode ? true : undefined}
          >
            <div className="flex min-h-0 min-w-0 flex-1">
              {isProjectSelectionShellPending ? null : renderBody()}
            </div>
            <AppWideLoadingOverlay active={isProjectSelectionShellPending} />
          </div>
          <LazyPrerenderedSurface
            open={browserOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={<InlineSidebarLoadingShell label="Browser" open={browserOpen} width={640} />}
            >
              <LazyBrowserSidebar
                open={browserOpen}
                dictationAdapter={resolvedAdapter}
                projectId={activeProjectId}
                fullWidth={browserFocusMode}
                fullWidthTarget={browserFullWidthTarget}
                onFullWidthChange={handleBrowserFullWidthChange}
                penToolDisabledReason={browserPenToolDisabledReason}
                projectBrowserTargets={activeBrowserLaunchTargets}
                projectRootPath={activeProject?.repository?.rootPath ?? null}
                projectStartTargets={activeProjectStartTargets}
                onProjectBrowserTargetUnavailable={handleBrowserLaunchTargetUnavailable}
                pendingOpenUrl={pendingBrowserOpenUrl}
                onPendingOpenUrlConsumed={handlePendingBrowserOpenUrlConsumed}
                onAddAgentContext={handleAddBrowserContextToAgentComposer}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <LazyPrerenderedSurface
            open={usageOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <OverlaySidebarLoadingShell
                  label="Project usage"
                  open={usageOpen}
                  width={420}
                />
              }
            >
              <LazyUsageStatsSidebar
                open={usageOpen}
                projectId={activeProjectId}
                projectName={activeProject?.name ?? null}
                summary={activeUsageSummary}
                onClose={() => setUsageOpen(false)}
                onRefresh={refreshUsageSummary}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <SessionNotificationsSidebar
            notifications={unreadCompletedSessionNotifications}
            onClose={() => setNotificationsOpen(false)}
            onOpenSession={handleOpenNotificationSession}
            open={notificationsOpen}
          />
          {MOBILE_EMULATOR_SURFACES_ENABLED ? (
            <IosEmulatorSurface
              open={iosOpen}
              prewarm={startupSurfacePrewarm.shouldMount && shouldIncludeIosSurface()}
            />
          ) : null}
          <LazyPrerenderedSurface
            open={workflowsOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label="Workflows"
                  open={workflowsOpen}
                  width={420}
                />
              }
            >
              <LazyWorkflowsSidebar
                open={workflowsOpen}
                agents={workflowAgentInspector.agents}
                agentsLoading={workflowAgentInspector.agentsStatus === 'loading'}
                agentsError={workflowAgentInspector.agentsError}
                workflowDefinitions={WORKFLOWS_ENABLED ? workflowDefinitions : []}
                workflowRuns={WORKFLOWS_ENABLED ? workflowRuns : []}
                workflowsLoading={
                  WORKFLOWS_ENABLED &&
                  (workflowDefinitionsStatus === 'loading' || workflowRunsStatus === 'loading')
                }
                workflowsError={WORKFLOWS_ENABLED ? workflowDefinitionsError : null}
                selectedWorkflowId={
                  !WORKFLOWS_ENABLED || selectedWorkflowIsDraft || selectedWorkflowTemplatePreviewId
                    ? null
                    : selectedWorkflowDefinition?.id ?? null
                }
                selectedWorkflowTemplateId={
                  WORKFLOWS_ENABLED ? selectedWorkflowTemplatePreviewId : null
                }
                selectedWorkflowRunId={WORKFLOWS_ENABLED ? selectedWorkflowRun?.id ?? null : null}
                onSelectWorkflow={WORKFLOWS_ENABLED ? handleSelectWorkflowDefinition : undefined}
                onSelectWorkflowTemplate={WORKFLOWS_ENABLED ? handlePreviewWorkflowTemplate : undefined}
                onSelectWorkflowRun={WORKFLOWS_ENABLED ? handleSelectWorkflowRun : undefined}
                onCreateWorkflow={WORKFLOWS_ENABLED ? handleCreateWorkflow : undefined}
                onCreateWorkflowWithAgentCreate={
                  WORKFLOWS_ENABLED ? handleStartWorkflowAgentCreate : undefined
                }
                onCreateWorkflowFromTemplate={
                  WORKFLOWS_ENABLED ? handleCreateWorkflowFromTemplate : undefined
                }
                onUseWorkflowInChat={
                  WORKFLOWS_ENABLED ? handleUseWorkflowInChat : undefined
                }
                onStartWorkflowRun={
                  WORKFLOWS_ENABLED
                    ? (workflowId) => {
                        void handleRequestWorkflowStart(workflowId)
                      }
                    : undefined
                }
                onCancelWorkflowRun={
                  WORKFLOWS_ENABLED
                    ? (runId) => {
                        void handleCancelWorkflowRun(runId)
                      }
                    : undefined
                }
                onResumeWorkflowRun={
                  WORKFLOWS_ENABLED
                    ? (runId, nodeRunId, decision) => {
                        void handleResumeWorkflowCheckpoint(runId, nodeRunId, decision, { decision })
                      }
                    : undefined
                }
                selectedAgentRef={workflowAgentInspector.selectedRef}
                onSelectAgent={handleInspectWorkflowAgent}
                onCreateAgent={handleCreateAgent}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onEditAgent={(ref) => handleStartAgentAuthoringFromRef('edit', ref)}
                onDeleteAgent={handleArchiveAgentDefinition}
                onUseAgentInChat={handleUseWorkflowAgentInChat}
                modelOptions={agentView?.composerModelOptions ?? []}
                onSetAgentDefaultModel={handleSetWorkflowAgentDefaultModel}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <LazyPrerenderedSurface
            open={vcsOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <OverlaySidebarLoadingShell
                  label="Source control"
                  open={vcsOpen}
                  width={720}
                />
              }
            >
              <LazyVcsSidebar
                open={vcsOpen}
                projectId={activeProjectId}
                status={repositoryStatus}
                branchLabel={repositoryStatus?.branchLabel ?? activeProject?.branchLabel ?? null}
                onClose={closeVcs}
                onRefreshStatus={refreshVcsStatus}
                onLoadDiff={loadRepositoryDiff}
                commitMessageModel={vcsCommitMessageModel}
                onGenerateCommitMessage={generateCommitMessage}
                onStage={stagePaths}
                onUnstage={unstagePaths}
                onDiscard={discardChanges}
                onCommit={commitChanges}
                onFetch={fetchRepository}
                onPull={pullRepository}
                onPush={pushRepository}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <LazyPrerenderedSurface
            open={agentDockSurfaceOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
            stickyPrewarm
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label={computerUseOpen ? "Computer Use" : "Agent"}
                  open={agentDockSurfaceOpen}
                  width={readPersistedAgentDockWidth()}
                />
              }
            >
              <LazyAgentDockSidebar
                open={agentDockSurfaceOpen}
                prewarm={startupSurfacePrewarm.shouldMount}
                agent={agentDockSurfaceAgent}
                highChurnStore={highChurnStore}
                sessions={agentDockSurfaceSessions}
                selectedSessionId={agentDockSurfaceSelectedSessionId}
                isCreatingSession={isCreatingAgentSession}
                onClose={computerUseOpen ? closeComputerUse : () => setAgentDockOpen(false)}
                onSelectSession={computerUseOpen ? () => undefined : handleSelectAgentSession}
                onCreateSession={computerUseOpen ? () => undefined : handleCreateAgentSession}
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                toolCallGroupingPreference={toolCallGroupingPreference}
                agentRoutingAutoSwitchEnabled={agentRoutingAutoSwitchEnabled}
                customAgentDefinitions={customAgentDefinitions}
                agentDefaultModels={agentDefaultModels}
                workflowDefinitions={
                  WORKFLOWS_ENABLED && !computerUseOpen ? workflowDefinitions : []
                }
                onStartWorkflowFromComposer={
                  WORKFLOWS_ENABLED && !computerUseOpen
                    ? handleStartWorkflowFromComposer
                    : undefined
                }
                onOpenAgentManagement={handleOpenAgentManagement}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onStartWorkflowAgentCreate={
                  WORKFLOWS_ENABLED ? handleStartWorkflowAgentCreate : undefined
                }
                onOpenSettings={handleOpenAgentProviderSettings}
                onOpenDiagnostics={handleOpenAgentDiagnostics}
                onStartLogin={(options) => startOpenAiLogin(options)}
                onStartAutonomousRun={() => startAutonomousRun()}
                onInspectAutonomousRun={() => inspectAutonomousRun()}
                onCancelAutonomousRun={(runId) => cancelAutonomousRun(runId)}
                onStartRuntimeRun={
                  computerUseOpen ? startComputerUseRuntimeRun : (options) => startRuntimeRun(options)
                }
                onUpdateRuntimeRunControls={
                  computerUseOpen
                    ? updateComputerUseRuntimeRunControls
                    : (request) => updateRuntimeRunControls(request)
                }
                onClearSidebarChat={computerUseOpen ? clearComputerUseChat : undefined}
                sidebarChatClearDisabled={computerUseOpen ? !canClearComputerUseChat : undefined}
                sidebarChatClearPending={
                  computerUseOpen ? computerUseClearChatPending : undefined
                }
                sidebarChatClearTitle={computerUseOpen ? clearComputerUseChatTitle : undefined}
                onComposerControlsChange={(controls) => {
                  persistComposerSettings(controls)
                  if (!computerUseOpen) {
                    setAgentComposerControls((current) =>
                      sameRuntimeRunControlInput(current, controls) ? current : controls,
                    )
                  }
                }}
                onStartRuntimeSession={
                  computerUseOpen ? startComputerUseRuntimeSession : (options) => startRuntimeSession(options)
                }
                onStopRuntimeRun={
                  computerUseOpen ? stopComputerUseRuntimeRun : (runId) => stopRuntimeRun(runId)
                }
                onSubmitManualCallback={(flowId, manualInput) =>
                  submitOpenAiCallback(flowId, { manualInput })
                }
                onLogout={computerUseOpen ? logoutComputerUseRuntimeSession : () => logoutRuntimeSession()}
                onResolveOperatorAction={async (actionId, decision, options) => {
                  const result = await resolveOperatorAction(actionId, decision, {
                    userAnswer: options?.userAnswer ?? null,
                  })
                  if (decision === 'approve') refreshCustomAgentDefinitions()
                  return result
                }}
                onResumeOperatorRun={(actionId, options) =>
                  resumeOperatorRun(actionId, { userAnswer: options?.userAnswer ?? null })
                }
                onCodeUndoApplied={handleAgentCodeUndoApplied}
                onRetryStream={retry}
                agentCreateCanvasIncluded={agentCreateCanvasIncluded}
                pendingInitialRuntimeAgentId={
                  computerUseOpen ? null : pendingAgentDockRuntimeAgentId
                }
                pendingInitialAgentDefinitionId={
                  computerUseOpen ? null : pendingAgentDockAgentDefinitionId
                }
                pendingComposerInsert={
                  !computerUseOpen && pendingBrowserComposerInsert?.target === 'agent-dock'
                    ? pendingBrowserComposerInsert.insert
                    : null
                }
                onPendingComposerInsertConsumed={handleBrowserComposerInsertConsumed}
                onPendingInitialRuntimeAgentIdConsumed={() => {
                  if (!computerUseOpen && activeProject?.selectedAgentSessionId) {
                    handleClearPendingInitialRuntimeAgent(activeProject.selectedAgentSessionId)
                  }
                }}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <LazyPrerenderedSurface
            open={terminalOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label="Terminal"
                  open={terminalOpen}
                  width={520}
                />
              }
            >
              <LazyTerminalSidebar
                open={terminalOpen}
                projectId={activeProjectId}
                onOpenBrowserUrl={handleOpenUrlInBrowser}
                onBrowserLaunchTargetDetected={handleBrowserLaunchTargetDetected}
                registerHandle={(handle) => {
                  terminalSidebarHandleRef.current = handle
                }}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <LazyPrerenderedSurface
            open={settingsOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense fallback={<ModalLoadingShell open={settingsOpen} />}>
              <LazySettingsDialog
                open={settingsOpen}
                onOpenChange={setSettingsOpen}
                initialSection={settingsInitialSection}
                agent={agentView}
                providerCredentials={providerCredentials}
                providerCredentialsLoadStatus={providerCredentialsLoadStatus}
                providerCredentialsLoadError={providerCredentialsLoadError}
                providerCredentialsSaveStatus={providerCredentialsSaveStatus}
                providerCredentialsSaveError={providerCredentialsSaveError}
                onRefreshProviderCredentials={(options) => refreshProviderCredentials(options)}
                onUpsertProviderCredential={(request) => upsertProviderCredential(request)}
                onDeleteProviderCredential={(providerId) => deleteProviderCredential(providerId)}
                onStartOAuthLogin={(request) => startOAuthLogin(request)}
                doctorReport={doctorReport}
                doctorReportStatus={doctorReportStatus}
                doctorReportError={doctorReportError}
                environmentDiscoveryStatus={environmentDiscoveryStatus}
                environmentProfileSummary={environmentProfileSummary}
                onRefreshEnvironmentDiscovery={(options) => refreshEnvironmentDiscovery(options)}
                onVerifyUserEnvironmentTool={(request) => verifyUserEnvironmentTool(request)}
                onSaveUserEnvironmentTool={(request) => saveUserEnvironmentTool(request)}
                onRemoveUserEnvironmentTool={(id) => removeUserEnvironmentTool(id)}
                onRunDoctorReport={(request) => runDoctorReport(request)}
                dictationAdapter={resolvedAdapter}
                desktopControlAdapter={resolvedAdapter}
                soulAdapter={resolvedAdapter}
                agentToolingAdapter={resolvedAdapter}
                webSearchAdapter={resolvedAdapter}
                powerAdapter={resolvedAdapter}
                toolCallGroupingPreference={toolCallGroupingPreference}
                onToolCallGroupingPreferenceChange={handleToolCallGroupingPreferenceChange}
                agentRoutingAutoSwitchEnabled={agentRoutingAutoSwitchEnabled}
                onAgentRoutingAutoSwitchChange={handleAgentRoutingAutoSwitchChange}
                memoryAdapter={memoryAdapter}
                projectStateAdapter={projectStateAdapter}
                dangerAdapter={dangerAdapter}
                projects={projects}
                projectStartTargets={activeProjectStartTargets}
                onUpdateProjectStartTargets={handleUpdateProjectStartTargets}
                resolveProjectRunnerSuggestRequest={resolveProjectRunnerSuggestRequest}
                onSuggestProjectStartTargets={handleSuggestProjectStartTargets}
                projectRunnerModelOptions={projectRunnerModelOptions}
                mcpRegistry={mcpRegistry}
                mcpImportDiagnostics={mcpImportDiagnostics}
                mcpRegistryLoadStatus={mcpRegistryLoadStatus}
                mcpRegistryLoadError={mcpRegistryLoadError}
                mcpRegistryMutationStatus={mcpRegistryMutationStatus}
                pendingMcpServerId={pendingMcpServerId}
                mcpRegistryMutationError={mcpRegistryMutationError}
                onRefreshMcpRegistry={(options) => refreshMcpRegistry(options)}
                onUpsertMcpServer={(request) => upsertMcpServer(request)}
                onRemoveMcpServer={(serverId) => removeMcpServer(serverId)}
                onImportMcpServers={(path) => importMcpServers(path)}
                onRefreshMcpServerStatuses={(options) => refreshMcpServerStatuses(options)}
                skillRegistry={skillRegistry}
                skillRegistryLoadStatus={skillRegistryLoadStatus}
                skillRegistryLoadError={skillRegistryLoadError}
                skillRegistryMutationStatus={skillRegistryMutationStatus}
                pendingSkillSourceId={pendingSkillSourceId}
                skillRegistryMutationError={skillRegistryMutationError}
                onRefreshSkillRegistry={(options) => refreshSkillRegistry(options)}
                onReloadSkillRegistry={(options) => reloadSkillRegistry(options)}
                onSetSkillEnabled={(request) => setSkillEnabled(request)}
                onRemoveSkill={(request) => removeSkill(request)}
                onUpsertSkillLocalRoot={(request) => upsertSkillLocalRoot(request)}
                onRemoveSkillLocalRoot={(request) => removeSkillLocalRoot(request)}
                onUpdateProjectSkillSource={(request) => updateProjectSkillSource(request)}
                onUpdateGithubSkillSource={(request) => updateGithubSkillSource(request)}
                onUpsertPluginRoot={(request) => upsertPluginRoot(request)}
                onRemovePluginRoot={(request) => removePluginRoot(request)}
                onSetPluginEnabled={(request) => setPluginEnabled(request)}
                onRemovePlugin={(request) => removePlugin(request)}
                platformOverride={platformOverride}
                onPlatformOverrideChange={setPlatformOverride}
                onStartOnboarding={() => {
                  setSettingsOpen(false)
                  setOnboardingDismissed(false)
                  setOnboardingOpen(true)
                }}
                githubSession={githubSession}
                githubAuthStatus={githubAuthStatus}
                githubAuthError={githubAuthError}
                onGithubLogin={() => void loginWithGithub()}
                onGithubLogout={() => void logoutGithub()}
                onListAgentDefinitions={(request) => resolvedAdapter.listAgentDefinitions(request)}
                onArchiveAgentDefinition={(request) => resolvedAdapter.archiveAgentDefinition(request)}
                onGetAgentDefinitionVersion={(request) => resolvedAdapter.getAgentDefinitionVersion(request)}
                onGetAgentDefinitionVersionDiff={(request) => resolvedAdapter.getAgentDefinitionVersionDiff(request)}
                onAgentRegistryChanged={refreshCustomAgentDefinitions}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <ProjectAddDialog
            open={projectAddOpen}
            onOpenChange={setProjectAddOpen}
            isImporting={isImporting}
            errorMessage={errorMessage}
            onSelectExisting={() => importProject()}
            onPickParentFolder={() => resolvedAdapter.pickParentFolder()}
            onCreate={(parentPath, name) => createProject(parentPath, name)}
          />
          <Suspense fallback={null}>
            {startTargetsDialogOpen && activeProjectId ? (
              <LazyStartTargetsDialog
                open={startTargetsDialogOpen}
                onOpenChange={setStartTargetsDialogOpen}
                projectName={shellProjectName ?? activeProject?.name ?? activeProjectId}
                initialTargets={activeProjectStartTargets}
                onSubmit={async (targets) => {
                  await handleUpdateProjectStartTargets(targets)
                }}
                resolveSuggestRequest={resolveProjectRunnerSuggestRequest}
                onSuggest={handleSuggestProjectStartTargets}
              />
            ) : null}
          </Suspense>
        </XeroShell>
        <DesktopControlBanner
          adapter={resolvedAdapter}
          onOpenSettings={() => openSettings('desktopControl')}
        />
      </div>
      <AppBootLoadingOverlay active={showAppBootLoading} />
    </>
  )
}

function ForcedUpdateGate({ children }: { children: ReactNode }) {
  const update = useForcedAppUpdate()

  if (update.canContinue) {
    return <>{children}</>
  }

  const blockingStatus =
    update.status === 'checking' ||
    update.status === 'downloading' ||
    update.status === 'installing' ||
    update.status === 'error'
      ? update.status
      : 'checking'

  return (
    <UpdateScreen
      status={blockingStatus}
      percent={update.progress.percent}
      version={update.version}
      error={update.error}
      onRetry={() => void update.retry()}
    />
  )
}

export default function App() {
  return (
    <ForcedUpdateGate>
      <XeroApp />
    </ForcedUpdateGate>
  )
}
