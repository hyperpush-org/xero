import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react"
import { AlertTriangle, Bot, Check, LoaderCircle, Plus, RefreshCw, Trash2 } from "lucide-react"

import type { XeroDesktopAdapter } from "@/src/lib/xero-desktop"
import type {
  AgentToolApplicationStyleDto,
  AgentToolingModelOverrideDto,
  AgentToolingSettingsDto,
  RuntimeProviderIdDto,
  UpsertAgentToolingSettingsRequestDto,
} from "@/src/lib/xero-model"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export type AgentToolingSettingsAdapter = Pick<
  XeroDesktopAdapter,
  "isDesktopRuntime" | "agentToolingSettings" | "agentToolingUpdateSettings"
>

interface AgentToolingSectionProps {
  adapter?: AgentToolingSettingsAdapter
}

interface StyleOption {
  value: AgentToolApplicationStyleDto
  label: string
  summary: string
  detail: string
}

const STYLE_OPTIONS: readonly StyleOption[] = [
  {
    value: "conservative",
    label: "Conservative",
    summary: "Narrow, single-purpose tool calls.",
    detail:
      "Best for smaller or less reliable models. Xero prefers focused, granular tools that are easy to inspect and recover from one step at a time.",
  },
  {
    value: "balanced",
    label: "Balanced",
    summary: "The standard mix of granular and declarative tools.",
    detail:
      "The default for general-purpose models. Xero exposes both step-by-step and declarative tools and lets the model choose between them.",
  },
  {
    value: "declarative_first",
    label: "Declarative-first",
    summary: "Encourage whole-change tool calls.",
    detail:
      "Best for high-capability coding models. Xero highlights tools that accept a batch, patch, or intent object so the model can describe a whole change in one validated call.",
  },
] as const

const PROVIDER_OPTIONS: ReadonlyArray<{ value: RuntimeProviderIdDto; label: string }> = [
  { value: "anthropic", label: "Anthropic" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "openai_codex", label: "OpenAI Codex" },
  { value: "openai_api", label: "OpenAI API" },
  { value: "github_models", label: "GitHub Models" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "ollama", label: "Ollama" },
  { value: "azure_openai", label: "Azure OpenAI" },
  { value: "gemini_ai_studio", label: "Gemini (AI Studio)" },
  { value: "bedrock", label: "AWS Bedrock" },
  { value: "vertex", label: "Google Vertex" },
] as const

const FALLBACK_SETTINGS: AgentToolingSettingsDto = {
  globalDefault: "balanced",
  modelOverrides: [],
  updatedAt: null,
}

type LoadState = "idle" | "loading" | "ready" | "error"
type SaveState = "idle" | "saving"

