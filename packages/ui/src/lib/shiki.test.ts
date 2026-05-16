import { describe, expect, it } from 'vitest'

import {
  createShikiTokenCache,
  createShikiTokenCacheKey,
  shouldSkipTokenization,
  type TokenizedLine,
} from './shiki'

function tokens(text: string): TokenizedLine[] {
  return [[{ content: text, offset: 0, color: '#fff', fontStyle: 0 }]]
}

describe('shiki token cache', () => {
  it('keys token entries by language, theme, and content hash', () => {
    const base = createShikiTokenCacheKey('const value = 1', 'ts', 'github-dark')

    expect(createShikiTokenCacheKey('const value = 1', 'tsx', 'github-dark')).not.toBe(base)
    expect(createShikiTokenCacheKey('const value = 1', 'ts', 'github-light')).not.toBe(base)
    expect(createShikiTokenCacheKey('const value = 2', 'ts', 'github-dark')).not.toBe(base)
    expect(createShikiTokenCacheKey('const value = 1', 'ts', 'github-dark')).toBe(base)
  })

  it('evicts least-recently-used entries by entry budget', () => {
    const cache = createShikiTokenCache({ maxBytes: 10_000, maxEntries: 2 })

    cache.set('first', 'ts', 'github-dark', tokens('first'))
    cache.set('second', 'ts', 'github-dark', tokens('second'))
    expect(cache.get('first', 'ts', 'github-dark')).not.toBeNull()

    cache.set('third', 'ts', 'github-dark', tokens('third'))

    expect(cache.get('second', 'ts', 'github-dark')).toBeNull()
    expect(cache.get('first', 'ts', 'github-dark')).not.toBeNull()
    expect(cache.get('third', 'ts', 'github-dark')).not.toBeNull()
    expect(cache.getStats().evictions).toBe(1)
  })

  it('enforces the byte budget without keeping oversized token arrays', () => {
    const cache = createShikiTokenCache({ maxBytes: 96, maxEntries: 10 })

    cache.set('small', 'ts', 'github-dark', tokens('ok'))
    cache.set('x'.repeat(200), 'ts', 'github-dark', tokens('x'.repeat(200)))

    expect(cache.get('small', 'ts', 'github-dark')).not.toBeNull()
    expect(cache.get('x'.repeat(200), 'ts', 'github-dark')).toBeNull()
    expect(cache.getStats().skippedByBudget).toBe(1)
  })

  it('invalidates only entries for the requested theme', () => {
    const cache = createShikiTokenCache({ maxBytes: 10_000, maxEntries: 10 })

    cache.set('dark code', 'ts', 'github-dark', tokens('dark'))
    cache.set('light code', 'ts', 'github-light', tokens('light'))

    expect(cache.invalidateTheme('github-dark')).toBe(1)
    expect(cache.get('dark code', 'ts', 'github-dark')).toBeNull()
    expect(cache.get('light code', 'ts', 'github-light')).not.toBeNull()
  })

  it('skips tokenization requests over the configured content budget', () => {
    expect(shouldSkipTokenization('abcd', 8)).toBe(false)
    expect(shouldSkipTokenization('abcde', 8)).toBe(true)
  })
})
