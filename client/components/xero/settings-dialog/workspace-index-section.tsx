import { useCallback, useEffect, useMemo, useState } from "react"
import {
  AlertTriangle,
  Database,
  FileSearch,
  Loader2,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { cn } from "@/lib/utils"
import { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  WorkspaceIndexStatusDto,
  WorkspaceQueryModeDto,
  WorkspaceQueryResponseDto,
} from "@/src/lib/xero-model/project"
import { SectionHeader } from "./section-header"

interface WorkspaceIndexSectionProps {
  projectId: string | null
  projectLabel: string | null
}

type LoadState = "idle" | "loading" | "ready" | "error"
type Tone = "good" | "info" | "warn" | "bad" | "neutral"

const QUERY_MODES: Array<{ value: WorkspaceQueryModeDto; label: string }> = [
  { value: "semantic", label: "Semantic" },
  { value: "symbol", label: "Symbols" },
  { value: "related_tests", label: "Tests" },
  { value: "impact", label: "Impact" },
]

const TONE_CLASS: Record<Tone, string> = {
  good: "border-success/30 bg-success/[0.08] text-success",
  info: "border-info/30 bg-info/[0.08] text-info",
  warn: "border-warning/30 bg-warning/[0.08] text-warning",
  bad: "border-destructive/40 bg-destructive/[0.08] text-destructive",
  neutral: "border-border bg-secondary/60 text-foreground/70",
}

function Pill({
  tone,
  children,
}: {
  tone: Tone
  children: React.ReactNode
}) {
  return (
    <span
      className={cn(
        "inline-flex h-[18px] items-center gap-1 rounded-full border px-1.5 text-[10.5px] font-medium",
        TONE_CLASS[tone],
      )}
    >
      {children}
    </span>
  )
}

function indexStateTone(state: string | undefined): Tone {
  switch (state) {
    case "ready":
      return "good"
    case "stale":
      return "warn"
    case "indexing":
      return "info"
    case "error":
      return "bad"
    default:
      return "neutral"
  }
}

export function WorkspaceIndexSection({ projectId, projectLabel }: WorkspaceIndexSectionProps) {
  const [status, setStatus] = useState<WorkspaceIndexStatusDto | null>(null)
  const [query, setQuery] = useState("")
  const [mode, setMode] = useState<WorkspaceQueryModeDto>("semantic")
  const [queryResponse, setQueryResponse] = useState<WorkspaceQueryResponseDto | null>(null)
  const [loadState, setLoadState] = useState<LoadState>("idle")
  const [indexState, setIndexState] = useState<LoadState>("idle")
  const [queryState, setQueryState] = useState<LoadState>("idle")
  const [error, setError] = useState<string | null>(null)

  const loadStatus = useCallback(async () => {
    if (!projectId) return
    setLoadState("loading")
    setError(null)
    try {
      const next = await XeroDesktopAdapter.workspaceStatus(projectId)
      setStatus(next)
      setLoadState("ready")
    } catch (caught) {
      setLoadState("error")
      setError(errorMessage(caught, "Xero could not load the workspace index status."))
    }
  }, [projectId])

  useEffect(() => {
    void loadStatus()
  }, [loadStatus])

  const runIndex = useCallback(
    async (force: boolean) => {
      if (!projectId) return
      setIndexState("loading")
      setError(null)
      try {
        const response = await XeroDesktopAdapter.workspaceIndex({ projectId, force })
        setStatus(response.status)
        setIndexState("ready")
      } catch (caught) {
        setIndexState("error")
        setError(errorMessage(caught, "Xero could not rebuild the workspace index."))
      }
    },
    [projectId],
  )

  const resetIndex = useCallback(async () => {
    if (!projectId) return
    setIndexState("loading")
    setError(null)
    try {
      const next = await XeroDesktopAdapter.workspaceReset(projectId)
      setStatus(next)
      setQueryResponse(null)
      setIndexState("ready")
    } catch (caught) {
      setIndexState("error")
      setError(errorMessage(caught, "Xero could not reset the workspace index."))
    }
  }, [projectId])

  const runQuery = useCallback(async () => {
    if (!projectId || !query.trim()) return
    setQueryState("loading")
    setError(null)
    try {
      const response = await XeroDesktopAdapter.workspaceQuery({
        projectId,
        query: query.trim(),
        mode,
        limit: 8,
        paths: [],
      })
      setQueryResponse(response)
      setQueryState("ready")
    } catch (caught) {
      setQueryState("error")
      setError(errorMessage(caught, "Xero could not query the workspace index."))
    }
  }, [mode, projectId, query])

  const stateLabel = status?.state ?? "empty"
  const stateTone = indexStateTone(stateLabel)
  const completedAt = useMemo(() => formatStatusTime(status?.completedAt), [status?.completedAt])
  const coverage = Math.round(status?.coveragePercent ?? 0)
  const isIndexing = indexState === "loading"

  if (!projectId) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Workspace Index"
          description="Local semantic code search is project-bound."
        />
        <EmptyPanel
          icon={<Database className="h-4 w-4 text-muted-foreground/70" />}
          title="Select a project"
          body="The index is stored in Xero app data for the active project."
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Workspace Index"
        description={
          projectLabel
            ? `Local semantic code index for ${projectLabel}.`
            : "Local semantic code index."
        }
        actions={
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant="outline"
              className="h-8 gap-1.5 text-[12px]"
              onClick={loadStatus}
              disabled={loadState === "loading"}
              aria-label="Refresh workspace index status"
            >
              {loadState === "loading" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              Refresh
            </Button>
            <Button
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={() => void runIndex(false)}
              disabled={isIndexing}
              aria-label="Index workspace"
            >
              {isIndexing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Database className="h-3.5 w-3.5" />
              )}
              Index
            </Button>
          </div>
        }
      />

      <section className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
        <CountTile label="Indexed" value={String(status?.indexedFiles ?? 0)} tone="info" />
        <CountTile label="Total files" value={String(status?.totalFiles ?? 0)} tone="neutral" />
        <CountTile label="Symbols" value={String(status?.symbolCount ?? 0)} tone="neutral" />
        <CountTile label="Coverage" value={`${coverage}%`} tone={coverage >= 95 ? "good" : coverage >= 50 ? "info" : "warn"} />
      </section>

      <section className="flex flex-col gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <span className="text-[12.5px] font-semibold text-foreground">Index health</span>
            <Pill tone={stateTone}>{stateLabel.replace(/_/g, " ")}</Pill>
          </div>
          <span className="text-[11px] text-muted-foreground">{completedAt}</span>
        </div>
        <Progress value={coverage} className="h-1.5" />
        <div className="flex items-center justify-between gap-3">
          <span className="text-[11.5px] text-muted-foreground">
            {status
              ? `${status.indexedFiles} of ${status.totalFiles} files indexed`
              : "Status not loaded"}
          </span>
          <div className="flex items-center gap-1.5">
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1.5 text-[11.5px]"
              onClick={() => void runIndex(true)}
              disabled={isIndexing}
              aria-label="Rebuild index"
            >
              <RefreshCw className="h-3 w-3" />
              Rebuild
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1.5 text-[11.5px] text-muted-foreground hover:text-destructive"
              onClick={() => void resetIndex()}
              disabled={isIndexing}
              aria-label="Reset index"
            >
              <RotateCcw className="h-3 w-3" />
              Reset
            </Button>
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-2.5">
        <h4 className="text-[12.5px] font-semibold text-foreground">Query</h4>
        <div className="flex gap-1 rounded-md border border-border/60 bg-secondary/30 p-1">
          {QUERY_MODES.map((item) => (
            <button
              key={item.value}
              type="button"
              className={cn(
                "flex flex-1 items-center justify-center rounded-md px-2 py-1.5 text-[12px] font-medium transition-colors",
                mode === item.value
                  ? "bg-background text-foreground shadow-sm ring-1 ring-border/40"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setMode(item.value)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <div className="flex gap-2">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void runQuery()
            }}
            placeholder="Search files, symbols, tests, or impact"
            className="text-[12.5px]"
          />
          <Button
            size="sm"
            className="h-9 gap-1.5 text-[12px]"
            onClick={() => void runQuery()}
            disabled={queryState === "loading" || !query.trim()}
            aria-label="Run query"
          >
            {queryState === "loading" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Search className="h-3.5 w-3.5" />
            )}
            Query
          </Button>
        </div>
      </section>

      {error ? (
        <p
          role="alert"
          className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive"
        >
          <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
          <span>{error}</span>
        </p>
      ) : null}

      {queryResponse ? (
        <section className="flex flex-col gap-2.5">
          <h4 className="text-[12.5px] font-semibold text-foreground">
            Results
            <span className="ml-1.5 font-normal text-muted-foreground">
              {queryResponse.results.length}
            </span>
          </h4>
          {queryResponse.results.length === 0 ? (
            <EmptyPanel
              icon={<FileSearch className="h-4 w-4 text-muted-foreground/70" />}
              title="No matches"
              body="Try a different query or switch search mode."
            />
          ) : (
            <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
              {queryResponse.results.map((result) => (
                <ResultRow key={`${result.rank}-${result.path}`} result={result} />
              ))}
            </div>
          )}
        </section>
      ) : null}
    </div>
  )
}

interface ResultRowProps {
  result: WorkspaceQueryResponseDto["results"][number]
}

function ResultRow({ result }: ResultRowProps) {
  const score = Math.round(result.score * 100)
  const tone: Tone = score >= 80 ? "good" : score >= 50 ? "info" : "neutral"
  return (
    <div className="flex items-start gap-3 px-3.5 py-3">
      <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-secondary/40 text-muted-foreground">
        <FileSearch className="h-3.5 w-3.5" aria-hidden="true" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="truncate font-mono text-[12.5px] font-medium text-foreground">
            {result.path}
          </p>
          <Pill tone={tone}>{score}%</Pill>
        </div>
        {result.summary ? (
          <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">
            {result.summary}
          </p>
        ) : null}
        {result.reasons.length > 0 ? (
          <p className="mt-1 truncate text-[11px] text-muted-foreground/80">
            {result.reasons.slice(0, 3).join(" · ")}
          </p>
        ) : null}
        {result.diffs.length > 0 || result.failures.length > 0 ? (
          <p className="mt-1 line-clamp-2 text-[11px] text-muted-foreground/80">
            {[...result.diffs.slice(0, 2), ...result.failures.slice(0, 1)].join(" · ")}
          </p>
        ) : null}
      </div>
    </div>
  )
}

function CountTile({
  label,
  value,
  tone,
}: {
  label: string
  value: string
  tone: Tone
}) {
  return (
    <div className={cn("flex flex-col gap-0.5 rounded-md border px-3 py-2", TONE_CLASS[tone])}>
      <span className="truncate text-[18px] font-semibold leading-none text-foreground">
        {value}
      </span>
      <span className="text-[10.5px] uppercase tracking-[0.12em] text-current/80">{label}</span>
    </div>
  )
}

function EmptyPanel({
  icon,
  title,
  body,
}: {
  icon: React.ReactNode
  title: string
  body: string
}) {
  return (
    <div className="flex min-h-[160px] items-center justify-center rounded-md border border-dashed border-border/60 bg-secondary/10 px-6 text-center">
      <div className="max-w-sm">
        <div className="mx-auto flex h-7 w-7 items-center justify-center rounded-md border border-border/60 bg-secondary/40">
          {icon}
        </div>
        <p className="mt-3 text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-1 text-[11.5px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}

function formatStatusTime(value: string | null | undefined): string {
  if (!value) return "Never indexed"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return `Indexed ${date.toLocaleString()}`
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
