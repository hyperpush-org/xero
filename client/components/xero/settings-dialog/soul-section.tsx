import { useCallback, useEffect, useMemo, useState } from "react"
import { AlertTriangle, Check, Heart, LoaderCircle, RefreshCw } from "lucide-react"

import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  SoulIdDto,
  SoulSettingsDto,
  UpsertSoulSettingsRequestDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export type SoulSettingsAdapter = Pick<
  XeroDesktopAdapter,
  "isDesktopRuntime" | "soulSettings" | "soulUpdateSettings"
>

interface SoulSectionProps {
  adapter?: SoulSettingsAdapter
}

const FALLBACK_SOUL_SETTINGS: SoulSettingsDto = {
  selectedSoulId: "steward",
  selectedSoul: {
    id: "steward",
    name: "Steady steward",
    summary: "Calm, grounded, and quietly thorough.",
    prompt:
      "Be calm, grounded, and quietly thorough. Help the user feel oriented. Prefer evidence, plain language, scoped action, and measured next steps.",
  },
  presets: [
    {
      id: "steward",
      name: "Steady steward",
      summary: "Calm, grounded, and quietly thorough.",
      prompt:
        "Be calm, grounded, and quietly thorough. Help the user feel oriented. Prefer evidence, plain language, scoped action, and measured next steps.",
    },
    {
      id: "pair",
      name: "Warm pair",
      summary: "Collaborative, teaching-aware, and conversational.",
      prompt:
        "Act like a generous pair programmer. Think with the user, name tradeoffs, teach briefly when useful, and keep collaboration warm without slowing momentum.",
    },
    {
      id: "builder",
      name: "Sharp builder",
      summary: "Decisive, pragmatic, and momentum-oriented.",
      prompt:
        "Be decisive and momentum-oriented. Minimize ceremony, choose sensible defaults, make progress in small verified steps, and keep summaries crisp.",
    },
    {
      id: "sentinel",
      name: "Careful sentinel",
      summary: "Skeptical, risk-aware, and verification-minded.",
      prompt:
        "Be constructively skeptical. Look for hidden risks, edge cases, missing tests, security hazards, and data-loss hazards. Call out uncertainty before it becomes damage.",
    },
  ],
  updatedAt: null,
}

export function SoulSection({ adapter }: SoulSectionProps) {
  const [settings, setSettings] = useState<SoulSettingsDto>(FALLBACK_SOUL_SETTINGS)
  const [loadState, setLoadState] = useState<"idle" | "loading" | "ready" | "error">("idle")
  const [saveState, setSaveState] = useState<"idle" | "saving">("idle")
  const [error, setError] = useState<string | null>(null)

  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() && adapter.soulSettings && adapter.soulUpdateSettings,
  )

  const load = useCallback(() => {
    if (!canUseAdapter || !adapter?.soulSettings) {
      setSettings(FALLBACK_SOUL_SETTINGS)
      setLoadState("ready")
      return
    }

    setLoadState("loading")
    setError(null)
    adapter
      .soulSettings()
      .then((nextSettings) => {
        setSettings(nextSettings)
        setLoadState("ready")
      })
      .catch((loadError) => {
        setSettings(FALLBACK_SOUL_SETTINGS)
        setError(getErrorMessage(loadError, "Xero could not load Soul settings."))
        setLoadState("error")
      })
  }, [adapter, canUseAdapter])

  useEffect(() => {
    load()
  }, [load])

  const selectedSoul = useMemo(
    () =>
      settings.presets.find((preset) => preset.id === settings.selectedSoulId) ??
      settings.selectedSoul,
    [settings.presets, settings.selectedSoul, settings.selectedSoulId],
  )
  const isBusy = loadState === "loading" || saveState === "saving"

  const updateSoul = (selectedSoulId: SoulIdDto) => {
    if (!adapter?.soulUpdateSettings) return

    const previous = settings
    const nextSoul =
      settings.presets.find((preset) => preset.id === selectedSoulId) ??
      settings.selectedSoul
    const request: UpsertSoulSettingsRequestDto = { selectedSoulId }

    setSettings((current) => ({
      ...current,
      selectedSoulId,
      selectedSoul: nextSoul,
    }))
    setSaveState("saving")
    setError(null)

    adapter
      .soulUpdateSettings(request)
      .then((nextSettings) => {
        setSettings(nextSettings)
      })
      .catch((saveError) => {
        setSettings(previous)
        setError(getErrorMessage(saveError, "Xero could not save Soul settings."))
      })
      .finally(() => setSaveState("idle"))
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Soul"
        description="Choose the premade behavior profile added to the start of each owned-agent conversation."
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
          <Heart className="h-4 w-4 text-muted-foreground" />
          <p className="text-[12.5px] font-medium text-foreground">Desktop runtime required</p>
          <p className="max-w-sm text-[11.5px] leading-[1.5] text-muted-foreground">
            Soul settings are available when Xero is running as a desktop app.
          </p>
        </div>
      ) : (
        <>
          {error ? (
            <Alert variant="destructive" className="rounded-md px-3 py-2 text-[12px]">
              <AlertTriangle className="h-3.5 w-3.5" />
              <AlertTitle className="text-[12px]">Soul settings need attention</AlertTitle>
              <AlertDescription className="text-[12px]">{error}</AlertDescription>
            </Alert>
          ) : null}

          <div className="rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
            <div className="flex items-start gap-2.5">
              <Heart className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <p className="text-[12.5px] font-semibold text-foreground">{selectedSoul.name}</p>
                  {saveState === "saving" ? (
                    <LoaderCircle className="h-3 w-3 animate-spin text-muted-foreground" />
                  ) : null}
                </div>
                <p className="mt-1 text-[11.5px] leading-[1.5] text-muted-foreground">
                  {selectedSoul.prompt}
                </p>
              </div>
            </div>
          </div>

          <RadioGroup
            value={settings.selectedSoulId}
            onValueChange={(value) => updateSoul(value as SoulIdDto)}
            className="grid grid-cols-1 gap-2"
            disabled={isBusy}
          >
            {settings.presets.map((preset) => {
              const selected = preset.id === settings.selectedSoulId
              return (
                <label
                  key={preset.id}
                  htmlFor={`soul-${preset.id}`}
                  className={cn(
                    "flex cursor-pointer items-start gap-3 rounded-lg border px-4 py-3 transition-colors",
                    selected
                      ? "border-primary/45 bg-primary/5"
                      : "border-border/70 bg-background hover:bg-accent/30",
                    isBusy && "cursor-default opacity-70",
                  )}
                >
                  <RadioGroupItem
                    id={`soul-${preset.id}`}
                    value={preset.id}
                    aria-label={preset.name}
                    className="mt-0.5"
                  />
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-2">
                      <span className="text-[13px] font-medium text-foreground">{preset.name}</span>
                      {selected ? (
                        <Check className="h-3.5 w-3.5 text-primary" />
                      ) : null}
                    </span>
                    <span className="mt-1 block text-[12px] leading-[1.5] text-muted-foreground">
                      {preset.summary}
                    </span>
                  </span>
                </label>
              )
            })}
          </RadioGroup>
        </>
      )}
    </div>
  )
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }
  return fallback
}
