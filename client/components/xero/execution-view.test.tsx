import type { ComponentProps } from 'react'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ExecutionPaneView } from '@/src/features/xero/use-xero-desktop-state'
import { XeroDesktopError } from '@/src/lib/xero-desktop'
import type {
  ListProjectFileIndexResponseDto,
  ListProjectFilesResponseDto,
  ReadProjectFileResponseDto,
  RepositoryDiffResponseDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import type { EditorTerminalTaskRequest } from './execution-view/editor-tasks'
import type {
  EditorAgentActivity,
  EditorAgentContextRequest,
} from './execution-view/agent-aware-editor-hooks'

function textFileResponse(projectId: string, path: string, text: string): ReadProjectFileResponseDto {
  return {
    kind: 'text',
    projectId,
    path,
    byteLength: text.length,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'text/plain; charset=utf-8',
    rendererKind: 'code',
    text,
  }
}

function writeFileResponse(
  projectId: string,
  path: string,
  content = '',
): WriteProjectFileResponseDto {
  return {
    projectId,
    path,
    byteLength: content.length,
    modifiedAt: '2026-01-01T00:00:01Z',
    contentHash: `saved-${path}-${content.length}`,
    mimeType: 'text/plain; charset=utf-8',
    rendererKind: 'code',
    preview: null,
  }
}

function svgFileResponse(projectId: string, path: string, text: string): ReadProjectFileResponseDto {
  return {
    kind: 'text',
    projectId,
    path,
    byteLength: text.length,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'image/svg+xml',
    rendererKind: 'svg',
    text,
  }
}

function markdownFileResponse(projectId: string, path: string, text: string): ReadProjectFileResponseDto {
  return {
    kind: 'text',
    projectId,
    path,
    byteLength: text.length,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'text/markdown; charset=utf-8',
    rendererKind: 'markdown',
    text,
  }
}

function csvFileResponse(projectId: string, path: string, text: string): ReadProjectFileResponseDto {
  return {
    kind: 'text',
    projectId,
    path,
    byteLength: text.length,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'text/csv; charset=utf-8',
    rendererKind: 'csv',
    text,
  }
}

function htmlFileResponse(projectId: string, path: string, text: string): ReadProjectFileResponseDto {
  return {
    kind: 'text',
    projectId,
    path,
    byteLength: text.length,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'text/html; charset=utf-8',
    rendererKind: 'html',
    text,
  }
}

function imageFileResponse(
  projectId: string,
  path: string,
  byteLength = 4096,
): ReadProjectFileResponseDto {
  return {
    kind: 'renderable',
    projectId,
    path,
    byteLength,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'image/png',
    rendererKind: 'image',
    previewUrl: `xero-asset://preview${path}`,
  }
}

function renderableFileResponse(
  projectId: string,
  path: string,
  rendererKind: 'pdf' | 'audio' | 'video',
  mimeType: string,
): ReadProjectFileResponseDto {
  return {
    kind: 'renderable',
    projectId,
    path,
    byteLength: 8192,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType,
    rendererKind,
    previewUrl: `xero-asset://preview${path}`,
  }
}

function unsupportedFileResponse(projectId: string, path: string): ReadProjectFileResponseDto {
  return {
    kind: 'unsupported',
    projectId,
    path,
    byteLength: 1048576,
    modifiedAt: '2026-01-01T00:00:00Z',
    contentHash: `test-${path}`,
    mimeType: 'application/octet-stream',
    reason: 'binary',
  }
}

function longMarkdownText(): string {
  return [
    '# Long doc',
    '',
    '```ts',
    'const payload = `' + 'x'.repeat(112 * 1024) + '`',
    '```',
  ].join('\n')
}

function largeCsvText(rowCount = 1_500): string {
  const rows = ['name,count']
  for (let index = 1; index <= rowCount; index += 1) {
    rows.push(`row-${index},${index}`)
  }
  return rows.join('\n')
}

vi.mock('./code-editor', async () => {
  const React = await import('react')

  function MockCodeEditor({
    documentVersion,
    filePath,
    onDirtyChange,
    onDocumentStatsChange,
    onSave,
    onSnapshotChange,
    onViewReady,
    savedValue = '',
    value,
  }: any) {
    const [draft, setDraft] = React.useState(value)
    const draftRef = React.useRef(value)

    React.useEffect(() => {
      setDraft(value)
      draftRef.current = value
    }, [documentVersion, filePath, value])

    React.useEffect(() => {
      const lineOffsets = () => {
        const text = draftRef.current
        const offsets = [0]
        for (let index = 0; index < text.length; index += 1) {
          if (text[index] === '\n') offsets.push(index + 1)
        }
        return offsets
      }
      const view = {
        state: {
          doc: {
            toString: () => draftRef.current,
            get lines() {
              return lineOffsets().length
            },
            line: (lineNumber: number) => {
              const offsets = lineOffsets()
              const from = offsets[Math.max(0, lineNumber - 1)] ?? 0
              const next = offsets[lineNumber]
              const to = next === undefined ? draftRef.current.length : Math.max(from, next - 1)
              return { from, to }
            },
          },
          selection: {
            main: {
              from: 0,
              to: 0,
              head: 0,
            },
          },
        },
        sliceDoc: (from: number, to: number) => draftRef.current.slice(from, to),
        dispatch: () => undefined,
        focus: () => undefined,
      }
      onViewReady?.(view)
      return () => onViewReady?.(null)
    }, [onViewReady])

    return (
      <div>
        <label>
          <span className="sr-only">Editor for {filePath}</span>
          <textarea
            aria-label={`Editor for ${filePath}`}
            onChange={(event) => {
              const next = event.target.value
              draftRef.current = next
              setDraft(next)
              onDirtyChange?.(next !== savedValue)
              onDocumentStatsChange?.({ lineCount: next.length === 0 ? 1 : next.split('\n').length })
            }}
            onBlur={() => onSnapshotChange?.(draftRef.current)}
            value={draft}
          />
        </label>
        <button onClick={() => onSave?.(draftRef.current)} type="button">
          Trigger save
        </button>
      </div>
    )
  }

  return {
    CodeEditor: MockCodeEditor,
    DEFAULT_EDITOR_RENDER_PREFERENCES: {
      fontSize: 13,
      tabSize: 2,
      insertSpaces: true,
      lineWrapping: true,
    },
  }
})

vi.mock('./file-tree', () => {
  function flatten(node: any): any[] {
    if (!node.children?.length) {
      return [node]
    }

    return [node, ...node.children.flatMap((child: any) => flatten(child))]
  }

  return {
    getFileIcon: (filename: string) => <span aria-hidden>{filename}</span>,
    FileTree: ({
      root,
      selectedPath,
      expandedFolders,
      dirtyPaths,
      diagnosticCountsByPath,
      agentActivityCountsByPath,
      creatingEntry,
      onSelectFile,
      onToggleFolder,
      onRequestRename,
      onRequestDelete,
      onRequestNewFile,
      onRequestNewFolder,
      onMoveEntry,
      onCreateEntry,
    }: any) => (
      <div data-testid="mock-file-tree">
        {flatten(root)
          .filter((node) => node.path !== '/')
          .map((node) =>
            node.type === 'folder' ? (
              <div data-testid={`folder:${node.path}`} key={node.path}>
                <span>
                  folder {node.path} {expandedFolders.has(node.path) ? 'expanded' : 'collapsed'}
                </span>
                <button onClick={() => onToggleFolder(node.path)} type="button">
                  Toggle {node.path}
                </button>
                <button onClick={() => onRequestRename(node.path, 'folder')} type="button">
                  Rename {node.path}
                </button>
                <button onClick={() => onRequestDelete(node.path, 'folder')} type="button">
                  Delete {node.path}
                </button>
                <button onClick={() => onRequestNewFile(node.path)} type="button">
                  New file in {node.path}
                </button>
                <button onClick={() => onRequestNewFolder(node.path)} type="button">
                  New folder in {node.path}
                </button>
                <button onClick={() => onMoveEntry('/README.md', node.path)} type="button">
                  Move README into {node.path}
                </button>
                {creatingEntry?.parentPath === node.path ? (
                  <form
                    onSubmit={(event) => {
                      event.preventDefault()
                      const form = event.currentTarget
                      const input = form.elements.namedItem('entryName') as HTMLInputElement
                      void onCreateEntry(input.value)
                    }}
                  >
                    <input
                      name="entryName"
                      placeholder={creatingEntry.type === 'file' ? 'folder/file.ext' : 'folder/subfolder'}
                    />
                    <button type="submit">Create</button>
                  </form>
                ) : null}
              </div>
            ) : (
              <div data-testid={`file:${node.path}`} key={node.path}>
                <span>
                  file {node.path} {selectedPath === node.path ? 'selected' : 'idle'}{' '}
                  {dirtyPaths?.has(node.path) ? 'dirty' : 'clean'} problems:{' '}
                  {diagnosticCountsByPath?.[node.path] ?? 0} agent:{' '}
                  {agentActivityCountsByPath?.[node.path] ?? 0}
                </span>
                <button onClick={() => onSelectFile(node.path)} type="button">
                  Open {node.path}
                </button>
                <button onClick={() => onRequestRename(node.path, 'file')} type="button">
                  Rename {node.path}
                </button>
                <button onClick={() => onRequestDelete(node.path, 'file')} type="button">
                  Delete {node.path}
                </button>
              </div>
            ),
          )}
      </div>
    ),
  }
})

import { ExecutionView } from './execution-view'

afterEach(() => {
  vi.clearAllMocks()
})

type ProjectNode = ListProjectFilesResponseDto['root']

function file(name: string, path: string): ProjectNode {
  return { name, path, type: 'file' }
}

function folder(name: string, path: string, children: ProjectNode[] = []): ProjectNode {
  return { name, path, type: 'folder', children }
}

function cloneNode(node: ProjectNode): ProjectNode {
  return node.type === 'folder'
    ? { ...node, children: node.children?.map((child) => cloneNode(child)) ?? [] }
    : { ...node }
}

function projectTreeView(root: ProjectNode): ListProjectFilesResponseDto['view'] {
  const nodesByPath: ListProjectFilesResponseDto['view']['nodesByPath'] = {}
  const childPathsByPath: ListProjectFilesResponseDto['view']['childPathsByPath'] = {}

  const ingest = (node: ProjectNode) => {
    nodesByPath[node.path] = {
      id: node.path,
      name: node.name,
      path: node.path,
      type: node.type,
      childrenLoaded: node.type === 'file' ? true : node.childrenLoaded ?? Boolean(node.children),
      truncated: node.truncated ?? false,
      omittedEntryCount: node.omittedEntryCount ?? 0,
    }
    if (node.type === 'folder') {
      childPathsByPath[node.path] = node.children?.map((child) => child.path) ?? []
      node.children?.forEach(ingest)
    }
  }
  ingest(root)

  return {
    rootPath: root.path,
    nodesByPath,
    childPathsByPath,
    loadedPaths: Object.values(nodesByPath)
      .filter((node) => node.type === 'folder' && node.childrenLoaded)
      .map((node) => node.path),
    stats: {
      byteSize: 1,
      childListCount: Object.keys(childPathsByPath).length,
      nodeCount: Object.keys(nodesByPath).length,
      unloadedFolderCount: Object.values(nodesByPath).filter(
        (node) => node.type === 'folder' && !node.childrenLoaded,
      ).length,
    },
    truncated: root.truncated ?? false,
    omittedEntryCount: root.omittedEntryCount ?? 0,
  }
}

function projectFileListing(projectId: string, path: string, root: ProjectNode): ListProjectFilesResponseDto {
  const listingRoot = findNode(root, path) ?? root
  const clonedRoot = cloneNode(listingRoot)
  return {
    projectId,
    path,
    root: clonedRoot,
    view: projectTreeView(clonedRoot),
    truncated: clonedRoot.truncated ?? false,
    omittedEntryCount: clonedRoot.omittedEntryCount ?? 0,
  }
}

function projectFileIndex(
  projectId: string,
  root: ProjectNode,
  includeHidden = false,
): ListProjectFileIndexResponseDto {
  const files: ListProjectFileIndexResponseDto['files'] = []
  const visit = (node: ProjectNode) => {
    if (node.type === 'file') {
      const hidden = node.path.split('/').filter(Boolean).some((segment) => segment.startsWith('.'))
      if (includeHidden || !hidden) {
        files.push({
          path: node.path,
          name: node.name,
          parentPath: parentPathOf(node.path),
          hidden,
        })
      }
      return
    }

    node.children?.forEach(visit)
  }
  visit(root)
  files.sort((left, right) => left.path.localeCompare(right.path))

  return {
    projectId,
    files,
    totalFiles: files.length,
    truncated: false,
    payloadBudget: null,
  }
}

function joinPath(parentPath: string, name: string): string {
  return parentPath === '/' ? `/${name}` : `${parentPath}/${name}`
}

function basename(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? 'root'
}

function parentPathOf(path: string): string {
  const segments = path.split('/').filter(Boolean)
  if (segments.length <= 1) {
    return '/'
  }

  return `/${segments.slice(0, -1).join('/')}`
}

function remapPath(candidate: string, oldBase: string, newBase: string): string {
  if (candidate === oldBase) return newBase
  if (candidate.startsWith(`${oldBase}/`)) return newBase + candidate.slice(oldBase.length)
  return candidate
}

function remapContentKeys(record: Record<string, string>, oldBase: string, newBase: string): Record<string, string> {
  const next: Record<string, string> = {}
  for (const [key, value] of Object.entries(record)) {
    next[remapPath(key, oldBase, newBase)] = value
  }
  return next
}

function removeContentKeys(record: Record<string, string>, path: string): Record<string, string> {
  const prefix = `${path}/`
  const next: Record<string, string> = {}
  for (const [key, value] of Object.entries(record)) {
    if (key === path || key.startsWith(prefix)) continue
    next[key] = value
  }
  return next
}

function addChild(root: ProjectNode, parentPath: string, child: ProjectNode): ProjectNode {
  if (root.type !== 'folder') {
    return root
  }

  if (root.path === parentPath) {
    return {
      ...root,
      children: [...(root.children ?? []), child],
    }
  }

  return {
    ...root,
    children: root.children?.map((candidate) => addChild(candidate, parentPath, child)) ?? [],
  }
}

function updateNodePaths(node: ProjectNode, oldBase: string, newBase: string): ProjectNode {
  const path = remapPath(node.path, oldBase, newBase)
  if (node.type === 'folder') {
    return {
      ...node,
      name: basename(path),
      path,
      children: node.children?.map((child) => updateNodePaths(child, oldBase, newBase)) ?? [],
    }
  }

  return {
    ...node,
    name: basename(path),
    path,
  }
}

function renamePath(root: ProjectNode, oldBase: string, newBase: string): ProjectNode {
  if (root.path === oldBase) {
    return updateNodePaths(root, oldBase, newBase)
  }

  if (root.type !== 'folder') {
    return root
  }

  return {
    ...root,
    children:
      root.children?.map((child) => {
        if (child.path === oldBase || child.path.startsWith(`${oldBase}/`)) {
          return updateNodePaths(child, oldBase, newBase)
        }
        return renamePath(child, oldBase, newBase)
      }) ?? [],
  }
}

function deletePath(root: ProjectNode, path: string): ProjectNode {
  if (root.type !== 'folder') {
    return root
  }

  return {
    ...root,
    children:
      root.children
        ?.filter((child) => child.path !== path)
        .map((child) => deletePath(child, path)) ?? [],
  }
}

function findNode(root: ProjectNode, path: string): ProjectNode | null {
  if (root.path === path) return root
  if (root.type !== 'folder') return null

  for (const child of root.children ?? []) {
    const found = findNode(child, path)
    if (found) return found
  }

  return null
}

function makeExecution(projectId = 'project-1', name = 'Xero'): ExecutionPaneView {
  return {
    project: {
      id: projectId,
      name,
      repository: {
        displayName: name,
        rootPath: `/tmp/${name}`,
      },
    } as ExecutionPaneView['project'],
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'abc123',
    statusEntries: [],
    statusCount: 0,
    hasChanges: false,
    diffScopes: [],
    verificationRecords: [],
    resumeHistory: [],
    latestDecisionOutcome: null,
    notificationBroker: {
      dispatches: [],
      actions: [],
      routes: [],
      byActionId: {},
      byRouteId: {},
      dispatchCount: 0,
      routeCount: 0,
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
      latestUpdatedAt: null,
      isTruncated: false,
      totalBeforeTruncation: 0,
    },
    operatorActionError: null,
    verificationUnavailableReason: 'Verification unavailable.',
  } as ExecutionPaneView
}

function makeGitExecution(): ExecutionPaneView {
  return {
    ...makeExecution(),
    statusEntries: [
      {
        path: 'README.md',
        staged: null,
        unstaged: 'modified',
        untracked: false,
      },
    ],
    statusCount: 1,
    hasChanges: true,
  }
}

function makeReadmeDiff(): RepositoryDiffResponseDto {
  return {
    repository: {
      id: 'repo-project-1',
      projectId: 'project-1',
      rootPath: '/tmp/Xero',
      displayName: 'Xero',
      branch: 'main',
      headSha: 'abc123',
      isGitRepo: true,
    },
    scope: 'unstaged',
    patch: [
      'diff --git a/README.md b/README.md',
      '--- a/README.md',
      '+++ b/README.md',
      '@@ -1,1 +1,1 @@',
      '-Hello',
      '+Hello git',
      '',
    ].join('\n'),
    files: [
      {
        oldPath: 'README.md',
        newPath: 'README.md',
        displayPath: 'README.md',
        status: 'modified',
        hunks: [
          {
            header: '@@ -1,1 +1,1 @@',
            oldStart: 1,
            oldLines: 1,
            newStart: 1,
            newLines: 1,
            rows: [
              { kind: 'remove', prefix: '-', text: 'Hello', oldLineNumber: 1 },
              { kind: 'add', prefix: '+', text: 'Hello git', newLineNumber: 1 },
            ],
            truncated: false,
          },
        ],
        patch: [
          'diff --git a/README.md b/README.md',
          '--- a/README.md',
          '+++ b/README.md',
          '@@ -1,1 +1,1 @@',
          '-Hello',
          '+Hello git',
          '',
        ].join('\n'),
        truncated: false,
        cacheKey: 'modified\u0000README.md\u0000README.md',
      },
    ],
    truncated: false,
    baseRevision: null,
  }
}

function makeEditorAgentActivity(
  overrides: Partial<EditorAgentActivity> = {},
): EditorAgentActivity {
  return {
    id: 'activity:run-1:1',
    path: '/README.md',
    operation: 'modify',
    status: 'recent',
    title: 'File changed',
    detail: 'modify: README.md',
    sessionTitle: 'Main session',
    paneId: 'pane-1',
    runId: 'run-1',
    createdAt: '2026-05-01T12:00:00Z',
    sequence: 1,
    changeGroupId: 'code-change-1',
    workspaceEpoch: 7,
    patchAvailability: {
      projectId: 'project-1',
      targetChangeGroupId: 'code-change-1',
      available: true,
      affectedPaths: ['README.md'],
      fileChangeCount: 1,
      textHunkCount: 1,
      textHunks: [
        {
          hunkId: 'hunk-1',
          patchFileId: 'patch-file-1',
          filePath: 'README.md',
          hunkIndex: 0,
          baseStartLine: 1,
          baseLineCount: 1,
          resultStartLine: 1,
          resultLineCount: 2,
        },
      ],
      unavailableReason: null,
    },
    ...overrides,
  }
}

function createDeferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })

  return { promise, resolve }
}

