"use client"

import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import {
  CircleCheckBig,
  CircleSlash,
  Coins,
  FileJson,
  Loader2,
  Network,
  Play,
  RadioTower,
  RefreshCw,
  Rocket,
  Search,
  Server,
  ShieldCheck,
  Square,
  Users,
  Wallet,
  Zap,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { createFrameCoalescer } from "@/lib/frame-governance"
import {
  useDeferredSidebarActivation,
  useSidebarWidthMotion,
} from "@/lib/sidebar-motion"
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import { Badge } from "@/components/ui/badge"
import { SolanaLogoIcon } from "./brand-icons"
import { SolanaMissingToolchain } from "./solana-missing-toolchain"
import { PanelHeader } from "./solana-panel-shell"
import {
  useSolanaWorkbench,
  type ClusterKind,
  type CodamaTarget,
  type DeployResult,
  type DeployAuthority,
  type FundingDelta,
  type LogEntry,
  type PersonaRole,
  type SimulateRequest,
} from "@/src/features/solana/use-solana-workbench"

const MIN_WIDTH = 360
const DEFAULT_WIDTH = 440
const MAX_WIDTH = 900
const STORAGE_KEY = "xero.solana.workbench.width"

const loadSolanaAuditPanel = () => import("./solana-audit-panel")
const loadSolanaDeployPanel = () => import("./solana-deploy-panel")
const loadSolanaIdlPanel = () => import("./solana-idl-panel")
const loadSolanaIndexerPanel = () => import("./solana-indexer-panel")
const loadSolanaLogFeed = () => import("./solana-log-feed")
const loadSolanaPersonaPanel = () => import("./solana-persona-panel")
const loadSolanaSafetyPanel = () => import("./solana-safety-panel")
const loadSolanaScenarioPanel = () => import("./solana-scenario-panel")
const loadSolanaTokenPanel = () => import("./solana-token-panel")
const loadSolanaTxInspector = () => import("./solana-tx-inspector")
const loadSolanaWalletPanel = () => import("./solana-wallet-panel")

export async function preloadSolanaWorkbenchPanels(): Promise<void> {
  await Promise.all([
    loadSolanaAuditPanel(),
    loadSolanaDeployPanel(),
    loadSolanaIdlPanel(),
    loadSolanaIndexerPanel(),
    loadSolanaLogFeed(),
    loadSolanaPersonaPanel(),
    loadSolanaSafetyPanel(),
    loadSolanaScenarioPanel(),
    loadSolanaTokenPanel(),
    loadSolanaTxInspector(),
    loadSolanaWalletPanel(),
  ])
}

const LazySolanaAuditPanel = lazy(() =>
  loadSolanaAuditPanel().then((module) => ({ default: module.SolanaAuditPanel })),
)
const LazySolanaDeployPanel = lazy(() =>
  loadSolanaDeployPanel().then((module) => ({ default: module.SolanaDeployPanel })),
)
const LazySolanaIdlPanel = lazy(() =>
  loadSolanaIdlPanel().then((module) => ({ default: module.SolanaIdlPanel })),
)
const LazySolanaIndexerPanel = lazy(() =>
  loadSolanaIndexerPanel().then((module) => ({ default: module.SolanaIndexerPanel })),
)
const LazySolanaLogFeed = lazy(() =>
  loadSolanaLogFeed().then((module) => ({ default: module.SolanaLogFeed })),
)
const LazySolanaPersonaPanel = lazy(() =>
  loadSolanaPersonaPanel().then((module) => ({ default: module.SolanaPersonaPanel })),
)
const LazySolanaSafetyPanel = lazy(() =>
  loadSolanaSafetyPanel().then((module) => ({ default: module.SolanaSafetyPanel })),
)
const LazySolanaScenarioPanel = lazy(() =>
  loadSolanaScenarioPanel().then((module) => ({ default: module.SolanaScenarioPanel })),
)
const LazySolanaTokenPanel = lazy(() =>
  loadSolanaTokenPanel().then((module) => ({ default: module.SolanaTokenPanel })),
)
const LazySolanaTxInspector = lazy(() =>
  loadSolanaTxInspector().then((module) => ({ default: module.SolanaTxInspector })),
)
const LazySolanaWalletPanel = lazy(() =>
  loadSolanaWalletPanel().then((module) => ({ default: module.SolanaWalletPanel })),
)

type TabId =
  | "cluster"
  | "personas"
  | "scenarios"
  | "tx"
  | "logs"
  | "indexer"
  | "idl"
  | "deploy"
  | "audit"
  | "token"
  | "wallet"
  | "safety"
  | "rpc"

interface SolanaWorkbenchSidebarProps {
  open: boolean
  prewarm?: boolean
}

function readPersistedWidth(): number | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage?.getItem?.(STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return parsed
  } catch {
    return null
  }
}

function writePersistedWidth(value: number) {
  if (typeof window === "undefined") return
  try {
    window.localStorage?.setItem?.(STORAGE_KEY, String(Math.round(value)))
  } catch {
    /* storage unavailable — default next session */
  }
}

