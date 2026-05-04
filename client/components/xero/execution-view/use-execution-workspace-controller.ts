import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  createBackendRequestCoordinator,
  isStaleBackendRequestError,
  stableBackendRequestKey,
} from '@/src/lib/backend-request-coordinator'
import { getDesktopErrorMessage } from '@/src/lib/xero-desktop'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  ListProjectFilesResponseDto,
  MoveProjectEntryRequestDto,
  MoveProjectEntryResponseDto,
  ProjectFileRendererKindDto,
  ProjectRenderableRendererKindDto,
  ProjectTextRendererKindDto,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import {
  applyProjectFileListing,
  createEmptyProjectFileTreeStore,
  DEFAULT_PROJECT_FILE_TREE_STORE_MAX_BYTES,
  findNode,
  getProjectFileTreeBudgetInfo,
  isFolderLoaded,
  materializeProjectFileTree,
  trimProjectFileTreeStoreToBudget,
  type ProjectFileTreeBudgetInfo,
  type ProjectFileTreeStore,
  type FileSystemNode,
} from '@/src/lib/file-system-tree'
import { getLangFromPath } from '@/lib/language-detection'

const EXECUTION_TREE_REQUEST_SCOPE = 'execution-project-tree'
const EXECUTION_FILE_READ_REQUEST_SCOPE = 'execution-file-read'

interface CursorPosition {
  line: number
  column: number
}

interface RenameTarget {
  path: string
  type: 'file' | 'folder'
}

interface DeleteTarget {
  path: string
  type: 'file' | 'folder'
}

interface NewChildTarget {
  parentPath: string
  type: 'file' | 'folder'
}

