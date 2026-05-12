import { describe, expect, it } from 'vitest'

import type {
  AgentAttachedSkillDto,
  AgentDbTouchpointDetailDto,
  AgentToolSummaryDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

import {
  AGENT_GRAPH_HEADER_NODE_ID,
  AGENT_GRAPH_OUTPUT_NODE_ID,
  AGENT_GRAPH_TRIGGER_HANDLES,
  STAGE_GROUP_FRAME_NODE_ID,
  agentGraphFromProjection,
  buildAgentGraph,
  humanizeIdentifier,
  stageNodeId,
  type AgentHeaderNodeData,
  toolCategoryPresentationForGroup,
} from './build-agent-graph'
import { layoutAgentGraphByCategory } from './layout'

function dbDetail(
  table: string,
  triggers: AgentDbTouchpointDetailDto['triggers'] = [],
  purpose = 'authored purpose',
): AgentDbTouchpointDetailDto {
  return {
    table,
    kind: 'write',
    purpose,
    triggers,
    columns: [],
  }
}

function toolSummary(name: string, group: string): AgentToolSummaryDto {
  return {
    name,
    group,
    description: `${name} description.`,
    effectClass: 'observe',
    riskClass: 'observe',
    tags: [],
    schemaFields: [],
    examples: [],
  }
}

function attachedSkill(): AgentAttachedSkillDto {
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

function fixtureDetail(): WorkflowAgentDetailDto {
  return {
    ref: { kind: 'built_in', runtimeAgentId: 'engineer', version: 1 },
    header: {
      displayName: 'Engineer',
      shortLabel: 'Build',
      description: 'Implements repository changes.',
      taskPurpose: 'Inspect, plan, edit, verify, summarize.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'engineering',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest', 'auto_edit', 'yolo'],
      allowPlanGate: true,
      allowVerificationGate: true,
      allowAutoCompact: true,
    },
    promptPolicy: 'engineer',
    toolPolicy: 'engineering',
    prompts: [
      {
        id: 'xero.system_policy.engineer',
        label: 'System policy',
        role: 'system',
        policy: 'engineer',
        source: 'xero-runtime',
        body: 'You are Xero Engineer.',
      },
    ],
    tools: [
      {
        name: 'Read',
        group: 'core',
        description: 'Read file.',
        effectClass: 'observe',
        riskClass: 'observe',
        tags: ['file', 'read'],
        schemaFields: ['path'],
        examples: ['Read src/lib.rs'],
      },
      {
        name: 'Edit',
        group: 'mutation',
        description: 'Edit file.',
        effectClass: 'write',
        riskClass: 'write',
        tags: ['file', 'mutation'],
        schemaFields: ['path', 'content'],
        examples: [],
      },
    ],
    dbTouchpoints: {
      reads: [
        { ...dbDetail('agent_runs'), kind: 'read', purpose: 'reads run state' },
        { ...dbDetail('agent_messages'), kind: 'read', purpose: 'reads transcript' },
      ],
      writes: [
        { ...dbDetail('agent_runs'), kind: 'write', purpose: 'persists state' },
        dbDetail('code_history_operations', [
          { kind: 'tool', name: 'Edit' },
          { kind: 'lifecycle', event: 'file_edit' },
        ], 'appends an op row per file mutation'),
      ],
      encouraged: [
        {
          ...dbDetail('project_context_records'),
          kind: 'encouraged',
          purpose: 'captures handoff notes',
          triggers: [{ kind: 'output_section', id: 'handoff_context' }],
        },
      ],
    },
    output: {
      contract: 'engineering_summary',
      label: 'Engineering Summary',
      description: 'Summary of files changed and verification.',
      sections: [
        {
          id: 'files_changed',
          label: 'Files Changed',
          description: 'Per-file edit summary.',
          emphasis: 'core',
          producedByTools: ['Edit'],
        },
        {
          id: 'verification',
          label: 'Verification',
          description: 'Tests run and outcome.',
          emphasis: 'core',
          producedByTools: [],
        },
        {
          id: 'handoff_context',
          label: 'Handoff Context',
          description: 'Durable notes for the next agent.',
          emphasis: 'standard',
          producedByTools: [],
        },
      ],
    },
    consumes: [
      {
        id: 'plan_pack',
        label: 'Accepted Plan Pack',
        description: 'Slices and decisions from Plan.',
        sourceAgent: 'plan',
        contract: 'plan_pack',
        sections: ['decisions', 'slices', 'build_handoff'],
        required: true,
      },
    ],
    attachedSkills: [],
  }
}

describe('buildAgentGraph', () => {
  it('hydrates Rust graph projection DTOs into React Flow nodes and edges', () => {
    const graph = agentGraphFromProjection({
      schema: 'xero.workflow_agent_graph_projection.v1',
      nodes: [
        {
          id: 'tool-group-frame:mcp',
          type: 'tool-group-frame',
          position: { x: 0, y: 0 },
          data: { label: 'MCP', count: 1, order: 110, sourceGroups: ['mcp_invoke'] },
          dragHandle: '.agent-tool-group-frame__drag-handle',
          style: { pointerEvents: 'none' },
        },
        {
          id: 'tool:Mcp Call Tool',
          type: 'tool',
          position: { x: 12, y: 26 },
          parentId: 'tool-group-frame:mcp',
          extent: 'parent',
          draggable: false,
          style: { pointerEvents: 'all' },
          data: {
            tool: toolSummary('Mcp Call Tool', 'mcp_invoke'),
            directConnectionHandles: { source: true, target: false },
          },
        },
      ],
      edges: [
        {
          id: 'e:header->tool-group-frame:mcp',
          source: AGENT_GRAPH_HEADER_NODE_ID,
          target: 'tool-group-frame:mcp',
          sourceHandle: 'tools',
          type: 'smoothstep',
          data: { category: 'tool' },
          className: 'agent-edge agent-edge-tool',
          marker: 'arrow_closed',
        },
      ],
      groups: [
        {
          key: 'mcp',
          label: 'MCP',
          kind: 'tool_category',
          order: 110,
          nodeIds: ['tool:Mcp Call Tool'],
          sourceGroups: ['mcp_invoke'],
        },
      ],
    })

    expect(graph.nodes[1]?.parentId).toBe('tool-group-frame:mcp')
    expect(graph.nodes[1]?.draggable).toBe(false)
    expect(graph.edges[0]?.markerEnd).toBeDefined()
  })

  it('produces one node per logical entity plus header, output, sections, and consumes', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)

    expect(nodes.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID)).toBeDefined()
    expect(nodes.find((node) => node.id === AGENT_GRAPH_OUTPUT_NODE_ID)).toBeDefined()

    const promptNodes = nodes.filter((n) => n.type === 'prompt')
    const toolNodes = nodes.filter((n) => n.type === 'tool')
    const dbNodes = nodes.filter((n) => n.type === 'db-table')
    const sectionNodes = nodes.filter((n) => n.type === 'output-section')
    const consumedNodes = nodes.filter((n) => n.type === 'consumed-artifact')

    expect(promptNodes).toHaveLength(detail.prompts.length)
    expect(toolNodes).toHaveLength(detail.tools.length)
    // De-duplicated by table name across reads/writes/encouraged.
    expect(dbNodes.length).toBeGreaterThanOrEqual(3)
    expect(sectionNodes).toHaveLength(detail.output.sections.length)
    expect(consumedNodes).toHaveLength(detail.consumes.length)
    // Each touchpoint kind is represented somewhere.
    expect(dbNodes.some((n) => (n.data as { touchpoint: string }).touchpoint === 'write')).toBe(true)
    expect(dbNodes.some((n) => (n.data as { touchpoint: string }).touchpoint === 'read')).toBe(true)
    expect(dbNodes.some((n) => (n.data as { touchpoint: string }).touchpoint === 'encouraged')).toBe(true)
  })

  it('hydrates granular tool policy into the header advanced state', () => {
    const detail = fixtureDetail()
    detail.toolPolicyDetails = {
      allowedTools: ['Read'],
      deniedTools: ['Delete'],
      allowedToolPacks: ['repo_intel'],
      deniedToolPacks: ['external_network'],
      allowedToolGroups: ['core'],
      deniedToolGroups: ['browser_control'],
      allowedEffectClasses: ['observe', 'runtime_state'],
      externalServiceAllowed: false,
      browserControlAllowed: false,
      skillRuntimeAllowed: false,
      subagentAllowed: false,
      allowedSubagentRoles: [],
      deniedSubagentRoles: [],
      allowedMcpServers: [],
      deniedMcpServers: [],
      allowedDynamicTools: [],
      deniedDynamicTools: [],
      commandAllowed: true,
      destructiveWriteAllowed: false,
    }
    const { nodes } = buildAgentGraph(detail)
    const header = nodes.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID)
    const advanced = (header?.data as AgentHeaderNodeData | undefined)?.advanced

    expect(advanced).toMatchObject({
      allowedEffectClasses: ['observe', 'runtime_state'],
      deniedTools: ['Delete'],
      allowedToolPacks: ['repo_intel'],
      deniedToolPacks: ['external_network'],
      allowedToolGroups: ['core'],
      deniedToolGroups: ['browser_control'],
      commandAllowed: true,
      browserControlAllowed: false,
    })
  })

  it('renders a table that is both read and written as two distinct cards', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const agentRunsNodes = nodes.filter(
      (n) => n.type === 'db-table' && (n.data as { table: string }).table === 'agent_runs',
    )
    expect(agentRunsNodes).toHaveLength(2)
    const touchpoints = agentRunsNodes.map(
      (node) => (node.data as { touchpoint: string }).touchpoint,
    )
    expect(touchpoints).toContain('read')
    expect(touchpoints).toContain('write')
  })

  it('renders attached skills as dedicated skills nodes and header context', () => {
    const detail = fixtureDetail()
    detail.attachedSkills = [attachedSkill()]

    const { nodes, edges } = buildAgentGraph(detail)
    const skillNode = nodes.find((node) => node.id === 'skills:rust-best-practices')
    const header = nodes.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID)
    const skillEdge = edges.find((edge) => edge.target === skillNode?.id)

    expect(skillNode?.type).toBe('skills')
    expect(skillNode?.data.skill).toMatchObject({
      skillId: 'rust-best-practices',
      sourceKind: 'bundled',
      availabilityStatus: 'available',
    })
    expect((header?.data as AgentHeaderNodeData | undefined)?.summary.attachedSkills).toBe(1)
    expect(skillEdge).toMatchObject({
      source: AGENT_GRAPH_HEADER_NODE_ID,
      sourceHandle: 'skills',
      className: 'agent-edge agent-edge-skill',
    })
  })

  it('hides the skills lane until a skill is attached', () => {
    const detail = fixtureDetail()
    detail.ref = { kind: 'custom', definitionId: 'custom-agent', version: 1 }
    detail.header = {
      ...detail.header,
      scope: 'project_custom',
    }

    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(nodes, new Map())
    const skillsLane = placed.find((node) => node.id === 'lane:skills')

    expect(skillsLane).toBeUndefined()
  })

  it('places attached skills under consumes and follows consume stack height', () => {
    const detail = fixtureDetail()
    detail.attachedSkills = [attachedSkill()]

    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(nodes, new Map())
    const consumed = placed.find((node) => node.type === 'consumed-artifact')
    const consumedLane = placed.find((node) => node.id === 'lane:consumed-artifact')
    const skillLane = placed.find((node) => node.id === 'lane:skills')
    const skillNode = placed.find((node) => node.id === 'skills:rust-best-practices')
    const consumedBottom = (consumed?.position.y ?? 0) + 96

    expect(skillLane?.position.x).toBe(
      (consumed?.position.x ?? 0) + (((consumedLane?.width ?? 0) - (skillLane?.width ?? 0)) / 2),
    )
    expect(skillLane?.position.y).toBe(consumedBottom + 64)
    expect(skillNode?.position.y).toBe((skillLane?.position.y ?? 0) + 26 + 16)

    const tallerDetail = fixtureDetail()
    tallerDetail.attachedSkills = [attachedSkill()]
    tallerDetail.consumes = [
      ...tallerDetail.consumes,
      {
        id: 'debug_summary',
        label: 'Debug Summary',
        description: 'Root cause notes from Debug.',
        sourceAgent: 'debug',
        contract: 'debug_summary',
        sections: ['constraints'],
        required: true,
      },
    ]
    const { nodes: tallerNodes } = buildAgentGraph(tallerDetail)
    const tallerPlaced = layoutAgentGraphByCategory(tallerNodes, new Map())
    const tallerSkillLane = tallerPlaced.find((node) => node.id === 'lane:skills')

    expect(tallerSkillLane?.position.x).toBe(skillLane?.position.x)
    expect(tallerSkillLane?.position.y).toBeGreaterThan(skillLane?.position.y ?? 0)
  })

  it('places stages above consumes and raises taller stage stacks automatically', () => {
    const detail = fixtureDetail()
    detail.workflowStructure = {
      startPhaseId: 'discover',
      phases: [
        { id: 'discover', title: 'Discover' },
        { id: 'draft', title: 'Draft' },
      ],
    }

    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(nodes, new Map())
    const consumed = placed.find((node) => node.type === 'consumed-artifact')
    const consumedLane = placed.find((node) => node.id === 'lane:consumed-artifact')
    const stageFrame = placed.find((node) => node.id === STAGE_GROUP_FRAME_NODE_ID)
    const stageLane = placed.find((node) => node.id === 'lane:stage')
    const firstStage = placed.find((node) => node.id === stageNodeId('discover'))
    const stageFrameBottom = (stageFrame?.position.y ?? 0) + (stageFrame?.height ?? 0)

    expect(stageFrame?.position.x).toBe(
      (consumed?.position.x ?? 0) + (((consumedLane?.width ?? 0) - 284) / 2),
    )
    expect(stageFrameBottom).toBe((consumedLane?.position.y ?? 0) - 64)
    expect(stageFrame?.width).toBe(284)
    expect(stageFrame?.height).toBe(304)
    expect(stageLane?.position.x).toBe(stageFrame?.position.x)
    expect(stageLane?.position.y).toBe((stageFrame?.position.y ?? 0) - 26 - 16)
    expect(firstStage?.position).toEqual({ x: 12, y: 12 })

    const tallerDetail = fixtureDetail()
    tallerDetail.workflowStructure = {
      startPhaseId: 'survey',
      phases: [
        { id: 'survey', title: 'Survey' },
        { id: 'plan', title: 'Plan' },
        { id: 'implement', title: 'Implement' },
        { id: 'verify', title: 'Verify' },
      ],
    }
    const { nodes: tallerNodes } = buildAgentGraph(tallerDetail)
    const tallerPlaced = layoutAgentGraphByCategory(tallerNodes, new Map())
    const tallerConsumedLane = tallerPlaced.find((node) => node.id === 'lane:consumed-artifact')
    const tallerStageFrame = tallerPlaced.find((node) => node.id === STAGE_GROUP_FRAME_NODE_ID)
    const tallerStageFrameBottom =
      (tallerStageFrame?.position.y ?? 0) + (tallerStageFrame?.height ?? 0)

    expect(tallerStageFrame?.height).toBeGreaterThan(stageFrame?.height ?? 0)
    expect(tallerStageFrame?.position.y).toBeLessThan(stageFrame?.position.y ?? 0)
    expect(tallerStageFrame?.position.x).toBe(stageFrame?.position.x)
    expect(tallerStageFrameBottom).toBe((tallerConsumedLane?.position.y ?? 0) - 64)
  })

  it('connects every non-header, non-section, non-consumed node back to the header', () => {
    const detail = fixtureDetail()
    const { nodes, edges } = buildAgentGraph(detail)
    for (const node of nodes) {
      if (node.id === AGENT_GRAPH_HEADER_NODE_ID) continue
      if (node.type === 'output-section') continue // parented under output, not header
      if (node.type === 'consumed-artifact') continue // flows into header
      if (node.type === 'lane-label') continue
      if (node.type === 'tool') continue // tools are parented under their tool-group-frame
      const hasEdge = edges.some(
        (edge) => edge.source === AGENT_GRAPH_HEADER_NODE_ID && edge.target === node.id,
      )
      expect(hasEdge, `${node.id} should be reachable from header`).toBe(true)
    }
  })

  it('parents tool nodes under a tool-group-frame and connects the frame to the header', () => {
    const detail = fixtureDetail()
    const { nodes, edges } = buildAgentGraph(detail)
    const toolNodes = nodes.filter((n) => n.type === 'tool')
    expect(toolNodes.length).toBeGreaterThan(0)
    for (const tool of toolNodes) {
      const parentId = (tool as { parentId?: string }).parentId
      expect(parentId, `${tool.id} should declare a parent tool-group-frame`).toBeTruthy()
      const parent = nodes.find((n) => n.id === parentId)
      expect(parent?.type).toBe('tool-group-frame')
      expect(parent?.dragHandle).toBe('.agent-tool-group-frame__drag-handle')
      expect(parent?.style?.pointerEvents).toBe('none')
      expect(tool.style?.pointerEvents).toBe('all')
      const headerToFrame = edges.some(
        (edge) => edge.source === AGENT_GRAPH_HEADER_NODE_ID && edge.target === parentId,
      )
      expect(headerToFrame, `${parentId} should be connected to the header`).toBe(true)
    }
  })

  it('marks tool handles only when direct trigger edges touch the tool', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const readTool = nodes.find((node) => node.id === 'tool:Read')
    const editTool = nodes.find((node) => node.id === 'tool:Edit')

    expect(readTool?.data.directConnectionHandles).toEqual({
      source: false,
      target: false,
    })
    expect(editTool?.data.directConnectionHandles).toEqual({
      source: true,
      target: false,
    })
  })

  it('groups split runtime access families into one visual tool category', () => {
    const detail = fixtureDetail()
    detail.tools = [
      toolSummary('Mcp List', 'mcp_list'),
      toolSummary('Mcp Call Tool', 'mcp_invoke'),
      toolSummary('Mcp Read Resource', 'mcp_invoke'),
      toolSummary('System Diagnostics', 'system_diagnostics_observe'),
      toolSummary('System Diagnostics Privileged', 'system_diagnostics_privileged'),
      toolSummary('Browser Observe', 'browser_observe'),
      toolSummary('Browser Control', 'browser_control'),
    ]

    const { nodes, edges } = buildAgentGraph(detail)
    const frameIds = nodes
      .filter((node) => node.type === 'tool-group-frame')
      .map((node) => node.id)

    expect(frameIds).toEqual([
      'tool-group-frame:system_diagnostics',
      'tool-group-frame:browser',
      'tool-group-frame:mcp',
    ])
    expect(frameIds).not.toContain('tool-group-frame:mcp_list')
    expect(frameIds).not.toContain('tool-group-frame:mcp_invoke')
    expect(frameIds).not.toContain('tool-group-frame:system_diagnostics_observe')
    expect(frameIds).not.toContain('tool-group-frame:system_diagnostics_privileged')

    const mcpFrame = nodes.find((node) => node.id === 'tool-group-frame:mcp')
    const diagnosticsFrame = nodes.find(
      (node) => node.id === 'tool-group-frame:system_diagnostics',
    )
    const browserFrame = nodes.find((node) => node.id === 'tool-group-frame:browser')

    expect(mcpFrame?.data).toMatchObject({
      label: 'MCP',
      count: 3,
      sourceGroups: ['mcp_invoke', 'mcp_list'],
    })
    expect(diagnosticsFrame?.data).toMatchObject({
      label: 'System Diagnostics',
      count: 2,
      sourceGroups: [
        'system_diagnostics_observe',
        'system_diagnostics_privileged',
      ],
    })
    expect(browserFrame?.data).toMatchObject({
      label: 'Browser',
      count: 2,
      sourceGroups: ['browser_control', 'browser_observe'],
    })

    for (const toolName of ['Mcp List', 'Mcp Call Tool', 'Mcp Read Resource']) {
      expect((nodes.find((node) => node.id === `tool:${toolName}`) as { parentId?: string })
        ?.parentId).toBe('tool-group-frame:mcp')
    }
    expect(
      edges.some(
        (edge) =>
          edge.source === AGENT_GRAPH_HEADER_NODE_ID &&
          edge.target === 'tool-group-frame:mcp',
      ),
    ).toBe(true)
  })

  it('uses user-facing tool category labels for raw runtime groups', () => {
    expect(toolCategoryPresentationForGroup('mutation')).toMatchObject({
      key: 'file_changes',
      label: 'File Changes',
    })
    expect(toolCategoryPresentationForGroup('command_session')).toMatchObject({
      key: 'commands',
      label: 'Commands',
    })
    expect(toolCategoryPresentationForGroup('macos')).toMatchObject({
      key: 'os_automation',
      label: 'OS Automation',
    })
    expect(humanizeIdentifier('mcp_list')).toBe('MCP List')
    expect(humanizeIdentifier('solana_idl')).toBe('Solana IDL')
  })

  it('marks lane labels as draggable section handles', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(nodes, new Map())
    const toolLane = placed.find((node) => node.id === 'lane:tool')

    expect(toolLane?.type).toBe('lane-label')
    expect(toolLane?.selectable).toBe(false)
    expect(toolLane?.draggable).toBe(true)
    expect(toolLane?.dragHandle).toBe('.agent-graph-lane-label')
  })

  it('matches the default prompt/header gap to the header/output gap', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(nodes, new Map())
    const header = placed.find((node) => node.id === AGENT_GRAPH_HEADER_NODE_ID)
    const prompt = placed.find((node) => node.type === 'prompt')
    const output = placed.find((node) => node.id === AGENT_GRAPH_OUTPUT_NODE_ID)

    const promptGap = (header?.position.y ?? 0) - ((prompt?.position.y ?? 0) + 48)
    const outputGap = (output?.position.y ?? 0) - ((header?.position.y ?? 0) + 210)

    expect(promptGap).toBe(100)
    expect(outputGap).toBe(promptGap)
  })

  it('keeps an expanded tool row top-anchored inside its category frame', () => {
    const detail = fixtureDetail()
    const baseTool = detail.tools[0]!
    detail.tools = ['alpha', 'middle', 'omega'].map((name) => ({
      ...baseTool,
      name,
      group: 'core',
    }))

    const { nodes } = buildAgentGraph(detail)
    const collapsed = layoutAgentGraphByCategory(nodes, new Map())
    const expanded = layoutAgentGraphByCategory(
      nodes,
      new Map([['tool:middle', { width: 240, height: 156 }]]),
    )

    const collapsedFrame = collapsed.find((node) => node.id === 'tool-group-frame:core')
    const expandedFrame = expanded.find((node) => node.id === 'tool-group-frame:core')
    const collapsedAlpha = collapsed.find((node) => node.id === 'tool:alpha')
    const collapsedMiddle = collapsed.find((node) => node.id === 'tool:middle')
    const expandedMiddle = expanded.find((node) => node.id === 'tool:middle')
    const collapsedOmega = collapsed.find((node) => node.id === 'tool:omega')
    const expandedOmega = expanded.find((node) => node.id === 'tool:omega')

    expect(collapsedMiddle?.position.y).toBe((collapsedAlpha?.position.y ?? 0) + 36 + 16)
    expect(collapsedOmega?.position.y).toBe((collapsedMiddle?.position.y ?? 0) + 36 + 16)
    expect(expandedFrame?.position.y).toBe(collapsedFrame?.position.y)
    expect(expandedMiddle?.position.y).toBe(collapsedMiddle?.position.y)
    expect(expandedOmega?.position.y).toBe((collapsedOmega?.position.y ?? 0) + 120)
  })

  it('parents output-section nodes under the output node', () => {
    const detail = fixtureDetail()
    const { nodes, edges } = buildAgentGraph(detail)
    const sectionNodes = nodes.filter((n) => n.type === 'output-section')
    for (const section of sectionNodes) {
      const parented = edges.some(
        (edge) => edge.source === AGENT_GRAPH_OUTPUT_NODE_ID && edge.target === section.id,
      )
      expect(parented, `${section.id} should be parented under output`).toBe(true)
    }
  })

  it('stacks output sections using each section row height', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(
      nodes,
      new Map([
        ['output-section:files_changed', { width: 200, height: 32 }],
        ['output-section:verification', { width: 200, height: 140 }],
        ['output-section:handoff_context', { width: 200, height: 32 }],
      ]),
    )

    const first = placed.find((node) => node.id === 'output-section:files_changed')
    const second = placed.find((node) => node.id === 'output-section:verification')
    const third = placed.find((node) => node.id === 'output-section:handoff_context')

    expect(first?.position.y).toBeDefined()
    expect(second?.position.y).toBe((first?.position.y ?? 0) + 32 + 16)
    expect(third?.position.y).toBe((second?.position.y ?? 0) + 140 + 16)
  })

  it('keeps database column gaps consistent for variable card heights', () => {
    const detail = fixtureDetail()
    detail.dbTouchpoints = {
      reads: [],
      writes: [
        dbDetail('short_table'),
        dbDetail('medium_table'),
        dbDetail('tall_table'),
      ],
      encouraged: [],
    }
    const { nodes } = buildAgentGraph(detail)
    const placed = layoutAgentGraphByCategory(
      nodes,
      new Map([
        ['db:write:short_table', { width: 260, height: 117 }],
        ['db:write:medium_table', { width: 260, height: 153 }],
        ['db:write:tall_table', { width: 260, height: 189 }],
      ]),
    )

    const heights = new Map([
      ['db:write:short_table', 117],
      ['db:write:medium_table', 153],
      ['db:write:tall_table', 189],
    ])
    const dbNodes = [...heights.keys()]
      .map((id) => placed.find((node) => node.id === id))
      .filter((node): node is NonNullable<typeof node> => Boolean(node))
      .sort((a, b) => a.position.y - b.position.y)

    expect(dbNodes).toHaveLength(3)
    expect(dbNodes[1]!.position.y).toBe(
      dbNodes[0]!.position.y + heights.get(dbNodes[0]!.id)! + 16,
    )
    expect(dbNodes[2]!.position.y).toBe(
      dbNodes[1]!.position.y + heights.get(dbNodes[1]!.id)! + 16,
    )
  })

  it('emits consumed-artifact edges that flow into the header', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const consumedEdges = edges.filter((e) => e.className === 'agent-edge agent-edge-consume')
    expect(consumedEdges).toHaveLength(detail.consumes.length)
    for (const edge of consumedEdges) {
      expect(edge.target).toBe(AGENT_GRAPH_HEADER_NODE_ID)
      expect(edge.targetHandle).toBe('consumed')
    }
  })

  it('draws a tool→db trigger edge when a touchpoint lists a tool trigger', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const triggerEdges = edges.filter((e) => e.className === 'agent-edge agent-edge-trigger')
    const editToCodeHistory = triggerEdges.find(
      (e) => e.source === 'tool:Edit' && e.target === 'db:write:code_history_operations',
    )
    expect(editToCodeHistory, 'expected Edit → code_history_operations edge').toBeDefined()
    expect(editToCodeHistory?.sourceHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.source)
    expect(editToCodeHistory?.targetHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.target)
  })

  it('draws a tool→output-section trigger edge when producedByTools lists a tool', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const editToFilesChanged = edges.find(
      (e) => e.source === 'tool:Edit' && e.target === 'output-section:files_changed',
    )
    expect(editToFilesChanged, 'expected Edit → files_changed edge').toBeDefined()
    expect(editToFilesChanged?.className).toBe('agent-edge agent-edge-trigger')
    expect(editToFilesChanged?.sourceHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.source)
    expect(editToFilesChanged?.targetHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.target)
  })

  it('draws a section→db trigger edge when a touchpoint lists an output_section trigger', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const sectionToDb = edges.find(
      (e) =>
        e.source === 'output-section:handoff_context' &&
        e.target === 'db:encouraged:project_context_records',
    )
    expect(sectionToDb, 'expected handoff_context → project_context_records edge').toBeDefined()
    expect(sectionToDb?.className).toBe('agent-edge agent-edge-trigger')
    expect(sectionToDb?.sourceHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.source)
    expect(sectionToDb?.targetHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.target)
  })

  it('handles agents with no encouraged tables and no consumed artifacts', () => {
    const detail = fixtureDetail()
    detail.dbTouchpoints.encouraged = []
    detail.consumes = []
    const { nodes } = buildAgentGraph(detail)
    expect(
      nodes.some(
        (n) => n.type === 'db-table' && (n.data as { touchpoint: string }).touchpoint === 'encouraged',
      ),
    ).toBe(false)
    expect(nodes.some((n) => n.type === 'consumed-artifact')).toBe(false)
  })

  it('does not emit a trigger edge when the referenced tool is not in the agent tool list', () => {
    const detail = fixtureDetail()
    detail.dbTouchpoints.writes[1] = {
      ...detail.dbTouchpoints.writes[1]!,
      triggers: [{ kind: 'tool', name: 'NotAToolTheAgentHas' }],
    }
    const { edges } = buildAgentGraph(detail)
    const orphan = edges.find(
      (e) => e.source === 'tool:NotAToolTheAgentHas',
    )
    expect(orphan).toBeUndefined()
  })

  it('does not emit a node or edge for lifecycle triggers — they live as in-card chips', () => {
    const detail = fixtureDetail()
    const { nodes, edges } = buildAgentGraph(detail)
    expect(nodes.some((n) => n.id.startsWith('lifecycle:'))).toBe(false)
    const codeHistoryEdges = edges.filter(
      (e) => e.target === 'db:write:code_history_operations',
    )
    // Ownership edge from header + tool trigger from Edit, but no lifecycle edge.
    for (const edge of codeHistoryEdges) {
      expect(edge.source.startsWith('lifecycle:')).toBe(false)
    }
    // The DB node still carries the lifecycle trigger in its data so the card
    // can render it as a chip in place.
    const codeHistoryNode = nodes.find(
      (n) => n.id === 'db:write:code_history_operations',
    )
    const triggers =
      (codeHistoryNode?.data as { triggers?: { kind: string }[] } | undefined)
        ?.triggers ?? []
    expect(triggers.some((t) => t.kind === 'lifecycle')).toBe(true)
  })

  it('emits an upstream-artifact trigger edge when a touchpoint references a consumed artifact', () => {
    const detail = fixtureDetail()
    detail.dbTouchpoints.writes = [
      ...detail.dbTouchpoints.writes,
      {
        table: 'agent_handoffs',
        kind: 'write',
        purpose: 'mirrors upstream handoff',
        triggers: [{ kind: 'upstream_artifact', id: 'plan_pack' }],
        columns: [],
      },
    ]
    const { edges } = buildAgentGraph(detail)
    const upstreamEdge = edges.find(
      (e) => e.source === 'consumed:plan_pack' && e.target === 'db:write:agent_handoffs',
    )
    expect(upstreamEdge, 'expected consumed-artifact → db trigger edge').toBeDefined()
    expect(upstreamEdge?.className).toBe('agent-edge agent-edge-trigger')
    expect(upstreamEdge?.sourceHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.source)
    expect(upstreamEdge?.targetHandle).toBe(AGENT_GRAPH_TRIGGER_HANDLES.target)
  })

  it('labels tool→DB trigger edges with the touchpoint kind', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const editToCodeHistory = edges.find(
      (e) => e.source === 'tool:Edit' && e.target === 'db:write:code_history_operations',
    )
    expect(editToCodeHistory?.type).toBe('trigger')
    expect((editToCodeHistory?.data as { triggerLabel?: string } | undefined)?.triggerLabel).toBe(
      'writes',
    )
  })

  it('labels tool→output-section trigger edges with "produces"', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const editToFilesChanged = edges.find(
      (e) => e.source === 'tool:Edit' && e.target === 'output-section:files_changed',
    )
    expect(editToFilesChanged?.type).toBe('trigger')
    expect((editToFilesChanged?.data as { triggerLabel?: string } | undefined)?.triggerLabel).toBe(
      'produces',
    )
  })

  it('exposes a header summary count that matches the number of DB touchpoint cards', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const headerNode = nodes.find((n) => n.id === AGENT_GRAPH_HEADER_NODE_ID)
    const dbNodes = nodes.filter((n) => n.type === 'db-table')
    const summary = (headerNode?.data as { summary?: { dbTables?: number } } | undefined)
      ?.summary
    expect(summary?.dbTables).toBe(dbNodes.length)
  })

  it('attaches a directional marker to every ownership edge', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const ownershipFamilies = [
      'agent-edge agent-edge-prompt',
      'agent-edge agent-edge-tool',
      'agent-edge agent-edge-db',
      'agent-edge agent-edge-output',
      'agent-edge agent-edge-output-section',
      'agent-edge agent-edge-consume',
    ]
    const familyEdges = edges.filter((e) =>
      ownershipFamilies.includes(e.className ?? ''),
    )
    expect(familyEdges.length).toBeGreaterThan(0)
    for (const edge of familyEdges) {
      expect(edge.markerEnd, `${edge.id} should have a markerEnd`).toBeDefined()
    }
  })
})
