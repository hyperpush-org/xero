// ---------------------------------------------------------------------------
// Space Invaders engine — pure game state & reducer.
//
// Coordinate space: logical 224x256 "arcade pixels", +x right, +y DOWN.
// The component scales this up for display.
// ---------------------------------------------------------------------------

// Field aspect (~1.59) is tuned to the panel's stage area so the scale factor
// stays ≥2× at typical sidebar widths without letterboxing.
export const FIELD_WIDTH = 280
export const FIELD_HEIGHT = 176

export const PLAYER_WIDTH = 13
export const PLAYER_HEIGHT = 8
export const PLAYER_Y = 152
export const PLAYER_SPEED = 105 // px per second

export const ALIEN_COLS = 11
export const ALIEN_ROWS = 5
export const ALIEN_WIDTH = 12
export const ALIEN_HEIGHT = 8
export const ALIEN_SPACING_X = 16
export const ALIEN_SPACING_Y = 12
export const ALIEN_START_X = 32
export const ALIEN_START_Y = 16
export const ALIEN_STEP_X = 2
export const ALIEN_DESCEND_Y = 6

export const PLAYER_BULLET_SPEED = 340
export const ALIEN_BULLET_SPEED = 120
export const PLAYER_BULLET_COOLDOWN = 340
export const BULLET_HEIGHT = 5

export const GROUND_Y = 164
export const INVASION_Y = PLAYER_Y - 2 // aliens reaching this line ends the game

const INITIAL_LIVES = 3
const HIT_DURATION = 700
const RESPAWN_DURATION = 1100
const WIN_GRACE = 600

export type AlienType = 'squid' | 'crab' | 'octopus'

export const ALIEN_SCORES: Record<AlienType, number> = {
  squid: 30,
  crab: 20,
  octopus: 10,
}

export function alienTypeForRow(row: number): AlienType {
  if (row === 0) return 'squid'
  if (row <= 2) return 'crab'
  return 'octopus'
}

export interface Alien {
  col: number
  row: number
  alive: boolean
}

export interface Bullet {
  x: number
  y: number
  vy: number
  from: 'player' | 'alien'
}

export interface Explosion {
  x: number
  y: number
  age: number
  kind: 'alien' | 'player'
}

export type GameStatus = 'idle' | 'playing' | 'paused' | 'over' | 'won'

export interface GameState {
  status: GameStatus
  score: number
  lives: number
  level: number

  playerX: number
  playerMove: -1 | 0 | 1
  playerCooldown: number
  playerHitTimer: number
  respawnTimer: number

  aliens: Alien[]
  alienOffsetX: number
  alienOffsetY: number
  alienDir: 1 | -1
  alienStepTimer: number
  alienAnimFrame: 0 | 1

  bullets: Bullet[]
  alienFireTimer: number

  explosions: Explosion[]
  winTimer: number
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'tick'; dt: number }
  | { type: 'setMove'; dir: -1 | 0 | 1 }
  | { type: 'fire' }
  | { type: 'next' }

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeAliens(): Alien[] {
  const aliens: Alien[] = []
  for (let row = 0; row < ALIEN_ROWS; row++) {
    for (let col = 0; col < ALIEN_COLS; col++) {
      aliens.push({ col, row, alive: true })
    }
  }
  return aliens
}

export function alienWorldPos(a: Alien, state: GameState): { x: number; y: number } {
  return {
    x: ALIEN_START_X + a.col * ALIEN_SPACING_X + state.alienOffsetX,
    y: ALIEN_START_Y + a.row * ALIEN_SPACING_Y + state.alienOffsetY,
  }
}

function rectsOverlap(
  a: { x: number; y: number; w: number; h: number },
  b: { x: number; y: number; w: number; h: number },
): boolean {
  return (
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
  )
}

function alienStepInterval(state: GameState): number {
  let alive = 0
  for (const a of state.aliens) if (a.alive) alive++
  if (alive === 0) return 1000
  const total = ALIEN_COLS * ALIEN_ROWS
  const frac = alive / total
  // 55 aliens -> ~500ms; 1 alien -> ~40ms.
  const base = 460 * frac + 45
  const levelMult = Math.max(0.35, 1 - (state.level - 1) * 0.12)
  return base * levelMult
}

function freshLevel(level: number, lives: number, score: number): GameState {
  return {
    status: 'playing',
    score,
    lives,
    level,
    playerX: Math.round((FIELD_WIDTH - PLAYER_WIDTH) / 2),
    playerMove: 0,
    playerCooldown: 0,
    playerHitTimer: 0,
    respawnTimer: 0,
    aliens: makeAliens(),
    alienOffsetX: 0,
    alienOffsetY: Math.min(6 * (level - 1), 24),
    alienDir: 1,
    alienStepTimer: 0,
    alienAnimFrame: 0,
    bullets: [],
    alienFireTimer: 900,
    explosions: [],
    winTimer: 0,
  }
}

