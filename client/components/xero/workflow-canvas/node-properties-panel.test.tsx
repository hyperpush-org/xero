import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { NodePropertiesPanel } from './node-properties-panel'
import type {
  AgentGraphNode,
  AgentHeaderAdvancedFields,
  AgentHeaderNodeData,
  StageNodeData,
} from './build-agent-graph'
import type { CustomAgentWorkflowPhaseDto } from '@/src/lib/xero-model/agent-definition'
import {
  CanvasModeProvider,
  type CanvasModeContextValue,
} from './canvas-mode-context'
import type {
  AgentAuthoringCatalogDto,
  AgentToolPackCatalogDto,
} from '@/src/lib/xero-model/workflow-agents'
import type { AgentDefinitionValidationDiagnosticDto } from '@/src/lib/xero-model/agent-definition'

const baseHeader = {
  displayName: 'Engineer',
  shortLabel: 'eng',
  description: 'Custom engineering agent.',
  taskPurpose: 'Build features end-to-end.',
  scope: 'project_custom' as const,
  lifecycleState: 'active' as const,
  baseCapabilityProfile: 'engineering' as const,
  defaultApprovalMode: 'suggest' as const,
  allowedApprovalModes: ['suggest' as const],
  allowPlanGate: true,
  allowVerificationGate: true,
  allowAutoCompact: true,
}

function makeAdvanced(overrides: Partial<AgentHeaderAdvancedFields> = {}): AgentHeaderAdvancedFields {
  return {
    workflowContract: '',
    finalResponseContract: '',
    examplePrompts: ['One', 'Two', 'Three'],
    refusalEscalationCases: ['One', 'Two', 'Three'],
    allowedEffectClasses: [],
    deniedTools: [],
    allowedToolPacks: [],
    deniedToolPacks: [],
    allowedToolGroups: [],
    deniedToolGroups: [],
    allowedMcpServers: [],
    deniedMcpServers: [],
    allowedDynamicTools: [],
    deniedDynamicTools: [],
    allowedSubagentRoles: [],
    deniedSubagentRoles: [],
    externalServiceAllowed: false,
    browserControlAllowed: false,
    skillRuntimeAllowed: false,
    subagentAllowed: false,
    commandAllowed: false,
    destructiveWriteAllowed: false,
    ...overrides,
  }
}

function makeAuthoringCatalog(): AgentAuthoringCatalogDto {
  return {
    schema: 'xero.agent_authoring_catalog.v1',
    projectId: 'project-1',
    baseCapabilityProfile: 'engineering',
    toolCategories: [],
    tools: [
      {
        name: 'destructive_write_tool',
        group: 'engineering',
        description: 'Mutates files on disk.',
        effectClass: 'destructive_write',
        riskClass: 'workspace_write',
        tags: ['filesystem'],
        schemaFields: [],
        examples: [],
        profileAvailability: {
          subjectKind: 'tool',
          subjectId: 'destructive_write_tool',
          baseCapabilityProfile: 'engineering',
          status: 'available',
          reason: 'Available on engineering profile.',
        },
      },
    ],
    dbTables: [],
    upstreamArtifacts: [],
    attachableSkills: [],
    profileAvailability: {
      subjectKind: 'profile',
      subjectId: 'engineering',
      baseCapabilityProfile: 'engineering',
      status: 'available',
      reason: 'Engineering profile is selected.',
    },
  } as unknown as AgentAuthoringCatalogDto
}

function makeToolPackCatalog(): AgentToolPackCatalogDto {
  return {
    schema: 'xero.agent_tool_pack_catalog.v1',
    projectId: 'project-1',
    uiDeferred: true,
    availablePackIds: ['release_notes_pack'],
    toolPacks: [
      {
        contractVersion: 1,
        packId: 'release_notes_pack',
        label: 'Release Notes',
        summary: 'Draft and publish release notes.',
        policyProfile: 'engineering',
        toolGroups: ['engineering'],
        tools: ['draft_release_notes', 'publish_release_notes'],
        capabilities: ['release_authoring'],
        allowedEffectClasses: ['write'],
        deniedEffectClasses: [],
        reviewRequirements: [],
        prerequisites: [],
        healthChecks: [],
        scenarioChecks: [],
        uiAffordances: [],
        cliCommands: [],
        approvalBoundaries: [],
      },
    ],
    healthReports: [],
  } as AgentToolPackCatalogDto
}

