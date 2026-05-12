import { Position } from '@xyflow/react'
import { describe, expect, it } from 'vitest'

import { EDITING_ARROW_TARGET_CLEARANCE, arrowTargetEndpoint } from './edge-endpoint-offset'

describe('arrowTargetEndpoint', () => {
  it('moves editing arrowheads outward from the target handle side', () => {
    expect(
      arrowTargetEndpoint({
        editing: true,
        markerEnd: 'url(#arrow)',
        targetX: 100,
        targetY: 200,
        targetPosition: Position.Bottom,
      }),
    ).toEqual({ x: 100, y: 200 + EDITING_ARROW_TARGET_CLEARANCE })

    expect(
      arrowTargetEndpoint({
        editing: true,
        markerEnd: 'url(#arrow)',
        targetX: 100,
        targetY: 200,
        targetPosition: Position.Left,
      }),
    ).toEqual({ x: 100 - EDITING_ARROW_TARGET_CLEARANCE, y: 200 })
  })

  it('leaves non-editing or markerless edges anchored at the true handle point', () => {
    const base = { targetX: 100, targetY: 200, targetPosition: Position.Top }

    expect(arrowTargetEndpoint({ ...base, editing: false, markerEnd: 'url(#arrow)' })).toEqual({
      x: 100,
      y: 200,
    })
    expect(arrowTargetEndpoint({ ...base, editing: true, markerEnd: undefined })).toEqual({
      x: 100,
      y: 200,
    })
  })
})
