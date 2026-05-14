"use client"

import { lazy, memo, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { EditorView as CodeMirrorView } from '@codemirror/view'
import { DEFAULT_EDITOR_RENDER_PREFERENCES, type EditorRenderPreferences } from './code-editor'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  FormatProjectDocumentRequestDto,
  FormatProjectDocumentResponseDto,
  ListProjectFileIndexRequestDto,
  ListProjectFileIndexResponseDto,
  ListProjectFilesResponseDto,
  MoveProjectEntryRequestDto,
  MoveProjectEntryResponseDto,
  ProjectDiagnosticDto,
  RepositoryDiffResponseDto,
  ProjectLintResponseDto,
  ProjectTypecheckResponseDto,
  ProjectUiStateResponseDto,
  ReadProjectUiStateRequestDto,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  ReplaceInProjectRequestDto,
  ReplaceInProjectResponseDto,
  RunProjectLintRequestDto,
  SearchProjectRequestDto,
  SearchProjectResponseDto,
  StatProjectFilesResponseDto,
  WriteProjectUiStateRequestDto,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import type { ExecutionPaneView } from '@/src/features/xero/use-xero-desktop-state'
import { DeleteFileDialog } from './delete-file-dialog'
import { RenameFileDialog } from './rename-file-dialog'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { AlertTriangle, Bot, GitCompare, RotateCcw } from 'lucide-react'
import { EditorContextMenu } from './execution-view/editor-context-menu'
import { EditorEmptyState, LoadingState } from './execution-view/editor-empty-state'
import { EditorLiveRegion } from './execution-view/editor-live-region'
import { ExplorerPane } from './execution-view/explorer-pane'
import { EditorStatusBar, PreviewStatusBar } from './execution-view/editor-status-bar'
import { EditorTopBar } from './execution-view/editor-top-bar'
import { EditorCommandPalette } from './execution-view/editor-command-palette'
import { ProblemsPeek } from './execution-view/problems-peek'
import {
  EditorNavigationDialog,
  type EditorFileIndexStatus,
  type EditorNavigationMode,
} from './execution-view/editor-navigation-dialog'
import {
  appendEditorTaskOutput,
  buildEditorTaskDefinitions,
  parseEditorTaskProblems,
  type EditorTaskDefinition,
  type EditorTerminalTaskRequest,
} from './execution-view/editor-tasks'
import {
  FileEditorHost,
  defaultModeForResource,
  resourceSupportsPreviewToggle,
  type FileEditorMode,
} from './execution-view/file-editor-host'
import {
  buildGitDiffLineMarkers,
  buildGitHunkPatch,
  buildGitStatusByProjectPath,
  findDiffFileForProjectPath,
  normalizeRepositoryPath,
  type EditorGitDiffFile,
  type EditorGitFileStatus,
} from './execution-view/git-aware-editing'
import {
  buildEditorAgentContextRequest,
  countAgentActivitiesByPath,
  type EditorAgentActivity,
  type EditorAgentContextKind,
  type EditorAgentContextRequest,
  type EditorSelectionContext,
} from './execution-view/agent-aware-editor-hooks'
import {
  DEFAULT_IMAGE_CONTROLS,
  type AssetPreviewResolution,
  type ImageControlsState,
  type ImageDimensions,
} from './execution-view/file-renderers'
import { useExecutionWorkspaceController } from './execution-view/use-execution-workspace-controller'

const LazyFindReplacePane = lazy(() =>
  import('./execution-view/find-replace-pane').then((module) => ({ default: module.FindReplacePane })),
)

export interface ExecutionViewProps {
  execution: ExecutionPaneView
  active?: boolean
  listProjectFileIndex: (
    request: ListProjectFileIndexRequestDto,
  ) => Promise<ListProjectFileIndexResponseDto>
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
  runProjectTypecheck?: (request: { projectId: string }) => Promise<ProjectTypecheckResponseDto>
  formatProjectDocument?: (
    request: FormatProjectDocumentRequestDto,
  ) => Promise<FormatProjectDocumentResponseDto>
  runProjectLint?: (request: RunProjectLintRequestDto) => Promise<ProjectLintResponseDto>
  getRepositoryDiff?: (
    projectId: string,
    scope: 'staged' | 'unstaged' | 'worktree',
  ) => Promise<RepositoryDiffResponseDto>
  gitRevertPatch?: (request: { projectId: string; patch: string }) => Promise<void>
  runEditorTerminalTask?: (request: EditorTerminalTaskRequest) => Promise<string | null>
  revokeProjectAssetTokens?: (projectId: string, paths?: string[]) => Promise<void>
  openProjectFileExternal?: (projectId: string, path: string) => Promise<void>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
  searchProject: (request: SearchProjectRequestDto) => Promise<SearchProjectResponseDto>
  replaceInProject: (request: ReplaceInProjectRequestDto) => Promise<ReplaceInProjectResponseDto>
  agentActivities?: EditorAgentActivity[]
  onSendEditorContextToAgent?: (request: EditorAgentContextRequest) => Promise<void> | void
}

type TypecheckPanelState =
  | { status: 'idle'; response: null; error: null }
  | { status: 'running'; response: ProjectTypecheckResponseDto | null; error: null }
  | { status: 'ready'; response: ProjectTypecheckResponseDto; error: null }
  | { status: 'error'; response: ProjectTypecheckResponseDto | null; error: string }

type LintPanelState =
  | { status: 'idle'; response: null; error: null }
  | { status: 'running'; response: ProjectLintResponseDto | null; error: null }
  | { status: 'ready'; response: ProjectLintResponseDto; error: null }
  | { status: 'error'; response: ProjectLintResponseDto | null; error: string }

type FormatStatus =
  | { status: 'idle' }
  | { status: 'running' }
  | { status: 'formatted'; message?: string }
  | { status: 'unchanged'; message?: string }
  | { status: 'unavailable'; message?: string }
  | { status: 'failed'; message: string }

type GitDiffPanelState =
  | { status: 'idle'; response: null; error: null; revision: string }
  | { status: 'loading'; response: RepositoryDiffResponseDto | null; error: null; revision: string }
  | { status: 'ready'; response: RepositoryDiffResponseDto; error: null; revision: string }
  | { status: 'error'; response: RepositoryDiffResponseDto | null; error: string; revision: string }

type GitHunkActionState =
  | { status: 'idle'; error: null; hunkIndex: null }
  | { status: 'running'; error: null; hunkIndex: number }
  | { status: 'error'; error: string; hunkIndex: number | null }

type EditorTaskRunStatus = 'running' | 'passed' | 'failed'
type AgentContextSendStatus = 'idle' | 'sending' | 'sent' | 'error'

interface EditorTaskRunState {
  runId: string
  taskId: string
  kind: EditorTaskDefinition['kind']
  label: string
  command: string
  status: EditorTaskRunStatus
  terminalId: string | null
  diagnostics: ProjectDiagnosticDto[]
  startedAt: string
  completedAt: string | null
  exitCode: number | null
  truncated: boolean
  message: string | null
}

const FORMAT_ON_SAVE_PREFS_KEY = 'editor.preferences:v1'

interface PersistedEditorPreferences {
  schema: 'xero.editor.preferences.v1'
  formatOnSave: boolean
  fontSize?: number
  tabSize?: number
  insertSpaces?: boolean
  lineWrapping?: boolean
}

function isPersistedEditorPreferences(value: unknown): value is PersistedEditorPreferences {
  if (!value || typeof value !== 'object') return false
  const candidate = value as { schema?: unknown; formatOnSave?: unknown }
  return (
    candidate.schema === 'xero.editor.preferences.v1' &&
    typeof candidate.formatOnSave === 'boolean'
  )
}

const FONT_SIZE_MIN = 10
const FONT_SIZE_MAX = 22
const TAB_SIZE_MIN = 1
const TAB_SIZE_MAX = 8

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min
  return Math.max(min, Math.min(max, value))
}

function copyText(value: string): void {
  if (typeof navigator !== 'undefined' && navigator.clipboard) {
    void navigator.clipboard.writeText(value).catch(() => {})
  }
}

function relativeEditorPath(path: string): string {
  return path.startsWith('/') ? path.slice(1) : path
}

function createUnstagedGitRevision(
  projectId: string,
  entries: ReadonlyArray<ExecutionPaneView['statusEntries'][number]>,
): string {
  const relevant = entries
    .filter((entry) => entry.unstaged || entry.untracked)
    .map((entry) =>
      [
        normalizeRepositoryPath(entry.path),
        entry.unstaged ?? '',
        entry.untracked ? '1' : '0',
      ].join('\u0000'),
    )
    .sort()
    .join('\u0001')
  return relevant ? `${projectId}\u0002${relevant}` : `${projectId}\u0002clean`
}

function editorTaskStatusToPanelStatus(
  task: EditorTaskRunState | undefined,
  fallback: 'idle' | 'running' | 'ready' | 'error',
): 'idle' | 'running' | 'ready' | 'error' {
  if (!task) return fallback
  if (task.status === 'running') return 'running'
  if (task.status === 'passed') return 'ready'
  return 'error'
}

function getEditorWord(view: CodeMirrorView | null): string | null {
  const state = view?.state as
    | (CodeMirrorView['state'] & {
        sliceDoc?: (from: number, to: number) => string
      })
    | undefined
  const selection = state?.selection?.main
  const doc = state?.doc
  if (!selection || !doc) return null

  if (selection.from !== selection.to) {
    const selected =
      typeof state.sliceDoc === 'function'
        ? state.sliceDoc(selection.from, selection.to)
        : doc.toString().slice(selection.from, selection.to)
    return normalizeSymbolWord(selected)
  }

  const content = doc.toString()
  const head = Math.max(0, Math.min(selection.head, content.length))
  let start = head
  let end = head
  while (start > 0 && /[\w$]/.test(content[start - 1] ?? '')) start -= 1
  while (end < content.length && /[\w$]/.test(content[end] ?? '')) end += 1
  return normalizeSymbolWord(content.slice(start, end))
}

