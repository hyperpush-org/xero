// ---------------------------------------------------------------------------
// Galaga engine — pure game state & reducer.
//
// Coordinate space: logical 224x256 "arcade pixels", +x right, +y DOWN.
// Squadron is arranged in a 4x8 formation that breathes side-to-side while
// individual enemies periodically break ranks to dive-bomb the player.
// ---------------------------------------------------------------------------

export const FIELD_WIDTH = 280
export const FIELD_HEIGHT = 200

export const PLAYER_WIDTH = 13
export const PLAYER_HEIGHT = 10
export const PLAYER_Y = 176
export const PLAYER_SPEED = 130

export const ENEMY_COLS = 8
export const ENEMY_ROWS = 4
export const ENEMY_WIDTH = 12
export const ENEMY_HEIGHT = 10
export const ENEMY_SPACING_X = 18
export const ENEMY_SPACING_Y = 14
export const ENEMY_FORMATION_Y = 24
export const FORMATION_AMPLITUDE = 10
export const FORMATION_PERIOD_MS = 3200

export const PLAYER_BULLET_SPEED = 380
export const ENEMY_BULLET_SPEED = 130
export const PLAYER_BULLET_COOLDOWN = 220
export const BULLET_HEIGHT = 6
export const BULLET_WIDTH = 2
export const MAX_PLAYER_BULLETS = 2

export const DIVE_SPEED = 100
export const DIVE_TURN_RATE = 2.6 // radians per second
export const DIVE_COOLDOWN_MIN = 2200
export const DIVE_COOLDOWN_MAX = 4500

export const GROUND_Y = 192
export const INVASION_Y = PLAYER_Y - 2

const INITIAL_LIVES = 3
const HIT_DURATION = 700
const RESPAWN_DURATION = 1100
const WIN_GRACE = 700

export type EnemyKind = 'boss' | 'butterfly' | 'bee'

export const ENEMY_SCORES: Record<EnemyKind, { formation: number; diving: number }> = {
  boss: { formation: 150, diving: 400 },
  butterfly: { formation: 80, diving: 160 },
  bee: { formation: 50, diving: 100 },
}

export function enemyKindForRow(row: number): EnemyKind {
  if (row === 0) return 'boss'
  if (row === 1) return 'butterfly'
  return 'bee'
}

export interface Enemy {
  id: number
  col: number
  row: number
  alive: boolean
  // 'formation' = sitting in slot, breathing with the whole squad.
  // 'diving' = free-flying along an arc toward the player area.
  mode: 'formation' | 'diving'
  // Absolute position only used while diving (formation slot is implicit).
  x: number
  y: number
  heading: number // radians; 0 = right, PI/2 = down
  diveTimer: number // ms until next dive attempt (only for 'formation')
  divePhase: 'plunge' | 'return'
  fireCooldown: number
}

export interface Bullet {
  x: number
  y: number
  vx: number
  vy: number
  from: 'player' | 'enemy'
}

export interface Explosion {
  x: number
  y: number
  age: number
  kind: 'enemy' | 'player'
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

  enemies: Enemy[]
  formationPhase: number // 0..1 sine position
  nextEnemyId: number

