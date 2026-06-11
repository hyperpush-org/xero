"use client"

import type { ButtonHTMLAttributes, ReactNode } from "react"

import { cn } from "@/lib/utils"

interface FloatingRightSidebarHeaderProps {
  title: string
  actions?: ReactNode
  className?: string
}

export function FloatingRightSidebarHeader({
  title,
  actions,
  className,
}: FloatingRightSidebarHeaderProps) {
  return (
    <header
      className={cn(
        "flex items-center justify-between gap-2 border-b border-border/60 px-2 py-1",
        className,
      )}
    >
      <div className="min-w-0">
        <p className="truncate text-[11px] uppercase tracking-wide text-muted-foreground">
          {title}
        </p>
      </div>
      {actions ? <div className="flex items-center gap-1">{actions}</div> : null}
    </header>
  )
}

export function FloatingRightSidebarHeaderButton({
  className,
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      className={cn(
        "inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors",
        "hover:bg-foreground/10 hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        "disabled:opacity-60",
        className,
      )}
      type={type}
      {...props}
    />
  )
}
