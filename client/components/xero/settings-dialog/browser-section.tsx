import {
  AlertTriangle,
  Check,
  Cookie,
  Globe2,
  Loader2,
  MonitorCog,
  MonitorUp,
  RefreshCw,
  SlidersHorizontal,
} from "lucide-react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { useCallback, useEffect, useRef, useState, type ElementType } from "react"
import {
  useCookieImport,
  type CookieImportStatus,
  type DetectedBrowser,
} from "@/components/xero/browser-cookie-import"
import { Button } from "@/components/ui/button"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { cn } from "@/lib/utils"
import {
  browserControlSettingsSchema,
  upsertBrowserControlSettingsRequestSchema,
  type BrowserControlPreferenceDto,
  type BrowserControlSettingsDto,
} from "@/src/lib/xero-model/browser"
import { SectionHeader } from "./section-header"

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

export function BrowserSection() {
  const { browsers, status, refresh, importFrom } = useCookieImport({
    autoLoad: true,
  })
  const {
    settings: browserControlSettings,
    loadState: browserControlLoadState,
    saveState: browserControlSaveState,
    error: browserControlError,
    updatePreference: updateBrowserControlPreference,
  } = useBrowserControlSettings()

  useEffect(() => {
    if (status.kind !== "success") return
    const t = setTimeout(() => {
      void refresh()
    }, 0)
    return () => clearTimeout(t)
  }, [status, refresh])

  const available = browsers.filter((b) => b.available)
  const unavailable = browsers.filter((b) => !b.available)
  const running = status.kind === "running"
  const summary = summarize(available.length, status)

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Browser"
        description="Copy cookies from other installed browsers into Xero's in-app browser so you stay signed in while developing."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            disabled={running}
            onClick={() => void refresh()}
            aria-label="Rescan installed browsers"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", running && "animate-spin")} />
            Rescan
          </Button>
        }
      />

      <ReadinessCard summary={summary} status={status} />

      <BrowserControlPreferenceCard
        settings={browserControlSettings}
        loadState={browserControlLoadState}
        saveState={browserControlSaveState}
        error={browserControlError}
        onChange={updateBrowserControlPreference}
      />

      <section className="flex flex-col gap-2.5">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Import from
          {available.length > 0 ? (
            <span className="ml-1.5 font-normal text-muted-foreground">{available.length}</span>
          ) : null}
        </h4>

        {available.length === 0 ? (
          <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-3.5 py-3 text-[11.5px] text-muted-foreground">
            No supported browsers detected. Install Chrome, Safari, Firefox, Edge, Brave, or Arc, then rescan.
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {available.map((browser) => (
              <BrowserCard
                key={browser.id}
                browser={browser}
                running={running && status.kind === "running" && status.source === browser.id}
                disabled={running}
                onClick={() => void importFrom(browser)}
                lastResult={
                  status.kind === "success" && status.source === browser.id
                    ? status.result
                    : null
                }
              />
            ))}
          </div>
        )}

        {status.kind === "error" ? (
          <div
            role="alert"
            className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive"
          >
            <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
            <span>{status.message}</span>
          </div>
        ) : null}

        {unavailable.length > 0 ? (
          <p className="text-[11px] text-muted-foreground/80">
            Not detected: {unavailable.map((b) => b.label).join(", ")}.
          </p>
        ) : null}
      </section>
    </div>
  )
}

type BrowserControlLoadState = "idle" | "loading" | "ready" | "error"
type BrowserControlSaveState = "idle" | "saving"

const DEFAULT_BROWSER_CONTROL_SETTINGS: BrowserControlSettingsDto = {
  preference: "default",
  updatedAt: null,
}

const BROWSER_CONTROL_OPTIONS: Array<{
  value: BrowserControlPreferenceDto
  label: string
  body: string
  icon: ElementType
}> = [
  {
    value: "default",
    label: "Default",
    body: "Try the in-app browser first, then fall back to the device browser.",
    icon: SlidersHorizontal,
  },
  {
    value: "in_app_browser",
    label: "In-app browser",
    body: "Keep agent browser work inside Xero's tabbed browser.",
    icon: MonitorUp,
  },
  {
    value: "native_browser",
    label: "Native browser",
    body: "Prefer the user's device browser and desktop automation.",
    icon: MonitorCog,
  },
]

