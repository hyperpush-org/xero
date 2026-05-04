"use client"

import { useEffect, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  Bot,
  ChevronDown,
  GitCompareArrows,
  Gamepad2,
  Github,
  Globe,
  Maximize2,
  Minus,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  Workflow as WorkflowIcon,
  Wrench,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { AndroidLogoIcon, AppleLogoIcon, SolanaLogoIcon } from "./brand-icons"
import { AppLogo } from "./app-logo"
import type { View } from "./data"
import { StatusFooter, type StatusFooterProps } from "./status-footer"

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

export type PlatformVariant = "macos" | "windows" | "linux"

export type SurfacePreloadTarget =
  | "browser"
  | "games"
  | "android"
  | "ios"
  | "settings"
  | "solana"
  | "tools"
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
  onOpenSettings?: () => void
  /** Open the settings dialog focused on the Account section (used when a session exists). */
  onOpenAccount?: () => void
  /** Kick off the GitHub OAuth flow directly (used when no session exists). */
  onAccountLogin?: () => void
  /** Truthy when a GitHub login is in flight — surfaces a subtle loading state on the avatar button. */
  accountAuthenticating?: boolean
  /** When provided, the account button shows the GitHub avatar + signed-in state. */
  accountAvatarUrl?: string | null
  accountLogin?: string | null
  onToggleGames?: () => void
  gamesOpen?: boolean
  onToggleBrowser?: () => void
  browserOpen?: boolean
  onToggleIos?: () => void
  iosOpen?: boolean
  onToggleAndroid?: () => void
  androidOpen?: boolean
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
  sidebarCollapsed?: boolean
  onToggleSidebar?: () => void
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
  /** `android.present` is already sufficient for the Android button —
   * we don't hide it when absent, just let the sidebar offer provisioning. */
  androidPresent: boolean
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
 * Xcode install (or an Android SDK provisioning finish) without a
 * reload. */
function useEmulatorSdkSignal(desktopRuntime: boolean): EmulatorSdkSignal {
  const [signal, setSignal] = useState<EmulatorSdkSignal>({
    ios: { supported: false, xcodePresent: false },
    androidPresent: false,
  })

  useEffect(() => {
    if (!desktopRuntime || !isTauri()) return
    let cancelled = false
    const unlisteners: UnlistenFn[] = []

    const probe = async () => {
      try {
        const status = await invoke<{
          android: { present: boolean }
          ios: { present: boolean; supported: boolean }
        }>("emulator_sdk_status")
        if (cancelled) return
        setSignal({
          ios: { supported: status.ios.supported, xcodePresent: status.ios.present },
          androidPresent: status.android.present,
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
    // installed Xcode (or brew-installed the Android SDK) between
    // sessions without triggering a backend event.
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
  onOpenSettings,
  onOpenAccount,
  onAccountLogin,
  accountAuthenticating = false,
  accountAvatarUrl = null,
  accountLogin = null,
  onToggleGames,
  gamesOpen = false,
  onToggleBrowser,
  browserOpen = false,
  onToggleIos,
  iosOpen = false,
  onToggleAndroid,
  androidOpen = false,
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
  sidebarCollapsed = false,
  onToggleSidebar,
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

  const runAfterMenuClosePaint = (action?: () => void) => {
    if (!action) return
    if (typeof window === "undefined") {
      action()
      return
    }

    if (typeof window.requestAnimationFrame !== "function") {
      window.setTimeout(action, 0)
      return
    }

    window.requestAnimationFrame(() => {
      window.setTimeout(action, 0)
    })
  }

  // ------------------------------------------------------------------
  // Shared pieces
  // ------------------------------------------------------------------

  const Logo = (
    <div className="flex items-center gap-2">
      <AppLogo className="h-3 w-3" />
      <span className="text-[13px] font-semibold tracking-[-0.01em] text-foreground/90">Xero</span>
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

  const accountSignedIn = Boolean(accountAvatarUrl)
  const accountAriaLabel = accountSignedIn
    ? `Account — signed in as ${accountLogin ?? "GitHub user"}`
    : accountAuthenticating
      ? "Signing in with GitHub…"
      : "Sign in with GitHub"
  const handleAccountClick = () => {
    if (accountSignedIn) {
      onOpenAccount?.()
    } else if (onAccountLogin) {
      onAccountLogin()
    } else {
      onOpenAccount?.()
    }
  }
  const AccountBtn = (
    <button
      aria-label={accountAriaLabel}
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-md transition-colors",
        accountSignedIn
          ? "text-foreground hover:bg-secondary/50"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
        accountAuthenticating && "opacity-70",
      )}
      disabled={accountAuthenticating}
      onClick={handleAccountClick}
      title={
        accountSignedIn
          ? `@${accountLogin ?? ""}`
          : accountAuthenticating
            ? "Waiting for GitHub…"
            : "Sign in with GitHub"
      }
      type="button"
    >
      {accountSignedIn && accountAvatarUrl ? (
        <img
          src={accountAvatarUrl}
          alt=""
          referrerPolicy="no-referrer"
          className="h-4 w-4 rounded-full"
        />
      ) : (
        <Github className="h-4 w-4" />
      )}
    </button>
  )

  const SettingsBtn = (
    <button
      aria-label="Settings"
      className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
      onFocus={() => onSurfacePreload?.("settings")}
      onClick={onOpenSettings}
      onPointerEnter={() => onSurfacePreload?.("settings")}
      type="button"
    >
      <Settings className="h-4 w-4" />
    </button>
  )

  const GamesBtn = (
    <button
      aria-label={gamesOpen ? "Close arcade" : "Open arcade"}
      aria-pressed={gamesOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        gamesOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onFocus={() => onSurfacePreload?.("games")}
      onClick={onToggleGames}
      onPointerEnter={() => onSurfacePreload?.("games")}
      type="button"
    >
      <Gamepad2 className="h-4 w-4" />
    </button>
  )

  // iOS Simulator requires Xcode. We keep the titlebar slot stable on
  // macOS:
  // - Xcode detected → normal toggle menu item.
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

  // Android button stays visible on every platform — even without an
  // SDK installed, clicking it opens the sidebar, which surfaces the
  // one-click provisioning flow. Setup-needed state is conveyed via the
  // tooltip rather than an amber tint so the tool menu reads as a
  // uniform set.
  // Before the first probe completes we don't know whether Xcode is
  // present — optimistically render the iOS toggle. Once the probe
  // resolves we flip to the CTA if Xcode is missing. The "supported"
  // flag stays true on all macOS hosts per the backend contract.
  const xcodeKnownMissing =
    platform === "macos" && desktopRuntime && emulatorSdk.ios.supported && !emulatorSdk.ios.xcodePresent
  const androidSetupNeeded =
    desktopRuntime && !emulatorSdk.androidPresent && !androidOpen
  const toolPanelOpen = iosOpen || androidOpen || browserOpen || solanaOpen

  const toolItemClassName =
    "min-w-44 cursor-pointer px-2.5 py-2 text-[13px]"
  const activeToolItemClassName = "bg-primary/10 text-primary focus:bg-primary/15 focus:text-primary"
  const ToolActiveDot = (
    <span
      aria-hidden="true"
      className="ml-auto h-1.5 w-1.5 rounded-full bg-primary"
    />
  )
  const ToolsMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          aria-label="Tools"
          aria-pressed={toolPanelOpen}
          className={cn(
            "flex items-center gap-1 rounded-md px-1.5 py-1.5 transition-colors data-[state=open]:bg-secondary/70 data-[state=open]:text-foreground",
            toolPanelOpen
              ? "bg-primary/15 text-primary"
              : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
          )}
          onFocus={() => onSurfacePreload?.("tools")}
          onPointerDown={() => onSurfacePreload?.("tools")}
          onPointerEnter={() => onSurfacePreload?.("tools")}
          title="Tools"
          type="button"
        >
          <Wrench className="h-4 w-4" />
          <ChevronDown className="h-3 w-3 opacity-70" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={8}>
        {platform === "macos" ? (
          xcodeKnownMissing ? (
            <DropdownMenuItem
              aria-label="Install Xcode"
              className={cn(
                toolItemClassName,
                "text-warning/90 focus:bg-warning/15 focus:text-warning",
              )}
              onFocus={() => onSurfacePreload?.("ios")}
              onPointerEnter={() => onSurfacePreload?.("ios")}
              onSelect={() => runAfterMenuClosePaint(handleInstallXcode)}
              title="iOS Simulator needs Xcode. Click to install."
            >
              <AppleLogoIcon className="h-4 w-4" />
              <span>Install Xcode</span>
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              aria-label={iosOpen ? "Close iOS simulator" : "Open iOS simulator"}
              className={cn(toolItemClassName, iosOpen && activeToolItemClassName)}
              onFocus={() => onSurfacePreload?.("ios")}
              onPointerEnter={() => onSurfacePreload?.("ios")}
              onSelect={() => runAfterMenuClosePaint(onToggleIos)}
            >
              <AppleLogoIcon className="h-4 w-4" />
              <span>iOS Simulator</span>
              {iosOpen ? ToolActiveDot : null}
            </DropdownMenuItem>
          )
        ) : null}
        <DropdownMenuItem
          aria-label={androidOpen ? "Close Android emulator" : "Open Android emulator"}
          className={cn(toolItemClassName, androidOpen && activeToolItemClassName)}
          onFocus={() => onSurfacePreload?.("android")}
          onPointerEnter={() => onSurfacePreload?.("android")}
          onSelect={() => runAfterMenuClosePaint(onToggleAndroid)}
          title={
            androidSetupNeeded
              ? "Android SDK not installed - click to set it up"
              : undefined
          }
        >
          <AndroidLogoIcon className="h-4 w-4" />
          <span>Android Emulator</span>
          {androidOpen ? ToolActiveDot : null}
        </DropdownMenuItem>
        <DropdownMenuItem
          aria-label={browserOpen ? "Close browser" : "Open browser"}
          className={cn(toolItemClassName, browserOpen && activeToolItemClassName)}
          onFocus={() => onSurfacePreload?.("browser")}
          onPointerEnter={() => onSurfacePreload?.("browser")}
          onSelect={() => runAfterMenuClosePaint(onToggleBrowser)}
        >
          <Globe className="h-4 w-4" />
          <span>Browser</span>
          {browserOpen ? ToolActiveDot : null}
        </DropdownMenuItem>
        <DropdownMenuItem
          aria-label={solanaOpen ? "Close Solana workbench" : "Open Solana workbench"}
          className={cn(toolItemClassName, solanaOpen && activeToolItemClassName)}
          onFocus={() => onSurfacePreload?.("solana")}
          onPointerEnter={() => onSurfacePreload?.("solana")}
          onSelect={() => runAfterMenuClosePaint(onToggleSolana)}
        >
          <SolanaLogoIcon className="h-4 w-4" mono />
          <span>Solana Workbench</span>
          {solanaOpen ? ToolActiveDot : null}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
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
      onFocus={() => onSurfacePreload?.("vcs")}
      onClick={onToggleVcs}
      onPointerEnter={() => onSurfacePreload?.("vcs")}
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
      onFocus={() => onSurfacePreload?.("workflows")}
      onClick={onToggleWorkflows}
      onPointerEnter={() => onSurfacePreload?.("workflows")}
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
      title={agentDockDisabled ? "Already in Agent view" : "Agent"}
      type="button"
    >
      <Bot className="h-4 w-4" />
    </button>
  )

  const SidebarToggleBtn = (
    <button
      aria-label={sidebarCollapsed ? "Expand project sidebar" : "Collapse project sidebar"}
      aria-pressed={!sidebarCollapsed}
      className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
      onClick={onToggleSidebar}
      type="button"
    >
      {sidebarCollapsed ? <PanelLeftOpen className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
    </button>
  )

  const Divider = <div className="h-4 w-px shrink-0 bg-border" />

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
    // macOS: [traffic-lights] [sidebar-toggle] [|] [nav] ··· (centered logo) ··· [vcs] [tools] [games] [settings]
    titlebar = (
      <header className="relative flex h-11 items-center border-b border-border bg-sidebar shrink-0 pl-3 pr-3">
        {TrafficLights}
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag mr-3 flex items-center gap-3 shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {SidebarToggleBtn}
            {Divider}
          </div>
        ) : null}
        {!chromeOnly ? NavButtons : null}
        {DragSpacer}
        <div className="pointer-events-none absolute left-1/2 top-1/2 flex -translate-x-1/2 -translate-y-1/2 items-center">
          {Logo}
        </div>
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag flex items-center gap-2 shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {WorkflowsBtn}
            {VcsBtn}
            {ToolsMenu}
            {GamesBtn}
            {AgentDockBtn}
            {AccountBtn}
            {SettingsBtn}
          </div>
        ) : null}
      </header>
    )
  } else {
    // Windows / Linux: [logo] [|] [sidebar-toggle] [|] [nav] ← drag zone → [games] [settings] [|] [min][max][close]
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
              {SidebarToggleBtn}
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
              {WorkflowsBtn}
              {VcsBtn}
              {ToolsMenu}
              {GamesBtn}
              {AgentDockBtn}
              {AccountBtn}
              {SettingsBtn}
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
