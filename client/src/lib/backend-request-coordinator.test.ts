import { describe, expect, it, vi } from 'vitest'

import {
  StaleBackendRequestError,
  backendRequestKey,
  createBackendRequestCoordinator,
  searchProjectRequestKey,
} from './backend-request-coordinator'

describe('backend request coordinator', () => {
  it('deduplicates identical in-flight request keys', async () => {
    const coordinator = createBackendRequestCoordinator()
    let calls = 0

    const first = coordinator.runDeduped('repository-diff:project-1:unstaged', async () => {
      calls += 1
      return { patch: '+one' }
    })
    const second = coordinator.runDeduped('repository-diff:project-1:unstaged', async () => {
      calls += 1
      return { patch: '+two' }
    })

    await expect(first).resolves.toEqual({ patch: '+one' })
    await expect(second).resolves.toEqual({ patch: '+one' })
    expect(calls).toBe(1)
  })

  it('rejects older latest-wins responses for the same scope', async () => {
    const coordinator = createBackendRequestCoordinator()
    let resolveFirst: (value: string) => void = () => undefined

    const first = coordinator.runLatest(
      'visible-search',
      'search:one',
      () =>
        new Promise<string>((resolve) => {
          resolveFirst = resolve
        }),
    )
    const second = coordinator.runLatest('visible-search', 'search:two', async () => 'second')

    await expect(second).resolves.toBe('second')
    resolveFirst('first')
    await expect(first).rejects.toBeInstanceOf(StaleBackendRequestError)
  })

  it('uses explicit search keys independent of property order', () => {
    expect(
      searchProjectRequestKey({ projectId: 'project-1', query: 'needle', regex: false }),
    ).toBe(
      searchProjectRequestKey({ regex: false, query: 'needle', projectId: 'project-1' }),
    )
  })

  it('does not stringify large command arguments to build dedupe keys', () => {
    const stringify = vi.spyOn(JSON, 'stringify')
    const key = backendRequestKey('search_project', {
      request: {
        projectId: 'project-1',
        query: 'needle',
        includeGlobs: Array.from({ length: 500 }, (_, index) => `src/${index}.ts`),
        excludeGlobs: [],
        maxFiles: 100,
      },
    })
    const stringifyCalls = stringify.mock.calls.length
    stringify.mockRestore()

    expect(key).toContain('search_project')
    expect(stringifyCalls).toBe(0)
  })

  it('requires an explicit key builder for deduped commands', () => {
    expect(() => backendRequestKey('unknown_command', { request: { projectId: 'project-1' } })).toThrow(
      /No explicit backend request key builder/,
    )
  })
})
