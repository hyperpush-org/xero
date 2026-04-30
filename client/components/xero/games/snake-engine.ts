// ---------------------------------------------------------------------------
// Snake engine — pure game state & reducer.
//
// Coordinate space: integer grid cells, +x right, +y DOWN. The head is at
// segments[0]; tail is segments[segments.length - 1].
// ---------------------------------------------------------------------------

export const BOARD_COLS = 24
export const BOARD_ROWS = 18

export const INITIAL_STEP_MS = 140
export const MIN_STEP_MS = 55
export const STEP_DECREMENT_PER_LEVEL = 10

export const SCORE_PER_FOOD = 10
export const GROW_PER_FOOD = 3
export const FOODS_PER_LEVEL = 5

const START_LEN = 4
// Cap the input buffer so rapid taps can't build a queue that plays out
// after the user expects it to settle.
const MAX_DIR_QUEUE = 2

export type Direction = 'up' | 'down' | 'left' | 'right'
export type GameStatus = 'idle' | 'playing' | 'paused' | 'over'

export interface Segment {
  x: number
  y: number
}

export interface GameState {
  status: GameStatus
  score: number
  best: number
  level: number
  foodEaten: number

  segments: Segment[]
  direction: Direction
  dirQueue: Direction[]
  food: Segment

  stepTimer: number
  growBy: number
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'tick'; dt: number }
  | { type: 'queueDir'; dir: Direction }

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function opposite(a: Direction, b: Direction): boolean {
  return (
    (a === 'up' && b === 'down') ||
    (a === 'down' && b === 'up') ||
    (a === 'left' && b === 'right') ||
    (a === 'right' && b === 'left')
  )
}

function advance(seg: Segment, dir: Direction): Segment {
  switch (dir) {
    case 'up':
      return { x: seg.x, y: seg.y - 1 }
    case 'down':
      return { x: seg.x, y: seg.y + 1 }
    case 'left':
      return { x: seg.x - 1, y: seg.y }
    case 'right':
      return { x: seg.x + 1, y: seg.y }
  }
}

export function stepInterval(level: number): number {
  return Math.max(
    MIN_STEP_MS,
    INITIAL_STEP_MS - (level - 1) * STEP_DECREMENT_PER_LEVEL,
  )
}

function placeFood(segments: Segment[]): Segment {
  const occupied = new Set(segments.map((s) => s.y * BOARD_COLS + s.x))
  for (let attempt = 0; attempt < 64; attempt++) {
    const x = Math.floor(Math.random() * BOARD_COLS)
    const y = Math.floor(Math.random() * BOARD_ROWS)
    if (!occupied.has(y * BOARD_COLS + x)) return { x, y }
  }
  // Fallback when random misses: scan for the first free cell.
  for (let y = 0; y < BOARD_ROWS; y++) {
    for (let x = 0; x < BOARD_COLS; x++) {
      if (!occupied.has(y * BOARD_COLS + x)) return { x, y }
    }
  }
  return { x: 0, y: 0 }
}

function initialSegments(): Segment[] {
  const startX = Math.floor(BOARD_COLS / 2) - 1
  const startY = Math.floor(BOARD_ROWS / 2)
  const segments: Segment[] = []
  for (let i = 0; i < START_LEN; i++) {
    segments.push({ x: startX - i, y: startY })
  }
  return segments
}

function freshGame(best: number): GameState {
  const segments = initialSegments()
  return {
    status: 'playing',
    score: 0,
    best,
    level: 1,
    foodEaten: 0,
    segments,
    direction: 'right',
    dirQueue: [],
    food: placeFood(segments),
    stepTimer: 0,
    growBy: 0,
  }
}

function doStep(state: GameState): GameState {
  let direction = state.direction
  let dirQueue = state.dirQueue
  if (dirQueue.length > 0) {
    const next = dirQueue[0]
    dirQueue = dirQueue.slice(1)
    if (next !== direction && !opposite(direction, next)) {
      direction = next
    }
  }

  const head = advance(state.segments[0], direction)

  if (
    head.x < 0 ||
    head.x >= BOARD_COLS ||
    head.y < 0 ||
    head.y >= BOARD_ROWS
  ) {
    return {
      ...state,
      status: 'over',
      direction,
      dirQueue,
      best: Math.max(state.best, state.score),
    }
  }

  // Self collision. If we aren't about to grow, the tail tip moves out of
  // the way this tick — so the last segment is safe to step into.
  const eating = head.x === state.food.x && head.y === state.food.y
  const willGrow = state.growBy > 0 || eating
  const checkUntil = willGrow ? state.segments.length : state.segments.length - 1
  for (let i = 0; i < checkUntil; i++) {
    const s = state.segments[i]
    if (s.x === head.x && s.y === head.y) {
      return {
        ...state,
        status: 'over',
        direction,
        dirQueue,
        best: Math.max(state.best, state.score),
      }
    }
  }

  let score = state.score
  let foodEaten = state.foodEaten
  let level = state.level
  let growBy = state.growBy
  let food = state.food

  if (eating) {
    score += SCORE_PER_FOOD * level
    foodEaten += 1
    growBy += GROW_PER_FOOD
    if (foodEaten % FOODS_PER_LEVEL === 0) level += 1
  }

  const nextSegments: Segment[] = [head, ...state.segments]
  if (growBy > 0) {
    growBy -= 1
  } else {
    nextSegments.pop()
  }

  if (eating) {
    food = placeFood(nextSegments)
  }

  return {
    ...state,
    direction,
    dirQueue,
    segments: nextSegments,
    food,
    growBy,
    score,
    foodEaten,
    level,
    best: Math.max(state.best, score),
  }
}

function tickState(state: GameState, dt: number): GameState {
  if (state.status !== 'playing') return state
  let s = state
  let timer = s.stepTimer + dt
  let safety = 6
  while (safety-- > 0 && s.status === 'playing') {
    const interval = stepInterval(s.level)
    if (timer < interval) break
    timer -= interval
    s = doStep(s)
  }
  return { ...s, stepTimer: s.status === 'playing' ? timer : 0 }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createInitialState(): GameState {
  const base = freshGame(0)
  return { ...base, status: 'idle' }
}

export function reduce(state: GameState, action: Action): GameState {
  switch (action.type) {
    case 'start':
      if (state.status === 'playing') return state
      return freshGame(state.best)
    case 'reset':
      return freshGame(state.best)
    case 'pause':
      if (state.status !== 'playing') return state
      return { ...state, status: 'paused' }
    case 'resume':
      if (state.status !== 'paused') return state
      return { ...state, status: 'playing' }
    case 'tick':
      return tickState(state, action.dt)
    case 'queueDir': {
      if (state.status !== 'playing') return state
      if (state.dirQueue.length >= MAX_DIR_QUEUE) return state
      const last = state.dirQueue[state.dirQueue.length - 1] ?? state.direction
      if (action.dir === last || opposite(action.dir, last)) return state
      return { ...state, dirQueue: [...state.dirQueue, action.dir] }
    }
    default:
      return state
  }
}
