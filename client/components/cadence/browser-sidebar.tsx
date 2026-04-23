"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { ArrowLeft, ArrowRight, Loader2, Plus, RotateCw, X } from "lucide-react"
import { cn } from "@/lib/utils"

const MIN_WIDTH = 320
const RIGHT_PADDING = 200
const DEFAULT_RATIO = 0.4
// Pixel inset on the left of the content area reserved for the resize handle.
// The native child webview paints on top of all HTML, so without this inset the
// right half of the resize handle (which straddles the sidebar edge) would be
// captured by the webview and the sidebar would become impossible to resize
// once a URL had been loaded.
const RESIZE_HANDLE_INSET = 6

interface BrowserSidebarProps {
  open: boolean
}

interface ViewportRect {
  x: number
  y: number
  width: number
  height: number
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

function rectsEqual(a: ViewportRect | null, b: ViewportRect): boolean {
  if (!a) return false
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height
}

function safeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return Promise.resolve(null)
  return invoke<T>(command, args).catch(() => null)
}

export function BrowserSidebar({ open }: BrowserSidebarProps) {
  const [width, setWidth] = useState(viewportDefaultWidth)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [address, setAddress] = useState("")
  const [tabs, setTabs] = useState<BrowserTabMeta[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [canGoBack, setCanGoBack] = useState(false)
  const [canGoForward, setCanGoForward] = useState(false)
  const [navError, setNavError] = useState<string | null>(null)
  const widthRef = useRef(width)
  widthRef.current = width
  const viewportRef = useRef<HTMLDivElement | null>(null)
  const lastSyncedRectRef = useRef<ViewportRect | null>(null)
  const addressFocusedRef = useRef(false)
  const hasWebviewRef = useRef(false)

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) ?? null,
    [tabs, activeTabId],
  )

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current) return

    // Reset the cache on every effect re-run (sidebar open, active tab change)
    // so the first tick always fires a browser_resize. Without this, switching
    // tabs leaves the new active tab parked at HIDDEN_OFFSET because the
    // viewport rect hasn't changed and rectsEqual short-circuits the call.
    lastSyncedRectRef.current = null

    let rafId = 0
    const tick = () => {
      const node = viewportRef.current
      if (node) {
        const rect = node.getBoundingClientRect()
        const next: ViewportRect = {
          x: Math.round(rect.left) + RESIZE_HANDLE_INSET,
          y: Math.round(rect.top),
          width: Math.max(1, Math.round(rect.width) - RESIZE_HANDLE_INSET),
          height: Math.round(rect.height),
        }
        if (next.width > 0 && next.height > 0 && !rectsEqual(lastSyncedRectRef.current, next)) {
          lastSyncedRectRef.current = next
          void invoke("browser_resize", { ...next, tab_id: activeTabId ?? null }).catch(() => {
            /* swallow */
          })
        }
      }
      rafId = requestAnimationFrame(tick)
    }
    rafId = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(rafId)
  }, [open, activeTabId])

  useEffect(() => {
    if (open || !isTauri() || !hasWebviewRef.current) return
    lastSyncedRectRef.current = null
    void invoke("browser_hide").catch(() => {
      /* swallow */
    })
  }, [open])

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleResize = () => {
      const nextMax = viewportMaxWidth()
      setMaxWidth(nextMax)
      setWidth((current) => Math.min(current, nextMax))
    }
    window.addEventListener("resize", handleResize)
    return () => window.removeEventListener("resize", handleResize)
  }, [])

  // Wire backend events
  useEffect(() => {
    if (!isTauri()) return
    const unsubs: UnlistenFn[] = []

    void listen<BrowserUrlChangedPayload>("browser:url_changed", (event) => {
      const payload = event.payload
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
      if (payload.tabId === activeTabId) {
        setCanGoBack(payload.canGoBack)
        setCanGoForward(payload.canGoForward)
        if (!addressFocusedRef.current) {
          setAddress(payload.url)
        }
      }
    }).then((unsub) => unsubs.push(unsub))

    void listen<BrowserLoadStatePayload>("browser:load_state", (event) => {
      const payload = event.payload
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
      if (payload.tabId === activeTabId) {
        setLoading(payload.loading)
        if (payload.url && !addressFocusedRef.current) {
          setAddress(payload.url)
        }
      }
    }).then((unsub) => unsubs.push(unsub))

    void listen<BrowserTabUpdatedPayload>("browser:tab_updated", (event) => {
      setTabs(event.payload.tabs)
      const active = event.payload.tabs.find((tab) => tab.active)
      if (active) {
        setActiveTabId(active.id)
      }
    }).then((unsub) => unsubs.push(unsub))

    return () => {
      unsubs.forEach((unsub) => unsub())
    }
  }, [activeTabId])

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
        setCanGoBack(active.canGoBack)
        setCanGoForward(active.canGoForward)
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
      setMaxWidth(ceiling)
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        const next = Math.max(MIN_WIDTH, Math.min(ceiling, startWidth + delta))
        setWidth(next)
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
  }, [])

  const openUrl = useCallback(
    (target: string, options?: { tabId?: string; newTab?: boolean }) => {
      setNavError(null)

      if (!isTauri()) {
        setAddress(target)
        return
      }

      const node = viewportRef.current
      if (!node) return
      const rect = node.getBoundingClientRect()
      const forceNew = options?.newTab === true
      const payload = {
        url: target,
        x: Math.round(rect.left) + RESIZE_HANDLE_INSET,
        y: Math.round(rect.top),
        width: Math.max(1, Math.round(rect.width) - RESIZE_HANDLE_INSET),
        height: Math.max(1, Math.round(rect.height)),
        tab_id: forceNew ? null : options?.tabId ?? activeTabId ?? null,
        new_tab: forceNew,
      }
      lastSyncedRectRef.current = {
        x: payload.x,
        y: payload.y,
        width: payload.width,
        height: payload.height,
      }
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
    [activeTabId],
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
    void invoke("browser_reload", { tab_id: activeTabId ?? null }).catch(() => {
      /* swallow */
    })
  }, [activeTabId])

  const handleTabFocus = useCallback(
    (tabId: string) => {
      if (!isTauri() || tabId === activeTabId) return
      void invoke<BrowserTabMeta>("browser_tab_focus", { tab_id: tabId })
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
      void invoke<BrowserTabMeta[]>("browser_tab_close", { tab_id: tabId })
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

  // Show the tab strip (and the + button) as soon as there's any tab — otherwise
  // users have no way to open a second tab because the new-tab trigger lives there.
  const showTabs = tabs.length > 0

  return (
    <aside
      aria-hidden={!open}
      className={cn(
        "relative flex shrink-0 flex-col overflow-hidden border-l border-border/80 bg-sidebar",
        !isResizing && "transition-[width] duration-200 ease-out",
        !open && "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={{ width: open ? width : 0 }}
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

      {showTabs ? (
        <div className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-b border-border/60 px-2">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={cn(
                "group flex h-6 max-w-[160px] shrink-0 items-center gap-1 rounded-md border px-2 text-[11px]",
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
      </div>

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
      </div>
    </aside>
  )
}
