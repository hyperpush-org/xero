import { Fragment, type ReactNode } from 'react'
import {
  AlertTriangle,
  Bot,
  Check,
  Code2,
  Copy,
  ExternalLink,
  Eye,
  FileSearch,
  GitCompare,
  Hash,
  ListTree,
  LocateFixed,
  Play,
  SearchCode,
  Sparkles,
  Terminal,
  Wand2,
} from 'lucide-react'
import {
  Breadcrumb,
  BreadcrumbEllipsis,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'
import { Slider } from '@/components/ui/slider'
import { Settings2, Layers } from 'lucide-react'
import { cn } from '@/lib/utils'
import { EditorTabs } from './editor-tabs'
import { ImageControls, type ImageControlsState } from './file-renderers'
import type { FileEditorMode } from './file-editor-host'
import type { EditorRenderPreferences } from '../code-editor'
import type { EditorTaskDefinition } from './editor-tasks'
import type { EditorGitFileStatus } from './git-aware-editing'

type EditorTaskMenuStatus = 'running' | 'passed' | 'failed'

interface EditorTopBarProps {
  // Tab strip
  openTabs: string[]
  activePath: string | null
  dirtyPaths: Set<string>
  stalePaths?: Record<string, { kind: 'changed' | 'deleted'; detectedAt: string }>
  diagnosticCountsByPath?: Record<string, number>
  gitStatusByPath?: Record<string, EditorGitFileStatus>
  pendingFilePath: string | null
  onSelectTab: (path: string) => void
  onCloseTab: (path: string) => void
  onQuickOpen?: () => void
  onGoToLine?: () => void
  onGoToSymbol?: () => void
  onGoToDefinition?: () => void
  onFindReferences?: () => void
  onOpenGitChanges?: () => void
  activeGitChangeCount?: number
  canSendEditorContextToAgent?: boolean
  hasActiveSelection?: boolean
  agentContextStatus?: 'idle' | 'sending' | 'sent' | 'error'
  onAskAgentAboutSelection?: () => void
  onFixActiveFileWithAgent?: () => void
  onRevealActiveFile?: () => void
  onCopyRelativePath?: () => void
  // Source / Preview toggle
  supportsModeToggle: boolean
  mode: FileEditorMode
  onModeChange: (mode: FileEditorMode) => void
  // Image controls (only when an image is in view)
  imageControls?: ImageControlsState
  onImageControlsChange?: (next: ImageControlsState) => void
  // Save / Revert (only for dirty text files)
  showSaveControls: boolean
  isDirty: boolean
  isSaving: boolean
  staleState?: { kind: 'changed' | 'deleted'; detectedAt: string } | null
  typecheckStatus?: 'idle' | 'running' | 'ready' | 'error'
  lintStatus?: 'idle' | 'running' | 'ready' | 'error'
  problemCount?: number
  onRunTypecheck?: () => void
  onRunLint?: () => void
  editorTasks?: EditorTaskDefinition[]
  editorTaskStatusById?: Record<string, EditorTaskMenuStatus>
  onRunEditorTask?: (taskId: string) => void
  formatStatus?: 'idle' | 'running' | 'formatted' | 'unchanged' | 'unavailable' | 'failed'
  formatStatusMessage?: string | null
  onFormatDocument?: () => void
  formatOnSave?: boolean
  onToggleFormatOnSave?: () => void
  onSave: () => void
  onRevert: () => void
  // Multi-file ergonomics
  dirtyCount?: number
  onSaveAll?: () => void
  onCloseOthers?: () => void
  onCloseSaved?: () => void
  // Editor preferences
  preferences?: EditorRenderPreferences
  onPreferencesChange?: (next: EditorRenderPreferences) => void
  // File path actions (PDF / media / unsupported)
  pathActions?: {
    onCopyPath?: () => void
    onOpenExternal?: () => void
  }
}

export function EditorTopBar({
  openTabs,
  activePath,
  dirtyPaths,
  stalePaths,
  diagnosticCountsByPath,
  gitStatusByPath,
  pendingFilePath,
  onSelectTab,
  onCloseTab,
  onQuickOpen,
  onGoToLine,
  onGoToSymbol,
  onGoToDefinition,
  onFindReferences,
  onOpenGitChanges,
  activeGitChangeCount = 0,
  canSendEditorContextToAgent = false,
  hasActiveSelection = false,
  agentContextStatus = 'idle',
  onAskAgentAboutSelection,
  onFixActiveFileWithAgent,
  onRevealActiveFile,
  onCopyRelativePath,
  supportsModeToggle,
  mode,
  onModeChange,
  imageControls,
  onImageControlsChange,
  showSaveControls,
  isDirty,
  isSaving,
  staleState,
  typecheckStatus = 'idle',
  lintStatus = 'idle',
  problemCount = 0,
  onRunTypecheck,
  onRunLint,
  editorTasks = [],
  editorTaskStatusById = {},
  onRunEditorTask,
  formatStatus = 'idle',
  formatStatusMessage,
  onFormatDocument,
  formatOnSave = false,
  onToggleFormatOnSave,
  onSave,
  onRevert,
  dirtyCount = 0,
  onSaveAll,
  onCloseOthers,
  onCloseSaved,
  preferences,
  onPreferencesChange,
  pathActions,
}: EditorTopBarProps) {
  const showImageControls = imageControls != null && onImageControlsChange != null
  const showPathActions = !!(pathActions?.onCopyPath || pathActions?.onOpenExternal)
  const showRevert = showSaveControls && isDirty
  const showFormatControls = !!onFormatDocument && showSaveControls
  const showPreferences = !!(preferences && onPreferencesChange)
  const showTabActions = !!(onSaveAll || onCloseOthers || onCloseSaved)
  const showTaskMenu = editorTasks.length > 0 && !!onRunEditorTask
  const showGitChanges = !!onOpenGitChanges && activeGitChangeCount > 0
  const showAgentHooks = canSendEditorContextToAgent && !!(onAskAgentAboutSelection || onFixActiveFileWithAgent)
  const showNavigationActions = !!(
    onQuickOpen ||
    onGoToLine ||
    onGoToSymbol ||
    onGoToDefinition ||
    onFindReferences ||
    showGitChanges
  )
  const hasActions =
    showNavigationActions ||
    supportsModeToggle ||
    showAgentHooks ||
    showImageControls ||
    showSaveControls ||
    showPathActions ||
    !!onRunTypecheck ||
    !!onRunLint ||
    showTaskMenu ||
    !!staleState ||
    showFormatControls ||
    showPreferences ||
    showTabActions

  return (
    <div
      className="flex shrink-0 flex-col border-b border-border bg-secondary/10"
      data-testid="editor-top-bar"
    >
      <div className="flex min-h-9 items-stretch">
        <EditorTabs
          openTabs={openTabs}
          activePath={activePath}
          dirtyPaths={dirtyPaths}
          stalePaths={stalePaths}
          diagnosticCountsByPath={diagnosticCountsByPath}
          gitStatusByPath={gitStatusByPath}
          pendingFilePath={pendingFilePath}
          onSelectTab={onSelectTab}
          onCloseTab={onCloseTab}
        />
        {hasActions ? (
          <div
            className="flex shrink-0 items-center gap-2 px-2"
            role="toolbar"
            aria-label="File actions"
          >
          {showNavigationActions ? (
            <div className="flex items-center gap-0.5">
              {onQuickOpen ? (
                <TopBarIconButton label="Quick open" onClick={onQuickOpen}>
                  <FileSearch className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
              {onGoToLine ? (
                <TopBarIconButton label="Go to line" onClick={onGoToLine}>
                  <Hash className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
              {onGoToSymbol ? (
                <TopBarIconButton label="Go to symbol" onClick={onGoToSymbol}>
                  <ListTree className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
              {onGoToDefinition ? (
                <TopBarIconButton label="Go to definition" onClick={onGoToDefinition}>
                  <SearchCode className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
              {onFindReferences ? (
                <TopBarIconButton label="Find references" onClick={onFindReferences}>
                  <LocateFixed className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
              {showGitChanges ? (
                <TopBarIconButton
                  label={`${activeGitChangeCount} Git change${activeGitChangeCount === 1 ? '' : 's'}`}
                  onClick={onOpenGitChanges!}
                >
                  <GitCompare className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
              ) : null}
            </div>
          ) : null}

          {supportsModeToggle ? (
            <>
              {showNavigationActions ? <Divider /> : null}
            <SourcePreviewToggle mode={mode} onModeChange={onModeChange} />
            </>
          ) : null}

          {showAgentHooks ? (
            <>
              {(showNavigationActions || supportsModeToggle) ? <Divider /> : null}
              <div className="flex items-center gap-0.5">
                {onAskAgentAboutSelection ? (
                  <TopBarIconButton
                    label={
                      hasActiveSelection
                        ? 'Ask agent about selection'
                        : 'Select code before asking the agent'
                    }
                    disabled={!hasActiveSelection || agentContextStatus === 'sending'}
                    onClick={onAskAgentAboutSelection}
                  >
                    <Bot className="h-3 w-3" aria-hidden="true" />
                  </TopBarIconButton>
                ) : null}
                {onFixActiveFileWithAgent ? (
                  <TopBarIconButton
                    label={
                      agentContextStatus === 'sending'
                        ? 'Sending file to agent'
                        : 'Fix this file with agent'
                    }
                    disabled={agentContextStatus === 'sending'}
                    onClick={onFixActiveFileWithAgent}
                  >
                    <Wand2 className="h-3 w-3" aria-hidden="true" />
                  </TopBarIconButton>
                ) : null}
              </div>
            </>
          ) : null}

          {showImageControls ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks) ? <Divider /> : null}
              <ImageControls controls={imageControls!} onControlsChange={onImageControlsChange!} />
            </>
          ) : null}

          {showPathActions ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls) ? <Divider /> : null}
              <div className="flex items-center gap-0.5">
                {pathActions?.onCopyPath ? (
                  <TopBarIconButton label="Copy path" onClick={pathActions.onCopyPath}>
                    <Copy className="h-3 w-3" aria-hidden="true" />
                  </TopBarIconButton>
                ) : null}
                {pathActions?.onOpenExternal ? (
                  <TopBarIconButton label="Open externally" onClick={pathActions.onOpenExternal}>
                    <ExternalLink className="h-3 w-3" aria-hidden="true" />
                  </TopBarIconButton>
                ) : null}
              </div>
            </>
          ) : null}

          {staleState ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions) ? <Divider /> : null}
              <span className="inline-flex items-center gap-1 rounded bg-warning/12 px-1.5 py-0.5 text-[11px] text-warning">
                <AlertTriangle className="h-3 w-3" aria-hidden="true" />
                {staleState.kind === 'deleted' ? 'Deleted on disk' : 'Changed on disk'}
              </span>
            </>
          ) : null}

          {onRunTypecheck ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState) ? <Divider /> : null}
              <TopBarIconButton
                label={
                  typecheckStatus === 'running'
                    ? 'Typecheck running'
                    : problemCount > 0
                      ? `Typecheck (${problemCount} problems)`
                      : 'Run typecheck'
                }
                disabled={typecheckStatus === 'running'}
                onClick={onRunTypecheck}
              >
                <Play className="h-3 w-3" aria-hidden="true" />
              </TopBarIconButton>
            </>
          ) : null}

          {onRunLint ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck) ? <Divider /> : null}
              <TopBarIconButton
                label={
                  lintStatus === 'running'
                    ? 'Lint running'
                    : lintStatus === 'error'
                      ? 'Lint failed'
                      : 'Run lint'
                }
                disabled={lintStatus === 'running'}
                onClick={onRunLint}
              >
                <Sparkles className="h-3 w-3" aria-hidden="true" />
              </TopBarIconButton>
            </>
          ) : null}

          {showTaskMenu ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck || onRunLint) ? <Divider /> : null}
              <EditorTaskMenu
                tasks={editorTasks}
                statusById={editorTaskStatusById}
                onRunTask={onRunEditorTask!}
              />
            </>
          ) : null}

          {showFormatControls ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck || onRunLint || showTaskMenu) ? <Divider /> : null}
              <div className="flex items-center gap-0.5">
                <TopBarIconButton
                  label={
                    formatStatus === 'running'
                      ? 'Formatting…'
                      : formatStatus === 'failed'
                        ? formatStatusMessage ?? 'Format failed'
                        : formatStatus === 'unavailable'
                          ? formatStatusMessage ?? 'No formatter available'
                          : 'Format document'
                  }
                  disabled={formatStatus === 'running'}
                  onClick={onFormatDocument!}
                >
                  <Wand2 className="h-3 w-3" aria-hidden="true" />
                </TopBarIconButton>
                {onToggleFormatOnSave ? (
                  <TopBarIconButton
                    label={
                      formatOnSave
                        ? 'Format on save: on. Click to disable.'
                        : 'Format on save: off. Click to enable.'
                    }
                    pressed={formatOnSave}
                    onClick={onToggleFormatOnSave}
                  >
                    {formatOnSave ? (
                      <Check className="h-3 w-3" aria-hidden="true" />
                    ) : (
                      <Wand2 className="h-3 w-3 opacity-50" aria-hidden="true" />
                    )}
                  </TopBarIconButton>
                ) : null}
              </div>
            </>
          ) : null}

          {showTabActions ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck || onRunLint || showTaskMenu || showFormatControls) ? <Divider /> : null}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    aria-label="Tab actions"
                    className={cn(
                      'inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
                      'hover:bg-secondary/50 hover:text-foreground',
                    )}
                  >
                    <Layers className="h-3 w-3" aria-hidden="true" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-48">
                  {onSaveAll ? (
                    <DropdownMenuItem
                      disabled={dirtyCount === 0}
                      onClick={onSaveAll}
                    >
                      Save all
                      {dirtyCount > 0 ? (
                        <span className="ml-auto text-[10px] text-muted-foreground">
                          {dirtyCount}
                        </span>
                      ) : null}
                    </DropdownMenuItem>
                  ) : null}
                  {onSaveAll && (onCloseOthers || onCloseSaved) ? (
                    <DropdownMenuSeparator />
                  ) : null}
                  {onCloseSaved ? (
                    <DropdownMenuItem onClick={onCloseSaved}>
                      Close saved tabs
                    </DropdownMenuItem>
                  ) : null}
                  {onCloseOthers ? (
                    <DropdownMenuItem onClick={onCloseOthers}>
                      Close other tabs
                    </DropdownMenuItem>
                  ) : null}
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          ) : null}

          {showPreferences ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck || onRunLint || showTaskMenu || showFormatControls || showTabActions) ? <Divider /> : null}
              <EditorPreferencesPopover
                preferences={preferences!}
                onPreferencesChange={onPreferencesChange!}
              />
            </>
          ) : null}

          {showSaveControls ? (
            <>
              {(showNavigationActions || supportsModeToggle || showAgentHooks || showImageControls || showPathActions || staleState || onRunTypecheck || onRunLint || showTaskMenu || showFormatControls || showTabActions || showPreferences) ? <Divider /> : null}
              <div className="flex items-center gap-1">
                {showRevert ? (
                  <button
                    className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
                    onClick={onRevert}
                    type="button"
                  >
                    Revert
                  </button>
                ) : null}
                <button
                  className={cn(
                    'rounded px-2 py-0.5 text-[11px] font-medium transition-colors',
                    isDirty && !isSaving
                      ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                      : 'text-muted-foreground/60',
                  )}
                  disabled={!isDirty || isSaving}
                  onClick={onSave}
                  type="button"
                  title="Save (⌘S)"
                >
                  {isSaving ? 'Saving…' : 'Save'}
                </button>
              </div>
            </>
          ) : null}
          </div>
        ) : null}
      </div>
      {activePath ? (
        <EditorBreadcrumbs
          path={activePath}
          onRevealActiveFile={onRevealActiveFile}
          onCopyRelativePath={onCopyRelativePath}
        />
      ) : null}
    </div>
  )
}

