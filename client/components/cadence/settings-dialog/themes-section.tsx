import { Check, Code2, Moon, Palette, Sun } from "lucide-react"
import { useMemo } from "react"
import { useTheme } from "@/src/features/theme/theme-provider"
import type { ThemeDefinition } from "@/src/features/theme/theme-definitions"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

export function ThemesSection() {
  const { themes, themeId, setThemeId } = useTheme()

  const { dark, light } = useMemo(() => {
    const dark: ThemeDefinition[] = []
    const light: ThemeDefinition[] = []
    for (const theme of themes) {
      if (theme.appearance === "light") light.push(theme)
      else dark.push(theme)
    }
    return { dark, light }
  }, [themes])

  const activeTheme = useMemo(
    () => themes.find((theme) => theme.id === themeId) ?? themes[0] ?? null,
    [themes, themeId],
  )

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Themes"
        description="Pick a palette for the entire app. Editor syntax highlighting and diff rendering follow the selected theme."
      />

      {activeTheme ? (
        <ActiveThemeCard theme={activeTheme} darkCount={dark.length} lightCount={light.length} />
      ) : null}

      {dark.length > 0 ? (
        <ThemeGroup
          icon={Moon}
          label="Dark"
          themes={dark}
          activeId={themeId}
          onSelect={setThemeId}
        />
      ) : null}

      {light.length > 0 ? (
        <ThemeGroup
          icon={Sun}
          label="Light"
          themes={light}
          activeId={themeId}
          onSelect={setThemeId}
        />
      ) : null}
    </div>
  )
}

function ActiveThemeCard({
  theme,
  darkCount,
  lightCount,
}: {
  theme: ThemeDefinition
  darkCount: number
  lightCount: number
}) {
  const isDark = theme.appearance !== "light"
  const ToneIcon = isDark ? Moon : Sun
  const appearanceLabel = isDark ? "Dark" : "Light"

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <ThemePreview theme={theme} size="lg" />

        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">{theme.name}</p>
            <AppearancePill icon={ToneIcon} label={appearanceLabel} dark={isDark} />
          </div>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">{theme.description}</p>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={Palette} label="Palette" value={theme.id} mono />
        <MetaItem icon={Moon} label="Dark" value={`${darkCount}`} />
        <MetaItem icon={Sun} label="Light" value={`${lightCount}`} />
        <MetaItem icon={Code2} label="Syntax" value="follows theme" />
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
          : "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300 dark:ring-amber-400/25",
      )}
    >
      <Icon className="h-3 w-3" />
      {label}
    </span>
  )
}

function MetaItem({
  icon: Icon,
  label,
  value,
  mono = false,
}: {
  icon: React.ElementType
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className={cn("text-foreground/80", mono && "font-mono text-[11.5px]")}>{value}</span>
    </span>
  )
}
