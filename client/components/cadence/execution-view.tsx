"use client"

import { useCallback, useState } from 'react'
import type { EditorView as CodeMirrorView } from '@codemirror/view'
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
import type { ExecutionPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { CodeEditor } from './code-editor'
import { DeleteFileDialog } from './delete-file-dialog'
import { NewFileDialog } from './new-file-dialog'
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
  const explorerSubtitle = execution.project.repository?.rootPath ?? execution.branchLabel ?? null
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

  const [editorView, setEditorView] = useState<CodeMirrorView | null>(null)
  const [findState, setFindState] = useState<{
    open: boolean
    query: string
    token: number
  }>({ open: false, query: '', token: 0 })

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

  return (
    <div className="flex min-h-0 w-full min-w-0 flex-1">
      {findState.open ? (
        <FindReplacePane
          view={editorView}
          onClose={handleCloseFind}
          initialQuery={findState.query}
          openToken={findState.token}
        />
      ) : (
        <ExplorerPane
          projectLabel={projectLabel}
          subtitle={explorerSubtitle}
          searchQuery={searchQuery}
          isTreeLoading={isTreeLoading}
          workspaceError={workspaceError}
          tree={tree}
          activePath={activePath}
          expandedFolders={expandedFolders}
          dirtyPaths={dirtyPaths}
          onSearchQueryChange={setSearchQuery}
          onSelectFile={handleSelectFile}
          onToggleFolder={handleToggleFolder}
          onRequestRename={handleRequestRename}
          onRequestDelete={handleRequestDelete}
          onRequestNewFile={handleRequestNewFile}
          onRequestNewFolder={handleRequestNewFolder}
          onCopyPath={handleCopyPath}
          onCollapseAll={collapseAll}
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

export function ExecutionView(props: ExecutionViewProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <EditorView {...props} />
    </div>
  )
}
