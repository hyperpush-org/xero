import { describe, expect, it } from 'vitest'
import {
  runtimeProtocolEventSchema,
  runtimeSubmissionEnvelopeSchema,
} from './runtime-protocol'

const submissionFixture = {
  protocolVersion: 3,
  submissionId: 'submission-1',
  trace: {
    traceId: 'd4f540327c3af2abc99f37979596ec6d',
    spanId: 'ae5365d98d6c430c',
    runTraceId: 'd4f540327c3af2abc99f37979596ec6d',
  },
  submittedAt: '2026-05-03T12:00:00Z',
  submission: {
    kind: 'start_run',
    payload: {
      projectId: 'project-1',
      agentSessionId: 'session-1',
      runId: 'run-1',
      prompt: 'Implement phase 2.',
      provider: {
        providerId: 'fake_provider',
        modelId: 'fake-model',
      },
      controls: {
        runtimeAgentId: 'engineer',
        approvalMode: 'suggest',
        planModeRequired: true,
      },
    },
  },
}

const eventFixture = {
  protocolVersion: 3,
  eventId: 7,
  projectId: 'project-1',
  runId: 'run-1',
  eventKind: 'context_manifest_recorded',
  trace: {
    traceId: 'd4f540327c3af2abc99f37979596ec6d',
    spanId: '97904fd6111fa2d4',
    parentSpanId: 'e24d46853cb84fa3',
    runTraceId: 'd4f540327c3af2abc99f37979596ec6d',
    storageWriteTraceId: '17fdcd179cdb2ddab6d76a9810aaa850',
  },
  payload: {
    kind: 'context_manifest_recorded',
    payload: {
      manifestId: 'manifest-1',
      contextHash: 'abc123',
      turnIndex: 0,
    },
  },
  occurredAt: '2026-05-03T12:00:01Z',
}

const environmentEventFixture = {
  ...eventFixture,
  eventId: 8,
  eventKind: 'environment_lifecycle_update',
  payload: {
    kind: 'environment_lifecycle_update',
    payload: {
      environmentId: 'env-project-1-run-1',
      state: 'ready',
      previousState: 'starting_conversation',
      sandboxGroupingPolicy: 'none',
      pendingMessageCount: 0,
      healthChecks: [
        {
          kind: 'filesystem_accessible',
          status: 'passed',
          summary: 'Workspace filesystem is accessible.',
          checkedAt: 'unix:1',
        },
      ],
      setupSteps: [
        {
          stepId: 'setup_scripts',
          label: 'No setup scripts configured',
          state: 'skipped',
        },
      ],
      detail: 'Environment is ready.',
    },
  },
}

describe('runtime protocol schemas', () => {
  it('accepts the Rust submission schema snapshot fixture', () => {
    const parsed = runtimeSubmissionEnvelopeSchema.parse(submissionFixture)

    expect(parsed.submission.kind).toBe('start_run')
    expect(parsed.protocolVersion).toBe(3)
  })

  it('accepts the Rust event schema snapshot fixture', () => {
    const parsed = runtimeProtocolEventSchema.parse(eventFixture)

    expect(parsed.eventKind).toBe('context_manifest_recorded')
    expect(parsed.payload.kind).toBe('context_manifest_recorded')
  })

  it('accepts environment lifecycle update events', () => {
    const parsed = runtimeProtocolEventSchema.parse(environmentEventFixture)

    expect(parsed.eventKind).toBe('environment_lifecycle_update')
    expect(parsed.payload.kind).toBe('environment_lifecycle_update')
  })

  it('requires runtime settings change submissions to carry object-shaped settings', () => {
    const parsed = runtimeSubmissionEnvelopeSchema.parse({
      ...submissionFixture,
      submission: {
        kind: 'runtime_settings_change',
        payload: {
          projectId: 'project-1',
          settings: {
            providerProfileId: 'openrouter',
          },
          reason: 'Switch runtime profile.',
        },
      },
    })
    expect(parsed.submission.kind).toBe('runtime_settings_change')

    const result = runtimeSubmissionEnvelopeSchema.safeParse({
      ...submissionFixture,
      submission: {
        kind: 'runtime_settings_change',
        payload: {
          projectId: 'project-1',
          settings: 'openrouter',
        },
      },
    })
    expect(result.success).toBe(false)
  })

  it('rejects protocol version mismatches at the frontend boundary', () => {
    const result = runtimeSubmissionEnvelopeSchema.safeParse({
      ...submissionFixture,
      protocolVersion: 4,
    })

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues[0]?.path).toEqual(['protocolVersion'])
    }
  })

  it('rejects trace contexts where runTraceId drifts from traceId', () => {
    const result = runtimeProtocolEventSchema.safeParse({
      ...eventFixture,
      trace: {
        ...eventFixture.trace,
        runTraceId: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      },
    })

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues[0]?.path).toEqual(['trace', 'runTraceId'])
    }
  })

  it('rejects runtime events whose outer and inner kinds drift', () => {
    const result = runtimeProtocolEventSchema.safeParse({
      ...eventFixture,
      payload: {
        kind: 'run_completed',
        payload: {
          summary: 'Done.',
          state: 'completed',
        },
      },
    })

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.issues[0]?.path).toEqual(['payload', 'kind'])
    }
  })
})
