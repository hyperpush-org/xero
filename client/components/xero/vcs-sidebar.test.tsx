import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import {
  DIFF_PARSE_CACHE_MAX_BYTES,
  DIFF_PATCH_CACHE_MAX_BYTES,
  DIFF_LINE_HIGHLIGHT_BYTE_LIMIT,
  DIFF_TOKENIZATION_BATCH_SIZE,
  createDiffPatchCache,
  createDiffTokenizationBatches,
  deriveVcsDiffScope,
  getDiffPatchCacheStats,
  getDiffParsingStats,
  parseDiffLines,
  parseDiffLinesForPatchKey,
  resetDiffPerformanceStatsForTests,
  setCachedDiffPatch,
  VcsSidebar,
  type VcsSidebarProps,
} from './vcs-sidebar'
import {
  createRepositoryStatusDiffRevision,
  type RepositoryDiffResponseDto,
  type RepositoryStatusView,
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
  const { diffRevision, ...statusOverrides } = overrides
  const status: Omit<RepositoryStatusView, 'diffRevision'> = {
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
    ...statusOverrides,
  }

  return {
    ...status,
    diffRevision: diffRevision ?? createRepositoryStatusDiffRevision(status),
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

function makeSingleFilePatch(line: string): string {
  return [
    'diff --git a/file.txt b/file.txt',
    '--- a/file.txt',
    '+++ b/file.txt',
    '@@ -1 +1 @@',
    `+${line}`,
  ].join('\n')
}

function renderVcsSidebar(
  patch: string,
  options: {
    open?: boolean
    status?: RepositoryStatusView
    onLoadDiff?: VcsSidebarProps['onLoadDiff']
    onGenerateCommitMessage?: VcsSidebarProps['onGenerateCommitMessage']
  } = {},
) {
  const onLoadDiff = options.onLoadDiff ?? vi.fn(async () => makeDiff(patch))
  const props: VcsSidebarProps = {
    open: options.open ?? true,
    projectId: 'project-1',
    status: options.status ?? makeStatus(),
    branchLabel: 'main',
    onClose: vi.fn(),
    onRefreshStatus: vi.fn(),
    onLoadDiff,
    commitMessageModel: {
      providerProfileId: 'openai-api-default',
      modelId: 'gpt-5.4',
      thinkingEffort: 'medium',
      label: 'gpt-5.4',
    },
    onGenerateCommitMessage: options.onGenerateCommitMessage ?? vi.fn(async () => ({
      message: 'feat: update project file',
      providerId: 'openai_api',
      modelId: 'gpt-5.4',
      diffTruncated: false,
    })),
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

  return { ...render(<VcsSidebar {...props} />), onLoadDiff, props }
}

describe('VcsSidebar', () => {
  it('derives the selected diff scope from staged and unstaged file state', () => {
    expect(deriveVcsDiffScope({ staged: 'modified', unstaged: null, untracked: false })).toBe('staged')
    expect(deriveVcsDiffScope({ staged: 'modified', unstaged: 'modified', untracked: false })).toBe('worktree')
    expect(deriveVcsDiffScope({ staged: null, unstaged: 'modified', untracked: false })).toBe('unstaged')
    expect(deriveVcsDiffScope({ staged: null, unstaged: null, untracked: true })).toBe('unstaged')
    expect(deriveVcsDiffScope(null)).toBeNull()
  })

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

    expect(screen.getByText('removed line').closest('div')).toHaveClass('bg-destructive/70')
    expect(screen.getByText('added line').closest('div')).toHaveClass('bg-success/70')
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

  it('moves onscreen immediately when opened after mounting closed', () => {
    const { props, rerender } = renderVcsSidebar('', { open: false })
    const panel = screen.getByLabelText('Source control panel')

    expect(panel).toHaveClass('invisible')
    expect(panel.style.transform).not.toBe('translate3d(0, 0, 0)')

    rerender(<VcsSidebar {...props} open />)

    expect(panel).not.toHaveClass('invisible')
    expect(panel.style.transform).toBe('translate3d(0, 0, 0)')
  })

  it('keeps the hidden panel unpainted when closed status changes add a diff pane', () => {
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
    const dirtyStatus = makeStatus()

    const { rerender } = renderVcsSidebar('', {
      open: false,
      status: cleanStatus,
    })

    expect(screen.getByLabelText('Source control panel')).toHaveClass('invisible')

    rerender(
      <VcsSidebar
        open={false}
        projectId="project-1"
        status={dirtyStatus}
        branchLabel="main"
        onRefreshStatus={vi.fn()}
        onLoadDiff={vi.fn(async () => makeDiff(''))}
        onStage={vi.fn(async () => undefined)}
        onUnstage={vi.fn(async () => undefined)}
        onDiscard={vi.fn(async () => undefined)}
        onCommit={vi.fn(async () => ({
          sha: 'def5678',
          summary: 'Commit summary',
          signature: { name: 'Test User', email: 'test@example.com' },
        }))}
        onFetch={vi.fn(async () => ({ remote: 'origin', refspecs: [] }))}
        onPull={vi.fn(async () => ({
          remote: 'origin',
          branch: 'main',
          updated: false,
          summary: 'Already up to date.',
          newHeadSha: null,
        }))}
        onPush={vi.fn(async () => ({ remote: 'origin', branch: 'main', updates: [] }))}
      />,
    )

    expect(screen.getByLabelText('Source control panel')).toHaveClass('invisible')
  })

  it('does not reload the selected diff when only repository totals change', async () => {
    const initialStatus = makeStatus()
    const { onLoadDiff, rerender } = renderVcsSidebar('diff --git a/file.txt b/file.txt\n+change', {
      status: initialStatus,
    })

    await waitFor(() => expect(onLoadDiff).toHaveBeenCalledTimes(1))

    rerender(
      <VcsSidebar
        open
        projectId="project-1"
        status={makeStatus({
          additions: initialStatus.additions + 10,
          deletions: initialStatus.deletions + 3,
          statusCount: initialStatus.statusCount + 1,
          entries: initialStatus.entries.map((entry) => ({ ...entry })),
        })}
        branchLabel="main"
        onClose={vi.fn()}
        onRefreshStatus={vi.fn()}
        onLoadDiff={onLoadDiff}
        commitMessageModel={{
          providerProfileId: 'openai-api-default',
          modelId: 'gpt-5.4',
          thinkingEffort: 'medium',
          label: 'gpt-5.4',
        }}
        onGenerateCommitMessage={vi.fn(async () => ({
          message: 'feat: update project file',
          providerId: 'openai_api',
          modelId: 'gpt-5.4',
          diffTruncated: false,
        }))}
        onStage={vi.fn(async () => undefined)}
        onUnstage={vi.fn(async () => undefined)}
        onDiscard={vi.fn(async () => undefined)}
        onCommit={vi.fn(async () => ({
          sha: 'def5678',
          summary: 'Commit summary',
          signature: { name: 'Test User', email: 'test@example.com' },
        }))}
        onFetch={vi.fn(async () => ({ remote: 'origin', refspecs: [] }))}
        onPull={vi.fn(async () => ({
          remote: 'origin',
          branch: 'main',
          updated: false,
          summary: 'Already up to date.',
          newHeadSha: null,
        }))}
        onPush={vi.fn(async () => ({ remote: 'origin', branch: 'main', updates: [] }))}
      />,
    )

    await waitFor(() => expect(screen.getByText('+11')).toBeInTheDocument())
    expect(onLoadDiff).toHaveBeenCalledTimes(1)
  })

  it('serves cached file patches for the same project, revision, scope, and path', async () => {
    const status = makeStatus({
      unstagedCount: 2,
      statusCount: 2,
      entries: [
        {
          path: 'file-a.txt',
          staged: null,
          unstaged: 'modified',
          untracked: false,
        },
        {
          path: 'file-b.txt',
          staged: null,
          unstaged: 'modified',
          untracked: false,
        },
      ],
    })
    const onLoadDiff = vi.fn(async () =>
      makeDiff(
        [
          'diff --git a/file-a.txt b/file-a.txt',
          '--- a/file-a.txt',
          '+++ b/file-a.txt',
          '@@ -1 +1 @@',
          '+first cached file',
          'diff --git a/file-b.txt b/file-b.txt',
          '--- a/file-b.txt',
          '+++ b/file-b.txt',
          '@@ -1 +1 @@',
          '+second cached file',
        ].join('\n'),
      ),
    )

    renderVcsSidebar('', { status, onLoadDiff })

    await waitFor(() => expect(screen.getByText('first cached file')).toBeInTheDocument())
    fireEvent.click(screen.getByText('file-b.txt'))

    await waitFor(() => expect(screen.getByText('second cached file')).toBeInTheDocument())
    expect(onLoadDiff).toHaveBeenCalledTimes(1)
  })

  it('windows large source-control file groups', async () => {
    const entries = Array.from({ length: 1_000 }, (_, index) => ({
      path: `src/file-${String(index).padStart(4, '0')}.ts`,
      staged: null,
      unstaged: 'modified' as const,
      untracked: false,
    }))

    renderVcsSidebar(makeSingleFilePatch('visible diff'), {
      status: makeStatus({
        unstagedCount: entries.length,
        statusCount: entries.length,
        entries,
      }),
    })

    await waitFor(() => expect(screen.getByText('file-0000.ts')).toBeInTheDocument())
    expect(screen.queryByText('file-0999.ts')).not.toBeInTheDocument()
  })

  it('windows large unified diffs', async () => {
    const patch = [
      'diff --git a/file.txt b/file.txt',
      '--- a/file.txt',
      '+++ b/file.txt',
      '@@ -1,1000 +1,1000 @@',
      ...Array.from({ length: 1_000 }, (_, index) => `+line-${String(index).padStart(4, '0')}`),
    ].join('\n')

    expect(parseDiffLines(patch)).toHaveLength(1_004)

    renderVcsSidebar(patch)

    await waitFor(() => expect(screen.getByText('line-0000')).toBeInTheDocument())
    expect(screen.queryByText('line-0999')).not.toBeInTheDocument()
  })

  it('caches parsed diff lines by patch key', () => {
    resetDiffPerformanceStatsForTests()
    const patch = makeSingleFilePatch('cached parse')

    const first = parseDiffLinesForPatchKey('project-1:file.txt:abc1234', patch)
    const second = parseDiffLinesForPatchKey('project-1:file.txt:abc1234', patch)

    expect(second).toBe(first)
    expect(getDiffParsingStats()).toMatchObject({
      hits: 1,
      misses: 1,
      parses: 1,
    })
  })

  it('bounds parsed diff cache entries by retained bytes', () => {
    resetDiffPerformanceStatsForTests()
    const largePatch = makeSingleFilePatch('x'.repeat(Math.ceil(DIFF_PARSE_CACHE_MAX_BYTES / 5)))

    parseDiffLinesForPatchKey('large-a', largePatch)
    parseDiffLinesForPatchKey('large-b', largePatch.replace('x', 'y'))

    const stats = getDiffParsingStats()
    expect(stats.byteSize).toBeLessThanOrEqual(DIFF_PARSE_CACHE_MAX_BYTES)
    expect(stats.evictions).toBeGreaterThan(0)
  })

  it('bounds selected diff patch cache entries by retained bytes', () => {
    const cache = createDiffPatchCache()
    const largePatch = 'x'.repeat(Math.ceil(DIFF_PATCH_CACHE_MAX_BYTES / 3))

    setCachedDiffPatch(cache, 'patch-a', largePatch)
    setCachedDiffPatch(cache, 'patch-b', largePatch.replace('x', 'y'))

    const stats = getDiffPatchCacheStats(cache)
    expect(stats.byteSize).toBeLessThanOrEqual(DIFF_PATCH_CACHE_MAX_BYTES)
    expect(stats.evictions).toBeGreaterThan(0)
  })

  it('plans diff tokenization in visible bounded batches and skips very long lines', () => {
    resetDiffPerformanceStatsForTests()
    const lines = parseDiffLines(
      [
        'diff --git a/file.txt b/file.txt',
        '--- a/file.txt',
        '+++ b/file.txt',
        '@@ -1,80 +1,80 @@',
        ...Array.from({ length: 80 }, (_, index) =>
          index === 7 ? `+${'x'.repeat(DIFF_LINE_HIGHLIGHT_BYTE_LIMIT / 2 + 1)}` : `+line-${index}`,
        ),
      ].join('\n'),
    )
    const indexes = Array.from({ length: lines.length }, (_, index) => index)
    const batches = createDiffTokenizationBatches({ indexes, lines })

    expect(batches.length).toBeGreaterThan(1)
    expect(batches.every((batch) => batch.length <= DIFF_TOKENIZATION_BATCH_SIZE)).toBe(true)
    expect(batches.flat()).not.toContain(11)
  })

  it('invalidates the selected diff cache when the repository revision changes', async () => {
    const onLoadDiff = vi
      .fn(async () => makeDiff(makeSingleFilePatch('fallback revision')))
      .mockResolvedValueOnce(makeDiff(makeSingleFilePatch('first revision')))
      .mockResolvedValueOnce(makeDiff(makeSingleFilePatch('second revision')))

    const { rerender } = renderVcsSidebar('', {
      status: makeStatus(),
      onLoadDiff,
    })

    await waitFor(() => expect(screen.getByText('first revision')).toBeInTheDocument())

    rerender(
      <VcsSidebar
        open
        projectId="project-1"
        status={makeStatus({ headShaLabel: 'def5678' })}
        branchLabel="main"
        onClose={vi.fn()}
        onRefreshStatus={vi.fn()}
        onLoadDiff={onLoadDiff}
        onStage={vi.fn(async () => undefined)}
        onUnstage={vi.fn(async () => undefined)}
        onDiscard={vi.fn(async () => undefined)}
        onCommit={vi.fn(async () => ({
          sha: 'def5678',
          summary: 'Commit summary',
          signature: { name: 'Test User', email: 'test@example.com' },
        }))}
        onFetch={vi.fn(async () => ({ remote: 'origin', refspecs: [] }))}
        onPull={vi.fn(async () => ({
          remote: 'origin',
          branch: 'main',
          updated: false,
          summary: 'Already up to date.',
          newHeadSha: null,
        }))}
        onPush={vi.fn(async () => ({ remote: 'origin', branch: 'main', updates: [] }))}
      />,
    )

    await waitFor(() => expect(screen.getByText('second revision')).toBeInTheDocument())
    expect(onLoadDiff).toHaveBeenCalledTimes(2)
  })

  it('generates a commit message from the staged diff', async () => {
    const onGenerateCommitMessage = vi.fn(async () => ({
      message: 'fix: tighten source control actions',
      providerId: 'openai_api',
      modelId: 'gpt-5.4',
      diffTruncated: false,
    }))
    renderVcsSidebar('', {
      status: makeStatus({
        stagedCount: 1,
        unstagedCount: 0,
        statusCount: 1,
        entries: [
          {
            path: 'file.txt',
            staged: 'modified',
            unstaged: null,
            untracked: false,
          },
        ],
      }),
      onGenerateCommitMessage,
    })

    fireEvent.click(screen.getByLabelText('Generate commit message with gpt-5.4'))

    await waitFor(() =>
      expect(screen.getByDisplayValue('fix: tighten source control actions')).toBeInTheDocument(),
    )
    expect(onGenerateCommitMessage).toHaveBeenCalledWith(
      'project-1',
      expect.objectContaining({ modelId: 'gpt-5.4' }),
    )
  })
})
