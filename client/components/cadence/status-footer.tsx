"use client"

import { formatDistanceToNow } from "date-fns"
import {
  Bell,
  CircleDot,
  Coins,
  DollarSign,
  GitBranch,
  GitCommit,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { formatTokenCount, formatMicrosUsd } from "@/src/lib/cadence-model/usage"

export interface FooterSpendData {
  totalTokens: number
  /** Cost in micros (1e-6 USD). */
  totalCostMicros?: number
}

export interface StatusFooterProps {
  git?: {
    branch?: string | null
    upstream?: {
      ahead?: number | null
      behind?: number | null
    } | null
    hasChanges?: boolean
    changedFiles?: number
    lastCommit?: {
      sha?: string | null
      message?: string | null
      committedAt?: string | null
    } | null
  } | null
  spend?: FooterSpendData | null
  notifications?: number
  /** Whether the spend section is currently active (sidebar open). */
  spendActive?: boolean
  onSpendClick?: () => void
}

type FooterGitUpstream = NonNullable<NonNullable<StatusFooterProps["git"]>["upstream"]>

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

export function StatusFooter({
  git = null,
  spend = null,
  notifications = 0,
  spendActive = false,
  onSpendClick,
}: StatusFooterProps) {
  const liveLastCommit = git?.lastCommit
  const liveLastCommitSha = formatShortSha(liveLastCommit?.sha)
  const liveLastCommitMessage = normalizeOptionalFooterText(liveLastCommit?.message)
  const liveLastCommitRelativeTime = formatRelativeCommitTime(liveLastCommit?.committedAt)
  const upstream = normalizeUpstream(git?.upstream)

  const branch = normalizeFooterText(git?.branch, "No branch")
  const workingTree = {
    dirty: git?.hasChanges ?? false,
    changedFiles: git?.changedFiles ?? 0,
  }
  const lastCommit =
    liveLastCommitSha && liveLastCommitMessage
      ? {
          shortSha: liveLastCommitSha,
          message: liveLastCommitMessage,
          relativeTime: liveLastCommitRelativeTime ?? "",
        }
      : {
          shortSha: "—",
          message: "No commits yet",
          relativeTime: "",
        }

  const tokensLabel = formatTokenCount(spend?.totalTokens ?? 0)
  const costLabel = resolveCostLabel(spend)
  const hasSpendData = (spend?.totalTokens ?? 0) > 0 || resolveCostMicros(spend) > 0

  const truncatedCommit =
    lastCommit.message.length > 46 ? `${lastCommit.message.slice(0, 46)}…` : lastCommit.message

  const spendAriaLabel = hasSpendData
    ? `Project spend: ${tokensLabel} tokens, ${costLabel}`
    : "Project spend: no usage recorded yet"

  return (
    <footer
      aria-label="Status bar"
      className="flex h-8 shrink-0 items-center justify-between gap-3 border-t border-border bg-sidebar px-3 text-[11px] leading-none text-muted-foreground"
    >
      {/* Left: git branch + working tree + last commit ----------------------- */}
      <div className="flex min-w-0 items-center gap-3">
        <span className="flex items-center gap-1.5">
          <GitBranch className="h-3 w-3" />
          <span className="font-medium text-foreground/80">{branch}</span>
          {upstream ? (
            <span className="text-muted-foreground/70">
              ↑{upstream.ahead} ↓{upstream.behind}
            </span>
          ) : null}
        </span>

        <Divider />

        <span className="flex items-center gap-1.5">
          <CircleDot
            className={cn(
              "h-3 w-3",
              workingTree.dirty ? "text-amber-500" : "text-emerald-500",
            )}
          />
          <span>
            {workingTree.dirty
              ? `${workingTree.changedFiles} change${workingTree.changedFiles === 1 ? "" : "s"}`
              : "clean"}
          </span>
        </span>

        <Divider />

        <span className="flex min-w-0 items-center gap-1.5">
          <GitCommit className="h-3 w-3 shrink-0" />
          <span className="font-mono text-foreground/70">{lastCommit.shortSha}</span>
          <span className="truncate">{truncatedCommit}</span>
          {lastCommit.relativeTime ? (
            <span className="shrink-0 text-muted-foreground/70">· {lastCommit.relativeTime}</span>
          ) : null}
        </span>
      </div>

      {/* Right: spend · notifications -------------------------------------- */}
      <div className="flex shrink-0 items-center gap-3">
        <button
          type="button"
          onClick={onSpendClick}
          aria-label={spendAriaLabel}
          aria-pressed={spendActive}
          title={hasSpendData ? "View project usage breakdown" : "No usage recorded yet"}
          className={cn(
            "flex items-center gap-1.5 rounded px-1.5 py-0.5 -my-0.5 transition-colors",
            "hover:bg-foreground/5 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            spendActive && "bg-foreground/10 text-foreground",
            !onSpendClick && "cursor-default",
          )}
        >
          <Coins className="h-3 w-3" />
          <span>{tokensLabel} tok</span>
          <span className="text-muted-foreground/60">·</span>
          <DollarSign className="h-3 w-3" />
          <span className="font-medium text-foreground/80">{costLabel}</span>
        </button>

        <Divider />

        <span className="flex items-center gap-1.5" aria-label={`${notifications} unread notifications`}>
          <Bell className="h-3 w-3" />
          <span>{notifications}</span>
        </span>
      </div>
    </footer>
  )
}

function resolveCostMicros(spend: FooterSpendData | null | undefined): number {
  if (!spend) return 0
  return typeof spend.totalCostMicros === "number" ? spend.totalCostMicros : 0
}

function resolveCostLabel(spend: FooterSpendData | null | undefined): string {
  return formatMicrosUsd(resolveCostMicros(spend))
}

function normalizeFooterText(value: string | null | undefined, fallback: string): string {
  const trimmed = value?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : fallback
}

function normalizeOptionalFooterText(value: string | null | undefined): string | null {
  const trimmed = value?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : null
}

function formatShortSha(value: string | null | undefined): string | null {
  const trimmed = value?.trim()
  if (!trimmed || trimmed === "No HEAD") {
    return null
  }

  return trimmed.slice(0, 7)
}

function normalizeUpstream(
  value: FooterGitUpstream | null | undefined,
): { ahead: number; behind: number } | null {
  if (!value) {
    return null
  }

  return {
    ahead: normalizeCount(value.ahead),
    behind: normalizeCount(value.behind),
  }
}

function normalizeCount(value: number | null | undefined): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.floor(value) : 0
}

function formatRelativeCommitTime(value: string | null | undefined): string | null {
  const trimmed = value?.trim()
  if (!trimmed) {
    return null
  }

  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.getTime())) {
    return null
  }

  return formatDistanceToNow(parsed, { addSuffix: true })
}

// ---------------------------------------------------------------------------
// Internal pieces
// ---------------------------------------------------------------------------

function Divider({ className }: { className?: string }) {
  return <span aria-hidden="true" className={cn("h-3 w-px bg-border", className)} />
}
