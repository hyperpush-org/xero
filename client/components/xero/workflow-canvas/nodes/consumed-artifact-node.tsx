'use client'

import { memo, useEffect, useState } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { ChevronDown, ChevronRight, GitMerge } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { ConsumedArtifactFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'
import { useAgentCanvasExpansion } from '../expansion-context'

export const ConsumedArtifactNode = memo(function ConsumedArtifactNode({ id, data }: NodeProps<ConsumedArtifactFlowNode>) {
  const { artifact } = data
  const [expanded, setExpanded] = useState(false)
  const { setExpanded: reportExpanded } = useAgentCanvasExpansion()

  useEffect(() => {
    reportExpanded(id, expanded)
    return () => {
      reportExpanded(id, false)
    }
  }, [id, expanded, reportExpanded])

  return (
    <>
      <Handle type="source" position={Position.Right} className="!bg-teal-500 !w-2 !h-2" />
      <div
        className={cn(
          'agent-card overflow-hidden rounded-md border bg-card text-card-foreground shadow-sm',
          'border-teal-500/40 dark:border-teal-400/40',
          expanded && 'is-card-expanded',
        )}
        style={{ width: 260 }}
      >
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-muted/40"
        >
          <GitMerge className="h-3.5 w-3.5 shrink-0 text-teal-500" />
          <span className="text-[11.5px] font-medium truncate flex-1">{artifact.label}</span>
          {artifact.required ? (
            <Badge
              variant="destructive"
              className="text-[8.5px] px-1 py-0 uppercase tracking-wide"
            >
              required
            </Badge>
          ) : (
            <Badge variant="outline" className="text-[8.5px] px-1 py-0 uppercase tracking-wide">
              optional
            </Badge>
          )}
          {expanded ? (
            <ChevronDown className="agent-node-chevron h-3 w-3 text-muted-foreground" />
          ) : (
            <ChevronRight className="agent-node-chevron h-3 w-3 text-muted-foreground" />
          )}
        </button>
        <div className="agent-node-chip-row px-2.5 pb-1.5 flex items-center gap-1.5 border-t border-border/50 pt-1.5">
          <span className="text-[9px] uppercase tracking-wide text-muted-foreground/70">
            from
          </span>
          <Badge
            variant="outline"
            className="text-[9px] px-1 py-0"
          >
            {humanizeIdentifier(artifact.sourceAgent)}
          </Badge>
          <span className="text-[9px] uppercase tracking-wide text-muted-foreground/70">
            via
          </span>
          <Badge
            variant="outline"
            className="text-[9px] px-1 py-0"
          >
            {humanizeIdentifier(artifact.contract)}
          </Badge>
        </div>
        <div className={cn('agent-card-body-wrapper', expanded && 'is-open')}>
          <div className="agent-card-body">
            <div className="px-2.5 py-2 space-y-1.5 border-t border-border/50">
              {artifact.description ? (
                <p className="agent-node-detail text-[10.5px] text-muted-foreground leading-snug">
                  {artifact.description}
                </p>
              ) : null}
              {artifact.sections.length > 0 ? (
                <div className="space-y-1">
                  <p className="text-[9px] uppercase tracking-wide text-muted-foreground/70">
                    sections used
                  </p>
                  <div className="agent-node-chip-row flex flex-wrap gap-1">
                    {artifact.sections.map((section) => (
                      <span
                        key={section}
                        className="text-[9px] px-1 py-0.5 rounded border border-foreground/30 bg-foreground/10 text-foreground/80"
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