function stepAliens(state: GameState): GameState {
  let minCol = ALIEN_COLS
  let maxCol = -1
  for (const a of state.aliens) {
    if (!a.alive) continue
    if (a.col < minCol) minCol = a.col
    if (a.col > maxCol) maxCol = a.col
  }
  if (maxCol < 0) return state

  const nextDir = state.alienDir
  const tryLeft =
    ALIEN_START_X + minCol * ALIEN_SPACING_X + state.alienOffsetX + nextDir * ALIEN_STEP_X
  const tryRight =
    ALIEN_START_X +
    maxCol * ALIEN_SPACING_X +
    ALIEN_WIDTH +
    state.alienOffsetX +
    nextDir * ALIEN_STEP_X

  const animFlip: 0 | 1 = state.alienAnimFrame === 0 ? 1 : 0

  if (tryLeft < 6 || tryRight > FIELD_WIDTH - 6) {
    return {
      ...state,
      alienDir: -nextDir as 1 | -1,
      alienOffsetY: state.alienOffsetY + ALIEN_DESCEND_Y,
      alienAnimFrame: animFlip,
    }
  }

  return {
    ...state,
    alienOffsetX: state.alienOffsetX + nextDir * ALIEN_STEP_X,
    alienAnimFrame: animFlip,
  }
}

