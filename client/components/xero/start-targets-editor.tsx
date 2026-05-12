"use client"

import { useEffect, useMemo, useState } from "react"
import { Loader2, Plus, Save, Sparkles, Trash2 } from "lucide-react"

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
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import type {
  StartTargetDto,
  StartTargetInputDto,
} from "@/src/lib/xero-desktop"
import type { RuntimeAgentIdDto } from "@/src/lib/xero-model/runtime"

export interface StartTargetsSuggestRequest {
  modelId: string
  providerProfileId: string | null
  runtimeAgentId: RuntimeAgentIdDto | null
  thinkingEffort:
    | "minimal"
    | "low"
    | "medium"
    | "high"
    | "x_high"
    | null
}

export interface SuggestedTarget {
  name: string
  command: string
}

interface StartTargetsEditorProps {
  initialTargets: StartTargetDto[]
  saveLabel?: string
  hideSaveOnPristine?: boolean
  className?: string
  onSave: (targets: StartTargetInputDto[]) => Promise<void>
  resolveSuggestRequest?: () => StartTargetsSuggestRequest | null
  onSuggest?: (
    request: StartTargetsSuggestRequest,
  ) => Promise<{ targets: SuggestedTarget[] }>
  onSaved?: () => void
}

interface RowState {
  rowKey: string
  id: string | null
  name: string
  command: string
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
})

const blankRow = (): RowState => ({
  rowKey: nextRowKey(),
  id: null,
  name: "",
  command: "",
})

function rowsEqualToInitial(rows: RowState[], initial: StartTargetDto[]) {
  if (rows.length !== initial.length) return false
  for (let i = 0; i < rows.length; i += 1) {
    const row = rows[i]
    const target = initial[i]
    if (row.id !== target.id) return false
    if (row.name.trim() !== target.name.trim()) return false
    if (row.command.trim() !== target.command.trim()) return false
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

export function StartTargetsEditor({
  initialTargets,
  saveLabel = "Save",
  hideSaveOnPristine = false,
  className,
  onSave,
  resolveSuggestRequest,
  onSuggest,
  onSaved,
}: StartTargetsEditorProps) {
  const [rows, setRows] = useState<RowState[]>(() =>
    initialTargets.length > 0 ? initialTargets.map(toRow) : [blankRow()],
  )
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

  const busy = saving || suggesting
  const aiEnabled = Boolean(onSuggest && resolveSuggestRequest)
  const pristine = useMemo(
    () => rowsEqualToInitial(rows, initialTargets),
    [rows, initialTargets],
  )

  const replaceRowsWithSuggestion = (suggestion: SuggestedTarget[]) => {
    const next = suggestion.map(
      ({ name, command }): RowState => ({
        rowKey: nextRowKey(),
        id: null,
        name,
        command,
      }),
    )
    setRows(next.length > 0 ? next : [blankRow()])
    setError(null)
    setSaveMessage(null)
  }

  const handleSuggest = async () => {
    if (!onSuggest || !resolveSuggestRequest) return
    const request = resolveSuggestRequest()
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

  const saveDisabled =
    busy || (hideSaveOnPristine && pristine) || (!hideSaveOnPristine && false)

  return (
    <div className={cn("flex flex-col gap-3", className)}>
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
      ) : (
        <p className="text-[12px] text-muted-foreground">
          One target = single root command. Add more for monorepos. Use{" "}
          <code className="font-mono">cd path && cmd</code> to run from a
          subdirectory.
        </p>
      )}

      <div className="flex items-center justify-between gap-2">
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

      <AlertDialog
        open={pendingSuggestion !== null}
        onOpenChange={(open) => {
          if (!open) setPendingSuggestion(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Replace current targets?</AlertDialogTitle>
            <AlertDialogDescription>
              AI suggested {pendingSuggestion?.length ?? 0} target
              {pendingSuggestion?.length === 1 ? "" : "s"}. This will replace
              your unsaved changes.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep current</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingSuggestion) {
                  replaceRowsWithSuggestion(pendingSuggestion)
                }
                setPendingSuggestion(null)
              }}
            >
              Replace
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