export function AgentToolingSection({ adapter }: AgentToolingSectionProps) {
  const [settings, setSettings] = useState<AgentToolingSettingsDto>(FALLBACK_SETTINGS)
  const [loadState, setLoadState] = useState<LoadState>("idle")
  const [saveState, setSaveState] = useState<SaveState>("idle")
  const [error, setError] = useState<string | null>(null)
  const [pendingOverrideKey, setPendingOverrideKey] = useState<string | null>(null)

  const canUseAdapter = Boolean(
    adapter?.isDesktopRuntime?.() &&
      adapter.agentToolingSettings &&
      adapter.agentToolingUpdateSettings,
  )

  const load = useCallback(() => {
    if (!canUseAdapter || !adapter?.agentToolingSettings) {
      setSettings(FALLBACK_SETTINGS)
      setLoadState("ready")
      return
    }

    setLoadState("loading")
    setError(null)
    adapter
      .agentToolingSettings()
      .then((next) => {
        setSettings(next)
        setLoadState("ready")
      })
      .catch((loadError) => {
        setSettings(FALLBACK_SETTINGS)
        setError(getErrorMessage(loadError, "Xero could not load Agent Tooling settings."))
        setLoadState("error")
      })
  }, [adapter, canUseAdapter])

  useEffect(() => {
    load()
  }, [load])

  const isBusy = loadState === "loading" || saveState === "saving"

  const submit = useCallback(
    async (
      request: UpsertAgentToolingSettingsRequestDto,
      options: { pendingKey?: string | null } = {},
    ) => {
      if (!adapter?.agentToolingUpdateSettings) return null

      setSaveState("saving")
      setPendingOverrideKey(options.pendingKey ?? null)
      setError(null)
      try {
        const next = await adapter.agentToolingUpdateSettings(request)
        setSettings(next)
        return next
      } catch (saveError) {
        setError(getErrorMessage(saveError, "Xero could not save Agent Tooling settings."))
        return null
      } finally {
        setSaveState("idle")
        setPendingOverrideKey(null)
      }
    },
    [adapter],
  )

  const updateGlobalDefault = useCallback(
    (value: AgentToolApplicationStyleDto) => {
      if (value === settings.globalDefault) return
      const previous = settings
      setSettings((current) => ({ ...current, globalDefault: value }))
      void submit({ globalDefault: value, modelOverrides: [] }).then((result) => {
        if (!result) {
          setSettings(previous)
        }
      })
    },
    [settings, submit],
  )

  const upsertOverride = useCallback(
    (providerId: string, modelId: string, style: AgentToolApplicationStyleDto) => {
      const key = makeOverrideKey(providerId, modelId)
      void submit(
        {
          modelOverrides: [{ providerId, modelId, style }],
        },
        { pendingKey: key },
      )
    },
    [submit],
  )

  const removeOverride = useCallback(
    (providerId: string, modelId: string) => {
      const key = makeOverrideKey(providerId, modelId)
      void submit(
        {
          modelOverrides: [{ providerId, modelId, style: null }],
        },
        { pendingKey: key },
      )
    },
    [submit],
  )

  const sortedOverrides = useMemo(
    () =>
      [...settings.modelOverrides].sort((left, right) => {
        const providerCompare = left.providerId.localeCompare(right.providerId)
        if (providerCompare !== 0) return providerCompare
        return left.modelId.localeCompare(right.modelId)
      }),
    [settings.modelOverrides],
  )

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Agent Tooling"
        description="Choose how Xero presents tools to each model. Pick a default behavior and override it for individual models when their capabilities differ."
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
        <UnavailableCard />
      ) : (
        <>
          {error ? (
            <Alert variant="destructive" className="rounded-md px-3 py-2 text-[12px]">
              <AlertTriangle className="h-3.5 w-3.5" />
              <AlertTitle className="text-[12px]">Agent Tooling settings need attention</AlertTitle>
              <AlertDescription className="text-[12px]">{error}</AlertDescription>
            </Alert>
          ) : null}

          <GlobalDefaultPanel
            value={settings.globalDefault}
            disabled={isBusy}
            saving={saveState === "saving" && pendingOverrideKey === null}
            onChange={updateGlobalDefault}
          />

          <ModelOverridesPanel
            overrides={sortedOverrides}
            globalDefault={settings.globalDefault}
            disabled={isBusy}
            pendingOverrideKey={pendingOverrideKey}
            onUpsertOverride={upsertOverride}
            onRemoveOverride={removeOverride}
          />
        </>
      )}
    </div>
  )
}

