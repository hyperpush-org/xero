"use client"

import { useCallback, useMemo, useState } from "react"
import {
  AlertOctagon,
  AlertTriangle,
  BookOpen,
  Bug,
  FileSearch,
  FlaskConical,
  Info,
  Loader2,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Target,
  X,
  Zap,
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
  AnalyzerKind,
  AuditEventPayload,
  ClusterKind,
  CoverageReport,
  ExploitDescriptor,
  ExploitKey,
  ExternalAnalyzerReport,
  Finding,
  FindingSeverity,
  FuzzReport,
  ReplayReport,
  StaticLintReport,
  TridentHarnessResult,
} from "@/src/features/solana/use-solana-workbench"

type AuditTab = "lints" | "external" | "fuzz" | "coverage" | "replay"

const ALL_SEVERITIES: FindingSeverity[] = [
  "critical",
  "high",
  "medium",
  "low",
  "informational",
]

const SEVERITY_LABEL: Record<FindingSeverity, string> = {
  critical: "Critical",
  high: "High",
  medium: "Medium",
  low: "Low",
  informational: "Info",
}

const SEVERITY_CLASS: Record<FindingSeverity, string> = {
  critical: "bg-destructive/15 text-destructive border-destructive/40",
  high: "bg-amber-500/15 text-amber-400 border-amber-500/40",
  medium: "bg-yellow-500/15 text-yellow-300 border-yellow-500/40",
  low: "bg-sky-500/15 text-sky-300 border-sky-500/40",
  informational: "bg-muted/40 text-muted-foreground border-border/60",
}

const SEVERITY_ICON: Record<FindingSeverity, React.ComponentType<{ className?: string }>> = {
  critical: AlertOctagon,
  high: AlertTriangle,
  medium: ShieldAlert,
  low: Info,
  informational: Info,
}

interface SolanaAuditPanelProps {
  cluster: ClusterKind
  clusterRunning: boolean
  busy: boolean
  findings: Finding[]
  events: AuditEventPayload[]
  lastStatic: StaticLintReport | null
  lastExternal: ExternalAnalyzerReport | null
  lastFuzz: FuzzReport | null
  lastCoverage: CoverageReport | null
  lastReplay: ReplayReport | null
  replayCatalog: ExploitDescriptor[]
  onClearFeed: () => void
  onRunStatic: (args: {
    projectRoot: string
    ruleIds?: string[]
    skipPaths?: string[]
  }) => Promise<StaticLintReport | null>
  onRunExternal: (args: {
    projectRoot: string
    analyzer?: AnalyzerKind
    timeoutS?: number | null
  }) => Promise<ExternalAnalyzerReport | null>
  onRunFuzz: (args: {
    projectRoot: string
    target: string
    durationS?: number | null
  }) => Promise<FuzzReport | null>
  onScaffoldFuzz: (args: {
    projectRoot: string
    target: string
    idlPath?: string | null
    overwrite?: boolean
  }) => Promise<TridentHarnessResult | null>
  onRunCoverage: (args: {
    projectRoot: string
    package?: string | null
    instructionNames?: string[]
  }) => Promise<CoverageReport | null>
  onRunReplay: (args: {
    exploit: ExploitKey
    targetProgram: string
    cluster: ClusterKind
    dryRun?: boolean
  }) => Promise<ReplayReport | null>
}