function renderHeaderEditor(
  overrides: Partial<AgentHeaderAdvancedFields> = {},
  options: {
    diagnostics?: AgentDefinitionValidationDiagnosticDto[]
    onUpdate?: (advanced: AgentHeaderAdvancedFields) => void
  } = {},
) {
  let current: AgentHeaderNodeData = {
    header: baseHeader,
    summary: {
      prompts: 0,
      tools: 0,
      dbTables: 0,
      outputSections: 0,
      consumes: 0,
      attachedSkills: 0,
    },
    advanced: makeAdvanced(overrides),
  }
  const node: AgentGraphNode = {
    id: 'agent-header',
    type: 'agent-header',
    position: { x: 0, y: 0 },
    data: current,
  } as AgentGraphNode

  const updateNodeData = vi.fn(
    (_nodeId: string, updater: (current: AgentHeaderNodeData) => AgentHeaderNodeData) => {
      current = updater(current)
      node.data = current
      options.onUpdate?.(current.advanced)
      rerender(
        <CanvasModeProvider value={makeContext()}>
          <NodePropertiesPanel
            selectedNode={{ ...node, data: current } as AgentGraphNode}
            onClose={vi.fn()}
          />
        </CanvasModeProvider>,
      )
    },
  )

  const makeContext = (): CanvasModeContextValue => ({
    editing: true,
    mode: 'edit',
    updateNodeData: updateNodeData as unknown as CanvasModeContextValue['updateNodeData'],
    removeNode: vi.fn(),
    removeToolGroup: vi.fn(),
    authoringCatalog: makeAuthoringCatalog(),
    toolPackCatalog: makeToolPackCatalog(),
    inferredAdvanced: {
      effectClasses: [],
      toolGroups: [],
      flags: {
        externalServiceAllowed: false,
        browserControlAllowed: false,
        skillRuntimeAllowed: false,
        subagentAllowed: false,
        commandAllowed: false,
        destructiveWriteAllowed: false,
      },
      toolGroupReasons: {},
      flagReasons: {
        externalServiceAllowed: [],
        browserControlAllowed: [],
        skillRuntimeAllowed: [],
        subagentAllowed: [],
        commandAllowed: [],
        destructiveWriteAllowed: [],
      },
    },
    policyDiagnostics: options.diagnostics ?? [],
    policyDiagnosticsLoading: false,
    stageList: [],
    agentToolNames: [],
  })

  const { rerender } = render(
    <CanvasModeProvider value={makeContext()}>
      <NodePropertiesPanel selectedNode={node} onClose={vi.fn()} />
    </CanvasModeProvider>,
  )

  return {
    getAdvanced: () => current.advanced,
    updateNodeData,
  }
}