interface UseExecutionWorkspaceControllerOptions {
  projectId: string
  active?: boolean
  listProjectFiles: (projectId: string, path?: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
}

function defaultExpandedFolders(root: FileSystemNode): Set<string> {
  return root.type === 'folder' ? new Set<string>(['/']) : new Set<string>()
}

function remapPath(candidate: string, oldBase: string, newBase: string): string {
  if (candidate === oldBase) return newBase
  if (candidate.startsWith(`${oldBase}/`)) return newBase + candidate.slice(oldBase.length)
  return candidate
}

function remapKeys<T>(record: Record<string, T>, oldBase: string, newBase: string): Record<string, T> {
  const next: Record<string, T> = {}
  for (const [key, value] of Object.entries(record)) {
    next[remapPath(key, oldBase, newBase)] = value
  }
  return next
}

function filterByPathNotWithin<T>(record: Record<string, T>, path: string, prefix: string): Record<string, T> {
  const next: Record<string, T> = {}
  for (const [key, value] of Object.entries(record)) {
    if (key === path || key.startsWith(prefix)) continue
    next[key] = value
  }
  return next
}

function filterByExactPaths<T>(record: Record<string, T>, paths: Set<string>): Record<string, T> {
  const next: Record<string, T> = {}
  for (const [key, value] of Object.entries(record)) {
    if (paths.has(key)) continue
    next[key] = value
  }
  return next
}

function childPath(parentPath: string, name: string): string {
  return parentPath === '/' ? `/${name}` : `${parentPath}/${name}`
}

function parentPathOf(path: string): string {
  const segments = path.split('/').filter(Boolean)
  if (segments.length <= 1) {
    return '/'
  }
  return `/${segments.slice(0, -1).join('/')}`
}

function splitEntryPath(value: string): string[] {
  return value
    .trim()
    .replace(/\\/g, '/')
    .split('/')
    .map((segment) => segment.trim())
    .filter(Boolean)
}

function countLines(value: string): number {
  return value.length === 0 ? 1 : value.split('\n').length
}

export type ProjectFileResource =
  | {
      kind: 'text'
      mimeType: string
      rendererKind: ProjectTextRendererKindDto
      byteLength: number
      modifiedAt: string
      contentHash: string
    }
  | {
      kind: 'renderable'
      mimeType: string
      rendererKind: ProjectRenderableRendererKindDto
      byteLength: number
      modifiedAt: string
      contentHash: string
      previewUrl: string
    }
  | {
      kind: 'unsupported'
      mimeType: string | null
      rendererKind: ProjectFileRendererKindDto | null
      byteLength: number
      modifiedAt: string
      contentHash: string
      reason: string
    }

function projectFileResourceFromResponse(response: ReadProjectFileResponseDto): ProjectFileResource {
  if (response.kind === 'text') {
    return {
      kind: 'text',
      mimeType: response.mimeType,
      rendererKind: response.rendererKind,
      byteLength: response.byteLength,
      modifiedAt: response.modifiedAt,
      contentHash: response.contentHash,
    }
  }

  if (response.kind === 'renderable') {
    return {
      kind: 'renderable',
      mimeType: response.mimeType,
      rendererKind: response.rendererKind,
      byteLength: response.byteLength,
      modifiedAt: response.modifiedAt,
      contentHash: response.contentHash,
      previewUrl: response.previewUrl,
    }
  }

  return {
    kind: 'unsupported',
    mimeType: response.mimeType,
    rendererKind: response.rendererKind ?? null,
    byteLength: response.byteLength,
    modifiedAt: response.modifiedAt,
    contentHash: response.contentHash,
    reason: response.reason,
  }
}

export function useExecutionWorkspaceController({
  projectId,
  active = true,
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  createProjectEntry,
  renameProjectEntry,
  moveProjectEntry,
  deleteProjectEntry,
}: UseExecutionWorkspaceControllerOptions) {
  const loadEpochRef = useRef(0)
  const treeRequestCoordinatorRef = useRef(createBackendRequestCoordinator())
  const fileReadRequestCoordinatorRef = useRef(createBackendRequestCoordinator())
  const pendingInitialTreeLoadRef = useRef<string | null>(projectId)

  const [treeStore, setTreeStoreState] = useState(createEmptyProjectFileTreeStore)
  const treeStoreRef = useRef(treeStore)
  const tree = useMemo(() => materializeProjectFileTree(treeStore), [treeStore])
  const [treeBudgetInfo, setTreeBudgetInfo] = useState<ProjectFileTreeBudgetInfo>({
    omittedEntryCount: 0,
    truncated: false,
  })
  const [savedContents, setSavedContents] = useState<Record<string, string>>({})
  const [fileContents, setFileContents] = useState<Record<string, string>>({})
  const [documentVersions, setDocumentVersions] = useState<Record<string, number>>({})
  const [lineCounts, setLineCounts] = useState<Record<string, number>>({})
  const [fileResources, setFileResources] = useState<Record<string, ProjectFileResource>>({})
  const [openTabs, setOpenTabs] = useState<string[]>([])
  const [activePath, setActivePath] = useState<string | null>(null)
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['/']))
  const [dirtyPaths, setDirtyPaths] = useState<Set<string>>(new Set())
  const [searchQuery, setSearchQuery] = useState('')
  const [cursor, setCursor] = useState<CursorPosition>({ line: 1, column: 1 })
  const [isTreeLoading, setIsTreeLoading] = useState(false)
  const [loadingFolders, setLoadingFolders] = useState<Set<string>>(new Set())
  const [pendingFilePath, setPendingFilePath] = useState<string | null>(null)
  const [savingPath, setSavingPath] = useState<string | null>(null)
  const [workspaceError, setWorkspaceError] = useState<string | null>(null)
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null)
  const [newChildTarget, setNewChildTarget] = useState<NewChildTarget | null>(null)

  const commitTreeStore = useCallback((nextStore: ProjectFileTreeStore) => {
    treeStoreRef.current = nextStore
    setTreeStoreState(nextStore)
  }, [])

  const openFile = useCallback((path: string) => {
    setOpenTabs((current) => (current.includes(path) ? current : [...current, path]))
    setActivePath(path)
  }, [])

  const refreshFolder = useCallback(
    async (path = '/', options: { preserveExpandedFolders?: boolean } = {}) => {
      const normalizedPath = path || '/'
      const isRootLoad = normalizedPath === '/'
      const requestEpoch = loadEpochRef.current
      if (isRootLoad) {
        setIsTreeLoading(true)
      }
      setLoadingFolders((current) => {
        const next = new Set(current)
        next.add(normalizedPath)
        return next
      })
      setWorkspaceError(null)

      try {
        const response = await treeRequestCoordinatorRef.current.runLatest(
          `${EXECUTION_TREE_REQUEST_SCOPE}:${normalizedPath}`,
          stableBackendRequestKey(['list_project_files', { path: normalizedPath, projectId }]),
          () => (isRootLoad ? listProjectFiles(projectId) : listProjectFiles(projectId, normalizedPath)),
        )
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        const nextStore = applyProjectFileListing(treeStoreRef.current, response)
        const budgetedStore = trimProjectFileTreeStoreToBudget(nextStore, {
          maxBytes: DEFAULT_PROJECT_FILE_TREE_STORE_MAX_BYTES,
          protectedPaths: [normalizedPath, activePath],
        }).store
        const nextTree = materializeProjectFileTree(budgetedStore)
        commitTreeStore(budgetedStore)
        setTreeBudgetInfo(getProjectFileTreeBudgetInfo(response))
        setExpandedFolders((current) => {
          if (isRootLoad && (!options.preserveExpandedFolders || current.size === 0)) {
            return defaultExpandedFolders(nextTree)
          }

          const next = new Set(Array.from(current).filter((path) => findNode(nextTree, path)?.type === 'folder'))
          if (next.size === 0) {
            const defaults = defaultExpandedFolders(nextTree)
            if (!isRootLoad) {
              defaults.add(normalizedPath)
            }
            return defaults
          }

          next.add('/')
          if (!isRootLoad) {
            next.add(normalizedPath)
          }
          return next
        })
        setOpenTabs((current) => current.filter((path) => findNode(nextTree, path)?.type === 'file'))
        setActivePath((current) => (current && findNode(nextTree, current)?.type === 'file' ? current : null))
      } catch (error) {
        if (isStaleBackendRequestError(error)) {
          return
        }
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        if (isRootLoad) {
          const emptyStore = createEmptyProjectFileTreeStore()
          commitTreeStore(emptyStore)
          setTreeBudgetInfo({ omittedEntryCount: 0, truncated: false })
          setOpenTabs([])
          setActivePath(null)
          setExpandedFolders(new Set(['/']))
        }
        setWorkspaceError(getDesktopErrorMessage(error))
      } finally {
        if (requestEpoch === loadEpochRef.current) {
          if (isRootLoad) {
            setIsTreeLoading(false)
          }
          setLoadingFolders((current) => {
            if (!current.has(normalizedPath)) return current
            const next = new Set(current)
            next.delete(normalizedPath)
            return next
          })
        }
      }
    },
    [activePath, commitTreeStore, listProjectFiles, projectId],
  )

  const refreshTree = useCallback(
    (options: { preserveExpandedFolders?: boolean } = {}) => refreshFolder('/', options),
    [refreshFolder],
  )

  const refreshFolderSet = useCallback(
    async (paths: Iterable<string>) => {
      const uniquePaths = Array.from(new Set(paths))
        .filter(Boolean)
        .sort((left, right) => left.split('/').length - right.split('/').length)
      for (const path of uniquePaths) {
        await refreshFolder(path, { preserveExpandedFolders: true })
      }
    },
    [refreshFolder],
  )

  useEffect(() => {
    loadEpochRef.current += 1
    treeRequestCoordinatorRef.current.cancelScope(EXECUTION_TREE_REQUEST_SCOPE)
    fileReadRequestCoordinatorRef.current.cancelScope(EXECUTION_FILE_READ_REQUEST_SCOPE)
    pendingInitialTreeLoadRef.current = projectId
    commitTreeStore(createEmptyProjectFileTreeStore())
    setTreeBudgetInfo({ omittedEntryCount: 0, truncated: false })
    setSavedContents({})
    setFileContents({})
    setDocumentVersions({})
    setLineCounts({})
    setFileResources({})
    setOpenTabs([])
    setActivePath(null)
    setExpandedFolders(new Set(['/']))
    setDirtyPaths(new Set())
    setSearchQuery('')
    setCursor({ line: 1, column: 1 })
    setPendingFilePath(null)
    setLoadingFolders(new Set())
    setSavingPath(null)
    setWorkspaceError(null)
    setRenameTarget(null)
    setDeleteTarget(null)
    setNewChildTarget(null)
  }, [commitTreeStore, projectId])

  useEffect(() => {
    if (!active || pendingInitialTreeLoadRef.current !== projectId) {
      return
    }

    pendingInitialTreeLoadRef.current = null
    void refreshTree({ preserveExpandedFolders: false })
  }, [active, projectId, refreshTree])

  const dropCachedPaths = useCallback((paths: Iterable<string>) => {
    const pathSet = new Set(paths)
    if (pathSet.size === 0) return

    setSavedContents((current) => filterByExactPaths(current, pathSet))
    setFileContents((current) => filterByExactPaths(current, pathSet))
    setDocumentVersions((current) => filterByExactPaths(current, pathSet))
    setLineCounts((current) => filterByExactPaths(current, pathSet))
    setFileResources((current) => filterByExactPaths(current, pathSet))
  }, [])

  const closeTab = useCallback(
    (path: string) => {
      setOpenTabs((current) => {
        const next = current.filter((candidate) => candidate !== path)
        if (activePath === path) {
          const index = current.indexOf(path)
          const neighbor = next[index] ?? next[index - 1] ?? null
          setActivePath(neighbor)
        }
        return next
      })
      setDirtyPaths((current) => {
        if (!current.has(path)) return current
        const next = new Set(current)
        next.delete(path)
        return next
      })
      dropCachedPaths([path])
    },
    [activePath, dropCachedPaths],
  )

  const handleSelectFile = useCallback(
    async (path: string, options: { force?: boolean } = {}) => {
      const node = findNode(tree, path)
      if ((node && node.type !== 'file') || !path.startsWith('/')) {
        return
      }

      if (!options.force && fileResources[path]) {
        openFile(path)
        return
      }

      const requestEpoch = loadEpochRef.current
      setPendingFilePath(path)
      setWorkspaceError(null)

      try {
        const response = await fileReadRequestCoordinatorRef.current.runLatest(
          EXECUTION_FILE_READ_REQUEST_SCOPE,
          stableBackendRequestKey(['read_project_file', { path, projectId }]),
          () => readProjectFile(projectId, path),
        )
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        const resource = projectFileResourceFromResponse(response)
        setFileResources((current) => ({ ...current, [path]: resource }))

        if (response.kind === 'text') {
          setSavedContents((current) => ({ ...current, [path]: response.text }))
          setFileContents((current) => ({ ...current, [path]: response.text }))
          setLineCounts((current) => ({ ...current, [path]: countLines(response.text) }))
        }

        openFile(path)
      } catch (error) {
        if (isStaleBackendRequestError(error)) {
          return
        }
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        setWorkspaceError(getDesktopErrorMessage(error))
      } finally {
        if (requestEpoch === loadEpochRef.current) {
          setPendingFilePath((current) => (current === path ? null : current))
        }
      }
    },
    [fileResources, openFile, projectId, readProjectFile, tree],
  )

  const handleToggleFolder = useCallback((path: string) => {
    const node = findNode(tree, path)
    if (node?.type !== 'folder') {
      return
    }

    const shouldLoad = !expandedFolders.has(path) && !isFolderLoaded(treeStoreRef.current, path)
    setExpandedFolders((current) => {
      const next = new Set(current)
      if (next.has(path)) {
        next.delete(path)
      } else {
        next.add(path)
      }
      return next
    })
    if (shouldLoad) {
      void refreshFolder(path, { preserveExpandedFolders: true })
    }
  }, [expandedFolders, refreshFolder, tree])

  const handleSnapshotChange = useCallback(
    (value: string) => {
      if (!activePath) {
        return
      }

      setFileContents((current) => {
        if (current[activePath] === value) {
          return current
        }
        return { ...current, [activePath]: value }
      })
      setLineCounts((current) => {
        const nextLineCount = countLines(value)
        if (current[activePath] === nextLineCount) {
          return current
        }
        return { ...current, [activePath]: nextLineCount }
      })

      setDirtyPaths((current) => {
        const savedValue = savedContents[activePath] ?? ''
        const isDirty = value !== savedValue
        if (isDirty === current.has(activePath)) {
          return current
        }

        const next = new Set(current)
        if (isDirty) next.add(activePath)
        else next.delete(activePath)
        return next
      })
    },
    [activePath, savedContents],
  )

  const handleDirtyChange = useCallback(
    (isDirty: boolean) => {
      if (!activePath) {
        return
      }

      setDirtyPaths((current) => {
        if (isDirty === current.has(activePath)) {
          return current
        }

        const next = new Set(current)
        if (isDirty) next.add(activePath)
        else next.delete(activePath)
        return next
      })
    },
    [activePath],
  )

  const handleDocumentStatsChange = useCallback(
    ({ lineCount }: { lineCount: number }) => {
      if (!activePath) {
        return
      }

      setLineCounts((current) => {
        if (current[activePath] === lineCount) {
          return current
        }
        return { ...current, [activePath]: lineCount }
      })
    },
    [activePath],
  )

  const bumpDocumentVersion = useCallback((path: string) => {
    setDocumentVersions((current) => ({ ...current, [path]: (current[path] ?? 0) + 1 }))
  }, [])

  const saveActive = useCallback(async (snapshot?: string) => {
    if (!activePath) {
      return
    }

    if (fileResources[activePath]?.kind !== 'text') {
      return
    }

    const requestEpoch = loadEpochRef.current
    const path = activePath
    const content = snapshot ?? fileContents[path] ?? ''
    setFileContents((current) => {
      if (current[path] === content) {
        return current
      }
      return { ...current, [path]: content }
    })
    setLineCounts((current) => {
      const nextLineCount = countLines(content)
      if (current[path] === nextLineCount) {
        return current
      }
      return { ...current, [path]: nextLineCount }
    })
    setSavingPath(path)
    setWorkspaceError(null)

    try {
      await writeProjectFile(projectId, path, content)
      if (requestEpoch !== loadEpochRef.current) {
        return
      }

      setSavedContents((current) => ({ ...current, [path]: content }))
      setDirtyPaths((current) => {
        if (!current.has(path)) return current
        const next = new Set(current)
        next.delete(path)
        return next
      })
    } catch (error) {
      if (requestEpoch !== loadEpochRef.current) {
        return
      }

      setWorkspaceError(getDesktopErrorMessage(error))
    } finally {
      if (requestEpoch === loadEpochRef.current) {
        setSavingPath((current) => (current === path ? null : current))
      }
    }
  }, [activePath, fileContents, fileResources, projectId, writeProjectFile])

  const revertActive = useCallback(() => {
    if (!activePath) {
      return
    }

    if (fileResources[activePath]?.kind !== 'text') {
      return
    }

    const savedValue = savedContents[activePath] ?? ''
    setFileContents((current) => ({ ...current, [activePath]: savedValue }))
    setLineCounts((current) => ({ ...current, [activePath]: countLines(savedValue) }))
    bumpDocumentVersion(activePath)
    setDirtyPaths((current) => {
      if (!current.has(activePath)) return current
      const next = new Set(current)
      next.delete(activePath)
      return next
    })
  }, [activePath, bumpDocumentVersion, fileResources, savedContents])

  const reloadProjectTree = useCallback(() => {
    const cleanOpenTabs = openTabs.filter((path) => !dirtyPaths.has(path))
    dropCachedPaths(cleanOpenTabs)
    void refreshTree({ preserveExpandedFolders: true }).then(() => {
      if (activePath && cleanOpenTabs.includes(activePath)) {
        void handleSelectFile(activePath, { force: true })
      }
    })
  }, [activePath, dirtyPaths, dropCachedPaths, handleSelectFile, openTabs, refreshTree])

  const collapseAll = useCallback(() => {
    setExpandedFolders(new Set(['/']))
  }, [])

  const handleRequestRename = useCallback((path: string, type: 'file' | 'folder') => {
    setRenameTarget({ path, type })
  }, [])

  const handleRequestDelete = useCallback((path: string, type: 'file' | 'folder') => {
    setDeleteTarget({ path, type })
  }, [])

  const handleRequestNewFile = useCallback((parentPath: string) => {
    setExpandedFolders((current) => {
      const next = new Set(current)
      next.add(parentPath)
      return next
    })
    if (!isFolderLoaded(treeStoreRef.current, parentPath)) {
      void refreshFolder(parentPath, { preserveExpandedFolders: true })
    }
    setNewChildTarget({ parentPath, type: 'file' })
  }, [refreshFolder])

  const handleRequestNewFolder = useCallback((parentPath: string) => {
    setExpandedFolders((current) => {
      const next = new Set(current)
      next.add(parentPath)
      return next
    })
    if (!isFolderLoaded(treeStoreRef.current, parentPath)) {
      void refreshFolder(parentPath, { preserveExpandedFolders: true })
    }
    setNewChildTarget({ parentPath, type: 'folder' })
  }, [refreshFolder])

  const handleCopyPath = useCallback((path: string) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(path).catch(() => {})
    }
  }, [])

  const handleRenameSubmit = useCallback(
    async (newName: string): Promise<string | null> => {
      if (!renameTarget) {
        return null
      }

      try {
        const response = await renameProjectEntry({
          projectId,
          path: renameTarget.path,
          newName,
        })
        const { path: oldPath } = renameTarget
        const newPath = response.path

        setSavedContents((current) => remapKeys(current, oldPath, newPath))
        setFileContents((current) => remapKeys(current, oldPath, newPath))
        setDocumentVersions((current) => remapKeys(current, oldPath, newPath))
        setLineCounts((current) => remapKeys(current, oldPath, newPath))
        setFileResources((current) => remapKeys(current, oldPath, newPath))
        setOpenTabs((current) => current.map((path) => remapPath(path, oldPath, newPath)))
        setDirtyPaths((current) => new Set(Array.from(current).map((path) => remapPath(path, oldPath, newPath))))
        setExpandedFolders((current) => new Set(Array.from(current).map((path) => remapPath(path, oldPath, newPath))))
        setActivePath((current) => (current ? remapPath(current, oldPath, newPath) : null))
        setWorkspaceError(null)
        await refreshFolderSet([
          parentPathOf(oldPath),
          ...Array.from(expandedFolders).map((path) => remapPath(path, oldPath, newPath)),
        ])
        return null
      } catch (error) {
        return getDesktopErrorMessage(error)
      }
    },
    [expandedFolders, projectId, refreshFolderSet, renameProjectEntry, renameTarget],
  )

  const handleDeleteSubmit = useCallback(async () => {
    if (!deleteTarget) {
      return
    }

    const deletedPath = deleteTarget.path
    const deletedPrefix = deletedPath.endsWith('/') ? deletedPath : `${deletedPath}/`

    try {
      await deleteProjectEntry(projectId, deletedPath)
      setSavedContents((current) => filterByPathNotWithin(current, deletedPath, deletedPrefix))
      setFileContents((current) => filterByPathNotWithin(current, deletedPath, deletedPrefix))
      setDocumentVersions((current) => filterByPathNotWithin(current, deletedPath, deletedPrefix))
      setLineCounts((current) => filterByPathNotWithin(current, deletedPath, deletedPrefix))
      setFileResources((current) => filterByPathNotWithin(current, deletedPath, deletedPrefix))
      setOpenTabs((current) => current.filter((path) => path !== deletedPath && !path.startsWith(deletedPrefix)))
      setDirtyPaths((current) => {
        const next = new Set<string>()
        for (const path of current) {
          if (path !== deletedPath && !path.startsWith(deletedPrefix)) {
            next.add(path)
          }
        }
        return next
      })
      setActivePath((current) => (current === deletedPath || current?.startsWith(deletedPrefix) ? null : current))
      setWorkspaceError(null)
      setDeleteTarget(null)
      await refreshFolderSet([parentPathOf(deletedPath)])
    } catch (error) {
      setWorkspaceError(getDesktopErrorMessage(error))
    }
  }, [deleteProjectEntry, deleteTarget, projectId, refreshFolderSet])

  const handleCreateSubmit = useCallback(
    async (name: string): Promise<string | null> => {
      if (!newChildTarget) {
        return null
      }

      const { parentPath, type } = newChildTarget
      const segments = splitEntryPath(name)
      if (segments.length === 0) {
        return 'Name cannot be empty'
      }
      if (name.trim().endsWith('/') && type === 'file') {
        return 'File paths must end with a file name'
      }

      try {
        let currentParentPath = parentPath
        const folderSegments = type === 'folder' ? segments : segments.slice(0, -1)
        const expandedPaths = new Set<string>([parentPath])

        for (const segment of folderSegments) {
          const nextPath = childPath(currentParentPath, segment)
          const existingNode = findNode(tree, nextPath)

          if (existingNode) {
            if (existingNode.type !== 'folder') {
              return `Xero cannot create inside \`${nextPath}\` because that path is a file.`
            }
            currentParentPath = nextPath
            expandedPaths.add(nextPath)
            continue
          }

          const response = await createProjectEntry({
            projectId,
            parentPath: currentParentPath,
            name: segment,
            entryType: 'folder',
          })
          currentParentPath = response.path
          expandedPaths.add(response.path)
        }

        let createdFilePath: string | null = null
        if (type === 'file') {
          const fileName = segments[segments.length - 1]
          const response = await createProjectEntry({
            projectId,
            parentPath: currentParentPath,
            name: fileName,
            entryType: 'file',
          })
          createdFilePath = response.path

          setSavedContents((current) => ({ ...current, [response.path]: '' }))
          setFileContents((current) => ({ ...current, [response.path]: '' }))
          setLineCounts((current) => ({ ...current, [response.path]: 1 }))
          setFileResources((current) => ({
            ...current,
            [response.path]: {
              kind: 'text',
              mimeType: 'text/plain; charset=utf-8',
              rendererKind: 'code',
              byteLength: 0,
              modifiedAt: new Date(0).toISOString(),
              contentHash: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
            },
          }))
          openFile(response.path)
        }

        setExpandedFolders((current) => {
          const next = new Set(current)
          for (const path of expandedPaths) {
            next.add(path)
          }
          if (createdFilePath) next.add(currentParentPath)
          return next
        })
        setWorkspaceError(null)
        setNewChildTarget(null)
        await refreshFolderSet(expandedPaths)
        return null
      } catch (error) {
        return getDesktopErrorMessage(error)
      }
    },
    [createProjectEntry, newChildTarget, openFile, projectId, refreshFolderSet, tree],
  )

  const handleMoveEntry = useCallback(
    async (path: string, targetParentPath: string): Promise<void> => {
      if (path === targetParentPath || targetParentPath.startsWith(`${path}/`)) {
        return
      }

      try {
        const response = await moveProjectEntry({
          projectId,
          path,
          targetParentPath,
        })
        const newPath = response.path

        setSavedContents((current) => remapKeys(current, path, newPath))
        setFileContents((current) => remapKeys(current, path, newPath))
        setDocumentVersions((current) => remapKeys(current, path, newPath))
        setLineCounts((current) => remapKeys(current, path, newPath))
        setFileResources((current) => remapKeys(current, path, newPath))
        setOpenTabs((current) => current.map((candidate) => remapPath(candidate, path, newPath)))
        setDirtyPaths((current) => new Set(Array.from(current).map((candidate) => remapPath(candidate, path, newPath))))
        setExpandedFolders((current) => {
          const next = new Set(Array.from(current).map((candidate) => remapPath(candidate, path, newPath)))
          next.add(targetParentPath)
          return next
        })
        setActivePath((current) => (current ? remapPath(current, path, newPath) : null))
        setWorkspaceError(null)
        await refreshFolderSet([
          parentPathOf(path),
          targetParentPath,
          ...Array.from(expandedFolders).map((candidate) => remapPath(candidate, path, newPath)),
        ])
      } catch (error) {
        setWorkspaceError(getDesktopErrorMessage(error))
      }
    },
    [expandedFolders, moveProjectEntry, projectId, refreshFolderSet],
  )

  const activeNode = useMemo(() => (activePath ? findNode(tree, activePath) : null), [activePath, tree])
  const activeResource = activePath ? fileResources[activePath] ?? null : null
  const activeContent = activePath ? fileContents[activePath] ?? '' : ''
  const activeSavedContent = activePath ? savedContents[activePath] ?? '' : ''
  const activeDocumentVersion = activePath ? documentVersions[activePath] ?? 0 : 0
  const activeLang = activePath ? getLangFromPath(activePath) ?? 'plaintext' : 'plaintext'
  const activeLineCount = activePath ? lineCounts[activePath] ?? countLines(fileContents[activePath] ?? '') : 0
  const isActiveDirty = activePath ? dirtyPaths.has(activePath) : false
  const isActiveSaving = activePath ? savingPath === activePath : false
  const isActiveLoading = activePath ? pendingFilePath === activePath : false
  const isActiveText = activeResource?.kind === 'text'

  return {
    tree,
    openTabs,
    activePath,
    setActivePath,
    expandedFolders,
    dirtyPaths,
    searchQuery,
    setSearchQuery,
    cursor,
    setCursor,
    isTreeLoading,
    loadingFolders,
    pendingFilePath,
    savingPath,
    workspaceError,
    treeBudgetInfo,
    renameTarget,
    setRenameTarget,
    deleteTarget,
    setDeleteTarget,
    newChildTarget,
    setNewChildTarget,
    activeNode,
    activeResource,
    activeContent,
    activeSavedContent,
    activeDocumentVersion,
    activeLang,
    activeLineCount,
    isActiveDirty,
    isActiveSaving,
    isActiveLoading,
    isActiveText,
    closeTab,
    handleSelectFile,
    handleToggleFolder,
    handleSnapshotChange,
    handleDirtyChange,
    handleDocumentStatsChange,
    saveActive,
    revertActive,
    reloadProjectTree,
    collapseAll,
    handleRequestRename,
    handleRequestDelete,
    handleRequestNewFile,
    handleRequestNewFolder,
    handleMoveEntry,
    handleCopyPath,
    handleRenameSubmit,
    handleDeleteSubmit,
    handleCreateSubmit,
  }
}
