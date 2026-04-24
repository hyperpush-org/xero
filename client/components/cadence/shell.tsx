"use client"

import { useEffect, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  Gamepad2,
  Globe,
  Maximize2,
  Minus,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { AndroidLogoIcon, AppleLogoIcon, SolanaLogoIcon } from "./brand-icons"
import type { View } from "./data"
import { StatusFooter, type StatusFooterProps } from "./status-footer"

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

export type PlatformVariant = "macos" | "windows" | "linux"

export function detectPlatform(): PlatformVariant {
  if (typeof navigator === "undefined") return "linux"
  const ua = navigator.userAgent
  if (/Mac OS X|macOS/.test(ua)) return "macos"
  if (/Windows/.test(ua)) return "windows"
  return "linux"
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface CadenceShellProps {
  activeView: View
  onViewChange: (view: View) => void
  children: React.ReactNode
  projectName?: string
  onOpenSettings?: () => void
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
  sidebarCollapsed?: boolean
  onToggleSidebar?: () => void
  /** Dev override — null means auto-detect */
  platformOverride?: PlatformVariant | null
  /** Hide app-level controls (nav, sidebar toggle, settings). Window chrome stays. */
  chromeOnly?: boolean
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

export function CadenceShell({
  activeView,
  onViewChange,
  children,
  onOpenSettings,
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
  sidebarCollapsed = false,
  onToggleSidebar,
  platformOverride,
  chromeOnly = false,
  footer,
}: CadenceShellProps) {
  const desktopRuntime = isTauri()
  const platform = platformOverride ?? detectPlatform()
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

  // ------------------------------------------------------------------
  // Shared pieces
  // ------------------------------------------------------------------

  const Logo = (
    <div className="flex items-center gap-2">
      <svg className="text-primary" fill="none" height="16" viewBox="0 0 24 24" width="16">
        <path d="M4 4h6v6H4V4Z" fill="currentColor" />
        <path d="M14 4h6v6h-6V4Z" fill="currentColor" fillOpacity="0.25" />
        <path d="M4 14h6v6H4v-6Z" fill="currentColor" fillOpacity="0.25" />
        <path d="M14 14h6v6h-6v-6Z" fill="currentColor" />
      </svg>
      <span className="text-[13px] font-semibold tracking-[-0.01em] text-foreground/90">Cadence</span>
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
          type="button"
        >
          {label}
        </button>
      ))}
    </nav>
  )

  const SettingsBtn = (
    <button
      aria-label="Settings"
      className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
      onClick={onOpenSettings}
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
      onClick={onToggleGames}
      type="button"
    >
      <Gamepad2 className="h-4 w-4" />
    </button>
  )

  const BrowserBtn = (
    <button
      aria-label={browserOpen ? "Close browser" : "Open browser"}
      aria-pressed={browserOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        browserOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onClick={onToggleBrowser}
      type="button"
    >
      <Globe className="h-4 w-4" />
    </button>
  )

  // iOS Simulator requires Xcode. We keep the titlebar slot stable on
  // macOS:
  // - Xcode detected → normal toggle button.
  // - Xcode missing → an amber-tinted CTA that opens the Xcode App Store
  //   listing. We deliberately don't let the user open the iOS sidebar
  //   in this state because every panel inside it would ship the same
  //   "Install Xcode" message.
  // - Non-macOS → nothing (Xcode can't run there).
  const handleInstallXcode = () => {
    if (!desktopRuntime) return
    void openUrl(XCODE_STORE_URL).catch(() => {
      // The opener plugin surfaces its own errors; we swallow here so
      // the click handler stays sync and doesn't leak a rejection.
    })
  }

  let IosBtn: React.ReactNode = null
  if (platform === "macos") {
    // Before the first probe completes we don't know whether Xcode is
    // present — optimistically render the toggle. Once the probe
    // resolves we flip to the CTA if Xcode is missing. The "supported"
    // flag stays true on all macOS hosts per the backend contract.
    const xcodeKnownMissing = desktopRuntime && emulatorSdk.ios.supported && !emulatorSdk.ios.xcodePresent
    IosBtn = xcodeKnownMissing ? (
      <button
        aria-label="Install Xcode"
        className={cn(
          "rounded-md p-1.5 transition-colors",
          "text-amber-300/80 hover:bg-amber-500/15 hover:text-amber-200",
        )}
        onClick={handleInstallXcode}
        title="iOS Simulator needs Xcode. Click to install."
        type="button"
      >
        <AppleLogoIcon className="h-4 w-4" />
      </button>
    ) : (
      <button
        aria-label={iosOpen ? "Close iOS simulator" : "Open iOS simulator"}
        aria-pressed={iosOpen}
        className={cn(
          "rounded-md p-1.5 transition-colors",
          iosOpen
            ? "bg-primary/15 text-primary"
            : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
        )}
        onClick={onToggleIos}
        type="button"
      >
        <AppleLogoIcon className="h-4 w-4" />
      </button>
    )
  }

  // Android button stays visible on every platform — even without an
  // SDK installed, clicking it opens the sidebar, which surfaces the
  // one-click provisioning flow. Setup-needed state is conveyed via the
  // tooltip rather than an amber tint so the titlebar icons read as a
  // uniform set.
  const androidSetupNeeded =
    desktopRuntime && !emulatorSdk.androidPresent && !androidOpen
  const AndroidBtn = (
    <button
      aria-label={androidOpen ? "Close Android emulator" : "Open Android emulator"}
      aria-pressed={androidOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        androidOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onClick={onToggleAndroid}
      title={
        androidSetupNeeded
          ? "Android SDK not installed — click to set it up"
          : undefined
      }
      type="button"
    >
      <AndroidLogoIcon className="h-4 w-4" />
    </button>
  )

  const SolanaBtn = (
    <button
      aria-label={solanaOpen ? "Close Solana workbench" : "Open Solana workbench"}
      aria-pressed={solanaOpen}
      className={cn(
        "rounded-md p-1.5 transition-colors",
        solanaOpen
          ? "bg-primary/15 text-primary"
          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground",
      )}
      onClick={onToggleSolana}
      type="button"
    >
      <SolanaLogoIcon className="h-4 w-4" mono />
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
    // macOS: [traffic-lights] [logo] [|] [sidebar-toggle] ← drag zone → [nav] [|] [games] [settings]
    titlebar = (
      <header className="flex h-11 items-center border-b border-border bg-sidebar shrink-0 pl-3 pr-3">
        {TrafficLights}
        {Logo}
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag ml-3 flex items-center gap-3 shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {Divider}
            {SidebarToggleBtn}
          </div>
        ) : null}
        {DragSpacer}
        {!chromeOnly ? (
          <div
            className="titlebar-no-drag flex items-center gap-2 shrink-0"
            data-titlebar-no-drag="true"
            onDoubleClick={stopTitlebarMouseEventPropagation}
            onMouseDown={stopTitlebarMouseEventPropagation}
          >
            {NavButtons}
            <div className="mx-1.5 h-4 w-px bg-border" />
            {IosBtn}
            {AndroidBtn}
            {BrowserBtn}
            {SolanaBtn}
            {GamesBtn}
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
              {IosBtn}
              {AndroidBtn}
              {BrowserBtn}
              {SolanaBtn}
              {GamesBtn}
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
    <div className="cadence-window-shell flex h-screen flex-col overflow-hidden bg-background text-foreground select-none">
      {titlebar}
      <main className="flex min-h-0 flex-1">{children}</main>
      <StatusFooter git={footer?.git} runtime={footer?.runtime} />
    </div>
  )
}
