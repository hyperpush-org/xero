"use client"

import { useCallback, useMemo, useState } from "react"
import {
  AlertOctagon,
  AlertTriangle,
  Coins,
  FileSearch,
  GitCompare,
  Info,
  Loader2,
  RefreshCw,
  RotateCcw,
  ScanSearch,
  ShieldAlert,
  ShieldCheck,
  Users,
} from "lucide-react"
import { cn } from "@/lib/utils"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type {
  ClusterDriftReport,
  ClusterKind,
  CostSnapshot,
  DriftStatus,
  ScopeCheckReport,
  SecretFinding,
  SecretScanReport,
  SecretSeverity,
  TrackedProgram,
} from "@/src/features/solana/use-solana-workbench"

type SafetyTab = "secrets" | "scope" | "drift" | "cost"
const ALL_SEVERITIES = "__all__"

const SEVERITY_CLASS: Record<SecretSeverity, string> = {
  critical: "bg-destructive/15 text-destructive border-destructive/40",
  high: "bg-amber-500/15 text-amber-400 border-amber-500/40",
  medium: "bg-yellow-500/15 text-yellow-300 border-yellow-500/40",
  low: "bg-sky-500/15 text-sky-300 border-sky-500/40",
}

const SEVERITY_LABEL: Record<SecretSeverity, string> = {
  critical: "Critical",
  high: "High",
  medium: "Medium",
  low: "Low",
}

const SEVERITY_ICON: Record<SecretSeverity, React.ComponentType<{ className?: string }>> = {
  critical: AlertOctagon,
  high: AlertTriangle,
  medium: ShieldAlert,
  low: Info,
}

const DRIFT_COPY: Record<
  DriftStatus,
  { label: string; className: string }
> = {
  in_sync: {
    label: "In sync",
    className: "bg-emerald-500/15 text-emerald-300 border-emerald-500/40",
  },
  drift: {
    label: "Drift",
    className: "bg-amber-500/15 text-amber-300 border-amber-500/40",
  },
  partially_deployed: {
    label: "Partial",
    className: "bg-sky-500/15 text-sky-300 border-sky-500/40",
  },
  inconclusive: {
    label: "Inconclusive",
    className: "bg-muted/40 text-muted-foreground border-border/60",
  },
}

function lamportsToSol(lamports: number): string {
  if (!Number.isFinite(lamports)) return "—"
  const sol = lamports / 1_000_000_000
  if (Math.abs(sol) >= 0.01) return `${sol.toFixed(4)} SOL`
  return `${lamports.toLocaleString()} lamports`
}

function shortHash(hash?: string | null): string {
  if (!hash) return "—"
  return hash.length > 12 ? `${hash.slice(0, 12)}…` : hash
}

interface SolanaSafetyPanelProps {
  busy: boolean
  projectRootDefault?: string
  lastSecretScan: SecretScanReport | null
  lastScopeCheck: ScopeCheckReport | null
  lastDrift: ClusterDriftReport | null
  lastCost: CostSnapshot | null
  trackedPrograms: TrackedProgram[]
  onScanSecrets: (args: {
    projectRoot: string
    skipPaths?: string[]
    minSeverity?: SecretSeverity | null
  }) => Promise<SecretScanReport | null>
  onRunScopeCheck: () => Promise<ScopeCheckReport | null>
  onCheckDrift: () => Promise<ClusterDriftReport | null>
  onRefreshCost: () => Promise<CostSnapshot | null>
  onResetCost: () => Promise<void>
}

