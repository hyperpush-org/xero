"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  CircleCheckBig,
  CircleSlash,
  FileJson,
  Loader2,
  Play,
  RefreshCw,
  Rocket,
  Search,
  Server,
  Square,
  Users,
  Waves,
  Zap,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { SolanaDeployPanel } from "./solana-deploy-panel"
import { SolanaIdlPanel } from "./solana-idl-panel"
import { SolanaMissingToolchain } from "./solana-missing-toolchain"
import { SolanaPersonaPanel } from "./solana-persona-panel"
import { SolanaScenarioPanel } from "./solana-scenario-panel"
import { SolanaTxInspector } from "./solana-tx-inspector"
import {
  useSolanaWorkbench,
  type ClusterKind,
  type CodamaTarget,
  type DeployAuthority,
  type FundingDelta,
  type PersonaRole,
  type SimulateRequest,
} from "@/src/features/solana/use-solana-workbench"

const MIN_WIDTH = 320
const DEFAULT_WIDTH = 420
const MAX_WIDTH = 900
const STORAGE_KEY = "cadence.solana.workbench.width"

type TabId = "personas" | "scenarios" | "tx" | "idl" | "deploy" | "rpc"

interface SolanaWorkbenchSidebarProps {
  open: boolean
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

export function SolanaWorkbenchSidebar({ open }: SolanaWorkbenchSidebarProps) {
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const widthRef = useRef(width)
  widthRef.current = width

  const workbench = useSolanaWorkbench({ active: open })
  const [selectedKind, setSelectedKind] = useState<ClusterKind>("localnet")
  const [activeTab, setActiveTab] = useState<TabId>("personas")

  useEffect(() => {
    if (!workbench.clusters.length) return
    setSelectedKind((current) => {
      if (workbench.clusters.some((c) => c.kind === current)) return current
      const firstStartable = workbench.clusters.find((c) => c.startable)
      return firstStartable?.kind ?? current
    })
  }, [workbench.clusters])

  useEffect(() => {
    writePersistedWidth(width)
  }, [width])

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      const startX = event.clientX
      const startWidth = widthRef.current
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        const next = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
        setWidth(next)
      }
      const handleUp = () => {
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
        setIsResizing(false)
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

  const scenariosForCluster = useMemo(
    () =>
      workbench.scenarios.filter((s) =>
        s.supportedClusters.includes(selectedKind),
      ),
    [workbench.scenarios, selectedKind],
  )

  const tabs: TabDescriptor[] = [
    {
      id: "personas",
      icon: Users,
      label: "Personas",
      count: workbench.personas.length || undefined,
    },
    {
      id: "scenarios",
      icon: Zap,
      label: "Scenarios",
      count: scenariosForCluster.length || undefined,
    },
    {
      id: "tx",
      icon: Search,
      label: "Tx",
    },
    {
      id: "idl",
      icon: FileJson,
      label: "IDL",
      count: Object.keys(workbench.idls).length || undefined,
    },
    {
      id: "deploy",
      icon: Rocket,
      label: "Deploy",
    },
    {
      id: "rpc",
      icon: Server,
      label: "RPC",
      count:
        workbench.rpcHealth.length > 0
          ? `${workbench.rpcHealth.filter((h) => h.healthy).length}/${workbench.rpcHealth.length}`
          : undefined,
    },
  ]

  return (
    <aside
      aria-hidden={!open}
      className={cn(
        "relative flex shrink-0 flex-col overflow-hidden border-l border-border/80 bg-sidebar",
        !isResizing && "transition-[width] duration-200 ease-out",
        !open && "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={{ width: open ? width : 0 }}
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

      <div className="flex h-10 shrink-0 items-center justify-between border-b border-border/70 pl-3 pr-2">
        <div className="flex items-center gap-2">
          <Waves aria-hidden className="h-3.5 w-3.5 text-primary" />
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Solana Workbench
          </span>
        </div>
        <button
          aria-label="Refresh toolchain"
          className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground disabled:opacity-60"
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

      <SolanaMissingToolchain
        loading={workbench.toolchainLoading}
        onRefresh={() => void workbench.refreshToolchain()}
        status={workbench.toolchain}
      />

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin">
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
                ? "Local cluster — Cadence can spin it up on your machine."
                : "Remote cluster — read-only from here."}
            </p>
          ) : null}
        </section>

        <section className="border-b border-border/70 px-3 py-3">
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

        <div
          role="tablist"
          aria-label="Workbench tools"
          className="sticky top-0 z-10 flex h-9 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-border/70 bg-sidebar px-2 scrollbar-thin"
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

        <div
          role="tabpanel"
          aria-labelledby={`tab-${activeTab}`}
          className="px-3 py-3"
        >
          {activeTab === "personas" ? (
            <SolanaPersonaPanel
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
          ) : null}

          {activeTab === "scenarios" ? (
            <SolanaScenarioPanel
              busy={workbench.scenarioBusy}
              cluster={selectedKind}
              clusterRunning={clusterRunning}
              lastRun={workbench.lastScenarioRun}
              onRunScenario={workbench.runScenario}
              personas={workbench.personas}
              scenarios={workbench.scenarios}
            />
          ) : null}

          {activeTab === "tx" ? (
            <SolanaTxInspector
              cluster={selectedKind}
              clusterRunning={clusterRunning}
              txBusy={workbench.txBusy}
              lastSimulation={workbench.lastSimulation}
              lastExplanation={workbench.lastExplanation}
              onSimulate={handleSimulate}
              onExplain={handleExplain}
              onEstimateFee={handleEstimateFee}
            />
          ) : null}

          {activeTab === "idl" ? (
            <SolanaIdlPanel
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
          ) : null}

          {activeTab === "deploy" ? (
            <SolanaDeployPanel
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
          ) : null}

          {activeTab === "rpc" ? (
            <RpcEndpoints
              onRefresh={() => void workbench.refreshRpcHealth()}
              rpcHealth={workbench.rpcHealth}
            />
          ) : null}
        </div>
      </div>
    </aside>
  )
}

