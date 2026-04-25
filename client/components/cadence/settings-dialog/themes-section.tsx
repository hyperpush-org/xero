import { Check, Moon, Palette, Sun } from "lucide-react"
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

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        icon={Palette}
        title="Themes"
        description="Pick a palette for the entire app. Editor syntax highlighting and diff rendering follow the selected theme."
        scope="app-wide"
      />

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

interface ThemeGroupProps {
  icon: React.ElementType
  label: string
  themes: ThemeDefinition[]
  activeId: string
  onSelect: (id: string) => void
}

function ThemeGroup({ icon: Icon, label, themes, activeId, onSelect }: ThemeGroupProps) {
  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex items-center gap-2">
        <Icon className="h-3 w-3 text-muted-foreground/70" />
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/70">
          {label}
        </span>
        <span className="ml-auto text-[10.5px] text-muted-foreground/60">
          {themes.length}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-2.5">
        {themes.map((theme) => (
          <ThemeCard
            key={theme.id}
            theme={theme}
            active={theme.id === activeId}
            onSelect={() => onSelect(theme.id)}
          />
        ))}
      </div>
    </div>
  )
}

interface ThemeCardProps {
  theme: ThemeDefinition
  active: boolean
  onSelect: () => void
}

function ThemeCard({ theme, active, onSelect }: ThemeCardProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      className={cn(
        "group relative flex flex-col gap-3 overflow-hidden rounded-lg border p-3 text-left transition-all motion-fast",
        active
          ? "border-primary/60 bg-primary/[0.05] shadow-sm"
          : "border-border bg-card hover:-translate-y-px hover:border-border/80 hover:bg-secondary/20 hover:shadow-sm",
      )}
    >
      <ThemePreview theme={theme} active={active} />
      <div className="flex items-center justify-between gap-2">
        <p className="truncate text-[13px] font-medium text-foreground">{theme.name}</p>
        <div
          className={cn(
            "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border transition-colors",
            active
              ? "border-primary bg-primary text-primary-foreground"
              : "border-border bg-transparent text-transparent group-hover:border-border/80",
          )}
          aria-hidden
        >
          <Check className="h-3 w-3" />
        </div>
      </div>
      <p className="-mt-1.5 line-clamp-2 text-[11.5px] leading-[1.4] text-muted-foreground">
        {theme.description}
      </p>
    </button>
  )
}

function ThemePreview({ theme, active }: { theme: ThemeDefinition; active: boolean }) {
  const c = theme.colors
  return (
    <div
      className={cn(
        "relative h-20 w-full overflow-hidden rounded-md border transition-colors",
        active ? "border-primary/40" : "border-border/70",
      )}
      style={{ backgroundColor: c.background }}
      aria-hidden
    >
      {/* Sidebar */}
      <div
        className="absolute inset-y-0 left-0 w-5"
        style={{ backgroundColor: c.sidebar }}
      >
        <div
          className="mx-1 mt-1.5 h-1 w-3 rounded-sm"
          style={{ backgroundColor: c.primary }}
        />
        <div
          className="mx-1 mt-1 h-0.5 w-2.5 rounded-sm opacity-50"
          style={{ backgroundColor: c.foreground }}
        />
        <div
          className="mx-1 mt-1 h-0.5 w-2 rounded-sm opacity-40"
          style={{ backgroundColor: c.foreground }}
        />
      </div>
      {/* Content lines */}
      <div className="absolute left-7 top-2 right-2 space-y-1">
        <div className="flex items-center gap-1">
          <div
            className="h-1 w-1 rounded-full"
            style={{ backgroundColor: c.primary }}
          />
          <div
            className="h-1 flex-1 rounded-sm"
            style={{ backgroundColor: c.foreground, opacity: 0.7 }}
          />
        </div>
        <div
          className="h-1 w-3/4 rounded-sm"
          style={{ backgroundColor: c.mutedForeground }}
        />
        <div
          className="h-1 w-1/2 rounded-sm"
          style={{ backgroundColor: c.mutedForeground }}
        />
      </div>
      {/* Accent block bottom-right */}
      <div className="absolute bottom-1.5 right-1.5 flex gap-1">
        <div
          className="h-2 w-3 rounded-sm"
          style={{ backgroundColor: c.accent }}
        />
        <div
          className="h-2 w-2 rounded-sm"
          style={{ backgroundColor: c.primary }}
        />
      </div>
    </div>
  )
}
