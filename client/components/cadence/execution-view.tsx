"use client"

import { useMemo } from 'react'
import { ChevronRight, FileCode, FilePlus, FolderPlus, RotateCcw, Search, X } from 'lucide-react'
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
import { cn } from '@/lib/utils'
import type { ExecutionPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { CodeEditor } from './code-editor'
import { FileTree, getFileIcon as getFileIconForName } from './file-tree'
import { RenameFileDialog } from './rename-file-dialog'
import { DeleteFileDialog } from './delete-file-dialog'
import { NewFileDialog } from './new-file-dialog'
import { Input } from '@/components/ui/input'
import { useExecutionWorkspaceController } from './execution-view/use-execution-workspace-controller'

interface CursorPosition {
  line: number
  column: number
}

export interface ExecutionViewProps {
  execution: ExecutionPaneView
  listProjectFiles: (projectId: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
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
  const {
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
    handleCopyPath,
    handleRenameSubmit,
    handleDeleteSubmit,
    handleCreateSubmit,
  } = useExecutionWorkspaceController({
    projectId,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
  })

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

export function ExecutionView(props: ExecutionViewProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <EditorView {...props} />
    </div>
  )
}
