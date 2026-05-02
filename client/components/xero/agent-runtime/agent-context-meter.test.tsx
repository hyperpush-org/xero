import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { AgentContextMeter } from '@/components/xero/agent-runtime/agent-context-meter'
import {
  XERO_SESSION_CONTEXT_CONTRACT_VERSION,
  createPublicSessionContextRedaction,
  type SessionContextBudgetDto,
  type SessionContextSnapshotDto,
} from '@/src/lib/xero-model/session-context'

const baseBudget: SessionContextBudgetDto = {
  budgetTokens: 100_000,
  contextWindowTokens: 128_000,
  effectiveInputBudgetTokens: 100_000,
  maxOutputTokens: 16_000,
  outputReserveTokens: 16_000,
  safetyReserveTokens: 12_000,
  remainingTokens: 58_000,
  pressurePercent: 42,
  estimatedTokens: 42_000,
  estimationSource: 'estimated',
  pressure: 'medium',
  knownProviderBudget: true,
  limitSource: 'live_catalog',
  limitConfidence: 'high',
  limitDiagnostic: 'OpenRouter reported context_length.',
  limitFetchedAt: '2026-05-01T14:00:00Z',
}

function makeSnapshot(overrides: {
  budget?: SessionContextBudgetDto
  modelId?: string
} = {}): SessionContextSnapshotDto {
  const budget = overrides.budget ?? baseBudget
  return {
    contractVersion: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
    snapshotId: 'snapshot-1',
    projectId: 'project-1',
    agentSessionId: 'agent-session-1',
    runId: 'run-1',
    providerId: 'openrouter',
    modelId: overrides.modelId ?? 'gpt-5.4',
    generatedAt: '2026-05-01T14:02:00Z',
    budget,
    providerRequestHash: 'a'.repeat(64),
    includedTokenEstimate: budget.estimatedTokens,
    deferredTokenEstimate: 0,
    codeMap: {
      generatedFromRoot: 'xero',
      sourceRoots: [],
      packageManifests: [],
      symbols: [],
      redaction: createPublicSessionContextRedaction(),
    },
    diff: null,
    contributors: [
      {
        contributorId: 'conversation-tail',
        kind: 'conversation_tail',
        label: 'Recent conversation',
        promptFragmentId: null,
        promptFragmentPriority: null,
        promptFragmentHash: null,
        promptFragmentProvenance: null,
        projectId: 'project-1',
        agentSessionId: 'agent-session-1',
        runId: 'run-1',
        sourceId: 'run-1',
        sequence: 1,
        estimatedTokens: budget.estimatedTokens,
        estimatedChars: budget.estimatedTokens * 4,
        recencyScore: 100,
        relevanceScore: 85,
        authorityScore: 70,
        rankScore: 850,
        taskPhase: 'execute',
        disposition: 'include',
        included: true,
        modelVisible: true,
        summary: null,
        omittedReason: null,
        text: 'Recent model-visible conversation context.',
        redaction: createPublicSessionContextRedaction(),
      },
    ],
    policyDecisions: [
      {
        contractVersion: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        decisionId: 'policy-1',
        kind: 'compaction',
        action: 'none',
        trigger: 'auto',
        reasonCode: 'budget_ok',
        message: 'Context budget is safe.',
        rawTranscriptPreserved: true,
        modelVisible: false,
        redaction: createPublicSessionContextRedaction(),
      },
    ],
    usageTotals: null,
    redaction: createPublicSessionContextRedaction(),
  }
}

describe('AgentContextMeter', () => {
  it('shows known budget pressure with progress semantics from the backend projection', () => {
    render(
      <AgentContextMeter
        status="ready"
        snapshot={makeSnapshot()}
      />,
    )

    expect(screen.getByText('58% left')).toHaveClass('hidden', 'sm:inline')
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '42')
    expect(screen.getByRole('progressbar')).toHaveAttribute(
      'aria-valuetext',
      '58 percent context remaining for gpt-5.4',
    )
  })

  it('masks known system prompt usage until the first user message is sent', () => {
    render(
      <AgentContextMeter
        status="ready"
        snapshot={makeSnapshot()}
        hasUserMessage={false}
      />,
    )

    expect(screen.getByText('Full')).toHaveClass('hidden', 'sm:inline')
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0')
    expect(screen.getByRole('progressbar')).toHaveAttribute(
      'aria-valuetext',
      '100 percent context remaining for gpt-5.4',
    )
  })

  it('keeps unknown model budgets explicit and avoids fake progress percentages', () => {
    const unknownBudget: SessionContextBudgetDto = {
      ...baseBudget,
      budgetTokens: null,
      contextWindowTokens: null,
      effectiveInputBudgetTokens: null,
      maxOutputTokens: null,
      remainingTokens: null,
      pressurePercent: null,
      pressure: 'unknown',
      knownProviderBudget: false,
      limitSource: 'unknown',
      limitConfidence: 'unknown',
      limitDiagnostic: 'No context-window metadata is available.',
      limitFetchedAt: null,
    }

    render(
      <AgentContextMeter
        status="ready"
        snapshot={makeSnapshot({ budget: unknownBudget })}
      />,
    )

    expect(screen.getByRole('button', { name: /context meter: context unknown/i })).toBeVisible()
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
  })

  it('reports over-budget overflow as a filled danger progress state', () => {
    const overBudget: SessionContextBudgetDto = {
      ...baseBudget,
      remainingTokens: 0,
      pressurePercent: 105,
      estimatedTokens: 105_000,
      pressure: 'over',
    }

    render(
      <AgentContextMeter
        status="ready"
        snapshot={makeSnapshot({ budget: overBudget })}
      />,
    )

    expect(screen.getByText('5K over')).toBeVisible()
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')
    expect(screen.getByRole('progressbar')).toHaveAttribute(
      'aria-valuetext',
      '0 percent context remaining for gpt-5.4',
    )
  })

  it('renders an unavailable state when refresh fails before a snapshot exists', () => {
    render(
      <AgentContextMeter
        status="error"
        snapshot={null}
      />,
    )

    expect(screen.getByRole('button', { name: /context meter: context unavailable/i })).toBeVisible()
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
  })
})
