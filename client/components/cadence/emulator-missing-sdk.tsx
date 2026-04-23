"use client"

import { useCallback, useEffect, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { ExternalLink, RefreshCw } from "lucide-react"
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
  }
}

interface Props {
  platform: EmulatorPlatform
  onDismiss?: () => void
}

/// Panel shown above the device picker when the host is missing the
/// necessary SDK. Distinct states:
/// - Android: either or both of `adb` / `emulator` not found on PATH or
///   `ANDROID_HOME`.
/// - iOS (macOS): Xcode / `xcrun` not found, or `idb_companion` not
///   bundled and not on PATH.
/// - iOS (non-macOS): hidden entirely — the shell already hides the
///   titlebar button on those hosts.
export function EmulatorMissingSdk({ platform, onDismiss }: Props) {
  const [status, setStatus] = useState<SdkStatus | null>(null)
  const [isProbing, setIsProbing] = useState(false)

  const probe = useCallback(async () => {
    if (!isTauri()) return
    setIsProbing(true)
    try {
      const next = await invoke<SdkStatus>("emulator_sdk_status")
      setStatus(next)
    } catch {
      setStatus(null)
    } finally {
      setIsProbing(false)
    }
  }, [])

  useEffect(() => {
    void probe()
  }, [probe])

  if (!status) return null

  const panel = platform === "android" ? androidPanel(status) : iosPanel(status)
  if (!panel) return null

  return (
    <div
      aria-live="polite"
      className="flex shrink-0 flex-col gap-2 border-b border-border/60 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed"
      role="region"
    >
      <div className="font-medium text-amber-200">{panel.title}</div>
      <div className="text-muted-foreground">{panel.detail}</div>
      <div className="flex flex-wrap items-center gap-2">
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
          onClick={() => void probe()}
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

interface Panel {
  title: string
  detail: string
  actions: Array<{ label: string; href: string }>
}

function androidPanel(status: SdkStatus): Panel | null {
  if (status.android.present) return null

  const missing: string[] = []
  if (!status.android.adbPath) missing.push("adb")
  if (!status.android.emulatorPath) missing.push("emulator")

  return {
    title: "Android SDK not found",
    detail: `Install Android Studio and create at least one AVD. Missing: ${
      missing.length ? missing.join(", ") : "unknown tool"
    }. Cadence looks on PATH, in ANDROID_HOME / ANDROID_SDK_ROOT, and at standard Android Studio install locations.`,
    actions: [
      {
        label: "Download Android Studio",
        href: "https://developer.android.com/studio",
      },
      {
        label: "Create an AVD",
        href: "https://developer.android.com/studio/run/managing-avds",
      },
    ],
  }
}

function iosPanel(status: SdkStatus): Panel | null {
  if (!status.ios.supported) return null
  // Packaged builds hydrate idb_companion into the Tauri resource directory
  // during build, so `idbCompanionPresent` flips true without the user
  // installing anything. The only remaining hard prerequisite is Xcode
  // itself — `idb_companion` links against Apple's private
  // CoreSimulator.framework, which only ships inside Xcode.
  if (status.ios.present && status.ios.idbCompanionPresent) return null

  if (!status.ios.present) {
    return {
      title: "Xcode command-line tools not found",
      detail:
        "Cadence needs Xcode installed so the iOS Simulator framework is available. Run `xcode-select --install` after installing Xcode to finish the setup.",
      actions: [
        { label: "Install Xcode", href: "https://apps.apple.com/app/xcode/id497799835" },
      ],
    }
  }

  // Xcode is present, but the bundled idb_companion is somehow missing —
  // typically a dev build started with `CADENCE_SKIP_SIDECAR_FETCH=1` or a
  // packaged build with a corrupted Resources/ tree. Give the user both a
  // manual path and the rebuild hint.
  return {
    title: "idb_companion sidecar missing",
    detail:
      "The bundled idb_companion helper is not present in this build. Packaged installers include it automatically; for a dev build, rebuild without CADENCE_SKIP_SIDECAR_FETCH, or install it via Homebrew so it resolves from PATH.",
    actions: [
      { label: "Install via Homebrew", href: "https://github.com/facebook/idb" },
    ],
  }
}
