"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { CheckCircle2, Download, ExternalLink, Loader2, RefreshCw, XCircle } from "lucide-react"
import { cn } from "@/lib/utils"
import type { EmulatorPlatform } from "@/src/features/emulator/use-emulator-session"

interface SdkStatus {
  android: {
    present: boolean
    sdkRoot: string | null
    emulatorPath: string | null
    adbPath: string | null
    avdmanagerPath: string | null
  }
  ios: {
    present: boolean
    xcrunPath: string | null
    simctlPath: string | null
    idbCompanionPresent: boolean
    supported: boolean
    axPermissionGranted: boolean
    screenRecordingPermissionGranted: boolean
    helperPresent: boolean
  }
}

type ProvisionPhase =
  | "starting"
  | "ensuring_java"
  | "downloading_java"
  | "extracting_java"
  | "downloading_cmdline_tools"
  | "extracting_cmdline_tools"
  | "accepting_licenses"
  | "installing_packages"
  | "creating_avd"
  | "completed"
  | "failed"

interface ProvisionEvent {
  phase: ProvisionPhase
  message: string | null
  progress: number | null
  error: string | null
}

interface ProvisionState {
  phase: ProvisionPhase
  message: string | null
  progress: number | null
  error: string | null
  active: boolean
}

const IDLE_STATE: ProvisionState = {
  phase: "starting",
  message: null,
  progress: null,
  error: null,
  active: false,
}

const PROVISION_EVENT = "emulator:android_provision"
const SDK_STATUS_CHANGED_EVENT = "emulator:sdk_status_changed"

const PHASE_LABELS: Record<ProvisionPhase, string> = {
  starting: "Starting",
  ensuring_java: "Checking Java runtime",
  downloading_java: "Downloading Java runtime",
  extracting_java: "Unpacking Java runtime",
  downloading_cmdline_tools: "Downloading Android tools",
  extracting_cmdline_tools: "Unpacking Android tools",
  accepting_licenses: "Accepting SDK licenses",
  installing_packages: "Installing platform-tools + emulator + system image",
  creating_avd: "Creating default AVD",
  completed: "Finished",
  failed: "Failed",
}

interface Props {
  active?: boolean
  platform: EmulatorPlatform
  onDismiss?: () => void
}

/// Panel shown above the device picker when the host is missing the
/// necessary SDK. Distinct states:
/// - Android: either or both of `adb` / `emulator` not found. Offers
///   first-run provisioning that downloads cmdline-tools, a Temurin
///   JRE (when needed), platform-tools, the emulator, a system image,
///   and a default AVD into the app's data dir.
/// - iOS (macOS): Xcode / `xcrun` not found.
/// - iOS (non-macOS): hidden entirely — the shell already hides the
///   titlebar button on those hosts.
export function EmulatorMissingSdk({ active = true, platform, onDismiss }: Props) {
  const [status, setStatus] = useState<SdkStatus | null>(null)
  const [isProbing, setIsProbing] = useState(false)
  const [provision, setProvision] = useState<ProvisionState>(IDLE_STATE)
  const onDismissRef = useRef(onDismiss)
  onDismissRef.current = onDismiss

  const probe = useCallback(async () => {
    if (!active || !isTauri()) return
    setIsProbing(true)
    try {
      const next = await invoke<SdkStatus>("emulator_sdk_status")
      setStatus(next)
    } catch {
      setStatus(null)
    } finally {
      setIsProbing(false)
    }
  }, [active])

  useEffect(() => {
    if (!active) return
    void probe()
  }, [active, probe])

  // Subscribe to provisioning events so the panel reflects backend
  // progress even if the user navigates away and back. Also refresh
  // probe status when the backend signals SDK discovery changed.
  useEffect(() => {
    if (!active || !isTauri()) return
    let cancelled = false
    const unlisten: UnlistenFn[] = []

    void listen<ProvisionEvent>(PROVISION_EVENT, (event) => {
      if (cancelled) return
      const payload = event.payload
      setProvision((prev) => ({
        phase: payload.phase,
        message: payload.message ?? prev.message,
        progress: payload.progress ?? null,
        error: payload.error,
        active: payload.phase !== "completed" && payload.phase !== "failed",
      }))
      if (payload.phase === "completed") {
        void probe().then(() => {
          if (!cancelled) {
            onDismissRef.current?.()
          }
        })
      }
    }).then((fn) => unlisten.push(fn))

    void listen(SDK_STATUS_CHANGED_EVENT, () => {
      if (!cancelled) void probe()
    }).then((fn) => unlisten.push(fn))

    return () => {
      cancelled = true
      unlisten.forEach((fn) => fn())
    }
  }, [active, probe])

  const provisionStart = useCallback(async () => {
    if (!active || !isTauri()) return
    setProvision({ ...IDLE_STATE, active: true, phase: "starting" })
    try {
      await invoke("emulator_android_provision")
    } catch (err) {
      const message = errorMessage(err)
      setProvision({
        phase: "failed",
        message: null,
        progress: null,
        error: message,
        active: false,
      })
    }
  }, [active])

  if (!active || !status) return null

  const shouldShowProvisionStream = platform === "android" && provision.active
  if (shouldShowProvisionStream) {
    return <ProvisionProgressCard state={provision} />
  }

  if (platform === "android") {
    const panel = androidPanel(status)
    if (!panel) return null
    return (
      <AndroidMissingCard
        failure={provision.error}
        isProbing={isProbing}
        onDismiss={onDismiss}
        onProbe={probe}
        onProvision={provisionStart}
        panel={panel}
      />
    )
  }

  // Screen Recording permission is needed for the Swift helper's
  // ScreenCaptureKit frame capture. Show this card when the helper
  // binary is present but permission hasn't been granted yet.
  if (status.ios.present && status.ios.helperPresent && !status.ios.screenRecordingPermissionGranted) {
    return (
      <IosScreenRecordingPermissionCard isProbing={isProbing} onDismiss={onDismiss} onProbe={probe} />
    )
  }

  // AX-permission case takes precedence when Xcode is fine but macOS
  // hasn't granted us the Accessibility right. We render a dedicated
  // card because it needs invoke-backed action buttons, not just hrefs.
  if (status.ios.present && !status.ios.axPermissionGranted) {
    return (
      <IosAxPermissionCard isProbing={isProbing} onDismiss={onDismiss} onProbe={probe} />
    )
  }

  const panel = iosPanel(status)
  if (!panel) return null
  return (
    <PanelCard
      actions={panel.actions}
      detail={panel.detail}
      isProbing={isProbing}
      onDismiss={onDismiss}
      onProbe={probe}
      title={panel.title}
    />
  )
}

