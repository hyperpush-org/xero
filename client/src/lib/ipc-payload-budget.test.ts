import { describe, expect, it, vi } from 'vitest'

import {
  IPC_PAYLOAD_BUDGETS,
  estimateIpcPayloadBytes,
  getIpcPayloadBudgetMetrics,
  recordIpcPayloadSample,
  resetIpcPayloadBudgetMetricsForTests,
} from './ipc-payload-budget'

describe('IPC payload budgets', () => {
  it('estimates JSON payload size and records largest samples by budget', () => {
    resetIpcPayloadBudgetMetricsForTests()

    const sample = recordIpcPayloadSample({
      boundary: 'command',
      name: 'get_repository_diff',
      payload: { patch: '+hello\n', truncated: false },
    })

    expect(sample?.budget.key).toBe('repositoryDiff')
    expect(sample?.observedBytes).toBeGreaterThan(0)

    const metrics = getIpcPayloadBudgetMetrics()
    expect(metrics).toHaveLength(1)
    expect(metrics[0]).toMatchObject({
      budgetKey: 'repositoryDiff',
      largestName: 'get_repository_diff',
      sampleCount: 1,
    })
  })

  it('marks over-budget runtime stream items as dropped metrics', () => {
    resetIpcPayloadBudgetMetricsForTests()

    const sample = recordIpcPayloadSample({
      boundary: 'channel',
      budgetKey: 'runtimeStreamItem',
      name: 'subscribe_runtime_stream:item',
      payload: { text: 'x'.repeat(IPC_PAYLOAD_BUDGETS.runtimeStreamItem.maxBytes) },
    })

    expect(sample?.overMaxBudget).toBe(true)
    expect(getIpcPayloadBudgetMetrics()[0]).toMatchObject({
      budgetKey: 'runtimeStreamItem',
      droppedCount: 1,
      overBudgetCount: 1,
    })
  })

  it('uses Rust-provided payload budget metadata instead of stringifying large command responses', () => {
    resetIpcPayloadBudgetMetricsForTests()
    const stringify = vi.spyOn(JSON, 'stringify')

    const sample = recordIpcPayloadSample({
      boundary: 'command',
      name: 'get_repository_diff',
      payload: {
        patch: '+'.repeat(100_000),
        payloadBudget: {
          key: 'repository_diff',
          budgetBytes: 96 * 1024,
          observedBytes: 123_456,
          truncated: true,
          dropped: false,
          message: 'truncated',
        },
      },
    })
    const stringifyCalls = stringify.mock.calls.length
    stringify.mockRestore()

    expect(sample?.observedBytes).toBe(123_456)
    expect(stringifyCalls).toBe(0)
  })

  it('keeps representative normal command DTOs under their warning budgets', () => {
    const repositoryStatus = {
      repository: {
        id: 'repo-1',
        projectId: 'project-1',
        rootPath: '/repo',
        displayName: 'repo',
        branch: 'main',
        headSha: 'abc1234',
        isGitRepo: true,
      },
      branch: null,
      lastCommit: null,
      entries: Array.from({ length: 400 }, (_, index) => ({
        path: `src/file-${index}.ts`,
        staged: null,
        unstaged: 'modified',
        untracked: false,
      })),
      hasStagedChanges: false,
      hasUnstagedChanges: true,
      hasUntrackedChanges: false,
      additions: 400,
      deletions: 0,
    }
    const searchResults = {
      projectId: 'project-1',
      totalMatches: 250,
      totalFiles: 25,
      truncated: false,
      files: Array.from({ length: 25 }, (_, fileIndex) => ({
        path: `/src/file-${fileIndex}.ts`,
        matches: Array.from({ length: 10 }, (_, matchIndex) => ({
          line: matchIndex + 1,
          column: 3,
          previewPrefix: 'const value = ',
          previewMatch: 'target',
          previewSuffix: ' + 1',
        })),
      })),
    }

    expect(estimateIpcPayloadBytes(repositoryStatus)).toBeLessThan(IPC_PAYLOAD_BUDGETS.repositoryStatus.warnBytes)
    expect(estimateIpcPayloadBytes(searchResults)).toBeLessThan(IPC_PAYLOAD_BUDGETS.projectSearchResults.warnBytes)
  })
})
