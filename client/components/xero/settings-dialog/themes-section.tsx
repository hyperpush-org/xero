import { Check, Moon, Palette, Pencil, Plus, Sun, Trash2, X } from "lucide-react"
import { useMemo, useState } from "react"
import { useTheme } from "@/src/features/theme/theme-provider"
import {
  CUSTOM_THEME_ID_PREFIX,
  EDITABLE_COLOR_KEYS,
  EDITABLE_COLOR_LABELS,
  type EditableColorKey,
  THEMES,
  type ThemeAppearance,
  type ThemeDefinition,
  deriveCustomEditorPalette,
  expandCustomColors,
  isCustomThemeId,
  normalizeHexColor,
  pickEditableColors,
} from "@/src/features/theme/theme-definitions"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

type EditableColors = Record<EditableColorKey, string>

interface DraftTheme {
  id: string
  name: string
  description: string
  appearance: ThemeAppearance
  baseId: string
  colors: EditableColors
}

export function ThemesSection() {
  const { themes, customThemes, themeId, setThemeId, saveCustomTheme, deleteCustomTheme } =
    useTheme()
  const [tab, setTab] = useState<"presets" | "custom">("presets")
  const [draft, setDraft] = useState<DraftTheme | null>(null)

  const { presets, presetDark, presetLight } = useMemo(() => {
    const presets = themes.filter((t) => !isCustomThemeId(t.id))
    return {
      presets,
      presetDark: presets.filter((t) => t.appearance === "dark"),
      presetLight: presets.filter((t) => t.appearance === "light"),
    }
  }, [themes])

  const activeTheme = useMemo(
    () => themes.find((theme) => theme.id === themeId) ?? themes[0] ?? null,
    [themes, themeId],
  )

  const startNewDraft = () => {
    const base = themes.find((t) => t.id === themeId) ?? THEMES[0]
    setDraft({
      id: `${CUSTOM_THEME_ID_PREFIX}${Date.now().toString(36)}`,
      name: "My Theme",
      description: "Custom palette",
      appearance: base.appearance,
      baseId: base.id,
      colors: pickEditableColors(base.colors),
    })
  }

  const startEditDraft = (theme: ThemeDefinition) => {
    const base = THEMES.find((t) => t.id === (theme as ThemeDefinition & { baseId?: string }).id)
    setDraft({
      id: theme.id,
      name: theme.name,
      description: theme.description,
      appearance: theme.appearance,
      baseId: base?.id ?? THEMES[0].id,
      colors: pickEditableColors(theme.colors),
    })
  }

  const handleSaveDraft = () => {
    if (!draft) return
    const base = THEMES.find((t) => t.id === draft.baseId) ?? THEMES[0]
    const colors = expandCustomColors(draft.colors, base.colors)
    const next: ThemeDefinition = {
      id: draft.id,
      name: draft.name.trim() || "Untitled Theme",
      description: draft.description.trim() || "Custom palette",
      appearance: draft.appearance,
      shiki: base.shiki,
      colors,
      editor: deriveCustomEditorPalette(base.editor, colors),
    }
    saveCustomTheme(next)
    setThemeId(next.id)
    setDraft(null)
  }

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Themes"
        description="Pick a palette for the entire app, or design your own in advanced mode."
      />

      {activeTheme ? <ActiveThemeCard theme={activeTheme} /> : null}

      <div role="tablist" className="flex items-center gap-5 border-b border-border/50">
        <TabButton
          icon={Palette}
          label="Presets"
          active={tab === "presets"}
          onClick={() => setTab("presets")}
        />
        <TabButton
          icon={Pencil}
          label="Advanced"
          active={tab === "custom"}
          onClick={() => setTab("custom")}
        />
      </div>

      {tab === "presets" ? (
        <div className="flex flex-col gap-7">
          {presetDark.length > 0 ? (
            <ThemeGroup
              icon={Moon}
              label="Dark"
              themes={presetDark}
              activeId={themeId}
              onSelect={setThemeId}
            />
          ) : null}

          {presetLight.length > 0 ? (
            <ThemeGroup
              icon={Sun}
              label="Light"
              themes={presetLight}
              activeId={themeId}
              onSelect={setThemeId}
            />
          ) : null}
        </div>
      ) : (
        <div className="flex flex-col gap-4">
          {draft ? (
            <ThemeEditor
              draft={draft}
              setDraft={setDraft}
              presets={presets}
              onSave={handleSaveDraft}
              onCancel={() => setDraft(null)}
              isExisting={customThemes.some((t) => t.id === draft.id)}
              onDelete={() => {
                deleteCustomTheme(draft.id)
                setDraft(null)
              }}
            />
          ) : (
            <CustomThemeList
              themes={customThemes}
              activeId={themeId}
              onSelect={setThemeId}
              onEdit={startEditDraft}
              onDelete={deleteCustomTheme}
              onCreate={startNewDraft}
            />
          )}
        </div>
      )}
    </div>
  )
}

