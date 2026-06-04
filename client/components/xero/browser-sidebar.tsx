"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import {
  ArrowLeft,
  ArrowRight,
  Cookie,
  FolderGit2,
  Loader2,
  MousePointerSquareDashed,
  Pencil,
  Plus,
  RotateCw,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { useSidebarOpenMotion, useSidebarWidthMotion } from "@/lib/sidebar-motion"
import { recordIpcPayloadSample } from "@/src/lib/ipc-payload-budget"
import { createSafeTauriUnlisten } from "@/src/lib/tauri-events"
import {
  useCookieImport,
  type CookieImportStatus,
  type DetectedBrowser,
} from "./browser-cookie-import"
import {
  BROWSER_TOOL_CLOSED_EVENT,
  BROWSER_TOOL_CONTEXT_EVENT,
  BROWSER_TOOL_STATE_EVENT,
  BROWSER_TOOL_DEACTIVATE_SCRIPT,
  BROWSER_TOOL_PREPARE_CAPTURE_SCRIPT,
  BROWSER_TOOL_RESTORE_CAPTURE_SCRIPT,
  browserScreenshotBytesFromBase64,
  buildBrowserToolActivationScript,
  buildBrowserToolAgentPrompt,
  buildBrowserToolVisiblePrompt,
  isDevServerUrl,
  readBrowserToolTheme,
  type BrowserAgentContextRequest,
  type BrowserToolContext,
} from "./browser-tool-injection"
import {
  createBrowserResizeScheduler,
  readBrowserViewportRect,
  type ViewportRect,
} from "./browser-resize-scheduler"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  normalizeLoopbackBrowserUrl,
  type BrowserLaunchTarget,
} from "./browser-launch-targets"

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
const TOOL_CAPTURE_SETTLE_MS = 50
const SIDEBAR_GEOMETRY_SETTLE_MS = 190
const OVERLAY_OCCLUSION_PADDING = 8
const OVERLAY_OCCLUSION_SELECTOR = [
  '[data-slot="alert-dialog-content"]',
  '[data-slot="context-menu-content"]',
  '[data-slot="context-menu-sub-content"]',
  '[data-slot="dialog-content"]',
  '[data-slot="drawer-content"]',
  '[data-slot="dropdown-menu-content"]',
  '[data-slot="dropdown-menu-sub-content"]',
  '[data-slot="hover-card-content"]',
  '[data-slot="menubar-content"]',
  '[data-slot="menubar-sub-content"]',
  '[data-slot="popover-content"]',
  '[data-slot="select-content"]',
  '[data-slot="sheet-content"]',
  '[data-slot="tooltip-content"]',
].join(",")

