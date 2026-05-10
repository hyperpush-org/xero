import { useCallback, useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { AlertCircle, KeyRound, Loader2, RefreshCw, RotateCcw, Server } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { StepHeader } from "./providers-step"

interface LocalEnvironmentConfig {
  launchMode: string | null
  envFilePath: string | null
  phxHost: string
  port: string
  databaseUrl: string
  corsOrigins: string
  poolSize: string
  rateLimitPerMinute: string
  hasSecretKeyBase: boolean
}

const DEFAULTS: Pick<
  LocalEnvironmentConfig,
  "phxHost" | "port" | "databaseUrl" | "corsOrigins" | "poolSize" | "rateLimitPerMinute"
> = {
  phxHost: "127.0.0.1",
  port: "4000",
  databaseUrl: "ecto://postgres:postgres@localhost/xero_prod",
  corsOrigins: "http://localhost:3000,http://127.0.0.1:3000,tauri://localhost",
  poolSize: "10",
  rateLimitPerMinute: "60",
}

interface LocalEnvironmentStepProps {
  onSaved?: () => void
}

export function LocalEnvironmentStep({ onSaved }: LocalEnvironmentStepProps) {
  const [config, setConfig] = useState<LocalEnvironmentConfig | null>(null)
  const [draft, setDraft] = useState<typeof DEFAULTS | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved" | "error">("idle")
  const [saveError, setSaveError] = useState<string | null>(null)
  const [regenerateStatus, setRegenerateStatus] = useState<"idle" | "running" | "ok" | "error">("idle")
  const [showAdvanced, setShowAdvanced] = useState(false)

  useEffect(() => {
    let cancelled = false
    void invoke<LocalEnvironmentConfig>("get_local_environment_config")
      .then((value) => {
        if (cancelled) return
        setConfig(value)
        setDraft({
          phxHost: value.phxHost,
          port: value.port,
          databaseUrl: value.databaseUrl,
          corsOrigins: value.corsOrigins,
          poolSize: value.poolSize,
          rateLimitPerMinute: value.rateLimitPerMinute,
        })
      })
      .catch((err: { message?: string } | string) => {
        if (cancelled) return
        const message = typeof err === "string" ? err : err?.message ?? "Could not load local environment."
        setLoadError(message)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const updateField = useCallback(
    <K extends keyof typeof DEFAULTS>(key: K, value: string) => {
      setDraft((current) => (current ? { ...current, [key]: value } : current))
      setSaveStatus("idle")
      setSaveError(null)
    },
    [],
  )

  const resetField = useCallback(
    <K extends keyof typeof DEFAULTS>(key: K) => {
      updateField(key, DEFAULTS[key])
    },
    [updateField],
  )

  const handleSave = useCallback(async () => {
    if (!draft) return
    setSaveStatus("saving")
    setSaveError(null)
    try {
      const next = await invoke<LocalEnvironmentConfig>("save_local_environment_config", {
        request: draft,
      })
      setConfig(next)
      setSaveStatus("saved")
      onSaved?.()
    } catch (err) {
      setSaveStatus("error")
      const message = typeof err === "string" ? err : (err as { message?: string })?.message ?? "Could not save local environment."
      setSaveError(message)
    }
  }, [draft, onSaved])

  const handleRegenerate = useCallback(async () => {
    setRegenerateStatus("running")
    try {
      await invoke<void>("regenerate_secret_key_base")
      const next = await invoke<LocalEnvironmentConfig>("get_local_environment_config")
      setConfig(next)
      setRegenerateStatus("ok")
    } catch {
      setRegenerateStatus("error")
    }
  }, [])

  if (loadError) {
    return (
      <div>
        <StepHeader
          title="Local environment"
          description="Review the auto-generated server config for this local-source run."
        />
        <Alert variant="destructive" className="mt-7">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{loadError}</AlertDescription>
        </Alert>
      </div>
    )
  }

  if (!config || !draft) {
    return (
      <div>
        <StepHeader
          title="Local environment"
          description="Review the auto-generated server config for this local-source run."
        />
        <div className="mt-10 flex items-center justify-center text-muted-foreground">
          <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          <span className="text-[12px]">Loading current settings…</span>
        </div>
      </div>
    )
  }

  return (
    <div>
      <StepHeader
        title="Local environment"
        description="Auto-generated for running Xero from source. Defaults work out of the box — adjust only if you have a port conflict or want to point at a different database."
      />

      <div className="mt-7 flex flex-col gap-4 animate-in fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both]">
        <FieldRow
          label="Server port"
          hint="Phoenix listens here. Change if 4000 is taken."
          value={draft.port}
          defaultValue={DEFAULTS.port}
          onChange={(v) => updateField("port", v)}
          onReset={() => resetField("port")}
          placeholder="4000"
          inputMode="numeric"
        />

        <FieldRow
          label="Bind host"
          hint="Loopback by default; set to 0.0.0.0 to expose on the LAN."
          value={draft.phxHost}
          defaultValue={DEFAULTS.phxHost}
          onChange={(v) => updateField("phxHost", v)}
          onReset={() => resetField("phxHost")}
          placeholder="127.0.0.1"
        />

        <div className="rounded-lg border border-border bg-card/40 p-3.5">
          <div className="flex items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/50 text-muted-foreground">
              <KeyRound className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-[13px] font-medium">Phoenix secret key base</p>
                {config.hasSecretKeyBase ? (
                  <Badge
                    variant="secondary"
                    className="border border-success/30 bg-success/10 px-1.5 py-0 text-[10px] text-success dark:text-success"
                  >
                    Generated
                  </Badge>
                ) : (
                  <Badge variant="outline" className="px-1.5 py-0 text-[10px] text-warning dark:text-warning">
                    Missing
                  </Badge>
                )}
              </div>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                Used to sign cookies. Stored locally in <code className="font-mono text-[10.5px]">server/.env</code>.
              </p>
              {regenerateStatus === "error" ? (
                <p className="mt-2 text-[11px] text-destructive">Could not regenerate secret. Try again.</p>
              ) : null}
              {regenerateStatus === "ok" ? (
                <p className="mt-2 text-[11px] text-success">Secret regenerated. Restart `pnpm start` to pick it up.</p>
              ) : null}
            </div>
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1.5 text-[11px]"
              disabled={regenerateStatus === "running"}
              onClick={() => void handleRegenerate()}
            >
              {regenerateStatus === "running" ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <RefreshCw className="h-3 w-3" />
              )}
              Regenerate
            </Button>
          </div>
        </div>

        <button
          type="button"
          onClick={() => setShowAdvanced((v) => !v)}
          className="self-start text-[11px] font-medium text-muted-foreground hover:text-foreground transition-colors"
        >
          {showAdvanced ? "Hide" : "Show"} advanced settings
        </button>

        {showAdvanced ? (
          <div className="flex flex-col gap-4 rounded-lg border border-dashed border-border bg-card/20 p-3.5 animate-in fade-in-0 slide-in-from-bottom-1 motion-fast">
            <FieldRow
              label="Database URL"
              hint="Defaults to the local Docker Postgres provisioned by `pnpm start`."
              value={draft.databaseUrl}
              defaultValue={DEFAULTS.databaseUrl}
              onChange={(v) => updateField("databaseUrl", v)}
              onReset={() => resetField("databaseUrl")}
              placeholder={DEFAULTS.databaseUrl}
              monospace
            />
            <FieldRow
              label="CORS origins"
              hint="Comma-separated. Tauri origin must stay in the list."
              value={draft.corsOrigins}
              defaultValue={DEFAULTS.corsOrigins}
              onChange={(v) => updateField("corsOrigins", v)}
              onReset={() => resetField("corsOrigins")}
              placeholder={DEFAULTS.corsOrigins}
              monospace
            />
            <div className="grid grid-cols-2 gap-3">
              <FieldRow
                label="Pool size"
                hint="DB connections."
                value={draft.poolSize}
                defaultValue={DEFAULTS.poolSize}
                onChange={(v) => updateField("poolSize", v)}
                onReset={() => resetField("poolSize")}
                placeholder={DEFAULTS.poolSize}
                inputMode="numeric"
              />
              <FieldRow
                label="Rate limit / min"
                hint="Per-IP request cap."
                value={draft.rateLimitPerMinute}
                defaultValue={DEFAULTS.rateLimitPerMinute}
                onChange={(v) => updateField("rateLimitPerMinute", v)}
                onReset={() => resetField("rateLimitPerMinute")}
                placeholder={DEFAULTS.rateLimitPerMinute}
                inputMode="numeric"
              />
            </div>
          </div>
        ) : null}

        {config.envFilePath ? (
          <div className="flex items-start gap-2 rounded-lg border border-border/60 bg-card/30 px-3 py-2.5">
            <Server className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <div className="min-w-0">
              <p className="text-[11px] font-medium text-foreground">Config file</p>
              <p className="mt-0.5 truncate font-mono text-[10.5px] text-muted-foreground">{config.envFilePath}</p>
            </div>
          </div>
        ) : null}

        {saveError ? (
          <Alert variant="destructive" className="py-2.5">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription className="text-[12px]">{saveError}</AlertDescription>
          </Alert>
        ) : null}

        <div className="flex items-center justify-between gap-2">
          <p className="text-[10.5px] text-muted-foreground/80">
            Changes apply on next <code className="font-mono">pnpm start</code>.
          </p>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-[11px]"
            disabled={saveStatus === "saving"}
            onClick={() => void handleSave()}
          >
            {saveStatus === "saving" ? (
              <>
                <Loader2 className="mr-1.5 h-3 w-3 animate-spin" />
                Saving
              </>
            ) : saveStatus === "saved" ? (
              "Saved"
            ) : (
              "Save changes"
            )}
          </Button>
        </div>
      </div>
    </div>
  )
}

interface FieldRowProps {
  label: string
  hint: string
  value: string
  defaultValue: string
  onChange: (value: string) => void
  onReset: () => void
  placeholder?: string
  inputMode?: "text" | "numeric"
  monospace?: boolean
}

function FieldRow({
  label,
  hint,
  value,
  defaultValue,
  onChange,
  onReset,
  placeholder,
  inputMode,
  monospace,
}: FieldRowProps) {
  const dirty = value !== defaultValue
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-[11.5px] font-medium text-foreground">{label}</Label>
        {dirty ? (
          <button
            type="button"
            onClick={onReset}
            className="flex items-center gap-1 text-[10.5px] text-muted-foreground hover:text-foreground transition-colors"
          >
            <RotateCcw className="h-3 w-3" />
            Reset
          </button>
        ) : null}
      </div>
      <Input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        inputMode={inputMode}
        className={`h-8 text-[12px] ${monospace ? "font-mono" : ""}`}
      />
      <p className="text-[10.5px] leading-tight text-muted-foreground">{hint}</p>
    </div>
  )
}
