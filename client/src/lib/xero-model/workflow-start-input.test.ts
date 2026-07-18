import { describe, expect, it } from 'vitest'

import type { WorkflowDefinitionDto, WorkflowSubgraphDto } from './workflow-definition'
import {
  buildWorkflowInitialInput,
  getWorkflowStartInputPlan,
  workflowStartInputKey,
  workflowStartInputPath,
} from './workflow-start-input'
import { instantiateWorkflowTemplate } from './workflow-templates'

describe('Workflow start input', () => {
  it('discovers labeled inputs across the top-level graph and declared subgraphs', () => {
    const definition = workflowWithInputsAcrossScopes()

    expect(getWorkflowStartInputPlan(definition)).toEqual({
      fields: [
        {
          key: 'goal',
          name: 'goal',
          path: '$.goal',
          label: 'Goal',
          required: true,
        },
        {
          key: 'ticket_id',
          name: 'ticket',
          path: '$.ticket_id',
          label: 'Ticket ID',
          required: true,
        },
        {
          key: 'audience',
          name: 'audience',
          path: '$.audience',
          label: 'Audience',
          required: false,
        },
        {
          key: 'notes',
          name: 'notes',
          path: '$.notes',
          label: 'Release notes',
          required: false,
        },
      ],
      requiredFields: [
        {
          key: 'goal',
          name: 'goal',
          path: '$.goal',
          label: 'Goal',
          required: true,
        },
        {
          key: 'ticket_id',
          name: 'ticket',
          path: '$.ticket_id',
          label: 'Ticket ID',
          required: true,
        },
      ],
      hasRequiredInput: true,
    })
  })

  it('builds trimmed initial input using explicit simple paths and default names', () => {
    const plan = getWorkflowStartInputPlan(workflowWithInputsAcrossScopes())

    expect(
      buildWorkflowInitialInput(plan.fields, {
        goal: '  Ship the release  ',
        ticket_id: '  XERO-42 ',
        audience: ' ',
        notes: '  Announce after verification. ',
      }),
    ).toEqual({
      goal: 'Ship the release',
      ticket_id: 'XERO-42',
      notes: 'Announce after verification.',
    })
    expect(workflowStartInputPath('summary', null)).toBe('$.summary')
    expect(workflowStartInputKey('ticket', '$.ticket_id')).toBe('ticket_id')
    expect(workflowStartInputKey('payload', '$')).toBe('$')
  })

  it('builds nested objects and arrays for configured JSON input paths', () => {
    const fields = [
      {
        key: '$.config.goal',
        name: 'goal',
        path: '$.config.goal',
        label: 'Goal',
        required: true,
      },
      {
        key: '$.reviewers[0].name',
        name: 'reviewer',
        path: '$.reviewers[0].name',
        label: 'Reviewer',
        required: false,
      },
    ]

    expect(
      buildWorkflowInitialInput(fields, {
        '$.config.goal': '  Ship nested input ',
        '$.reviewers[0].name': ' Ada ',
      }),
    ).toEqual({
      config: { goal: 'Ship nested input' },
      reviewers: [{ name: 'Ada' }],
    })
    expect(workflowStartInputKey('goal', '$.config.goal')).toBe('$.config.goal')
  })

  it('rejects prototype-polluting input paths', () => {
    expect(() =>
      buildWorkflowInitialInput(
        [
          {
            key: '$.__proto__.polluted',
            name: 'unsafe',
            path: '$.__proto__.polluted',
            label: 'Unsafe',
            required: true,
          },
        ],
        { '$.__proto__.polluted': 'yes' },
      ),
    ).toThrow('is invalid')
    expect(({} as Record<string, unknown>).polluted).toBeUndefined()
  })

  it('rejects input paths with unsafe sparse-array indexes', () => {
    expect(() =>
      buildWorkflowInitialInput(
        [
          {
            key: '$.items[4294967294]',
            name: 'item',
            path: '$.items[4294967294]',
            label: 'Item',
            required: true,
          },
        ],
        { '$.items[4294967294]': 'unsafe' },
      ),
    ).toThrow('is invalid')
  })

  it('reports when a Workflow has no required start input', () => {
    const definition = instantiateWorkflowTemplate({
      projectId: 'project-1',
      templateId: 'release_train',
    })

    const plan = getWorkflowStartInputPlan(definition)

    expect(plan.hasRequiredInput).toBe(false)
    expect(plan.requiredFields).toEqual([])
  })
})

function workflowWithInputsAcrossScopes(): WorkflowDefinitionDto {
  const definition = instantiateWorkflowTemplate({
    projectId: 'project-1',
    templateId: 'continuous_delivery',
  })
  const sourceAgent = definition.nodes.find((node) => node.type === 'agent')
  if (!sourceAgent || sourceAgent.type !== 'agent') {
    throw new Error('Expected the template to contain an Agent node.')
  }

  const nodes = definition.nodes.map((node) => {
    if (node.type !== 'agent') return node

    const inputBindings = node.inputBindings.map((binding) =>
      binding.source === 'run_input' && binding.name === 'goal'
        ? { ...binding, required: true }
        : binding,
    )
    if (node.id !== sourceAgent.id) return { ...node, inputBindings }

    return {
      ...node,
      inputBindings: [
        ...inputBindings,
        {
          source: 'run_input' as const,
          name: 'notes',
          required: false,
          promptLabel: 'Release notes',
        },
      ],
    }
  })
  const subgraph: WorkflowSubgraphDto = {
    id: 'release_details',
    title: 'Release details',
    description: '',
    startNodeId: 'release_audience',
    nodes: [
      {
        ...sourceAgent,
        id: 'release_audience',
        title: 'Release audience',
        inputBindings: [
          {
            source: 'run_input',
            name: 'audience',
            required: false,
            path: '$.audience',
            promptLabel: 'Audience',
          },
          {
            source: 'run_input',
            name: 'goal',
            required: false,
            path: '$.goal',
            promptLabel: 'Alternate goal label',
          },
        ],
      },
    ],
    edges: [],
    inputBindings: [
      {
        source: 'run_input',
        name: 'ticket',
        required: true,
        path: '$.ticket_id',
        promptLabel: 'Ticket ID',
      },
    ],
    outputContract: {
      artifactType: 'release_details',
      schemaVersion: 1,
      extraction: 'json_object',
      required: true,
    },
  }

  return {
    ...definition,
    nodes,
    subgraphs: [subgraph],
  }
}
