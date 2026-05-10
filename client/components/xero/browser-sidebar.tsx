"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import {
  ArrowLeft,
  ArrowRight,
  Cookie,
  Loader2,
  MousePointerSquareDashed,
  Pencil,
  Plus,
  RotateCw,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { createFrameCoalescer } from "@/lib/frame-governance"
import { useSidebarWidthMotion } from "@/lib/sidebar-motion"
import { recordIpcPayloadSample } from "@/src/lib/ipc-payload-budget"
import { createSafeTauriUnlisten } from "@/src/lib/tauri-events"
import {
  useCookieImport,
  type CookieImportStatus,
  type DetectedBrowser,
} from "./browser-cookie-import"
import {
  InspectOverlay,
  PenOverlay,
  isDevServerUrl,
} from "./browser-tool-overlay"
import {
  createBrowserResizeScheduler,
  readBrowserViewportRect,
} from "./browser-resize-scheduler"

type ToolMode = "pen" | "inspect" | null

const MIN_WIDTH = 320
const RIGHT_PADDING = 200
const DEFAULT_RATIO = 0.4
const COOKIE_IMPORT_PROMPTED_KEY = "xero.browser.cookieImportPrompted"
// Pixel inset on the left of the content area reserved for the resize handle.
// The native child webview paints on top of all HTML, so without this inset the
// right half of the resize handle (which straddles the sidebar edge) would be
// captured by the webview and the sidebar would become impossible to resize
// once a URL had been loaded.
const RESIZE_HANDLE_INSET = 6

interface BrowserSidebarProps {
  open: boolean
}

interface BrowserTabMeta {
  id: string
  label: string
  title: string | null
  url: string | null
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
  active: boolean
}

interface BrowserUrlChangedPayload {
  tabId: string
  url: string
  title: string | null
  canGoBack: boolean
  canGoForward: boolean
}

interface BrowserLoadStatePayload {
  tabId: string
  loading: boolean
  url: string | null
  error: string | null
}

interface BrowserTabUpdatedPayload {
  tabs: BrowserTabMeta[]
}

type BrowserCoalescedEvent =
  | { key: string; payload: BrowserLoadStatePayload; type: "load" }
  | { key: string; payload: BrowserTabUpdatedPayload; type: "tabs" }
  | { key: string; payload: BrowserUrlChangedPayload; type: "url" }

interface BrowserEventCoalescerHandlers {
  onLoadState: (payload: BrowserLoadStatePayload) => void
  onTabUpdated: (payload: BrowserTabUpdatedPayload) => void
  onUrlChanged: (payload: BrowserUrlChangedPayload) => void
  schedule?: (callback: () => void) => () => void
}

function scheduleBrowserEventFlush(callback: () => void): () => void {
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    const frame = window.requestAnimationFrame(callback)
    return () => window.cancelAnimationFrame(frame)
  }

  const timeout = setTimeout(callback, 0)
  return () => clearTimeout(timeout)
}

export function createBrowserEventCoalescer({
  onLoadState,
  onTabUpdated,
  onUrlChanged,
  schedule = scheduleBrowserEventFlush,
}: BrowserEventCoalescerHandlers) {
  let pending: BrowserCoalescedEvent[] = []
  let cancelScheduled: (() => void) | null = null
  let disposed = false

  const flush = () => {
    if (disposed) return
    cancelScheduled = null
    const events = pending
    pending = []
    for (const event of events) {
      if (event.type === "url") {
        onUrlChanged(event.payload)
      } else if (event.type === "load") {
        onLoadState(event.payload)
      } else {
        onTabUpdated(event.payload)
      }
    }
  }

  const enqueue = (event: BrowserCoalescedEvent) => {
    if (disposed) return
    pending = pending.filter((candidate) => candidate.key !== event.key)
    pending.push(event)
    if (!cancelScheduled) {
      cancelScheduled = schedule(flush)
    }
  }

  return {
    enqueueLoadState(payload: BrowserLoadStatePayload) {
      enqueue({ key: `load:${payload.tabId}`, payload, type: "load" })
    },
    enqueueTabUpdated(payload: BrowserTabUpdatedPayload) {
      enqueue({ key: "tabs", payload, type: "tabs" })
    },
    enqueueUrlChanged(payload: BrowserUrlChangedPayload) {
      enqueue({ key: `url:${payload.tabId}`, payload, type: "url" })
    },
    dispose() {
      disposed = true
      pending = []
      cancelScheduled?.()
      cancelScheduled = null
    },
    flush,
  }
}

