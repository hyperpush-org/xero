"use client"

import { useEffect, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  Bot,
  GitCompareArrows,
  Globe,
  Maximize2,
  Minus,
  Workflow as WorkflowIcon,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { AppleLogoIcon, SolanaLogoIcon } from "./brand-icons"
import { AppLogo } from "./app-logo"
import type { View } from "./data"
import { StatusFooter, type StatusFooterProps } from "./status-footer"

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

export type PlatformVariant = "macos" | "windows" | "linux"

export type SurfacePreloadTarget =
  | "agent-dock"
  | "browser"
  | "ios"
  | "settings"
  | "solana"
  | "usage"
  | "vcs"
  | "workflows"

export function detectPlatform(): PlatformVariant {
  if (typeof navigator === "undefined") return "linux"
  const ua = navigator.userAgent
  if (/Mac OS X|macOS/.test(ua)) return "macos"
  if (/Windows/.test(ua)) return "windows"
  return "linux"
}

function isPlatformVariant(value: unknown): value is PlatformVariant {
  return value === "macos" || value === "windows" || value === "linux"
}

function useDesktopPlatform(desktopRuntime: boolean): PlatformVariant {
  const [platform, setPlatform] = useState<PlatformVariant>(() => detectPlatform())

  useEffect(() => {
    if (!desktopRuntime || !isTauri()) return
    let cancelled = false
    void invoke<unknown>("desktop_platform")
      .then((value) => {
        if (!cancelled && isPlatformVariant(value)) {
          setPlatform(value)
        }
      })
      .catch(() => {
        // Keep the user-agent fallback when the command is unavailable in
        // tests or older dev builds.
      })
    return () => {
      cancelled = true
    }
  }, [desktopRuntime])

  return platform
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface XeroShellProps {
  activeView: View
  onViewChange: (view: View) => void
  onViewPreload?: (view: View) => void
  onSurfacePreload?: (target: SurfacePreloadTarget) => void
  children: React.ReactNode
  projectName?: string
  onToggleBrowser?: () => void
  browserOpen?: boolean
  onToggleIos?: () => void
  iosOpen?: boolean
  onToggleSolana?: () => void
  solanaOpen?: boolean
  onToggleVcs?: () => void
  vcsOpen?: boolean
  onToggleWorkflows?: () => void
  workflowsOpen?: boolean
  onToggleAgentDock?: () => void
  agentDockOpen?: boolean
  /** Disabled state for the agent dock toggle (e.g., when on the agent view). */
  agentDockDisabled?: boolean
  /** Number of changed files in the working tree — surfaced as a badge on the diff button. */
  vcsChangeCount?: number
  /** Lines added across the working tree (for the +/- badge). */
  vcsAdditions?: number
  /** Lines deleted across the working tree (for the +/- badge). */
  vcsDeletions?: number
  /** Dev override — null means auto-detect */
  platformOverride?: PlatformVariant | null
  /** Hide app-level controls (nav, sidebar toggle, settings). Window chrome stays. */
  chromeOnly?: boolean
  /** Hide the app status footer while retaining the main shell chrome. */
  hideFooter?: boolean
  footer?: StatusFooterProps
}

type WindowAction = "close" | "minimize" | "toggle-maximize"

const NAV_ITEMS: { id: View; label: string }[] = [
  { id: "phases", label: "Workflow" },
  { id: "agent", label: "Agent" },
  { id: "execution", label: "Editor" },
]

interface IosSdkSignal {
  supported: boolean
  xcodePresent: boolean
}

interface EmulatorSdkSignal {
  ios: IosSdkSignal
}

/** App Store listing for Xcode — the CTA target when the button is in
 * "Install Xcode" mode. */
const XCODE_STORE_URL = "https://apps.apple.com/app/xcode/id497799835"

function formatVcsCount(value: number): string {
  if (value >= 100_000) return `${Math.round(value / 1000)}k`
  if (value >= 10_000) return `${(value / 1000).toFixed(0)}k`
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`
  return value.toString()
}

/** Poll `emulator_sdk_status` and keep it in sync with the backend
 * `emulator:sdk_status_changed` event so the titlebar reacts to a fresh
 * Xcode install without a reload. */
function useEmulatorSdkSignal(desktopRuntime: boolean): EmulatorSdkSignal {
  const [signal, setSignal] = useState<EmulatorSdkSignal>({
    ios: { supported: false, xcodePresent: false },
  })

  useEffect(() => {
    if (!desktopRuntime || !isTauri()) return
    let cancelled = false
    const unlisteners: UnlistenFn[] = []

    const probe = async () => {
      try {
        const status = await invoke<{
          ios: { present: boolean; supported: boolean }
        }>("emulator_sdk_status")
        if (cancelled) return
        setSignal({
          ios: { supported: status.ios.supported, xcodePresent: status.ios.present },
        })
      } catch {
        // Probe failures leave the button in its last-known state —
        // transient errors shouldn't flicker the titlebar.
      }
    }

    void probe()

    void listen("emulator:sdk_status_changed", () => {
      void probe()
    }).then((fn) => {
      if (cancelled) {
        fn()
      } else {
        unlisteners.push(fn)
      }
    })

    // Also re-probe when the window regains focus — the user might have
    // installed Xcode between sessions without triggering a backend event.
    const onFocus = () => {
      void probe()
    }
    window.addEventListener("focus", onFocus)

    return () => {
      cancelled = true
      unlisteners.forEach((fn) => fn())
      window.removeEventListener("focus", onFocus)
    }
  }, [desktopRuntime])

  return signal
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export function XeroShell({
  activeView,
  onViewChange,
  onViewPreload,
  onSurfacePreload,
  children,
  projectName,
  onToggleBrowser,
  browserOpen = false,
  onToggleIos,
  iosOpen = false,
  onToggleSolana,
  solanaOpen = false,
  onToggleVcs,
  vcsOpen = false,
  onToggleWorkflows,
  workflowsOpen = false,
  onToggleAgentDock,
  agentDockOpen = false,
  agentDockDisabled = false,
  vcsChangeCount = 0,
  vcsAdditions = 0,
  vcsDeletions = 0,
  platformOverride,
  chromeOnly = false,
  hideFooter = false,
  footer,
}: XeroShellProps) {
  const desktopRuntime = isTauri()
  const detectedPlatform = useDesktopPlatform(desktopRuntime)
  const platform = platformOverride ?? detectedPlatform
  const emulatorSdk = useEmulatorSdkSignal(desktopRuntime)

  const handleWindowAction = async (action: WindowAction) => {
    if (!desktopRuntime) return
    const w = getCurrentWindow()
    if (action === "close") { await w.close(); return }
    if (action === "minimize") { await w.minimize(); return }
    await w.toggleMaximize()
  }

  const handleTitlebarPointerDown = async (e: React.MouseEvent<HTMLElement>) => {
    if (!desktopRuntime || e.button !== 0) return
    const target = e.target instanceof HTMLElement ? e.target : null
    if (target?.closest('[data-titlebar-no-drag="true"]')) return
    const w = getCurrentWindow()
    if (e.detail === 2) { await w.toggleMaximize(); return }
    await w.startDragging()
  }

  const stopTitlebarMouseEventPropagation = (e: React.MouseEvent<HTMLElement>) => {
    e.stopPropagation()
  }

  const queueSurfacePreload = (target: SurfacePreloadTarget) => {
    if (!onSurfacePreload) return
    if (typeof window === "undefined") {
      onSurfacePreload(target)
      return
    }

    const idleWindow = window as Window & {
      requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number
    }
    if (typeof idleWindow.requestIdleCallback === "function") {
      idleWindow.requestIdleCallback(() => onSurfacePreload(target), { timeout: 1200 })
      return
    }

    if (typeof window.requestAnimationFrame !== "function") {
      window.setTimeout(() => onSurfacePreload(target), 0)
      return
    }
    window.requestAnimationFrame(() => {
      window.setTimeout(() => onSurfacePreload(target), 0)
    })
  }

  // ------------------------------------------------------------------
  // Shared pieces
  // ------------------------------------------------------------------

  const trimmedProjectName = projectName?.trim()
  const Logo = (
    <div className="flex min-w-0 items-center gap-2">
      <AppLogo className="h-3 w-3 shrink-0" />
      <span className="shrink-0 text-[13px] font-semibold tracking-[-0.01em] text-foreground/90">Xero</span>
      {trimmedProjectName ? (
        <>
          <span
            aria-hidden="true"
            className="shrink-0 text-[13px] font-light text-muted-foreground/40"
          >
            /
          </span>
          <span className="min-w-0 truncate text-[13px] font-medium tracking-[-0.01em] text-foreground/75">
            {trimmedProjectName}
          </span>
        </>
      ) : null}
    </div>
  )

  const NavButtons = (
    <nav
      className="titlebar-no-drag flex items-center gap-1"
      data-titlebar-no-drag="true"
      onDoubleClick={stopTitlebarMouseEventPropagation}
      onMouseDown={stopTitlebarMouseEventPropagation}
    >
      {NAV_ITEMS.map(({ id, label }) => (
        <button
          key={id}
          className={cn(
            "rounded-md px-3 py-1.5 text-[13px] font-medium transition-colors",
            activeView === id
              ? "bg-secondary text-foreground"
              : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
          )}
          onClick={() => onViewChange(id)}
          onFocus={() => onViewPreload?.(id)}
          onPointerEnter={() => onViewPreload?.(id)}
          type="button"
        >
          {label}
        </button>
      ))}
    </nav>
  )

  // iOS Simulator requires Xcode. We keep the titlebar slot stable on
  // macOS:
  // - Xcode detected → normal toggle button.
  // - Xcode missing → an amber-tinted CTA that opens the Xcode App Store
  //   listing. We deliberately don't let the user open the iOS sidebar
  //   in this state because every panel inside it would ship the same
  //   "Install Xcode" message.
  // - Non-macOS → no iOS item (Xcode can't run there).
  const handleInstallXcode = () => {
    if (!desktopRuntime) return
    void openUrl(XCODE_STORE_URL).catch(() => {
      // The opener plugin surfaces its own errors; we swallow here so
      // the click handler stays sync and doesn't leak a rejection.
    })
  }

  // Before the first probe completes we don't know whether Xcode is
  // present — optimistically render the iOS toggle. Once the probe
  // resolves we flip to the CTA if Xcode is missing. The "supported"
  // flag stays true on all macOS hosts per the backend contract.
  const xcodeKnownMissing =
    platform === "macos" && desktopRuntime && emulatorSdk.ios.supported && !emulatorSdk.ios.xcodePresent

  const titlebarToolButtonClassName = (open: boolean, tone?: "warning") =>
    cn(
      "size-7 rounded-md text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      open && "bg-primary/15 text-primary hover:bg-primary/20 hover:text-primary",
      tone === "warning" && "text-warning/90 hover:bg-warning/15 hover:text-warning",
    )

  const IosToolBtn = platform === "macos" ? (
    xcodeKnownMissing ? (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            aria-label="Install Xcode"
            className={titlebarToolButtonClassName(false, "warning")}
            onClick={handleInstallXcode}
            onFocus={() => queueSurfacePreload("ios")}
            onPointerEnter={() => queueSurfacePreload("ios")}
            size="icon-sm"
            title="iOS Simulator needs Xcode. Click to install."
            type="button"
            variant="ghost"
          >
            <AppleLogoIcon className="size-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom" sideOffset={8}>Install Xcode</TooltipContent>
      </Tooltip>
    ) : (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            aria-label={iosOpen ? "Close iOS simulator" : "Open iOS simulator"}
            aria-pressed={iosOpen}
            className={titlebarToolButtonClassName(iosOpen)}
            onClick={onToggleIos}
            onFocus={() => queueSurfacePreload("ios")}
            onPointerEnter={() => queueSurfacePreload("ios")}
            size="icon-sm"
            title="iOS Simulator"
            type="button"
            variant="ghost"
          >
            <AppleLogoIcon className="size-5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom" sideOffset={8}>iOS Simulator</TooltipContent>
      </Tooltip>
    )
  ) : null

  const BrowserToolBtn = (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={browserOpen ? "Close browser" : "Open browser"}
          aria-pressed={browserOpen}
          className={titlebarToolButtonClassName(browserOpen)}
          onClick={onToggleBrowser}
          onFocus={() => queueSurfacePreload("browser")}
          onPointerEnter={() => queueSurfacePreload("browser")}
          size="icon-sm"
          title="Browser"
          type="button"
          variant="ghost"
        >
          <Globe className="h-4 w-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom" sideOffset={8}>Browser</TooltipContent>
    </Tooltip>
  )

  const SolanaToolBtn = (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={solanaOpen ? "Close Solana workbench" : "Open Solana workbench"}
          aria-pressed={solanaOpen}
          className={titlebarToolButtonClassName(solanaOpen)}
          onClick={onToggleSolana}
          onFocus={() => queueSurfacePreload("solana")}
          onPointerEnter={() => queueSurfacePreload("solana")}
          size="icon-sm"
          title="Solana Workbench"
          type="button"
          variant="ghost"
        >
          <SolanaLogoIcon className="size-4" mono />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom" sideOffset={8}>Solana Workbench</TooltipContent>
    </Tooltip>
  )

  const hasVcsLineChanges = vcsAdditions > 0 || vcsDeletions > 0
  const VcsBtn = (
    <button
      aria-label={vcsOpen ? "Close source control" : "Open source control"}
      aria-pressed={vcsOpen}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-1.5 py-1 transition-colors",
        vcsOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onFocus={() => queueSurfacePreload("vcs")}
      onClick={onToggleVcs}
      onPointerEnter={() => queueSurfacePreload("vcs")}
      title={
        vcsChangeCount > 0
          ? `${vcsChangeCount} file${vcsChangeCount === 1 ? "" : "s"} changed · +${vcsAdditions} −${vcsDeletions}`
          : "Source control"
      }
      type="button"
    >
      <GitCompareArrows className="h-4 w-4" />
      {hasVcsLineChanges ? (
        <span
          aria-hidden="true"
          className="flex items-center gap-1 font-mono text-[10.5px] font-semibold leading-none tabular-nums"
        >
          <span className="text-success">+{formatVcsCount(vcsAdditions)}</span>
          <span className="text-destructive">−{formatVcsCount(vcsDeletions)}</span>
        </span>
      ) : vcsChangeCount > 0 ? (
        <span
          aria-hidden="true"
          className="rounded-full bg-warning/90 px-1 py-px font-mono text-[9px] font-semibold leading-none tabular-nums text-black"
        >
          {vcsChangeCount > 99 ? "99+" : vcsChangeCount}
        </span>
      ) : null}
    </button>
  )

  const WorkflowsBtn = (
    <button
      aria-label={workflowsOpen ? "Close workflows" : "Open workflows"}
      aria-pressed={workflowsOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        workflowsOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onFocus={() => queueSurfacePreload("workflows")}
      onClick={onToggleWorkflows}
      onPointerEnter={() => queueSurfacePreload("workflows")}
      title="Workflows"
      type="button"
    >
      <WorkflowIcon className="h-4 w-4" />
    </button>
  )

  const AgentDockBtn = (
    <button
      aria-label={agentDockOpen ? "Close agent dock" : "Open agent dock"}
      aria-pressed={agentDockOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        agentDockDisabled && "cursor-not-allowed opacity-40",
        agentDockOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      disabled={agentDockDisabled}
      onClick={onToggleAgentDock}
      onFocus={() => queueSurfacePreload("agent-dock")}
      onPointerEnter={() => queueSurfacePreload("agent-dock")}
      title={agentDockDisabled ? "Already in Agent view" : "Agent"}
      type="button"
    >
      <Bot className="h-[17px] w-[17px]" />
    </button>
  )

  const DragSpacer = (
    <div
      aria-hidden="true"
      className="titlebar-drag-region min-w-0 flex-1 self-stretch"
      data-tauri-drag-region
      onMouseDown={(e) => void handleTitlebarPointerDown(e)}
    />
  )

  // ------------------------------------------------------------------
  // macOS traffic lights
  // ------------------------------------------------------------------

  const TrafficLights = (
    <div
      className="titlebar-no-drag mr-5 flex items-center gap-2"
      data-titlebar-no-drag="true"
      onDoubleClick={stopTitlebarMouseEventPropagation}
      onMouseDown={stopTitlebarMouseEventPropagation}
    >
      <button
        aria-label="Close window"
        className="h-3 w-3 rounded-full bg-[#ec6a5e] transition-opacity hover:opacity-85 disabled:opacity-45"
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("close")}
        type="button"
      />
      <button
        aria-label="Minimize window"
        className="h-3 w-3 rounded-full bg-[#f5bf4f] transition-opacity hover:opacity-85 disabled:opacity-45"
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("minimize")}
        type="button"
      />
      <button
        aria-label="Toggle maximize"
        className="h-3 w-3 rounded-full bg-[#61c554] transition-opacity hover:opacity-85 disabled:opacity-45"
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("toggle-maximize")}
        type="button"
      />
    </div>
  )

  // ------------------------------------------------------------------
  // Windows / Linux rectangular controls
  // ------------------------------------------------------------------

  const isLinux = platform === "linux"

  const RectControls = (
    <div
      className="titlebar-no-drag flex h-full items-stretch"
      data-titlebar-no-drag="true"
      onDoubleClick={stopTitlebarMouseEventPropagation}
      onMouseDown={stopTitlebarMouseEventPropagation}
    >
      <button
        aria-label="Minimize window"
        className={cn(
          "flex h-11 w-11 items-center justify-center text-foreground/60 transition-colors hover:bg-secondary/70 hover:text-foreground disabled:opacity-40",
          isLinux && "rounded-sm mx-0.5",
        )}
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("minimize")}
        type="button"
      >
        <Minus className="h-3.5 w-3.5" />
      </button>
      <button
        aria-label="Toggle maximize"
        className={cn(
          "flex h-11 w-11 items-center justify-center text-foreground/60 transition-colors hover:bg-secondary/70 hover:text-foreground disabled:opacity-40",
          isLinux && "rounded-sm mx-0.5",
        )}
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("toggle-maximize")}
        type="button"
      >
        <Maximize2 className="h-3 w-3" />
      </button>
      <button
        aria-label="Close window"
        className={cn(
          "flex h-11 w-11 items-center justify-center text-foreground/60 transition-colors hover:bg-destructive hover:text-white disabled:opacity-40",
          isLinux && "rounded-sm mx-0.5",
        )}
        disabled={!desktopRuntime}
        onClick={() => void handleWindowAction("close")}
        type="button"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  )

  // ------------------------------------------------------------------
  // Layout variants
  // ------------------------------------------------------------------

  let titlebar: React.ReactNode

  if (platform === "macos") {
    // macOS: [traffic-lights] [nav] ... (centered logo) ... [vcs] [workflows] [agent] [ios] [browser] [solana]
    titlebar = (
      <header className="relative flex h-11 items-center border-b border-border bg-sidebar shrink-0 pl-3 pr-3">
        {TrafficLights}
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag mr-3 flex items-center shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {NavButtons}
          </div>
        ) : null}
        {DragSpacer}
        <div className="pointer-events-none absolute left-1/2 top-1/2 flex max-w-[40vw] -translate-x-1/2 -translate-y-1/2 items-center">
          {Logo}
        </div>
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag flex items-center gap-2 shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {VcsBtn}
            {WorkflowsBtn}
            {AgentDockBtn}
            {IosToolBtn}
            {BrowserToolBtn}
            {SolanaToolBtn}
          </div>
        ) : null}
      </header>
    )
  } else {
    // Windows / Linux: [logo] [|] [nav] <- drag zone -> [vcs] [workflows] [agent] [browser] [solana] [|] [min][max][close]
    titlebar = (
      <header className="flex h-11 items-center border-b border-border bg-sidebar shrink-0 pl-3">
        <div
          className="titlebar-no-drag flex items-center shrink-0"
          data-titlebar-no-drag="true"
          onDoubleClick={stopTitlebarMouseEventPropagation}
          onMouseDown={stopTitlebarMouseEventPropagation}
        >
          {Logo}
          {!chromeOnly ? (
            <>
              <div className="mx-4 h-4 w-px bg-border" />
              {NavButtons}
            </>
          ) : null}
        </div>
        {DragSpacer}
        <div
          className="titlebar-no-drag flex items-center shrink-0"
          data-titlebar-no-drag="true"
          onDoubleClick={stopTitlebarMouseEventPropagation}
          onMouseDown={stopTitlebarMouseEventPropagation}
        >
          {!chromeOnly ? (
            <>
              {VcsBtn}
              {WorkflowsBtn}
              {AgentDockBtn}
              {BrowserToolBtn}
              {SolanaToolBtn}
              <div className="mx-2 h-4 w-px bg-border" />
            </>
          ) : null}
          {RectControls}
        </div>
      </header>
    )
  }

  return (
    <div className="xero-window-shell flex h-screen flex-col overflow-hidden bg-background text-foreground select-none">
      {titlebar}
      <main className="shell-main-row flex min-h-0 flex-1">{children}</main>
      {hideFooter ? null : (
        <StatusFooter
          git={footer?.git}
          spend={footer?.spend}
          notifications={footer?.notifications}
          spendActive={footer?.spendActive}
          onSpendClick={footer?.onSpendClick}
        />
      )}
    </div>
  )
}
