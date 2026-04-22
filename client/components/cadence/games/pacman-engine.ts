// ---------------------------------------------------------------------------
// Pac-Man engine — pure game state & reducer.
//
// Coordinate space: integer maze cells, +x right, +y DOWN.
// Pacman and ghosts step cell-by-cell on independent timers.
// Row 9 wraps horizontally (the tunnel).
// ---------------------------------------------------------------------------

export const BOARD_COLS = 19
export const BOARD_ROWS = 21

// Maze legend:
//   '#' wall, '.' dot, 'o' power pellet, ' ' empty/path,
//   'G' ghost spawn (passable, no pellet), 'P' pacman spawn (passable, no pellet).
// Row 9 is the tunnel row (open ends).
const MAZE_RAW: readonly string[] = [
  '###################', //  0
  '#........#........#', //  1
  '#o##.###.#.###.##o#', //  2
  '#.................#', //  3
  '#.##.#.#####.#.##.#', //  4
  '#....#...#...#....#', //  5
  '####.###.#.###.####', //  6
  '####.#       #.####', //  7
  '####.# ## ## #.####', //  8 — ghost house top, door at col 9
  '    .  #GGG#  .    ', //  9 — tunnel row + ghost spawns
  '####.# ##### #.####', // 10 — ghost house bottom
  '####.#       #.####', // 11
  '####.###.#.###.####', // 12
  '#........#........#', // 13
  '#.##.###.#.###.##.#', // 14
  '#o.#.....P.....#.o#', // 15 — pacman spawn
  '##.#.#.#####.#.#.##', // 16
  '#....#...#...#....#', // 17
  '#.######.#.######.#', // 18
  '#.................#', // 19
  '###################', // 20
]

// Cells that ghosts may not return to once they've left the house. This keeps
// the door behaving as a one-way gate without a separate cell type.
const GHOST_HOUSE_CELLS = new Set([
  cellKey(8, 9),
  cellKey(9, 9),
  cellKey(10, 9),
])

function cellKey(x: number, y: number): number {
  return y * BOARD_COLS + x
}

// Step intervals (ms). Pacman is fastest; eaten ghosts race home.
export const PACMAN_STEP_MS = 145
export const GHOST_STEP_MS = 175
export const GHOST_FRIGHTENED_STEP_MS = 285
export const GHOST_EATEN_STEP_MS = 80
export const FRIGHTENED_DURATION_MS = 6500
// The "warning" tail of frightened mode — renderer flashes ghosts white.
export const FRIGHTENED_WARN_MS = 1800
export const MOUTH_ANIM_MS = 90

export const SCORE_DOT = 10
export const SCORE_POWER = 50
export const SCORE_GHOST_BASE = 200

const INITIAL_LIVES = 3

const GHOST_PALETTE = ['#ef4444', '#f472b6', '#22d3ee']

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export type CellType = 'wall' | 'dot' | 'power' | 'empty'
export type Direction = 'up' | 'down' | 'left' | 'right'
export type GhostMode = 'chase' | 'frightened' | 'eaten'
export type GameStatus = 'idle' | 'playing' | 'paused' | 'over' | 'won'

export interface Pacman {
  x: number
  y: number
  dir: Direction
  queuedDir: Direction | null
}

export interface Ghost {
  id: number
  color: string
  x: number
  y: number
  dir: Direction
  mode: GhostMode
  homeX: number
  homeY: number
  stepTimer: number
  hasLeftHouse: boolean
}

export interface GameState {
  status: GameStatus
  score: number
  best: number
  level: number
  lives: number
  cells: CellType[]
  pacman: Pacman
  ghosts: Ghost[]
  dotsRemaining: number
  totalDots: number
  frightenedTimer: number
  eatenChain: number
  pacmanStepTimer: number
  pacmanSpawn: { x: number; y: number }
  ghostSpawns: Array<{ x: number; y: number }>
  mouthAnimTimer: number
  mouthOpen: boolean
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'next' }
  | { type: 'tick'; dt: number }
  | { type: 'queueDir'; dir: Direction }

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function dirVec(dir: Direction): { dx: number; dy: number } {
  switch (dir) {
    case 'up':
      return { dx: 0, dy: -1 }
    case 'down':
      return { dx: 0, dy: 1 }
    case 'left':
      return { dx: -1, dy: 0 }
    case 'right':
      return { dx: 1, dy: 0 }
  }
}