function useBrowserControlSettings() {
  const [settings, setSettings] = useState<BrowserControlSettingsDto>(DEFAULT_BROWSER_CONTROL_SETTINGS)
  const [loadState, setLoadState] = useState<BrowserControlLoadState>("idle")
  const [saveState, setSaveState] = useState<BrowserControlSaveState>("idle")
  const [error, setError] = useState<string | null>(null)
  const loadedRef = useRef(false)

  const load = useCallback(async () => {
    if (!isTauri()) {
      setSettings(DEFAULT_BROWSER_CONTROL_SETTINGS)
      setLoadState("ready")
      return DEFAULT_BROWSER_CONTROL_SETTINGS
    }

    setLoadState("loading")
    setError(null)
    try {
      const response = await invoke<unknown>("browser_control_settings")
      const parsed = browserControlSettingsSchema.parse(response)
      setSettings(parsed)
      setLoadState("ready")
      return parsed
    } catch (loadError) {
      setLoadState("error")
      setError(getErrorMessage(loadError, "Xero could not load browser control settings."))
      setSettings(DEFAULT_BROWSER_CONTROL_SETTINGS)
      return DEFAULT_BROWSER_CONTROL_SETTINGS
    }
  }, [])

  useEffect(() => {
    if (loadedRef.current) return
    loadedRef.current = true
    void load()
  }, [load])

  const updatePreference = useCallback(
    async (preference: BrowserControlPreferenceDto) => {
      const previous = settings
      const request = upsertBrowserControlSettingsRequestSchema.parse({ preference })
      setSettings((current) => ({ ...current, preference }))
      setSaveState("saving")
      setError(null)

      if (!isTauri()) {
        const localSettings: BrowserControlSettingsDto = { preference, updatedAt: null }
        setSettings(localSettings)
        setSaveState("idle")
        return localSettings
      }

      try {
        const response = await invoke<unknown>("browser_control_update_settings", { request })
        const parsed = browserControlSettingsSchema.parse(response)
        setSettings(parsed)
        return parsed
      } catch (saveError) {
        setSettings(previous)
        setError(getErrorMessage(saveError, "Xero could not save browser control settings."))
        return previous
      } finally {
        setSaveState("idle")
      }
    },
    [settings],
  )

  return {
    settings,
    loadState,
    saveState,
    error,
    updatePreference,
  }
}

function BrowserControlPreferenceCard({
  settings,
  loadState,
  saveState,
  error,
  onChange,
}: {
  settings: BrowserControlSettingsDto
  loadState: BrowserControlLoadState
  saveState: BrowserControlSaveState
  error: string | null
  onChange: (preference: BrowserControlPreferenceDto) => Promise<BrowserControlSettingsDto>
}) {
  const busy = loadState === "loading" || saveState === "saving"
  const selectedOption = BROWSER_CONTROL_OPTIONS.find((option) => option.value === settings.preference)

  void selectedOption

  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Agent browser control</h4>
        {busy ? (
          <span className="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" />
            {loadState === "loading" ? "Loading" : "Saving"}
          </span>
        ) : null}
      </div>

      <RadioGroup
        value={settings.preference}
        onValueChange={(value) => void onChange(value as BrowserControlPreferenceDto)}
        className="grid grid-cols-1 gap-2 sm:grid-cols-3"
        aria-label="Agent browser control preference"
        disabled={busy}
      >
        {BROWSER_CONTROL_OPTIONS.map((option) => (
          <BrowserControlPreferenceOption
            key={option.value}
            option={option}
            checked={settings.preference === option.value}
            disabled={busy}
          />
        ))}
      </RadioGroup>

      {error ? (
        <div
          role="alert"
          className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive"
        >
          <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      ) : null}
    </section>
  )
}

