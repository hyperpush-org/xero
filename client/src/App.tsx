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
import { AgentCommandPalette } from '@/components/xero/agent-runtime/agent-command-palette'
import { ArchivedSessionsDialog } from '@/components/xero/archived-sessions-dialog'
import { type View } from '@/components/xero/data'
import { LoadingScreen } from '@/components/xero/loading-screen'
import { NoProjectEmptyState } from '@/components/xero/no-project-empty-state'
import { OnboardingFlow } from '@/components/xero/onboarding/onboarding-flow'
import { ProjectLoadErrorState } from '@/components/xero/project-load-error-state'
import { PhaseView } from '@/components/xero/phase-view'
import { ProjectAddDialog } from '@/components/xero/project-add-dialog'
import { ProjectRail } from '@/components/xero/project-rail'
import { XeroShell, type PlatformVariant, type SurfacePreloadTarget } from '@/components/xero/shell'
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
import { mapAgentSession, type RuntimeRunControlInputDto } from '@/src/lib/xero-model/runtime'
import type { AgentDefinitionSummaryDto } from '@/src/lib/xero-model/agent-definition'
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
} from '@/src/features/xero/use-xero-desktop-state'
import { useGitHubAuth } from '@/src/lib/github-auth'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'
import { startLayoutShiftGuard } from '@/lib/layout-shift-guard'
import { cn } from '@/lib/utils'

export interface XeroAppProps {
  adapter?: XeroDesktopAdapter
}

const loadAgentRuntime = () => import('@/components/xero/agent-runtime')
const loadExecutionView = () => import('@/components/xero/execution-view')
const loadGamesSidebar = () => import('@/components/xero/games-sidebar')
const loadBrowserSidebar = () => import('@/components/xero/browser-sidebar')
const loadIosEmulatorSidebar = () => import('@/components/xero/ios-emulator-sidebar')
const loadAndroidEmulatorSidebar = () => import('@/components/xero/android-emulator-sidebar')
const loadSolanaWorkbenchSidebar = () => import('@/components/xero/solana-workbench-sidebar')
const loadSettingsDialog = () => import('@/components/xero/settings-dialog')
const loadUsageStatsSidebar = () => import('@/components/xero/usage-stats-sidebar')
const loadVcsSidebar = () => import('@/components/xero/vcs-sidebar')
const loadWorkflowsSidebar = () => import('@/components/xero/workflows-sidebar')

const warmedSurfaceChunks = new Set<SurfacePreloadTarget>()

const IDLE_SURFACE_PRELOAD_SEQUENCE: SurfacePreloadTarget[] = [
  'solana',
  'workflows',
  'vcs',
  'browser',
  'settings',
  'usage',
  'android',
  'ios',
  'games',
]

