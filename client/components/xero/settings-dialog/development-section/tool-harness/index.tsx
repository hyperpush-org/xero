import { invoke, isTauri } from "@tauri-apps/api/core"
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  FlaskConical,
  Loader2,
  PlayCircle,
  RefreshCw,
  XCircle,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"
import {
  developerToolCatalogResponseSchema,
  developerToolHarnessProjectSchema,
  developerToolSyntheticRunRequestSchema,
  developerToolSyntheticRunResponseSchema,
  type DeveloperToolCatalogEntryDto,
  type DeveloperToolCatalogResponseDto,
  type DeveloperToolHarnessProjectDto,
  type DeveloperToolSyntheticRunResponseDto,
} from "@/src/lib/xero-model/developer-tool-harness"

type LoadState = "idle" | "loading" | "ready" | "error"

type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue }

interface ToolRunState {
  status: LoadState
  result?: DeveloperToolSyntheticRunResponseDto
  errorMessage?: string
  expanded: boolean
}

export function ToolHarness() {
  const [catalog, setCatalog] = useState<DeveloperToolCatalogResponseDto | null>(null)
  const [state, setState] = useState<LoadState>("idle")
  const [error, setError] = useState<string | null>(null)
  const [harnessProject, setHarnessProject] =
    useState<DeveloperToolHarnessProjectDto | null>(null)
  const [projectState, setProjectState] = useState<LoadState>("idle")
  const [projectError, setProjectError] = useState<string | null>(null)
  const [runs, setRuns] = useState<Record<string, ToolRunState>>({})

  const loadHarnessProject = useCallback(async () => {
    if (!isTauri()) {
      setProjectState("error")
      setProjectError("Tool harness requires the Tauri desktop runtime.")
      return
    }
    setProjectState("loading")
    setProjectError(null)
    try {
      const response = await invoke<unknown>("developer_tool_harness_project")
      const parsed = developerToolHarnessProjectSchema.parse(response)
      setHarnessProject(parsed)
      setProjectState("ready")
    } catch (err) {
      setHarnessProject(null)
      setProjectState("error")
      setProjectError(
        errorMessage(err, "Xero could not prepare the tool harness fixture project."),
      )
    }
  }, [])

  const loadCatalog = useCallback(async () => {
    if (!isTauri()) {
      setCatalog(null)
      setState("error")
      setError("Tool harness requires the Tauri desktop runtime.")
      return
    }

    setState("loading")
    setError(null)
    try {
      const response = await invoke<unknown>("developer_tool_catalog", {
        request: { skillToolEnabled: true },
      })
      const parsed = developerToolCatalogResponseSchema.parse(response)
      setCatalog(parsed)
      setState("ready")
    } catch (err) {
      setCatalog(null)
      setState("error")
      setError(errorMessage(err, "Xero could not load the developer tool catalog."))
    }
  }, [])

  useEffect(() => {
    void loadHarnessProject()
    void loadCatalog()
  }, [loadHarnessProject, loadCatalog])

  const handleRun = useCallback(
    async (entry: DeveloperToolCatalogEntryDto) => {
      if (!harnessProject || !isTauri()) return
      setRuns((current) => ({
        ...current,
        [entry.toolName]: {
          status: "loading",
          result: current[entry.toolName]?.result,
          expanded: true,
        },
      }))
      try {
        const args = synthesizeDefaults(entry.inputSchema)
        const request = developerToolSyntheticRunRequestSchema.parse({
          projectId: harnessProject.projectId,
          calls: [{ toolName: entry.toolName, input: args }],
          options: {
            stopOnFailure: true,
            approveWrites: false,
            operatorApproveAll: false,
          },
        })
        const response = await invoke<unknown>("developer_tool_synthetic_run", {
          request,
        })
        const parsed = developerToolSyntheticRunResponseSchema.parse(response)
        setRuns((current) => ({
          ...current,
          [entry.toolName]: {
            status: "ready",
            result: parsed,
            expanded: true,
          },
        }))
      } catch (err) {
        setRuns((current) => ({
          ...current,
          [entry.toolName]: {
            status: "error",
            errorMessage: errorMessage(
              err,
              "Xero could not run the synthetic harness call.",
            ),
            expanded: true,
          },
        }))
      }
    },
    [harnessProject],
  )

  const toggleExpanded = useCallback((toolName: string) => {
    setRuns((current) => {
      const entry = current[toolName]
      if (!entry) return current
      return {
        ...current,
        [toolName]: { ...entry, expanded: !entry.expanded },
      }
    })
  }, [])

  const refresh = useCallback(() => {
    void loadHarnessProject()
    void loadCatalog()
  }, [loadHarnessProject, loadCatalog])

  const entries = catalog?.entries ?? []

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <FlaskConical className="h-3.5 w-3.5 text-muted-foreground" />
          <h4 className="text-[12.5px] font-semibold text-foreground">Tool harness</h4>
          {catalog ? (
            <Badge variant="outline" className="h-5 text-[10.5px]">
              {entries.length} tool{entries.length === 1 ? "" : "s"}
            </Badge>
          ) : null}
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 gap-1.5 text-[12px]"
          disabled={state === "loading" || projectState === "loading"}
          onClick={refresh}
          aria-label="Refresh tool harness"
        >
          <RefreshCw
            className={cn(
              "h-3.5 w-3.5",
              (state === "loading" || projectState === "loading") && "animate-spin",
            )}
          />
          Refresh
        </Button>
      </div>

      <HarnessProjectBanner
        project={harnessProject}
        state={projectState}
        errorMessage={projectError}
      />

      <div className="overflow-hidden rounded-lg border border-border/60 bg-card/30">
        {state === "error" ? (
          <CatalogError message={error ?? "Xero could not load the developer tool catalog."} />
        ) : null}

        {state === "loading" && !catalog ? (
          <CatalogSkeleton />
        ) : catalog ? (
          entries.length === 0 ? (
            <p className="px-4 py-6 text-center text-[12px] text-muted-foreground">
              No tools are available on this host.
            </p>
          ) : (
            <ul className="divide-y divide-border/40">
              {entries.map((entry) => (
                <ToolRow
                  key={entry.toolName}
                  entry={entry}
                  runState={runs[entry.toolName]}
                  canRun={Boolean(harnessProject) && entry.runtimeAvailable}
                  hostOsLabel={catalog.hostOsLabel}
                  onRun={() => void handleRun(entry)}
                  onToggle={() => toggleExpanded(entry.toolName)}
                />
              ))}
            </ul>
          )
        ) : null}
      </div>
    </section>
  )
}

