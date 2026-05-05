import { useCallback, useEffect, useMemo, useState } from "react"
import { Database, Loader2, RefreshCw, RotateCcw, Search } from "lucide-react"
import { Badge } from "@/components/ui/badge"
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

const QUERY_MODES: Array<{ value: WorkspaceQueryModeDto; label: string }> = [
  { value: "semantic", label: "Semantic" },
  { value: "symbol", label: "Symbols" },
  { value: "related_tests", label: "Tests" },
  { value: "impact", label: "Impact" },
]

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

  const runIndex = useCallback(async (force: boolean) => {
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
  }, [projectId])

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
  const stateTone = stateLabel === "ready" ? "default" : stateLabel === "stale" ? "secondary" : "outline"
  const completedAt = useMemo(() => formatStatusTime(status?.completedAt), [status?.completedAt])

  if (!projectId) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Workspace Index"
          description="Local semantic code search is project-bound."
        />
        <div className="flex min-h-[220px] items-center justify-center rounded-lg border border-border/60 bg-card/30 text-center">
          <div className="max-w-sm px-6">
            <Database className="mx-auto h-4 w-4 text-muted-foreground/70" />
            <p className="mt-3 text-[13px] font-medium text-foreground">Select a project</p>
            <p className="mt-1.5 text-[12px] leading-[1.55] text-muted-foreground">
              The index is stored in Xero app data for the active project.
            </p>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Workspace Index"
        description={projectLabel ? `Local semantic code index for ${projectLabel}.` : "Local semantic code index."}
      />

      <section className="rounded-lg border border-border/60 bg-card/30 p-4">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2">
              <h4 className="text-[13px] font-semibold text-foreground">Index health</h4>
              <Badge variant={stateTone}>{stateLabel.replace("_", " ")}</Badge>
            </div>
            <p className="mt-1 text-[12px] text-muted-foreground">
              {status ? `${status.indexedFiles} of ${status.totalFiles} files · ${status.symbolCount} symbols` : "Status not loaded"}
            </p>
          </div>
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={loadStatus} disabled={loadState === "loading"}>
              {loadState === "loading" ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
              Refresh
            </Button>
            <Button size="sm" onClick={() => void runIndex(false)} disabled={indexState === "loading"}>
              {indexState === "loading" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Database className="h-4 w-4" />}
              Index
            </Button>
          </div>
        </div>

        <div className="mt-4">
          <Progress value={status?.coveragePercent ?? 0} className="h-2" />
          <div className="mt-2 flex items-center justify-between text-[11.5px] text-muted-foreground">
            <span>{Math.round(status?.coveragePercent ?? 0)}% coverage</span>
            <span>{completedAt}</span>
          </div>
        </div>

        <div className="mt-4 flex gap-2">
          <Button size="sm" variant="outline" onClick={() => void runIndex(true)} disabled={indexState === "loading"}>
            <RefreshCw className="h-4 w-4" />
            Rebuild
          </Button>
          <Button size="sm" variant="ghost" onClick={() => void resetIndex()} disabled={indexState === "loading"}>
            <RotateCcw className="h-4 w-4" />
            Reset
          </Button>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Query</h4>
        <div className="flex gap-1 rounded-md border border-border/60 bg-secondary/30 p-1">
          {QUERY_MODES.map((item) => (
            <button
              key={item.value}
              type="button"
              className={cn(
                "flex flex-1 items-center justify-center rounded-md px-2 py-1.5 text-[12.5px] font-medium transition-colors",
                mode === item.value ? "bg-background text-foreground shadow-sm ring-1 ring-border/40" : "text-muted-foreground hover:text-foreground",
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
          />
          <Button onClick={() => void runQuery()} disabled={queryState === "loading" || !query.trim()}>
            {queryState === "loading" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
            Query
          </Button>
        </div>
      </section>

      {error ? (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          {error}
        </p>
      ) : null}

      {queryResponse ? (
        <section className="flex flex-col gap-2">
          {queryResponse.results.map((result) => (
            <div key={`${result.rank}-${result.path}`} className="rounded-lg border border-border/60 bg-card/30 p-3">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate text-[13px] font-medium text-foreground">{result.path}</p>
                  <p className="mt-1 line-clamp-2 text-[12px] leading-[1.5] text-muted-foreground">{result.summary}</p>
                </div>
                <Badge variant="outline">{Math.round(result.score * 100)}%</Badge>
              </div>
              {result.reasons.length > 0 ? (
                <p className="mt-2 text-[11.5px] text-muted-foreground">{result.reasons.slice(0, 3).join(" · ")}</p>
              ) : null}
              {result.diffs.length > 0 || result.failures.length > 0 ? (
                <p className="mt-2 line-clamp-2 text-[11.5px] text-muted-foreground">
                  {[...result.diffs.slice(0, 2), ...result.failures.slice(0, 1)].join(" · ")}
                </p>
              ) : null}
            </div>
          ))}
        </section>
      ) : null}
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
