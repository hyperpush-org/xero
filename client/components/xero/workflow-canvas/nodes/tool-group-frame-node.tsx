'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'

import type { ToolGroupFrameFlowNode } from '../build-agent-graph'

export const ToolGroupFrameNode = memo(function ToolGroupFrameNode({
  data,
  width,
  height,
}: NodeProps<ToolGroupFrameFlowNode>) {
  const style: React.CSSProperties = {}
  if (typeof width === 'number') style.width = width
  if (typeof height === 'number') style.height = height

  return (
    <div className="agent-tool-group-frame" style={style}>
      {/* Left-side target handle so the header → category edge attaches to
          the frame's edge rather than to its drawn label. The handle sits at
          the vertical centre of the frame, which is where smoothstep edges
          look cleanest entering a wide block of tools. */}
      <Handle
        type="target"
        position={Position.Left}
        className="!bg-sky-500 !w-2 !h-2"
      />
      <span className="agent-tool-group-frame__drag-handle agent-tool-group-frame__label">
        <span>{data.label}</span>
        {data.count > 0 ? (
          <span className="agent-tool-group-frame__count">{data.count}</span>
        ) : null}
      </span>
    </div>
  )
})
