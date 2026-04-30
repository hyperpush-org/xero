import * as React from "react"

interface SectionHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
}

export type SectionScope = "app-wide" | "project-bound" | "system" | "developer"

export function SectionHeader({ title, description, actions }: SectionHeaderProps) {
  return (
    <div className="flex items-end justify-between gap-3 border-b border-border/70 pb-3">
      <div className="min-w-0">
        <h3 className="text-[13px] font-semibold tracking-tight text-foreground">{title}</h3>
        {description ? (
          <p className="mt-0.5 text-[12px] leading-[1.5] text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
    </div>
  )
}
