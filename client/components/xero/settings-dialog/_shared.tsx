import * as React from "react"
import { AlertTriangle, CheckCircle2 } from "lucide-react"

import { cn } from "@/lib/utils"

export type Tone = "good" | "info" | "warn" | "bad" | "neutral"

export const TONE_BORDER: Record<Tone, string> = {
  good: "border-success/30 bg-success/[0.08] text-success",
  info: "border-info/30 bg-info/[0.08] text-info",
  warn: "border-warning/30 bg-warning/[0.08] text-warning",
  bad: "border-destructive/40 bg-destructive/[0.08] text-destructive",
  neutral: "border-border bg-secondary/60 text-foreground/70",
}

const PILL_BASE =
  "inline-flex h-[18px] items-center gap-1 rounded-full border px-1.5 text-[10.5px] font-medium"

export function Pill({
  tone,
  children,
  className,
}: {
  tone: Tone
  children: React.ReactNode
  className?: string
}) {
  return <span className={cn(PILL_BASE, TONE_BORDER[tone], className)}>{children}</span>
}

const INLINE_TONE: Record<Tone, string> = {
  good: "text-success",
  info: "text-info",
  warn: "text-warning",
  bad: "text-destructive",
  neutral: "text-foreground",
}

interface InlineCountsProps {
  items: Array<{ label: string; value: string | number; tone?: Tone }>
  className?: string
  "data-testid"?: string
}

export function InlineCounts({ items, className, ...rest }: InlineCountsProps) {
  return (
    <div
      data-testid={rest["data-testid"]}
      className={cn(
        "flex flex-wrap items-center gap-x-5 gap-y-1.5 text-[11.5px]",
        className,
      )}
    >
      {items.map((item) => (
        <div key={item.label} className="flex items-baseline gap-1.5">
          <span className="text-muted-foreground">{item.label}</span>
          <span
            className={cn(
              "text-[13px] font-semibold tabular-nums leading-none",
              INLINE_TONE[item.tone ?? "neutral"],
            )}
          >
            {item.value}
          </span>
        </div>
      ))}
    </div>
  )
}

export function EmptyPanel({
  icon,
  title,
  body,
}: {
  icon: React.ReactNode
  title: string
  body: string
}) {
  return (
    <div className="flex min-h-[160px] items-center justify-center rounded-md border border-dashed border-border/60 bg-secondary/10 px-6 text-center">
      <div className="max-w-sm">
        <div className="mx-auto flex h-7 w-7 items-center justify-center rounded-md border border-border/60 bg-secondary/40">
          {icon}
        </div>
        <p className="mt-3 text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-1 text-[11.5px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}

export function ErrorBanner({ message }: { message: string }) {
  return (
    <p
      role="alert"
      className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive"
    >
      <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
      <span>{message}</span>
    </p>
  )
}

export function SuccessBanner({
  message,
  testId,
}: {
  message: string
  testId?: string
}) {
  return (
    <p
      role="status"
      data-testid={testId}
      className="flex items-start gap-2 rounded-md border border-success/30 bg-success/[0.06] px-3 py-2 text-[12px] text-success"
    >
      <CheckCircle2 className="mt-px h-3.5 w-3.5 shrink-0" />
      <span>{message}</span>
    </p>
  )
}

export function SubHeading({
  children,
  count,
  className,
}: {
  children: React.ReactNode
  count?: React.ReactNode
  className?: string
}) {
  return (
    <h4 className={cn("text-[12.5px] font-semibold text-foreground", className)}>
      {children}
      {count !== undefined && count !== null ? (
        <span className="ml-1.5 font-normal text-muted-foreground">{count}</span>
      ) : null}
    </h4>
  )
}

export function ListContainer({
  children,
  className,
  testId,
}: {
  children: React.ReactNode
  className?: string
  testId?: string
}) {
  return (
    <div
      data-testid={testId}
      className={cn(
        "overflow-hidden rounded-md border border-border/60 divide-y divide-border/40",
        className,
      )}
    >
      {children}
    </div>
  )
}