export function SolanaAuditPanel({
  cluster,
  clusterRunning,
  busy,
  findings,
  events,
  lastStatic,
  lastExternal,
  lastFuzz,
  lastCoverage,
  lastReplay,
  replayCatalog,
  onClearFeed,
  onRunStatic,
  onRunExternal,
  onRunFuzz,
  onScaffoldFuzz,
  onRunCoverage,
  onRunReplay,
}: SolanaAuditPanelProps) {
  const [activeTab, setActiveTab] = useState<AuditTab>("lints")
  const [severityFilter, setSeverityFilter] = useState<FindingSeverity[]>([])

  const filteredFindings = useMemo(() => {
    if (severityFilter.length === 0) return findings
    const allow = new Set(severityFilter)
    return findings.filter((f) => allow.has(f.severity))
  }, [findings, severityFilter])

  const severityCounts = useMemo(() => {
    const counts: Record<FindingSeverity, number> = {
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
      informational: 0,
    }
    for (const finding of findings) {
      counts[finding.severity] += 1
    }
    return counts
  }, [findings])

  const toggleSeverity = useCallback((severity: FindingSeverity) => {
    setSeverityFilter((current) => {
      if (current.includes(severity)) {
        return current.filter((s) => s !== severity)
      }
      return [...current, severity]
    })
  }, [])

  return (
    <div className="flex flex-col gap-3">
      <div
        role="tablist"
        aria-label="Audit sections"
        className="flex shrink-0 items-center gap-0.5 overflow-x-auto border-b border-border/60 scrollbar-thin"
      >
        <AuditTabButton
          active={activeTab === "lints"}
          onClick={() => setActiveTab("lints")}
          icon={FileSearch}
          label="Lints"
        />
        <AuditTabButton
          active={activeTab === "external"}
          onClick={() => setActiveTab("external")}
          icon={ShieldCheck}
          label="Analyzer"
        />
        <AuditTabButton
          active={activeTab === "fuzz"}
          onClick={() => setActiveTab("fuzz")}
          icon={Bug}
          label="Fuzz"
        />
        <AuditTabButton
          active={activeTab === "coverage"}
          onClick={() => setActiveTab("coverage")}
          icon={FlaskConical}
          label="Coverage"
        />
        <AuditTabButton
          active={activeTab === "replay"}
          onClick={() => setActiveTab("replay")}
          icon={Target}
          label="Replay"
        />
      </div>

      {findings.length > 0 ? (
        <section className="rounded-md border border-border/60 bg-background/40 p-2">
          <div className="flex items-center justify-between">
            <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Findings
            </div>
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
              onClick={onClearFeed}
              aria-label="Clear audit feed"
            >
              <X className="h-3 w-3" />
              Clear
            </button>
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {ALL_SEVERITIES.map((severity) => {
              const active = severityFilter.includes(severity)
              const count = severityCounts[severity]
              return (
                <button
                  key={severity}
                  type="button"
                  onClick={() => toggleSeverity(severity)}
                  className={cn(
                    "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[10.5px] transition-colors",
                    SEVERITY_CLASS[severity],
                    active ? "ring-1 ring-current" : "opacity-80",
                    count === 0 && "opacity-40",
                  )}
                  disabled={count === 0 && severityFilter.length === 0}
                >
                  <span>{SEVERITY_LABEL[severity]}</span>
                  <span className="tabular-nums font-mono">{count}</span>
                </button>
              )
            })}
          </div>
          <ul className="mt-2 flex flex-col divide-y divide-border/40">
            {filteredFindings.slice(-20).reverse().map((finding) => (
              <FindingRow key={finding.id} finding={finding} />
            ))}
          </ul>
        </section>
      ) : null}

      {activeTab === "lints" ? (
        <LintsTab
          busy={busy}
          report={lastStatic}
          onRun={(projectRoot, ruleIds) =>
            onRunStatic({ projectRoot, ruleIds })
          }
        />
      ) : null}
      {activeTab === "external" ? (
        <ExternalTab busy={busy} report={lastExternal} onRun={onRunExternal} />
      ) : null}
      {activeTab === "fuzz" ? (
        <FuzzTab
          busy={busy}
          report={lastFuzz}
          onRun={onRunFuzz}
          onScaffold={onScaffoldFuzz}
        />
      ) : null}
      {activeTab === "coverage" ? (
        <CoverageTab busy={busy} report={lastCoverage} onRun={onRunCoverage} />
      ) : null}
      {activeTab === "replay" ? (
        <ReplayTab
          busy={busy}
          cluster={cluster}
          clusterRunning={clusterRunning}
          catalog={replayCatalog}
          report={lastReplay}
          onRun={onRunReplay}
        />
      ) : null}

      {events.length > 0 ? (
        <section className="rounded-md border border-border/50 bg-background/30 px-2 py-2">
          <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Audit feed
          </div>
          <ul className="mt-1 flex flex-col gap-0.5 text-[11px] text-muted-foreground">
            {events.slice(-5).reverse().map((event) => (
              <li
                key={`${event.runId}-${event.tsMs}-${event.phase}-${event.finding?.id ?? ""}`}
                className="flex items-center justify-between gap-2"
              >
                <span className="font-mono text-[10.5px]">
                  {event.kind}·{event.phase}
                </span>
                <span className="min-w-0 truncate">
                  {event.message ?? event.finding?.title ?? ""}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  )
}

interface AuditTabButtonProps {
  active: boolean
  onClick: () => void
  icon: React.ComponentType<{ className?: string }>
  label: string
}

function AuditTabButton({ active, onClick, icon: Icon, label }: AuditTabButtonProps) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "relative inline-flex shrink-0 items-center gap-1 px-2 py-1.5 text-[11px] transition-colors",
        active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      <span>{label}</span>
      {active ? (
        <span className="absolute inset-x-1 -bottom-px h-px bg-primary" />
      ) : null}
    </button>
  )
}

function FindingRow({ finding }: { finding: Finding }) {
  const Icon = SEVERITY_ICON[finding.severity]
  return (
    <li className="flex items-start gap-2 py-2">
      <Icon
        className={cn(
          "mt-0.5 h-3.5 w-3.5 shrink-0",
          finding.severity === "critical" && "text-destructive",
          finding.severity === "high" && "text-amber-400",
          finding.severity === "medium" && "text-yellow-300",
          finding.severity === "low" && "text-sky-300",
          finding.severity === "informational" && "text-muted-foreground",
        )}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0 truncate text-[12px] font-medium text-foreground">
            {finding.title}
          </div>
          <span
            className={cn(
              "shrink-0 rounded border px-1.5 py-0.5 text-[9.5px] uppercase tracking-wide",
              SEVERITY_CLASS[finding.severity],
            )}
          >
            {SEVERITY_LABEL[finding.severity]}
          </span>
        </div>
        <div className="text-[11px] text-muted-foreground">{finding.message}</div>
        {finding.file ? (
          <div className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
            {finding.file}
            {finding.line != null ? `:${finding.line}` : ""}
            {finding.column != null ? `:${finding.column}` : ""}
          </div>
        ) : null}
        {finding.fixHint ? (
          <div className="mt-1 text-[11px] italic text-foreground/80">
            Fix: {finding.fixHint}
          </div>
        ) : null}
        {finding.referenceUrl ? (
          <a
            className="mt-0.5 inline-flex items-center gap-1 text-[10.5px] text-primary hover:underline"
            href={finding.referenceUrl}
            target="_blank"
            rel="noreferrer"
          >
            <BookOpen className="h-3 w-3" />
            Reference
          </a>
        ) : null}
      </div>
    </li>
  )
}

function LintsTab({
  busy,
  report,
  onRun,
}: {
  busy: boolean
  report: StaticLintReport | null
  onRun: (projectRoot: string, ruleIds?: string[]) => Promise<StaticLintReport | null>
}) {
  const [projectRoot, setProjectRoot] = useState(report?.projectRoot ?? "")
  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault()
      if (!projectRoot.trim()) return
      await onRun(projectRoot.trim())
    },
    [projectRoot, onRun],
  )

  return (
    <form className="flex flex-col gap-2" onSubmit={handleSubmit}>
      <label className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        Project root
      </label>
      <input
        value={projectRoot}
        onChange={(e) => setProjectRoot(e.target.value)}
        placeholder="/Users/you/programs/my-anchor"
        className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px] focus:outline-none focus:ring-1 focus:ring-primary/50"
      />
      <button
        type="submit"
        disabled={busy || !projectRoot.trim()}
        className="inline-flex items-center gap-1.5 self-start rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50"
      >
        {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <Zap className="h-3 w-3" />}
        Run static lints
      </button>
      {report ? (
        <div className="rounded-md border border-border/60 bg-background/40 px-2 py-2 text-[11px] text-muted-foreground">
          <div>
            {report.filesScanned} file(s) scanned · {report.findings.length} finding(s) in{" "}
            {report.elapsedMs}ms
          </div>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {report.rules.map((rule) => (
              <span
                key={rule}
                className="rounded border border-border/50 bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px]"
              >
                {rule}
              </span>
            ))}
          </div>
        </div>
      ) : null}
    </form>
  )
}

