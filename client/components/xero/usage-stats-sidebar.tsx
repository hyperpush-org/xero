"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { RefreshCw, X } from "lucide-react"

import { cn } from "@/lib/utils"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { FloatingRightSidebarFrame } from "@/components/xero/floating-right-sidebar-frame"
import {
  formatMicrosUsd,
  formatTokenCount,
  type ProjectUsageModelBreakdownDto,
  type ProjectUsageSummaryDto,
} from "@/src/lib/xero-model/usage"

const MIN_WIDTH = 320
const DEFAULT_WIDTH = 420
const MAX_WIDTH = 720
const RIGHT_PADDING = 280
const WIDTH_STORAGE_KEY = "xero.usageStats.width"

export interface UsageStatsSidebarProps {
  open: boolean
  projectId: string | null
  projectName?: string | null
  summary: ProjectUsageSummaryDto | null
  loadError?: string | null
  onClose?: () => void
  onRefresh?: (projectId: string) => Promise<unknown>
}

function viewportMaxWidth(): number {
  if (typeof window === "undefined") return MAX_WIDTH
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, window.innerWidth - RIGHT_PADDING))
}

function clampWidth(width: number, max = viewportMaxWidth()): number {
  return Math.max(MIN_WIDTH, Math.min(max, width))
}

function readPersistedWidth(): number | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(WIDTH_STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return clampWidth(parsed)
  } catch {
    return null
  }
}

function writePersistedWidth(width: number): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage.setItem(WIDTH_STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable */
  }
}

const PROVIDER_LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  openai_api: "OpenAI",
  openai_codex: "OpenAI Codex",
  openrouter: "OpenRouter",
  github_models: "GitHub Models",
  azure_openai: "Azure OpenAI",
  gemini_ai_studio: "Gemini",
  bedrock: "AWS Bedrock",
  vertex: "Vertex AI",
  ollama: "Ollama (local)",
}

function providerLabel(providerId: string): string {
  return PROVIDER_LABELS[providerId] ?? providerId
}

function formatRelative(timestamp: string | null | undefined): string | null {
  if (!timestamp) return null
  const parsed = Date.parse(timestamp)
  if (!Number.isFinite(parsed)) return null
  const diffMs = Date.now() - parsed
  const minutes = Math.round(diffMs / 60_000)
  if (minutes < 1) return "just now"
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.round(hours / 24)
  return `${days}d ago`
}

export function UsageStatsSidebar(props: UsageStatsSidebarProps) {
  const { open, summary, projectId, loadError, onClose, onRefresh } = props
  const [width, setWidth] = useState<number>(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const widthRef = useRef(width)
  widthRef.current = width

  useEffect(() => {
    if (typeof window === "undefined") return
    const handle = () => setWidth((current) => clampWidth(current))
    window.addEventListener("resize", handle)
    return () => window.removeEventListener("resize", handle)
  }, [])

  const handleResizeStart = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    let latestWidth = startWidth
    const widthUpdates = createFrameCoalescer<number>({
      onFlush: setWidth,
    })
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"

    const handleMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX
      latestWidth = clampWidth(startWidth + delta)
      widthUpdates.schedule(latestWidth)
    }
    const handleUp = () => {
      widthUpdates.flush()
      window.removeEventListener("pointermove", handleMove)
      window.removeEventListener("pointerup", handleUp)
      window.removeEventListener("pointercancel", handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
      writePersistedWidth(latestWidth)
    }

    window.addEventListener("pointermove", handleMove)
    window.addEventListener("pointerup", handleUp)
    window.addEventListener("pointercancel", handleUp)
  }, [])

  const totals = summary?.totals
  const breakdown = summary?.byModel ?? []
  const lastUpdated = formatRelative(totals?.lastUpdatedAt)

  const topModelShare = useMemo(() => computeTopModelShare(breakdown), [breakdown])

  const handleRefresh = async () => {
    if (!projectId || !onRefresh || isRefreshing) return
    setIsRefreshing(true)
    try {
      await onRefresh(projectId)
    } finally {
      setIsRefreshing(false)
    }
  }

  return (
    <FloatingRightSidebarFrame
      label="Project usage statistics"
      onOverlayClick={() => onClose?.()}
      open={open}
      width={width}
    >
      <div
        aria-label="Resize usage panel"
        aria-orientation="vertical"
        aria-valuemax={viewportMaxWidth()}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div className="flex min-h-0 flex-1 flex-col">
        <header className="flex items-center justify-between gap-2 border-b border-border/60 px-2 py-1">
          <div className="min-w-0">
            <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
              Project usage
            </p>
          </div>
          <div className="flex items-center gap-1">
            {onRefresh && projectId ? (
              <button
                type="button"
                onClick={handleRefresh}
                aria-label="Refresh usage"
                disabled={isRefreshing}
                className={cn(
                  "inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors",
                  "hover:bg-foreground/10 hover:text-foreground",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                  isRefreshing && "opacity-60",
                )}
              >
                <RefreshCw className={cn("h-3.5 w-3.5", isRefreshing && "animate-spin")} />
              </button>
            ) : null}
            <button
              type="button"
              onClick={onClose}
              aria-label="Close usage panel"
              className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </header>

        <div className="flex-1 overflow-y-auto">
          {!projectId ? (
            <EmptyMessage>Select a project to see its usage.</EmptyMessage>
          ) : loadError ? (
            <ErrorMessage message={loadError} />
          ) : !totals ? (
            <EmptyMessage>Loading usage…</EmptyMessage>
          ) : totals.runCount === 0 ? (
            <EmptyMessage>No agent runs recorded for this project yet.</EmptyMessage>
          ) : (
            <div className="px-5 py-5">
              {/* Totals — flat numbers, no card */}
              <section>
                <div className="grid grid-cols-2 gap-6">
                  <Stat
                    label="Total tokens"
                    value={formatTokenCount(totals.totalTokens)}
                    sublabel={`${totals.runCount} run${totals.runCount === 1 ? "" : "s"}`}
                  />
                  <Stat
                    label="Estimated cost"
                    value={formatMicrosUsd(totals.estimatedCostMicros)}
                    sublabel={lastUpdated ? `Updated ${lastUpdated}` : undefined}
                  />
                </div>
                <dl className="mt-5 grid grid-cols-2 gap-x-6 gap-y-1.5 border-t border-border/40 pt-4 text-[11px]">
                  <TokenBucket label="Input" value={totals.inputTokens} />
                  <TokenBucket label="Output" value={totals.outputTokens} />
                  <TokenBucket label="Cache read" value={totals.cacheReadTokens} />
                  <TokenBucket label="Cache write" value={totals.cacheCreationTokens} />
                </dl>
              </section>

              {/* Per-model breakdown — hairline-divided rows, no per-row cards */}
              <section className="mt-6 border-t border-border/40 pt-4">
                <div className="flex items-center justify-between">
                  <h3 className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                    By model
                  </h3>
                  {topModelShare > 0 ? (
                    <span className="text-[11px] text-muted-foreground/80">
                      Top {topModelShare}%
                    </span>
                  ) : null}
                </div>
                <ul className="mt-1 divide-y divide-border/40">
                  {breakdown.map((row) => (
                    <ModelRow
                      key={`${row.providerId}:${row.modelId}`}
                      row={row}
                      totalCostMicros={totals.estimatedCostMicros}
                    />
                  ))}
                </ul>
              </section>
            </div>
          )}
        </div>
      </div>
    </FloatingRightSidebarFrame>
  )
}