interface BrowserSidebarProps {
  open: boolean
  onAddAgentContext?: (request: BrowserAgentContextRequest) => Promise<void>
  projectBrowserTargets?: BrowserLaunchTarget[]
  pendingOpenUrl?: { id: string; url: string } | null
  onPendingOpenUrlConsumed?: (id: string) => void
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

interface BrowserResizeDragPayload extends ViewportRect {
  tabId: string | null
  sidebarWidth: number
  complete: boolean
}

interface BrowserResizeDragRuntime {
  latestRect: ViewportRect | null
  latestWidth: number
  nativeActive: boolean
  finish: (() => void) | null
}

interface NormalizedBrowserToolContextEvent {
  tabId: string | null
  context: BrowserToolContext
}

interface NormalizedBrowserToolClosedEvent {
  tabId: string | null
  mode: ToolMode
}

interface NormalizedBrowserToolStateEvent {
  tabId: string | null
  mode: ToolMode
  strokeCount: number
  hasDrawing: boolean
}

type BrowserOcclusionRect = ViewportRect

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

function occlusionRectsKey(rects: readonly BrowserOcclusionRect[]): string {
  return rects.map((rect) => `${rect.x},${rect.y},${rect.width},${rect.height}`).join(";")
}

function isVisibleOverlayElement(element: HTMLElement): boolean {
  if (element.closest('[aria-hidden="true"]')) return false
  const style = window.getComputedStyle(element)
  return style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0"
}

function intersectClientRects(
  viewport: ViewportRect,
  overlay: DOMRect,
): BrowserOcclusionRect | null {
  const left = Math.max(viewport.x, Math.floor(overlay.left - OVERLAY_OCCLUSION_PADDING))
  const top = Math.max(viewport.y, Math.floor(overlay.top - OVERLAY_OCCLUSION_PADDING))
  const right = Math.min(
    viewport.x + viewport.width,
    Math.ceil(overlay.right + OVERLAY_OCCLUSION_PADDING),
  )
  const bottom = Math.min(
    viewport.y + viewport.height,
    Math.ceil(overlay.bottom + OVERLAY_OCCLUSION_PADDING),
  )

  const width = right - left
  const height = bottom - top
  if (width <= 0 || height <= 0) return null

  return {
    x: left - viewport.x,
    y: top - viewport.y,
    width,
    height,
  }
}

export function collectBrowserOverlayOcclusionRects(
  viewportNode: HTMLElement,
  inset = 0,
  root: ParentNode = document,
): BrowserOcclusionRect[] {
  const viewport = readBrowserViewportRect(viewportNode, inset)
  const rects: BrowserOcclusionRect[] = []
  const seen = new Set<string>()

  root.querySelectorAll<HTMLElement>(OVERLAY_OCCLUSION_SELECTOR).forEach((element) => {
    if (!isVisibleOverlayElement(element)) return
    const rect = intersectClientRects(viewport, element.getBoundingClientRect())
    if (!rect) return
    const key = `${rect.x},${rect.y},${rect.width},${rect.height}`
    if (seen.has(key)) return
    seen.add(key)
    rects.push(rect)
  })

  return rects
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
  if (/^https?:\/\//i.test(trimmed)) return normalizeLoopbackBrowserUrl(trimmed)
  if (/^(?:localhost|127\.0\.0\.1|0\.0\.0\.0)(?::\d{1,5})?(?:[/?#].*)?$/i.test(trimmed)) {
    return normalizeLoopbackBrowserUrl(`http://${trimmed}`)
  }
  if (/^\[::1\](?::\d{1,5})?(?:[/?#].*)?$/i.test(trimmed)) {
    return `http://${trimmed}`
  }
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

function getToolErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message
  if (typeof error === "string" && error.trim()) return error
  if (typeof error === "object" && error && "message" in error) {
    const message = String((error as { message?: unknown }).message ?? "").trim()
    if (message) return message
  }
  return fallback
}

function isBrowserToolContext(value: unknown): value is BrowserToolContext {
  if (!value || typeof value !== "object") return false
  const context = value as { kind?: unknown; page?: unknown }
  if (context.kind !== "pen" && context.kind !== "inspect") return false
  if (!context.page || typeof context.page !== "object") return false
  const page = context.page as { url?: unknown }
  return typeof page.url === "string"
}

function readBrowserToolEventTabId(value: unknown): string | null {
  if (!value || typeof value !== "object") return null
  const payload = value as { tabId?: unknown; tab_id?: unknown }
  const tabId = payload.tabId ?? payload.tab_id
  return typeof tabId === "string" && tabId.trim() ? tabId : null
}

function normalizeBrowserToolContextEvent(
  value: unknown,
): NormalizedBrowserToolContextEvent | null {
  if (!value || typeof value !== "object") return null
  const payload = value as { context?: unknown }
  const context = payload.context ?? value
  if (!isBrowserToolContext(context)) return null

  return {
    tabId: readBrowserToolEventTabId(value),
    context,
  }
}

function normalizeBrowserToolClosedEvent(
  value: unknown,
): NormalizedBrowserToolClosedEvent | null {
  if (!value || typeof value !== "object") return null
  const payload = value as { mode?: unknown }
  const mode =
    payload.mode === "pen" || payload.mode === "inspect" ? payload.mode : null

  return {
    tabId: readBrowserToolEventTabId(value),
    mode,
  }
}

function normalizeBrowserToolStateEvent(
  value: unknown,
): NormalizedBrowserToolStateEvent | null {
  if (!value || typeof value !== "object") return null
  const payload = value as {
    hasDrawing?: unknown
    mode?: unknown
    strokeCount?: unknown
    stroke_count?: unknown
  }
  const mode =
    payload.mode === "pen" || payload.mode === "inspect" ? payload.mode : null
  const strokeCount = Number(payload.strokeCount ?? payload.stroke_count ?? 0)

  return {
    tabId: readBrowserToolEventTabId(value),
    mode,
    strokeCount: Number.isFinite(strokeCount) ? Math.max(0, strokeCount) : 0,
    hasDrawing: payload.hasDrawing === true || strokeCount > 0,
  }
}

function imageNameForContext(context: BrowserToolContext): string {
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-")
  return `browser-${context.kind}-${timestamp}.png`
}

function buildBrowserToolAgentPromptForCapture(
  context: BrowserToolContext,
  screenshotAttached: boolean,
): string {
  const fallbackLine =
    context.kind === "pen"
      ? "The browser sketch screenshot could not be captured, so use the note as the primary context."
      : "The browser element screenshot could not be captured, so use the selected element metadata as the primary context."

  const prompt = buildBrowserToolAgentPrompt(context, { screenshotAttached })
  return screenshotAttached ? prompt : [prompt, fallbackLine].join("\n")
}

function waitForBrowserToolPaint(): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, TOOL_CAPTURE_SETTLE_MS)
  })
}

function readBrowserViewportRectForWidth(
  node: HTMLElement,
  width: number,
  inset = 0,
): ViewportRect {
  const rect = node.getBoundingClientRect()
  const roundedWidth = Math.max(1, Math.round(width))

  return {
    x: Math.round(rect.right) - roundedWidth + inset,
    y: Math.round(rect.top),
    width: Math.max(1, roundedWidth - inset),
    height: Math.max(1, Math.round(rect.height)),
  }
}

export function BrowserSidebar({
  open,
  onAddAgentContext,
  projectBrowserTargets = [],
  pendingOpenUrl = null,
  onPendingOpenUrlConsumed,
}: BrowserSidebarProps) {
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
  const [penHasDrawing, setPenHasDrawing] = useState(false)
  const [toolSubmitting, setToolSubmitting] = useState(false)
  const [toolSubmitError, setToolSubmitError] = useState<string | null>(null)
  const motionOpen = useSidebarOpenMotion(open)
  const [openGeometrySettled, setOpenGeometrySettled] = useState(false)
  const targetWidth = motionOpen ? width : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, { isResizing })
  const {
    browsers: cookieBrowsers,
    status: importStatus,
    refresh: refreshCookieSources,
    importFrom: importCookiesFromBrowser,
  } = useCookieImport()
  const widthRef = useRef(width)
  widthRef.current = width
  const tabsRef = useRef(tabs)
  tabsRef.current = tabs
  const sidebarRef = useRef<HTMLElement | null>(null)
  const contentRef = useRef<HTMLDivElement | null>(null)
  const viewportRef = useRef<HTMLDivElement | null>(null)
  const addressFocusedRef = useRef(false)
  const hasWebviewRef = useRef(false)
  if (tabs.length > 0) {
    hasWebviewRef.current = true
  }
  const isResizingRef = useRef(false)
  const resizeDragRuntimeRef = useRef<BrowserResizeDragRuntime | null>(null)
  const cookieSourcesLoadedRef = useRef(false)
  const openRef = useRef(open)
  const activeTabIdRef = useRef(activeTabId)
  const toolModeRef = useRef(toolMode)
  const injectedToolModeRef = useRef<ToolMode>(null)
  const toolActivationRequestRef = useRef(0)
  const onAddAgentContextRef = useRef(onAddAgentContext)
  const consumedPendingOpenUrlIdsRef = useRef<Set<string>>(new Set())
  const occlusionFrameRef = useRef<number | null>(null)
  const lastOcclusionKeyRef = useRef("")

  openRef.current = open
  activeTabIdRef.current = activeTabId
  toolModeRef.current = toolMode
  onAddAgentContextRef.current = onAddAgentContext

  useEffect(() => {
    if (!open || !motionOpen) {
      setOpenGeometrySettled(false)
      return
    }

    const timeout = window.setTimeout(() => {
      setOpenGeometrySettled(true)
    }, SIDEBAR_GEOMETRY_SETTLE_MS)

    return () => window.clearTimeout(timeout)
  }, [motionOpen, open])

  const cancelBrowserOverlayOcclusionSync = useCallback(() => {
    if (occlusionFrameRef.current === null) return
    window.cancelAnimationFrame(occlusionFrameRef.current)
    occlusionFrameRef.current = null
  }, [])

  const syncBrowserOverlayOcclusions = useCallback((options?: { force?: boolean }) => {
    if (occlusionFrameRef.current !== null) return

    occlusionFrameRef.current = window.requestAnimationFrame(() => {
      occlusionFrameRef.current = null

      if (
        !openRef.current ||
        (!hasWebviewRef.current && tabsRef.current.length === 0) ||
        !isTauri()
      ) {
        return
      }

      const node = viewportRef.current
      if (!node) return

      const rects = collectBrowserOverlayOcclusionRects(node, RESIZE_HANDLE_INSET)
      const key = occlusionRectsKey(rects)
      if (!options?.force && key === lastOcclusionKeyRef.current) return

      lastOcclusionKeyRef.current = key
      void invoke("browser_set_occlusion_regions", {
        rects,
        tabId: activeTabIdRef.current,
      }).catch(() => {
        /* swallow */
      })
    })
  }, [])

  const resizeScheduler = useMemo(
    () =>
      createBrowserResizeScheduler({
        getEnabled: () =>
          openRef.current &&
          (hasWebviewRef.current || tabsRef.current.length > 0) &&
          isTauri(),
        getNode: () => viewportRef.current,
        getTabId: () => activeTabIdRef.current,
        inset: RESIZE_HANDLE_INSET,
        onResize: (rect, tabId) => {
          void invoke("browser_resize", { ...rect, tabId }).catch(() => {
            /* swallow */
          })
          syncBrowserOverlayOcclusions({ force: true })
        },
      }),
    [syncBrowserOverlayOcclusions],
  )

  const syncBrowserViewportToWidth = useCallback(
    (nextWidth: number) => {
      if (
        !openRef.current ||
        (!hasWebviewRef.current && tabsRef.current.length === 0) ||
        !isTauri()
      ) {
        return
      }
      const node = viewportRef.current
      if (!node) return

      const rect = readBrowserViewportRectForWidth(
        node,
        nextWidth,
        RESIZE_HANDLE_INSET,
      )
      resizeScheduler.markSynced(rect)
      void invoke("browser_resize", {
        ...rect,
        tabId: activeTabIdRef.current,
      }).catch(() => {
        /* swallow */
      })
      syncBrowserOverlayOcclusions({ force: true })
    },
    [resizeScheduler, syncBrowserOverlayOcclusions],
  )

  const markBrowserViewportSyncedToWidth = useCallback(
    (nextWidth: number): ViewportRect | null => {
      const node = viewportRef.current
      if (!node) return null

      const rect = readBrowserViewportRectForWidth(
        node,
        nextWidth,
        RESIZE_HANDLE_INSET,
      )
      resizeScheduler.markSynced(rect)
      return rect
    },
    [resizeScheduler],
  )

  const applySidebarWidth = useCallback((nextWidth: number) => {
    widthRef.current = nextWidth
    const nextWidthStyle = `${nextWidth}px`
    if (sidebarRef.current) {
      sidebarRef.current.style.width = nextWidthStyle
      sidebarRef.current.style.transition = "none"
    }
    if (contentRef.current) {
      contentRef.current.style.width = nextWidthStyle
    }
  }, [])

  const applyNativeResizeDrag = useCallback(
    (payload: BrowserResizeDragPayload) => {
      const runtime = resizeDragRuntimeRef.current
      if (!runtime?.nativeActive) return
      if (payload.tabId && payload.tabId !== activeTabIdRef.current) return

      const nextWidth = Math.max(
        MIN_WIDTH,
        Math.min(viewportMaxWidth(), payload.sidebarWidth),
      )
      const rect = {
        x: payload.x,
        y: payload.y,
        width: payload.width,
        height: payload.height,
      }

      runtime.latestWidth = nextWidth
      runtime.latestRect = rect
      applySidebarWidth(nextWidth)
      resizeScheduler.markSynced(rect)

      if (payload.complete) {
        runtime.finish?.()
      }
    },
    [applySidebarWidth, resizeScheduler],
  )

  const setSidebarWidthAndSync = useCallback(
    (nextWidth: number, options: { syncBrowser?: boolean } = {}) => {
      if (isResizingRef.current) {
        applySidebarWidth(nextWidth)
      } else {
        widthRef.current = nextWidth
      }
      setWidth(nextWidth)
      if (options.syncBrowser !== false) {
        syncBrowserViewportToWidth(nextWidth)
      }
    },
    [applySidebarWidth, syncBrowserViewportToWidth],
  )

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) ?? null,
    [tabs, activeTabId],
  )

  const isDevTab = isDevServerUrl(activeTab?.url ?? null)
  const pageLabel = activeTab?.title ?? activeTab?.url ?? null
  const resizeLockedByPenDrawing = toolMode === "pen" || penHasDrawing

  const deactivateInjectedTool = useCallback(async () => {
    if (!isTauri()) return
    await invoke("browser_eval_fire_and_forget", {
      js: BROWSER_TOOL_DEACTIVATE_SCRIPT,
    }).catch(() => {
      /* the active page may already have navigated away */
    })
    injectedToolModeRef.current = null
    setPenHasDrawing(false)
  }, [])

  const restoreInjectedToolCapture = useCallback(async () => {
    if (!isTauri()) return
    await invoke("browser_eval_fire_and_forget", {
      js: BROWSER_TOOL_RESTORE_CAPTURE_SCRIPT,
    }).catch(() => {
      /* best-effort restore */
    })
  }, [])

  const prepareInjectedToolCapture = useCallback(async () => {
    if (!isTauri()) return
    await invoke("browser_eval_fire_and_forget", {
      js: BROWSER_TOOL_PREPARE_CAPTURE_SCRIPT,
    })
  }, [])

  const addBrowserToolContextToAgent = useCallback(
    async (payload: unknown) => {
      const normalized = normalizeBrowserToolContextEvent(payload)
      if (!normalized) {
        return
      }
      if (normalized.tabId && normalized.tabId !== activeTabIdRef.current) {
        return
      }

      const context = normalized.context
      setToolSubmitError(null)
      setToolSubmitting(true)
      try {
        await waitForBrowserToolPaint()
        await prepareInjectedToolCapture()
        await waitForBrowserToolPaint()
        const screenshotBase64 = await invoke<string>("browser_screenshot").catch(() => null)
        let image: BrowserAgentContextRequest["image"] | undefined
        if (screenshotBase64) {
          try {
            image = {
              bytes: browserScreenshotBytesFromBase64(screenshotBase64),
              mediaType: "image/png",
              originalName: imageNameForContext(context),
            }
          } catch {
            image = undefined
          }
        }
        const add = onAddAgentContextRef.current
        if (add) {
          await add({
            prompt: buildBrowserToolAgentPromptForCapture(context, Boolean(image)),
            visiblePrompt: buildBrowserToolVisiblePrompt(context),
            ...(image ? { image } : {}),
          })
        }
        await deactivateInjectedTool()
        setToolMode(null)
      } catch (error) {
        setToolSubmitError(
          getToolErrorMessage(error, "Xero could not add this browser context to the agent composer."),
        )
        await restoreInjectedToolCapture()
      } finally {
        setToolSubmitting(false)
      }
    },
    [deactivateInjectedTool, prepareInjectedToolCapture, restoreInjectedToolCapture],
  )

  // Tools are gated to dev-server tabs. Drop the active tool whenever the
  // tab/URL changes to one that isn't a dev server, or when the sidebar closes.
  useEffect(() => {
    if (toolMode === null) return
    if (!open || !isDevTab) {
      setToolMode(null)
      setPenHasDrawing(false)
    }
  }, [open, isDevTab, toolMode])

  useEffect(() => {
    if (!isTauri()) return

    if (!open || !activeTabId || !isDevTab || toolMode === null) {
      if (injectedToolModeRef.current !== null) {
        void deactivateInjectedTool()
      }
      return
    }

    const requestId = ++toolActivationRequestRef.current
    setPenHasDrawing(false)
    const script = buildBrowserToolActivationScript({
      mode: toolMode,
      pageLabel,
      theme: readBrowserToolTheme(),
    })
    setToolSubmitError(null)
    void invoke("browser_eval_fire_and_forget", {
      js: script,
    })
      .then(() => {
        if (requestId !== toolActivationRequestRef.current || toolModeRef.current !== toolMode) {
          void deactivateInjectedTool()
          return
        }
        injectedToolModeRef.current = toolMode
      })
      .catch((error: unknown) => {
        if (requestId !== toolActivationRequestRef.current) return
        injectedToolModeRef.current = null
        setToolMode(null)
        setToolSubmitError(
          getToolErrorMessage(error, "Xero could not activate this browser tool."),
        )
      })
  }, [activeTabId, deactivateInjectedTool, isDevTab, open, pageLabel, toolMode])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

    // Reset the cache on every effect re-run (sidebar open, active tab change)
    // so the first scheduled sync fires a browser_resize. Without this,
    // switching tabs leaves the new active tab parked at HIDDEN_OFFSET because
    // the viewport rect may be unchanged.
    resizeScheduler.reset()
    resizeScheduler.schedule({ force: true })
  }, [activeTabId, open, resizeScheduler])

  useEffect(() => {
    if (!openGeometrySettled || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

    resizeScheduler.reset()
    resizeScheduler.schedule({ force: true })
  }, [activeTabId, openGeometrySettled, resizeScheduler])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

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
  }, [activeTabId, open, resizeScheduler])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

    syncBrowserOverlayOcclusions({ force: true })

    const schedule = () => syncBrowserOverlayOcclusions()
    const observer = new MutationObserver(schedule)
    if (document.body) {
      observer.observe(document.body, {
        attributes: true,
        attributeFilter: ["aria-hidden", "class", "data-state", "hidden", "style"],
        childList: true,
        subtree: true,
      })
    }

    window.addEventListener("resize", schedule)
    window.addEventListener("scroll", schedule, true)
    document.addEventListener("animationend", schedule, true)
    document.addEventListener("transitionend", schedule, true)

    return () => {
      observer.disconnect()
      window.removeEventListener("resize", schedule)
      window.removeEventListener("scroll", schedule, true)
      document.removeEventListener("animationend", schedule, true)
      document.removeEventListener("transitionend", schedule, true)
    }
  }, [activeTabId, open, syncBrowserOverlayOcclusions])

  useEffect(() => {
    if (
      open ||
      !isTauri() ||
      (!hasWebviewRef.current && tabsRef.current.length === 0)
    ) {
      return
    }
    resizeScheduler.cancel()
    resizeScheduler.reset()
    void invoke("browser_hide").catch(() => {
      /* swallow */
    })
    lastOcclusionKeyRef.current = ""
  }, [open, resizeScheduler])

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

  useEffect(
    () => () => {
      resizeScheduler.cancel()
      cancelBrowserOverlayOcclusionSync()
    },
    [cancelBrowserOverlayOcclusionSync, resizeScheduler],
  )

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
        hasWebviewRef.current = payload.tabs.length > 0
        const active = payload.tabs.find((tab) => tab.active)
        if (active) {
          activeTabIdRef.current = active.id
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

    trackUnlisten(
      listen<BrowserResizeDragPayload>("browser:resize_drag", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:resize_drag", payload: event.payload })
        applyNativeResizeDrag(event.payload)
      }),
    )

    trackUnlisten(
      listen<unknown>(BROWSER_TOOL_CONTEXT_EVENT, (event) => {
        recordIpcPayloadSample({ boundary: "event", name: BROWSER_TOOL_CONTEXT_EVENT, payload: event.payload })
        void addBrowserToolContextToAgent(event.payload)
      }),
    )

    trackUnlisten(
      listen<unknown>(BROWSER_TOOL_CLOSED_EVENT, (event) => {
        recordIpcPayloadSample({ boundary: "event", name: BROWSER_TOOL_CLOSED_EVENT, payload: event.payload })
        const payload = normalizeBrowserToolClosedEvent(event.payload)
        if (!payload) return
        if (!payload.tabId || payload.tabId === activeTabIdRef.current) {
          injectedToolModeRef.current = null
          setToolMode(null)
          setPenHasDrawing(false)
        }
      }),
    )

    trackUnlisten(
      listen<unknown>(BROWSER_TOOL_STATE_EVENT, (event) => {
        recordIpcPayloadSample({ boundary: "event", name: BROWSER_TOOL_STATE_EVENT, payload: event.payload })
        const payload = normalizeBrowserToolStateEvent(event.payload)
        if (!payload) return
        if (payload.tabId && payload.tabId !== activeTabIdRef.current) return
        setPenHasDrawing(payload.mode === "pen" && payload.hasDrawing)
      }),
    )

    return () => {
      cancelled = true
      coalescer.dispose()
      unsubs.forEach((unsub) => unsub())
    }
  }, [addBrowserToolContextToAgent, applyNativeResizeDrag])

  // Hydrate tabs when sidebar opens
  useEffect(() => {
    if (!open || !isTauri()) return
    let cancelled = false
    void safeInvoke<BrowserTabMeta[]>("browser_tab_list").then((list) => {
      if (cancelled || !list) return
      setTabs(list)
      hasWebviewRef.current = list.length > 0
      const active = list.find((tab) => tab.active) ?? list[0] ?? null
      if (active) {
        activeTabIdRef.current = active.id
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
      if (resizeLockedByPenDrawing) return
      const startX = event.clientX
      const startWidth = widthRef.current
      const ceiling = viewportMaxWidth()
      setMaxWidth(ceiling)
      isResizingRef.current = true
      setIsResizing(true)
      const hasBrowserWebview = hasWebviewRef.current || tabsRef.current.length > 0
      const runtime: BrowserResizeDragRuntime = {
        latestRect: null,
        latestWidth: startWidth,
        nativeActive:
          isTauri() && hasBrowserWebview && viewportRef.current !== null,
        finish: null,
      }
      resizeDragRuntimeRef.current = runtime

      if (runtime.nativeActive && viewportRef.current) {
        const rect = viewportRef.current.getBoundingClientRect()
        runtime.latestRect = readBrowserViewportRectForWidth(
          viewportRef.current,
          startWidth,
          RESIZE_HANDLE_INSET,
        )
        void invoke("browser_resize_drag_start", {
          startClientX: startX,
          startWidth,
          right: Math.round(rect.right),
          top: Math.round(rect.top),
          height: Math.max(1, Math.round(rect.height)),
          minWidth: MIN_WIDTH,
          maxWidth: ceiling,
          inset: RESIZE_HANDLE_INSET,
          tabId: activeTabIdRef.current,
        }).catch(() => {
          if (resizeDragRuntimeRef.current === runtime) {
            runtime.nativeActive = false
          }
        })
      }

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const cleanupDragListeners = () => {
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
      }

      const finishDrag = () => {
        if (resizeDragRuntimeRef.current !== runtime) return
        const usedNativeDragTracker = runtime.nativeActive
        const finalWidth = runtime.latestWidth
        if (usedNativeDragTracker) {
          const finalRect =
            runtime.latestRect ?? markBrowserViewportSyncedToWidth(finalWidth)
          void invoke("browser_resize_drag_end", {
            ...(finalRect ?? {}),
            tabId: activeTabIdRef.current,
          }).catch(() => {
            syncBrowserViewportToWidth(finalWidth)
          })
        }
        setSidebarWidthAndSync(finalWidth, {
          syncBrowser: !usedNativeDragTracker,
        })
        cleanupDragListeners()
        resizeDragRuntimeRef.current = null
        runtime.nativeActive = false
        isResizingRef.current = false
        setIsResizing(false)
        if (!usedNativeDragTracker) {
          resizeScheduler.schedule({ force: true })
        }
      }
      runtime.finish = finishDrag

      const handleMove = (ev: PointerEvent) => {
        ev.preventDefault()
        const delta = startX - ev.clientX
        const latestWidth = Math.max(MIN_WIDTH, Math.min(ceiling, startWidth + delta))
        runtime.latestWidth = latestWidth
        applySidebarWidth(latestWidth)
        if (runtime.nativeActive) {
          runtime.latestRect = markBrowserViewportSyncedToWidth(latestWidth)
        } else {
          syncBrowserViewportToWidth(latestWidth)
        }
      }
      const handleUp = () => {
        runtime.finish?.()
      }

      window.addEventListener("pointermove", handleMove)
      window.addEventListener("pointerup", handleUp)
      window.addEventListener("pointercancel", handleUp)
    },
    [
      applySidebarWidth,
      markBrowserViewportSyncedToWidth,
      resizeLockedByPenDrawing,
      resizeScheduler,
      setSidebarWidthAndSync,
      syncBrowserViewportToWidth,
    ],
  )

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    if (resizeLockedByPenDrawing) return
    const step = event.shiftKey ? 32 : 8
    const ceiling = viewportMaxWidth()
    const delta = event.key === "ArrowLeft" ? step : -step
    const nextWidth = Math.max(MIN_WIDTH, Math.min(ceiling, widthRef.current + delta))
    setMaxWidth(ceiling)
    setSidebarWidthAndSync(nextWidth)
    resizeScheduler.schedule({ force: true })
  }, [resizeLockedByPenDrawing, resizeScheduler, setSidebarWidthAndSync])

  const openUrl = useCallback(
    (target: string, options?: { tabId?: string; newTab?: boolean }) => {
      setNavError(null)
      const navigationTarget = normalizeLoopbackBrowserUrl(target)

      if (!isTauri()) {
        setAddress(navigationTarget)
        return
      }

      const node = viewportRef.current
      if (!node) return
      const viewport = readBrowserViewportRect(node, RESIZE_HANDLE_INSET)
      const forceNew = options?.newTab === true
      const payload = {
        url: navigationTarget,
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
            activeTabIdRef.current = meta.id
            setActiveTabId(meta.id)
          }
          syncBrowserOverlayOcclusions({ force: true })
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
    [activeTabId, resizeScheduler, syncBrowserOverlayOcclusions],
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
            activeTabIdRef.current = meta.id
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
            activeTabIdRef.current = null
            setActiveTabId(null)
            setAddress("")
            hasWebviewRef.current = false
          } else {
            const next = list.find((tab) => tab.active) ?? list[0]
            activeTabIdRef.current = next.id
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

  const handleOpenProjectBrowserTarget = useCallback(
    (target: BrowserLaunchTarget) => {
      setAddress(target.url)
      openUrl(target.url)
    },
    [openUrl],
  )

  useEffect(() => {
    if (!open || !openGeometrySettled || !pendingOpenUrl) return
    if (consumedPendingOpenUrlIdsRef.current.has(pendingOpenUrl.id)) return
    consumedPendingOpenUrlIdsRef.current.add(pendingOpenUrl.id)
    const url = normalizeLoopbackBrowserUrl(pendingOpenUrl.url)
    setAddress(url)
    openUrl(url)
    onPendingOpenUrlConsumed?.(pendingOpenUrl.id)
  }, [onPendingOpenUrlConsumed, open, openGeometrySettled, openUrl, pendingOpenUrl])

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
      ref={sidebarRef}
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
        aria-disabled={resizeLockedByPenDrawing ? true : undefined}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] bg-transparent transition-colors",
          resizeLockedByPenDrawing
            ? "cursor-not-allowed hover:bg-destructive/20"
            : "cursor-col-resize hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div
        ref={contentRef}
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
        {projectBrowserTargets.length > 1 ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                aria-label="Open project app in browser"
                className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
                title="Open project app"
                type="button"
              >
                <FolderGit2 className="h-3.5 w-3.5" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              {projectBrowserTargets.map((target) => (
                <DropdownMenuItem
                  key={target.id}
                  className="flex min-w-0 flex-col items-start gap-0.5"
                  onSelect={() => handleOpenProjectBrowserTarget(target)}
                >
                  <span className="max-w-full truncate text-[12px]">{target.label}</span>
                  <span className="max-w-full truncate font-mono text-[10.5px] text-muted-foreground">
                    {target.url}
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : (
          <button
            aria-label="Open project app in browser"
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
            disabled={projectBrowserTargets.length === 0}
            onClick={() => {
              const [target] = projectBrowserTargets
              if (target) handleOpenProjectBrowserTarget(target)
            }}
            title={
              projectBrowserTargets.length === 0
                ? "No browser-supported project app detected"
                : "Open project app"
            }
            type="button"
          >
            <FolderGit2 className="h-3.5 w-3.5" />
          </button>
        )}
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
              disabled={toolSubmitting}
              onClick={() => {
                setToolSubmitError(null)
                setToolMode((current) => (current === "pen" ? null : "pen"))
              }}
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
              disabled={toolSubmitting}
              onClick={() => {
                setToolSubmitError(null)
                setToolMode((current) => (current === "inspect" ? null : "inspect"))
              }}
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

      {toolSubmitError ? (
        <div className="shrink-0 border-b border-destructive/30 bg-destructive/10 px-3 py-2 text-[11.5px] leading-relaxed text-destructive">
          {toolSubmitError}
        </div>
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
