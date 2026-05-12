"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { listen } from "@tauri-apps/api/event"
import { isTauri } from "@tauri-apps/api/core"
import { Terminal as XTerm, type ITheme as IXTermTheme } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import { WebLinksAddon } from "@xterm/addon-web-links"
import { Plus, X } from "lucide-react"
import { cn } from "@/lib/utils"
import { useSidebarOpenMotion, useSidebarWidthMotion } from "@/lib/sidebar-motion"
import { createSafeTauriUnlisten } from "@/src/lib/tauri-events"
import { XeroDesktopAdapter as defaultAdapter } from "@/src/lib/xero-desktop"
import type {
  TerminalDataEventPayload,
  TerminalExitEventPayload,
  TerminalTitleEventPayload,
} from "@/src/lib/xero-desktop"
import { useTheme } from "@/src/features/theme/theme-provider"
import type {
  EditorPalette,
  ThemeDefinition,
} from "@/src/features/theme/theme-definitions"

import "@xterm/xterm/css/xterm.css"

const MIN_WIDTH = 360
const DEFAULT_RATIO = 0.34
const RIGHT_PADDING = 200
const TERMINAL_FONT_SIZE = 13
const TERMINAL_FONT_FAMILY =
  'ui-monospace, "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", monospace'
const TERMINAL_SHIFT_ENTER_SEQUENCE = "\x1b[13;2u"
const MAX_TAB_LABEL_LENGTH = 48

/**
 * Build an xterm theme from the active Xero theme. ANSI slots draw from the
 * editor's syntax palette (so red/green/yellow/blue/magenta/cyan stay coherent
 * with the code editor) with semantic fallbacks for slots the editor doesn't
 * carry. The result feels like the terminal is part of the same workspace
 * instead of a chrome-dark island bolted onto the side.
 */
function buildXTermTheme(theme: ThemeDefinition): IXTermTheme {
  const p: EditorPalette = theme.editor
  const c = theme.colors
  return {
    background: p.background,
    foreground: p.foreground,
    cursor: p.cursor,
    cursorAccent: p.background,
    selectionBackground: p.selection,
    selectionInactiveBackground: p.selectionMatch,
    black: p.background,
    brightBlack: p.comment,
    red: p.tagName,
    brightRed: c.destructive,
    green: p.string,
    brightGreen: c.success,
    yellow: p.heading,
    brightYellow: c.warning,
    blue: p.meta,
    brightBlue: c.info,
    magenta: p.keyword,
    brightMagenta: p.control,
    cyan: p.link,
    brightCyan: p.attribute,
    white: p.foreground,
    brightWhite: p.variableDef,
  }
}

export interface TerminalSidebarHandle {
  /**
   * Spawn a new tab and write the given shell command to its stdin. Used by
   * the titlebar Play button to launch the project's start command. Returns
   * the new terminal id, or null if the sidebar isn't ready.
   */
  spawnTabWithCommand: (command: string) => Promise<string | null>
}

interface TerminalSidebarProps {
  open: boolean
  projectId: string | null
  /** Imperative handle exposed to App.tsx so Play can spawn a tab here. */
  registerHandle?: (handle: TerminalSidebarHandle | null) => void
  /** Called when the user opens this sidebar via the titlebar icon. */
  onOpen?: () => void
}

interface TerminalTab {
  id: string
  label: string
  terminal: XTerm
  fit: FitAddon
}

function viewportDefaultWidth(): number {
  if (typeof window === "undefined") return 560
  return Math.round(window.innerWidth * DEFAULT_RATIO)
}

function viewportMaxWidth(): number {
  if (typeof window === "undefined") return 1400
  return Math.max(MIN_WIDTH, window.innerWidth - RIGHT_PADDING)
}

function createXTerm(xtermTheme: IXTermTheme): { terminal: XTerm; fit: FitAddon } {
  const terminal = new XTerm({
    fontFamily: TERMINAL_FONT_FAMILY,
    fontSize: TERMINAL_FONT_SIZE,
    lineHeight: 1.35,
    cursorBlink: true,
    convertEol: false,
    allowProposedApi: true,
    scrollback: 5000,
    theme: xtermTheme,
  })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  terminal.loadAddon(new WebLinksAddon())
  return { terminal, fit }
}

function isPlainShiftEnter(event: KeyboardEvent): boolean {
  return (
    event.type === "keydown" &&
    event.key === "Enter" &&
    event.shiftKey &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey
  )
}

function sanitizeTerminalTabLabel(label: string): string | null {
  const compact = label.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim()
  if (compact.length === 0) return null
  return compact.length > MAX_TAB_LABEL_LENGTH
    ? `${compact.slice(0, MAX_TAB_LABEL_LENGTH - 1)}…`
    : compact
}

