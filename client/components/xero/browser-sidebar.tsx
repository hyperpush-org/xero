"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type WheelEvent,
} from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import {
  ArrowLeft,
  ArrowRight,
  Cookie,
  FolderGit2,
  Loader2,
  MousePointerSquareDashed,
  PanelLeftClose,
  PanelLeftOpen,
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
  BROWSER_TOOL_FINISH_CAPTURE_SCRIPT,
  BROWSER_TOOL_PREPARE_CAPTURE_SCRIPT,
  BROWSER_TOOL_RESTORE_CAPTURE_SCRIPT,
  browserScreenshotBytesFromBase64,
  buildBrowserToolActivationScript,
  buildBrowserToolAgentPrompt,
  buildBrowserToolContextCard,
  buildBrowserToolVisiblePrompt,
  isDevServerUrl,
  readBrowserToolTheme,
  type BrowserAgentContextRequest,
  type BrowserToolContext,
  type BrowserToolPromptMetadata,
} from "./browser-tool-injection"
import {
  createBrowserResizeScheduler,
  readBrowserViewportRect,
  type ViewportRect,
} from "./browser-resize-scheduler"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import {
  browserLaunchTargetMatchesBrowserStartTarget,
  browserLaunchTargetMatchesUrl,
  browserLaunchTargetMatchesStartTarget,
  browserRunningServerDisplayLabel,
  makeBrowserLaunchTarget,
  normalizeLoopbackBrowserUrl,
  type BrowserServerLabelStartTarget,
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
const BROWSER_CAPTURE_FRAME_OCCLUSION_PADDING = 0
const BROWSER_CAPTURE_OVERLAY_EXIT_MS = 180
const BROWSER_CAPTURE_OVERLAY_SELECTOR = '[data-xero-browser-capture-overlay="true"]'
const BROWSER_OCCLUSION_CLICK_EVENT = "browser:occlusion_click"
const PROJECT_BROWSER_TARGET_POLL_MS = 2_000
const PROJECT_TARGET_MENU_LEFT = 104
const EMPTY_BROWSER_LAUNCH_TARGETS: BrowserLaunchTarget[] = []
const EMPTY_BROWSER_START_TARGETS: BrowserServerLabelStartTarget[] = []
const OVERLAY_OCCLUSION_SELECTOR = [
  BROWSER_CAPTURE_OVERLAY_SELECTOR,
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
  projectId?: string | null
  fullWidth?: boolean
  fullWidthTarget?: number | null
  onFullWidthChange?: (fullWidth: boolean) => void
  onAddAgentContext?: (request: BrowserAgentContextRequest) => Promise<void>
  penToolDisabledReason?: string | null
  projectBrowserTargets?: BrowserLaunchTarget[]
  onProjectBrowserTargetUnavailable?: (url: string) => void
  pendingOpenUrl?: { id: string; url: string } | null
  onPendingOpenUrlConsumed?: (id: string) => void
  projectRootPath?: string | null
  projectStartTargets?: BrowserServerLabelStartTarget[]
}

interface BrowserTabMeta {
  id: string
  projectId?: string | null
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

interface BrowserDevServerUnavailablePayload {
  tabId?: string
  tab_id?: string
  url?: string
}

interface BrowserResizeDragPayload extends ViewportRect {
  tabId: string | null
  sidebarWidth: number
  complete: boolean
}

interface BrowserOcclusionWheelPayload {
  deltaX?: number | null
  deltaY?: number | null
  x?: number | null
  y?: number | null
}

interface BrowserOcclusionClickPayload {
  x?: number | null
  y?: number | null
}

interface BrowserResizeDragRuntime {
  latestRect: ViewportRect | null
  latestWidth: number
  nativeActive: boolean
  finish: (() => void) | null
}

interface BrowserRunningDevServer {
  cwd?: string | null
  detectedAt: number
  label: string
  processName?: string | null
  url: string
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
  padding = OVERLAY_OCCLUSION_PADDING,
): BrowserOcclusionRect | null {
  const left = Math.max(viewport.x, Math.floor(overlay.left - padding))
  const top = Math.max(viewport.y, Math.floor(overlay.top - padding))
  const right = Math.min(
    viewport.x + viewport.width,
    Math.ceil(overlay.right + padding),
  )
  const bottom = Math.min(
    viewport.y + viewport.height,
    Math.ceil(overlay.bottom + padding),
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
    const padding = element.matches(BROWSER_CAPTURE_OVERLAY_SELECTOR)
      ? BROWSER_CAPTURE_FRAME_OCCLUSION_PADDING
      : OVERLAY_OCCLUSION_PADDING
    const rect = intersectClientRects(viewport, element.getBoundingClientRect(), padding)
    if (!rect) return
    const key = `${rect.x},${rect.y},${rect.width},${rect.height}`
    if (seen.has(key)) return
    seen.add(key)
    rects.push(rect)
  })

  return rects
}

function canNativeWheelScrollElement(element: HTMLElement): boolean {
  const style = window.getComputedStyle(element)
  const overflowY = style.overflowY
  const overflowX = style.overflowX
  const canScrollY =
    (overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay") &&
    element.scrollHeight > element.clientHeight
  const canScrollX =
    (overflowX === "auto" || overflowX === "scroll" || overflowX === "overlay") &&
    element.scrollWidth > element.clientWidth
  return canScrollY || canScrollX
}

function findNativeWheelOverlayScrollTarget(start: Element | null): HTMLElement | null {
  let element = start instanceof HTMLElement ? start : start?.parentElement ?? null
  while (element && element !== document.body) {
    if (element.closest(OVERLAY_OCCLUSION_SELECTOR) && canNativeWheelScrollElement(element)) {
      return element
    }
    element = element.parentElement
  }
  return null
}

function viewportDefaultWidth() {
  if (typeof window === "undefined") return 640
  return Math.round(window.innerWidth * DEFAULT_RATIO)
}

function viewportMaxWidth() {
  if (typeof window === "undefined") return 1600
  return Math.max(MIN_WIDTH, window.innerWidth - RIGHT_PADDING)
}

function viewportFullWidthTarget() {
  if (typeof window === "undefined") return 960
  return Math.max(MIN_WIDTH, window.innerWidth)
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

function browserToolUrlHostLabel(url: string): string | null {
  try {
    const parsed = new URL(url)
    const host = parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname
    if (!host) return null
    return isDevServerUrl(url) ? `local app ${host}` : host
  } catch {
    return null
  }
}

function browserToolAppLabelForContext(
  context: BrowserToolContext,
  activeTab: BrowserTabMeta | null,
  targets: readonly BrowserLaunchTarget[],
): string | null {
  const candidateUrls = [context.page.url, activeTab?.url ?? null].filter(
    (url): url is string => Boolean(url),
  )
  for (const url of candidateUrls) {
    const target = targets.find((candidate) => browserLaunchTargetMatchesUrl(candidate, url))
    if (target?.label.trim()) {
      return target.label.trim()
    }
  }
  for (const url of candidateUrls) {
    const hostLabel = browserToolUrlHostLabel(url)
    if (hostLabel) return hostLabel
  }
  return activeTab?.title?.trim() || null
}

function buildBrowserToolPromptMetadataForContext(options: {
  activeTab: BrowserTabMeta | null
  attachmentName?: string | null
  captureIndex: number
  context: BrowserToolContext
  targets: readonly BrowserLaunchTarget[]
}): BrowserToolPromptMetadata {
  return {
    appLabel: browserToolAppLabelForContext(
      options.context,
      options.activeTab,
      options.targets,
    ),
    attachmentName: options.attachmentName ?? null,
    captureIndex: options.captureIndex,
  }
}

function buildBrowserToolAgentPromptForCapture(
  context: BrowserToolContext,
  screenshotAttached: boolean,
  metadata?: BrowserToolPromptMetadata,
): string {
  const fallbackLine =
    context.kind === "pen"
      ? "The browser sketch screenshot could not be captured, so use the note as the primary context."
      : "The browser element screenshot could not be captured, so use the selected element metadata as the primary context."

  const prompt = buildBrowserToolAgentPrompt(context, { metadata, screenshotAttached })
  return screenshotAttached ? prompt : [prompt, fallbackLine].join("\n")
}

function waitForBrowserToolPaint(): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, TOOL_CAPTURE_SETTLE_MS)
  })
}

