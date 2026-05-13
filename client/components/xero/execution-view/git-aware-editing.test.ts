import { describe, expect, it } from 'vitest'
import type { RepositoryDiffResponseDto, RepositoryStatusEntryView } from '@/src/lib/xero-model'
import {
  buildGitDiffLineMarkers,
  buildGitHunkPatch,
  buildGitStatusByProjectPath,
  findDiffFileForProjectPath,
} from './git-aware-editing'

function makeDiffFile(
  overrides: Partial<RepositoryDiffResponseDto['files'][number]> = {},
): RepositoryDiffResponseDto['files'][number] {
  const file: RepositoryDiffResponseDto['files'][number] = {
    oldPath: 'src/app.ts',
    newPath: 'src/app.ts',
    displayPath: 'src/app.ts',
    status: 'modified' as const,
    patch: [
      'diff --git a/src/app.ts b/src/app.ts',
      '--- a/src/app.ts',
      '+++ b/src/app.ts',
      '@@ -1,4 +1,5 @@',
      ' alpha',
      '-beta',
      '+BETA',
      '+inserted',
      ' gamma',
      '@@ -9,3 +10,2 @@',
      ' tail',
      '-removed',
      ' done',
      '',
    ].join('\n'),
    truncated: false,
    cacheKey: 'modified\u0000src/app.ts\u0000src/app.ts',
    hunks: [
      {
        header: '@@ -1,4 +1,5 @@',
        oldStart: 1,
        oldLines: 4,
        newStart: 1,
        newLines: 5,
        truncated: false,
        rows: [
          { kind: 'context', prefix: ' ', text: 'alpha', oldLineNumber: 1, newLineNumber: 1 },
          { kind: 'remove', prefix: '-', text: 'beta', oldLineNumber: 2 },
          { kind: 'add', prefix: '+', text: 'BETA', newLineNumber: 2 },
          { kind: 'add', prefix: '+', text: 'inserted', newLineNumber: 3 },
          { kind: 'context', prefix: ' ', text: 'gamma', oldLineNumber: 3, newLineNumber: 4 },
        ],
      },
      {
        header: '@@ -9,3 +10,2 @@',
        oldStart: 9,
        oldLines: 3,
        newStart: 10,
        newLines: 2,
        truncated: false,
        rows: [
          { kind: 'context', prefix: ' ', text: 'tail', oldLineNumber: 9, newLineNumber: 10 },
          { kind: 'remove', prefix: '-', text: 'removed', oldLineNumber: 10 },
          { kind: 'context', prefix: ' ', text: 'done', oldLineNumber: 11, newLineNumber: 11 },
        ],
      },
    ],
  }
  return { ...file, ...overrides }
}

describe('git-aware editor helpers', () => {
  it('normalizes repository status paths for editor project paths', () => {
    const entries: RepositoryStatusEntryView[] = [
      { path: 'src/app.ts', staged: null, unstaged: 'modified', untracked: false },
      { path: '/README.md', staged: null, unstaged: null, untracked: true },
    ]

    const statuses = buildGitStatusByProjectPath(entries)

    expect(statuses['/src/app.ts']).toMatchObject({
      label: 'M',
      description: 'unstaged modified',
      tone: 'modified',
    })
    expect(statuses['/README.md']).toMatchObject({
      label: 'U',
      description: 'untracked',
      tone: 'added',
    })
  })

  it('finds diff files by old, new, or display path', () => {
    const diff = {
      repository: {
        id: 'repo-1',
        projectId: 'project-1',
        rootPath: '/tmp/project',
        displayName: 'project',
        branch: 'main',
        headSha: 'abc123',
        isGitRepo: true,
      },
      scope: 'unstaged' as const,
      patch: '',
      files: [makeDiffFile()],
      truncated: false,
      baseRevision: 'HEAD',
    }

    expect(findDiffFileForProjectPath(diff, '/src/app.ts')?.displayPath).toBe('src/app.ts')
  })

  it('derives added, changed, and deleted gutter line markers from hunks', () => {
    expect(buildGitDiffLineMarkers(makeDiffFile())).toEqual([
      { line: 2, kind: 'changed', hunkIndex: 0 },
      { line: 3, kind: 'changed', hunkIndex: 0 },
      { line: 11, kind: 'deleted', hunkIndex: 1 },
    ])
  })

  it('extracts a single hunk patch with the file header preserved', () => {
    expect(buildGitHunkPatch(makeDiffFile(), 1)).toBe(
      [
        'diff --git a/src/app.ts b/src/app.ts',
        '--- a/src/app.ts',
        '+++ b/src/app.ts',
        '@@ -9,3 +10,2 @@',
        ' tail',
        '-removed',
        ' done',
        '',
      ].join('\n'),
    )
  })
})
