import { describe, expect, it } from 'vitest'

import { solveLayout } from './agent-workspace-layout'

describe('solveLayout', () => {
  it.each([
    [1, 640, 400, '1x1'],
    [2, 700, 300, '1x2'],
    [2, 330, 600, '2x1'],
    [3, 990, 300, '1x3'],
    [3, 330, 840, '3x1'],
    [4, 760, 560, '2x2'],
    [4, 330, 1120, '4x1'],
    [5, 990, 560, '2x3'],
    [5, 640, 840, '3x2'],
    [6, 990, 560, '2x3'],
    [6, 640, 840, '3x2'],
  ])('chooses the first viable arrangement for %i panes', (paneCount, width, height, key) => {
    expect(
      solveLayout({
        paneCount,
        availableWidth: width,
        availableHeight: height,
      }).arrangement.key,
    ).toBe(key)
  })

  it('uses valid persisted track ratios for the solved arrangement', () => {
    const solved = solveLayout({
      paneCount: 2,
      availableWidth: 960,
      availableHeight: 300,
      userLayout: {
        '1x2': [2, 1, 1],
      },
    })

    expect(solved.arrangement.key).toBe('1x2')
    expect(solved.ratios).toEqual([2 / 3, 1 / 3, 1])
  })

  it('falls back to even ratios when saved ratios violate minimum pane size', () => {
    const solved = solveLayout({
      paneCount: 2,
      availableWidth: 900,
      availableHeight: 300,
      userLayout: {
        '1x2': [0.05, 0.95, 1],
      },
    })

    expect(solved.arrangement.key).toBe('1x2')
    expect(solved.ratios).toEqual([0.5, 0.5, 1])
  })

  it('falls back to the next arrangement when the preferred grid cannot fit evenly', () => {
    const solved = solveLayout({
      paneCount: 4,
      availableWidth: 360,
      availableHeight: 1120,
    })

    expect(solved.arrangement.key).toBe('4x1')
    expect(solved.fallback).toBeUndefined()
  })

  it('returns a scrolling stack fallback when no arrangement can satisfy minimum pane size', () => {
    const solved = solveLayout({
      paneCount: 6,
      availableWidth: 240,
      availableHeight: 240,
    })

    expect(solved.arrangement).toMatchObject({
      key: 'stack',
      rows: 6,
      columns: 1,
      cellCount: 6,
    })
    expect(solved.fallback).toBe('stack')
  })

  it('clamps unsupported pane counts to the phase-one cap', () => {
    const solved = solveLayout({
      paneCount: 99,
      availableWidth: 990,
      availableHeight: 560,
    })

    expect(solved.arrangement.key).toBe('2x3')
  })
})
