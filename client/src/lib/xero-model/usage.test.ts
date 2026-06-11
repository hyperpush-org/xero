import { describe, expect, it } from 'vitest'

import { summarizeProjectUsageSpend, type ProjectUsageSummaryDto } from './usage'

function usageSummary(overrides: Partial<ProjectUsageSummaryDto> = {}): ProjectUsageSummaryDto {
  return {
    projectId: 'project-1',
    totals: {
      runCount: 2,
      inputTokens: 10,
      billableInputTokens: 10,
      outputTokens: 5,
      totalTokens: 15,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      estimatedCostMicros: 200,
    },
    byModel: [],
    ...overrides,
  }
}

describe('summarizeProjectUsageSpend', () => {
  it('sums footer spend from every model row when breakdown data is available', () => {
    const spend = summarizeProjectUsageSpend(
      usageSummary({
        totals: {
          runCount: 2,
          inputTokens: 1,
          billableInputTokens: 1,
          outputTokens: 1,
          totalTokens: 2,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          estimatedCostMicros: 2,
        },
        byModel: [
          {
            providerId: 'anthropic',
            modelId: 'claude-sonnet-4-6',
            runCount: 1,
            inputTokens: 100,
            billableInputTokens: 75,
            outputTokens: 50,
            totalTokens: 175,
            cacheReadTokens: 20,
            cacheCreationTokens: 5,
            estimatedCostMicros: 1_000_000,
          },
          {
            providerId: 'openai_codex',
            modelId: 'gpt-5.1',
            runCount: 1,
            inputTokens: 25,
            billableInputTokens: 25,
            outputTokens: 10,
            totalTokens: 35,
            cacheReadTokens: 0,
            cacheCreationTokens: 0,
            estimatedCostMicros: 250_000,
          },
        ],
      }),
    )

    expect(spend).toEqual({
      totalTokens: 210,
      totalCostMicros: 1_250_000,
    })
  })

  it('falls back to top-level totals for empty projects or legacy summaries', () => {
    expect(summarizeProjectUsageSpend(usageSummary())).toEqual({
      totalTokens: 15,
      totalCostMicros: 200,
    })
  })
})