function SolanaPanelFallback() {
  return (
    <div
      aria-label="Loading workbench panel"
      aria-live="polite"
      className="flex flex-col gap-3"
      role="status"
    >
      <div className="h-7 w-36 rounded-md bg-secondary/50" />
      <div className="h-24 rounded-md border border-border/60 bg-background/35" />
      <div className="space-y-2">
        <div className="h-3 w-3/4 rounded bg-secondary/45" />
        <div className="h-3 w-1/2 rounded bg-secondary/35" />
      </div>
    </div>
  )
}

function SolanaPanelSlot({
  active,
  children,
  padded = true,
  tabId,
  warmLayout = false,
}: {
  active: boolean
  children: React.ReactNode
  padded?: boolean
  tabId: TabId
  warmLayout?: boolean
}) {
  return (
    <div
      aria-hidden={!active}
      aria-labelledby={`tab-${tabId}`}
      className={cn(
        "min-w-0",
        active
          ? "relative"
          : warmLayout
            ? "pointer-events-none invisible absolute inset-x-0 top-0 overflow-hidden"
            : "hidden",
        padded && "px-3 py-3",
      )}
      hidden={!active && !warmLayout}
      inert={!active ? true : undefined}
      role="tabpanel"
    >
      {children}
    </div>
  )
}

