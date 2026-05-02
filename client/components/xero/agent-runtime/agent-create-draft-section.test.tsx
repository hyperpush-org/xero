import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { AgentCreateDraftSection } from '@/components/xero/agent-runtime/agent-create-draft-section'
import type { RuntimeStreamToolItemView } from '@/src/lib/xero-model'

function makeToolItem(overrides: Partial<RuntimeStreamToolItemView> = {}): RuntimeStreamToolItemView {
  return {
    id: 'tool-1',
    runId: 'run-1',
    sequence: 1,
    createdAt: '2026-05-01T12:00:00Z',
    kind: 'tool',
    toolCallId: 'tool-call-1',
    toolName: 'agent_definition',
    toolState: 'succeeded',
    detail: 'Drafted agent definition `team_research` for review.',
    toolSummary: null,
    ...overrides,
  }
}

describe('AgentCreateDraftSection', () => {
  it('renders the empty primer when no agent_definition tool calls exist yet', () => {
    const onOpen = vi.fn()
    render(
      <AgentCreateDraftSection
        runtimeStreamItems={[]}
        pendingApprovalCount={0}
        onOpenAgentManagement={onOpen}
      />,
    )

    expect(screen.getByText(/Describe the agent you want/i)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /Manage agents/i }))
    expect(onOpen).toHaveBeenCalledTimes(1)
  })

  it('shows recent agent_definition tool activity and surfaces pending approval count', () => {
    render(
      <AgentCreateDraftSection
        runtimeStreamItems={[
          makeToolItem({ id: 'tool-1', detail: 'Drafted agent definition `team_research` for review.' }),
          makeToolItem({
            id: 'tool-2',
            sequence: 2,
            toolState: 'failed',
            detail: 'Agent definition `team_research` failed validation with 2 diagnostic(s).',
          }),
        ]}
        pendingApprovalCount={2}
      />,
    )

    expect(screen.getByText('2 pending approvals')).toBeInTheDocument()
    expect(screen.getByText(/Drafted agent definition/)).toBeInTheDocument()
    expect(screen.getByText(/failed validation/)).toBeInTheDocument()
  })

  it('ignores non agent_definition tool items', () => {
    render(
      <AgentCreateDraftSection
        runtimeStreamItems={[
          {
            id: 'tool-other',
            runId: 'run-1',
            sequence: 1,
            createdAt: '2026-05-01T12:00:00Z',
            kind: 'tool',
            toolCallId: 'tool-call-other',
            toolName: 'project_record',
            toolState: 'succeeded',
            detail: 'Saved record',
            toolSummary: null,
          },
        ]}
        pendingApprovalCount={0}
      />,
    )

    expect(screen.getByText(/Describe the agent you want/i)).toBeInTheDocument()
    expect(screen.queryByText('Saved record')).not.toBeInTheDocument()
  })
})