describe('NodePropertiesPanel granular policy editor', () => {
  it('renders an unchanged prompt editor (regression guard)', () => {
    const promptNode: AgentGraphNode = {
      id: 'prompt:test',
      type: 'prompt',
      position: { x: 0, y: 0 },
      data: {
        prompt: {
          id: 'test',
          label: 'Test prompt',
          role: 'system',
          source: 'custom',
          body: 'Be useful.',
        },
      },
    } as AgentGraphNode

    const { container } = render(
      <NodePropertiesPanel selectedNode={promptNode} onClose={vi.fn()} />,
    )
    const panel = container.querySelector('.agent-properties-panel')
    expect(panel).not.toBeNull()
    expect(panel).toHaveClass('w-[272px]')
  })

  it('expands an allowed tool pack into its granted tool chips so the user can see what was added', () => {
    renderHeaderEditor({
      allowedToolPacks: ['release_notes_pack'],
    })
    // The pack label appears as a removable chip…
    expect(screen.getByText('Release Notes')).toBeInTheDocument()
    // …and the panel renders the pack's tool grant list (humanized).
    expect(screen.getByText(/Pack grants 2 tools/i)).toBeInTheDocument()
    expect(screen.getByText('Draft Release Notes')).toBeInTheDocument()
    expect(screen.getByText('Publish Release Notes')).toBeInTheDocument()
  })

  it('flags a tool pack listed in both allow and deny sets', () => {
    renderHeaderEditor({
      allowedToolPacks: ['release_notes_pack'],
      deniedToolPacks: ['release_notes_pack'],
    })
    const warning = screen.getByText(/listed as both allowed and denied/i)
    expect(warning).toBeInTheDocument()
    expect(warning.textContent).toMatch(/Release Notes/)
  })

  it('flags a subagent role listed in both allow and deny sets', () => {
    renderHeaderEditor({
      subagentAllowed: true,
      allowedSubagentRoles: ['engineer'],
      deniedSubagentRoles: ['engineer'],
    })
    expect(
      screen.getByText(/listed as both allowed and denied/i),
    ).toBeInTheDocument()
  })

  it('surfaces validator diagnostics from preview against the matching section', () => {
    renderHeaderEditor(
      { allowedToolPacks: ['mystery_pack'] },
      {
        diagnostics: [
          {
            code: 'agent_definition_tool_pack_unknown',
            message: 'Tool pack `mystery_pack` is not known to Xero.',
            path: 'toolPolicy.allowedToolPacks',
            repairHint: 'Pick a pack from the catalog.',
          },
        ],
      },
    )
    expect(
      screen.getByText('Tool pack `mystery_pack` is not known to Xero.'),
    ).toBeInTheDocument()
    expect(screen.getByText('Pick a pack from the catalog.')).toBeInTheDocument()
    expect(screen.getByText('agent_definition_tool_pack_unknown')).toBeInTheDocument()
  })

  it('removes a denied tool via the chip-remove button', async () => {
    const { getAdvanced } = renderHeaderEditor({
      deniedTools: ['destructive_write_tool'],
    })
    const chip = screen.getByRole('button', { name: /Remove Destructive Write Tool/i })
    fireEvent.click(chip)
    expect(getAdvanced().deniedTools).toEqual([])
  })
})

interface RenderStageEditorOptions {
  phases?: CustomAgentWorkflowPhaseDto[]
  selectedPhaseId?: string
  startPhaseId?: string
  agentToolNames?: string[]
}

function renderStageEditor(options: RenderStageEditorOptions = {}) {
  const phases: CustomAgentWorkflowPhaseDto[] = options.phases ?? [
    { id: 'gather', title: 'Gather' },
    { id: 'draft', title: 'Draft' },
  ]
  const selectedId = options.selectedPhaseId ?? phases[0]!.id
  let startPhaseId = options.startPhaseId ?? phases[0]!.id
  const phaseStates = new Map<string, CustomAgentWorkflowPhaseDto>(
    phases.map((phase) => [phase.id, phase]),
  )

  const buildNode = (): AgentGraphNode => {
    const phase = phaseStates.get(selectedId)!
    return {
      id: `workflow-phase:${phase.id}`,
      type: 'stage',
      position: { x: 0, y: 0 },
      data: { phase, isStart: phase.id === startPhaseId } as StageNodeData,
    } as AgentGraphNode
  }

  const updateNodeData = vi.fn(
    (
      _nodeId: string,
      updater: (current: { phase: CustomAgentWorkflowPhaseDto; isStart: boolean }) => {
        phase: CustomAgentWorkflowPhaseDto
        isStart: boolean
      },
    ) => {
      const phase = phaseStates.get(selectedId)!
      const prev = { phase, isStart: phase.id === startPhaseId }
      const next = updater(prev)
      phaseStates.set(selectedId, next.phase)
      if (next.isStart && next.phase.id) {
        startPhaseId = next.phase.id
      } else if (!next.isStart && startPhaseId === selectedId) {
        startPhaseId = ''
      }
      rerender(
        <CanvasModeProvider value={makeContext()}>
          <NodePropertiesPanel selectedNode={buildNode()} onClose={vi.fn()} />
        </CanvasModeProvider>,
      )
    },
  )

  const makeContext = (): CanvasModeContextValue => ({
    editing: true,
    mode: 'edit',
    updateNodeData: updateNodeData as unknown as CanvasModeContextValue['updateNodeData'],
    removeNode: vi.fn(),
    removeToolGroup: vi.fn(),
    authoringCatalog: makeAuthoringCatalog(),
    toolPackCatalog: makeToolPackCatalog(),
    inferredAdvanced: {
      effectClasses: [],
      toolGroups: [],
      flags: {
        externalServiceAllowed: false,
        browserControlAllowed: false,
        skillRuntimeAllowed: false,
        subagentAllowed: false,
        commandAllowed: false,
        destructiveWriteAllowed: false,
      },
      toolGroupReasons: {},
      flagReasons: {
        externalServiceAllowed: [],
        browserControlAllowed: [],
        skillRuntimeAllowed: [],
        subagentAllowed: [],
        commandAllowed: [],
        destructiveWriteAllowed: [],
      },
    },
    policyDiagnostics: [],
    policyDiagnosticsLoading: false,
    stageList: Array.from(phaseStates.values()).map((entry) => ({
      id: entry.id,
      title: entry.title,
    })),
    agentToolNames: options.agentToolNames ?? [],
  })

  const { rerender } = render(
    <CanvasModeProvider value={makeContext()}>
      <NodePropertiesPanel selectedNode={buildNode()} onClose={vi.fn()} />
    </CanvasModeProvider>,
  )

  return {
    getPhase: () => phaseStates.get(selectedId)!,
    getStartPhaseId: () => startPhaseId,
    updateNodeData,
  }
}

