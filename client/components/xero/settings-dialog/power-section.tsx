import { useCallback, useEffect, useState } from "react"
import { AlertTriangle, BatteryCharging, Laptop, LoaderCircle, RefreshCw, Zap } from "lucide-react"

import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  AdrenalineModeSettingsDto,
  ClosedLidModeSettingsDto,
  UpsertAdrenalineModeSettingsRequestDto,
  UpsertClosedLidModeSettingsRequestDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export type PowerSettingsAdapter = Pick<
  XeroDesktopAdapter,
  | "isDesktopRuntime"
  | "adrenalineModeSettings"
  | "adrenalineModeUpdateSettings"
  | "closedLidModeSettings"
  | "closedLidModeUpdateSettings"
>

interface PowerSectionProps {
  adapter?: PowerSettingsAdapter
}

type StatusTone = "ok" | "warn" | "bad" | "muted"

const TONE_BG: Record<StatusTone, string> = {
  ok: "bg-success/10",
  warn: "bg-warning/10",
  bad: "bg-destructive/10",
  muted: "bg-muted/40",
}

const TONE_RING: Record<StatusTone, string> = {
  ok: "ring-success/20",
  warn: "ring-warning/25",
  bad: "ring-destructive/25",
  muted: "ring-border/60",
}

const TONE_TEXT: Record<StatusTone, string> = {
  ok: "text-success dark:text-success",
  warn: "text-warning dark:text-warning",
  bad: "text-destructive",
  muted: "text-muted-foreground",
}

const TONE_DOT: Record<StatusTone, string> = {
  ok: "bg-success dark:bg-success",
  warn: "bg-warning dark:bg-warning",
  bad: "bg-destructive",
  muted: "bg-muted-foreground/60",
}

const FALLBACK_ADRENALINE_SETTINGS: AdrenalineModeSettingsDto = {
  enabled: false,
  assertionKind: "prevent_idle_system_sleep",
  active: false,
  activeStatus: "unsupported",
  platformSupported: false,
  updatedAt: null,
  diagnosticMessage: null,
}

const FALLBACK_CLOSED_LID_SETTINGS: ClosedLidModeSettingsDto = {
  enabled: false,
  active: false,
  activeStatus: "unsupported",
  platformSupported: false,
  authorizationRequired: false,
  currentDisablesleep: null,
  previousDisablesleep: null,
  updatedAt: null,
  diagnosticMessage: null,
}

