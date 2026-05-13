import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  createBackendRequestCoordinator,
  isStaleBackendRequestError,
  listProjectFilesRequestKey,
  readProjectFileRequestKey,
} from '@/src/lib/backend-request-coordinator'
import { XeroDesktopError, getDesktopErrorMessage } from '@/src/lib/xero-desktop'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  ListProjectFilesResponseDto,
  MoveProjectEntryRequestDto,
  MoveProjectEntryResponseDto,
  ProjectFileRendererKindDto,
  ProjectFilePreviewDto,
  ProjectRenderableRendererKindDto,
  ProjectTextRendererKindDto,
  ProjectUiStateResponseDto,
  ReadProjectFileResponseDto,
  ReadProjectUiStateRequestDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  StatProjectFilesResponseDto,
  WriteProjectUiStateRequestDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import {
  applyProjectFileListing,
  createEmptyProjectFileTreeStore,
  findNode,
  getProjectFileTreeBudgetInfo,
  isFolderLoaded,
  materializeProjectFileTree,
  type ProjectFileTreeBudgetInfo,
  type ProjectFileTreeStore,
  type FileSystemNode,
} from '@/src/lib/file-system-tree'
import { getLangFromPath } from '@/lib/language-detection'
import {
  detectDocumentSettings,
  normalizeToLf,
  serializeWithSettings,
  type DocumentSettings,
} from '@/lib/document-settings'

const EXECUTION_TREE_REQUEST_SCOPE = 'execution-project-tree'
const EXECUTION_FILE_READ_REQUEST_SCOPE = 'execution-file-read'
const EDITOR_DRAFTS_UI_STATE_KEY = 'editor.drafts:v1'
const DRAFT_PERSIST_DEBOUNCE_MS = 400
const OPEN_FILE_STAT_POLL_MS = 2500
const TREE_REFRESH_POLL_MS = 7000

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
  writeProjectFile: (
    projectId: string,
    path: string,
    content: string,
    options?: {
      expectedContentHash?: string
      expectedModifiedAt?: string
      overwrite?: boolean
    },
  ) => Promise<WriteProjectFileResponseDto>
  statProjectFiles?: (projectId: string, paths: string[]) => Promise<StatProjectFilesResponseDto>
  readProjectUiState?: (request: ReadProjectUiStateRequestDto) => Promise<ProjectUiStateResponseDto>
  writeProjectUiState?: (request: WriteProjectUiStateRequestDto) => Promise<ProjectUiStateResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
}

interface StaleOpenFileState {
  kind: 'changed' | 'deleted'
  detectedAt: string
}

interface SaveConflictState {
  path: string
  mine: string
  disk: string
  diskResource: Extract<ProjectFileResource, { kind: 'text' }>
  detectedAt: string
}

type DirtyGuardOperation =
  | { kind: 'close'; path: string }
  | { kind: 'reload' }
  | { kind: 'rename'; path: string; entryType: 'file' | 'folder' }
  | { kind: 'delete'; path: string; entryType: 'file' | 'folder' }
  | { kind: 'close-others' }
  | { kind: 'save-all' }

interface RefreshFolderOptions {
  preserveExpandedFolders?: boolean
  reportErrors?: boolean
  showLoadingState?: boolean
}

interface DirtyGuardState {
  operation: DirtyGuardOperation
  paths: string[]
}

