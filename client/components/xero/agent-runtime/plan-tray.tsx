import { useEffect, useMemo, useRef, useState } from 'react'
import { CheckCircle2, ChevronDown, ChevronRight, Circle, Loader2 } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Progress } from '@/components/ui/progress'
import { cn } from '@/lib/utils'
import type { RuntimeStreamPlanItemDto, RuntimeStreamPlanItemView } from '@/src/lib/xero-model'

interface PlanTrayProps {
  plan: RuntimeStreamPlanItemView | null
  density?: 'comfortable' | 'compact'
}

interface PlanPhaseGroup {
  key: string
  title: string | null
  completed: number
  items: { item: RuntimeStreamPlanItemDto; index: number }[]
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
  const groups = useMemo(() => groupPlanItemsByPhase(items ?? []), [items])

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
  const progressValue = counts.total > 0 ? Math.round((counts.completed / counts.total) * 100) : 0
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
          <span className="flex shrink-0 items-center gap-2">
            {!isCompact ? (
              <span className="hidden w-16 sm:block">
                <Progress value={progressValue} className="h-1.5 bg-muted" />
              </span>
            ) : null}
            {counts.inProgress > 0 ? (
              <Badge variant="secondary" className="h-5 rounded-sm bg-primary/10 px-1.5 text-[10.5px] text-primary">
                <Loader2 className="h-2.5 w-2.5 animate-spin" />
                {counts.inProgress} active
              </Badge>
            ) : null}
          </span>
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div aria-label="Plan steps" className="flex flex-col gap-1 border-t border-border/40 px-3 py-2">
          {groups.map((group) => (
            <div key={group.key} className="flex flex-col gap-0.5">
              {group.title ? (
                <div className="flex min-w-0 items-center justify-between gap-2 px-1 py-0.5">
                  <span className="min-w-0 truncate text-[11px] font-medium text-muted-foreground">
                    {group.title}
                  </span>
                  <Badge variant="outline" className="h-5 rounded-sm px-1.5 text-[10px] text-muted-foreground">
                    {group.completed}/{group.items.length}
                  </Badge>
                </div>
              ) : null}
              <ol className="flex flex-col gap-0.5" aria-label={group.title ? `${group.title} steps` : 'Plan steps'}>
                {group.items.map(({ item, index }) => (
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
                        <span className="select-none text-muted-foreground/60">
                          {item.sliceId ?? `${index + 1}.`}
                        </span>{' '}
                        {item.title}
                      </span>
                      {item.notes || item.handoffNote ? (
                        <span className="truncate text-[11px] text-muted-foreground/85">
                          {item.notes ?? item.handoffNote}
                        </span>
                      ) : null}
                    </span>
                  </li>
                ))}
              </ol>
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function groupPlanItemsByPhase(items: RuntimeStreamPlanItemDto[]): PlanPhaseGroup[] {
  const groups: PlanPhaseGroup[] = []
  const groupIndexes = new Map<string, number>()

  items.forEach((item, index) => {
    const phaseTitle = item.phaseTitle?.trim() || null
    const phaseId = item.phaseId?.trim() || null
    const key = phaseId ?? phaseTitle ?? 'flat'
    let groupIndex = groupIndexes.get(key)
    if (groupIndex == null) {
      groupIndex = groups.length
      groupIndexes.set(key, groupIndex)
      groups.push({
        key,
        title: phaseTitle,
        completed: 0,
        items: [],
      })
    }

    const group = groups[groupIndex]
    if (!group) return
    if (item.status === 'completed') {
      group.completed += 1
    }
    group.items.push({ item, index })
  })

  return groups
}