interface HarnessProjectBannerProps {
  project: DeveloperToolHarnessProjectDto | null
  state: LoadState
  errorMessage: string | null
}

function HarnessProjectBanner({
  project,
  state,
  errorMessage: bannerError,
}: HarnessProjectBannerProps) {
  let label: string
  if (state === "loading") {
    label = "Preparing harness fixture project…"
  } else if (state === "error") {
    label = bannerError ?? "Could not prepare harness fixture project."
  } else if (!project) {
    label = "Harness fixture project unavailable."
  } else {
    label = `Harness fixture: ${project.displayName}`
  }

  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-md border border-border/60 bg-secondary/15 px-3 py-2 text-[11.5px]",
        state === "error" || !project ? "text-muted-foreground" : "text-foreground/80",
      )}
    >
      <FlaskConical className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate">{label}</span>
      {project ? (
        <code
          className="ml-auto shrink-0 truncate rounded bg-muted/40 px-1.5 py-0.5 font-mono text-[10.5px] text-muted-foreground"
          title={project.rootPath}
        >
          {project.rootPath}
        </code>
      ) : null}
    </div>
  )
}

interface ToolRowProps {
  entry: DeveloperToolCatalogEntryDto
  runState: ToolRunState | undefined
  canRun: boolean
  hostOsLabel: string
  onRun: () => void
  onToggle: () => void
}

function ToolRow({ entry, runState, canRun, hostOsLabel, onRun, onToggle }: ToolRowProps) {
  const status = runState?.status ?? "idle"
  const isRunning = status === "loading"
  const expanded = runState?.expanded ?? false
  const hasResult = Boolean(runState?.result || runState?.errorMessage)

  return (
    <li className="flex flex-col">
      <div className="flex items-start gap-3 px-3 py-2.5">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="font-mono text-[12px] font-medium text-foreground">
              {entry.toolName}
            </span>
            <Badge variant="outline" className="h-4 shrink-0 text-[10px]">
              {entry.group}
            </Badge>
            <Badge variant="outline" className="h-4 shrink-0 text-[10px]">
              risk: {entry.riskClass}
            </Badge>
            {!entry.runtimeAvailable ? (
              <Badge
                variant="outline"
                className="h-4 shrink-0 border-warning/40 bg-warning/10 text-[10px] text-warning"
              >
                Unavailable on {hostOsLabel}
              </Badge>
            ) : null}
            <RunStatusBadge runState={runState} />
          </div>
          <p className="mt-1 line-clamp-2 text-[11.5px] leading-[1.45] text-muted-foreground">
            {entry.description}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {hasResult ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 w-8 p-0"
              aria-label={expanded ? "Hide result" : "Show result"}
              aria-expanded={expanded}
              onClick={onToggle}
            >
              <ChevronDown
                className={cn(
                  "h-3.5 w-3.5 transition-transform motion-fast",
                  expanded ? "rotate-180" : "",
                )}
              />
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            onClick={onRun}
            disabled={!canRun || isRunning}
          >
            {isRunning ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <PlayCircle className="h-3.5 w-3.5" />
            )}
            Run
          </Button>
        </div>
      </div>

      {expanded && hasResult ? (
        <div className="border-t border-border/40 bg-background/40 px-3 py-2.5">
          {runState?.errorMessage ? (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-md border border-destructive/35 bg-destructive/10 px-3 py-2 text-[11.5px] text-destructive"
            >
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>{runState.errorMessage}</span>
            </div>
          ) : runState?.result ? (
            <RunResultPanel result={runState.result} />
          ) : null}
        </div>
      ) : null}
    </li>
  )
}