function tickState(state: GameState, dt: number): GameState {
  if (state.status !== 'playing') return state

  const dtSec = dt / 1000

  // --- Player movement -------------------------------------------------
  let playerX = state.playerX
  if (state.playerHitTimer <= 0) {
    playerX += state.playerMove * PLAYER_SPEED * dtSec
    playerX = Math.max(6, Math.min(FIELD_WIDTH - 6 - PLAYER_WIDTH, playerX))
  }

  const playerHitTimer = Math.max(0, state.playerHitTimer - dt)
  const respawnTimer = Math.max(0, state.respawnTimer - dt)
  const playerCooldown = Math.max(0, state.playerCooldown - dt)

  // --- Move bullets ----------------------------------------------------
  const movedBullets: Bullet[] = state.bullets
    .map((b) => ({ ...b, y: b.y + b.vy * dtSec }))
    .filter((b) => b.y > -10 && b.y < FIELD_HEIGHT + 10 && b.y < GROUND_Y + 2)

  // --- Player bullets vs aliens ---------------------------------------
  const aliens = state.aliens.map((a) => ({ ...a }))
  const afterAlienHits: Bullet[] = []
  let scoreGain = 0
  let explosions = state.explosions

  for (const b of movedBullets) {
    if (b.from !== 'player') {
      afterAlienHits.push(b)
      continue
    }
    let hit = false
    for (let i = 0; i < aliens.length; i++) {
      const a = aliens[i]
      if (!a.alive) continue
      const pos = alienWorldPos(a, state)
      if (
        rectsOverlap(
          { x: b.x, y: b.y, w: 1, h: BULLET_HEIGHT },
          { x: pos.x, y: pos.y, w: ALIEN_WIDTH, h: ALIEN_HEIGHT },
        )
      ) {
        a.alive = false
        scoreGain += ALIEN_SCORES[alienTypeForRow(a.row)]
        explosions = [
          ...explosions,
          {
            x: pos.x + ALIEN_WIDTH / 2,
            y: pos.y + ALIEN_HEIGHT / 2,
            age: 0,
            kind: 'alien',
          },
        ]
        hit = true
        break
      }
    }
    if (!hit) afterAlienHits.push(b)
  }

  // --- Alien bullets vs player ----------------------------------------
  let lives = state.lives
  let nextHit = playerHitTimer
  let nextRespawn = respawnTimer
  const afterPlayerHits: Bullet[] = []
  const immune = playerHitTimer > 0 || respawnTimer > 0

  for (const b of afterAlienHits) {
    if (b.from === 'alien' && !immune) {
      if (
        rectsOverlap(
          { x: b.x, y: b.y, w: 3, h: BULLET_HEIGHT },
          { x: playerX, y: PLAYER_Y, w: PLAYER_WIDTH, h: PLAYER_HEIGHT },
        )
      ) {
        lives = Math.max(0, lives - 1)
        nextHit = HIT_DURATION
        nextRespawn = RESPAWN_DURATION
        explosions = [
          ...explosions,
          {
            x: playerX + PLAYER_WIDTH / 2,
            y: PLAYER_Y + PLAYER_HEIGHT / 2,
            age: 0,
            kind: 'player',
          },
        ]
        continue
      }
    }
    afterPlayerHits.push(b)
  }

  // --- Assemble mid-state and advance alien march timer ---------------
  let s: GameState = {
    ...state,
    playerX,
    playerCooldown,
    playerHitTimer: nextHit,
    respawnTimer: nextRespawn,
    lives,
    aliens,
    bullets: afterPlayerHits,
    score: state.score + scoreGain,
    explosions,
  }

  let alienStepTimer = s.alienStepTimer + dt
  const interval = alienStepInterval(s)
  let safety = 4
  while (alienStepTimer >= interval && safety-- > 0) {
    alienStepTimer -= interval
    s = stepAliens(s)
  }
  s = { ...s, alienStepTimer }

  // --- Age & prune explosions -----------------------------------------
  s = {
    ...s,
    explosions: s.explosions
      .map((e) => ({ ...e, age: e.age + dt }))
      .filter((e) => e.age < (e.kind === 'player' ? 900 : 300)),
  }

  // --- Alien fire -----------------------------------------------------
  let alienFireTimer = s.alienFireTimer - dt
  if (alienFireTimer <= 0) {
    const baseDelay = 550 + Math.random() * 950 - s.level * 55
    alienFireTimer = Math.max(180, baseDelay)
    // Shoot from bottom-most alien of a random column.
    const columns = new Map<number, Alien>()
    for (const a of s.aliens) {
      if (!a.alive) continue
      const existing = columns.get(a.col)
      if (!existing || a.row > existing.row) columns.set(a.col, a)
    }
    const candidates = Array.from(columns.values())
    if (candidates.length > 0) {
      const shooter = candidates[Math.floor(Math.random() * candidates.length)]
      const pos = alienWorldPos(shooter, s)
      s = {
        ...s,
        bullets: [
          ...s.bullets,
          {
            x: pos.x + ALIEN_WIDTH / 2 - 1,
            y: pos.y + ALIEN_HEIGHT,
            vy: ALIEN_BULLET_SPEED,
            from: 'alien',
          },
        ],
      }
    }
  }
  s = { ...s, alienFireTimer }

  // --- Win / lose conditions ------------------------------------------
  let aliveCount = 0
  for (const a of s.aliens) if (a.alive) aliveCount++
  if (aliveCount === 0) {
    const wt = s.winTimer + dt
    if (wt >= WIN_GRACE) return { ...s, status: 'won', winTimer: 0 }
    return { ...s, winTimer: wt }
  }

  let lowest = 0
  for (const a of s.aliens) {
    if (!a.alive) continue
    const y = ALIEN_START_Y + a.row * ALIEN_SPACING_Y + s.alienOffsetY + ALIEN_HEIGHT
    if (y > lowest) lowest = y
  }
  if (lowest >= INVASION_Y) return { ...s, status: 'over' }

  if (s.lives <= 0 && s.playerHitTimer <= 0 && s.respawnTimer <= 0) {
    return { ...s, status: 'over' }
  }

  return s
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createInitialState(): GameState {
  return { ...freshLevel(1, INITIAL_LIVES, 0), status: 'idle' }
}

export function reduce(state: GameState, action: Action): GameState {
  switch (action.type) {
    case 'start':
      if (state.status === 'playing') return state
      return freshLevel(1, INITIAL_LIVES, 0)
    case 'reset':
      return freshLevel(1, INITIAL_LIVES, 0)
    case 'pause':
      if (state.status !== 'playing') return state
      return { ...state, status: 'paused' }
    case 'resume':
      if (state.status !== 'paused') return state
      return { ...state, status: 'playing' }
    case 'next':
      if (state.status !== 'won') return state
      return freshLevel(state.level + 1, state.lives, state.score)
    case 'tick':
      return tickState(state, action.dt)
    case 'setMove':
      if (state.status !== 'playing') return state
      if (state.playerMove === action.dir) return state
      return { ...state, playerMove: action.dir }
    case 'fire': {
      if (state.status !== 'playing') return state
      if (state.playerCooldown > 0) return state
      if (state.playerHitTimer > 0 || state.respawnTimer > 0) return state
      // Classic SI: only one player bullet on the screen at a time.
      for (const b of state.bullets) if (b.from === 'player') return state
      return {
        ...state,
        bullets: [
          ...state.bullets,
          {
            x: state.playerX + Math.floor(PLAYER_WIDTH / 2),
            y: PLAYER_Y - BULLET_HEIGHT,
            vy: -PLAYER_BULLET_SPEED,
            from: 'player',
          },
        ],
        playerCooldown: PLAYER_BULLET_COOLDOWN,
      }
    }
    default:
      return state
  }
}
