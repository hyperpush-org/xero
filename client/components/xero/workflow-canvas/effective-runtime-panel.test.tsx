import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { EffectiveRuntimePanel, diagnosticToRow } from './effective-runtime-panel'
import type { AgentDefinitionPreviewResponseDto } from '@/src/lib/xero-model/agent-definition'

function buildPreviewFixture(
  overrides: Partial<AgentDefinitionPreviewResponseDto> = {},
): AgentDefinitionPreviewResponseDto {
  const base: AgentDefinitionPreviewResponseDto = {
    schema: 'xero.agent_definition_preview_command.v1',
    projectId: 'project-effective-runtime-test',
    applied: false,
    message: 'Previewed effective runtime for agent definition `helper` version 1.',
    definition: {
      definitionId: 'helper',
      version: 1,
      displayName: 'Helper',
      shortLabel: 'Helper',
      description: 'A helper agent.',
      scope: 'project_custom',
      lifecycleState: 'active',
      baseCapabilityProfile: 'observe_only',
    },
    validation: { status: 'valid', diagnostics: [] },
    effectiveRuntimePreview: {
      schema: 'xero.agent_effective_runtime_preview.v1',
      schemaVersion: 1,
      source: {
        kind: 'normalized_agent_definition_snapshot',
        uiDeferred: true,
        uiDeferralReason: 'fixture',
      },
      definition: {
        definitionId: 'helper',
        version: 1,
        displayName: 'Helper',
        scope: 'project_custom',
        lifecycleState: 'active',
        baseCapabilityProfile: 'observe_only',
        runtimeAgentId: 'ask',
      },
      validation: { status: 'valid' },
      prompt: {
        compiler: 'PromptCompiler',
        selectionMode: 'capability_ceiling_without_task_prompt',
        promptSha256: 'a'.repeat(64),
        promptBudgetTokens: 8000,
        estimatedPromptTokens: 240,
        fragmentCount: 2,
        fragmentIds: ['xero.system_policy', 'xero.tool_policy'],
        fragments: [
          {
            id: 'xero.system_policy',
            priority: 1000,
            title: 'System Policy',
            provenance: 'runtime',
            budgetPolicy: 'always',
            inclusionReason: 'base runtime policy',
            content: 'Follow system rules.',
            sha256: 'b'.repeat(64),
            tokenEstimate: 120,
          },
          {
            id: 'xero.tool_policy',
            priority: 900,
            title: 'Tool Policy',
            provenance: 'runtime',
            budgetPolicy: 'soft',
            inclusionReason: 'active tool contract',
            content: 'Use only allowed tools.',
            sha256: 'c'.repeat(64),
            tokenEstimate: 120,
          },
        ],
      },
      graphValidation: {
        schema: 'xero.agent_graph_validation_summary.v1',
        status: 'valid',
        diagnosticCount: 0,
        categories: [
          { category: 'unavailable_tools', count: 0, diagnostics: [] },
        ],
      },
      graphRepairHints: {
        schema: 'xero.agent_graph_repair_hints.v1',
        supported: [
          {
            kind: 'tool',
            capabilityId: 'read',
            status: 'supported',
            note: 'Tool read is available.',
          },
        ],
        partiallySupported: [],
        unsupported: [],
      },
      attachedSkillInjection: {
        schema: 'xero.agent_attached_skill_injection_preview.v1',
        schemaVersion: 1,
        selectionMode: 'definition_attached_skills_without_skill_tool',
        status: 'resolved',
        skillToolRequired: false,
        attachmentCount: 0,
        resolvedCount: 0,
        staleCount: 0,
        unavailableCount: 0,
        blockedCount: 0,
        entries: [],
      },
      effectiveToolAccess: {
        selectionMode: 'capability_ceiling_without_task_prompt',
        skillToolEnabled: false,
        runtimeAgentId: 'ask',
        requestedTools: ['read'],
        requestedEffectClasses: ['observe'],
        explicitlyDeniedTools: ['write'],
        allowedToolCount: 1,
        deniedCapabilityCount: 1,
        allowedTools: [
          {
            toolName: 'read',
            group: 'filesystem',
            description: 'Read files.',
            riskClass: 'low',
            effectClass: 'observe',
            tags: ['read'],
            schemaFields: ['path'],
            runtimeProfileAllowed: true,
            customPolicyAllowed: true,
            hostAvailable: true,
            effectiveAllowed: true,
            deniedBy: [],
          },
        ],
        deniedCapabilities: [
          {
            toolName: 'write',
            group: 'filesystem',
            description: 'Write files.',
            riskClass: 'high',
            effectClass: 'write',
            tags: ['write'],
            schemaFields: ['path', 'content'],
            runtimeProfileAllowed: false,
            customPolicyAllowed: true,
            hostAvailable: true,
            effectiveAllowed: false,
            deniedBy: ['runtime_profile'],
          },
        ],
      },
      capabilityPermissionExplanations: [
        {
          schema: 'xero.capability_permission_explanation.v1',
          subjectKind: 'custom_agent',
          subjectId: 'helper',
          summary: 'Custom agent definition can select prompts, policies, and tools.',
          dataAccess: 'project runtime state',
          networkAccess: 'depends_on_effective_tool_policy',
          fileMutation: 'depends_on_effective_tool_policy',
          confirmationRequired: true,
          riskClass: 'custom_agent_runtime',
        },
      ],
      policies: {
        toolPolicy: 'observe_only',
        outputContract: null,
        contextPolicy: null,
        memoryPolicy: null,
        retrievalPolicy: null,
        handoffPolicy: null,
        attachedSkills: [],
        workflowContract: null,
        workflowStructure: null,
        finalResponseContract: null,
      },
      riskyCapabilityPrompts: [],
      runtimeConsistency: {
        toolPolicySource: 'AutonomousAgentToolPolicy::from_definition_snapshot',
        toolRegistrySource: 'ToolRegistry::builtin_with_options',
        promptCompilerSource: 'PromptCompiler::with_agent_definition_snapshot',
        taskPromptNarrowing: 'not_applied_in_preview',
      },
    },
    uiDeferred: true,
  }
  return { ...base, ...overrides }
}

