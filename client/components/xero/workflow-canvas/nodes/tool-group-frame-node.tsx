'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { GripVertical, Trash2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import type { ToolGroupFrameFlowNode } from '../build-agent-graph'
import { useCanvasMode } from '../canvas-mode-context'

export const ToolGroupFrameNode = memo(function ToolGroupFrameNode({
  data,
  width,
  height,
}: NodeProps<ToolGroupFrameFlowNode>) {
  const { editing, removeToolGroup } = useCanvasMode()
  const style: React.CSSProperties = {}
  if (typeof width === 'number') style.width = width
  if (typeof height === 'number') style.height = height
  const sourceGroups = Array.isArray(data.sourceGroups) ? data.sourceGroups : []
  const removeLabel = `Remove ${data.label} tool category`

  return (
    <div className="agent-tool-group-frame" style={style}>
      {/* Left-side target handle so the header → category edge attaches to
          the frame's edge rather than to its drawn label. The handle sits at
          the vertical centre of the frame, which is where smoothstep edges
          look cleanest entering a wide block of tools. */}
      <Handle
        type="target"
        position={Position.Left}
        className="!bg-sky-500"
      />
      <span
        className="agent-tool-group-frame__drag-handle agent-tool-group-frame__label"
      >
        <GripVertical
          aria-hidden="true"
          className="agent-tool-group-frame__grip"
        />
        <span className="agent-tool-group-frame__label-text">{data.label}</span>
        {data.count > 0 ? (
          <span className="agent-tool-group-frame__count">{data.count}</span>
        ) : null}
        {editing && sourceGroups.length > 0 ? (
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={removeLabel}
            title={removeLabel}
            className="agent-tool-group-frame__remove size-5 rounded-full p-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
            onPointerDown={(event) => {
              event.stopPropagation()
            }}
            onClick={(event) => {
              event.stopPropagation()
              removeToolGroup(sourceGroups)
            }}
          >
            <Trash2 aria-hidden="true" />
          </Button>
        ) : null}
      </span>
    </div>
  )
})
