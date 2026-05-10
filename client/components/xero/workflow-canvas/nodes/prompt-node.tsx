'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { FileText } from 'lucide-react'

import { Badge } from '@/components/ui/badge'

import type { PromptFlowNode } from '../build-agent-graph'

const ROLE_LABEL: Record<'system' | 'developer' | 'task', string> = {
  system: 'System',
  developer: 'Developer',
  task: 'Task',
}

export const PromptNode = memo(function PromptNode({ data }: NodeProps<PromptFlowNode>) {
  const { prompt } = data
  const tokenEstimate = Math.ceil(prompt.body.length / 4)

  return (
    <>
      <Handle type="target" position={Position.Bottom} className="!bg-amber-500" />
      <div className="agent-card overflow-hidden text-card-foreground" style={{ width: 300 }}>
        <div className="agent-card-tone-strip" data-tone="amber" />
        <div className="agent-card-base flex w-full items-center gap-2 px-3 py-2 text-left">
          <FileText className="h-3 w-3 shrink-0 text-amber-500/80" />
          <span className="text-[12px] truncate flex-1 font-medium text-foreground/95">{prompt.label}</span>
          <Badge variant="outline" className="text-[10px] px-1.5 py-0 font-medium">
            {ROLE_LABEL[prompt.role]}
          </Badge>
          <span className="agent-node-secondary text-[10px] text-muted-foreground/80 tabular-nums">~{tokenEstimate}t</span>
        </div>
      </div>
    </>
  )
})