function computeTopModelShare(breakdown: ProjectUsageModelBreakdownDto[]): number {
  if (breakdown.length === 0) return 0
  const total = breakdown.reduce((sum, row) => sum + row.estimatedCostMicros, 0)
  if (total <= 0) return 0
  const top = breakdown[0]?.estimatedCostMicros ?? 0
  return Math.round((top / total) * 100)
}

function Stat(props: {
  label: string
  value: string
  sublabel?: string
}) {
  return (
    <div>
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {props.label}
      </p>
      <p className="mt-1 text-2xl font-semibold tabular-nums leading-none">
        {props.value}
      </p>
      {props.sublabel ? (
        <p className="mt-1 text-[11px] text-muted-foreground/80">{props.sublabel}</p>
      ) : null}
    </div>
  )
}

function TokenBucket({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono tabular-nums text-foreground/85">
        {formatTokenCount(value)}
      </dd>
    </div>
  )
}

function ModelRow({
  row,
  totalCostMicros,
}: {
  row: ProjectUsageModelBreakdownDto
  totalCostMicros: number
}) {
  const sharePercent =
    totalCostMicros > 0 ? Math.round((row.estimatedCostMicros / totalCostMicros) * 100) : 0
  return (
    <li className="py-3">
      <div className="flex items-baseline justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[11px] text-muted-foreground">
            {providerLabel(row.providerId)}
          </p>
          <p className="truncate font-mono text-[13px] leading-tight text-foreground/95">
            {row.modelId}
          </p>
        </div>
        <div className="shrink-0 text-right">
          <p className="text-sm font-semibold tabular-nums leading-tight">
            {formatMicrosUsd(row.estimatedCostMicros)}
          </p>
          <p className="text-[11px] text-muted-foreground">
            {formatTokenCount(row.totalTokens)} tok · {row.runCount} run
            {row.runCount === 1 ? "" : "s"}
          </p>
        </div>
      </div>
      {totalCostMicros > 0 ? (
        <div className="mt-2 h-px w-full overflow-hidden bg-foreground/5">
          <div
            className="h-full bg-primary/50"
            style={{ width: `${Math.min(100, Math.max(2, sharePercent))}%` }}
          />
        </div>
      ) : null}
      <div className="mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">
        <span>in {formatTokenCount(row.inputTokens)}</span>
        <span>out {formatTokenCount(row.outputTokens)}</span>
        {row.cacheReadTokens > 0 ? (
          <span>cache·r {formatTokenCount(row.cacheReadTokens)}</span>
        ) : null}
        {row.cacheCreationTokens > 0 ? (
          <span>cache·w {formatTokenCount(row.cacheCreationTokens)}</span>
        ) : null}
      </div>
    </li>
  )
}

function EmptyMessage({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center px-6 py-10 text-center text-xs text-muted-foreground">
      {children}
    </div>
  )
}

function ErrorMessage({ message }: { message: string }) {
  return (
    <div className="px-6 py-10 text-center">
      <p className="text-sm font-medium text-destructive">Could not load usage</p>
      <p className="mt-1 text-xs text-muted-foreground">{message}</p>
    </div>
  )
}