  bullets: Bullet[]

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

function makeEnemies(level: number, startId: number): { enemies: Enemy[]; nextId: number } {
  const enemies: Enemy[] = []
  let id = startId
  for (let row = 0; row < ENEMY_ROWS; row++) {
    for (let col = 0; col < ENEMY_COLS; col++) {
      enemies.push({
        id: id++,
        col,
        row,
        alive: true,
        mode: 'formation',
        x: 0,
        y: 0,
        heading: Math.PI / 2,
        // Grace before the first dive so the opening isn't a swarm: level 1
        // enemies sit for 4–8s before anyone breaks ranks.
        diveTimer:
          DIVE_COOLDOWN_MIN +
          Math.random() * (DIVE_COOLDOWN_MAX - DIVE_COOLDOWN_MIN) +
          (ENEMY_ROWS - row) * 500 +
          1600 -
          Math.min((level - 1) * 260, 2200),
        divePhase: 'plunge',
        fireCooldown: 1200 + Math.random() * 1600,
      })
    }
  }
  return { enemies, nextId: id }
}

export function formationSlotX(col: number, phase: number): number {
  const base =
    (FIELD_WIDTH - (ENEMY_COLS - 1) * ENEMY_SPACING_X - ENEMY_WIDTH) / 2 +
    col * ENEMY_SPACING_X
  return base + Math.sin(phase * Math.PI * 2) * FORMATION_AMPLITUDE
}

export function formationSlotY(row: number): number {
  return ENEMY_FORMATION_Y + row * ENEMY_SPACING_Y
}

export function enemyWorldPos(e: Enemy, state: GameState): { x: number; y: number } {
  if (e.mode === 'diving') return { x: e.x, y: e.y }
  return {
    x: formationSlotX(e.col, state.formationPhase),
    y: formationSlotY(e.row),
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

function freshLevel(level: number, lives: number, score: number, startId = 1): GameState {
  const { enemies, nextId } = makeEnemies(level, startId)
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
    enemies,
    formationPhase: 0,
    nextEnemyId: nextId,
    bullets: [],
    explosions: [],
    winTimer: 0,
  }
}

// Pick at most N formation enemies to begin diving, prefer outer columns so
// the squad unravels visibly instead of spawning divers from the interior.
function chooseDiver(enemies: Enemy[]): Enemy | null {
  let best: Enemy | null = null
  let bestScore = -Infinity
  for (const e of enemies) {
    if (!e.alive || e.mode !== 'formation') continue
    if (e.diveTimer > 0) continue
    // Outer columns + lower rows dive first (visual drama).
    const edge = Math.abs(e.col - (ENEMY_COLS - 1) / 2)
    const score = edge * 2 + e.row + Math.random() * 0.5
    if (score > bestScore) {
      bestScore = score
      best = e
    }
  }
  return best
}

function updateDiver(e: Enemy, dt: number, playerX: number): Enemy {
  const dtSec = dt / 1000
  // Target: plunge toward player, then bank back to top once past the bottom.
  const targetX =
    e.divePhase === 'plunge' ? playerX + PLAYER_WIDTH / 2 : formationSlotX(e.col, 0)
  const targetY = e.divePhase === 'plunge' ? PLAYER_Y + 40 : -ENEMY_HEIGHT - 10

  const dx = targetX - (e.x + ENEMY_WIDTH / 2)
  const dy = targetY - (e.y + ENEMY_HEIGHT / 2)
  const desired = Math.atan2(dy, dx)

  // Rotate heading toward desired, capped by DIVE_TURN_RATE.
  let delta = desired - e.heading
  while (delta > Math.PI) delta -= Math.PI * 2
  while (delta < -Math.PI) delta += Math.PI * 2
  const maxTurn = DIVE_TURN_RATE * dtSec
  const heading = e.heading + Math.max(-maxTurn, Math.min(maxTurn, delta))

  const speed = DIVE_SPEED
  const x = e.x + Math.cos(heading) * speed * dtSec
  const y = e.y + Math.sin(heading) * speed * dtSec

  let divePhase = e.divePhase
  if (divePhase === 'plunge' && y > PLAYER_Y + 8) divePhase = 'return'

  return { ...e, x, y, heading, divePhase }
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

  // --- Formation phase -------------------------------------------------
  let formationPhase = state.formationPhase + dt / FORMATION_PERIOD_MS
  if (formationPhase > 1) formationPhase -= Math.floor(formationPhase)

  // --- Move bullets ----------------------------------------------------
  const movedBullets: Bullet[] = state.bullets
    .map((b) => ({ ...b, x: b.x + b.vx * dtSec, y: b.y + b.vy * dtSec }))
    .filter(
      (b) =>
        b.y > -12 && b.y < FIELD_HEIGHT + 12 && b.x > -6 && b.x < FIELD_WIDTH + 6,
    )

  // --- Update enemies --------------------------------------------------
  let enemies = state.enemies.map((e) => {
    if (!e.alive) return e
    if (e.mode === 'diving') {
      let next = updateDiver(e, dt, playerX)
      // Re-dock when the returning enemy has climbed back past the top.
      if (next.divePhase === 'return' && next.y < -ENEMY_HEIGHT) {
        next = {
          ...next,
          mode: 'formation',
          diveTimer:
            DIVE_COOLDOWN_MIN +
            Math.random() * (DIVE_COOLDOWN_MAX - DIVE_COOLDOWN_MIN),
          divePhase: 'plunge',
          heading: Math.PI / 2,
        }
      }
      return next
    }
    return { ...e, diveTimer: Math.max(0, e.diveTimer - dt) }
  })

  // Active-diver budget. Level 1 allows a single diver so the player has
  // time to read attack patterns; later stages ramp to a cap of 4.
  const maxDivers = Math.min(4, state.level)
  const currentDivers = enemies.reduce((n, e) => (e.alive && e.mode === 'diving' ? n + 1 : n), 0)
  if (currentDivers < maxDivers) {
    const diver = chooseDiver(enemies)
    if (diver) {
      const slotX = formationSlotX(diver.col, formationPhase)
      const slotY = formationSlotY(diver.row)
      enemies = enemies.map((e) =>
        e.id === diver.id
          ? {
              ...e,
              mode: 'diving',
              x: slotX,
              y: slotY,
              heading: Math.PI / 2,
              divePhase: 'plunge',
              fireCooldown: 950 + Math.random() * 800,
            }
          : e,
      )
    }
  }

  // --- Enemy fire (divers only; keeps bullet count manageable) ---------
  const newBullets: Bullet[] = []
  enemies = enemies.map((e) => {
    if (!e.alive || e.mode !== 'diving') return e
    const nextCooldown = e.fireCooldown - dt
    if (nextCooldown <= 0 && e.y > 0 && e.y < PLAYER_Y - 10) {
      const cx = e.x + ENEMY_WIDTH / 2
      const cy = e.y + ENEMY_HEIGHT
      const targetX = playerX + PLAYER_WIDTH / 2
      const dx = targetX - cx
      const dy = PLAYER_Y - cy
      const len = Math.max(1, Math.hypot(dx, dy))
      newBullets.push({
        x: cx - BULLET_WIDTH / 2,
        y: cy,
        vx: (dx / len) * ENEMY_BULLET_SPEED,
        vy: (dy / len) * ENEMY_BULLET_SPEED,
        from: 'enemy',
      })
      return { ...e, fireCooldown: 2000 + Math.random() * 1800 }
    }
    return { ...e, fireCooldown: nextCooldown }
  })

  // --- Player bullets vs enemies --------------------------------------
  const afterEnemyHits: Bullet[] = []
  let scoreGain = 0
  let explosions = state.explosions

  for (const b of movedBullets) {
    if (b.from !== 'player') {
      afterEnemyHits.push(b)
      continue
    }
    let hit = false
    for (let i = 0; i < enemies.length; i++) {
      const e = enemies[i]
      if (!e.alive) continue
      const pos =
        e.mode === 'diving'
          ? { x: e.x, y: e.y }
          : {
              x: formationSlotX(e.col, formationPhase),
              y: formationSlotY(e.row),
            }
      if (
        rectsOverlap(
          { x: b.x, y: b.y, w: BULLET_WIDTH, h: BULLET_HEIGHT },
          { x: pos.x, y: pos.y, w: ENEMY_WIDTH, h: ENEMY_HEIGHT },
        )
      ) {
        enemies[i] = { ...e, alive: false }
        const kind = enemyKindForRow(e.row)
        scoreGain += e.mode === 'diving' ? ENEMY_SCORES[kind].diving : ENEMY_SCORES[kind].formation
        explosions = [
          ...explosions,
          { x: pos.x + ENEMY_WIDTH / 2, y: pos.y + ENEMY_HEIGHT / 2, age: 0, kind: 'enemy' },
        ]
        hit = true
        break
      }
    }
    if (!hit) afterEnemyHits.push(b)
  }

  // --- Combined bullet list with new diver shots ----------------------
  const combinedBullets = [...afterEnemyHits, ...newBullets]

  // --- Enemy contact / bullets vs player ------------------------------
  let lives = state.lives
  let nextHit = playerHitTimer
  let nextRespawn = respawnTimer
  const afterPlayerHits: Bullet[] = []
  const immune = playerHitTimer > 0 || respawnTimer > 0
  const playerRect = { x: playerX, y: PLAYER_Y, w: PLAYER_WIDTH, h: PLAYER_HEIGHT }

  for (const b of combinedBullets) {
    if (b.from === 'enemy' && !immune) {
      if (
        rectsOverlap(
          { x: b.x, y: b.y, w: BULLET_WIDTH, h: BULLET_HEIGHT },
          playerRect,
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

  // Enemy-body vs player collision (diver rams)
  if (!immune) {
    for (let i = 0; i < enemies.length; i++) {
      const e = enemies[i]
      if (!e.alive || e.mode !== 'diving') continue
      if (
        rectsOverlap(
          { x: e.x, y: e.y, w: ENEMY_WIDTH, h: ENEMY_HEIGHT },
          playerRect,
        )
      ) {
        lives = Math.max(0, lives - 1)
        nextHit = HIT_DURATION
        nextRespawn = RESPAWN_DURATION
        enemies[i] = { ...e, alive: false }
        explosions = [
          ...explosions,
          { x: e.x + ENEMY_WIDTH / 2, y: e.y + ENEMY_HEIGHT / 2, age: 0, kind: 'enemy' },
          {
            x: playerX + PLAYER_WIDTH / 2,
            y: PLAYER_Y + PLAYER_HEIGHT / 2,
            age: 0,
            kind: 'player',
          },
        ]
        break
      }
    }
  }

  // --- Age & prune explosions -----------------------------------------
  const agedExplosions = explosions
    .map((e) => ({ ...e, age: e.age + dt }))
    .filter((e) => e.age < (e.kind === 'player' ? 900 : 320))

  // --- Win / lose conditions ------------------------------------------
  let aliveCount = 0
  for (const e of enemies) if (e.alive) aliveCount++

  let s: GameState = {
    ...state,
    playerX,
    playerCooldown,
    playerHitTimer: nextHit,
    respawnTimer: nextRespawn,
    lives,
    enemies,
    formationPhase,
    bullets: afterPlayerHits,
    score: state.score + scoreGain,
    explosions: agedExplosions,
  }

  if (aliveCount === 0) {
    const wt = s.winTimer + dt
    if (wt >= WIN_GRACE) return { ...s, status: 'won', winTimer: 0 }
    return { ...s, winTimer: wt }
  }

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
      return freshLevel(state.level + 1, state.lives, state.score, state.nextEnemyId)
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
      let playerBullets = 0
      for (const b of state.bullets) if (b.from === 'player') playerBullets++
      if (playerBullets >= MAX_PLAYER_BULLETS) return state
      return {
        ...state,
        bullets: [
          ...state.bullets,
          {
            x: state.playerX + Math.floor(PLAYER_WIDTH / 2) - BULLET_WIDTH / 2,
            y: PLAYER_Y - BULLET_HEIGHT,
            vx: 0,
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
