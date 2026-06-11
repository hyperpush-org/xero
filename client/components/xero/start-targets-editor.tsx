"use client"

import { useEffect, useMemo, useState } from "react"
import { Globe2, Loader2, Plus, Save, Sparkles, Trash2 } from "lucide-react"
import { BaseAlertDialog } from "@xero/ui/components/base-dialog"
import {
  ModelThinkingSelect,
  type ModelThinkingSelectGroup,
  type ModelThinkingSelectOption,
} from "@xero/ui/components/model-thinking-select"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import type {
  StartTargetDto,
  StartTargetInputDto,
} from "@/src/lib/xero-desktop"
import { getProviderModelThinkingEffortLabel } from "@/src/lib/xero-model"
import type { RuntimeAgentIdDto } from "@/src/lib/xero-model/runtime"

export interface StartTargetsSuggestRequest {
  modelId: string
  providerId?: string | null
  providerProfileId: string | null
  runtimeAgentId: RuntimeAgentIdDto | null
  thinkingEffort:
    | "none"
    | "minimal"
    | "low"
    | "medium"
    | "high"
    | "x_high"
    | null
}

type StartTargetsThinkingEffort = Exclude<
  StartTargetsSuggestRequest["thinkingEffort"],
  null
>

export interface StartTargetsModelOption {
  selectionKey: string
  providerId: string
  providerProfileId: string | null
  providerLabel: string
  modelId: string
  label: string
  thinkingEffortOptions: StartTargetsThinkingEffort[]
  defaultThinkingEffort: StartTargetsSuggestRequest["thinkingEffort"]
}

export interface SuggestedTarget {
  name: string
  command: string
  browserSupported?: boolean
}

interface StartTargetsEditorProps {
  initialTargets: StartTargetDto[]
  saveLabel?: string
  hideSaveOnPristine?: boolean
  fixedFooter?: boolean
  className?: string
  onSave: (targets: StartTargetInputDto[]) => Promise<void>
  resolveSuggestRequest?: () => StartTargetsSuggestRequest | null
  onSuggest?: (
    request: StartTargetsSuggestRequest,
  ) => Promise<{ targets: SuggestedTarget[] }>
  modelOptions?: StartTargetsModelOption[]
  showModelSelector?: boolean
  onSaved?: () => void
}

interface RowState {
  rowKey: string
  id: string | null
  name: string
  command: string
  browserSupported: boolean
}

let rowKeyCounter = 0
const nextRowKey = () => {
  rowKeyCounter += 1
  return `row-${rowKeyCounter}`
}

const toRow = (target: StartTargetDto): RowState => ({
  rowKey: nextRowKey(),
  id: target.id,
  name: target.name,
  command: target.command,
  browserSupported: target.browserSupported === true,
})

const blankRow = (): RowState => ({
  rowKey: nextRowKey(),
  id: null,
  name: "",
  command: "",
  browserSupported: false,
})

function rowsEqualToInitial(rows: RowState[], initial: StartTargetDto[]) {
  if (rows.length !== initial.length) return false
  for (let i = 0; i < rows.length; i += 1) {
    const row = rows[i]
    const target = initial[i]
    if (row.id !== target.id) return false
    if (row.name.trim() !== target.name.trim()) return false
    if (row.command.trim() !== target.command.trim()) return false
    if (row.browserSupported !== (target.browserSupported === true)) return false
  }
  return true
}

function validate(rows: RowState[]): string | null {
  const usable = rows.filter(
    (row) => row.name.trim().length > 0 || row.command.trim().length > 0,
  )
  if (usable.length === 0) {
    // Clearing the list is allowed.
    return null
  }
  const names = new Set<string>()
  for (const row of usable) {
    const name = row.name.trim()
    const command = row.command.trim()
    if (name.length === 0) {
      return "Every target needs a name."
    }
    if (command.length === 0) {
      return `Target "${name}" needs a command.`
    }
    const key = name.toLowerCase()
    if (names.has(key)) {
      return `Target names must be unique. "${name}" appears more than once.`
    }
    names.add(key)
  }
  return null
}

function findModelForRequest(
  options: readonly StartTargetsModelOption[],
  request: StartTargetsSuggestRequest | null,
): StartTargetsModelOption | null {
  const modelId = request?.modelId.trim() ?? ""
  if (!modelId) return null

  const providerId = request?.providerId?.trim() ?? ""
  if (providerId) {
    const model = options.find(
      (option) => option.providerId === providerId && option.modelId === modelId,
    )
    if (model) return model
  }

  const providerProfileId = request?.providerProfileId?.trim() ?? ""
  if (providerProfileId) {
    const model = options.find(
      (option) =>
        option.providerProfileId === providerProfileId &&
        option.modelId === modelId,
    )
    if (model) return model
  }

  return options.find((option) => option.modelId === modelId) ?? null
}