function ExternalTab({
  busy,
  report,
  onRun,
}: {
  busy: boolean
  report: ExternalAnalyzerReport | null
  onRun: (args: {
    projectRoot: string
    analyzer?: AnalyzerKind
    timeoutS?: number | null
  }) => Promise<ExternalAnalyzerReport | null>
}) {
  const [projectRoot, setProjectRoot] = useState(report?.analyzer ? "" : "")
  const [analyzer, setAnalyzer] = useState<AnalyzerKind>(report?.analyzer ?? "auto")

  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={async (e) => {
        e.preventDefault()
        if (!projectRoot.trim()) return
        await onRun({ projectRoot: projectRoot.trim(), analyzer })
      }}
    >
      <label className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        Analyzer
      </label>
      <Select
        value={analyzer}
        onValueChange={(value) => setAnalyzer(value as AnalyzerKind)}
      >
        <SelectTrigger
          aria-label="Analyzer"
          className="h-8 w-full border-border/70 bg-background/50 text-[11.5px]"
          size="sm"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="auto">Auto (try all)</SelectItem>
          <SelectItem value="sec3">Sec3</SelectItem>
          <SelectItem value="soteria">Soteria</SelectItem>
          <SelectItem value="aderyn">Aderyn</SelectItem>
        </SelectContent>
      </Select>
      <label className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        Project root
      </label>
      <input
        value={projectRoot}
        onChange={(e) => setProjectRoot(e.target.value)}
        placeholder="/Users/you/programs/my-anchor"
        className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
      />
      <button
        type="submit"
        disabled={busy || !projectRoot.trim()}
        className="inline-flex items-center gap-1.5 self-start rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50"
      >
        {busy ? (
          <Loader2 className="h-3 w-3 animate-spin" />
        ) : (
          <ShieldCheck className="h-3 w-3" />
        )}
        Run external analyzer
      </button>
      {report ? (
        <div className="rounded-md border border-border/60 bg-background/40 px-2 py-2 text-[11px] text-muted-foreground">
          <div>{report.summary}</div>
          {!report.analyzerInstalled ? (
            <div className="mt-1 text-foreground/80">
              Install one of: Sec3 · Soteria · Aderyn (<code>cargo install aderyn</code>)
            </div>
          ) : null}
        </div>
      ) : null}
    </form>
  )
}

