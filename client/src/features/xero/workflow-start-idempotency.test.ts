import { describe, expect, it, vi } from 'vitest'

import {
  WorkflowStartIdempotencyCoordinator,
  type WorkflowStartIdempotencyStorage,
} from './workflow-start-idempotency'

function memoryStorage(
  entries: Iterable<readonly [string, string]> = [],
): WorkflowStartIdempotencyStorage {
  const values = new Map<string, string>(entries)
  return {
    get length() {
      return values.size
    },
    key: (index) => [...values.keys()][index] ?? null,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => void values.delete(key),
  }
}

describe('WorkflowStartIdempotencyCoordinator', () => {
  it('reuses an ambiguous start key and rotates it after a definitive success', async () => {
    const createKey = vi.fn().mockReturnValueOnce('workflow-start-1').mockReturnValueOnce('workflow-start-2')
    const startWorkflowRun = vi
      .fn<(idempotencyKey: string) => Promise<{ runId: string }>>()
      .mockRejectedValueOnce(new Error('IPC response channel closed'))
      .mockResolvedValueOnce({ runId: 'run-1' })
      .mockResolvedValueOnce({ runId: 'run-2' })
    const coordinator = new WorkflowStartIdempotencyCoordinator(createKey, memoryStorage())
    const identity = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      initialInput: { options: { second: 2, first: 1 }, goal: 'ship' },
    }

    await expect(coordinator.run(identity, startWorkflowRun)).rejects.toThrow(
      'IPC response channel closed',
    )
    await expect(
      coordinator.run(
        {
          ...identity,
          initialInput: { goal: 'ship', options: { first: 1, second: 2 } },
        },
        startWorkflowRun,
      ),
    ).resolves.toEqual({ runId: 'run-1' })
    await expect(coordinator.run(identity, startWorkflowRun)).resolves.toEqual({ runId: 'run-2' })

    expect(startWorkflowRun.mock.calls.map(([key]) => key)).toEqual([
      'workflow-start-1',
      'workflow-start-1',
      'workflow-start-2',
    ])
    expect(createKey).toHaveBeenCalledTimes(2)
  })

  it('rotates the key when the requested project, Workflow, or input changes', async () => {
    const createKey = vi
      .fn()
      .mockReturnValueOnce('workflow-start-1')
      .mockReturnValueOnce('workflow-start-2')
    const startWorkflowRun = vi.fn<(idempotencyKey: string) => Promise<void>>().mockRejectedValue(
      new Error('ambiguous transport failure'),
    )
    const coordinator = new WorkflowStartIdempotencyCoordinator(createKey, memoryStorage())

    await expect(
      coordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: { goal: 'first' } },
        startWorkflowRun,
      ),
    ).rejects.toThrow('ambiguous transport failure')
    await expect(
      coordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: { goal: 'second' } },
        startWorkflowRun,
      ),
    ).rejects.toThrow('ambiguous transport failure')

    expect(startWorkflowRun.mock.calls.map(([key]) => key)).toEqual([
      'workflow-start-1',
      'workflow-start-2',
    ])
  })

  it('rotates after an explicitly definitive adapter rejection', async () => {
    const createKey = vi
      .fn()
      .mockReturnValueOnce('workflow-start-1')
      .mockReturnValueOnce('workflow-start-2')
    const definitiveError = Object.assign(new Error('Fix the input before retrying.'), {
      errorClass: 'user_fixable',
      retryable: false,
    })
    const startWorkflowRun = vi
      .fn<(idempotencyKey: string) => Promise<void>>()
      .mockRejectedValueOnce(definitiveError)
      .mockResolvedValueOnce(undefined)
    const coordinator = new WorkflowStartIdempotencyCoordinator(createKey, memoryStorage())
    const identity = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      initialInput: null,
    }

    await expect(coordinator.run(identity, startWorkflowRun)).rejects.toBe(definitiveError)
    await expect(coordinator.run(identity, startWorkflowRun)).resolves.toBeUndefined()

    expect(startWorkflowRun.mock.calls.map(([key]) => key)).toEqual([
      'workflow-start-1',
      'workflow-start-2',
    ])
  })

  it('retains independent ambiguous keys for interleaved Workflow starts', async () => {
    const createKey = vi.fn().mockReturnValueOnce('workflow-start-a').mockReturnValueOnce('workflow-start-b')
    const startWorkflowRun = vi
      .fn<(idempotencyKey: string) => Promise<void>>()
      .mockRejectedValue(new Error('transport response lost'))
    const coordinator = new WorkflowStartIdempotencyCoordinator(createKey, memoryStorage())
    const workflowA = {
      projectId: 'project-1',
      workflowId: 'workflow-a',
      initialInput: { goal: 'ship a', options: { second: 2, first: 1 } },
    }
    const workflowB = {
      projectId: 'project-1',
      workflowId: 'workflow-b',
      initialInput: { goal: 'ship b' },
    }

    await expect(coordinator.run(workflowA, startWorkflowRun)).rejects.toThrow(
      'transport response lost',
    )
    await expect(coordinator.run(workflowB, startWorkflowRun)).rejects.toThrow(
      'transport response lost',
    )
    await expect(
      coordinator.run(
        {
          ...workflowA,
          initialInput: { options: { first: 1, second: 2 }, goal: 'ship a' },
        },
        startWorkflowRun,
      ),
    ).rejects.toThrow('transport response lost')
    await expect(coordinator.run(workflowB, startWorkflowRun)).rejects.toThrow(
      'transport response lost',
    )

    expect(startWorkflowRun.mock.calls.map(([key]) => key)).toEqual([
      'workflow-start-a',
      'workflow-start-b',
      'workflow-start-a',
      'workflow-start-b',
    ])
    expect(createKey).toHaveBeenCalledTimes(2)
  })

  it('reuses the durable key after an app restart loses the committed IPC response', async () => {
    const storage = memoryStorage()
    const createKey = vi
      .fn()
      .mockReturnValueOnce('workflow-start-before-restart')
      .mockReturnValueOnce('workflow-start-after-success')
    const identity = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      initialInput: { goal: 'survive restart' },
    }
    const firstProcess = new WorkflowStartIdempotencyCoordinator(createKey, storage)
    await expect(
      firstProcess.run(identity, async () => {
        throw new Error('app exited after backend commit')
      }),
    ).rejects.toThrow('app exited after backend commit')

    const restartedProcess = new WorkflowStartIdempotencyCoordinator(createKey, storage)
    const replay = vi.fn().mockResolvedValue({ runId: 'committed-run' })
    await expect(restartedProcess.run(identity, replay)).resolves.toEqual({
      runId: 'committed-run',
    })
    expect(replay).toHaveBeenCalledWith('workflow-start-before-restart')

    const nextStart = vi.fn().mockResolvedValue({ runId: 'new-run' })
    await restartedProcess.run(identity, nextStart)
    expect(nextStart).toHaveBeenCalledWith('workflow-start-after-success')
  })

  it('drops a key that another renderer has definitively cleared', async () => {
    const storage = memoryStorage()
    const identity = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      initialInput: { goal: 'coordinate renderers' },
    }
    const first = new WorkflowStartIdempotencyCoordinator(
      vi.fn().mockReturnValueOnce('workflow-start-shared').mockReturnValueOnce('workflow-start-new'),
      storage,
    )
    await expect(
      first.run(identity, async () => {
        throw new Error('response lost')
      }),
    ).rejects.toThrow('response lost')

    const second = new WorkflowStartIdempotencyCoordinator(
      () => 'must-not-create',
      storage,
    )
    const replay = vi.fn().mockResolvedValue({ runId: 'committed-run' })
    await second.run(identity, replay)
    expect(replay).toHaveBeenCalledWith('workflow-start-shared')

    const nextStart = vi.fn().mockResolvedValue({ runId: 'new-run' })
    await first.run(identity, nextStart)
    expect(nextStart).toHaveBeenCalledWith('workflow-start-new')
  })

  it('preserves another renderer pending start while clearing a completed start', async () => {
    const storage = memoryStorage()
    const workflowA = {
      projectId: 'project-1',
      workflowId: 'workflow-a',
      initialInput: null,
    }
    const workflowB = {
      projectId: 'project-1',
      workflowId: 'workflow-b',
      initialInput: null,
    }
    const first = new WorkflowStartIdempotencyCoordinator(() => 'workflow-start-a', storage)
    const second = new WorkflowStartIdempotencyCoordinator(() => 'workflow-start-b', storage)
    let finishFirst: ((value: { runId: string }) => void) | undefined
    const firstStart = first.run(
      workflowA,
      () => new Promise<{ runId: string }>((resolve) => {
        finishFirst = resolve
      }),
    )
    await expect(
      second.run(workflowB, async () => {
        throw new Error('response lost for b')
      }),
    ).rejects.toThrow('response lost for b')

    finishFirst?.({ runId: 'run-a' })
    await expect(firstStart).resolves.toEqual({ runId: 'run-a' })

    const replayB = vi.fn().mockResolvedValue({ runId: 'run-b' })
    await second.run(workflowB, replayB)
    expect(replayB).toHaveBeenCalledWith('workflow-start-b')
  })

  it('converges concurrent renderers on one durable key for the same start', async () => {
    const storage = memoryStorage()
    const identity = {
      projectId: 'project-1',
      workflowId: 'workflow-1',
      initialInput: { goal: 'same start' },
    }
    const firstCreateKey = vi.fn(() => 'workflow-start-first')
    const secondCreateKey = vi.fn(() => 'workflow-start-second')
    const first = new WorkflowStartIdempotencyCoordinator(firstCreateKey, storage)
    const second = new WorkflowStartIdempotencyCoordinator(secondCreateKey, storage)
    const observedKeys: string[] = []
    const ambiguousStart = async (idempotencyKey: string) => {
      observedKeys.push(idempotencyKey)
      throw new Error('response lost')
    }

    const outcomes = await Promise.allSettled([
      first.run(identity, ambiguousStart),
      second.run(identity, ambiguousStart),
    ])

    expect(outcomes.map((outcome) => outcome.status)).toEqual(['rejected', 'rejected'])
    expect(observedKeys).toEqual(['workflow-start-first', 'workflow-start-first'])
    expect(firstCreateKey).toHaveBeenCalledTimes(1)
    expect(secondCreateKey).not.toHaveBeenCalled()
  })

  it('refuses to evict an unresolved durable key when the pending cap is full', async () => {
    const storage = memoryStorage()
    let sequence = 0
    const createKey = () => `workflow-start-${sequence++}`
    const coordinator = new WorkflowStartIdempotencyCoordinator(createKey, storage)
    const ambiguous = vi.fn().mockRejectedValue(new Error('response lost'))

    for (let index = 0; index < 256; index += 1) {
      await expect(
        coordinator.run(
          {
            projectId: 'project-1',
            workflowId: `workflow-${index}`,
            initialInput: null,
          },
          ambiguous,
        ),
      ).rejects.toThrow('response lost')
    }
    const neverCalled = vi.fn()
    await expect(
      coordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-over-cap', initialInput: null },
        neverCalled,
      ),
    ).rejects.toThrow('too many unresolved Workflow start requests')
    expect(neverCalled).not.toHaveBeenCalled()

    const restarted = new WorkflowStartIdempotencyCoordinator(createKey, storage)
    const replay = vi.fn().mockResolvedValue({ runId: 'run-0' })
    await restarted.run(
      { projectId: 'project-1', workflowId: 'workflow-0', initialInput: null },
      replay,
    )
    expect(replay).toHaveBeenCalledWith('workflow-start-0')
  })

  it('fails closed when durable pending state is corrupt or unreadable', async () => {
    const corrupt: WorkflowStartIdempotencyStorage = {
      getItem: () => '{not-json',
      setItem: vi.fn(),
      removeItem: vi.fn(),
    }
    const operation = vi.fn()
    const corruptCoordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-new',
      corrupt,
    )
    await expect(
      corruptCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        operation,
      ),
    ).rejects.toThrow('durable request store is unreadable')
    expect(operation).not.toHaveBeenCalled()

    const unreadable: WorkflowStartIdempotencyStorage = {
      getItem: () => {
        throw new Error('storage denied')
      },
      setItem: vi.fn(),
      removeItem: vi.fn(),
    }
    const unreadableCoordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-new',
      unreadable,
    )
    await expect(
      unreadableCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        operation,
      ),
    ).rejects.toThrow('storage denied')
    expect(operation).not.toHaveBeenCalled()
  })

  it('fails closed when durable pending state exceeds the supported cap', async () => {
    const oversized = memoryStorage(
      Array.from({ length: 257 }, (_, index) => {
        const fingerprint = `workflow-${index}`
        return [
          `xero.workflow-start.pending.v2.${fingerprint}`,
          JSON.stringify({
            version: 2,
            fingerprint,
            idempotencyKey: `workflow-start-${index}`,
          }),
        ] as const
      }),
    )
    const operation = vi.fn()
    const coordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-new',
      oversized,
    )

    await expect(
      coordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        operation,
      ),
    ).rejects.toThrow('durable request store is unreadable')
    expect(operation).not.toHaveBeenCalled()
  })

  it('does not issue IPC when persistence fails and retains a key when clearing fails', async () => {
    const writeFailure: WorkflowStartIdempotencyStorage = {
      getItem: () => null,
      setItem: () => {
        throw new Error('disk full')
      },
      removeItem: vi.fn(),
    }
    const operation = vi.fn()
    const writeCoordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-write',
      writeFailure,
    )
    await expect(
      writeCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        operation,
      ),
    ).rejects.toThrow('disk full')
    expect(operation).not.toHaveBeenCalled()

    const silentWriteFailure: WorkflowStartIdempotencyStorage = {
      getItem: () => null,
      setItem: vi.fn(),
      removeItem: vi.fn(),
    }
    const silentOperation = vi.fn()
    const silentWriteCoordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-silent-write',
      silentWriteFailure,
    )
    await expect(
      silentWriteCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        silentOperation,
      ),
    ).rejects.toThrow('could not verify the persisted Workflow request identity')
    expect(silentOperation).not.toHaveBeenCalled()

    const values = new Map<string, string>()
    const removeFailure: WorkflowStartIdempotencyStorage = {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
      removeItem: () => {
        throw new Error('remove failed')
      },
    }
    const clearCoordinator = new WorkflowStartIdempotencyCoordinator(
      () => 'workflow-start-clear',
      removeFailure,
    )
    const committed = vi.fn().mockResolvedValue({ runId: 'run-1' })
    await expect(
      clearCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        committed,
      ),
    ).rejects.toThrow('remove failed')
    await expect(
      clearCoordinator.run(
        { projectId: 'project-1', workflowId: 'workflow-1', initialInput: null },
        committed,
      ),
    ).rejects.toThrow('remove failed')
    expect(committed.mock.calls.map(([key]) => key)).toEqual([
      'workflow-start-clear',
      'workflow-start-clear',
    ])
  })
})
