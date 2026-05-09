import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentSummaryDto } from '@/src/lib/xero-model/workflow-agents'

import { WorkflowsSidebar } from './workflows-sidebar'

const REAL_AGENTS: WorkflowAgentSummaryDto[] = [
  {
    ref: { kind: 'built_in', runtimeAgentId: 'engineer', version: 1 },
    displayName: 'Engineer',
    shortLabel: 'Build',
    description: 'Implements repository changes.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    lastUsedAt: null,
    useCount: 0,
  },
  {
    ref: { kind: 'built_in', runtimeAgentId: 'ask', version: 1 },
    displayName: 'Ask',
    shortLabel: 'Ask',
    description: 'Answers in chat without mutating state.',
    scope: 'built_in',
    lifecycleState: 'active',
    baseCapabilityProfile: 'observe_only',
    lastUsedAt: null,
    useCount: 0,
  },
  {
    ref: { kind: 'custom', definitionId: 'security_reviewer', version: 1 },
    displayName: 'Security Reviewer',
    shortLabel: 'SecRev',
    description: 'Project-specific threat model reviewer.',
    scope: 'project_custom',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    lastUsedAt: null,
    useCount: 0,
  },
]

beforeEach(() => {
  // Default the persisted tab to "agents" so tests don't have to click.
  window.localStorage.setItem('xero.library.tab', 'agents')
})

afterEach(() => {
  window.localStorage.clear()
})

describe('WorkflowsSidebar', () => {
  it('renders real agents from props with scope badges', () => {
    render(<WorkflowsSidebar open agents={REAL_AGENTS} />)

    expect(screen.getByText('Engineer')).toBeInTheDocument()
    expect(screen.getByText('Ask')).toBeInTheDocument()
    expect(screen.getByText('Security Reviewer')).toBeInTheDocument()

    // Scope badge per row.
    expect(screen.getAllByText('Built-in').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('Project')).toBeInTheDocument()
  })

  it('invokes onSelectAgent with the row ref when clicked', () => {
    const onSelectAgent = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        onSelectAgent={onSelectAgent}
      />,
    )

    fireEvent.click(screen.getByLabelText('Inspect Engineer'))

    expect(onSelectAgent).toHaveBeenCalledWith({
      kind: 'built_in',
      runtimeAgentId: 'engineer',
      version: 1,
    })
  })

  it('marks the selected agent row as pressed', () => {
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        selectedAgentRef={{ kind: 'built_in', runtimeAgentId: 'ask', version: 1 }}
      />,
    )

    const askButton = screen.getByLabelText('Inspect Ask')
    expect(askButton.getAttribute('aria-pressed')).toBe('true')

    const engineerButton = screen.getByLabelText('Inspect Engineer')
    expect(engineerButton.getAttribute('aria-pressed')).toBe('false')
  })

  it('shows a loading message before the agent list is ready', () => {
    render(<WorkflowsSidebar open agents={[]} agentsLoading />)
    expect(screen.getByText(/loading agents/i)).toBeInTheDocument()
  })

  it('shows the error message when the agent list fails to load', () => {
    render(
      <WorkflowsSidebar open agents={[]} agentsError={new Error('boom')} />,
    )
    expect(screen.getByText('Failed to load agents.')).toBeInTheDocument()
    expect(screen.getByText('boom')).toBeInTheDocument()
  })

  it('creates an agent directly from the agents header without opening a mode menu', () => {
    const onCreateAgent = vi.fn()
    const onCreateAgentByHand = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        onCreateAgent={onCreateAgent}
        onCreateAgentByHand={onCreateAgentByHand}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'New agent' }))

    expect(onCreateAgent).toHaveBeenCalledTimes(1)
    expect(onCreateAgentByHand).not.toHaveBeenCalled()
    expect(screen.queryByText('Build with AI')).not.toBeInTheDocument()
    expect(screen.queryByText('Build by hand')).not.toBeInTheDocument()
  })
})
