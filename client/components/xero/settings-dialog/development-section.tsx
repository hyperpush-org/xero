import {
  Apple,
  AppWindow,
  FlaskConical,
  Laptop,
  PlayCircle,
  Sparkles,
  Wand2,
} from "lucide-react"
import type { PlatformVariant } from "@/components/xero/shell"
import { detectPlatform } from "@/components/xero/shell"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { ToolErrorLog } from "./development-section/tool-error-log"
import { ToolHarness } from "./development-section/tool-harness"
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
  const overriding = current !== null && current !== detected
  const effectivePlatform = current ?? detected

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Development"
        description="Developer tooling and preview options. Not visible in production builds."
      />

      <PreviewCard
        detected={detected}
        effective={effectivePlatform}
        overriding={overriding}
      />

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">Onboarding</h4>
        <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
          <ToolRow
            icon={Sparkles}
            title="Onboarding flow"
            body="Reopen the first-run setup flow to test provider setup and project import."
            actionLabel="Start onboarding"
            actionIcon={PlayCircle}
            onAction={onStartOnboarding}
            disabled={!onStartOnboarding}
          />
        </ul>
      </section>

      <section className="flex flex-col gap-3">
        <h4 className="text-[12.5px] font-semibold text-foreground">
          Toolbar platform
        </h4>

        <div className="flex gap-1 rounded-md border border-border/60 bg-secondary/30 p-1">
          {PLATFORM_OPTIONS.map((option) => {
            const active = current === option.value
            const Icon = option.icon
            return (
              <button
                key={option.label}
                type="button"
                className={cn(
                  "flex flex-1 items-center justify-center gap-1.5 rounded-md py-1.5 text-[12.5px] font-medium transition-[background-color,color,box-shadow] motion-fast",
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
      </section>

      <ToolErrorLog />

      <ToolHarness />
    </div>
  )
}

function PreviewCard({
  detected,
  effective,
  overriding,
}: {
  detected: PlatformVariant
  effective: PlatformVariant
  overriding: boolean
}) {
  const tone = overriding ? "warn" : "muted"

  return (
    <div className="flex items-start gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
      <FlaskConical
        className={cn(
          "mt-0.5 h-4 w-4 shrink-0",
          tone === "warn" ? "text-warning dark:text-warning" : "text-muted-foreground",
        )}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <p className="truncate text-[12.5px] font-semibold text-foreground">
            Developer preview
          </p>
          {overriding ? (
            <PreviewPill tone="warn" label="Overriding" />
          ) : (
            <PreviewPill tone="muted" label="Auto" />
          )}
        </div>
        <p className="mt-0.5 text-[11.5px] leading-[1.5] text-muted-foreground">
          {overriding
            ? `Rendering as ${formatPlatform(effective)} instead of detected ${formatPlatform(detected)}. Switch back to Auto to use the real platform.`
            : "Using the toolbar layout for the detected operating system."}
        </p>
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
          ? "bg-warning/10 text-warning ring-warning/25 dark:text-warning"
          : "bg-muted/40 text-muted-foreground ring-border/60",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          tone === "warn" ? "bg-warning dark:bg-warning" : "bg-muted-foreground/60",
        )}
        aria-hidden
      />
      {label}
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