function normalizeSymbolWord(value: string): string | null {
  const trimmed = value.trim()
  return /^[A-Za-z_$][\w$]*$/.test(trimmed) ? trimmed : null
}

function sameEditorSelection(
  left: EditorSelectionContext | null | undefined,
  right: EditorSelectionContext | null,
): boolean {
  if (!left || !right) return left === right
  return (
    left.text === right.text &&
    left.fromLine === right.fromLine &&
    left.fromColumn === right.fromColumn &&
    left.toLine === right.toLine &&
    left.toColumn === right.toColumn
  )
}

function findLocalDefinition(content: string, word: string): { line: number; column: number } | null {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const patterns = [
    new RegExp(`\\b(?:function|class|interface|type|enum|struct|fn|def)\\s+${escaped}\\b`),
    new RegExp(`\\b(?:const|let|var)\\s+${escaped}\\b`),
    new RegExp(`\\bfunc\\s+(?:\\([^)]+\\)\\s*)?${escaped}\\b`),
  ]

  const lines = content.split('\n')
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? ''
    if (!patterns.some((pattern) => pattern.test(line))) continue
    const column = line.indexOf(word)
    return {
      line: index + 1,
      column: column >= 0 ? column + 1 : 1,
    }
  }

  return null
}

function EditorView({
  execution,
  active = true,
  listProjectFileIndex,
  listProjectFiles,
  readProjectFile,
  writeProjectFile,
  statProjectFiles,
  readProjectUiState,
  writeProjectUiState,
  runProjectTypecheck,
  formatProjectDocument,
  runProjectLint,
  getRepositoryDiff,
  gitRevertPatch,
  runEditorTerminalTask,
  revokeProjectAssetTokens,
  openProjectFileExternal,
  createProjectEntry,
  renameProjectEntry,
  moveProjectEntry,
  deleteProjectEntry,
  searchProject,
  replaceInProject,
  agentActivities = [],
  onSendEditorContextToAgent,
}: ExecutionViewProps) {
  const projectId = execution.project.id
  const projectLabel = execution.project.repository?.displayName ?? execution.project.name
  const projectRoot = execution.project.repository?.rootPath ?? null
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
    loadingFolders,
    pendingFilePath,
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
  } = useExecutionWorkspaceController({
    projectId,
    active,
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
  })

  const [editorView, setEditorView] = useState<CodeMirrorView | null>(null)
  const [findState, setFindState] = useState<{
    open: boolean
    query: string
    initialScope: 'file' | 'project'
    token: number
  }>({ open: false, query: '', initialScope: 'file', token: 0 })
  const pendingJumpRef = useRef<{ path: string; line: number; column: number } | null>(null)
  const assetPreviewUrlCacheRef = useRef<Map<string, Promise<AssetPreviewResolution>>>(new Map())
  const previousProjectIdRef = useRef(projectId)
  const editorViewRef = useRef<{ path: string; view: CodeMirrorView } | null>(null)
  const [editorModeByPath, setEditorModeByPath] = useState<Record<string, FileEditorMode>>({})
  const [imageControlsByPath, setImageControlsByPath] = useState<Record<string, ImageControlsState>>({})
  const [imageDimensionsByPath, setImageDimensionsByPath] = useState<Record<string, ImageDimensions | null>>({})
  const [typecheckState, setTypecheckState] = useState<TypecheckPanelState>({
    status: 'idle',
    response: null,
    error: null,
  })
  const [lintState, setLintState] = useState<LintPanelState>({
    status: 'idle',
    response: null,
    error: null,
  })
  const [editorTaskStates, setEditorTaskStates] = useState<Record<string, EditorTaskRunState>>({})
  const [formatStatus, setFormatStatus] = useState<FormatStatus>({ status: 'idle' })
  const [formatOnSave, setFormatOnSave] = useState(false)
  const [editorPreferences, setEditorPreferences] = useState<EditorRenderPreferences>(
    DEFAULT_EDITOR_RENDER_PREFERENCES,
  )
  const [preferencesHydrated, setPreferencesHydrated] = useState(false)
  const [navigationMode, setNavigationMode] = useState<EditorNavigationMode | null>(null)
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [problemsPeekOpen, setProblemsPeekOpen] = useState(false)
  const [preferencesPopoverOpen, setPreferencesPopoverOpen] = useState(false)
  const [includeHiddenQuickOpen, setIncludeHiddenQuickOpen] = useState(false)
  const [fileIndexState, setFileIndexState] = useState<{
    status: EditorFileIndexStatus
    response: ListProjectFileIndexResponseDto | null
    error: string | null
    includeHidden: boolean
  }>({ status: 'idle', response: null, error: null, includeHidden: false })
  const [gitDiffState, setGitDiffState] = useState<GitDiffPanelState>({
    status: 'idle',
    response: null,
    error: null,
    revision: `${projectId}\u0002clean`,
  })
  const [gitHunkDialog, setGitHunkDialog] = useState<{ path: string; hunkIndex: number | null } | null>(null)
  const [gitHunkAction, setGitHunkAction] = useState<GitHunkActionState>({
    status: 'idle',
    error: null,
    hunkIndex: null,
  })
  const [selectionByPath, setSelectionByPath] = useState<Record<string, EditorSelectionContext | null>>({})
  const [agentContextStatus, setAgentContextStatus] = useState<AgentContextSendStatus>('idle')
  const [agentContextError, setAgentContextError] = useState<string | null>(null)
  const [agentPreviewActivity, setAgentPreviewActivity] = useState<EditorAgentActivity | null>(null)
  const fileIndexEpochRef = useRef(0)
  const editorTasks = useMemo(
    () => buildEditorTaskDefinitions(execution.project.startTargets ?? []),
    [execution.project.startTargets],
  )
  const typecheckTask = useMemo(
    () => editorTasks.find((task) => task.kind === 'typecheck') ?? null,
    [editorTasks],
  )
  const lintTask = useMemo(
    () => editorTasks.find((task) => task.kind === 'lint') ?? null,
    [editorTasks],
  )

  const activeMode: FileEditorMode = activePath
    ? editorModeByPath[activePath] ?? defaultModeForResource(activeResource)
    : 'source'
  const activeImageControls: ImageControlsState = activePath
    ? imageControlsByPath[activePath] ?? DEFAULT_IMAGE_CONTROLS
    : DEFAULT_IMAGE_CONTROLS
  const activeImageDimensions: ImageDimensions | null = activePath
    ? imageDimensionsByPath[activePath] ?? null
    : null
  const typecheckDiagnostics = typecheckState.response?.diagnostics ?? []
  const lintDiagnostics = lintState.response?.diagnostics ?? []
  const editorTaskDiagnostics = useMemo(
    () => Object.values(editorTaskStates).flatMap((task) => task.diagnostics),
    [editorTaskStates],
  )
  const projectDiagnostics = useMemo(
    () => [...typecheckDiagnostics, ...lintDiagnostics, ...editorTaskDiagnostics],
    [editorTaskDiagnostics, typecheckDiagnostics, lintDiagnostics],
  )
  const editorTaskStatusById = useMemo(() => {
    const statuses: Record<string, EditorTaskRunStatus> = {}
    for (const task of Object.values(editorTaskStates)) {
      statuses[task.taskId] = task.status
    }
    return statuses
  }, [editorTaskStates])
  const diagnosticsByPath = useMemo(() => {
    const counts: Record<string, number> = {}
    for (const diagnostic of projectDiagnostics) {
      if (!diagnostic.path) continue
      counts[diagnostic.path] = (counts[diagnostic.path] ?? 0) + 1
    }
    return counts
  }, [projectDiagnostics])
  const activeDiagnostics = useMemo(
    () => projectDiagnostics.filter((diagnostic) => diagnostic.path === activePath),
    [activePath, projectDiagnostics],
  )
  const gitStatusByPath = useMemo(
    () => buildGitStatusByProjectPath(execution.statusEntries),
    [execution.statusEntries],
  )
  const activeGitStatus = activePath ? gitStatusByPath[activePath] ?? null : null
  const unstagedGitRevision = useMemo(
    () => createUnstagedGitRevision(projectId, execution.statusEntries),
    [execution.statusEntries, projectId],
  )
  const activeGitDiffFile = useMemo(
    () => findDiffFileForProjectPath(gitDiffState.response, activePath),
    [activePath, gitDiffState.response],
  )
  const activeGitDiffMarkers = useMemo(
    () => buildGitDiffLineMarkers(activeGitDiffFile),
    [activeGitDiffFile],
  )
  const activeGitHunkCount = activeGitDiffFile?.hunks.length ?? 0
  const activeGitChangeCount = activeGitHunkCount > 0 ? activeGitHunkCount : activeGitStatus ? 1 : 0
  const activeSelection = activePath ? selectionByPath[activePath] ?? null : null
  const activeHasSelection = Boolean(activeSelection?.text.trim())
  const agentActivityCountsByPath = useMemo(
    () => countAgentActivitiesByPath(agentActivities),
    [agentActivities],
  )
  const activeAgentActivities = useMemo(
    () => (activePath ? agentActivities.filter((activity) => activity.path === activePath) : []),
    [activePath, agentActivities],
  )

  const liveStatusMessage = useMemo(() => {
    if (formatStatus.status === 'running') return 'Formatting document.'
    if (formatStatus.status === 'formatted') return formatStatus.message ?? 'Document formatted.'
    if (formatStatus.status === 'unchanged') return formatStatus.message ?? 'No formatting changes.'
    if (formatStatus.status === 'unavailable') return formatStatus.message ?? 'Formatter unavailable.'
    if (typecheckState.status === 'running') return 'Running typecheck.'
    if (lintState.status === 'running') return 'Running lint.'
    if (projectDiagnostics.length > 0) {
      const errors = projectDiagnostics.filter((diagnostic) => diagnostic.severity === 'error').length
      const warnings = projectDiagnostics.filter((diagnostic) => diagnostic.severity === 'warning').length
      const parts: string[] = []
      if (errors > 0) parts.push(`${errors} error${errors === 1 ? '' : 's'}`)
      if (warnings > 0) parts.push(`${warnings} warning${warnings === 1 ? '' : 's'}`)
      if (parts.length === 0) parts.push(`${projectDiagnostics.length} problems`)
      return `Problems updated: ${parts.join(' and ')}.`
    }
    if (typecheckState.status === 'ready' || lintState.status === 'ready') {
      return 'No problems found.'
    }
    return null
  }, [
    formatStatus,
    lintState.status,
    projectDiagnostics,
    typecheckState.status,
  ])

  const liveAlertMessage = useMemo(() => {
    if (saveConflict) {
      return `${saveConflict.path} changed on disk. Choose Reload, Overwrite, or Keep mine.`
    }
    if (formatStatus.status === 'failed') {
      return `Format failed: ${formatStatus.message}`
    }
    if (typecheckState.status === 'error') {
      return `Typecheck failed: ${typecheckState.error}`
    }
    if (lintState.status === 'error') {
      return `Lint failed: ${lintState.error}`
    }
    if (workspaceError) {
      return `Workspace error: ${workspaceError}`
    }
    if (dirtyGuard) {
      const count = dirtyGuard.paths.length
      return count === 1
        ? 'Unsaved changes. Choose Save, Discard, or Cancel.'
        : `${count} files have unsaved changes. Choose Save, Discard, or Cancel.`
    }
    return null
  }, [dirtyGuard, formatStatus, lintState, saveConflict, typecheckState, workspaceError])

  // When the active project changes the controller resets caches, so prune mode state too.
  useEffect(() => {
    const previousProjectId = previousProjectIdRef.current
    if (previousProjectId !== projectId) {
      void revokeProjectAssetTokens?.(previousProjectId).catch(() => {})
      previousProjectIdRef.current = projectId
    }
    setEditorModeByPath({})
    setImageControlsByPath({})
    setImageDimensionsByPath({})
    assetPreviewUrlCacheRef.current.clear()
    setLintState({ status: 'idle', response: null, error: null })
    setTypecheckState({ status: 'idle', response: null, error: null })
    setEditorTaskStates({})
    setFormatStatus({ status: 'idle' })
    setPreferencesHydrated(false)
    setNavigationMode(null)
    setCommandPaletteOpen(false)
    setProblemsPeekOpen(false)
    setPreferencesPopoverOpen(false)
    setFileIndexState({ status: 'idle', response: null, error: null, includeHidden: false })
    setGitDiffState({ status: 'idle', response: null, error: null, revision: `${projectId}\u0002clean` })
    setGitHunkDialog(null)
    setGitHunkAction({ status: 'idle', error: null, hunkIndex: null })
    setSelectionByPath({})
    setAgentContextStatus('idle')
    setAgentContextError(null)
    setAgentPreviewActivity(null)
    fileIndexEpochRef.current += 1
  }, [projectId, revokeProjectAssetTokens])

  useEffect(() => {
    if (!active || !readProjectUiState || preferencesHydrated) {
      if (!readProjectUiState && !preferencesHydrated) setPreferencesHydrated(true)
      return
    }
    let cancelled = false
    void readProjectUiState({ projectId, key: FORMAT_ON_SAVE_PREFS_KEY })
      .then((response) => {
        if (cancelled) return
        const persisted = response.value
        if (isPersistedEditorPreferences(persisted)) {
          setFormatOnSave(persisted.formatOnSave)
          setEditorPreferences((current) => ({
            fontSize: clamp(
              persisted.fontSize ?? current.fontSize,
              FONT_SIZE_MIN,
              FONT_SIZE_MAX,
            ),
            tabSize: clamp(
              persisted.tabSize ?? current.tabSize,
              TAB_SIZE_MIN,
              TAB_SIZE_MAX,
            ),
            insertSpaces:
              typeof persisted.insertSpaces === 'boolean'
                ? persisted.insertSpaces
                : current.insertSpaces,
            lineWrapping:
              typeof persisted.lineWrapping === 'boolean'
                ? persisted.lineWrapping
                : current.lineWrapping,
          }))
        }
        setPreferencesHydrated(true)
      })
      .catch(() => {
        if (!cancelled) setPreferencesHydrated(true)
      })
    return () => {
      cancelled = true
    }
  }, [active, preferencesHydrated, projectId, readProjectUiState])

  useEffect(() => {
    if (!writeProjectUiState || !preferencesHydrated) return
    const value: PersistedEditorPreferences = {
      schema: 'xero.editor.preferences.v1',
      formatOnSave,
      fontSize: editorPreferences.fontSize,
      tabSize: editorPreferences.tabSize,
      insertSpaces: editorPreferences.insertSpaces,
      lineWrapping: editorPreferences.lineWrapping,
    }
    void writeProjectUiState({ projectId, key: FORMAT_ON_SAVE_PREFS_KEY, value }).catch(() => {})
  }, [editorPreferences, formatOnSave, preferencesHydrated, projectId, writeProjectUiState])

  const resolveAssetPreviewUrl = useCallback(
    (path: string) => {
      const key = `${projectId}:${path}`
      const cached = assetPreviewUrlCacheRef.current.get(key)
      if (cached) {
        return cached
      }

      const request: Promise<AssetPreviewResolution> = readProjectFile(projectId, path)
        .then((response) => {
          if (response.kind === 'renderable' && response.rendererKind === 'image') {
            return {
              path,
              url: response.previewUrl,
              mimeType: response.mimeType,
              byteLength: response.byteLength,
              rendererKind: response.rendererKind,
            }
          }

          if (response.kind === 'unsupported') {
            return {
              path,
              url: null,
              reason:
                response.reason === 'too_large_for_preview' ||
                response.reason === 'too_large_for_text_editing'
                  ? 'oversized'
                  : response.reason === 'binary'
                    ? 'unsupportedType'
                    : 'unavailable',
              mimeType: response.mimeType,
              byteLength: response.byteLength,
              rendererKind: response.rendererKind ?? null,
            } as const
          }

          return {
            path,
            url: null,
            reason: 'unsupportedType',
            mimeType: response.mimeType,
            byteLength: response.byteLength,
            rendererKind: response.rendererKind,
          } as const
        })
        .catch((error) => ({
          path,
          url: null,
          reason: 'missing' as const,
          message: error instanceof Error ? error.message : String(error),
        }))
      assetPreviewUrlCacheRef.current.set(key, request)
      return request
    },
    [projectId, readProjectFile],
  )

  const loadFileIndex = useCallback(
    (includeHidden: boolean) => {
      const epoch = ++fileIndexEpochRef.current
      setFileIndexState((current) => ({
        status: 'loading',
        response: current.response,
        error: null,
        includeHidden,
      }))
      void listProjectFileIndex({
        projectId,
        includeHidden,
      })
        .then((response) => {
          if (epoch !== fileIndexEpochRef.current) return
          setFileIndexState({ status: 'ready', response, error: null, includeHidden })
        })
        .catch((error) => {
          if (epoch !== fileIndexEpochRef.current) return
          setFileIndexState({
            status: 'error',
            response: null,
            error: error instanceof Error ? error.message : String(error),
            includeHidden,
          })
        })
    },
    [listProjectFileIndex, projectId],
  )

  useEffect(() => {
    if (!active || navigationMode !== 'quick-open') return
    if (
      fileIndexState.status === 'idle' ||
      fileIndexState.includeHidden !== includeHiddenQuickOpen
    ) {
      loadFileIndex(includeHiddenQuickOpen)
    }
  }, [
    active,
    fileIndexState.includeHidden,
    fileIndexState.status,
    includeHiddenQuickOpen,
    loadFileIndex,
    navigationMode,
  ])

  useEffect(() => {
    const hasUnstagedChanges = execution.statusEntries.some((entry) => entry.unstaged || entry.untracked)
    if (!active || !getRepositoryDiff || !hasUnstagedChanges) {
      setGitDiffState({
        status: 'idle',
        response: null,
        error: null,
        revision: unstagedGitRevision,
      })
      return
    }

    let cancelled = false
    setGitDiffState((current) => ({
      status: 'loading',
      response: current.response,
      error: null,
      revision: unstagedGitRevision,
    }))
    void getRepositoryDiff(projectId, 'unstaged')
      .then((response) => {
        if (cancelled) return
        setGitDiffState({
          status: 'ready',
          response,
          error: null,
          revision: unstagedGitRevision,
        })
      })
      .catch((error) => {
        if (cancelled) return
        setGitDiffState((current) => ({
          status: 'error',
          response: current.response,
          error: error instanceof Error ? error.message : String(error),
          revision: unstagedGitRevision,
        }))
      })
    return () => {
      cancelled = true
    }
  }, [active, execution.statusEntries, getRepositoryDiff, projectId, unstagedGitRevision])

  const handleEditorViewReady = useCallback((path: string, view: CodeMirrorView | null) => {
    if (view) {
      editorViewRef.current = { path, view }
      setEditorView(view)
      return
    }

    if (editorViewRef.current?.path === path) {
      editorViewRef.current = null
      setEditorView(null)
    }
  }, [])

  const flushEditorSnapshot = useCallback(() => {
    const trackedEditor = editorViewRef.current
    if (!trackedEditor) {
      return undefined
    }

    const snapshot = trackedEditor.view.state.doc.toString()
    handleSnapshotChange(snapshot, trackedEditor.path)
    return snapshot
  }, [handleSnapshotChange])

  const handleActiveSnapshotChange = useCallback(
    (value: string) => {
      if (!activePath) return
      handleSnapshotChange(value, activePath)
    },
    [activePath, handleSnapshotChange],
  )

  const handleActiveDirtyChange = useCallback(
    (isDirty: boolean) => {
      if (!activePath) return
      handleDirtyChange(isDirty, activePath)
    },
    [activePath, handleDirtyChange],
  )

  const handleActiveDocumentStatsChange = useCallback(
    (stats: { lineCount: number }) => {
      if (!activePath) return
      handleDocumentStatsChange(stats, activePath)
    },
    [activePath, handleDocumentStatsChange],
  )

  const handleActiveSelectionChange = useCallback(
    (selection: EditorSelectionContext | null) => {
      if (!activePath) return
      setSelectionByPath((current) => {
        if (sameEditorSelection(current[activePath], selection)) return current
        return { ...current, [activePath]: selection }
      })
    },
    [activePath],
  )

  const handleActiveEditorViewReady = useCallback(
    (view: CodeMirrorView | null) => {
      if (!activePath) return
      handleEditorViewReady(activePath, view)
    },
    [activePath, handleEditorViewReady],
  )

  const handleModeChange = useCallback(
    (mode: FileEditorMode) => {
      if (!activePath) return
      flushEditorSnapshot()
      setEditorModeByPath((current) =>
        current[activePath] === mode ? current : { ...current, [activePath]: mode },
      )
    },
    [activePath, flushEditorSnapshot],
  )

  const handleImageControlsChange = useCallback(
    (next: ImageControlsState) => {
      if (!activePath) return
      setImageControlsByPath((current) => ({ ...current, [activePath]: next }))
    },
    [activePath],
  )

  const handleImageDimensionsChange = useCallback(
    (dimensions: ImageDimensions | null) => {
      if (!activePath) return
      setImageDimensionsByPath((current) => ({ ...current, [activePath]: dimensions }))
    },
    [activePath],
  )

  const activeIsImageView =
    !!activeResource &&
    ((activeResource.kind === 'renderable' && activeResource.rendererKind === 'image') ||
      (activeResource.kind === 'text' && activeResource.rendererKind === 'svg' && activeMode === 'preview'))

  const activeIsRenderable =
    !!activeResource &&
    activeResource.kind === 'renderable' &&
    (activeResource.rendererKind === 'pdf' ||
      activeResource.rendererKind === 'audio' ||
      activeResource.rendererKind === 'video')
  const activeFileVisible =
    !!activePath &&
    (activeNode?.type === 'file' || !!activeResource || pendingFilePath === activePath)

  const activePathActions = activePath && activeIsRenderable
    ? {
        onCopyPath: () => handleCopyPath(activePath),
        onOpenExternal: openProjectFileExternal ? () => handleOpenExternal(activePath) : undefined,
      }
    : undefined

  const formatActiveDocument = useCallback(
    async (snapshot?: string): Promise<string | null> => {
      if (!formatProjectDocument || !activePath || !isActiveText) return null
      const content = snapshot ?? flushEditorSnapshot() ?? activeContent
      if (typeof content !== 'string') return null
      setFormatStatus({ status: 'running' })
      try {
        const response = await formatProjectDocument({
          projectId,
          path: activePath,
          content,
        })
        if (response.status === 'formatted' && response.content != null) {
          setFormatStatus({
            status: 'formatted',
            message: `${response.formatterId ?? 'Formatter'} reformatted ${activePath}.`,
          })
          return response.content
        }
        if (response.status === 'unchanged') {
          setFormatStatus({
            status: 'unchanged',
            message: response.message ?? 'Document already formatted.',
          })
          return null
        }
        if (response.status === 'unavailable') {
          setFormatStatus({
            status: 'unavailable',
            message: response.message ?? 'No formatter is configured for this file.',
          })
          return null
        }
        const failureDiagnostic = response.diagnostics[0]
        setFormatStatus({
          status: 'failed',
          message:
            failureDiagnostic?.message ?? response.message ?? 'Formatter failed.',
        })
        return null
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Formatter failed.'
        setFormatStatus({ status: 'failed', message })
        return null
      }
    },
    [activeContent, activePath, flushEditorSnapshot, formatProjectDocument, isActiveText, projectId],
  )

  const handleFormatActive = useCallback(() => {
    void formatActiveDocument().then((formatted) => {
      if (formatted != null) {
        handleActiveSnapshotChange(formatted)
      }
    })
  }, [formatActiveDocument, handleActiveSnapshotChange])

  const handleSaveActive = useCallback(
    (snapshot?: string) => {
      void (async () => {
        let snapshotToSave = snapshot ?? flushEditorSnapshot()
        if (formatOnSave && formatProjectDocument && isActiveText && activePath) {
          const formatted = await formatActiveDocument(snapshotToSave ?? undefined)
          if (formatted != null) {
            handleActiveSnapshotChange(formatted)
            snapshotToSave = formatted
          }
        }
        await saveActive(snapshotToSave)
      })()
    },
    [
      activePath,
      flushEditorSnapshot,
      formatActiveDocument,
      formatOnSave,
      formatProjectDocument,
      handleActiveSnapshotChange,
      isActiveText,
      saveActive,
    ],
  )

  const handleSaveAll = useCallback(() => {
    const snapshot = flushEditorSnapshot()
    if (activePath && snapshot !== undefined) {
      void saveAll({ [activePath]: snapshot })
      return
    }
    void saveAll()
  }, [activePath, flushEditorSnapshot, saveAll])

  const handleCloseOthers = useCallback(() => {
    flushEditorSnapshot()
    closeOthers()
  }, [closeOthers, flushEditorSnapshot])

  const handleCloseSaved = useCallback(() => {
    flushEditorSnapshot()
    closeSaved()
  }, [closeSaved, flushEditorSnapshot])

  const startEditorTerminalTask = useCallback(
    (task: EditorTaskDefinition): boolean => {
      if (!runEditorTerminalTask) return false
      flushEditorSnapshot()
      if (task.kind === 'typecheck') {
        setTypecheckState({ status: 'idle', response: null, error: null })
      }
      if (task.kind === 'lint') {
        setLintState({ status: 'idle', response: null, error: null })
      }

      const runId = `${task.id}:${Date.now()}`
      const startedAt = new Date().toISOString()
      let output = ''
      let truncated = false

      const updateTaskState = (updater: (current: EditorTaskRunState) => EditorTaskRunState) => {
        setEditorTaskStates((current) => {
          const existing = current[task.id]
          if (!existing || existing.runId !== runId) return current
          return { ...current, [task.id]: updater(existing) }
        })
      }

      setEditorTaskStates((current) => ({
        ...current,
        [task.id]: {
          runId,
          taskId: task.id,
          kind: task.kind,
          label: task.label,
          command: task.command,
          status: 'running',
          terminalId: null,
          diagnostics: [],
          startedAt,
          completedAt: null,
          exitCode: null,
          truncated: false,
          message: `${task.label} running in Terminal.`,
        },
      }))

      void runEditorTerminalTask({
        taskId: task.id,
        kind: task.kind,
        label: task.terminalLabel,
        command: task.command,
        exitWhenDone: true,
        onData: (data) => {
          const next = appendEditorTaskOutput(output, data)
          output = next.output
          truncated = truncated || next.truncated
          const diagnostics = parseEditorTaskProblems(output, { projectRoot })
          updateTaskState((current) => ({
            ...current,
            diagnostics,
            truncated,
          }))
        },
        onExit: ({ terminalId, exitCode }) => {
          const diagnostics = parseEditorTaskProblems(output, { projectRoot })
          const status = exitCode === 0 ? 'passed' : 'failed'
          const message =
            exitCode === 0
              ? diagnostics.length > 0
                ? `${task.label} completed with diagnostics.`
                : `${task.label} passed.`
              : exitCode === null
                ? `${task.label} stopped before reporting an exit code.`
                : `${task.label} exited with code ${exitCode}.`
          updateTaskState((current) => ({
            ...current,
            terminalId,
            diagnostics,
            status,
            exitCode,
            truncated,
            completedAt: new Date().toISOString(),
            message,
          }))
        },
      })
        .then((terminalId) => {
          if (!terminalId) {
            updateTaskState((current) => ({
              ...current,
              status: 'failed',
              completedAt: new Date().toISOString(),
              message: 'Terminal was not ready to run this task.',
            }))
            return
          }
          updateTaskState((current) => ({ ...current, terminalId }))
        })
        .catch((error) => {
          updateTaskState((current) => ({
            ...current,
            status: 'failed',
            completedAt: new Date().toISOString(),
            message: error instanceof Error ? error.message : 'Terminal task failed.',
          }))
        })

      return true
    },
    [flushEditorSnapshot, projectRoot, runEditorTerminalTask],
  )

  const handleRunTypecheck = useCallback(() => {
    if (typecheckTask && startEditorTerminalTask(typecheckTask)) return
    if (!runProjectTypecheck) return
    flushEditorSnapshot()
    setTypecheckState((current) => ({
      status: 'running',
      response: current.response,
      error: null,
    }))
    void runProjectTypecheck({ projectId })
      .then((response) => {
        setTypecheckState({ status: 'ready', response, error: null })
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : 'Typecheck failed.'
        setTypecheckState((current) => ({
          status: 'error',
          response: current.response,
          error: message,
        }))
      })
  }, [flushEditorSnapshot, projectId, runProjectTypecheck, startEditorTerminalTask, typecheckTask])

  const handleRunLint = useCallback(() => {
    if (lintTask && startEditorTerminalTask(lintTask)) return
    if (!runProjectLint) return
    flushEditorSnapshot()
    setLintState((current) => ({
      status: 'running',
      response: current.response,
      error: null,
    }))
    void runProjectLint({ projectId })
      .then((response) => {
        setLintState({ status: 'ready', response, error: null })
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : 'Lint failed.'
        setLintState((current) => ({
          status: 'error',
          response: current.response,
          error: message,
        }))
      })
  }, [flushEditorSnapshot, lintTask, projectId, runProjectLint, startEditorTerminalTask])

  const handleRunEditorTask = useCallback(
    (taskId: string) => {
      const task = editorTasks.find((entry) => entry.id === taskId)
      if (!task) return
      if (task.kind === 'typecheck') {
        handleRunTypecheck()
        return
      }
      if (task.kind === 'lint') {
        handleRunLint()
        return
      }
      startEditorTerminalTask(task)
    },
    [editorTasks, handleRunLint, handleRunTypecheck, startEditorTerminalTask],
  )

  const handleToggleFormatOnSave = useCallback(() => {
    setFormatOnSave((current) => !current)
  }, [])

  const handleSelectTab = useCallback(
    (path: string) => {
      flushEditorSnapshot()
      setActivePath(path)
    },
    [flushEditorSnapshot, setActivePath],
  )

  const handleCloseTab = useCallback(
    (path: string) => {
      flushEditorSnapshot()
      assetPreviewUrlCacheRef.current.delete(`${projectId}:${path}`)
      void revokeProjectAssetTokens?.(projectId, [path]).catch(() => {})
      closeTab(path)
    },
    [closeTab, flushEditorSnapshot, projectId, revokeProjectAssetTokens],
  )

  const handleReloadProjectTree = useCallback(() => {
    assetPreviewUrlCacheRef.current.clear()
    void revokeProjectAssetTokens?.(projectId).catch(() => {})
    reloadProjectTree()
  }, [projectId, reloadProjectTree, revokeProjectAssetTokens])

  const handleOpenExternal = useCallback(
    (path: string) => {
      void openProjectFileExternal?.(projectId, path).catch(() => {})
    },
    [openProjectFileExternal, projectId],
  )

  const handleSelectFileWithSnapshot = useCallback(
    (path: string) => {
      flushEditorSnapshot()
      void handleSelectFile(path)
    },
    [flushEditorSnapshot, handleSelectFile],
  )

  const handleMoveEntryWithSnapshot = useCallback(
    (path: string, targetParentPath: string) => {
      flushEditorSnapshot()
      return handleMoveEntry(path, targetParentPath)
    },
    [flushEditorSnapshot, handleMoveEntry],
  )

  const handleCreateSubmitWithSnapshot = useCallback(
    (name: string) => {
      flushEditorSnapshot()
      return handleCreateSubmit(name)
    },
    [flushEditorSnapshot, handleCreateSubmit],
  )

  const handleRenameSubmitWithSnapshot = useCallback(
    (newName: string) => {
      flushEditorSnapshot()
      return handleRenameSubmit(newName)
    },
    [flushEditorSnapshot, handleRenameSubmit],
  )

  const handleDeleteSubmitWithSnapshot = useCallback(() => {
    flushEditorSnapshot()
    return handleDeleteSubmit()
  }, [flushEditorSnapshot, handleDeleteSubmit])

  const handleOpenFind = useCallback(
    ({
      initialQuery,
      initialScope = 'file',
    }: {
      withReplace: boolean
      initialQuery: string
      initialScope?: 'file' | 'project'
    }) => {
      flushEditorSnapshot()
      setFindState((prev) => ({
        open: true,
        query: initialQuery || prev.query,
        initialScope,
        token: prev.token + 1,
      }))
    },
    [flushEditorSnapshot],
  )

  const handleCloseFind = useCallback(() => {
    setFindState((prev) => ({ ...prev, open: false }))
    editorView?.focus()
  }, [editorView])

  const focusEditorView = useCallback(() => {
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
        scrollIntoView: true,
      })
      view.focus()
    },
    [editorView],
  )

  const handleOpenAtLine = useCallback(
    (path: string, line: number, column: number) => {
      flushEditorSnapshot()
      if (activePath === path) {
        jumpEditorToCursor(line, column)
        return
      }
      pendingJumpRef.current = { path, line, column }
      void handleSelectFile(path)
    },
    [activePath, flushEditorSnapshot, handleSelectFile, jumpEditorToCursor],
  )

  const handleGoToActiveLine = useCallback(
    (line: number, column: number) => {
      if (!activePath) return
      if (activeResource?.kind === 'text' && activeMode !== 'source') {
        pendingJumpRef.current = { path: activePath, line, column }
        setEditorModeByPath((current) => ({ ...current, [activePath]: 'source' }))
        return
      }
      pendingJumpRef.current = null
      jumpEditorToCursor(line, column)
    },
    [activeMode, activePath, activeResource?.kind, jumpEditorToCursor],
  )

  const handleOpenQuickFile = useCallback(
    (path: string) => {
      flushEditorSnapshot()
      void handleSelectFile(path)
    },
    [flushEditorSnapshot, handleSelectFile],
  )

  const handleRevealActiveFile = useCallback(() => {
    if (!activePath) return
    void revealPathInExplorer(activePath)
  }, [activePath, revealPathInExplorer])

  const handleCopyRelativePath = useCallback(() => {
    if (!activePath) return
    copyText(relativeEditorPath(activePath))
  }, [activePath])

  const handleSendEditorContextToAgent = useCallback(
    async (kind: EditorAgentContextKind) => {
      if (!activePath || !isActiveText || !onSendEditorContextToAgent) return
      const snapshot = flushEditorSnapshot()
      const content = snapshot ?? activeContent
      const request = buildEditorAgentContextRequest({
        kind,
        path: activePath,
        content,
        savedContent: activeSavedContent,
        isDirty: content !== activeSavedContent,
        selection: selectionByPath[activePath] ?? null,
      })

      setAgentContextStatus('sending')
      setAgentContextError(null)
      try {
        await onSendEditorContextToAgent(request)
        setAgentContextStatus('sent')
      } catch (error) {
        setAgentContextStatus('error')
        setAgentContextError(error instanceof Error ? error.message : 'Could not send editor context to the agent.')
      }
    },
    [
      activeContent,
      activePath,
      activeSavedContent,
      flushEditorSnapshot,
      isActiveText,
      onSendEditorContextToAgent,
      selectionByPath,
    ],
  )

  const handleAskAgentAboutSelection = useCallback(() => {
    void handleSendEditorContextToAgent('ask_selection')
  }, [handleSendEditorContextToAgent])

  const handleFixActiveFileWithAgent = useCallback(() => {
    void handleSendEditorContextToAgent('fix_file')
  }, [handleSendEditorContextToAgent])

  const handleGoToDefinition = useCallback(() => {
    const word = getEditorWord(editorView)
    if (!word || !activePath || !isActiveText) return
    const target = findLocalDefinition(activeContent, word)
    if (target) {
      handleGoToActiveLine(target.line, target.column)
      return
    }
    handleOpenFind({ withReplace: false, initialQuery: word, initialScope: 'project' })
  }, [activeContent, activePath, editorView, handleGoToActiveLine, handleOpenFind, isActiveText])

  const handleFindReferences = useCallback(() => {
    const word = getEditorWord(editorView)
    if (!word || !activePath || !isActiveText) return
    handleOpenFind({ withReplace: false, initialQuery: word, initialScope: 'project' })
  }, [activePath, editorView, handleOpenFind, isActiveText])

  const handleOpenSymbolNavigation = useCallback(() => {
    if (!activePath || !isActiveText) return
    setNavigationMode('go-symbol')
  }, [activePath, isActiveText])

  const handleOpenActiveGitChanges = useCallback(() => {
    if (!activePath || !activeGitStatus) return
    setGitHunkAction({ status: 'idle', error: null, hunkIndex: null })
    setGitHunkDialog({ path: activePath, hunkIndex: null })
  }, [activeGitStatus, activePath])

  const handleGitDiffLineClick = useCallback(
    (marker: { hunkIndex: number }) => {
      if (!activePath || !activeGitStatus) return
      setGitHunkAction({ status: 'idle', error: null, hunkIndex: null })
      setGitHunkDialog({ path: activePath, hunkIndex: marker.hunkIndex })
    },
    [activeGitStatus, activePath],
  )

  const handleRevertGitHunk = useCallback(
    async (file: EditorGitDiffFile, hunkIndex: number) => {
      if (!gitRevertPatch || !activePath || isActiveDirty) return
      const patch = buildGitHunkPatch(file, hunkIndex)
      if (!patch) {
        setGitHunkAction({
          status: 'error',
          error: 'This hunk cannot be reverted because the diff is truncated.',
          hunkIndex,
        })
        return
      }

      setGitHunkAction({ status: 'running', error: null, hunkIndex })
      try {
        await gitRevertPatch({ projectId, patch })
        setGitHunkAction({ status: 'idle', error: null, hunkIndex: null })
        setGitHunkDialog(null)
        reloadProjectTree()
        await handleSelectFile(activePath, { force: true })
      } catch (error) {
        setGitHunkAction({
          status: 'error',
          error: error instanceof Error ? error.message : 'Could not revert this hunk.',
          hunkIndex,
        })
      }
    },
    [activePath, gitRevertPatch, handleSelectFile, isActiveDirty, projectId, reloadProjectTree],
  )

  useEffect(() => {
    if (!active || typeof window === 'undefined') return
    const handleNavigationKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase()
      const mod = event.metaKey || event.ctrlKey
      if (mod && !event.altKey && !event.shiftKey && key === 'p') {
        event.preventDefault()
        setNavigationMode('quick-open')
        return
      }
      if (mod && event.shiftKey && !event.altKey && key === 'p') {
        event.preventDefault()
        setCommandPaletteOpen((prev) => !prev)
        return
      }
      if (event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey && key === 'g') {
        event.preventDefault()
        setNavigationMode('go-line')
        return
      }
      if (mod && event.shiftKey && !event.altKey && key === 'o') {
        event.preventDefault()
        handleOpenSymbolNavigation()
        return
      }
      if (mod && event.altKey && !event.shiftKey && key === 's') {
        event.preventDefault()
        handleSaveAll()
        return
      }
      if (mod && event.altKey && !event.shiftKey && (event.key === 'ArrowRight' || event.key === 'ArrowLeft')) {
        if (openTabs.length === 0 || !activePath) return
        event.preventDefault()
        const currentIndex = openTabs.indexOf(activePath)
        if (currentIndex < 0) return
        const direction = event.key === 'ArrowRight' ? 1 : -1
        const nextIndex = (currentIndex + direction + openTabs.length) % openTabs.length
        const nextPath = openTabs[nextIndex]
        if (nextPath && nextPath !== activePath) {
          flushEditorSnapshot()
          setActivePath(nextPath)
        }
        return
      }
      if (!event.metaKey && !event.ctrlKey && !event.altKey && event.key === 'F12') {
        event.preventDefault()
        if (event.shiftKey) {
          handleFindReferences()
        } else {
          handleGoToDefinition()
        }
      }
    }
    window.addEventListener('keydown', handleNavigationKeyDown)
    return () => window.removeEventListener('keydown', handleNavigationKeyDown)
  }, [
    active,
    activePath,
    flushEditorSnapshot,
    handleFindReferences,
    handleGoToDefinition,
    handleOpenSymbolNavigation,
    handleSaveAll,
    openTabs,
    setActivePath,
  ])

  // Auto-open the problems peek when a diagnostics run completes or a
  // formatter run fails, so the user sees results without an extra click.
  useEffect(() => {
    if (typecheckState.status === 'ready' || typecheckState.status === 'error') {
      setProblemsPeekOpen(true)
    }
  }, [typecheckState.status])
  useEffect(() => {
    if (lintState.status === 'ready' || lintState.status === 'error') {
      setProblemsPeekOpen(true)
    }
  }, [lintState.status])
  useEffect(() => {
    if (formatStatus.status === 'failed') {
      setProblemsPeekOpen(true)
    }
  }, [formatStatus.status])
  useEffect(() => {
    if (editorTaskDiagnostics.length > 0) {
      setProblemsPeekOpen(true)
    }
  }, [editorTaskDiagnostics.length])

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

  const gitDialogStatus: EditorGitFileStatus | null = gitHunkDialog?.path
    ? gitStatusByPath[gitHunkDialog.path] ?? null
    : null
  const gitDialogDiffFile = gitHunkDialog?.path
    ? findDiffFileForProjectPath(gitDiffState.response, gitHunkDialog.path)
    : null

  return (
    <div className="flex min-h-0 w-full min-w-0 flex-1">
      <EditorLiveRegion status={liveStatusMessage} alert={liveAlertMessage} />
      {findState.open ? (
        <Suspense
          fallback={
            <aside
              aria-label="Loading find and replace"
              className="w-[320px] shrink-0 border-r border-border bg-background"
            />
          }
        >
          <LazyFindReplacePane
            view={editorView}
            projectId={projectId}
            activePath={activePath}
            activeContent={activeContent}
            onClose={handleCloseFind}
            onOpenAtLine={handleOpenAtLine}
            searchProject={searchProject}
            replaceInProject={replaceInProject}
            initialQuery={findState.query}
            initialScope={findState.initialScope}
            openToken={findState.token}
          />
        </Suspense>
      ) : (
        <ExplorerPane
          searchQuery={searchQuery}
          isTreeLoading={isTreeLoading}
          workspaceError={workspaceError}
          treeBudgetInfo={treeBudgetInfo}
          tree={tree}
          activePath={activePath}
          expandedFolders={expandedFolders}
          loadingFolders={loadingFolders}
          dirtyPaths={dirtyPaths}
          stalePaths={stalePaths}
          diagnosticCountsByPath={diagnosticsByPath}
          gitStatusByPath={gitStatusByPath}
          agentActivityCountsByPath={agentActivityCountsByPath}
          creatingEntry={newChildTarget}
          onSearchQueryChange={setSearchQuery}
          onSelectFile={handleSelectFileWithSnapshot}
          onToggleFolder={handleToggleFolder}
          onRequestRename={handleRequestRename}
          onRequestDelete={handleRequestDelete}
          onRequestNewFile={handleRequestNewFile}
          onRequestNewFolder={handleRequestNewFolder}
          onMoveEntry={handleMoveEntryWithSnapshot}
          onCancelCreate={() => setNewChildTarget(null)}
          onCreateEntry={handleCreateSubmitWithSnapshot}
          onCopyPath={handleCopyPath}
          onOpenFind={() => handleOpenFind({ withReplace: true, initialQuery: '' })}
          onReload={handleReloadProjectTree}
        />
      )}

      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <EditorTopBar
          openTabs={openTabs}
          activePath={activePath}
          dirtyPaths={dirtyPaths}
          stalePaths={stalePaths}
          diagnosticCountsByPath={diagnosticsByPath}
          gitStatusByPath={gitStatusByPath}
          pendingFilePath={pendingFilePath}
          onSelectTab={handleSelectTab}
          onCloseTab={handleCloseTab}
          onRevealActiveFile={activePath ? handleRevealActiveFile : undefined}
          onCopyRelativePath={activePath ? handleCopyRelativePath : undefined}
          supportsModeToggle={!!activeResource && resourceSupportsPreviewToggle(activeResource)}
          mode={activeMode}
          onModeChange={handleModeChange}
          imageControls={activeIsImageView ? activeImageControls : undefined}
          onImageControlsChange={activeIsImageView ? handleImageControlsChange : undefined}
          showSaveControls={isActiveText}
          isDirty={isActiveDirty}
          isSaving={isActiveSaving}
          staleState={activePath ? stalePaths[activePath] ?? null : null}
          onSave={() => {
            handleSaveActive()
          }}
          onRevert={revertActive}
          onOpenCommandPalette={() => setCommandPaletteOpen(true)}
          pathActions={activePathActions}
        />

        <div className="flex min-h-0 flex-1 flex-col bg-background">
          {activePath && activeFileVisible ? (
            isActiveLoading || !activeResource ? (
              <LoadingState path={activePath} />
            ) : (
              <>
                {agentContextError ? (
                  <div
                    role="alert"
                    className="flex shrink-0 items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 py-1.5 text-[11px] text-destructive"
                  >
                    <AlertTriangle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                    <span className="min-w-0 flex-1 truncate">{agentContextError}</span>
                  </div>
                ) : null}
                {activeAgentActivities.length > 0 ? (
                  <AgentFileActivityBanner
                    activities={activeAgentActivities}
                    isDirty={isActiveDirty}
                    onPreview={(activity) => setAgentPreviewActivity(activity)}
                  />
                ) : null}
                <EditorContextMenu
                  view={isActiveText && activeMode === 'source' ? editorView : null}
                  filePath={activePath}
                  hasSelection={activeHasSelection}
                  onOpenFind={isActiveText ? handleOpenFind : undefined}
                  onGoToLine={
                    activePath && isActiveText ? () => setNavigationMode('go-line') : undefined
                  }
                  onRevealInExplorer={activePath ? handleRevealActiveFile : undefined}
                  onCopyPath={activePath ? () => handleCopyPath(activePath) : undefined}
                >
                  <div className="flex min-h-0 flex-1 overflow-hidden">
                    <FileEditorHost
                      key={activePath}
                      filePath={activePath}
                      resource={activeResource}
                      textValue={activeContent}
                      textSavedValue={activeSavedContent}
                      textDocumentVersion={activeDocumentVersion}
                      preferences={editorPreferences}
                      onSnapshotChange={handleActiveSnapshotChange}
                      onDirtyChange={handleActiveDirtyChange}
                      onDocumentStatsChange={handleActiveDocumentStatsChange}
                      diagnostics={activeDiagnostics}
                      gitDiffMarkers={activeGitDiffMarkers}
                      onSave={handleSaveActive}
                      onCursorChange={setCursor}
                      onSelectionChange={handleActiveSelectionChange}
                      onOpenFind={handleOpenFind}
                      onGitDiffLineClick={handleGitDiffLineClick}
                      onViewReady={handleActiveEditorViewReady}
                      onResolveAssetPreviewUrl={resolveAssetPreviewUrl}
                      sourceLine={cursor.line}
                      onCopyPath={handleCopyPath}
                      onOpenExternal={openProjectFileExternal ? handleOpenExternal : undefined}
                      mode={activeMode}
                      imageControls={activeIsImageView ? activeImageControls : undefined}
                      onImageDimensionsChange={activeIsImageView ? handleImageDimensionsChange : undefined}
                    />
                  </div>
                </EditorContextMenu>
                {isActiveText && (activeMode === 'source' || !resourceSupportsPreviewToggle(activeResource)) ? (
                  <EditorStatusBar
                    cursor={cursor}
                    lang={activeLang}
                    lineCount={activeLineCount}
                    isDirty={isActiveDirty}
                    isSaving={isActiveSaving}
                    staleState={activePath ? stalePaths[activePath] ?? null : null}
                    diagnosticCount={projectDiagnostics.length}
                    problemsBusy={
                      typecheckState.status === 'running' ||
                      lintState.status === 'running' ||
                      Object.values(editorTaskStates).some((task) => task.status === 'running')
                    }
                    problemsPeekOpen={problemsPeekOpen}
                    onToggleProblemsPeek={() => setProblemsPeekOpen((prev) => !prev)}
                    gitStatus={activeGitStatus}
                    documentSettings={
                      activeResource?.kind === 'text' ? activeResource.documentSettings : null
                    }
                    byteLength={activeResource?.byteLength}
                    preferences={editorPreferences}
                    onPreferencesChange={setEditorPreferences}
                    preferencesOpen={preferencesPopoverOpen}
                    onPreferencesOpenChange={setPreferencesPopoverOpen}
                  />
                ) : (
                  <PreviewStatusBar
                    rendererKind={activeResource.rendererKind ?? 'binary'}
                    mimeType={activeResource.mimeType}
                    byteLength={activeResource.byteLength}
                    dimensions={activeIsImageView ? activeImageDimensions : null}
                  />
                )}
              </>
            )
          ) : (
            <EditorEmptyState loadingPath={pendingFilePath} projectLabel={projectLabel} />
          )}
        </div>
        <ProblemsPeek
          open={problemsPeekOpen}
          onOpenChange={setProblemsPeekOpen}
          typecheckState={typecheckState}
          lintState={lintState}
          editorTaskStates={Object.values(editorTaskStates)}
          formatStatus={formatStatus}
          onRunTypecheck={
            runProjectTypecheck || runEditorTerminalTask ? handleRunTypecheck : undefined
          }
          onRunLint={
            runProjectLint || runEditorTerminalTask ? handleRunLint : undefined
          }
          onOpenAtLine={handleOpenAtLine}
        />
      </section>

      <EditorNavigationDialog
        mode={navigationMode}
        open={navigationMode !== null}
        activePath={activePath}
        activeContent={activeContent}
        activeLineCount={activeLineCount}
        cursor={cursor}
        files={fileIndexState.response?.files ?? []}
        fileIndexStatus={fileIndexState.status}
        fileIndexError={fileIndexState.error}
        fileIndexTruncated={fileIndexState.response?.truncated ?? false}
        includeHidden={includeHiddenQuickOpen}
        onIncludeHiddenChange={setIncludeHiddenQuickOpen}
        onRefreshFileIndex={() => loadFileIndex(includeHiddenQuickOpen)}
        onOpenChange={(open) => {
          if (!open) setNavigationMode(null)
        }}
        onOpenFile={handleOpenQuickFile}
        onGoToLine={handleGoToActiveLine}
        onCloseFocus={focusEditorView}
      />
      <EditorCommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        onQuickOpen={() => setNavigationMode('quick-open')}
        onGoToLine={activePath ? () => setNavigationMode('go-line') : undefined}
        onGoToSymbol={isActiveText ? handleOpenSymbolNavigation : undefined}
        onGoToDefinition={isActiveText ? handleGoToDefinition : undefined}
        onFindReferences={isActiveText ? handleFindReferences : undefined}
        onOpenGitChanges={activeGitStatus ? handleOpenActiveGitChanges : undefined}
        activeGitChangeCount={activeGitChangeCount}
        onRunTypecheck={
          runProjectTypecheck || runEditorTerminalTask ? handleRunTypecheck : undefined
        }
        onRunLint={runProjectLint || runEditorTerminalTask ? handleRunLint : undefined}
        typecheckRunning={
          editorTaskStatusToPanelStatus(editorTaskStates.typecheck, typecheckState.status) ===
          'running'
        }
        lintRunning={
          editorTaskStatusToPanelStatus(editorTaskStates.lint, lintState.status) === 'running'
        }
        problemCount={projectDiagnostics.length}
        onFormatDocument={formatProjectDocument ? handleFormatActive : undefined}
        formatRunning={formatStatus.status === 'running'}
        formatOnSave={formatOnSave}
        onToggleFormatOnSave={formatProjectDocument ? handleToggleFormatOnSave : undefined}
        dirtyCount={dirtyPaths.size}
        onSaveAll={openTabs.length > 0 ? handleSaveAll : undefined}
        onCloseSaved={openTabs.length > 0 ? handleCloseSaved : undefined}
        onCloseOthers={openTabs.length > 1 ? handleCloseOthers : undefined}
        hasActiveSelection={activeHasSelection}
        agentBusy={agentContextStatus === 'sending'}
        onAskAgentAboutSelection={
          onSendEditorContextToAgent && isActiveText
            ? handleAskAgentAboutSelection
            : undefined
        }
        onFixActiveFileWithAgent={
          onSendEditorContextToAgent && isActiveText
            ? handleFixActiveFileWithAgent
            : undefined
        }
        editorTasks={runEditorTerminalTask ? editorTasks : undefined}
        editorTaskStatusById={editorTaskStatusById}
        onRunEditorTask={runEditorTerminalTask ? handleRunEditorTask : undefined}
        onOpenPreferences={() => setPreferencesPopoverOpen(true)}
      />
      <RenameFileDialog
        open={!!renameTarget}
        onOpenChange={(open) => {
          if (!open) setRenameTarget(null)
        }}
        currentPath={renameTarget?.path ?? ''}
        type={renameTarget?.type ?? 'file'}
        onRename={(newName) => handleRenameSubmitWithSnapshot(newName)}
      />
      <DeleteFileDialog
        open={!!deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
        path={deleteTarget?.path ?? ''}
        type={deleteTarget?.type ?? 'file'}
        onDelete={() => {
          void handleDeleteSubmitWithSnapshot()
        }}
      />
      <UnsavedChangesDialog
        guard={dirtyGuard}
        onCancel={cancelDirtyGuard}
        onDiscard={discardDirtyGuard}
        onSave={() => {
          void saveDirtyGuard()
        }}
        onCloseFocus={focusEditorView}
      />
      <SaveConflictDialog
        conflict={saveConflict}
        onKeepMine={keepMineSaveConflict}
        onOverwrite={() => {
          void overwriteSaveConflict()
        }}
        onReload={reloadSaveConflictFromDisk}
        onCloseFocus={focusEditorView}
      />
      <AgentEditPreviewDialog
        activity={agentPreviewActivity}
        activeContent={activeContent}
        activePath={activePath}
        activeSavedContent={activeSavedContent}
        isActiveDirty={isActiveDirty}
        onOpenChange={(open) => {
          if (!open) setAgentPreviewActivity(null)
        }}
        projectId={projectId}
        readProjectFile={readProjectFile}
      />
      <GitHunkDialog
        action={gitHunkAction}
        canRevert={!!gitRevertPatch && !isActiveDirty}
        diffError={gitDiffState.error}
        diffFile={gitDialogDiffFile}
        initialHunkIndex={gitHunkDialog?.hunkIndex ?? null}
        isDirty={isActiveDirty}
        onOpenChange={(open) => {
          if (!open) {
            setGitHunkDialog(null)
            setGitHunkAction({ status: 'idle', error: null, hunkIndex: null })
          }
        }}
        onRevertHunk={handleRevertGitHunk}
        onCloseFocus={focusEditorView}
        open={!!gitHunkDialog}
        path={gitHunkDialog?.path ?? ''}
        status={gitDialogStatus}
      />
    </div>
  )
}