describe('NodePropertiesPanel stage editor', () => {
  it('round-trips a retry-limit edit', () => {
    const { getPhase } = renderStageEditor()
    const retryInput = screen.getByLabelText('Retry limit') as HTMLInputElement
    fireEvent.change(retryInput, { target: { value: '3' } })
    expect(getPhase().retryLimit).toBe(3)
  })

  it('clears retry-limit when input is emptied', () => {
    const { getPhase } = renderStageEditor({
      phases: [{ id: 'gather', title: 'Gather', retryLimit: 5 }],
    })
    const retryInput = screen.getByLabelText('Retry limit') as HTMLInputElement
    fireEvent.change(retryInput, { target: { value: '' } })
    expect(getPhase().retryLimit).toBeUndefined()
  })

  it('appends a todo gate via the Todo button', () => {
    const { getPhase } = renderStageEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Todo' }))
    expect(getPhase().requiredChecks).toEqual([{ kind: 'todo_completed', todoId: '' }])
  })

  it('appends an exit branch when picking a target stage', () => {
    const { getPhase } = renderStageEditor()
    // Adding without going through the picker UI directly — invoke the click
    // on the rendered exit-target item by name. The CatalogPicker renders the
    // target stage label inside the popover; toggling open then clicking the
    // item exercises the same code path.
    fireEvent.click(screen.getByText('Add exit to another stage…'))
    fireEvent.click(screen.getByRole('option', { name: /Draft/i }))
    const branches = getPhase().branches ?? []
    expect(branches).toHaveLength(1)
    expect(branches[0]?.targetPhaseId).toBe('draft')
    expect(branches[0]?.condition).toEqual({ kind: 'always' })
  })

  it('marking a non-start stage as start updates the start phase id', () => {
    const { getStartPhaseId } = renderStageEditor({ selectedPhaseId: 'draft' })
    expect(getStartPhaseId()).toBe('gather')
    fireEvent.click(screen.getByLabelText('Mark as start'))
    expect(getStartPhaseId()).toBe('draft')
  })

  it('unmarking the start stage clears the start phase id', () => {
    const { getStartPhaseId } = renderStageEditor({ selectedPhaseId: 'gather' })
    expect(getStartPhaseId()).toBe('gather')
    fireEvent.click(screen.getByLabelText('Mark as start'))
    expect(getStartPhaseId()).toBe('')
  })

  it('renders the allowed-tools helper when the list is empty', () => {
    renderStageEditor()
    expect(
      screen.getByText(/Empty — every tool the agent has is allowed/i),
    ).toBeInTheDocument()
  })

  it('removes an allowed tool via its chip', () => {
    const { getPhase } = renderStageEditor({
      phases: [{ id: 'gather', title: 'Gather', allowedTools: ['destructive_write_tool'] }],
    })
    const chip = screen.getByRole('button', { name: /Remove Destructive Write Tool/i })
    fireEvent.click(chip)
    expect(getPhase().allowedTools).toBeUndefined()
  })
})
