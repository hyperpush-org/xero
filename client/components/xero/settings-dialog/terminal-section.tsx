"use client"

import { useEffect, useMemo, useState } from "react"
import {
  ModelThinkingSelect,
  type ModelThinkingSelectGroup,
  type ModelThinkingSelectOption,
} from "@xero/ui/components/model-thinking-select"

import { Switch } from "@/components/ui/switch"
import type { StartTargetsModelOption } from "@/components/xero/start-targets-editor"
import {
  loadTerminalSuggestionSettings,
  persistTerminalSuggestionSettings,
  subscribeTerminalSuggestionSettings,
  type TerminalSuggestionModelSelection,
  type TerminalSuggestionSettings,
} from "@/components/xero/terminal-suggestion-settings"
import { cn } from "@/lib/utils"
import { getProviderModelThinkingEffortLabel } from "@/src/lib/xero-model"

import { SectionHeader } from "./section-header"

interface TerminalSectionProps {
  modelOptions?: StartTargetsModelOption[]
}

const DEFAULT_MODEL_VALUE = "__terminal_default_model__"

function optionMatchesSelection(
  option: StartTargetsModelOption,
  selection: TerminalSuggestionModelSelection | null,
): boolean {
  if (!selection?.modelId || option.modelId !== selection.modelId) return false
  if (selection.providerProfileId) {
    return option.providerProfileId === selection.providerProfileId
  }
  if (selection.providerId) {
    return option.providerId === selection.providerId
  }
  return true
}

function selectionFromOption(
  option: StartTargetsModelOption,
  thinkingEffort: TerminalSuggestionModelSelection["thinkingEffort"] =
    option.defaultThinkingEffort ?? null,
): TerminalSuggestionModelSelection {
  return {
    providerId: option.providerId,
    providerProfileId: option.providerProfileId,
    modelId: option.modelId,
    runtimeAgentId: null,
    thinkingEffort,
  }
}

function formatStoredModelLabel(selection: TerminalSuggestionModelSelection): string {
  const provider = selection.providerProfileId ?? selection.providerId
  return provider ? `${provider} - ${selection.modelId}` : selection.modelId ?? "Saved model"
}

