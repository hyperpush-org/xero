import { useDeferredValue, useMemo, useState, type ElementType, type ReactNode } from "react"
import {
  AlertCircle,
  AlertTriangle,
  ArrowRight,
  Check,
  CheckCircle2,
  ChevronDown,
  Clipboard,
  CircleSlash,
  Info,
  LoaderCircle,
  Plus,
  Play,
  RotateCcw,
  Stethoscope,
  X,
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
  type EnvironmentProbeReportDto,
  type EnvironmentProfileSummaryDto,
  type EnvironmentToolCategoryDto,
  type EnvironmentToolSummaryDto,
  type VerifyUserToolRequestDto,
  type VerifyUserToolResponseDto,
  verifyUserToolRequestSchema,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

interface DiagnosticsSectionProps {
  doctorReport: XeroDoctorReportDto | null
  doctorReportStatus: DoctorReportRunStatus
  doctorReportError: OperatorActionErrorView | null
  environmentDiscoveryStatus?: EnvironmentDiscoveryStatusDto | null
  environmentProfileSummary?: EnvironmentProfileSummaryDto
  onRefreshEnvironmentDiscovery?: (options?: { force?: boolean }) => Promise<EnvironmentDiscoveryStatusDto | null>
  onVerifyUserEnvironmentTool?: (request: VerifyUserToolRequestDto) => Promise<VerifyUserToolResponseDto | null>
  onSaveUserEnvironmentTool?: (request: VerifyUserToolRequestDto) => Promise<EnvironmentProbeReportDto | null>
  onRemoveUserEnvironmentTool?: (id: string) => Promise<EnvironmentProbeReportDto | null>
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
} satisfies Record<XeroDiagnosticStatusDto, ElementType>

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

const MODE_LABEL: Record<RunDoctorReportRequestDto["mode"], string> = {
  quick_local: "Quick · local checks",
  extended_network: "Extended · network checks",
}

const CAPABILITY_LABEL: Record<EnvironmentCapabilityStateDto, string> = {
  ready: "Ready",
  partial: "Partial",
  missing: "Missing",
  blocked: "Blocked",
  unknown: "Unknown",
}

const TOOL_CATEGORY_LABEL: Record<EnvironmentToolCategoryDto, string> = {
  base_developer_tool: "Developer tools",
  package_manager: "Package managers",
  platform_package_manager: "Platform package managers",
  language_runtime: "Languages & runtimes",
  container_orchestration: "Containers & orchestration",
  mobile_tooling: "Mobile",
  cloud_deployment: "Cloud & deployment",
  database_cli: "Databases",
  solana_tooling: "Solana",
  agent_ai_cli: "AI & agent CLIs",
  editor: "Editors",
  build_tool: "Build tools",
  linter: "Linters & formatters",
  version_manager: "Version managers",
  iac_tool: "Infrastructure",
  shell_utility: "Shell utilities",
}

const TOOL_CATEGORY_ORDER: EnvironmentToolCategoryDto[] = [
  "base_developer_tool",
  "language_runtime",
  "package_manager",
  "platform_package_manager",
  "version_manager",
  "build_tool",
  "linter",
  "editor",
  "shell_utility",
  "container_orchestration",
  "iac_tool",
  "cloud_deployment",
  "database_cli",
  "mobile_tooling",
  "solana_tooling",
  "agent_ai_cli",
]

const HEADLINER_TOOL_PRIORITY = [
  "git",
  "node",
  "rustc",
  "python3",
  "python",
  "go",
  "java",
  "swift",
  "ruby",
  "dotnet",
  "php",
  "deno",
  "bun",
  "pnpm",
  "npm",
  "yarn",
  "uv",
  "pip",
  "brew",
  "docker",
  "kubectl",
  "claude",
  "codex",
  "aider",
  "gemini",
  "ollama",
]

const HEADLINER_LIMIT = 8

export function DiagnosticsSection({
  doctorReport,
  doctorReportStatus,
  doctorReportError,
  environmentDiscoveryStatus = null,
  environmentProfileSummary = null,
  onRefreshEnvironmentDiscovery,
  onVerifyUserEnvironmentTool,
  onSaveUserEnvironmentTool,
  onRemoveUserEnvironmentTool,
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
        onVerifyUserEnvironmentTool={onVerifyUserEnvironmentTool}
        onSaveUserEnvironmentTool={onSaveUserEnvironmentTool}
        onRemoveUserEnvironmentTool={onRemoveUserEnvironmentTool}
      />

      {doctorReportError ? (
        <Alert variant="destructive" className="rounded-md px-3.5 py-2.5 text-[13px]">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle className="text-[13px] font-semibold">Doctor report failed</AlertTitle>
          <AlertDescription className="text-[12.5px] leading-[1.5]">
            <p>{doctorReportError.message}</p>
            {doctorReportError.code ? <p className="font-mono text-[12px]">code: {doctorReportError.code}</p> : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {parsedReport && !parsedReport.success ? (
        <Alert variant="destructive" className="rounded-md px-3.5 py-2.5 text-[13px]">
          <XCircle className="h-4 w-4" />
          <AlertTitle className="text-[13px] font-semibold">Malformed report</AlertTitle>
          <AlertDescription className="text-[12.5px] leading-[1.5]">
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
    <div className="flex flex-col items-center gap-5 rounded-lg border border-border/60 bg-secondary/10 px-6 py-10 text-center">
      <div
        className={cn(
          "flex h-11 w-11 items-center justify-center rounded-full border border-border/60 bg-card/60",
          isRunning && "animate-pulse",
        )}
      >
        {isRunning ? (
          <LoaderCircle className="h-5 w-5 animate-spin text-muted-foreground" />
        ) : (
          <Stethoscope className="h-5 w-5 text-muted-foreground" />
        )}
      </div>
      <div className="flex max-w-sm flex-col gap-1">
        <p className="text-[14px] font-semibold tracking-tight text-foreground">
          {isRunning ? "Running diagnostics" : "No diagnostics yet"}
        </p>
        <p className="text-[12.5px] leading-[1.5] text-muted-foreground">
          {isRunning
            ? "Xero is collecting current desktop state."
            : "Run a check to verify providers, runtime, MCP, and settings."}
        </p>
      </div>
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-9 gap-1.5 px-3.5 text-[12.5px]"
          disabled={!canRun}
          onClick={() => onRun("quick_local")}
        >
          <RotateCcw className="h-3.5 w-3.5" />
          Quick
        </Button>
        <Button
          type="button"
          size="sm"
          className="h-9 gap-1.5 px-3.5 text-[12.5px]"
          disabled={!canRun}
          onClick={() => onRun("extended_network")}
        >
          <Play className="h-3.5 w-3.5" />
          Extended
        </Button>
      </div>
    </div>
  )
}

function EnvironmentProfilePanel({
  status,
  summary,
  canRefresh,
  onRefresh,
  onVerifyUserEnvironmentTool,
  onSaveUserEnvironmentTool,
  onRemoveUserEnvironmentTool,
}: {
  status: EnvironmentDiscoveryStatusDto | null
  summary: EnvironmentProfileSummaryDto
  canRefresh: boolean
  onRefresh: () => void
  onVerifyUserEnvironmentTool?: (request: VerifyUserToolRequestDto) => Promise<VerifyUserToolResponseDto | null>
  onSaveUserEnvironmentTool?: (request: VerifyUserToolRequestDto) => Promise<EnvironmentProbeReportDto | null>
  onRemoveUserEnvironmentTool?: (id: string) => Promise<EnvironmentProbeReportDto | null>
}) {
  const [showAllTools, setShowAllTools] = useState(false)
  const [addToolOpen, setAddToolOpen] = useState(false)
  const [removingToolId, setRemovingToolId] = useState<string | null>(null)
  const deferredSummary = useDeferredValue(summary)
  const presentTools = useMemo(
    () => deferredSummary?.tools.filter((tool) => tool.present) ?? [],
    [deferredSummary],
  )
  const attentionCapabilities = useMemo(
    () =>
      deferredSummary?.capabilities
        .filter((capability) => capability.state !== "ready")
        .slice(0, 4) ?? [],
    [deferredSummary],
  )
  const headlinerTools = useMemo(() => pickHeadlinerTools(presentTools), [presentTools])
  const headlinerIds = useMemo(() => new Set(headlinerTools.map((tool) => tool.id)), [headlinerTools])
  const remainingTools = useMemo(
    () => presentTools.filter((tool) => !headlinerIds.has(tool.id)),
    [presentTools, headlinerIds],
  )
  const groupedRemaining = useMemo(() => groupToolsByCategory(remainingTools), [remainingTools])
  const isProbing = status?.status === "probing"
  const isStale = Boolean(status?.stale)
  const canMutateUserTools = Boolean(onVerifyUserEnvironmentTool && onSaveUserEnvironmentTool)

  if (!summary && !isProbing && !status?.diagnostics.length) {
    return null
  }

  const removeTool = (tool: EnvironmentToolSummaryDto) => {
    if (!tool.custom || !onRemoveUserEnvironmentTool || removingToolId) return
    const confirmed = typeof window === "undefined" || window.confirm(`Remove custom tool "${tool.id}"?`)
    if (!confirmed) return
    setRemovingToolId(tool.id)
    void onRemoveUserEnvironmentTool(tool.id)
      .catch(() => undefined)
      .finally(() => setRemovingToolId(null))
  }

  return (
    <section className="overflow-hidden rounded-lg border border-border/60">
      <header className="flex items-center justify-between gap-3 border-b border-border/40 bg-secondary/10 px-4 py-3">
        <div className="min-w-0">
          <p className="text-[13.5px] font-semibold tracking-tight text-foreground">Developer environment</p>
          {summary ? (
            <p className="mt-0.5 truncate text-[12px] text-muted-foreground">
              {summary.platform.osKind} {summary.platform.arch}
              {presentTools.length > 0 ? ` · ${presentTools.length} tool${presentTools.length === 1 ? "" : "s"}` : ""}
              {summary.refreshedAt ? ` · ${formatTimestamp(summary.refreshedAt)}` : ""}
            </p>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {isStale ? (
            <span className="inline-flex h-[20px] items-center rounded-full border border-warning/30 bg-warning/[0.08] px-2 text-[11px] font-medium text-warning">
              Stale
            </span>
          ) : null}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
            disabled={!canRefresh || isProbing}
            onClick={onRefresh}
          >
            {isProbing ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
            Refresh
          </Button>
        </div>
      </header>

      {summary ? (
        <div className="flex flex-col gap-3.5 px-4 py-3.5">
          {headlinerTools.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {headlinerTools.map((tool) => (
                <ToolChip
                  key={tool.id}
                  tool={tool}
                  removing={removingToolId === tool.id}
                  onRemove={tool.custom ? removeTool : undefined}
                />
              ))}
            </div>
          ) : null}

          {groupedRemaining.length > 0 ? (
            <Collapsible open={showAllTools} onOpenChange={setShowAllTools}>
              <CollapsibleTrigger
                className={cn(
                  "group inline-flex items-center gap-1.5 self-start rounded-md text-[12.5px] font-medium",
                  "text-muted-foreground transition-colors hover:text-foreground",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                )}
              >
                <ChevronDown
                  className={cn(
                    "h-3.5 w-3.5 transition-transform motion-fast",
                    showAllTools ? "rotate-0" : "-rotate-90",
                  )}
                  aria-hidden
                />
                {showAllTools ? "Hide" : "Show"} all {presentTools.length} detected tools
              </CollapsibleTrigger>
              <CollapsibleContent className="mt-3 flex flex-col gap-3.5">
                {groupedRemaining.map((group) => (
                  <div key={group.key} className="flex flex-col gap-2">
                    <p className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/80">
                      {TOOL_CATEGORY_LABEL[group.key] ?? group.key}
                      <span className="ml-1.5 font-normal text-muted-foreground/50">{group.tools.length}</span>
                    </p>
                    <div className="flex flex-wrap gap-1.5">
                      {group.tools.map((tool) => (
                        <ToolChip
                          key={tool.id}
                          tool={tool}
                          removing={removingToolId === tool.id}
                          onRemove={tool.custom ? removeTool : undefined}
                        />
                      ))}
                    </div>
                  </div>
                ))}
              </CollapsibleContent>
            </Collapsible>
          ) : null}

          <div className="flex items-center justify-between gap-3 border-t border-border/40 pt-3">
            <p className="text-[12.5px] text-muted-foreground">
              Add a verified CLI that is not in the built-in catalog.
            </p>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-8 gap-1.5 text-[12.5px]"
              disabled={!canMutateUserTools}
              onClick={() => setAddToolOpen(true)}
            >
              <Plus className="h-3.5 w-3.5" />
              Add tool
            </Button>
          </div>
        </div>
      ) : null}

      {summary ? (
        <AddToolDialog
          open={addToolOpen}
          onOpenChange={setAddToolOpen}
          onVerify={onVerifyUserEnvironmentTool}
          onSave={onSaveUserEnvironmentTool}
        />
      ) : null}

      {attentionCapabilities.length > 0 ? (
        <ul className="divide-y divide-border/40 border-t border-border/40">
          {attentionCapabilities.map((capability) => {
            const tone = capability.state === "blocked" || capability.state === "missing" ? "warning" : "muted"
            return (
              <li
                key={capability.id}
                className="flex items-start justify-between gap-3 px-4 py-2.5"
              >
                <div className="flex min-w-0 items-start gap-2.5">
                  <AlertCircle
                    className={cn(
                      "mt-[1px] h-4 w-4 shrink-0",
                      tone === "warning" ? "text-warning" : "text-muted-foreground/70",
                    )}
                    aria-hidden
                  />
                  <div className="min-w-0">
                    <p className="truncate text-[12.5px] font-medium text-foreground">{capability.id}</p>
                    {capability.message ? (
                      <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">
                        {capability.message}
                      </p>
                    ) : null}
                  </div>
                </div>
                <span
                  className={cn(
                    "inline-flex h-[20px] shrink-0 items-center rounded-full border px-2 text-[11px] font-medium",
                    tone === "warning"
                      ? "border-warning/30 bg-warning/[0.08] text-warning"
                      : "border-border bg-secondary/60 text-foreground/70",
                  )}
                >
                  {CAPABILITY_LABEL[capability.state]}
                </span>
              </li>
            )
          })}
        </ul>
      ) : null}

      {status?.diagnostics.length ? (
        <ul className="divide-y divide-border/40 border-t border-border/40">
          {status.diagnostics.slice(0, 4).map((diagnostic) => (
            <li
              key={`${diagnostic.code}-${diagnostic.message}`}
              className="flex items-start gap-2.5 px-4 py-2.5"
            >
              <Info className="mt-[1px] h-4 w-4 shrink-0 text-muted-foreground/70" aria-hidden />
              <div className="min-w-0 text-[12.5px] leading-[1.5] text-muted-foreground">
                <span className="font-mono text-[11.5px] text-foreground/80">{diagnostic.code}</span>
                <span className="mx-1.5 text-muted-foreground/40">·</span>
                {diagnostic.message}
              </div>
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  )
}

function ToolChip({
  tool,
  removing = false,
  onRemove,
}: {
  tool: EnvironmentToolSummaryDto
  removing?: boolean
  onRemove?: (tool: EnvironmentToolSummaryDto) => void
}) {
  const hoverDetail = [tool.version, tool.displayPath].filter(Boolean).join("\n") || tool.id
  return (
    <span
      className={cn(
        "group inline-flex h-[26px] items-center gap-1 rounded-md border border-border/60 bg-secondary/40 text-[12px] font-medium text-foreground",
        onRemove ? "pl-2.5 pr-1" : "px-2.5",
        removing && "opacity-60",
      )}
      title={hoverDetail}
    >
      <span>{tool.id}</span>
      {onRemove ? (
        <button
          type="button"
          className="inline-flex h-[18px] w-[18px] items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring group-hover:opacity-100"
          disabled={removing}
          aria-label={`Remove ${tool.id}`}
          title={`Remove ${tool.id}`}
          onClick={(event) => {
            event.stopPropagation()
            onRemove(tool)
          }}
        >
          {removing ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <X className="h-3.5 w-3.5" />}
        </button>
      ) : null}
    </span>
  )
}

function AddToolDialog({
  open,
  onOpenChange,
  onVerify,
  onSave,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onVerify?: (request: VerifyUserToolRequestDto) => Promise<VerifyUserToolResponseDto | null>
  onSave?: (request: VerifyUserToolRequestDto) => Promise<EnvironmentProbeReportDto | null>
}) {
  const [id, setId] = useState("")
  const [command, setCommand] = useState("")
  const [argsText, setArgsText] = useState("--version")
  const [category, setCategory] = useState<EnvironmentToolCategoryDto>("base_developer_tool")
  const [verification, setVerification] = useState<VerifyUserToolResponseDto | null>(null)
  const [verifiedRequestKey, setVerifiedRequestKey] = useState<string | null>(null)
  const [formError, setFormError] = useState<string | null>(null)
  const [isVerifying, setIsVerifying] = useState(false)
  const [isSaving, setIsSaving] = useState(false)

  const request = useMemo(
    () => ({
      id: id.trim(),
      category,
      command: command.trim(),
      args: parseVersionArgs(argsText),
    }),
    [argsText, category, command, id],
  )
  const requestKey = useMemo(() => JSON.stringify(request), [request])
  const canSave =
    Boolean(onSave) &&
    verification?.record.probeStatus === "ok" &&
    verification.record.present &&
    verifiedRequestKey === requestKey &&
    !isSaving
  const consentCommand = [command.trim() || "<command>", ...parseVersionArgs(argsText)].join(" ")

  const reset = () => {
    setId("")
    setCommand("")
    setArgsText("--version")
    setCategory("base_developer_tool")
    setVerification(null)
    setVerifiedRequestKey(null)
    setFormError(null)
    setIsVerifying(false)
    setIsSaving(false)
  }

  const close = (nextOpen: boolean) => {
    onOpenChange(nextOpen)
    if (!nextOpen) reset()
  }

  const verify = () => {
    const parsed = verifyUserToolRequestSchema.safeParse(request)
    if (!parsed.success) {
      setFormError(parsed.error.issues[0]?.message ?? "Check the custom tool fields.")
      setVerification(null)
      setVerifiedRequestKey(null)
      return
    }
    if (!onVerify) return
    setFormError(null)
    setVerification(null)
    setVerifiedRequestKey(null)
    setIsVerifying(true)
    void onVerify(parsed.data)
      .then((response) => {
        if (!response) return
        setVerification(response)
        setVerifiedRequestKey(JSON.stringify(parsed.data))
      })
      .catch((error) => {
        setFormError(error instanceof Error ? error.message : "Verification failed.")
      })
      .finally(() => setIsVerifying(false))
  }

  const save = () => {
    const parsed = verifyUserToolRequestSchema.safeParse(request)
    if (!parsed.success || !onSave || !canSave) return
    setIsSaving(true)
    setFormError(null)
    void onSave(parsed.data)
      .then((report) => {
        if (report) close(false)
      })
      .catch((error) => {
        setFormError(error instanceof Error ? error.message : "Save failed.")
      })
      .finally(() => setIsSaving(false))
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="max-w-md gap-4 rounded-lg p-5">
        <DialogHeader className="gap-1.5">
          <DialogTitle className="text-[15px] font-semibold tracking-tight">Add developer tool</DialogTitle>
          <DialogDescription className="text-[12.5px] leading-[1.55]">
            Xero will run <span className="font-mono text-foreground">{consentCommand}</span> to verify the tool. The first non-empty line of output with secrets redacted is stored as the version.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3">
          <FieldRow label="Tool name">
            <Input
              value={id}
              onChange={(event) => setId(event.target.value.toLowerCase())}
              placeholder="terraform"
              className="h-9 text-[12.5px]"
              autoComplete="off"
            />
          </FieldRow>
          <FieldRow label="Command">
            <Input
              value={command}
              onChange={(event) => setCommand(event.target.value)}
              placeholder="terraform"
              className="h-9 text-[12.5px]"
              autoComplete="off"
            />
          </FieldRow>
          <FieldRow label="Version arguments">
            <Input
              value={argsText}
              onChange={(event) => setArgsText(event.target.value)}
              placeholder="--version"
              className="h-9 text-[12.5px]"
              autoComplete="off"
            />
          </FieldRow>
          <FieldRow label="Category">
            <Select value={category} onValueChange={(value) => setCategory(value as EnvironmentToolCategoryDto)}>
              <SelectTrigger size="sm" className="h-9 w-full text-[12.5px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {TOOL_CATEGORY_ORDER.map((option) => (
                  <SelectItem key={option} value={option} className="text-[12.5px]">
                    {TOOL_CATEGORY_LABEL[option]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </FieldRow>
        </div>

        <VerifyResult result={verification} error={formError} command={command.trim()} />

        <DialogFooter className="gap-2 sm:justify-between">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 gap-1.5 text-[12.5px]"
            disabled={!onVerify || isVerifying || isSaving}
            onClick={verify}
          >
            {isVerifying ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
            Verify
          </Button>
          <div className="flex gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-9 text-[12.5px]"
              onClick={() => close(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              className="h-9 gap-1.5 text-[12.5px]"
              disabled={!canSave}
              onClick={save}
            >
              {isSaving ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function FieldRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-[12px] font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  )
}

function VerifyResult({
  result,
  error,
  command,
}: {
  result: VerifyUserToolResponseDto | null
  error: string | null
  command: string
}) {
  if (error) {
    return (
      <Alert variant="destructive" className="rounded-md px-3.5 py-2.5 text-[12.5px]">
        <XCircle className="h-4 w-4" />
        <AlertTitle className="text-[12.5px] font-semibold">Verification failed</AlertTitle>
        <AlertDescription className="text-[12.5px]">{error}</AlertDescription>
      </Alert>
    )
  }
  if (!result) return null

  const { record } = result
  if (record.probeStatus === "ok" && record.present) {
    return (
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-success/20 bg-success/10 px-3.5 py-2.5 text-[12.5px] text-success dark:text-success">
        <Badge variant="outline" className="border-success/30 text-[11.5px] text-success dark:text-success">
          Verified
        </Badge>
        <span className="min-w-0 truncate font-mono">{record.version ?? record.id}</span>
        {record.displayPath ? <span className="text-success/75 dark:text-success/75">{record.displayPath}</span> : null}
      </div>
    )
  }

  const diagnostic = result.diagnostics.find((item) => item.toolId === record.id)
  const message =
    record.probeStatus === "missing"
      ? `Could not find ${command || record.id} on PATH.`
      : diagnostic?.message ?? "The version probe did not return usable output."

  return (
    <div className="rounded-md border border-warning/25 bg-warning/10 px-3.5 py-2.5 text-[12.5px] leading-[1.5] text-warning">
      {message}
    </div>
  )
}

function parseVersionArgs(value: string): string[] {
  return value
    .split(/[,\s]+/)
    .map((part) => part.trim())
    .filter(Boolean)
}

function pickHeadlinerTools(presentTools: EnvironmentToolSummaryDto[]): EnvironmentToolSummaryDto[] {
  if (presentTools.length === 0) return []
  const byId = new Map(presentTools.map((tool) => [tool.id, tool]))
  const picked: EnvironmentToolSummaryDto[] = []
  for (const id of HEADLINER_TOOL_PRIORITY) {
    if (picked.length >= HEADLINER_LIMIT) break
    const tool = byId.get(id)
    if (tool) picked.push(tool)
  }
  return picked
}

function groupToolsByCategory(
  tools: EnvironmentToolSummaryDto[],
): Array<{ key: EnvironmentToolCategoryDto; tools: EnvironmentToolSummaryDto[] }> {
  if (tools.length === 0) return []
  const buckets = new Map<EnvironmentToolCategoryDto, EnvironmentToolSummaryDto[]>()
  for (const tool of tools) {
    const list = buckets.get(tool.category) ?? []
    list.push(tool)
    buckets.set(tool.category, list)
  }
  const orderIndex = new Map(TOOL_CATEGORY_ORDER.map((category, index) => [category, index]))
  return Array.from(buckets.entries())
    .map(([key, group]) => ({
      key,
      tools: [...group].sort((a, b) => a.id.localeCompare(b.id)),
    }))
    .sort((a, b) => {
      const ai = orderIndex.get(a.key) ?? Number.MAX_SAFE_INTEGER
      const bi = orderIndex.get(b.key) ?? Number.MAX_SAFE_INTEGER
      return ai - bi
    })
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
  icon: ElementType
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
  const populatedGroups = useMemo(
    () => CHECK_GROUPS.filter(({ key }) => report[key].length > 0),
    [report],
  )
  const skippedGroupCount = CHECK_GROUPS.length - populatedGroups.length

  void skippedGroupCount

  return (
    <div className="flex flex-col gap-5">
      <ReportSummary
        report={report}
        copied={copied}
        isRunning={isRunning}
        canRun={canRun}
        onCopy={onCopy}
        onRun={onRun}
      />

      {populatedGroups.length > 0 ? (
        <div className="flex flex-col gap-5">
          {populatedGroups.map(({ key, label }) => (
            <CheckGroup key={key} label={label} checks={report[key]} />
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed border-border/60 bg-secondary/10 px-4 py-3.5 text-[12.5px] text-muted-foreground">
          No checks returned for this run.
        </p>
      )}
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
    <section className="overflow-hidden rounded-lg border border-border/60">
      <header className="flex flex-wrap items-center justify-between gap-3 bg-secondary/10 px-4 py-3">
        <div className="min-w-0">
          <p className="text-[13.5px] font-semibold tracking-tight text-foreground">{MODE_LABEL[report.mode]}</p>
          <p className="mt-0.5 truncate text-[12px] text-muted-foreground">{generatedAt}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 px-2.5 text-[12px] text-muted-foreground hover:text-foreground"
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
            className="h-8 gap-1.5 px-2.5 text-[12px]"
            disabled={!canRun}
            onClick={() => onRun("extended_network")}
          >
            <Play className="h-3.5 w-3.5" />
            Extended
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:text-foreground"
            onClick={onCopy}
            aria-label={copied ? "Copied" : "Copy JSON"}
            title={copied ? "Copied" : "Copy JSON"}
          >
            {copied ? (
              <Check className="h-4 w-4 text-success dark:text-success" />
            ) : (
              <Clipboard className="h-4 w-4" />
            )}
          </Button>
        </div>
      </header>

      <div className="flex divide-x divide-border/40 border-t border-border/40">
        <SummaryTile tone="passed" value={report.summary.passed} />
        <SummaryTile tone="warning" value={report.summary.warnings} />
        <SummaryTile tone="failed" value={report.summary.failed} />
        <SummaryTile tone="skipped" value={report.summary.skipped} />
      </div>
    </section>
  )
}

function SummaryTile({
  tone,
  value,
}: {
  tone: XeroDiagnosticStatusDto
  value: number
}) {
  const Icon = STATUS_ICON[tone]
  const isZero = value === 0
  return (
    <div className="flex flex-1 items-center gap-2 px-3.5 py-2.5">
      <Icon
        className={cn(
          "h-4 w-4 shrink-0",
          isZero ? "text-muted-foreground/40" : STATUS_TEXT[tone],
        )}
        aria-hidden
      />
      <span
        className={cn(
          "text-[15px] font-semibold tabular-nums",
          isZero ? "text-foreground/40" : "text-foreground",
        )}
      >
        {value}
      </span>
      <span className="text-[12px] text-muted-foreground">{STATUS_LABEL[tone]}</span>
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
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">
          {label}
          <span className="ml-2 text-[12px] font-normal text-muted-foreground">{checks.length}</span>
        </h4>
        <div className="flex items-center gap-1">
          {(["failed", "warning", "skipped", "passed"] as const).map((tone) =>
            counts[tone] > 0 ? <CountChip key={tone} tone={tone} value={counts[tone]} /> : null,
          )}
        </div>
      </div>
      <div className="overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40">
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
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium tabular-nums ring-1 ring-inset",
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

  return (
    <div className="flex items-start gap-3 px-4 py-3">
      <Icon
        className={cn("mt-[1px] h-4 w-4 shrink-0", STATUS_TEXT[check.status])}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] leading-[1.5] text-foreground">{check.message}</p>
        {check.remediation && check.status !== "passed" ? (
          <div
            className={cn(
              "mt-1.5 flex items-start gap-1.5 text-[12px] leading-[1.5]",
              check.status === "failed"
                ? "text-destructive/90"
                : "text-warning/90 dark:text-warning/90",
            )}
          >
            <ArrowRight className="mt-[2px] h-3.5 w-3.5 shrink-0" aria-hidden />
            <p>{check.remediation}</p>
          </div>
        ) : null}
      </div>
    </div>
  )
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