function opposite(dir: Direction): Direction {
  switch (dir) {
    case 'up':
      return 'down'
    case 'down':
      return 'up'
    case 'left':
      return 'right'
    case 'right':
      return 'left'
  }
}

function wrapX(x: number): number {
  if (x < 0) return BOARD_COLS - 1
  if (x >= BOARD_COLS) return 0
  return x
}

function isPassable(cells: CellType[], x: number, y: number): boolean {
  if (y < 0 || y >= BOARD_ROWS) return false
  const wx = wrapX(x)
  return cells[cellKey(wx, y)] !== 'wall'
}

// Pacman additionally cannot step into ghost-house cells, even though the
// cells themselves are passable for ghosts. Keeps the maze fair.
function isPassableForPacman(cells: CellType[], x: number, y: number): boolean {
  if (!isPassable(cells, x, y)) return false
  return !GHOST_HOUSE_CELLS.has(cellKey(wrapX(x), y))
}

function ghostInterval(mode: GhostMode): number {
  if (mode === 'frightened') return GHOST_FRIGHTENED_STEP_MS
  if (mode === 'eaten') return GHOST_EATEN_STEP_MS
  return GHOST_STEP_MS
}

function parseMaze(): {
  cells: CellType[]
  pacmanSpawn: { x: number; y: number }
  ghostSpawns: Array<{ x: number; y: number }>
  totalDots: number
} {
  const cells: CellType[] = new Array(BOARD_COLS * BOARD_ROWS).fill('empty')
  const ghostSpawns: Array<{ x: number; y: number }> = []
  let pacmanSpawn = { x: 9, y: 15 }
  let totalDots = 0
  for (let y = 0; y < BOARD_ROWS; y++) {
    const row = MAZE_RAW[y] ?? ''
    for (let x = 0; x < BOARD_COLS; x++) {
      const ch = row[x] ?? ' '
      const idx = cellKey(x, y)
      switch (ch) {
        case '#':
          cells[idx] = 'wall'
          break
        case '.':
          cells[idx] = 'dot'
          totalDots += 1
          break
        case 'o':
          cells[idx] = 'power'
          totalDots += 1
          break
        case 'G':
          cells[idx] = 'empty'
          ghostSpawns.push({ x, y })
          break
        case 'P':
          cells[idx] = 'empty'
          pacmanSpawn = { x, y }
          break
        default:
          cells[idx] = 'empty'
      }
    }
  }
  return { cells, pacmanSpawn, ghostSpawns, totalDots }
}

// ---------------------------------------------------------------------------
// State construction
// ---------------------------------------------------------------------------

function makeGhost(
  id: number,
  spawn: { x: number; y: number },
  staggerMs: number,
): Ghost {
  return {
    id,
    color: GHOST_PALETTE[id % GHOST_PALETTE.length],
    x: spawn.x,
    y: spawn.y,
    dir: id % 2 === 0 ? 'left' : 'right',
    mode: 'chase',
    homeX: spawn.x,
    homeY: spawn.y,
    // Negative timer = wait this many ms before first move. Staggered so the
    // ghosts leave the house in sequence rather than as a clump.
    stepTimer: -staggerMs,
    hasLeftHouse: false,
  }
}

