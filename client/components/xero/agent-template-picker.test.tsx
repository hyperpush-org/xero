import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { AgentTemplatePicker } from '@/components/xero/agent-template-picker'
import type { RuntimeAgentIdDto } from '@/src/lib/xero-model/runtime'
import type { WorkflowAgentSummaryDto } from '@/src/lib/xero-model/workflow-agents'

function builtIn(
  id: RuntimeAgentIdDto,
  displayName: string,
  profile: WorkflowAgentSummaryDto['baseCapabilityProfile'],
): WorkflowAgentSummaryDto {
  return {
    ref: { kind: 'built_in', runtimeAgentId: id, version: 1 },
    displayName,
    shortLabel: displayName,
    description: `${displayName} description`,
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: profile,
    lastUsedAt: null,
    useCount: 0,
  }
}

function custom(
  definitionId: string,
  displayName: string,
  profile: WorkflowAgentSummaryDto['baseCapabilityProfile'],
  options: { lifecycleState?: WorkflowAgentSummaryDto['lifecycleState']; lastUsedAt?: string | null } = {},
): WorkflowAgentSummaryDto {
  return {
    ref: { kind: 'custom', definitionId, version: 1 },
    displayName,
    shortLabel: displayName,
    description: `${displayName} description`,
    scope: 'project_custom',
    lifecycleState: options.lifecycleState ?? 'active',
    baseCapabilityProfile: profile,
    lastUsedAt: options.lastUsedAt ?? null,
    useCount: 0,
  }
}

describe('AgentTemplatePicker', () => {
  it('renders built-in and custom templates in grouped sections', () => {
    const agents: WorkflowAgentSummaryDto[] = [
      builtIn('engineer', 'Engineer', 'engineering'),
      builtIn('plan', 'Plan', 'planning'),
      custom('def-1', 'My Planner', 'planning'),
    ]

    render(
      <AgentTemplatePicker
        agents={agents}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    expect(screen.getByText(/Engineer$/)).toBeInTheDocument()
    expect(screen.getByText('Plan')).toBeInTheDocument()
    expect(screen.getByText('My Planner')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: /Built-in/ })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: /Your agents/ })).toBeInTheDocument()
  })

  it('filters out archived templates', () => {
    const agents: WorkflowAgentSummaryDto[] = [
      custom('def-1', 'Active Agent', 'engineering'),
      custom('def-2', 'Archived Agent', 'engineering', { lifecycleState: 'archived' }),
    ]

    render(
      <AgentTemplatePicker
        agents={agents}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    expect(screen.getByText('Active Agent')).toBeInTheDocument()
    expect(screen.queryByText('Archived Agent')).not.toBeInTheDocument()
  })

  it('sorts custom agents by lastUsedAt descending', () => {
    const agents: WorkflowAgentSummaryDto[] = [
      custom('older', 'Older Agent', 'engineering', { lastUsedAt: '2026-01-01T00:00:00Z' }),
      custom('newer', 'Newer Agent', 'engineering', { lastUsedAt: '2026-05-01T00:00:00Z' }),
    ]

    render(
      <AgentTemplatePicker
        agents={agents}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    const yourSection = screen.getByRole('heading', { name: /Your agents/ }).closest('section')!
    const buttons = within(yourSection).getAllByRole('button')
    expect(buttons[0]).toHaveTextContent('Newer Agent')
    expect(buttons[1]).toHaveTextContent('Older Agent')
  })

  it('invokes onSelectTemplate with the ref when a template is clicked', () => {
    const onSelectTemplate = vi.fn()
    const agents: WorkflowAgentSummaryDto[] = [
      builtIn('engineer', 'Engineer', 'engineering'),
    ]

    render(
      <AgentTemplatePicker
        agents={agents}
        onSelectTemplate={onSelectTemplate}
        onStartBlank={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Engineer/ }))
    expect(onSelectTemplate).toHaveBeenCalledTimes(1)
    expect(onSelectTemplate.mock.calls[0][0]).toEqual({
      kind: 'built_in',
      runtimeAgentId: 'engineer',
      version: 1,
    })
  })

  it('invokes onStartBlank when the blank action is clicked', () => {
    const onStartBlank = vi.fn()

    render(
      <AgentTemplatePicker
        agents={[]}
        onSelectTemplate={vi.fn()}
        onStartBlank={onStartBlank}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Start blank/ }))
    expect(onStartBlank).toHaveBeenCalledTimes(1)
  })

  it('renders a loading state when loading is true', () => {
    render(
      <AgentTemplatePicker
        agents={[]}
        loading
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    expect(screen.getByText(/Loading templates/)).toBeInTheDocument()
  })

  it('renders an error message when error is provided', () => {
    render(
      <AgentTemplatePicker
        agents={[]}
        error={new Error('Network down')}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    expect(screen.getByText('Network down')).toBeInTheDocument()
  })

  it('shows an empty hint when there are no templates and no error', () => {
    render(
      <AgentTemplatePicker
        agents={[]}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
      />,
    )

    expect(
      screen.getByText(/No templates available yet — start blank/i),
    ).toBeInTheDocument()
  })

  it('renders a back button when onCancel is provided', () => {
    const onCancel = vi.fn()

    render(
      <AgentTemplatePicker
        agents={[]}
        onSelectTemplate={vi.fn()}
        onStartBlank={vi.fn()}
        onCancel={onCancel}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Back' }))
    expect(onCancel).toHaveBeenCalledTimes(1)
  })
})