export function SolanaWorkbenchSidebar({
  open,
  prewarm = false,
}: SolanaWorkbenchSidebarProps) {
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const {
    activateAfterAnimation: activateWorkbenchAfterAnimation,
    active: workbenchActive,
  } = useDeferredSidebarActivation(open)
  const targetWidth = open ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, {
    durationMs: prewarm ? 80 : undefined,
    isResizing,
  })
  const widthRef = useRef(width)
  widthRef.current = width

  const workbench = useSolanaWorkbench({ active: workbenchActive })
  const [selectedKind, setSelectedKind] = useState<ClusterKind>("localnet")
  const [activeTab, setActiveTab] = useState<TabId>("cluster")

  useEffect(() => {
    if (!workbench.clusters.length) return
    setSelectedKind((current) => {
      if (workbench.clusters.some((c) => c.kind === current)) return current
      const firstStartable = workbench.clusters.find((c) => c.startable)
      return firstStartable?.kind ?? current
    })
  }, [workbench.clusters])

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      const startX = event.clientX
      const startWidth = widthRef.current
      let latestWidth = startWidth
      const widthUpdates = createFrameCoalescer<number>({
        onFlush: setWidth,
      })
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        latestWidth = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
        widthUpdates.schedule(latestWidth)
      }
      const handleUp = () => {
        widthUpdates.flush()
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
        setIsResizing(false)
        writePersistedWidth(latestWidth)
      }

      window.addEventListener("pointermove", handleMove)
      window.addEventListener("pointerup", handleUp)
      window.addEventListener("pointercancel", handleUp)
    },
    [],
  )

  const handleStart = useCallback(() => {
    void workbench.start(selectedKind)
  }, [workbench, selectedKind])

  const handleStop = useCallback(() => {
    void workbench.stop()
  }, [workbench])

  const selectedCluster = useMemo(
    () => workbench.clusters.find((c) => c.kind === selectedKind) ?? null,
    [workbench.clusters, selectedKind],
  )

  const clusterRunning = workbench.status.running && workbench.status.kind === selectedKind

  const refreshPersonasForCluster = useCallback(() => {
    void workbench.refreshPersonas(selectedKind)
  }, [workbench, selectedKind])

  const handleCreatePersona = useCallback(
    async (name: string, role: PersonaRole, note: string | null) => {
      const response = await workbench.createPersona({
        name,
        cluster: selectedKind,
        role,
        note,
      })
      return response?.receipt ?? null
    },
    [workbench, selectedKind],
  )

  const handleDeletePersona = useCallback(
    async (name: string) => workbench.deletePersona(selectedKind, name),
    [workbench, selectedKind],
  )

  const handleFundPersona = useCallback(
    async (name: string, delta: FundingDelta) =>
      workbench.fundPersona(selectedKind, name, delta),
    [workbench, selectedKind],
  )

  const handleSimulate = useCallback(
    async (request: SimulateRequest) => workbench.simulateTx(request),
    [workbench],
  )

  const handleExplain = useCallback(
    async (signature: string) =>
      workbench.explainTx({ cluster: selectedKind, signature }),
    [workbench, selectedKind],
  )

  const handleEstimateFee = useCallback(
    async (programIds: string[]) =>
      workbench.estimatePriorityFee(selectedKind, programIds),
    [workbench, selectedKind],
  )

  const handleIdlFetch = useCallback(
    async (programId: string) => workbench.fetchIdl(programId, selectedKind),
    [workbench, selectedKind],
  )

  const handleIdlDrift = useCallback(
    async (programId: string, localPath: string) =>
      workbench.driftIdl(programId, selectedKind, localPath),
    [workbench, selectedKind],
  )

  const handleCodama = useCallback(
    async (idlPath: string, targets: CodamaTarget[], outputDir: string) =>
      workbench.generateCodama(idlPath, targets, outputDir),
    [workbench],
  )

  const handlePublishIdl = useCallback(
    async (args: {
      programId: string
      idlPath: string
      authorityPersona: string
      mode: "init" | "upgrade"
    }) =>
      workbench.publishIdl({
        programId: args.programId,
        cluster: selectedKind,
        idlPath: args.idlPath,
        authorityPersona: args.authorityPersona,
        mode: args.mode,
      }),
    [workbench, selectedKind],
  )

  const handleBuildProgram = useCallback(
    async (args: { manifestPath: string; profile: "dev" | "release"; program: string | null }) =>
      workbench.buildProgram({
        manifestPath: args.manifestPath,
        profile: args.profile,
        program: args.program,
      }),
    [workbench],
  )

  const handleUpgradeCheck = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      localSoPath: string
      expectedAuthority: string
      localIdlPath: string | null
    }) => workbench.upgradeCheck(args),
    [workbench],
  )

  const handleDeploy = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      soPath: string
      authority: DeployAuthority
      idlPath: string | null
      isFirstDeploy: boolean
    }) => workbench.deployProgram(args),
    [workbench],
  )

  const handleSubmitVerified = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      manifestPath: string
      githubUrl: string
      commitHash: string | null
      libraryName: string | null
    }) => workbench.submitVerifiedBuild(args),
    [workbench],
  )

  const handleRollback = useCallback(
    async (args: {
      programId: string
      cluster: ClusterKind
      previousSha256: string
      authority: DeployAuthority
    }) => workbench.rollbackProgram(args),
    [workbench],
  )

  const handleSubscribeLogs = useCallback(
    (filter: {
      cluster: ClusterKind
      programIds: string[]
      includeDecoded: boolean
    }) => workbench.subscribeLogs(filter),
    [workbench],
  )

  const handleFetchRecentLogs = useCallback(
    (args: {
      cluster: ClusterKind
      programIds?: string[]
      lastN?: number
      rpcUrl?: string | null
      cachedOnly?: boolean
    }) => workbench.fetchRecentLogs(args),
    [workbench],
  )

  const handleScaffoldIndexer = useCallback(
    (args: {
      kind: "carbon" | "log_parser" | "helius_webhook"
      idlPath: string
      outputDir: string
      projectSlug?: string | null
      overwrite?: boolean
      rpcUrl?: string | null
    }) => workbench.scaffoldIndexer(args),
    [workbench],
  )

  const handleRunIndexer = useCallback(
    (args: {
      cluster: ClusterKind
      programIds: string[]
      lastN?: number
      rpcUrl?: string | null
    }) => workbench.runIndexer(args),
    [workbench],
  )

  const notifications = useMemo(() => {
    const logErrorCount = countErroredLogs(workbench.logEntries)
    const indexerErrorCount = countErroredLogs(
      workbench.lastIndexerRun?.entries ?? [],
    )
    const idlIssueCount =
      (workbench.lastDriftReport?.breakingCount ?? 0) +
      (workbench.lastDriftReport?.riskyCount ?? 0) +
      (workbench.lastIdlEvent?.phase === "invalid" ? 1 : 0)
    const deployIssueCount =
      (workbench.lastBuildReport && !workbench.lastBuildReport.success ? 1 : 0) +
      (workbench.lastUpgradeSafety?.verdict &&
      workbench.lastUpgradeSafety.verdict !== "ok"
        ? 1
        : 0) +
      (deployResultHasIssue(workbench.lastDeployResult) ? 1 : 0) +
      (workbench.lastVerifiedBuild && !workbench.lastVerifiedBuild.success
        ? 1
        : 0) +
      (workbench.lastRollback && deployResultHasIssue(workbench.lastRollback.deploy)
        ? 1
        : 0) +
      (workbench.lastDeployProgress?.phase === "failed" ? 1 : 0)
    const txIssueCount =
      (workbench.lastSimulation && !workbench.lastSimulation.success ? 1 : 0) +
      (workbench.lastSend &&
      (workbench.lastSend.err != null || !workbench.lastSend.explanation.ok)
        ? 1
        : 0) +
      (workbench.lastExplanation &&
      (workbench.lastExplanation.err != null ||
        !workbench.lastExplanation.explanation.ok)
        ? 1
        : 0)
    const tokenIssueCount =
      (workbench.lastTokenCreate && !workbench.lastTokenCreate.success ? 1 : 0) +
      (workbench.lastTokenCreate?.incompatibilities.length ?? 0) +
      (workbench.lastMetaplexMint && !workbench.lastMetaplexMint.success ? 1 : 0)
    const safetyIssueCount =
      (workbench.lastSecretScan?.findings.length ?? 0) +
      (workbench.lastScopeCheck?.warnings.length ?? 0) +
      (workbench.lastClusterDrift?.entries.filter((e) => e.status === "drift")
        .length ?? 0) +
      (workbench.lastCostSnapshot?.providers.filter(
        (provider) => provider.health === "degraded" || Boolean(provider.warning),
      ).length ?? 0)
    const rpcUnhealthyCount = workbench.rpcHealth.filter((h) => !h.healthy).length

    return {
      cluster:
        activityNotification(
          workbench.isStarting,
          "Validator starting",
        ) ??
        activityNotification(
          workbench.isStopping,
          "Validator stopping",
        ) ??
        issueNotification(
          workbench.error ? 1 : 0,
          "validator error",
          "validator errors",
          "danger",
        ),
      personas: activityNotification(
        workbench.personaBusy,
        "Persona action in progress",
      ),
      scenarios:
        activityNotification(
          workbench.scenarioBusy,
          "Scenario run in progress",
        ) ??
        issueNotification(
          workbench.lastScenarioRun?.status === "failed" ? 1 : 0,
          "failed scenario run",
          "failed scenario runs",
          "danger",
        ) ??
        issueNotification(
          workbench.lastScenarioRun?.status === "pendingPipeline" ? 1 : 0,
          "scenario waiting for pipeline setup",
          "scenarios waiting for pipeline setup",
        ),
      tx:
        activityNotification(workbench.txBusy, "Transaction action in progress") ??
        issueNotification(txIssueCount, "transaction issue", "transaction issues", "danger"),
      logs:
        activityNotification(workbench.logBusy, "Log request in progress") ??
        issueNotification(logErrorCount, "errored log entry", "errored log entries", "danger"),
      indexer:
        activityNotification(workbench.indexerBusy, "Indexer action in progress") ??
        issueNotification(
          indexerErrorCount,
          "indexer transaction issue",
          "indexer transaction issues",
          "danger",
        ),
      idl:
        activityNotification(workbench.idlBusy, "IDL action in progress") ??
        issueNotification(idlIssueCount, "IDL issue", "IDL issues", "danger"),
      deploy:
        activityNotification(workbench.programBusy, "Deploy action in progress") ??
        issueNotification(deployIssueCount, "deploy issue", "deploy issues", "danger"),
      audit:
        activityNotification(workbench.auditBusy, "Audit run in progress") ??
        issueNotification(
          workbench.auditFindings.filter((finding) => finding.severity !== "informational")
            .length,
          "audit finding",
          "audit findings",
          "danger",
        ),
      token:
        activityNotification(workbench.tokenBusy, "Token action in progress") ??
        issueNotification(tokenIssueCount, "token issue", "token issues", "danger"),
      wallet:
        activityNotification(workbench.walletBusy, "Wallet scaffold in progress") ??
        issueNotification(
          workbench.lastWalletScaffold?.apiKeyEnv ? 1 : 0,
          "wallet scaffold needs an API key",
          "wallet scaffolds need API keys",
        ),
      safety:
        activityNotification(workbench.safetyBusy, "Safety check in progress") ??
        issueNotification(safetyIssueCount, "safety issue", "safety issues", "danger"),
      rpc: issueNotification(
        rpcUnhealthyCount,
        "unhealthy RPC endpoint",
        "unhealthy RPC endpoints",
        "danger",
      ),
    } satisfies Record<TabId, TabNotification | undefined>
  }, [workbench])

  const tabs: TabDescriptor[] = [
    {
      id: "cluster",
      icon: Network,
      label: "Cluster",
      notification: notifications.cluster,
    },
    {
      id: "personas",
      icon: Users,
      label: "Personas",
      notification: notifications.personas,
    },
    {
      id: "scenarios",
      icon: Zap,
      label: "Scenarios",
      notification: notifications.scenarios,
    },
    {
      id: "tx",
      icon: Search,
      label: "Tx",
      notification: notifications.tx,
    },
    {
      id: "logs",
      icon: RadioTower,
      label: "Logs",
      notification: notifications.logs,
    },
    {
      id: "indexer",
      icon: FileJson,
      label: "Indexer",
      notification: notifications.indexer,
    },
    {
      id: "idl",
      icon: FileJson,
      label: "IDL",
      notification: notifications.idl,
    },
    {
      id: "deploy",
      icon: Rocket,
      label: "Deploy",
      notification: notifications.deploy,
    },
    {
      id: "audit",
      icon: ShieldCheck,
      label: "Audit",
      notification: notifications.audit,
    },
    {
      id: "token",
      icon: Coins,
      label: "Token",
      notification: notifications.token,
    },
    {
      id: "wallet",
      icon: Wallet,
      label: "Wallet",
      notification: notifications.wallet,
    },
    {
      id: "safety",
      icon: ShieldCheck,
      label: "Safety",
      notification: notifications.safety,
    },
    {
      id: "rpc",
      icon: Server,
      label: "RPC",
      notification: notifications.rpc,
    },
  ]
  const activeTabLabel = tabs.find((tab) => tab.id === activeTab)?.label ?? "Cluster"

  return (
    <aside
      aria-hidden={!open}
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      onTransitionEnd={(event) => {
        if (event.target === event.currentTarget && event.propertyName === "width") {
          activateWorkbenchAfterAnimation()
        }
      }}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize Solana workbench sidebar"
        aria-orientation="vertical"
        aria-valuemax={MAX_WIDTH}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-20 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div
        className="flex h-full min-w-0 shrink-0 flex-col"
        style={{ width }}
      >
      <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/70 pl-3 pr-2">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <SolanaLogoIcon className="h-3.5 w-3.5 shrink-0 text-muted-foreground/90" mono />
          <Breadcrumb aria-label="Solana Workbench breadcrumb" className="min-w-0 overflow-hidden">
            <BreadcrumbList className="flex-nowrap gap-1.5 text-[11px] font-semibold sm:gap-1.5">
              <BreadcrumbItem className="min-w-0">
                <span className="truncate text-muted-foreground">
                  Solana Workbench
                </span>
              </BreadcrumbItem>
              <BreadcrumbSeparator className="text-muted-foreground/50 [&>svg]:size-3" />
              <BreadcrumbItem className="min-w-0">
                <BreadcrumbPage className="truncate text-[11px] font-semibold text-foreground">
                  {activeTabLabel}
                </BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
        </div>
        <button
          aria-label="Refresh toolchain"
          className="shrink-0 rounded-md p-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground disabled:opacity-60"
          disabled={workbench.toolchainLoading}
          onClick={() => void workbench.refreshToolchain()}
          type="button"
        >
          <RefreshCw
            className={cn(
              "h-3.5 w-3.5",
              workbench.toolchainLoading && "animate-spin",
            )}
          />
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        <SolanaMissingToolchain
          installEvent={workbench.toolchainInstallEvent}
          installing={workbench.toolchainInstalling}
          loading={workbench.toolchainLoading}
          onInstall={() => void workbench.installToolchain()}
          onRefresh={() => void workbench.refreshToolchain()}
          status={workbench.toolchain}
        />

      <div className="flex min-h-0 flex-1">
        <div
          role="tablist"
          aria-label="Workbench tools"
          aria-orientation="vertical"
          className="flex w-10 shrink-0 flex-col items-stretch overflow-x-hidden overflow-y-auto border-r border-border/70 bg-sidebar scrollbar-thin"
        >
          {tabs.map((tab) => (
            <TabButton
              key={tab.id}
              tab={tab}
              active={activeTab === tab.id}
              onClick={() => setActiveTab(tab.id)}
            />
          ))}
        </div>

        <div className="relative flex min-w-0 flex-1 flex-col overflow-y-auto scrollbar-thin">
          <SolanaPanelSlot
            active={activeTab === "cluster"}
            padded={false}
            tabId="cluster"
          >
            <div className="px-3 pt-3">
              <PanelHeader
                icon={Network}
                title="Cluster"
                description="Pick a cluster and control the local validator."
                busy={workbench.isStarting || workbench.isStopping}
              />
            </div>
            <section className="border-b border-border/70 px-3 py-3">
              <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                Cluster
              </div>
              <div className="flex flex-wrap gap-1.5">
                {workbench.clusters.map((cluster) => (
                  <button
                    key={cluster.kind}
                    type="button"
                    disabled={!cluster.startable && !workbench.status.running}
                    onClick={() => setSelectedKind(cluster.kind)}
                    className={cn(
                      "rounded-md border px-2 py-1 text-[11px] transition-colors",
                      selectedKind === cluster.kind
                        ? "border-primary/50 bg-primary/10 text-primary"
                        : "border-border/70 bg-background/40 text-foreground/80 hover:border-primary/40 hover:text-foreground",
                      !cluster.startable && "opacity-60",
                    )}
                  >
                    {cluster.label}
                  </button>
                ))}
              </div>
              {selectedCluster ? (
                <p className="mt-2 text-[11px] text-muted-foreground">
                  {selectedCluster.startable
                    ? "Local cluster — Xero can spin it up on your machine."
                    : "Remote cluster — read-only from here."}
                </p>
              ) : null}
            </section>

            <section className="px-3 py-3">
              <div className="mb-2 flex items-center justify-between">
                <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                  Validator
                </div>
                <StatusDot
                  running={workbench.status.running}
                  starting={workbench.isStarting}
                />
              </div>
              <div className="space-y-1.5 text-[11px] text-foreground/85">
                <KV
                  label="State"
                  value={
                    workbench.isStarting
                      ? "Starting…"
                      : workbench.isStopping
                        ? "Stopping…"
                        : workbench.status.running
                          ? "Running"
                          : "Stopped"
                  }
                />
                <KV label="RPC" value={workbench.status.rpcUrl ?? "—"} mono />
                <KV label="WS" value={workbench.status.wsUrl ?? "—"} mono />
                {workbench.status.uptimeS != null ? (
                  <KV
                    label="Uptime"
                    value={`${workbench.status.uptimeS}s`}
                    mono
                  />
                ) : null}
                {workbench.lastEvent?.message ? (
                  <div className="text-[11px] italic text-muted-foreground">
                    {workbench.lastEvent.message}
                  </div>
                ) : null}
              </div>
              <div className="mt-3 flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleStart}
                  disabled={
                    !selectedCluster?.startable ||
                    workbench.isStarting ||
                    workbench.isStopping ||
                    workbench.status.running
                  }
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors",
                    "hover:bg-primary/25 disabled:opacity-50",
                  )}
                >
                  {workbench.isStarting ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <Play className="h-3 w-3 fill-current" />
                  )}
                  Start
                </button>
                <button
                  type="button"
                  onClick={handleStop}
                  disabled={!workbench.status.running || workbench.isStopping}
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-md border border-border/70 bg-background/40 px-2.5 py-1 text-[11px] text-foreground/85 transition-colors",
                    "hover:border-destructive/50 hover:text-destructive disabled:opacity-50",
                  )}
                >
                  {workbench.isStopping ? (
                    <Loader2 className="h-3 w-3 animate-spin" />
                  ) : (
                    <Square className="h-3 w-3" />
                  )}
                  Stop
                </button>
              </div>
              {workbench.error ? (
                <p className="mt-2 text-[11px] text-destructive">
                  {workbench.error}
                </p>
              ) : null}
            </section>
          </SolanaPanelSlot>

          <Suspense fallback={activeTab === "personas" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "personas"} tabId="personas">
              <LazySolanaPersonaPanel
                busy={workbench.personaBusy}
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                onCreate={handleCreatePersona}
                onDelete={handleDeletePersona}
                onFund={handleFundPersona}
                onRefresh={refreshPersonasForCluster}
                personas={workbench.personas}
                roles={workbench.personaRoles}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "scenarios" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "scenarios"} tabId="scenarios">
              <LazySolanaScenarioPanel
                busy={workbench.scenarioBusy}
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                lastRun={workbench.lastScenarioRun}
                onRunScenario={workbench.runScenario}
                personas={workbench.personas}
                scenarios={workbench.scenarios}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "tx" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "tx"} tabId="tx">
              <LazySolanaTxInspector
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                txBusy={workbench.txBusy}
                lastSimulation={workbench.lastSimulation}
                lastExplanation={workbench.lastExplanation}
                onSimulate={handleSimulate}
                onExplain={handleExplain}
                onEstimateFee={handleEstimateFee}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "logs" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "logs"} tabId="logs">
              <LazySolanaLogFeed
                cluster={selectedKind}
                busy={workbench.logBusy}
                entries={workbench.logEntries}
                decodedEvents={workbench.decodedLogEvents}
                activeSubscriptions={workbench.activeLogSubscriptions}
                lastFetch={workbench.lastLogFetch}
                onSubscribe={handleSubscribeLogs}
                onUnsubscribe={workbench.unsubscribeLogs}
                onFetchRecent={handleFetchRecentLogs}
                onRefreshSubscriptions={workbench.refreshActiveLogSubscriptions}
                onClear={workbench.clearLogFeed}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "indexer" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "indexer"} tabId="indexer">
              <LazySolanaIndexerPanel
                cluster={selectedKind}
                busy={workbench.indexerBusy}
                lastScaffold={workbench.lastIndexerScaffold}
                lastRun={workbench.lastIndexerRun}
                onScaffold={handleScaffoldIndexer}
                onRun={handleRunIndexer}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "idl" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "idl"} tabId="idl">
              <LazySolanaIdlPanel
                cluster={selectedKind}
                idls={workbench.idls}
                idlBusy={workbench.idlBusy}
                lastIdlEvent={workbench.lastIdlEvent}
                lastDriftReport={workbench.lastDriftReport}
                lastCodamaReport={workbench.lastCodamaReport}
                lastPublishReport={workbench.lastPublishReport}
                lastDeployProgress={workbench.lastDeployProgress}
                activeWatches={workbench.activeIdlWatches}
                personaNames={workbench.personas.map((p) => p.name)}
                onLoad={workbench.loadIdl}
                onFetch={handleIdlFetch}
                onDrift={handleIdlDrift}
                onCodama={handleCodama}
                onPublish={handlePublishIdl}
                onStartWatch={workbench.startIdlWatch}
                onStopWatch={workbench.stopIdlWatch}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "deploy" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "deploy"} tabId="deploy">
              <LazySolanaDeployPanel
                busy={workbench.programBusy}
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                personaNames={workbench.personas.map((p) => p.name)}
                lastBuildReport={workbench.lastBuildReport}
                lastUpgradeSafety={workbench.lastUpgradeSafety}
                lastDeployResult={workbench.lastDeployResult}
                lastSquadsProposal={workbench.lastSquadsProposal}
                lastVerifiedBuild={workbench.lastVerifiedBuild}
                lastRollback={workbench.lastRollback}
                onBuild={handleBuildProgram}
                onUpgradeCheck={handleUpgradeCheck}
                onDeploy={handleDeploy}
                onSubmitVerified={handleSubmitVerified}
                onRollback={handleRollback}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "audit" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "audit"} tabId="audit">
              <LazySolanaAuditPanel
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                busy={workbench.auditBusy}
                findings={workbench.auditFindings}
                events={workbench.auditEvents}
                lastStatic={workbench.lastStaticReport}
                lastExternal={workbench.lastExternalReport}
                lastFuzz={workbench.lastFuzzReport}
                lastCoverage={workbench.lastCoverageReport}
                lastReplay={workbench.lastReplayReport}
                replayCatalog={workbench.replayCatalog}
                onClearFeed={workbench.clearAuditFeed}
                onRunStatic={workbench.runStaticAudit}
                onRunExternal={workbench.runExternalAudit}
                onRunFuzz={workbench.runFuzzAudit}
                onScaffoldFuzz={workbench.scaffoldFuzzHarness}
                onRunCoverage={workbench.runCoverageAudit}
                onRunReplay={workbench.runReplay}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "token" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "token"} tabId="token">
              <LazySolanaTokenPanel
                cluster={selectedKind}
                clusterRunning={clusterRunning}
                busy={workbench.tokenBusy}
                personaNames={workbench.personas.map((p) => p.name)}
                matrix={workbench.extensionMatrix}
                lastTokenCreate={workbench.lastTokenCreate}
                lastMetaplexMint={workbench.lastMetaplexMint}
                onCreateToken={workbench.createToken}
                onMintMetaplex={workbench.mintMetaplex}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "wallet" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "wallet"} tabId="wallet">
              <LazySolanaWalletPanel
                cluster={selectedKind}
                busy={workbench.walletBusy}
                descriptors={workbench.walletDescriptors}
                lastScaffold={workbench.lastWalletScaffold}
                onGenerate={workbench.generateWalletScaffold}
              />
            </SolanaPanelSlot>
          </Suspense>

          <Suspense fallback={activeTab === "safety" ? <SolanaPanelFallback /> : null}>
            <SolanaPanelSlot active={activeTab === "safety"} tabId="safety">
              <LazySolanaSafetyPanel
                busy={workbench.safetyBusy}
                lastSecretScan={workbench.lastSecretScan}
                lastScopeCheck={workbench.lastScopeCheck}
                lastDrift={workbench.lastClusterDrift}
                lastCost={workbench.lastCostSnapshot}
                trackedPrograms={workbench.trackedPrograms}
                onScanSecrets={workbench.scanSecrets}
                onRunScopeCheck={workbench.runScopeCheck}
                onCheckDrift={() => workbench.checkClusterDrift()}
                onRefreshCost={() => workbench.refreshCostSnapshot()}
                onResetCost={workbench.resetCostLedger}
              />
            </SolanaPanelSlot>
          </Suspense>

          <SolanaPanelSlot active={activeTab === "rpc"} tabId="rpc">
            <RpcEndpoints
              active={activeTab === "rpc"}
              onRefresh={() => workbench.refreshRpcHealth()}
              rpcHealth={workbench.rpcHealth}
            />
          </SolanaPanelSlot>
        </div>
        </div>
      </div>
      </div>
    </aside>
  )
}

