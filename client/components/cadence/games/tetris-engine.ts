// ---------------------------------------------------------------------------
// Tetris engine — pure game state & reducer.
//
// Coordinate convention: +x is right, +y is DOWN (screen coordinates). SRS
// kick tables from the Tetris Guideline are expressed with +y up; every
// offset pulled from the spec is negated on y when translated here.
// ---------------------------------------------------------------------------

export const BOARD_WIDTH = 10
export const BOARD_HEIGHT = 20

export const DAS_MS = 140
export const ARR_MS = 35
export const SOFT_DROP_MS = 40

export const LOCK_DELAY_MS = 500
export const LINE_CLEAR_MS = 260
export const MAX_LOCK_RESETS = 15

export type PieceType = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L'

export const PIECE_TYPES: PieceType[] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L']

export const PIECE_COLORS: Record<PieceType, string> = {
  I: '#22d3ee',
  O: '#facc15',
  T: '#a855f7',
  S: '#4ade80',
  Z: '#ef4444',
  J: '#3b82f6',
  L: '#f97316',
}

export type Rotation = 0 | 1 | 2 | 3
export type Cell = number // 0 = empty; 1..7 = piece type index + 1

// Piece cell offsets per rotation state. Each rotation is a list of [x, y]
// cells relative to the piece's bounding-box origin.
export const PIECE_ROTATIONS: Record<PieceType, Array<Array<[number, number]>>> = {
  I: [
    [[0, 1], [1, 1], [2, 1], [3, 1]],
    [[2, 0], [2, 1], [2, 2], [2, 3]],
    [[0, 2], [1, 2], [2, 2], [3, 2]],
    [[1, 0], [1, 1], [1, 2], [1, 3]],
  ],
  O: [
    [[1, 0], [2, 0], [1, 1], [2, 1]],
    [[1, 0], [2, 0], [1, 1], [2, 1]],
    [[1, 0], [2, 0], [1, 1], [2, 1]],
    [[1, 0], [2, 0], [1, 1], [2, 1]],
  ],
  T: [
    [[1, 0], [0, 1], [1, 1], [2, 1]],
    [[1, 0], [1, 1], [2, 1], [1, 2]],
    [[0, 1], [1, 1], [2, 1], [1, 2]],
    [[1, 0], [0, 1], [1, 1], [1, 2]],
  ],
  S: [
    [[1, 0], [2, 0], [0, 1], [1, 1]],
    [[1, 0], [1, 1], [2, 1], [2, 2]],
    [[1, 1], [2, 1], [0, 2], [1, 2]],
    [[0, 0], [0, 1], [1, 1], [1, 2]],
  ],
  Z: [
    [[0, 0], [1, 0], [1, 1], [2, 1]],
    [[2, 0], [1, 1], [2, 1], [1, 2]],
    [[0, 1], [1, 1], [1, 2], [2, 2]],
    [[1, 0], [0, 1], [1, 1], [0, 2]],
  ],
  J: [
    [[0, 0], [0, 1], [1, 1], [2, 1]],
    [[1, 0], [2, 0], [1, 1], [1, 2]],
    [[0, 1], [1, 1], [2, 1], [2, 2]],
    [[1, 0], [1, 1], [0, 2], [1, 2]],
  ],
  L: [
    [[2, 0], [0, 1], [1, 1], [2, 1]],
    [[1, 0], [1, 1], [1, 2], [2, 2]],
    [[0, 1], [1, 1], [2, 1], [0, 2]],
    [[0, 0], [1, 0], [1, 1], [1, 2]],
  ],
}

// SRS wall-kick tables. Keys are `${from}->${to}`. Offsets use screen coords
// (+y = down). The `from->to` mapping always advances by ±1 (mod 4).
export const KICKS_JLSTZ: Record<string, Array<[number, number]>> = {
  '0->1': [[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]],
  '1->0': [[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]],
  '1->2': [[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]],
  '2->1': [[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]],
  '2->3': [[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]],
  '3->2': [[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]],
  '3->0': [[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]],
  '0->3': [[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]],
}

