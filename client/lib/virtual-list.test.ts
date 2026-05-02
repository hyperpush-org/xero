import { describe, expect, it } from 'vitest'

import { calculateVirtualRange, getVirtualIndexes, shouldVirtualizeRows } from './virtual-list'

describe('virtual-list helpers', () => {
  it('calculates a bounded visible range with overscan and spacers', () => {
    const range = calculateVirtualRange({
      itemCount: 1_000,
      itemSize: 20,
      viewportSize: 100,
      scrollOffset: 1_000,
      overscan: 2,
    })

    expect(range).toEqual({
      startIndex: 48,
      endIndex: 57,
      beforeSize: 960,
      afterSize: 18_860,
      totalSize: 20_000,
      renderedCount: 9,
    })
    expect(getVirtualIndexes(range)).toEqual([48, 49, 50, 51, 52, 53, 54, 55, 56])
  })

  it('clamps ranges near the end of a list', () => {
    const range = calculateVirtualRange({
      itemCount: 10,
      itemSize: 24,
      viewportSize: 72,
      scrollOffset: 10_000,
      overscan: 1,
    })

    expect(range.startIndex).toBe(6)
    expect(range.endIndex).toBe(10)
    expect(range.afterSize).toBe(0)
    expect(range.renderedCount).toBe(4)
  })

  it('keeps small lists on the simple render path', () => {
    expect(shouldVirtualizeRows(240, 240)).toBe(false)
    expect(shouldVirtualizeRows(241, 240)).toBe(true)
  })
})