function IosAxPermissionCard({
  isProbing,
  onDismiss,
  onProbe,
}: {
  isProbing: boolean
  onDismiss?: () => void
  onProbe: () => void
}) {
  const [busy, setBusy] = useState(false)

  // macOS doesn't notify processes when their Accessibility trust state
  // flips — a user toggling the switch in System Settings happens
  // out-of-band. Poll while this banner is mounted so it disappears
  // within a second or two of approval, without the user having to click
  // Re-check.
  useEffect(() => {
    if (!isTauri()) return
    const handle = window.setInterval(() => {
      onProbe()
    }, 1500)
    return () => window.clearInterval(handle)
  }, [onProbe])

  const handlePrompt = useCallback(async () => {
    if (!isTauri()) return
    setBusy(true)
    try {
      // Triggers macOS's "wants to control this computer" system prompt
      // if Xero isn't yet listed in Accessibility, else no-ops. Either
      // way we re-probe right after so the banner clears the moment the
      // user flips the toggle.
      await invoke("emulator_ios_request_ax_permission")
    } finally {
      setBusy(false)
      onProbe()
    }
  }, [onProbe])

  const handleOpenSettings = useCallback(async () => {
    if (!isTauri()) return
    await invoke("emulator_ios_open_accessibility_settings")
  }, [])

  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-warning/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-warning">Accessibility permission needed</div>
      <div className="text-muted-foreground">
        Xero taps the iOS Simulator by posting synthetic mouse events — macOS
        requires Accessibility permission for that to work. Without it, taps
        silently do nothing. Enable Xero in System Settings → Privacy &
        Security → Accessibility, then re-check.
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className={cn(
            "inline-flex items-center gap-1 rounded-md border border-warning/60 bg-warning/20 px-2 py-0.5",
            "font-medium text-[11px] text-warning transition-colors hover:border-warning hover:bg-warning/30 disabled:opacity-60",
          )}
          disabled={busy}
          onClick={handleOpenSettings}
          type="button"
        >
          <ExternalLink className="h-3 w-3" />
          Open Accessibility settings
        </button>
        <button
          className={cn(
            "inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5",
            "text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60",
          )}
          disabled={busy}
          onClick={handlePrompt}
          type="button"
        >
          {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
          Prompt macOS
        </button>
        <button
          aria-label="Re-detect permission"
          className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60"
          disabled={isProbing}
          onClick={onProbe}
          type="button"
        >
          <RefreshCw className={cn("h-3 w-3", isProbing && "animate-spin")} />
          Re-check
        </button>
        {onDismiss ? (
          <button
            className="ml-auto text-[11px] text-muted-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
            onClick={onDismiss}
            type="button"
          >
            Dismiss
          </button>
        ) : null}
      </div>
    </div>
  )
}