function FuzzTab({
  busy,
  report,
  onRun,
  onScaffold,
}: {
  busy: boolean
  report: FuzzReport | null
  onRun: (args: {
    projectRoot: string
    target: string
    durationS?: number | null
  }) => Promise<FuzzReport | null>
  onScaffold: (args: {
    projectRoot: string
    target: string
    idlPath?: string | null
    overwrite?: boolean
  }) => Promise<TridentHarnessResult | null>
}) {
  const [projectRoot, setProjectRoot] = useState(report?.projectRoot ?? "")
  const [target, setTarget] = useState(report?.target ?? "")
  const [duration, setDuration] = useState("60")
  const [idlPath, setIdlPath] = useState("")

  return (
    <div className="flex flex-col gap-3">
      <section className="flex flex-col gap-2 rounded-md border border-border/60 bg-background/40 p-2">
        <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Scaffold harness
        </div>
        <input
          value={projectRoot}
          onChange={(e) => setProjectRoot(e.target.value)}
          placeholder="Project root"
          className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
        />
        <input
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          placeholder="Fuzz target (program name)"
          className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
        />
        <input
          value={idlPath}
          onChange={(e) => setIdlPath(e.target.value)}
          placeholder="(optional) IDL path — seeds instruction stubs"
          className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
        />
        <button
          type="button"
          disabled={busy || !projectRoot.trim() || !target.trim()}
          onClick={() =>
            void onScaffold({
              projectRoot: projectRoot.trim(),
              target: target.trim(),
              idlPath: idlPath.trim() ? idlPath.trim() : null,
            })
          }
          className="inline-flex items-center gap-1.5 self-start rounded-md border border-border/70 bg-background/40 px-2.5 py-1 text-[11px] font-medium text-foreground/85 transition-colors hover:border-primary/40 disabled:opacity-50"
        >
          <FlaskConical className="h-3 w-3" />
          Scaffold Trident harness
        </button>
      </section>
      <section className="flex flex-col gap-2 rounded-md border border-border/60 bg-background/40 p-2">
        <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Run fuzz
        </div>
        <div className="flex items-center gap-2">
          <input
            value={duration}
            onChange={(e) => setDuration(e.target.value)}
            className="w-20 rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
          />
          <span className="text-[11px] text-muted-foreground">seconds</span>
        </div>
        <button
          type="button"
          disabled={busy || !projectRoot.trim() || !target.trim()}
          onClick={() =>
            void onRun({
              projectRoot: projectRoot.trim(),
              target: target.trim(),
              durationS: Number.parseInt(duration, 10) || 60,
            })
          }
          className="inline-flex items-center gap-1.5 self-start rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50"
        >
          {busy ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Bug className="h-3 w-3" />
          )}
          Run fuzz target
        </button>
        {report ? (
          <div className="text-[11px] text-muted-foreground">
            {report.crashes.length} crash(es) · coverage {report.coverageLines} lines ·{" "}
            {report.coverageDelta >= 0 ? "+" : ""}
            {report.coverageDelta} delta · {report.durationS}s
          </div>
        ) : null}
      </section>
    </div>
  )
}

