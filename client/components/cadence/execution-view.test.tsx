import type { ComponentProps } from 'react'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ExecutionPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { ListProjectFilesResponseDto, ReadProjectFileResponseDto } from '@/src/lib/cadence-model'

vi.mock('./code-editor', () => ({
  CodeEditor: ({ filePath, onChange, onSave, value }: any) => (
    <div>
      <label>
        <span className="sr-only">Editor for {filePath}</span>
        <textarea
          aria-label={`Editor for ${filePath}`}
          onChange={(event) => onChange(event.target.value)}
          value={value}
        />
      </label>
      <button onClick={onSave} type="button">
        Trigger save
      </button>
    </div>
  ),
}))

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
      onSelectFile,
      onToggleFolder,
      onRequestRename,
      onRequestDelete,
      onRequestNewFile,
      onRequestNewFolder,
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
              </div>
            ) : (
              <div data-testid={`file:${node.path}`} key={node.path}>
                <span>
                  file {node.path} {selectedPath === node.path ? 'selected' : 'idle'}{' '}
                  {dirtyPaths?.has(node.path) ? 'dirty' : 'clean'}
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

function makeExecution(projectId = 'project-1', name = 'Cadence'): ExecutionPaneView {
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
  let currentFileContents = {
    '/README.md': '# Cadence\n',
    '/src/main.tsx': 'console.log("hello")\n',
    ...(options?.fileContents ?? {}),
  }

  const listProjectFiles = vi.fn(async (projectId: string) => ({
    projectId,
    root: cloneNode(currentRoot),
  }))
  const readProjectFile = vi.fn(async (projectId: string, path: string) => ({
    projectId,
    path,
    content: currentFileContents[path] ?? '',
  }))
  const writeProjectFile = vi.fn(async (projectId: string, path: string, content: string) => {
    currentFileContents[path] = content
    return { projectId, path }
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
  const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => {
    currentRoot = deletePath(currentRoot, path)
    currentFileContents = removeContentKeys(currentFileContents, path)
    return { projectId, path }
  })

  return {
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
  }
}

function renderExecutionView(
  overrides: Partial<ComponentProps<typeof ExecutionView>> = {},
) {
  const workspace = createWorkspaceHarness()
  const props: ComponentProps<typeof ExecutionView> = {
    execution: makeExecution(),
    listProjectFiles: workspace.listProjectFiles,
    readProjectFile: workspace.readProjectFile,
    writeProjectFile: workspace.writeProjectFile,
    createProjectEntry: workspace.createProjectEntry,
    renameProjectEntry: workspace.renameProjectEntry,
    deleteProjectEntry: workspace.deleteProjectEntry,
    ...overrides,
  }

  return {
    workspace,
    ...render(<ExecutionView {...props} />),
  }
}

describe('ExecutionView', () => {
  it('opens files, tracks dirty state, reverts edits, and saves through the workspace controller', async () => {
    const { workspace } = renderExecutionView()

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))

    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    const editor = await screen.findByLabelText('Editor for /README.md')
    expect(editor).toHaveValue('# Cadence\n')
    expect(screen.getByText('Saved')).toBeVisible()

    fireEvent.change(editor, { target: { value: '# Cadence\nUpdated\n' } })

    expect(screen.getByText('● Unsaved')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Revert' })).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('dirty')

    fireEvent.click(screen.getByRole('button', { name: 'Revert' }))

    await waitFor(() => expect(screen.getByLabelText('Editor for /README.md')).toHaveValue('# Cadence\n'))
    expect(screen.getByText('Saved')).toBeVisible()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('clean')

    fireEvent.change(screen.getByLabelText('Editor for /README.md'), { target: { value: '# Saved\n' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(workspace.writeProjectFile).toHaveBeenCalledWith('project-1', '/README.md', '# Saved\n'))
    await waitFor(() => expect(screen.getByText('Saved')).toBeVisible())
    expect(screen.queryByRole('button', { name: 'Revert' })).not.toBeInTheDocument()
    expect(screen.getByTestId('file:/README.md')).toHaveTextContent('clean')
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
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
      />,
    )

    expect(await screen.findByTestId('folder:/src')).toHaveTextContent('expanded')

    fireEvent.click(screen.getByRole('button', { name: 'Open /src/main.tsx' }))
    await waitFor(() => expect(workspace.readProjectFile).toHaveBeenCalledWith('project-1', '/src/main.tsx'))

    fireEvent.change(await screen.findByLabelText('Editor for /src/main.tsx'), {
      target: { value: 'console.log("dirty")\n' },
    })
    expect(screen.getByTestId('file:/src/main.tsx')).toHaveTextContent('dirty')

    fireEvent.click(screen.getByRole('button', { name: 'New file in /src' }))
    fireEvent.change(screen.getByPlaceholderText('filename.ext'), { target: { value: 'notes.md' } })
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
    expect(screen.getByTestId('file:/app/main.tsx')).toHaveTextContent('selected dirty')
    expect(screen.getByLabelText('Editor for /app/main.tsx')).toHaveValue('console.log("dirty")\n')
    expect(screen.getByRole('button', { name: 'Close main.tsx' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Close notes.md' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Open /app/main.tsx' }))
    expect(screen.getByLabelText('Editor for /app/main.tsx')).toHaveValue('console.log("dirty")\n')
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Open /app/notes.md' }))
    expect(screen.getByLabelText('Editor for /app/notes.md')).toHaveValue('')
    expect(workspace.readProjectFile).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Delete /app/notes.md' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))

    await waitFor(() => expect(workspace.deleteProjectEntry).toHaveBeenCalledWith('project-1', '/app/notes.md'))
    await waitFor(() => expect(screen.queryByTestId('file:/app/notes.md')).not.toBeInTheDocument())
    expect(screen.queryByRole('button', { name: 'Close notes.md' })).not.toBeInTheDocument()
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
  })

  it('ignores stale file reads after the selected project changes', async () => {
    const slowRead = createDeferred<ReadProjectFileResponseDto>()
    const listProjectFiles = vi.fn(async (projectId: string) => ({
      projectId,
      root:
        projectId === 'project-1'
          ? folder('root', '/', [file('README.md', '/README.md')])
          : folder('root', '/', [file('app.py', '/app.py')]),
    }))
    const readProjectFile = vi.fn((projectId: string, path: string) => {
      if (projectId === 'project-1') {
        return slowRead.promise
      }

      return Promise.resolve({
        projectId,
        path,
        content: 'print("project two")\n',
      })
    })
    const writeProjectFile = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))
    const createProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(request.parentPath, request.name),
    }))
    const renameProjectEntry = vi.fn(async (request) => ({
      projectId: request.projectId,
      path: joinPath(parentPathOf(request.path), request.newName),
    }))
    const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))

    const { rerender } = render(
      <ExecutionView
        execution={makeExecution('project-1', 'Project One')}
        listProjectFiles={listProjectFiles}
        readProjectFile={readProjectFile}
        writeProjectFile={writeProjectFile}
        createProjectEntry={createProjectEntry}
        renameProjectEntry={renameProjectEntry}
        deleteProjectEntry={deleteProjectEntry}
      />,
    )

    expect(await screen.findByTestId('file:/README.md')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /README.md' }))

    await waitFor(() => expect(readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))

    rerender(
      <ExecutionView
        execution={makeExecution('project-2', 'Project Two')}
        listProjectFiles={listProjectFiles}
        readProjectFile={readProjectFile}
        writeProjectFile={writeProjectFile}
        createProjectEntry={createProjectEntry}
        renameProjectEntry={renameProjectEntry}
        deleteProjectEntry={deleteProjectEntry}
      />,
    )

    expect(await screen.findByTestId('file:/app.py')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Open /app.py' }))

    await waitFor(() => expect(readProjectFile).toHaveBeenCalledWith('project-2', '/app.py'))
    await waitFor(() => expect(screen.getByLabelText('Editor for /app.py')).toHaveValue('print("project two")\n'))

    await act(async () => {
      slowRead.resolve({
        projectId: 'project-1',
        path: '/README.md',
        content: '# stale response\n',
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
        listProjectFiles={workspace.listProjectFiles}
        readProjectFile={workspace.readProjectFile}
        writeProjectFile={workspace.writeProjectFile}
        createProjectEntry={workspace.createProjectEntry}
        renameProjectEntry={workspace.renameProjectEntry}
        deleteProjectEntry={workspace.deleteProjectEntry}
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
})