function RunStatusBadge({ runState }: { runState: ToolRunState | undefined }) {
  if (!runState) return null
  if (runState.status === "loading") {
    return (
      <Badge variant="outline" className="h-4 shrink-0 text-[10px]">
        Running…
      </Badge>
    )
  }
  if (runState.status === "error") {
    return (
      <Badge
        variant="outline"
        className="h-4 shrink-0 border-destructive/40 bg-destructive/10 text-[10px] text-destructive"
      >
        Error
      </Badge>
    )
  }
  if (runState.status === "ready" && runState.result) {
    const failed = runState.result.hadFailure
    return (
      <Badge
        variant="outline"
        className={cn(
          "h-4 shrink-0 text-[10px]",
          failed
            ? "border-destructive/40 bg-destructive/10 text-destructive"
            : "border-emerald-400/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
        )}
      >
        {failed ? "Failed" : "Success"}
      </Badge>
    )
  }
  return null
}

function RunResultPanel({ result }: { result: DeveloperToolSyntheticRunResponseDto }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-1.5 text-[10.5px] text-muted-foreground">
        <span>Run</span>
        <code className="rounded bg-muted/40 px-1 py-0.5 font-mono text-foreground">
          {result.runId}
        </code>
        {result.stoppedEarly ? (
          <Badge variant="outline" className="h-4 text-[10px]">
            Stopped early
          </Badge>
        ) : null}
      </div>
      <ul className="flex flex-col gap-2">
        {result.results.map((entry) => (
          <li
            key={entry.toolCallId}
            className="rounded-md border border-border/40 bg-card/40 p-2"
          >
            <div className="flex items-center gap-1.5 text-[11.5px]">
              {entry.ok ? (
                <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />
              ) : (
                <XCircle className="h-3.5 w-3.5 text-destructive" />
              )}
              <span className="font-mono text-foreground">{entry.toolName}</span>
              <span className="font-mono text-[10.5px] text-muted-foreground">
                {entry.toolCallId}
              </span>
            </div>
            {entry.summary ? (
              <p className="mt-1 text-[11.5px] text-muted-foreground">{entry.summary}</p>
            ) : null}
            <details className="mt-1.5">
              <summary className="cursor-pointer text-[10.5px] text-muted-foreground">
                Output
              </summary>
              <pre className="mt-1 max-h-56 overflow-auto rounded bg-muted/40 p-2 font-mono text-[10.5px] leading-[1.5] text-foreground/85">
                {JSON.stringify(entry.output, null, 2)}
              </pre>
            </details>
          </li>
        ))}
      </ul>
    </div>
  )
}

function CatalogError({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 border-b border-destructive/35 bg-destructive/10 px-4 py-3"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
      <p className="text-[12px] leading-[1.5] text-destructive/90">{message}</p>
    </div>
  )
}

function CatalogSkeleton() {
  return (
    <div className="flex flex-col gap-3 px-4 py-4">
      <div className="flex items-center gap-2">
        <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
        <span className="text-[12px] text-muted-foreground">Loading tool catalog…</span>
      </div>
      <Skeleton className="h-32" />
    </div>
  )
}

function synthesizeDefaults(schema: unknown): Record<string, JsonValue> {
  const value = synthesizeValue(schema)
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, JsonValue>
  }
  return {}
}

function synthesizeValue(schema: unknown): JsonValue {
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) return null
  const raw = schema as Record<string, unknown>

  if (Array.isArray(raw.enum) && raw.enum.length > 0) {
    return raw.enum[0] as JsonValue
  }

  if (Array.isArray(raw.examples) && raw.examples.length > 0) {
    return raw.examples[0] as JsonValue
  }

  if ("default" in raw) {
    return raw.default as JsonValue
  }

  const type = pickPrimitiveType(raw)

  switch (type) {
    case "object": {
      const out: Record<string, JsonValue> = {}
      const required = Array.isArray(raw.required)
        ? (raw.required as unknown[]).filter(
            (entry): entry is string => typeof entry === "string",
          )
        : []
      const properties =
        raw.properties && typeof raw.properties === "object"
          ? (raw.properties as Record<string, unknown>)
          : {}
      for (const key of required) {
        out[key] = synthesizeValue(properties[key])
      }
      return out
    }
    case "array":
      return []
    case "string":
      return ""
    case "integer":
    case "number":
      return 0
    case "boolean":
      return false
    default:
      return null
  }
}

function pickPrimitiveType(raw: Record<string, unknown>): string {
  const value = raw.type
  if (typeof value === "string") return value
  if (Array.isArray(value)) {
    for (const candidate of value) {
      if (typeof candidate === "string" && candidate !== "null") return candidate
    }
  }
  if (raw.properties || raw.required) return "object"
  if (raw.items) return "array"
  if (raw.enum) return "string"
  return "any"
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof (error as { message?: unknown }).message === "string"
  ) {
    return (error as { message: string }).message
  }
  return fallback
}
