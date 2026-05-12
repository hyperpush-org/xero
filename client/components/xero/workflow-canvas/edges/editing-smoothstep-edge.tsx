'use client'

import { memo } from 'react'
import {
  BaseEdge,
  Position,
  getSmoothStepPath,
  type EdgeProps,
} from '@xyflow/react'

import { useCanvasMode } from '../canvas-mode-context'
import { arrowTargetEndpoint } from './edge-endpoint-offset'

export const EditingSmoothStepEdge = memo(function EditingSmoothStepEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  label,
  labelStyle,
  labelShowBg,
  labelBgStyle,
  labelBgPadding,
  labelBgBorderRadius,
  style,
  sourcePosition = Position.Bottom,
  targetPosition = Position.Top,
  markerEnd,
  markerStart,
  pathOptions,
  interactionWidth,
}: EdgeProps) {
  const { editing } = useCanvasMode()
  const target = arrowTargetEndpoint({
    editing,
    markerEnd,
    targetX,
    targetY,
    targetPosition,
  })
  const [path, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX: target.x,
    targetY: target.y,
    targetPosition,
    borderRadius: pathOptions?.borderRadius,
    offset: pathOptions?.offset,
    stepPosition: pathOptions?.stepPosition,
  })

  return (
    <BaseEdge
      id={id}
      path={path}
      labelX={labelX}
      labelY={labelY}
      label={label}
      labelStyle={labelStyle}
      labelShowBg={labelShowBg}
      labelBgStyle={labelBgStyle}
      labelBgPadding={labelBgPadding}
      labelBgBorderRadius={labelBgBorderRadius}
      style={style}
      markerEnd={markerEnd}
      markerStart={markerStart}
      interactionWidth={interactionWidth}
    />
  )
})
