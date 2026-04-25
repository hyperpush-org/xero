import { Code2, Monitor, Rocket } from "lucide-react"
import type { PlatformVariant } from "@/components/cadence/shell"
import { detectPlatform } from "@/components/cadence/shell"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SectionHeader } from "./section-header"

const PLATFORM_OPTIONS: Array<{ value: PlatformVariant | null; label: string; hint: string }> = [
  { value: null, label: "Auto", hint: "Use detected OS" },
  { value: "macos", label: "macOS", hint: "Traffic lights · tabs right" },
  { value: "windows", label: "Windows", hint: "Tabs left · controls right" },
  { value: "linux", label: "Linux", hint: "Same as Windows, rounded" },
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
  const currentHint = PLATFORM_OPTIONS.find((option) => option.value === current)?.hint

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        icon={Code2}
        title="Development"
        description="Developer tooling and preview options. Not visible in production builds."
        scope="developer"
      />

      <div className="rounded-lg border border-border bg-card px-5 py-4">
        <div className="flex items-start gap-3.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Monitor className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-foreground">Toolbar platform</p>
            <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">
              Override the detected platform to preview different toolbar layouts.
            </p>
            <p className="mt-1 text-[11.5px] text-muted-foreground/80">
              Detected:{" "}
              <span className="font-mono font-medium text-foreground/80">{detected}</span>
            </p>
          </div>
        </div>

        <div className="mt-4 flex gap-1 rounded-lg border border-border/70 bg-secondary/30 p-1">
          {PLATFORM_OPTIONS.map(({ value, label }) => {
            const active = current === value
            return (
              <button
                key={label}
                type="button"
                className={cn(
                  "flex-1 rounded-md py-2 text-[13px] font-medium transition-all motion-fast",
                  active
                    ? "bg-background text-foreground shadow-sm ring-1 ring-border/40"
                    : "text-muted-foreground hover:text-foreground",
                )}
                onClick={() => onPlatformOverrideChange?.(value)}
              >
                {label}
              </button>
            )
          })}
        </div>

        {currentHint ? (
          <p className="mt-2.5 text-[12px] text-muted-foreground">
            <span className="text-muted-foreground/70">Behavior:</span> {currentHint}
          </p>
        ) : null}
      </div>

      <div className="rounded-lg border border-border bg-card px-5 py-4">
        <div className="flex items-center gap-3.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-secondary/60">
            <Rocket className="h-4 w-4 text-foreground/70" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-foreground">Onboarding flow</p>
            <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">
              Reopen the first-run setup flow to test provider setup, project import, and notification routing.
            </p>
          </div>
          <Button
            size="sm"
            className="h-9 text-[12px]"
            disabled={!onStartOnboarding}
            onClick={onStartOnboarding}
          >
            Start onboarding
          </Button>
        </div>
      </div>
    </div>
  )
}