function freshState(best: number, level: number): GameState {
  const { cells, pacmanSpawn, ghostSpawns, totalDots } = parseMaze()
  const ghosts: Ghost[] = ghostSpawns.map((spawn, i) =>
    makeGhost(i, spawn, i * 600),
  )
  return {
    status: 'playing',
    score: 0,
    best,
    level,
    lives: INITIAL_LIVES,
    cells,
    pacman: {
      x: pacmanSpawn.x,
      y: pacmanSpawn.y,
      dir: 'left',
      queuedDir: null,
    },
    ghosts,
    dotsRemaining: totalDots,
    totalDots,
    frightenedTimer: 0,
    eatenChain: 0,
    pacmanStepTimer: 0,
    pacmanSpawn,
    ghostSpawns,
    mouthAnimTimer: 0,
    mouthOpen: true,
  }
}

function resetPositionsAfterDeath(state: GameState): GameState {
  const ghosts = state.ghosts.map((g, i) => {
    const spawn = state.ghostSpawns[i] ?? { x: g.homeX, y: g.homeY }
    return {
      ...g,
      x: spawn.x,
      y: spawn.y,
      dir: i % 2 === 0 ? ('left' as Direction) : ('right' as Direction),
      mode: 'chase' as GhostMode,
      stepTimer: -i * 600,
      hasLeftHouse: false,
    }
  })
  return {
    ...state,
    pacman: {
      x: state.pacmanSpawn.x,
      y: state.pacmanSpawn.y,
      dir: 'left',
      queuedDir: null,
    },
    ghosts,
    pacmanStepTimer: 0,
    frightenedTimer: 0,
    eatenChain: 0,
  }
}

// ---------------------------------------------------------------------------
// Pacman step
// ---------------------------------------------------------------------------

function pacmanStep(state: GameState): GameState {
  const pac = state.pacman
  let dir = pac.dir

  // Honor a queued turn if the new direction is currently passable. Otherwise
  // keep moving forward and try again next step.
  if (pac.queuedDir) {
    const v = dirVec(pac.queuedDir)
    if (isPassableForPacman(state.cells, pac.x + v.dx, pac.y + v.dy)) {
      dir = pac.queuedDir
    }
  }

  const v = dirVec(dir)
  const nx = wrapX(pac.x + v.dx)
  const ny = pac.y + v.dy

  if (!isPassableForPacman(state.cells, nx, ny)) {
    // Blocked. Hold position but remember direction so the renderer can still
    // face the chosen way and queued turns stay armed.
    return { ...state, pacman: { ...pac, dir } }
  }

  let cells = state.cells
  let score = state.score
  let dotsRemaining = state.dotsRemaining
  let frightenedTimer = state.frightenedTimer
  let eatenChain = state.eatenChain
  let ghosts = state.ghosts

  const idx = cellKey(nx, ny)
  const cell = cells[idx]
  if (cell === 'dot') {
    cells = cells.slice()
    cells[idx] = 'empty'
    score += SCORE_DOT
    dotsRemaining -= 1
  } else if (cell === 'power') {
    cells = cells.slice()
    cells[idx] = 'empty'
    score += SCORE_POWER
    dotsRemaining -= 1
    frightenedTimer = FRIGHTENED_DURATION_MS
    eatenChain = 0
    ghosts = ghosts.map((g) =>
      g.mode === 'eaten'
        ? g
        : {
            ...g,
            mode: 'frightened' as GhostMode,
            // Reverse on power-up — classic Pac-Man behavior.
            dir: opposite(g.dir),
          },
    )
  }

  return {
    ...state,
    pacman: { x: nx, y: ny, dir, queuedDir: pac.queuedDir },
    cells,
    score,
    dotsRemaining,
    frightenedTimer,
    eatenChain,
    ghosts,
    best: Math.max(state.best, score),
  }
}

// ---------------------------------------------------------------------------
// Ghost AI
// ---------------------------------------------------------------------------

