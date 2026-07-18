import { describe, expect, it } from 'vitest'

import gsdAutoDefinitionFixture from '../../../test-fixtures/workflows/gsd_auto.definition.json'

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

  it('enforces Xero\'s bounded git-status policy instead of self-authorized command lists', () => {
    const workflow = linearWorkflow()
    const commandNode: Extract<WorkflowNodeDto, { type: 'command' }> = {
      id: 'agent-a',
      type: 'command',
      title: 'Repository status',
      description: '',
      position: { x: 0, y: 0 },
      command: 'git',
      args: ['status', '--short'],
      allowedCommands: ['git'],
      workingDirectory: null,
      timeoutSeconds: 30,
      successExitCodes: [0],
      outputContract: {
        artifactType: 'text_output',
        schemaVersion: 1,
        extraction: 'generic_text',
        required: true,
      },
      parser: { extraction: 'generic_text' },
    }
    workflow.nodes[0] = commandNode

    expect(validateWorkflowDefinition(workflow)).toEqual({ status: 'valid', diagnostics: [] })

    workflow.nodes[0] = {
      ...commandNode,
      command: 'pnpm',
      args: ['test'],
      allowedCommands: ['pnpm'],
    }
    expect(validateWorkflowDefinition(workflow).diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'workflow_command_not_allowed_by_app_policy',
        path: 'nodes.0.command',
      }),
    ]))

    workflow.nodes[0] = { ...commandNode, args: ['push'] }
    expect(validateWorkflowDefinition(workflow).diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'workflow_command_arguments_not_allowed_by_app_policy',
        path: 'nodes.0.args',
      }),
    ]))

    workflow.nodes[0] = { ...commandNode, args: ['status', '--', '../outside'] }
    expect(validateWorkflowDefinition(workflow).diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'workflow_command_arguments_not_allowed_by_app_policy',
        path: 'nodes.0.args',
      }),
    ]))
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

  it('rejects a conditional route without an explicit always fallback', () => {
    const workflow = linearWorkflow()
    workflow.edges[0] = {
      ...workflow.edges[0],
      condition: { kind: 'node_status', nodeId: 'agent-a', status: 'succeeded' },
    }

    const report = validateWorkflowDefinition(workflow)

    expect(report.status).toBe('invalid')
    expect(report.diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'conditional_route_fallback_missing',
        path: 'nodes.agent-a',
      }),
    ]))
  })

  it('rejects Needs Human terminals inside subgraphs', () => {
    const workflow = linearWorkflow()
    workflow.subgraphs = [{
      id: 'review',
      title: 'Review',
      description: '',
      startNodeId: 'needs-human',
      nodes: [{
        id: 'needs-human',
        type: 'terminal',
        title: 'Needs human',
        description: '',
        position: { x: 0, y: 0 },
        terminalStatus: 'needs_human',
      }],
      edges: [],
      inputBindings: [],
      outputContract: {
        artifactType: 'subgraph_result',
        schemaVersion: 1,
        extraction: 'generic_text',
        required: true,
      },
    }]

    const report = validateWorkflowDefinition(workflow)

    expect(report.status).toBe('invalid')
    expect(report.diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'subgraph_needs_human_terminal_unsupported',
        path: 'subgraphs.0.nodes.0.terminalStatus',
      }),
    ]))
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
    workflow.edges.push({
      id: 'edge-b-human-fallback',
      fromNodeId: 'agent-b',
      toNodeId: 'human',
      type: 'conditional',
      label: 'fallback',
      priority: 90,
      condition: { kind: 'always' },
    })

    const report = validateWorkflowDefinition(workflow)
    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'valid' })
  })

  it('applies full node, edge, reference, state, command, and loop validation inside subgraphs', () => {
    const workflow = linearWorkflow()
    const source = workflow.nodes[0]
    const sink = workflow.nodes[1]
    if (source.type !== 'agent' || sink.type !== 'agent') throw new Error('expected agent nodes')
    workflow.subgraphs = [{
      id: 'local-flow',
      title: 'Local flow',
      description: '',
      startNodeId: 'source',
      nodes: [
        { ...source, id: 'source', title: 'Source' },
        {
          ...sink,
          id: 'sink',
          title: 'Sink',
          inputBindings: [{
            source: 'artifact',
            name: 'missing',
            required: true,
            artifactRef: 'ghost.text_output',
          }],
        },
        {
          id: 'state',
          type: 'state_read',
          title: 'Read state',
          description: '',
          position: { x: 0, y: 0 },
          query: {
            entityType: 'milestone',
            filters: [{ path: 'invalid', operator: 'eq', value: 'open', values: [] }],
            includeArchived: false,
          },
          outputArtifactType: 'state_result',
        },
        {
          id: 'command',
          type: 'command',
          title: 'Command',
          description: '',
          position: { x: 0, y: 0 },
          command: 'pnpm',
          args: [],
          allowedCommands: ['npm'],
          timeoutSeconds: 30,
          successExitCodes: [0],
          outputContract: {
            artifactType: 'command_result',
            schemaVersion: 1,
            extraction: 'generic_text',
            required: true,
          },
          parser: { extraction: 'generic_text' },
        },
      ],
      edges: [
        {
          id: 'source-to-sink',
          fromNodeId: 'source',
          toNodeId: 'sink',
          type: 'success',
          label: '',
          priority: 10,
          condition: { kind: 'node_status', nodeId: 'ghost', status: 'succeeded' },
        },
        {
          id: 'sink-to-source',
          fromNodeId: 'sink',
          toNodeId: 'source',
          type: 'loop',
          label: 'retry',
          priority: 20,
          condition: { kind: 'loop_attempt_lt', loopKey: 'missing-loop', value: 2 },
          loopPolicy: {
            loopKey: 'local-loop',
            maxAttempts: 2,
            attemptScope: 'run',
            carryoverPolicy: 'selected',
            selectedArtifactRefs: ['ghost.text_output'],
            resetPolicy: 'never',
            onExhausted: 'ghost',
          },
        },
      ],
      inputBindings: [],
      outputContract: {
        artifactType: 'subgraph_result',
        schemaVersion: 1,
        extraction: 'generic_text',
        required: true,
      },
    }]

    const report = validateWorkflowDefinition(workflow)
    const diagnostics = new Map(report.diagnostics.map((diagnostic) => [diagnostic.code, diagnostic.path]))

    expect(report.status).toBe('invalid')
    expect(diagnostics.get('artifact_ref_missing')).toBe('subgraphs.0.nodes.1.inputBindings.0.artifactRef')
    expect(diagnostics.get('state_query_filter_path_invalid')).toBe('subgraphs.0.nodes.2.query.filters.0.path')
    expect(diagnostics.get('command_not_in_allowlist')).toBe('subgraphs.0.nodes.3.allowedCommands')
    expect(diagnostics.get('condition_node_ref_missing')).toBe('subgraphs.0.edges.0.condition')
    expect(diagnostics.get('condition_loop_key_missing')).toBe('subgraphs.0.edges.1.condition')
    expect(diagnostics.get('loop_exhaustion_target_missing')).toBe('subgraphs.0.edges.1.loopPolicy.onExhausted')
    expect(diagnostics.get('loop_artifact_ref_missing')).toBe('subgraphs.0.edges.1.loopPolicy.selectedArtifactRefs.0')
  })

  it('validates nested subgraph invocations and rejects recursive invocation cycles', () => {
    const workflow = linearWorkflow()
    const terminal = {
      id: 'local-done',
      type: 'terminal' as const,
      title: 'Done',
      description: '',
      position: { x: 0, y: 0 },
      terminalStatus: 'success' as const,
    }
    const genericSubgraphOutput = {
      artifactType: 'subgraph_result',
      schemaVersion: 1,
      extraction: 'generic_text' as const,
      required: true,
    }
    workflow.subgraphs = [
      {
        id: 'inner',
        title: 'Inner',
        description: '',
        startNodeId: terminal.id,
        nodes: [terminal],
        edges: [],
        inputBindings: [],
        outputContract: genericSubgraphOutput,
      },
      {
        id: 'outer',
        title: 'Outer',
        description: '',
        startNodeId: 'invoke-inner',
        nodes: [
          {
            id: 'invoke-inner',
            type: 'subgraph',
            title: 'Invoke inner',
            description: '',
            position: { x: 0, y: 0 },
            subgraphId: 'inner',
            inputBindings: [],
            outputContract: genericSubgraphOutput,
          },
          { ...terminal, id: 'outer-done' },
        ],
        edges: [{
          id: 'inner-to-done',
          fromNodeId: 'invoke-inner',
          toNodeId: 'outer-done',
          type: 'success',
          label: '',
          priority: 10,
          condition: { kind: 'always' },
        }],
        inputBindings: [],
        outputContract: genericSubgraphOutput,
      },
    ]

    expect(validateWorkflowDefinition(workflow)).toEqual({ status: 'valid', diagnostics: [] })

    const outerNode = workflow.subgraphs[1].nodes[0]
    if (outerNode.type !== 'subgraph') throw new Error('expected subgraph node')
    outerNode.subgraphId = 'outer'
    const report = validateWorkflowDefinition(workflow)

    expect(report.status).toBe('invalid')
    expect(report.diagnostics).toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: 'recursive_subgraph_invocation',
        path: 'subgraphs.1.nodes.0.subgraphId',
      }),
    ]))
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
    workflow.edges.push({
      id: 'edge-a-safe-fallback',
      fromNodeId: 'agent-a',
      toNodeId: 'done',
      type: 'success',
      label: 'fallback',
      priority: 90,
      condition: { kind: 'always' },
    })

    const report = validateWorkflowDefinition(workflow)
    expect(report, JSON.stringify(report.diagnostics)).toMatchObject({ status: 'valid' })
  })

  it('rejects two default else edges from the same node', () => {
    const workflow = linearWorkflow()
    workflow.edges.push({
      id: 'edge-b-other-default',
      fromNodeId: 'agent-b',
      toNodeId: 'done',
      type: 'success',
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

  it('uses backend-compatible built-in versions before the agent catalog loads', () => {
    const workflow = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })
    const versions = new Map(
      workflow.nodes.flatMap((node) =>
        node.type === 'agent' && node.agentRef.kind === 'built_in'
          ? [[node.agentRef.runtimeAgentId, node.agentRef.version] as const]
          : [],
      ),
    )

    expect(Object.fromEntries(versions)).toMatchObject({
      plan: 2,
      engineer: 2,
      debug: 2,
      generalist: 1,
    })
  })

  it('ships review stages as read-only Ask agents and mutation stages with auto-edit approval', () => {
    const continuous = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'continuous_delivery',
    })
    const gsd = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })
    const bugTriage = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'bug_triage_fix_loop',
    })
    const agent = (workflow: WorkflowDefinitionDto, nodeId: string) => {
      const node = workflow.nodes.find((candidate) => candidate.id === nodeId)
      expect(node?.type).toBe('agent')
      if (node?.type !== 'agent') {
        throw new Error(`Expected ${nodeId} to be an agent node`)
      }
      return node
    }

    for (const [workflow, nodeId] of [
      [continuous, 'check'],
      [continuous, 'review'],
      [gsd, 'phase_verify'],
      [gsd, 'phase_review'],
    ] as const) {
      expect(agent(workflow, nodeId).agentRef).toMatchObject({
        kind: 'built_in',
        runtimeAgentId: 'ask',
      })
      expect(agent(workflow, nodeId).runOverrides).toBeNull()
    }

    for (const [workflow, nodeId] of [
      [continuous, 'work'],
      [continuous, 'debug'],
      [continuous, 'fix'],
      [gsd, 'phase_execute'],
      [gsd, 'debug_phase'],
      [gsd, 'phase_fix'],
      [bugTriage, 'fix_bug'],
    ] as const) {
      expect(agent(workflow, nodeId).runOverrides?.approvalMode).toBe('auto_edit')
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

  it('keeps the cross-runtime GSD Auto fixture identical to the shipped template', () => {
    const workflow = instantiateWorkflowTemplate({
      projectId: 'project-fixture',
      templateId: 'gsd_auto',
    })
    workflow.id = 'workflow-gsd-auto-fixture'

    expect(workflow).toEqual(gsdAutoDefinitionFixture)
  })

  it('keeps command artifacts strict enough to preserve bounded output diagnostics', () => {
    const workflow = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'gsd_auto',
    })
    const commandResult = workflow.artifactContracts.find(
      (contract) => contract.artifactType === 'command_result',
    )

    expect(commandResult?.jsonSchema?.required).toEqual(
      expect.arrayContaining([
        'stdoutTruncated',
        'stderrTruncated',
        'stdoutDrainIncomplete',
        'stderrDrainIncomplete',
        'stdoutReadError',
        'stderrReadError',
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