function GlobalDefaultPanel({
  value,
  disabled,
  saving,
  onChange,
}: {
  value: AgentToolApplicationStyleDto
  disabled: boolean
  saving: boolean
  onChange: (value: AgentToolApplicationStyleDto) => void
}) {
  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex items-center justify-between gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Global default</h4>
        {saving ? (
          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <LoaderCircle className="h-3 w-3 animate-spin" />
            Saving…
          </span>
        ) : null}
      </div>
      <RadioGroup
        value={value}
        onValueChange={(next) => onChange(next as AgentToolApplicationStyleDto)}
        className="grid grid-cols-1 gap-2"
        disabled={disabled}
      >
        {STYLE_OPTIONS.map((option) => {
          const selected = option.value === value
          return (
            <label
              key={option.value}
              htmlFor={`agent-tooling-default-${option.value}`}
              className={cn(
                "flex cursor-pointer items-start gap-3 rounded-lg border px-4 py-3 transition-colors",
                selected
                  ? "border-primary/45 bg-primary/5"
                  : "border-border/70 bg-background hover:bg-accent/30",
                disabled && "cursor-default opacity-70",
              )}
            >
              <RadioGroupItem
                id={`agent-tooling-default-${option.value}`}
                value={option.value}
                aria-label={option.label}
                className="mt-0.5"
              />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2">
                  <span className="text-[13px] font-medium text-foreground">{option.label}</span>
                  {selected ? <Check className="h-3.5 w-3.5 text-primary" /> : null}
                </span>
                <span className="mt-0.5 block text-[12px] font-medium text-foreground/85">
                  {option.summary}
                </span>
                <span className="mt-1 block text-[11.5px] leading-[1.5] text-muted-foreground">
                  {option.detail}
                </span>
              </span>
            </label>
          )
        })}
      </RadioGroup>
    </section>
  )
}

function ModelOverridesPanel({
  overrides,
  globalDefault,
  disabled,
  pendingOverrideKey,
  onUpsertOverride,
  onRemoveOverride,
}: {
  overrides: AgentToolingModelOverrideDto[]
  globalDefault: AgentToolApplicationStyleDto
  disabled: boolean
  pendingOverrideKey: string | null
  onUpsertOverride: (providerId: string, modelId: string, style: AgentToolApplicationStyleDto) => void
  onRemoveOverride: (providerId: string, modelId: string) => void
}) {
  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h4 className="text-[12.5px] font-semibold text-foreground">Per-model overrides</h4>
          <p className="mt-0.5 text-[11.5px] leading-[1.5] text-muted-foreground">
            Pin a different style for specific provider/model pairs. Models without an override
            inherit the global default.
          </p>
        </div>
      </div>

      {overrides.length === 0 ? (
        <div className="rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-5 text-center">
          <p className="text-[12px] text-muted-foreground">
            No overrides yet. Every model uses the {styleLabel(globalDefault)} default.
          </p>
        </div>
      ) : (
        <ul
          aria-label="Per-model overrides"
          className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40"
        >
          {overrides.map((entry) => {
            const key = makeOverrideKey(entry.providerId, entry.modelId)
            return (
              <OverrideRow
                key={key}
                entry={entry}
                disabled={disabled}
                saving={pendingOverrideKey === key}
                onChangeStyle={(style) => onUpsertOverride(entry.providerId, entry.modelId, style)}
                onRemove={() => onRemoveOverride(entry.providerId, entry.modelId)}
              />
            )
          })}
        </ul>
      )}

      <AddOverrideForm disabled={disabled} onSubmit={onUpsertOverride} />
    </section>
  )
}

