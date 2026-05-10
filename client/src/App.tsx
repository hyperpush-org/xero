import {
  Activity,
  useCallback,
  useEffect,
  lazy,
  useMemo,
  useRef,
  useState,
  Suspense,
  type ReactNode,
} from 'react'
import type { AgentPaneCloseState, AgentRuntimeProps } from '@/components/xero/agent-runtime'
import { SetupEmptyState } from '@/components/xero/agent-runtime/setup-empty-state'
import { AgentWorkspace } from '@/components/xero/agent-workspace'
import { AgentSessionsSidebar } from '@/components/xero/agent-sessions-sidebar'
import { AgentWorkspaceDndProvider } from '@/components/xero/agent-runtime/agent-workspace-dnd-provider'
import { AgentCommandPalette } from '@/components/xero/agent-runtime/agent-command-palette'
import { type View } from '@/components/xero/data'
import { LoadingScreen } from '@/components/xero/loading-screen'
import { NoProjectEmptyState } from '@/components/xero/no-project-empty-state'
import { OnboardingFlow } from '@/components/xero/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/xero/project-load-error-state'
import { PhaseView } from '@/components/xero/phase-view'
import { ProjectAddDialog } from '@/components/xero/project-add-dialog'
import { ProjectRail } from '@/components/xero/project-rail'
import { UpdateScreen } from '@/components/xero/update-screen'
import { XeroShell, type PlatformVariant, type SurfacePreloadTarget } from '@/components/xero/shell'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { StatusFooterProps } from '@/components/xero/status-footer'
import type { SettingsSection } from '@/components/xero/settings-dialog'
import type { VcsCommitMessageModel } from '@/components/xero/vcs-sidebar'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { XeroDesktopAdapter as DefaultXeroDesktopAdapter, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { mapAgentSession, type RuntimeAgentIdDto, type RuntimeRunControlInputDto } from '@/src/lib/xero-model/runtime'
import {
  canonicalCustomAgentDefinitionSchema,
  type AgentDefinitionSummaryDto,
} from '@/src/lib/xero-model/agent-definition'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringAttachableSkillDto,
  AgentAuthoringSkillSearchResultDto,
  SearchAgentAuthoringSkillsResponseDto,
  AgentRefDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'
import { useWorkflowAgentInspector } from '@/src/features/xero/use-workflow-agent-inspector'
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
  type RefreshSource,
} from '@/src/features/xero/use-xero-desktop-state'
import { useForcedAppUpdate } from '@/src/features/updates/use-forced-app-update'
import {
  clearProjectSelectionPreview,
  previewProjectSelection,
} from '@/src/features/xero/project-selection-preview'
import { useGitHubAuth } from '@/src/lib/github-auth'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'
import { SHORTCUT_DEFINITIONS, type ShortcutId } from '@/src/features/shortcuts/shortcuts-definitions'
import { useShortcutListener } from '@/src/features/shortcuts/use-shortcut-listener'
import { startLayoutShiftGuard } from '@/lib/layout-shift-guard'
import { cn } from '@/lib/utils'

export interface XeroAppProps {
  adapter?: XeroDesktopAdapter
}

const loadAgentRuntime = () => import('@/components/xero/agent-runtime')
const loadExecutionView = () => import('@/components/xero/execution-view')
const loadBrowserSidebar = () => import('@/components/xero/browser-sidebar')
const loadIosEmulatorSidebar = () => import('@/components/xero/ios-emulator-sidebar')
const loadSolanaWorkbenchSidebar = () => import('@/components/xero/solana-workbench-sidebar')
const loadSettingsDialog = () => import('@/components/xero/settings-dialog')
const loadUsageStatsSidebar = () => import('@/components/xero/usage-stats-sidebar')
const loadVcsSidebar = () => import('@/components/xero/vcs-sidebar')
const loadWorkflowsSidebar = () => import('@/components/xero/workflows-sidebar')
const loadAgentDockSidebar = () => import('@/components/xero/agent-dock-sidebar')

const warmedSurfaceChunks = new Set<SurfacePreloadTarget>()

const IDLE_SURFACE_PRELOAD_SEQUENCE: SurfacePreloadTarget[] = [
  'agent-dock',
  'solana',
  'workflows',
  'vcs',
  'browser',
  'settings',
  'usage',
  'ios',
]

const SOLANA_WORKBENCH_WIDTH_STORAGE_KEY = 'xero.solana.workbench.width'
const SOLANA_WORKBENCH_MIN_WIDTH = 360
const SOLANA_WORKBENCH_DEFAULT_WIDTH = 440
const SOLANA_WORKBENCH_MAX_WIDTH = 900
const AGENT_DOCK_WIDTH_STORAGE_KEY = 'xero.agentDock.width'
const AGENT_DOCK_MIN_WIDTH = 320
const AGENT_DOCK_DEFAULT_WIDTH = 560
const AGENT_DOCK_MAX_WIDTH = 720
const STARTUP_SURFACE_PREWARM_SETTLE_MS = 320
const STARTUP_SURFACE_PRELOAD_TARGETS: SurfacePreloadTarget[] = [
  'agent-dock',
  'browser',
  'ios',
  'solana',
  'settings',
  'usage',
  'vcs',
  'workflows',
]