describe('EffectiveRuntimePanel', () => {
  it('returns null while closed', () => {
    const { container } = render(
      <EffectiveRuntimePanel
        open={false}
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage={null}
        preview={buildPreviewFixture()}
      />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('shows a loading state until the first preview lands', () => {
    render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading
        errorMessage={null}
        preview={null}
      />,
    )
    expect(screen.getByText(/Compiling effective runtime/i)).toBeInTheDocument()
  })

  it('renders an error state when the preview fails', () => {
    render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage="boom"
        preview={null}
      />,
    )
    expect(screen.getByText(/Preview failed/i)).toBeInTheDocument()
    expect(screen.getByText('boom')).toBeInTheDocument()
  })

  it('renders the overview, prompt fragments, allowed tools, and denied capabilities', async () => {
    const preview = buildPreviewFixture()
    const { container } = render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage={null}
        preview={preview}
      />,
    )

    expect(screen.getByText('Effective runtime')).toBeInTheDocument()
    expect(screen.getByText('Helper', { selector: 'dd' })).toBeInTheDocument()
    expect(screen.getByText('v1')).toBeInTheDocument()
    expect(screen.getByText('Observe-only')).toBeInTheDocument()
    expect(screen.getByText('1 allowed tools')).toBeInTheDocument()
    expect(screen.getByText('1 denied')).toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Prompt' }))
    expect(await screen.findByText('System Policy')).toBeInTheDocument()
    expect(screen.getByText('Tool Policy')).toBeInTheDocument()
    expect(screen.getByText(/sha256:/)).toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Tools' }))
    expect(await screen.findByText('read', { selector: 'span' })).toBeInTheDocument()
    expect(screen.getAllByText('write').length).toBeGreaterThan(0)
    expect(screen.getByText(/Denied by: runtime_profile/)).toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Workflow' }))
    expect(
      await screen.findByText(/No workflow state machine declared/i),
    ).toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Diagnostics' }))
    expect(await screen.findByText(/No diagnostics/i)).toBeInTheDocument()

    expect(container).toMatchSnapshot()
  })

  it('renders workflow phases when the snapshot declares a state machine', async () => {
    const preview = buildPreviewFixture()
    const fixtureWithWorkflow: AgentDefinitionPreviewResponseDto = {
      ...preview,
      effectiveRuntimePreview: {
        ...preview.effectiveRuntimePreview,
        policies: {
          ...preview.effectiveRuntimePreview.policies,
          workflowStructure: {
            startPhaseId: 'plan',
            phases: [
              {
                id: 'plan',
                title: 'Plan',
                allowedTools: ['read', 'todo'],
                requiredChecks: [
                  { kind: 'todo_completed', todoId: 'gather-context' },
                ],
                branches: [
                  { targetPhaseId: 'execute', condition: { kind: 'always' } },
                ],
              },
              {
                id: 'execute',
                title: 'Execute',
                allowedTools: ['read', 'write'],
              },
            ],
          },
        },
      },
    }

    render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage={null}
        preview={fixtureWithWorkflow}
      />,
    )

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Workflow' }))
    expect(await screen.findByText('Plan')).toBeInTheDocument()
    expect(screen.getByText('Execute')).toBeInTheDocument()
    expect(screen.getByText('Start')).toBeInTheDocument()
    expect(screen.getByText(/todo:gather-context/)).toBeInTheDocument()
    expect(screen.getByText(/→ execute \(always\)/)).toBeInTheDocument()
  })

  it('flags definition and graph diagnostics on the diagnostics tab', async () => {
    const preview = buildPreviewFixture()
    const fixtureWithDiagnostics: AgentDefinitionPreviewResponseDto = {
      ...preview,
      validation: {
        status: 'invalid',
        diagnostics: [
          {
            code: 'agent_definition_tool_denied_by_runtime_profile',
            message: 'Tool `write` is denied by runtime profile observe_only.',
            path: 'tools[0]',
            deniedTool: 'write',
            deniedEffectClass: 'write',
            baseCapabilityProfile: 'observe_only',
            reason: 'profile_denial',
            repairHint: 'remove_tool',
          },
        ],
      },
      effectiveRuntimePreview: {
        ...preview.effectiveRuntimePreview,
        graphValidation: {
          schema: 'xero.agent_graph_validation_summary.v1',
          status: 'invalid',
          diagnosticCount: 1,
          categories: [
            {
              category: 'unavailable_tools',
              count: 1,
              diagnostics: [
                {
                  code: 'agent_graph_unavailable_tool',
                  message: 'Tool `web_fetch` is not available in this host runtime.',
                  path: 'tools[1]',
                  deniedTool: 'web_fetch',
                  deniedEffectClass: null,
                  baseCapabilityProfile: null,
                  reason: 'host_unavailable',
                  repairHint: 'remove_tool',
                },
              ],
            },
          ],
        },
      },
    }

    render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage={null}
        preview={fixtureWithDiagnostics}
      />,
    )

    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Diagnostics' }))
    expect(
      await screen.findByText('agent_definition_tool_denied_by_runtime_profile'),
    ).toBeInTheDocument()
    expect(screen.getByText('agent_graph_unavailable_tool')).toBeInTheDocument()
    expect(screen.getAllByText(/Repair: remove_tool/).length).toBe(2)
    expect(screen.getByText('unavailable_tools')).toBeInTheDocument()
  })

  it('lets the user trigger a refresh', () => {
    const onRefreshActive = vi.fn()
    render(
      <EffectiveRuntimePanel
        open
        onClose={vi.fn()}
        agentLabel="Helper"
        loading={false}
        errorMessage={null}
        preview={buildPreviewFixture()}
        onRefreshActive={onRefreshActive}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /Refresh/i }))
    expect(onRefreshActive).toHaveBeenCalled()
  })
})

describe('diagnosticToRow', () => {
  it('maps validation diagnostics into a renderable row shape', () => {
    const row = diagnosticToRow({
      code: 'agent_definition_tool_denied',
      message: 'denied',
      path: 'tools[0]',
      deniedTool: 'write',
      deniedEffectClass: 'write',
      baseCapabilityProfile: 'observe_only',
      reason: 'profile_denial',
      repairHint: 'remove_tool',
    })
    expect(row).toEqual({
      code: 'agent_definition_tool_denied',
      path: 'tools[0]',
      message: 'denied',
      repairHint: 'remove_tool',
      category: null,
    })
  })

  it('preserves a category when one is supplied (graph diagnostic)', () => {
    const row = diagnosticToRow(
      {
        code: 'agent_graph_unavailable_tool',
        message: 'Tool not available.',
        path: 'tools[2]',
        deniedTool: 'web_fetch',
        deniedEffectClass: null,
        baseCapabilityProfile: null,
        reason: 'host_unavailable',
        repairHint: null,
      },
      'unavailable_tools',
    )
    expect(row.category).toBe('unavailable_tools')
    expect(row.repairHint).toBeNull()
  })
})
