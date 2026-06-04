"use client"

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react"
import { listen } from "@tauri-apps/api/event"
import { isTauri } from "@tauri-apps/api/core"
import { Terminal as XTerm, type ITheme as IXTermTheme } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import { WebLinksAddon } from "@xterm/addon-web-links"
import { Plus, Settings2, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { useSidebarOpenMotion, useSidebarWidthMotion } from "@/lib/sidebar-motion"
import { createSafeTauriUnlisten } from "@/src/lib/tauri-events"
import { XeroDesktopAdapter as defaultAdapter } from "@/src/lib/xero-desktop"
import type {
  TerminalDataEventPayload,
  TerminalExitEventPayload,
  TerminalSuggestionCandidateDto,
  TerminalTitleEventPayload,
} from "@/src/lib/xero-desktop"
import { useTheme } from "@/src/features/theme/theme-provider"
import {
  browserLaunchTargetLabel,
  extractBrowserSupportedDevServerUrls,
  isBrowserSupportedDevServerUrl,
  makeBrowserLaunchTarget,
  type BrowserLaunchTarget,
} from "./browser-launch-targets"
import type {
  EditorPalette,
  ThemeDefinition,
} from "@xero/ui/theme"
import type { EditorTerminalTaskExit } from "./execution-view/editor-tasks"
import {
  StaleTerminalSuggestionGate,
  TerminalInputTracker,
  acceptedSuggestionWrite,
  isProbablySecretCommand,
  shouldShowCandidate,
  type TerminalSuggestionSnapshot,
} from "./terminal-suggestions"
import {
  loadTerminalSuggestionSettings,
  persistTerminalSuggestionSettings,
  subscribeTerminalSuggestionSettings,
  type TerminalSuggestionSettings,
} from "./terminal-suggestion-settings"

import "@xterm/xterm/css/xterm.css"

const MIN_WIDTH = 360
const DEFAULT_RATIO = 0.34
const RIGHT_PADDING = 200
const TERMINAL_FONT_SIZE = 13
const TERMINAL_FONT_FAMILY =
  'ui-monospace, "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", monospace'
const TERMINAL_SHIFT_ENTER_SEQUENCE = "\x1b[13;2u"
const MAX_TAB_LABEL_LENGTH = 48
const TERMINAL_SUGGESTION_DEBOUNCE_MS = 110

/**
 * Build an xterm theme from the active Xero theme. ANSI slots draw from the
 * editor's syntax palette (so red/green/yellow/blue/magenta/cyan stay coherent
 * with the code editor) with semantic fallbacks for slots the editor doesn't
 * carry. The result feels like the terminal is part of the same workspace
 * instead of a chrome-dark island bolted onto the side.
 */
function withAlpha(color: string, alpha: string): string {
  return /^#[0-9a-f]{6}$/i.test(color) ? `${color}${alpha}` : color
}

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
    scrollbarSliderBackground: withAlpha(p.foreground, "26"),
    scrollbarSliderHoverBackground: withAlpha(p.foreground, "3d"),
    scrollbarSliderActiveBackground: withAlpha(p.foreground, "52"),
  }
}

export interface TerminalSidebarHandle {
  /**
   * Spawn a new tab and write the given shell command to its stdin. Used by
   * the titlebar Play button to launch the project's start command. Returns
   * the new terminal id, or null if the sidebar isn't ready.
   */
  spawnTabWithCommand: (
    command: string,
    options?: TerminalSpawnOptions,
  ) => Promise<string | null>
}

export type TerminalSpawnSource =
  | {
      kind: "start-target"
      targetId?: string | null
      targetName?: string | null
    }
  | {
      kind: "editor-task"
      label?: string | null
    }
  | {
      kind: "xero-command"
      label?: string | null
    }

export interface TerminalSpawnOptions {
  label?: string
  browserSupported?: boolean
  exitWhenDone?: boolean
  source?: TerminalSpawnSource
  onData?: (data: string) => void
  onExit?: (event: EditorTerminalTaskExit) => void
}

interface TerminalSuggestionState {
  terminalId: string
  snapshot: TerminalSuggestionSnapshot
  candidates: TerminalSuggestionCandidateDto[]
  selectedIndex: number
}

interface InternalTerminalSpawnOptions extends TerminalSpawnOptions {
  labelLocked?: boolean
}

interface TerminalSidebarProps {
  open: boolean
  projectId: string | null
  /** Imperative handle exposed to App.tsx so Play can spawn a tab here. */
  registerHandle?: (handle: TerminalSidebarHandle | null) => void
  /** Called when the user opens this sidebar via the titlebar icon. */
  onOpen?: () => void
  onOpenBrowserUrl?: (url: string) => void
  onBrowserLaunchTargetDetected?: (target: BrowserLaunchTarget) => void
}

