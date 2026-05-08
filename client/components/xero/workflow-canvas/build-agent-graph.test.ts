import { describe, expect, it } from 'vitest'

import type {
  AgentDbTouchpointDetailDto,
  WorkflowAgentDetailDto,
} from '@/src/lib/xero-model/workflow-agents'

import {
  AGENT_GRAPH_HEADER_NODE_ID,
  AGENT_GRAPH_OUTPUT_NODE_ID,
  buildAgentGraph,
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
  }
}

describe('buildAgentGraph', () => {
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

  it('classifies tables that appear in writes as write-only nodes (write wins over read)', () => {
    const detail = fixtureDetail()
    const { nodes } = buildAgentGraph(detail)
    const agentRunsNodes = nodes.filter(
      (n) => n.type === 'db-table' && (n.data as { table: string }).table === 'agent_runs',
    )
    expect(agentRunsNodes).toHaveLength(1)
    expect((agentRunsNodes[0]!.data as { touchpoint: string }).touchpoint).toBe('write')
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
    const collapsedMiddle = collapsed.find((node) => node.id === 'tool:middle')
    const expandedMiddle = expanded.find((node) => node.id === 'tool:middle')
    const collapsedOmega = collapsed.find((node) => node.id === 'tool:omega')
    const expandedOmega = expanded.find((node) => node.id === 'tool:omega')

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
    expect(second?.position.y).toBe((first?.position.y ?? 0) + 32 + 12)
    expect(third?.position.y).toBe((second?.position.y ?? 0) + 140 + 12)
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
  })

  it('draws a tool→output-section trigger edge when producedByTools lists a tool', () => {
    const detail = fixtureDetail()
    const { edges } = buildAgentGraph(detail)
    const editToFilesChanged = edges.find(
      (e) => e.source === 'tool:Edit' && e.target === 'output-section:files_changed',
    )
    expect(editToFilesChanged, 'expected Edit → files_changed edge').toBeDefined()
    expect(editToFilesChanged?.className).toBe('agent-edge agent-edge-trigger')
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
})
