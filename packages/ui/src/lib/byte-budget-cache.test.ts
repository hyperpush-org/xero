import { describe, expect, it } from 'vitest'

import {
  createByteBudgetCache,
  estimateJsonBytes,
  estimateUtf16Bytes,
} from './byte-budget-cache'

describe('byte budget cache', () => {
  it('evicts least-recently-used entries by entry count', () => {
    const cache = createByteBudgetCache<string, string>({ maxBytes: 10_000, maxEntries: 2 })

    cache.set('first', 'first-value', 100)
    cache.set('second', 'second-value', 100)
    expect(cache.get('first')).toBe('first-value')

    cache.set('third', 'third-value', 100)

    expect(cache.get('second')).toBeNull()
    expect(cache.get('first')).toBe('first-value')
    expect(cache.get('third')).toBe('third-value')
    expect(cache.getStats()).toMatchObject({
      entries: 2,
      evictions: 1,
    })
  })

  it('evicts oldest entries until retained bytes fit the budget', () => {
    const cache = createByteBudgetCache<string, string>({ maxBytes: 250, maxEntries: 10 })

    cache.set('first', 'a', 100)
    cache.set('second', 'b', 100)
    cache.set('third', 'c', 100)

    expect(cache.get('first')).toBeNull()
    expect(cache.get('second')).toBe('b')
    expect(cache.get('third')).toBe('c')
    expect(cache.getStats()).toMatchObject({
      byteSize: 200,
      entries: 2,
      evictions: 1,
    })
  })

  it('skips entries larger than the whole cache budget', () => {
    const cache = createByteBudgetCache<string, string>({ maxBytes: 64, maxEntries: 10 })

    expect(cache.set('oversized', 'value', 128)).toBe(false)

    expect(cache.get('oversized')).toBeNull()
    expect(cache.getStats()).toMatchObject({
      entries: 0,
      skippedByBudget: 1,
    })
  })

  it('estimates UTF-16 and JSON payload bytes for cache accounting', () => {
    expect(estimateUtf16Bytes('abcd')).toBe(8)
    expect(estimateJsonBytes({ label: 'abcd' })).toBeGreaterThan(8)
  })
})
