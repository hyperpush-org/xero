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

  it('opens the shared workflow creation dialog from the workflows header', () => {
    const onCreateWorkflow = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={WORKFLOWS}
        onCreateWorkflow={onCreateWorkflow}
        onCreateWorkflowFromTemplate={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    fireEvent.click(screen.getByRole('button', { name: 'New workflow' }))

    expect(onCreateWorkflow).not.toHaveBeenCalled()
    expect(screen.getByRole('heading', { name: 'Create workflow' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Blank workflow/ }))
    expect(onCreateWorkflow).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('routes workflow creation to Agent Create from the shared dialog', () => {
    const onCreateWorkflowWithAgentCreate = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={WORKFLOWS}
        onCreateWorkflow={vi.fn()}
        onCreateWorkflowWithAgentCreate={onCreateWorkflowWithAgentCreate}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    fireEvent.click(screen.getByRole('button', { name: 'New workflow' }))
    fireEvent.click(screen.getByRole('button', { name: /Use Agent Create/ }))

    expect(onCreateWorkflowWithAgentCreate).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('uses the shared row action menu for workflow-specific actions', async () => {
    const onSelectWorkflow = vi.fn()
    const onStartWorkflowRun = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={WORKFLOWS}
        onSelectWorkflow={onSelectWorkflow}
        onStartWorkflowRun={onStartWorkflowRun}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Release pipeline' }), {
      button: 0,
      ctrlKey: false,
    })

    fireEvent.click(await screen.findByRole('menuitem', { name: 'Open workflow' }))
    expect(onSelectWorkflow).toHaveBeenCalledWith('release-pipeline')

    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for Release pipeline' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Start run' }))
    expect(onStartWorkflowRun).toHaveBeenCalledWith('release-pipeline')
  })

  it('uses the shared row action menu for workflow template actions', async () => {
    const onCreateWorkflowFromTemplate = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={[]}
        onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    fireEvent.pointerDown(screen.getByRole('button', { name: 'More actions for GSD Auto' }), {
      button: 0,
      ctrlKey: false,
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Use template' }))

    expect(onCreateWorkflowFromTemplate).toHaveBeenCalledWith('gsd_auto')
  })

  it('inspects workflow templates from row selection without creating a draft', () => {
    const onSelectWorkflowTemplate = vi.fn()
    const onCreateWorkflowFromTemplate = vi.fn()
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={[]}
        selectedWorkflowTemplateId="gsd_auto"
        onSelectWorkflowTemplate={onSelectWorkflowTemplate}
        onCreateWorkflowFromTemplate={onCreateWorkflowFromTemplate}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))
    fireEvent.click(screen.getByLabelText('Inspect workflow template GSD Auto'))

    expect(onSelectWorkflowTemplate).toHaveBeenCalledWith('gsd_auto')
    expect(onCreateWorkflowFromTemplate).not.toHaveBeenCalled()
    expect(screen.getByLabelText('Inspect workflow template GSD Auto').parentElement).toHaveClass(
      'bg-primary/10',
    )
  })

  it('renders workflow templates without a section header or divider', () => {
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={[]}
        onSelectWorkflowTemplate={vi.fn()}
        onCreateWorkflowFromTemplate={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))

    const templateButton = screen.getByLabelText('Inspect workflow template GSD Auto')
    expect(screen.queryByRole('heading', { name: 'Templates' })).not.toBeInTheDocument()
    expect(templateButton.closest('section')).toBeNull()
    expect(templateButton.closest('ul')).toHaveClass('py-1')
  })

  it('uses the same library row shell for agents, workflows, and workflow templates', () => {
    render(
      <WorkflowsSidebar
        open
        agents={REAL_AGENTS}
        workflowDefinitions={WORKFLOWS}
        onSelectWorkflow={vi.fn()}
        onSelectWorkflowTemplate={vi.fn()}
        onCreateWorkflowFromTemplate={vi.fn()}
      />,
    )

    const agentShell = screen.getByLabelText('Inspect Engineer').parentElement
    expect(agentShell).toHaveClass(
      'group',
      'relative',
      'flex',
      'items-start',
      'gap-3',
      'px-3',
      'py-3',
    )

    fireEvent.click(screen.getByRole('tab', { name: /workflows/i }))

    const workflowShell = screen.getByLabelText('Open workflow Release pipeline').parentElement
    const templateShell = screen.getByLabelText('Inspect workflow template GSD Auto').parentElement
    expect(workflowShell).toHaveClass(
      'group',
      'relative',
      'flex',
      'items-start',
      'gap-3',
      'px-3',
      'py-3',
    )
    expect(templateShell).toHaveClass(
      'group',
      'relative',
      'flex',
      'items-start',
      'gap-3',
      'px-3',
      'py-3',
    )
  })
})