function IosScreenRecordingPermissionCard({
  isProbing,
  onDismiss,
  onProbe,
}: {
  isProbing: boolean
  onDismiss?: () => void
  onProbe: () => void
}) {
  const [busy, setBusy] = useState(false)

  // Poll while this banner is mounted so it disappears within a second
  // or two of the user granting the permission.
  useEffect(() => {
    if (!isTauri()) return
    const handle = window.setInterval(() => {
      onProbe()
    }, 1500)
    return () => window.clearInterval(handle)
  }, [onProbe])

  const handlePrompt = useCallback(async () => {
    if (!isTauri()) return
    setBusy(true)
    try {
      await invoke("emulator_ios_request_screen_recording_permission")
    } finally {
      setBusy(false)
      onProbe()
    }
  }, [onProbe])

  const handleOpenSettings = useCallback(async () => {
    if (!isTauri()) return
    await invoke("emulator_ios_open_screen_recording_settings")
  }, [])

  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-warning/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-warning">Screen Recording permission needed</div>
      <div className="text-muted-foreground">
        Xero captures the iOS Simulator window for a smooth preview using
        ScreenCaptureKit — macOS requires Screen Recording permission for this.
        Without it, Xero falls back to slower screenshot polling. Enable Xero in
        System Settings → Privacy & Security → Screen Recording.
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className={cn(
            "inline-flex items-center gap-1 rounded-md border border-warning/60 bg-warning/20 px-2 py-0.5",
            "font-medium text-[11px] text-warning transition-colors hover:border-warning hover:bg-warning/30 disabled:opacity-60",
          )}
          disabled={busy}
          onClick={handleOpenSettings}
          type="button"
        >
          <ExternalLink className="h-3 w-3" />
          Open Screen Recording settings
        </button>
        <button
          className={cn(
            "inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5",
            "text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60",
          )}
          disabled={busy}
          onClick={handlePrompt}
          type="button"
        >
          {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
          Prompt macOS
        </button>
        <button
          aria-label="Re-detect permission"
          className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60"
          disabled={isProbing}
          onClick={onProbe}
          type="button"
        >
          <RefreshCw className={cn("h-3 w-3", isProbing && "animate-spin")} />
          Re-check
        </button>
        {onDismiss ? (
          <button
            className="ml-auto text-[11px] text-muted-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
            onClick={onDismiss}
            type="button"
          >
            Dismiss
          </button>
        ) : null}
      </div>
    </div>
  )
}

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    const message = (err as { message?: unknown }).message
    if (typeof message === "string" && message.length > 0) return message
  }
  if (typeof err === "string" && err.length > 0) return err
  return "Android SDK provisioning failed"
}

interface AndroidPanel {
  title: string
  detail: string
  actions: Array<{ label: string; href: string }>
}

function androidPanel(status: SdkStatus): AndroidPanel | null {
  if (status.android.present) return null

  return {
    title: "Android SDK not set up",
    detail:
      "Xero can install the Android SDK (command-line tools, emulator, platform-tools, and a default API 34 image) into the app's data directory. Expect a one-time ~1.5 GB download and ~5 minutes.",
    actions: [
      {
        label: "About the Android SDK",
        href: "https://developer.android.com/tools",
      },
    ],
  }
}

function iosPanel(status: SdkStatus): AndroidPanel | null {
  if (!status.ios.supported) return null
  // idb_companion is optional — the sidebar streams screenshots and routes
  // input through Core Graphics events regardless. Only surface a panel
  // when Xcode / xcrun itself is missing.
  if (status.ios.present) return null

  return {
    title: "Xcode command-line tools not found",
    detail:
      "Xero needs Xcode installed so the iOS Simulator framework is available. Run `xcode-select --install` after installing Xcode to finish the setup. Interaction also requires granting Xero Accessibility permission in System Settings → Privacy & Security → Accessibility.",
    actions: [
      { label: "Install Xcode", href: "https://apps.apple.com/app/xcode/id497799835" },
    ],
  }
}

