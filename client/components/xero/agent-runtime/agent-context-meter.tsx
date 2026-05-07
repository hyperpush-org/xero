import { AlertTriangle } from 'lucide-react'

import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type {
  SessionContextBudgetDto,
  SessionContextSnapshotDto,
} from '@/src/lib/xero-model/session-context'

export type AgentContextMeterStatus = 'idle' | 'loading' | 'ready' | 'stale' | 'error'

interface AgentContextMeterProps {
  status: AgentContextMeterStatus
  snapshot: SessionContextSnapshotDto | null
  hasUserMessage?: boolean
  error?: {
    message?: string | null
  } | null
}

const RING_RADIUS = 8.5
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS

function formatTokens(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) {
    return 'Unknown'
  }

  if (value >= 1_000_000) {
    return `${trimDecimal(value / 1_000_000)}M`
  }

  if (value >= 1_000) {
    return `${trimDecimal(value / 1_000)}K`
  }

  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value)
}

function trimDecimal(value: number): string {
  return value.toLocaleString(undefined, {
    maximumFractionDigits: value >= 10 ? 0 : 1,
    minimumFractionDigits: 0,
  })
}

function getBudgetLabel(status: AgentContextMeterStatus, budget: SessionContextBudgetDto | null): string {
  if (status === 'loading' && !budget) return 'Context'
  if (status === 'error' && !budget) return 'Context unavailable'
  if (!budget || !budget.knownProviderBudget) return 'Context unknown'

  if (budget.pressure === 'over') {
    const overflow = Math.max(0, budget.estimatedTokens - (budget.effectiveInputBudgetTokens ?? 0))
    return `${formatTokens(overflow)} over`
  }

  const remaining = budget.remainingTokens ?? 0
  const pressurePercent = Math.min(100, budget.pressurePercent ?? 0)
  const remainingPercent = Math.max(0, 100 - pressurePercent)
  if (remainingPercent >= 95) return 'Full'
  if (remaining < 20_000) return `${formatTokens(remaining)} left`
  return `${remainingPercent}% left`
}

function getRingTone(budget: SessionContextBudgetDto | null, status: AgentContextMeterStatus): string {
  if (status === 'error') return 'stroke-destructive text-destructive'
  if (!budget?.knownProviderBudget) return 'stroke-muted-foreground/55 text-muted-foreground'

  switch (budget.pressure) {
    case 'low':
      return 'stroke-primary/65 text-primary'
    case 'medium':
      return 'stroke-sky-500 text-sky-600 dark:text-sky-400'
    case 'high':
      return 'stroke-amber-500 text-amber-600 dark:text-amber-400'
    case 'over':
      return 'stroke-destructive text-destructive'
    case 'unknown':
      return 'stroke-muted-foreground/55 text-muted-foreground'
  }
}

function getErrorMessage(error: AgentContextMeterProps['error']): string | null {
  const message = error?.message?.trim()
  return message && message.length > 0 ? message : null
}

export function AgentContextMeter({
  status,
  snapshot,
  hasUserMessage = true,
  error = null,
}: AgentContextMeterProps) {
  const budget = snapshot?.budget ?? null
  const knownBudget = Boolean(budget?.knownProviderBudget && budget.pressurePercent != null)
  const hideBaselineUsage = knownBudget && !hasUserMessage
  const pressure = knownBudget && !hideBaselineUsage ? Math.min(100, budget?.pressurePercent ?? 0) : 0
  const remainingPercent = knownBudget ? Math.max(0, 100 - pressure) : null
  const label = hideBaselineUsage ? 'Full' : getBudgetLabel(status, budget)
  const ringTone = hideBaselineUsage ? 'stroke-primary/65 text-primary' : getRingTone(budget, status)
  const fillOffset = RING_CIRCUMFERENCE * (1 - pressure / 100)
  const errorMessage = status === 'error' && !snapshot ? getErrorMessage(error) : null
  const undoContributorCount =
    snapshot?.contributors?.filter((contributor) =>
      [
        'code_rollback',
        'code_history_operation',
        'code_history_notice',
        'code_history_mailbox_notice',
      ].includes(contributor.kind),
    ).length ?? 0
  const undoContext =
    undoContributorCount > 0
      ? `Code undo is in context; current files are authoritative and history stayed visible.`
      : null
  const tooltip =
    errorMessage ??
    undoContext ??
    (remainingPercent != null ? `${remainingPercent}% remaining` : label)
  const ariaValueText = knownBudget
    ? `${remainingPercent} percent context remaining for ${snapshot?.modelId ?? 'the selected model'}`
    : errorMessage
      ? `${label}: ${errorMessage}`
    : label

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className={cn(
            'inline-flex h-8 min-w-[2rem] items-center gap-1.5 rounded-md px-1.5 text-[12px] font-medium text-muted-foreground transition-colors',
            'hover:bg-muted/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45',
          )}
          aria-label={`Context meter: ${ariaValueText}`}
        >
          <span
            className="relative inline-flex h-5 w-5 shrink-0 items-center justify-center"
            role={knownBudget ? 'progressbar' : 'status'}
            aria-valuemin={knownBudget ? 0 : undefined}
            aria-valuemax={knownBudget ? 100 : undefined}
            aria-valuenow={knownBudget ? pressure : undefined}
            aria-valuetext={ariaValueText}
          >
            <svg className="h-4.5 w-4.5" viewBox="0 0 20 20" aria-hidden="true">
              <circle
                className="fill-none stroke-muted/80"
                cx="10"
                cy="10"
                r={RING_RADIUS}
                strokeWidth="2.5"
              />
              <circle
                className={cn(
                  'origin-center -rotate-90 fill-none transition-[stroke-dashoffset,stroke] duration-300',
                  status === 'loading' || status === 'stale'
                    ? 'motion-safe:animate-spin'
                    : null,
                  ringTone,
                )}
                cx="10"
                cy="10"
                r={RING_RADIUS}
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeDasharray={
                  knownBudget
                    ? `${RING_CIRCUMFERENCE} ${RING_CIRCUMFERENCE}`
                    : status === 'loading' || status === 'stale'
                      ? '13 40'
                      : '2.5 4.5'
                }
                strokeDashoffset={knownBudget ? fillOffset : 0}
              />
            </svg>
            {status === 'error' && !snapshot ? (
              <AlertTriangle className="absolute h-2.5 w-2.5 text-destructive" />
            ) : null}
          </span>
        </button>
      </TooltipTrigger>
      <TooltipContent side="top">{tooltip}</TooltipContent>
    </Tooltip>
  )
}
