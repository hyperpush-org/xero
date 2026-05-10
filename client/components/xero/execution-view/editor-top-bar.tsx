import type { ReactNode } from 'react'
import { Code2, Copy, ExternalLink, Eye } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { EditorTabs } from './editor-tabs'
import { ImageControls, type ImageControlsState } from './file-renderers'
import type { FileEditorMode } from './file-editor-host'

interface EditorTopBarProps {
  // Tab strip
  openTabs: string[]
  activePath: string | null
  dirtyPaths: Set<string>
  pendingFilePath: string | null
  onSelectTab: (path: string) => void
  onCloseTab: (path: string) => void
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
  onSave: () => void
  onRevert: () => void
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
  pendingFilePath,
  onSelectTab,
  onCloseTab,
  supportsModeToggle,
  mode,
  onModeChange,
  imageControls,
  onImageControlsChange,
  showSaveControls,
  isDirty,
  isSaving,
  onSave,
  onRevert,
  pathActions,
}: EditorTopBarProps) {
  const showImageControls = imageControls != null && onImageControlsChange != null
  const showPathActions = !!(pathActions?.onCopyPath || pathActions?.onOpenExternal)
  const showRevert = showSaveControls && isDirty
  const hasActions =
    supportsModeToggle || showImageControls || showSaveControls || showPathActions

  return (
    <div
      className="flex shrink-0 items-stretch border-b border-border bg-secondary/10"
      data-testid="editor-top-bar"
    >
      <EditorTabs
        openTabs={openTabs}
        activePath={activePath}
        dirtyPaths={dirtyPaths}
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
          {supportsModeToggle ? (
            <SourcePreviewToggle mode={mode} onModeChange={onModeChange} />
          ) : null}

          {showImageControls ? (
            <>
              {supportsModeToggle ? <Divider /> : null}
              <ImageControls controls={imageControls!} onControlsChange={onImageControlsChange!} />
            </>
          ) : null}

          {showPathActions ? (
            <>
              {(supportsModeToggle || showImageControls) ? <Divider /> : null}
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

          {showSaveControls ? (
            <>
              {(supportsModeToggle || showImageControls || showPathActions) ? <Divider /> : null}
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