function BrowserControlPreferenceOption({
  option,
  checked,
  disabled,
}: {
  option: (typeof BROWSER_CONTROL_OPTIONS)[number]
  checked: boolean
  disabled: boolean
}) {
  const Icon = option.icon

  return (
    <label
      className={cn(
        "group flex cursor-pointer items-center gap-2.5 rounded-md border border-border/60 px-3 py-2.5 text-left transition-colors",
        "hover:border-primary/35 hover:bg-secondary/30",
        checked && "border-primary/45 bg-primary/5",
        disabled && "cursor-not-allowed opacity-65 hover:border-border/60 hover:bg-transparent",
      )}
    >
      <Icon
        className={cn(
          "h-3.5 w-3.5 shrink-0",
          checked ? "text-primary" : "text-muted-foreground",
        )}
        aria-hidden
      />
      <span className="flex-1 text-[12.5px] font-medium text-foreground">{option.label}</span>
      <RadioGroupItem value={option.value} aria-label={option.label} disabled={disabled} />
    </label>
  )
}

function ReadinessCard({
  summary,
  status,
}: {
  summary: { tone: StatusTone; label: string; body: string }
  status: CookieImportStatus
}) {
  return (
    <div className="flex items-start gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
      <Cookie className={cn("mt-0.5 h-4 w-4 shrink-0", TONE_TEXT[summary.tone])} aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="truncate text-[12.5px] font-semibold text-foreground">
            In-app browser cookies
          </p>
          <StatusPill tone={summary.tone} label={summary.label} />
          {status.kind === "running" ? (
            <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
              <Loader2 className="h-3 w-3 animate-spin" />
              Importing…
            </span>
          ) : null}
        </div>
        <p className="mt-0.5 text-[11.5px] leading-[1.5] text-muted-foreground">{summary.body}</p>
      </div>
    </div>
  )
}

interface BrowserCardProps {
  browser: DetectedBrowser
  running: boolean
  disabled: boolean
  onClick: () => void
  lastResult: { imported: number; domains: number; skipped: number } | null
}

function BrowserCard({ browser, running, disabled, onClick, lastResult }: BrowserCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={`Import cookies from ${browser.label}`}
      className={cn(
        "group flex items-center gap-2.5 rounded-md border border-border/60 px-3 py-2.5 text-left transition-colors",
        "hover:border-primary/35 hover:bg-secondary/30",
        "disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-border/60 disabled:hover:bg-transparent",
      )}
    >
      {running ? (
        <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-primary" />
      ) : (
        <Globe2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground group-hover:text-primary" />
      )}
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">{browser.label}</p>
        {running || lastResult ? (
          <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
            {running
              ? "Importing cookies…"
              : lastResult
                ? `${lastResult.imported} cookies · ${lastResult.domains} domains`
                : ""}
          </p>
        ) : null}
      </div>
      {lastResult && !running ? (
        <Check className="h-3.5 w-3.5 shrink-0 text-success dark:text-success" aria-hidden />
      ) : null}
    </button>
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

function summarize(
  availableCount: number,
  status: CookieImportStatus,
): { tone: StatusTone; label: string; body: string } {
  if (status.kind === "running") {
    return {
      tone: "warn",
      label: "Importing",
      body: "Reading cookies from your selected browser. macOS may prompt once for Keychain access — approve it to continue.",
    }
  }
  if (status.kind === "success") {
    const skipped = status.result.skipped
    return {
      tone: "ok",
      label: "Imported",
      body: `Imported ${status.result.imported} cookies across ${status.result.domains} domains${
        skipped > 0 ? ` (${skipped} skipped)` : ""
      }. Reload the in-app browser to apply.`,
    }
  }
  if (status.kind === "error") {
    return {
      tone: "bad",
      label: "Failed",
      body: "The last import didn't complete. Check the message below and try again — your existing cookies are unchanged.",
    }
  }
  if (availableCount === 0) {
    return {
      tone: "muted",
      label: "No sources",
      body: "Xero didn't detect any installed browsers. Install a supported browser and rescan to pull existing sessions in.",
    }
  }
  return {
    tone: "ok",
    label: "Ready",
    body: `${availableCount} ${
      availableCount === 1 ? "browser is" : "browsers are"
    } ready to import from. Pick a source below — Xero reads cookies locally and never uploads them.`,
  }
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error && "message" in error) {
    const message = String((error as { message?: unknown }).message ?? "").trim()
    if (message.length > 0) return message
  }
  const message = String(error ?? "").trim()
  return message.length > 0 ? message : fallback
}
