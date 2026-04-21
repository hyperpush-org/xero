"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ChevronRight, FileCode, FilePlus, FolderPlus, RotateCcw, Search, X } from 'lucide-react'
import { getDesktopErrorMessage } from '@/src/lib/cadence-desktop'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  ListProjectFilesResponseDto,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/cadence-model'
import {
  createEmptyFileSystem,
  findNode,
  listAllFolderPaths,
  mapProjectFileTree,
  type FileSystemNode,
} from '@/src/lib/file-system-tree'
import { getLangFromPath } from '@/lib/shiki'
import { cn } from '@/lib/utils'
import type { ExecutionPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { CodeEditor } from './code-editor'
import { FileTree, getFileIcon as getFileIconForName } from './file-tree'
import { RenameFileDialog } from './rename-file-dialog'
import { DeleteFileDialog } from './delete-file-dialog'
import { NewFileDialog } from './new-file-dialog'
import { Input } from '@/components/ui/input'

interface CursorPosition {
  line: number
  column: number
}

interface ExecutionViewProps {
  execution: ExecutionPaneView
  listProjectFiles: (projectId: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
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

function EditorView({
  execution,
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  createProjectEntry,
  renameProjectEntry,
  deleteProjectEntry,
}: ExecutionViewProps) {
  const projectId = execution.project.id
  const projectLabel = execution.project.repository?.displayName ?? execution.project.name
  const repositoryPath = execution.project.repository?.rootPath ?? null
  const loadEpochRef = useRef(0)

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

  const [renameTarget, setRenameTarget] = useState<{ path: string; type: 'file' | 'folder' } | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<{ path: string; type: 'file' | 'folder' } | null>(null)
  const [newChildTarget, setNewChildTarget] = useState<{ parentPath: string; type: 'file' | 'folder' } | null>(null)

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
    void refreshTree({ preserveExpandedFolders: false })
  }, [projectId, refreshTree])

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
    const content = fileContents[activePath] ?? ''
    setSavingPath(activePath)
    setWorkspaceError(null)

    try {
      await writeProjectFile(projectId, activePath, content)
      if (requestEpoch !== loadEpochRef.current) {
        return
      }

      setSavedContents((current) => ({ ...current, [activePath]: content }))
      setDirtyPaths((current) => {
        if (!current.has(activePath)) return current
        const next = new Set(current)
        next.delete(activePath)
        return next
      })
    } catch (error) {
      if (requestEpoch !== loadEpochRef.current) {
        return
      }

      setWorkspaceError(getDesktopErrorMessage(error))
    } finally {
      if (requestEpoch === loadEpochRef.current) {
        setSavingPath((current) => (current === activePath ? null : current))
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
    setNewChildTarget({ parentPath, type: 'file' })
  }, [])

  const handleRequestNewFolder = useCallback((parentPath: string) => {
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
        setExpandedFolders((current) =>
          new Set(Array.from(current).map((path) => remapPath(path, oldPath, newPath))),
        )
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
      setActivePath((current) =>
        current === deletedPath || current?.startsWith(deletedPrefix) ? null : current,
      )
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

      try {
        const response = await createProjectEntry({
          projectId,
          parentPath,
          name,
          entryType: type,
        })

        if (type === 'file') {
          setSavedContents((current) => ({ ...current, [response.path]: '' }))
          setFileContents((current) => ({ ...current, [response.path]: '' }))
          openFile(response.path)
        }

        setExpandedFolders((current) => {
          const next = new Set(current)
          next.add(parentPath)
          if (type === 'folder') {
            next.add(response.path)
          }
          return next
        })
        setWorkspaceError(null)
        await refreshTree({ preserveExpandedFolders: true })
        return null
      } catch (error) {
        return getDesktopErrorMessage(error)
      }
    },
    [createProjectEntry, newChildTarget, openFile, projectId, refreshTree],
  )

  const activeNode = activePath ? findNode(tree, activePath) : null
  const activeContent = activePath ? fileContents[activePath] ?? '' : ''
  const activeLang = activePath ? getLangFromPath(activePath) ?? 'plaintext' : 'plaintext'
  const activeLineCount = activePath ? (fileContents[activePath]?.split('\n').length ?? 0) : 0
  const isActiveDirty = activePath ? dirtyPaths.has(activePath) : false
  const isActiveSaving = activePath ? savingPath === activePath : false
  const isActiveLoading = activePath ? pendingFilePath === activePath : false
  const explorerSubtitle = useMemo(() => {
    if (!repositoryPath) {
      return execution.branchLabel
    }

    return repositoryPath
  }, [execution.branchLabel, repositoryPath])

  return (
    <div className="flex min-h-0 w-full flex-1 min-w-0">
      <aside className="flex w-[260px] shrink-0 flex-col border-r border-border bg-sidebar">
        <div className="flex shrink-0 items-start justify-between gap-2 px-3 pt-2.5 pb-2">
          <div className="min-w-0">
            <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Explorer
            </span>
            <p className="truncate text-[11px] text-foreground/85">{projectLabel}</p>
            <p className="truncate text-[10px] text-muted-foreground">{explorerSubtitle}</p>
          </div>
          <div className="flex items-center gap-0.5">
            <IconButton label="New file" onClick={() => handleRequestNewFile('/')}>
              <FilePlus className="h-3.5 w-3.5" />
            </IconButton>
            <IconButton label="New folder" onClick={() => handleRequestNewFolder('/')}>
              <FolderPlus className="h-3.5 w-3.5" />
            </IconButton>
            <IconButton label="Collapse all" onClick={collapseAll}>
              <ChevronRight className="h-3.5 w-3.5 rotate-90" />
            </IconButton>
            <IconButton label="Reload project" onClick={reloadProjectTree}>
              <RotateCcw className={cn('h-3.5 w-3.5', isTreeLoading && 'animate-spin')} />
            </IconButton>
          </div>
        </div>

        <div className="shrink-0 px-2 pb-2">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              aria-label="Search files"
              className="h-7 pl-6 pr-6 text-[11px]"
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="Search files…"
              value={searchQuery}
            />
            {searchQuery ? (
              <button
                aria-label="Clear search"
                className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                onClick={() => setSearchQuery('')}
                type="button"
              >
                <X className="h-3 w-3" />
              </button>
            ) : null}
          </div>
        </div>

        {workspaceError ? (
          <div className="mx-2 mb-2 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-2 text-[11px] text-destructive">
            {workspaceError}
          </div>
        ) : null}

        {isTreeLoading && !tree.children?.length ? (
          <div className="flex flex-1 items-center justify-center px-6 text-center text-[11px] text-muted-foreground">
            Loading selected project files…
          </div>
        ) : (
          <FileTree
            root={tree}
            selectedPath={activePath}
            expandedFolders={expandedFolders}
            dirtyPaths={dirtyPaths}
            searchQuery={searchQuery}
            onSelectFile={(path) => {
              void handleSelectFile(path)
            }}
            onToggleFolder={handleToggleFolder}
            onRequestRename={handleRequestRename}
            onRequestDelete={handleRequestDelete}
            onRequestNewFile={handleRequestNewFile}
            onRequestNewFolder={handleRequestNewFolder}
            onCopyPath={handleCopyPath}
          />
        )}
      </aside>

      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-stretch border-b border-border bg-secondary/10">
          {openTabs.length === 0 ? (
            <div className="flex h-9 items-center px-3 text-[11px] text-muted-foreground/70">
              {pendingFilePath ? `Opening ${pendingFilePath.split('/').pop() ?? pendingFilePath}…` : 'No files open'}
            </div>
          ) : (
            <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto scrollbar-thin">
              {openTabs.map((tabPath) => {
                const isActive = activePath === tabPath
                const isDirty = dirtyPaths.has(tabPath)
                const name = tabPath.split('/').pop() ?? tabPath
                return (
                  <div
                    key={tabPath}
                    className={cn(
                      'group relative flex shrink-0 items-center gap-1.5 border-r border-border pl-3 pr-2 text-[12px] transition-colors',
                      isActive
                        ? 'bg-background text-foreground'
                        : 'bg-secondary/10 text-muted-foreground hover:bg-secondary/30 hover:text-foreground',
                    )}
                  >
                    <button type="button" onClick={() => setActivePath(tabPath)} className="flex items-center gap-1.5 py-1.5">
                      {getFileIconForName(name)}
                      <span className="font-mono">{name}</span>
                    </button>
                    <button
                      aria-label={`Close ${name}`}
                      className={cn(
                        'ml-0.5 flex h-4 w-4 items-center justify-center rounded-sm transition-colors',
                        isDirty
                          ? 'text-primary hover:bg-muted hover:text-foreground'
                          : 'text-muted-foreground opacity-0 hover:bg-muted hover:text-foreground group-hover:opacity-100',
                        isActive && 'opacity-100',
                      )}
                      onClick={(event) => {
                        event.stopPropagation()
                        closeTab(tabPath)
                      }}
                      type="button"
                    >
                      {isDirty ? <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden /> : <X className="h-3 w-3" />}
                    </button>
                    {isActive ? <span className="absolute inset-x-0 -bottom-px h-px bg-primary" aria-hidden /> : null}
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {activePath ? (
          <div className="flex shrink-0 items-center justify-between border-b border-border bg-background px-3 py-1.5">
            <Breadcrumb path={activePath} />
            <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
              {isActiveDirty ? (
                <button
                  className="rounded px-1.5 py-0.5 transition-colors hover:bg-secondary/50 hover:text-foreground"
                  onClick={revertActive}
                  type="button"
                >
                  Revert
                </button>
              ) : null}
              <button
                className={cn(
                  'rounded px-2 py-0.5 font-medium transition-colors',
                  isActiveDirty && !isActiveSaving
                    ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                    : 'text-muted-foreground',
                )}
                disabled={!isActiveDirty || isActiveSaving}
                onClick={() => {
                  void saveActive()
                }}
                type="button"
                title="Save (⌘S)"
              >
                {isActiveSaving ? 'Saving…' : 'Save'}
              </button>
            </div>
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 flex-col bg-background">
          {activePath && activeNode?.type === 'file' ? (
            isActiveLoading ? (
              <LoadingState path={activePath} />
            ) : (
              <>
                <div className="flex-1 overflow-hidden">
                  <CodeEditor
                    value={activeContent}
                    filePath={activePath}
                    onChange={handleChange}
                    onSave={() => {
                      void saveActive()
                    }}
                    onCursorChange={setCursor}
                  />
                </div>
                <StatusBar
                  cursor={cursor}
                  lang={activeLang}
                  lineCount={activeLineCount}
                  isDirty={isActiveDirty}
                  isSaving={isActiveSaving}
                />
              </>
            )
          ) : (
            <EditorEmptyState loadingPath={pendingFilePath} projectLabel={projectLabel} />
          )}
        </div>
      </section>

      <RenameFileDialog
        open={!!renameTarget}
        onOpenChange={(open) => {
          if (!open) setRenameTarget(null)
        }}
        currentPath={renameTarget?.path ?? ''}
        type={renameTarget?.type ?? 'file'}
        onRename={(newName) => handleRenameSubmit(newName)}
      />
      <DeleteFileDialog
        open={!!deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
        path={deleteTarget?.path ?? ''}
        type={deleteTarget?.type ?? 'file'}
        onDelete={() => {
          void handleDeleteSubmit()
        }}
      />
      <NewFileDialog
        open={!!newChildTarget}
        onOpenChange={(open) => {
          if (!open) setNewChildTarget(null)
        }}
        parentPath={newChildTarget?.parentPath ?? '/'}
        type={newChildTarget?.type ?? 'file'}
        onCreate={(name) => handleCreateSubmit(name)}
      />
    </div>
  )
}

function IconButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      aria-label={label}
      className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  )
}

function Breadcrumb({ path }: { path: string }) {
  const segments = path.split('/').filter(Boolean)
  return (
    <div className="flex min-w-0 items-center gap-1 truncate font-mono text-[11px] text-muted-foreground">
      {segments.map((segment, index) => (
        <span key={`${segment}-${index}`} className="flex items-center gap-1">
          {index > 0 ? <ChevronRight className="h-3 w-3 text-muted-foreground/40" /> : null}
          <span className={cn(index === segments.length - 1 && 'text-foreground/85')}>{segment}</span>
        </span>
      ))}
    </div>
  )
}

function StatusBar({
  cursor,
  lang,
  lineCount,
  isDirty,
  isSaving,
}: {
  cursor: CursorPosition
  lang: string
  lineCount: number
  isDirty: boolean
  isSaving: boolean
}) {
  return (
    <div className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground">
      <div className="flex items-center gap-3">
        <span className="tabular-nums">Ln {cursor.line}, Col {cursor.column}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="tabular-nums">{lineCount} lines</span>
        <span className="text-muted-foreground/40">·</span>
        <span>Spaces: 2</span>
      </div>
      <div className="flex items-center gap-3">
        {isSaving ? <span className="text-primary">Saving…</span> : isDirty ? <span className="text-primary">● Unsaved</span> : <span>Saved</span>}
        <span className="text-muted-foreground/40">·</span>
        <span>UTF-8</span>
        <span className="text-muted-foreground/40">·</span>
        <span>LF</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="capitalize">{lang}</span>
      </div>
    </div>
  )
}

function LoadingState({ path }: { path: string }) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background">
      <div className="text-center text-[12px] text-muted-foreground">Opening {path.split('/').pop() ?? path}…</div>
    </div>
  )
}

function EditorEmptyState({ loadingPath, projectLabel }: { loadingPath: string | null; projectLabel: string }) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background">
      <div className="flex max-w-sm flex-col items-center gap-4 text-center">
        <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-border bg-card">
          <FileCode className="h-6 w-6 text-muted-foreground" />
        </div>
        <div>
          <p className="text-[14px] font-medium text-foreground">
            {loadingPath ? 'Opening file…' : 'Select a file to start editing'}
          </p>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            {loadingPath
              ? `Cadence is loading ${loadingPath.split('/').pop() ?? loadingPath} from ${projectLabel}.`
              : `Pick a file from the selected project explorer. Edits save directly back to ${projectLabel}.`}
          </p>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-muted-foreground/70">
          <Shortcut keys={['⌘', 'S']} label="Save" />
          <span>·</span>
          <Shortcut keys={['⌘', 'W']} label="Close tab" />
        </div>
      </div>
    </div>
  )
}

function Shortcut({ keys, label }: { keys: string[]; label: string }) {
  return (
    <span className="flex items-center gap-1">
      {keys.map((key, index) => (
        <kbd
          key={`${key}-${index}`}
          className="rounded border border-border bg-card px-1.5 py-0.5 font-mono text-[10px] text-foreground/70"
        >
          {key}
        </kbd>
      ))}
      <span>{label}</span>
    </span>
  )
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

export function ExecutionView(props: ExecutionViewProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <EditorView {...props} />
    </div>
  )
}
