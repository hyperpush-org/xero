import { useCallback, useLayoutEffect, useMemo, useRef, useState, type UIEvent } from 'react'
import { calculateVirtualRange, getVirtualIndexes } from '@/lib/virtual-list'

interface UseFixedVirtualizerOptions {
  enabled: boolean
  itemCount: number
  itemSize: number
  overscan?: number
  initialViewportSize?: number
  scrollToIndex?: number | null
}

export function useFixedVirtualizer({
  enabled,
  itemCount,
  itemSize,
  overscan = 8,
  initialViewportSize = 480,
  scrollToIndex = null,
}: UseFixedVirtualizerOptions) {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [viewportSize, setViewportSize] = useState(initialViewportSize)
  const [scrollOffset, setScrollOffset] = useState(0)

  const measure = useCallback(() => {
    const node = scrollRef.current
    if (!node) return
    setViewportSize(node.clientHeight || initialViewportSize)
  }, [initialViewportSize])

  useLayoutEffect(() => {
    if (!enabled) return
    measure()

    const node = scrollRef.current
    if (!node || typeof window === 'undefined') return

    const ResizeObserverCtor = window.ResizeObserver
    if (typeof ResizeObserverCtor === 'function') {
      const observer = new ResizeObserverCtor(measure)
      observer.observe(node)
      return () => observer.disconnect()
    }

    window.addEventListener('resize', measure)
    return () => window.removeEventListener('resize', measure)
  }, [enabled, measure])

  useLayoutEffect(() => {
    if (!enabled || scrollToIndex === null || scrollToIndex < 0) return
    const node = scrollRef.current
    if (!node) return

    const viewSize = node.clientHeight || viewportSize || initialViewportSize
    const targetTop = scrollToIndex * itemSize
    const targetBottom = targetTop + itemSize
    const currentTop = node.scrollTop
    const currentBottom = currentTop + viewSize

    if (targetTop < currentTop) {
      node.scrollTop = targetTop
      setScrollOffset(targetTop)
    } else if (targetBottom > currentBottom) {
      const nextOffset = Math.max(0, targetBottom - viewSize)
      node.scrollTop = nextOffset
      setScrollOffset(nextOffset)
    }
  }, [enabled, initialViewportSize, itemSize, scrollToIndex, viewportSize])

  const onScroll = useCallback((event: UIEvent<HTMLDivElement>) => {
    if (!enabled) return
    setScrollOffset(event.currentTarget.scrollTop)
  }, [enabled])

  const range = useMemo(
    () =>
      enabled
        ? calculateVirtualRange({
            itemCount,
            itemSize,
            viewportSize,
            scrollOffset,
            overscan,
          })
        : calculateVirtualRange({
            itemCount,
            itemSize,
            viewportSize: itemCount * itemSize,
            scrollOffset: 0,
            overscan: 0,
          }),
    [enabled, itemCount, itemSize, overscan, scrollOffset, viewportSize],
  )
  const indexes = useMemo(() => getVirtualIndexes(range), [range])

  return {
    indexes,
    onScroll,
    range,
    scrollRef,
  }
}