function ghostTarget(
  ghost: Ghost,
  state: GameState,
): { x: number; y: number } {
  if (ghost.mode === 'eaten') {
    return { x: ghost.homeX, y: ghost.homeY }
  }
  if (!ghost.hasLeftHouse) {
    // Aim straight up at the door so the ghost spills into the maze.
    return { x: 9, y: 6 }
  }
  if (ghost.mode === 'frightened') {
    // Target picked at random in caller; placeholder.
    return { x: ghost.x, y: ghost.y }
  }
  const pac = state.pacman
  if (ghost.id === 0) {
    // Direct chase.
    return { x: pac.x, y: pac.y }
  }
  if (ghost.id === 1) {
    // Ambush: aim 4 tiles ahead of pacman's nose.
    const v = dirVec(pac.dir)
    return { x: pac.x + v.dx * 4, y: pac.y + v.dy * 4 }
  }
  // Shy ghost: chase when distant, retreat when close.
  const dist = Math.abs(ghost.x - pac.x) + Math.abs(ghost.y - pac.y)
  if (dist > 7) return { x: pac.x, y: pac.y }
  return { x: 0, y: BOARD_ROWS - 1 }
}

function pickGhostDirection(ghost: Ghost, state: GameState): Direction {
  const candidates: Direction[] = ['up', 'down', 'left', 'right']
  const back = opposite(ghost.dir)
  // Filter walls and reverse. Eaten ghosts may pass through ghost-house cells;
  // chase/frightened ghosts that have already left may not re-enter.
  const valid = candidates.filter((d) => {
    if (d === back) return false
    const v = dirVec(d)
    const nx = wrapX(ghost.x + v.dx)
    const ny = ghost.y + v.dy
    if (!isPassable(state.cells, nx, ny)) return false
    if (
      ghost.mode !== 'eaten' &&
      ghost.hasLeftHouse &&
      GHOST_HOUSE_CELLS.has(cellKey(nx, ny))
    ) {
      return false
    }
    return true
  })

  if (valid.length === 0) return back
  if (valid.length === 1) return valid[0]

  if (ghost.mode === 'frightened') {
    return valid[Math.floor(Math.random() * valid.length)]
  }

  const target = ghostTarget(ghost, state)
  let bestDir = valid[0]
  let bestDist = Infinity
  for (const d of valid) {
    const v = dirVec(d)
    const tx = ghost.x + v.dx
    const ty = ghost.y + v.dy
    // Squared euclidean distance — matches the original arcade tie-break order
    // closely enough and avoids sqrt in a hot path.
    const dx = tx - target.x
    const dy = ty - target.y
    const dist = dx * dx + dy * dy
    if (dist < bestDist) {
      bestDist = dist
      bestDir = d
    }
  }
  return bestDir
}

function stepGhost(ghost: Ghost, state: GameState): Ghost {
  const dir = pickGhostDirection(ghost, state)
  const v = dirVec(dir)
  const nx = wrapX(ghost.x + v.dx)
  const ny = ghost.y + v.dy
  if (!isPassable(state.cells, nx, ny)) return { ...ghost, dir }

  let mode = ghost.mode
  let hasLeftHouse = ghost.hasLeftHouse
  if (!hasLeftHouse && !GHOST_HOUSE_CELLS.has(cellKey(nx, ny))) {
    hasLeftHouse = true
  }
  if (mode === 'eaten' && nx === ghost.homeX && ny === ghost.homeY) {
    mode = 'chase'
    hasLeftHouse = false
  }

  return { ...ghost, x: nx, y: ny, dir, mode, hasLeftHouse }
}

function advanceGhosts(state: GameState, dt: number): GameState {
  const ghosts = state.ghosts.map((g) => {
    let g2 = g
    let timer = g2.stepTimer + dt
    let interval = ghostInterval(g2.mode)
    let safety = 8
    while (safety-- > 0 && timer >= interval) {
      timer -= interval
      g2 = stepGhost(g2, state)
      interval = ghostInterval(g2.mode)
    }
    return { ...g2, stepTimer: timer }
  })
  return { ...state, ghosts }
}

// ---------------------------------------------------------------------------
// Collisions
// ---------------------------------------------------------------------------