interface TabDescriptor {
  id: TabId
  icon: React.ComponentType<{ className?: string }>
  label: string
  count?: string | number
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
  return (
    <button
      id={`tab-${tab.id}`}
      role="tab"
      aria-selected={active}
      type="button"
      onClick={onClick}
      className={cn(
        "relative inline-flex shrink-0 items-center gap-1.5 px-2 py-1.5 text-[11px] transition-colors",
        active
          ? "text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      <span>{tab.label}</span>
      {tab.count != null ? (
        <span
          className={cn(
            "rounded px-1 text-[9.5px] font-medium tabular-nums",
            active
              ? "bg-primary/15 text-primary"
              : "bg-secondary/60 text-muted-foreground",
          )}
        >
          {tab.count}
        </span>
      ) : null}
      {active ? (
        <span className="absolute inset-x-1 -bottom-px h-px bg-primary" />
      ) : null}
    </button>
  )
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
}: {
  rpcHealth: ReturnType<typeof useSolanaWorkbench>["rpcHealth"]
  onRefresh: () => void
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-end">
        <button
          type="button"
          aria-label="Refresh RPC health"
          className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
          onClick={onRefresh}
        >
          <RefreshCw className="h-3 w-3" />
          Probe
        </button>
      </div>
      {rpcHealth.length === 0 ? (
        <p className="text-[11.5px] text-muted-foreground">
          Probe to check the free-tier endpoint pool.
        </p>
      ) : (
        <ul className="flex flex-col divide-y divide-border/40">
          {rpcHealth.map((health) => (
            <li
              key={`${health.cluster}-${health.id}`}
              className="flex items-center justify-between gap-2 px-1 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12px] font-medium text-foreground">
                  {health.label ?? health.id}
                </div>
                <div className="truncate font-mono text-[10.5px] text-muted-foreground">
                  {health.cluster} · {health.url}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5 text-[11px]">
                {health.healthy ? (
                  <CircleCheckBig className="h-3.5 w-3.5 text-emerald-400" />
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
          <span className="absolute inset-0 animate-ping rounded-full bg-emerald-400/50" />
          <span className="relative h-2 w-2 rounded-full bg-emerald-400" />
        </>
      ) : starting ? (
        <span className="h-2 w-2 animate-pulse rounded-full bg-amber-400" />
      ) : (
        <span className="h-2 w-2 rounded-full bg-muted-foreground/50" />
      )}
    </span>
  )
}
