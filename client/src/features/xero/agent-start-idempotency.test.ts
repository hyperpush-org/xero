import { describe, expect, it, vi } from 'vitest'

import { AgentStartIdempotencyCoordinator } from './agent-start-idempotency'

describe('AgentStartIdempotencyCoordinator', () => {
  const memoryStorage = () => {
    const values = new Map<string, string>()
    return {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => void values.set(key, value),
      removeItem: (key: string) => void values.delete(key),
    }
  }

  it('survives a renderer restart after backend commit but before the IPC response', async () => {
    const values = new Map<string, string>()
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => void values.set(key, value),
      removeItem: (key: string) => void values.delete(key),
    }
    const identity = {
      projectId: 'project-1',
      agentSessionId: 'session-1',
      prompt: 'Run exactly once.',
      controls: { runtimeAgentId: 'engineer' },
      attachments: [],
    }
    const beforeRestart = new AgentStartIdempotencyCoordinator(
      () => 'agent-run-before-restart',
      { storage },
    )
    await expect(
      beforeRestart.run(identity, async () => {
        throw new Error('IPC response lost')
      }),
    ).rejects.toThrow('IPC response lost')

    const createAfterRestart = vi.fn().mockReturnValue('duplicate-agent-run')
    const afterRestart = new AgentStartIdempotencyCoordinator(createAfterRestart, { storage })
    const operation = vi.fn(async (runId: string) => runId)
    await expect(afterRestart.run(identity, operation)).resolves.toBe('agent-run-before-restart')

    expect(operation).toHaveBeenCalledWith('agent-run-before-restart')
    expect(createAfterRestart).not.toHaveBeenCalled()
  })

  it('rejects a definitive payload error and rotates the next run id', async () => {
    const ids = vi.fn().mockReturnValueOnce('run-1').mockReturnValueOnce('run-2')
    const coordinator = new AgentStartIdempotencyCoordinator(ids, { storage: memoryStorage() })
    const identity = {
      projectId: 'project-1',
      agentSessionId: 'session-1',
      prompt: 'Start.',
      controls: null,
      attachments: [],
    }
    const rejected = Object.assign(new Error('invalid'), {
      errorClass: 'user_fixable',
      retryable: false,
    })
    await expect(coordinator.run(identity, async () => Promise.reject(rejected))).rejects.toBe(
      rejected,
    )
    await expect(coordinator.run(identity, async (runId) => runId)).resolves.toBe('run-2')
  })
})