export function PowerSection({ adapter }: PowerSectionProps) {
  const [settings, setSettings] = useState<AdrenalineModeSettingsDto>(FALLBACK_ADRENALINE_SETTINGS)
  const [closedLidSettings, setClosedLidSettings] = useState<ClosedLidModeSettingsDto>(
    FALLBACK_CLOSED_LID_SETTINGS,
  )
  const [loadState, setLoadState] = useState<"idle" | "loading" | "ready" | "error">("idle")
  const [saveTarget, setSaveTarget] = useState<"adrenaline" | "closed_lid" | null>(null)
  const [confirmClosedLidOpen, setConfirmClosedLidOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() &&
      adapter.adrenalineModeSettings &&
      adapter.adrenalineModeUpdateSettings &&
      adapter.closedLidModeSettings &&
      adapter.closedLidModeUpdateSettings,
  )

  const load = useCallback(() => {
    if (!canUseAdapter || !adapter?.adrenalineModeSettings) {
      setSettings(FALLBACK_ADRENALINE_SETTINGS)
      setClosedLidSettings(FALLBACK_CLOSED_LID_SETTINGS)
      setLoadState("ready")
      return
    }

    setLoadState("loading")
    setError(null)
    Promise.all([adapter.adrenalineModeSettings(), adapter.closedLidModeSettings?.()])
      .then(([nextSettings, nextClosedLidSettings]) => {
        setSettings(nextSettings)
        setClosedLidSettings(nextClosedLidSettings ?? FALLBACK_CLOSED_LID_SETTINGS)
        setLoadState("ready")
      })
      .catch((loadError) => {
        setSettings(FALLBACK_ADRENALINE_SETTINGS)
        setClosedLidSettings(FALLBACK_CLOSED_LID_SETTINGS)
        setError(getErrorMessage(loadError, "Xero could not load power settings."))
        setLoadState("error")
      })
  }, [adapter, canUseAdapter])

  useEffect(() => {
    load()
  }, [load])

  const isBusy = loadState === "loading" || saveTarget !== null
  const switchDisabled = isBusy || !canUseAdapter || !settings.platformSupported
  const closedLidSwitchDisabled = isBusy || !canUseAdapter || !closedLidSettings.platformSupported
  const status = statusSummary(settings, saveTarget === "adrenaline")
  const closedLidStatus = closedLidStatusSummary(closedLidSettings, saveTarget === "closed_lid")
  const attentionMessage = error ?? settings.diagnosticMessage ?? closedLidSettings.diagnosticMessage

  const updateAdrenalineMode = (enabled: boolean) => {
    if (!adapter?.adrenalineModeUpdateSettings) return

    const previous = settings
    const request: UpsertAdrenalineModeSettingsRequestDto = {
      enabled,
      assertionKind: settings.assertionKind,
    }

    setSettings((current) => ({
      ...current,
      enabled,
      active: enabled ? current.active : false,
      activeStatus: enabled ? current.activeStatus : "inactive",
      diagnosticMessage: null,
    }))
    setSaveTarget("adrenaline")
    setError(null)

    adapter
      .adrenalineModeUpdateSettings(request)
      .then((nextSettings) => {
        setSettings(nextSettings)
      })
      .catch((saveError) => {
        setSettings(previous)
        setError(getErrorMessage(saveError, "Xero could not save Adrenaline Mode settings."))
      })
      .finally(() => setSaveTarget(null))
  }

  const requestClosedLidModeChange = (enabled: boolean) => {
    if (enabled) {
      setConfirmClosedLidOpen(true)
      return
    }

    updateClosedLidMode(false, false)
  }

  const updateClosedLidMode = (enabled: boolean, acknowledgeGlobalPowerChange: boolean) => {
    if (!adapter?.closedLidModeUpdateSettings) return

    const previous = closedLidSettings
    const request: UpsertClosedLidModeSettingsRequestDto = {
      enabled,
      acknowledgeGlobalPowerChange,
    }

    setClosedLidSettings((current) => ({
      ...current,
      enabled,
      activeStatus: enabled ? current.activeStatus : "inactive",
      diagnosticMessage: null,
    }))
    setSaveTarget("closed_lid")
    setError(null)

    adapter
      .closedLidModeUpdateSettings(request)
      .then((nextSettings) => {
        setClosedLidSettings(nextSettings)
      })
      .catch((saveError) => {
        setClosedLidSettings(previous)
        setError(getErrorMessage(saveError, "Xero could not save Closed-Lid Mode settings."))
      })
      .finally(() => setSaveTarget(null))
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Power"
        description="Control Xero's app-level power behavior on this device."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={isBusy || !canUseAdapter}
            onClick={load}
          >
            {loadState === "loading" ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
        }
      />

      {!canUseAdapter ? (
        <div className="flex flex-col items-center gap-2 rounded-md border border-dashed border-border/60 bg-secondary/10 px-5 py-8 text-center">
          <Zap className="h-4 w-4 text-muted-foreground" />
          <p className="text-[12.5px] font-medium text-foreground">Desktop runtime required</p>
          <p className="max-w-sm text-[11.5px] leading-[1.5] text-muted-foreground">
            Power settings are available when Xero is running as a desktop app.
          </p>
        </div>
      ) : (
        <>
          {attentionMessage ? (
            <Alert variant="destructive" className="rounded-md px-3 py-2 text-[12px]">
              <AlertTriangle className="h-3.5 w-3.5" />
              <AlertTitle className="text-[12px]">Power settings need attention</AlertTitle>
              <AlertDescription className="text-[12px]">
                {attentionMessage}
              </AlertDescription>
            </Alert>
          ) : null}

          <section className="rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
            <div className="flex items-start justify-between gap-4">
              <div className="flex min-w-0 items-start gap-3">
                <BatteryCharging className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h4 className="text-[12.5px] font-semibold text-foreground">
                      Adrenaline Mode
                    </h4>
                    <StatusPill tone={status.tone} label={status.label} />
                    {saveTarget === "adrenaline" ? (
                      <LoaderCircle className="h-3 w-3 animate-spin text-muted-foreground" />
                    ) : null}
                  </div>
                  <p className="mt-1 text-[11.5px] leading-[1.5] text-muted-foreground">
                    Keep this device awake while Xero is running.
                  </p>
                  <p className="mt-1 text-[11px] leading-[1.5] text-muted-foreground/80">
                    {status.body}
                  </p>
                </div>
              </div>
              <Switch
                checked={settings.enabled}
                disabled={switchDisabled}
                onCheckedChange={updateAdrenalineMode}
                aria-label="Adrenaline Mode"
                className="mt-0.5"
              />
            </div>
          </section>

          <section className="rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
            <div className="flex items-start justify-between gap-4">
              <div className="flex min-w-0 items-start gap-3">
                <Laptop className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h4 className="text-[12.5px] font-semibold text-foreground">
                      Closed-Lid Mode
                    </h4>
                    <StatusPill tone={closedLidStatus.tone} label={closedLidStatus.label} />
                    {saveTarget === "closed_lid" ? (
                      <LoaderCircle className="h-3 w-3 animate-spin text-muted-foreground" />
                    ) : null}
                  </div>
                  <p className="mt-1 text-[11.5px] leading-[1.5] text-muted-foreground">
                    Keep the system awake after the lid closes.
                  </p>
                  <p className="mt-1 text-[11px] leading-[1.5] text-muted-foreground/80">
                    {closedLidStatus.body}
                  </p>
                </div>
              </div>
              <Switch
                checked={closedLidSettings.enabled}
                disabled={closedLidSwitchDisabled}
                onCheckedChange={requestClosedLidModeChange}
                aria-label="Closed-Lid Mode"
                className="mt-0.5"
              />
            </div>
          </section>

          <Alert className="rounded-md border-warning/25 bg-warning/[0.07] px-3 py-2 text-[12px]">
            <AlertTriangle className="h-3.5 w-3.5 text-warning" />
            <AlertTitle className="text-[12px]">
              Closed-Lid Mode changes system power settings
            </AlertTitle>
            <AlertDescription className="text-[12px]">
              The operating system may ask for administrator approval. The built-in display can
              still turn off when the lid closes, and heat or battery drain can increase.
            </AlertDescription>
          </Alert>

          <AlertDialog open={confirmClosedLidOpen} onOpenChange={setConfirmClosedLidOpen}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Enable Closed-Lid Mode?</AlertDialogTitle>
                <AlertDialogDescription>
                  Xero will ask the operating system for administrator approval if needed and set a
                  global power option so work can continue after this device lid closes. The setting
                  remains active until you turn it off here.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={saveTarget === "closed_lid"}>
                  Cancel
                </AlertDialogCancel>
                <AlertDialogAction
                  disabled={saveTarget === "closed_lid"}
                  onClick={() => updateClosedLidMode(true, true)}
                >
                  Enable
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </>
      )}
    </div>
  )
}

