import { describe, expect, it } from 'vitest'

import {
  validateWorkflowDefinition,
  type WorkflowDefinitionDto,
  type WorkflowNodeDto,
} from './workflow-definition'
import {
  WORKFLOW_TEMPLATE_LIBRARY,
  instantiateBlankWorkflow,
  instantiateWorkflowTemplate,
} from './workflow-templates'

const builtInAgentRef = {
  kind: 'built_in',
  runtimeAgentId: 'engineer',
  version: 2,
} as const

const readableTemplateCardWidth = 240
const readableTemplateCardHeight = 96
const readableTemplateHorizontalGap = 140
const readableTemplateVerticalGap = 16

function linearWorkflow(): WorkflowDefinitionDto {
  return {
    schema: 'xero.workflow_definition.v1',
    id: 'workflow-linear',
    projectId: 'project-1',
    name: 'Linear workflow',
    description: '',
    version: 1,
    startNodeId: 'agent-a',
    nodes: [
      {
        id: 'agent-a',
        type: 'agent',
        title: 'Agent A',
        description: '',
        position: { x: 0, y: 0 },
        agentRef: builtInAgentRef,
        inputBindings: [],
        outputContract: {
          artifactType: 'text_output',
          schemaVersion: 1,
          extraction: 'generic_text',
          required: true,
        },
        failurePolicy: {
          quotaFailureClasses: [],
          transientFailureClasses: [],
        },
        resourceScopes: [],
      },
      {
        id: 'agent-b',
        type: 'agent',
        title: 'Agent B',
        description: '',
        position: { x: 320, y: 0 },
        agentRef: {
          kind: 'custom',
          definitionId: 'project-agent',
          version: 1,
        },
        inputBindings: [
          {
            source: 'artifact',
            name: 'handoff',
            required: true,
            artifactRef: 'agent-a.text_output',
          },
        ],
        outputContract: {
          artifactType: 'implementation_summary',
          schemaVersion: 1,
          extraction: 'generic_text',
          required: true,
        },
        failurePolicy: {
          quotaFailureClasses: [],
          transientFailureClasses: [],
        },
        resourceScopes: [],
      },
      {
        id: 'done',
        type: 'terminal',
        title: 'Done',
        description: '',
        position: { x: 640, y: 0 },
        terminalStatus: 'success',
      },
    ],
    edges: [
      {
        id: 'edge-a-b',
        fromNodeId: 'agent-a',
        toNodeId: 'agent-b',
        type: 'success',
        label: '',
        priority: 10,
        condition: { kind: 'node_status', nodeId: 'agent-a', status: 'succeeded' },
      },
      {
        id: 'edge-b-done',
        fromNodeId: 'agent-b',
        toNodeId: 'done',
        type: 'success',
        label: '',
        priority: 10,
        condition: { kind: 'always' },
      },
    ],
    subgraphs: [],
    artifactContracts: [],
    runPolicy: {
      concurrencyLimit: 1,
      resourceConflictPolicy: {
        mode: 'serialize_conflicts',
        defaultScopes: [],
      },
      recoveryDefaults: {
        debugMaxAttempts: 2,
        gapClosureMaxAttempts: 2,
        reviewFixMaxAttempts: 3,
      },
    },
  }
}

