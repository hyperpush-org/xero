'use client'

import { memo } from 'react'
import {
  BaseEdge,
  EdgeLabelRenderer,
  Position,
  getBezierPath,
  type EdgeProps,
} from '@xyflow/react'

import type { CustomAgentWorkflowBranchConditionDto } from '@/src/lib/xero-model/agent-definition'

interface PhaseBranchEdgeData extends Record<string, unknown> {
  condition?: CustomAgentWorkflowBranchConditionDto
  label?: string
}

function describeCondition(condition: CustomAgentWorkflowBranchConditionDto | undefined): string {
  if (!condition) return 'when'
  switch (condition.kind) {
    case 'always':
      return 'always'
    case 'todo_completed':
      return `todo: ${condition.todoId}`
    case 'tool_succeeded': {
      const count = condition.minCount && condition.minCount > 1 ? ` × ${condition.minCount}` : ''
      return `tool: ${condition.toolName}${count}`
    }
  }
}

// When source and target sit on the SAME side (both Left, both Right) and are
// roughly aligned on that axis, React Flow's stock bezier collapses to a
// near-straight line because its curvature formula scales off the
// cross-axis distance. Stages stack vertically with both handles on the
// left, so we need an explicit arc.
//
// Returns a cubic bezier path that bows away from the shared side by a
// distance proportional to the gap between the two endpoints, plus the
// midpoint coordinates for the label.
function getSameSideArc(
  sourceX: number,
  sourceY: number,
  targetX: number,
  targetY: number,
  side: Position,
): [string, number, number] {
  const dy = targetY - sourceY
  // Scale the bow with the vertical gap so adjacent cards get a tight arc
  // and distant ones get a fatter sweep. Floor + ceiling keep tiny/huge
  // edges legible.
  const arc = Math.max(48, Math.min(160, Math.abs(dy) * 0.45))
  const direction = side === Position.Left ? -1 : 1
  const c1x = sourceX + arc * direction
  const c2x = targetX + arc * direction
  const c1y = sourceY + dy * 0.25
  const c2y = sourceY + dy * 0.75
  const path = `M ${sourceX},${sourceY} C ${c1x},${c1y} ${c2x},${c2y} ${targetX},${targetY}`
  // Bezier midpoint (t=0.5) gives a good label anchor sitting on the curve's
  // outer apex.
  const labelX = (sourceX + 3 * c1x + 3 * c2x + targetX) / 8
  const labelY = (sourceY + 3 * c1y + 3 * c2y + targetY) / 8
  return [path, labelX, labelY]
}

export const PhaseBranchEdge = memo(function PhaseBranchEdge(props: EdgeProps) {
  const {
    id,
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    markerEnd,
    style,
    interactionWidth,
    data,
  } = props
  const branchData = (data ?? undefined) as PhaseBranchEdgeData | undefined
  const label = branchData?.label?.trim() || describeCondition(branchData?.condition)

  const sameSide =
    (sourcePosition === Position.Left && targetPosition === Position.Left) ||
    (sourcePosition === Position.Right && targetPosition === Position.Right)

  const [edgePath, labelX, labelY] = sameSide
    ? getSameSideArc(sourceX, sourceY, targetX, targetY, sourcePosition)
    : getBezierPath({
        sourceX,
        sourceY,
        sourcePosition,
        targetX,
        targetY,
        targetPosition,
      })

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={style}
        interactionWidth={interactionWidth}
      />
      <EdgeLabelRenderer>
        <div
          data-edge-id={id}
          className="agent-edge-phase-branch-label"
          style={{
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
          }}
        >
          {label}
        </div>
      </EdgeLabelRenderer>
    </>
  )
})