interface TerminalTab {
  id: string
  projectId: string
  label: string
  labelLocked?: boolean
  browserSupported?: boolean | null
  cwd: string | null
  shell: string
  running: boolean
  terminal: XTerm
  fit: FitAddon
}

interface XTermWithCursorMetrics {
  buffer?: {
    active?: {
      cursorX?: number
      cursorY?: number
    }
  }
  _core?: {
    _renderService?: {
      dimensions?: {
        css?: {
          cell?: {
            width?: number
            height?: number
          }
        }
      }
    }
  }
}

function viewportDefaultWidth(): number {
  if (typeof window === "undefined") return 560
  return Math.round(window.innerWidth * DEFAULT_RATIO)
}

function viewportMaxWidth(): number {
  if (typeof window === "undefined") return 1400
  return Math.max(MIN_WIDTH, window.innerWidth - RIGHT_PADDING)
}

function openExternalLink(uri: string): void {
  const nextWindow = window.open()
  if (!nextWindow) return
  try {
    nextWindow.opener = null
  } catch {
    // Best effort.
  }
  nextWindow.location.href = uri
}

function createXTerm(
  xtermTheme: IXTermTheme,
  handleLink: (uri: string) => void,
): { terminal: XTerm; fit: FitAddon } {
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
  terminal.loadAddon(
    new WebLinksAddon((event, uri) => {
      event.preventDefault()
      handleLink(uri)
    }),
  )
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

function terminalShortcutWrite(event: KeyboardEvent): string | null {
  if (event.type !== "keydown") return null
  if (event.shiftKey) return null

  const hasWordModifier = event.altKey || event.ctrlKey
  if (event.metaKey && !event.altKey && !event.ctrlKey) {
    if (event.key === "ArrowLeft") return "\x01"
    if (event.key === "ArrowRight") return "\x05"
    if (event.key === "ArrowUp") return "\x01"
    if (event.key === "ArrowDown") return "\x05"
    if (event.key === "Backspace") return "\x15"
    if (event.key === "Delete") return "\x0b"
    return null
  }

  if (hasWordModifier && !event.metaKey) {
    if (event.key === "ArrowLeft") return "\x1bb"
    if (event.key === "ArrowRight") return "\x1bf"
    if (event.key === "Backspace") return "\x17"
    if (event.key === "Delete") return "\x1bd"
  }

  return null
}

function sanitizeTerminalTabLabel(label: string): string | null {
  const compact = label.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim()
  if (compact.length === 0) return null
  return compact.length > MAX_TAB_LABEL_LENGTH
    ? `${compact.slice(0, MAX_TAB_LABEL_LENGTH - 1)}…`
    : compact
}

function buildTerminalCommandWrite(command: string, options?: TerminalSpawnOptions): string {
  const trimmed = command.trim()
  if (!trimmed) return ""
  if (!options?.exitWhenDone) return `${trimmed}\r`
  return `(\n${trimmed}\n)\n__xero_task_status=$?; printf '\\n[xero task exited with status %s]\\n' "$__xero_task_status"; exit "$__xero_task_status"\r`
}

export function TerminalSidebar({
  open,
  projectId,
  registerHandle,
  onOpenBrowserUrl,
  onBrowserLaunchTargetDetected,
}: TerminalSidebarProps) {
  const [width, setWidth] = useState(viewportDefaultWidth)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [tabs, setTabs] = useState<TerminalTab[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [suggestionSettings, setSuggestionSettings] = useState<TerminalSuggestionSettings>(
    loadTerminalSuggestionSettings,
  )
  const [suggestionState, setSuggestionState] = useState<TerminalSuggestionState | null>(null)
  const motionOpen = useSidebarOpenMotion(open)
  const targetWidth = motionOpen ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const { theme } = useTheme()
  const xtermTheme = useMemo(() => buildXTermTheme(theme), [theme])
  const xtermThemeRef = useRef(xtermTheme)
  xtermThemeRef.current = xtermTheme
  const onOpenBrowserUrlRef = useRef(onOpenBrowserUrl)
  const onBrowserLaunchTargetDetectedRef = useRef(onBrowserLaunchTargetDetected)
  onOpenBrowserUrlRef.current = onOpenBrowserUrl
  onBrowserLaunchTargetDetectedRef.current = onBrowserLaunchTargetDetected

  const widthRef = useRef(width)
  widthRef.current = width
  const tabsRef = useRef<TerminalTab[]>([])
  tabsRef.current = tabs
  const activeTabIdRef = useRef<string | null>(activeTabId)
  activeTabIdRef.current = activeTabId
  const openRef = useRef(open)
  openRef.current = open
  const projectIdRef = useRef<string | null>(projectId)
  projectIdRef.current = projectId
  const terminalViewportRef = useRef<HTMLDivElement | null>(null)
  const terminalHostsRef = useRef<Map<string, HTMLDivElement>>(new Map())
  const openedTerminalIdsRef = useRef<Set<string>>(new Set())
  const pendingWriteBuffersRef = useRef<Map<string, string>>(new Map())
  const suppressingLiveOutputIdsRef = useRef<Set<string>>(new Set())
  const closingTerminalIdsRef = useRef<Set<string>>(new Set())
  const taskHandlersRef = useRef<Map<string, Pick<TerminalSpawnOptions, "onData" | "onExit">>>(new Map())
  const autoOpeningTerminalRef = useRef(false)
  const autoOpeningTerminalPromiseRef = useRef<Promise<string | null> | null>(null)
  const lastTabReplacementPendingRef = useRef(false)
  const previousProjectIdRef = useRef<string | null>(projectId)
  const activeTabByProjectRef = useRef<Map<string, string>>(new Map())
  const commandTerminalIdsRef = useRef<Set<string>>(new Set())
  const userInputTerminalIdsRef = useRef<Set<string>>(new Set())
  const inputTrackersRef = useRef<Map<string, TerminalInputTracker>>(new Map())
  const suggestionGateRef = useRef(new StaleTerminalSuggestionGate())
  const suggestionDebounceRef = useRef<number | null>(null)
  const suggestionStateRef = useRef<TerminalSuggestionState | null>(null)
  const suggestionSettingsRef = useRef(suggestionSettings)

  suggestionStateRef.current = suggestionState
  suggestionSettingsRef.current = suggestionSettings

  useEffect(() => {
    if (!suggestionSettings.enabled) {
      suggestionGateRef.current.invalidate()
      setSuggestionState(null)
    }
  }, [suggestionSettings])

  useEffect(
    () =>
      subscribeTerminalSuggestionSettings((settings) => {
        setSuggestionSettings(settings)
      }),
    [],
  )

  const handleTerminalLink = useCallback((uri: string) => {
    if (isBrowserSupportedDevServerUrl(uri)) {
      onOpenBrowserUrlRef.current?.(uri)
      return
    }
    openExternalLink(uri)
  }, [])

  const detectBrowserLaunchTargets = useCallback((tab: TerminalTab | undefined, data: string) => {
    if (!tab) return
    if (tab.browserSupported === false) return
    const urls = extractBrowserSupportedDevServerUrls(data)
    for (const url of urls) {
      const target = makeBrowserLaunchTarget({
        label: browserLaunchTargetLabel(url, tab.label),
        url,
        source: tab.label,
      })
      if (target) onBrowserLaunchTargetDetectedRef.current?.(target)
    }
  }, [])

  const trackerForTerminal = useCallback((terminalId: string) => {
    const existing = inputTrackersRef.current.get(terminalId)
    if (existing) return existing
    const tracker = new TerminalInputTracker()
    inputTrackersRef.current.set(terminalId, tracker)
    return tracker
  }, [])

  const clearSuggestion = useCallback(() => {
    suggestionGateRef.current.invalidate()
    if (suggestionDebounceRef.current !== null) {
      window.clearTimeout(suggestionDebounceRef.current)
      suggestionDebounceRef.current = null
    }
    setSuggestionState(null)
  }, [])

  const scheduleSuggestions = useCallback(
    (tab: TerminalTab, snapshot: TerminalSuggestionSnapshot) => {
      if (!suggestionSettingsRef.current.enabled || !defaultAdapter.terminalSuggest) {
        clearSuggestion()
        return
      }
      if (snapshot.suppressed || !tab.running) {
        clearSuggestion()
        return
      }
      if (suggestionDebounceRef.current !== null) {
        window.clearTimeout(suggestionDebounceRef.current)
      }
      const requestId = suggestionGateRef.current.next()
      suggestionDebounceRef.current = window.setTimeout(() => {
        suggestionDebounceRef.current = null
        const settings = suggestionSettingsRef.current
        const modelSelection = settings.modelSelection
        void defaultAdapter.terminalSuggest?.({
          projectId: tab.projectId,
          terminalId: tab.id,
          buffer: snapshot.buffer,
          cursor: snapshot.cursor,
          cwd: tab.cwd,
          shell: tab.shell,
          recentBlockContext: null,
          requestId,
          enableAi: settings.aiEnabled,
          providerId: modelSelection?.providerId ?? null,
          providerProfileId: modelSelection?.providerProfileId ?? null,
          modelId: modelSelection?.modelId ?? null,
          runtimeAgentId: modelSelection?.runtimeAgentId ?? null,
          thinkingEffort: modelSelection?.thinkingEffort ?? null,
        }).then((response) => {
          if (!suggestionGateRef.current.isCurrent(response.requestId)) return
          const candidates = response.candidates.filter((candidate) =>
            shouldShowCandidate(snapshot, candidate),
          )
          setSuggestionState(
            candidates.length > 0
              ? { terminalId: tab.id, snapshot, candidates, selectedIndex: 0 }
              : null,
          )
        }).catch(() => {
          if (suggestionGateRef.current.isCurrent(requestId)) {
            setSuggestionState(null)
          }
        })
      }, TERMINAL_SUGGESTION_DEBOUNCE_MS)
    },
    [clearSuggestion],
  )

  const recordTerminalCommand = useCallback((tab: TerminalTab, command: string) => {
    if (!command || isProbablySecretCommand(command)) return
    void defaultAdapter.terminalRecordCommand?.({
      projectId: tab.projectId,
      command,
      cwd: tab.cwd,
      shell: tab.shell,
    }).catch(() => undefined)
  }, [])

  const ignoreSuggestion = useCallback((tab: TerminalTab, candidate: TerminalSuggestionCandidateDto) => {
    void defaultAdapter.terminalIgnoreSuggestion?.({
      projectId: tab.projectId,
      display: candidate.display,
    }).catch(() => undefined)
  }, [])

  const acceptSuggestion = useCallback(
    (tab: TerminalTab, candidate: TerminalSuggestionCandidateDto, mode: "full" | "word") => {
      const write = acceptedSuggestionWrite(candidate, mode)
      if (!write) return
      const tracker = trackerForTerminal(tab.id)
      const result = tracker.applyInput(write)
      userInputTerminalIdsRef.current.add(tab.id)
      suppressingLiveOutputIdsRef.current.delete(tab.id)
      void defaultAdapter.terminalWrite?.(tab.id, write)
      const snapshot = result.snapshot
      setSuggestionState(null)
      scheduleSuggestions(tab, snapshot)
    },
    [scheduleSuggestions, trackerForTerminal],
  )

  const terminalCursorOverlayStyle = useCallback(
    (tab: TerminalTab, snapshot: TerminalSuggestionSnapshot): CSSProperties => {
      const terminal = tab.terminal as unknown as XTermWithCursorMetrics
      const cellWidth =
        terminal._core?._renderService?.dimensions?.css?.cell?.width ??
        TERMINAL_FONT_SIZE * 0.62
      const cellHeight =
        terminal._core?._renderService?.dimensions?.css?.cell?.height ??
        TERMINAL_FONT_SIZE * 1.35
      const cursorX =
        typeof terminal.buffer?.active?.cursorX === "number"
          ? terminal.buffer.active.cursorX
          : snapshot.cursor
      const cursorY =
        typeof terminal.buffer?.active?.cursorY === "number"
          ? terminal.buffer.active.cursorY
          : 0
      return {
        left: 12 + cursorX * cellWidth,
        top: 12 + cursorY * cellHeight,
        lineHeight: `${cellHeight}px`,
      }
    },
    [],
  )

  const activeProjectTabs = useMemo(
    () => tabs.filter((tab) => tab.projectId === projectId),
    [projectId, tabs],
  )

  const activeTab = useMemo(
    () => activeProjectTabs.find((tab) => tab.id === activeTabId) ?? null,
    [activeProjectTabs, activeTabId],
  )

  const updateTabLabel = useCallback((terminalId: string, label: string) => {
    const nextLabel = sanitizeTerminalTabLabel(label)
    if (!nextLabel) return
    setTabs((current) =>
      current.map((tab) =>
        tab.id === terminalId && !tab.labelLocked && tab.label !== nextLabel
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
      if (suppressingLiveOutputIdsRef.current.has(terminalId)) return
      taskHandlersRef.current.get(terminalId)?.onData?.(data)
      const tab = tabsRef.current.find((entry) => entry.id === terminalId)
      if (tab) {
        const snapshot = trackerForTerminal(terminalId).observeOutput(data)
        if (suggestionStateRef.current?.terminalId === terminalId && snapshot.suppressed) {
          clearSuggestion()
        }
        if (!openedTerminalIdsRef.current.has(terminalId)) {
          const buffered = pendingWriteBuffersRef.current.get(terminalId) ?? ""
          pendingWriteBuffersRef.current.set(terminalId, buffered + data)
          return
        }
        detectBrowserLaunchTargets(tab, data)
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
        suppressingLiveOutputIdsRef.current.delete(terminalId)
        return
      }
      suppressingLiveOutputIdsRef.current.delete(terminalId)
      const tab = tabsRef.current.find((entry) => entry.id === terminalId)
      const code = exitCode ?? null
      taskHandlersRef.current.get(terminalId)?.onExit?.({ terminalId, exitCode: code })
      taskHandlersRef.current.delete(terminalId)
      if (!tab) return
      tab.terminal.write(`\r\n\x1b[2m[exited${code === null ? '' : ` with code ${code}`}]\x1b[0m\r\n`)
      setTabs((current) =>
        current.map((entry) =>
          entry.id === terminalId ? { ...entry, running: false } : entry,
        ),
      )
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
  }, [clearSuggestion, trackerForTerminal, updateTabLabel])

  const registerTerminalHost = useCallback((tab: TerminalTab, node: HTMLDivElement | null) => {
    if (!node) {
      terminalHostsRef.current.delete(tab.id)
      return
    }
    terminalHostsRef.current.set(tab.id, node)
    if (openedTerminalIdsRef.current.has(tab.id)) return
    tab.terminal.open(node)
    openedTerminalIdsRef.current.add(tab.id)
    const buffered = pendingWriteBuffersRef.current.get(tab.id)
    if (buffered) {
      tab.terminal.write(buffered)
      pendingWriteBuffersRef.current.delete(tab.id)
    }
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

  const writeCommandToTerminal = useCallback(
    (terminalId: string, command: string | undefined, options?: TerminalSpawnOptions) => {
      if (!command?.trim()) return false
      commandTerminalIdsRef.current.add(terminalId)
      window.setTimeout(() => {
        const write = buildTerminalCommandWrite(command, options)
        if (!write) return
        suppressingLiveOutputIdsRef.current.delete(terminalId)
        void defaultAdapter.terminalWrite?.(terminalId, write)
      }, 80)
      return true
    },
    [],
  )

  const findReusableBlankTab = useCallback((targetProjectId: string): TerminalTab | null => {
    const projectTabs = tabsRef.current.filter((tab) => tab.projectId === targetProjectId)
    const activeProjectTab =
      activeTabIdRef.current
        ? projectTabs.find((tab) => tab.id === activeTabIdRef.current) ?? null
        : null
    const seen = new Set<string>()
    const candidates = [activeProjectTab, ...projectTabs.slice().reverse()]
      .filter((tab): tab is TerminalTab => Boolean(tab))
      .filter((tab) => {
        if (seen.has(tab.id)) return false
        seen.add(tab.id)
        return true
      })

    return candidates.find((tab) =>
      tab.running &&
      tab.labelLocked !== true &&
      !commandTerminalIdsRef.current.has(tab.id) &&
      !userInputTerminalIdsRef.current.has(tab.id) &&
      !taskHandlersRef.current.has(tab.id),
    ) ?? null
  }, [])

  const claimReusableBlankTabForCommand = useCallback(
    (
      targetProjectId: string,
      command: string | undefined,
      options?: InternalTerminalSpawnOptions,
    ): string | null => {
      if (!command?.trim()) return null
      const tab = findReusableBlankTab(targetProjectId)
      if (!tab) return null

      const nextLabel = sanitizeTerminalTabLabel(options?.label ?? "") ?? tab.label
      const nextLabelLocked = options?.labelLocked ?? Boolean(options?.label)
      const nextBrowserSupported = options?.browserSupported ?? null
      const updateTab = (entry: TerminalTab): TerminalTab =>
        entry.id === tab.id
          ? {
              ...entry,
              label: nextLabel,
              labelLocked: nextLabelLocked,
              browserSupported: nextBrowserSupported,
            }
          : entry

      if (options?.onData || options?.onExit) {
        taskHandlersRef.current.set(tab.id, {
          onData: options.onData,
          onExit: options.onExit,
        })
      }
      tabsRef.current = tabsRef.current.map(updateTab)
      setTabs((current) => current.map(updateTab))
      activeTabIdRef.current = tab.id
      activeTabByProjectRef.current.set(targetProjectId, tab.id)
      setActiveTabId(tab.id)
      writeCommandToTerminal(tab.id, command, options)
      return tab.id
    },
    [findReusableBlankTab, writeCommandToTerminal],
  )

  const disposeTerminalTab = useCallback(
    (
      tab: TerminalTab,
      options: { notifyTask?: boolean } = {},
    ) => {
      closingTerminalIdsRef.current.add(tab.id)
      terminalHostsRef.current.delete(tab.id)
      openedTerminalIdsRef.current.delete(tab.id)
      pendingWriteBuffersRef.current.delete(tab.id)
      inputTrackersRef.current.delete(tab.id)
      suppressingLiveOutputIdsRef.current.delete(tab.id)
      commandTerminalIdsRef.current.delete(tab.id)
      userInputTerminalIdsRef.current.delete(tab.id)
      if (suggestionStateRef.current?.terminalId === tab.id) {
        clearSuggestion()
      }
      try { tab.terminal.dispose() } catch { /* swallow */ }
      if (options.notifyTask !== false) {
        taskHandlersRef.current.get(tab.id)?.onExit?.({ terminalId: tab.id, exitCode: null })
      }
      taskHandlersRef.current.delete(tab.id)
      void defaultAdapter.terminalClose?.(tab.id).catch(() => undefined)
    },
    [clearSuggestion],
  )

  const spawnTab = useCallback(
    async (command?: string, options?: InternalTerminalSpawnOptions): Promise<string | null> => {
      if (!isTauri()) return null
      const targetProjectId = projectIdRef.current
      if (!targetProjectId) return null
      const cols = 120
      const rows = 32
      try {
        if (command?.trim()) {
          const claimedTabId = claimReusableBlankTabForCommand(targetProjectId, command, options)
          if (claimedTabId) return claimedTabId

          const pendingBlankTab = autoOpeningTerminalPromiseRef.current
          if (pendingBlankTab) {
            await pendingBlankTab.catch(() => null)
            const claimedPendingTabId = claimReusableBlankTabForCommand(
              targetProjectId,
              command,
              options,
            )
            if (claimedPendingTabId) return claimedPendingTabId
          }
        }

        const response = await defaultAdapter.terminalOpen?.({
          projectId: targetProjectId,
          clientTerminalId: null,
          cols,
          rows,
          suppressTranscriptUntilInput: false,
        })
        if (!response) return null
        const { terminal, fit } = createXTerm(xtermThemeRef.current, handleTerminalLink)
        terminal.attachCustomKeyEventHandler((event) => {
          const visibleSuggestion = suggestionStateRef.current
          const currentCandidate =
            visibleSuggestion?.terminalId === response.terminalId
              ? visibleSuggestion.candidates[visibleSuggestion.selectedIndex]
              : null
          if (currentCandidate) {
            const currentTab = tabsRef.current.find((entry) => entry.id === response.terminalId)
            const acceptsFull =
              event.key === "Tab" ||
              (event.key === "ArrowRight" && !event.altKey && !event.metaKey && !event.shiftKey) ||
              (event.key.toLowerCase() === "f" && event.ctrlKey && !event.altKey && !event.metaKey)
            const acceptsWord =
              event.altKey && !event.ctrlKey && !event.metaKey && (event.key === "ArrowRight" || event.key.toLowerCase() === "f")
            if (event.key === "Escape") {
              event.preventDefault()
              event.stopPropagation()
              if (currentTab) ignoreSuggestion(currentTab, currentCandidate)
              clearSuggestion()
              return false
            }
            if (currentTab && (acceptsFull || acceptsWord)) {
              event.preventDefault()
              event.stopPropagation()
              acceptSuggestion(currentTab, currentCandidate, acceptsWord ? "word" : "full")
              return false
            }
          }

          const shortcutWrite = terminalShortcutWrite(event)
          if (shortcutWrite) {
            event.preventDefault()
            event.stopPropagation()
            suppressingLiveOutputIdsRef.current.delete(response.terminalId)
            userInputTerminalIdsRef.current.add(response.terminalId)
            const tracker = trackerForTerminal(response.terminalId)
            const tracked = tracker.applyInput(shortcutWrite)
            const currentTab = tabsRef.current.find((entry) => entry.id === response.terminalId)
            clearSuggestion()
            if (currentTab && tracked.kind === "edit") {
              scheduleSuggestions(currentTab, tracked.snapshot)
            }
            void defaultAdapter.terminalWrite?.(response.terminalId, shortcutWrite)
            return false
          }

          if (!isPlainShiftEnter(event)) return true

          event.preventDefault()
          event.stopPropagation()
          suppressingLiveOutputIdsRef.current.delete(response.terminalId)
          userInputTerminalIdsRef.current.add(response.terminalId)
          void defaultAdapter.terminalWrite?.(
            response.terminalId,
            TERMINAL_SHIFT_ENTER_SEQUENCE,
          )
          return false
        })
        terminal.onData((data) => {
          suppressingLiveOutputIdsRef.current.delete(response.terminalId)
          userInputTerminalIdsRef.current.add(response.terminalId)
          const tracked = trackerForTerminal(response.terminalId).applyInput(data)
          const currentTab = tabsRef.current.find((entry) => entry.id === response.terminalId)
          if (tracked.kind === "submit") {
            clearSuggestion()
            if (currentTab && tracked.command) {
              recordTerminalCommand(currentTab, tracked.command)
            }
          } else if (tracked.kind === "reset") {
            clearSuggestion()
          } else if (currentTab) {
            scheduleSuggestions(currentTab, tracked.snapshot)
          }
          void defaultAdapter.terminalWrite?.(response.terminalId, data)
        })
        terminal.onResize(({ cols: c, rows: r }) => {
          void defaultAdapter.terminalResize?.(response.terminalId, c, r)
        })
        terminal.onTitleChange((title) => {
          updateTabLabel(response.terminalId, title)
        })
        const initialLabel =
          sanitizeTerminalTabLabel(options?.label ?? "") ??
          sanitizeTerminalTabLabel(response.shell.split(/[\\/]/).pop() ?? response.shell) ??
          "terminal"
        const tab: TerminalTab = {
          id: response.terminalId,
          projectId: targetProjectId,
          label: initialLabel,
          labelLocked: options?.labelLocked ?? !!options?.label,
          browserSupported: options?.browserSupported ?? null,
          cwd: response.cwd ?? null,
          shell: response.shell,
          running: true,
          terminal,
          fit,
        }
        if (options?.onData || options?.onExit) {
          taskHandlersRef.current.set(response.terminalId, {
            onData: options.onData,
            onExit: options.onExit,
          })
        }
        tabsRef.current = [...tabsRef.current, tab]
        setTabs((current) => current.some((entry) => entry.id === tab.id) ? current : [...current, tab])
        activeTabIdRef.current = response.terminalId
        setActiveTabId(response.terminalId)
        writeCommandToTerminal(response.terminalId, command, options)
        return response.terminalId
      } catch (error) {
        console.error("Could not open terminal", error)
        return null
      }
    },
    [
      acceptSuggestion,
      claimReusableBlankTabForCommand,
      clearSuggestion,
      handleTerminalLink,
      ignoreSuggestion,
      recordTerminalCommand,
      scheduleSuggestions,
      trackerForTerminal,
      updateTabLabel,
      writeCommandToTerminal,
    ],
  )

  const ensureTerminalTab = useCallback(() => {
    if (!isTauri()) return
    if (autoOpeningTerminalRef.current) return
    autoOpeningTerminalRef.current = true
    const openPromise = spawnTab()
    autoOpeningTerminalPromiseRef.current = openPromise
    void openPromise.finally(() => {
      autoOpeningTerminalRef.current = false
      if (autoOpeningTerminalPromiseRef.current === openPromise) {
        autoOpeningTerminalPromiseRef.current = null
      }
    })
  }, [spawnTab])

  useEffect(() => {
    if (!projectId || !activeTabId) return
    const activeTab = tabs.find((tab) => tab.id === activeTabId)
    if (activeTab?.projectId === projectId) {
      activeTabByProjectRef.current.set(projectId, activeTabId)
    }
  }, [activeTabId, projectId, tabs])

  useEffect(() => {
    const previousProjectId = previousProjectIdRef.current
    if (previousProjectId && previousProjectId !== projectId) {
      const previousActiveTabId = activeTabIdRef.current
      if (
        previousActiveTabId &&
        tabsRef.current.some(
          (tab) => tab.id === previousActiveTabId && tab.projectId === previousProjectId,
        )
      ) {
        activeTabByProjectRef.current.set(previousProjectId, previousActiveTabId)
      }
      clearSuggestion()
    }

    previousProjectIdRef.current = projectId

    if (!projectId) {
      setActiveTabId(null)
      return
    }

    const projectTabs = tabsRef.current.filter((tab) => tab.projectId === projectId)
    const rememberedTabId = activeTabByProjectRef.current.get(projectId) ?? null
    const nextActiveTabId =
      projectTabs.find((tab) => tab.id === rememberedTabId)?.id ??
      projectTabs[projectTabs.length - 1]?.id ??
      null
    setActiveTabId(nextActiveTabId)
  }, [clearSuggestion, projectId])

  // Auto-create the first tab when the sidebar opens or recovers from an
  // unexpected empty state.
  useEffect(() => {
    if (!open) return
    if (activeProjectTabs.length > 0) return
    ensureTerminalTab()
  }, [activeProjectTabs.length, ensureTerminalTab, open, projectId])

  useEffect(() => {
    if (!registerHandle) return
    registerHandle({ spawnTabWithCommand: (command, options) => spawnTab(command, options) })
    return () => {
      registerHandle(null)
    }
  }, [registerHandle, spawnTab])

  const handleCloseTab = useCallback(
    (id: string) => {
      const snapshot = tabsRef.current
      const tab = snapshot.find((entry) => entry.id === id)
      if (!tab) return
      const remaining = snapshot.filter(
        (entry) => entry.projectId === tab.projectId && entry.id !== id,
      )
      const closeTab = (fallbackActiveTabId: string | null) => {
        disposeTerminalTab(tab, { notifyTask: true })
        if (fallbackActiveTabId) {
          activeTabByProjectRef.current.set(tab.projectId, fallbackActiveTabId)
        } else {
          activeTabByProjectRef.current.delete(tab.projectId)
        }
        setTabs((current) => current.filter((entry) => entry.id !== id))
        setActiveTabId((current) => {
          if (current !== id) return current
          return fallbackActiveTabId
        })
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
    [disposeTerminalTab, spawnTab],
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

  const visibleSuggestion =
    activeTab && suggestionState?.terminalId === activeTab.id
      ? suggestionState
      : null
  const visibleCandidate = visibleSuggestion?.candidates[visibleSuggestion.selectedIndex] ?? null
  const visibleSuggestionStyle =
    visibleSuggestion && activeTab
      ? terminalCursorOverlayStyle(activeTab, visibleSuggestion.snapshot)
      : undefined

  const updateSuggestionSetting = useCallback(
    (key: "enabled" | "aiEnabled", value: boolean) => {
      setSuggestionSettings((current) => {
        const next = { ...current, [key]: value }
        persistTerminalSuggestionSettings(next)
        return next
      })
    },
    [],
  )

  // Cleanup on unmount: dispose xterm instances + kill PTYs.
  useEffect(() => {
    return () => {
      const snapshot = tabsRef.current
      snapshot.forEach((tab) =>
        disposeTerminalTab(tab, { notifyTask: true }),
      )
    }
  }, [disposeTerminalTab])

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
            {activeProjectTabs.map((tab) => (
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
                onClick={() => setActiveTabId(tab.id)}
              >
                <button
                  className="flex min-w-0 flex-1 items-center truncate text-left font-mono"
                  title={tab.label}
                  type="button"
                >
                  <span className="truncate">{tab.label}</span>
                </button>
                <button
                  aria-label="Close terminal"
                  className="flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity hover:bg-secondary/60 hover:text-foreground group-hover:opacity-100"
                  onClick={(event) => {
                    event.stopPropagation()
                    void handleCloseTab(tab.id)
                  }}
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
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  aria-label="Terminal suggestion settings"
                  className="h-6 w-6 shrink-0 text-muted-foreground"
                  size="icon"
                  title="Terminal suggestion settings"
                  type="button"
                  variant="ghost"
                >
                  <Settings2 className="h-3.5 w-3.5" />
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-80 p-3">
                <div className="space-y-3">
                  <div className="border-b border-border/70 pb-2">
                    <div className="text-[12px] font-medium text-foreground">
                      Inline terminal suggestions
                    </div>
                  </div>

                  <label className="flex items-start justify-between gap-4 rounded-md px-1 py-1.5">
                    <span className="min-w-0">
                      <span className="flex items-center gap-2 text-[12px] font-medium text-foreground">
                        Command suggestions
                        <span className="rounded-sm bg-secondary px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-muted-foreground">
                          Local
                        </span>
                      </span>
                      <span className="mt-1 block text-[11px] leading-4 text-muted-foreground">
                        Uses recent terminal commands, shell history, project files, and package scripts.
                      </span>
                    </span>
                    <Switch
                      checked={suggestionSettings.enabled}
                      className="mt-0.5"
                      onCheckedChange={(checked) => updateSuggestionSetting("enabled", checked)}
                    />
                  </label>

                  <label
                    className={cn(
                      "flex items-start justify-between gap-4 rounded-md px-1 py-1.5",
                      !suggestionSettings.enabled && "opacity-60",
                    )}
                  >
                    <span className="min-w-0">
                      <span className="flex items-center gap-2 text-[12px] font-medium text-foreground">
                        AI suggestions
                        <span className="rounded-sm bg-secondary px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-muted-foreground">
                          Fallback
                        </span>
                      </span>
                      <span className="mt-1 block text-[11px] leading-4 text-muted-foreground">
                        Only asks the configured model when local sources have no useful match.
                      </span>
                    </span>
                    <Switch
                      checked={suggestionSettings.aiEnabled}
                      className="mt-0.5"
                      disabled={!suggestionSettings.enabled}
                      onCheckedChange={(checked) => updateSuggestionSetting("aiEnabled", checked)}
                    />
                  </label>
                </div>
              </PopoverContent>
            </Popover>
          </div>
        </div>

        <div
          ref={terminalViewportRef}
          className="xero-terminal-viewport relative min-h-0 flex-1 overflow-hidden px-3 pb-3 pt-3"
          onClick={() => {
            activeTab?.terminal.focus()
          }}
          style={{ backgroundColor: xtermTheme.background }}
        >
          <style>{`
            .xero-terminal-viewport .xterm .xterm-scrollable-element > .scrollbar {
              width: 8px !important;
              z-index: var(--scrollbar-z-index) !important;
              background: transparent !important;
            }
            .xero-terminal-viewport .xterm .xterm-scrollable-element > .scrollbar > .slider {
              left: 1px !important;
              width: 6px !important;
              z-index: var(--scrollbar-z-index) !important;
              border-radius: 999px !important;
              background: var(--scrollbar-thumb) !important;
            }
            .xero-terminal-viewport .xterm .xterm-scrollable-element > .scrollbar > .slider:hover {
              background: var(--scrollbar-thumb-hover) !important;
            }
          `}</style>
          {tabs.map((tab) => (
            <div
              key={tab.id}
              ref={(node) => registerTerminalHost(tab, node)}
              className={cn(
                "h-full w-full",
                tab.projectId === projectId && tab.id === activeTabId ? "block" : "hidden",
              )}
            />
          ))}
          {visibleCandidate ? (
            <div
              className="pointer-events-none absolute z-20 max-w-[calc(100%-24px)]"
              data-testid="terminal-inline-suggestion"
              style={visibleSuggestionStyle}
            >
              <div
                aria-hidden="true"
                className="truncate font-mono text-[13px] text-muted-foreground/55"
              >
                {visibleCandidate.replacement}
              </div>
            </div>
          ) : null}
        </div>
        {activeProjectTabs.length === 0 ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 top-9 flex items-center justify-center text-[12px] text-muted-foreground">
            {isTauri() ? "Opening terminal…" : "Terminals are only available in the desktop app."}
          </div>
        ) : null}
      </div>
    </aside>
  )
}
