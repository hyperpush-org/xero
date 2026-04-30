import { useCallback, useEffect, useMemo, useState } from "react"
import type React from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  AlertTriangle,
  CheckCircle2,
  Cpu,
  Globe2,
  Languages,
  LoaderCircle,
  Mic,
  PackageOpen,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Volume2,
} from "lucide-react"

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
  ok: "bg-emerald-500/10",
  warn: "bg-amber-500/10",
  bad: "bg-destructive/10",
  muted: "bg-muted/40",
}

const TONE_RING: Record<StatusTone, string> = {
  ok: "ring-emerald-500/20",
  warn: "ring-amber-500/25",
  bad: "ring-destructive/25",
  muted: "ring-border/60",
}

const TONE_TEXT: Record<StatusTone, string> = {
  ok: "text-emerald-600 dark:text-emerald-400",
  warn: "text-amber-600 dark:text-amber-400",
  bad: "text-destructive",
  muted: "text-muted-foreground",
}

const TONE_DOT: Record<StatusTone, string> = {
  ok: "bg-emerald-500 dark:bg-emerald-400",
  warn: "bg-amber-500 dark:bg-amber-400",
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

          <ReadinessCard status={status!} settings={selectedSettings} saving={saveState === "saving"} />

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
  settings,
  saving,
}: {
  status: DictationStatusDto
  settings: DictationSettingsDto
  saving: boolean
}) {
  const summary = summarizeReadiness(status)
  const activeEngineLabel =
    status.modern.available && (settings.enginePreference === "automatic" || settings.enginePreference === "modern")
      ? "Modern engine"
      : status.legacy.available
        ? "Legacy engine"
        : status.modern.available
          ? "Modern engine"
          : "No engine available"
  const localeLabel = settings.locale ?? status.defaultLocale ?? "System default"
  const updatedLabel = useMemo(() => formatTimestamp(settings.updatedAt), [settings.updatedAt])

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <div
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
            TONE_BG[summary.tone],
            TONE_RING[summary.tone],
          )}
          aria-hidden
        >
          <Mic className={cn("h-5 w-5", TONE_TEXT[summary.tone])} />
        </div>

        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">
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
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">{summary.body}</p>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={Cpu} label="Active" value={activeEngineLabel} />
        <MetaItem icon={Globe2} label="Locale" value={localeLabel} mono />
        {status.osVersion ? <MetaItem icon={Sparkles} label="macOS" value={status.osVersion} /> : null}
        {updatedLabel ? <MetaItem icon={CheckCircle2} label="Updated" value={updatedLabel} /> : null}
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
    <section className="flex flex-col gap-3">
      <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
        Preferences
      </h4>
      <div className="grid gap-3 lg:grid-cols-3">
        <PreferenceCard
          icon={Cpu}
          label="Engine preference"
          value={settings.enginePreference}
          disabled={disabled}
          options={ENGINE_OPTIONS}
          onValueChange={(enginePreference) => onUpdate({ enginePreference })}
        />
        <PreferenceCard
          icon={ShieldCheck}
          label="Privacy mode"
          value={settings.privacyMode}
          disabled={disabled}
          options={PRIVACY_OPTIONS}
          onValueChange={(privacyMode) => onUpdate({ privacyMode })}
        />
        <LocaleCard
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

function PreferenceCard<T extends string>({
  icon: Icon,
  label,
  value,
  disabled,
  options,
  onValueChange,
}: {
  icon: React.ElementType
  label: string
  value: T
  disabled: boolean
  options: Array<{ value: T; label: string; detail: string }>
  onValueChange: (value: T) => void
}) {
  const selected = options.find((option) => option.value === value)

  return (
    <div className="flex flex-col gap-2.5 rounded-lg border border-border/60 bg-card/30 px-3.5 py-3.5">
      <div className="flex items-center gap-2">
        <span
          className="flex size-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground"
          aria-hidden
        >
          <Icon className="h-3 w-3" />
        </span>
        <label className="text-[12px] font-medium text-foreground">{label}</label>
      </div>
      <Select value={value} disabled={disabled} onValueChange={(nextValue) => onValueChange(nextValue as T)}>
        <SelectTrigger aria-label={label} className="h-8 w-full text-[12.5px]" size="sm">
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
      <p className="text-[11.5px] leading-[1.45] text-muted-foreground">{selected?.detail}</p>
    </div>
  )
}

function LocaleCard({
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
    <div className="flex flex-col gap-2.5 rounded-lg border border-border/60 bg-card/30 px-3.5 py-3.5">
      <div className="flex items-center gap-2">
        <span
          className="flex size-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground"
          aria-hidden
        >
          <Languages className="h-3 w-3" />
        </span>
        <label className="text-[12px] font-medium text-foreground" htmlFor="dictation-locale">
          Locale
        </label>
      </div>
      <Select value={selectedLocale} disabled={disabled} onValueChange={onValueChange}>
        <SelectTrigger
          id="dictation-locale"
          aria-label="Dictation locale"
          className="h-8 w-full text-[12.5px]"
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
      <p
        className={cn(
          "text-[11.5px] leading-[1.45]",
          selectedLocaleUnsupported ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground",
        )}
      >
        {selectedLocaleUnsupported
          ? "The selected locale is not in the current backend-supported list."
          : "Use System default unless a project needs a specific recognition locale."}
      </p>
    </div>
  )
}

