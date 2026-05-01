import { AlertTriangle, RefreshCw } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from '@/components/ui/hover-card'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type {
  SessionContextBudgetDto,
  SessionContextContributorKindDto,
  SessionContextSnapshotDto,
} from '@/src/lib/xero-model/session-context'

import type { OperatorActionErrorView } from '@/src/features/xero/use-xero-desktop-state/types'

export type AgentContextMeterStatus = 'idle' | 'loading' | 'ready' | 'stale' | 'error'

interface AgentContextMeterProps {
  status: AgentContextMeterStatus
  snapshot: SessionContextSnapshotDto | null
  error: OperatorActionErrorView | null
  onRefresh?: () => void
}

const RING_RADIUS = 8.5
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS

const CONTRIBUTOR_LABELS: Record<SessionContextContributorKindDto, string> = {
  system_prompt: 'System prompt',
  instruction_file: 'Instructions',
  skill_context: 'Skills',
  approved_memory: 'Memory',
  compaction_summary: 'Compaction',
  conversation_tail: 'Conversation',
  tool_result: 'Tool results',
  tool_summary: 'Tool summaries',
  tool_descriptor: 'Tools',
  file_observation: 'Files',
  code_symbol: 'Code map',
  dependency_metadata: 'Dependencies',
  run_artifact: 'Run artifacts',
  provider_usage: 'Usage totals',
}

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

function formatSource(value: SessionContextBudgetDto['limitSource']): string {
  switch (value) {
    case 'live_catalog':
      return 'Live catalog'
    case 'app_profile':
      return 'Provider profile'
    case 'built_in_registry':
      return 'Built-in registry'
    case 'heuristic':
      return 'Heuristic'
    case 'unknown':
      return 'Unknown'
  }
}

function formatConfidence(value: SessionContextBudgetDto['limitConfidence']): string {
  switch (value) {
    case 'high':
      return 'High'
    case 'medium':
      return 'Medium'
    case 'low':
      return 'Low'
    case 'unknown':
      return 'Unknown'
  }
}

function formatPolicyAction(value: string | null | undefined): string {
  switch (value) {
    case 'compact_now':
      return 'Compact before continuing'
    case 'blocked':
      return 'Blocked'
    case 'skipped':
      return 'Skipped'
    case 'inject_memory':
      return 'Inject memory'
    case 'exclude_memory':
      return 'No memory injection'
    case 'include_instruction':
      return 'Include instructions'
    case 'none':
      return 'Continue'
    default:
      return 'Continue'
  }
}