function CoverageTab({
  busy,
  report,
  onRun,
}: {
  busy: boolean
  report: CoverageReport | null
  onRun: (args: {
    projectRoot: string
    package?: string | null
    instructionNames?: string[]
  }) => Promise<CoverageReport | null>
}) {
  const [projectRoot, setProjectRoot] = useState(report?.projectRoot ?? "")
  const [pkg, setPkg] = useState("")
  const [instructions, setInstructions] = useState("")

  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={async (e) => {
        e.preventDefault()
        if (!projectRoot.trim()) return
        const names = instructions
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
        await onRun({
          projectRoot: projectRoot.trim(),
          package: pkg.trim() ? pkg.trim() : null,
          instructionNames: names,
        })
      }}
    >
      <input
        value={projectRoot}
        onChange={(e) => setProjectRoot(e.target.value)}
        placeholder="Project root"
        className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
      />
      <input
        value={pkg}
        onChange={(e) => setPkg(e.target.value)}
        placeholder="(optional) cargo package"
        className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
      />
      <input
        value={instructions}
        onChange={(e) => setInstructions(e.target.value)}
        placeholder="(optional) instruction names, comma separated"
        className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
      />
      <button
        type="submit"
        disabled={busy || !projectRoot.trim()}
        className="inline-flex items-center gap-1.5 self-start rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50"
      >
        {busy ? (
          <Loader2 className="h-3 w-3 animate-spin" />
        ) : (
          <FlaskConical className="h-3 w-3" />
        )}
        Run coverage
      </button>
      {report ? (
        <div className="rounded-md border border-border/60 bg-background/40 px-2 py-2 text-[11px] text-muted-foreground">
          <div>
            Lines: {report.lineCoveragePercent.toFixed(1)}% · Functions:{" "}
            {report.functionCoveragePercent.toFixed(1)}% · {report.files.length} file(s)
          </div>
          {report.instructions.length > 0 ? (
            <ul className="mt-1 flex flex-col gap-0.5">
              {report.instructions.map((ix) => (
                <li key={ix.instruction} className="flex items-center justify-between gap-2">
                  <span className="font-mono text-[10.5px]">{ix.instruction}</span>
                  <span className="tabular-nums">
                    {ix.functionsHit}/{ix.functionsFound} fn
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </form>
  )
}

function ReplayTab({
  busy,
  cluster,
  clusterRunning,
  catalog,
  report,
  onRun,
}: {
  busy: boolean
  cluster: ClusterKind
  clusterRunning: boolean
  catalog: ExploitDescriptor[]
  report: ReplayReport | null
  onRun: (args: {
    exploit: ExploitKey
    targetProgram: string
    cluster: ClusterKind
    dryRun?: boolean
  }) => Promise<ReplayReport | null>
}) {
  const [selected, setSelected] = useState<ExploitKey | null>(
    catalog[0]?.key ?? null,
  )
  const [targetProgram, setTargetProgram] = useState("")
  const [dryRun, setDryRun] = useState(true)

  const current = useMemo(
    () => catalog.find((c) => c.key === selected) ?? null,
    [catalog, selected],
  )

  if (catalog.length === 0) {
    return (
      <p className="text-[11.5px] text-muted-foreground">
        Loading exploit library…
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-2">
      <ul className="flex flex-col gap-1">
        {catalog.map((descriptor) => (
          <li key={descriptor.key}>
            <button
              type="button"
              onClick={() => setSelected(descriptor.key)}
              className={cn(
                "w-full rounded-md border px-2 py-1.5 text-left text-[11px] transition-colors",
                selected === descriptor.key
                  ? "border-primary/50 bg-primary/10 text-foreground"
                  : "border-border/60 bg-background/30 text-foreground/85 hover:border-primary/40",
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="truncate font-medium">{descriptor.title}</span>
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                  slot {descriptor.exploitSlot}
                </span>
              </div>
              <div className="text-[10.5px] text-muted-foreground">
                {descriptor.summary}
              </div>
            </button>
          </li>
        ))}
      </ul>
      {current ? (
        <form
          className="flex flex-col gap-2 rounded-md border border-border/60 bg-background/40 p-2"
          onSubmit={async (e) => {
            e.preventDefault()
            if (!selected || !targetProgram.trim()) return
            await onRun({
              exploit: selected,
              targetProgram: targetProgram.trim(),
              cluster,
              dryRun,
            })
          }}
        >
          <input
            value={targetProgram}
            onChange={(e) => setTargetProgram(e.target.value)}
            placeholder="Your program id (target for the replay)"
            className="rounded-md border border-border/70 bg-background/50 px-2 py-1 font-mono text-[11.5px]"
          />
          <label className="inline-flex items-center gap-2 text-[11px] text-foreground/85">
            <input
              type="checkbox"
              checked={dryRun}
              onChange={(e) => setDryRun(e.target.checked)}
            />
            Dry run (narrate only)
          </label>
          {cluster === "mainnet" ? (
            <p className="text-[11px] text-destructive">
              Replay is not permitted on mainnet — switch to mainnet fork first.
            </p>
          ) : null}
          {!clusterRunning && !dryRun ? (
            <p className="text-[11px] text-amber-400">
              Cluster is not running. Start a local or fork validator before executing a live
              replay.
            </p>
          ) : null}
          <button
            type="submit"
            disabled={
              busy ||
              !targetProgram.trim() ||
              cluster === "mainnet"
            }
            className="inline-flex items-center gap-1.5 self-start rounded-md border border-primary/50 bg-primary/15 px-2.5 py-1 text-[11px] font-medium text-primary transition-colors hover:bg-primary/25 disabled:opacity-50"
          >
            {busy ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Target className="h-3 w-3" />
            )}
            Replay exploit
          </button>
        </form>
      ) : null}
      {report ? (
        <div className="rounded-md border border-border/60 bg-background/40 px-2 py-2 text-[11px] text-muted-foreground">
          <div>
            {report.outcome.replace(/_/g, " ")} · slot {report.snapshotSlot}
          </div>
          <div className="mt-1">{report.summary}</div>
          {report.steps.length > 0 ? (
            <ul className="mt-2 flex flex-col gap-0.5">
              {report.steps.map((step) => (
                <li
                  key={`${step.stepIndex}-${step.label}`}
                  className="flex items-start gap-2"
                >
                  <RefreshCw className="mt-0.5 h-3 w-3 shrink-0 text-muted-foreground" />
                  <div className="min-w-0">
                    <div className="truncate text-foreground/85">
                      {step.label}
                    </div>
                    <div className="truncate font-mono text-[10px]">
                      {step.message}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