export const KICKS_I: Record<string, Array<[number, number]>> = {
  '0->1': [[0, 0], [-2, 0], [1, 0], [-2, 1], [1, -2]],
  '1->0': [[0, 0], [2, 0], [-1, 0], [2, -1], [-1, 2]],
  '1->2': [[0, 0], [-1, 0], [2, 0], [-1, -2], [2, 1]],
  '2->1': [[0, 0], [1, 0], [-2, 0], [1, 2], [-2, -1]],
  '2->3': [[0, 0], [2, 0], [-1, 0], [2, -1], [-1, 2]],
  '3->2': [[0, 0], [-2, 0], [1, 0], [-2, 1], [1, -2]],
  '3->0': [[0, 0], [1, 0], [-2, 0], [1, 2], [-2, -1]],
  '0->3': [[0, 0], [-1, 0], [2, 0], [-1, -2], [2, 1]],
}

// Gravity interval (ms per gridline fall) per level. Indexed by level.
const GRAVITY_TABLE = [
  1000, 800, 650, 500, 400, 300, 220, 160, 120, 90,
  70, 55, 42, 32, 24, 18, 13, 10,
]

export function gravityInterval(level: number): number {
  return GRAVITY_TABLE[Math.min(level, GRAVITY_TABLE.length - 1)]
}

const LINE_SCORE = [0, 100, 300, 500, 800] as const

// ---------------------------------------------------------------------------
// State shape
// ---------------------------------------------------------------------------

export interface Piece {
  type: PieceType
  rotation: Rotation
  x: number
  y: number
}

export type GameStatus = 'idle' | 'playing' | 'paused' | 'over'

export interface GameState {
  board: Cell[][]
  current: Piece | null
  hold: PieceType | null
  hasHeld: boolean
  queue: PieceType[]
  bag: PieceType[]
  score: number
  lines: number
  level: number
  status: GameStatus
  lockTimer: number
  lockResets: number
  dropTimer: number
  lineClear: { rows: number[]; elapsed: number } | null
  lastClearCount: number
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'tick'; dt: number }
  | { type: 'move'; dx: number }
  | { type: 'softDrop' }
  | { type: 'hardDrop' }
  | { type: 'rotate'; dir: 1 | -1 }
  | { type: 'hold' }

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function emptyBoard(): Cell[][] {
  return Array.from({ length: BOARD_HEIGHT }, () =>
    Array.from({ length: BOARD_WIDTH }, () => 0),
  )
}

function shuffleBag(): PieceType[] {
  const bag = [...PIECE_TYPES]
  for (let i = bag.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    const tmp = bag[i]
    bag[i] = bag[j]
    bag[j] = tmp
  }
  return bag
}

function refillQueue(
  queue: PieceType[],
  bag: PieceType[],
  minLength = 5,
): { queue: PieceType[]; bag: PieceType[] } {
  const nextQueue = [...queue]
  let nextBag = [...bag]
  while (nextQueue.length < minLength) {
    if (nextBag.length === 0) nextBag = shuffleBag()
    nextQueue.push(nextBag.shift() as PieceType)
  }
  return { queue: nextQueue, bag: nextBag }
}

export function pieceCells(piece: Piece): Array<[number, number]> {
  const rots = PIECE_ROTATIONS[piece.type]
  const cells = rots[piece.rotation]
  return cells.map(([cx, cy]) => [piece.x + cx, piece.y + cy] as [number, number])
}

function isValid(board: Cell[][], piece: Piece): boolean {
  for (const [x, y] of pieceCells(piece)) {
    if (x < 0 || x >= BOARD_WIDTH) return false
    if (y >= BOARD_HEIGHT) return false
    if (y >= 0 && board[y][x] !== 0) return false
  }
  return true
}

function spawnPiece(type: PieceType): Piece {
  return { type, rotation: 0, x: 3, y: 0 }
}

function attemptSpawn(state: GameState): GameState {
  const refilled = refillQueue(state.queue, state.bag)
  const queue = [...refilled.queue]
  const nextType = queue.shift() as PieceType
  const topped = refillQueue(queue, refilled.bag)
  const piece = spawnPiece(nextType)
  if (!isValid(state.board, piece)) {
    return {
      ...state,
      current: null,
      queue: topped.queue,
      bag: topped.bag,
      status: 'over',
    }
  }
  return {
    ...state,
    current: piece,
    queue: topped.queue,
    bag: topped.bag,
    hasHeld: false,
    lockTimer: 0,
    lockResets: 0,
    dropTimer: 0,
  }
}

