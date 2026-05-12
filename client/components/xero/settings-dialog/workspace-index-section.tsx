import { useCallback, useEffect, useMemo, useState } from "react"
import {
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
import {
  EmptyPanel,
  ErrorBanner,
  InlineCounts,
  Pill,
  SubHeading,
  type Tone,
} from "./_shared"

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

function coverageTone(coverage: number): Tone {
  if (coverage >= 95) return "good"
  if (coverage >= 50) return "info"
  return "warn"
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
  const isEmpty = stateLabel === "empty" || (status?.indexedFiles ?? 0) === 0
  const stateDisplay = stateLabel.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase())

  if (!projectId) {
    return (
      <div className="flex flex-col gap-7">
        <SectionHeader
          title="Workspace Index"
          description="Local semantic code search is project-bound."
        />
        <EmptyPanel
          icon={<Database className="h-5 w-5 text-muted-foreground/70" />}
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
            {!isEmpty ? (
              <Button
                size="sm"
                className="h-8 gap-1.5 text-[12px]"
                onClick={() => void runIndex(false)}
                disabled={isIndexing}
                aria-label="Update workspace index"
              >
                {isIndexing ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Database className="h-3.5 w-3.5" />
                )}
                Update
              </Button>
            ) : null}
          </div>
        }
      />

      <section className="rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
        <div className="flex items-start gap-2.5">
          <Database className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-[12.5px] font-semibold text-foreground">Index health</p>
              <Pill tone={stateTone}>{stateDisplay}</Pill>
              <span className="ml-auto text-[11px] text-muted-foreground">{completedAt}</span>
            </div>

            {isEmpty ? (
              <p className="mt-2.5 text-[11.5px] leading-[1.55] text-muted-foreground">
                Indexing scans <span className="font-medium text-foreground">{status?.totalFiles ?? 0}</span> files to enable
                semantic, symbol, test, and impact queries. Nothing leaves your machine.
              </p>
            ) : (
              <>
                <div className="mt-2 flex items-center gap-2.5">
                  <Progress value={coverage} className="h-1.5 flex-1" />
                  <span className="shrink-0 text-[11.5px] font-medium tabular-nums text-foreground">
                    {coverage}%
                  </span>
                </div>
                <InlineCounts
                  className="mt-2.5"
                  items={[
                    {
                      label: "Indexed",
                      value: status?.indexedFiles ?? 0,
                      tone: coverageTone(coverage),
                    },
                    {
                      label: "Total",
                      value: status?.totalFiles ?? 0,
                    },
                    {
                      label: "Symbols",
                      value: status?.symbolCount ?? 0,
                    },
                  ]}
                />
              </>
            )}

            <div className="mt-3 flex items-center gap-1.5">
              {isEmpty ? (
                <Button
                  size="sm"
                  className="h-7 gap-1.5 text-[11.5px]"
                  onClick={() => void runIndex(false)}
                  disabled={isIndexing}
                  aria-label="Build index"
                >
                  {isIndexing ? <Loader2 className="h-3 w-3 animate-spin" /> : <Database className="h-3 w-3" />}
                  {isIndexing ? "Building" : "Build index"}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 gap-1.5 text-[11.5px]"
                  onClick={() => void runIndex(true)}
                  disabled={isIndexing}
                  aria-label="Rebuild index"
                >
                  {isIndexing ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                  Rebuild
                </Button>
              )}
              {!isEmpty ? (
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
              ) : null}
            </div>
          </div>
        </div>
      </section>

      <section className={cn("flex flex-col gap-2.5", isEmpty && "opacity-60")}>
        <SubHeading>Query</SubHeading>
        <div className="flex gap-1 rounded-md border border-border/60 bg-secondary/30 p-1">
          {QUERY_MODES.map((item) => (
            <button
              key={item.value}
              type="button"
              disabled={isEmpty}
              className={cn(
                "flex flex-1 items-center justify-center rounded-md px-2 py-1.5 text-[12px] font-medium transition-colors",
                mode === item.value
                  ? "bg-background text-foreground shadow-sm ring-1 ring-border/40"
                  : "text-muted-foreground hover:text-foreground",
                isEmpty && "cursor-not-allowed hover:text-muted-foreground",
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
            placeholder={isEmpty ? "Build the index to enable search" : "Search files, symbols, tests, or impact"}
            className="text-[12.5px]"
            disabled={isEmpty}
          />
          <Button
            size="sm"
            className="h-9 gap-1.5 text-[12px]"
            onClick={() => void runQuery()}
            disabled={isEmpty || queryState === "loading" || !query.trim()}
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

      {error ? <ErrorBanner message={error} /> : null}

      {queryResponse ? (
        <section className="flex flex-col gap-2.5">
          <SubHeading count={queryResponse.results.length}>Results</SubHeading>
          {queryResponse.results.length === 0 ? (
            <EmptyPanel
              icon={<FileSearch className="h-5 w-5 text-muted-foreground/70" />}
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

function formatStatusTime(value: string | null | undefined): string {
  if (!value) return "Never indexed"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return `Indexed ${date.toLocaleString()}`
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