function resolveCollisions(state: GameState): GameState {
  let ghosts = state.ghosts
  let score = state.score
  let lives = state.lives
  let eatenChain = state.eatenChain
  let died = false

  for (let i = 0; i < ghosts.length; i++) {
    const g = ghosts[i]
    if (g.x !== state.pacman.x || g.y !== state.pacman.y) continue
    if (g.mode === 'eaten') continue
    if (g.mode === 'frightened') {
      eatenChain += 1
      score += SCORE_GHOST_BASE * 2 ** (eatenChain - 1)
      ghosts = ghosts.map((gg, j) =>
        j === i ? { ...gg, mode: 'eaten' as GhostMode } : gg,
      )
    } else {
      died = true
      break
    }
  }

  if (died) {
    lives -= 1
    const next: GameState = {
      ...state,
      ghosts,
      score,
      lives,
      eatenChain: 0,
      best: Math.max(state.best, score),
    }
    if (lives < 0) {
      return { ...next, lives: 0, status: 'over' }
    }
    return resetPositionsAfterDeath(next)
  }

  return {
    ...state,
    ghosts,
    score,
    lives,
    eatenChain,
    best: Math.max(state.best, score),
  }
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

function tickState(state: GameState, dt: number): GameState {
  if (state.status !== 'playing') return state
  let s = state

  // Mouth animation drives the pacman sprite open/close cycle.
  let mouthTimer = s.mouthAnimTimer + dt
  let mouthOpen = s.mouthOpen
  let safety = 8
  while (safety-- > 0 && mouthTimer >= MOUTH_ANIM_MS) {
    mouthTimer -= MOUTH_ANIM_MS
    mouthOpen = !mouthOpen
  }
  s = { ...s, mouthAnimTimer: mouthTimer, mouthOpen }

  // Frightened countdown — when it lapses, every frightened ghost reverts.
  if (s.frightenedTimer > 0) {
    const next = Math.max(0, s.frightenedTimer - dt)
    if (next === 0) {
      s = {
        ...s,
        frightenedTimer: 0,
        eatenChain: 0,
        ghosts: s.ghosts.map((g) =>
          g.mode === 'frightened' ? { ...g, mode: 'chase' as GhostMode } : g,
        ),
      }
    } else {
      s = { ...s, frightenedTimer: next }
    }
  }

  // Pacman steps. Loop in case a long frame produced multiple due steps.
  let pacTimer = s.pacmanStepTimer + dt
  safety = 6
  while (safety-- > 0 && pacTimer >= PACMAN_STEP_MS && s.status === 'playing') {
    pacTimer -= PACMAN_STEP_MS
    s = pacmanStep(s)
    s = resolveCollisions(s)
    if (s.status !== 'playing') break
    if (s.dotsRemaining === 0) {
      s = {
        ...s,
        status: 'won',
        best: Math.max(s.best, s.score),
        pacmanStepTimer: 0,
      }
      return s
    }
  }
  s = { ...s, pacmanStepTimer: pacTimer }

  if (s.status !== 'playing') return s

  s = advanceGhosts(s, dt)
  s = resolveCollisions(s)

  return s
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createInitialState(): GameState {
  const base = freshState(0, 1)
  return { ...base, status: 'idle' }
}

export function reduce(state: GameState, action: Action): GameState {
  switch (action.type) {
    case 'start':
      if (state.status === 'playing') return state
      return freshState(state.best, 1)
    case 'reset':
      return freshState(state.best, 1)
    case 'next':
      if (state.status !== 'won') return state
      return freshState(state.best, state.level + 1)
    case 'pause':
      if (state.status !== 'playing') return state
      return { ...state, status: 'paused' }
    case 'resume':
      if (state.status !== 'paused') return state
      return { ...state, status: 'playing' }
    case 'tick':
      return tickState(state, action.dt)
    case 'queueDir':
      if (state.status !== 'playing') return state
      return {
        ...state,
        pacman: { ...state.pacman, queuedDir: action.dir },
      }
    default:
      return state
  }
}
