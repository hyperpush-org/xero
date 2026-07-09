import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { WorkflowAgentSummaryDto } from '@/src/lib/xero-model/workflow-agents'
import type { WorkflowDefinitionSummaryDto } from '@/src/lib/xero-model/workflow-definition'
import type { ComposerModelOptionView } from '@/src/features/xero/use-xero-desktop-state/runtime-provider'

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
    description: 'User-created threat model reviewer.',
    scope: 'global_custom',
    lifecycleState: 'active',
    baseCapabilityProfile: 'engineering',
    lastUsedAt: null,
    useCount: 0,
  },
]

const WORKFLOWS: WorkflowDefinitionSummaryDto[] = [
  {
    id: 'release-pipeline',
    projectId: 'project-1',
    name: 'Release pipeline',
    description: 'Build, verify, and hand off a release.',
    activeVersionId: 'release-pipeline-v1',
    activeVersionNumber: 1,
    createdAt: '2026-05-23T00:00:00.000Z',
    updatedAt: '2026-05-23T00:00:00.000Z',
  },
]

const MODEL_OPTIONS: ComposerModelOptionView[] = [
  {
    selectionKey: 'openai_codex:gpt-5.4',
    profileId: 'openai_codex-default',
    providerId: 'openai_codex',
    providerLabel: 'OpenAI Codex',
    modelId: 'gpt-5.4',
    displayName: 'GPT-5.4',
    thinking: {
      supported: true,
      effortOptions: ['low', 'medium', 'high'],
      defaultEffort: 'low',
    },
    thinkingEffortOptions: ['low', 'medium', 'high'],
    defaultThinkingEffort: 'low',
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
    expect(screen.getByText('User')).toBeInTheDocument()
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

  it('invokes onUseAgentInChat from the row action menu', async () => {
    const onUseAgentInChat = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        onUseAgentInChat={onUseAgentInChat}
      />,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Ask' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Use in Chat' }))

    expect(onUseAgentInChat).toHaveBeenCalledWith({
      kind: 'built_in',
      runtimeAgentId: 'ask',
      version: 1,
    })
  })

  it('saves a default model from the row action menu', async () => {
    const onSetAgentDefaultModel = vi.fn(async () => undefined)
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        onSetAgentDefaultModel={onSetAgentDefaultModel}
      />,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Engineer' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Default model' }))

    expect(await screen.findByRole('dialog')).toHaveTextContent('Engineer default model')

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onSetAgentDefaultModel).toHaveBeenCalledWith(REAL_AGENTS[0], null),
    )
  })

  it('saves a default model with the selected thinking level', async () => {
    const onSetAgentDefaultModel = vi.fn(async () => undefined)
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        modelOptions={MODEL_OPTIONS}
        onSetAgentDefaultModel={onSetAgentDefaultModel}
      />,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Engineer' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Default model' }))

    fireEvent.pointerDown(await screen.findByRole('combobox', { name: 'Model' }), {
      button: 0,
      pointerId: 1,
      pointerType: 'mouse',
    })
    fireEvent.click(await screen.findByRole('option', { name: 'GPT-5.4' }))
    const thinkingItem = screen.getByRole('menuitem', { name: /Thinking/i })
    fireEvent.keyDown(thinkingItem, { key: 'ArrowRight' })
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'High' }))

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onSetAgentDefaultModel).toHaveBeenCalledWith(REAL_AGENTS[0], {
        providerId: 'openai_codex',
        providerProfileId: 'openai_codex-default',
        modelId: 'gpt-5.4',
        selectionKey: 'openai_codex:gpt-5.4',
        thinkingEffort: 'high',
      }),
    )
  })

  it('confirms before deleting a user-created agent', async () => {
    const onDeleteAgent = vi.fn(async () => undefined)
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        onDeleteAgent={onDeleteAgent}
      />,
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Security Reviewer' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Delete' }))

    const dialog = await screen.findByRole('alertdialog')
    expect(dialog).toHaveTextContent('Delete Security Reviewer?')
    expect(onDeleteAgent).not.toHaveBeenCalled()

    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete' }))

    await waitFor(() =>
      expect(onDeleteAgent).toHaveBeenCalledWith({
        kind: 'custom',
        definitionId: 'security_reviewer',
        version: 1,
      }),
    )
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

  it('keeps the closed layout sidebar on the shared width transition', () => {
    const { container } = render(<WorkflowsSidebar open={false} agents={REAL_AGENTS} />)
    const aside = container.querySelector('aside') as HTMLElement

    expect(aside).toHaveAttribute('aria-hidden', 'true')
    expect(aside.style.width).toBe('0px')
    expect(aside.style.transition).toContain('width 160ms')
  })

  it('stages the first open from zero width so it can slide out', async () => {
    const { container } = render(<WorkflowsSidebar open agents={REAL_AGENTS} />)
    const aside = container.querySelector('aside') as HTMLElement

    expect(aside).toHaveAttribute('aria-hidden', 'false')
    expect(aside.style.width).toBe('0px')
    await waitFor(() => expect(aside.style.width).toBe('380px'))
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

  it('lists workflows and routes create and select actions when enabled', () => {
    const onCreateWorkflow = vi.fn()
    const onSelectWorkflow = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={WORKFLOWS}
        onCreateWorkflow={onCreateWorkflow}
        onSelectWorkflow={onSelectWorkflow}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))

    expect(screen.queryByText('Workflows are coming soon')).not.toBeInTheDocument()
    expect(screen.getByText('Release pipeline')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Search workflows' })).toBeEnabled()

    fireEvent.click(screen.getByRole('button', { name: 'New workflow' }))
    expect(onCreateWorkflow).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: /Blank workflow/ }))
    expect(onCreateWorkflow).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Open workflow Release pipeline' }))
    expect(onSelectWorkflow).toHaveBeenCalledWith('release-pipeline')
  })

  it('switches between the workflows and agents tabs', () => {
    render(<WorkflowsSidebar open agents={REAL_AGENTS} workflowDefinitions={WORKFLOWS} />)

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    expect(screen.getByText('Release pipeline')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: /agents/i }))
    expect(screen.getByRole('tab', { name: /agents/i })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByText('Engineer')).toBeInTheDocument()
  })
})