function readPersistedSolanaWorkbenchWidth(): number {
  if (typeof window === 'undefined') {
    return SOLANA_WORKBENCH_DEFAULT_WIDTH
  }

  try {
    const raw = window.localStorage?.getItem?.(SOLANA_WORKBENCH_WIDTH_STORAGE_KEY)
    if (!raw) {
      return SOLANA_WORKBENCH_DEFAULT_WIDTH
    }
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed)) {
      return SOLANA_WORKBENCH_DEFAULT_WIDTH
    }
    return Math.max(
      SOLANA_WORKBENCH_MIN_WIDTH,
      Math.min(SOLANA_WORKBENCH_MAX_WIDTH, parsed),
    )
  } catch {
    return SOLANA_WORKBENCH_DEFAULT_WIDTH
  }
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

function preloadSurfaceChunk(target: SurfacePreloadTarget): void {
  if (import.meta.env.MODE === 'test') {
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
  if (target === 'solana') {
    void loadSolanaWorkbenchSidebar().then((module) => module.preloadSolanaWorkbenchPanels())
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
    void loadAgentDockSidebar()
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
  await Promise.all([
    loadAgentRuntime(),
    loadExecutionView(),
    loadBrowserSidebar(),
    loadIosEmulatorSidebar(),
    loadSolanaWorkbenchSidebar().then((module) => module.preloadSolanaWorkbenchPanels()),
    loadSettingsDialog().then((module) => {
      void module.preloadSettingsSectionChunks().catch(() => undefined)
    }),
    loadUsageStatsSidebar(),
    loadVcsSidebar(),
    loadWorkflowsSidebar(),
    loadAgentDockSidebar(),
  ])
  STARTUP_SURFACE_PRELOAD_TARGETS.forEach((target) => warmedSurfaceChunks.add(target))
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
    const queue = [...IDLE_SURFACE_PRELOAD_SEQUENCE]

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
const LazySolanaWorkbenchSidebar = lazy(() =>
  loadSolanaWorkbenchSidebar().then((module) => ({ default: module.SolanaWorkbenchSidebar })),
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
): VcsCommitMessageModel | null {
  const modelId = composerControls?.modelId?.trim() || agent?.selectedModelId?.trim() || null
  if (!agent || !modelId) {
    return null
  }

  const providerId = agent.selectedModel?.providerId ?? agent.selectedProviderId ?? null
  const selectedModelOption =
    agent.providerModelCatalog.models.find(
      (model) =>
        model.modelId === modelId &&
        (!composerControls?.providerProfileId || model.profileId === composerControls.providerProfileId),
    ) ??
    agent.providerModelCatalog.models.find(
      (model) => model.modelId === modelId || model.selectionKey === `${providerId}:${modelId}`,
    ) ?? agent.selectedModelOption
  const providerProfileId =
    composerControls?.providerProfileId ??
    agent.runtimeRunActiveControls?.providerProfileId ??
    agent.runtimeRunPendingControls?.providerProfileId ??
    selectedModelOption?.profileId ??
    getCloudProviderDefaultProfileId(providerId) ??
    null

  return {
    providerProfileId,
    modelId,
    thinkingEffort:
      composerControls?.thinkingEffort ??
      agent.selectedThinkingEffort ??
      agent.selectedModelDefaultThinkingEffort ??
      null,
    label: selectedModelOption?.label ?? modelId,
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
    Boolean(left.planModeRequired) === Boolean(right.planModeRequired)
  )
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
}

function LazyPrerenderedSurface({
  children,
  open,
  prewarm = false,
}: LazyPrerenderedSurfaceProps) {
  const shouldMount = useActivatedSurface(open, prewarm)
  const renderedChildren = useFrozenSurfaceChildren(children, {
    active: open,
    prewarm,
  })

  if (!shouldMount) {
    return null
  }

  return <>{renderedChildren}</>
}

function SolanaWorkbenchOpeningShell({ open }: { open: boolean }) {
  const [width] = useState(readPersistedSolanaWorkbenchWidth)
  const targetWidth = open ? width : 0

  return (
    <aside
      aria-busy={open}
      aria-hidden={!open}
      aria-label="Loading Solana Workbench"
      className={cn(
        'sidebar-layout-island relative flex shrink-0 flex-col overflow-hidden bg-sidebar',
        open ? 'border-l border-border/80' : 'border-l-0',
      )}
      inert={!open ? true : undefined}
      style={{ width: targetWidth }}
    >
      <div className="flex h-full min-w-0 shrink-0 flex-col" style={{ width }}>
        <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 pl-3 pr-2">
          <div className="min-w-0 truncate text-[11px] font-semibold text-foreground">
            Solana Workbench
          </div>
          <div className="h-3 w-3 shrink-0 rounded-full border border-primary/30 border-t-primary animate-spin" />
        </div>
        <div className="flex min-h-0 flex-1">
          <div className="flex w-10 shrink-0 flex-col gap-1 border-r border-border/70 bg-sidebar px-2 py-3">
            {Array.from({ length: 8 }, (_, index) => (
              <div
                aria-hidden="true"
                className="h-5 w-5 rounded-md bg-secondary/45"
                key={index}
              />
            ))}
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-3 px-3 py-3">
            <div className="h-7 w-40 rounded-md bg-secondary/50" />
            <div className="h-20 rounded-md border border-border/60 bg-background/35" />
            <div className="space-y-2">
              <div className="h-3 w-3/4 rounded bg-secondary/45" />
              <div className="h-3 w-1/2 rounded bg-secondary/35" />
            </div>
          </div>
        </div>
      </div>
    </aside>
  )
}

function isForegroundProjectLoad(source: RefreshSource): boolean {
  return source === 'startup' || source === 'selection' || source === 'import' || source === 'remove'
}

function InlineSidebarLoadingShell({
  label,
  open,
  width = 420,
}: {
  label: string
  open: boolean
  width?: number
}) {
  const targetWidth = open ? width : 0

  return (
    <aside
      aria-busy={open}
      aria-hidden={!open}
      aria-label={`Loading ${label}`}
      className={cn(
        'sidebar-layout-island relative flex shrink-0 flex-col overflow-hidden bg-sidebar',
        open ? 'border-l border-border/80' : 'border-l-0',
      )}
      inert={!open ? true : undefined}
      style={{ width: targetWidth }}
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
  if (!open) {
    return null
  }

  return (
    <>
      <div className="fixed inset-0 z-40 bg-black/30" />
      <aside
        aria-busy="true"
        aria-label={`Loading ${label}`}
        className="fixed inset-y-0 right-0 z-50 flex flex-col overflow-hidden border-l border-border/80 bg-sidebar shadow-2xl"
        style={{ width }}
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
      </aside>
    </>
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

function useSolanaWorkbenchActivation(open: boolean, prewarm: boolean): boolean {
  const [active, setActive] = useState(prewarm)

  useEffect(() => {
    if (prewarm) {
      setActive(true)
      return
    }

    if (!open || active) {
      return
    }

    return scheduleAfterNextPaint(() => setActive(true))
  }, [active, open, prewarm])

  return active
}

function SolanaWorkbenchSurface({ open, prewarm = false }: { open: boolean; prewarm?: boolean }) {
  const shouldMount = useActivatedSurface(open, prewarm)
  const active = useSolanaWorkbenchActivation(open, prewarm)

  if (!shouldMount) {
    return null
  }

  return (
    <div
      aria-hidden={!open}
      className="relative flex shrink-0 overflow-hidden"
      inert={!open ? true : undefined}
      style={{ width: open ? undefined : 0 }}
    >
      <Suspense fallback={<SolanaWorkbenchOpeningShell open={open} />}>
        <LazySolanaWorkbenchSidebar active={active} open prewarm={prewarm} />
      </Suspense>
    </div>
  )
}

function AppBootLoadingOverlay({ active }: { active: boolean }) {
  if (!active) {
    return null
  }

  // Rendered as an app-root sibling of XeroShell; the shell main row uses
  // paint containment, which would otherwise clip fixed descendants.
  return (
    <div className="fixed inset-0 z-[2147483647] bg-background">
      <LoadingScreen className="h-screen w-screen" />
    </div>
  )
}

export function XeroApp({ adapter }: XeroAppProps) {
  const resolvedAdapter = adapter ?? DefaultXeroDesktopAdapter
  const [activeView, setActiveViewRaw] = useState<View>('phases')

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
    checkProviderProfile,
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
    refreshNotificationRoutes,
    upsertNotificationRoute,
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
  } = useXeroDesktopState({ adapter, subscribeRuntimeStreams: false })

  const {
    session: githubSession,
    status: githubAuthStatus,
    error: githubAuthError,
    login: loginWithGithub,
    logout: logoutGithub,
  } = useGitHubAuth()

  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsInitialSection, setSettingsInitialSection] = useState<SettingsSection>('providers')
  const [pendingAgentSessionId, setPendingAgentSessionId] = useState<string | null>(null)
  const [paneCloseStates, setPaneCloseStates] = useState<Record<string, AgentPaneCloseState>>({})
  const [pendingPaneCloseId, setPendingPaneCloseId] = useState<string | null>(null)
  const [agentComposerControls, setAgentComposerControls] =
    useState<RuntimeRunControlInputDto | null>(null)
  const [isCreatingAgentSession, setIsCreatingAgentSession] = useState(false)
  const [projectAddOpen, setProjectAddOpen] = useState(false)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [iosOpen, setIosOpen] = useState(false)
  const [solanaOpen, setSolanaOpen] = useState(false)
  const [vcsOpen, setVcsOpen] = useState(false)
  const [workflowsOpen, setWorkflowsOpen] = useState(false)
  const workflowAgentInspector = useWorkflowAgentInspector({
    adapter: resolvedAdapter,
    projectId: activeProjectId,
  })
  const [usageOpen, setUsageOpen] = useState(false)
  const [agentDockOpen, setAgentDockOpen] = useState(false)
  const [pendingInitialRuntimeAgent, setPendingInitialRuntimeAgent] =
    useState<{ agentSessionId: string; runtimeAgentId: RuntimeAgentIdDto } | null>(null)
  const [agentAuthoringSession, setAgentAuthoringSession] = useState<{
    mode: 'create' | 'edit' | 'duplicate'
    initialDetail: WorkflowAgentDetailDto | null
  } | null>(null)
  const [agentAuthoringLoading, setAgentAuthoringLoading] = useState(false)
  const [agentAuthoringCatalog, setAgentAuthoringCatalog] =
    useState<AgentAuthoringCatalogDto | null>(null)
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
  const shouldRestoreExplorerFromAutoCollapseRef = useRef(false)
  const previousBrowserOpenRef = useRef<boolean>(browserOpen)

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

  const openSettings = useCallback((section: SettingsSection = 'providers') => {
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

  const toggleBrowser = useCallback(() => {
    if (browserOpen) {
      setBrowserOpen(false)
      return
    }
    setIosOpen(false)
    setSolanaOpen(false)
    setVcsOpen(false)
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setBrowserOpen(true)
  }, [browserOpen])

  const toggleIos = useCallback(() => {
    if (iosOpen) {
      setIosOpen(false)
      return
    }
    setBrowserOpen(false)
    setSolanaOpen(false)
    setVcsOpen(false)
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setIosOpen(true)
  }, [iosOpen])

  const toggleSolana = useCallback(() => {
    if (solanaOpen) {
      setSolanaOpen(false)
      return
    }
    setBrowserOpen(false)
    setIosOpen(false)
    setVcsOpen(false)
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setSolanaOpen(true)
  }, [solanaOpen])

  const toggleVcs = useCallback(() => {
    if (vcsOpen) {
      setVcsOpen(false)
      return
    }
    setBrowserOpen(false)
    setIosOpen(false)
    setSolanaOpen(false)
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setVcsOpen(true)
  }, [vcsOpen])

  const toggleWorkflows = useCallback(() => {
    if (workflowsOpen) {
      setWorkflowsOpen(false)
      return
    }
    setBrowserOpen(false)
    setIosOpen(false)
    setSolanaOpen(false)
    setVcsOpen(false)
    setAgentDockOpen(false)
    setWorkflowsOpen(true)
  }, [workflowsOpen])

  const toggleAgentDock = useCallback(() => {
    if (agentDockOpen) {
      setAgentDockOpen(false)
      return
    }
    setBrowserOpen(false)
    setIosOpen(false)
    setSolanaOpen(false)
    setVcsOpen(false)
    setWorkflowsOpen(false)
    setUsageOpen(false)
    setAgentDockOpen(true)
  }, [agentDockOpen])
  useEffect(() => {
    if (activeView === 'agent' && agentDockOpen) {
      setAgentDockOpen(false)
    }
  }, [activeView, agentDockOpen])

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
    'browser' | 'ios' | 'solana' | 'vcs' | 'workflows' | 'usage' | 'agentDock' | null
  >(null)
  const phaseSidebarStateRef = useRef({
    browserOpen,
    iosOpen,
    solanaOpen,
    vcsOpen,
    workflowsOpen,
    usageOpen,
    agentDockOpen,
  })
  useEffect(() => {
    phaseSidebarStateRef.current = {
      browserOpen,
      iosOpen,
      solanaOpen,
      vcsOpen,
      workflowsOpen,
      usageOpen,
      agentDockOpen,
    }
  }, [browserOpen, iosOpen, solanaOpen, vcsOpen, workflowsOpen, usageOpen, agentDockOpen])
  useEffect(() => {
    if (phaseNodePanelOpen) {
      // Don't double-collapse if we already stashed a sidebar for this open.
      if (phaseSidebarRestoreKeyRef.current !== null) return
      const snapshot = phaseSidebarStateRef.current
      if (snapshot.browserOpen) {
        phaseSidebarRestoreKeyRef.current = 'browser'
        setBrowserOpen(false)
      } else if (snapshot.iosOpen) {
        phaseSidebarRestoreKeyRef.current = 'ios'
        setIosOpen(false)
      } else if (snapshot.solanaOpen) {
        phaseSidebarRestoreKeyRef.current = 'solana'
        setSolanaOpen(false)
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
      snapshot.iosOpen ||
      snapshot.solanaOpen ||
      snapshot.vcsOpen ||
      snapshot.workflowsOpen ||
      snapshot.usageOpen ||
      snapshot.agentDockOpen
    // If the user has opened a different sidebar in the meantime, respect
    // that and don't reintroduce the auto-collapsed one (would break the
    // single-open invariant the toggle handlers maintain).
    if (anySidebarOpen) return
    if (key === 'browser') setBrowserOpen(true)
    else if (key === 'ios') setIosOpen(true)
    else if (key === 'solana') setSolanaOpen(true)
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
  const [onboardingOpen, setOnboardingOpen] = useState(false)
  const [launchMode, setLaunchMode] = useState<string | null>(null)
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    void invoke<string>('get_launch_mode')
      .then((value) => {
        if (!cancelled && typeof value === 'string' && value.length > 0) {
          setLaunchMode(value)
        }
      })
      .catch(() => {
        // Command unavailable in older builds — leave launchMode null.
      })
    return () => {
      cancelled = true
    }
  }, [])
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
    spendActive: usageOpen,
    onSpendClick: activeProjectId
      ? () => {
          preloadSurfaceChunk('usage')
          setUsageOpen((current) => !current)
        }
      : undefined,
  }
  const vcsCommitMessageModel = useMemo(
    () => getVcsCommitMessageModel(agentView, agentComposerControls),
    [agentComposerControls, agentView],
  )
  const workflowAgentCreateActive =
    activeView === 'phases' && agentAuthoringSession?.mode === 'create'
  const pendingAgentDockRuntimeAgentId: RuntimeAgentIdDto | null =
    workflowAgentCreateActive && isCreatingAgentSession
      ? 'agent_create'
      : pendingInitialRuntimeAgent &&
          pendingInitialRuntimeAgent.agentSessionId === activeProject?.selectedAgentSessionId
        ? pendingInitialRuntimeAgent.runtimeAgentId
        : null

  useEffect(() => {
    if (!onboardingDismissed && !isLoading && projects.length === 0) {
      setOnboardingOpen(true)
    }
  }, [isLoading, onboardingDismissed, projects.length])

  const selectedAgentSessionId = activeProject?.selectedAgentSessionId ?? null
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
  const paneCount = agentWorkspaceLayout?.paneSlots.length ?? 1
  const isMultiPane = paneCount > 1
  useEffect(() => {
    const livePaneIds = new Set(agentWorkspaceLayout?.paneSlots.map((slot) => slot.id) ?? [])
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
  }, [agentWorkspaceLayout])
  const sessionPaneAssignments = useMemo<Record<string, number>>(() => {
    const map: Record<string, number> = {}
    if (!agentWorkspaceLayout) return map
    agentWorkspaceLayout.paneSlots.forEach((slot, index) => {
      if (slot.agentSessionId) {
        map[slot.agentSessionId] = index + 1
      }
    })
    return map
  }, [agentWorkspaceLayout])
  const dndPaneSlots = useMemo(() => {
    if (!agentWorkspaceLayout || !activeProject) return []
    const projectLabel = activeProject.name ?? null
    return agentWorkspaceLayout.paneSlots.map((slot, index) => {
      const session = slot.agentSessionId
        ? activeProject.agentSessions.find(
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
  }, [activeProject, agentWorkspaceLayout])
  const agentCommandPalettePanes = useMemo(() => {
    if (!agentWorkspaceLayout || !activeProject) return []
    return agentWorkspaceLayout.paneSlots.map((slot, index) => {
      const session = activeProject.agentSessions.find(
        (candidate) => candidate.agentSessionId === slot.agentSessionId,
      )
      return {
        paneId: slot.id,
        paneNumber: index + 1,
        sessionTitle: session?.title ?? 'Untitled',
        isFocused: slot.id === agentWorkspaceLayout.focusedPaneId,
      }
    })
  }, [activeProject, agentWorkspaceLayout])
  const preSpawnExplorerModeRef = useRef<'pinned' | 'collapsed' | null>(null)
  useEffect(() => {
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

      const pane = agentWorkspacePanes.find((candidate) => candidate.paneId === paneId)
      if (!pane) {
        return null
      }

      return {
        hasRunningRun: Boolean(pane.agent?.runtimeRun && !pane.agent.runtimeRun.isTerminal),
        hasUnsavedComposerText: false,
        sessionTitle: pane.agent?.project.selectedAgentSession?.title?.trim() || 'New Chat',
      }
    },
    [agentWorkspacePanes, paneCloseStates],
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
      setWorkflowsOpen(false)
      setBrowserOpen(false)
      setIosOpen(false)
      setSolanaOpen(false)
      setVcsOpen(false)
      setUsageOpen(false)
      setActiveView('phases')
      setAgentAuthoringSession({ mode: 'create', initialDetail: null })
      setAgentDockOpen(true)
      setIsCreatingAgentSession(true)
      void createAgentSession()
        .then((updatedProject) => {
          const newSessionId = updatedProject?.selectedAgentSessionId
          if (newSessionId) {
            setPendingInitialRuntimeAgent({
              agentSessionId: newSessionId,
              runtimeAgentId: 'agent_create',
            })
          }
        })
        .finally(() => {
          setIsCreatingAgentSession(false)
        })
    },
    [activeProjectId, createAgentSession],
  )

  const handleClearPendingInitialRuntimeAgent = useCallback(
    (agentSessionId: string) => {
      setPendingInitialRuntimeAgent((current) =>
        current?.agentSessionId === agentSessionId ? null : current,
      )
    },
    [],
  )

  // Lazy-load the baseline authoring catalog once a session opens. Skills can
  // be expanded later by query-scoped online search from the picker.
  useEffect(() => {
    if (!agentAuthoringSession) return
    if (!activeProjectId) return
    if (agentAuthoringCatalog) return
    let cancelled = false
    void resolvedAdapter
      .getAgentAuthoringCatalog({ projectId: activeProjectId })
      .then((catalog) => {
        if (cancelled) return
        setAgentAuthoringCatalog(catalog)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        console.error('Failed to load agent authoring catalog', error)
      })
    return () => {
      cancelled = true
    }
  }, [agentAuthoringSession, activeProjectId, agentAuthoringCatalog, resolvedAdapter])

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
      const skill = await resolvedAdapter.resolveAgentAuthoringSkill({
        projectId: activeProjectId,
        source: result.source,
        skillId: result.skillId,
      })
      setAgentAuthoringCatalog((current) => {
        if (!current) return current
        const bySourceId = new Map<string, AgentAuthoringAttachableSkillDto>()
        for (const skill of current.attachableSkills) {
          bySourceId.set(skill.sourceId, skill)
        }
        bySourceId.set(skill.sourceId, skill)
        return {
          ...current,
          attachableSkills: [...bySourceId.values()],
        }
      })
      return skill
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleStartAgentAuthoringCreate = useCallback(() => {
    setWorkflowsOpen(false)
    setAgentDockOpen(false)
    setActiveView('phases')
    setAgentAuthoringSession({ mode: 'create', initialDetail: null })
  }, [])

  const handleStartWorkflowAgentCreate = useCallback(() => {
    if (!activeProjectId) return
    setWorkflowsOpen(false)
    setBrowserOpen(false)
    setIosOpen(false)
    setSolanaOpen(false)
    setVcsOpen(false)
    setUsageOpen(false)
    setActiveView('phases')
    setAgentAuthoringSession({ mode: 'create', initialDetail: null })
    setAgentDockOpen(true)
    if (activeProject?.selectedAgentSessionId) {
      setPendingInitialRuntimeAgent({
        agentSessionId: activeProject.selectedAgentSessionId,
        runtimeAgentId: 'agent_create',
      })
    }
  }, [activeProject?.selectedAgentSessionId, activeProjectId, setActiveView])

  const handleStartAgentAuthoringFromRef = useCallback(
    async (mode: 'edit' | 'duplicate', ref: AgentRefDto) => {
      if (!activeProjectId) return
      setAgentAuthoringLoading(true)
      try {
        const detail = await resolvedAdapter.getWorkflowAgentDetail({
          projectId: activeProjectId,
          ref,
        })
        setWorkflowsOpen(false)
        setAgentDockOpen(false)
        setActiveView('phases')
        setAgentAuthoringSession({ mode, initialDetail: detail })
      } catch (error) {
        console.error('Failed to load agent definition for authoring', error)
      } finally {
        setAgentAuthoringLoading(false)
      }
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleCloseAgentAuthoring = useCallback(() => {
    setAgentAuthoringSession(null)
  }, [])

  const handleAgentAuthoringSubmit = useCallback(
    async ({
      snapshot,
      mode,
      definitionId,
    }: {
      snapshot: Record<string, unknown>
      mode: 'create' | 'edit' | 'duplicate'
      definitionId?: string
    }) => {
      if (!activeProjectId) {
        throw new Error('Select a project before saving an agent definition.')
      }
      const definition = canonicalCustomAgentDefinitionSchema.parse(snapshot)
      if (mode === 'edit' && definitionId) {
        return resolvedAdapter.updateAgentDefinition({
          projectId: activeProjectId,
          definitionId,
          definition,
        })
      }
      return resolvedAdapter.saveAgentDefinition({
        projectId: activeProjectId,
        definition,
      })
    },
    [activeProjectId, resolvedAdapter],
  )

  const handleAgentAuthoringSaved = useCallback(() => {
    refreshCustomAgentDefinitions()
    void workflowAgentInspector.refreshAgents()
  }, [refreshCustomAgentDefinitions, workflowAgentInspector])

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
  const handleAgentComposerControlsChange = useCallback((
    _paneId: string,
    controls: RuntimeRunControlInputDto | null,
  ) => {
    setAgentComposerControls((current) =>
      sameRuntimeRunControlInput(current, controls) ? current : controls,
    )
  }, [])
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
  const handleAgentRefreshNotificationRoutes = useCallback(
    (
      _paneId: string,
      options?: Parameters<NonNullable<AgentRuntimeProps['onRefreshNotificationRoutes']>>[0],
    ) => refreshNotificationRoutes(options),
    [refreshNotificationRoutes],
  )
  const handleAgentUpsertNotificationRoute = useCallback(
    (
      _paneId: string,
      request: Parameters<NonNullable<AgentRuntimeProps['onUpsertNotificationRoute']>>[0],
    ) => upsertNotificationRoute(request),
    [upsertNotificationRoute],
  )
  const handleAgentCodeUndoApplied = useCallback(() => retry(), [retry])
  const handleStartWorkflowRun = useCallback(() => startRuntimeRun(), [startRuntimeRun])

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
    const sessionsPeekAvailable = activeView === 'agent' && explorerMode === 'collapsed'
    const agentUsesHeavySwitchSurface = paneCount >= 3

    return (
      <AgentWorkspaceDndProvider
        paneSlots={dndPaneSlots}
        onReorderPanes={reorderPanes}
        onOpenSessionInNewPane={openSessionInNewPane}
      >
        <AgentSessionsSidebar
          projectId={activeProject.id}
          sessions={activeProject.agentSessions}
          selectedSessionId={activeProject.selectedAgentSessionId}
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
          enabled={activeView === 'agent' && Boolean(agentWorkspaceLayout)}
          panes={agentCommandPalettePanes}
          spawnDisabled={paneCount >= 6}
          onSpawnPane={handleSpawnPane}
          onClosePane={handleClosePane}
          onFocusPane={handleFocusPane}
          onCycleFocus={cycleFocusPane}
        />
        <AlertDialog
          open={pendingPaneCloseId !== null}
          onOpenChange={(open) => {
            if (!open) {
              setPendingPaneCloseId(null)
            }
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Close agent pane?</AlertDialogTitle>
              <AlertDialogDescription>
                {pendingPaneCloseCopy} Closing keeps the session in the sidebar, but this pane will stop showing it.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                onClick={handleConfirmClosePane}
              >
                Close pane
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
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
                canStartRun={Boolean(
                  agentView?.runtimeRunActionStatus !== undefined &&
                    !agentView.runtimeRun &&
                    agentView.runtimeSession?.isAuthenticated,
                )}
                isStartingRun={agentView?.runtimeRunActionStatus === 'running'}
                onOpenSettings={handleOpenAgentProviderSettings}
                onStartRun={handleStartWorkflowRun}
                onToggleWorkflows={toggleWorkflows}
                workflowsOpen={workflowsOpen}
                onCreateAgent={handleCreateAgent}
                agentDetail={workflowAgentInspector.detail}
                agentDetailStatus={workflowAgentInspector.detailStatus}
                agentDetailError={workflowAgentInspector.detailError}
                onClearAgentSelection={() => workflowAgentInspector.selectAgent(null)}
                onReloadAgentDetail={workflowAgentInspector.reloadDetail}
                authoringSession={agentAuthoringSession}
                authoringCatalog={agentAuthoringCatalog}
                onSearchAttachableSkills={handleSearchAttachableSkills}
                onResolveAttachableSkill={handleResolveAttachableSkill}
                onAuthoringSubmit={handleAgentAuthoringSubmit}
                onAuthoringSaved={(response) => {
                  handleAgentAuthoringSaved()
                  if (response.applied) handleCloseAgentAuthoring()
                }}
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

          {agentView ? (
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
                layout={agentWorkspaceLayout}
                panes={agentWorkspacePanes}
                highChurnStore={highChurnStore}
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                customAgentDefinitions={customAgentDefinitions}
                onOpenAgentManagement={handleOpenAgentManagement}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onStartWorkflowAgentCreate={handleStartWorkflowAgentCreate}
                onCreateSession={handleCreateAgentSession}
                pendingInitialRuntimeAgent={pendingInitialRuntimeAgent}
                onClearPendingInitialRuntimeAgent={handleClearPendingInitialRuntimeAgent}
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
                onRefreshNotificationRoutes={handleAgentRefreshNotificationRoutes}
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
                onUpsertNotificationRoute={handleAgentUpsertNotificationRoute}
                onCodeUndoApplied={handleAgentCodeUndoApplied}
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
                  listProjectFiles={listProjectFiles}
                  readProjectFile={readProjectFile}
                  writeProjectFile={writeProjectFile}
                  revokeProjectAssetTokens={revokeProjectAssetTokens}
                  openProjectFileExternal={openProjectFileExternal}
                  createProjectEntry={createProjectEntry}
                  renameProjectEntry={renameProjectEntry}
                  moveProjectEntry={moveProjectEntry}
                  deleteProjectEntry={deleteProjectEntry}
                  searchProject={searchProject}
                  replaceInProject={replaceInProject}
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
  const shouldAutoOpenOnboarding = !onboardingDismissed && !isLoading && projects.length === 0
  const showOnboarding = (onboardingOpen || shouldAutoOpenOnboarding) && !onboardingDismissed && !isLoading
  const isForegroundProjectSelection = pendingProjectSelectionId !== null
  const pendingProjectSelectionName = pendingProjectSelectionId
    ? projects.find((project) => project.id === pendingProjectSelectionId)?.name ?? null
    : null
  const shellProjectName = pendingProjectSelectionName ?? activeProject?.name
  const foregroundProjectLoad = isForegroundProjectLoad(refreshSource)
  const isBlockingProjectLoading =
    isProjectLoading &&
    foregroundProjectLoad &&
    !isForegroundProjectSelection &&
    !activeProject
  const isProjectSelectionShellPending =
    pendingProjectSelectionId !== null && activeProjectId !== pendingProjectSelectionId
  const startupSurfacePrewarm = useStartupSurfacePrewarm(
    !showOnboarding && !isLoading && !isBlockingProjectLoading,
  )
  useIdleSurfacePreloads(
    !showOnboarding &&
      !isLoading &&
      !isBlockingProjectLoading &&
      startupSurfacePrewarm.ready,
  )
  const showStartupSurfacePrewarm = !startupSurfacePrewarm.ready
  const showAppBootLoading = !showOnboarding && (
    isLoading ||
    isBlockingProjectLoading ||
    showStartupSurfacePrewarm
  )

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
        onToggleSolana={toggleSolana}
        solanaOpen={solanaOpen}
        onToggleVcs={toggleVcs}
        vcsOpen={vcsOpen}
        onToggleWorkflows={toggleWorkflows}
        workflowsOpen={workflowsOpen}
        onToggleAgentDock={toggleAgentDock}
        agentDockOpen={agentDockOpen}
        agentDockDisabled={activeView === 'agent' || !activeProject}
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
          notificationRoutes={agentView?.notificationRoutes ?? []}
          notificationRouteMutationStatus={agentView?.notificationRouteMutationStatus ?? 'idle'}
          pendingNotificationRouteId={agentView?.pendingNotificationRouteId ?? null}
          notificationRouteMutationError={agentView?.notificationRouteMutationError ?? null}
          environmentPermissionRequests={environmentDiscoveryStatus?.permissionRequests ?? []}
          onResolveEnvironmentPermissions={resolveEnvironmentPermissions}
          launchMode={launchMode}
          onImportProject={async () => {
            await importProject()
          }}
          onRefreshProviderCredentials={(options) => refreshProviderCredentials(options)}
          onUpsertProviderCredential={(request) => upsertProviderCredential(request)}
          onDeleteProviderCredential={(providerId) => deleteProviderCredential(providerId)}
          onStartOAuthLogin={(request) => startOAuthLogin(request)}
          onUpsertNotificationRoute={(request) => upsertNotificationRoute(request)}
          onComplete={() => {
            setOnboardingDismissed(true)
            setOnboardingOpen(false)
          }}
          onDismiss={() => {
            setOnboardingDismissed(true)
            setOnboardingOpen(false)
          }}
        />
      </XeroShell>
    )
  }

  return (
    <>
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
          onToggleSolana={toggleSolana}
          solanaOpen={solanaOpen}
          onToggleVcs={toggleVcs}
          vcsOpen={vcsOpen}
          onToggleWorkflows={toggleWorkflows}
          workflowsOpen={workflowsOpen}
          onToggleAgentDock={toggleAgentDock}
          agentDockOpen={agentDockOpen}
          agentDockDisabled={activeView === 'agent' || !activeProject}
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
            onOpenSettings={() => openSettings('providers')}
            onPreloadProject={prefetchProject}
            onPreviewProject={handlePreviewProject}
            onRemoveProject={handleRemoveProject}
            onSelectProject={handleSelectProject}
            pendingProjectSelectionId={pendingProjectSelectionId}
            pendingProjectRemovalId={pendingProjectRemovalId}
            projectRemovalStatus={projectRemovalStatus}
            projects={projects}
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
          {isProjectSelectionShellPending ? <LoadingScreen /> : renderBody()}
          <LazyPrerenderedSurface
            open={browserOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={<InlineSidebarLoadingShell label="Browser" open={browserOpen} width={640} />}
            >
              <LazyBrowserSidebar open={browserOpen} />
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
          <LazyPrerenderedSurface
            open={iosOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label="iOS Simulator"
                  open={iosOpen}
                  width={640}
                />
              }
            >
              <LazyIosEmulatorSidebar open={iosOpen} />
            </Suspense>
          </LazyPrerenderedSurface>
          <SolanaWorkbenchSurface
            open={solanaOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          />
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
                selectedAgentRef={workflowAgentInspector.selectedRef}
                onSelectAgent={workflowAgentInspector.selectAgent}
                onCreateAgent={handleCreateAgent}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onEditAgent={(ref) => handleStartAgentAuthoringFromRef('edit', ref)}
                onDuplicateAgent={(ref) => handleStartAgentAuthoringFromRef('duplicate', ref)}
                onDeleteAgent={handleArchiveAgentDefinition}
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
            open={agentDockOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label="Agent"
                  open={agentDockOpen}
                  width={readPersistedAgentDockWidth()}
                />
              }
            >
              <LazyAgentDockSidebar
                open={agentDockOpen}
                agent={agentView}
                highChurnStore={highChurnStore}
                sessions={activeProject?.agentSessions ?? []}
                selectedSessionId={activeProject?.selectedAgentSessionId ?? null}
                isCreatingSession={isCreatingAgentSession}
                onClose={() => setAgentDockOpen(false)}
                onSelectSession={handleSelectAgentSession}
                onCreateSession={handleCreateAgentSession}
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                customAgentDefinitions={customAgentDefinitions}
                onOpenAgentManagement={handleOpenAgentManagement}
                onCreateAgentByHand={handleStartAgentAuthoringCreate}
                onStartWorkflowAgentCreate={handleStartWorkflowAgentCreate}
                onOpenSettings={handleOpenAgentProviderSettings}
                onOpenDiagnostics={handleOpenAgentDiagnostics}
                onStartLogin={(options) => startOpenAiLogin(options)}
                onStartAutonomousRun={() => startAutonomousRun()}
                onInspectAutonomousRun={() => inspectAutonomousRun()}
                onCancelAutonomousRun={(runId) => cancelAutonomousRun(runId)}
                onStartRuntimeRun={(options) => startRuntimeRun(options)}
                onUpdateRuntimeRunControls={(request) => updateRuntimeRunControls(request)}
                onComposerControlsChange={(controls) =>
                  setAgentComposerControls((current) =>
                    sameRuntimeRunControlInput(current, controls) ? current : controls,
                  )
                }
                onStartRuntimeSession={(options) => startRuntimeSession(options)}
                onStopRuntimeRun={(runId) => stopRuntimeRun(runId)}
                onSubmitManualCallback={(flowId, manualInput) =>
                  submitOpenAiCallback(flowId, { manualInput })
                }
                onLogout={() => logoutRuntimeSession()}
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
                onRefreshNotificationRoutes={(options) => refreshNotificationRoutes(options)}
                onUpsertNotificationRoute={(request) => upsertNotificationRoute(request)}
                onCodeUndoApplied={handleAgentCodeUndoApplied}
                onRetryStream={retry}
                agentCreateCanvasIncluded={workflowAgentCreateActive}
                pendingInitialRuntimeAgentId={pendingAgentDockRuntimeAgentId}
                onPendingInitialRuntimeAgentIdConsumed={() => {
                  if (activeProject?.selectedAgentSessionId) {
                    handleClearPendingInitialRuntimeAgent(activeProject.selectedAgentSessionId)
                  }
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
                onCheckProviderProfile={(profileId, options) => checkProviderProfile(profileId, options)}
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
                soulAdapter={resolvedAdapter}
                agentToolingAdapter={resolvedAdapter}
                onUpsertNotificationRoute={(request) =>
                  upsertNotificationRoute({ ...request, updatedAt: new Date().toISOString() })
                }
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
                onAgentRegistryChanged={refreshCustomAgentDefinitions}
              />
            </Suspense>
          </LazyPrerenderedSurface>
          <ProjectAddDialog
            open={projectAddOpen}
            onOpenChange={setProjectAddOpen}
            isImporting={isImporting}
            onSelectExisting={() => importProject()}
            onPickParentFolder={() => resolvedAdapter.pickParentFolder()}
            onCreate={(parentPath, name) => createProject(parentPath, name)}
          />
        </XeroShell>
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