function EditorBreadcrumbs({
  path,
  onRevealActiveFile,
  onCopyRelativePath,
}: {
  path: string
  onRevealActiveFile?: () => void
  onCopyRelativePath?: () => void
}) {
  const segments = path.split('/').filter(Boolean)
  const visibleSegments =
    segments.length <= 4
      ? segments.map((segment, index) => ({ segment, index }))
      : [
          { segment: segments[0]!, index: 0 },
          ...segments.slice(-3).map((segment, offset) => ({
            segment,
            index: segments.length - 3 + offset,
          })),
        ]
  const showEllipsis = segments.length > 4

  return (
    <div className="flex h-7 min-w-0 items-center justify-between gap-3 border-t border-border/60 px-3">
      <Breadcrumb aria-label="Editor breadcrumb" className="min-w-0 overflow-hidden">
        <BreadcrumbList className="flex-nowrap gap-1 text-[11px] sm:gap-1">
          <BreadcrumbItem className="min-w-0">
            <BreadcrumbPage className="font-mono text-[11px] text-muted-foreground">
              project
            </BreadcrumbPage>
          </BreadcrumbItem>
          {showEllipsis ? (
            <>
              <BreadcrumbSeparator className="text-muted-foreground/50 [&>svg]:size-3" />
              <BreadcrumbItem>
                <BreadcrumbEllipsis className="size-4" />
              </BreadcrumbItem>
            </>
          ) : null}
          {visibleSegments.map(({ segment, index }) => {
            const isLast = index === segments.length - 1
            return (
              <Fragment key={`${segment}:${index}`}>
                <BreadcrumbSeparator className="text-muted-foreground/50 [&>svg]:size-3" />
                <BreadcrumbItem className="min-w-0">
                  {isLast ? (
                    <BreadcrumbPage className="truncate font-mono text-[11px] font-medium text-foreground">
                      {segment}
                    </BreadcrumbPage>
                  ) : (
                    <span className="truncate font-mono text-[11px] text-muted-foreground">
                      {segment}
                    </span>
                  )}
                </BreadcrumbItem>
              </Fragment>
            )
          })}
        </BreadcrumbList>
      </Breadcrumb>
      {onRevealActiveFile || onCopyRelativePath ? (
        <div className="flex shrink-0 items-center gap-0.5">
          {onRevealActiveFile ? (
            <TopBarIconButton label="Reveal in explorer" onClick={onRevealActiveFile}>
              <LocateFixed className="h-3 w-3" aria-hidden="true" />
            </TopBarIconButton>
          ) : null}
          {onCopyRelativePath ? (
            <TopBarIconButton label="Copy relative path" onClick={onCopyRelativePath}>
              <Copy className="h-3 w-3" aria-hidden="true" />
            </TopBarIconButton>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function EditorTaskMenu({
  tasks,
  statusById,
  onRunTask,
}: {
  tasks: EditorTaskDefinition[]
  statusById: Record<string, EditorTaskMenuStatus>
  onRunTask: (taskId: string) => void
}) {
  const projectTasks = tasks.filter((task) => task.kind !== 'start')
  const startTasks = tasks.filter((task) => task.kind === 'start')
  const runningCount = tasks.filter((task) => statusById[task.id] === 'running').length

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={runningCount > 0 ? `Editor tasks (${runningCount} running)` : 'Editor tasks'}
          className={cn(
            'inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
            'hover:bg-secondary/50 hover:text-foreground',
          )}
        >
          <Terminal className="h-3 w-3" aria-hidden="true" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
          Editor tasks
        </DropdownMenuLabel>
        {projectTasks.map((task) => (
          <EditorTaskMenuItem
            key={task.id}
            task={task}
            status={statusById[task.id]}
            onRunTask={onRunTask}
          />
        ))}
        {projectTasks.length > 0 && startTasks.length > 0 ? <DropdownMenuSeparator /> : null}
        {startTasks.length > 0 ? (
          <DropdownMenuLabel className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            Start targets
          </DropdownMenuLabel>
        ) : null}
        {startTasks.map((task) => (
          <EditorTaskMenuItem
            key={task.id}
            task={task}
            status={statusById[task.id]}
            onRunTask={onRunTask}
          />
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function EditorTaskMenuItem({
  task,
  status,
  onRunTask,
}: {
  task: EditorTaskDefinition
  status?: EditorTaskMenuStatus
  onRunTask: (taskId: string) => void
}) {
  return (
    <DropdownMenuItem
      disabled={status === 'running'}
      onClick={() => onRunTask(task.id)}
    >
      <Play className="h-3 w-3" aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate">{task.label}</span>
      {status ? (
        <span className="text-[10px] text-muted-foreground">
          {status === 'running' ? 'running' : status}
        </span>
      ) : null}
    </DropdownMenuItem>
  )
}

function Divider() {
  return <span aria-hidden className="h-4 w-px bg-border/60" />
}

function SourcePreviewToggle({
  mode,
  onModeChange,
}: {
  mode: FileEditorMode
  onModeChange: (mode: FileEditorMode) => void
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Editor mode"
      className="inline-flex h-6 items-center rounded-md bg-secondary/40 p-0.5"
    >
      <SegmentedToggleButton
        active={mode === 'source'}
        label="Show source"
        onClick={() => onModeChange('source')}
      >
        <Code2 className="h-3 w-3" aria-hidden="true" />
        <span>Source</span>
      </SegmentedToggleButton>
      <SegmentedToggleButton
        active={mode === 'preview'}
        label="Show preview"
        onClick={() => onModeChange('preview')}
      >
        <Eye className="h-3 w-3" aria-hidden="true" />
        <span>Preview</span>
      </SegmentedToggleButton>
    </div>
  )
}

function SegmentedToggleButton({
  active,
  children,
  label,
  onClick,
}: {
  active: boolean
  children: ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <button
      role="radio"
      type="button"
      aria-label={label}
      aria-checked={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-5 items-center gap-1 rounded px-2 text-[11px] font-medium transition-colors',
        active
          ? 'bg-background text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}

function EditorPreferencesPopover({
  preferences,
  onPreferencesChange,
}: {
  preferences: EditorRenderPreferences
  onPreferencesChange: (next: EditorRenderPreferences) => void
}) {
  const clampFontSize = (value: number) => Math.max(10, Math.min(22, Math.round(value)))
  const clampTabSize = (value: number) => Math.max(1, Math.min(8, Math.round(value)))

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="Editor preferences"
          className={cn(
            'inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
            'hover:bg-secondary/50 hover:text-foreground',
          )}
        >
          <Settings2 className="h-3 w-3" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 space-y-3 text-[12px]">
        <div className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Editor preferences
        </div>
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-[12px]" htmlFor="editor-pref-font-size">
              Font size
            </Label>
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {preferences.fontSize}px
            </span>
          </div>
          <Slider
            id="editor-pref-font-size"
            min={10}
            max={22}
            step={1}
            value={[preferences.fontSize]}
            onValueChange={(values) =>
              onPreferencesChange({
                ...preferences,
                fontSize: clampFontSize(values[0] ?? preferences.fontSize),
              })
            }
          />
        </div>
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-[12px]" htmlFor="editor-pref-tab-size">
              Tab size
            </Label>
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {preferences.tabSize}
            </span>
          </div>
          <Slider
            id="editor-pref-tab-size"
            min={1}
            max={8}
            step={1}
            value={[preferences.tabSize]}
            onValueChange={(values) =>
              onPreferencesChange({
                ...preferences,
                tabSize: clampTabSize(values[0] ?? preferences.tabSize),
              })
            }
          />
        </div>
        <div className="flex items-center justify-between gap-3">
          <Label className="text-[12px]" htmlFor="editor-pref-insert-spaces">
            Insert spaces
          </Label>
          <Switch
            id="editor-pref-insert-spaces"
            checked={preferences.insertSpaces}
            onCheckedChange={(checked) =>
              onPreferencesChange({ ...preferences, insertSpaces: checked })
            }
          />
        </div>
        <div className="flex items-center justify-between gap-3">
          <Label className="text-[12px]" htmlFor="editor-pref-line-wrapping">
            Line wrapping
          </Label>
          <Switch
            id="editor-pref-line-wrapping"
            checked={preferences.lineWrapping}
            onCheckedChange={(checked) =>
              onPreferencesChange({ ...preferences, lineWrapping: checked })
            }
          />
        </div>
      </PopoverContent>
    </Popover>
  )
}

function TopBarIconButton({
  children,
  label,
  onClick,
  disabled,
  pressed,
}: {
  children: ReactNode
  label: string
  onClick: () => void
  disabled?: boolean
  pressed?: boolean
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-pressed={pressed}
          disabled={disabled}
          onClick={onClick}
          className={cn(
            'inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors',
            'hover:bg-secondary/50 hover:text-foreground disabled:pointer-events-none disabled:opacity-40',
            pressed && 'bg-secondary/60 text-foreground',
          )}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