export function TerminalSidebar({
  open,
  projectId,
  registerHandle,
}: TerminalSidebarProps) {
  const [width, setWidth] = useState(viewportDefaultWidth)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [tabs, setTabs] = useState<TerminalTab[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const motionOpen = useSidebarOpenMotion(open)
  const targetWidth = motionOpen ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const { theme } = useTheme()
  const xtermTheme = useMemo(() => buildXTermTheme(theme), [theme])
  const xtermThemeRef = useRef(xtermTheme)
  xtermThemeRef.current = xtermTheme

  const widthRef = useRef(width)
  widthRef.current = width
  const tabsRef = useRef<TerminalTab[]>([])
  tabsRef.current = tabs
  const openRef = useRef(open)
  openRef.current = open
  const projectIdRef = useRef<string | null>(projectId)
  projectIdRef.current = projectId
  const terminalViewportRef = useRef<HTMLDivElement | null>(null)
  const terminalHostsRef = useRef<Map<string, HTMLDivElement>>(new Map())
  const openedTerminalIdsRef = useRef<Set<string>>(new Set())
  const pendingWriteBuffersRef = useRef<Map<string, string>>(new Map())
  const closingTerminalIdsRef = useRef<Set<string>>(new Set())
  const autoOpeningTerminalRef = useRef(false)
  const lastTabReplacementPendingRef = useRef(false)

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) ?? null,
    [tabs, activeTabId],
  )

  const updateTabLabel = useCallback((terminalId: string, label: string) => {
    const nextLabel = sanitizeTerminalTabLabel(label)
    if (!nextLabel) return
    setTabs((current) =>
      current.map((tab) =>
        tab.id === terminalId && tab.label !== nextLabel
          ? { ...tab, label: nextLabel }
          : tab,
      ),
    )
  }, [])

  // Subscribe to streaming output + exit events. Writes go straight to the
  // matching xterm instance; if the tab isn't fully wired up yet we buffer.
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    const unlisteners: Array<() => void> = []

    void listen<TerminalDataEventPayload>("terminal:data", (event) => {
      const { terminalId, data } = event.payload
      if (closingTerminalIdsRef.current.has(terminalId)) return
      const tab = tabsRef.current.find((entry) => entry.id === terminalId)
      if (tab) {
        tab.terminal.write(data)
        return
      }
      const buffered = pendingWriteBuffersRef.current.get(terminalId) ?? ""
      pendingWriteBuffersRef.current.set(terminalId, buffered + data)
    }).then((fn) => {
      const unlisten = createSafeTauriUnlisten(fn)
      if (cancelled) {
        unlisten()
      } else {
        unlisteners.push(unlisten)
      }
    })

    void listen<TerminalExitEventPayload>("terminal:exit", (event) => {
      const { terminalId, exitCode } = event.payload
      if (closingTerminalIdsRef.current.has(terminalId)) {
        closingTerminalIdsRef.current.delete(terminalId)
        return
      }
      const tab = tabsRef.current.find((entry) => entry.id === terminalId)
      if (!tab) return
      const code = exitCode ?? null
      tab.terminal.write(`\r\n\x1b[2m[exited${code === null ? '' : ` with code ${code}`}]\x1b[0m\r\n`)
    }).then((fn) => {
      const unlisten = createSafeTauriUnlisten(fn)
      if (cancelled) {
        unlisten()
      } else {
        unlisteners.push(unlisten)
      }
    })

    void listen<TerminalTitleEventPayload>("terminal:title", (event) => {
      const { terminalId, title } = event.payload
      if (closingTerminalIdsRef.current.has(terminalId)) return
      updateTabLabel(terminalId, title)
    }).then((fn) => {
      const unlisten = createSafeTauriUnlisten(fn)
      if (cancelled) {
        unlisten()
      } else {
        unlisteners.push(unlisten)
      }
    })

    return () => {
      cancelled = true
      unlisteners.forEach((fn) => fn())
    }
  }, [updateTabLabel])

  const registerTerminalHost = useCallback((tab: TerminalTab, node: HTMLDivElement | null) => {
    if (!node) {
      terminalHostsRef.current.delete(tab.id)
      return
    }
    terminalHostsRef.current.set(tab.id, node)
    if (openedTerminalIdsRef.current.has(tab.id)) return
    tab.terminal.open(node)
    openedTerminalIdsRef.current.add(tab.id)
  }, [])

  // Keep each xterm mounted once. Switching tabs only changes visibility, then
  // refits the newly active instance after layout has settled.
  useEffect(() => {
    if (!activeTab) return
    const frame = window.requestAnimationFrame(() => {
      try {
        activeTab.fit.fit()
        activeTab.terminal.focus()
      } catch { /* swallow */ }
    })
    return () => {
      window.cancelAnimationFrame(frame)
    }
  }, [activeTab])

  // Push palette changes into every live xterm. Each xterm keeps its own
  // ITerminalOptions copy, so swapping the theme on the provider needs to fan
  // out to all tabs — not just the active one — or background tabs stay
  // painted with the previous palette until they're focused again.
  useEffect(() => {
    for (const tab of tabsRef.current) {
      tab.terminal.options.theme = xtermTheme
    }
  }, [xtermTheme])

  // Resize observer: refit the active terminal whenever the sidebar size
  // changes, then push the new dimensions to the backing PTY.
  useEffect(() => {
    if (!activeTab) return
    const node = terminalViewportRef.current
    if (!node) return
    let raf = 0
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(raf)
      raf = window.requestAnimationFrame(() => {
        try {
          activeTab.fit.fit()
          const cols = activeTab.terminal.cols
          const rows = activeTab.terminal.rows
          if (cols > 0 && rows > 0 && isTauri()) {
            void defaultAdapter.terminalResize?.(activeTab.id, cols, rows)
          }
        } catch { /* swallow */ }
      })
    })
    observer.observe(node)
    return () => {
      observer.disconnect()
      cancelAnimationFrame(raf)
    }
  }, [activeTab])

  const spawnTab = useCallback(
    async (command?: string): Promise<string | null> => {
      if (!isTauri()) return null
      const cols = 120
      const rows = 32
      try {
        const response = await defaultAdapter.terminalOpen?.({
          projectId: projectIdRef.current ?? null,
          cols,
          rows,
        })
        if (!response) return null
        const { terminal, fit } = createXTerm(xtermThemeRef.current)
        terminal.attachCustomKeyEventHandler((event) => {
          if (!isPlainShiftEnter(event)) return true

          event.preventDefault()
          event.stopPropagation()
          void defaultAdapter.terminalWrite?.(
            response.terminalId,
            TERMINAL_SHIFT_ENTER_SEQUENCE,
          )
          return false
        })
        terminal.onData((data) => {
          void defaultAdapter.terminalWrite?.(response.terminalId, data)
        })
        terminal.onResize(({ cols: c, rows: r }) => {
          void defaultAdapter.terminalResize?.(response.terminalId, c, r)
        })
        terminal.onTitleChange((title) => {
          updateTabLabel(response.terminalId, title)
        })
        const buffered = pendingWriteBuffersRef.current.get(response.terminalId)
        if (buffered) {
          terminal.write(buffered)
          pendingWriteBuffersRef.current.delete(response.terminalId)
        }
        const initialLabel = sanitizeTerminalTabLabel(
          response.shell.split("/").pop() ?? response.shell,
        ) ?? "terminal"
        const tab: TerminalTab = {
          id: response.terminalId,
          label: initialLabel,
          terminal,
          fit,
        }
        setTabs((current) => [...current, tab])
        setActiveTabId(response.terminalId)
        if (command && command.trim().length > 0) {
          // Defer the write until the PTY has had a chance to wire up the
          // shell prompt. A small delay is usually enough.
          window.setTimeout(() => {
            void defaultAdapter.terminalWrite?.(
              response.terminalId,
              `${command.trim()}\r`,
            )
          }, 80)
        }
        return response.terminalId
      } catch (error) {
        console.error("Could not open terminal", error)
        return null
      }
    },
    [updateTabLabel],
  )

  const ensureTerminalTab = useCallback(() => {
    if (!isTauri()) return
    if (autoOpeningTerminalRef.current) return
    autoOpeningTerminalRef.current = true
    void spawnTab().finally(() => {
      autoOpeningTerminalRef.current = false
    })
  }, [spawnTab])

  // Auto-create the first tab when the sidebar opens or recovers from an
  // unexpected empty state.
  useEffect(() => {
    if (!open) return
    if (tabs.length > 0) return
    ensureTerminalTab()
  }, [ensureTerminalTab, open, tabs.length])

  useEffect(() => {
    if (!registerHandle) return
    registerHandle({ spawnTabWithCommand: (command) => spawnTab(command) })
    return () => {
      registerHandle(null)
    }
  }, [registerHandle, spawnTab])

  const handleCloseTab = useCallback(
    (id: string) => {
      const snapshot = tabsRef.current
      const tab = snapshot.find((entry) => entry.id === id)
      if (!tab) return
      const remaining = snapshot.filter((entry) => entry.id !== id)
      const closeTab = (fallbackActiveTabId: string | null) => {
        closingTerminalIdsRef.current.add(id)
        terminalHostsRef.current.delete(id)
        openedTerminalIdsRef.current.delete(id)
        tab.terminal.dispose()
        pendingWriteBuffersRef.current.delete(id)
        setTabs((current) => current.filter((entry) => entry.id !== id))
        setActiveTabId((current) => {
          if (current !== id) return current
          return fallbackActiveTabId
        })
        void defaultAdapter.terminalClose?.(id).catch(() => undefined)
      }

      if (remaining.length === 0 && openRef.current && isTauri()) {
        if (lastTabReplacementPendingRef.current) return
        lastTabReplacementPendingRef.current = true
        void spawnTab()
          .then((replacementId) => {
            if (!replacementId) return
            closeTab(replacementId)
          })
          .finally(() => {
            lastTabReplacementPendingRef.current = false
          })
        return
      }

      const fallbackActiveTabId = remaining.length > 0 ? remaining[remaining.length - 1].id : null
      closeTab(fallbackActiveTabId)
    },
    [spawnTab],
  )

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      const startX = event.clientX
      const startWidth = widthRef.current
      const ceiling = viewportMaxWidth()
      setMaxWidth(ceiling)
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        setWidth(Math.max(MIN_WIDTH, Math.min(ceiling, startWidth + delta)))
      }
      const handleUp = () => {
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
        setIsResizing(false)
      }

      window.addEventListener("pointermove", handleMove)
      window.addEventListener("pointerup", handleUp)
      window.addEventListener("pointercancel", handleUp)
    },
    [],
  )

  const handleResizeKey = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
      event.preventDefault()
      const step = event.shiftKey ? 32 : 8
      const ceiling = viewportMaxWidth()
      setMaxWidth(ceiling)
      setWidth((current) => {
        const delta = event.key === "ArrowLeft" ? step : -step
        return Math.max(MIN_WIDTH, Math.min(ceiling, current + delta))
      })
    },
    [],
  )

  // Cleanup on unmount: dispose xterm instances + kill PTYs.
  useEffect(() => {
    return () => {
      const snapshot = tabsRef.current
      snapshot.forEach((tab) => {
        closingTerminalIdsRef.current.add(tab.id)
        terminalHostsRef.current.delete(tab.id)
        openedTerminalIdsRef.current.delete(tab.id)
        try { tab.terminal.dispose() } catch { /* swallow */ }
        void defaultAdapter.terminalClose?.(tab.id).catch(() => undefined)
      })
    }
  }, [])

  return (
    <aside
      aria-hidden={!open}
      aria-label="Terminal sidebar"
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize terminal sidebar"
        aria-orientation="vertical"
        aria-valuemax={maxWidth}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div className="flex h-full min-w-0 shrink-0 flex-col" style={{ width }}>
        <div className="flex h-9 shrink-0 items-center justify-between border-b border-border/70">
          <div className="flex h-full min-w-0 flex-1 items-center gap-1 overflow-x-auto">
            {tabs.map((tab) => (
              <div
                key={tab.id}
                className={cn(
                  // Underline-style tab. Sits full-height inside the h-9 header
                  // strip so the active underline lands exactly on top of the
                  // header's bottom border. No rounding, no border, no fill —
                  // selection is signalled solely by the primary-colored bar.
                  "group relative flex h-full max-w-[180px] shrink-0 items-center gap-1 px-2 text-[11px]",
                  tab.id === activeTabId
                    ? "text-foreground after:absolute after:inset-x-0 after:-bottom-px after:z-10 after:h-[2px] after:bg-primary"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                <button
                  className="flex min-w-0 flex-1 items-center truncate text-left font-mono"
                  onClick={() => setActiveTabId(tab.id)}
                  title={tab.label}
                  type="button"
                >
                  <span className="truncate">{tab.label}</span>
                </button>
                <button
                  aria-label="Close terminal"
                  className="flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity hover:bg-secondary/60 hover:text-foreground group-hover:opacity-100"
                  onClick={() => void handleCloseTab(tab.id)}
                  type="button"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
            <button
              aria-label="New terminal"
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
              onClick={() => void spawnTab()}
              title="New terminal"
              type="button"
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <div
          ref={terminalViewportRef}
          className="relative min-h-0 flex-1 overflow-hidden px-3 pb-3 pt-3"
          onClick={() => {
            activeTab?.terminal.focus()
          }}
          style={{ backgroundColor: xtermTheme.background }}
        >
          {tabs.map((tab) => (
            <div
              key={tab.id}
              ref={(node) => registerTerminalHost(tab, node)}
              className={cn(
                "h-full w-full",
                tab.id === activeTabId ? "block" : "hidden",
              )}
            />
          ))}
        </div>
        {tabs.length === 0 ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 top-9 flex items-center justify-center text-[12px] text-muted-foreground">
            {isTauri() ? "Opening terminal…" : "Terminals are only available in the desktop app."}
          </div>
        ) : null}
      </div>
    </aside>
  )
}
