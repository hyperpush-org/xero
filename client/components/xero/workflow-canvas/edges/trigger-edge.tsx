'use client'

import { memo, useMemo } from 'react'
import {
  BaseEdge,
  EdgeLabelRenderer,
  Position,
  getBezierPath,
  useStore,
  type EdgeProps,
  type ReactFlowState,
} from '@xyflow/react'

interface TriggerEdgeData extends Record<string, unknown> {
  triggerLabel?: string
  suppressLabel?: boolean
}

// Approximate half-extents of a rendered trigger label pill. We don't measure
// the label after render to avoid a layout-after-render reflow loop; the
// pill is short ("writes", "reads", "produces", "encouraged") and pads to
// roughly 50–60px wide × 18px tall, so 30×10 half-extents cover the
// worst case with a margin.
const LABEL_HALF_W = 30
const LABEL_HALF_H = 10
const EDGE_COLLISION_MARGIN = 180

// Sample positions along the bezier curve. Index 0 (midpoint) is React
// Flow's natural label position; subsequent indices alternate either side
// of the midpoint and progressively further toward source/target so a
// label that overlaps a node migrates along the *curve* until clear. Bias
// slightly toward the source ("shift to the left a bit") because trigger
// edges typically pass through their target's neighbourhood and the
// overlap is usually with a sibling card just outside the target.
const SAMPLE_T_VALUES = [
  0.5,
  0.42,
  0.58,
  0.34,
  0.66,
  0.26,
  0.74,
  0.2,
  0.8,
  0.14,
  0.86,
]

// React Flow's default curvature for getBezierPath is 0.25. We replicate
// the same control-point math here so sampled positions match the rendered
// curve exactly.
const BEZIER_CURVATURE = 0.25

// Node types that don't visually block the canvas — lane labels are slim
// header strips and tool-group frames are mostly empty space around their
// child tool cards. Treat them as transparent for collision detection so a
// label centred over a frame's border isn't pushed away unnecessarily.
const EXCLUDED_TYPES = new Set(['lane-label', 'tool-group-frame'])

function selectNodeLookup(state: ReactFlowState) {
  return state.nodeLookup
}

function labelOverlapsRect(
  px: number,
  py: number,
  x: number,
  y: number,
  w: number,
  h: number,
): boolean {
  return (
    px + LABEL_HALF_W >= x &&
    px - LABEL_HALF_W <= x + w &&
    py + LABEL_HALF_H >= y &&
    py - LABEL_HALF_H <= y + h
  )
}

// Mirrors @xyflow/system's calculateControlOffset so our sampled bezier
// matches the path React Flow actually draws.
function calculateControlOffset(distance: number, curvature: number): number {
  if (distance >= 0) return 0.5 * distance
  return curvature * 25 * Math.sqrt(-distance)
}

function getBezierControl(
  pos: Position,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  curvature: number,
): [number, number] {
  switch (pos) {
    case Position.Left:
      return [x1 - calculateControlOffset(x1 - x2, curvature), y1]
    case Position.Right:
      return [x1 + calculateControlOffset(x2 - x1, curvature), y1]
    case Position.Top:
      return [x1, y1 - calculateControlOffset(y1 - y2, curvature)]
    case Position.Bottom:
      return [x1, y1 + calculateControlOffset(y2 - y1, curvature)]
    default:
      return [x1, y1]
  }
}

function cubicBezierPoint(
  t: number,
  p0x: number,
  p0y: number,
  p1x: number,
  p1y: number,
  p2x: number,
  p2y: number,
  p3x: number,
  p3y: number,
): [number, number] {
  const u = 1 - t
  const uu = u * u
  const uuu = uu * u
  const tt = t * t
  const ttt = tt * t
  const x = uuu * p0x + 3 * uu * t * p1x + 3 * u * tt * p2x + ttt * p3x
  const y = uuu * p0y + 3 * uu * t * p1y + 3 * u * tt * p2y + ttt * p3y
  return [x, y]
}

type TriggerEdgePathProps = Pick<
  EdgeProps,
  | 'id'
  | 'sourceX'
  | 'sourceY'
  | 'sourcePosition'
  | 'targetX'
  | 'targetY'
  | 'targetPosition'
  | 'markerEnd'
  | 'style'
  | 'interactionWidth'
>

function getTriggerEdgePath({
  sourceX,
  sourceY,
  sourcePosition,
  targetX,
  targetY,
  targetPosition,
}: TriggerEdgePathProps): [string, number, number] {
  const [path, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  })
  return [path, labelX, labelY]
}