describe('validateWorkflowDefinition', () => {
  it('accepts a linear workflow with a custom downstream agent', () => {
    const report = validateWorkflowDefinition(linearWorkflow())

    expect(report).toEqual({ status: 'valid', diagnostics: [] })
  })

  it('accepts a conditional router with one explicit else edge', () => {
    const workflow = linearWorkflow()
    workflow.nodes.splice(2, 0, {
      id: 'router',
      type: 'router',
      title: 'Route',
      description: '',
      position: { x: 640, y: 0 },
    })
    workflow.edges = [
      {
        id: 'edge-a-router',
        fromNodeId: 'agent-a',
        toNodeId: 'router',
        type: 'success',
        label: '',
        priority: 10,
        condition: { kind: 'always' },
      },
      {
        id: 'edge-router-agent-b',
        fromNodeId: 'router',
        toNodeId: 'agent-b',
        type: 'conditional',
        label: 'needs work',
        priority: 10,
        condition: {
          kind: 'artifact_field_equals',
          artifactRef: 'agent-a.text_output',
          path: '$.status',
          value: 'needs_changes',
        },
      },
      {
        id: 'edge-router-done',
        fromNodeId: 'router',
        toNodeId: 'done',
        type: 'conditional',
        label: 'else',
        priority: 999,
        condition: { kind: 'always' },
      },
      {
        id: 'edge-b-done',
        fromNodeId: 'agent-b',
        toNodeId: 'done',
        type: 'success',
        label: '',
        priority: 10,
        condition: { kind: 'always' },
      },
    ]

    const report = validateWorkflowDefinition(workflow)
    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'valid' })
  })

  it('rejects a cycle without an explicit loop policy', () => {
    const workflow = linearWorkflow()
    workflow.edges.push({
      id: 'edge-b-a',
      fromNodeId: 'agent-b',
      toNodeId: 'agent-a',
      type: 'conditional',
      label: 'retry',
      priority: 20,
      condition: { kind: 'always' },
    })

    const report = validateWorkflowDefinition(workflow)

    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'invalid' })
    expect(report.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      'cycle_without_loop_policy',
    )
  })

  it('accepts a bounded loop with an exhaustion target', () => {
    const workflow = linearWorkflow()
    workflow.nodes.push({
      id: 'human',
      type: 'human_checkpoint',
      title: 'Human review',
      description: '',
      position: { x: 320, y: 240 },
      checkpointType: 'decision',
      prompt: 'Choose the next route.',
      decisionOptions: ['retry', 'stop'],
      resumePayloadSchema: null,
      stateUpdates: [],
    })
    workflow.edges.push({
      id: 'edge-b-a',
      fromNodeId: 'agent-b',
      toNodeId: 'agent-a',
      type: 'loop',
      label: 'retry',
      priority: 20,
      condition: { kind: 'loop_attempt_lt', loopKey: 'agent_retry', value: 2 },
      loopPolicy: {
        loopKey: 'agent_retry',
        maxAttempts: 2,
        attemptScope: 'run',
        carryoverPolicy: 'all',
        selectedArtifactRefs: [],
        resetPolicy: 'never',
        onExhausted: 'human',
      },
    })

    const report = validateWorkflowDefinition(workflow)
    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'valid' })
  })

  it('rejects router conditions that read fields outside the artifact schema', () => {
    const workflow = linearWorkflow()
    const firstNode = workflow.nodes[0]
    if (firstNode.type !== 'agent') throw new Error('expected first node to be an agent')
    workflow.nodes[0] = {
      ...firstNode,
      outputContract: {
        artifactType: 'verification_result',
        schemaVersion: 1,
        extraction: 'json_object',
        required: true,
      },
    }
    if (workflow.nodes[1].type === 'agent') {
      workflow.nodes[1] = {
        ...workflow.nodes[1],
        inputBindings: [{
          source: 'artifact',
          name: 'handoff',
          required: true,
          artifactRef: 'agent-a.verification_result',
        }],
      }
    }
    workflow.artifactContracts = [
      {
        artifactType: 'verification_result',
        schemaVersion: 1,
        displayName: 'Verification result',
        description: '',
        jsonSchema: {
          type: 'object',
          required: ['status'],
          properties: {
            status: { type: 'string', enum: ['passed', 'gaps_found', 'human_needed'] },
          },
          additionalProperties: false,
        },
      },
    ]
    workflow.edges[0] = {
      ...workflow.edges[0],
      condition: {
        kind: 'artifact_field_equals',
        artifactRef: 'agent-a.verification_result',
        path: '$.missing',
        value: 'passed',
      },
    }

    const report = validateWorkflowDefinition(workflow)

    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'invalid' })
    expect(report.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      'condition_artifact_path_not_in_schema',
    )
  })

  it('accepts router conditions that read declared artifact schema fields', () => {
    const workflow = linearWorkflow()
    const firstNode = workflow.nodes[0]
    if (firstNode.type !== 'agent') throw new Error('expected first node to be an agent')
    workflow.nodes[0] = {
      ...firstNode,
      outputContract: {
        artifactType: 'verification_result',
        schemaVersion: 1,
        extraction: 'json_object',
        required: true,
      },
    }
    if (workflow.nodes[1].type === 'agent') {
      workflow.nodes[1] = {
        ...workflow.nodes[1],
        inputBindings: [{
          source: 'artifact',
          name: 'handoff',
          required: true,
          artifactRef: 'agent-a.verification_result',
        }],
      }
    }
    workflow.artifactContracts = [
      {
        artifactType: 'verification_result',
        schemaVersion: 1,
        displayName: 'Verification result',
        description: '',
        jsonSchema: {
          type: 'object',
          required: ['status'],
          properties: {
            status: { type: 'string', enum: ['passed', 'gaps_found', 'human_needed'] },
          },
          additionalProperties: false,
        },
      },
    ]
    workflow.edges[0] = {
      ...workflow.edges[0],
      condition: {
        kind: 'artifact_field_equals',
        artifactRef: 'agent-a.verification_result',
        path: '$.status',
        value: 'passed',
      },
    }

    const report = validateWorkflowDefinition(workflow)
    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'valid' })
  })

  it('rejects two default else edges from the same node', () => {
    const workflow = linearWorkflow()
    workflow.edges.push({
      id: 'edge-b-other-default',
      fromNodeId: 'agent-b',
      toNodeId: 'done',
      type: 'conditional',
      label: 'else again',
      priority: 999,
      condition: { kind: 'always' },
    })

    const report = validateWorkflowDefinition(workflow)

    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'invalid' })
    expect(report.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      'duplicate_default_edge',
    )
  })

  it('accepts every starter workflow template', () => {
    for (const template of WORKFLOW_TEMPLATE_LIBRARY) {
      const report = validateWorkflowDefinition(
        instantiateWorkflowTemplate({
          projectId: 'project-1',
          templateId: template.id,
        }),
      )

      expect(report, template.id).toEqual({ status: 'valid', diagnostics: [] })
    }
  })

  it('exposes starter templates before the advanced GSD Auto template', () => {
    expect(WORKFLOW_TEMPLATE_LIBRARY.map((template) => template.id)).toEqual([
      'continuous_delivery',
      'bug_triage_fix_loop',
      'release_train',
      'gsd_auto',
    ])
    expect(WORKFLOW_TEMPLATE_LIBRARY.map((template) => template.difficulty)).toEqual([
      'starter',
      'intermediate',
      'intermediate',
      'advanced',
    ])
  })

  it('uses the visible template name as the draft workflow name', () => {
    for (const template of WORKFLOW_TEMPLATE_LIBRARY) {
      const workflow = instantiateWorkflowTemplate({
        projectId: 'project-1',
        templateId: template.id,
      })

      expect(workflow.name).toBe(template.name)
    }
  })

  it('spaces starter workflow templates into readable lanes', () => {
    for (const template of WORKFLOW_TEMPLATE_LIBRARY) {
      const workflow = instantiateWorkflowTemplate({
        projectId: 'project-1',
        templateId: template.id,
      })

      expectReadableTemplateNodeSpacing(workflow.nodes, template.id)
      for (const subgraph of workflow.subgraphs) {
        expectReadableTemplateNodeSpacing(subgraph.nodes, `${template.id}:${subgraph.id}`)
      }
    }
  })

  it('keeps starter workflow templates reachable including loop exhaustion routes', () => {
    for (const template of WORKFLOW_TEMPLATE_LIBRARY) {
      const workflow = instantiateWorkflowTemplate({
        projectId: 'project-1',
        templateId: template.id,
      })

      expect(expectUnreachableWorkflowNodes(workflow, template.id)).toEqual([])
      for (const subgraph of workflow.subgraphs) {
        expect(expectUnreachableWorkflowNodes(subgraph, `${template.id}:${subgraph.id}`)).toEqual([])
      }
    }
  })

  it('models GSD Auto as a visible milestone and phase delivery loop', () => {
    const workflow = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })
    const report = validateWorkflowDefinition(workflow)

    expect(report, JSON.stringify(report.diagnostics)).toEqual({ status: 'valid', diagnostics: [] })
    expect(workflow.startNodeId).toBe('gsd_start')
    expect(workflow.nodes).toHaveLength(47)
    expect(workflow.subgraphs).toEqual([])
    expect(workflow.nodes.filter((node) => node.type === 'subgraph')).toEqual([])
    expect(workflow.nodes.map((node) => node.id)).toEqual(
      expect.arrayContaining([
        'gsd_start',
        'project_ideation',
        'project_requirements',
        'project_roadmap',
        'new_milestone_intake',
        'milestone_intake',
        'seed_phase_1',
        'smart_discuss',
        'phase_plan',
        'phase_execute',
        'debug_phase',
        'phase_verify',
        'phase_review',
        'phase_fix',
        'write_verification_evidence',
        'mark_phase_complete',
        'audit_milestone',
        'archive_milestone',
      ]),
    )
    expect(workflow.edges.map((edge) => edge.id)).toEqual(
      expect.arrayContaining([
        'start_to_load',
        'milestone_missing',
        'ideation_to_requirements',
        'new_milestone_to_requirements',
        'requirements_to_roadmap',
        'roadmap_to_milestone',
        'phase_available',
        'gap_closure_to_execute',
        'debug_to_execute',
        'fix_back_to_review',
        'verification_record_to_complete',
        'archive_to_next_milestone',
      ]),
    )
    expect(
      workflow.nodes.some((node) => node.type === 'state_write' && node.operation.entityType === 'milestone_archive'),
    ).toBe(true)
    expect(workflow.edges).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'partial_run_done',
          toNodeId: 'partial_success',
          condition: expect.objectContaining({ kind: 'all' }),
        }),
        expect.objectContaining({
          id: 'next_milestone_start',
          toNodeId: 'new_milestone_intake',
        }),
      ]),
    )
  })

  it('keeps blank workflow drafts empty until the user adds a start node', () => {
    const draft = instantiateBlankWorkflow({
      projectId: 'project-1',
    })
    const report = validateWorkflowDefinition(
      draft,
    )

    expect(draft.nodes).toEqual([])
    expect(report.status).toBe('invalid')
    expect(report.diagnostics).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ code: 'schema_invalid', path: 'startNodeId' }),
        expect.objectContaining({ code: 'schema_invalid', path: 'nodes' }),
      ]),
    )
  })
})

