import { z } from 'zod'

export const projectUsageTotalsSchema = z.object({
  runCount: z.number().int().nonnegative(),
  inputTokens: z.number().int().nonnegative(),
  outputTokens: z.number().int().nonnegative(),
  totalTokens: z.number().int().nonnegative(),
  cacheReadTokens: z.number().int().nonnegative(),
  cacheCreationTokens: z.number().int().nonnegative(),
  estimatedCostMicros: z.number().int().nonnegative(),
  lastUpdatedAt: z.string().min(1).optional(),
})

export const projectUsageModelBreakdownSchema = z.object({
  providerId: z.string().min(1),
  modelId: z.string().min(1),
  runCount: z.number().int().nonnegative(),
  inputTokens: z.number().int().nonnegative(),
  outputTokens: z.number().int().nonnegative(),
  totalTokens: z.number().int().nonnegative(),
  cacheReadTokens: z.number().int().nonnegative(),
  cacheCreationTokens: z.number().int().nonnegative(),
  estimatedCostMicros: z.number().int().nonnegative(),
  lastUpdatedAt: z.string().min(1).optional(),
})

export const projectUsageSummarySchema = z.object({
  projectId: z.string().min(1),
  totals: projectUsageTotalsSchema,
  byModel: z.array(projectUsageModelBreakdownSchema),
})

export const agentUsageUpdatedPayloadSchema = z.object({
  projectId: z.string().min(1),
  runId: z.string().min(1),
})

export type ProjectUsageTotalsDto = z.infer<typeof projectUsageTotalsSchema>
export type ProjectUsageModelBreakdownDto = z.infer<typeof projectUsageModelBreakdownSchema>
export type ProjectUsageSummaryDto = z.infer<typeof projectUsageSummarySchema>
export type AgentUsageUpdatedPayloadDto = z.infer<typeof agentUsageUpdatedPayloadSchema>

export const AGENT_USAGE_UPDATED_EVENT = 'agent_usage_updated'

/** Convert micros (1e-6 USD) to a fractional dollar number for display. */
export function microsToUsd(micros: number): number {
  return micros / 1_000_000
}

/** "1.28M tok" / "150.0K tok" / "523 tok" — short token-count for footers/cards. */
export function formatTokenCount(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0'
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
  return `${Math.round(value)}`
}

/**
 * Format a USD amount with magnitude-aware precision so small spend
 * (sub-cent) doesn't render as "$0.00" and large spend doesn't show
 * unnecessary trailing zeros.
 */
export function formatUsd(usd: number): string {
  if (!Number.isFinite(usd) || usd <= 0) return '$0.00'
  if (usd < 0.01) return `$${usd.toFixed(4)}`
  if (usd < 1) return `$${usd.toFixed(3)}`
  return `$${usd.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`
}

export function formatMicrosUsd(micros: number): string {
  return formatUsd(microsToUsd(micros))
}