function viewportDefaultWidth() {
  if (typeof window === "undefined") return 640
  return Math.round(window.innerWidth * DEFAULT_RATIO)
}

function viewportMaxWidth() {
  if (typeof window === "undefined") return 1600
  return Math.max(MIN_WIDTH, window.innerWidth - RIGHT_PADDING)
}

function normalizeUrl(input: string): string | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  if (/^https?:\/\//i.test(trimmed)) return trimmed
  if (/^[\w.-]+\.[a-z]{2,}(\/.*)?$/i.test(trimmed)) return `https://${trimmed}`
  const query = encodeURIComponent(trimmed)
  return `https://www.google.com/search?q=${query}`
}

function safeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return Promise.resolve(null)
  return invoke<T>(command, args).catch(() => null)
}

function readCookiePromptFlag(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      typeof window.localStorage?.getItem === "function" &&
      window.localStorage.getItem(COOKIE_IMPORT_PROMPTED_KEY) === "true"
    )
  } catch {
    return false
  }
}

function writeCookiePromptFlag(): void {
  try {
    if (
      typeof window !== "undefined" &&
      typeof window.localStorage?.setItem === "function"
    ) {
      window.localStorage.setItem(COOKIE_IMPORT_PROMPTED_KEY, "true")
    }
  } catch {
    /* storage quota / privacy mode — the banner will re-appear next session */
  }
}