interface TabDescriptor {
  id: TabId
  icon: React.ComponentType<{ className?: string }>
  label: string
  notification?: TabNotification
}

type TabNotificationTone = "activity" | "attention" | "danger"

interface TabNotification {
  label: string
  tone: TabNotificationTone
  value?: string | number
}

function TabButton({
  tab,
  active,
  onClick,
}: {
  tab: TabDescriptor
  active: boolean
  onClick: () => void
}) {
  const Icon = tab.icon
  const tooltip = tab.notification
    ? `${tab.label} — ${tab.notification.label}`
    : tab.label
  return (
    <button
      id={`tab-${tab.id}`}
      role="tab"
      aria-selected={active}
      aria-label={tab.notification ? `${tab.label}, ${tab.notification.label}` : tab.label}
      title={tooltip}
      type="button"
      onClick={onClick}
      className={cn(
        "group relative inline-flex h-10 w-10 shrink-0 items-center justify-center transition-colors",
        active
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground",
      )}
    >
      <Icon className="h-4 w-4" />
      {tab.notification ? (
        <Badge
          aria-hidden="true"
          className={cn(
            "pointer-events-none absolute right-0 top-0 flex h-3.5 min-w-[14px] rounded-full border-0 px-[3px] py-0 text-[8.5px] font-semibold leading-none tabular-nums ring-1 ring-sidebar",
            tab.notification.tone === "activity" &&
              "bg-primary text-primary-foreground",
            tab.notification.tone === "attention" &&
              "bg-warning text-warning",
            tab.notification.tone === "danger" &&
              "bg-destructive text-destructive-foreground",
          )}
        >
          {tab.notification.tone === "activity" ? (
            <Loader2 className="h-2.5 w-2.5 animate-spin" />
          ) : (
            tab.notification.value
          )}
        </Badge>
      ) : null}
    </button>
  )
}

