'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { Target } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { OutputFlowNode } from '../build-agent-graph'
import { humanizeIdentifier } from '../build-agent-graph'

export const OutputNode = memo(function OutputNode({ data }: NodeProps<OutputFlowNode>) {
  const { output } = data

  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-foreground" />
      <Handle type="source" position={Position.Bottom} className="!bg-foreground" />
      <div
        className={cn('agent-card overflow-hidden text-card-foreground')}
        style={{ width: 300 }}
      >
        <div className="agent-card-tone-strip" data-tone="foreground" />
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border/40">
          <Target className="h-3.5 w-3.5 shrink-0 text-foreground/70" />
          <span className="text-[12px] font-medium truncate flex-1">{output.label}</span>
          <Badge
            variant="outline"
            className="agent-node-secondary text-[10px] px-1.5 py-0"
          >
            {humanizeIdentifier(output.contract)}
          </Badge>
        </div>
        <p className="agent-node-detail px-3 py-1.5 text-[11px] text-muted-foreground leading-snug">
          {output.description}
        </p>
      </div>
    </>
  )
})
