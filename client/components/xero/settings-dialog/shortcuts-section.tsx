import { RotateCcw, X } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { Kbd, KbdGroup } from "@/components/ui/kbd"
import { detectPlatform } from "@/components/xero/shell"
import {
  SHORTCUT_CATEGORIES,
  SHORTCUT_DEFINITIONS,
  bindingFromEvent,
  bindingsEqual,
  formatBinding,
  isBindingEmpty,
  type ShortcutBinding,
  type ShortcutDefinition,
  type ShortcutId,
} from "@/src/features/shortcuts/shortcuts-definitions"
import { useShortcuts } from "@/src/features/shortcuts/shortcuts-provider"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export function ShortcutsSection() {
  const { bindings, setBinding, resetBinding, resetAll } = useShortcuts()
  const platform = useMemo<"macos" | "other">(
    () => (detectPlatform() === "macos" ? "macos" : "other"),
    [],
  )
  const [recordingId, setRecordingId] = useState<ShortcutId | null>(null)

  const allCustom = useMemo(
    () =>
      SHORTCUT_DEFINITIONS.some(
        (def) => !bindingsEqual(bindings[def.id], def.defaultBinding),
      ),
    [bindings],
  )

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Keyboard Shortcuts"
        description="Customize key bindings for navigation and common actions. Click a shortcut to record a new combination."
        actions={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!allCustom}
            onClick={resetAll}
          >
            <RotateCcw className="h-3.5 w-3.5" />
            Reset all
          </Button>
        }
      />

      <div className="flex flex-col gap-6">
        {SHORTCUT_CATEGORIES.map((category) => {
          const items = SHORTCUT_DEFINITIONS.filter((def) => def.category === category)
          if (items.length === 0) return null
          return (
            <section key={category} className="flex flex-col gap-2">
              <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
                {category}
              </h4>
              <div className="flex flex-col overflow-hidden rounded-lg border border-border/60 bg-card/30">
                {items.map((def, idx) => (
                  <ShortcutRow
                    key={def.id}
                    definition={def}
                    binding={bindings[def.id]}
                    platform={platform}
                    isRecording={recordingId === def.id}
                    onStartRecording={() => setRecordingId(def.id)}
                    onStopRecording={() => setRecordingId((cur) => (cur === def.id ? null : cur))}
                    onChange={(next) => {
                      setBinding(def.id, next)
                      setRecordingId(null)
                    }}
                    onReset={() => {
                      resetBinding(def.id)
                      setRecordingId(null)
                    }}
                    onClear={() => {
                      setBinding(def.id, { mod: false, shift: false, alt: false, key: "" })
                      setRecordingId(null)
                    }}
                    showDivider={idx > 0}
                  />
                ))}
              </div>
            </section>
          )
        })}
      </div>
    </div>
  )
}

interface ShortcutRowProps {
  definition: ShortcutDefinition
  binding: ShortcutBinding
  platform: "macos" | "other"
  isRecording: boolean
  showDivider: boolean
  onStartRecording: () => void
  onStopRecording: () => void
  onChange: (binding: ShortcutBinding) => void
  onReset: () => void
  onClear: () => void
}

function ShortcutRow({
  definition,
  binding,
  platform,
  isRecording,
  showDivider,
  onStartRecording,
  onStopRecording,
  onChange,
  onReset,
  onClear,
}: ShortcutRowProps) {
  const isCustom = !bindingsEqual(binding, definition.defaultBinding)
  const isEmpty = isBindingEmpty(binding)
  const captureRef = useRef<HTMLButtonElement | null>(null)

  // While the row is in "record mode", capture the next non-modifier keystroke
  // and persist it as the new binding. Escape cancels without changing.
  useEffect(() => {
    if (!isRecording) return
    captureRef.current?.focus()

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        event.stopPropagation()
        onStopRecording()
        return
      }
      const captured = bindingFromEvent(event)
      if (!captured) return
      event.preventDefault()
      event.stopPropagation()
      onChange(captured)
    }

    window.addEventListener("keydown", onKeyDown, true)
    return () => window.removeEventListener("keydown", onKeyDown, true)
  }, [isRecording, onChange, onStopRecording])

  return (
    <div
      className={cn(
        "flex items-center gap-3 px-3.5 py-2.5",
        showDivider && "border-t border-border/50",
      )}
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">{definition.label}</p>
        <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground">
          {definition.description}
        </p>
      </div>

      <button
        ref={captureRef}
        type="button"
        onClick={() => {
          if (isRecording) {
            onStopRecording()
          } else {
            onStartRecording()
          }
        }}
        aria-pressed={isRecording}
        className={cn(
          "inline-flex h-7 min-w-[110px] items-center justify-center gap-1 rounded-md border px-2 text-[12px] font-medium transition-colors",
          isRecording
            ? "border-primary/60 bg-primary/[0.08] text-foreground"
            : isEmpty
              ? "border-dashed border-border/60 bg-transparent text-muted-foreground hover:border-primary/40 hover:text-foreground"
              : "border-border/60 bg-background/40 text-foreground hover:border-primary/40",
        )}
      >
        {isRecording ? (
          <span className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
            Press keys…
          </span>
        ) : isEmpty ? (
          <span>Set shortcut</span>
        ) : (
          <BindingKbd binding={binding} platform={platform} />
        )}
      </button>

      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className={cn("h-7 w-7", !isCustom && "invisible")}
          onClick={onReset}
          aria-label={`Reset ${definition.label}`}
          title="Reset to default"
        >
          <RotateCcw className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className={cn("h-7 w-7 text-muted-foreground hover:text-destructive", isEmpty && "invisible")}
          onClick={onClear}
          aria-label={`Clear ${definition.label}`}
          title="Clear shortcut"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  )
}

function BindingKbd({
  binding,
  platform,
}: {
  binding: ShortcutBinding
  platform: "macos" | "other"
}) {
  const parts: string[] = []
  if (platform === "macos") {
    if (binding.alt) parts.push("⌥")
    if (binding.shift) parts.push("⇧")
    if (binding.mod) parts.push("⌘")
  } else {
    if (binding.mod) parts.push("Ctrl")
    if (binding.shift) parts.push("Shift")
    if (binding.alt) parts.push("Alt")
  }
  // Use formatBinding for the rendered key portion to keep arrow / space
  // labels consistent.
  const keyLabel = formatBinding({ mod: false, shift: false, alt: false, key: binding.key }, platform)

  return (
    <KbdGroup>
      {parts.map((part) => (
        <Kbd key={part}>{part}</Kbd>
      ))}
      <Kbd>{keyLabel}</Kbd>
    </KbdGroup>
  )
}
