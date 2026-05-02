import { describe, expect, it } from 'vitest'
import type { RepositoryStatusView } from '@/src/lib/xero-model'
import {
  createRepositoryStatusDiffRevision,
  createRepositoryStatusSyncKey,
} from './repository-status'

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

describe('repository status revisions', () => {
  it('keeps the diff revision stable when only shell badge totals change', () => {
    const revision = createRepositoryStatusDiffRevision(makeStatus())

    expect(
      createRepositoryStatusDiffRevision(
        makeStatus({
          statusCount: 4,
          additions: 20,
          deletions: 8,
        }),
      ),
    ).toBe(revision)
  })

  it('changes the diff revision when diff-relevant repository state changes', () => {
    const revision = createRepositoryStatusDiffRevision(makeStatus())

    expect(createRepositoryStatusDiffRevision(makeStatus({ headShaLabel: 'def5678' }))).not.toBe(revision)
    expect(
      createRepositoryStatusDiffRevision(
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
      ),
    ).not.toBe(revision)
  })

  it('still lets the full sync key observe shell-visible count updates', () => {
    const status = makeStatus()

    expect(createRepositoryStatusSyncKey(makeStatus({ additions: status.additions + 1 }))).not.toBe(
      createRepositoryStatusSyncKey(status),
    )
  })
})
