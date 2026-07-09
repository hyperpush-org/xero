import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { WorkflowCanvasEmptyState } from '@/components/xero/workflow-canvas-empty-state'
import type { RuntimeAgentIdDto } from '@/src/lib/xero-model/runtime'
import type { WorkflowAgentSummaryDto } from '@/src/lib/xero-model/workflow-agents'

function builtIn(id: RuntimeAgentIdDto, displayName: string): WorkflowAgentSummaryDto {
  return {
    ref: { kind: 'built_in', runtimeAgentId: id, version: 1 },
    displayName,
    shortLabel: displayName,
    description: '',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    lastUsedAt: null,
    useCount: 0,
  }
}

function template(definitionId: string, displayName: string): WorkflowAgentSummaryDto {
  return {
    ref: { kind: 'custom', definitionId, version: 1 },
    displayName,
    shortLabel: displayName,
    description: '',
    scope: 'global_custom',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    lastUsedAt: null,
    useCount: 0,
  }
}

describe('WorkflowCanvasEmptyState', () => {
  it('opens the starting-point dialog before creating an agent', () => {
    const onCreateAgent = vi.fn()

    render(<WorkflowCanvasEmptyState onCreateAgent={onCreateAgent} />)

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))
    expect(onCreateAgent).not.toHaveBeenCalled()
    expect(screen.getByRole('heading', { name: 'Create agent' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /New agent/ })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /From template/ })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /New agent/ }))

    expect(onCreateAgent).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('opens the create-workflow dialog and starts a blank workflow', () => {
    const onCreateWorkflow = vi.fn()

    render(<WorkflowCanvasEmptyState onCreateWorkflow={onCreateWorkflow} />)

    expect(screen.getByRole('heading', { name: 'Start with a workflow' })).toBeInTheDocument()
    expect(screen.queryByText('Workflows coming soon')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Create workflow/i }))
    expect(onCreateWorkflow).not.toHaveBeenCalled()
    expect(screen.getByRole('heading', { name: 'Create workflow' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Blank workflow/ }))
    expect(onCreateWorkflow).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('routes workflow template selection through onCreateWorkflowFromTemplate', () => {
    const onCreateWorkflowFromTemplate = vi.fn()

    render(
      <WorkflowCanvasEmptyState
        onCreateWorkflow={vi.fn()}
        onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Create workflow/i }))
    fireEvent.click(screen.getByRole('button', { name: /From template/ }))
    expect(screen.getByText(/Templates open as editable workflow drafts/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Plan, build, verify/ }))
    expect(onCreateWorkflowFromTemplate).toHaveBeenCalledWith('continuous_delivery')
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('routes the Agent Create choice through onCreateWorkflowWithAgentCreate', () => {
    const onCreateWorkflowWithAgentCreate = vi.fn()

    render(
      <WorkflowCanvasEmptyState
        onCreateWorkflow={vi.fn()}
        onCreateWorkflowWithAgentCreate={onCreateWorkflowWithAgentCreate}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Create workflow/i }))
    fireEvent.click(screen.getByRole('button', { name: /Use Agent Create/ }))
    expect(onCreateWorkflowWithAgentCreate).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('does not show a separate "Start from template" action in the main list', () => {
    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onCreateAgentFromTemplate={vi.fn()}
        templates={[template('def-1', 'Engineer Plus')]}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start from template' })).toBeNull()
  })

  it('opens a choice dialog when templates are available and Create agent is clicked', () => {
    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onCreateAgentFromTemplate={vi.fn()}
        templates={[template('def-1', 'Engineer Plus')]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))

    expect(screen.getByRole('heading', { name: 'Create agent' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /New agent/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /From template/ })).toBeInTheDocument()
  })

  it('routes the "New agent" choice through onCreateAgent and closes the dialog', () => {
    const onCreateAgent = vi.fn()

    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={onCreateAgent}
        onCreateAgentFromTemplate={vi.fn()}
        templates={[template('def-1', 'Engineer Plus')]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))
    fireEvent.click(screen.getByRole('button', { name: /New agent/ }))

    expect(onCreateAgent).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('button', { name: /From template/ })).toBeNull()
  })

  it('switches to the template picker when "From template" is chosen and routes selection', () => {
    const onCreateAgentFromTemplate = vi.fn()

    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onCreateAgentFromTemplate={onCreateAgentFromTemplate}
        templates={[template('def-1', 'Engineer Plus')]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))
    fireEvent.click(screen.getByRole('button', { name: /From template/ }))

    expect(screen.getByText(/Templates open on the canvas/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Engineer Plus/ }))
    expect(onCreateAgentFromTemplate).toHaveBeenCalledWith({
      kind: 'custom',
      definitionId: 'def-1',
      version: 1,
    })
  })

  it('hides crawl and agent_create built-ins from the template picker', () => {
    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onCreateAgentFromTemplate={vi.fn()}
        templates={[
          builtIn('ask', 'Ask'),
          builtIn('crawl', 'Crawl'),
          builtIn('agent_create', 'Agent Create'),
          builtIn('engineer', 'Engineer'),
        ]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))
    fireEvent.click(screen.getByRole('button', { name: /From template/ }))

    expect(screen.getByRole('button', { name: /Ask/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Engineer/ })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^Crawl/ })).toBeNull()
    expect(screen.queryByRole('button', { name: /Agent Create/ })).toBeNull()
  })

  it('returns to the choice view when Back is clicked from the template picker', () => {
    render(
      <WorkflowCanvasEmptyState
        onCreateAgent={vi.fn()}
        onCreateAgentFromTemplate={vi.fn()}
        templates={[template('def-1', 'Engineer Plus')]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Create agent' }))
    fireEvent.click(screen.getByRole('button', { name: /From template/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Back' }))

    expect(screen.getByRole('button', { name: /New agent/ })).toBeInTheDocument()
    expect(screen.queryByText(/Templates open on the canvas/i)).toBeNull()
  })

  it('runs browse-workflows when available and hides the action otherwise', () => {
    const onBrowseWorkflows = vi.fn()
    const { rerender } = render(
      <WorkflowCanvasEmptyState onCreateAgent={vi.fn()} onBrowseWorkflows={onBrowseWorkflows} />,
    )

    const runWorkflow = screen.getByRole('button', { name: /Run an existing workflow/i })
    expect(runWorkflow).toBeEnabled()
    expect(runWorkflow).not.toHaveTextContent('Coming soon')

    fireEvent.click(runWorkflow)
    expect(onBrowseWorkflows).toHaveBeenCalledTimes(1)

    rerender(<WorkflowCanvasEmptyState onCreateAgent={vi.fn()} />)
    expect(
      screen.queryByRole('button', { name: /Run an existing workflow/i }),
    ).not.toBeInTheDocument()
  })
})
