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
  "inline-flex h-[20px] items-center gap-1 rounded-full border px-2 text-[11px] font-medium"

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
        "flex flex-wrap items-center gap-x-5 gap-y-1.5 text-[12.5px]",
        className,
      )}
    >
      {items.map((item) => (
        <div key={item.label} className="flex items-baseline gap-1.5">
          <span className="text-muted-foreground">{item.label}</span>
          <span
            className={cn(
              "text-[14px] font-semibold tabular-nums leading-none",
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
    <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-border/60 bg-secondary/10 px-6 py-10 text-center">
      <div className="flex max-w-sm flex-col items-center gap-3">
        <div className="flex h-11 w-11 items-center justify-center rounded-full border border-border/60 bg-card/60">
          {icon}
        </div>
        <div className="flex flex-col gap-1">
          <p className="text-[14px] font-semibold tracking-tight text-foreground">{title}</p>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">{body}</p>
        </div>
      </div>
    </div>
  )
}

export function ErrorBanner({ message }: { message: string }) {
  return (
    <p
      role="alert"
      className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-2.5 text-[12.5px] leading-[1.5] text-destructive"
    >
      <AlertTriangle className="mt-[1px] h-4 w-4 shrink-0" />
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
      className="flex items-start gap-2.5 rounded-md border border-success/30 bg-success/[0.06] px-3.5 py-2.5 text-[12.5px] leading-[1.5] text-success"
    >
      <CheckCircle2 className="mt-[1px] h-4 w-4 shrink-0" />
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
    <h4 className={cn("text-[13.5px] font-semibold tracking-tight text-foreground", className)}>
      {children}
      {count !== undefined && count !== null ? (
        <span className="ml-2 text-[12px] font-normal text-muted-foreground">{count}</span>
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
        "overflow-hidden rounded-lg border border-border/60 divide-y divide-border/40",
        className,
      )}
    >
      {children}
    </div>
  )
}