export function BrowserSidebar({ open }: BrowserSidebarProps) {
  const [width, setWidth] = useState(viewportDefaultWidth)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [address, setAddress] = useState("")
  const [tabs, setTabs] = useState<BrowserTabMeta[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [navError, setNavError] = useState<string | null>(null)
  const [showCookieBanner, setShowCookieBanner] = useState(false)
  const [toolMode, setToolMode] = useState<ToolMode>(null)
  const targetWidth = open ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { animate: false, isResizing })
  const {
    browsers: cookieBrowsers,
    status: importStatus,
    refresh: refreshCookieSources,
    importFrom: importCookiesFromBrowser,
  } = useCookieImport()
  const widthRef = useRef(width)
  widthRef.current = width
  const viewportRef = useRef<HTMLDivElement | null>(null)
  const addressFocusedRef = useRef(false)
  const hasWebviewRef = useRef(false)
  const cookieSourcesLoadedRef = useRef(false)
  const openRef = useRef(open)
  const activeTabIdRef = useRef(activeTabId)
  const toolModeRef = useRef(toolMode)

  openRef.current = open
  activeTabIdRef.current = activeTabId
  toolModeRef.current = toolMode

  const resizeScheduler = useMemo(
    () =>
      createBrowserResizeScheduler({
        getEnabled: () =>
          openRef.current &&
          hasWebviewRef.current &&
          toolModeRef.current === null &&
          isTauri(),
        getNode: () => viewportRef.current,
        getTabId: () => activeTabIdRef.current,
        inset: RESIZE_HANDLE_INSET,
        onResize: (rect, tabId) => {
          void invoke("browser_resize", { ...rect, tabId }).catch(() => {
            /* swallow */
          })
        },
      }),
    [],
  )

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) ?? null,
    [tabs, activeTabId],
  )

  const isDevTab = isDevServerUrl(activeTab?.url ?? null)
  const pageLabel = activeTab?.title ?? activeTab?.url ?? null

  // Tools are gated to dev-server tabs. Drop the active tool whenever the
  // tab/URL changes to one that isn't a dev server, or when the sidebar closes.
  useEffect(() => {
    if (toolMode === null) return
    if (!open || !isDevTab) {
      setToolMode(null)
    }
  }, [open, isDevTab, toolMode])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current) return
    // Tool overlays (pen / inspect) sit on top of the viewport in HTML, but the
    // native child webview always paints on top of HTML — so we hide the
    // webview while a tool is active and avoid scheduling it back on top here.
    if (toolMode !== null) return

    // Reset the cache on every effect re-run (sidebar open, active tab change)
    // so the first scheduled sync fires a browser_resize. Without this,
    // switching tabs leaves the new active tab parked at HIDDEN_OFFSET because
    // the viewport rect may be unchanged.
    resizeScheduler.reset()
    resizeScheduler.schedule({ force: true })
  }, [activeTabId, open, resizeScheduler, toolMode])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current) return
    if (toolMode !== null) return

    const node = viewportRef.current
    if (!node) return

    const ResizeObserverCtor = window.ResizeObserver
    if (typeof ResizeObserverCtor !== "function") {
      resizeScheduler.schedule()
      return
    }

    const observer = new ResizeObserverCtor(() => {
      resizeScheduler.schedule()
    })
    observer.observe(node)

    return () => observer.disconnect()
  }, [activeTabId, open, resizeScheduler, toolMode])

  useEffect(() => {
    if (open || !isTauri() || !hasWebviewRef.current) return
    resizeScheduler.cancel()
    resizeScheduler.reset()
    void invoke("browser_hide").catch(() => {
      /* swallow */
    })
  }, [open, resizeScheduler])

  useEffect(() => {
    if (!isTauri() || !hasWebviewRef.current) return
    if (toolMode === null) return
    resizeScheduler.cancel()
    resizeScheduler.reset()
    void invoke("browser_hide").catch(() => {
      /* swallow */
    })
  }, [resizeScheduler, toolMode])

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleResize = () => {
      const nextMax = viewportMaxWidth()
      setMaxWidth(nextMax)
      setWidth((current) => Math.min(current, nextMax))
      resizeScheduler.schedule({ force: true })
    }
    window.addEventListener("resize", handleResize)
    return () => window.removeEventListener("resize", handleResize)
  }, [resizeScheduler])

  useEffect(() => () => resizeScheduler.cancel(), [resizeScheduler])

  // Wire backend events
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    const unsubs: UnlistenFn[] = []
    const coalescer = createBrowserEventCoalescer({
      onUrlChanged: (payload) => {
        setTabs((current) => {
          const match = current.some((tab) => tab.id === payload.tabId)
          if (!match) return current
          return current.map((tab) =>
            tab.id === payload.tabId
              ? {
                  ...tab,
                  url: payload.url,
                  title: payload.title ?? tab.title,
                  canGoBack: payload.canGoBack,
                  canGoForward: payload.canGoForward,
                }
              : tab,
          )
        })
        if (payload.tabId === activeTabIdRef.current && !addressFocusedRef.current) {
          setAddress(payload.url)
        }
      },
      onLoadState: (payload) => {
        setTabs((current) =>
          current.map((tab) =>
            tab.id === payload.tabId
              ? {
                  ...tab,
                  loading: payload.loading,
                  url: payload.url ?? tab.url,
                }
              : tab,
          ),
        )
        if (payload.tabId === activeTabIdRef.current) {
          setLoading(payload.loading)
          if (payload.url && !addressFocusedRef.current) {
            setAddress(payload.url)
          }
        }
      },
      onTabUpdated: (payload) => {
        setTabs(payload.tabs)
        const active = payload.tabs.find((tab) => tab.active)
        if (active) {
          setActiveTabId(active.id)
        }
      },
    })

    const trackUnlisten = (promise: Promise<UnlistenFn>) => {
      void promise.then((unsub) => {
        const safeUnsub = createSafeTauriUnlisten(unsub)
        if (cancelled) {
          safeUnsub()
        } else {
          unsubs.push(safeUnsub)
        }
      })
    }

    trackUnlisten(
      listen<BrowserUrlChangedPayload>("browser:url_changed", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:url_changed", payload: event.payload })
        coalescer.enqueueUrlChanged(event.payload)
      }),
    )

    trackUnlisten(
      listen<BrowserLoadStatePayload>("browser:load_state", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:load_state", payload: event.payload })
        coalescer.enqueueLoadState(event.payload)
      }),
    )

    trackUnlisten(
      listen<BrowserTabUpdatedPayload>("browser:tab_updated", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:tab_updated", payload: event.payload })
        coalescer.enqueueTabUpdated(event.payload)
      }),
    )

    return () => {
      cancelled = true
      coalescer.dispose()
      unsubs.forEach((unsub) => unsub())
    }
  }, [])

  // Hydrate tabs when sidebar opens
  useEffect(() => {
    if (!open || !isTauri()) return
    let cancelled = false
    void safeInvoke<BrowserTabMeta[]>("browser_tab_list").then((list) => {
      if (cancelled || !list) return
      setTabs(list)
      const active = list.find((tab) => tab.active) ?? list[0] ?? null
      if (active) {
        setActiveTabId(active.id)
        if (active.url && !addressFocusedRef.current) setAddress(active.url)
      }
    })
    return () => {
      cancelled = true
    }
  }, [open])

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      const startX = event.clientX
      const startWidth = widthRef.current
      const ceiling = viewportMaxWidth()
      let latestWidth = startWidth
      const widthUpdates = createFrameCoalescer<number>({
        onFlush: (next) => {
          setWidth(next)
          resizeScheduler.schedule()
        },
      })
      setMaxWidth(ceiling)
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        latestWidth = Math.max(MIN_WIDTH, Math.min(ceiling, startWidth + delta))
        widthUpdates.schedule(latestWidth)
      }
      const handleUp = () => {
        widthUpdates.flush()
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
        setIsResizing(false)
        resizeScheduler.schedule({ force: true })
      }

      window.addEventListener("pointermove", handleMove)
      window.addEventListener("pointerup", handleUp)
      window.addEventListener("pointercancel", handleUp)
    },
    [resizeScheduler],
  )

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      return Math.max(MIN_WIDTH, Math.min(ceiling, current + delta))
    })
    resizeScheduler.schedule({ force: true })
  }, [resizeScheduler])

  const openUrl = useCallback(
    (target: string, options?: { tabId?: string; newTab?: boolean }) => {
      setNavError(null)

      if (!isTauri()) {
        setAddress(target)
        return
      }

      const node = viewportRef.current
      if (!node) return
      const viewport = readBrowserViewportRect(node, RESIZE_HANDLE_INSET)
      const forceNew = options?.newTab === true
      const payload = {
        url: target,
        ...viewport,
        tabId: forceNew ? null : options?.tabId ?? activeTabId ?? null,
        newTab: forceNew,
      }
      resizeScheduler.markSynced(viewport)
      hasWebviewRef.current = true
      setLoading(true)
      void invoke<BrowserTabMeta>("browser_show", payload)
        .then((meta) => {
          if (meta) {
            setActiveTabId(meta.id)
          }
        })
        .catch((error: unknown) => {
          hasWebviewRef.current = false
          setLoading(false)
          const message =
            typeof error === "object" && error && "message" in error
              ? String((error as { message?: unknown }).message ?? "")
              : String(error)
          setNavError(message || "Failed to open page")
        })
    },
    [activeTabId, resizeScheduler],
  )

  const handleSubmit = useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      const next = normalizeUrl(address)
      if (!next) return
      setAddress(next)
      openUrl(next)
    },
    [address, openUrl],
  )

  const handleBack = useCallback(() => {
    if (!isTauri()) return
    void invoke("browser_back").catch(() => {
      /* swallow */
    })
  }, [])

  const handleForward = useCallback(() => {
    if (!isTauri()) return
    void invoke("browser_forward").catch(() => {
      /* swallow */
    })
  }, [])

  const handleReload = useCallback(() => {
    if (!isTauri()) return
    void invoke("browser_reload", { tabId: activeTabId ?? null }).catch(() => {
      /* swallow */
    })
  }, [activeTabId])

  const handleTabFocus = useCallback(
    (tabId: string) => {
      if (!isTauri() || tabId === activeTabId) return
      void invoke<BrowserTabMeta>("browser_tab_focus", { tabId })
        .then((meta) => {
          if (meta) {
            setActiveTabId(meta.id)
            if (meta.url && !addressFocusedRef.current) setAddress(meta.url)
          }
        })
        .catch(() => {
          /* swallow */
        })
    },
    [activeTabId],
  )

  const handleTabClose = useCallback(
    (tabId: string) => {
      if (!isTauri()) return
      void invoke<BrowserTabMeta[]>("browser_tab_close", { tabId })
        .then((list) => {
          if (!list) return
          setTabs(list)
          if (list.length === 0) {
            setActiveTabId(null)
            setAddress("")
            hasWebviewRef.current = false
          } else {
            const next = list.find((tab) => tab.active) ?? list[0]
            setActiveTabId(next.id)
            if (next.url) setAddress(next.url)
          }
        })
        .catch(() => {
          /* swallow */
        })
    },
    [],
  )

  const handleNewTab = useCallback(() => {
    if (!isTauri()) return
    openUrl("https://www.google.com/", { newTab: true })
  }, [openUrl])

  // First-run prompt: once a webview exists and the user hasn't been prompted
  // yet, probe installed browsers and pop the banner. Keyed off tabs (not
  // sidebar open) because the import command needs a live webview to anchor
  // the shared cookie store.
  useEffect(() => {
    if (!open || !isTauri()) return
    if (tabs.length === 0) return
    if (cookieSourcesLoadedRef.current) return
    cookieSourcesLoadedRef.current = true

    const prompted = readCookiePromptFlag()

    void refreshCookieSources().then((list) => {
      if (prompted) return
      if (!list.some((browser) => browser.available)) return
      setShowCookieBanner(true)
    })
  }, [open, tabs.length, refreshCookieSources])

  const handleImportCookies = useCallback(
    async (browser: DetectedBrowser) => {
      await importCookiesFromBrowser(browser)
      writeCookiePromptFlag()
    },
    [importCookiesFromBrowser],
  )

  const handleDismissCookieBanner = useCallback(() => {
    setShowCookieBanner(false)
    writeCookiePromptFlag()
  }, [])

  // Show the tab strip (and the + button) as soon as there's any tab — otherwise
  // users have no way to open a second tab because the new-tab trigger lives there.
  const showTabs = tabs.length > 0

  return (
    <aside
      aria-hidden={!open}
      className={cn(
        widthMotion.islandClassName,
        "relative flex shrink-0 flex-col overflow-hidden bg-sidebar",
        open ? "border-l border-border/80" : "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize browser sidebar"
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

      <div
        className="flex h-full min-w-0 shrink-0 flex-col"
        style={{ width }}
      >
      {showTabs ? (
        <div className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-b border-border/60">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={cn(
                "group flex h-8 max-w-[160px] shrink-0 items-center gap-1 border px-2 text-[11px]",
                tab.id === activeTabId
                  ? "border-primary/40 bg-background/80 text-foreground"
                  : "border-border/50 bg-sidebar/60 text-muted-foreground hover:text-foreground",
              )}
            >
              <button
                className="min-w-0 flex-1 truncate text-left"
                onClick={() => handleTabFocus(tab.id)}
                title={tab.title ?? tab.url ?? "New tab"}
                type="button"
              >
                {tab.loading ? (
                  <Loader2 className="mr-1 inline h-3 w-3 animate-spin" />
                ) : null}
                <span className="truncate">
                  {tab.title ?? tab.url ?? "New tab"}
                </span>
              </button>
              <button
                aria-label="Close tab"
                className="flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity hover:bg-secondary/60 hover:text-foreground group-hover:opacity-100"
                onClick={() => handleTabClose(tab.id)}
                type="button"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          ))}
          <button
            aria-label="New tab"
            className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            onClick={handleNewTab}
            type="button"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
      ) : null}

      <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border/70 px-2">
        <button
          aria-label="Back"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
          disabled={!activeTab}
          onClick={handleBack}
          type="button"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label="Forward"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
          disabled={!activeTab}
          onClick={handleForward}
          type="button"
        >
          <ArrowRight className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label={loading ? "Stop" : "Reload"}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
          disabled={!activeTab}
          onClick={handleReload}
          type="button"
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RotateCw className="h-3.5 w-3.5" />
          )}
        </button>
        <form className="ml-1 flex min-w-0 flex-1" onSubmit={handleSubmit}>
          <input
            aria-label="Address"
            className="h-7 w-full min-w-0 rounded-md border border-border/70 bg-background/40 px-2 text-[11.5px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
            onBlur={() => {
              addressFocusedRef.current = false
            }}
            onChange={(event) => setAddress(event.target.value)}
            onFocus={(event) => {
              addressFocusedRef.current = true
              event.currentTarget.select()
            }}
            placeholder="Search or enter URL"
            type="text"
            value={address}
          />
        </form>
        {isDevTab ? (
          <div
            className="ml-1 flex shrink-0 items-center gap-0.5 rounded-md border border-border/60 bg-background/40 px-0.5"
            data-testid="browser-dev-tools"
          >
            <button
              aria-label="Sketch on page"
              aria-pressed={toolMode === "pen"}
              className={cn(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground",
                toolMode === "pen"
                  ? "bg-primary/15 text-primary hover:bg-primary/20 hover:text-primary"
                  : null,
              )}
              onClick={() =>
                setToolMode((current) => (current === "pen" ? null : "pen"))
              }
              title="Sketch on page"
              type="button"
            >
              <Pencil className="h-3.5 w-3.5" />
            </button>
            <button
              aria-label="Inspect element"
              aria-pressed={toolMode === "inspect"}
              className={cn(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground",
                toolMode === "inspect"
                  ? "bg-success/15 text-success hover:bg-success/20 hover:text-success"
                  : null,
              )}
              onClick={() =>
                setToolMode((current) => (current === "inspect" ? null : "inspect"))
              }
              title="Inspect element"
              type="button"
            >
              <MousePointerSquareDashed className="h-3.5 w-3.5" />
            </button>
          </div>
        ) : null}
      </div>

      {showCookieBanner ? (
        <CookieImportBanner
          browsers={cookieBrowsers}
          onDismiss={handleDismissCookieBanner}
          onImport={handleImportCookies}
          onRefresh={() => void refreshCookieSources()}
          status={importStatus}
        />
      ) : null}

      <div
        ref={viewportRef}
        className="relative flex min-h-0 flex-1 items-center justify-center bg-background/40"
      >
        {navError ? (
          <div className="px-6 text-center text-[11.5px] leading-relaxed text-destructive">
            {navError}
          </div>
        ) : !activeTab ? (
          <div className="px-6 text-center text-[11.5px] leading-relaxed text-muted-foreground/80">
            Enter a URL to start browsing.
          </div>
        ) : !isTauri() ? (
          <div className="px-6 text-center text-[11.5px] leading-relaxed text-muted-foreground">
            <div className="font-mono text-foreground/85">{activeTab.url ?? ""}</div>
            <div className="mt-2 text-muted-foreground/80">
              Browser engine is only available in the desktop app.
            </div>
          </div>
        ) : null}
        {toolMode === "pen" ? (
          <PenOverlay
            pageLabel={pageLabel}
            onSubmit={() => setToolMode(null)}
            onExit={() => setToolMode(null)}
          />
        ) : null}
        {toolMode === "inspect" ? (
          <InspectOverlay
            pageLabel={pageLabel}
            onSubmit={() => setToolMode(null)}
            onExit={() => setToolMode(null)}
          />
        ) : null}
      </div>
      </div>
    </aside>
  )
}

