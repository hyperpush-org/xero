import { describe, expect, it } from 'vitest'

import { instantiateBlankWorkflow } from './workflow-templates'
import {
  getWorkflowRunRequestSchema,
  resumeWorkflowNextIncompletePhaseRequestSchema,
  startWorkflowRunRequestSchema,
  updateWorkflowDefinitionRequestSchema,
} from './workflow-run'

describe('Workflow mutation request schemas', () => {
  it('requires an exact expected version for definition updates', () => {
    const definition = instantiateBlankWorkflow({
      projectId: 'project-1',
      name: 'Workflow',
    })
    definition.startNodeId = 'done'
    definition.nodes = [
      {
        id: 'done',
        type: 'terminal',
        title: 'Done',
        description: '',
        position: { x: 0, y: 0 },
        terminalStatus: 'success',
      },
    ]

    const valid = updateWorkflowDefinitionRequestSchema.safeParse({
      workflowId: definition.id,
      expectedVersion: definition.version,
      definition,
    })
    expect(valid.success, valid.error?.message).toBe(true)
    expect(
      updateWorkflowDefinitionRequestSchema.safeParse({
        workflowId: definition.id,
        expectedVersion: definition.version + 1,
        definition,
      }).success,
    ).toBe(false)
    expect(
      updateWorkflowDefinitionRequestSchema.safeParse({
        workflowId: definition.id,
        definition,
      }).success,
    ).toBe(false)
  })

  it('requires a bounded explicit idempotency key for Workflow starts', () => {
    const request = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      idempotencyKey: 'workflow-start-request-1',
      initialInput: { goal: 'ship' },
    }

    expect(startWorkflowRunRequestSchema.parse(request)).toEqual(request)
    expect(
      startWorkflowRunRequestSchema.safeParse({
        ...request,
        idempotencyKey: '',
      }).success,
    ).toBe(false)
    expect(
      startWorkflowRunRequestSchema.safeParse({
        ...request,
        idempotencyKey: 'x'.repeat(201),
      }).success,
    ).toBe(false)
    expect(
      startWorkflowRunRequestSchema.safeParse({
        projectId: request.projectId,
        workflowId: request.workflowId,
      }).success,
    ).toBe(false)
  })

  it('requires idempotency only for phase resume, not ordinary run reads', () => {
    expect(
      getWorkflowRunRequestSchema.parse({ projectId: 'project-1', runId: 'run-1' }),
    ).toEqual({ projectId: 'project-1', runId: 'run-1' })
    expect(
      resumeWorkflowNextIncompletePhaseRequestSchema.safeParse({
        projectId: 'project-1',
        runId: 'run-1',
      }).success,
    ).toBe(false)
    expect(
      resumeWorkflowNextIncompletePhaseRequestSchema.parse({
        projectId: 'project-1',
        runId: 'run-1',
        idempotencyKey: 'resume-1',
      }),
    ).toEqual({
      projectId: 'project-1',
      runId: 'run-1',
      idempotencyKey: 'resume-1',
    })
  })
})