function resolveInitialModelSelectionKey(
  options: readonly StartTargetsModelOption[],
  request: StartTargetsSuggestRequest | null,
): string | null {
  return findModelForRequest(options, request)?.selectionKey ?? options[0]?.selectionKey ?? null
}

function resolveInitialThinkingEffort(
  options: readonly StartTargetsModelOption[],
  request: StartTargetsSuggestRequest | null,
): StartTargetsSuggestRequest["thinkingEffort"] {
  const option = findModelForRequest(options, request) ?? options[0] ?? null
  if (!option) return request?.thinkingEffort ?? null
  return normalizeThinkingEffortForModel(
    option,
    request?.thinkingEffort ?? option.defaultThinkingEffort ?? null,
  )
}

function normalizeThinkingEffortForModel(
  option: StartTargetsModelOption,
  thinkingEffort: StartTargetsSuggestRequest["thinkingEffort"],
): StartTargetsSuggestRequest["thinkingEffort"] {
  if (thinkingEffort && option.thinkingEffortOptions.includes(thinkingEffort)) {
    return thinkingEffort
  }
  return option.defaultThinkingEffort ?? null
}

function requestWithModelOption(
  request: StartTargetsSuggestRequest | null,
  option: StartTargetsModelOption | null,
  thinkingEffort: StartTargetsSuggestRequest["thinkingEffort"],
): StartTargetsSuggestRequest | null {
  if (!option) return request
  return {
    modelId: option.modelId,
    providerId: option.providerId,
    providerProfileId: option.providerProfileId,
    runtimeAgentId: request?.runtimeAgentId ?? null,
    thinkingEffort: normalizeThinkingEffortForModel(
      option,
      thinkingEffort ?? request?.thinkingEffort ?? option.defaultThinkingEffort ?? null,
    ),
  }
}

