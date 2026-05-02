import { useCallback, useEffect, useMemo, useState } from "react"
import type React from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import { AlertTriangle, LoaderCircle, Mic, RotateCcw } from "lucide-react"

import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  DictationEnginePreferenceDto,
  DictationPermissionStateDto,
  DictationPrivacyModeDto,
  DictationSettingsDto,
  DictationStatusDto,
  UpsertDictationSettingsRequestDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export type DictationSettingsAdapter = Pick<
  XeroDesktopAdapter,
  | "isDesktopRuntime"
  | "speechDictationStatus"
  | "speechDictationSettings"
  | "speechDictationUpdateSettings"
>

interface DictationSectionProps {
  adapter?: DictationSettingsAdapter
}

const SYSTEM_LOCALE_VALUE = "__system__"

const DEFAULT_SETTINGS: DictationSettingsDto = {
  enginePreference: "automatic",
  privacyMode: "on_device_preferred",
  locale: null,
  updatedAt: null,
}

const ENGINE_OPTIONS: Array<{ value: DictationEnginePreferenceDto; label: string; detail: string }> = [
  { value: "automatic", label: "Automatic", detail: "Use modern dictation when available" },
  { value: "modern", label: "Prefer macOS 26 Dictation", detail: "Require the modern SpeechAnalyzer path first" },
  { value: "legacy", label: "Legacy only", detail: "Use SFSpeechRecognizer" },
]

const PRIVACY_OPTIONS: Array<{ value: DictationPrivacyModeDto; label: string; detail: string }> = [
  { value: "on_device_preferred", label: "On-device preferred", detail: "Try local recognition before asking for another path" },
  { value: "on_device_required", label: "On-device required", detail: "Never use Apple server recognition" },
  { value: "allow_network", label: "Allow Apple server recognition", detail: "Permit Apple recognition when local support is unavailable" },
]

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

export function DictationSection({ adapter }: DictationSectionProps) {
  const [status, setStatus] = useState<DictationStatusDto | null>(null)
  const [settings, setSettings] = useState<DictationSettingsDto | null>(null)
  const [loadState, setLoadState] = useState<"idle" | "loading" | "ready" | "error">("idle")
  const [saveState, setSaveState] = useState<"idle" | "saving">("idle")
  const [error, setError] = useState<string | null>(null)

  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() &&
      adapter.speechDictationStatus &&
      adapter.speechDictationSettings &&
      adapter.speechDictationUpdateSettings,
  )

  const load = useCallback(() => {
    if (!canUseAdapter || !adapter?.speechDictationStatus || !adapter.speechDictationSettings) {
      setLoadState("ready")
      setStatus(null)
      setSettings(DEFAULT_SETTINGS)
      return
    }

    setLoadState("loading")
    setError(null)
    Promise.all([adapter.speechDictationStatus(), adapter.speechDictationSettings()])
      .then(([nextStatus, nextSettings]) => {
        setStatus(nextStatus)
        setSettings(nextSettings)
        setLoadState("ready")
      })
      .catch((loadError) => {
        setError(getErrorMessage(loadError, "Xero could not load dictation settings."))
        setLoadState("error")
      })
  }, [adapter, canUseAdapter])

  useEffect(() => {
    load()
  }, [load])

  const localeOptions = useMemo(() => {
    const values = new Set<string>()
    if (status?.defaultLocale) values.add(status.defaultLocale)
    for (const locale of status?.supportedLocales ?? []) values.add(locale)
    if (settings?.locale) values.add(settings.locale)
    return [...values].sort((left, right) => left.localeCompare(right))
  }, [settings?.locale, status?.defaultLocale, status?.supportedLocales])

  const selectedSettings = settings ?? DEFAULT_SETTINGS
  const selectedLocale = selectedSettings.locale ?? SYSTEM_LOCALE_VALUE
  const selectedLocaleUnsupported = Boolean(
    selectedSettings.locale &&
      localeOptions.length > 0 &&
      !localeOptions.some((locale) => normalizeLocale(locale) === normalizeLocale(selectedSettings.locale)),
  )
  const isMacos = status?.platform === "macos"
  const isBusy = loadState === "loading" || saveState === "saving"

  const updateSettings = (patch: Partial<UpsertDictationSettingsRequestDto>) => {
    if (!adapter?.speechDictationUpdateSettings || !settings) return

    const request: UpsertDictationSettingsRequestDto = {
      enginePreference: patch.enginePreference ?? settings.enginePreference,
      privacyMode: patch.privacyMode ?? settings.privacyMode,
      locale: patch.locale === undefined ? settings.locale : patch.locale,
    }

    setSaveState("saving")
    setError(null)
    adapter
      .speechDictationUpdateSettings(request)
      .then((nextSettings) => {
        setSettings(nextSettings)
      })
      .catch((saveError) => {
        setError(getErrorMessage(saveError, "Xero could not save dictation settings."))
      })
      .finally(() => setSaveState("idle"))
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Dictation"
        description="Configure native macOS speech input for the agent composer."
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
              <RotateCcw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
        }
      />

      {!canUseAdapter ? (
        <UnavailableCard
          title="Desktop runtime required"
          body="Dictation settings are only available when Xero is running as a desktop app."
        />
      ) : loadState === "loading" && !status ? (
        <LoadingCard />
      ) : !isMacos ? (
        <UnavailableCard
          title="Native dictation is macOS-only"
          body={
            status?.platform === "unsupported"
              ? "Xero's native speech input pipeline is currently shipped for macOS only."
              : "Switch to a macOS device to configure native dictation."
          }
        />
      ) : (
        <>
          {error ? (
            <Alert variant="destructive" className="rounded-lg px-3.5 py-3 text-[12px]">
              <AlertTriangle className="h-3.5 w-3.5" />
              <AlertTitle className="text-[12.5px]">Dictation settings need attention</AlertTitle>
              <AlertDescription className="text-[12px]">{error}</AlertDescription>
            </Alert>
          ) : null}

          <ReadinessCard status={status!} saving={saveState === "saving"} />

          <PreferencesPanel
            settings={selectedSettings}
            disabled={isBusy}
            localeOptions={localeOptions}
            selectedLocale={selectedLocale}
            selectedLocaleUnsupported={selectedLocaleUnsupported}
            defaultLocale={status?.defaultLocale ?? null}
            onUpdate={updateSettings}
          />

          <CapabilitiesPanel status={status!} />
        </>
      )}
    </div>
  )
}