function AgentFileActivityBanner({
  activities,
  isDirty,
  onPreview,
}: {
  activities: EditorAgentActivity[]
  isDirty: boolean
  onPreview: (activity: EditorAgentActivity) => void
}) {
  const latest = activities[0]
  if (!latest) return null

  const activeCount = activities.filter((activity) => activity.status === 'active').length
  const statusLabel =
    activeCount > 0
      ? `${activeCount} active ${activeCount === 1 ? 'agent edit' : 'agent edits'}`
      : `${activities.length} recent ${activities.length === 1 ? 'agent edit' : 'agent edits'}`

  return (
    <div
      role="status"
      className="flex shrink-0 items-center gap-2 border-b border-info/25 bg-info/[0.08] px-3 py-1.5 text-[11px] text-foreground"
    >
      <Bot className="h-3.5 w-3.5 shrink-0 text-info" aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate">
        Agent activity on this file: {statusLabel}
        {latest.sessionTitle ? ` from ${latest.sessionTitle}` : ''}.
        {isDirty ? ' Your editor draft is unsaved.' : ''}
      </span>
      <Button
        className="h-6 shrink-0 rounded px-2 text-[11px]"
        onClick={() => onPreview(latest)}
        size="sm"
        type="button"
        variant="ghost"
      >
        Preview
      </Button>
    </div>
  )
}

