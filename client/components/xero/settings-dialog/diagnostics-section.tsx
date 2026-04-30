import { useMemo, useState } from "react"
import {
  AlertTriangle,
  ArrowRight,
  Check,
  CheckCircle2,
  Clipboard,
  CircleSlash,
  LoaderCircle,
  Play,
  RotateCcw,
  Stethoscope,
  XCircle,
} from "lucide-react"
import type {
  DoctorReportRunStatus,
  OperatorActionErrorView,
} from "@/src/features/xero/use-xero-desktop-state"
import {
  xeroDoctorReportSchema,
  renderXeroDoctorReport,
  type XeroDiagnosticCheckDto,
  type XeroDiagnosticStatusDto,
  type XeroDoctorReportDto,
  type RunDoctorReportRequestDto,
  type EnvironmentCapabilityStateDto,
  type EnvironmentDiscoveryStatusDto,
  type EnvironmentProfileSummaryDto,
  type EnvironmentToolCategoryDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

interface DiagnosticsSectionProps {
  doctorReport: XeroDoctorReportDto | null
  doctorReportStatus: DoctorReportRunStatus
  doctorReportError: OperatorActionErrorView | null
  environmentDiscoveryStatus?: EnvironmentDiscoveryStatusDto | null
  environmentProfileSummary?: EnvironmentProfileSummaryDto
  onRefreshEnvironmentDiscovery?: (options?: { force?: boolean }) => Promise<EnvironmentDiscoveryStatusDto | null>
  onRunDoctorReport?: (request?: Partial<RunDoctorReportRequestDto>) => Promise<XeroDoctorReportDto>
}

type CheckGroupKey =
  | "dictationChecks"
  | "profileChecks"
  | "modelCatalogChecks"
  | "runtimeSupervisorChecks"
  | "mcpDependencyChecks"
  | "settingsDependencyChecks"

const CHECK_GROUPS: Array<{ key: CheckGroupKey; label: string }> = [
  { key: "profileChecks", label: "Providers" },
  { key: "modelCatalogChecks", label: "Model catalogs" },
  { key: "runtimeSupervisorChecks", label: "Agent runtime" },
  { key: "mcpDependencyChecks", label: "MCP dependencies" },
  { key: "settingsDependencyChecks", label: "Settings dependencies" },
  { key: "dictationChecks", label: "Dictation" },
]

const STATUS_ORDER: Record<XeroDiagnosticStatusDto, number> = {
  failed: 0,
  warning: 1,
  skipped: 2,
  passed: 3,
}

const STATUS_ICON = {
  passed: CheckCircle2,
  warning: AlertTriangle,
  failed: XCircle,
  skipped: CircleSlash,
} satisfies Record<XeroDiagnosticStatusDto, React.ElementType>

const STATUS_LABEL: Record<XeroDiagnosticStatusDto, string> = {
  passed: "Passed",
  warning: "Warning",
  failed: "Failed",
  skipped: "Skipped",
}

const STATUS_TEXT: Record<XeroDiagnosticStatusDto, string> = {
  passed: "text-success dark:text-success",
  warning: "text-warning dark:text-warning",
  failed: "text-destructive",
  skipped: "text-muted-foreground",
}

const STATUS_BG: Record<XeroDiagnosticStatusDto, string> = {
  passed: "bg-success/10",
  warning: "bg-warning/10",
  failed: "bg-destructive/10",
  skipped: "bg-muted/40",
}

const STATUS_RING: Record<XeroDiagnosticStatusDto, string> = {
  passed: "ring-success/20",
  warning: "ring-warning/25",
  failed: "ring-destructive/25",
  skipped: "ring-border/60",
}

const STATUS_ACCENT: Record<XeroDiagnosticStatusDto, string> = {
  passed: "before:bg-success/50 dark:before:bg-success/50",
  warning: "before:bg-warning/70 dark:before:bg-warning/70",
  failed: "before:bg-destructive/80",
  skipped: "before:bg-border/60",
}

const MODE_LABEL: Record<RunDoctorReportRequestDto["mode"], string> = {
  quick_local: "Quick · local checks",
  extended_network: "Extended · network checks",
}

const CATEGORY_LABEL: Record<EnvironmentToolCategoryDto, string> = {
  base_developer_tool: "Developer tools",
  package_manager: "Package managers",
  platform_package_manager: "Platform managers",
  language_runtime: "Runtimes",
  container_orchestration: "Containers",
  mobile_tooling: "Mobile",
  cloud_deployment: "Cloud",
  database_cli: "Databases",
  solana_tooling: "Solana",
  agent_ai_cli: "Agent CLIs",
}

const CAPABILITY_LABEL: Record<EnvironmentCapabilityStateDto, string> = {
  ready: "Ready",
  partial: "Partial",
  missing: "Missing",
  blocked: "Blocked",
  unknown: "Unknown",
}

export function DiagnosticsSection({
  doctorReport,
  doctorReportStatus,
  doctorReportError,
  environmentDiscoveryStatus = null,
  environmentProfileSummary = null,
  onRefreshEnvironmentDiscovery,
  onRunDoctorReport,
}: DiagnosticsSectionProps) {
  const [copied, setCopied] = useState(false)
  const parsedReport = useMemo(() => {
    if (!doctorReport) return null
    return xeroDoctorReportSchema.safeParse(doctorReport)
  }, [doctorReport])
  const report = parsedReport?.success ? parsedReport.data : null
  const isRunning = doctorReportStatus === "running"
  const canRun = Boolean(onRunDoctorReport) && !isRunning

  const runReport = (mode: RunDoctorReportRequestDto["mode"]) => {
    void onRunDoctorReport?.({ mode }).catch(() => undefined)
  }

  const refreshEnvironment = () => {
    void onRefreshEnvironmentDiscovery?.({ force: true }).catch(() => undefined)
  }

  const copyReport = () => {
    if (!report || typeof navigator === "undefined" || !navigator.clipboard) return
    void navigator.clipboard.writeText(renderXeroDoctorReport(report, "json")).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1600)
    })
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Diagnostics"
        description="Run local provider, runtime, MCP, and settings checks without exposing secrets or local paths."
      />

      <EnvironmentProfilePanel
        status={environmentDiscoveryStatus}
        summary={environmentProfileSummary}
        canRefresh={Boolean(onRefreshEnvironmentDiscovery)}
        onRefresh={refreshEnvironment}
      />

      {doctorReportError ? (
        <Alert variant="destructive" className="rounded-md px-3 py-2 text-[12px]">
          <AlertTriangle className="h-3.5 w-3.5" />
          <AlertTitle className="text-[12px]">Doctor report failed</AlertTitle>
          <AlertDescription className="text-[12px]">
            <p>{doctorReportError.message}</p>
            {doctorReportError.code ? <p className="font-mono text-[11px]">code: {doctorReportError.code}</p> : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {parsedReport && !parsedReport.success ? (
        <Alert variant="destructive" className="rounded-md px-3 py-2 text-[12px]">
          <XCircle className="h-3.5 w-3.5" />
          <AlertTitle className="text-[12px]">Malformed report</AlertTitle>
          <AlertDescription className="text-[12px]">
            <p>The desktop backend returned diagnostics that failed the shared contract.</p>
          </AlertDescription>
        </Alert>
      ) : null}

      {!report ? (
        <EmptyState
          isRunning={isRunning}
          canRun={canRun}
          onRun={runReport}
        />
      ) : (
        <ReportView
          report={report}
          copied={copied}
          isRunning={isRunning}
          canRun={canRun}
          onCopy={copyReport}
          onRun={runReport}
        />
      )}
    </div>
  )
}

function EmptyState({
  isRunning,
  canRun,
  onRun,
}: {
  isRunning: boolean
  canRun: boolean
  onRun: (mode: RunDoctorReportRequestDto["mode"]) => void
}) {
  return (
    <div className="flex flex-col items-center gap-5 rounded-xl border border-dashed border-border/70 bg-secondary/15 px-6 py-10 text-center">
      <div className="flex size-11 items-center justify-center rounded-full border border-border/60 bg-background/60 text-muted-foreground">
        {isRunning ? (
          <LoaderCircle className="h-5 w-5 animate-spin" />
        ) : (
          <Stethoscope className="h-5 w-5" />
        )}
      </div>
      <div className="flex max-w-sm flex-col gap-1.5">
        <p className="text-[14px] font-semibold text-foreground">
          {isRunning ? "Running diagnostics" : "No diagnostics yet"}
        </p>
        <p className="text-[12.5px] leading-[1.55] text-muted-foreground">
          {isRunning
            ? "Xero is collecting current desktop state."
            : "Run a doctor report to verify providers, agent runtime, MCP, and settings dependencies."}
        </p>
      </div>
      <div className="grid w-full max-w-md grid-cols-1 gap-2 sm:grid-cols-2">
        <ModeCard
          title="Quick"
          body="Local checks only — no network calls."
          icon={RotateCcw}
          variant="outline"
          disabled={!canRun}
          onClick={() => onRun("quick_local")}
        />
        <ModeCard
          title="Extended"
          body="Includes provider reachability over the network."
          icon={Play}
          variant="default"
          disabled={!canRun}
          onClick={() => onRun("extended_network")}
        />
      </div>
    </div>
  )
}

function EnvironmentProfilePanel({
  status,
  summary,
  canRefresh,
  onRefresh,
}: {
  status: EnvironmentDiscoveryStatusDto | null
  summary: EnvironmentProfileSummaryDto
  canRefresh: boolean
  onRefresh: () => void
}) {
  const presentTools = summary?.tools.filter((tool) => tool.present) ?? []
  const missingTools = summary?.tools.filter((tool) => !tool.present) ?? []
  const readyCapabilities = summary?.capabilities.filter((capability) => capability.state === "ready") ?? []
  const attentionCapabilities =
    summary?.capabilities.filter((capability) => capability.state !== "ready").slice(0, 4) ?? []
  const highlightedTools = presentTools.slice(0, 8)
  const isProbing = status?.status === "probing"

  return (
    <section className="rounded-xl border border-border/70 bg-card/35">
      <header className="flex flex-wrap items-start justify-between gap-3 border-b border-border/50 px-5 py-4">
        <div className="min-w-0">
          <h4 className="text-[13px] font-semibold tracking-tight text-foreground">Developer environment</h4>
          <p className="mt-1 text-[11.5px] leading-[1.5] text-muted-foreground">
            {summary
              ? `${summary.platform.osKind} ${summary.platform.arch} · ${
                  summary.refreshedAt ? formatTimestamp(summary.refreshedAt) : "Not refreshed yet"
                }`
              : status?.status === "probing"
                ? "Discovery is running in the background."
                : "No environment profile has been recorded yet."}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={status?.stale ? "destructive" : "secondary"} className="rounded-md text-[11px]">
            {status?.stale ? "Stale" : summary ? CAPABILITY_LABEL.ready : "Pending"}
          </Badge>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={!canRefresh || isProbing}
            onClick={onRefresh}
          >
            {isProbing ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
            Refresh
          </Button>
        </div>
      </header>

      <div className="grid gap-0 border-b border-border/50 sm:grid-cols-3">
        <EnvironmentMetric label="Present tools" value={presentTools.length} />
        <EnvironmentMetric label="Missing tools" value={missingTools.length} />
        <EnvironmentMetric label="Ready capabilities" value={readyCapabilities.length} isLast />
      </div>

      {summary ? (
        <div className="grid gap-5 px-5 py-4 md:grid-cols-[1.2fr_0.8fr]">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
              Detected tools
            </p>
            {highlightedTools.length > 0 ? (
              <div className="mt-3 flex flex-wrap gap-2">
                {highlightedTools.map((tool) => (
                  <span
                    key={tool.id}
                    className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/60 bg-background/45 px-2 py-1 text-[11.5px] text-foreground"
                    title={tool.displayPath ?? tool.id}
                  >
                    <span className="font-medium">{tool.id}</span>
                    {tool.version ? (
                      <span className="truncate text-muted-foreground">{tool.version}</span>
                    ) : null}
                    <span className="text-muted-foreground/70">{CATEGORY_LABEL[tool.category]}</span>
                  </span>
                ))}
              </div>
            ) : (
              <p className="mt-2 text-[12px] text-muted-foreground">No installed developer tools were detected yet.</p>
            )}
          </div>

          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
              Capability attention
            </p>
            {attentionCapabilities.length > 0 ? (
              <div className="mt-3 flex flex-col gap-2">
                {attentionCapabilities.map((capability) => (
                  <div key={capability.id} className="rounded-md border border-border/60 bg-background/35 px-3 py-2">
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate text-[12px] font-medium text-foreground">{capability.id}</span>
                      <Badge variant="outline" className="shrink-0 rounded-md text-[10.5px]">
                        {CAPABILITY_LABEL[capability.state]}
                      </Badge>
                    </div>
                    {capability.message ? (
                      <p className="mt-1 text-[11.5px] leading-[1.45] text-muted-foreground">{capability.message}</p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : (
              <p className="mt-2 text-[12px] text-muted-foreground">Recorded capabilities are ready or not yet available.</p>
            )}
          </div>
        </div>
      ) : null}

      {status?.diagnostics.length ? (
        <div className="border-t border-border/50 px-5 py-3">
          <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
            Environment diagnostics
          </p>
          <div className="mt-2 flex flex-col gap-1.5">
            {status.diagnostics.slice(0, 4).map((diagnostic) => (
              <p key={`${diagnostic.code}-${diagnostic.message}`} className="text-[12px] leading-[1.45] text-muted-foreground">
                <span className="font-medium text-foreground">{diagnostic.code}</span>
                <span className="mx-1 text-muted-foreground/50">·</span>
                {diagnostic.message}
              </p>
            ))}
          </div>
        </div>
      ) : null}
    </section>
  )
}

function EnvironmentMetric({
  label,
  value,
  isLast = false,
}: {
  label: string
  value: number
  isLast?: boolean
}) {
  return (
    <div className={cn("px-5 py-3", !isLast && "border-b border-border/40 sm:border-b-0 sm:border-r")}>
      <p className="text-[18px] font-semibold leading-none tabular-nums text-foreground">{value}</p>
      <p className="mt-1 text-[10.5px] font-medium uppercase tracking-[0.1em] text-muted-foreground">{label}</p>
    </div>
  )
}

function ModeCard({
  title,
  body,
  icon: Icon,
  variant,
  disabled,
  onClick,
}: {
  title: string
  body: string
  icon: React.ElementType
  variant: "default" | "outline"
  disabled: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "group flex flex-col items-start gap-1.5 rounded-lg border px-3.5 py-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60",
        variant === "default"
          ? "border-primary/40 bg-primary/10 hover:border-primary/60 hover:bg-primary/15"
          : "border-border/70 bg-card/40 hover:border-border hover:bg-card/60",
      )}
    >
      <span className="flex items-center gap-2 text-[12.5px] font-semibold text-foreground">
        <Icon className="h-3.5 w-3.5 text-muted-foreground group-hover:text-foreground" />
        {title}
      </span>
      <span className="text-[11.5px] leading-[1.5] text-muted-foreground">{body}</span>
    </button>
  )
}

function ReportView({
  report,
  copied,
  isRunning,
  canRun,
  onCopy,
  onRun,
}: {
  report: XeroDoctorReportDto
  copied: boolean
  isRunning: boolean
  canRun: boolean
  onCopy: () => void
  onRun: (mode: RunDoctorReportRequestDto["mode"]) => void
}) {
  const populatedGroups = CHECK_GROUPS.filter(({ key }) => report[key].length > 0)
  const skippedGroupCount = CHECK_GROUPS.length - populatedGroups.length

  return (
    <div className="flex flex-col gap-6">
      <ReportSummary
        report={report}
        copied={copied}
        isRunning={isRunning}
        canRun={canRun}
        onCopy={onCopy}
        onRun={onRun}
      />

      {populatedGroups.length > 0 ? (
        <div className="flex flex-col gap-6">
          {populatedGroups.map(({ key, label }) => (
            <CheckGroup key={key} label={label} checks={report[key]} />
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-border/60 bg-card/30 px-4 py-6 text-center text-[12.5px] text-muted-foreground">
          No checks returned for this run.
        </p>
      )}

      {skippedGroupCount > 0 && populatedGroups.length > 0 ? (
        <p className="text-[11.5px] text-muted-foreground/70">
          {skippedGroupCount} group{skippedGroupCount === 1 ? "" : "s"} returned no checks for this run.
        </p>
      ) : null}
    </div>
  )
}

function ReportSummary({
  report,
  copied,
  isRunning,
  canRun,
  onCopy,
  onRun,
}: {
  report: XeroDoctorReportDto
  copied: boolean
  isRunning: boolean
  canRun: boolean
  onCopy: () => void
  onRun: (mode: RunDoctorReportRequestDto["mode"]) => void
}) {
  const generatedAt = useMemo(() => formatTimestamp(report.generatedAt), [report.generatedAt])

  return (
    <section className="overflow-hidden rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <header className="flex flex-wrap items-start justify-between gap-3 px-5 py-4">
        <div className="min-w-0">
          <h4 className="text-[13px] font-semibold tracking-tight text-foreground">Report summary</h4>
          <p className="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[11.5px] text-muted-foreground">
            <span>{MODE_LABEL[report.mode]}</span>
            <span aria-hidden className="text-muted-foreground/40">·</span>
            <span>{generatedAt}</span>
            <span aria-hidden className="text-muted-foreground/40">·</span>
            <span>
              <span className="tabular-nums text-foreground/80">{report.summary.total}</span>
              <span className="ml-1 text-muted-foreground/80">checks</span>
            </span>
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
            disabled={!canRun}
            onClick={() => onRun("quick_local")}
          >
            {isRunning ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RotateCcw className="h-3.5 w-3.5" />
            )}
            Quick
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={!canRun}
            onClick={() => onRun("extended_network")}
          >
            <Play className="h-3.5 w-3.5" />
            Extended
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
            onClick={onCopy}
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-success dark:text-success" />
            ) : (
              <Clipboard className="h-3.5 w-3.5" />
            )}
            {copied ? "Copied" : "Copy JSON"}
          </Button>
        </div>
      </header>

      <div className="grid grid-cols-2 border-t border-border/50 sm:grid-cols-4">
        <SummaryTile tone="passed" value={report.summary.passed} isLast={false} />
        <SummaryTile tone="warning" value={report.summary.warnings} isLast={false} />
        <SummaryTile tone="failed" value={report.summary.failed} isLast={false} />
        <SummaryTile tone="skipped" value={report.summary.skipped} isLast />
      </div>
    </section>
  )
}

function SummaryTile({
  tone,
  value,
  isLast,
}: {
  tone: XeroDiagnosticStatusDto
  value: number
  isLast: boolean
}) {
  const Icon = STATUS_ICON[tone]
  const isZero = value === 0
  return (
    <div
      className={cn(
        "flex items-center gap-3 px-4 py-3 sm:py-3.5",
        !isLast && "border-r border-border/40",
        // Stack borders for the 2-col fallback (sm:grid-cols-4 layers over)
      )}
    >
      <span
        className={cn(
          "flex size-8 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
          isZero ? "bg-muted/30 ring-border/50" : cn(STATUS_BG[tone], STATUS_RING[tone]),
        )}
        aria-hidden
      >
        <Icon className={cn("h-4 w-4", isZero ? "text-muted-foreground/50" : STATUS_TEXT[tone])} />
      </span>
      <div className="flex min-w-0 flex-col">
        <span
          className={cn(
            "text-[18px] font-semibold leading-none tabular-nums",
            isZero ? "text-foreground/40" : "text-foreground",
          )}
        >
          {value}
        </span>
        <span className="mt-1 text-[10.5px] font-medium uppercase tracking-[0.1em] text-muted-foreground">
          {STATUS_LABEL[tone]}
        </span>
      </div>
    </div>
  )
}

function CheckGroup({
  label,
  checks,
}: {
  label: string
  checks: XeroDiagnosticCheckDto[]
}) {
  const sorted = useMemo(
    () => [...checks].sort((a, b) => STATUS_ORDER[a.status] - STATUS_ORDER[b.status]),
    [checks],
  )
  const counts = useMemo(() => {
    const acc: Record<XeroDiagnosticStatusDto, number> = {
      passed: 0,
      warning: 0,
      failed: 0,
      skipped: 0,
    }
    for (const check of checks) acc[check.status] += 1
    return acc
  }, [checks])

  return (
    <section className="flex flex-col gap-2.5">
      <header className="flex items-center justify-between gap-3 px-1">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
          {label}
        </h4>
        <div className="flex items-center gap-1.5">
          {(["failed", "warning", "skipped", "passed"] as const).map((tone) =>
            counts[tone] > 0 ? <CountChip key={tone} tone={tone} value={counts[tone]} /> : null,
          )}
        </div>
      </header>
      <div className="flex flex-col gap-1.5">
        {sorted.map((check, index) => (
          <CheckRow key={`${check.checkId}-${index}`} check={check} />
        ))}
      </div>
    </section>
  )
}

function CountChip({ tone, value }: { tone: XeroDiagnosticStatusDto; value: number }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium tabular-nums ring-1 ring-inset",
        STATUS_BG[tone],
        STATUS_RING[tone],
        STATUS_TEXT[tone],
      )}
      title={`${value} ${STATUS_LABEL[tone].toLowerCase()}`}
    >
      <span
        className={cn("size-1.5 rounded-full", {
          "bg-success dark:bg-success": tone === "passed",
          "bg-warning dark:bg-warning": tone === "warning",
          "bg-destructive": tone === "failed",
          "bg-muted-foreground/60": tone === "skipped",
        })}
        aria-hidden
      />
      {value}
    </span>
  )
}

function CheckRow({ check }: { check: XeroDiagnosticCheckDto }) {
  const Icon = STATUS_ICON[check.status]
  const meta = buildMetaParts(check)

  return (
    <div
      className={cn(
        "relative overflow-hidden rounded-lg border border-border/60 bg-card/30",
        "before:absolute before:inset-y-0 before:left-0 before:w-[3px]",
        STATUS_ACCENT[check.status],
      )}
    >
      <div className="flex items-start gap-3 px-4 py-3 pl-[18px]">
        <span
          className={cn(
            "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
            STATUS_BG[check.status],
            STATUS_RING[check.status],
          )}
          aria-hidden
        >
          <Icon className={cn("h-3 w-3", STATUS_TEXT[check.status])} />
        </span>

        <div className="min-w-0 flex-1">
          <p className="text-[12.5px] font-medium leading-[1.5] text-foreground">
            {check.message}
          </p>

          {meta.length > 0 || check.code ? (
            <div className="mt-1.5 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-[11px] text-muted-foreground/80">
              {meta.map((part, index) => (
                <span key={`m-${index}`} className="inline-flex items-center gap-1">
                  {index > 0 ? <span aria-hidden className="text-muted-foreground/30">·</span> : null}
                  {part.label ? (
                    <>
                      <span className="text-muted-foreground/80">{part.label}</span>
                      <span className="font-mono text-[10.5px] text-foreground/70">{part.value}</span>
                    </>
                  ) : (
                    <span className="rounded-sm bg-muted/50 px-1 py-px font-mono text-[10px] uppercase tracking-wide text-foreground/70">
                      {part.value}
                    </span>
                  )}
                </span>
              ))}
              {meta.length > 0 ? <span aria-hidden className="text-muted-foreground/30">·</span> : null}
              <span className="font-mono text-[10.5px] text-muted-foreground/70">{check.code}</span>
            </div>
          ) : null}

          {check.remediation ? (
            <div
              className={cn(
                "mt-2.5 flex items-start gap-2 rounded-md px-2.5 py-1.5 text-[11.5px] leading-[1.5]",
                check.status === "failed"
                  ? "bg-destructive/8 text-foreground/85 ring-1 ring-inset ring-destructive/20"
                  : "bg-warning/8 text-foreground/85 ring-1 ring-inset ring-warning/20",
              )}
            >
              <ArrowRight
                className={cn(
                  "mt-0.5 h-3 w-3 shrink-0",
                  check.status === "failed"
                    ? "text-destructive"
                    : "text-warning dark:text-warning",
                )}
                aria-hidden
              />
              <p>{check.remediation}</p>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}

interface MetaPart {
  label: string
  value: string
}

function buildMetaParts(check: XeroDiagnosticCheckDto): MetaPart[] {
  const parts: MetaPart[] = []
  if (check.affectedProviderId) parts.push({ label: "provider", value: check.affectedProviderId })
  if (check.retryable) parts.push({ label: "", value: "retryable" })
  if (check.redacted) parts.push({ label: "", value: "redacted" })
  return parts
}

function formatTimestamp(iso: string): string {
  const parsed = new Date(iso)
  if (Number.isNaN(parsed.getTime())) return iso
  return parsed.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })
}