function ReadinessCard({
  status,
  saving,
}: {
  status: DictationStatusDto
  saving: boolean
}) {
  const summary = summarizeReadiness(status)

  return (
    <div className="flex items-start gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
      <Mic className={cn("mt-0.5 h-4 w-4 shrink-0", TONE_TEXT[summary.tone])} aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="truncate text-[12.5px] font-semibold text-foreground">
            Native macOS dictation
          </p>
          <StatusPill tone={summary.tone} label={summary.label} />
          {saving ? (
            <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
              <LoaderCircle className="h-3 w-3 animate-spin" />
              Saving…
            </span>
          ) : null}
        </div>
        <p className="mt-0.5 text-[11.5px] leading-[1.5] text-muted-foreground">{summary.body}</p>
      </div>
    </div>
  )
}

function PreferencesPanel({
  settings,
  disabled,
  localeOptions,
  selectedLocale,
  selectedLocaleUnsupported,
  defaultLocale,
  onUpdate,
}: {
  settings: DictationSettingsDto
  disabled: boolean
  localeOptions: string[]
  selectedLocale: string
  selectedLocaleUnsupported: boolean
  defaultLocale: string | null
  onUpdate: (patch: Partial<UpsertDictationSettingsRequestDto>) => void
}) {
  return (
    <section className="flex flex-col gap-2.5">
      <h4 className="text-[12.5px] font-semibold text-foreground">Preferences</h4>
      <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
        <PreferenceRow
          label="Engine"
          value={settings.enginePreference}
          disabled={disabled}
          options={ENGINE_OPTIONS}
          onValueChange={(enginePreference) => onUpdate({ enginePreference })}
        />
        <PreferenceRow
          label="Privacy"
          value={settings.privacyMode}
          disabled={disabled}
          options={PRIVACY_OPTIONS}
          onValueChange={(privacyMode) => onUpdate({ privacyMode })}
        />
        <LocaleRow
          disabled={disabled}
          localeOptions={localeOptions}
          selectedLocale={selectedLocale}
          selectedLocaleUnsupported={selectedLocaleUnsupported}
          defaultLocale={defaultLocale}
          onValueChange={(value) => onUpdate({ locale: value === SYSTEM_LOCALE_VALUE ? null : value })}
        />
      </div>
    </section>
  )
}

