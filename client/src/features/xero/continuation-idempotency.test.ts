import { describe, expect, it, vi } from 'vitest'

import { ContinuationIdempotencyCoordinator } from './continuation-idempotency'

describe('ContinuationIdempotencyCoordinator', () => {
  const memoryStorage = () => {
    const values = new Map<string, string>()
    return {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => void values.set(key, value),
      removeItem: (key: string) => void values.delete(key),
    }
  }

  it('reuses an id after an ambiguous response and rotates after success', async () => {
    const ids = vi.fn().mockReturnValueOnce('request-1').mockReturnValueOnce('request-2')
    const operation = vi
      .fn<(requestId: string) => Promise<string>>()
      .mockRejectedValueOnce(new Error('response channel closed'))
      .mockResolvedValueOnce('accepted')
      .mockResolvedValueOnce('accepted again')
    const coordinator = new ContinuationIdempotencyCoordinator(ids, { storage: memoryStorage() })
    const identity = {
      channel: 'runtime-control' as const,
      targetId: 'run-1',
      payload: { prompt: 'continue', controls: { model: 'm', effort: 'high' } },
    }

    await expect(coordinator.run(identity, operation)).rejects.toThrow('response channel closed')
    await expect(
      coordinator.run(
        {
          ...identity,
          payload: { controls: { effort: 'high', model: 'm' }, prompt: 'continue' },
        },
        operation,
      ),
    ).resolves.toBe('accepted')
    await expect(coordinator.run(identity, operation)).resolves.toBe('accepted again')

    expect(operation.mock.calls.map(([id]) => id)).toEqual(['request-1', 'request-1', 'request-2'])
  })

  it('rotates for a genuinely different submission and after a definitive rejection', async () => {
    const ids = vi
      .fn()
      .mockReturnValueOnce('request-1')
      .mockReturnValueOnce('request-2')
      .mockReturnValueOnce('request-3')
    const ambiguous = new Error('transport lost')
    const definitive = Object.assign(new Error('invalid prompt'), {
      errorClass: 'user_fixable',
      retryable: false,
    })
    const operation = vi
      .fn<(requestId: string) => Promise<void>>()
      .mockRejectedValueOnce(ambiguous)
      .mockRejectedValueOnce(definitive)
      .mockResolvedValueOnce(undefined)
    const coordinator = new ContinuationIdempotencyCoordinator(ids, { storage: memoryStorage() })

    await expect(
      coordinator.run({ channel: 'send', targetId: 'run-1', payload: { prompt: 'one' } }, operation),
    ).rejects.toBe(ambiguous)
    await expect(
      coordinator.run({ channel: 'send', targetId: 'run-1', payload: { prompt: 'two' } }, operation),
    ).rejects.toBe(definitive)
    await expect(
      coordinator.run({ channel: 'send', targetId: 'run-1', payload: { prompt: 'two' } }, operation),
    ).resolves.toBeUndefined()

    expect(operation.mock.calls.map(([id]) => id)).toEqual(['request-1', 'request-2', 'request-3'])
  })

  it('retains independent ambiguous ids for interleaved runs', async () => {
    const ids = vi.fn().mockReturnValueOnce('request-a').mockReturnValueOnce('request-b')
    const operation = vi.fn<(requestId: string) => Promise<void>>().mockRejectedValue(
      new Error('transport response lost'),
    )
    const coordinator = new ContinuationIdempotencyCoordinator(ids, { storage: memoryStorage() })
    const runA = {
      channel: 'send' as const,
      targetId: 'run-a',
      payload: { prompt: 'continue a', values: [undefined, () => undefined, Symbol('x')] },
    }
    const runB = {
      channel: 'send' as const,
      targetId: 'run-b',
      payload: { prompt: 'continue b' },
    }

    await expect(coordinator.run(runA, operation)).rejects.toThrow('transport response lost')
    await expect(coordinator.run(runB, operation)).rejects.toThrow('transport response lost')
    await expect(
      coordinator.run(
        {
          ...runA,
          payload: { prompt: 'continue a', values: [null, null, null] },
        },
        operation,
      ),
    ).rejects.toThrow('transport response lost')
    await expect(coordinator.run(runB, operation)).rejects.toThrow('transport response lost')

    expect(operation.mock.calls.map(([id]) => id)).toEqual([
      'request-a',
      'request-b',
      'request-a',
      'request-b',
    ])
    expect(ids).toHaveBeenCalledTimes(2)
  })

  it('serializes storage mutation across coordinators while another request clears', async () => {
    const storage = memoryStorage()
    const first = new ContinuationIdempotencyCoordinator(() => 'request-a', { storage })
    const second = new ContinuationIdempotencyCoordinator(() => 'request-b', { storage })
    const identityA = {
      channel: 'send' as const,
      targetId: 'run-a',
      payload: { prompt: 'a' },
    }
    const identityB = {
      channel: 'send' as const,
      targetId: 'run-b',
      payload: { prompt: 'b' },
    }
    let finishA: ((value: string) => void) | undefined
    const pendingA = first.run(
      identityA,
      () =>
        new Promise<string>((resolve) => {
          finishA = resolve
        }),
    )

    await expect(
      second.run(identityB, async () => {
        throw new Error('response lost for b')
      }),
    ).rejects.toThrow('response lost for b')
    finishA?.('accepted')
    await expect(pendingA).resolves.toBe('accepted')

    const replayB = vi.fn(async (requestId: string) => requestId)
    await expect(second.run(identityB, replayB)).resolves.toBe('request-b')
    expect(replayB).toHaveBeenCalledWith('request-b')
  })

  it('reuses the durable id after a renderer restart loses the committed response', async () => {
    const storage = memoryStorage()
    const createFirstId = vi.fn().mockReturnValue('request-before-restart')
    const identity = {
      channel: 'send' as const,
      targetId: 'run-restart',
      payload: { prompt: 'continue exactly once' },
    }
    const first = new ContinuationIdempotencyCoordinator(createFirstId, { storage })

    await expect(
      first.run(identity, async () => {
        throw new Error('renderer exited after backend commit')
      }),
    ).rejects.toThrow('renderer exited')

    const createAfterRestart = vi.fn().mockReturnValue('duplicate-request')
    const afterRestart = new ContinuationIdempotencyCoordinator(createAfterRestart, { storage })
    const operation = vi.fn(async (requestId: string) => requestId)
    await expect(afterRestart.run(identity, operation)).resolves.toBe('request-before-restart')

    expect(operation).toHaveBeenCalledWith('request-before-restart')
    expect(createAfterRestart).not.toHaveBeenCalled()
  })

  it('does not dispatch when persisting a new identity fails', async () => {
    const storage = memoryStorage()
    storage.setItem = () => {
      throw new Error('disk full')
    }
    const operation = vi.fn()
    const coordinator = new ContinuationIdempotencyCoordinator(() => 'request-1', { storage })

    await expect(
      coordinator.run(
        { channel: 'send', targetId: 'run-1', payload: { prompt: 'continue' } },
        operation,
      ),
    ).rejects.toThrow('disk full')
    expect(operation).not.toHaveBeenCalled()
  })

  it('defers unreadable durable storage failure until dispatch and then fails closed', async () => {
    const storage = {
      getItem: () => {
        throw new Error('storage denied')
      },
      setItem: vi.fn(),
      removeItem: vi.fn(),
    }
    const operation = vi.fn()
    const coordinator = new ContinuationIdempotencyCoordinator(() => 'request-1', { storage })

    await expect(
      coordinator.run(
        { channel: 'send', targetId: 'run-1', payload: { prompt: 'continue' } },
        operation,
      ),
    ).rejects.toThrow('storage denied')
    expect(operation).not.toHaveBeenCalled()
  })

  it('restores a delivered identity in memory when clearing durable storage fails', async () => {
    const storage = memoryStorage()
    const coordinator = new ContinuationIdempotencyCoordinator(() => 'request-1', { storage })
    const identity = { channel: 'send' as const, targetId: 'run-1', payload: { prompt: 'go' } }
    await expect(
      coordinator.run(identity, async () => {
        storage.removeItem = () => {
          throw new Error('storage unavailable')
        }
        return 'committed'
      }),
    ).rejects.toThrow('storage unavailable')

    storage.removeItem = (key) => void key
    const replay = vi.fn(async (requestId: string) => requestId)
    await expect(coordinator.run(identity, replay)).resolves.toBe('request-1')
    expect(replay).toHaveBeenCalledWith('request-1')
  })

  it('keeps all unresolved identities and refuses to dispatch past the durable cap', async () => {
    const storage = memoryStorage()
    let sequence = 0
    const coordinator = new ContinuationIdempotencyCoordinator(
      () => `request-${sequence++}`,
      { storage },
    )
    const ambiguous = vi.fn().mockRejectedValue(new Error('response lost'))
    for (let index = 0; index < 256; index += 1) {
      await expect(
        coordinator.run(
          { channel: 'send', targetId: `run-${index}`, payload: { prompt: 'continue' } },
          ambiguous,
        ),
      ).rejects.toThrow('response lost')
    }
    const neverCalled = vi.fn()
    await expect(
      coordinator.run(
        { channel: 'send', targetId: 'run-over-cap', payload: { prompt: 'continue' } },
        neverCalled,
      ),
    ).rejects.toThrow('too many unresolved operations')
    expect(neverCalled).not.toHaveBeenCalled()
  })
})
