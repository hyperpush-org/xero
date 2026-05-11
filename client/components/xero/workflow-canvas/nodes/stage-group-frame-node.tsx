'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'

import type { StageGroupFrameFlowNode } from '../build-agent-graph'

export const StageGroupFrameNode = memo(function StageGroupFrameNode({
  width,
  height,
}: NodeProps<StageGroupFrameFlowNode>) {
  const style: React.CSSProperties = {}
  if (typeof width === 'number') style.width = width
  if (typeof height === 'number') style.height = height

  return (
    <div className="agent-stage-group-frame" style={style}>
      {/* Right-side target handle so the agent header → stages edge attaches
          to the frame's edge rather than to a specific stage card. The
          STAGES column lives on the left of the agent, so the right edge of
          the frame is what the edge hits. */}
      <Handle
        id="workflow"
        type="target"
        position={Position.Right}
        className="!bg-amber-500"
      />
    </div>
  )
})