function expectReadableTemplateNodeSpacing(nodes: readonly WorkflowNodeDto[], context: string) {
  const minSameLaneDelta = readableTemplateCardWidth + readableTemplateHorizontalGap
  const minSameColumnDelta = readableTemplateCardHeight + readableTemplateVerticalGap
  const nodesByLane = new Map<number, WorkflowNodeDto[]>()
  const nodesByColumn = new Map<number, WorkflowNodeDto[]>()

  for (const node of nodes) {
    nodesByLane.set(node.position.y, [...(nodesByLane.get(node.position.y) ?? []), node])
    nodesByColumn.set(node.position.x, [...(nodesByColumn.get(node.position.x) ?? []), node])
  }

  for (const laneNodes of nodesByLane.values()) {
    const sortedNodes = [...laneNodes].sort((left, right) => left.position.x - right.position.x)
    for (let index = 1; index < sortedNodes.length; index += 1) {
      const previous = sortedNodes[index - 1]
      const next = sortedNodes[index]
      expect(
        next.position.x - previous.position.x,
        `${context} should leave readable horizontal space between ${previous.id} and ${next.id}`,
      ).toBeGreaterThanOrEqual(minSameLaneDelta)
    }
  }

  for (const columnNodes of nodesByColumn.values()) {
    const sortedNodes = [...columnNodes].sort((left, right) => left.position.y - right.position.y)
    for (let index = 1; index < sortedNodes.length; index += 1) {
      const previous = sortedNodes[index - 1]
      const next = sortedNodes[index]
      expect(
        next.position.y - previous.position.y,
        `${context} should leave readable vertical space between ${previous.id} and ${next.id}`,
      ).toBeGreaterThanOrEqual(minSameColumnDelta)
    }
  }
}

