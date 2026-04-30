"use client"

import { useMemo, useState } from "react"
import {
  Activity,
  BadgeCheck,
  Loader2,
  Radio,
  RefreshCw,
  Rows4,
  Trash2,
  XCircle,
} from "lucide-react"
import { cn } from "@/lib/utils"
import type {
  ClusterKind,
  LogDecodedEventPayload,
  LogEntry,
  LogFilter,
  LogsActiveSubscription,
  LogsRecentResponse,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaLogFeedProps {
  cluster: ClusterKind
  busy: boolean
  entries: LogEntry[]
  decodedEvents: LogDecodedEventPayload[]
  activeSubscriptions: LogsActiveSubscription[]
  lastFetch: LogsRecentResponse | null
  onSubscribe: (filter: LogFilter) => Promise<string | null>
  onUnsubscribe: (token: string) => Promise<boolean>
  onFetchRecent: (args: {
    cluster: ClusterKind
    programIds?: string[]
    lastN?: number
    rpcUrl?: string | null
    cachedOnly?: boolean
  }) => Promise<LogsRecentResponse | null>
  onRefreshSubscriptions: () => Promise<void>
  onClear: () => void
}

type FeedFilter = "all" | "errors" | "events"

export function SolanaLogFeed({
  cluster,
  busy,
  entries,
  decodedEvents,
  activeSubscriptions,
  lastFetch,
  onSubscribe,
  onUnsubscribe,
  onFetchRecent,
  onRefreshSubscriptions,
  onClear,
}: SolanaLogFeedProps) {
  const [programIdsInput, setProgramIdsInput] = useState("")
  const [lastNInput, setLastNInput] = useState("25")
  const [includeDecoded, setIncludeDecoded] = useState(true)
  const [selectedToken, setSelectedToken] = useState<string | null>(null)
  const [feedFilter, setFeedFilter] = useState<FeedFilter>("all")
  const [status, setStatus] = useState<string | null>(null)

  const programIds = useMemo(
    () =>
      programIdsInput
        .split(/[\s,]+/)
        .map((value) => value.trim())
        .filter(Boolean),
    [programIdsInput],
  )

  const orderedEntries = useMemo(
    () => [...entries].sort((a, b) => (b.receivedMs ?? 0) - (a.receivedMs ?? 0)),
    [entries],
  )

  const filteredEntries = useMemo(() => {
    switch (feedFilter) {
      case "errors":
        return orderedEntries.filter((entry) => !entry.explanation.ok)
      case "events":
        return orderedEntries.filter((entry) => entry.anchorEvents.length > 0)
      default:
        return orderedEntries
    }
  }, [feedFilter, orderedEntries])

  const errorCount = useMemo(
    () => orderedEntries.filter((entry) => !entry.explanation.ok).length,
    [orderedEntries],
  )
  const eventCount = useMemo(
    () => orderedEntries.filter((entry) => entry.anchorEvents.length > 0).length,
    [orderedEntries],
  )

  const handleSubscribe = async () => {
    if (programIds.length === 0) {
      setStatus("Provide at least one program id before subscribing.")
      return
    }
    const token = await onSubscribe({
      cluster,
      programIds,
      includeDecoded,
    })
    if (token) {
      setSelectedToken(token)
      setStatus(`Subscribed (${token})`)
      return
    }
    setStatus("Subscribe failed")
  }

  const handleUnsubscribe = async () => {
    if (!selectedToken) {
      setStatus("Pick a subscription token first.")
      return
    }
    const ok = await onUnsubscribe(selectedToken)
    if (ok) {
      setSelectedToken(null)
      setStatus("Subscription stopped")
    } else {
      setStatus("Unsubscribe failed")
    }
  }

  const handleFetchRecent = async () => {
    const parsedLastN = Number.parseInt(lastNInput, 10)
    const response = await onFetchRecent({
      cluster,
      programIds,
      lastN: Number.isFinite(parsedLastN) ? parsedLastN : 25,
      cachedOnly: false,
    })
    if (response) {
      setStatus(`Fetched ${response.fetched} tx log entries`)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <section className="space-y-1.5">
        <div className="text-[11.5px] font-medium text-foreground">Live log stream</div>
        <input
          aria-label="Program IDs"
          className="h-8 rounded-md border border-border/60 bg-background px-2.5 text-[12px] outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/60"
          onChange={(event) => setProgramIdsInput(event.target.value)}
          placeholder="Program IDs (comma-separated)"
          value={programIdsInput}
        />
        <div className="flex items-center gap-1.5">
          <input
            aria-label="Last N"
            className="h-8 w-20 rounded-md border border-border/60 bg-background px-2 text-[12px] outline-none transition-colors focus:border-primary/60"
            inputMode="numeric"
            onChange={(event) => setLastNInput(event.target.value)}
            value={lastNInput}
          />
          <label className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <input
              checked={includeDecoded}
              onChange={(event) => setIncludeDecoded(event.target.checked)}
              type="checkbox"
            />
            decoded events
          </label>
          <button
            type="button"
            onClick={handleFetchRecent}
            disabled={busy}
            className={cn(
              "ml-auto inline-flex h-8 items-center gap-1 rounded-md border border-border/60 bg-background px-2 text-[11px] text-foreground/85 transition-colors",
              "hover:border-primary/40 hover:text-foreground disabled:opacity-50",
            )}
          >
            {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <Rows4 className="h-3 w-3" />}
            Fetch
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          <button
            type="button"
            onClick={handleSubscribe}
            disabled={busy}
            className={cn(
              "inline-flex h-8 items-center gap-1 rounded-md border border-primary/50 bg-primary/10 px-2 text-[11px] font-medium text-primary",
              "hover:bg-primary/20 disabled:opacity-50",
            )}
          >
            <Radio className="h-3 w-3" />
            Subscribe
          </button>
          <button
            type="button"
            onClick={handleUnsubscribe}
            disabled={busy || !selectedToken}
            className={cn(
              "inline-flex h-8 items-center gap-1 rounded-md border border-border/60 bg-background px-2 text-[11px] text-foreground/80",
              "hover:border-destructive/60 hover:text-destructive disabled:opacity-50",
            )}
          >
            <XCircle className="h-3 w-3" />
            Unsubscribe
          </button>
          <button
            type="button"
            onClick={() => void onRefreshSubscriptions()}
            className="inline-flex h-8 items-center gap-1 rounded-md border border-border/60 bg-background px-2 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
          >
            <RefreshCw className="h-3 w-3" />
            Tokens
          </button>
          <button
            type="button"
            onClick={onClear}
            className="inline-flex h-8 items-center gap-1 rounded-md border border-border/60 bg-background px-2 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
          >
            <Trash2 className="h-3 w-3" />
            Clear feed
          </button>
        </div>

        {status ? <p className="text-[11px] text-muted-foreground">{status}</p> : null}
        {selectedToken ? (
          <p className="truncate font-mono text-[10.5px] text-muted-foreground">
            active token: {selectedToken}
          </p>
        ) : null}
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <div className="text-[11px] uppercase tracking-wider text-muted-foreground">
            Subscriptions
          </div>
          <span className="text-[11px] text-muted-foreground">{activeSubscriptions.length}</span>
        </div>
        {activeSubscriptions.length === 0 ? (
          <p className="text-[11px] text-muted-foreground">No active subscriptions.</p>
        ) : (
          <ul className="space-y-1">
            {activeSubscriptions.map((subscription) => (
              <li
                key={subscription.token}
                className={cn(
                  "rounded-md border border-border/50 bg-background/40 px-2 py-1",
                  selectedToken === subscription.token && "border-primary/50",
                )}
              >
                <button
                  type="button"
                  onClick={() => setSelectedToken(subscription.token)}
                  className="w-full text-left"
                >
                  <div className="truncate font-mono text-[10.5px] text-foreground/80">
                    {subscription.token}
                  </div>
                  <div className="truncate text-[10.5px] text-muted-foreground">
                    {subscription.filter.cluster} · {subscription.filter.programIds.join(", ") || "(all)"}
                  </div>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <div className="text-[11px] uppercase tracking-wider text-muted-foreground">
            Feed
          </div>
          <div className="text-[10.5px] text-muted-foreground">
            decoded events: {decodedEvents.length}
          </div>
        </div>

        <div className="flex flex-wrap gap-1">
          <FilterChip
            active={feedFilter === "all"}
            count={orderedEntries.length}
            label="All"
            onClick={() => setFeedFilter("all")}
          />
          <FilterChip
            active={feedFilter === "errors"}
            count={errorCount}
            label="Errors"
            onClick={() => setFeedFilter("errors")}
          />
          <FilterChip
            active={feedFilter === "events"}
            count={eventCount}
            label="Anchor events"
            onClick={() => setFeedFilter("events")}
          />
        </div>

        {lastFetch ? (
          <p className="text-[11px] text-muted-foreground">
            Last fetch · {lastFetch.fetched} entries from {lastFetch.cluster}
          </p>
        ) : null}

        {filteredEntries.length === 0 ? (
          <p className="text-[11.5px] text-muted-foreground">
            No log entries yet. Subscribe or fetch recent logs to populate the feed.
          </p>
        ) : (
          <ul className="space-y-1.5">
            {filteredEntries.map((entry) => (
              <li
                key={`${entry.cluster}:${entry.signature}:${entry.slot ?? "na"}`}
                className="rounded-md border border-border/50 bg-background/40 px-2 py-1.5"
              >
                <details>
                  <summary className="cursor-pointer list-none">
                    <div className="flex items-center justify-between gap-2">
                      <div className="min-w-0">
                        <div className="truncate font-mono text-[10.5px] text-foreground/80">
                          {shortSig(entry.signature)}
                        </div>
                        <div className="truncate text-[10.5px] text-muted-foreground">
                          slot {entry.slot ?? "?"} · {entry.programsInvoked.join(", ") || "unknown program"}
                        </div>
                      </div>
                      <div className="flex items-center gap-1 text-[10.5px]">
                        {entry.explanation.ok ? (
                          <BadgeCheck className="h-3.5 w-3.5 text-emerald-400" />
                        ) : (
                          <XCircle className="h-3.5 w-3.5 text-destructive" />
                        )}
                        {entry.anchorEvents.length > 0 ? (
                          <Activity className="h-3.5 w-3.5 text-primary" />
                        ) : null}
                      </div>
                    </div>
                    <div className="mt-1 text-[11px] text-foreground/85">
                      {entry.explanation.summary}
                    </div>
                  </summary>
                  <div className="mt-2 space-y-2">
                    {entry.anchorEvents.length > 0 ? (
                      <div className="text-[11px] text-foreground/85">
                        events: {entry.anchorEvents.map((event) => event.eventName ?? event.discriminatorHex).join(", ")}
                      </div>
                    ) : null}
                    <pre className="max-h-36 overflow-auto rounded border border-border/50 bg-background p-2 font-mono text-[10px] text-foreground/80">
                      {entry.rawLogs.join("\n")}
                    </pre>
                  </div>
                </details>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

function shortSig(value: string): string {
  if (value.length <= 16) return value
  return `${value.slice(0, 8)}…${value.slice(-8)}`
}

function FilterChip({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[10.5px] transition-colors",
        active
          ? "border-primary/50 bg-primary/10 text-primary"
          : "border-border/60 bg-background text-muted-foreground hover:text-foreground",
      )}
    >
      <span>{label}</span>
      <span className="font-mono tabular-nums">{count}</span>
    </button>
  )
}
