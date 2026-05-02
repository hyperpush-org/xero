import {
  createRepositoryStatusEntriesRevision,
  createRepositoryStatusDiffRevision,
  type RepositoryStatusView,
} from '@/src/lib/xero-model'

export { createRepositoryStatusDiffRevision }

export function createRepositoryStatusSyncKey(status: RepositoryStatusView | null): string {
  if (!status) {
    return 'none'
  }

  return JSON.stringify({
    projectId: status.projectId,
    repositoryId: status.repositoryId,
    branchLabel: status.branchLabel,
    headShaLabel: status.headShaLabel,
    upstream: status.upstream ?? null,
    lastCommit: status.lastCommit,
    diffRevision: status.diffRevision,
    stagedCount: status.stagedCount,
    unstagedCount: status.unstagedCount,
    untrackedCount: status.untrackedCount,
    statusCount: status.statusCount,
    additions: status.additions,
    deletions: status.deletions,
    hasChanges: status.hasChanges,
    entriesRevision: createRepositoryStatusEntriesRevision(status.entries),
  })
}