interface PersistedEditorDrafts {
  schema: 'xero.editor.drafts.v1'
  projectId: string
  updatedAt: string
  drafts: Record<
    string,
    {
      content: string
      savedContentHash?: string
      savedModifiedAt?: string
      updatedAt: string
    }
  >
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

function ancestorFolderPaths(path: string): string[] {
  const segments = path.split('/').filter(Boolean)
  if (segments.length <= 1) {
    return ['/']
  }

  const ancestors = ['/']
  for (let index = 1; index < segments.length; index += 1) {
    ancestors.push(`/${segments.slice(0, index).join('/')}`)
  }
  return ancestors
}

function shouldKeepOpenPath(root: FileSystemNode, path: string): boolean {
  const node = findNode(root, path)
  if (node) return node.type === 'file'

  const parent = findNode(root, parentPathOf(path))
  return !(parent?.type === 'folder' && parent.childrenLoaded)
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
      preview?: ProjectFilePreviewDto | null
      documentSettings: DocumentSettings
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
      preview: response.preview ?? null,
      documentSettings: detectDocumentSettings(response.text),
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

function projectFileResourceFromWriteResponse(
  response: WriteProjectFileResponseDto,
  fallback: ProjectFileResource,
): ProjectFileResource {
  if (isTextRendererKind(response.rendererKind)) {
    return {
      kind: 'text',
      mimeType: response.mimeType,
      rendererKind: response.rendererKind,
      byteLength: response.byteLength,
      modifiedAt: response.modifiedAt,
      contentHash: response.contentHash,
      preview: response.preview ?? null,
      documentSettings:
        fallback.kind === 'text'
          ? fallback.documentSettings
          : detectDocumentSettings(''),
    }
  }

  return {
    ...fallback,
    byteLength: response.byteLength,
    modifiedAt: response.modifiedAt,
    contentHash: response.contentHash,
    mimeType: response.mimeType,
    rendererKind: response.rendererKind,
  } as ProjectFileResource
}

function isTextRendererKind(value: ProjectFileRendererKindDto): value is ProjectTextRendererKindDto {
  return value === 'code' || value === 'svg' || value === 'markdown' || value === 'csv' || value === 'html'
}

function dirtyPathsWithin(dirtyPaths: Set<string>, path: string, type: 'file' | 'folder'): string[] {
  if (type === 'file') {
    return dirtyPaths.has(path) ? [path] : []
  }

  const prefix = path.endsWith('/') ? path : `${path}/`
  return Array.from(dirtyPaths)
    .filter((candidate) => candidate === path || candidate.startsWith(prefix))
    .sort()
}

function isPersistedEditorDrafts(value: unknown, projectId: string): value is PersistedEditorDrafts {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Partial<PersistedEditorDrafts>
  return candidate.schema === 'xero.editor.drafts.v1' && candidate.projectId === projectId && typeof candidate.drafts === 'object'
}

export function useExecutionWorkspaceController({
  projectId,
  active = true,
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  statProjectFiles,
  readProjectUiState,
  writeProjectUiState,
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
  const fileResourcesRef = useRef(fileResources)
  const [openTabs, setOpenTabs] = useState<string[]>([])
  const openTabsRef = useRef(openTabs)
  const [activePath, setActivePath] = useState<string | null>(null)
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['/']))
  const expandedFoldersRef = useRef(expandedFolders)
  const [dirtyPaths, setDirtyPaths] = useState<Set<string>>(new Set())
  const dirtyPathsRef = useRef(dirtyPaths)
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
  const [stalePaths, setStalePaths] = useState<Record<string, StaleOpenFileState>>({})
  const [saveConflict, setSaveConflict] = useState<SaveConflictState | null>(null)
  const [dirtyGuard, setDirtyGuard] = useState<DirtyGuardState | null>(null)
  const [draftsHydrated, setDraftsHydrated] = useState(false)

  fileResourcesRef.current = fileResources
  openTabsRef.current = openTabs
  expandedFoldersRef.current = expandedFolders
  dirtyPathsRef.current = dirtyPaths
  const fileContentsRef = useRef(fileContents)
  fileContentsRef.current = fileContents

  const commitTreeStore = useCallback((nextStore: ProjectFileTreeStore) => {
    treeStoreRef.current = nextStore
    setTreeStoreState(nextStore)
  }, [])

  const openFile = useCallback((path: string) => {
    setOpenTabs((current) => (current.includes(path) ? current : [...current, path]))
    setActivePath(path)
  }, [])

  const refreshFolder = useCallback(
    async (path = '/', options: RefreshFolderOptions = {}) => {
      const normalizedPath = path || '/'
      const isRootLoad = normalizedPath === '/'
      const showLoadingState = options.showLoadingState ?? true
      const reportErrors = options.reportErrors ?? true
      const requestEpoch = loadEpochRef.current
      if (showLoadingState && isRootLoad) {
        setIsTreeLoading(true)
      }
      if (showLoadingState) {
        setLoadingFolders((current) => {
          const next = new Set(current)
          next.add(normalizedPath)
          return next
        })
      }
      if (reportErrors) {
        setWorkspaceError(null)
      }

      try {
        const response = await treeRequestCoordinatorRef.current.runLatest(
          `${EXECUTION_TREE_REQUEST_SCOPE}:${normalizedPath}`,
          listProjectFilesRequestKey(projectId, normalizedPath),
          () => (isRootLoad ? listProjectFiles(projectId) : listProjectFiles(projectId, normalizedPath)),
        )
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        const previousStore = treeStoreRef.current
        const nextStore = applyProjectFileListing(previousStore, response)
        const nextBudgetInfo = getProjectFileTreeBudgetInfo(response)
        setTreeBudgetInfo((current) =>
          current.omittedEntryCount === nextBudgetInfo.omittedEntryCount &&
          current.truncated === nextBudgetInfo.truncated
            ? current
            : nextBudgetInfo,
        )
        if (nextStore === previousStore) {
          return
        }

        const nextTree = materializeProjectFileTree(nextStore)
        commitTreeStore(nextStore)
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
        setOpenTabs((current) => current.filter((path) => shouldKeepOpenPath(nextTree, path)))
        setActivePath((current) => (current && shouldKeepOpenPath(nextTree, current) ? current : null))
      } catch (error) {
        if (isStaleBackendRequestError(error)) {
          return
        }
        if (requestEpoch !== loadEpochRef.current) {
          return
        }
        if (!reportErrors) {
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
          if (showLoadingState && isRootLoad) {
            setIsTreeLoading(false)
          }
          if (showLoadingState) {
            setLoadingFolders((current) => {
              if (!current.has(normalizedPath)) return current
              const next = new Set(current)
              next.delete(normalizedPath)
              return next
            })
          }
        }
      }
    },
    [activePath, commitTreeStore, listProjectFiles, projectId],
  )

  const refreshTree = useCallback(
    (options: RefreshFolderOptions = {}) => refreshFolder('/', options),
    [refreshFolder],
  )

  const refreshFolderSet = useCallback(
    async (paths: Iterable<string>, options: RefreshFolderOptions = {}) => {
      const uniquePaths = Array.from(new Set(paths))
        .filter(Boolean)
        .sort((left, right) => left.split('/').length - right.split('/').length)
      for (const path of uniquePaths) {
        await refreshFolder(path, { ...options, preserveExpandedFolders: true })
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
    setStalePaths({})
    setSaveConflict(null)
    setDirtyGuard(null)
    setDraftsHydrated(false)
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

  const closeTabNow = useCallback(
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

  const closeTab = useCallback(
    (path: string) => {
      if (dirtyPathsRef.current.has(path)) {
        setDirtyGuard({ operation: { kind: 'close', path }, paths: [path] })
        return
      }
      closeTabNow(path)
    },
    [closeTabNow],
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
          readProjectFileRequestKey(projectId, path),
          () => readProjectFile(projectId, path),
        )
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        const resource = projectFileResourceFromResponse(response)
        setFileResources((current) => ({ ...current, [path]: resource }))
        setStalePaths((current) => {
          if (!current[path]) return current
          const next = { ...current }
          delete next[path]
          return next
        })

        if (response.kind === 'text') {
          const normalized = normalizeToLf(response.text)
          setSavedContents((current) => ({ ...current, [path]: normalized }))
          setFileContents((current) => ({ ...current, [path]: normalized }))
          setLineCounts((current) => ({ ...current, [path]: countLines(normalized) }))
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
    (value: string, pathOverride?: string) => {
      const targetPath = pathOverride ?? activePath
      if (!targetPath) {
        return
      }

      setFileContents((current) => {
        if (current[targetPath] === value) {
          return current
        }
        return { ...current, [targetPath]: value }
      })
      setLineCounts((current) => {
        const nextLineCount = countLines(value)
        if (current[targetPath] === nextLineCount) {
          return current
        }
        return { ...current, [targetPath]: nextLineCount }
      })

      setDirtyPaths((current) => {
        const savedValue = savedContents[targetPath] ?? ''
        const isDirty = value !== savedValue
        if (isDirty === current.has(targetPath)) {
          return current
        }

        const next = new Set(current)
        if (isDirty) next.add(targetPath)
        else next.delete(targetPath)
        return next
      })
    },
    [activePath, savedContents],
  )

  const handleDirtyChange = useCallback(
    (isDirty: boolean, pathOverride?: string) => {
      const targetPath = pathOverride ?? activePath
      if (!targetPath) {
        return
      }

      setDirtyPaths((current) => {
        if (isDirty === current.has(targetPath)) {
          return current
        }

        const next = new Set(current)
        if (isDirty) next.add(targetPath)
        else next.delete(targetPath)
        return next
      })
    },
    [activePath],
  )

  const handleDocumentStatsChange = useCallback(
    ({ lineCount }: { lineCount: number }, pathOverride?: string) => {
      const targetPath = pathOverride ?? activePath
      if (!targetPath) {
        return
      }

      setLineCounts((current) => {
        if (current[targetPath] === lineCount) {
          return current
        }
        return { ...current, [targetPath]: lineCount }
      })
    },
    [activePath],
  )

  const bumpDocumentVersion = useCallback((path: string) => {
    setDocumentVersions((current) => ({ ...current, [path]: (current[path] ?? 0) + 1 }))
  }, [])

  const showSaveConflict = useCallback(
    async (path: string, mine: string) => {
      try {
        const diskResponse = await readProjectFile(projectId, path)
        if (diskResponse.kind !== 'text') {
          setWorkspaceError(`Xero cannot compare ${path} because it is no longer a text file.`)
          return
        }
        setSaveConflict({
          path,
          mine,
          disk: normalizeToLf(diskResponse.text),
          diskResource: projectFileResourceFromResponse(diskResponse) as Extract<ProjectFileResource, { kind: 'text' }>,
          detectedAt: new Date().toISOString(),
        })
      } catch (error) {
        setWorkspaceError(getDesktopErrorMessage(error))
      }
    },
    [projectId, readProjectFile],
  )

  const savePath = useCallback(async (path: string, snapshot?: string, options: { overwrite?: boolean } = {}) => {
    const resource = fileResourcesRef.current[path]
    if (resource?.kind !== 'text') {
      return false
    }
    const requestEpoch = loadEpochRef.current
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
      if (!options.overwrite && stalePaths[path]) {
        await showSaveConflict(path, content)
        return false
      }

      const payload = serializeWithSettings(content, resource.documentSettings)
      const response = await writeProjectFile(projectId, path, payload, {
        expectedContentHash: resource.contentHash,
        expectedModifiedAt: resource.modifiedAt,
        overwrite: options.overwrite ?? false,
      })
      if (requestEpoch !== loadEpochRef.current) {
        return false
      }

      setSavedContents((current) => ({ ...current, [path]: content }))
      setFileResources((current) => ({
        ...current,
        [path]: projectFileResourceFromWriteResponse(response, current[path] ?? resource),
      }))
      setDirtyPaths((current) => {
        if (!current.has(path)) return current
        const next = new Set(current)
        next.delete(path)
        return next
      })
      setStalePaths((current) => {
        if (!current[path]) return current
        const next = { ...current }
        delete next[path]
        return next
      })
      return true
    } catch (error) {
      if (requestEpoch !== loadEpochRef.current) {
        return false
      }

      if (error instanceof XeroDesktopError && error.code === 'project_file_changed_since_read') {
        await showSaveConflict(path, content)
        return false
      }

      setWorkspaceError(getDesktopErrorMessage(error))
      return false
    } finally {
      if (requestEpoch === loadEpochRef.current) {
        setSavingPath((current) => (current === path ? null : current))
      }
    }
  }, [fileContents, projectId, showSaveConflict, stalePaths, writeProjectFile])

  const saveActive = useCallback(async (snapshot?: string) => {
    if (!activePath) {
      return false
    }

    return savePath(activePath, snapshot)
  }, [activePath, savePath])

  const saveAll = useCallback(
    async (overrides: Record<string, string | undefined> = {}) => {
      const dirty = Array.from(dirtyPathsRef.current)
      if (dirty.length === 0) return
      for (const path of dirty) {
        const latest = overrides[path] ?? fileContentsRef.current[path]
        await savePath(path, latest)
      }
    },
    [savePath],
  )

  const closeOthers = useCallback(() => {
    if (!activePath) return
    const others = openTabsRef.current.filter((path) => path !== activePath)
    const cleanOthers = others.filter((path) => !dirtyPathsRef.current.has(path))
    const dirtyOthers = others.filter((path) => dirtyPathsRef.current.has(path))
    if (dirtyOthers.length > 0) {
      setDirtyGuard({
        operation: { kind: 'close-others' },
        paths: dirtyOthers.sort(),
      })
      return
    }
    for (const path of cleanOthers) {
      closeTabNow(path)
    }
  }, [activePath, closeTabNow])

  const closeSaved = useCallback(() => {
    const saved = openTabsRef.current.filter((path) => !dirtyPathsRef.current.has(path))
    for (const path of saved) {
      closeTabNow(path)
    }
  }, [closeTabNow])

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

  const reloadProjectTreeNow = useCallback(() => {
    const cleanOpenTabs = openTabs.filter((path) => !dirtyPaths.has(path))
    dropCachedPaths(cleanOpenTabs)
    void refreshTree({ preserveExpandedFolders: true }).then(() => {
      if (activePath && cleanOpenTabs.includes(activePath)) {
        void handleSelectFile(activePath, { force: true })
      }
    })
  }, [activePath, dirtyPaths, dropCachedPaths, handleSelectFile, openTabs, refreshTree])

  const reloadProjectTree = useCallback(() => {
    const dirty = Array.from(dirtyPathsRef.current)
    if (dirty.length > 0) {
      setDirtyGuard({ operation: { kind: 'reload' }, paths: dirty.sort() })
      return
    }
    reloadProjectTreeNow()
  }, [reloadProjectTreeNow])

  const collapseAll = useCallback(() => {
    setExpandedFolders(new Set(['/']))
  }, [])

  const revealPathInExplorer = useCallback(
    async (path: string) => {
      if (!path.startsWith('/')) return
      const ancestors = ancestorFolderPaths(path)
      setExpandedFolders((current) => {
        const next = new Set(current)
        for (const ancestor of ancestors) {
          next.add(ancestor)
        }
        return next
      })
      for (const ancestor of ancestors) {
        if (!isFolderLoaded(treeStoreRef.current, ancestor)) {
          await refreshFolder(ancestor, { preserveExpandedFolders: true })
        }
      }
    },
    [refreshFolder],
  )

  const handleRequestRename = useCallback((path: string, type: 'file' | 'folder') => {
    const dirty = dirtyPathsWithin(dirtyPathsRef.current, path, type)
    if (dirty.length > 0) {
      setDirtyGuard({ operation: { kind: 'rename', path, entryType: type }, paths: dirty })
      return
    }
    setRenameTarget({ path, type })
  }, [])

  const handleRequestDelete = useCallback((path: string, type: 'file' | 'folder') => {
    const dirty = dirtyPathsWithin(dirtyPathsRef.current, path, type)
    if (dirty.length > 0) {
      setDirtyGuard({ operation: { kind: 'delete', path, entryType: type }, paths: dirty })
      return
    }
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
              documentSettings: detectDocumentSettings(''),
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

  const discardDrafts = useCallback(
    (paths: string[]) => {
      setDirtyPaths((current) => {
        const next = new Set(current)
        for (const path of paths) {
          next.delete(path)
        }
        return next
      })
      setFileContents((current) => {
        const next = { ...current }
        for (const path of paths) {
          next[path] = savedContents[path] ?? ''
        }
        return next
      })
      setLineCounts((current) => {
        const next = { ...current }
        for (const path of paths) {
          next[path] = countLines(savedContents[path] ?? '')
        }
        return next
      })
      for (const path of paths) {
        bumpDocumentVersion(path)
      }
    },
    [bumpDocumentVersion, savedContents],
  )

  const continueAfterDirtyGuard = useCallback(
    (operation: DirtyGuardOperation) => {
      if (operation.kind === 'close') {
        closeTabNow(operation.path)
        return
      }
      if (operation.kind === 'rename') {
        setRenameTarget({ path: operation.path, type: operation.entryType })
        return
      }
      if (operation.kind === 'delete') {
        setDeleteTarget({ path: operation.path, type: operation.entryType })
        return
      }
      if (operation.kind === 'close-others') {
        const currentActivePath = activePath
        for (const path of openTabsRef.current.slice()) {
          if (path !== currentActivePath) {
            closeTabNow(path)
          }
        }
        return
      }
      if (operation.kind === 'save-all') {
        return
      }

      const cachedTabs = openTabsRef.current
      dropCachedPaths(cachedTabs)
      void refreshTree({ preserveExpandedFolders: true }).then(() => {
        const currentActivePath = activePath
        if (currentActivePath && cachedTabs.includes(currentActivePath)) {
          void handleSelectFile(currentActivePath, { force: true })
        }
      })
    },
    [activePath, closeTabNow, dropCachedPaths, handleSelectFile, refreshTree],
  )

  const saveDirtyGuard = useCallback(async () => {
    if (!dirtyGuard) return
    for (const path of dirtyGuard.paths) {
      const saved = await savePath(path)
      if (!saved) return
    }
    const operation = dirtyGuard.operation
    setDirtyGuard(null)
    continueAfterDirtyGuard(operation)
  }, [continueAfterDirtyGuard, dirtyGuard, savePath])

  const discardDirtyGuard = useCallback(() => {
    if (!dirtyGuard) return
    const operation = dirtyGuard.operation
    discardDrafts(dirtyGuard.paths)
    setDirtyGuard(null)
    continueAfterDirtyGuard(operation)
  }, [continueAfterDirtyGuard, dirtyGuard, discardDrafts])

  const cancelDirtyGuard = useCallback(() => {
    setDirtyGuard(null)
  }, [])

  const reloadSaveConflictFromDisk = useCallback(() => {
    if (!saveConflict) return
    const { path, disk, diskResource } = saveConflict
    setSavedContents((current) => ({ ...current, [path]: disk }))
    setFileContents((current) => ({ ...current, [path]: disk }))
    setFileResources((current) => ({ ...current, [path]: diskResource }))
    setLineCounts((current) => ({ ...current, [path]: countLines(disk) }))
    bumpDocumentVersion(path)
    setDirtyPaths((current) => {
      if (!current.has(path)) return current
      const next = new Set(current)
      next.delete(path)
      return next
    })
    setStalePaths((current) => {
      if (!current[path]) return current
      const next = { ...current }
      delete next[path]
      return next
    })
    setSaveConflict(null)
  }, [bumpDocumentVersion, saveConflict])

  const overwriteSaveConflict = useCallback(async () => {
    if (!saveConflict) return
    const conflict = saveConflict
    setSaveConflict(null)
    const saved = await savePath(conflict.path, conflict.mine, { overwrite: true })
    if (!saved) {
      setSaveConflict(conflict)
    }
  }, [saveConflict, savePath])

  const keepMineSaveConflict = useCallback(() => {
    setSaveConflict(null)
  }, [])

  useEffect(() => {
    if (!active || !readProjectUiState || draftsHydrated) {
      if (!readProjectUiState && !draftsHydrated) setDraftsHydrated(true)
      return
    }

    let cancelled = false
    const requestEpoch = loadEpochRef.current
    void readProjectUiState({ projectId, key: EDITOR_DRAFTS_UI_STATE_KEY })
      .then(async (response) => {
        if (cancelled || requestEpoch !== loadEpochRef.current) return
        if (!isPersistedEditorDrafts(response.value, projectId)) {
          setDraftsHydrated(true)
          return
        }

        const entries = Object.entries(response.value.drafts)
          .filter(([path, draft]) => path.startsWith('/') && typeof draft.content === 'string')
          .slice(0, 20)
        if (entries.length === 0) {
          setDraftsHydrated(true)
          return
        }

        const restoredResources: Record<string, ProjectFileResource> = {}
        const restoredSaved: Record<string, string> = {}
        const restoredContents: Record<string, string> = {}
        const restoredLines: Record<string, number> = {}
        const restoredDirty = new Set<string>()
        const restoredTabs: string[] = []
        const restoredStale: Record<string, StaleOpenFileState> = {}

        for (const [path, draft] of entries) {
          try {
            const disk = await readProjectFile(projectId, path)
            if (cancelled || requestEpoch !== loadEpochRef.current) return
            if (disk.kind !== 'text') continue
            const resource = projectFileResourceFromResponse(disk)
            restoredResources[path] = resource
            restoredSaved[path] = normalizeToLf(disk.text)
            restoredContents[path] = normalizeToLf(draft.content)
            restoredLines[path] = countLines(normalizeToLf(draft.content))
            restoredDirty.add(path)
            restoredTabs.push(path)
            if (
              draft.savedContentHash &&
              draft.savedContentHash !== disk.contentHash
            ) {
              restoredStale[path] = { kind: 'changed', detectedAt: new Date().toISOString() }
            }
          } catch {
            // Draft recovery is best effort. The persisted content stays in app-data
            // until a successful write clears it.
          }
        }

        if (cancelled || requestEpoch !== loadEpochRef.current) return
        if (restoredTabs.length > 0) {
          setFileResources((current) => ({ ...current, ...restoredResources }))
          setSavedContents((current) => ({ ...current, ...restoredSaved }))
          setFileContents((current) => ({ ...current, ...restoredContents }))
          setLineCounts((current) => ({ ...current, ...restoredLines }))
          setDirtyPaths((current) => new Set([...Array.from(current), ...Array.from(restoredDirty)]))
          setOpenTabs((current) => Array.from(new Set([...current, ...restoredTabs])))
          setActivePath((current) => current ?? restoredTabs[0] ?? null)
          setStalePaths((current) => ({ ...current, ...restoredStale }))
        }
        setDraftsHydrated(true)
      })
      .catch(() => {
        if (!cancelled && requestEpoch === loadEpochRef.current) {
          setDraftsHydrated(true)
        }
      })

    return () => {
      cancelled = true
    }
  }, [active, draftsHydrated, projectId, readProjectFile, readProjectUiState])

  useEffect(() => {
    if (!writeProjectUiState || !draftsHydrated) return
    const persistDrafts = () => {
      const drafts: PersistedEditorDrafts['drafts'] = {}
      for (const path of dirtyPathsRef.current) {
        const resource = fileResourcesRef.current[path]
        if (resource?.kind !== 'text') continue
        drafts[path] = {
          content: fileContents[path] ?? '',
          savedContentHash: resource.contentHash,
          savedModifiedAt: resource.modifiedAt,
          updatedAt: new Date().toISOString(),
        }
      }
      const value: PersistedEditorDrafts | null =
        Object.keys(drafts).length > 0
          ? {
              schema: 'xero.editor.drafts.v1',
              projectId,
              updatedAt: new Date().toISOString(),
              drafts,
            }
          : null
      void writeProjectUiState({ projectId, key: EDITOR_DRAFTS_UI_STATE_KEY, value }).catch(() => {})
    }
    const handle = window.setTimeout(persistDrafts, DRAFT_PERSIST_DEBOUNCE_MS)
    return () => {
      window.clearTimeout(handle)
      persistDrafts()
    }
  }, [dirtyPaths, draftsHydrated, fileContents, projectId, writeProjectUiState])

  useEffect(() => {
    if (!active || dirtyPaths.size === 0 || typeof window === 'undefined') return
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', handleBeforeUnload)
    return () => window.removeEventListener('beforeunload', handleBeforeUnload)
  }, [active, dirtyPaths])

  useEffect(() => {
    if (!active || !statProjectFiles) return

    let cancelled = false
    const checkOpenFiles = async () => {
      const paths = openTabsRef.current.filter((path) => fileResourcesRef.current[path])
      if (paths.length === 0) return
      try {
        const response = await statProjectFiles(projectId, paths)
        if (cancelled) return
        setStalePaths((current) => {
          let changed = false
          const next = { ...current }
          const seen = new Set<string>()
          for (const file of response.files) {
            seen.add(file.path)
            const resource = fileResourcesRef.current[file.path]
            if (!resource) continue
            if (file.kind === 'missing') {
              if (next[file.path]?.kind !== 'deleted') {
                next[file.path] = { kind: 'deleted', detectedAt: new Date().toISOString() }
                changed = true
              }
              continue
            }
            if (file.kind !== 'file') continue
            const isStale = file.contentHash !== resource.contentHash || file.modifiedAt !== resource.modifiedAt
            if (isStale && next[file.path]?.kind !== 'changed') {
              next[file.path] = { kind: 'changed', detectedAt: new Date().toISOString() }
              changed = true
            } else if (!isStale && next[file.path]) {
              delete next[file.path]
              changed = true
            }
          }
          for (const path of paths) {
            if (!seen.has(path) && next[path]?.kind !== 'deleted') {
              next[path] = { kind: 'deleted', detectedAt: new Date().toISOString() }
              changed = true
            }
          }
          return changed ? next : current
        })
      } catch {
        // Polling is a freshness aid; command failures should not block typing.
      }
    }

    void checkOpenFiles()
    const handle = window.setInterval(() => {
      void checkOpenFiles()
    }, OPEN_FILE_STAT_POLL_MS)
    return () => {
      cancelled = true
      window.clearInterval(handle)
    }
  }, [active, projectId, statProjectFiles])

  useEffect(() => {
    if (!active) return
    const handle = window.setInterval(() => {
      void refreshFolderSet(expandedFoldersRef.current, {
        reportErrors: false,
        showLoadingState: false,
      })
    }, TREE_REFRESH_POLL_MS)
    return () => window.clearInterval(handle)
  }, [active, refreshFolderSet])

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
    stalePaths,
    saveConflict,
    dirtyGuard,
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
    closeOthers,
    closeSaved,
    handleSelectFile,
    handleToggleFolder,
    handleSnapshotChange,
    handleDirtyChange,
    handleDocumentStatsChange,
    saveActive,
    saveAll,
    revertActive,
    reloadProjectTree,
    collapseAll,
    revealPathInExplorer,
    handleRequestRename,
    handleRequestDelete,
    handleRequestNewFile,
    handleRequestNewFolder,
    handleMoveEntry,
    handleCopyPath,
    handleRenameSubmit,
    handleDeleteSubmit,
    handleCreateSubmit,
    saveDirtyGuard,
    discardDirtyGuard,
    cancelDirtyGuard,
    reloadSaveConflictFromDisk,
    overwriteSaveConflict,
    keepMineSaveConflict,
  }
}
