import type {
  RepositoryDiffResponseDto,
  RepositoryStatusEntryView,
} from '@/src/lib/xero-model'

export type EditorGitChangeKind =
  | 'added'
  | 'modified'
  | 'deleted'
  | 'renamed'
  | 'copied'
  | 'type_change'
  | 'conflicted'

export type EditorGitDiffLineKind = 'added' | 'changed' | 'deleted'

export interface EditorGitFileStatus {
  path: string
  repositoryPath: string
  label: string
  description: string
  tone: 'added' | 'modified' | 'deleted' | 'warning' | 'conflicted'
  staged: EditorGitChangeKind | null
  unstaged: EditorGitChangeKind | null
  untracked: boolean
}

export interface EditorGitDiffLineMarker {
  line: number
  kind: EditorGitDiffLineKind
  hunkIndex: number
}

export type EditorGitDiffFile = RepositoryDiffResponseDto['files'][number]
export type EditorGitDiffHunk = EditorGitDiffFile['hunks'][number]

const CHANGE_LABELS: Record<EditorGitChangeKind, string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
  type_change: 'T',
  conflicted: '!',
}

const CHANGE_DESCRIPTIONS: Record<EditorGitChangeKind, string> = {
  added: 'added',
  modified: 'modified',
  deleted: 'deleted',
  renamed: 'renamed',
  copied: 'copied',
  type_change: 'type changed',
  conflicted: 'conflicted',
}

export function projectPathFromRepositoryPath(path: string): string {
  const normalized = normalizeRepositoryPath(path)
  return normalized ? `/${normalized}` : '/'
}

export function normalizeRepositoryPath(path: string): string {
  return path.trim().replace(/^\/+/, '')
}

export function buildGitStatusByProjectPath(
  entries: readonly RepositoryStatusEntryView[] = [],
): Record<string, EditorGitFileStatus> {
  const statuses: Record<string, EditorGitFileStatus> = {}
  for (const entry of entries) {
    const repositoryPath = normalizeRepositoryPath(entry.path)
    if (!repositoryPath) continue
    const projectPath = projectPathFromRepositoryPath(repositoryPath)
    statuses[projectPath] = describeGitStatusEntry({
      ...entry,
      path: repositoryPath,
    })
  }
  return statuses
}

export function findDiffFileForProjectPath(
  diff: RepositoryDiffResponseDto | null,
  projectPath: string | null,
): EditorGitDiffFile | null {
  if (!diff || !projectPath) return null
  const repositoryPath = normalizeRepositoryPath(projectPath)
  if (!repositoryPath) return null

  return (
    diff.files.find((file) => {
      const oldPath = file.oldPath ? normalizeRepositoryPath(file.oldPath) : null
      const newPath = file.newPath ? normalizeRepositoryPath(file.newPath) : null
      const displayPath = normalizeRepositoryPath(file.displayPath)
      return oldPath === repositoryPath || newPath === repositoryPath || displayPath === repositoryPath
    }) ?? null
  )
}

export function buildGitDiffLineMarkers(
  file: EditorGitDiffFile | null | undefined,
): EditorGitDiffLineMarker[] {
  if (!file) return []
  const byLine = new Map<number, EditorGitDiffLineMarker>()

  file.hunks.forEach((hunk, hunkIndex) => {
    const hasRemoval = hunk.rows.some((row) => row.kind === 'remove')
    for (let rowIndex = 0; rowIndex < hunk.rows.length; rowIndex += 1) {
      const row = hunk.rows[rowIndex]
      if (row.kind === 'add' && row.newLineNumber) {
        setPreferredMarker(byLine, {
          line: row.newLineNumber,
          kind: hasRemoval ? 'changed' : 'added',
          hunkIndex,
        })
      }
      if (row.kind === 'remove') {
        if (hunk.rows[rowIndex + 1]?.kind === 'add') continue
        setPreferredMarker(byLine, {
          line: nearestNewLineForRemovedRow(hunk, rowIndex),
          kind: 'deleted',
          hunkIndex,
        })
      }
    }
  })

  return Array.from(byLine.values()).sort((left, right) => left.line - right.line)
}

export function buildGitHunkPatch(
  file: EditorGitDiffFile,
  hunkIndex: number,
): string | null {
  if (file.truncated) return null
  const hunk = file.hunks[hunkIndex]
  if (!hunk || hunk.truncated) return null

  const lines = file.patch.split('\n')
  const hunkStarts = lines
    .map((line, index) => (line.startsWith('@@ ') ? index : -1))
    .filter((index) => index >= 0)
  const start = hunkStarts[hunkIndex]
  if (start == null) return null

  const headerEnd = hunkStarts[0] ?? start
  const end = hunkStarts[hunkIndex + 1] ?? lines.length
  const selectedLines = [
    ...lines.slice(0, headerEnd),
    ...lines.slice(start, end),
  ].filter((line, index, all) => index < all.length - 1 || line.length > 0)

  if (selectedLines.length === 0) return null
  return `${selectedLines.join('\n')}\n`
}

function describeGitStatusEntry(entry: RepositoryStatusEntryView): EditorGitFileStatus {
  const primary = entry.untracked ? 'added' : entry.unstaged ?? entry.staged ?? 'modified'
  const staged = entry.staged
  const unstaged = entry.untracked ? 'added' : entry.unstaged
  const parts: string[] = []
  if (entry.untracked) {
    parts.push('untracked')
  } else {
    if (staged) parts.push(`staged ${CHANGE_DESCRIPTIONS[staged]}`)
    if (unstaged) parts.push(`unstaged ${CHANGE_DESCRIPTIONS[unstaged]}`)
  }

  return {
    path: projectPathFromRepositoryPath(entry.path),
    repositoryPath: normalizeRepositoryPath(entry.path),
    label: entry.untracked ? 'U' : CHANGE_LABELS[primary],
    description: parts.length > 0 ? parts.join(', ') : CHANGE_DESCRIPTIONS[primary],
    tone: gitToneForChange(primary),
    staged: staged ?? null,
    unstaged: unstaged ?? null,
    untracked: entry.untracked,
  }
}

function gitToneForChange(change: EditorGitChangeKind): EditorGitFileStatus['tone'] {
  switch (change) {
    case 'added':
      return 'added'
    case 'deleted':
      return 'deleted'
    case 'renamed':
    case 'copied':
    case 'type_change':
      return 'warning'
    case 'conflicted':
      return 'conflicted'
    case 'modified':
    default:
      return 'modified'
  }
}

function setPreferredMarker(
  byLine: Map<number, EditorGitDiffLineMarker>,
  marker: EditorGitDiffLineMarker,
): void {
  const existing = byLine.get(marker.line)
  if (!existing || markerPriority(marker.kind) > markerPriority(existing.kind)) {
    byLine.set(marker.line, marker)
  }
}

function markerPriority(kind: EditorGitDiffLineKind): number {
  switch (kind) {
    case 'deleted':
      return 3
    case 'changed':
      return 2
    case 'added':
      return 1
  }
}

function nearestNewLineForRemovedRow(hunk: EditorGitDiffHunk, rowIndex: number): number {
  for (let index = rowIndex + 1; index < hunk.rows.length; index += 1) {
    const line = hunk.rows[index]?.newLineNumber
    if (line) return line
  }
  for (let index = rowIndex - 1; index >= 0; index -= 1) {
    const line = hunk.rows[index]?.newLineNumber
    if (line) return line
  }
  return Math.max(1, hunk.newStart)
}
