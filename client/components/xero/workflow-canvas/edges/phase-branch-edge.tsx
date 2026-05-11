'use client'

import { memo } from 'react'
import {
  BaseEdge,
  EdgeLabelRenderer,
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

  const [edgePath, labelX, labelY] = getBezierPath({
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