function expectUnreachableWorkflowNodes(
  definition: Pick<WorkflowDefinitionDto, 'nodes' | 'edges' | 'startNodeId'>,
  context: string,
): string[] {
  const nodeIds = new Set(definition.nodes.map((node) => node.id))
  const outgoingEdges = new Map<string, string[]>()
  for (const node of definition.nodes) {
    outgoingEdges.set(node.id, [])
  }
  for (const edge of definition.edges) {
    outgoingEdges.get(edge.fromNodeId)?.push(edge.toNodeId)
    const exhaustionTarget = edge.loopPolicy?.onExhausted
    if (exhaustionTarget && exhaustionTarget !== edge.toNodeId) {
      outgoingEdges.get(edge.fromNodeId)?.push(exhaustionTarget)
    }
  }

  const reachedNodeIds = new Set<string>()
  const queue = [definition.startNodeId]
  while (queue.length > 0) {
    const nodeId = queue.shift()
    if (!nodeId || reachedNodeIds.has(nodeId) || !nodeIds.has(nodeId)) continue
    reachedNodeIds.add(nodeId)
    for (const nextNodeId of outgoingEdges.get(nodeId) ?? []) {
      if (!reachedNodeIds.has(nextNodeId)) {
        queue.push(nextNodeId)
      }
    }
  }

  return definition.nodes
    .map((node) => node.id)
    .filter((nodeId) => !reachedNodeIds.has(nodeId))
    .map((nodeId) => `${context}:${nodeId}`)
}
