"use client"

import { useCallback, useMemo, useState } from "react"
import {
  AlertCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  Loader2,
  RefreshCw,
  Sparkles,
  Upload,
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
  ClusterKind,
  CodamaGenerationReport,
  CodamaTarget,
  DeployProgressPayload,
  DriftReport,
  Idl,
  IdlChangedEvent,
  IdlPublishReport,
} from "@/src/features/solana/use-solana-workbench"

const ALL_TARGETS: CodamaTarget[] = ["ts", "rust", "umi"]
const NO_AUTHORITY_PERSONA = "__none__"

interface SolanaIdlPanelProps {
  cluster: ClusterKind
  idls: Record<string, Idl>
  idlBusy: boolean
  lastIdlEvent: IdlChangedEvent | null
  lastDriftReport: DriftReport | null
  lastCodamaReport: CodamaGenerationReport | null
  lastPublishReport: IdlPublishReport | null
  lastDeployProgress: DeployProgressPayload | null
  activeWatches: string[]
  personaNames: string[]
  onLoad: (path: string) => Promise<Idl | null>
  onFetch: (programId: string) => Promise<Idl | null>
  onDrift: (programId: string, localPath: string) => Promise<DriftReport | null>
  onCodama: (
    idlPath: string,
    targets: CodamaTarget[],
    outputDir: string,
  ) => Promise<CodamaGenerationReport | null>
  onPublish: (args: {
    programId: string
    idlPath: string
    authorityPersona: string
    mode: "init" | "upgrade"
  }) => Promise<IdlPublishReport | null>
  onStartWatch: (path: string) => Promise<string | null>
  onStopWatch: (token: string) => Promise<boolean>
}