function PreferenceRow<T extends string>({
  label,
  value,
  disabled,
  options,
  onValueChange,
}: {
  label: string
  value: T
  disabled: boolean
  options: Array<{ value: T; label: string; detail: string }>
  onValueChange: (value: T) => void
}) {
  return (
    <div className="flex items-center justify-between gap-3 px-3.5 py-2.5">
      <label className="text-[12.5px] font-medium text-foreground">{label}</label>
      <Select value={value} disabled={disabled} onValueChange={(nextValue) => onValueChange(nextValue as T)}>
        <SelectTrigger
          aria-label={label}
          className="h-8 w-auto min-w-[180px] text-[12.5px]"
          size="sm"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function LocaleRow({
  disabled,
  localeOptions,
  selectedLocale,
  selectedLocaleUnsupported,
  defaultLocale,
  onValueChange,
}: {
  disabled: boolean
  localeOptions: string[]
  selectedLocale: string
  selectedLocaleUnsupported: boolean
  defaultLocale: string | null
  onValueChange: (value: string) => void
}) {
  return (
    <div className="flex items-center justify-between gap-3 px-3.5 py-2.5">
      <label className="text-[12.5px] font-medium text-foreground" htmlFor="dictation-locale">
        Locale
        {selectedLocaleUnsupported ? (
          <span className="ml-1.5 text-[11px] font-normal text-warning dark:text-warning">
            (unsupported)
          </span>
        ) : null}
      </label>
      <Select value={selectedLocale} disabled={disabled} onValueChange={onValueChange}>
        <SelectTrigger
          id="dictation-locale"
          aria-label="Dictation locale"
          className="h-8 w-auto min-w-[180px] text-[12.5px]"
          size="sm"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={SYSTEM_LOCALE_VALUE}>
            System default{defaultLocale ? ` (${defaultLocale})` : ""}
          </SelectItem>
          {localeOptions.map((locale) => (
            <SelectItem key={locale} value={locale}>
              {locale}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function CapabilitiesPanel({ status }: { status: DictationStatusDto }) {
  const modernAssetsTone: StatusTone =
    status.modernAssets.status === "installed" ? "ok" : status.modern.available ? "warn" : "muted"

  return (
    <section className="flex flex-col gap-2.5">
      <h4 className="text-[12.5px] font-semibold text-foreground">System availability</h4>
      <ul className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
        <EngineRow
          label="Modern engine"
          available={status.modern.available}
          reason={status.modern.reason}
        />
        <EngineRow
          label="Legacy engine"
          available={status.legacy.available}
          reason={status.legacy.reason}
        />
        <PermissionRow kind="microphone" state={status.microphonePermission} />
        <PermissionRow kind="speech recognition" state={status.speechPermission} />
        <CapabilityRow
          label="Modern speech assets"
          tone={modernAssetsTone}
          pillLabel={
            status.modernAssets.status === "installed"
              ? "Installed"
              : status.modern.available
                ? "Missing"
                : "—"
          }
        />
      </ul>
    </section>
  )
}

function EngineRow({
  label,
  available,
  reason,
}: {
  label: string
  available: boolean
  reason?: string | null
}) {
  return (
    <CapabilityRow
      label={label}
      tone={available ? "ok" : "warn"}
      pillLabel={available ? "Ready" : reason ? humanizeReason(reason) : "Unavailable"}
    />
  )
}

function PermissionRow({
  kind,
  state,
}: {
  kind: "microphone" | "speech recognition"
  state: DictationPermissionStateDto
}) {
  const denied = state === "denied" || state === "restricted"
  const tone: StatusTone = state === "authorized" ? "ok" : denied ? "bad" : "warn"
  const pane = kind === "microphone" ? "Privacy_Microphone" : "Privacy_SpeechRecognition"
  const pillLabel =
    state === "authorized"
      ? "Allowed"
      : denied
        ? "Blocked"
        : state === "not_determined"
          ? "Will prompt"
          : "Check"

  return (
    <CapabilityRow
      label={`${capitalize(kind)} permission`}
      tone={tone}
      pillLabel={pillLabel}
      action={
        denied ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-[11.5px]"
            onClick={() => void openMacosPrivacyPane(pane)}
          >
            Open Settings
          </Button>
        ) : null
      }
    />
  )
}

function CapabilityRow({
  label,
  tone,
  pillLabel,
  action,
}: {
  label: string
  tone: StatusTone
  pillLabel: string
  action?: React.ReactNode
}) {
  return (
    <li className="flex items-center justify-between gap-3 px-3.5 py-2.5">
      <p className="text-[12.5px] text-foreground">{label}</p>
      <div className="flex shrink-0 items-center gap-1.5">
        {action}
        <StatusPill tone={tone} label={pillLabel} />
      </div>
    </li>
  )
}

function StatusPill({ tone, label }: { tone: StatusTone; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em] ring-1 ring-inset",
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

function UnavailableCard({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-md border border-dashed border-border/60 bg-secondary/10 px-5 py-8 text-center">
      <Mic className="h-4 w-4 text-muted-foreground" />
      <p className="text-[12.5px] font-medium text-foreground">{title}</p>
      <p className="max-w-sm text-[11.5px] leading-[1.5] text-muted-foreground">{body}</p>
    </div>
  )
}

function LoadingCard() {
  return (
    <div className="flex items-center justify-center gap-2 rounded-md border border-border/60 px-4 py-10 text-[12px] text-muted-foreground">
      <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
      Loading dictation settings
    </div>
  )
}

function summarizeReadiness(status: DictationStatusDto): {
  tone: StatusTone
  label: string
  body: string
} {
  const micBlocked = status.microphonePermission === "denied" || status.microphonePermission === "restricted"
  const speechBlocked = status.speechPermission === "denied" || status.speechPermission === "restricted"
  const anyBlocked = micBlocked || speechBlocked
  const noEngine = !status.modern.available && !status.legacy.available

  if (noEngine) {
    return {
      tone: "bad",
      label: "Unavailable",
      body: "No dictation engine is available on this Mac. Check the system requirements below.",
    }
  }

  if (anyBlocked) {
    return {
      tone: "bad",
      label: "Permissions blocked",
      body: "macOS is blocking microphone or speech recognition for Xero. Open System Settings to allow access.",
    }
  }

  const micPending = status.microphonePermission === "not_determined"
  const speechPending = status.speechPermission === "not_determined"
  const assetsMissing = status.modern.available && status.modernAssets.status !== "installed"
  const onlyLegacy = !status.modern.available && status.legacy.available

  if (micPending || speechPending) {
    return {
      tone: "warn",
      label: "Permissions pending",
      body: "macOS will request microphone and speech permission the first time dictation runs.",
    }
  }

  if (onlyLegacy) {
    return {
      tone: "warn",
      label: "Legacy only",
      body: "Only the legacy SFSpeechRecognizer engine is available. Modern dictation requires macOS 26.",
    }
  }

  if (assetsMissing) {
    return {
      tone: "warn",
      label: "Assets missing",
      body: "Modern speech assets are not installed for the current locale. The first session will download them.",
    }
  }

  return {
    tone: "ok",
    label: "Ready",
    body: "Dictation is ready to use. Pick an engine, privacy mode, and locale below.",
  }
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  if (typeof error === "string" && error.trim().length > 0) return error
  return fallback
}

function humanizeReason(reason: string): string {
  return reason
    .replace(/_/g, " ")
    .replace(/^\w/, (letter: string) => letter.toUpperCase())
}

function normalizeLocale(locale: string | null | undefined): string {
  return (locale ?? "").trim().replace(/-/g, "_").toLowerCase()
}

function capitalize(value: string): string {
  return value.replace(/^\w/, (letter: string) => letter.toUpperCase())
}

async function openMacosPrivacyPane(pane: string): Promise<void> {
  await openUrl(`x-apple.systempreferences:com.apple.preference.security?${pane}`)
}
