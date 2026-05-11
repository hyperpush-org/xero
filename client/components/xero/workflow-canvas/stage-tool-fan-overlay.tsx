'use client'

import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
} from 'react'
import { useStoreApi, type Viewport } from '@xyflow/react'

// Renders stage→tool admittance lines as a single SVG path managed
// imperatively. Pulling these out of React-Flow's edges array means hovering
// a stage no longer flips 20–30 individual edge `<g>` elements from
// display:none into the layout/paint tree in one frame — instead we update
// the `d` attribute on one `<path>`, which the browser tessellates and
// paints as a single operation. Net cost is a constant ~1 paint regardless
// of how many tools a stage admits.

export interface StageToolFanOverlayRef {
  showStage: (stageId: string | null) => void
}

export interface StageToolFanEntry {
  stageId: string
  targets: ReadonlyArray<string>
}

interface StageToolFanOverlayProps {
  entries: ReadonlyArray<StageToolFanEntry>
}

const HORIZONTAL_CONTROL_MIN = 60

export const StageToolFanOverlay = forwardRef<
  StageToolFanOverlayRef,
  StageToolFanOverlayProps
>(function StageToolFanOverlay({ entries }, ref) {
  const storeApi = useStoreApi()
  const svgRef = useRef<SVGSVGElement | null>(null)
  const pathRef = useRef<SVGPathElement | null>(null)
  const activeStageIdRef = useRef<string | null>(null)

  const entriesMap = useMemo(() => {
    const map = new Map<string, ReadonlyArray<string>>()
    for (const entry of entries) {
      map.set(entry.stageId, entry.targets)
    }
    return map
  }, [entries])
  const entriesMapRef = useRef(entriesMap)
  entriesMapRef.current = entriesMap

  const updatePath = useCallback(() => {
    const path = pathRef.current
    if (!path) return

    const stageId = activeStageIdRef.current
    if (!stageId) {
      if (path.getAttribute('d')) path.setAttribute('d', '')
      return
    }

    const targets = entriesMapRef.current.get(stageId)
    if (!targets || targets.length === 0) {
      if (path.getAttribute('d')) path.setAttribute('d', '')
      return
    }

    // Read positions from React-Flow's store. internals.positionAbsolute
    // accounts for parent frames; .measured is the post-render size. We fall
    // back to width/height on the node when measured isn't populated yet
    // (initial layout, before the first DOM measurement settled).
    const { nodeLookup } = storeApi.getState()
    const stage = nodeLookup.get(stageId)
    if (!stage) {
      if (path.getAttribute('d')) path.setAttribute('d', '')
      return
    }
    const stageW = stage.measured?.width ?? stage.width ?? 0
    const stageH = stage.measured?.height ?? stage.height ?? 0
    const sx = stage.internals.positionAbsolute.x + stageW
    const sy = stage.internals.positionAbsolute.y + stageH / 2

    let d = ''
    for (const targetId of targets) {
      const tool = nodeLookup.get(targetId)
      if (!tool) continue
      const toolH = tool.measured?.height ?? tool.height ?? 0
      const tx = tool.internals.positionAbsolute.x
      const ty = tool.internals.positionAbsolute.y + toolH / 2
      // Cubic bezier with horizontally-shifted control points — mirrors the
      // smoothstep-ish curve the React-Flow `default` edge type produced, so
      // the visual is the same as before, just rendered through one path.
      const cOffset = Math.max(HORIZONTAL_CONTROL_MIN, (tx - sx) / 2)
      d += `M${sx} ${sy}C${sx + cOffset} ${sy} ${tx - cOffset} ${ty} ${tx} ${ty}`
    }

    path.setAttribute('d', d)
  }, [storeApi])

  const applyViewport = useCallback((viewport: Viewport) => {
    const svg = svgRef.current
    if (!svg) return
    // Mirror the React-Flow viewport so paths drawn in world coordinates
    // align with the rest of the canvas content. transformOrigin: 0 0 keeps
    // scaling anchored at the canvas origin to match React-Flow's behaviour.
    svg.style.transform = `translate(${viewport.x}px, ${viewport.y}px) scale(${viewport.zoom})`
  }, [])

  // Subscribe to viewport `transform` and `nodeLookup` directly on the store
  // rather than via `useOnViewportChange`. That hook writes to a single
  // `onViewportChange` slot, which would clobber `WorkflowCanvasDots`'
  // subscription and leave the dot background frozen during pan/zoom.
  useEffect(() => {
    const initial = storeApi.getState()
    let lastTransform = initial.transform
    let lastNodeLookup = initial.nodeLookup
    applyViewport({ x: lastTransform[0], y: lastTransform[1], zoom: lastTransform[2] })
    return storeApi.subscribe((state) => {
      if (state.transform !== lastTransform) {
        lastTransform = state.transform
        applyViewport({ x: lastTransform[0], y: lastTransform[1], zoom: lastTransform[2] })
      }
      if (state.nodeLookup !== lastNodeLookup) {
        lastNodeLookup = state.nodeLookup
        if (activeStageIdRef.current) updatePath()
      }
    })
  }, [applyViewport, storeApi, updatePath])

  useImperativeHandle(
    ref,
    () => ({
      showStage: (stageId) => {
        if (activeStageIdRef.current === stageId) return
        activeStageIdRef.current = stageId
        updatePath()
      },
    }),
    [updatePath],
  )

  return (
    <svg
      ref={svgRef}
      className="agent-stage-tool-fan-overlay"
      aria-hidden="true"
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        width: 0,
        height: 0,
        overflow: 'visible',
        pointerEvents: 'none',
        transformOrigin: '0 0',
      }}
    >
      <path ref={pathRef} fill="none" />
    </svg>
  )
})