function AgentEditPreviewDialog({
  activity,
  projectId,
  readProjectFile,
  activePath,
  activeContent,
  activeSavedContent,
  isActiveDirty,
  onOpenChange,
}: {
  activity: EditorAgentActivity | null
  projectId: string
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  activePath: string | null
  activeContent: string
  activeSavedContent: string
  isActiveDirty: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [diskText, setDiskText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const isOpenDirtyFile = Boolean(activity && activePath === activity.path && isActiveDirty)

  useEffect(() => {
    if (!activity) {
      setDiskText(null)
      setError(null)
      setLoading(false)
      return
    }

    let cancelled = false
    setLoading(true)
    setDiskText(null)
    setError(null)
    void readProjectFile(projectId, activity.path)
      .then((response) => {
        if (cancelled) return
        if (response.kind === 'text') {
          setDiskText(response.text)
          return
        }
        setError('This agent-touched file is not text-previewable in the editor.')
      })
      .catch((readError) => {
        if (!cancelled) {
          setError(readError instanceof Error ? readError.message : 'Could not load the agent-touched file.')
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [activity, projectId, readProjectFile])

  const patchAvailability = activity?.patchAvailability ?? null
  const textHunks = patchAvailability?.textHunks?.filter((hunk) => `/${hunk.filePath.replace(/^\/+/, '')}` === activity?.path) ?? []

  return (
    <Dialog open={!!activity} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <div className="flex items-center gap-2 text-info">
            <Bot className="h-5 w-5" aria-hidden="true" />
            <DialogTitle>Agent edit preview</DialogTitle>
          </div>
          <DialogDescription>
            {activity?.path} {activity?.operation ? `was ${activity.operation}` : 'was changed'} by an agent.
          </DialogDescription>
        </DialogHeader>

        {isOpenDirtyFile ? (
          <div className="rounded-md border border-warning/35 bg-warning/10 px-3 py-2 text-[12px] text-warning">
            Your open editor draft has unsaved changes. Review the draft and the agent copy before saving or reloading this file.
          </div>
        ) : null}

        {activity ? (
          <div className="grid gap-2 text-[12px] text-muted-foreground sm:grid-cols-3">
            <PreviewMeta label="Session" value={activity.sessionTitle ?? 'Agent'} />
            <PreviewMeta label="Change group" value={activity.changeGroupId ?? 'Unavailable'} />
            <PreviewMeta label="Patch" value={patchAvailability?.available ? 'Available' : patchAvailability?.unavailableReason ?? 'Unavailable'} />
          </div>
        ) : null}

        {textHunks.length > 0 ? (
          <div className="rounded-md border border-border bg-muted/20">
            <div className="border-b border-border px-3 py-1.5 text-[11px] font-medium text-muted-foreground">
              Patch hunks
            </div>
            <div className="max-h-28 overflow-auto p-2 text-[11px]">
              {textHunks.map((hunk) => (
                <div className="flex items-center justify-between gap-3 py-0.5" key={hunk.hunkId}>
                  <span className="font-mono">{hunk.hunkId}</span>
                  <span className="text-muted-foreground">
                    result {hunk.resultStartLine}-{hunk.resultStartLine + Math.max(0, hunk.resultLineCount - 1)}
                  </span>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {loading ? (
          <div className="rounded-md border border-border bg-muted/20 px-3 py-8 text-center text-[12px] text-muted-foreground">
            Loading agent copy...
          </div>
        ) : error ? (
          <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
            {error}
          </div>
        ) : (
          <div className="grid max-h-80 min-h-0 gap-2 overflow-hidden md:grid-cols-3">
            <ConflictColumn label="Saved base" value={activePath === activity?.path ? activeSavedContent : ''} />
            <ConflictColumn label="Your editor draft" value={activePath === activity?.path ? activeContent : ''} />
            <ConflictColumn label="Agent / disk" value={diskText ?? ''} />
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} type="button">
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function PreviewMeta({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border border-border bg-muted/20 px-2 py-1.5">
      <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </div>
      <div className="truncate text-[12px] text-foreground" title={value}>
        {value}
      </div>
    </div>
  )
}

function UnsavedChangesDialog({
  guard,
  onCancel,
  onDiscard,
  onSave,
  onCloseFocus,
}: {
  guard: { paths: string[]; operation: { kind: string } } | null
  onCancel: () => void
  onDiscard: () => void
  onSave: () => void
  onCloseFocus?: () => void
}) {
  const count = guard?.paths.length ?? 0
  return (
    <Dialog open={!!guard} onOpenChange={(open) => {
      if (!open) onCancel()
    }}>
      <DialogContent
        className="sm:max-w-lg"
        onCloseAutoFocus={(event) => {
          if (onCloseFocus) {
            event.preventDefault()
            onCloseFocus()
          }
        }}
      >
        <DialogHeader>
          <div className="flex items-center gap-2 text-warning">
            <AlertTriangle className="h-5 w-5" aria-hidden="true" />
            <DialogTitle>Unsaved changes</DialogTitle>
          </div>
          <DialogDescription>
            {count === 1
              ? 'This file has unsaved changes.'
              : `${count} files have unsaved changes.`}
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-40 overflow-auto rounded-md border border-border bg-muted/30 p-2 font-mono text-[11px]">
          {(guard?.paths ?? []).map((path) => (
            <div className="truncate" key={path} title={path}>
              {path}
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onCancel} type="button">
            Cancel
          </Button>
          <Button variant="outline" onClick={onDiscard} type="button">
            Discard
          </Button>
          <Button onClick={onSave} type="button">
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function SaveConflictDialog({
  conflict,
  onKeepMine,
  onOverwrite,
  onReload,
  onCloseFocus,
}: {
  conflict: { path: string; mine: string; disk: string } | null
  onKeepMine: () => void
  onOverwrite: () => void
  onReload: () => void
  onCloseFocus?: () => void
}) {
  const [showCompare, setShowCompare] = useState(false)

  useEffect(() => {
    if (conflict) setShowCompare(false)
  }, [conflict])

  return (
    <Dialog open={!!conflict} onOpenChange={(open) => {
      if (!open) onKeepMine()
    }}>
      <DialogContent
        className="sm:max-w-3xl"
        onCloseAutoFocus={(event) => {
          if (onCloseFocus) {
            event.preventDefault()
            onCloseFocus()
          }
        }}
      >
        <DialogHeader>
          <div className="flex items-center gap-2 text-warning">
            <GitCompare className="h-5 w-5" aria-hidden="true" />
            <DialogTitle>File changed on disk</DialogTitle>
          </div>
          <DialogDescription>
            {conflict?.path} changed outside Xero after this tab loaded.
          </DialogDescription>
        </DialogHeader>
        {showCompare && conflict ? (
          <div className="grid max-h-72 min-h-0 grid-cols-2 gap-2 overflow-hidden">
            <ConflictColumn label="Mine" value={conflict.mine} />
            <ConflictColumn label="On disk" value={conflict.disk} />
          </div>
        ) : null}
        <DialogFooter className="gap-2 sm:justify-between">
          <Button variant="outline" onClick={() => setShowCompare((current) => !current)} type="button">
            <GitCompare className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            Compare
          </Button>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={onKeepMine} type="button">
              Keep mine
            </Button>
            <Button variant="outline" onClick={onReload} type="button">
              <RotateCcw className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Reload
            </Button>
            <Button onClick={onOverwrite} type="button">
              Overwrite
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function GitHunkDialog({
  action,
  canRevert,
  diffError,
  diffFile,
  initialHunkIndex,
  isDirty,
  onOpenChange,
  onRevertHunk,
  onCloseFocus,
  open,
  path,
  status,
}: {
  action: GitHunkActionState
  canRevert: boolean
  diffError: string | null
  diffFile: EditorGitDiffFile | null
  initialHunkIndex: number | null
  isDirty: boolean
  onOpenChange: (open: boolean) => void
  onRevertHunk: (file: EditorGitDiffFile, hunkIndex: number) => void
  onCloseFocus?: () => void
  open: boolean
  path: string
  status: EditorGitFileStatus | null
}) {
  const visibleHunks = diffFile?.hunks ?? []
  const selectedHunks =
    initialHunkIndex == null
      ? visibleHunks.map((hunk, index) => ({ hunk, index }))
      : visibleHunks[initialHunkIndex]
        ? [{ hunk: visibleHunks[initialHunkIndex], index: initialHunkIndex }]
        : []

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-3xl"
        onCloseAutoFocus={(event) => {
          if (onCloseFocus) {
            event.preventDefault()
            onCloseFocus()
          }
        }}
      >
        <DialogHeader>
          <div className="flex items-center gap-2 text-primary">
            <GitCompare className="h-5 w-5" aria-hidden="true" />
            <DialogTitle>Git changes</DialogTitle>
          </div>
          <DialogDescription>
            {path} {status ? `is ${status.description}.` : 'has no current Git status.'}
          </DialogDescription>
        </DialogHeader>
        {isDirty ? (
          <div className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-[12px] text-warning">
            Save or discard unsaved editor changes before reverting Git hunks.
          </div>
        ) : null}
        {diffError ? (
          <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
            {diffError}
          </div>
        ) : null}
        {selectedHunks.length > 0 && diffFile ? (
          <div className="max-h-[420px] space-y-3 overflow-auto pr-1 scrollbar-thin">
            {selectedHunks.map(({ hunk, index }) => (
              <div className="rounded-md border border-border" key={`${hunk.header}:${index}`}>
                <div className="flex items-center justify-between gap-3 border-b border-border bg-muted/35 px-3 py-2">
                  <div className="min-w-0">
                    <div className="truncate font-mono text-[11px] text-foreground">
                      {hunk.header}
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      {hunk.rows.length} row{hunk.rows.length === 1 ? '' : 's'}
                      {hunk.truncated ? ' · truncated' : ''}
                    </div>
                  </div>
                  <Button
                    className="h-7 shrink-0 gap-1.5 rounded px-2 text-[11px]"
                    disabled={!canRevert || hunk.truncated || action.status === 'running'}
                    onClick={() => onRevertHunk(diffFile, index)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <RotateCcw className="h-3 w-3" aria-hidden="true" />
                    {action.status === 'running' && action.hunkIndex === index ? 'Reverting' : 'Revert hunk'}
                  </Button>
                </div>
                <pre className="max-h-64 overflow-auto whitespace-pre-wrap px-3 py-2 font-mono text-[11px] leading-5">
                  {[hunk.header, ...hunk.rows.map((row) => `${row.prefix}${row.text}`)].join('\n')}
                </pre>
              </div>
            ))}
          </div>
        ) : (
          <div className="rounded-md border border-border bg-muted/25 px-3 py-3 text-[12px] text-muted-foreground">
            No unstaged hunks are available for this file.
          </div>
        )}
        {action.status === 'error' && action.error ? (
          <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
            {action.error}
          </div>
        ) : null}
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} type="button">
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ConflictColumn({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-h-0 overflow-hidden rounded-md border border-border">
      <div className="border-b border-border bg-muted/40 px-2 py-1 text-[11px] font-medium">
        {label}
      </div>
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap p-2 text-[11px] leading-5">
        {value}
      </pre>
    </div>
  )
}

export const ExecutionView = memo(function ExecutionView(props: ExecutionViewProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <EditorView {...props} />
    </div>
  )
})