function StatusPill({ tone, label }: { tone: StatusTone; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase ring-1 ring-inset",
        TONE_BG[tone],
        TONE_RING[tone],
        TONE_TEXT[tone],
      )}
    >
      <span className={cn("size-1.5 rounded-full", TONE_DOT[tone])} aria-hidden />
      {label}
    </span>
  )
}

function statusSummary(
  settings: AdrenalineModeSettingsDto,
  saving: boolean,
): { tone: StatusTone; label: string; body: string } {
  if (!settings.platformSupported || settings.activeStatus === "unsupported") {
    return {
      tone: "muted",
      label: "Not supported",
      body: "Adrenaline Mode uses platform power assertions and is disabled on this platform.",
    }
  }

  if (saving) {
    return {
      tone: "warn",
      label: "Saving",
      body: "Xero is updating the power assertion.",
    }
  }

  if (settings.active) {
    return {
      tone: "ok",
      label: "Active",
      body: "Xero is holding a process-scoped idle sleep assertion.",
    }
  }

  if (settings.enabled) {
    return {
      tone: "warn",
      label: "Inactive",
      body: "The preference is enabled, but Xero is not currently holding the assertion.",
    }
  }

  return {
    tone: "muted",
    label: "Inactive",
    body: "Enable Adrenaline Mode to prevent idle sleep while Xero is open.",
  }
}

function closedLidStatusSummary(
  settings: ClosedLidModeSettingsDto,
  saving: boolean,
): { tone: StatusTone; label: string; body: string } {
  if (!settings.platformSupported || settings.activeStatus === "unsupported") {
    return {
      tone: "muted",
      label: "Not supported",
      body: "Closed-Lid Mode uses platform power settings and is disabled on this platform.",
    }
  }

  if (saving) {
    return {
      tone: "warn",
      label: "Saving",
      body:
        "The operating system may ask for administrator approval before changing the global power setting.",
    }
  }

  if (settings.activeStatus === "needs_attention") {
    return {
      tone: "warn",
      label: "Review",
      body: settings.enabled
        ? "The preference is enabled, but the system is not currently reporting Closed-Lid Mode as active."
        : "The system is reporting Closed-Lid Mode as active outside of Xero's enabled preference.",
    }
  }

  if (settings.enabled && settings.active) {
    return {
      tone: "ok",
      label: "Active",
      body:
        "Xero has enabled the global closed-lid power setting; turn this off here to restore the prior value.",
    }
  }

  return {
    tone: "muted",
    label: "Inactive",
    body: "Enable only when you need work to continue while the laptop lid is closed.",
  }
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }

  if (typeof error === "object" && error && "message" in error) {
    const message = String((error as { message?: unknown }).message ?? "").trim()
    if (message.length > 0) return message
  }

  const message = String(error ?? "").trim()
  return message.length > 0 ? message : fallback
}
