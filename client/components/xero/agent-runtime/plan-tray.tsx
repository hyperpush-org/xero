import { useEffect, useMemo, useRef, useState } from 'react'
import { CheckCircle2, ChevronDown, ChevronRight, Circle, Loader2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
import type { RuntimeStreamPlanItemView } from '@/src/lib/xero-model'

interface PlanTrayProps {
  plan: RuntimeStreamPlanItemView | null
  density?: 'comfortable' | 'compact'
}

export function PlanTray({ plan, density = 'comfortable' }: PlanTrayProps) {
  const [open, setOpen] = useState(false)
  const userToggledRef = useRef(false)
  const lastPlanIdRef = useRef<string | null>(null)
  const lastInProgressCountRef = useRef(0)

  const items = plan?.items ?? null
  const counts = useMemo(() => {
    if (!items) return null
    let pending = 0
    let inProgress = 0
    let completed = 0
    for (const item of items) {
      if (item.status === 'pending') pending += 1
      else if (item.status === 'in_progress') inProgress += 1
      else if (item.status === 'completed') completed += 1
    }
    return { pending, inProgress, completed, total: items.length }
  }, [items])

  useEffect(() => {
    if (!plan) {
      userToggledRef.current = false
      lastPlanIdRef.current = null
      lastInProgressCountRef.current = 0
      setOpen(false)
      return
    }

    if (lastPlanIdRef.current !== plan.planId) {
      userToggledRef.current = false
      lastPlanIdRef.current = plan.planId
      lastInProgressCountRef.current = 0
    }

    if (!counts || userToggledRef.current) return

    if (counts.inProgress > 0 && lastInProgressCountRef.current === 0) {
      setOpen(true)
    }
    lastInProgressCountRef.current = counts.inProgress
  }, [plan, counts])

  if (!plan || !items || items.length === 0 || !counts) {
    return null
  }

  const isCompact = density === 'compact'
  const inProgressItem = items.find((item) => item.status === 'in_progress') ?? null
  const summaryHead = `Plan · ${counts.completed}/${counts.total}`
  const summaryDetail =
    inProgressItem ? `Currently: ${inProgressItem.title}` : counts.pending === 0 ? 'All steps complete' : null

  const handleToggle = (next: boolean) => {
    userToggledRef.current = true
    setOpen(next)
  }

  return (
    <Collapsible
      open={open}
      onOpenChange={handleToggle}
      className={cn(
        'shrink-0 border-t border-border/60 bg-card/85 supports-[backdrop-filter]:bg-card/70',
        'motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1',
      )}
    >
      <CollapsibleTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          className={cn(
            'flex w-full items-center justify-between gap-2 rounded-none px-3 py-1.5',
            'text-[12px] font-medium text-foreground/85 hover:bg-muted/40',
          )}
          aria-label={open ? 'Collapse plan tray' : 'Expand plan tray'}
        >
          <span className="flex min-w-0 items-center gap-1.5">
            {open ? (
              <ChevronDown aria-hidden="true" className="h-3 w-3 text-muted-foreground" />
            ) : (
              <ChevronRight aria-hidden="true" className="h-3 w-3 text-muted-foreground" />
            )}
            <span className="shrink-0 text-foreground">{summaryHead}</span>
            {!isCompact && summaryDetail ? (
              <span className="min-w-0 truncate text-muted-foreground">· {summaryDetail}</span>
            ) : null}
          </span>
          {counts.inProgress > 0 ? (
            <span className="inline-flex items-center gap-1 rounded-sm bg-primary/10 px-1.5 py-0.5 text-[10.5px] font-medium text-primary">
              <Loader2 className="h-2.5 w-2.5 animate-spin" />
              {counts.inProgress} active
            </span>
          ) : null}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ol
          aria-label="Plan steps"
          className="flex flex-col gap-0.5 border-t border-border/40 px-3 py-2"
        >
          {items.map((item, index) => (
            <li
              key={item.id}
              className={cn(
                'flex items-start gap-2 rounded-sm px-1 py-0.5 text-[12px]',
                item.status === 'in_progress' ? 'text-foreground' : null,
                item.status === 'completed' ? 'text-muted-foreground' : null,
                item.status === 'pending' ? 'text-foreground/85' : null,
              )}
            >
              <span className="mt-0.5 shrink-0">
                {item.status === 'completed' ? (
                  <CheckCircle2 className="h-3 w-3 text-success" />
                ) : item.status === 'in_progress' ? (
                  <Loader2 className="h-3 w-3 animate-spin text-primary" />
                ) : (
                  <Circle className="h-3 w-3 text-muted-foreground/60" />
                )}
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span
                  className={cn(
                    'truncate',
                    item.status === 'completed' ? 'line-through decoration-muted-foreground/40' : null,
                  )}
                >
                  <span className="select-none text-muted-foreground/60">{index + 1}.</span>{' '}
                  {item.title}
                </span>
                {item.notes ? (
                  <span className="truncate text-[11px] text-muted-foreground/85">{item.notes}</span>
                ) : null}
              </span>
            </li>
          ))}
        </ol>
      </CollapsibleContent>
    </Collapsible>
  )
}
