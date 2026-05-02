import { describe, expect, it, vi } from 'vitest'
import {
  createXeroHighChurnStore,
  selectRepositoryShellStatus,
  selectRepositoryStatus,
  shallowEqualObject,
} from './high-churn-store'
import {
  createRepositoryStatusDiffRevision,
  type RepositoryStatusView,
} from '@/src/lib/xero-model'

function makeStatus(overrides: Partial<RepositoryStatusView> = {}): RepositoryStatusView {
  const { diffRevision, ...statusOverrides } = overrides
  const status = {
    projectId: 'project-1',
    repositoryId: 'repo-project-1',
    branchLabel: 'main',
    headShaLabel: 'abc1234',
    upstream: null,
    lastCommit: null,
    stagedCount: 0,
    unstagedCount: 1,
    untrackedCount: 0,
    statusCount: 1,
    additions: 2,
    deletions: 1,
    hasChanges: true,
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: null,
        unstaged: 'modified',
        untracked: false,
      },
    ],
    ...statusOverrides,
  }

  return {
    ...status,
    diffRevision: diffRevision ?? createRepositoryStatusDiffRevision(status),
  }
}

describe('high churn selector store', () => {
  it('notifies repository shell subscribers only when shell-visible fields change', () => {
    const store = createXeroHighChurnStore()
    const shellListener = vi.fn()
    const fullStatusListener = vi.fn()

    store.subscribeSelector(selectRepositoryShellStatus, shallowEqualObject, shellListener)
    store.subscribeSelector(selectRepositoryStatus, Object.is, fullStatusListener)

    store.setRepositoryStatus(makeStatus())

    expect(shellListener).toHaveBeenCalledTimes(1)
    expect(fullStatusListener).toHaveBeenCalledTimes(1)

    store.setRepositoryStatus(
      makeStatus({
        entries: [
          {
            path: 'client/src/App.tsx',
            staged: 'modified',
            unstaged: null,
            untracked: false,
          },
        ],
      }),
    )

    expect(shellListener).toHaveBeenCalledTimes(1)
    expect(fullStatusListener).toHaveBeenCalledTimes(2)

    store.setRepositoryStatus(makeStatus({ statusCount: 2, additions: 5 }))

    expect(shellListener).toHaveBeenCalledTimes(2)
    expect(fullStatusListener).toHaveBeenCalledTimes(3)
  })
})
