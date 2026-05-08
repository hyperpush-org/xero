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

import { Clock, GitMerge as ArtifactIcon } from 'lucide-react'

import { cn } from '@/lib/utils'
import type { AgentTriggerRefDto } from '@/src/lib/xero-model/workflow-agents'

import type { DbTableFlowNode, DbTableTouchpointKind } from '../build-agent-graph'
import { humanizeIdentifier, lifecycleEventLabel } from '../build-agent-graph'
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

// Tool and output-section triggers also render as edges into this card, so
// the chips here duplicate spatial info. We still keep them as in-card chips
// because the *name* (which tool, which section) is more useful in place
// than chasing the edge to its source. Lifecycle and upstream-artifact
// triggers render *only* as in-card chips — emitting them as edges would
// create the spaghetti the read-mode inspector is meant to avoid.
type ChipTone = 'tool' | 'section' | 'lifecycle' | 'upstream'

interface TriggerChip {
  label: string
  tone: ChipTone
  icon?: typeof Clock
}

function triggerChipLabel(trigger: AgentTriggerRefDto): TriggerChip | null {
  switch (trigger.kind) {
    case 'tool':
      return { label: `Tool: ${humanizeIdentifier(trigger.name)}`, tone: 'tool' }
    case 'output_section':
      return { label: `Section: ${humanizeIdentifier(trigger.id)}`, tone: 'section' }
    case 'lifecycle':
      return {
        label: `on ${lifecycleEventLabel(trigger.event)}`,
        tone: 'lifecycle',
        icon: Clock,
      }
    case 'upstream_artifact':
      return {
        label: `from ${humanizeIdentifier(trigger.id)}`,
        tone: 'upstream',
        icon: ArtifactIcon,
      }
  }
}

const TONE_CLASS: Record<ChipTone, string> = {
  tool: 'bg-sky-500/15 text-sky-700 dark:text-sky-300 border-sky-500/30',
  section: 'bg-foreground/10 text-foreground/80 border-foreground/20',
  lifecycle: 'bg-fuchsia-500/12 text-fuchsia-700 dark:text-fuchsia-300 border-fuchsia-500/30',
  upstream: 'bg-teal-500/15 text-teal-700 dark:text-teal-300 border-teal-500/30',
}

// When collapsed, show this many chips inline. Lifecycle chips dominate on
// busy agents (Engineer's code_history_operations has ~7 lifecycle events),
// so the limit was bumped from 2 → 3 to give the user a useful preview
// before they need to click "More".
const COLLAPSED_TRIGGER_LIMIT = 3

export const DbTableNode = memo(function DbTableNode({ id, data }: NodeProps<DbTableFlowNode>) {
  const { table, touchpoint, purpose, triggers, columns } = data
  const [expanded, setExpanded] = useState(false)
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()
  const canExpand = columns.length > 0
  const isExpanded = canExpand && expanded

  useEffect(() => {
    reportExpanded(id, isExpanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, isExpanded, reportExpanded])

  const Icon = TOUCHPOINT_ICON[touchpoint]
  const displayTable = humanizeIdentifier(table)
  const chipTriggers = triggers
    .map(triggerChipLabel)
    .filter((chip): chip is TriggerChip => chip !== null)
  const visibleChips =
    canExpand && !isExpanded ? chipTriggers.slice(0, COLLAPSED_TRIGGER_LIMIT) : chipTriggers
  const hiddenCount = canExpand ? Math.max(0, chipTriggers.length - visibleChips.length) : 0

  return (
    <>
      <Handle type="target" position={Position.Left} className="!bg-emerald-500 !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          TOUCHPOINT_BORDER[touchpoint],
          isExpanded && 'is-card-expanded',
        )}
        style={{ width: 260 }}
      >
        <div className="flex items-center gap-2 px-2.5 py-1.5">
          <span
            aria-hidden="true"
            className={cn('h-2 w-2 shrink-0 rounded-full', TOUCHPOINT_DOT[touchpoint])}
          />
          <Database className="h-3 w-3 shrink-0 text-muted-foreground" />
          <span className="text-[11px] truncate flex-1 font-medium">
            {displayTable}
          </span>
          <Icon className="h-3 w-3 shrink-0 text-muted-foreground" />
        </div>
        {purpose ? (
          <p className="agent-node-detail px-2.5 pb-1.5 text-[10px] text-muted-foreground leading-snug">
            {purpose}
          </p>
        ) : null}
        {chipTriggers.length > 0 ? (
          <div className="agent-node-chip-row px-2.5 pb-1.5 flex flex-wrap gap-1">
            {visibleChips.map((chip, idx) => {
              const ChipIcon = chip.icon
              return (
                <span
                  key={`${chip.label}:${idx}`}
                  className={cn(
                    'inline-flex items-center gap-1 text-[9px] px-1 py-0.5 rounded border font-mono leading-tight',
                    TONE_CLASS[chip.tone],
                  )}
                >
                  {ChipIcon ? (
                    <ChipIcon
                      aria-hidden="true"
                      className="h-2.5 w-2.5 shrink-0"
                    />
                  ) : null}
                  <span>{chip.label}</span>
                </span>
              )
            })}
            {hiddenCount > 0 ? (
              <button
                type="button"
                onClick={() => {
                  if (locked) return
                  setExpanded(true)
                }}
                disabled={locked}
                className="text-[9px] px-1 py-0.5 rounded border border-dashed border-border/60 text-muted-foreground hover:bg-muted/40"
              >
                +{hiddenCount} more
              </button>
            ) : null}
          </div>
        ) : null}
        {isExpanded ? (
          <div className="agent-node-detail px-2.5 pb-1.5 border-t border-border/50 pt-1">
            <p className="text-[9px] uppercase tracking-wide text-muted-foreground/70 mb-0.5">
              columns
            </p>
            <p className="text-[10px] font-mono text-foreground/80 leading-snug break-words">
              {columns.join(', ')}
            </p>
          </div>
        ) : null}
        {canExpand ? (
          <button
            type="button"
            onClick={() => {
              if (locked) return
              setExpanded((v) => !v)
            }}
            disabled={locked}
            className="agent-card-base flex w-full items-center gap-1 px-2.5 py-1 text-left text-[10px] text-muted-foreground hover:bg-muted/40 border-t border-border/50"
          >
            {isExpanded ? (
              <ChevronDown className="agent-node-chevron h-3 w-3" />
            ) : (
              <ChevronRight className="agent-node-chevron h-3 w-3" />
            )}
            <span>{isExpanded ? 'Less' : 'More'}</span>
          </button>
        ) : null}
      </div>
    </>
  )
})
