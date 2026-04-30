import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { VcsSidebar, type VcsSidebarProps } from './vcs-sidebar'
import type {
  RepositoryDiffResponseDto,
  RepositoryStatusView,
} from '@/src/lib/xero-model/project'

const repository = {
  id: 'repo-project-1',
  projectId: 'project-1',
  rootPath: '/tmp/project-1',
  displayName: 'Project 1',
  branch: 'main',
  headSha: 'abc1234',
  isGitRepo: true,
}

function makeStatus(overrides: Partial<RepositoryStatusView> = {}): RepositoryStatusView {
  return {
    projectId: 'project-1',
    repositoryId: repository.id,
    branchLabel: 'main',
    headShaLabel: 'abc1234',
    lastCommit: null,
    stagedCount: 0,
    unstagedCount: 1,
    untrackedCount: 0,
    statusCount: 1,
    additions: 1,
    deletions: 1,
    hasChanges: true,
    entries: [
      {
        path: 'file.txt',
        staged: null,
        unstaged: 'modified',
        untracked: false,
      },
    ],
    ...overrides,
  }
}

function makeDiff(patch: string): RepositoryDiffResponseDto {
  return {
    repository,
    scope: 'unstaged',
    patch,
    truncated: false,
    baseRevision: null,
  }
}

function renderVcsSidebar(
  patch: string,
  options: { status?: RepositoryStatusView } = {},
) {
  const onLoadDiff = vi.fn(async () => makeDiff(patch))
  const props: VcsSidebarProps = {
    open: true,
    projectId: 'project-1',
    status: options.status ?? makeStatus(),
    branchLabel: 'main',
    onClose: vi.fn(),
    onRefreshStatus: vi.fn(),
    onLoadDiff,
    onStage: vi.fn(async () => undefined),
    onUnstage: vi.fn(async () => undefined),
    onDiscard: vi.fn(async () => undefined),
    onCommit: vi.fn(async () => ({
      sha: 'def5678',
      summary: 'Commit summary',
      signature: { name: 'Test User', email: 'test@example.com' },
    })),
    onFetch: vi.fn(async () => ({ remote: 'origin', refspecs: [] })),
    onPull: vi.fn(async () => ({
      remote: 'origin',
      branch: 'main',
      updated: false,
      summary: 'Already up to date.',
      newHeadSha: null,
    })),
    onPush: vi.fn(async () => ({ remote: 'origin', branch: 'main', updates: [] })),
  }

  return { ...render(<VcsSidebar {...props} />), onLoadDiff }
}

describe('VcsSidebar', () => {
  it('highlights removed and added diff rows with red and green backgrounds', async () => {
    renderVcsSidebar(
      [
        'diff --git a/file.txt b/file.txt',
        'index 100644..100755 100644',
        '--- a/file.txt',
        '+++ b/file.txt',
        '@@ -1,3 +1,3 @@',
        ' context line',
        '-removed line',
        '+added line',
      ].join('\n'),
    )

    await waitFor(() => expect(screen.getByText('removed line')).toBeInTheDocument())

    expect(screen.getByText('removed line').closest('div')).toHaveClass('bg-red-950/70')
    expect(screen.getByText('added line').closest('div')).toHaveClass('bg-green-950/70')
  })

  it('does not render the diff pane when there are no changes to display', () => {
    const cleanStatus = makeStatus({
      stagedCount: 0,
      unstagedCount: 0,
      untrackedCount: 0,
      statusCount: 0,
      additions: 0,
      deletions: 0,
      hasChanges: false,
      entries: [],
    })

    const { onLoadDiff } = renderVcsSidebar('', { status: cleanStatus })

    expect(screen.getByLabelText('Source control panel')).toHaveStyle({ width: '300px' })
    expect(screen.queryByLabelText('Resize source control sidebar')).not.toBeInTheDocument()
    expect(screen.queryByText('Select a file')).not.toBeInTheDocument()
    expect(onLoadDiff).not.toHaveBeenCalled()
  })
})