export function TerminalSection({ modelOptions = [] }: TerminalSectionProps) {
  const [settings, setSettings] = useState<TerminalSuggestionSettings>(
    loadTerminalSuggestionSettings,
  )

  useEffect(
    () =>
      subscribeTerminalSuggestionSettings((nextSettings) => {
        setSettings(nextSettings)
      }),
    [],
  )

  const modelGroups = useMemo<ModelThinkingSelectGroup[]>(() => {
    const groups = new Map<
      string,
      { providerLabel: string; options: ModelThinkingSelectOption[] }
    >()
    for (const option of modelOptions) {
      const existing = groups.get(option.providerLabel)
      const item = { id: option.selectionKey, label: option.label }
      if (existing) {
        existing.options.push(item)
      } else {
        groups.set(option.providerLabel, {
          providerLabel: option.providerLabel,
          options: [item],
        })
      }
    }
    return [
      {
        id: "default",
        options: [{ id: DEFAULT_MODEL_VALUE, label: "Default model" }],
      },
      ...Array.from(groups.values()).map((group) => ({
        id: group.providerLabel,
        label: group.providerLabel,
        options: group.options,
      })),
    ]
  }, [modelOptions])

  const selectedModel =
    modelOptions.find((option) => optionMatchesSelection(option, settings.modelSelection)) ??
    null
  const modelSelectValue = selectedModel?.selectionKey ?? DEFAULT_MODEL_VALUE
  const thinkingOptions = useMemo<ModelThinkingSelectOption[]>(
    () =>
      (selectedModel?.thinkingEffortOptions ?? []).map((effort) => ({
        id: effort,
        label: getProviderModelThinkingEffortLabel(effort),
      })),
    [selectedModel],
  )
  const selectedThinkingEffort =
    selectedModel &&
    settings.modelSelection?.thinkingEffort &&
    selectedModel.thinkingEffortOptions.includes(settings.modelSelection.thinkingEffort)
      ? settings.modelSelection.thinkingEffort
      : selectedModel?.defaultThinkingEffort ?? null

  const updateSettings = (patch: Partial<TerminalSuggestionSettings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch }
      persistTerminalSuggestionSettings(next)
      return next
    })
  }

  const handleModelChange = (value: string) => {
    if (value === DEFAULT_MODEL_VALUE) {
      updateSettings({ modelSelection: null })
      return
    }
    const option = modelOptions.find((entry) => entry.selectionKey === value)
    if (!option) return
    const currentThinking = settings.modelSelection?.thinkingEffort ?? null
    const nextThinking =
      currentThinking && option.thinkingEffortOptions.includes(currentThinking)
        ? currentThinking
        : option.defaultThinkingEffort ?? null
    updateSettings({ modelSelection: selectionFromOption(option, nextThinking) })
  }

  const handleThinkingChange = (value: string) => {
    if (!selectedModel) return
    const thinkingEffort = value as TerminalSuggestionModelSelection["thinkingEffort"]
    if (!thinkingEffort || !selectedModel.thinkingEffortOptions.includes(thinkingEffort)) {
      return
    }
    updateSettings({
      modelSelection: selectionFromOption(selectedModel, thinkingEffort),
    })
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Terminal"
        description="Configure inline terminal autocomplete for shells launched inside Xero."
      />

      <div className="overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <SettingRow
          badge="Local"
          checked={settings.enabled}
          description="Uses recent terminal commands, shell history, project files, and package scripts."
          label="Command suggestions"
          onCheckedChange={(checked) => updateSettings({ enabled: checked })}
        />
        <SettingRow
          badge="Fallback"
          checked={settings.aiEnabled}
          className={!settings.enabled ? "opacity-60" : undefined}
          description="Only asks the configured model when local sources have no useful match."
          disabled={!settings.enabled}
          label="AI suggestions"
          onCheckedChange={(checked) => updateSettings({ aiEnabled: checked })}
        />
        <div
          className={cn(
            "grid gap-4 border-t border-border/50 px-4 py-4 sm:grid-cols-[minmax(0,1fr)_260px]",
            !settings.enabled && "opacity-60",
          )}
        >
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
              Model
              <span className="rounded-sm bg-secondary px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide text-muted-foreground">
                Command autocomplete
              </span>
            </div>
            <p className="mt-1 text-[12.5px] leading-[1.55] text-muted-foreground">
              When AI suggestions are enabled, terminal command suggestions always
              use this model instead of the active chat model.
            </p>
            {settings.modelSelection && !selectedModel ? (
              <p className="mt-2 text-[12px] leading-[1.45] text-warning">
                Saved model unavailable: {formatStoredModelLabel(settings.modelSelection)}
              </p>
            ) : null}
          </div>

          <ModelThinkingSelect
            ariaLabel="Model"
            disabled={!settings.enabled || modelGroups.length === 0}
            groups={modelGroups}
            onChange={handleModelChange}
            onThinkingChange={handleThinkingChange}
            placeholder="Default model"
            searchPlaceholder="Search models..."
            selectedThinkingId={selectedThinkingEffort}
            thinkingDisabled={
              !settings.enabled ||
              !selectedModel ||
              selectedModel.thinkingEffortOptions.length === 0
            }
            thinkingOptions={thinkingOptions}
            thinkingPlaceholder={selectedModel ? "Thinking unavailable" : "Choose model"}
            value={modelSelectValue}
            variant="field"
          />

          {modelOptions.length === 0 ? (
            <p className="sm:col-start-2 text-[12px] leading-[1.45] text-muted-foreground">
              Configure a provider before choosing a dedicated suggestion model.
            </p>
          ) : null}
        </div>
      </div>
    </div>
  )
}

interface SettingRowProps {
  badge: string
  checked: boolean
  className?: string
  description: string
  disabled?: boolean
  label: string
  onCheckedChange: (checked: boolean) => void
}

function SettingRow({
  badge,
  checked,
  className,
  description,
  disabled = false,
  label,
  onCheckedChange,
}: SettingRowProps) {
  return (
    <label
      className={cn(
        "flex items-start justify-between gap-4 border-b border-border/50 px-4 py-4",
        className,
      )}
    >
      <span className="min-w-0">
        <span className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
          {label}
          <span className="rounded-sm bg-secondary px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide text-muted-foreground">
            {badge}
          </span>
        </span>
        <span className="mt-1 block text-[12.5px] leading-[1.55] text-muted-foreground">
          {description}
        </span>
      </span>
      <Switch
        aria-label={label}
        checked={checked}
        className="mt-0.5"
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </label>
  )
}
