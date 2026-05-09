'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, Layers } from 'lucide-react'

import { cn } from '@/lib/utils'

import type { OutputSectionFlowNode } from '../build-agent-graph'
import { AGENT_GRAPH_TRIGGER_HANDLES, humanizeIdentifier } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

const TRIGGER_HANDLE_CLASS = '!bg-fuchsia-500'

const EMPHASIS_DOT: Record<string, string> = {
  core: 'bg-foreground',
  standard: 'bg-foreground/50',
  optional: 'bg-foreground/25',
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

  const hasDetail = Boolean(section.description) || section.producedByTools.length > 0

  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-foreground" />
      <Handle
        id={AGENT_GRAPH_TRIGGER_HANDLES.target}
        type="target"
        position={Position.Left}
        className={TRIGGER_HANDLE_CLASS}
      />
      <Handle
        id={AGENT_GRAPH_TRIGGER_HANDLES.source}
        type="source"
        position={Position.Right}
        className={TRIGGER_HANDLE_CLASS}
      />
      <div
        className={cn(
          'agent-card overflow-hidden text-card-foreground',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 200 }}
      >
        <div className="agent-card-tone-strip" data-tone="foreground" />
        <button
          type="button"
          onClick={() => {
            if (locked || !hasDetail) return
            setExpanded((v) => !v)
          }}
          disabled={locked || !hasDetail}
          aria-expanded={expanded}
          className={cn(
            'flex w-full items-center gap-2 px-3 py-2 text-left',
            hasDetail && !locked && 'nodrag nopan hover:bg-muted/40 cursor-pointer',
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
          <span className="text-[12px] font-medium truncate flex-1">
            {section.label}
          </span>
          <span className="agent-node-secondary text-[10px] uppercase tracking-wide text-muted-foreground/70">
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
            <div className="px-3 pb-2 pt-1.5 space-y-1.5 border-t border-border/50">
              {section.description ? (
                <p className="agent-node-detail text-[11px] text-muted-foreground leading-snug">
                  {section.description}
                </p>
              ) : null}
              {section.producedByTools.length > 0 ? (
                <div className="agent-node-chip-row flex flex-wrap gap-1.5">
                  <span className="text-[10px] uppercase tracking-wide text-muted-foreground/70">
                    from
                  </span>
                  {section.producedByTools.map((tool) => (
                    <span
                      key={tool}
                      className="text-[10px] px-1.5 py-0.5 rounded border border-sky-500/30 bg-sky-500/15 text-sky-700 dark:text-sky-300"
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
