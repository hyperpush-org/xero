"use client"

import { memo, useEffect, useMemo, useState } from 'react'
import {
  Bot,
  Check,
  FileSearch,
  GitCompare,
  Hash,
  Layers,
  ListTree,
  LocateFixed,
  Play,
  SearchCode,
  Settings2,
  Sparkles,
  Terminal,
  Wand2,
} from 'lucide-react'

import {
  CommandDialog,
  CommandEmpty,
  CommandFooter,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandMeta,
  CommandSeparator,
  CommandShortcut,
} from '@/components/ui/command'
import type { EditorTaskDefinition } from './editor-tasks'

export type EditorCommandTaskStatus = 'running' | 'passed' | 'failed'

export interface EditorCommandPaletteProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  // Navigate
  onQuickOpen?: () => void
  onGoToLine?: () => void
  onGoToSymbol?: () => void
  onGoToDefinition?: () => void
  onFindReferences?: () => void
  onOpenGitChanges?: () => void
  activeGitChangeCount?: number
  // Diagnostics
  onRunTypecheck?: () => void
  onRunLint?: () => void
  typecheckRunning?: boolean
  lintRunning?: boolean
  problemCount?: number
  // Edit
  onFormatDocument?: () => void
  formatRunning?: boolean
  formatOnSave?: boolean
  onToggleFormatOnSave?: () => void
  // Tabs
  dirtyCount?: number
  onSaveAll?: () => void
  onCloseSaved?: () => void
  onCloseOthers?: () => void
  // AI
  hasActiveSelection?: boolean
  agentBusy?: boolean
  onAskAgentAboutSelection?: () => void
  onFixActiveFileWithAgent?: () => void
  // Editor tasks (project start/run targets)
  editorTasks?: EditorTaskDefinition[]
  editorTaskStatusById?: Record<string, EditorCommandTaskStatus>
  onRunEditorTask?: (taskId: string) => void
  // Preferences
  onOpenPreferences?: () => void
}