function ActiveThemeCard({ theme }: { theme: ThemeDefinition }) {
  const isDark = theme.appearance !== "light"
  const ToneIcon = isDark ? Moon : Sun
  const appearanceLabel = isDark ? "Dark" : "Light"
  const isCustom = isCustomThemeId(theme.id)

  return (
    <div className="flex items-center gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
      <ThemePreview theme={theme} size="sm" />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <p className="truncate text-[12.5px] font-semibold text-foreground">{theme.name}</p>
          <AppearancePill icon={ToneIcon} label={appearanceLabel} dark={isDark} />
          {isCustom ? (
            <span className="inline-flex h-[18px] items-center gap-1 rounded-full bg-primary/10 px-1.5 text-[10.5px] font-medium text-primary">
              <Pencil className="h-3 w-3" />
              Custom
            </span>
          ) : null}
        </div>
        <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground">{theme.description}</p>
      </div>
    </div>
  )
}

interface ThemeGroupProps {
  icon: React.ElementType
  label: string
  themes: ThemeDefinition[]
  activeId: string
  onSelect: (id: string) => void
}

function ThemeGroup({ icon: Icon, label, themes, activeId, onSelect }: ThemeGroupProps) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Icon className="h-3.5 w-3.5 text-muted-foreground/80" />
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
          {label}
        </h4>
        <span className="ml-auto text-[11px] tabular-nums text-muted-foreground">
          {themes.length} {themes.length === 1 ? "theme" : "themes"}
        </span>
      </div>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {themes.map((theme) => (
          <ThemeRow
            key={theme.id}
            theme={theme}
            active={theme.id === activeId}
            onSelect={() => onSelect(theme.id)}
          />
        ))}
      </div>
    </section>
  )
}

interface ThemeRowProps {
  theme: ThemeDefinition
  active: boolean
  onSelect: () => void
}

function ThemeRow({ theme, active, onSelect }: ThemeRowProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      className={cn(
        "group relative flex items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-all motion-fast",
        active
          ? "border-primary/50 bg-primary/[0.06] shadow-[0_0_0_1px_var(--tw-ring-color,transparent)]"
          : "border-border/60 bg-card/30 hover:-translate-y-px hover:border-primary/40 hover:bg-card/60 hover:shadow-sm",
      )}
    >
      <ThemePreview theme={theme} size="sm" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[12.5px] font-medium text-foreground">{theme.name}</p>
        <p className="mt-0.5 line-clamp-1 text-[11.5px] leading-[1.4] text-muted-foreground">
          {theme.description}
        </p>
      </div>
      <div
        className={cn(
          "flex h-4 w-4 shrink-0 items-center justify-center rounded-full transition-colors",
          active
            ? "bg-primary text-primary-foreground"
            : "border border-border/70 bg-transparent text-transparent",
        )}
        aria-hidden
      >
        <Check className="h-2.5 w-2.5" />
      </div>
    </button>
  )
}

interface CustomThemeListProps {
  themes: ThemeDefinition[]
  activeId: string
  onSelect: (id: string) => void
  onEdit: (theme: ThemeDefinition) => void
  onDelete: (id: string) => void
  onCreate: () => void
}

