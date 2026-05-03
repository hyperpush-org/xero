export type AgentWorkspaceArrangementKey =
  | '1x1'
  | '1x2'
  | '2x1'
  | '1x3'
  | '3x1'
  | '2x2'
  | '4x1'
  | '1x4'
  | '2x3'
  | '3x2'
  | 'stack'

export interface AgentWorkspaceArrangement {
  key: AgentWorkspaceArrangementKey
  rows: number
  columns: number
  cellCount: number
}

export interface SolveAgentWorkspaceLayoutInput {
  paneCount: number
  availableWidth: number
  availableHeight: number
  minPaneWidth?: number
  minPaneHeight?: number
  userLayout?: Record<string, number[]> | number[]
}

export interface SolvedAgentWorkspaceLayout {
  arrangement: AgentWorkspaceArrangement
  ratios: number[]
  fallback?: 'stack'
}

const DEFAULT_MIN_PANE_WIDTH = 320
const DEFAULT_MIN_PANE_HEIGHT = 280
const MAX_PANE_COUNT = 6

const PREFERENCES: Record<number, AgentWorkspaceArrangement[]> = {
  1: [{ key: '1x1', rows: 1, columns: 1, cellCount: 1 }],
  2: [
    { key: '1x2', rows: 1, columns: 2, cellCount: 2 },
    { key: '2x1', rows: 2, columns: 1, cellCount: 2 },
  ],
  3: [
    { key: '1x3', rows: 1, columns: 3, cellCount: 3 },
    { key: '3x1', rows: 3, columns: 1, cellCount: 3 },
  ],
  4: [
    { key: '2x2', rows: 2, columns: 2, cellCount: 4 },
    { key: '4x1', rows: 4, columns: 1, cellCount: 4 },
    { key: '1x4', rows: 1, columns: 4, cellCount: 4 },
  ],
  5: [
    { key: '2x3', rows: 2, columns: 3, cellCount: 6 },
    { key: '3x2', rows: 3, columns: 2, cellCount: 6 },
  ],
  6: [
    { key: '2x3', rows: 2, columns: 3, cellCount: 6 },
    { key: '3x2', rows: 3, columns: 2, cellCount: 6 },
  ],
}

function normalizePaneCount(paneCount: number): number {
  if (!Number.isFinite(paneCount)) {
    return 1
  }

  return Math.max(1, Math.min(MAX_PANE_COUNT, Math.trunc(paneCount)))
}

function normalizePositive(value: number | undefined, fallback: number): number {
  if (!Number.isFinite(value) || (value ?? 0) <= 0) {
    return fallback
  }

  return value as number
}

function evenRatios(count: number): number[] {
  return Array.from({ length: count }, () => 1 / count)
}

function normalizeRatios(values: number[] | undefined, count: number): number[] | null {
  if (!values || values.length !== count) {
    return null
  }

  const finiteValues = values.map((value) => (Number.isFinite(value) && value > 0 ? value : 0))
  const total = finiteValues.reduce((sum, value) => sum + value, 0)
  if (total <= 0) {
    return null
  }

  return finiteValues.map((value) => value / total)
}

function resolveUserRatios(
  arrangement: AgentWorkspaceArrangement,
  userLayout: SolveAgentWorkspaceLayoutInput['userLayout'],
): number[] | null {
  const values = Array.isArray(userLayout) ? userLayout : userLayout?.[arrangement.key]
  if (!values || values.length !== arrangement.columns + arrangement.rows) {
    return null
  }

  const columnRatios = normalizeRatios(values.slice(0, arrangement.columns), arrangement.columns)
  const rowRatios = normalizeRatios(values.slice(arrangement.columns), arrangement.rows)
  if (!columnRatios || !rowRatios) {
    return null
  }

  return [...columnRatios, ...rowRatios]
}

function getEvenArrangementRatios(arrangement: AgentWorkspaceArrangement): number[] {
  return [...evenRatios(arrangement.columns), ...evenRatios(arrangement.rows)]
}

function isArrangementViable(
  arrangement: AgentWorkspaceArrangement,
  ratios: number[],
  options: {
    availableWidth: number
    availableHeight: number
    minPaneWidth: number
    minPaneHeight: number
  },
): boolean {
  const columnRatios = ratios.slice(0, arrangement.columns)
  const rowRatios = ratios.slice(arrangement.columns)
  const narrowestCell = Math.min(...columnRatios) * options.availableWidth
  const shortestCell = Math.min(...rowRatios) * options.availableHeight

  return narrowestCell >= options.minPaneWidth && shortestCell >= options.minPaneHeight
}

export function solveLayout(input: SolveAgentWorkspaceLayoutInput): SolvedAgentWorkspaceLayout {
  const paneCount = normalizePaneCount(input.paneCount)
  const availableWidth = normalizePositive(input.availableWidth, 0)
  const availableHeight = normalizePositive(input.availableHeight, 0)
  const minPaneWidth = normalizePositive(input.minPaneWidth, DEFAULT_MIN_PANE_WIDTH)
  const minPaneHeight = normalizePositive(input.minPaneHeight, DEFAULT_MIN_PANE_HEIGHT)

  for (const arrangement of PREFERENCES[paneCount]) {
    const even = getEvenArrangementRatios(arrangement)
    const userRatios = resolveUserRatios(arrangement, input.userLayout)
    if (
      userRatios &&
      isArrangementViable(arrangement, userRatios, {
        availableWidth,
        availableHeight,
        minPaneWidth,
        minPaneHeight,
      })
    ) {
      return {
        arrangement,
        ratios: userRatios,
      }
    }

    if (
      isArrangementViable(arrangement, even, {
        availableWidth,
        availableHeight,
        minPaneWidth,
        minPaneHeight,
      })
    ) {
      return {
        arrangement,
        ratios: even,
      }
    }
  }

  const arrangement: AgentWorkspaceArrangement = {
    key: 'stack',
    rows: paneCount,
    columns: 1,
    cellCount: paneCount,
  }

  return {
    arrangement,
    ratios: getEvenArrangementRatios(arrangement),
    fallback: 'stack',
  }
}