function formatTimestamp(value: string | null | undefined): string {
  if (!value) {
    return 'Not refreshed yet'
  }

  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }

  return date.toLocaleTimeString(undefined, {
    hour: 'numeric',
    minute: '2-digit',
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

function getTopContributorGroups(snapshot: SessionContextSnapshotDto | null) {
  if (!snapshot) {
    return []
  }

  const groups = new Map<SessionContextContributorKindDto, number>()
  for (const contributor of snapshot.contributors) {
    if (!contributor.included || !contributor.modelVisible) {
      continue
    }

    groups.set(
      contributor.kind,
      (groups.get(contributor.kind) ?? 0) + contributor.estimatedTokens,
    )
  }

  return Array.from(groups.entries())
    .sort((left, right) => right[1] - left[1])
    .slice(0, 4)
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="max-w-[11rem] truncate text-right font-medium text-foreground">{value}</dd>
    </div>
  )
}

export function AgentContextMeter({
  status,
  snapshot,
  error,
  onRefresh,
}: AgentContextMeterProps) {
  const budget = snapshot?.budget ?? null
  const knownBudget = Boolean(budget?.knownProviderBudget && budget.pressurePercent != null)
  const pressure = knownBudget ? Math.min(100, budget?.pressurePercent ?? 0) : 0
  const label = getBudgetLabel(status, budget)
  const ringTone = getRingTone(budget, status)
  const fillOffset = RING_CIRCUMFERENCE * (1 - pressure / 100)
  const primaryPolicy = snapshot?.policyDecisions.find((decision) => decision.kind === 'compaction') ?? snapshot?.policyDecisions[0] ?? null
  const topContributorGroups = getTopContributorGroups(snapshot)
  const tooltip = knownBudget
    ? `${label} for ${snapshot?.modelId ?? 'the selected model'}`
    : 'Context window unknown'
  const ariaValueText = knownBudget
    ? `${Math.max(0, 100 - pressure)} percent context remaining for ${snapshot?.modelId ?? 'the selected model'}`
    : label

  return (
    <HoverCard openDelay={160} closeDelay={80}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="inline-flex">
            <HoverCardTrigger asChild>
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
                  <svg className="h-5 w-5" viewBox="0 0 20 20" aria-hidden="true">
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
                <span className="hidden max-w-[6.25rem] truncate sm:inline">{label}</span>
              </button>
            </HoverCardTrigger>
          </span>
        </TooltipTrigger>
        <TooltipContent side="top">{tooltip}</TooltipContent>
      </Tooltip>
      <HoverCardContent align="end" side="top" className="w-80 p-0">
        <div className="border-b border-border/60 px-3.5 py-3">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate text-[13px] font-semibold text-foreground">
                {snapshot ? `${snapshot.providerId} / ${snapshot.modelId}` : 'Context meter'}
              </p>
              <p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
                {knownBudget
                  ? 'Backend projection for the next provider request.'
                  : "Xero can estimate the next request size, but this model's context window is not known."}
              </p>
            </div>
            {onRefresh && (status === 'error' || status === 'stale') ? (
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                className="h-7 w-7 shrink-0"
                aria-label="Refresh context meter"
                onClick={onRefresh}
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
            ) : null}
          </div>
          {error ? (
            <p className="mt-2 rounded-md border border-destructive/25 bg-destructive/5 px-2 py-1.5 text-[11px] leading-relaxed text-destructive">
              {error.message}
            </p>
          ) : null}
        </div>
        <dl className="space-y-1.5 px-3.5 py-3 text-[11px]">
          <DetailRow label="Context left" value={budget ? formatTokens(budget.remainingTokens) : 'Unknown'} />
          <DetailRow label="Next-turn estimate" value={budget ? formatTokens(budget.estimatedTokens) : 'Unknown'} />
          <DetailRow label="Effective budget" value={budget ? formatTokens(budget.effectiveInputBudgetTokens) : 'Unknown'} />
          <DetailRow label="Model window" value={budget ? formatTokens(budget.contextWindowTokens) : 'Unknown'} />
          <DetailRow label="Output reserve" value={budget ? formatTokens(budget.outputReserveTokens) : 'Unknown'} />
          <DetailRow label="Safety reserve" value={budget ? formatTokens(budget.safetyReserveTokens) : 'Unknown'} />
          <DetailRow label="Budget source" value={budget ? formatSource(budget.limitSource) : 'Unknown'} />
          <DetailRow label="Confidence" value={budget ? formatConfidence(budget.limitConfidence) : 'Unknown'} />
          <DetailRow label="Pressure" value={budget?.pressure ?? 'unknown'} />
          <DetailRow label="Policy" value={formatPolicyAction(primaryPolicy?.action)} />
          <DetailRow
            label="Compaction summary"
            value={
              snapshot?.contributors.some((contributor) => contributor.kind === 'compaction_summary' && contributor.modelVisible)
                ? 'Active'
                : 'None'
            }
          />
          <DetailRow label="Refreshed" value={formatTimestamp(snapshot?.generatedAt)} />
        </dl>
        {topContributorGroups.length > 0 ? (
          <div className="border-t border-border/60 px-3.5 py-3">
            <p className="mb-2 text-[11px] font-semibold text-muted-foreground">Top context groups</p>
            <div className="space-y-1.5">
              {topContributorGroups.map(([kind, tokens]) => (
                <div key={kind} className="flex items-center justify-between gap-3 text-[11px]">
                  <span className="truncate text-muted-foreground">{CONTRIBUTOR_LABELS[kind]}</span>
                  <span className="shrink-0 font-medium text-foreground">{formatTokens(tokens)}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}
        {budget?.limitDiagnostic ? (
          <p className="border-t border-border/60 px-3.5 py-2.5 text-[11px] leading-relaxed text-muted-foreground">
            {budget.limitDiagnostic}
          </p>
        ) : null}
      </HoverCardContent>
    </HoverCard>
  )
}