function CustomThemeList({
  themes,
  activeId,
  onSelect,
  onEdit,
  onDelete,
  onCreate,
}: CustomThemeListProps) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Pencil className="h-3.5 w-3.5 text-muted-foreground/80" />
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
          Your Themes
        </h4>
        <span className="ml-auto text-[11px] tabular-nums text-muted-foreground">
          {themes.length} {themes.length === 1 ? "theme" : "themes"}
        </span>
      </div>

      {themes.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border/60 bg-card/20 px-4 py-6 text-center">
          <p className="text-[12.5px] text-muted-foreground">
            No custom themes yet. Start from a preset and tune it to your liking.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2">
          {themes.map((theme) => (
            <CustomThemeRow
              key={theme.id}
              theme={theme}
              active={theme.id === activeId}
              onSelect={() => onSelect(theme.id)}
              onEdit={() => onEdit(theme)}
              onDelete={() => onDelete(theme.id)}
            />
          ))}
        </div>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={onCreate}
        className="self-start"
      >
        <Plus className="h-3.5 w-3.5" />
        New theme
      </Button>
    </section>
  )
}

function CustomThemeRow({
  theme,
  active,
  onSelect,
  onEdit,
  onDelete,
}: {
  theme: ThemeDefinition
  active: boolean
  onSelect: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  return (
    <div
      className={cn(
        "group relative flex items-center gap-3 rounded-lg border px-3 py-2.5 transition-all motion-fast",
        active
          ? "border-primary/50 bg-primary/[0.06]"
          : "border-border/60 bg-card/30 hover:border-primary/40 hover:bg-card/60",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        aria-pressed={active}
      >
        <ThemePreview theme={theme} size="sm" />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[12.5px] font-medium text-foreground">{theme.name}</p>
          <p className="mt-0.5 line-clamp-1 text-[11.5px] leading-[1.4] text-muted-foreground">
            {theme.description}
          </p>
        </div>
        <div
          className={cn(
            "flex h-4 w-4 shrink-0 items-center justify-center rounded-full transition-colors",
            active
              ? "bg-primary text-primary-foreground"
              : "border border-border/70 bg-transparent text-transparent",
          )}
          aria-hidden
        >
          <Check className="h-2.5 w-2.5" />
        </div>
      </button>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="h-7 w-7"
          onClick={onEdit}
          aria-label={`Edit ${theme.name}`}
        >
          <Pencil className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="h-7 w-7 text-muted-foreground hover:text-destructive"
          onClick={onDelete}
          aria-label={`Delete ${theme.name}`}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  )
}

interface ThemeEditorProps {
  draft: DraftTheme
  setDraft: (next: DraftTheme | null) => void
  presets: ThemeDefinition[]
  onSave: () => void
  onCancel: () => void
  onDelete: () => void
  isExisting: boolean
}

function ThemeEditor({
  draft,
  setDraft,
  presets,
  onSave,
  onCancel,
  onDelete,
  isExisting,
}: ThemeEditorProps) {
  const updateColor = (key: EditableColorKey, raw: string) => {
    const fallback = draft.colors[key]
    setDraft({ ...draft, colors: { ...draft.colors, [key]: normalizeHexColor(raw, fallback) } })
  }

  const previewColors = useMemo(() => {
    const base = presets.find((t) => t.id === draft.baseId) ?? presets[0]
    return expandCustomColors(draft.colors, base.colors)
  }, [draft.baseId, draft.colors, presets])

  return (
    <div className="flex flex-col gap-5 rounded-xl border border-border/70 bg-card/30 p-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h4 className="text-[12.5px] font-semibold text-foreground">
            {isExisting ? "Edit theme" : "New theme"}
          </h4>
          <p className="mt-0.5 text-[11.5px] text-muted-foreground">
            Tune the palette below. Syntax highlighting is inherited from the base preset.
          </p>
        </div>
        <PreviewSwatch colors={previewColors} />
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label="Name">
          <Input
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            placeholder="My Theme"
            className="h-8 text-[12.5px]"
          />
        </Field>
        <Field label="Description">
          <Input
            value={draft.description}
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            placeholder="Short tagline"
            className="h-8 text-[12.5px]"
          />
        </Field>
        <Field label="Appearance">
          <Select
            value={draft.appearance}
            onValueChange={(v) => setDraft({ ...draft, appearance: v as ThemeAppearance })}
          >
            <SelectTrigger className="h-8 text-[12.5px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="dark">Dark</SelectItem>
              <SelectItem value="light">Light</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field label="Inherit syntax from">
          <Select
            value={draft.baseId}
            onValueChange={(v) => setDraft({ ...draft, baseId: v })}
          >
            <SelectTrigger className="h-8 text-[12.5px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {presets.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      </div>

      <div className="flex flex-col gap-2">
        <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/85">
          Colors
        </p>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {EDITABLE_COLOR_KEYS.map((key) => (
            <ColorField
              key={key}
              label={EDITABLE_COLOR_LABELS[key]}
              value={draft.colors[key]}
              onChange={(v) => updateColor(key, v)}
            />
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/60 pt-4">
        {isExisting ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onDelete}
            className="mr-auto text-muted-foreground hover:text-destructive"
          >
            <Trash2 className="h-3.5 w-3.5" />
            Delete
          </Button>
        ) : null}
        <Button type="button" variant="outline" size="sm" onClick={onCancel}>
          <X className="h-3.5 w-3.5" />
          Cancel
        </Button>
        <Button type="button" size="sm" onClick={onSave}>
          <Check className="h-3.5 w-3.5" />
          Save & Apply
        </Button>
      </div>
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-[11px] font-medium text-muted-foreground">{label}</Label>
      {children}
    </div>
  )
}

function ColorField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (next: string) => void
}) {
  return (
    <div className="flex items-center gap-2 rounded-md border border-border/60 bg-background/40 px-2 py-1.5">
      <input
        type="color"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-7 w-7 shrink-0 cursor-pointer rounded-md border border-border/60 bg-transparent p-0"
        aria-label={label}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[11px] text-muted-foreground">{label}</span>
        <Input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="h-6 border-0 bg-transparent px-0 font-mono text-[11.5px] shadow-none focus-visible:ring-0"
          spellCheck={false}
        />
      </div>
    </div>
  )
}

function PreviewSwatch({ colors }: { colors: ThemeDefinition["colors"] }) {
  return (
    <div
      className="flex h-12 w-20 shrink-0 overflow-hidden rounded-md border border-border/60 ring-1 ring-inset ring-black/5 dark:ring-white/5"
      style={{ backgroundColor: colors.background }}
      aria-hidden
    >
      <div className="w-1/3" style={{ backgroundColor: colors.sidebar }} />
      <div className="flex flex-1 flex-col justify-end gap-0.5 p-1">
        <div className="h-1 w-full rounded-sm" style={{ backgroundColor: colors.primary }} />
        <div className="h-1 w-full rounded-sm" style={{ backgroundColor: colors.accent }} />
      </div>
    </div>
  )
}

function TabButton({
  icon: Icon,
  label,
  active,
  onClick,
}: {
  icon: React.ElementType
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "-mb-px inline-flex items-center gap-1.5 border-b-2 px-0.5 py-2 text-[12.5px] font-medium transition-colors",
        active
          ? "border-foreground text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      {label}
    </button>
  )
}

function ThemePreview({ theme, size }: { theme: ThemeDefinition; size: "sm" | "lg" }) {
  const c = theme.colors
  const dimensions = size === "lg" ? "h-12 w-12" : "h-9 w-9"
  return (
    <div
      className={cn(
        "relative flex shrink-0 overflow-hidden rounded-md border border-border/60 ring-1 ring-inset ring-black/5 dark:ring-white/5",
        dimensions,
      )}
      style={{ backgroundColor: c.background }}
      aria-hidden
    >
      <div className="w-1/3" style={{ backgroundColor: c.sidebar }} />
      <div className="flex flex-1 flex-col justify-end gap-0.5 p-1">
        <div className="h-1 w-full rounded-sm" style={{ backgroundColor: c.primary }} />
        <div className="h-1 w-full rounded-sm" style={{ backgroundColor: c.accent }} />
      </div>
    </div>
  )
}

function AppearancePill({
  icon: Icon,
  label,
  dark,
}: {
  icon: React.ElementType
  label: string
  dark: boolean
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em] ring-1 ring-inset",
        dark
          ? "bg-indigo-500/10 text-indigo-500 ring-indigo-500/20 dark:text-indigo-300 dark:ring-indigo-400/25"
          : "bg-warning/10 text-warning ring-warning/20 dark:text-warning dark:ring-warning/25",
      )}
    >
      <Icon className="h-3 w-3" />
      {label}
    </span>
  )
}