function CapabilitiesPanel({ status }: { status: DictationStatusDto }) {
  const modernAssetsTone: StatusTone =
    status.modernAssets.status === "installed" ? "ok" : status.modern.available ? "warn" : "muted"

  return (
    <section className="flex flex-col gap-3">
      <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
        System availability
      </h4>
      <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <EngineRow
          icon={Sparkles}
          label="Modern engine"
          available={status.modern.available}
          reason={status.modern.reason}
          hint="macOS 26 SpeechAnalyzer path"
        />
        <EngineRow
          icon={Cpu}
          label="Legacy engine"
          available={status.legacy.available}
          reason={status.legacy.reason}
          hint="SFSpeechRecognizer fallback"
        />
        <PermissionRow kind="microphone" state={status.microphonePermission} />
        <PermissionRow kind="speech recognition" state={status.speechPermission} />
        <CapabilityRow
          icon={PackageOpen}
          label="Modern speech assets"
          tone={modernAssetsTone}
          value={modernAssetLabel(status)}
          pillLabel={
            status.modernAssets.status === "installed"
              ? "Installed"
              : status.modern.available
                ? "Check"
                : "—"
          }
        />
      </ul>
    </section>
  )
}

function EngineRow({
  icon,
  label,
  available,
  reason,
  hint,
}: {
  icon: React.ElementType
  label: string
  available: boolean
  reason?: string | null
  hint: string
}) {
  return (
    <CapabilityRow
      icon={icon}
      label={label}
      tone={available ? "ok" : "warn"}
      value={available ? hint : reason ? humanizeReason(reason) : "Unavailable"}
      pillLabel={available ? "Ready" : "Unavailable"}
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
  const Icon = kind === "microphone" ? Mic : Volume2
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
      icon={Icon}
      label={`${capitalize(kind)} permission`}
      tone={tone}
      value={permissionLabel(state)}
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
  icon: Icon,
  label,
  tone,
  value,
  pillLabel,
  action,
}: {
  icon: React.ElementType
  label: string
  tone: StatusTone
  value: string
  pillLabel: string
  action?: React.ReactNode
}) {
  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <div
        className={cn(
          "mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60",
          TONE_TEXT[tone],
        )}
        aria-hidden
      >
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{label}</p>
        <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">{value}</p>
      </div>
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

function MetaItem({
  icon: Icon,
  label,
  value,
  mono = false,
}: {
  icon: React.ElementType
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className={cn("text-foreground/80", mono && "font-mono text-[11.5px]")}>{value}</span>
    </span>
  )
}

function UnavailableCard({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border/70 bg-secondary/15 px-6 py-10 text-center">
      <div className="flex size-11 items-center justify-center rounded-full border border-border/60 bg-background/60 text-muted-foreground">
        <Mic className="h-5 w-5" />
      </div>
      <div className="flex max-w-sm flex-col gap-1.5">
        <p className="text-[14px] font-semibold text-foreground">{title}</p>
        <p className="text-[12.5px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}

function LoadingCard() {
  return (
    <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border/70 bg-secondary/15 px-6 py-10 text-center">
      <div className="flex size-11 items-center justify-center rounded-full border border-border/60 bg-background/60 text-muted-foreground">
        <LoaderCircle className="h-5 w-5 animate-spin" />
      </div>
      <p className="text-[12.5px] font-medium text-foreground">Loading dictation settings</p>
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

function permissionLabel(state: DictationPermissionStateDto): string {
  switch (state) {
    case "authorized":
      return "Allowed"
    case "denied":
    case "restricted":
      return "Open System Settings > Privacy & Security and allow Xero."
    case "not_determined":
      return "macOS will ask the first time dictation starts."
    case "unsupported":
      return "Unsupported on this system."
    case "unknown":
      return "Current permission state is unknown."
  }
}

function modernAssetLabel(status: DictationStatusDto): string {
  if (!status.modern.available) return "Modern engine unavailable"
  switch (status.modernAssets.status) {
    case "installed":
      return status.modernAssets.locale ? `Installed for ${status.modernAssets.locale}` : "Installed"
    case "not_installed":
      return status.modernAssets.locale ? `Not installed for ${status.modernAssets.locale}` : "Not installed"
    case "unsupported_locale":
      return "Unsupported locale"
    case "unavailable":
    case "unknown":
      return "Asset status unknown"
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

function formatTimestamp(iso: string | null | undefined): string | null {
  if (!iso) return null
  const parsed = new Date(iso)
  if (Number.isNaN(parsed.getTime())) return null
  return parsed.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  })
}

async function openMacosPrivacyPane(pane: string): Promise<void> {
  await openUrl(`x-apple.systempreferences:com.apple.preference.security?${pane}`)
}
