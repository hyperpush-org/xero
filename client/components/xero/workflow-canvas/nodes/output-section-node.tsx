'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, Layers } from 'lucide-react'

import { cn } from '@/lib/utils'

import type { OutputSectionFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const EMPHASIS_DOT: Record<string, string> = {
  core: 'bg-foreground',
  standard: 'bg-foreground/50',
  optional: 'bg-foreground/25',
}

const EMPHASIS_BORDER: Record<string, string> = {
  core: 'border-foreground/55',
  standard: 'border-foreground/30',
  optional: 'border-foreground/20',
}

const EMPHASIS_LABEL: Record<string, string> = {
  core: 'core',
  standard: 'std',
  optional: 'opt',
}

export const OutputSectionNode = memo(function OutputSectionNode({ id, data }: NodeProps<OutputSectionFlowNode>) {
  const { section } = data
  const [expanded, setExpanded] = useState(false)
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  const hasDetail =
    Boolean(section.description) || section.producedByTools.length > 0

  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-foreground !w-2 !h-2" />
      <Handle type="source" position={Position.Right} className="!bg-foreground !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          EMPHASIS_BORDER[section.emphasis] ?? 'border-foreground/30',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 200 }}
      >
        <button
          type="button"
          onClick={() => {
            if (locked || !hasDetail) return
            setExpanded((v) => !v)
          }}
          disabled={locked || !hasDetail}
          className={cn(
            'flex w-full items-center gap-2 px-2 py-1.5 text-left',
            hasDetail && !locked && 'hover:bg-muted/40 cursor-pointer',
          )}
        >
          <span
            aria-hidden="true"
            className={cn(
              'h-2 w-2 shrink-0 rounded-full',
              EMPHASIS_DOT[section.emphasis] ?? 'bg-foreground/30',
            )}
          />
          <Layers className="h-3 w-3 shrink-0 text-muted-foreground" />
          <span className="text-[11px] font-medium truncate flex-1">
            {section.label}
          </span>
          <span className="agent-node-secondary text-[8.5px] uppercase tracking-wide text-muted-foreground/70">
            {EMPHASIS_LABEL[section.emphasis] ?? section.emphasis}
          </span>
          {hasDetail ? (
            expanded ? (
              <ChevronDown className="agent-node-chevron h-3 w-3 text-muted-foreground" />
            ) : (
              <ChevronRight className="agent-node-chevron h-3 w-3 text-muted-foreground" />
            )
          ) : null}
        </button>
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-2 pb-2 pt-1 space-y-1.5 border-t border-border/50">
              {section.description ? (
                <p className="agent-node-detail text-[10px] text-muted-foreground leading-snug">
                  {section.description}
                </p>
              ) : null}
              {section.producedByTools.length > 0 ? (
                <div className="agent-node-chip-row flex flex-wrap gap-1">
                  <span className="text-[9px] uppercase tracking-wide text-muted-foreground/70">
                    from
                  </span>
                  {section.producedByTools.map((tool) => (
                    <span
                      key={tool}
                      className="text-[9px] px-1 py-0.5 rounded border border-sky-500/30 bg-sky-500/15 text-sky-700 dark:text-sky-300"
                    >
                      {humanizeIdentifier(tool)}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </>
  )
})
