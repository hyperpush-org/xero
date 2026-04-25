import * as React from "react"
import { cn } from "@/lib/utils"

export type SectionScope = "app-wide" | "project-bound" | "system" | "developer"

interface SectionHeaderProps {
  icon: React.ElementType
  title: string
  description: string
  scope?: SectionScope
  actions?: React.ReactNode
}

const SCOPE_LABEL: Record<SectionScope, string> = {
  "app-wide": "App-wide",
  "project-bound": "Project",
  system: "System",
  developer: "Dev only",
}

export function SectionHeader({ icon: Icon, title, description, scope, actions }: SectionHeaderProps) {
  return (
    <div className="flex items-start gap-3.5">
      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-border/70 bg-primary/[0.06]">
        <Icon className="h-[18px] w-[18px] text-primary" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-[15px] font-semibold tracking-tight text-foreground">{title}</h3>
          {scope ? <ScopePill scope={scope} /> : null}
        </div>
        <p className="mt-1 text-[13px] leading-[1.5] text-muted-foreground">{description}</p>
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2 pt-0.5">{actions}</div> : null}
    </div>
  )
}

function ScopePill({ scope }: { scope: SectionScope }) {
  return (
    <span
      className={cn(
        "rounded-sm border px-1.5 py-px text-[10.5px] font-medium uppercase tracking-[0.08em]",
        scope === "app-wide"
          ? "border-primary/30 bg-primary/[0.08] text-primary"
          : scope === "project-bound"
            ? "border-border bg-secondary/60 text-muted-foreground"
            : scope === "developer"
              ? "border-amber-500/30 bg-amber-500/[0.08] text-amber-500 dark:text-amber-300"
              : "border-border bg-secondary/60 text-muted-foreground",
      )}
    >
      {SCOPE_LABEL[scope]}
    </span>
  )
}