export function SolanaIdlPanel({
  cluster,
  idls,
  idlBusy,
  lastIdlEvent,
  lastDriftReport,
  lastCodamaReport,
  lastPublishReport,
  lastDeployProgress,
  activeWatches,
  personaNames,
  onLoad,
  onFetch,
  onDrift,
  onCodama,
  onPublish,
  onStartWatch,
  onStopWatch,
}: SolanaIdlPanelProps) {
  const [idlPath, setIdlPath] = useState("")
  const [programId, setProgramId] = useState("")
  const [outputDir, setOutputDir] = useState("clients")
  const [authorityPersona, setAuthorityPersona] = useState<string>(
    personaNames[0] ?? "",
  )
  const [mode, setMode] = useState<"init" | "upgrade">("init")
  const [selectedTargets, setSelectedTargets] = useState<CodamaTarget[]>([
    "ts",
  ])
  const [status, setStatus] = useState<string | null>(null)
  const [idlCollapsed, setIdlCollapsed] = useState(true)

  const idlEntries = useMemo(() => Object.entries(idls), [idls])

  const handleLoad = useCallback(async () => {
    const trimmed = idlPath.trim()
    if (!trimmed) {
      setStatus("Provide a path to target/idl/*.json")
      return
    }
    setStatus(null)
    const idl = await onLoad(trimmed)
    if (idl) {
      setStatus(`Loaded — hash ${idl.hash.slice(0, 12)}…`)
      const pid = readProgramId(idl)
      if (pid) setProgramId(pid)
    }
  }, [idlPath, onLoad])

  const handleFetch = useCallback(async () => {
    const trimmed = programId.trim()
    if (!trimmed) {
      setStatus("Provide a program id to fetch from chain")
      return
    }
    setStatus(null)
    const idl = await onFetch(trimmed)
    setStatus(idl ? "Fetched on-chain IDL" : "No on-chain IDL for that program")
  }, [programId, onFetch])

  const handleDrift = useCallback(async () => {
    if (!programId.trim() || !idlPath.trim()) {
      setStatus("Provide both program id and local IDL path")
      return
    }
    setStatus("Comparing local vs on-chain IDL…")
    const report = await onDrift(programId.trim(), idlPath.trim())
    if (!report) return
    if (report.identical) {
      setStatus("Local and on-chain IDL are identical")
    } else {
      setStatus(
        `Drift: ${report.breakingCount} breaking · ${report.riskyCount} risky · ${report.nonBreakingCount} additive`,
      )
    }
  }, [idlPath, onDrift, programId])

  const handleCodama = useCallback(async () => {
    if (!idlPath.trim() || !outputDir.trim()) {
      setStatus("Provide IDL path and output directory")
      return
    }
    if (selectedTargets.length === 0) {
      setStatus("Pick at least one Codama target")
      return
    }
    setStatus("Generating clients…")
    const report = await onCodama(idlPath.trim(), selectedTargets, outputDir.trim())
    if (report) {
      setStatus(
        report.allSucceeded
          ? `Generated ${report.targets.length} target(s) in ${report.elapsedMs}ms`
          : `Some targets failed — check output below`,
      )
    }
  }, [idlPath, onCodama, outputDir, selectedTargets])

  const handlePublish = useCallback(async () => {
    if (!programId.trim() || !idlPath.trim() || !authorityPersona) {
      setStatus("Program id, IDL path, and authority persona are required")
      return
    }
    setStatus(`Publishing IDL (${mode})…`)
    const report = await onPublish({
      programId: programId.trim(),
      idlPath: idlPath.trim(),
      authorityPersona,
      mode,
    })
    if (report) {
      setStatus(
        report.success
          ? `Published — signature ${report.signature?.slice(0, 16) ?? "n/a"}…`
          : `Publish failed (exit ${report.exitCode ?? "?"})`,
      )
    }
  }, [authorityPersona, idlPath, mode, onPublish, programId])

  const handleToggleWatch = useCallback(async () => {
    const trimmed = idlPath.trim()
    if (!trimmed) {
      setStatus("Provide an IDL path to watch")
      return
    }
    const currentToken = activeWatches.find(
      (_token) => true, // surface the first active watch regardless — one-at-a-time UX
    )
    if (currentToken) {
      setStatus("Stopping watch…")
      await onStopWatch(currentToken)
      setStatus("Watch stopped")
    } else {
      setStatus("Starting watch…")
      const token = await onStartWatch(trimmed)
      setStatus(token ? `Watching (${token})` : "Could not start watch")
    }
  }, [activeWatches, idlPath, onStartWatch, onStopWatch])

  const toggleTarget = useCallback((target: CodamaTarget) => {
    setSelectedTargets((current) =>
      current.includes(target)
        ? current.filter((t) => t !== target)
        : [...current, target],
    )
  }, [])

  const hasActiveWatch = activeWatches.length > 0

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <input
          type="text"
          value={idlPath}
          onChange={(e) => setIdlPath(e.target.value)}
          placeholder="target/idl/my_program.json"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
        />
        <input
          type="text"
          value={programId}
          onChange={(e) => setProgramId(e.target.value)}
          placeholder="program id (base58)"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 font-mono text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
        />
      </div>

      <div className="flex flex-wrap gap-1.5">
        <IdlActionButton
          label="Load"
          icon={<RefreshCw className="h-3 w-3" />}
          onClick={handleLoad}
          busy={idlBusy}
        />
        <IdlActionButton
          label="Fetch"
          icon={<Upload className="h-3 w-3" />}
          onClick={handleFetch}
          busy={idlBusy}
        />
        <IdlActionButton
          label="Drift"
          icon={<AlertCircle className="h-3 w-3" />}
          onClick={handleDrift}
          busy={idlBusy}
        />
        <IdlActionButton
          label={hasActiveWatch ? "Stop watch" : "Watch"}
          icon={hasActiveWatch ? <EyeOff className="h-3 w-3" /> : <Eye className="h-3 w-3" />}
          onClick={handleToggleWatch}
          busy={false}
          active={hasActiveWatch}
        />
      </div>

      <div className="border-t border-border/50 pt-3">
        <div className="mb-2 text-[11.5px] font-medium text-foreground">
          Codama codegen
        </div>
        <div className="flex flex-col gap-1.5">
          <input
            type="text"
            value={outputDir}
            onChange={(e) => setOutputDir(e.target.value)}
            placeholder="output dir (e.g. clients)"
            className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[11.5px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
          />
          <div className="flex flex-wrap gap-1">
            {ALL_TARGETS.map((target) => {
              const enabled = selectedTargets.includes(target)
              return (
                <button
                  key={target}
                  type="button"
                  onClick={() => toggleTarget(target)}
                  className={cn(
                    "rounded-md border px-2.5 py-1 text-[11.5px] transition-colors",
                    enabled
                      ? "border-primary/50 bg-primary/10 text-primary"
                      : "border-border/60 bg-background text-foreground/80 hover:border-primary/40",
                  )}
                >
                  {target}
                </button>
              )
            })}
          </div>
          <button
            type="button"
            onClick={handleCodama}
            disabled={idlBusy}
            className="mt-0.5 inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-[12px] font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
          >
            {idlBusy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="h-3.5 w-3.5" />
            )}
            Generate
          </button>
        </div>
      </div>

      <div className="border-t border-border/50 pt-3">
        <div className="mb-2 text-[11.5px] font-medium text-foreground">
          Publish IDL
        </div>
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-4 text-[11.5px]">
            <label className="flex items-center gap-1.5 text-foreground/85">
              <input
                type="radio"
                checked={mode === "init"}
                onChange={() => setMode("init")}
                className="accent-primary"
              />
              init
            </label>
            <label className="flex items-center gap-1.5 text-foreground/85">
              <input
                type="radio"
                checked={mode === "upgrade"}
                onChange={() => setMode("upgrade")}
                className="accent-primary"
              />
              upgrade
            </label>
          </div>
          <Select
            value={authorityPersona || NO_AUTHORITY_PERSONA}
            onValueChange={(value) =>
              setAuthorityPersona(value === NO_AUTHORITY_PERSONA ? "" : value)
            }
          >
            <SelectTrigger
              aria-label="Authority persona"
              className="h-8 w-full border-border/60 bg-background text-[11.5px] focus:border-primary/60"
              size="sm"
            >
              <SelectValue placeholder="Authority persona" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NO_AUTHORITY_PERSONA}>Authority persona</SelectItem>
              {personaNames.map((name) => (
                <SelectItem key={name} value={name}>
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <button
            type="button"
            onClick={handlePublish}
            disabled={idlBusy}
            className="mt-0.5 inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-[12px] font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
          >
            {idlBusy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Upload className="h-3.5 w-3.5" />
            )}
            Publish
          </button>
        </div>
      </div>

      {status ? (
        <p className="text-[11px] text-muted-foreground">{status}</p>
      ) : null}

      {lastIdlEvent ? (
        <div className="mt-2 rounded-md border border-border/50 bg-background/30 p-2 text-[11px] text-foreground/85">
          <div className="flex items-center justify-between">
            <span className="font-mono">{lastIdlEvent.phase}</span>
            <span className="text-muted-foreground">
              {lastIdlEvent.programName ?? "unknown"}
            </span>
          </div>
          <div className="text-[10.5px] text-muted-foreground truncate">
            {lastIdlEvent.path}
          </div>
          <div className="text-[10.5px] text-muted-foreground">
            hash: {lastIdlEvent.hash.slice(0, 16)}…
          </div>
        </div>
      ) : null}

      {lastDriftReport ? <DriftSummary report={lastDriftReport} /> : null}

      {lastCodamaReport ? <CodamaSummary report={lastCodamaReport} /> : null}

      {lastPublishReport ? <PublishSummary report={lastPublishReport} /> : null}

      {lastDeployProgress ? (
        <div className="mt-2 rounded-md border border-border/40 bg-background/20 px-2 py-1 text-[10.5px] text-muted-foreground">
          deploy/{lastDeployProgress.phase}: {lastDeployProgress.detail}
        </div>
      ) : null}

      {idlEntries.length > 0 ? (
        <div>
          <button
            type="button"
            onClick={() => setIdlCollapsed((v) => !v)}
            className="text-[11px] font-medium text-muted-foreground hover:text-foreground"
          >
            {idlCollapsed ? "Show" : "Hide"} cached ({idlEntries.length})
          </button>
          {!idlCollapsed ? (
            <ul className="mt-1.5 flex flex-col gap-1">
              {idlEntries.map(([key, idl]) => (
                <li
                  key={key}
                  className="rounded-md border border-border/40 bg-background/20 px-2 py-1.5 text-[11px] text-foreground/80"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-mono">{key}</span>
                    <span className="text-muted-foreground">
                      {idl.hash.slice(0, 10)}…
                    </span>
                  </div>
                  <div className="truncate text-muted-foreground">
                    {idl.source.kind === "file"
                      ? idl.source.path
                      : idl.source.kind === "chain"
                        ? `${idl.source.cluster} · ${idl.source.idlAddress}`
                        : "synthetic"}
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

function IdlActionButton({
  label,
  icon,
  onClick,
  busy,
  active,
}: {
  label: string
  icon: React.ReactNode
  onClick: () => void
  busy: boolean
  active?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      className={cn(
        "inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] transition-colors",
        active
          ? "border-primary/60 bg-primary/20 text-primary"
          : "border-border/70 bg-background/40 text-foreground/85 hover:border-primary/40 hover:text-foreground",
        busy && "opacity-60",
      )}
    >
      {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : icon}
      {label}
    </button>
  )
}

function DriftSummary({ report }: { report: DriftReport }) {
  return (
    <div className="mt-2 rounded-md border border-border/50 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center justify-between">
        <span className="font-semibold text-foreground">Drift</span>
        <span className="text-muted-foreground">
          {report.identical ? "identical" : "diff"}
        </span>
      </div>
      {!report.identical ? (
        <>
          <div className="text-[10.5px] text-muted-foreground">
            {report.breakingCount} breaking · {report.riskyCount} risky ·{" "}
            {report.nonBreakingCount} additive
          </div>
          <ul className="mt-1 flex flex-col gap-0.5">
            {report.changes.slice(0, 6).map((change, i) => (
              <li
                key={i}
                className={cn(
                  "truncate",
                  change.severity === "breaking" && "text-destructive",
                  change.severity === "risky" && "text-amber-400",
                )}
              >
                {change.severity[0].toUpperCase()} · {change.path}
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  )
}

function CodamaSummary({ report }: { report: CodamaGenerationReport }) {
  return (
    <div className="mt-2 rounded-md border border-border/50 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center justify-between">
        <span className="font-semibold text-foreground">Codama</span>
        <span className="text-muted-foreground">
          {report.elapsedMs}ms ·{" "}
          {report.allSucceeded ? (
            <span className="inline-flex items-center gap-0.5 text-emerald-400">
              <CheckCircle2 className="h-3 w-3" /> ok
            </span>
          ) : (
            <span className="inline-flex items-center gap-0.5 text-destructive">
              <AlertCircle className="h-3 w-3" /> failed
            </span>
          )}
        </span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5 text-[10.5px] text-muted-foreground">
        {report.targets.map((target, i) => (
          <li key={i}>
            <span className="font-mono">{target.target}</span>{" "}
            <span className={target.success ? "" : "text-destructive"}>
              {target.success ? "ok" : "fail"}
            </span>{" "}
            · {target.elapsedMs}ms
          </li>
        ))}
      </ul>
    </div>
  )
}

function PublishSummary({ report }: { report: IdlPublishReport }) {
  return (
    <div className="mt-2 rounded-md border border-border/50 bg-background/30 p-2 text-[11px]">
      <div className="flex items-center justify-between">
        <span className="font-semibold text-foreground">Publish</span>
        <span
          className={cn(
            "inline-flex items-center gap-0.5",
            report.success ? "text-emerald-400" : "text-destructive",
          )}
        >
          {report.success ? (
            <CheckCircle2 className="h-3 w-3" />
          ) : (
            <AlertCircle className="h-3 w-3" />
          )}{" "}
          {report.mode}
        </span>
      </div>
      {report.signature ? (
        <div className="text-[10.5px] text-muted-foreground truncate font-mono">
          sig {report.signature}
        </div>
      ) : null}
      {!report.success && report.stderrExcerpt ? (
        <div className="mt-1 rounded-md border border-destructive/40 bg-destructive/10 p-1 text-[10.5px] text-destructive">
          {report.stderrExcerpt.slice(0, 200)}
          {report.stderrExcerpt.length > 200 ? "…" : ""}
        </div>
      ) : null}
    </div>
  )
}

function readProgramId(idl: Idl): string | null {
  if (!idl.value || typeof idl.value !== "object") return null
  const value = idl.value as {
    address?: string
    metadata?: { address?: string }
  }
  return value.address ?? value.metadata?.address ?? null
}