function AndroidMissingCard({
  failure,
  isProbing,
  onDismiss,
  onProbe,
  onProvision,
  panel,
}: {
  failure: string | null
  isProbing: boolean
  onDismiss?: () => void
  onProbe: () => void
  onProvision: () => void
  panel: AndroidPanel
}) {
  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-warning/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-warning">{panel.title}</div>
      <div className="text-muted-foreground">{panel.detail}</div>
      {failure ? (
        <div className="flex items-start gap-1.5 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-destructive">
          <XCircle className="mt-[2px] h-3 w-3 shrink-0" />
          <span className="break-words">{failure}</span>
        </div>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <button
          className={cn(
            "inline-flex items-center gap-1 rounded-md border border-warning/60 bg-warning/20 px-2 py-0.5",
            "font-medium text-[11px] text-warning transition-colors hover:border-warning hover:bg-warning/30",
          )}
          onClick={onProvision}
          type="button"
        >
          <Download className="h-3 w-3" />
          Set up Android (~1.5 GB, ~5 min)
        </button>
        {panel.actions.map((action) => (
          <a
            className={cn(
              "inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5",
              "text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary",
            )}
            href={action.href}
            key={action.label}
            rel="noreferrer"
            target="_blank"
          >
            {action.label}
            <ExternalLink className="h-3 w-3" />
          </a>
        ))}
        <button
          aria-label="Re-detect SDK"
          className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60"
          disabled={isProbing}
          onClick={onProbe}
          type="button"
        >
          <RefreshCw className={cn("h-3 w-3", isProbing && "animate-spin")} />
          Re-detect
        </button>
        {onDismiss ? (
          <button
            className="ml-auto text-[11px] text-muted-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
            onClick={onDismiss}
            type="button"
          >
            Dismiss
          </button>
        ) : null}
      </div>
    </div>
  )
}

function PanelCard({
  actions,
  detail,
  isProbing,
  onDismiss,
  onProbe,
  title,
}: {
  actions: Array<{ label: string; href: string }>
  detail: string
  isProbing: boolean
  onDismiss?: () => void
  onProbe: () => void
  title: string
}) {
  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-warning/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-warning">{title}</div>
      <div className="text-muted-foreground">{detail}</div>
      <div className="flex flex-wrap items-center gap-2">
        {actions.map((action) => (
          <a
            className={cn(
              "inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5",
              "text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary",
            )}
            href={action.href}
            key={action.label}
            rel="noreferrer"
            target="_blank"
          >
            {action.label}
            <ExternalLink className="h-3 w-3" />
          </a>
        ))}
        <button
          aria-label="Re-detect SDK"
          className="inline-flex items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 py-0.5 text-[11px] text-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60"
          disabled={isProbing}
          onClick={onProbe}
          type="button"
        >
          <RefreshCw className={cn("h-3 w-3", isProbing && "animate-spin")} />
          Re-detect
        </button>
        {onDismiss ? (
          <button
            className="ml-auto text-[11px] text-muted-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
            onClick={onDismiss}
            type="button"
          >
            Dismiss
          </button>
        ) : null}
      </div>
    </div>
  )
}

function ProvisionProgressCard({ state }: { state: ProvisionState }) {
  const label = PHASE_LABELS[state.phase] ?? state.phase
  const percent = state.progress != null ? Math.round(state.progress * 100) : null

  const completed = state.phase === "completed"
  const failed = state.phase === "failed"

  return (
    <div
      aria-live="polite"
      className={cn(
        "flex shrink-0 flex-col gap-2 border-b px-3 py-2 text-[11px] leading-relaxed",
        failed
          ? "border-destructive/40 bg-destructive/10"
          : completed
            ? "border-success/40 bg-success/10"
            : "border-border/60 bg-warning/10",
      )}
      role="region"
    >
      <div className="flex items-center gap-2">
        {failed ? (
          <XCircle className="h-3.5 w-3.5 text-destructive" />
        ) : completed ? (
          <CheckCircle2 className="h-3.5 w-3.5 text-success" />
        ) : (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-warning" />
        )}
        <span className="font-medium text-foreground">
          {failed ? "Provisioning failed" : completed ? "Provisioning complete" : label}
        </span>
        {percent != null && !completed && !failed ? (
          <span className="text-muted-foreground">{percent}%</span>
        ) : null}
      </div>
      {state.message ? (
        <div className="truncate text-muted-foreground" title={state.message}>
          {state.message}
        </div>
      ) : null}
      {state.error ? (
        <div className="break-words text-destructive" title={state.error}>
          {state.error}
        </div>
      ) : null}
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-border/70">
        <div
          className={cn(
            "h-full motion-progress",
            failed
              ? "bg-destructive"
              : completed
                ? "bg-success"
                : percent != null
                  ? "bg-warning"
                  : "animate-pulse bg-warning/60",
          )}
          style={{ transform: `scaleX(${percent != null ? Math.max(0, Math.min(100, percent)) / 100 : 1})` }}
        />
      </div>
    </div>
  )
}
