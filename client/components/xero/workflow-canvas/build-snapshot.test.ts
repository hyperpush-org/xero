import { describe, expect, it } from 'vitest'

import type {
  CanonicalCustomAgentDefinitionDto,
  CustomAgentToolPolicyDto,
} from '@/src/lib/xero-model/agent-definition'
import type {
  AgentAttachedSkillDto,
  AgentToolPolicyDetailsDto,
  AgentHeaderDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

import {
  AGENT_DEFINITION_SCHEMA,
  AGENT_DEFINITION_SCHEMA_VERSION,
  canonicalCustomAgentDefinitionSchema,
} from '@/src/lib/xero-model/agent-definition'

import {
  AGENT_GRAPH_HEADER_NODE_ID,
  buildAgentGraph,
  type AgentHeaderNodeData,
} from './build-agent-graph'
import { buildSnapshotFromGraph } from './build-snapshot'

function detail(): WorkflowAgentDetailDto {
  return {
    ref: { kind: 'custom', definitionId: 'release_notes_helper', version: 2 },
    header: {
      displayName: 'Release Notes Helper',
      shortLabel: 'Release',
      description: 'Draft release notes from reviewed context.',
      taskPurpose: 'Retrieve approved release context and draft source-cited notes.',
      scope: 'project_custom',
      lifecycleState: 'active',
      baseCapabilityProfile: 'observe_only',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest'],
      allowPlanGate: false,
      allowVerificationGate: false,
      allowAutoCompact: true,
    },
    promptPolicy: null,
    toolPolicy: 'observe_only',
    toolPolicyDetails: {
      allowedTools: ['read'],
      deniedTools: ['write'],
      allowedToolPacks: ['release_notes_pack'],
      deniedToolPacks: ['external_network'],
      allowedToolGroups: ['core'],
      deniedToolGroups: ['browser_control'],
      allowedMcpServers: [],
      deniedMcpServers: [],
      allowedDynamicTools: [],
      deniedDynamicTools: [],
      allowedEffectClasses: ['observe'],
      externalServiceAllowed: false,
      browserControlAllowed: false,
      skillRuntimeAllowed: false,
      subagentAllowed: false,
      allowedSubagentRoles: [],
      deniedSubagentRoles: [],
      commandAllowed: false,
      destructiveWriteAllowed: false,
    },
    prompts: [
      {
        id: 'release_prompt',
        label: 'Release prompt',
        role: 'system',
        policy: null,
        source: 'custom',
        body: 'Draft release notes.',
      },
    ],
    tools: [
      {
        name: 'read',
        group: 'core',
        description: 'Read file content.',
        effectClass: 'observe',
        riskClass: 'observe',
        tags: ['file'],
        schemaFields: ['path'],
        examples: ['Read CHANGELOG.md'],
      },
    ],
    dbTouchpoints: {
      reads: [
        {
          table: 'project_context_records',
          kind: 'read',
          purpose: 'Read approved release records.',
          triggers: [{ kind: 'tool', name: 'read' }],
          columns: ['record_id', 'summary'],
        },
      ],
      writes: [],
      encouraged: [],
    },
    output: {
      contract: 'answer',
      label: 'Release answer',
      description: 'Release notes with risks.',
      sections: [
        {
          id: 'changes',
          label: 'Changes',
          description: 'User-visible changes.',
          emphasis: 'core',
          producedByTools: ['read'],
        },
      ],
    },
    consumes: [
      {
        id: 'plan_pack',
        label: 'Plan Pack',
        description: 'Optional plan context.',
        sourceAgent: 'plan',
        contract: 'plan_pack',
        sections: ['decisions'],
        required: false,
      },
    ],
    attachedSkills: [],
  }
}

function attachedRustSkill(): AgentAttachedSkillDto {
  return {
    id: 'rust-best-practices',
    sourceId: 'skill-source:v1:global:bundled:xero:rust-best-practices',
    skillId: 'rust-best-practices',
    name: 'Rust Best Practices',
    description: 'Guide for writing idiomatic Rust.',
    sourceKind: 'bundled',
    scope: 'global',
    versionHash: 'hash-rust',
    includeSupportingAssets: false,
    required: true,
    sourceState: 'enabled',
    trustState: 'trusted',
    availabilityStatus: 'available',
    availabilityReason: 'Skill source is enabled, trusted, and pinned for attachment.',
  }
}

function broadDetail(): WorkflowAgentDetailDto {
  const base = detail()
  const tools = [
    ['read', 'core', 'observe'],
    ['write', 'mutation', 'write'],
    ['command', 'command_session', 'command'],
    ['browser_control', 'browser_control', 'browser_control'],
    ['skill', 'skill_runtime', 'skill_runtime'],
    ['subagent', 'coordination', 'agent_delegation'],
    ['external_lookup', 'external_service', 'external_service'],
  ] as const
  return {
    ...base,
    ref: { kind: 'custom', definitionId: 'shipper', version: 4 },
    header: {
      ...base.header,
      displayName: 'Shipper',
      shortLabel: 'Ship',
      description: 'Implements, verifies, and coordinates release tasks.',
      taskPurpose: 'Edit files, run scoped verification, and coordinate bounded follow-up work.',
      baseCapabilityProfile: 'engineering',
      defaultApprovalMode: 'auto_edit',
      allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
      allowPlanGate: true,
      allowVerificationGate: true,
    },
    toolPolicy: 'engineering',
    toolPolicyDetails: {
      allowedTools: tools.map(([name]) => name),
      deniedTools: ['delete'],
      allowedToolPacks: ['repo_write_pack'],
      deniedToolPacks: ['production_deploy'],
      allowedToolGroups: tools.map(([, group]) => group),
      deniedToolGroups: ['dangerous'],
      allowedMcpServers: [],
      deniedMcpServers: [],
      allowedDynamicTools: [],
      deniedDynamicTools: [],
      allowedEffectClasses: tools.map(([, , effect]) => effect),
      externalServiceAllowed: true,
      browserControlAllowed: true,
      skillRuntimeAllowed: true,
      subagentAllowed: true,
      allowedSubagentRoles: ['engineer'],
      deniedSubagentRoles: [],
      commandAllowed: true,
      destructiveWriteAllowed: true,
    },
    tools: tools.map(([name, group, effectClass]) => ({
      name,
      group,
      description: `${name} capability.`,
      effectClass,
      riskClass: effectClass,
      tags: [group],
      schemaFields: ['input'],
      examples: [`Use ${name}.`],
    })),
    dbTouchpoints: {
      reads: base.dbTouchpoints.reads,
      writes: [
        {
          table: 'code_history_operations',
          kind: 'write',
          purpose: 'Persist file-operation history.',
          triggers: [{ kind: 'tool', name: 'write' }],
          columns: ['operation_id', 'path'],
        },
      ],
      encouraged: [
        {
          table: 'project_context_records',
          kind: 'encouraged',
          purpose: 'Persist handoff notes after verification.',
          triggers: [{ kind: 'output_section', id: 'verification' }],
          columns: ['record_id', 'summary'],
        },
      ],
    },
    output: {
      contract: 'engineering_summary',
      label: 'Shipping Summary',
      description: 'Files changed, verification, risks, and handoff.',
      sections: [
        {
          id: 'files_changed',
          label: 'Files Changed',
          description: 'Repository edits made.',
          emphasis: 'core',
          producedByTools: ['write'],
        },
        {
          id: 'verification',
          label: 'Verification',
          description: 'Commands and checks run.',
          emphasis: 'core',
          producedByTools: ['command'],
        },
      ],
    },
  }
}

function detailFromSnapshot(
  snapshot: CanonicalCustomAgentDefinitionDto,
): WorkflowAgentDetailDto {
  const toolPolicy =
    typeof snapshot.toolPolicy === 'string' ? undefined : snapshot.toolPolicy
  const toolPolicyName: WorkflowAgentDetailDto['toolPolicy'] =
    typeof snapshot.toolPolicy === 'string'
      ? snapshot.toolPolicy
      : snapshot.baseCapabilityProfile === 'debugging'
        ? 'engineering'
        : snapshot.baseCapabilityProfile
  const header: AgentHeaderDto = {
    displayName: snapshot.displayName,
    shortLabel: snapshot.shortLabel,
    description: snapshot.description,
    taskPurpose: snapshot.taskPurpose,
    scope: snapshot.scope,
    lifecycleState: snapshot.lifecycleState,
    baseCapabilityProfile: snapshot.baseCapabilityProfile,
    defaultApprovalMode: snapshot.defaultApprovalMode,
    allowedApprovalModes: [...snapshot.allowedApprovalModes],
    allowPlanGate: true,
    allowVerificationGate: true,
    allowAutoCompact: true,
  }
  return {
    ref: {
      kind: 'custom',
      definitionId: snapshot.id,
      version: snapshot.version ?? 1,
    },
    header,
    promptPolicy: null,
    toolPolicy: toolPolicyName,
    toolPolicyDetails: completeToolPolicyDetails(toolPolicy),
    prompts: snapshot.prompts.map((prompt) => ({
      id: prompt.id,
      label: prompt.label,
      role: prompt.role,
      policy: null,
      source: prompt.source,
      body: prompt.body,
    })),
    tools: snapshot.tools.map((tool) => ({ ...tool })),
    dbTouchpoints: {
      reads: snapshot.dbTouchpoints.reads.map((entry) => ({ ...entry })),
      writes: snapshot.dbTouchpoints.writes.map((entry) => ({ ...entry })),
      encouraged: snapshot.dbTouchpoints.encouraged.map((entry) => ({ ...entry })),
    },
    output: {
      ...snapshot.output,
      sections: snapshot.output.sections.map((section) => ({ ...section })),
    },
    consumes: snapshot.consumes.map((entry) => ({ ...entry })),
    attachedSkills: snapshot.attachedSkills.map((skill) => ({
      ...skill,
      sourceState: 'enabled',
      trustState: 'trusted',
      availabilityStatus: 'available',
      availabilityReason: 'Skill source is enabled, trusted, and pinned for attachment.',
    })),
    workflowStructure: snapshot.workflowStructure
      ? {
          startPhaseId: snapshot.workflowStructure.startPhaseId,
          phases: snapshot.workflowStructure.phases.map((phase) => ({ ...phase })),
        }
      : undefined,
  }
}

function completeToolPolicyDetails(
  policy: CustomAgentToolPolicyDto | undefined,
): AgentToolPolicyDetailsDto | undefined {
  if (!policy) return undefined
  return {
    allowedTools: policy.allowedTools ?? [],
    deniedTools: policy.deniedTools ?? [],
    allowedToolPacks: policy.allowedToolPacks ?? [],
    deniedToolPacks: policy.deniedToolPacks ?? [],
    allowedToolGroups: policy.allowedToolGroups ?? [],
    deniedToolGroups: policy.deniedToolGroups ?? [],
    allowedMcpServers: policy.allowedMcpServers ?? [],
    deniedMcpServers: policy.deniedMcpServers ?? [],
    allowedDynamicTools: policy.allowedDynamicTools ?? [],
    deniedDynamicTools: policy.deniedDynamicTools ?? [],
    allowedEffectClasses: policy.allowedEffectClasses ?? [],
    externalServiceAllowed: policy.externalServiceAllowed ?? false,
    browserControlAllowed: policy.browserControlAllowed ?? false,
    skillRuntimeAllowed: policy.skillRuntimeAllowed ?? false,
    subagentAllowed: policy.subagentAllowed ?? false,
    allowedSubagentRoles: policy.allowedSubagentRoles ?? (policy.subagentAllowed ? ['engineer'] : []),
    deniedSubagentRoles: policy.deniedSubagentRoles ?? [],
    commandAllowed: policy.commandAllowed ?? false,
    destructiveWriteAllowed: policy.destructiveWriteAllowed ?? false,
  }
}

function stableProjection(snapshot: CanonicalCustomAgentDefinitionDto) {
  return {
    id: snapshot.id,
    displayName: snapshot.displayName,
    shortLabel: snapshot.shortLabel,
    taskPurpose: snapshot.taskPurpose,
    baseCapabilityProfile: snapshot.baseCapabilityProfile,
    allowedApprovalModes: snapshot.allowedApprovalModes,
    toolPolicy: snapshot.toolPolicy,
    workflowContract: snapshot.workflowContract,
    finalResponseContract: snapshot.finalResponseContract,
    examplePrompts: snapshot.examplePrompts,
    refusalEscalationCases: snapshot.refusalEscalationCases,
    prompts: snapshot.prompts,
    tools: snapshot.tools,
    output: snapshot.output,
    dbTouchpoints: snapshot.dbTouchpoints,
    consumes: snapshot.consumes,
    attachedSkills: snapshot.attachedSkills,
    workflowStructure: snapshot.workflowStructure,
  }
}

function parsedSnapshot(
  source: WorkflowAgentDetailDto,
  definitionId?: string | null,
): CanonicalCustomAgentDefinitionDto {
  const graph = buildAgentGraph(source)
  return canonicalCustomAgentDefinitionSchema.parse(
    buildSnapshotFromGraph(graph.nodes, graph.edges, {
      initialDefinitionId: definitionId,
      attachedSkills: source.attachedSkills,
    }).snapshot,
  )
}

function expectStableSaveReloadRoundTrip(
  source: WorkflowAgentDetailDto,
  definitionId: string,
) {
  const first = parsedSnapshot(source, definitionId)
  const reloaded = detailFromSnapshot(first)
  const second = parsedSnapshot(reloaded, definitionId)
  expect(stableProjection(second)).toEqual(stableProjection(first))
}

describe('buildSnapshotFromGraph', () => {
  it('serializes the canonical custom-agent graph without dropping granular policy', () => {
    const graph = buildAgentGraph(detail())
    const { snapshot } = buildSnapshotFromGraph(graph.nodes, graph.edges, {
      initialDefinitionId: 'release_notes_helper',
    })

    expect(snapshot.schema).toBe(AGENT_DEFINITION_SCHEMA)
    expect(snapshot.schemaVersion).toBe(AGENT_DEFINITION_SCHEMA_VERSION)
    expect(snapshot.attachedSkills).toEqual([])
    expect(snapshot.tools).toMatchObject([{ name: 'read', group: 'core' }])
    expect(snapshot.output).toMatchObject({
      contract: 'answer',
      sections: [{ id: 'changes', producedByTools: ['read'] }],
    })
    expect(snapshot.dbTouchpoints).toMatchObject({
      reads: [{ table: 'project_context_records', kind: 'read' }],
    })
    expect(snapshot.consumes).toMatchObject([{ id: 'plan_pack' }])
    expect(snapshot.toolPolicy).toMatchObject({
      allowedTools: ['read'],
      deniedTools: ['write'],
      allowedToolPacks: ['release_notes_pack'],
      deniedToolPacks: ['external_network'],
      allowedToolGroups: ['core'],
      deniedToolGroups: ['browser_control'],
      allowedEffectClasses: ['observe'],
    })
  })

  it('keeps narrow and broad custom agents stable across save and reload', () => {
    expectStableSaveReloadRoundTrip(detail(), 'release_notes_helper')
    expectStableSaveReloadRoundTrip(broadDetail(), 'shipper')
  })

  it('preserves edited graph fields while keeping the existing definition id', () => {
    const graph = buildAgentGraph(detail())
    const header = graph.nodes.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID)
    const headerData = header?.data as AgentHeaderNodeData
    headerData.header = {
      ...headerData.header,
      displayName: 'Release Notes Helper Edited',
      description: 'Draft edited release notes from reviewed context.',
    }
    headerData.advanced = {
      ...headerData.advanced,
      deniedTools: ['write', 'delete'],
      allowedToolPacks: ['release_notes_pack', 'qa_notes_pack'],
      allowedEffectClasses: ['observe', 'runtime_state'],
    }
    const changesSection = graph.nodes.find((node) => node.id === 'output-section:changes')
    if (changesSection?.type === 'output-section') {
      changesSection.data.section = {
        ...changesSection.data.section,
        label: 'Edited Changes',
        producedByTools: ['read'],
      }
    }

    const snapshot = canonicalCustomAgentDefinitionSchema.parse(
      buildSnapshotFromGraph(graph.nodes, graph.edges, {
        initialDefinitionId: 'release_notes_helper',
      }).snapshot,
    )
    const toolPolicy = snapshot.toolPolicy as CustomAgentToolPolicyDto

    expect(snapshot.id).toBe('release_notes_helper')
    expect(snapshot.displayName).toBe('Release Notes Helper Edited')
    expect(snapshot.output.sections[0]).toMatchObject({
      id: 'changes',
      label: 'Edited Changes',
      producedByTools: ['read'],
    })
    expect(toolPolicy.deniedTools).toEqual(['write', 'delete'])
    expect(toolPolicy.allowedToolPacks).toEqual(['release_notes_pack', 'qa_notes_pack'])
    expect(toolPolicy.allowedEffectClasses).toEqual(['observe', 'runtime_state'])
  })

  it('duplicates an existing graph as an ordinary custom agent with a fresh slug id', () => {
    const source = detail()
    source.header = {
      ...source.header,
      displayName: 'Release Notes Helper Copy',
      shortLabel: 'Release Copy',
    }
    source.attachedSkills = [attachedRustSkill()]

    const snapshot = parsedSnapshot(source, null)

    expect(snapshot.id).toBe('release_notes_helper_copy')
    expect(snapshot.displayName).toBe('Release Notes Helper Copy')
    expect(snapshot.tools).toMatchObject([{ name: 'read', group: 'core' }])
    expect(snapshot.dbTouchpoints.reads[0]?.table).toBe('project_context_records')
    expect(snapshot.consumes[0]?.id).toBe('plan_pack')
    expect(snapshot.attachedSkills).toEqual([
      {
        id: 'rust-best-practices',
        sourceId: 'skill-source:v1:global:bundled:xero:rust-best-practices',
        skillId: 'rust-best-practices',
        name: 'Rust Best Practices',
        description: 'Guide for writing idiomatic Rust.',
        sourceKind: 'bundled',
        scope: 'global',
        versionHash: 'hash-rust',
        includeSupportingAssets: false,
        required: true,
      },
    ])
  })

  it('omits workflowStructure when no phases are authored', () => {
    const graph = buildAgentGraph(detail())
    const { snapshot } = buildSnapshotFromGraph(graph.nodes, graph.edges, {
      initialDefinitionId: 'release_notes_helper',
    })
    expect((snapshot as Record<string, unknown>).workflowStructure).toBeUndefined()
  })

  it('round-trips an authored workflow state machine through the canvas snapshot', () => {
    const source = detail()
    source.workflowStructure = {
      startPhaseId: 'gather',
      phases: [
        {
          id: 'gather',
          title: 'Gather',
          description: 'Pull approved release records.',
          allowedTools: ['read'],
          requiredChecks: [{ kind: 'tool_succeeded', toolName: 'read' }],
          branches: [{ targetPhaseId: 'draft', condition: { kind: 'always' } }],
        },
        {
          id: 'draft',
          title: 'Draft',
          description: 'Compose the release notes.',
        },
      ],
    }

    const snapshot = parsedSnapshot(source, 'release_notes_helper')

    expect(snapshot.workflowStructure).toEqual({
      startPhaseId: 'gather',
      phases: [
        {
          id: 'gather',
          title: 'Gather',
          description: 'Pull approved release records.',
          allowedTools: ['read'],
          requiredChecks: [{ kind: 'tool_succeeded', toolName: 'read' }],
          branches: [{ targetPhaseId: 'draft', condition: { kind: 'always' } }],
        },
        {
          id: 'draft',
          title: 'Draft',
          description: 'Compose the release notes.',
        },
      ],
    })
  })

  it('keeps workflow phases stable across save and reload', () => {
    const source = detail()
    source.workflowStructure = {
      startPhaseId: 'gather',
      phases: [
        {
          id: 'gather',
          title: 'Gather',
          description: 'Pull approved release records.',
          allowedTools: ['read'],
          branches: [{ targetPhaseId: 'draft', condition: { kind: 'always' } }],
        },
        {
          id: 'draft',
          title: 'Draft',
          description: 'Compose the release notes.',
        },
      ],
    }
    expectStableSaveReloadRoundTrip(source, 'release_notes_helper')
  })

  it('lets canvas-drawn phase-branch edges override authored branches', () => {
    const source = detail()
    source.workflowStructure = {
      startPhaseId: 'gather',
      phases: [
        { id: 'gather', title: 'Gather', branches: [] },
        { id: 'draft', title: 'Draft', branches: [] },
      ],
    }
    const graph = buildAgentGraph(source)
    const edges = [
      ...graph.edges,
      {
        id: 'e:phase-branch:gather->draft:0',
        source: 'workflow-phase:gather',
        target: 'workflow-phase:draft',
        type: 'phase-branch' as const,
        data: {
          category: 'phase-branch',
          sourcePhaseId: 'gather',
          targetPhaseId: 'draft',
          branchIndex: 0,
          condition: { kind: 'tool_succeeded' as const, toolName: 'read' },
        },
      },
    ]

    const snapshot = canonicalCustomAgentDefinitionSchema.parse(
      buildSnapshotFromGraph(graph.nodes, edges, {
        initialDefinitionId: 'release_notes_helper',
      }).snapshot,
    )

    expect(snapshot.workflowStructure?.phases[0]).toMatchObject({
      id: 'gather',
      branches: [
        {
          targetPhaseId: 'draft',
          condition: { kind: 'tool_succeeded', toolName: 'read' },
        },
      ],
    })
  })

  it('serializes attached skills from skills nodes', () => {
    const source = detail()
    source.attachedSkills = [attachedRustSkill()]
    const graph = buildAgentGraph(source)
    const skillNode = graph.nodes.find((node) => node.id === 'skills:rust-best-practices')
    if (skillNode?.type === 'skills') {
      skillNode.data.skill = {
        ...skillNode.data.skill,
        includeSupportingAssets: true,
      }
    }

    const snapshot = canonicalCustomAgentDefinitionSchema.parse(
      buildSnapshotFromGraph(graph.nodes, graph.edges, {
        initialDefinitionId: 'release_notes_helper',
      }).snapshot,
    )

    expect(snapshot.attachedSkills).toEqual([
      {
        id: 'rust-best-practices',
        sourceId: 'skill-source:v1:global:bundled:xero:rust-best-practices',
        skillId: 'rust-best-practices',
        name: 'Rust Best Practices',
        description: 'Guide for writing idiomatic Rust.',
        sourceKind: 'bundled',
        scope: 'global',
        versionHash: 'hash-rust',
        includeSupportingAssets: true,
        required: true,
      },
    ])
  })
})
