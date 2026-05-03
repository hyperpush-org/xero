"use client"

import { useEffect, useMemo, useState } from 'react'
import { ArrowLeftRight, Plus, SplitSquareHorizontal, X } from 'lucide-react'

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/components/ui/command'

export interface AgentCommandPalettePane {
  paneId: string
  paneNumber: number
  sessionTitle: string
  isFocused: boolean
}

export interface AgentCommandPaletteProps {
  enabled: boolean
  panes: AgentCommandPalettePane[]
  spawnDisabled: boolean
  onSpawnPane: () => void
  onClosePane: (paneId: string) => void
  onFocusPane: (paneId: string) => void
  onCycleFocus: (delta: number) => void
}

export function AgentCommandPalette({
  enabled,
  panes,
  spawnDisabled,
  onSpawnPane,
  onClosePane,
  onFocusPane,
  onCycleFocus,
}: AgentCommandPaletteProps) {
  const [open, setOpen] = useState(false)
  const [searchValue, setSearchValue] = useState('')

  useEffect(() => {
    if (typeof window === 'undefined') return
    if (!enabled) {
      if (open) setOpen(false)
      return
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      const meta = event.metaKey || event.ctrlKey
      if (meta && !event.shiftKey && !event.altKey && (event.key === 'k' || event.key === 'K')) {
        const target = event.target as HTMLElement | null
        const targetTag = target?.tagName ?? ''
        // Allow ⌘K to open even from textarea/input (matches typical palettes).
        if (targetTag === 'INPUT' || targetTag === 'TEXTAREA' || target?.isContentEditable) {
          // Skip if user is rebinding native ⌘K, but most editors don't, so open.
        }
        event.preventDefault()
        setOpen((prev) => !prev)
      } else if (event.key === 'Escape' && open) {
        setOpen(false)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [enabled, open])

  const focusedPaneNumber = useMemo(
    () => panes.find((pane) => pane.isFocused)?.paneNumber ?? null,
    [panes],
  )
  const canCycle = panes.length > 1
  const announcedCommandValues = useMemo(
    () => [
      'spawn pane new',
      'close focused pane',
      'cycle pane focus next',
      'cycle pane focus previous',
      ...panes.map((pane) => `focus pane ${pane.paneNumber} ${pane.sessionTitle}`),
    ],
    [panes],
  )
  const announcedResultCount = useMemo(() => {
    const query = searchValue.trim().toLowerCase()
    if (!query) {
      return announcedCommandValues.length
    }

    return announcedCommandValues.filter((value) => value.toLowerCase().includes(query)).length
  }, [announcedCommandValues, searchValue])

  const runAndClose = (callback: () => void) => () => {
    callback()
    setOpen(false)
  }

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title="Agent command palette"
      description="Run a workspace command. Search by name or shortcut."
    >
      <CommandInput
        placeholder="Type a command or search..."
        value={searchValue}
        onValueChange={setSearchValue}
      />
      <div role="status" aria-live="polite" className="sr-only">
        {open
          ? `${announcedResultCount} command${announcedResultCount === 1 ? '' : 's'} available.`
          : ''}
      </div>
      <CommandList>
        <CommandEmpty>No matching commands.</CommandEmpty>
        <CommandGroup heading="Workspace">
          <CommandItem
            value="spawn pane new"
            disabled={spawnDisabled}
            onSelect={runAndClose(onSpawnPane)}
          >
            <SplitSquareHorizontal className="size-4" />
            <span>Spawn pane</span>
            <span className="ml-auto text-xs text-muted-foreground">
              {spawnDisabled ? 'limit reached' : '⌘⇧N'}
            </span>
          </CommandItem>
          <CommandItem
            value="close focused pane"
            disabled={panes.length <= 1 || focusedPaneNumber == null}
            onSelect={runAndClose(() => {
              const focused = panes.find((p) => p.isFocused)
              if (focused) onClosePane(focused.paneId)
            })}
          >
            <X className="size-4" />
            <span>Close focused pane</span>
            <span className="ml-auto text-xs text-muted-foreground">⌘W</span>
          </CommandItem>
          <CommandItem
            value="cycle pane focus next"
            disabled={!canCycle}
            onSelect={runAndClose(() => onCycleFocus(1))}
          >
            <ArrowLeftRight className="size-4" />
            <span>Cycle to next pane</span>
            <span className="ml-auto text-xs text-muted-foreground">⌥→</span>
          </CommandItem>
          <CommandItem
            value="cycle pane focus previous"
            disabled={!canCycle}
            onSelect={runAndClose(() => onCycleFocus(-1))}
          >
            <ArrowLeftRight className="size-4 -scale-x-100" />
            <span>Cycle to previous pane</span>
            <span className="ml-auto text-xs text-muted-foreground">⌥←</span>
          </CommandItem>
        </CommandGroup>
        {panes.length > 0 ? (
          <>
            <CommandSeparator />
            <CommandGroup heading="Focus pane">
              {panes.map((pane) => (
                <CommandItem
                  key={pane.paneId}
                  value={`focus pane ${pane.paneNumber} ${pane.sessionTitle}`}
                  onSelect={runAndClose(() => onFocusPane(pane.paneId))}
                >
                  <Plus className="size-4 opacity-0" />
                  <span>
                    Focus pane {pane.paneNumber} — {pane.sessionTitle || 'Untitled'}
                  </span>
                  <span className="ml-auto text-xs text-muted-foreground">
                    {pane.isFocused ? 'focused' : `⌘${pane.paneNumber}`}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          </>
        ) : null}
      </CommandList>
    </CommandDialog>
  )
}