function lockCurrent(state: GameState, extraScore = 0): GameState {
  if (!state.current) return state
  const board = state.board.map((row) => [...row])
  const colorIdx = PIECE_TYPES.indexOf(state.current.type) + 1
  for (const [x, y] of pieceCells(state.current)) {
    if (y >= 0 && y < BOARD_HEIGHT && x >= 0 && x < BOARD_WIDTH) {
      board[y][x] = colorIdx
    }
  }

  const cleared: number[] = []
  for (let y = 0; y < BOARD_HEIGHT; y++) {
    if (board[y].every((c) => c !== 0)) cleared.push(y)
  }

  const withLock: GameState = {
    ...state,
    board,
    current: null,
    score: state.score + extraScore,
    lockTimer: 0,
    lockResets: 0,
    dropTimer: 0,
  }

  if (cleared.length > 0) {
    return { ...withLock, lineClear: { rows: cleared, elapsed: 0 }, lastClearCount: cleared.length }
  }
  return attemptSpawn(withLock)
}

function completeLineClear(state: GameState): GameState {
  if (!state.lineClear) return state
  const rows = new Set(state.lineClear.rows)
  const remaining = state.board.filter((_, y) => !rows.has(y))
  while (remaining.length < BOARD_HEIGHT) {
    remaining.unshift(Array.from({ length: BOARD_WIDTH }, () => 0))
  }
  const count = state.lineClear.rows.length
  const levelForScore = state.level + 1
  const gain = LINE_SCORE[count] * levelForScore
  const newLines = state.lines + count
  const newLevel = Math.floor(newLines / 10)
  return attemptSpawn({
    ...state,
    board: remaining,
    lineClear: null,
    score: state.score + gain,
    lines: newLines,
    level: newLevel,
  })
}

function applyLockReset(state: GameState): GameState {
  if (state.lockResets >= MAX_LOCK_RESETS) return state
  return { ...state, lockTimer: 0, lockResets: state.lockResets + 1 }
}

function tryTranslate(state: GameState, dx: number, dy: number): GameState {
  if (!state.current) return state
  const moved: Piece = { ...state.current, x: state.current.x + dx, y: state.current.y + dy }
  if (!isValid(state.board, moved)) return state
  const grounded = !isValid(state.board, { ...moved, y: moved.y + 1 })
  const withMove = { ...state, current: moved }
  return grounded ? applyLockReset(withMove) : { ...withMove, lockTimer: 0, lockResets: 0 }
}

function tryRotate(state: GameState, dir: 1 | -1): GameState {
  if (!state.current) return state
  if (state.current.type === 'O') return state
  const from = state.current.rotation
  const to = (((from + dir) % 4) + 4) % 4 as Rotation
  const table = state.current.type === 'I' ? KICKS_I : KICKS_JLSTZ
  const offsets = table[`${from}->${to}`] ?? [[0, 0]]
  for (const [dx, dy] of offsets) {
    const candidate: Piece = {
      ...state.current,
      rotation: to,
      x: state.current.x + dx,
      y: state.current.y + dy,
    }
    if (isValid(state.board, candidate)) {
      const grounded = !isValid(state.board, { ...candidate, y: candidate.y + 1 })
      const rotated = { ...state, current: candidate }
      return grounded ? applyLockReset(rotated) : { ...rotated, lockTimer: 0, lockResets: 0 }
    }
  }
  return state
}

function hardDropPiece(state: GameState): GameState {
  if (!state.current) return state
  let piece = state.current
  let dropped = 0
  while (true) {
    const next = { ...piece, y: piece.y + 1 }
    if (!isValid(state.board, next)) break
    piece = next
    dropped++
  }
  return lockCurrent({ ...state, current: piece }, dropped * 2)
}

function softDropOne(state: GameState): GameState {
  if (!state.current) return state
  const next = { ...state.current, y: state.current.y + 1 }
  if (!isValid(state.board, next)) return state
  return { ...state, current: next, score: state.score + 1, dropTimer: 0 }
}