const SOLANA_WORKBENCH_WIDTH_STORAGE_KEY = 'xero.solana.workbench.width'
const SOLANA_WORKBENCH_MIN_WIDTH = 360
const SOLANA_WORKBENCH_DEFAULT_WIDTH = 440
const SOLANA_WORKBENCH_MAX_WIDTH = 900
const SIDEBAR_REVEAL_EASE_CSS = 'cubic-bezier(0.22, 1, 0.36, 1)'
const SIDEBAR_WIDTH_DURATION_MS = 200
const SOLANA_WORKBENCH_MOUNT_DELAY_MS = SIDEBAR_WIDTH_DURATION_MS + 40
const STARTUP_SURFACE_PREWARM_SETTLE_MS = 320
const STARTUP_SURFACE_PRELOAD_TARGETS: SurfacePreloadTarget[] = [
  'games',
  'browser',
  'ios',
  'android',
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

function preloadSurfaceChunk(target: SurfacePreloadTarget): void {
  if (import.meta.env.MODE === 'test') {
    return
  }

  if (target === 'tools') {
    preloadSurfaceChunk('browser')
    preloadSurfaceChunk('solana')
    preloadSurfaceChunk('android')
    preloadSurfaceChunk('ios')
    return
  }

  if (warmedSurfaceChunks.has(target)) {
    return
  }
  warmedSurfaceChunks.add(target)

  if (target === 'games') {
    void loadGamesSidebar()
    return
  }
  if (target === 'browser') {
    void loadBrowserSidebar()
    return
  }
  if (target === 'ios') {
    void loadIosEmulatorSidebar()
    return
  }
  if (target === 'android') {
    void loadAndroidEmulatorSidebar()
    return
  }
  if (target === 'solana') {
    void loadSolanaWorkbenchSidebar().then((module) => module.preloadSolanaWorkbenchPanels())
    return
  }
  if (target === 'settings') {
    void loadSettingsDialog()
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
    loadGamesSidebar(),
    loadBrowserSidebar(),
    loadIosEmulatorSidebar(),
    loadAndroidEmulatorSidebar(),
    loadSolanaWorkbenchSidebar().then((module) => module.preloadSolanaWorkbenchPanels()),
    loadSettingsDialog(),
    loadUsageStatsSidebar(),
    loadVcsSidebar(),
    loadWorkflowsSidebar(),
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
const LazyGamesSidebar = lazy(() =>
  loadGamesSidebar().then((module) => ({ default: module.GamesSidebar })),
)
const LazyBrowserSidebar = lazy(() =>
  loadBrowserSidebar().then((module) => ({ default: module.BrowserSidebar })),
)
const LazyIosEmulatorSidebar = lazy(() =>
  loadIosEmulatorSidebar().then((module) => ({ default: module.IosEmulatorSidebar })),
)
const LazyAndroidEmulatorSidebar = lazy(() =>
  loadAndroidEmulatorSidebar().then((module) => ({ default: module.AndroidEmulatorSidebar })),
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

function LazyActivityPane({
  active,
  children,
  className,
  name,
  prewarm = false,
}: LazyActivityPaneProps) {
  const shouldMount = useActivatedSurface(active, prewarm)

  if (!shouldMount) {
    return null
  }

  const pane = (
    <div
      aria-hidden={!active}
      className={className}
      inert={!active ? true : undefined}
    >
      {children}
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

  if (!shouldMount) {
    return null
  }

  return (
    <div
      aria-hidden={!active}
      className={className}
      inert={!active ? true : undefined}
    >
      {children}
    </div>
  )
}

interface LazyActivitySurfaceProps {
  children: ReactNode
  name: string
  open: boolean
  prewarm?: boolean
}

function LazyActivitySurface({ children, name, open, prewarm = false }: LazyActivitySurfaceProps) {
  const shouldMount = useActivatedSurface(open, prewarm)

  if (!shouldMount) {
    return null
  }

  return (
    <Activity mode={open ? 'visible' : 'hidden'} name={name}>
      {children}
    </Activity>
  )
}

function useDeferredSolanaWorkbenchMount(open: boolean, prewarm: boolean): boolean {
  const [mounted, setMounted] = useState(prewarm)

  useEffect(() => {
    if (prewarm) {
      setMounted(true)
      preloadSurfaceChunk('solana')
      return
    }

    if (!open || mounted) {
      return
    }

    preloadSurfaceChunk('solana')

    if (typeof window === 'undefined') {
      setMounted(true)
      return
    }

    let timer = 0
    let frame = 0
    const scheduleMount = () => {
      timer = window.setTimeout(() => {
        setMounted(true)
      }, SOLANA_WORKBENCH_MOUNT_DELAY_MS)
    }

    if (typeof window.requestAnimationFrame === 'function') {
      frame = window.requestAnimationFrame(scheduleMount)
    } else {
      scheduleMount()
    }

    return () => {
      if (frame !== 0) {
        window.cancelAnimationFrame(frame)
      }
      window.clearTimeout(timer)
    }
  }, [mounted, open, prewarm])

  return mounted
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
        'sidebar-motion-island relative flex shrink-0 flex-col overflow-hidden bg-sidebar',
        open ? 'border-l border-border/80' : 'border-l-0',
      )}
      inert={!open ? true : undefined}
      style={{
        width: targetWidth,
        transition: `width ${SIDEBAR_WIDTH_DURATION_MS}ms ${SIDEBAR_REVEAL_EASE_CSS}`,
      }}
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
        'sidebar-motion-island relative flex shrink-0 flex-col overflow-hidden bg-sidebar',
        open ? 'border-l border-border/80' : 'border-l-0',
      )}
      inert={!open ? true : undefined}
      style={{
        width: targetWidth,
        transition: `width ${SIDEBAR_WIDTH_DURATION_MS}ms ${SIDEBAR_REVEAL_EASE_CSS}`,
      }}
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
        className="gpu-layer fixed inset-y-0 right-0 z-50 flex flex-col overflow-hidden border-l border-border/80 bg-sidebar shadow-2xl"
        style={{ width }}
      >
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

function SolanaWorkbenchSurface({ open, prewarm = false }: { open: boolean; prewarm?: boolean }) {
  const shouldMount = useActivatedSurface(open, prewarm)
  const workbenchMounted = useDeferredSolanaWorkbenchMount(open, prewarm)

  if (!shouldMount) {
    return null
  }

  return (
    <Activity mode={open ? 'visible' : 'hidden'} name="solana-workbench-sidebar">
      {workbenchMounted ? (
        <Suspense fallback={<SolanaWorkbenchOpeningShell open={open} />}>
          <LazySolanaWorkbenchSidebar open={open} prewarm={prewarm} />
        </Suspense>
      ) : (
        <SolanaWorkbenchOpeningShell open={open} />
      )}
    </Activity>
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
    importProject,
    createProject,
    removeProject,
    retry,
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
    renameAgentSession,
    activeUsageSummary,
    refreshUsageSummary,
    spawnPane,
    closePane,
    focusPane,
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
  const [archivedSessionsOpen, setArchivedSessionsOpen] = useState(false)
  const [projectAddOpen, setProjectAddOpen] = useState(false)
  const [gamesOpen, setGamesOpen] = useState(false)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [iosOpen, setIosOpen] = useState(false)
  const [androidOpen, setAndroidOpen] = useState(false)
  const [solanaOpen, setSolanaOpen] = useState(false)
  const [vcsOpen, setVcsOpen] = useState(false)
  const [workflowsOpen, setWorkflowsOpen] = useState(false)
  const [usageOpen, setUsageOpen] = useState(false)
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

  const toggleGames = useCallback(() => {
    setGamesOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('games')
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleBrowser = useCallback(() => {
    setBrowserOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('browser')
        setGamesOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleIos = useCallback(() => {
    setIosOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('ios')
        setGamesOpen(false)
        setBrowserOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleAndroid = useCallback(() => {
    setAndroidOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('android')
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleSolana = useCallback(() => {
    setSolanaOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('solana')
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setVcsOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleVcs = useCallback(() => {
    setVcsOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('vcs')
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setWorkflowsOpen(false)
      }
      return next
    })
  }, [])

  const toggleWorkflows = useCallback(() => {
    setWorkflowsOpen((current) => {
      const next = !current
      if (next) {
        preloadSurfaceChunk('workflows')
        setGamesOpen(false)
        setBrowserOpen(false)
        setIosOpen(false)
        setAndroidOpen(false)
        setSolanaOpen(false)
        setVcsOpen(false)
      }
      return next
    })
  }, [])
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const toggleSidebarCollapsed = useCallback(() => {
    setSidebarCollapsed((current) => !current)
  }, [])
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
  const shouldRestoreSidebarFromAutoCollapseRef = useRef(false)
  const shouldRestoreExplorerFromAutoCollapseRef = useRef(false)
  const shouldRestoreSidebarFromWorkflowsRef = useRef(false)
  const previousViewRef = useRef<View>(activeView)
  const previousBrowserOpenRef = useRef<boolean>(browserOpen)
  const previousWorkflowsOpenRef = useRef<boolean>(workflowsOpen)

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

  useEffect(() => {
    const previousView = previousViewRef.current
    const autoCollapseViews: View[] = ['execution', 'agent']
    const isAutoCollapseView = autoCollapseViews.includes(activeView)
    const wasAutoCollapseView = autoCollapseViews.includes(previousView)

    if (isAutoCollapseView && !wasAutoCollapseView) {
      shouldRestoreSidebarFromAutoCollapseRef.current = !sidebarCollapsed
      if (!sidebarCollapsed) {
        setSidebarCollapsed(true)
      }
    }

    if (!isAutoCollapseView && wasAutoCollapseView && shouldRestoreSidebarFromAutoCollapseRef.current) {
      shouldRestoreSidebarFromAutoCollapseRef.current = false
      if (sidebarCollapsed) {
        setSidebarCollapsed(false)
      }
    }

    if (!isAutoCollapseView && !wasAutoCollapseView) {
      shouldRestoreSidebarFromAutoCollapseRef.current = false
    }

    previousViewRef.current = activeView
  }, [activeView, sidebarCollapsed])

  useEffect(() => {
    const wasOpen = previousWorkflowsOpenRef.current

    if (workflowsOpen && !wasOpen) {
      shouldRestoreSidebarFromWorkflowsRef.current = !sidebarCollapsed
      if (!sidebarCollapsed) {
        setSidebarCollapsed(true)
      }
    } else if (
      !workflowsOpen &&
      wasOpen &&
      shouldRestoreSidebarFromWorkflowsRef.current
    ) {
      shouldRestoreSidebarFromWorkflowsRef.current = false
      if (sidebarCollapsed) {
        setSidebarCollapsed(false)
      }
    }

    previousWorkflowsOpenRef.current = workflowsOpen
  }, [workflowsOpen, sidebarCollapsed])

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
        return
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

  const handleArchiveAgentSession = useCallback((agentSessionId: string) => {
    setPendingAgentSessionId(agentSessionId)
    void archiveAgentSession(agentSessionId).finally(() => {
      setPendingAgentSessionId(null)
    })
  }, [archiveAgentSession])

  const handleRenameAgentSession = useCallback(async (agentSessionId: string, title: string) => {
    await renameAgentSession(agentSessionId, title)
  }, [renameAgentSession])

  const handleOpenSearchResult = useCallback((result: SessionTranscriptSearchResultSnippetDto) => {
    if (!activeProject) return
    setActiveView('agent')
    if (!result.archived && result.agentSessionId !== activeProject.selectedAgentSessionId) {
      handleSelectAgentSession(result.agentSessionId)
    }
  }, [activeProject, handleSelectAgentSession, setActiveView])

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
  const handleStartWorkflowRun = useCallback(() => startRuntimeRun(), [startRuntimeRun])
  const handleCreateWorkflow = useCallback(() => {
    if (!workflowsOpen) {
      toggleWorkflows()
    }
  }, [toggleWorkflows, workflowsOpen])

  const handleSelectProject = useCallback(
    (projectId: string) => {
      void selectProject(projectId)
    },
    [selectProject],
  )

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
    const getViewPaneClassName = (visible: boolean) =>
      cn(
        'view-pane absolute inset-0 flex min-h-0 min-w-0 transform-gpu overflow-hidden transition-[opacity,transform] motion-standard',
        visible
          ? 'z-10 translate-x-0 opacity-100'
          : 'pointer-events-none z-0 translate-x-2 opacity-0',
      )
    const sessionsPeekAvailable = activeView === 'agent' && explorerMode === 'collapsed'

    return (
      <>
        <AgentSessionsSidebar
          projectId={activeProject.id}
          sessions={activeProject.agentSessions}
          selectedSessionId={activeProject.selectedAgentSessionId}
          onSelectSession={handleSelectAgentSession}
          onCreateSession={handleCreateAgentSession}
          onArchiveSession={handleArchiveAgentSession}
          onOpenArchivedSessions={() => setArchivedSessionsOpen(true)}
          onRenameSession={handleRenameAgentSession}
          onSearchSessions={
            resolvedAdapter.searchSessionTranscripts
              ? async (query) => {
                  const response = await resolvedAdapter.searchSessionTranscripts?.({
                    projectId: activeProject.id,
                    query,
                    includeArchived: true,
                    limit: 12,
                  })
                  return response?.results ?? []
                }
              : undefined
          }
          onOpenSearchResult={handleOpenSearchResult}
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
        <ArchivedSessionsDialog
          open={archivedSessionsOpen}
          onOpenChange={setArchivedSessionsOpen}
          projectId={activeProject.id}
          projectLabel={activeProject.name}
          onLoad={async (projectId) => {
            const response = await resolvedAdapter.listAgentSessions({
              projectId,
              includeArchived: true,
            })
            return response.sessions
              .filter((session) => session.status === 'archived')
              .map(mapAgentSession)
          }}
          onRestore={async (agentSessionId) => {
            await restoreAgentSession(agentSessionId)
            await selectAgentSession(agentSessionId)
          }}
          onDelete={async (agentSessionId) => {
            await deleteAgentSession(agentSessionId)
          }}
        />
        <AgentCommandPalette
          enabled={activeView === 'agent' && Boolean(agentWorkspaceLayout)}
          panes={
            agentWorkspaceLayout
              ? agentWorkspaceLayout.paneSlots.map((slot, index) => {
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
              : []
          }
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
              className={getViewPaneClassName(activeView === 'phases')}
              name="workflow-pane"
              prewarm={startupSurfacePrewarm.shouldMount}
            >
              <PhaseView
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
                onCreateWorkflow={handleCreateWorkflow}
              />
            </LazyActivityPane>
          ) : null}

          {agentView ? (
            <LazyActivityPane
              active={activeView === 'agent'}
              className={getViewPaneClassName(activeView === 'agent')}
              name="agent-pane"
              prewarm={startupSurfacePrewarm.shouldMount}
            >
              <AgentWorkspace
                layout={agentWorkspaceLayout}
                panes={agentWorkspacePanes}
                highChurnStore={highChurnStore}
                desktopAdapter={resolvedAdapter}
                accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
                accountLogin={githubSession?.user.login ?? null}
                customAgentDefinitions={customAgentDefinitions}
                onOpenAgentManagement={handleOpenAgentManagement}
                onCreateSession={handleCreateAgentSession}
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
      </>
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
  const startupSurfacePrewarm = useStartupSurfacePrewarm(
    !showOnboarding && !isLoading && !isProjectLoading,
  )
  useIdleSurfacePreloads(
    !showOnboarding &&
      !isLoading &&
      !isProjectLoading &&
      startupSurfacePrewarm.ready,
  )
  const showStartupSurfacePrewarm = !startupSurfacePrewarm.ready
  const showAppBootLoading = !showOnboarding && (
    isLoading ||
    isProjectLoading ||
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
        projectName={activeProject?.name}
        onOpenSettings={() => openSettings('providers')}
        onOpenAccount={() => openSettings('account')}
        onAccountLogin={() => {
          void loginWithGithub()
        }}
        accountAuthenticating={githubAuthStatus === 'authenticating'}
        accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
        accountLogin={githubSession?.user.login ?? null}
        onToggleGames={toggleGames}
        gamesOpen={gamesOpen}
        onToggleBrowser={toggleBrowser}
        browserOpen={browserOpen}
        onToggleIos={toggleIos}
        iosOpen={iosOpen}
        onToggleAndroid={toggleAndroid}
        androidOpen={androidOpen}
        onToggleSolana={toggleSolana}
        solanaOpen={solanaOpen}
        onToggleVcs={toggleVcs}
        vcsOpen={vcsOpen}
        onToggleWorkflows={toggleWorkflows}
        workflowsOpen={workflowsOpen}
        vcsChangeCount={repositoryStatus?.statusCount ?? 0}
        vcsAdditions={repositoryStatus?.additions ?? 0}
        vcsDeletions={repositoryStatus?.deletions ?? 0}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={toggleSidebarCollapsed}
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
          projectName={activeProject?.name}
          onOpenSettings={() => openSettings('providers')}
          onOpenAccount={() => openSettings('account')}
          onAccountLogin={() => {
            void loginWithGithub()
          }}
          accountAuthenticating={githubAuthStatus === 'authenticating'}
          accountAvatarUrl={githubSession?.user.avatarUrl ?? null}
          accountLogin={githubSession?.user.login ?? null}
          onToggleGames={toggleGames}
          gamesOpen={gamesOpen}
          onToggleBrowser={toggleBrowser}
          browserOpen={browserOpen}
          onToggleIos={toggleIos}
          iosOpen={iosOpen}
          onToggleAndroid={toggleAndroid}
          androidOpen={androidOpen}
          onToggleSolana={toggleSolana}
          solanaOpen={solanaOpen}
          onToggleVcs={toggleVcs}
          vcsOpen={vcsOpen}
          onToggleWorkflows={toggleWorkflows}
          workflowsOpen={workflowsOpen}
          vcsChangeCount={repositoryStatus?.statusCount ?? 0}
          vcsAdditions={repositoryStatus?.additions ?? 0}
          vcsDeletions={repositoryStatus?.deletions ?? 0}
          sidebarCollapsed={sidebarCollapsed}
          onToggleSidebar={toggleSidebarCollapsed}
          platformOverride={platformOverride}
          footer={statusFooter}
        >
          <ProjectRail
            activeProjectId={activeProjectId}
            collapsed={sidebarCollapsed}
            errorMessage={errorMessage}
            isImporting={isImporting}
            isLoading={isLoading || isProjectLoading}
            onImportProject={() => setProjectAddOpen(true)}
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
          {renderBody()}
          <LazyActivitySurface
            name="games-sidebar"
            open={gamesOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={<InlineSidebarLoadingShell label="Games" open={gamesOpen} width={360} />}
            >
              <LazyGamesSidebar accountLogin={githubSession?.user.login ?? null} open={gamesOpen} />
            </Suspense>
          </LazyActivitySurface>
          <LazyActivitySurface
            name="browser-sidebar"
            open={browserOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={<InlineSidebarLoadingShell label="Browser" open={browserOpen} width={640} />}
            >
              <LazyBrowserSidebar open={browserOpen} />
            </Suspense>
          </LazyActivitySurface>
          <LazyActivitySurface
            name="usage-sidebar"
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
          </LazyActivitySurface>
          <LazyActivitySurface
            name="ios-emulator-sidebar"
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
          </LazyActivitySurface>
          <LazyActivitySurface
            name="android-emulator-sidebar"
            open={androidOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          >
            <Suspense
              fallback={
                <InlineSidebarLoadingShell
                  label="Android Emulator"
                  open={androidOpen}
                  width={640}
                />
              }
            >
              <LazyAndroidEmulatorSidebar open={androidOpen} />
            </Suspense>
          </LazyActivitySurface>
          <SolanaWorkbenchSurface
            open={solanaOpen}
            prewarm={startupSurfacePrewarm.shouldMount}
          />
          <LazyActivitySurface
            name="workflows-sidebar"
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
              <LazyWorkflowsSidebar open={workflowsOpen} />
            </Suspense>
          </LazyActivitySurface>
          <LazyActivitySurface
            name="vcs-sidebar"
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
          </LazyActivitySurface>
          <LazyActivitySurface
            name="settings-dialog"
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
                soulAdapter={resolvedAdapter}
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
          </LazyActivitySurface>
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

export default function App() {
  return <XeroApp />
}
