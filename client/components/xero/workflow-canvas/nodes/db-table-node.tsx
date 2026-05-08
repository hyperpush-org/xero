'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  ChevronDown,
  ChevronRight,
  Database,
  Sparkles,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import type { AgentTriggerRefDto } from '@/src/lib/xero-model/workflow-agents'

import type { DbTableFlowNode, DbTableTouchpointKind } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const TOUCHPOINT_ICON: Record<DbTableTouchpointKind, typeof ArrowDownToLine> = {
  read: ArrowDownToLine,
  write: ArrowUpFromLine,
  encouraged: Sparkles,
}

const TOUCHPOINT_DOT: Record<DbTableTouchpointKind, string> = {
  read: 'bg-emerald-500',
  write: 'bg-rose-500',
  encouraged: 'bg-violet-400',
}

const TOUCHPOINT_BORDER: Record<DbTableTouchpointKind, string> = {
  read: 'border-emerald-500/30 dark:border-emerald-400/30',
  write: 'border-rose-500/40 dark:border-rose-400/40',
  encouraged: 'border-violet-500/30 dark:border-violet-400/30',
}

const TOUCHPOINT_TITLE: Record<DbTableTouchpointKind, string> = {
  read: 'Reads',
  write: 'Writes',
  encouraged: 'Encouraged write',
}

function triggerChipLabel(trigger: AgentTriggerRefDto): { label: string; tone: 'tool' | 'section' | 'lifecycle' | 'upstream' } {
  switch (trigger.kind) {
    case 'tool':
      return { label: `Tool: ${trigger.name}`, tone: 'tool' }
    case 'output_section':
      return { label: `Section: ${trigger.id}`, tone: 'section' }
    case 'lifecycle':
      return { label: `Lifecycle: ${trigger.event}`, tone: 'lifecycle' }
    case 'upstream_artifact':
      return { label: `Upstream: ${trigger.id}`, tone: 'upstream' }
  }
}

const TONE_CLASS: Record<'tool' | 'section' | 'lifecycle' | 'upstream', string> = {
  tool: 'bg-sky-500/15 text-sky-700 dark:text-sky-300 border-sky-500/30',
  section: 'bg-foreground/10 text-foreground/80 border-foreground/20',
  lifecycle: 'bg-muted/40 text-muted-foreground border-border/50',
  upstream: 'bg-teal-500/15 text-teal-700 dark:text-teal-300 border-teal-500/30',
}

const COLLAPSED_TRIGGER_LIMIT = 2

export const DbTableNode = memo(function DbTableNode({ id, data }: NodeProps<DbTableFlowNode>) {
  const { table, touchpoint, purpose, triggers, columns } = data
  const [expanded, setExpanded] = useState(false)
  const { setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  const Icon = TOUCHPOINT_ICON[touchpoint]
  const visibleTriggers = expanded ? triggers : triggers.slice(0, COLLAPSED_TRIGGER_LIMIT)
  const hiddenCount = Math.max(0, triggers.length - visibleTriggers.length)

  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-emerald-500 !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          TOUCHPOINT_BORDER[touchpoint],
          expanded && 'is-card-expanded',
        )}
        style={{ width: 260 }}
        title={`${TOUCHPOINT_TITLE[touchpoint]}: ${table}`}
      >
        <div className="flex items-center gap-2 px-2.5 py-1.5">
          <span
            aria-hidden="true"
            className={cn('h-2 w-2 shrink-0 rounded-full', TOUCHPOINT_DOT[touchpoint])}
          />
          <Database className="h-3 w-3 shrink-0 text-muted-foreground" />
          <span className="font-mono text-[11px] truncate flex-1">{table}</span>
          <Icon className="h-3 w-3 shrink-0 text-muted-foreground" />
        </div>
        {purpose ? (
          <p className="px-2.5 pb-1.5 text-[10px] text-muted-foreground leading-snug">
            {purpose}
          </p>
        ) : null}
        {triggers.length > 0 ? (
          <div className="px-2.5 pb-1.5 flex flex-wrap gap-1">
            {visibleTriggers.map((trigger, idx) => {
              const chip = triggerChipLabel(trigger)
              return (
                <span
                  key={`${chip.label}:${idx}`}
                  className={cn(
                    'text-[9px] px-1 py-0.5 rounded border font-mono leading-tight',
                    TONE_CLASS[chip.tone],
                  )}
                >
                  {chip.label}
                </span>
              )
            })}
            {hiddenCount > 0 ? (
              <button
                type="button"
                onClick={() => setExpanded(true)}
                className="text-[9px] px-1 py-0.5 rounded border border-dashed border-border/60 text-muted-foreground hover:bg-muted/40"
              >
                +{hiddenCount} more
              </button>
            ) : null}
          </div>
        ) : null}
        {expanded && columns.length > 0 ? (
          <div className="px-2.5 pb-1.5 border-t border-border/50 pt-1">
            <p className="text-[9px] uppercase tracking-wide text-muted-foreground/70 mb-0.5">
              columns
            </p>
            <p className="text-[10px] font-mono text-foreground/80 leading-snug break-words">
              {columns.join(', ')}
            </p>
          </div>
        ) : null}
        {triggers.length > COLLAPSED_TRIGGER_LIMIT || columns.length > 0 ? (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="agent-card-base flex w-full items-center gap-1 px-2.5 py-1 text-left text-[10px] text-muted-foreground hover:bg-muted/40 border-t border-border/50"
          >
            {expanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
            <span>{expanded ? 'Less' : 'More'}</span>
          </button>
        ) : null}
      </div>
    </>
  )
})
