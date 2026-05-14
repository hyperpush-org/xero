import * as React from "react"

interface SectionHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
}

export type SectionScope = "app-wide" | "project-bound" | "system" | "developer"

export function SectionHeader({ title, description, actions }: SectionHeaderProps) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-3 border-b border-border/60 pb-5">
      <div className="min-w-0 flex-1">
        <h2 className="text-[18px] font-semibold leading-tight tracking-[-0.01em] text-foreground">
          {title}
        </h2>
        {description ? (
          <p className="mt-1.5 text-[12.5px] leading-[1.55] text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {actions ? (
        <div className="flex shrink-0 items-center gap-1.5 pt-0.5">{actions}</div>
      ) : null}
    </div>
  )
}