export function StartTargetsEditor({
  initialTargets,
  saveLabel = "Save",
  hideSaveOnPristine = false,
  fixedFooter = false,
  className,
  onSave,
  resolveSuggestRequest,
  onSuggest,
  modelOptions = [],
  showModelSelector = true,
  onSaved,
}: StartTargetsEditorProps) {
  const [rows, setRows] = useState<RowState[]>(() =>
    initialTargets.length > 0 ? initialTargets.map(toRow) : [blankRow()],
  )
  const initialSuggestRequest = resolveSuggestRequest?.() ?? null
  const [selectedModelSelectionKey, setSelectedModelSelectionKey] = useState<
    string | null
  >(() => resolveInitialModelSelectionKey(modelOptions, initialSuggestRequest))
  const [selectedThinkingEffort, setSelectedThinkingEffort] = useState<
    StartTargetsSuggestRequest["thinkingEffort"]
  >(() => resolveInitialThinkingEffort(modelOptions, initialSuggestRequest))
  const [error, setError] = useState<string | null>(null)
  const [saveMessage, setSaveMessage] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [suggesting, setSuggesting] = useState(false)
  const [pendingSuggestion, setPendingSuggestion] = useState<
    SuggestedTarget[] | null
  >(null)

  useEffect(() => {
    setRows(
      initialTargets.length > 0 ? initialTargets.map(toRow) : [blankRow()],
    )
    setError(null)
    setSaveMessage(null)
  }, [initialTargets])

  useEffect(() => {
    setSelectedModelSelectionKey((current) => {
      if (current && modelOptions.some((option) => option.selectionKey === current)) {
        return current
      }
      return resolveInitialModelSelectionKey(
        modelOptions,
        resolveSuggestRequest?.() ?? null,
      )
    })
  }, [modelOptions, resolveSuggestRequest])

  const busy = saving || suggesting
  const aiEnabled = Boolean(onSuggest && resolveSuggestRequest)
  const selectedModel =
    modelOptions.find((option) => option.selectionKey === selectedModelSelectionKey) ??
    null
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
    return Array.from(groups.values()).map((group) => ({
      id: group.providerLabel,
      label: group.providerLabel,
      options: group.options,
    }))
  }, [modelOptions])
  const thinkingOptions = useMemo<ModelThinkingSelectOption[]>(
    () =>
      (selectedModel?.thinkingEffortOptions ?? []).map((effort) => ({
        id: effort,
        label: getProviderModelThinkingEffortLabel(effort),
      })),
    [selectedModel],
  )

  useEffect(() => {
    if (!selectedModel) {
      setSelectedThinkingEffort(null)
      return
    }
    setSelectedThinkingEffort((current) =>
      normalizeThinkingEffortForModel(selectedModel, current),
    )
  }, [selectedModel])
  const pristine = useMemo(
    () => rowsEqualToInitial(rows, initialTargets),
    [rows, initialTargets],
  )

  const replaceRowsWithSuggestion = (suggestion: SuggestedTarget[]) => {
    const next = suggestion.map(
      ({ name, command, browserSupported }): RowState => ({
        rowKey: nextRowKey(),
        id: null,
        name,
        command,
        browserSupported: browserSupported === true,
      }),
    )
    setRows(next.length > 0 ? next : [blankRow()])
    setError(null)
    setSaveMessage(null)
  }

  const handleSuggest = async () => {
    if (!onSuggest || !resolveSuggestRequest) return
    const baseRequest = resolveSuggestRequest()
    const request = requestWithModelOption(
      baseRequest,
      showModelSelector ? selectedModel : null,
      selectedThinkingEffort,
    )
    if (!request) {
      setError("Configure a model in the Agent pane before using AI suggest.")
      return
    }
    setError(null)
    setSaveMessage(null)
    setSuggesting(true)
    try {
      const result = await onSuggest(request)
      const suggestion = result.targets.filter(
        (target) => target.name.trim().length > 0 && target.command.trim().length > 0,
      )
      if (suggestion.length === 0) {
        setError("AI returned no usable targets. Try a different model.")
        return
      }
      const hasUserContent = rows.some(
        (row) => row.name.trim().length > 0 || row.command.trim().length > 0,
      )
      if (hasUserContent && !pristine) {
        setPendingSuggestion(suggestion)
        return
      }
      replaceRowsWithSuggestion(suggestion)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "AI suggestion failed.")
    } finally {
      setSuggesting(false)
    }
  }

  const handleSave = async () => {
    const validationError = validate(rows)
    if (validationError) {
      setError(validationError)
      setSaveMessage(null)
      return
    }
    setError(null)
    setSaveMessage(null)
    setSaving(true)
    try {
      const payload: StartTargetInputDto[] = rows
        .filter((row) => row.name.trim().length > 0 && row.command.trim().length > 0)
        .map((row) => ({
          id: row.id ?? null,
          name: row.name.trim(),
          command: row.command.trim(),
          browserSupported: row.browserSupported,
        }))
      await onSave(payload)
      setSaveMessage("Saved.")
      onSaved?.()
    } catch (caught) {
      setError(
        caught instanceof Error ? caught.message : "Could not save start targets.",
      )
    } finally {
      setSaving(false)
    }
  }

  const updateRow = (rowKey: string, patch: Partial<RowState>) => {
    setRows((prev) =>
      prev.map((row) => (row.rowKey === rowKey ? { ...row, ...patch } : row)),
    )
    setError(null)
    setSaveMessage(null)
  }

  const removeRow = (rowKey: string) => {
    setRows((prev) => {
      const next = prev.filter((row) => row.rowKey !== rowKey)
      return next.length > 0 ? next : [blankRow()]
    })
    setError(null)
    setSaveMessage(null)
  }

  const addRow = () => {
    setRows((prev) => [...prev, blankRow()])
    setError(null)
    setSaveMessage(null)
  }

  const handleModelChange = (value: string) => {
    const option = modelOptions.find((entry) => entry.selectionKey === value)
    setSelectedModelSelectionKey(value)
    if (!option) return
    setSelectedThinkingEffort((current) =>
      normalizeThinkingEffortForModel(option, current),
    )
  }

  const handleThinkingChange = (value: string) => {
    if (!selectedModel) return
    const nextThinkingEffort = value as StartTargetsSuggestRequest["thinkingEffort"]
    setSelectedThinkingEffort(
      normalizeThinkingEffortForModel(selectedModel, nextThinkingEffort),
    )
  }

  const saveDisabled =
    busy || (hideSaveOnPristine && pristine) || (!hideSaveOnPristine && false)

  return (
    <div
      className={cn(
        fixedFooter
          ? "grid min-h-0 grid-rows-[minmax(0,1fr)_auto]"
          : "flex flex-col gap-3",
        className,
      )}
    >
      <div
        className={cn(
          "flex flex-col gap-3",
          fixedFooter && "min-h-0 overflow-y-auto overscroll-contain px-6 pb-4 pt-1 scrollbar-thin",
        )}
      >
        <div className="flex flex-col gap-2">
          {rows.map((row, index) => (
            <div
              key={row.rowKey}
              className="rounded-md border border-border/60 bg-secondary/20 p-2.5"
            >
              <div className="flex items-center gap-2">
                <Input
                  aria-label={`Target ${index + 1} name`}
                  value={row.name}
                  onChange={(event) => updateRow(row.rowKey, { name: event.target.value })}
                  placeholder="web"
                  disabled={busy}
                  className="h-8 w-32 font-mono text-[12.5px]"
                />
                <Input
                  aria-label={`Target ${index + 1} command`}
                  value={row.command}
                  onChange={(event) => updateRow(row.rowKey, { command: event.target.value })}
                  placeholder="cd apps/web && pnpm dev"
                  disabled={busy}
                  className="h-8 flex-1 font-mono text-[12.5px]"
                />
                <Button
                  aria-label={`Remove target ${index + 1}`}
                  variant="ghost"
                  size="icon"
                  onClick={() => removeRow(row.rowKey)}
                  disabled={busy}
                  className="h-8 w-8 shrink-0 text-muted-foreground hover:text-destructive"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
              <div className="mt-2 flex items-center justify-between rounded-md border border-border/40 bg-background/35 px-2 py-1.5">
                <div className="flex min-w-0 items-center gap-2 text-[11.5px] text-muted-foreground">
                  <Globe2 className="h-3.5 w-3.5 shrink-0" />
                  <span>Browser supported</span>
                </div>
                <Switch
                  aria-label={`Target ${index + 1} browser supported`}
                  checked={row.browserSupported}
                  disabled={busy}
                  onCheckedChange={(checked) => updateRow(row.rowKey, { browserSupported: checked })}
                />
              </div>
            </div>
          ))}
        </div>

        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={addRow}
          disabled={busy}
          className="self-start text-muted-foreground hover:text-foreground"
        >
          <Plus className="h-3.5 w-3.5" />
          Add target
        </Button>

        {error ? (
          <p className="rounded-md border border-destructive/40 bg-destructive/10 px-2.5 py-1.5 text-[12px] text-destructive">
            {error}
          </p>
        ) : saveMessage ? (
          <p className="text-[12px] text-success">{saveMessage}</p>
        ) : null}

        {aiEnabled && showModelSelector ? (
          <div className="rounded-md border border-border/60 bg-secondary/15 px-4 py-3">
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0">
                <div className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
                  AI model
                </div>
              </div>
              {modelOptions.length > 0 ? (
                <div className="w-full sm:w-[260px]">
                  <ModelThinkingSelect
                    ariaLabel="AI suggestion model"
                    disabled={busy}
                    groups={modelGroups}
                    onChange={handleModelChange}
                    onThinkingChange={handleThinkingChange}
                    placeholder="Select model"
                    searchPlaceholder="Search models..."
                    selectedThinkingId={selectedThinkingEffort}
                    thinkingDisabled={
                      busy ||
                      !selectedModel ||
                      selectedModel.thinkingEffortOptions.length === 0
                    }
                    thinkingOptions={thinkingOptions}
                    thinkingPlaceholder={
                      selectedModel ? "Thinking unavailable" : "Choose model"
                    }
                    triggerClassName="h-8 text-[12px]"
                    value={selectedModelSelectionKey}
                    variant="field"
                  />
                </div>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>

      <div
        className={cn(
          "flex items-center justify-between gap-2",
          fixedFooter && "border-t border-border/60 bg-background/95 px-6 py-4",
        )}
      >
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={!aiEnabled || busy}
          onClick={() => void handleSuggest()}
          className="text-muted-foreground hover:text-foreground"
        >
          {suggesting ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Sparkles className="h-3.5 w-3.5" />
          )}
          {suggesting ? "Asking AI…" : "Suggest with AI"}
        </Button>
        <Button
          size="sm"
          onClick={() => void handleSave()}
          disabled={saveDisabled}
          className="gap-1.5"
        >
          {saving ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Save className="h-3.5 w-3.5" />
          )}
          {saving ? "Saving…" : saveLabel}
        </Button>
      </div>

      <BaseAlertDialog
        open={pendingSuggestion !== null}
        onOpenChange={(open) => {
          if (!open) setPendingSuggestion(null)
        }}
        variant="confirmation"
        title="Replace current targets?"
        description={
          <>
              AI suggested {pendingSuggestion?.length ?? 0} target
              {pendingSuggestion?.length === 1 ? "" : "s"}. This will replace
              your unsaved changes.
          </>
        }
        cancelAction={{ label: "Keep current" }}
        action={{
          label: "Replace",
          onClick: () => {
            if (pendingSuggestion) {
              replaceRowsWithSuggestion(pendingSuggestion)
            }
            setPendingSuggestion(null)
          },
        }}
      />
    </div>
  )
}
