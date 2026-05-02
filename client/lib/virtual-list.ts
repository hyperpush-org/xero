export interface VirtualRangeInput {
  itemCount: number
  itemSize: number
  viewportSize: number
  scrollOffset: number
  overscan?: number
}

export interface VirtualRange {
  startIndex: number
  endIndex: number
  beforeSize: number
  afterSize: number
  totalSize: number
  renderedCount: number
}

export function calculateVirtualRange({
  itemCount,
  itemSize,
  viewportSize,
  scrollOffset,
  overscan = 6,
}: VirtualRangeInput): VirtualRange {
  if (itemCount <= 0 || itemSize <= 0) {
    return {
      startIndex: 0,
      endIndex: 0,
      beforeSize: 0,
      afterSize: 0,
      totalSize: 0,
      renderedCount: 0,
    }
  }

  const safeViewportSize = Math.max(0, viewportSize)
  const safeOverscan = Math.max(0, Math.floor(overscan))
  const totalSize = itemCount * itemSize
  const maxScrollOffset = Math.max(0, totalSize - safeViewportSize)
  const safeScrollOffset = Math.min(Math.max(0, scrollOffset), maxScrollOffset)
  const firstVisibleIndex = Math.floor(safeScrollOffset / itemSize)
  const visibleCount = Math.max(1, Math.ceil(safeViewportSize / itemSize))
  const startIndex = Math.max(0, firstVisibleIndex - safeOverscan)
  const endIndex = Math.min(itemCount, firstVisibleIndex + visibleCount + safeOverscan)

  return {
    startIndex,
    endIndex,
    beforeSize: startIndex * itemSize,
    afterSize: Math.max(0, totalSize - endIndex * itemSize),
    totalSize,
    renderedCount: endIndex - startIndex,
  }
}

export function getVirtualIndexes(range: Pick<VirtualRange, 'startIndex' | 'endIndex'>): number[] {
  const indexes: number[] = []
  for (let index = range.startIndex; index < range.endIndex; index += 1) {
    indexes.push(index)
  }
  return indexes
}

export function shouldVirtualizeRows(itemCount: number, threshold: number): boolean {
  return itemCount > Math.max(0, threshold)
}