function createWorkspaceHarness(options?: {
  root?: ProjectNode
  fileContents?: Record<string, string>
}) {
  let currentRoot = cloneNode(
    options?.root ??
      folder('root', '/', [
        file('README.md', '/README.md'),
        folder('src', '/src', [file('main.tsx', '/src/main.tsx')]),
      ]),
  )
  let currentFileContents: Record<string, string> = {
    '/README.md': '# Xero\n',
    '/src/main.tsx': 'console.log("hello")\n',
    ...(options?.fileContents ?? {}),
  }

  const listProjectFiles = vi.fn(async (projectId: string, path = '/') =>
    projectFileListing(projectId, path, currentRoot),
  )
  const listProjectFileIndex = vi.fn(async (request: { projectId: string; includeHidden?: boolean }) =>
    projectFileIndex(request.projectId, currentRoot, request.includeHidden ?? false),
  )
  const readProjectFile = vi.fn(async (projectId: string, path: string) => ({
    ...textFileResponse(projectId, path, currentFileContents[path] ?? ''),
  }))
  const writeProjectFile = vi.fn(async (projectId: string, path: string, content: string) => {
    currentFileContents[path] = content
    return writeFileResponse(projectId, path, content)
  })
  const createProjectEntry = vi.fn(async (request) => {
    const path = joinPath(request.parentPath, request.name)
    const nextNode =
      request.entryType === 'folder'
        ? folder(request.name, path, [])
        : file(request.name, path)

    currentRoot = addChild(currentRoot, request.parentPath, nextNode)
    if (request.entryType === 'file') {
      currentFileContents[path] = ''
    }

    return {
      projectId: request.projectId,
      path,
    }
  })
  const renameProjectEntry = vi.fn(async (request) => {
    const nextPath = joinPath(parentPathOf(request.path), request.newName)
    currentRoot = renamePath(currentRoot, request.path, nextPath)
    currentFileContents = remapContentKeys(currentFileContents, request.path, nextPath)

    return {
      projectId: request.projectId,
      path: nextPath,
    }
  })
  const moveProjectEntry = vi.fn(async (request) => {
    const nextPath = joinPath(request.targetParentPath, basename(request.path))
    const movedNode = findNode(currentRoot, request.path)
    if (movedNode) {
      currentRoot = addChild(deletePath(currentRoot, request.path), request.targetParentPath, updateNodePaths(movedNode, request.path, nextPath))
      currentFileContents = remapContentKeys(currentFileContents, request.path, nextPath)
    }

    return {
      projectId: request.projectId,
      path: nextPath,
    }
  })
  const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => {
    currentRoot = deletePath(currentRoot, path)
    currentFileContents = removeContentKeys(currentFileContents, path)
    return { projectId, path }
  })
  const searchProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
    projectId,
    totalMatches: 0,
    totalFiles: 0,
    truncated: false,
    files: [],
  }))
  const replaceInProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
    projectId,
    filesChanged: 0,
    totalReplacements: 0,
  }))

  return {
    listProjectFileIndex,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
  }
}

