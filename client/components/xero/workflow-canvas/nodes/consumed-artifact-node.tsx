'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { GitMerge } from 'lucide-react'

import { Badge } from '@/components/ui/badge'

import type { ConsumedArtifactFlowNode } from '../build-agent-graph'
import { AGENT_GRAPH_TRIGGER_HANDLES, humanizeIdentifier } from '../build-agent-graph'
import { useCanvasMode } from '../canvas-mode-context'

const TRIGGER_HANDLE_CLASS = '!bg-fuchsia-500'

export const ConsumedArtifactNode = memo(function ConsumedArtifactNode({ data }: NodeProps<ConsumedArtifactFlowNode>) {
  const { artifact } = data
  const { editing } = useCanvasMode()
  const isPlaceholder = !artifact.id || /^artifact_\d+$/.test(artifact.id)

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
      <div className="agent-card overflow-hidden text-card-foreground" style={{ width: 260 }}>
        <div className="agent-card-tone-strip" data-tone="teal" />
        <div className="flex w-full items-center gap-2 px-3 py-2 text-left">
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
        </div>
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
      </div>
    </>
  )
})
