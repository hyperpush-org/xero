"use client"

import { useEffect, useMemo, useState } from "react"
import {
  ModelThinkingSelect,
  type ModelThinkingSelectGroup,
  type ModelThinkingSelectOption,
} from "@xero/ui/components/model-thinking-select"

import type { StartTargetsModelOption } from "@/components/xero/start-targets-editor"
import {
  loadSourceControlSettings,
  persistSourceControlSettings,
  subscribeSourceControlSettings,
  type SourceControlModelSelection,
  type SourceControlSettings,
} from "@/components/xero/source-control-settings"
import { getProviderModelThinkingEffortLabel } from "@/src/lib/xero-model"

import { SectionHeader } from "./section-header"

interface SourceControlSectionProps {
  modelOptions?: StartTargetsModelOption[]
}

const DEFAULT_MODEL_VALUE = "__source_control_default_model__"

function optionMatchesSelection(
  option: StartTargetsModelOption,
  selection: SourceControlModelSelection | null,
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
  thinkingEffort: SourceControlModelSelection["thinkingEffort"] =
    option.defaultThinkingEffort ?? null,
): SourceControlModelSelection {
  return {
    providerId: option.providerId,
    providerProfileId: option.providerProfileId,
    modelId: option.modelId,
    thinkingEffort,
  }
}

function formatStoredModelLabel(selection: SourceControlModelSelection): string {
  const provider = selection.providerProfileId ?? selection.providerId
  return provider ? `${provider} - ${selection.modelId}` : selection.modelId ?? "Saved model"
}

export function SourceControlSection({ modelOptions = [] }: SourceControlSectionProps) {
  const [settings, setSettings] = useState<SourceControlSettings>(
    loadSourceControlSettings,
  )

  useEffect(
    () =>
      subscribeSourceControlSettings((nextSettings) => {
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
    modelOptions.find((option) =>
      optionMatchesSelection(option, settings.commitMessageModelSelection),
    ) ?? null
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
    settings.commitMessageModelSelection?.thinkingEffort &&
    selectedModel.thinkingEffortOptions.includes(
      settings.commitMessageModelSelection.thinkingEffort,
    )
      ? settings.commitMessageModelSelection.thinkingEffort
      : selectedModel?.defaultThinkingEffort ?? null

  const updateSettings = (patch: Partial<SourceControlSettings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch }
      persistSourceControlSettings(next)
      return next
    })
  }

  const handleModelChange = (value: string) => {
    if (value === DEFAULT_MODEL_VALUE) {
      updateSettings({ commitMessageModelSelection: null })
      return
    }
    const option = modelOptions.find((entry) => entry.selectionKey === value)
    if (!option) return
    const currentThinking = settings.commitMessageModelSelection?.thinkingEffort ?? null
    const nextThinking =
      currentThinking && option.thinkingEffortOptions.includes(currentThinking)
        ? currentThinking
        : option.defaultThinkingEffort ?? null
    updateSettings({ commitMessageModelSelection: selectionFromOption(option, nextThinking) })
  }

  const handleThinkingChange = (value: string) => {
    if (!selectedModel) return
    const thinkingEffort = value as SourceControlModelSelection["thinkingEffort"]
    if (!thinkingEffort || !selectedModel.thinkingEffortOptions.includes(thinkingEffort)) {
      return
    }
    updateSettings({
      commitMessageModelSelection: selectionFromOption(selectedModel, thinkingEffort),
    })
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Source Control"
        description="Configure source-control assistance for commits and staged changes."
      />

      <div className="overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <div className="grid gap-4 px-4 py-4 sm:grid-cols-[minmax(0,1fr)_260px]">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-foreground">
              Commit message model
              <span className="rounded-sm bg-secondary px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide text-muted-foreground">
                LLM
              </span>
            </div>
            <p className="mt-1 text-[12.5px] leading-[1.55] text-muted-foreground">
              Uses this model for AI-generated commit messages. Default uses the last
              active chat model and thinking level.
            </p>
            {settings.commitMessageModelSelection && !selectedModel ? (
              <p className="mt-2 text-[12px] leading-[1.45] text-warning">
                Saved model unavailable:{" "}
                {formatStoredModelLabel(settings.commitMessageModelSelection)}
              </p>
            ) : null}
          </div>

          <ModelThinkingSelect
            ariaLabel="Commit message model"
            disabled={modelGroups.length === 0}
            groups={modelGroups}
            onChange={handleModelChange}
            onThinkingChange={handleThinkingChange}
            placeholder="Default model"
            searchPlaceholder="Search models..."
            selectedThinkingId={selectedThinkingEffort}
            thinkingDisabled={
              !selectedModel || selectedModel.thinkingEffortOptions.length === 0
            }
            thinkingOptions={thinkingOptions}
            thinkingPlaceholder={selectedModel ? "Thinking unavailable" : "Choose model"}
            value={modelSelectValue}
            variant="field"
          />

          {modelOptions.length === 0 ? (
            <p className="text-[12px] leading-[1.45] text-muted-foreground sm:col-start-2">
              Configure a provider before choosing a dedicated commit-message model.
            </p>
          ) : null}
        </div>
      </div>
    </div>
  )
}