function OverrideRow({
  entry,
  disabled,
  saving,
  onChangeStyle,
  onRemove,
}: {
  entry: AgentToolingModelOverrideDto
  disabled: boolean
  saving: boolean
  onChangeStyle: (style: AgentToolApplicationStyleDto) => void
  onRemove: () => void
}) {
  const providerLabel =
    PROVIDER_OPTIONS.find((option) => option.value === entry.providerId)?.label ?? entry.providerId

  return (
    <li className="flex flex-wrap items-center justify-between gap-3 px-3.5 py-2.5">
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">{entry.modelId}</p>
        <p className="mt-0.5 text-[11px] text-muted-foreground">{providerLabel}</p>
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        {saving ? (
          <LoaderCircle aria-hidden className="h-3 w-3 animate-spin text-muted-foreground" />
        ) : null}
        <Select
          value={entry.style}
          disabled={disabled}
          onValueChange={(value) => onChangeStyle(value as AgentToolApplicationStyleDto)}
        >
          <SelectTrigger
            aria-label={`Style for ${providerLabel} ${entry.modelId}`}
            className="h-8 w-auto min-w-[160px] text-[12.5px]"
            size="sm"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {STYLE_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
          aria-label={`Remove override for ${providerLabel} ${entry.modelId}`}
          disabled={disabled}
          onClick={onRemove}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
    </li>
  )
}

function AddOverrideForm({
  disabled,
  onSubmit,
}: {
  disabled: boolean
  onSubmit: (providerId: string, modelId: string, style: AgentToolApplicationStyleDto) => void
}) {
  const [providerId, setProviderId] = useState<RuntimeProviderIdDto>("anthropic")
  const [modelId, setModelId] = useState("")
  const [style, setStyle] = useState<AgentToolApplicationStyleDto>("balanced")
  const [validation, setValidation] = useState<string | null>(null)

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const trimmedModelId = modelId.trim()
    if (trimmedModelId.length === 0) {
      setValidation("Enter a model id before saving the override.")
      return
    }
    setValidation(null)
    onSubmit(providerId, trimmedModelId, style)
    setModelId("")
    setStyle("balanced")
  }

  return (
    <form
      className="flex flex-col gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3"
      onSubmit={submit}
      aria-label="Add per-model override"
    >
      <div className="grid grid-cols-1 gap-2.5 md:grid-cols-[1fr_1.4fr_1fr]">
        <div className="flex flex-col gap-1">
          <Label htmlFor="agent-tooling-add-provider" className="text-[11.5px] font-medium text-foreground">
            Provider
          </Label>
          <Select
            value={providerId}
            disabled={disabled}
            onValueChange={(value) => setProviderId(value as RuntimeProviderIdDto)}
          >
            <SelectTrigger
              id="agent-tooling-add-provider"
              aria-label="Provider"
              className="h-8 text-[12.5px]"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PROVIDER_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex flex-col gap-1">
          <Label htmlFor="agent-tooling-add-model" className="text-[11.5px] font-medium text-foreground">
            Model id
          </Label>
          <Input
            id="agent-tooling-add-model"
            placeholder="e.g. claude-opus-4-7"
            value={modelId}
            disabled={disabled}
            onChange={(event) => setModelId(event.target.value)}
            className="h-8 text-[12.5px]"
            spellCheck={false}
            autoComplete="off"
          />
        </div>
        <div className="flex flex-col gap-1">
          <Label htmlFor="agent-tooling-add-style" className="text-[11.5px] font-medium text-foreground">
            Style
          </Label>
          <Select
            value={style}
            disabled={disabled}
            onValueChange={(value) => setStyle(value as AgentToolApplicationStyleDto)}
          >
            <SelectTrigger
              id="agent-tooling-add-style"
              aria-label="Style"
              className="h-8 text-[12.5px]"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {STYLE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      <div className="flex items-center justify-between gap-3">
        <p className="text-[11px] text-muted-foreground">
          {validation ?? "Override applies whenever this provider/model pair starts a new run."}
        </p>
        <Button
          type="submit"
          size="sm"
          className="h-8 gap-1.5 text-[12px]"
          disabled={disabled}
        >
          <Plus className="h-3.5 w-3.5" />
          Add override
        </Button>
      </div>
    </form>
  )
}

function UnavailableCard() {
  return (
    <div className="flex flex-col items-center gap-2 rounded-md border border-dashed border-border/60 bg-secondary/10 px-5 py-8 text-center">
      <Bot className="h-4 w-4 text-muted-foreground" />
      <p className="text-[12.5px] font-medium text-foreground">Desktop runtime required</p>
      <p className="max-w-sm text-[11.5px] leading-[1.5] text-muted-foreground">
        Agent Tooling settings are available when Xero is running as a desktop app.
      </p>
    </div>
  )
}

function styleLabel(value: AgentToolApplicationStyleDto): string {
  return STYLE_OPTIONS.find((option) => option.value === value)?.label ?? value
}

function makeOverrideKey(providerId: string, modelId: string): string {
  return `${providerId}::${modelId}`
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }
  if (typeof error === "string" && error.trim().length > 0) {
    return error
  }
  return fallback
}
