'use client'

import { Position, type EdgeProps } from '@xyflow/react'

export const EDITING_ARROW_TARGET_CLEARANCE = 12

interface ArrowTargetEndpointInput {
  editing: boolean
  markerEnd: EdgeProps['markerEnd']
  targetX: number
  targetY: number
  targetPosition?: Position
  clearance?: number
}

export function arrowTargetEndpoint({
  editing,
  markerEnd,
  targetX,
  targetY,
  targetPosition = Position.Top,
  clearance = EDITING_ARROW_TARGET_CLEARANCE,
}: ArrowTargetEndpointInput): { x: number; y: number } {
  if (!editing || !markerEnd || clearance <= 0) return { x: targetX, y: targetY }

  switch (targetPosition) {
    case Position.Left:
      return { x: targetX - clearance, y: targetY }
    case Position.Right:
      return { x: targetX + clearance, y: targetY }
    case Position.Bottom:
      return { x: targetX, y: targetY + clearance }
    case Position.Top:
    default:
      return { x: targetX, y: targetY - clearance }
  }
}