function renderExecutionView(
  overrides: Partial<ComponentProps<typeof ExecutionView>> = {},
  workspace = createWorkspaceHarness(),
) {
  const props: ComponentProps<typeof ExecutionView> = {
    execution: makeExecution(),
    listProjectFileIndex: workspace.listProjectFileIndex,
    listProjectFiles: workspace.listProjectFiles,
    readProjectFile: workspace.readProjectFile,
    writeProjectFile: workspace.writeProjectFile,
    createProjectEntry: workspace.createProjectEntry,
    renameProjectEntry: workspace.renameProjectEntry,
    moveProjectEntry: workspace.moveProjectEntry,
    deleteProjectEntry: workspace.deleteProjectEntry,
    searchProject: workspace.searchProject,
    replaceInProject: workspace.replaceInProject,
    ...overrides,
  }

  return {
    workspace,
    ...render(<ExecutionView {...props} />),
  }
}

async function runPaletteCommand(name: string | RegExp) {
  fireEvent.keyDown(window, { key: 'P', code: 'KeyP', metaKey: true, shiftKey: true })
  const option = await screen.findByRole('option', { name })
  fireEvent.click(option)
  await waitFor(() => expect(screen.queryByRole('option', { name })).not.toBeInTheDocument())
}

describe('ExecutionView', () => {
  it('defers project tree loading while the editor pane is hidden', async () => {
    const { rerender, workspace } = renderExecutionView({ active: false })

    expect(workspace.listProjectFiles).not.toHaveBeenCalled()
    expect(screen.getByText('No files open')).toBeVisible()

    rerender(
      <ExecutionView
        active={false}
        execution={makeExecution('project-2', 'Project Two')}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    expect(workspace.listProjectFiles).not.toHaveBeenCalled()

    rerender(
      <ExecutionView
        active
        execution={makeExecution('project-2', 'Project Two')}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    await waitFor(() => expect(workspace.listProjectFiles).toHaveBeenCalledWith('project-2'))
  })

  it('opens files, tracks dirty state, reverts edits, and saves through the workspace controller', async () => {
    const { workspace } = renderExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    expect(screen.getByText('Explorer')).toBeVisible()
    expect(screen.queryByText('/tmp/Xero')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Search files')).toHaveValue('')
    const newFileButton = screen.getByRole('button', { name: 'New file' })
    expect(newFileButton).toHaveClass('size-6')
    expect(newFileButton).not.toHaveClass('size-9')
    expect(newFileButton.querySelector('svg')).toHaveClass('size-3')
    expect(screen.getByText('No files open')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))

    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    const editor = await screen.findByLabelText('Editor for /README.md')
    expect(editor).toHaveValue('# Xero\n')
    expect(screen.getByText('Saved')).toBeVisible()

    fireEvent.change(editor, { target: { value: '# Xero\nUpdated\n' } })

    expect(screen.getByText('● Unsaved')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Revert' })).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('dirty')

    fireEvent.click(screen.getByRole('button', { name: 'Revert' }))

    await waitFor(() => expect(screen.getByLabelText('Editor for /README.md')).toHaveValue('# Xero\n'))
    expect(screen.getByText('Saved')).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('clean')

    fireEvent.change(screen.getByLabelText('Editor for /README.md'), { target: { value: '# Saved\n' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(workspace.writeProjectFile).toHaveBeenCalledWith('project-1', '/README.md', '# Saved\n', {
        expectedContentHash: 'test-/README.md',
        expectedModifiedAt: '2026-01-01T00:00:00Z',
        overwrite: false,
      }),
    )
    await waitFor(() => expect(screen.getByText('Saved')).toBeVisible())
    expect(screen.queryByRole('button', { name: 'Revert' })).not.toBeInTheDocument()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('clean')
  })

  it('opens Git hunk actions for the active file and sends a reverse patch', async () => {
    const getRepositoryDiff = vi.fn(async () => makeReadmeDiff())
    const gitRevertPatch = vi.fn(async () => undefined)
    renderExecutionView({
      execution: makeGitExecution(),
      getRepositoryDiff,
      gitRevertPatch,
    })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))

    await waitFor(() => {
      expect(getRepositoryDiff).toHaveBeenCalledWith('project-1', 'unstaged')
    })
    await runPaletteCommand(/Show Git Changes/)

    expect(await screen.findByText('@@ -1,1 +1,1 @@')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Revert hunk' }))

    await waitFor(() => {
      expect(gitRevertPatch).toHaveBeenCalledWith({
        projectId: 'project-1',
        patch: expect.stringContaining('diff --git a/README.md b/README.md'),
      })
    })
  })

  it('resizes the editor explorer from the separator and persists the width', async () => {
    const { container } = renderExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()

    const explorer = container.querySelector('aside') as HTMLElement
    const separator = screen.getByRole('separator', { name: 'Resize explorer sidebar' })
    const before = Number.parseInt(explorer.style.width, 10)

    fireEvent.keyDown(separator, { key: 'ArrowRight' })

    await waitFor(() =>
      expect(Number.parseInt(separator.getAttribute('aria-valuenow') ?? '', 10)).toBeGreaterThan(before),
    )
    const after = Number.parseInt(separator.getAttribute('aria-valuenow') ?? '', 10)
    expect(window.localStorage.getItem('xero.editor.explorer.width')).toBe(String(after))
  })

  it('opens the find and replace sidebar from the explorer header search action', async () => {
    renderExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open find and replace' }))

    const findInput = await screen.findByLabelText('Find')
    expect(findInput).toHaveFocus()
    expect(screen.getByLabelText('Replace')).toBeVisible()
    expect(screen.queryByLabelText('Search files')).not.toBeInTheDocument()
  })

  it('quick opens indexed files outside the loaded explorer and exposes breadcrumb actions', async () => {
    const unloadedRoot = folder('root', '/', [
      file('README.md', '/README.md'),
      { name: 'src', path: '/src', type: 'folder', childrenLoaded: false },
    ])
    const loadedSrc = folder('src', '/src', [file('main.tsx', '/src/main.tsx')])
    const workspace = createWorkspaceHarness({
      root: unloadedRoot,
      fileContents: {
        '/src/main.tsx': 'export function App() {\n  return null\n}\n',
      },
    })
    workspace.listProjectFiles.mockImplementation(async (projectId: string, path = '/') =>
      path === '/src'
        ? projectFileListing(projectId, path, loadedSrc)
        : projectFileListing(projectId, path, unloadedRoot),
    )
    workspace.listProjectFileIndex.mockImplementation(async (request: { projectId: string; includeHidden?: boolean }) => ({
      projectId: request.projectId,
      files: [
        { path: '/README.md', name: 'README.md', parentPath: '/', hidden: false },
        { path: '/src/main.tsx', name: 'main.tsx', parentPath: '/src', hidden: false },
      ],
      totalFiles: 2,
      truncated: false,
      payloadBudget: null,
    }))
    const clipboardWriteText = vi.fn(async () => undefined)
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: clipboardWriteText },
    })

    renderExecutionView({}, workspace)

    expect(await screen.findByTestId('folder:/src')).toHaveTextContent('collapsed')
    expect(screen.queryByTestId('file:/src/main.tsx')).not.toBeInTheDocument()

    fireEvent.keyDown(window, { key: 'p', metaKey: true })

    await waitFor(() =>
      expect(workspace.listProjectFileIndex).toHaveBeenCalledWith({
        projectId: 'project-1',
        includeHidden: false,
      }),
    )
    fireEvent.change(await screen.findByPlaceholderText('Open file by name or path'), {
      target: { value: 'main' },
    })
    fireEvent.click(await screen.findByText('/src/main.tsx'))

    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/src/main.tsx'))
    expect(await screen.findByLabelText('Editor for /src/main.tsx')).toHaveValue(
      'export function App() {\n  return null\n}\n',
    )
    const breadcrumb = screen.getByRole('navigation', { name: 'Editor breadcrumb' })
    expect(within(breadcrumb).getByText('src')).toBeVisible()
    expect(within(breadcrumb).getByText('main.tsx')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Copy relative path' }))
    await waitFor(() => expect(clipboardWriteText).toHaveBeenCalledWith('src/main.tsx'))

    fireEvent.click(screen.getByRole('button', { name: 'Reveal in explorer' }))
    await waitFor(() => expect(workspace.listProjectFiles).toHaveBeenCalledWith('project-1', '/src'))
    expect(await screen.findByTestId('file:/src/main.tsx')).toBeVisible()
  })

  it('opens line, symbol, and reference navigation for source files', async () => {
    const workspace = createWorkspaceHarness({
      fileContents: {
        '/src/main.tsx': 'target()\nfunction target() {\n  return true\n}\n',
      },
    })

    renderExecutionView({}, workspace)

    expect(await screen.findByTestId('file:/src/main.tsx')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/src/main.tsx'))
    expect(await screen.findByLabelText('Editor for /src/main.tsx')).toBeVisible()

    fireEvent.keyDown(window, { key: 'g', ctrlKey: true })
    const lineInput = await screen.findByLabelText('Line and column')
    fireEvent.change(lineInput, { target: { value: '2:10' } })
    fireEvent.click(screen.getByRole('button', { name: 'Go' }))
    await waitFor(() => expect(screen.queryByLabelText('Line and column')).not.toBeInTheDocument())

    fireEvent.keyDown(window, { key: 'o', code: 'KeyO', metaKey: true, shiftKey: true })
    expect(await screen.findByPlaceholderText('Search symbols in file')).toBeVisible()
    fireEvent.click(await screen.findByText('target'))
    await waitFor(() => expect(screen.queryByPlaceholderText('Search symbols in file')).not.toBeInTheDocument())

    fireEvent.keyDown(window, { key: 'F12', shiftKey: true })
    expect(await screen.findByLabelText('Find')).toHaveValue('target')
    expect(screen.getByRole('button', { name: 'Project' })).toHaveAttribute('aria-pressed', 'true')
  })

  it('keeps tabs, dirty markers, expanded folders, cached contents, and active paths in sync across create rename delete flows', async () => {
    const workspace = createWorkspaceHarness({
      root: folder('root', '/', [folder('src', '/src', [file('main.tsx', '/src/main.tsx')])]),
      fileContents: {
        '/src/main.tsx': 'console.log("hello")\n',
      },
    })

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    expect(await screen.findByTestId('folder:/src')).toHaveTextContent('collapsed')

    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/src/main.tsx'))

    fireEvent.change(await screen.findByLabelText('Editor for /src/main.tsx'), {
      target: { value: 'console.log("dirty")\n' },
    })
    expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('dirty')

    fireEvent.click(screen.getByRole('button', { name: 'New file in /src' }))
    fireEvent.change(screen.getByPlaceholderText('folder/file.ext'), { target: { value: 'notes.md' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create' }))

    await waitFor(() =>
      expect(workspace.createProjectEntry).toHaveBeenCalledWith({
        projectId: 'project-1',
        parentPath: '/src',
        name: 'notes.md',
        entryType: 'file',
      }),
    )
    expect(await screen.findByTestId('file:/src/notes.md')).toBeVisible()
    expect(screen.getByLabelText('Editor for /src/notes.md')).toHaveValue('')
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    expect(screen.getByLabelText('Editor for /src/main.tsx')).toHaveValue('console.log("dirty")\n')
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Rename /src' }))
    const unsavedRenameDialog = await screen.findByRole('dialog', { name: 'Unsaved changes' })
    fireEvent.click(within(unsavedRenameDialog).getByRole('button', { name: 'Save' }))
    await waitFor(() =>
      expect(workspace.writeProjectFile).toHaveBeenCalledWith('project-1', '/src/main.tsx', 'console.log("dirty")\n', {
        expectedContentHash: 'test-/src/main.tsx',
        expectedModifiedAt: '2026-01-01T00:00:00Z',
        overwrite: false,
      }),
    )
    await waitFor(() => expect(screen.getByDisplayValue('src')).toBeVisible())
    fireEvent.change(screen.getByDisplayValue('src'), { target: { value: 'app' } })
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }))

    await waitFor(() =>
      expect(workspace.renameProjectEntry).toHaveBeenCalledWith({
        projectId: 'project-1',
        path: '/src',
        newName: 'app',
      }),
    )
    expect(await screen.findByTestId('folder:/app')).toHaveTextContent('expanded')
    expect(screen.getByTestId('file:/app/main.tsx')).toHaveTextContent('selected clean')
    expect(screen.getByLabelText('Editor for /app/main.tsx')).toHaveValue('console.log("dirty")\n')
    expect(screen.getByRole('button', { name: 'Close main.tsx' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Close notes.md' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /app/main.tsx' }))
    expect(screen.getByLabelText('Editor for /app/main.tsx')).toHaveValue('console.log("dirty")\n')
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Open /app/notes.md' }))
    await waitFor(() => expect(screen.getByLabelText('Editor for /app/notes.md')).toHaveValue(''))
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Delete /app/notes.md' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))

    await waitFor(() => expect(workspace.deleteProjectEntry).toHaveBeenCalledWith('project-1', '/app/notes.md'))
    await waitFor(() => expect(screen.queryByTestId('file:/app/notes.md')).not.toBeInTheDocument())
    expect(screen.queryByRole('button', { name: 'Close notes.md' })).not.toBeInTheDocument()
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
  })

  it('creates nested file paths inline and moves open files between folders', async () => {
    const workspace = createWorkspaceHarness()

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'New file in /src' }))
    fireEvent.change(screen.getByPlaceholderText('folder/file.ext'), { target: { value: 'docs/notes.md' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create' }))

    await waitFor(() =>
      expect(workspace.createProjectEntry).toHaveBeenNthCalledWith(1, {
        projectId: 'project-1',
        parentPath: '/src',
        name: 'docs',
        entryType: 'folder',
      }),
    )
    await waitFor(() =>
      expect(workspace.createProjectEntry).toHaveBeenNthCalledWith(2, {
        projectId: 'project-1',
        parentPath: '/src/docs',
        name: 'notes.md',
        entryType: 'file',
      }),
    )
    expect(await screen.findByTestId('file:/src/docs/notes.md')).toBeVisible()
    expect(screen.getByLabelText('Editor for /src/docs/notes.md')).toHaveValue('')

    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# Moved while dirty\n' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Move README into /src' }))
    await waitFor(() =>
      expect(workspace.moveProjectEntry).toHaveBeenCalledWith({
        projectId: 'project-1',
        path: '/README.md',
        targetParentPath: '/src',
      }),
    )
    expect(await screen.findByTestId('file:/src/README.md')).toHaveTextContent('selected dirty')
    expect(screen.getByLabelText('Editor for /src/README.md')).toHaveValue('# Moved while dirty\n')
  })

  it('ignores stale file reads after the selected project changes', async () => {
    const slowRead = createDeferred<ReadProjectFileResponseDto>()
    const rootForProject = (projectId: string) =>
      projectId === 'project-1'
        ? folder('root', '/', [file('README.md', '/README.md')])
        : folder('root', '/', [file('app.py', '/app.py')])
    const listProjectFiles = vi.fn(async (projectId: string, path = '/') =>
      projectFileListing(projectId, path, rootForProject(projectId)),
    )
    const listProjectFileIndex = vi.fn(async (request: { projectId: string; includeHidden?: boolean }) =>
      projectFileIndex(request.projectId, rootForProject(request.projectId), request.includeHidden ?? false),
    )
    const readProjectFile = vi.fn((projectId: string, path: string) => {
      if (projectId === 'project-1') {
        return slowRead.promise
      }

      return Promise.resolve({
        ...textFileResponse(projectId, path, 'print("project two")\n'),
      })
    })
    const writeProjectFile = vi.fn(async (projectId: string, path: string, content = '') =>
      writeFileResponse(projectId, path, content),
    )
    const revokeProjectAssetTokens = vi.fn(async () => undefined)
    const openProjectFileExternal = vi.fn(async () => undefined)
    const createProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(request.parentPath, request.name),
    }))
    const renameProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(parentPathOf(request.path), request.newName),
    }))
    const moveProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(request.targetParentPath, basename(request.path)),
    }))
    const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))
    const searchProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
      projectId,
      totalMatches: 0,
      totalFiles: 0,
      truncated: false,
      files: [],
    }))
    const replaceInProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
      projectId,
      filesChanged: 0,
      totalReplacements: 0,
    }))

    const { rerender } = render(
      <ExecutionView
        execution={makeExecution('project-1', 'Project One')}
        listProjectFileIndex={listProjectFileIndex}
        listProjectFiles={listProjectFiles}
        readProjectFile={readProjectFile}
        writeProjectFile={writeProjectFile}
        createProjectEntry={createProjectEntry}
        renameProjectEntry={renameProjectEntry}
        moveProjectEntry={moveProjectEntry}
        deleteProjectEntry={deleteProjectEntry}
        searchProject={searchProject}
        replaceInProject={replaceInProject}
      />,
    )

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))

    await waitFor(() => expect(readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))

    rerender(
      <ExecutionView
        execution={makeExecution('project-2', 'Project Two')}
        listProjectFileIndex={listProjectFileIndex}
        listProjectFiles={listProjectFiles}
        readProjectFile={readProjectFile}
        writeProjectFile={writeProjectFile}
        createProjectEntry={createProjectEntry}
        renameProjectEntry={renameProjectEntry}
        moveProjectEntry={moveProjectEntry}
        deleteProjectEntry={deleteProjectEntry}
        searchProject={searchProject}
        replaceInProject={replaceInProject}
      />,
    )

    expect(await screen.findByTestId('file:/app.py')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /app.py' }))

    await waitFor(() => expect(readProjectFile).toHaveBeenCalledWith('project-2', '/app.py'))
    await waitFor(() => expect(screen.getByLabelText('Editor for /app.py')).toHaveValue('print("project two")\n'))

    await act(async () => {
      slowRead.resolve({
        ...textFileResponse('project-1', '/README.md', '# stale response\n'),
      })
      await slowRead.promise
    })

    expect(screen.queryByLabelText('Editor for /README.md')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Editor for /app.py')).toHaveValue('print("project two")\n')
  })

  it('surfaces save failures without clearing the dirty editor state', async () => {
    const workspace = createWorkspaceHarness()
    workspace.writeProjectFile.mockImplementationOnce(async () => {
      throw new Error('Disk write failed.')
    })

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))

    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# Failure path\n' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(screen.getByText('Disk write failed.')).toBeVisible())
    expect(screen.getByText('● Unsaved')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Revert' })).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('dirty')
  })

  it('asks before closing a dirty tab', async () => {
    renderExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# Unsaved close\n' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Close README.md' }))
    const cancelDialog = await screen.findByRole('dialog', { name: 'Unsaved changes' })
    expect(within(cancelDialog).getByText('/README.md')).toBeVisible()

    fireEvent.click(within(cancelDialog).getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Unsaved changes' })).not.toBeInTheDocument())
    expect(screen.getByLabelText('Editor for /README.md')).toHaveValue('# Unsaved close\n')

    fireEvent.click(screen.getByRole('button', { name: 'Close README.md' }))
    const discardDialog = await screen.findByRole('dialog', { name: 'Unsaved changes' })
    fireEvent.click(within(discardDialog).getByRole('button', { name: 'Discard' }))

    await waitFor(() => expect(screen.queryByRole('button', { name: 'Close README.md' })).not.toBeInTheDocument())
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
  })

  it('surfaces conflict-safe save choices when disk metadata changed', async () => {
    const readProjectFile = vi
      .fn()
      .mockResolvedValueOnce(textFileResponse('project-1', '/README.md', '# Xero\n'))
      .mockResolvedValueOnce(textFileResponse('project-1', '/README.md', '# Disk\n'))
    const writeProjectFile = vi
      .fn()
      .mockRejectedValueOnce(
        new XeroDesktopError({
          code: 'project_file_changed_since_read',
          errorClass: 'user_fixable',
          message: '`/README.md` changed on disk.',
        }),
      )
      .mockImplementation(async (projectId: string, path: string, content: string) =>
        writeFileResponse(projectId, path, content),
      )

    renderExecutionView({ readProjectFile, writeProjectFile })

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# Mine\n' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    const conflictDialog = await screen.findByRole('dialog', { name: 'File changed on disk' })
    expect(within(conflictDialog).getByText('/README.md changed outside Xero after this tab loaded.')).toBeVisible()
    fireEvent.click(within(conflictDialog).getByRole('button', { name: 'Compare' }))
    expect(within(conflictDialog).getByText('# Mine')).toBeVisible()
    expect(within(conflictDialog).getByText('# Disk')).toBeVisible()

    fireEvent.click(within(conflictDialog).getByRole('button', { name: 'Overwrite' }))
    await waitFor(() =>
      expect(writeProjectFile).toHaveBeenLastCalledWith('project-1', '/README.md', '# Mine\n', {
        expectedContentHash: 'test-/README.md',
        expectedModifiedAt: '2026-01-01T00:00:00Z',
        overwrite: true,
      }),
    )
    await waitFor(() => expect(screen.getByText('Saved')).toBeVisible())
  })

  it('runs TypeScript diagnostics and opens problem locations', async () => {
    const runProjectTypecheck = vi.fn(async () => ({
      projectId: 'project-1',
      status: 'failed' as const,
      source: 'typescript',
      command: ['tsc', '--noEmit', '--pretty', 'false'],
      cwd: '/tmp/Xero',
      diagnostics: [
        {
          path: '/src/main.tsx',
          line: 1,
          column: 13,
          severity: 'error' as const,
          code: 'TS2322',
          message: "Type 'string' is not assignable to type 'number'.",
          source: 'typescript',
        },
      ],
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: '2026-01-01T00:00:01Z',
      durationMs: 1000,
      exitCode: 2,
      truncated: false,
      message: 'local TypeScript compiler',
      lspServers: [
        {
          serverId: 'typescript_language_server',
          language: 'TypeScript/JavaScript',
          command: 'typescript-language-server',
          args: ['--stdio'],
          available: false,
          supportsDiagnostics: true,
          supportsSymbols: true,
          supportsHover: true,
          supportsCompletion: true,
          supportsDefinition: true,
          supportsReferences: true,
          supportsRename: true,
          supportsCodeActions: true,
          installSuggestion: {
            reason: '`typescript-language-server` was not found on PATH.',
            candidateCommands: [
              {
                label: 'npm global',
                argv: ['npm', 'install', '-g', 'typescript', 'typescript-language-server'],
              },
            ],
          },
        },
      ],
    }))

    renderExecutionView({ runProjectTypecheck })

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    await runPaletteCommand(/Run Typecheck/)

    await waitFor(() => expect(runProjectTypecheck).toHaveBeenCalledWith({ projectId: 'project-1' }))
    expect(await screen.findByTestId('problems-panel')).toHaveTextContent('1 error')
    expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('problems: 1')

    fireEvent.click(screen.getByRole('button', { name: /TS2322/ }))
    await waitFor(() => expect(screen.getByLabelText('Editor for /src/main.tsx')).toBeVisible())
    expect(screen.getByText('1 problem')).toBeVisible()
    expect(screen.getByText('LSP servers: 0/1 available')).toBeVisible()
  })

  it('formats the active document through the formatter and writes the result', async () => {
    const formatProjectDocument = vi.fn(async (request: {
      projectId: string
      path: string
      content: string
    }) => ({
      projectId: request.projectId,
      path: request.path,
      status: 'formatted' as const,
      formatterId: 'prettier',
      command: ['prettier', '--stdin-filepath=README.md'],
      content: '# Pretty\n',
      rangeApplied: null,
      diagnostics: [],
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: '2026-01-01T00:00:01Z',
      durationMs: 12,
      message: null,
    }))

    const { workspace } = renderExecutionView({ formatProjectDocument })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    const editor = await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(editor, { target: { value: '# raw\n' } })

    await runPaletteCommand(/Format Document/)

    await waitFor(() =>
      expect(formatProjectDocument).toHaveBeenCalledWith({
        projectId: 'project-1',
        path: '/README.md',
        content: '# raw\n',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Editor for /README.md')).toHaveValue('# Pretty\n'))
  })

  it('runs format on save when the toggle is enabled before writing through the workspace', async () => {
    const formatProjectDocument = vi.fn(async (request: {
      projectId: string
      path: string
      content: string
    }) => ({
      projectId: request.projectId,
      path: request.path,
      status: 'formatted' as const,
      formatterId: 'prettier',
      command: ['prettier'],
      content: '# Pretty\n',
      rangeApplied: null,
      diagnostics: [],
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: '2026-01-01T00:00:01Z',
      durationMs: 12,
      message: null,
    }))

    const { workspace } = renderExecutionView({ formatProjectDocument })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')
    await runPaletteCommand(/Format on Save: Off/)
    fireEvent.change(screen.getByLabelText('Editor for /README.md'), {
      target: { value: '# raw\n' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(formatProjectDocument).toHaveBeenCalled())
    await waitFor(() =>
      expect(workspace.writeProjectFile).toHaveBeenCalledWith(
        'project-1',
        '/README.md',
        '# Pretty\n',
        expect.objectContaining({ overwrite: false }),
      ),
    )
  })

  it('reports lint diagnostics in the Problems panel and updates file tree badges', async () => {
    const runProjectLint = vi.fn(async () => ({
      projectId: 'project-1',
      status: 'failed' as const,
      source: 'eslint',
      command: ['eslint', '--format', 'json', '.'],
      cwd: '/tmp/Xero',
      diagnostics: [
        {
          path: '/src/main.tsx',
          line: 4,
          column: 2,
          severity: 'warning' as const,
          code: 'no-unused-vars',
          message: "'foo' is defined but never used.",
          source: 'eslint',
        },
      ],
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: '2026-01-01T00:00:01Z',
      durationMs: 25,
      exitCode: 1,
      truncated: false,
      message: null,
    }))

    renderExecutionView({ runProjectLint })

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    await runPaletteCommand(/Run Lint/)

    await waitFor(() =>
      expect(runProjectLint).toHaveBeenCalledWith({ projectId: 'project-1' }),
    )
    expect(await screen.findByTestId('problems-panel')).toHaveTextContent('0 errors · 1 warning')
    expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('problems: 1')
    expect(screen.getByRole('button', { name: /no-unused-vars/ })).toBeVisible()
  })

  it('runs editor typecheck tasks in a labeled terminal and streams matched problems', async () => {
    let taskRequest: EditorTerminalTaskRequest | null = null
    const runEditorTerminalTask = vi.fn(async (request: EditorTerminalTaskRequest) => {
      taskRequest = request
      return 'term-1'
    })

    renderExecutionView({ runEditorTerminalTask })

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    await runPaletteCommand(/Run Typecheck/)

    await waitFor(() => expect(runEditorTerminalTask).toHaveBeenCalled())
    expect(taskRequest).toMatchObject({
      taskId: 'typecheck',
      kind: 'typecheck',
      label: 'task: typecheck',
      exitWhenDone: true,
    })

    act(() => {
      taskRequest?.onData?.(
        "src/main.tsx(2,4): error TS1005: ';' expected.\n",
      )
    })

    expect(await screen.findByTestId('problems-panel')).toHaveTextContent('1 error')
    expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('problems: 1')

    act(() => {
      taskRequest?.onExit?.({ terminalId: 'term-1', exitCode: 2 })
    })

    expect(screen.getByTestId('problems-panel')).toHaveTextContent('Typecheck exited with code 2')
    fireEvent.click(screen.getByRole('button', { name: /TS1005/ }))
    await waitFor(() => expect(screen.getByLabelText('Editor for /src/main.tsx')).toBeVisible())
  })

  it('sends the current editor draft to the agent from the top bar', async () => {
    const onSendEditorContextToAgent = vi.fn(async (_request: EditorAgentContextRequest) => undefined)
    const { workspace } = renderExecutionView({ onSendEditorContextToAgent })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    const editor = await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(editor, { target: { value: '# Draft\n' } })

    await runPaletteCommand(/Fix File with Agent/)

    await waitFor(() => expect(onSendEditorContextToAgent).toHaveBeenCalledTimes(1))
    expect(onSendEditorContextToAgent).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'fix_file',
        path: '/README.md',
        content: '# Draft\n',
        savedContent: '# Xero\n',
        isDirty: true,
        selection: null,
      }),
    )
    expect(onSendEditorContextToAgent.mock.calls[0]?.[0].prompt).toContain('unsaved changes')
  })

  it('surfaces agent file activity and previews dirty editor conflicts', async () => {
    const { workspace } = renderExecutionView({
      agentActivities: [makeEditorAgentActivity()],
    })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    expect(await screen.findByText(/Agent activity on this file/)).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('agent: 1')

    const editor = await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(editor, { target: { value: '# Mine\n' } })
    fireEvent.click(screen.getByRole('button', { name: 'Preview' }))

    const dialog = await screen.findByRole('dialog', { name: 'Agent edit preview' })
    expect(within(dialog).getByText(/Your open editor draft has unsaved changes/)).toBeVisible()
    expect(within(dialog).getByText('hunk-1')).toBeVisible()
    expect(within(dialog).getByText('Saved base')).toBeVisible()
    expect(within(dialog).getByText('Your editor draft')).toBeVisible()
    expect(within(dialog).getByText('Agent / disk')).toBeVisible()
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
  })

  it('surfaces formatter failure diagnostics without rewriting the document', async () => {
    const formatProjectDocument = vi.fn(async () => ({
      projectId: 'project-1',
      path: '/README.md',
      status: 'failed' as const,
      formatterId: 'prettier',
      command: ['prettier'],
      content: null,
      rangeApplied: null,
      diagnostics: [
        {
          path: '/README.md',
          line: null,
          column: null,
          severity: 'error' as const,
          code: 'exit:2',
          message: 'Unexpected token at line 3',
          source: 'prettier',
        },
      ],
      startedAt: '2026-01-01T00:00:00Z',
      completedAt: '2026-01-01T00:00:01Z',
      durationMs: 4,
      message: 'Unexpected token at line 3',
    }))

    const { workspace } = renderExecutionView({ formatProjectDocument })

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(screen.getByLabelText('Editor for /README.md'), {
      target: { value: '# raw\n' },
    })
    await runPaletteCommand(/Format Document/)

    await waitFor(() => expect(formatProjectDocument).toHaveBeenCalled())
    expect(workspace.writeProjectFile).not.toHaveBeenCalled()
    expect(await screen.findByTestId('problems-panel')).toHaveTextContent(
      'Unexpected token at line 3',
    )
  })

  it('preserves the original CRLF line endings and final-newline on save', async () => {
    const harness = createWorkspaceHarness({
      fileContents: {
        '/README.md': 'first\r\nsecond\r\nthird\r\n',
      },
    })
    const { workspace } = renderExecutionView({}, harness)

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    const editor = await screen.findByLabelText('Editor for /README.md')
    expect(editor).toHaveValue('first\nsecond\nthird\n')
    fireEvent.change(editor, { target: { value: 'first\nsecond UPDATED\nthird\n' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(workspace.writeProjectFile).toHaveBeenLastCalledWith(
        'project-1',
        '/README.md',
        'first\r\nsecond UPDATED\r\nthird\r\n',
        expect.objectContaining({ overwrite: false }),
      ),
    )
    expect(screen.getByText('CRLF')).toBeVisible()
  })

  it('shows detected indent + EOL in the status bar', async () => {
    const harness = createWorkspaceHarness({
      fileContents: {
        '/src/main.tsx': 'function foo() {\n    return 1\n    if (cond) {\n        return 2\n    }\n}\n',
      },
    })
    renderExecutionView({}, harness)

    fireEvent.click(await screen.findByRole('button', { name: 'Open /src/main.tsx' }))
    await screen.findByLabelText('Editor for /src/main.tsx')
    expect(screen.getByText('Spaces (4)')).toBeVisible()
    expect(screen.getByText('LF')).toBeVisible()
  })

  it('saves all dirty tabs through the Save All keyboard shortcut', async () => {
    const { workspace } = renderExecutionView()

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')
    fireEvent.change(screen.getByLabelText('Editor for /README.md'), {
      target: { value: '# updated\n' },
    })
    await waitFor(() =>
      expect(screen.getByTestId('file:/README.md')).toHaveTextContent('dirty'),
    )
    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await screen.findByLabelText('Editor for /src/main.tsx')
    fireEvent.change(screen.getByLabelText('Editor for /src/main.tsx'), {
      target: { value: 'console.log("updated")\n' },
    })
    await waitFor(() =>
      expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('dirty'),
    )

    act(() => {
      fireEvent.keyDown(window, { key: 's', code: 'KeyS', metaKey: true, altKey: true })
    })

    await waitFor(() => {
      expect(workspace.writeProjectFile).toHaveBeenCalledWith(
        'project-1',
        '/README.md',
        '# updated\n',
        expect.objectContaining({ overwrite: false }),
      )
    })
    await waitFor(() => {
      expect(workspace.writeProjectFile).toHaveBeenCalledWith(
        'project-1',
        '/src/main.tsx',
        'console.log("updated")\n',
        expect.objectContaining({ overwrite: false }),
      )
    })
  })

  it('cycles between tabs with the next/previous tab keyboard shortcut', async () => {
    renderExecutionView()

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')
    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await screen.findByLabelText('Editor for /src/main.tsx')

    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowLeft', metaKey: true, altKey: true })
    })

    await waitFor(() =>
      expect(screen.getByTestId('file:/README.md')).toHaveTextContent('selected'),
    )

    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowRight', metaKey: true, altKey: true })
    })

    await waitFor(() =>
      expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('selected'),
    )
  })

  it('exposes the editor commands chip and preferences gear when text files are open', async () => {
    renderExecutionView()

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')

    expect(screen.getByRole('button', { name: /Open editor commands/ })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Editor preferences' })).toBeVisible()
  })

  it('exposes editor tabs as a tablist with arrow-key navigation', async () => {
    renderExecutionView()

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    await screen.findByLabelText('Editor for /README.md')
    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await screen.findByLabelText('Editor for /src/main.tsx')

    const tablist = screen.getByRole('tablist', { name: 'Open editor tabs' })
    const tabs = within(tablist).getAllByRole('tab')
    expect(tabs).toHaveLength(2)
    const active = tabs.find((tab) => tab.getAttribute('aria-selected') === 'true')
    expect(active).toBeDefined()
    expect(active).toHaveAttribute('tabIndex', '0')
    const inactive = tabs.find((tab) => tab.getAttribute('aria-selected') !== 'true')
    expect(inactive).toHaveAttribute('tabIndex', '-1')

    fireEvent.keyDown(active!, { key: 'ArrowLeft' })
    await waitFor(() =>
      expect(screen.getByTestId('file:/README.md')).toHaveTextContent('selected'),
    )
  })

  it('announces save failures through the assertive live region', async () => {
    const workspace = createWorkspaceHarness()
    workspace.writeProjectFile.mockImplementationOnce(async () => {
      throw new Error('Disk write failed.')
    })

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# Failure path\n' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    const alert = await screen.findByTestId('editor-live-region-alert')
    await waitFor(() => expect(alert.textContent).toMatch(/Workspace error: Disk write failed\./))
    expect(alert).toHaveAttribute('aria-live', 'assertive')
  })

  it('shows an editor context menu with Cut, Copy, Paste, and navigation actions', async () => {
    renderExecutionView()

    fireEvent.click(await screen.findByRole('button', { name: 'Open /README.md' }))
    const editor = await screen.findByLabelText('Editor for /README.md')

    fireEvent.contextMenu(editor)

    await screen.findByRole('menuitem', { name: /^Cut/ })
    const items = screen.getAllByRole('menuitem').map((node) => node.textContent ?? '')
    expect(items.some((text) => text.startsWith('Cut'))).toBe(true)
    expect(items.some((text) => text.startsWith('Copy') && !text.startsWith('Copy path'))).toBe(true)
    expect(items.some((text) => text.startsWith('Paste'))).toBe(true)
    expect(items.some((text) => text.startsWith('Select all'))).toBe(true)
    expect(items.some((text) => text.startsWith('Find'))).toBe(true)
    expect(items.some((text) => text.includes('Reveal in Explorer'))).toBe(true)
    expect(items.some((text) => text.startsWith('Copy path'))).toBe(true)
  })
})

