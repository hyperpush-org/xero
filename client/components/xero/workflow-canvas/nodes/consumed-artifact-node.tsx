'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, GitMerge } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { ConsumedArtifactFlowNode } from '../build-agent-graph'
import { AGENT_GRAPH_TRIGGER_HANDLES, humanizeIdentifier } from '../build-agent-graph'
import { useCanvasMode } from '../canvas-mode-context'
import { useAgentCanvasExpansion } from '../expansion-context'

const TRIGGER_HANDLE_CLASS = '!bg-fuchsia-500'

export const ConsumedArtifactNode = memo(function ConsumedArtifactNode({ id, data }: NodeProps<ConsumedArtifactFlowNode>) {
  const { artifact } = data
  const { editing } = useCanvasMode()
  const [expanded, setExpanded] = useState(false)
  const { locked, setExpanded: reportExpanded } = useAgentCanvasExpansion()
  const isPlaceholder = !artifact.id || /^artifact_\d+$/.test(artifact.id)

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle type="source" position={Position.Right} className="!bg-teal-500" />
      <Handle
        id={AGENT_GRAPH_TRIGGER_HANDLES.source}
        type="source"
        position={Position.Right}
        className={TRIGGER_HANDLE_CLASS}
        style={{ top: '72%' }}
      />
      <div
        className={cn(
          'agent-card overflow-hidden text-card-foreground',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 260 }}
      >
        <div className="agent-card-tone-strip" data-tone="teal" />
        <button
          type="button"
          onClick={() => {
            if (locked) return
            setExpanded((v) => !v)
          }}
          disabled={locked}
          aria-expanded={expanded}
          className="nodrag nopan flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-muted/40"
        >
          <GitMerge className="h-3.5 w-3.5 shrink-0 text-teal-500" />
          <span className="text-[12px] font-medium truncate flex-1">
            {isPlaceholder
              ? editing
                ? 'Choose an artifact'
                : 'Untitled artifact'
              : artifact.label}
          </span>
          {artifact.required ? (
            <Badge
              variant="secondary"
              className="text-[10px] px-1.5 py-0 uppercase tracking-wide"
            >
              required
            </Badge>
          ) : null}
          {expanded ? (
            <ChevronDown className="agent-node-chevron h-3 w-3 text-muted-foreground" />
          ) : (
            <ChevronRight className="agent-node-chevron h-3 w-3 text-muted-foreground" />
          )}
        </button>
        <div className="agent-node-chip-row px-3 pb-1.5 flex items-center gap-1.5 border-t border-border/50 pt-1.5">
          <span className="text-[10px] uppercase tracking-wide text-muted-foreground/70">
            from
          </span>
          <Badge
            variant="outline"
            className="text-[10px] px-1.5 py-0"
          >
            {humanizeIdentifier(artifact.sourceAgent)}
          </Badge>
          <span className="text-[10px] uppercase tracking-wide text-muted-foreground/70">
            via
          </span>
          <Badge
            variant="outline"
            className="text-[10px] px-1.5 py-0"
          >
            {humanizeIdentifier(artifact.contract)}
          </Badge>
        </div>
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-3 py-2 space-y-1.5 border-t border-border/50">
              {artifact.description ? (
                <p className="agent-node-detail text-[11px] text-muted-foreground leading-snug">
                  {artifact.description}
                </p>
              ) : null}
              {artifact.sections.length > 0 ? (
                <div className="space-y-1">
                  <p className="text-[10px] uppercase tracking-wide text-muted-foreground/70">
                    sections used
                  </p>
                  <div className="agent-node-chip-row flex flex-wrap gap-1.5">
                    {artifact.sections.map((section) => (
                      <span
                        key={section}
                        className="text-[10px] px-1.5 py-0.5 rounded border border-foreground/30 bg-foreground/10 text-foreground/80"
                      >
                        {humanizeIdentifier(section)}
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </>
  )
})