const TriggerEdgeWithoutLabel = memo(function TriggerEdgeWithoutLabel({
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
}: TriggerEdgePathProps) {
  const [edgePath] = getTriggerEdgePath({
    id,
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    markerEnd,
    style,
  })

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      markerEnd={markerEnd}
      style={style}
      interactionWidth={interactionWidth}
    />
  )
})

interface TriggerEdgeWithLabelProps extends EdgeProps {
  label: string
}

const TriggerEdgeWithLabel = memo(function TriggerEdgeWithLabel({
  id,
  source,
  target,
  sourceX,
  sourceY,
  sourcePosition,
  targetX,
  targetY,
  targetPosition,
  markerEnd,
  style,
  interactionWidth,
  label,
}: TriggerEdgeWithLabelProps) {
  const [edgePath, midX, midY] = getTriggerEdgePath({
    id,
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    markerEnd,
    style,
  })

  const nodeLookup = useStore(selectNodeLookup)

  // Walk SAMPLE_T_VALUES along the actual bezier curve until we find a
  // position whose label rect doesn't overlap any blocking node. Sampling
  // the curve (not the linear chord) keeps the label visually anchored on
  // the rendered edge path no matter how far we shift it.
  const labelPos = useMemo(() => {
    if (!label) return { x: midX, y: midY }

    const [c1x, c1y] = getBezierControl(
      sourcePosition,
      sourceX,
      sourceY,
      targetX,
      targetY,
      BEZIER_CURVATURE,
    )
    const [c2x, c2y] = getBezierControl(
      targetPosition,
      targetX,
      targetY,
      sourceX,
      sourceY,
      BEZIER_CURVATURE,
    )

    const candidateXs: number[] = []
    const candidateYs: number[] = []
    for (const t of SAMPLE_T_VALUES) {
      const [px, py] =
        t === 0.5
          ? [midX, midY]
          : cubicBezierPoint(t, sourceX, sourceY, c1x, c1y, c2x, c2y, targetX, targetY)
      candidateXs.push(px)
      candidateYs.push(py)
    }
    const minX = Math.min(sourceX, targetX) - EDGE_COLLISION_MARGIN
    const maxX = Math.max(sourceX, targetX) + EDGE_COLLISION_MARGIN
    const minY = Math.min(sourceY, targetY) - EDGE_COLLISION_MARGIN
    const maxY = Math.max(sourceY, targetY) + EDGE_COLLISION_MARGIN

    let blockedMask = 0
    const allBlockedMask = (1 << SAMPLE_T_VALUES.length) - 1
    for (const [nodeId, node] of nodeLookup) {
      if (blockedMask === allBlockedMask) break
      if (nodeId === source || nodeId === target) continue
      if (node.type && EXCLUDED_TYPES.has(node.type)) continue
      const w = node.measured?.width
      const h = node.measured?.height
      if (!w || !h) continue
      const x = node.internals.positionAbsolute.x
      const y = node.internals.positionAbsolute.y
      if (x > maxX || x + w < minX || y > maxY || y + h < minY) continue
      for (let index = 0; index < SAMPLE_T_VALUES.length; index++) {
        const bit = 1 << index
        if ((blockedMask & bit) !== 0) continue
        if (!labelOverlapsRect(candidateXs[index], candidateYs[index], x, y, w, h)) continue
        blockedMask |= bit
      }
    }

    for (let index = 0; index < SAMPLE_T_VALUES.length; index++) {
      if ((blockedMask & (1 << index)) === 0) {
        return { x: candidateXs[index], y: candidateYs[index] }
      }
    }

    // Every sample overlapped something — fall back to the bezier midpoint
    // so the label still renders rather than disappearing.
    return { x: midX, y: midY }
  }, [
    label,
    midX,
    midY,
    sourceX,
    sourceY,
    sourcePosition,
    source,
    targetX,
    targetY,
    targetPosition,
    target,
    nodeLookup,
  ])

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
          className="agent-edge-trigger-label"
          style={{
            transform: `translate(-50%, -50%) translate(${labelPos.x}px, ${labelPos.y}px)`,
          }}
        >
          {label}
        </div>
      </EdgeLabelRenderer>
    </>
  )
})

export const TriggerEdge = memo(function TriggerEdge(props: EdgeProps) {
  const triggerData = props.data as TriggerEdgeData | undefined
  const label = triggerData?.triggerLabel

  if (!label || triggerData?.suppressLabel) {
    return <TriggerEdgeWithoutLabel {...props} />
  }

  return <TriggerEdgeWithLabel {...props} label={label} />
})