describe('ExecutionView file editor host', () => {
  function createMixedHarness() {
    const root = folder('root', '/', [
      file('README.md', '/README.md'),
      file('logo.svg', '/logo.svg'),
      file('index.html', '/index.html'),
      file('photo.png', '/photo.png'),
      file('large-photo.png', '/large-photo.png'),
      file('data.csv', '/data.csv'),
      file('large.csv', '/large.csv'),
      file('paper.pdf', '/paper.pdf'),
      file('theme.mp3', '/theme.mp3'),
      file('demo.mp4', '/demo.mp4'),
      folder('docs', '/docs', [
        file('guide.md', '/docs/guide.md'),
        file('logo.png', '/docs/logo.png'),
      ]),
      file('long.md', '/long.md'),
      file('archive.bin', '/archive.bin'),
    ])
    const currentRoot = cloneNode(root)

    const responses: Record<string, () => ReadProjectFileResponseDto> = {
      '/README.md': () => textFileResponse('project-1', '/README.md', '# Xero\n'),
      '/logo.svg': () =>
        svgFileResponse('project-1', '/logo.svg', '<svg xmlns="http://www.w3.org/2000/svg"></svg>'),
      '/index.html': () =>
        htmlFileResponse(
          'project-1',
          '/index.html',
          [
            '<!doctype html>',
            '<h1>Preview App</h1>',
            '<script>window.xeroUnsafe = true</script>',
            '<img alt="Logo" src="./docs/logo.png">',
          ].join('\n'),
        ),
      '/photo.png': () => imageFileResponse('project-1', '/photo.png'),
      '/large-photo.png': () =>
        imageFileResponse('project-1', '/large-photo.png', 128 * 1024 * 1024),
      '/data.csv': () => csvFileResponse('project-1', '/data.csv', 'name,count\nAlpha,1\n'),
      '/large.csv': () => csvFileResponse('project-1', '/large.csv', largeCsvText()),
      '/paper.pdf': () =>
        renderableFileResponse('project-1', '/paper.pdf', 'pdf', 'application/pdf'),
      '/theme.mp3': () =>
        renderableFileResponse('project-1', '/theme.mp3', 'audio', 'audio/mpeg'),
      '/demo.mp4': () =>
        renderableFileResponse('project-1', '/demo.mp4', 'video', 'video/mp4'),
      '/docs/guide.md': () =>
        markdownFileResponse(
          'project-1',
          '/docs/guide.md',
          [
            '# Guide',
            '',
            '![Logo](./logo.png)',
            '',
            '| Package | Status |',
            '| --- | --- |',
            '| renderer | ready |',
            '',
            '```ts',
            'const preview = true',
            '```',
          ].join('\n'),
        ),
      '/docs/logo.png': () => imageFileResponse('project-1', '/docs/logo.png'),
      '/long.md': () => markdownFileResponse('project-1', '/long.md', longMarkdownText()),
      '/archive.bin': () => unsupportedFileResponse('project-1', '/archive.bin'),
    }

    const listProjectFiles = vi.fn(async (projectId: string, path = '/') =>
      projectFileListing(projectId, path, currentRoot),
    )
    const listProjectFileIndex = vi.fn(async (request: { projectId: string; includeHidden?: boolean }) =>
      projectFileIndex(request.projectId, currentRoot, request.includeHidden ?? false),
    )
    const readProjectFile = vi.fn(async (_projectId: string, path: string) => {
      const builder = responses[path]
      if (!builder) {
        return textFileResponse('project-1', path, '')
      }
      return builder()
    })
    const writeProjectFile = vi.fn(async (projectId: string, path: string, content = '') =>
      writeFileResponse(projectId, path, content),
    )
    const revokeProjectAssetTokens = vi.fn(async () => undefined)
    const openProjectFileExternal = vi.fn(async () => undefined)
    const createProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(request.parentPath, request.name),
    }))
    const renameProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(parentPathOf(request.path), request.newName),
    }))
    const moveProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(request.targetParentPath, basename(request.path)),
    }))
    const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))
    const searchProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
      projectId,
      totalMatches: 0,
      totalFiles: 0,
      truncated: false,
      files: [],
    }))
    const replaceInProject = vi.fn(async ({ projectId }: { projectId: string }) => ({
      projectId,
      filesChanged: 0,
      totalReplacements: 0,
    }))

    return {
      listProjectFileIndex,
      listProjectFiles,
      readProjectFile,
      writeProjectFile,
      revokeProjectAssetTokens,
      openProjectFileExternal,
      createProjectEntry,
      renameProjectEntry,
      moveProjectEntry,
      deleteProjectEntry,
      searchProject,
      replaceInProject,
    }
  }

  function renderHostExecutionView() {
    const workspace = createMixedHarness()
    return {
      workspace,
      ...render(
        <ExecutionView
          execution={makeExecution()}
          listProjectFileIndex={workspace.listProjectFileIndex}
          listProjectFiles={workspace.listProjectFiles}
          readProjectFile={workspace.readProjectFile}
          writeProjectFile={workspace.writeProjectFile}
          revokeProjectAssetTokens={workspace.revokeProjectAssetTokens}
          openProjectFileExternal={workspace.openProjectFileExternal}
          createProjectEntry={workspace.createProjectEntry}
          renameProjectEntry={workspace.renameProjectEntry}
          moveProjectEntry={workspace.moveProjectEntry}
          deleteProjectEntry={workspace.deleteProjectEntry}
          searchProject={workspace.searchProject}
          replaceInProject={workspace.replaceInProject}
        />,
      ),
    }
  }

  it('routes text/code files to CodeMirror without a source/preview toggle', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))

    expect(await screen.findByLabelText('Editor for /README.md')).toBeVisible()
    expect(screen.queryByRole('radio', { name: 'Show source' })).not.toBeInTheDocument()
    expect(screen.queryByTestId('preview-status-bar')).not.toBeInTheDocument()
    expect(screen.getByText('Saved')).toBeVisible()
  })

  it('routes SVG files to source mode by default with a preview toggle', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/logo.svg')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /logo.svg' }))

    // SVG defaults to preview surface
    expect(await screen.findByTestId('svg-preview')).toBeVisible()
    expect(screen.getByRole('radio', { name: 'Show source' })).toBeVisible()
    expect(screen.getByRole('radio', { name: 'Show preview' })).toBeVisible()
    // Save controls hidden while in preview mode (no dirty state from CodeMirror yet)
    expect(screen.queryByText('Ln 1, Col 1')).not.toBeInTheDocument()
    expect(screen.getByTestId('preview-status-bar')).toBeVisible()

    // Toggle to source — shows CodeMirror and the source-mode status bar
    fireEvent.click(screen.getByRole('radio', { name: 'Show source' }))
    expect(await screen.findByLabelText('Editor for /logo.svg')).toBeVisible()
    expect(screen.getByText(/Ln 1, Col 1/)).toBeVisible()
  })

  it('renders HTML files in a sandboxed preview without executing project scripts', async () => {
    const { workspace } = renderHostExecutionView()

    expect(await screen.findByTestId('file:/index.html')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /index.html' }))

    expect(await screen.findByLabelText('Editor for /index.html')).toBeVisible()
    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))

    const preview = await screen.findByTestId('html-preview')
    const iframe = preview.querySelector('iframe')
    expect(iframe).toHaveAttribute('sandbox', '')
    await waitFor(() =>
      expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/docs/logo.png'),
    )
    expect(iframe?.getAttribute('srcdoc')).not.toContain('<script>')
    expect(iframe?.getAttribute('srcdoc')).toContain('xero-asset://preview/docs/logo.png')
    expect(screen.getByRole('status')).toHaveTextContent('Removed 1 blocked element')
  })

  it('renders an image preview for renderable image files and hides save controls', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/photo.png')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /photo.png' }))

    const preview = await screen.findByTestId('image-preview')
    expect(preview.querySelector('img')).toHaveAttribute('src', 'xero-asset://preview/photo.png')
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Revert' })).not.toBeInTheDocument()
    expect(screen.queryByRole('radio', { name: 'Show source' })).not.toBeInTheDocument()
    // Image controls live in the unified top bar.
    expect(screen.getByRole('button', { name: 'Zoom in' })).toBeVisible()
    expect(screen.getByTestId('preview-status-bar')).toBeVisible()
  })

  it('revokes project asset preview tokens when preview tabs close or the project changes', async () => {
    const { workspace, rerender } = renderHostExecutionView()

    expect(await screen.findByTestId('file:/photo.png')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /photo.png' }))
    expect(await screen.findByTestId('image-preview')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Close photo.png' }))

    await waitFor(() =>
      expect(workspace.revokeProjectAssetTokens).toHaveBeenCalledWith('project-1', ['/photo.png']),
    )

    rerender(
      <ExecutionView
        execution={makeExecution('project-2', 'Project Two')}
        listProjectFileIndex={workspace.listProjectFileIndex}
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        revokeProjectAssetTokens={workspace.revokeProjectAssetTokens}
        openProjectFileExternal={workspace.openProjectFileExternal}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        moveProjectEntry={workspace.moveProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
        searchProject={workspace.searchProject}
        replaceInProject={workspace.replaceInProject}
      />,
    )

    await waitFor(() => expect(workspace.revokeProjectAssetTokens).toHaveBeenCalledWith('project-1'))
  })

  it('keeps large image previews URL-backed without source editor state', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/large-photo.png')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /large-photo.png' }))

    const preview = await screen.findByTestId('image-preview')
    expect(preview.querySelector('img')).toHaveAttribute(
      'src',
      'xero-asset://preview/large-photo.png',
    )
    expect(screen.getByTestId('preview-status-bar')).toHaveTextContent('128 MB')
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
  })

  it('renders PDF previews with fallback actions', async () => {
    const { workspace } = renderHostExecutionView()

    expect(await screen.findByTestId('file:/paper.pdf')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /paper.pdf' }))

    const preview = await screen.findByTestId('pdf-preview')
    const object = preview.querySelector('object')
    expect(object).toHaveAttribute('data', 'xero-asset://preview/paper.pdf')
    expect(object).toHaveAttribute('type', 'application/pdf')
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()

    fireEvent.click(screen.getAllByRole('button', { name: 'Open externally' })[0])
    await waitFor(() =>
      expect(workspace.openProjectFileExternal).toHaveBeenCalledWith('project-1', '/paper.pdf'),
    )
  })

  it('renders audio and video previews with native controls', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/theme.mp3')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /theme.mp3' }))

    const audioPreview = await screen.findByTestId('audio-preview')
    expect(audioPreview.querySelector('audio')).toHaveAttribute(
      'src',
      'xero-asset://preview/theme.mp3',
    )
    expect(audioPreview.querySelector('audio')).toHaveAttribute('controls')

    fireEvent.click(screen.getByRole('button', { name: 'Open /demo.mp4' }))
    const videoPreview = await screen.findByTestId('video-preview')
    expect(videoPreview.querySelector('video')).toHaveAttribute(
      'src',
      'xero-asset://preview/demo.mp4',
    )
    expect(videoPreview.querySelector('video')).toHaveAttribute('controls')
  })

  it('renders Markdown preview with GFM tables, highlighted code, and relative images', async () => {
    const { workspace } = renderHostExecutionView()

    expect(await screen.findByTestId('file:/docs/guide.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /docs/guide.md' }))

    expect(await screen.findByLabelText('Editor for /docs/guide.md')).toBeVisible()
    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))

    expect(await screen.findByTestId('markdown-preview')).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Guide' })).toBeVisible()
    expect(screen.getByRole('table')).toBeVisible()
    expect(screen.getByText('const preview = true')).toBeVisible()

    await waitFor(() =>
      expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/docs/logo.png'),
    )
    expect(await screen.findByAltText('Logo')).toHaveAttribute(
      'src',
      'xero-asset://preview/docs/logo.png',
    )
  })

  it('renders long Markdown previews without highlighting oversized code blocks', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/long.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /long.md' }))

    expect(await screen.findByLabelText('Editor for /long.md')).toBeVisible()
    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))

    expect(await screen.findByTestId('markdown-preview')).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Long doc' })).toBeVisible()
    expect(screen.getByText('Plain')).toBeVisible()
  })

  it('renders CSV preview and reflects unsaved source edits when toggled back to preview', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/data.csv')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /data.csv' }))

    const editor = await screen.findByLabelText('Editor for /data.csv')
    expect(editor).toHaveValue('name,count\nAlpha,1\n')

    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))
    expect(await screen.findByTestId('csv-preview')).toBeVisible()
    expect(screen.getByText('Alpha')).toBeVisible()
    expect(screen.getByText('count')).toBeVisible()

    fireEvent.click(screen.getByRole('radio', { name: 'Show source' }))
    fireEvent.change(await screen.findByLabelText('Editor for /data.csv'), {
      target: { value: 'name,count\nBeta,2\n' },
    })
    expect(screen.getByText('● Unsaved')).toBeVisible()

    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))
    expect(await screen.findByTestId('csv-preview')).toBeVisible()
    expect(screen.getByText('Beta')).toBeVisible()
    expect(screen.queryByText('Alpha')).not.toBeInTheDocument()
  })

  it('bounds large CSV previews to the row and column limits', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/large.csv')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /large.csv' }))

    expect(await screen.findByLabelText('Editor for /large.csv')).toBeVisible()
    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))

    const preview = await screen.findByTestId('csv-preview')
    expect(preview).toHaveTextContent('1,501 rows')
    expect(preview).toHaveTextContent('Preview limited to 1,000 rows and 80 columns')
    expect(screen.getByText('row-999')).toBeVisible()
    expect(screen.queryByText('row-1200')).not.toBeInTheDocument()
  })

  it('shows a metadata panel for unsupported binary files', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/archive.bin')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /archive.bin' }))

    const panel = await screen.findByTestId('unsupported-file-panel')
    expect(panel).toHaveTextContent('Xero cannot preview archive.bin')
    expect(panel).toHaveTextContent('binary')
    expect(panel).toHaveTextContent('application/octet-stream')
    expect(panel).toHaveTextContent('1.0 MB')
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
  })

  it('keeps preview mode per tab and isolates dirty state from other tabs', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    fireEvent.change(await screen.findByLabelText('Editor for /README.md'), {
      target: { value: '# dirty markdown\n' },
    })
    expect(screen.getByText('● Unsaved')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /photo.png' }))
    expect(await screen.findByTestId('image-preview')).toBeVisible()
    // Image tab should not show dirty/saving state since it's not text-backed
    expect(screen.queryByText('● Unsaved')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))
    expect(await screen.findByLabelText('Editor for /README.md')).toHaveValue('# dirty markdown\n')
    expect(screen.getByText('● Unsaved')).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('dirty')
    expect(screen.getByTestId('file:/photo.png')).toHaveTextContent('clean')
  })

  it('preserves unsaved SVG edits when toggling between source and preview', async () => {
    renderHostExecutionView()

    expect(await screen.findByTestId('file:/logo.svg')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /logo.svg' }))

    fireEvent.click(await screen.findByRole('radio', { name: 'Show source' }))
    const editor = await screen.findByLabelText('Editor for /logo.svg')
    fireEvent.change(editor, { target: { value: '<svg><rect /></svg>' } })
    expect(screen.getByText('● Unsaved')).toBeVisible()

    fireEvent.click(screen.getByRole('radio', { name: 'Show preview' }))
    expect(await screen.findByTestId('svg-preview')).toBeVisible()

    fireEvent.click(screen.getByRole('radio', { name: 'Show source' }))
    expect(await screen.findByLabelText('Editor for /logo.svg')).toHaveValue('<svg><rect /></svg>')
    expect(screen.getByText('● Unsaved')).toBeVisible()
  })
})
