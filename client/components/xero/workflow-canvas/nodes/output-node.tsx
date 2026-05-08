'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { Target } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

import type { OutputFlowNode } from '../build-agent-graph'

export const OutputNode = memo(function OutputNode({ data }: NodeProps<OutputFlowNode>) {
  const { output } = data
  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-foreground !w-2 !h-2" />
      <Handle type="source" position={Position.Bottom} className="!bg-foreground !w-2 !h-2" />
      <div
        className={cn(
          'rounded-md border bg-card text-card-foreground shadow-sm',
          'border-foreground/40',
        )}
        style={{ width: 300 }}
      >
        <div className="flex items-center gap-2 px-2.5 py-2 border-b">
          <Target className="h-3.5 w-3.5 shrink-0" />
          <span className="text-[12px] font-medium truncate flex-1">{output.label}</span>
          <Badge variant="outline" className="text-[9px] font-mono px-1 py-0">
            {output.contract}
          </Badge>
        </div>
        <p className="px-2.5 py-1.5 text-[10.5px] text-muted-foreground leading-snug">
          {output.description}
        </p>
      </div>
    </>
  )
})
