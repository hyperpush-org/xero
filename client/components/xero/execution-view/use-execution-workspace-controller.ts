import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getDesktopErrorMessage } from '@/src/lib/xero-desktop'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  ListProjectFilesResponseDto,
  MoveProjectEntryRequestDto,
  MoveProjectEntryResponseDto,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import {
  createEmptyFileSystem,
  findNode,
  mapProjectFileTree,
  type FileSystemNode,
} from '@/src/lib/file-system-tree'
import { getLangFromPath } from '@/lib/shiki'

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
  listProjectFiles: (projectId: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
}

function defaultExpandedFolders(root: FileSystemNode): Set<string> {
  const folders = new Set<string>(['/'])

  for (const candidate of ['/src', '/app', '/components']) {
    if (findNode(root, candidate)?.type === 'folder') {
      folders.add(candidate)
    }
  }

  if (folders.size === 1 && root.children?.length === 1 && root.children[0]?.type === 'folder') {
    folders.add(root.children[0].path)
  }

  return folders
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

function childPath(parentPath: string, name: string): string {
  return parentPath === '/' ? `/${name}` : `${parentPath}/${name}`
}

function splitEntryPath(value: string): string[] {
  return value
    .trim()
    .replace(/\\/g, '/')
    .split('/')
    .map((segment) => segment.trim())
    .filter(Boolean)
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
  const pendingInitialTreeLoadRef = useRef<string | null>(projectId)

  const [tree, setTree] = useState<FileSystemNode>(createEmptyFileSystem)
  const [savedContents, setSavedContents] = useState<Record<string, string>>({})
  const [fileContents, setFileContents] = useState<Record<string, string>>({})
  const [openTabs, setOpenTabs] = useState<string[]>([])
  const [activePath, setActivePath] = useState<string | null>(null)
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['/']))
  const [dirtyPaths, setDirtyPaths] = useState<Set<string>>(new Set())
  const [searchQuery, setSearchQuery] = useState('')
  const [cursor, setCursor] = useState<CursorPosition>({ line: 1, column: 1 })
  const [isTreeLoading, setIsTreeLoading] = useState(false)
  const [pendingFilePath, setPendingFilePath] = useState<string | null>(null)
  const [savingPath, setSavingPath] = useState<string | null>(null)
  const [workspaceError, setWorkspaceError] = useState<string | null>(null)
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null)
  const [newChildTarget, setNewChildTarget] = useState<NewChildTarget | null>(null)

  const openFile = useCallback((path: string) => {
    setOpenTabs((current) => (current.includes(path) ? current : [...current, path]))
    setActivePath(path)
  }, [])

  const refreshTree = useCallback(
    async (options: { preserveExpandedFolders?: boolean } = {}) => {
      const requestEpoch = loadEpochRef.current
      setIsTreeLoading(true)
      setWorkspaceError(null)

      try {
        const response = await listProjectFiles(projectId)
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        const nextTree = mapProjectFileTree(response)
        setTree(nextTree)
        setExpandedFolders((current) => {
          if (!options.preserveExpandedFolders || current.size === 0) {
            return defaultExpandedFolders(nextTree)
          }

          const next = new Set(Array.from(current).filter((path) => findNode(nextTree, path)?.type === 'folder'))
          if (next.size === 0) {
            return defaultExpandedFolders(nextTree)
          }

          next.add('/')
          return next
        })
        setOpenTabs((current) => current.filter((path) => findNode(nextTree, path)?.type === 'file'))
        setActivePath((current) => (current && findNode(nextTree, current)?.type === 'file' ? current : null))
      } catch (error) {
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        setTree(createEmptyFileSystem())
        setOpenTabs([])
        setActivePath(null)
        setExpandedFolders(new Set(['/']))
        setWorkspaceError(getDesktopErrorMessage(error))
      } finally {
        if (requestEpoch === loadEpochRef.current) {
          setIsTreeLoading(false)
        }
      }
    },
    [listProjectFiles, projectId],
  )

  useEffect(() => {
    loadEpochRef.current += 1
    pendingInitialTreeLoadRef.current = projectId
    setTree(createEmptyFileSystem())
    setSavedContents({})
    setFileContents({})
    setOpenTabs([])
    setActivePath(null)
    setExpandedFolders(new Set(['/']))
    setDirtyPaths(new Set())
    setSearchQuery('')
    setCursor({ line: 1, column: 1 })
    setPendingFilePath(null)
    setSavingPath(null)
    setWorkspaceError(null)
    setRenameTarget(null)
    setDeleteTarget(null)
    setNewChildTarget(null)
  }, [projectId])

  useEffect(() => {
    if (!active || pendingInitialTreeLoadRef.current !== projectId) {
      return
    }

    pendingInitialTreeLoadRef.current = null
    void refreshTree({ preserveExpandedFolders: false })
  }, [active, projectId, refreshTree])

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
    },
    [activePath],
  )

  const handleSelectFile = useCallback(
    async (path: string) => {
      const node = findNode(tree, path)
      if (!node || node.type !== 'file') {
        return
      }

      if (fileContents[path] !== undefined) {
        openFile(path)
        return
      }

      const requestEpoch = loadEpochRef.current
      setPendingFilePath(path)
      setWorkspaceError(null)

      try {
        const response = await readProjectFile(projectId, path)
        if (requestEpoch !== loadEpochRef.current) {
          return
        }

        setSavedContents((current) => ({ ...current, [path]: response.content }))
        setFileContents((current) => ({ ...current, [path]: response.content }))
        openFile(path)
      } catch (error) {
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
    [fileContents, openFile, projectId, readProjectFile, tree],
  )

  const handleToggleFolder = useCallback((path: string) => {
    setExpandedFolders((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }, [])

  const handleChange = useCallback(
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

  const saveActive = useCallback(async () => {
    if (!activePath) {
      return
    }

    const requestEpoch = loadEpochRef.current
    const path = activePath
    const content = fileContents[path] ?? ''
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
  }, [activePath, fileContents, projectId, writeProjectFile])

  const revertActive = useCallback(() => {
    if (!activePath) {
      return
    }

    const savedValue = savedContents[activePath] ?? ''
    setFileContents((current) => ({ ...current, [activePath]: savedValue }))
    setDirtyPaths((current) => {
      if (!current.has(activePath)) return current
      const next = new Set(current)
      next.delete(activePath)
      return next
    })
  }, [activePath, savedContents])

  const reloadProjectTree = useCallback(() => {
    void refreshTree({ preserveExpandedFolders: true })
  }, [refreshTree])

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
    setNewChildTarget({ parentPath, type: 'file' })
  }, [])

  const handleRequestNewFolder = useCallback((parentPath: string) => {
    setExpandedFolders((current) => {
      const next = new Set(current)
      next.add(parentPath)
      return next
    })
    setNewChildTarget({ parentPath, type: 'folder' })
  }, [])

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
        setOpenTabs((current) => current.map((path) => remapPath(path, oldPath, newPath)))
        setDirtyPaths((current) => new Set(Array.from(current).map((path) => remapPath(path, oldPath, newPath))))
        setExpandedFolders((current) => new Set(Array.from(current).map((path) => remapPath(path, oldPath, newPath))))
        setActivePath((current) => (current ? remapPath(current, oldPath, newPath) : null))
        setWorkspaceError(null)
        await refreshTree({ preserveExpandedFolders: true })
        return null
      } catch (error) {
        return getDesktopErrorMessage(error)
      }
    },
    [projectId, refreshTree, renameProjectEntry, renameTarget],
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
      await refreshTree({ preserveExpandedFolders: true })
    } catch (error) {
      setWorkspaceError(getDesktopErrorMessage(error))
    }
  }, [deleteProjectEntry, deleteTarget, projectId, refreshTree])

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
        await refreshTree({ preserveExpandedFolders: true })
        return null
      } catch (error) {
        return getDesktopErrorMessage(error)
      }
    },
    [createProjectEntry, newChildTarget, openFile, projectId, refreshTree, tree],
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
        setOpenTabs((current) => current.map((candidate) => remapPath(candidate, path, newPath)))
        setDirtyPaths((current) => new Set(Array.from(current).map((candidate) => remapPath(candidate, path, newPath))))
        setExpandedFolders((current) => {
          const next = new Set(Array.from(current).map((candidate) => remapPath(candidate, path, newPath)))
          next.add(targetParentPath)
          return next
        })
        setActivePath((current) => (current ? remapPath(current, path, newPath) : null))
        setWorkspaceError(null)
        await refreshTree({ preserveExpandedFolders: true })
      } catch (error) {
        setWorkspaceError(getDesktopErrorMessage(error))
      }
    },
    [moveProjectEntry, projectId, refreshTree],
  )

  const activeNode = useMemo(() => (activePath ? findNode(tree, activePath) : null), [activePath, tree])
  const activeContent = activePath ? fileContents[activePath] ?? '' : ''
  const activeLang = activePath ? getLangFromPath(activePath) ?? 'plaintext' : 'plaintext'
  const activeLineCount = activePath ? (fileContents[activePath]?.split('\n').length ?? 0) : 0
  const isActiveDirty = activePath ? dirtyPaths.has(activePath) : false
  const isActiveSaving = activePath ? savingPath === activePath : false
  const isActiveLoading = activePath ? pendingFilePath === activePath : false

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
    pendingFilePath,
    savingPath,
    workspaceError,
    renameTarget,
    setRenameTarget,
    deleteTarget,
    setDeleteTarget,
    newChildTarget,
    setNewChildTarget,
    activeNode,
    activeContent,
    activeLang,
    activeLineCount,
    isActiveDirty,
    isActiveSaving,
    isActiveLoading,
    closeTab,
    handleSelectFile,
    handleToggleFolder,
    handleChange,
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