export function SolanaSafetyPanel({
  busy,
  projectRootDefault,
  lastSecretScan,
  lastScopeCheck,
  lastDrift,
  lastCost,
  trackedPrograms,
  onScanSecrets,
  onRunScopeCheck,
  onCheckDrift,
  onRefreshCost,
  onResetCost,
}: SolanaSafetyPanelProps) {
  const [activeTab, setActiveTab] = useState<SafetyTab>("secrets")
  const [projectRoot, setProjectRoot] = useState<string>(projectRootDefault ?? "")
  const [minSeverity, setMinSeverity] = useState<SecretSeverity | "">("")

  const handleScan = useCallback(async () => {
    const trimmed = projectRoot.trim()
    if (!trimmed) return
    await onScanSecrets({
      projectRoot: trimmed,
      minSeverity: minSeverity === "" ? null : minSeverity,
    })
  }, [minSeverity, onScanSecrets, projectRoot])

  const counts = useMemo(() => {
    if (!lastSecretScan) return null
    const acc: Record<SecretSeverity, number> = {
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
    }
    for (const f of lastSecretScan.findings) acc[f.severity] += 1
    return acc
  }, [lastSecretScan])

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-[11px] font-medium text-foreground/80">
          <ShieldCheck className="h-3.5 w-3.5 text-primary" />
          Safety
        </div>
        {busy ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
        ) : null}
      </div>

      <div role="tablist" className="flex gap-1 rounded-md border border-border/60 bg-background/40 p-0.5 text-[11px]">
        <SafetyTabButton
          active={activeTab === "secrets"}
          icon={ScanSearch}
          label="Secrets"
          count={lastSecretScan?.findings.length ?? null}
          onClick={() => setActiveTab("secrets")}
        />
        <SafetyTabButton
          active={activeTab === "scope"}
          icon={Users}
          label="Scope"
          count={lastScopeCheck?.warnings.length ?? null}
          onClick={() => setActiveTab("scope")}
        />
        <SafetyTabButton
          active={activeTab === "drift"}
          icon={GitCompare}
          label="Drift"
          count={
            lastDrift
              ? lastDrift.entries.filter((e) => e.status === "drift").length
              : null
          }
          onClick={() => setActiveTab("drift")}
        />
        <SafetyTabButton
          active={activeTab === "cost"}
          icon={Coins}
          label="Cost"
          count={lastCost ? lastCost.totals.txCount : null}
          onClick={() => setActiveTab("cost")}
        />
      </div>

      {activeTab === "secrets" ? (
        <section className="flex flex-col gap-3 rounded-md border border-border/60 bg-background/30 p-3">
          <div className="flex flex-col gap-2">
            <label className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
              Project root
            </label>
            <input
              value={projectRoot}
              onChange={(e) => setProjectRoot(e.target.value)}
              placeholder="/absolute/path/to/project"
              className="rounded-md border border-border/60 bg-background px-2 py-1 font-mono text-[11px]"
            />
            <div className="flex items-center gap-2">
              <label className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
                Min severity
              </label>
              <Select
                value={minSeverity || ALL_SEVERITIES}
                onValueChange={(value) =>
                  setMinSeverity(
                    value === ALL_SEVERITIES ? "" : (value as SecretSeverity),
                  )
                }
              >
                <SelectTrigger
                  aria-label="Min severity"
                  className="h-7 w-[112px] border-border/60 bg-background px-2 text-[11px]"
                  size="sm"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_SEVERITIES}>All</SelectItem>
                  <SelectItem value="critical">Critical+</SelectItem>
                  <SelectItem value="high">High+</SelectItem>
                  <SelectItem value="medium">Medium+</SelectItem>
                  <SelectItem value="low">Low+</SelectItem>
                </SelectContent>
              </Select>
              <button
                type="button"
                disabled={busy || !projectRoot.trim()}
                onClick={() => void handleScan()}
                className={cn(
                  "ml-auto inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary",
                  "hover:bg-primary/25 disabled:opacity-50",
                )}
              >
                <FileSearch className="h-3 w-3" />
                Scan
              </button>
            </div>
          </div>

          {lastSecretScan ? (
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-2 text-[11px]">
                <span
                  className={cn(
                    "rounded-full border px-2 py-0.5 font-medium",
                    lastSecretScan.blocksDeploy
                      ? "border-destructive/50 bg-destructive/10 text-destructive"
                      : "border-emerald-500/40 bg-emerald-500/10 text-emerald-300",
                  )}
                >
                  {lastSecretScan.blocksDeploy
                    ? "Deploy blocked"
                    : "Deploy OK"}
                </span>
                <span className="text-muted-foreground">
                  {lastSecretScan.filesScanned} files scanned · {lastSecretScan.findings.length}{" "}
                  findings · {lastSecretScan.durationMs}ms
                </span>
              </div>
              {counts ? (
                <div className="flex flex-wrap gap-1.5">
                  {(Object.keys(counts) as SecretSeverity[]).map((sev) =>
                    counts[sev] > 0 ? (
                      <span
                        key={sev}
                        className={cn(
                          "inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[10.5px]",
                          SEVERITY_CLASS[sev],
                        )}
                      >
                        {SEVERITY_LABEL[sev]}: {counts[sev]}
                      </span>
                    ) : null,
                  )}
                </div>
              ) : null}
              <ul className="flex flex-col divide-y divide-border/40">
                {lastSecretScan.findings.map((finding) => (
                  <SecretFindingRow key={`${finding.ruleId}-${finding.path}-${finding.line ?? 0}`} finding={finding} />
                ))}
              </ul>
            </div>
          ) : (
            <p className="text-[11px] text-muted-foreground">
              Scan a project to surface committed keypairs, RPC keys, and other
              secrets before you deploy.
            </p>
          )}
        </section>
      ) : null}

      {activeTab === "scope" ? (
        <section className="flex flex-col gap-3 rounded-md border border-border/60 bg-background/30 p-3">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-muted-foreground">
              Scope the workbench's personas for cross-cluster reuse and
              mainnet-tagged imports.
            </span>
            <button
              type="button"
              disabled={busy}
              onClick={() => void onRunScopeCheck()}
              className="inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2 py-1 text-[11px] text-primary hover:bg-primary/25 disabled:opacity-50"
            >
              <Users className="h-3 w-3" />
              Check
            </button>
          </div>
          {lastScopeCheck ? (
            <>
              <div className="text-[11px] text-muted-foreground">
                {lastScopeCheck.personasInspected} personas inspected · {lastScopeCheck.warnings.length} warnings
              </div>
              <ul className="flex flex-col divide-y divide-border/40">
                {lastScopeCheck.warnings.map((w, idx) => (
                  <li
                    key={`${w.persona}-${w.kind}-${idx}`}
                    className="flex flex-col gap-1 py-2"
                  >
                    <div className="flex items-center gap-2 text-[11.5px] font-medium">
                      <span
                        className={cn(
                          "rounded-md border px-1.5 py-0.5 text-[10.5px]",
                          SEVERITY_CLASS[w.severity],
                        )}
                      >
                        {SEVERITY_LABEL[w.severity]}
                      </span>
                      <span>{w.persona}</span>
                      <span className="text-muted-foreground">· {w.cluster}</span>
                    </div>
                    <p className="text-[11px] text-foreground/80">{w.message}</p>
                    <p className="text-[10.5px] text-muted-foreground">
                      Fix: {w.remediation}
                    </p>
                  </li>
                ))}
              </ul>
            </>
          ) : (
            <p className="text-[11px] text-muted-foreground">
              Run to surface reused keypairs across clusters, mainnet labels on
              devnet personas, and imported keys on non-local clusters.
            </p>
          )}
        </section>
      ) : null}

      {activeTab === "drift" ? (
        <section className="flex flex-col gap-3 rounded-md border border-border/60 bg-background/30 p-3">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-muted-foreground">
              Compares {trackedPrograms.length} tracked external programs across
              clusters.
            </span>
            <button
              type="button"
              disabled={busy}
              onClick={() => void onCheckDrift()}
              className="inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2 py-1 text-[11px] text-primary hover:bg-primary/25 disabled:opacity-50"
            >
              <GitCompare className="h-3 w-3" />
              Check
            </button>
          </div>
          {lastDrift ? (
            <ul className="flex flex-col divide-y divide-border/40">
              {lastDrift.entries.map((entry) => (
                <li
                  key={entry.program.programId}
                  className="flex flex-col gap-1 py-2"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={cn(
                        "rounded-md border px-1.5 py-0.5 text-[10.5px]",
                        DRIFT_COPY[entry.status].className,
                      )}
                    >
                      {DRIFT_COPY[entry.status].label}
                    </span>
                    <span className="text-[11.5px] font-medium">
                      {entry.program.label}
                    </span>
                  </div>
                  <p className="text-[11px] text-foreground/80">{entry.summary}</p>
                  <ul className="grid grid-cols-2 gap-1 text-[10.5px] text-muted-foreground">
                    {entry.probes.map((probe) => (
                      <li
                        key={`${probe.cluster}-${probe.rpcUrl}`}
                        className="font-mono"
                      >
                        {probe.cluster}: {shortHash(probe.programDataSha256)}
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-[11px] text-muted-foreground">
              Run a check to compare program bytes across configured clusters.
            </p>
          )}
        </section>
      ) : null}

      {activeTab === "cost" ? (
        <section className="flex flex-col gap-3 rounded-md border border-border/60 bg-background/30 p-3">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-muted-foreground">
              Local cost ledger plus per-provider health.
            </span>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                disabled={busy}
                onClick={() => void onRefreshCost()}
                className="inline-flex items-center gap-1.5 rounded-md border border-primary/50 bg-primary/15 px-2 py-1 text-[11px] text-primary hover:bg-primary/25 disabled:opacity-50"
              >
                <RefreshCw className="h-3 w-3" />
                Refresh
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void onResetCost()}
                className="inline-flex items-center gap-1.5 rounded-md border border-border/60 bg-background/60 px-2 py-1 text-[11px] text-muted-foreground hover:text-foreground disabled:opacity-50"
              >
                <RotateCcw className="h-3 w-3" />
                Reset
              </button>
            </div>
          </div>
          {lastCost ? (
            <div className="flex flex-col gap-2 text-[11.5px]">
              <div className="grid grid-cols-2 gap-2 rounded-md border border-border/60 bg-background/40 p-2">
                <Metric label="Tx" value={lastCost.totals.txCount.toLocaleString()} />
                <Metric
                  label="Lamports spent"
                  value={lamportsToSol(lastCost.totals.lamportsSpent)}
                />
                <Metric
                  label="CUs"
                  value={lastCost.totals.computeUnitsUsed.toLocaleString()}
                />
                <Metric
                  label="Rent"
                  value={lamportsToSol(lastCost.totals.rentLockedLamports)}
                />
              </div>
              {lastCost.local.byCluster.length > 0 ? (
                <div className="flex flex-col gap-1">
                  <h4 className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
                    Per cluster
                  </h4>
                  <ul className="flex flex-col divide-y divide-border/40">
                    {lastCost.local.byCluster.map((bucket) => (
                      <li
                        key={bucket.cluster}
                        className="flex items-center justify-between py-1.5 text-[11px]"
                      >
                        <span className="capitalize">{bucket.cluster}</span>
                        <span className="font-mono text-muted-foreground">
                          {bucket.txCount} tx · {lamportsToSol(bucket.lamportsSpent)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
              {lastCost.providers.length > 0 ? (
                <div className="flex flex-col gap-1">
                  <h4 className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
                    Providers ({lastCost.totals.providersHealthy} healthy / {lastCost.totals.providersDegraded} degraded)
                  </h4>
                  <ul className="flex flex-col divide-y divide-border/40">
                    {lastCost.providers.map((provider) => (
                      <li
                        key={`${provider.cluster}-${provider.endpointId}`}
                        className="flex flex-col gap-0.5 py-1.5 text-[11px]"
                      >
                        <div className="flex items-center gap-2">
                          <span className="font-medium">{provider.endpointId}</span>
                          <span className="text-muted-foreground">
                            · {provider.cluster}
                          </span>
                          <span className="ml-auto text-muted-foreground capitalize">
                            {provider.health}
                          </span>
                        </div>
                        {provider.warning ? (
                          <p className="text-[10.5px] text-muted-foreground">
                            {provider.warning}
                          </p>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </div>
          ) : (
            <p className="text-[11px] text-muted-foreground">
              Refresh to roll up tx lamports, CUs, and provider health.
            </p>
          )}
        </section>
      ) : null}
    </div>
  )
}

function SafetyTabButton({
  active,
  icon: Icon,
  label,
  count,
  onClick,
}: {
  active: boolean
  icon: React.ComponentType<{ className?: string }>
  label: string
  count: number | null
  onClick: () => void
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "inline-flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 transition-colors",
        active
          ? "bg-primary/10 text-primary"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="h-3 w-3" />
      {label}
      {count != null && count > 0 ? (
        <span
          className={cn(
            "rounded px-1 text-[9.5px] tabular-nums",
            active ? "bg-primary/20 text-primary" : "bg-secondary/60 text-muted-foreground",
          )}
        >
          {count}
        </span>
      ) : null}
    </button>
  )
}

function SecretFindingRow({ finding }: { finding: SecretFinding }) {
  const Icon = SEVERITY_ICON[finding.severity]
  return (
    <li className="flex flex-col gap-1 py-2">
      <div className="flex items-center gap-2">
        <span
          className={cn(
            "inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[10.5px]",
            SEVERITY_CLASS[finding.severity],
          )}
        >
          <Icon className="h-3 w-3" />
          {SEVERITY_LABEL[finding.severity]}
        </span>
        <span className="text-[11.5px] font-medium">{finding.title}</span>
      </div>
      <div className="font-mono text-[10.5px] text-muted-foreground">
        {finding.path}
        {finding.line != null ? `:${finding.line}` : ""} · {finding.evidence}
      </div>
      <div className="text-[10.5px] text-foreground/80">{finding.remediation}</div>
      {finding.referenceUrl ? (
        <a
          href={finding.referenceUrl}
          target="_blank"
          rel="noreferrer"
          className="text-[10.5px] text-primary hover:underline"
        >
          Docs →
        </a>
      ) : null}
    </li>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span className="font-mono tabular-nums text-foreground/90">{value}</span>
    </div>
  )
}