export const EditorCommandPalette = memo(function EditorCommandPalette({
  open,
  onOpenChange,
  onQuickOpen,
  onGoToLine,
  onGoToSymbol,
  onGoToDefinition,
  onFindReferences,
  onOpenGitChanges,
  activeGitChangeCount = 0,
  onRunTypecheck,
  onRunLint,
  typecheckRunning = false,
  lintRunning = false,
  problemCount = 0,
  onFormatDocument,
  formatRunning = false,
  formatOnSave = false,
  onToggleFormatOnSave,
  dirtyCount = 0,
  onSaveAll,
  onCloseSaved,
  onCloseOthers,
  hasActiveSelection = false,
  agentBusy = false,
  onAskAgentAboutSelection,
  onFixActiveFileWithAgent,
  editorTasks = [],
  editorTaskStatusById = {},
  onRunEditorTask,
  onOpenPreferences,
}: EditorCommandPaletteProps) {
  const [searchValue, setSearchValue] = useState('')

  useEffect(() => {
    if (!open) setSearchValue('')
  }, [open])

  const run = (callback?: () => void) => () => {
    onOpenChange(false)
    callback?.()
  }

  const projectTasks = useMemo(
    () =>
      editorTasks.filter(
        (task) => task.kind !== 'typecheck' && task.kind !== 'lint',
      ),
    [editorTasks],
  )

  const hasNavigate =
    !!onQuickOpen ||
    !!onGoToLine ||
    !!onGoToSymbol ||
    !!onGoToDefinition ||
    !!onFindReferences ||
    (!!onOpenGitChanges && activeGitChangeCount > 0)
  const hasDiagnostics = !!onRunTypecheck || !!onRunLint
  const hasEdit = !!onFormatDocument || !!onToggleFormatOnSave
  const hasTabs = !!onSaveAll || !!onCloseSaved || !!onCloseOthers
  const hasAi = !!onAskAgentAboutSelection || !!onFixActiveFileWithAgent
  const hasProjectTasks = projectTasks.length > 0 && !!onRunEditorTask
  const hasPrefs = !!onOpenPreferences

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Editor commands"
      description="Run an editor command. Search by name."
      footer={<CommandFooter primaryLabel="Run" />}
    >
      <CommandInput
        placeholder="Search editor commands…"
        value={searchValue}
        onValueChange={setSearchValue}
      />
      <CommandList>
        <CommandEmpty>No matching commands.</CommandEmpty>

        {hasNavigate ? (
          <CommandGroup heading="Navigate">
            {onQuickOpen ? (
              <CommandItem value="quick open file go to" onSelect={run(onQuickOpen)}>
                <FileSearch />
                <span>Quick Open File…</span>
                <CommandShortcut>⌘P</CommandShortcut>
              </CommandItem>
            ) : null}
            {onGoToLine ? (
              <CommandItem value="go to line" onSelect={run(onGoToLine)}>
                <Hash />
                <span>Go to Line…</span>
                <CommandShortcut>⌃G</CommandShortcut>
              </CommandItem>
            ) : null}
            {onGoToSymbol ? (
              <CommandItem value="go to symbol" onSelect={run(onGoToSymbol)}>
                <ListTree />
                <span>Go to Symbol…</span>
                <CommandShortcut>⌘⇧O</CommandShortcut>
              </CommandItem>
            ) : null}
            {onGoToDefinition ? (
              <CommandItem value="go to definition" onSelect={run(onGoToDefinition)}>
                <SearchCode />
                <span>Go to Definition</span>
                <CommandShortcut>F12</CommandShortcut>
              </CommandItem>
            ) : null}
            {onFindReferences ? (
              <CommandItem value="find references" onSelect={run(onFindReferences)}>
                <LocateFixed />
                <span>Find References</span>
                <CommandShortcut>⇧F12</CommandShortcut>
              </CommandItem>
            ) : null}
            {onOpenGitChanges && activeGitChangeCount > 0 ? (
              <CommandItem value="show git changes diff" onSelect={run(onOpenGitChanges)}>
                <GitCompare />
                <span>Show Git Changes</span>
                <CommandMeta>
                  {activeGitChangeCount} change{activeGitChangeCount === 1 ? '' : 's'}
                </CommandMeta>
              </CommandItem>
            ) : null}
          </CommandGroup>
        ) : null}

        {hasDiagnostics ? (
          <>
            {hasNavigate ? <CommandSeparator /> : null}
            <CommandGroup heading="Diagnostics">
              {onRunTypecheck ? (
                <CommandItem
                  value="run typecheck project"
                  disabled={typecheckRunning}
                  onSelect={run(onRunTypecheck)}
                >
                  <Play />
                  <span>{typecheckRunning ? 'Typecheck Running…' : 'Run Typecheck'}</span>
                  {problemCount > 0 ? (
                    <CommandMeta className="text-destructive">
                      {problemCount} problem{problemCount === 1 ? '' : 's'}
                    </CommandMeta>
                  ) : null}
                </CommandItem>
              ) : null}
              {onRunLint ? (
                <CommandItem
                  value="run lint project"
                  disabled={lintRunning}
                  onSelect={run(onRunLint)}
                >
                  <Sparkles />
                  <span>{lintRunning ? 'Lint Running…' : 'Run Lint'}</span>
                </CommandItem>
              ) : null}
            </CommandGroup>
          </>
        ) : null}

        {hasEdit ? (
          <>
            {hasNavigate || hasDiagnostics ? <CommandSeparator /> : null}
            <CommandGroup heading="Edit">
              {onFormatDocument ? (
                <CommandItem
                  value="format document"
                  disabled={formatRunning}
                  onSelect={run(onFormatDocument)}
                >
                  <Wand2 />
                  <span>{formatRunning ? 'Formatting…' : 'Format Document'}</span>
                  <CommandShortcut>⌥⇧F</CommandShortcut>
                </CommandItem>
              ) : null}
              {onToggleFormatOnSave ? (
                <CommandItem
                  value="toggle format on save"
                  onSelect={run(onToggleFormatOnSave)}
                >
                  {formatOnSave ? <Check /> : <Wand2 className="opacity-50" />}
                  <span>
                    Format on Save: {formatOnSave ? 'On' : 'Off'}
                  </span>
                </CommandItem>
              ) : null}
            </CommandGroup>
          </>
        ) : null}

        {hasTabs ? (
          <>
            {hasNavigate || hasDiagnostics || hasEdit ? <CommandSeparator /> : null}
            <CommandGroup heading="Tabs">
              {onSaveAll ? (
                <CommandItem
                  value="save all tabs"
                  disabled={dirtyCount === 0}
                  onSelect={run(onSaveAll)}
                >
                  <Layers />
                  <span>Save All</span>
                  {dirtyCount > 0 ? <CommandMeta>{dirtyCount}</CommandMeta> : null}
                </CommandItem>
              ) : null}
              {onCloseSaved ? (
                <CommandItem value="close saved tabs" onSelect={run(onCloseSaved)}>
                  <Layers />
                  <span>Close Saved Tabs</span>
                </CommandItem>
              ) : null}
              {onCloseOthers ? (
                <CommandItem value="close other tabs" onSelect={run(onCloseOthers)}>
                  <Layers />
                  <span>Close Other Tabs</span>
                </CommandItem>
              ) : null}
            </CommandGroup>
          </>
        ) : null}

        {hasAi ? (
          <>
            {hasNavigate || hasDiagnostics || hasEdit || hasTabs ? <CommandSeparator /> : null}
            <CommandGroup heading="AI">
              {onAskAgentAboutSelection ? (
                <CommandItem
                  value="ask agent about selection"
                  disabled={!hasActiveSelection || agentBusy}
                  onSelect={run(onAskAgentAboutSelection)}
                >
                  <Bot />
                  <span>Ask Agent About Selection</span>
                  {!hasActiveSelection ? (
                    <CommandMeta>select code first</CommandMeta>
                  ) : null}
                </CommandItem>
              ) : null}
              {onFixActiveFileWithAgent ? (
                <CommandItem
                  value="fix file with agent"
                  disabled={agentBusy}
                  onSelect={run(onFixActiveFileWithAgent)}
                >
                  <Wand2 />
                  <span>Fix File with Agent</span>
                </CommandItem>
              ) : null}
            </CommandGroup>
          </>
        ) : null}

        {hasProjectTasks ? (
          <>
            {hasNavigate || hasDiagnostics || hasEdit || hasTabs || hasAi ? <CommandSeparator /> : null}
            <CommandGroup heading="Project tasks">
              {projectTasks.map((task) => {
                const status = editorTaskStatusById[task.id]
                return (
                  <CommandItem
                    key={task.id}
                    value={`run task ${task.label} ${task.id}`}
                    disabled={status === 'running'}
                    onSelect={run(() => onRunEditorTask?.(task.id))}
                  >
                    <Terminal />
                    <span>{task.label}</span>
                    {status ? (
                      <CommandMeta>
                        {status === 'running' ? 'running' : status}
                      </CommandMeta>
                    ) : null}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </>
        ) : null}

        {hasPrefs ? (
          <>
            {hasNavigate || hasDiagnostics || hasEdit || hasTabs || hasAi || hasProjectTasks ? (
              <CommandSeparator />
            ) : null}
            <CommandGroup heading="Preferences">
              <CommandItem value="editor preferences settings" onSelect={run(onOpenPreferences)}>
                <Settings2 />
                <span>Editor Preferences…</span>
              </CommandItem>
            </CommandGroup>
          </>
        ) : null}
      </CommandList>
    </CommandDialog>
  )
})