function activityNotification(
  active: boolean,
  label: string,
): TabNotification | undefined {
  return active ? { label, tone: "activity" } : undefined
}

function issueNotification(
  count: number,
  singular: string,
  plural: string,
  tone: Exclude<TabNotificationTone, "activity"> = "attention",
): TabNotification | undefined {
  if (count <= 0) return undefined
  return {
    label: `${count} ${count === 1 ? singular : plural}`,
    tone,
    value: count > 99 ? "99+" : count,
  }
}

function countErroredLogs(entries: LogEntry[]): number {
  return entries.filter(
    (entry) => entry.err != null || !entry.explanation.ok,
  ).length
}

function deployResultHasIssue(result: DeployResult | null): boolean {
  if (!result) return false
  if (result.kind === "direct") {
    return (
      !result.outcome.success ||
      result.idlPublish?.success === false ||
      result.codama?.allSucceeded === false
    )
  }
  return !result.bufferWrite.success
}

function KV({
  label,
  value,
  mono,
}: {
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="w-12 shrink-0 text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 truncate text-foreground/85",
          mono && "font-mono text-[10.5px]",
        )}
      >
        {value}
      </span>
    </div>
  )
}

function RpcEndpoints({
  rpcHealth,
  onRefresh,
  active,
}: {
  rpcHealth: ReturnType<typeof useSolanaWorkbench>["rpcHealth"]
  onRefresh: () => Promise<void>
  active: boolean
}) {
  const [probing, setProbing] = useState(false)
  const hasProbedRef = useRef(false)

  const runProbe = useCallback(async () => {
    if (probing) return
    setProbing(true)
    try {
      await onRefresh()
    } finally {
      setProbing(false)
    }
  }, [onRefresh, probing])

  useEffect(() => {
    if (!active || hasProbedRef.current) return
    hasProbedRef.current = true
    void runProbe()
  }, [active, runProbe])

  const grouped = useMemo(() => {
    const map = new Map<ClusterKind, typeof rpcHealth>()
    for (const entry of rpcHealth) {
      const list = map.get(entry.cluster) ?? []
      list.push(entry)
      map.set(entry.cluster, list)
    }
    return Array.from(map.entries())
  }, [rpcHealth])

  const totals = useMemo(() => {
    const healthy = rpcHealth.filter((h) => h.healthy).length
    return { healthy, total: rpcHealth.length }
  }, [rpcHealth])

  return (
    <div className="flex flex-col gap-3">
      <PanelHeader
        icon={Server}
        title="RPC endpoints"
        description="Latency and health probes for the bundled free-tier RPC pool."
        busy={probing}
        right={
          <button
            type="button"
            aria-label="Probe RPC health"
            disabled={probing}
            onClick={() => void runProbe()}
            className="inline-flex h-7 items-center gap-1 rounded-md border border-border/70 bg-background/40 px-2 text-[11px] text-foreground/85 transition-colors hover:border-primary/40 hover:text-foreground disabled:opacity-50"
          >
            <RefreshCw className={cn("h-3 w-3", probing && "animate-spin")} />
            {probing ? "Probing" : "Probe"}
          </button>
        }
      />

      {rpcHealth.length === 0 ? (
        <div className="rounded-md border border-dashed border-border/70 bg-background/30 px-3 py-3 text-[11.5px] leading-relaxed text-muted-foreground">
          {probing ? (
            <span className="inline-flex items-center gap-2">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Probing free-tier RPC endpoints…
            </span>
          ) : (
            <>
              <span className="italic">No probe results yet.</span>{" "}
              <span>
                Probe to measure latency and health for the bundled RPC pool across
                each cluster.
              </span>
            </>
          )}
        </div>
      ) : (
        <>
          <div className="flex items-center justify-between rounded-md border border-border/70 bg-background/40 px-2.5 py-1.5 text-[11px] text-muted-foreground">
            <span>
              <span className="font-mono tabular-nums text-foreground/85">
                {totals.healthy}
              </span>
              {" / "}
              <span className="font-mono tabular-nums">{totals.total}</span>{" "}
              healthy
            </span>
            <span className="text-[10.5px] text-muted-foreground/80">
              {grouped.length} cluster{grouped.length === 1 ? "" : "s"}
            </span>
          </div>
          <div className="flex flex-col gap-3">
            {grouped.map(([cluster, list]) => (
              <section key={cluster} className="flex flex-col gap-1">
                <div className="flex items-center justify-between text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                  <span>{cluster}</span>
                  <span className="font-mono tabular-nums text-muted-foreground/70">
                    {list.filter((e) => e.healthy).length}/{list.length}
                  </span>
                </div>
                <ul className="flex flex-col divide-y divide-border/40 rounded-md border border-border/70 bg-background/40">
                  {list.map((health) => (
                    <li
                      key={`${health.cluster}-${health.id}`}
                      className="flex items-center justify-between gap-2 px-2.5 py-2"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[11.5px] font-medium text-foreground">
                          {health.label ?? health.id}
                        </div>
                        <div className="truncate font-mono text-[10.5px] text-muted-foreground">
                          {health.url}
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1.5 text-[11px]">
                        {health.healthy ? (
                          <CircleCheckBig className="h-3.5 w-3.5 text-success" />
                        ) : (
                          <CircleSlash className="h-3.5 w-3.5 text-destructive" />
                        )}
                        {health.latencyMs != null ? (
                          <span className="font-mono tabular-nums text-muted-foreground">
                            {health.latencyMs}ms
                          </span>
                        ) : null}
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        </>
      )}
    </div>
  )
}

function StatusDot({
  running,
  starting,
}: {
  running: boolean
  starting: boolean
}) {
  return (
    <span className="relative flex h-2 w-2 items-center justify-center">
      {running ? (
        <>
          <span className="absolute inset-0 animate-ping rounded-full bg-success/50" />
          <span className="relative h-2 w-2 rounded-full bg-success" />
        </>
      ) : starting ? (
        <span className="h-2 w-2 animate-pulse rounded-full bg-warning" />
      ) : (
        <span className="h-2 w-2 rounded-full bg-muted-foreground/50" />
      )}
    </span>
  )
}