interface CookieImportBannerProps {
  browsers: DetectedBrowser[]
  onDismiss: () => void
  onImport: (browser: DetectedBrowser) => void
  onRefresh: () => void
  status: CookieImportStatus
}

function CookieImportBanner({
  browsers,
  onDismiss,
  onImport,
  onRefresh,
  status,
}: CookieImportBannerProps) {
  const available = browsers.filter((b) => b.available)
  const isRunning = status.kind === "running"

  return (
    <div className="shrink-0 border-b border-border/70 bg-sidebar/80 px-3 py-2 text-[11.5px]">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="font-medium text-foreground">Import cookies</div>
          <div className="mt-0.5 text-muted-foreground/80">
            Stay signed in by copying cookies from another browser. Cookies apply
            on the next reload.
          </div>
        </div>
        <button
          aria-label="Dismiss"
          className="flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          onClick={onDismiss}
          type="button"
        >
          <X className="h-3 w-3" />
        </button>
      </div>

      {available.length === 0 ? (
        <div className="mt-2 text-muted-foreground/80">
          No supported browsers detected on this machine.{" "}
          <button
            className="underline underline-offset-2 hover:text-foreground"
            onClick={onRefresh}
            type="button"
          >
            Re-scan
          </button>
          .
        </div>
      ) : (
        <div className="mt-2 flex flex-wrap gap-1">
          {available.map((browser) => {
            const running = isRunning && status.source === browser.id
            return (
              <button
                key={browser.id}
                className="flex items-center gap-1 rounded-md border border-border/60 bg-background/60 px-2 py-1 text-foreground transition-colors hover:border-primary/40 hover:bg-background disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isRunning}
                onClick={() => onImport(browser)}
                type="button"
              >
                {running ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <Cookie className="h-3 w-3" />
                )}
                <span>{browser.label}</span>
              </button>
            )
          })}
        </div>
      )}

      {status.kind === "success" ? (
        <div className="mt-2 text-foreground/85">
          Imported {status.result.imported} cookies across{" "}
          {status.result.domains} domains
          {status.result.skipped > 0
            ? ` (${status.result.skipped} skipped)`
            : ""}
          .
        </div>
      ) : null}
      {status.kind === "error" ? (
        <div className="mt-2 text-destructive">{status.message}</div>
      ) : null}
    </div>
  )
}