function scheduleProjectBrowserTargetPoll(callback: () => void): number {
  const timeout = window.setTimeout(callback, PROJECT_BROWSER_TARGET_POLL_MS)
  ;(timeout as unknown as { unref?: () => void }).unref?.()
  return timeout
}

function shouldRepeatProjectBrowserTargetPoll(): boolean {
  const processEnv = (globalThis as { process?: { env?: Record<string, string | undefined> } })
    .process?.env
  if (import.meta.env.MODE === "test" || import.meta.env.VITEST || processEnv?.VITEST) {
    return false
  }
  return !(typeof window !== "undefined" && /jsdom/i.test(window.navigator.userAgent))
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

function browserTabBelongsToProject(tab: BrowserTabMeta, projectId: string | null): boolean {
  if (!projectId) return true
  return tab.projectId === projectId
}

function selectActiveBrowserTab(
  tabs: BrowserTabMeta[],
  projectId: string | null,
): BrowserTabMeta | null {
  const projectTabs = tabs.filter((tab) => browserTabBelongsToProject(tab, projectId))
  return projectTabs.find((tab) => tab.active) ?? projectTabs[0] ?? null
}

export function BrowserSidebar({
  open,
  projectId = null,
  fullWidth = false,
  fullWidthTarget = null,
  onFullWidthChange,
  onAddAgentContext,
  penToolDisabledReason = null,
  projectBrowserTargets = EMPTY_BROWSER_LAUNCH_TARGETS,
  onProjectBrowserTargetUnavailable,
  pendingOpenUrl = null,
  onPendingOpenUrlConsumed,
  projectRootPath = null,
  projectStartTargets = EMPTY_BROWSER_START_TARGETS,
}: BrowserSidebarProps) {
  const [width, setWidth] = useState(viewportDefaultWidth)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const [address, setAddress] = useState("")
  const [addressSuggestionsOpen, setAddressSuggestionsOpen] = useState(false)
  const [projectTargetPickerOpen, setProjectTargetPickerOpen] = useState(false)
  const [tabs, setTabs] = useState<BrowserTabMeta[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [navError, setNavError] = useState<string | null>(null)
  const [showCookieBanner, setShowCookieBanner] = useState(false)
  const [toolMode, setToolMode] = useState<ToolMode>(null)
  const [penHasDrawing, setPenHasDrawing] = useState(false)
  const [toolSubmitting, setToolSubmitting] = useState(false)
  const [captureOverlayVisible, setCaptureOverlayVisible] = useState(false)
  const [captureOverlayExiting, setCaptureOverlayExiting] = useState(false)
  const [toolSubmitError, setToolSubmitError] = useState<string | null>(null)
  const [discoveredProjectBrowserTargets, setDiscoveredProjectBrowserTargets] =
    useState<BrowserLaunchTarget[]>([])
  const [projectBrowserTargetLiveness, setProjectBrowserTargetLiveness] = useState<Record<string, boolean>>({})
  const motionOpen = useSidebarOpenMotion(open)
  const [openGeometrySettled, setOpenGeometrySettled] = useState(false)
  const activeFullWidth = open && fullWidth
  const renderedWidth = activeFullWidth
    ? Math.max(MIN_WIDTH, Math.round(fullWidthTarget ?? viewportFullWidthTarget()))
    : width
  const targetWidth = motionOpen ? renderedWidth : 0
  const widthMotion = useSidebarWidthMotion(targetWidth, {
    isResizing: isResizing && !activeFullWidth,
  })
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
  const projectTargetPickerButtonRef = useRef<HTMLButtonElement | null>(null)
  const projectTargetPanelRef = useRef<HTMLDivElement | null>(null)
  const addressFocusedRef = useRef(false)
  const hasWebviewRef = useRef(false)
  if (tabs.length > 0) {
    hasWebviewRef.current = true
  }
  const isResizingRef = useRef(false)
  const resizeDragRuntimeRef = useRef<BrowserResizeDragRuntime | null>(null)
  const cookieSourcesLoadedRef = useRef(false)
  const openRef = useRef(open)
  const projectIdRef = useRef(projectId)
  const activeTabIdRef = useRef(activeTabId)
  const activeTabRef = useRef<BrowserTabMeta | null>(null)
  const browserToolTargetsRef = useRef<BrowserLaunchTarget[]>(EMPTY_BROWSER_LAUNCH_TARGETS)
  const browserToolCaptureSequenceRef = useRef(0)
  const toolModeRef = useRef(toolMode)
  const injectedToolModeRef = useRef<ToolMode>(null)
  const finishCaptureOnOverlayExitRef = useRef(false)
  const toolActivationRequestRef = useRef(0)
  const onAddAgentContextRef = useRef(onAddAgentContext)
  const onProjectBrowserTargetUnavailableRef = useRef(onProjectBrowserTargetUnavailable)
  const consumedPendingOpenUrlIdsRef = useRef<Set<string>>(new Set())
  const occlusionFrameRef = useRef<number | null>(null)
  const lastOcclusionKeyRef = useRef("")
  const projectTargetScopeKeyRef = useRef<string | null>(null)

  openRef.current = open
  projectIdRef.current = projectId
  activeTabIdRef.current = activeTabId
  toolModeRef.current = toolMode
  onAddAgentContextRef.current = onAddAgentContext
  onProjectBrowserTargetUnavailableRef.current = onProjectBrowserTargetUnavailable

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

  const applyNativeOcclusionWheel = useCallback((payload: BrowserOcclusionWheelPayload) => {
    const deltaX = Number.isFinite(payload.deltaX) ? Number(payload.deltaX) : 0
    const deltaY = Number.isFinite(payload.deltaY) ? Number(payload.deltaY) : 0
    if (deltaX === 0 && deltaY === 0) return

    let target: HTMLElement | null = null
    const x = Number.isFinite(payload.x) ? Number(payload.x) : null
    const y = Number.isFinite(payload.y) ? Number(payload.y) : null
    const viewport = viewportRef.current
    if (viewport && x !== null && y !== null) {
      const viewportRect = viewport.getBoundingClientRect()
      target = findNativeWheelOverlayScrollTarget(
        document.elementFromPoint(viewportRect.left + x, viewportRect.top + y),
      )
    }

    const scrollTarget = target ?? projectTargetPanelRef.current
    if (!scrollTarget) return

    if (deltaX !== 0) {
      scrollTarget.scrollLeft += deltaX
    }
    if (deltaY !== 0) {
      scrollTarget.scrollTop += deltaY
    }
  }, [])

  const applyNativeOcclusionClick = useCallback((payload: BrowserOcclusionClickPayload) => {
    const x = Number.isFinite(payload.x) ? Number(payload.x) : null
    const y = Number.isFinite(payload.y) ? Number(payload.y) : null
    const viewport = viewportRef.current
    if (!viewport || x === null || y === null) return

    const viewportRect = viewport.getBoundingClientRect()
    const element = document.elementFromPoint(viewportRect.left + x, viewportRect.top + y)
    const clickTarget = element?.closest<HTMLElement>(
      'button,a,input,select,textarea,[role="button"],[tabindex]:not([tabindex="-1"])',
    )
    if (!clickTarget) return

    clickTarget.focus({ preventScroll: true })
    clickTarget.dispatchEvent(
      new MouseEvent("click", {
        bubbles: true,
        button: 0,
        cancelable: true,
        clientX: viewportRect.left + x,
        clientY: viewportRect.top + y,
      }),
    )
  }, [])

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
    () => tabs.find((tab) => tab.id === activeTabId && browserTabBelongsToProject(tab, projectId)) ?? null,
    [tabs, activeTabId, projectId],
  )
  const activeProjectTabs = useMemo(
    () => tabs.filter((tab) => browserTabBelongsToProject(tab, projectId)),
    [projectId, tabs],
  )

  const isDevTab = isDevServerUrl(activeTab?.url ?? null)
  const pageLabel = activeTab?.title ?? activeTab?.url ?? null
  const browserSupportedProjectStartTargets = useMemo(
    () => projectStartTargets.filter((target) => target.browserSupported === true),
    [projectStartTargets],
  )
  const configuredProjectBrowserTargets = useMemo(
    () =>
      projectBrowserTargets.filter((target) =>
        browserLaunchTargetMatchesBrowserStartTarget(target, projectStartTargets),
      ),
    [projectBrowserTargets, projectStartTargets],
  )
  const availableProjectBrowserTargets = useMemo(() => {
    const byId = new Map<string, BrowserLaunchTarget>()
    for (const target of discoveredProjectBrowserTargets) {
      byId.set(target.id, target)
    }
    for (const target of configuredProjectBrowserTargets) {
      byId.set(target.id, target)
    }
    const candidates = Array.from(byId.values()).sort((left, right) => right.detectedAt - left.detectedAt)
    if (browserSupportedProjectStartTargets.length === 0) return candidates

    const usedTargetIds = new Set<string>()
    return browserSupportedProjectStartTargets.flatMap((startTarget) => {
      const target = candidates.find(
        (candidate) =>
          !usedTargetIds.has(candidate.id) &&
          browserLaunchTargetMatchesStartTarget(candidate, startTarget),
      )
      if (!target) return []
      usedTargetIds.add(target.id)
      return [target]
    })
  }, [browserSupportedProjectStartTargets, configuredProjectBrowserTargets, discoveredProjectBrowserTargets])
  const liveProjectBrowserTargets = useMemo(
    () => availableProjectBrowserTargets.filter((target) => projectBrowserTargetLiveness[target.id] === true),
    [availableProjectBrowserTargets, projectBrowserTargetLiveness],
  )
  const showProjectTargetPanel =
    liveProjectBrowserTargets.length > 0 && (addressSuggestionsOpen || projectTargetPickerOpen)
  const isCheckingProjectBrowserTargets =
    availableProjectBrowserTargets.length > 0 &&
    availableProjectBrowserTargets.some((target) => !(target.id in projectBrowserTargetLiveness))
  activeTabRef.current = activeTab
  browserToolTargetsRef.current = availableProjectBrowserTargets
  const resizeLockedByPenDrawing = toolMode === "pen" || penHasDrawing
  const resizeDisabled = activeFullWidth || resizeLockedByPenDrawing
  const isPenToolDisabled = toolSubmitting || Boolean(penToolDisabledReason)
  const penToolTooltip = penToolDisabledReason ?? "Sketch on page"
  const fullWidthButtonLabel = activeFullWidth ? "Show agent panel" : "Hide agent panel"
  const projectTargetScopeKey = useMemo(
    () =>
      JSON.stringify([
        projectId ?? "",
        projectRootPath ?? "",
        projectStartTargets.map((target) => [
          target.name,
          target.command,
          target.browserSupported === true,
        ]),
      ]),
    [projectId, projectRootPath, projectStartTargets],
  )

  useEffect(() => {
    if (!penToolDisabledReason || toolMode !== "pen") return
    setToolMode(null)
  }, [penToolDisabledReason, toolMode])

  useEffect(() => {
    if (projectTargetScopeKeyRef.current === null) {
      projectTargetScopeKeyRef.current = projectTargetScopeKey
      return
    }
    if (projectTargetScopeKeyRef.current === projectTargetScopeKey) return
    projectTargetScopeKeyRef.current = projectTargetScopeKey
    setDiscoveredProjectBrowserTargets([])
    setProjectBrowserTargetLiveness({})
    setProjectTargetPickerOpen(false)
  }, [projectTargetScopeKey])

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

  const finishInjectedToolCapture = useCallback(async () => {
    if (!isTauri()) return
    try {
      await invoke("browser_eval_fire_and_forget", {
        js: BROWSER_TOOL_FINISH_CAPTURE_SCRIPT(BROWSER_CAPTURE_OVERLAY_EXIT_MS),
      })
      injectedToolModeRef.current = null
      setPenHasDrawing(false)
    } catch {
      await deactivateInjectedTool()
    }
  }, [deactivateInjectedTool])

  const checkProjectBrowserTargetRunning = useCallback(async (target: BrowserLaunchTarget) => {
    if (!isTauri()) return false
    const running = await invoke<boolean>("browser_dev_server_running", {
      url: target.url,
    }).catch(() => false)
    return running === true
  }, [])

  const markProjectBrowserTargetUnavailable = useCallback((target: BrowserLaunchTarget) => {
    setProjectBrowserTargetLiveness((current) => {
      if (current[target.id] === false) return current
      return { ...current, [target.id]: false }
    })
    onProjectBrowserTargetUnavailableRef.current?.(target.url)
  }, [])

  const refreshRunningProjectBrowserTargets = useCallback(async () => {
    if (!isTauri()) return
    const servers = await invoke<BrowserRunningDevServer[]>(
      "browser_list_running_dev_servers",
    ).catch(() => null)
    if (!servers) return

    const targets = servers.flatMap((server) => {
      const label = browserRunningServerDisplayLabel(
        server,
        projectStartTargets,
        projectRootPath,
      )
      if (!label) return []

      const target = makeBrowserLaunchTarget({
        detectedAt: server.detectedAt,
        label,
        source: label.split(" · ", 1)[0] ?? null,
        url: server.url,
      })
      return target ? [target] : []
    })
    setDiscoveredProjectBrowserTargets(targets)
  }, [projectRootPath, projectStartTargets])

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
      if (context.kind === "pen" && penToolDisabledReason) {
        setToolSubmitError(penToolDisabledReason)
        await restoreInjectedToolCapture()
        return
      }
      const captureIndex = browserToolCaptureSequenceRef.current + 1
      browserToolCaptureSequenceRef.current = captureIndex
      if (context.kind === "inspect") {
        const metadata = buildBrowserToolPromptMetadataForContext({
          activeTab: activeTabRef.current,
          captureIndex,
          context,
          targets: browserToolTargetsRef.current,
        })
        try {
          const add = onAddAgentContextRef.current
          if (add) {
            await add({
              prompt: buildBrowserToolAgentPrompt(context, {
                metadata,
                screenshotAttached: false,
              }),
              visiblePrompt: buildBrowserToolVisiblePrompt(context),
              contextCard: buildBrowserToolContextCard(context),
            })
          }
          await deactivateInjectedTool()
          setToolMode(null)
        } catch (error) {
          setToolSubmitError(
            getToolErrorMessage(error, "Xero could not add this browser context to the agent composer."),
          )
          await restoreInjectedToolCapture()
        }
        return
      }

      setToolSubmitting(true)
      try {
        await prepareInjectedToolCapture()
        await waitForBrowserToolPaint()
        const screenshotBase64 = await invoke<string>("browser_screenshot").catch(() => null)
        const imageName = imageNameForContext(context)
        let image: BrowserAgentContextRequest["image"] | undefined
        if (screenshotBase64) {
          try {
            image = {
              bytes: browserScreenshotBytesFromBase64(screenshotBase64),
              mediaType: "image/png",
              originalName: imageName,
            }
          } catch {
            image = undefined
          }
        }
        const metadata = buildBrowserToolPromptMetadataForContext({
          activeTab: activeTabRef.current,
          attachmentName: image ? imageName : null,
          captureIndex,
          context,
          targets: browserToolTargetsRef.current,
        })
        const add = onAddAgentContextRef.current
        if (add) {
          await add({
            prompt: buildBrowserToolAgentPromptForCapture(context, Boolean(image), metadata),
            visiblePrompt: buildBrowserToolVisiblePrompt(context),
            contextCard: buildBrowserToolContextCard(context),
            ...(image ? { image } : {}),
          })
        }
        finishCaptureOnOverlayExitRef.current = true
        injectedToolModeRef.current = null
        setPenHasDrawing(false)
        setToolMode(null)
      } catch (error) {
        finishCaptureOnOverlayExitRef.current = false
        setToolSubmitError(
          getToolErrorMessage(error, "Xero could not add this browser context to the agent composer."),
        )
        await restoreInjectedToolCapture()
      } finally {
        setToolSubmitting(false)
      }
    },
    [
      deactivateInjectedTool,
      penToolDisabledReason,
      prepareInjectedToolCapture,
      restoreInjectedToolCapture,
    ],
  )

  useEffect(() => {
    if (toolSubmitting) {
      setCaptureOverlayVisible(true)
      setCaptureOverlayExiting(false)
      return
    }

    if (!captureOverlayVisible) return

    finishCaptureOnOverlayExitRef.current =
      finishCaptureOnOverlayExitRef.current && isTauri()
    setCaptureOverlayExiting(true)
    const timeout = window.setTimeout(() => {
      setCaptureOverlayVisible(false)
      setCaptureOverlayExiting(false)
    }, BROWSER_CAPTURE_OVERLAY_EXIT_MS)

    return () => window.clearTimeout(timeout)
  }, [captureOverlayVisible, toolSubmitting])

  useEffect(() => {
    if (!captureOverlayExiting || !finishCaptureOnOverlayExitRef.current) return
    finishCaptureOnOverlayExitRef.current = false
    void finishInjectedToolCapture()
  }, [captureOverlayExiting, finishInjectedToolCapture])

  useEffect(() => {
    if (!open || !isTauri()) return
    syncBrowserOverlayOcclusions({ force: true })
  }, [captureOverlayExiting, captureOverlayVisible, open, syncBrowserOverlayOcclusions])

  useEffect(() => {
    if (!open || !isTauri()) {
      setDiscoveredProjectBrowserTargets([])
      return
    }

    let cancelled = false
    let timeout: number | null = null

    const refreshTargets = async () => {
      await refreshRunningProjectBrowserTargets()
      if (cancelled) return
      if (shouldRepeatProjectBrowserTargetPoll()) {
        timeout = scheduleProjectBrowserTargetPoll(refreshTargets)
      }
    }

    void refreshTargets()

    return () => {
      cancelled = true
      if (timeout !== null) window.clearTimeout(timeout)
    }
  }, [open, refreshRunningProjectBrowserTargets])

  useEffect(() => {
    if (!open || availableProjectBrowserTargets.length === 0 || !isTauri()) {
      setProjectBrowserTargetLiveness({})
      setProjectTargetPickerOpen(false)
      return
    }

    let cancelled = false
    let timeout: number | null = null

    const checkTargets = async () => {
      const snapshot = availableProjectBrowserTargets
      const results = await Promise.all(
        snapshot.map(async (target) => ({
          running: await checkProjectBrowserTargetRunning(target),
          target,
        })),
      )
      if (cancelled) return

      const next: Record<string, boolean> = {}
      for (const { running, target } of results) {
        next[target.id] = running
        if (!running) {
          onProjectBrowserTargetUnavailableRef.current?.(target.url)
        }
      }
      setProjectBrowserTargetLiveness(next)
      if (shouldRepeatProjectBrowserTargetPoll()) {
        timeout = scheduleProjectBrowserTargetPoll(checkTargets)
      }
    }

    void checkTargets()

    return () => {
      cancelled = true
      if (timeout !== null) window.clearTimeout(timeout)
    }
  }, [availableProjectBrowserTargets, checkProjectBrowserTargetRunning, open])

  useEffect(() => {
    if (liveProjectBrowserTargets.length > 0) return
    setProjectTargetPickerOpen(false)
  }, [liveProjectBrowserTargets.length])

  useEffect(() => {
    if (!open) {
      setProjectTargetPickerOpen(false)
      setAddressSuggestionsOpen(false)
    }
  }, [open])

  useEffect(() => {
    if (!projectTargetPickerOpen) return

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Node)) return
      if (projectTargetPickerButtonRef.current?.contains(target)) return
      if (projectTargetPanelRef.current?.contains(target)) return
      setProjectTargetPickerOpen(false)
    }

    document.addEventListener("pointerdown", handlePointerDown, true)
    return () => document.removeEventListener("pointerdown", handlePointerDown, true)
  }, [projectTargetPickerOpen])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return
    resizeScheduler.reset()
    resizeScheduler.schedule({ force: true })
    syncBrowserOverlayOcclusions({ force: true })
  }, [open, resizeScheduler, showProjectTargetPanel, syncBrowserOverlayOcclusions])

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
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return
    const forceSync = () => {
      resizeScheduler.reset()
      resizeScheduler.schedule({ force: true })
    }

    forceSync()
    const timeout = window.setTimeout(forceSync, SIDEBAR_GEOMETRY_SETTLE_MS)
    return () => window.clearTimeout(timeout)
  }, [activeFullWidth, fullWidthTarget, open, resizeScheduler])

  useEffect(() => {
    if (!openGeometrySettled || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

    resizeScheduler.reset()
    resizeScheduler.schedule({ force: true })
  }, [activeTabId, openGeometrySettled, resizeScheduler])

  useEffect(() => {
    if (!open || !isTauri()) return
    if (!hasWebviewRef.current && tabsRef.current.length === 0) return

    const viewportNode = viewportRef.current
    const sidebarNode = sidebarRef.current
    if (!viewportNode && !sidebarNode) return

    const ResizeObserverCtor = window.ResizeObserver
    if (typeof ResizeObserverCtor !== "function") {
      resizeScheduler.schedule()
      return
    }

    const observer = new ResizeObserverCtor(() => {
      resizeScheduler.schedule()
    })
    if (viewportNode) observer.observe(viewportNode)
    if (sidebarNode && sidebarNode !== viewportNode) observer.observe(sidebarNode)

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
        hasWebviewRef.current = payload.tabs.some((tab) =>
          browserTabBelongsToProject(tab, projectIdRef.current),
        )
        const active = selectActiveBrowserTab(payload.tabs, projectIdRef.current)
        if (active) {
          activeTabIdRef.current = active.id
          setActiveTabId(active.id)
          setLoading(active.loading)
          if (active.url && !addressFocusedRef.current) {
            setAddress(active.url)
          }
        } else {
          activeTabIdRef.current = null
          setActiveTabId(null)
          setLoading(false)
          if (!addressFocusedRef.current) {
            setAddress("")
          }
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
      listen<BrowserDevServerUnavailablePayload>("browser:dev_server_unavailable", (event) => {
        recordIpcPayloadSample({
          boundary: "event",
          name: "browser:dev_server_unavailable",
          payload: event.payload,
        })
        const url = typeof event.payload?.url === "string" ? event.payload.url : null
        if (url) onProjectBrowserTargetUnavailableRef.current?.(url)
      }),
    )

    trackUnlisten(
      listen<BrowserResizeDragPayload>("browser:resize_drag", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:resize_drag", payload: event.payload })
        applyNativeResizeDrag(event.payload)
      }),
    )

    trackUnlisten(
      listen<BrowserOcclusionClickPayload>(BROWSER_OCCLUSION_CLICK_EVENT, (event) => {
        recordIpcPayloadSample({ boundary: "event", name: BROWSER_OCCLUSION_CLICK_EVENT, payload: event.payload })
        applyNativeOcclusionClick(event.payload)
      }),
    )

    trackUnlisten(
      listen<BrowserOcclusionWheelPayload>("browser:occlusion_wheel", (event) => {
        recordIpcPayloadSample({ boundary: "event", name: "browser:occlusion_wheel", payload: event.payload })
        applyNativeOcclusionWheel(event.payload)
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
  }, [addBrowserToolContextToAgent, applyNativeOcclusionClick, applyNativeOcclusionWheel, applyNativeResizeDrag])

  // Hydrate tabs when sidebar opens
  useEffect(() => {
    if (!open || !isTauri()) return
    let cancelled = false
    void safeInvoke<BrowserTabMeta[]>("browser_tab_list", {
      projectId,
    }).then((list) => {
      if (cancelled || !list) return
      setTabs(list)
      hasWebviewRef.current = list.length > 0
      const active = selectActiveBrowserTab(list, projectId)
      if (active) {
        activeTabIdRef.current = active.id
        setActiveTabId(active.id)
        setLoading(active.loading)
        if (active.url && !addressFocusedRef.current) setAddress(active.url)
      } else {
        activeTabIdRef.current = null
        setActiveTabId(null)
        setLoading(false)
        if (!addressFocusedRef.current) setAddress("")
      }
    })
    return () => {
      cancelled = true
    }
  }, [open, projectId])

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
        projectId: projectIdRef.current,
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
    if (loading) {
      setLoading(false)
      void invoke("browser_stop").catch(() => {
        /* swallow */
      })
      return
    }
    void invoke("browser_reload", { tabId: activeTabId ?? null }).catch(() => {
      /* swallow */
    })
  }, [activeTabId, loading])

  const handleTabFocus = useCallback(
    (tabId: string) => {
      if (!isTauri() || tabId === activeTabId) return
      void invoke<BrowserTabMeta>("browser_tab_focus", {
        projectId: projectIdRef.current,
        tabId,
      })
        .then((meta) => {
          if (meta) {
            activeTabIdRef.current = meta.id
            setActiveTabId(meta.id)
            setLoading(meta.loading)
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
      void invoke<BrowserTabMeta[]>("browser_tab_close", {
        projectId: projectIdRef.current,
        tabId,
      })
        .then((list) => {
          if (!list) return
          setTabs(list)
          if (list.length === 0) {
            activeTabIdRef.current = null
            setActiveTabId(null)
            setAddress("")
            setLoading(false)
            hasWebviewRef.current = false
          } else {
            const next = selectActiveBrowserTab(list, projectIdRef.current) ?? list[0]
            activeTabIdRef.current = next.id
            setActiveTabId(next.id)
            setLoading(next.loading)
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
    async (target: BrowserLaunchTarget) => {
      setAddressSuggestionsOpen(false)
      setProjectTargetPickerOpen(false)

      if (projectBrowserTargetLiveness[target.id] !== true) {
        const running = await checkProjectBrowserTargetRunning(target)
        if (!running) {
          markProjectBrowserTargetUnavailable(target)
          return
        }
      }

      setProjectBrowserTargetLiveness((current) => {
        if (current[target.id] === true) return current
        return { ...current, [target.id]: true }
      })
      setAddress(target.url)
      openUrl(target.url)
    },
    [
      checkProjectBrowserTargetRunning,
      markProjectBrowserTargetUnavailable,
      openUrl,
      projectBrowserTargetLiveness,
    ],
  )

  const handleProjectTargetPanelWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    const panel = event.currentTarget
    const deltaY =
      event.deltaMode === 1
        ? event.deltaY * 16
        : event.deltaMode === 2
          ? event.deltaY * panel.clientHeight
          : event.deltaY

    panel.scrollTop += deltaY
    event.preventDefault()
    event.stopPropagation()
  }, [])

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
    if (activeProjectTabs.length === 0) return
    if (cookieSourcesLoadedRef.current) return
    cookieSourcesLoadedRef.current = true

    const prompted = readCookiePromptFlag()

    void refreshCookieSources().then((list) => {
      if (prompted) return
      if (!list.some((browser) => browser.available)) return
      setShowCookieBanner(true)
    })
  }, [activeProjectTabs.length, open, refreshCookieSources])

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
  const showTabs = activeProjectTabs.length > 0

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
      {!activeFullWidth ? (
        <div
          aria-label="Resize browser sidebar"
          aria-orientation="vertical"
          aria-valuemax={maxWidth}
          aria-valuemin={MIN_WIDTH}
          aria-valuenow={width}
          aria-disabled={resizeDisabled ? true : undefined}
          className={cn(
            "absolute inset-y-0 -left-[3px] z-10 w-[6px] bg-transparent transition-colors",
            resizeDisabled
              ? "cursor-not-allowed hover:bg-destructive/20"
              : "cursor-col-resize hover:bg-primary/30",
            isResizing && "bg-primary/40",
          )}
          onKeyDown={handleResizeKey}
          onPointerDown={handleResizeStart}
          role="separator"
          tabIndex={open ? 0 : -1}
        />
      ) : null}

      <div
        ref={contentRef}
        className="relative flex h-full min-w-0 shrink-0 flex-col"
        style={{ width: renderedWidth }}
      >
      {showTabs ? (
        <div className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-b border-border/60">
          {activeProjectTabs.map((tab) => (
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
        <button
          ref={projectTargetPickerButtonRef}
          aria-expanded={projectTargetPickerOpen}
          aria-label="Open project app in browser"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
          disabled={liveProjectBrowserTargets.length === 0}
          onClick={() => {
            if (liveProjectBrowserTargets.length > 1) {
              setProjectTargetPickerOpen((current) => !current)
              setAddressSuggestionsOpen(false)
              return
            }
            const [target] = liveProjectBrowserTargets
            if (target) void handleOpenProjectBrowserTarget(target)
          }}
          title={
            liveProjectBrowserTargets.length === 0
              ? isCheckingProjectBrowserTargets
                ? "Checking project app availability"
                : "No running browser-supported project app detected"
              : "Open project app"
          }
          type="button"
        >
          <FolderGit2 className="h-3.5 w-3.5" />
        </button>
        <form className="ml-1 flex min-w-0 flex-1" onSubmit={handleSubmit}>
          <input
            aria-label="Address"
            autoCapitalize="none"
            autoComplete="off"
            autoCorrect="off"
            className="h-7 w-full min-w-0 rounded-md border border-border/70 bg-background/40 px-2 text-[11.5px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
            onBlur={() => {
              addressFocusedRef.current = false
              setAddressSuggestionsOpen(false)
            }}
            onChange={(event) => {
              setAddress(event.target.value)
              setAddressSuggestionsOpen(true)
              setProjectTargetPickerOpen(false)
            }}
            onFocus={(event) => {
              addressFocusedRef.current = true
              setAddressSuggestionsOpen(true)
              setProjectTargetPickerOpen(false)
              event.currentTarget.select()
            }}
            placeholder="Search or enter URL"
            spellCheck={false}
            type="text"
            value={address}
          />
        </form>
        {isDevTab ? (
          <div
            className="ml-1 flex shrink-0 items-center gap-0.5 rounded-md border border-border/60 bg-background/40 px-0.5"
            data-testid="browser-dev-tools"
          >
            {onFullWidthChange ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    aria-label={fullWidthButtonLabel}
                    aria-pressed={activeFullWidth}
                    className={cn(
                      "flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground",
                      activeFullWidth
                        ? "bg-primary/15 text-primary hover:bg-primary/20 hover:text-primary"
                        : null,
                    )}
                    onClick={() => onFullWidthChange(!activeFullWidth)}
                    title={fullWidthButtonLabel}
                    type="button"
                  >
                    {activeFullWidth ? (
                      <PanelLeftOpen className="h-3.5 w-3.5" />
                    ) : (
                      <PanelLeftClose className="h-3.5 w-3.5" />
                    )}
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom">{fullWidthButtonLabel}</TooltipContent>
              </Tooltip>
            ) : null}
            <Tooltip>
              <TooltipTrigger asChild>
                <span
                  className="inline-flex"
                  tabIndex={penToolDisabledReason ? 0 : -1}
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
                    disabled={isPenToolDisabled}
                    onClick={() => {
                      if (penToolDisabledReason) return
                      setToolSubmitError(null)
                      setToolMode((current) => (current === "pen" ? null : "pen"))
                    }}
                    title={penToolTooltip}
                    type="button"
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                </span>
              </TooltipTrigger>
              <TooltipContent side="bottom">{penToolTooltip}</TooltipContent>
            </Tooltip>
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

      {showProjectTargetPanel ? (
        <div
          ref={projectTargetPanelRef}
          aria-label="Project app suggestions"
          className="motion-popover absolute z-30 max-h-72 w-[min(18rem,calc(100%-7rem))] origin-top-left overflow-x-hidden overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
          data-slot="dropdown-menu-content"
          data-state="open"
          onWheelCapture={handleProjectTargetPanelWheel}
          style={{ left: PROJECT_TARGET_MENU_LEFT, top: showTabs ? 78 : 46 }}
        >
          <div className="px-2 py-1.5 text-[11px] font-medium uppercase text-muted-foreground">
            Local server
          </div>
          {liveProjectBrowserTargets.map((target) => (
            <button
              key={target.id}
              aria-label={`Open ${target.label}`}
              className="relative flex w-full min-w-0 cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm outline-hidden select-none transition-colors hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
              onClick={() => {
                void handleOpenProjectBrowserTarget(target)
              }}
              onMouseDown={(event) => event.preventDefault()}
              title={target.url}
              type="button"
            >
              <span className="min-w-0 truncate font-mono text-[12.5px]">
                {target.label}
              </span>
            </button>
          ))}
        </div>
      ) : null}

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
        {captureOverlayVisible ? (
          <div
            role="status"
            aria-live="polite"
            aria-busy={toolSubmitting}
            aria-label="Adding browser context"
            className="xero-browser-capture-indicator pointer-events-none absolute inset-0 z-20"
            data-state={captureOverlayExiting ? "closed" : "open"}
          >
            <div className="xero-browser-capture-aura" aria-hidden="true">
              <span className="xero-browser-capture-aura-field" />
            </div>
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-top"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-right"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-bottom"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-left"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-corner xero-browser-capture-occlusion-top-left"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-corner xero-browser-capture-occlusion-top-right"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-corner xero-browser-capture-occlusion-bottom-right"
            />
            <span
              data-xero-browser-capture-overlay="true"
              className="xero-browser-capture-occlusion xero-browser-capture-occlusion-corner xero-browser-capture-occlusion-bottom-left"
            />
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
