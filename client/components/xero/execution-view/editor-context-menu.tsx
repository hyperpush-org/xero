'use client'

import { useCallback, type ReactNode } from 'react'
import type { EditorView } from '@codemirror/view'
import {
  ArrowDownToLine,
  ArrowRightToLine,
  ClipboardCopy,
  ClipboardPaste,
  Copy,
  Focus,
  ListChecks,
  Scissors,
  Search,
} from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'

export interface EditorContextMenuProps {
  view: EditorView | null
  filePath: string | null
  hasSelection: boolean
  readOnly?: boolean
  onOpenFind?: (options: { withReplace: boolean; initialQuery: string }) => void
  onGoToLine?: () => void
  onRevealInExplorer?: () => void
  onCopyPath?: () => void
  children: ReactNode
}

function selectedText(view: EditorView): string {
  const { from, to } = view.state.selection.main
  return view.state.sliceDoc(from, to)
}

export function EditorContextMenu({
  view,
  filePath,
  hasSelection,
  readOnly = false,
  onOpenFind,
  onGoToLine,
  onRevealInExplorer,
  onCopyPath,
  children,
}: EditorContextMenuProps) {
  const handleCut = useCallback(async () => {
    if (!view) return
    const text = selectedText(view)
    if (!text) return
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      // Browser may block clipboard outside user gesture chain; fall back to nothing.
    }
    if (readOnly) return
    const { from, to } = view.state.selection.main
    view.dispatch({ changes: { from, to, insert: '' } })
    view.focus()
  }, [readOnly, view])

  const handleCopy = useCallback(async () => {
    if (!view) return
    const text = selectedText(view)
    if (!text) return
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      // ignore
    }
    view.focus()
  }, [view])

  const handlePaste = useCallback(async () => {
    if (!view || readOnly) return
    let text = ''
    try {
      text = await navigator.clipboard.readText()
    } catch {
      return
    }
    if (!text) return
    const { from, to } = view.state.selection.main
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
      scrollIntoView: true,
    })
    view.focus()
  }, [readOnly, view])

  const handleSelectAll = useCallback(() => {
    if (!view) return
    view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } })
    view.focus()
  }, [view])

  const handleFindInFile = useCallback(() => {
    if (!view || !onOpenFind) return
    const initial = selectedText(view)
    onOpenFind({ withReplace: false, initialQuery: initial })
  }, [onOpenFind, view])

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-56">
        <ContextMenuItem disabled={!view || !hasSelection || readOnly} onSelect={() => void handleCut()}>
          <Scissors className="size-4" aria-hidden="true" />
          Cut
          <ContextMenuShortcut>⌘X</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem disabled={!view || !hasSelection} onSelect={() => void handleCopy()}>
          <ClipboardCopy className="size-4" aria-hidden="true" />
          Copy
          <ContextMenuShortcut>⌘C</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem disabled={!view || readOnly} onSelect={() => void handlePaste()}>
          <ClipboardPaste className="size-4" aria-hidden="true" />
          Paste
          <ContextMenuShortcut>⌘V</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem disabled={!view} onSelect={handleSelectAll}>
          <ListChecks className="size-4" aria-hidden="true" />
          Select all
          <ContextMenuShortcut>⌘A</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        {onOpenFind ? (
          <ContextMenuItem disabled={!view} onSelect={handleFindInFile}>
            <Search className="size-4" aria-hidden="true" />
            Find…
            <ContextMenuShortcut>⌘F</ContextMenuShortcut>
          </ContextMenuItem>
        ) : null}
        {onGoToLine ? (
          <ContextMenuItem onSelect={onGoToLine}>
            <ArrowDownToLine className="size-4" aria-hidden="true" />
            Go to line…
          </ContextMenuItem>
        ) : null}
        {onOpenFind || onGoToLine ? <ContextMenuSeparator /> : null}
        {onRevealInExplorer && filePath ? (
          <ContextMenuItem onSelect={onRevealInExplorer}>
            <Focus className="size-4" aria-hidden="true" />
            Reveal in Explorer
          </ContextMenuItem>
        ) : null}
        {onCopyPath && filePath ? (
          <ContextMenuItem onSelect={onCopyPath}>
            <Copy className="size-4" aria-hidden="true" />
            Copy path
          </ContextMenuItem>
        ) : null}
        {filePath ? (
          <ContextMenuItem
            disabled
            onSelect={(event) => event.preventDefault()}
          >
            <ArrowRightToLine className="size-4" aria-hidden="true" />
            <span className="truncate font-mono text-[11px]">{filePath}</span>
          </ContextMenuItem>
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  )
}
