'use client'

import { memo } from 'react'
import type { NodeProps } from '@xyflow/react'

import type { LaneLabelFlowNode } from '../build-agent-graph'

export const LaneLabelNode = memo(function LaneLabelNode({ data, width }: NodeProps<LaneLabelFlowNode>) {
  return (
    <div
      className="agent-graph-lane-label flex items-center gap-1.5 select-none border-b border-border/40 pb-1"
      style={typeof width === 'number' ? { width } : undefined}
    >
      <span>{data.label}</span>
      {data.count > 0 ? (
        <span className="rounded-full bg-muted/70 px-1.5 py-[1px] font-mono text-[9px] leading-none tabular-nums text-muted-foreground">
          {data.count}
        </span>
      ) : null}
    </div>
  )
})
