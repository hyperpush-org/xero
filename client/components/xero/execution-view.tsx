"use client"

import { useCallback, useEffect, useRef, useState } from 'react'
import { EditorView as CodeMirrorView } from '@codemirror/view'
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
  ReplaceInProjectRequestDto,
  ReplaceInProjectResponseDto,
  SearchProjectRequestDto,
  SearchProjectResponseDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import type { ExecutionPaneView } from '@/src/features/xero/use-xero-desktop-state'
import { CodeEditor } from './code-editor'
import { DeleteFileDialog } from './delete-file-dialog'
import { RenameFileDialog } from './rename-file-dialog'
import { EditorEmptyState, LoadingState } from './execution-view/editor-empty-state'
import { ExplorerPane } from './execution-view/explorer-pane'
import { FindReplacePane } from './execution-view/find-replace-pane'
import { EditorStatusBar, EditorToolbar } from './execution-view/editor-status-bar'
import { EditorTabs } from './execution-view/editor-tabs'
import { useExecutionWorkspaceController } from './execution-view/use-execution-workspace-controller'

export interface ExecutionViewProps {
  execution: ExecutionPaneView
  listProjectFiles: (projectId: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
  searchProject: (request: SearchProjectRequestDto) => Promise<SearchProjectResponseDto>
  replaceInProject: (request: ReplaceInProjectRequestDto) => Promise<ReplaceInProjectResponseDto>
}

function EditorView({
  execution,
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  createProjectEntry,
  renameProjectEntry,
  moveProjectEntry,
  deleteProjectEntry,
  searchProject,
  replaceInProject,
}: ExecutionViewProps) {
  const projectId = execution.project.id
  const projectLabel = execution.project.repository?.displayName ?? execution.project.name
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
    handleRequestRename,
    handleRequestDelete,
    handleRequestNewFile,
    handleRequestNewFolder,
    handleMoveEntry,
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
    moveProjectEntry,
    deleteProjectEntry,
  })

  const [editorView, setEditorView] = useState<CodeMirrorView | null>(null)
  const [findState, setFindState] = useState<{
    open: boolean
    query: string
    token: number
  }>({ open: false, query: '', token: 0 })
  const pendingJumpRef = useRef<{ path: string; line: number; column: number } | null>(null)

  const handleOpenFind = useCallback(
    ({ initialQuery }: { withReplace: boolean; initialQuery: string }) => {
      setFindState((prev) => ({
        open: true,
        query: initialQuery || prev.query,
        token: prev.token + 1,
      }))
    },
    [],
  )

  const handleCloseFind = useCallback(() => {
    setFindState((prev) => ({ ...prev, open: false }))
    editorView?.focus()
  }, [editorView])

  const jumpEditorToCursor = useCallback(
    (line: number, column: number) => {
      const view = editorView
      if (!view) return
      const doc = view.state.doc
      if (line < 1 || line > doc.lines) return
      const target = doc.line(line)
      const pos = Math.min(target.to, target.from + Math.max(0, column - 1))
      view.dispatch({
        selection: { anchor: pos },
        // Scroll centered so the match isn't hidden behind the top/bottom
        // chrome on small editor panes.
        effects: [CodeMirrorView.scrollIntoView(pos, { y: 'center' })],
      })
      view.focus()
    },
    [editorView],
  )

  const handleOpenAtLine = useCallback(
    (path: string, line: number, column: number) => {
      if (activePath === path) {
        jumpEditorToCursor(line, column)
        return
      }
      pendingJumpRef.current = { path, line, column }
      void handleSelectFile(path)
    },
    [activePath, handleSelectFile, jumpEditorToCursor],
  )

  // After a jump-triggered file open finishes, position the cursor once the
  // editor state matches the requested file.
  useEffect(() => {
    const pending = pendingJumpRef.current
    if (!pending) return
    if (pending.path !== activePath) return
    if (isActiveLoading) return
    if (!editorView) return
    pendingJumpRef.current = null
    jumpEditorToCursor(pending.line, pending.column)
  }, [activePath, isActiveLoading, editorView, jumpEditorToCursor])

  return (
    <div className="flex min-h-0 w-full min-w-0 flex-1">
      {findState.open ? (
        <FindReplacePane
          view={editorView}
          projectId={projectId}
          activePath={activePath}
          activeContent={activeContent}
          onClose={handleCloseFind}
          onOpenAtLine={handleOpenAtLine}
          searchProject={searchProject}
          replaceInProject={replaceInProject}
          initialQuery={findState.query}
          openToken={findState.token}
        />
      ) : (
        <ExplorerPane
          searchQuery={searchQuery}
          isTreeLoading={isTreeLoading}
          workspaceError={workspaceError}
          tree={tree}
          activePath={activePath}
          expandedFolders={expandedFolders}
          dirtyPaths={dirtyPaths}
          creatingEntry={newChildTarget}
          onSearchQueryChange={setSearchQuery}
          onSelectFile={handleSelectFile}
          onToggleFolder={handleToggleFolder}
          onRequestRename={handleRequestRename}
          onRequestDelete={handleRequestDelete}
          onRequestNewFile={handleRequestNewFile}
          onRequestNewFolder={handleRequestNewFolder}
          onMoveEntry={handleMoveEntry}
          onCancelCreate={() => setNewChildTarget(null)}
          onCreateEntry={handleCreateSubmit}
          onCopyPath={handleCopyPath}
          onOpenFind={() => handleOpenFind({ withReplace: true, initialQuery: '' })}
          onReload={reloadProjectTree}
        />
      )}

      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <EditorTabs
          openTabs={openTabs}
          activePath={activePath}
          dirtyPaths={dirtyPaths}
          pendingFilePath={pendingFilePath}
          onSelectTab={setActivePath}
          onCloseTab={closeTab}
        />

        {activePath ? (
          <EditorToolbar
            activePath={activePath}
            isDirty={isActiveDirty}
            isSaving={isActiveSaving}
            onRevert={revertActive}
            onSave={() => {
              void saveActive()
            }}
          />
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
                    onOpenFind={handleOpenFind}
                    onViewReady={setEditorView}
                  />
                </div>
                <EditorStatusBar
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
    </div>
  )
}

export function ExecutionView(props: ExecutionViewProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <EditorView {...props} />
    </div>
  )
}