function holdPiece(state: GameState): GameState {
  if (!state.current || state.hasHeld) return state
  const currentType = state.current.type

  if (state.hold) {
    const nextPiece = spawnPiece(state.hold)
    if (!isValid(state.board, nextPiece)) {
      return { ...state, current: null, hold: currentType, status: 'over' }
    }
    return {
      ...state,
      current: nextPiece,
      hold: currentType,
      hasHeld: true,
      lockTimer: 0,
      lockResets: 0,
      dropTimer: 0,
    }
  }

  const refilled = refillQueue(state.queue, state.bag)
  const queue = [...refilled.queue]
  const nextType = queue.shift() as PieceType
  const topped = refillQueue(queue, refilled.bag)
  const nextPiece = spawnPiece(nextType)
  if (!isValid(state.board, nextPiece)) {
    return {
      ...state,
      current: null,
      hold: currentType,
      queue: topped.queue,
      bag: topped.bag,
      status: 'over',
    }
  }
  return {
    ...state,
    current: nextPiece,
    hold: currentType,
    hasHeld: true,
    queue: topped.queue,
    bag: topped.bag,
    lockTimer: 0,
    lockResets: 0,
    dropTimer: 0,
  }
}

function tickState(state: GameState, dt: number): GameState {
  if (state.status !== 'playing') return state

  if (state.lineClear) {
    const elapsed = state.lineClear.elapsed + dt
    if (elapsed >= LINE_CLEAR_MS) return completeLineClear(state)
    return { ...state, lineClear: { ...state.lineClear, elapsed } }
  }

  if (!state.current) return attemptSpawn(state)

  const interval = gravityInterval(state.level)
  let s = state
  let dropTimer = s.dropTimer + dt
  // Apply as many gravity steps as time allows (caps to avoid huge jumps).
  let steps = 0
  while (dropTimer >= interval && steps < 4) {
    const moved: Piece = { ...(s.current as Piece), y: (s.current as Piece).y + 1 }
    if (isValid(s.board, moved)) {
      s = { ...s, current: moved }
      dropTimer -= interval
      steps++
    } else {
      dropTimer = 0
      break
    }
  }
  s = { ...s, dropTimer }

  const below: Piece = { ...(s.current as Piece), y: (s.current as Piece).y + 1 }
  if (!isValid(s.board, below)) {
    const lockTimer = s.lockTimer + dt
    if (lockTimer >= LOCK_DELAY_MS) return lockCurrent(s)
    return { ...s, lockTimer }
  }
  return { ...s, lockTimer: 0 }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createInitialState(): GameState {
  return {
    board: emptyBoard(),
    current: null,
    hold: null,
    hasHeld: false,
    queue: [],
    bag: [],
    score: 0,
    lines: 0,
    level: 0,
    status: 'idle',
    lockTimer: 0,
    lockResets: 0,
    dropTimer: 0,
    lineClear: null,
    lastClearCount: 0,
  }
}

function startFresh(): GameState {
  const base: GameState = {
    board: emptyBoard(),
    current: null,
    hold: null,
    hasHeld: false,
    queue: [],
    bag: [],
    score: 0,
    lines: 0,
    level: 0,
    status: 'playing',
    lockTimer: 0,
    lockResets: 0,
    dropTimer: 0,
    lineClear: null,
    lastClearCount: 0,
  }
  return attemptSpawn(base)
}

export function reduce(state: GameState, action: Action): GameState {
  switch (action.type) {
    case 'start':
      if (state.status === 'playing') return state
      return startFresh()
    case 'reset':
      return startFresh()
    case 'pause':
      if (state.status !== 'playing') return state
      return { ...state, status: 'paused' }
    case 'resume':
      if (state.status !== 'paused') return state
      return { ...state, status: 'playing' }
    case 'tick':
      return tickState(state, action.dt)
    case 'move':
      if (state.status !== 'playing' || state.lineClear) return state
      return tryTranslate(state, action.dx, 0)
    case 'softDrop':
      if (state.status !== 'playing' || state.lineClear) return state
      return softDropOne(state)
    case 'hardDrop':
      if (state.status !== 'playing' || state.lineClear) return state
      return hardDropPiece(state)
    case 'rotate':
      if (state.status !== 'playing' || state.lineClear) return state
      return tryRotate(state, action.dir)
    case 'hold':
      if (state.status !== 'playing' || state.lineClear) return state
      return holdPiece(state)
    default:
      return state
  }
}

export function ghostPieceFor(state: GameState): Piece | null {
  if (!state.current) return null
  let p = state.current
  while (true) {
    const next: Piece = { ...p, y: p.y + 1 }
    if (!isValid(state.board, next)) break
    p = next
  }
  return p
}
