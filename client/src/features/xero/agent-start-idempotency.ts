import {
  ContinuationIdempotencyCoordinator,
  type ContinuationIdempotencyOptions,
} from './continuation-idempotency'

export interface AgentStartIdentity {
  projectId: string
  agentSessionId: string
  prompt: string
  controls: unknown
  attachments: unknown
}

/**
 * Persists a caller-owned run id before IPC dispatch. If the renderer or app exits after the
 * backend commits but before the response arrives, the next instance reuses the same run id and
 * lets the backend replay the original start instead of creating a duplicate run.
 */
export class AgentStartIdempotencyCoordinator {
  private readonly coordinator: ContinuationIdempotencyCoordinator

  constructor(createRunId: () => string, options: ContinuationIdempotencyOptions) {
    this.coordinator = new ContinuationIdempotencyCoordinator(createRunId, {
      ...options,
      storageKey: options.storageKey ?? 'xero.pending-agent-start-identities.v1',
    })
  }

  run<T>(identity: AgentStartIdentity, operation: (runId: string) => Promise<T>): Promise<T> {
    return this.coordinator.run(
      {
        channel: 'agent-start',
        targetId: `${identity.projectId}\0${identity.agentSessionId}`,
        payload: {
          prompt: identity.prompt,
          controls: identity.controls,
          attachments: identity.attachments,
        },
      },
      operation,
    )
  }
}
