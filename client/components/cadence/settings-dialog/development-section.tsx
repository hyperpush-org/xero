import {
  Apple,
  AppWindow,
  Cpu,
  Eye,
  FlaskConical,
  Laptop,
  PlayCircle,
  Sparkles,
  Wand2,
} from "lucide-react"
import type { PlatformVariant } from "@/components/cadence/shell"
import { detectPlatform } from "@/components/cadence/shell"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

interface PlatformOption {
  value: PlatformVariant | null
  label: string
  hint: string
  icon: React.ElementType
}

const PLATFORM_OPTIONS: PlatformOption[] = [
  { value: null, label: "Auto", hint: "Use detected OS", icon: Wand2 },
  { value: "macos", label: "macOS", hint: "Traffic lights · tabs right", icon: Apple },
  { value: "windows", label: "Windows", hint: "Tabs left · controls right", icon: AppWindow },
  { value: "linux", label: "Linux", hint: "Same as Windows, rounded", icon: Laptop },
]

export interface DevelopmentSectionProps {
  platformOverride?: PlatformVariant | null
  onPlatformOverrideChange?: (value: PlatformVariant | null) => void
  onStartOnboarding?: () => void
}

export function DevelopmentSection({
  platformOverride,
  onPlatformOverrideChange,
  onStartOnboarding,
}: DevelopmentSectionProps) {
  const detected = detectPlatform()
  const current = platformOverride ?? null
  const currentOption = PLATFORM_OPTIONS.find((option) => option.value === current) ?? PLATFORM_OPTIONS[0]
  const overriding = current !== null && current !== detected
  const effectivePlatform = current ?? detected

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Development"
        description="Developer tooling and preview options. Not visible in production builds."
      />

      <PreviewCard
        currentOption={currentOption}
        detected={detected}
        effective={effectivePlatform}
        overriding={overriding}
      />

      <section className="flex flex-col gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Toolbar platform
        </h4>

        <div className="flex flex-col gap-2.5 rounded-lg border border-border/60 bg-card/30 p-3.5">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <span
                className="flex size-6 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground"
                aria-hidden
              >
                <Cpu className="h-3 w-3" />
              </span>
              <label className="text-[12px] font-medium text-foreground">Render toolbar as</label>
            </div>
            <span className="text-[11px] text-muted-foreground">
              Detected{" "}
              <span className="font-mono text-foreground/80">{detected}</span>
            </span>
          </div>

          <div className="flex gap-1 rounded-md border border-border/70 bg-secondary/30 p-1">
            {PLATFORM_OPTIONS.map((option) => {
              const active = current === option.value
              const Icon = option.icon
              return (
                <button
                  key={option.label}
                  type="button"
                  className={cn(
                    "flex flex-1 items-center justify-center gap-1.5 rounded-md py-1.5 text-[12.5px] font-medium transition-all motion-fast",
                    active
                      ? "bg-background text-foreground shadow-sm ring-1 ring-border/40"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  onClick={() => onPlatformOverrideChange?.(option.value)}
                  aria-pressed={active}
                >
                  <Icon className="h-3.5 w-3.5" />
                  {option.label}
                </button>
              )
            })}
          </div>

          <p className="text-[11.5px] leading-[1.5] text-muted-foreground">
            <span className="text-muted-foreground/70">Behavior:</span> {currentOption.hint}
          </p>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
          Tools
        </h4>
        <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
          <ToolRow
            icon={Sparkles}
            title="Onboarding flow"
            body="Reopen the first-run setup flow to test provider setup, project import, and notification routing."
            actionLabel="Start onboarding"
            actionIcon={PlayCircle}
            onAction={onStartOnboarding}
            disabled={!onStartOnboarding}
          />
        </ul>
      </section>
    </div>
  )
}

function PreviewCard({
  currentOption,
  detected,
  effective,
  overriding,
}: {
  currentOption: PlatformOption
  detected: PlatformVariant
  effective: PlatformVariant
  overriding: boolean
}) {
  const tone = overriding ? "warn" : "muted"

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <div
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-full ring-1 ring-inset",
            tone === "warn"
              ? "bg-amber-500/10 ring-amber-500/25"
              : "bg-muted/40 ring-border/60",
          )}
          aria-hidden
        >
          <FlaskConical
            className={cn(
              "h-5 w-5",
              tone === "warn"
                ? "text-amber-600 dark:text-amber-400"
                : "text-muted-foreground",
            )}
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">
              Developer preview
            </p>
            {overriding ? (
              <PreviewPill tone="warn" label="Overriding" />
            ) : (
              <PreviewPill tone="muted" label="Auto" />
            )}
          </div>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">
            {overriding
              ? `Toolbar is rendering as ${formatPlatform(effective)} instead of the detected ${formatPlatform(detected)}. Switch back to Auto to use the real platform.`
              : "Cadence is using the toolbar layout for the detected operating system. Override below to preview other platforms."}
          </p>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={Eye} label="Active" value={formatPlatform(effective)} />
        <MetaItem icon={Cpu} label="Detected" value={formatPlatform(detected)} mono />
        <MetaItem icon={currentOption.icon} label="Mode" value={currentOption.label} />
      </div>
    </div>
  )
}

function ToolRow({
  icon: Icon,
  title,
  body,
  actionLabel,
  actionIcon: ActionIcon,
  onAction,
  disabled,
}: {
  icon: React.ElementType
  title: string
  body: string
  actionLabel: string
  actionIcon: React.ElementType
  onAction?: () => void
  disabled?: boolean
}) {
  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
      <Button
        size="sm"
        variant="outline"
        className="h-8 shrink-0 gap-1.5 text-[12px]"
        disabled={disabled}
        onClick={onAction}
      >
        <ActionIcon className="h-3.5 w-3.5" />
        {actionLabel}
      </Button>
    </li>
  )
}

function PreviewPill({ tone, label }: { tone: "warn" | "muted"; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em] ring-1 ring-inset",
        tone === "warn"
          ? "bg-amber-500/10 text-amber-600 ring-amber-500/25 dark:text-amber-400"
          : "bg-muted/40 text-muted-foreground ring-border/60",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          tone === "warn" ? "bg-amber-500 dark:bg-amber-400" : "bg-muted-foreground/60",
        )}
        aria-hidden
      />
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

function formatPlatform(platform: PlatformVariant): string {
  switch (platform) {
    case "macos":
      return "macOS"
    case "windows":
      return "Windows"
    case "linux":
      return "Linux"
  }
}
