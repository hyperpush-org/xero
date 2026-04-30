// ---------------------------------------------------------------------------
// Asteroids engine — pure game state & reducer.
//
// Coordinate space: logical 240x180 "arcade pixels" with continuous floats,
// +x right, +y DOWN. Angle 0 points up (−y), angle grows clockwise.
// Positions wrap at the field edges; collisions use wrapped distance.
// ---------------------------------------------------------------------------

export const FIELD_WIDTH = 240
export const FIELD_HEIGHT = 180

export const SHIP_RADIUS = 5
export const SHIP_ROTATION_SPEED = 4.2 // rad/s
export const SHIP_THRUST = 90 // px/s²
export const SHIP_DRAG_PER_SEC = 0.55 // velocity *= exp(-drag * dt)
export const SHIP_MAX_SPEED = 110
export const SHIP_INVULN_MS = 2200
export const RESPAWN_DELAY_MS = 1100

export const BULLET_SPEED = 195 // px/s
export const BULLET_LIFETIME = 900 // ms
export const BULLET_COOLDOWN = 220 // ms
export const MAX_BULLETS = 4

export const ASTEROID_LARGE_RADIUS = 13
export const ASTEROID_MEDIUM_RADIUS = 8
export const ASTEROID_SMALL_RADIUS = 4

export const ASTEROID_SPEED_MIN = 18
export const ASTEROID_SPEED_MAX = 42

export const ASTEROID_LARGE_SCORE = 20
export const ASTEROID_MEDIUM_SCORE = 50
export const ASTEROID_SMALL_SCORE = 100

export const ASTEROID_VERTEX_COUNT = 10
export const LEVEL_START_COUNT = 4
export const SAFE_RESPAWN_RADIUS = 30

const INITIAL_LIVES = 3

export type GameStatus = 'idle' | 'playing' | 'paused' | 'over' | 'won'
export type AsteroidSize = 'large' | 'medium' | 'small'

export interface Asteroid {
  id: number
  x: number
  y: number
  vx: number
  vy: number
  size: AsteroidSize
  angle: number
  spin: number
  shape: number[] // radius multipliers at evenly spaced angles
}

export interface Ship {
  x: number
  y: number
  vx: number
  vy: number
  angle: number
  alive: boolean
  invulnTimer: number // ms remaining of invulnerability
  thrusting: boolean
}

export interface Bullet {
  id: number
  x: number
  y: number
  vx: number
  vy: number
  life: number // ms remaining
}

export interface Particle {
  x: number
  y: number
  vx: number
  vy: number
  life: number
  maxLife: number
}

export interface GameState {
  status: GameStatus
  score: number
  best: number
  lives: number
  level: number

  ship: Ship
  bullets: Bullet[]
  asteroids: Asteroid[]
  particles: Particle[]

  rotateDir: -1 | 0 | 1
  thrusting: boolean

  fireCooldown: number
  respawnTimer: number // ms; counts down while ship is dead

  bulletIdCounter: number
  asteroidIdCounter: number
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'tick'; dt: number }
  | { type: 'setRotate'; dir: -1 | 0 | 1 }
  | { type: 'setThrust'; thrusting: boolean }
  | { type: 'fire' }
  | { type: 'next' }

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function wrap(v: number, max: number): number {
  return ((v % max) + max) % max
}

export function radiusForSize(size: AsteroidSize): number {
  if (size === 'large') return ASTEROID_LARGE_RADIUS
  if (size === 'medium') return ASTEROID_MEDIUM_RADIUS
  return ASTEROID_SMALL_RADIUS
}

function scoreForSize(size: AsteroidSize): number {
  if (size === 'large') return ASTEROID_LARGE_SCORE
  if (size === 'medium') return ASTEROID_MEDIUM_SCORE
  return ASTEROID_SMALL_SCORE
}

function makeShape(): number[] {
  const shape: number[] = []
  for (let i = 0; i < ASTEROID_VERTEX_COUNT; i++) {
    shape.push(0.72 + Math.random() * 0.5)
  }
  return shape
}

function randomAsteroidVelocity(): { vx: number; vy: number } {
  const angle = Math.random() * Math.PI * 2
  const speed =
    ASTEROID_SPEED_MIN + Math.random() * (ASTEROID_SPEED_MAX - ASTEROID_SPEED_MIN)
  return { vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed }
}

function randomSpawnAwayFromShip(
  shipX: number,
  shipY: number,
): { x: number; y: number } {
  for (let attempt = 0; attempt < 12; attempt++) {
    const x = Math.random() * FIELD_WIDTH
    const y = Math.random() * FIELD_HEIGHT
    const dx = x - shipX
    const dy = y - shipY
    if (dx * dx + dy * dy > 60 * 60) return { x, y }
  }
  return {
    x: wrap(shipX + FIELD_WIDTH / 2, FIELD_WIDTH),
    y: wrap(shipY + FIELD_HEIGHT / 2, FIELD_HEIGHT),
  }
}

function createAsteroid(
  id: number,
  x: number,
  y: number,
  size: AsteroidSize,
  vx?: number,
  vy?: number,
): Asteroid {
  const v =
    vx !== undefined && vy !== undefined ? { vx, vy } : randomAsteroidVelocity()
  return {
    id,
    x,
    y,
    vx: v.vx,
    vy: v.vy,
    size,
    angle: Math.random() * Math.PI * 2,
    spin: (Math.random() - 0.5) * 1.6,
    shape: makeShape(),
  }
}

function freshShip(): Ship {
  return {
    x: FIELD_WIDTH / 2,
    y: FIELD_HEIGHT / 2,
    vx: 0,
    vy: 0,
    angle: 0,
    alive: true,
    invulnTimer: SHIP_INVULN_MS,
    thrusting: false,
  }
}

function makeAsteroidsForLevel(
  level: number,
  shipX: number,
  shipY: number,
  startId: number,
): { asteroids: Asteroid[]; nextId: number } {
  const count = LEVEL_START_COUNT + (level - 1)
  const asteroids: Asteroid[] = []
  let id = startId
  for (let i = 0; i < count; i++) {
    const pos = randomSpawnAwayFromShip(shipX, shipY)
    asteroids.push(createAsteroid(id++, pos.x, pos.y, 'large'))
  }
  return { asteroids, nextId: id }
}

function freshLevel(
  level: number,
  lives: number,
  score: number,
  best: number,
): GameState {
  const ship = freshShip()
  const { asteroids, nextId } = makeAsteroidsForLevel(level, ship.x, ship.y, 0)
  return {
    status: 'playing',
    score,
    best,
    lives,
    level,
    ship,
    bullets: [],
    asteroids,
    particles: [],
    rotateDir: 0,
    thrusting: false,
    fireCooldown: 0,
    respawnTimer: 0,
    bulletIdCounter: 0,
    asteroidIdCounter: nextId,
  }
}

// Shortest-distance-squared accounting for torus wrap.
function distanceSquaredWrapped(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): number {
  let dx = x1 - x2
  let dy = y1 - y2
  if (dx > FIELD_WIDTH / 2) dx -= FIELD_WIDTH
  else if (dx < -FIELD_WIDTH / 2) dx += FIELD_WIDTH
  if (dy > FIELD_HEIGHT / 2) dy -= FIELD_HEIGHT
  else if (dy < -FIELD_HEIGHT / 2) dy += FIELD_HEIGHT
  return dx * dx + dy * dy
}

function isSafeSpawn(asteroids: Asteroid[], x: number, y: number): boolean {
  for (const a of asteroids) {
    const r = radiusForSize(a.size) + SAFE_RESPAWN_RADIUS
    if (distanceSquaredWrapped(a.x, a.y, x, y) < r * r) return false
  }
  return true
}

function splitAsteroid(
  asteroid: Asteroid,
  bulletVx: number,
  bulletVy: number,
  startId: number,
): { pieces: Asteroid[]; nextId: number } {
  if (asteroid.size === 'small') return { pieces: [], nextId: startId }
  const newSize: AsteroidSize = asteroid.size === 'large' ? 'medium' : 'small'
  const bulletMag = Math.hypot(bulletVx, bulletVy) || 1
  const perpX = -bulletVy / bulletMag
  const perpY = bulletVx / bulletMag
  const jitter = 0.5
  const pieces: Asteroid[] = []
  for (let side = 0; side < 2; side++) {
    const sign = side === 0 ? 1 : -1
    const baseAngle = Math.atan2(perpY * sign, perpX * sign)
    const angle = baseAngle + (Math.random() - 0.5) * jitter
    const speed =
      ASTEROID_SPEED_MIN +
      Math.random() * (ASTEROID_SPEED_MAX - ASTEROID_SPEED_MIN) +
      (newSize === 'small' ? 12 : 6)
    pieces.push(
      createAsteroid(
        startId + side,
        asteroid.x,
        asteroid.y,
        newSize,
        Math.cos(angle) * speed,
        Math.sin(angle) * speed,
      ),
    )
  }
  return { pieces, nextId: startId + 2 }
}

function makeExplosionParticles(x: number, y: number, count: number): Particle[] {
  const particles: Particle[] = []
  for (let i = 0; i < count; i++) {
    const a = Math.random() * Math.PI * 2
    const s = 28 + Math.random() * 70
    const life = 280 + Math.random() * 360
    particles.push({
      x,
      y,
      vx: Math.cos(a) * s,
      vy: Math.sin(a) * s,
      life,
      maxLife: life,
    })
  }
  return particles
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

function stepPhysics(state: GameState, dt: number): GameState {
  const dtSec = dt / 1000

  let ship = state.ship
  let bullets = state.bullets
  let asteroids = state.asteroids
  let particles = state.particles
  let score = state.score
  let lives = state.lives
  let fireCooldown = Math.max(0, state.fireCooldown - dt)
  let respawnTimer = state.respawnTimer
  let bulletIdCounter = state.bulletIdCounter
  let asteroidIdCounter = state.asteroidIdCounter

  // Ship physics.
  if (ship.alive) {
    const angle = ship.angle + state.rotateDir * SHIP_ROTATION_SPEED * dtSec
    let vx = ship.vx
    let vy = ship.vy
    if (state.thrusting) {
      vx += Math.sin(angle) * SHIP_THRUST * dtSec
      vy -= Math.cos(angle) * SHIP_THRUST * dtSec
      const speed = Math.hypot(vx, vy)
      if (speed > SHIP_MAX_SPEED) {
        vx = (vx / speed) * SHIP_MAX_SPEED
        vy = (vy / speed) * SHIP_MAX_SPEED
      }
    }
    const drag = Math.exp(-SHIP_DRAG_PER_SEC * dtSec)
    vx *= drag
    vy *= drag

    ship = {
      ...ship,
      x: wrap(ship.x + vx * dtSec, FIELD_WIDTH),
      y: wrap(ship.y + vy * dtSec, FIELD_HEIGHT),
      vx,
      vy,
      angle,
      invulnTimer: Math.max(0, ship.invulnTimer - dt),
      thrusting: state.thrusting,
    }

    // Thrust trail — emit a small particle behind the ship while thrusting.
    if (state.thrusting && Math.random() < 0.6) {
      const rear = ship.angle + Math.PI
      const rx = ship.x + Math.sin(rear) * SHIP_RADIUS * 0.9
      const ry = ship.y - Math.cos(rear) * SHIP_RADIUS * 0.9
      const spread = (Math.random() - 0.5) * 0.5
      const sp = 40 + Math.random() * 40
      particles = [
        ...particles,
        {
          x: rx,
          y: ry,
          vx: Math.sin(rear + spread) * sp - ship.vx * 0.2,
          vy: -Math.cos(rear + spread) * sp - ship.vy * 0.2,
          life: 220,
          maxLife: 220,
        },
      ]
    }
  } else if (lives > 0) {
    respawnTimer = Math.max(0, respawnTimer - dt)
    if (
      respawnTimer === 0 &&
      isSafeSpawn(asteroids, FIELD_WIDTH / 2, FIELD_HEIGHT / 2)
    ) {
      ship = freshShip()
    }
  }

  // Bullets.
  const liveBullets: Bullet[] = []
  for (const b of bullets) {
    const life = b.life - dt
    if (life <= 0) continue
    liveBullets.push({
      ...b,
      x: wrap(b.x + b.vx * dtSec, FIELD_WIDTH),
      y: wrap(b.y + b.vy * dtSec, FIELD_HEIGHT),
      life,
    })
  }
  bullets = liveBullets

  // Asteroids drift + spin.
  asteroids = asteroids.map((a) => ({
    ...a,
    x: wrap(a.x + a.vx * dtSec, FIELD_WIDTH),
    y: wrap(a.y + a.vy * dtSec, FIELD_HEIGHT),
    angle: a.angle + a.spin * dtSec,
  }))

  // Particles.
  particles = particles
    .map((p) => ({
      ...p,
      x: p.x + p.vx * dtSec,
      y: p.y + p.vy * dtSec,
      life: p.life - dt,
    }))
    .filter((p) => p.life > 0)

  // Bullet vs asteroid collisions.
  const destroyedAsteroidIds = new Set<number>()
  const consumedBulletIds = new Set<number>()
  for (const b of bullets) {
    if (consumedBulletIds.has(b.id)) continue
    for (const a of asteroids) {
      if (destroyedAsteroidIds.has(a.id)) continue
      const r = radiusForSize(a.size)
      if (distanceSquaredWrapped(b.x, b.y, a.x, a.y) < r * r) {
        destroyedAsteroidIds.add(a.id)
        consumedBulletIds.add(b.id)
        score += scoreForSize(a.size)
        const split = splitAsteroid(a, b.vx, b.vy, asteroidIdCounter)
        asteroidIdCounter = split.nextId
        asteroids = [...asteroids, ...split.pieces]
        const burst =
          a.size === 'large' ? 14 : a.size === 'medium' ? 10 : 6
        particles = [...particles, ...makeExplosionParticles(a.x, a.y, burst)]
        break
      }
    }
  }
  if (consumedBulletIds.size > 0) {
    bullets = bullets.filter((b) => !consumedBulletIds.has(b.id))
  }
  if (destroyedAsteroidIds.size > 0) {
    asteroids = asteroids.filter((a) => !destroyedAsteroidIds.has(a.id))
  }

  // Ship vs asteroid.
  if (ship.alive && ship.invulnTimer === 0) {
    for (const a of asteroids) {
      const r = radiusForSize(a.size) + SHIP_RADIUS * 0.75
      if (distanceSquaredWrapped(ship.x, ship.y, a.x, a.y) < r * r) {
        particles = [...particles, ...makeExplosionParticles(ship.x, ship.y, 22)]
        lives -= 1
        ship = { ...ship, alive: false, thrusting: false, vx: 0, vy: 0 }
        if (lives <= 0) {
          const best = Math.max(state.best, score)
          return {
            ...state,
            ship,
            bullets,
            asteroids,
            particles,
            score,
            lives: 0,
            best,
            status: 'over',
            fireCooldown,
            respawnTimer: 0,
            bulletIdCounter,
            asteroidIdCounter,
          }
        }
        respawnTimer = RESPAWN_DELAY_MS
        break
      }
    }
  }

  // Wave cleared.
  if (asteroids.length === 0) {
    const best = Math.max(state.best, score)
    return {
      ...state,
      ship,
      bullets: [],
      asteroids: [],
      particles,
      score,
      lives,
      best,
      status: 'won',
      fireCooldown,
      respawnTimer,
      bulletIdCounter,
      asteroidIdCounter,
    }
  }

  const best = Math.max(state.best, score)
  return {
    ...state,
    ship,
    bullets,
    asteroids,
    particles,
    score,
    lives,
    best,
    fireCooldown,
    respawnTimer,
    bulletIdCounter,
    asteroidIdCounter,
  }
}

function tickState(state: GameState, dt: number): GameState {
  if (state.status !== 'playing') return state
  return stepPhysics(state, dt)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createInitialState(): GameState {
  return { ...freshLevel(1, INITIAL_LIVES, 0, 0), status: 'idle' }
}

export function reduce(state: GameState, action: Action): GameState {
  switch (action.type) {
    case 'start':
      if (state.status === 'playing') return state
      return freshLevel(1, INITIAL_LIVES, 0, state.best)
    case 'reset':
      return freshLevel(1, INITIAL_LIVES, 0, state.best)
    case 'pause':
      if (state.status !== 'playing') return state
      return { ...state, status: 'paused' }
    case 'resume':
      if (state.status !== 'paused') return state
      return { ...state, status: 'playing' }
    case 'next':
      if (state.status !== 'won') return state
      return freshLevel(state.level + 1, state.lives, state.score, state.best)
    case 'tick':
      return tickState(state, action.dt)
    case 'setRotate':
      if (state.status !== 'playing') return state
      if (state.rotateDir === action.dir) return state
      return { ...state, rotateDir: action.dir }
    case 'setThrust':
      if (state.status !== 'playing') return state
      if (state.thrusting === action.thrusting) return state
      return { ...state, thrusting: action.thrusting }
    case 'fire': {
      if (state.status !== 'playing') return state
      if (!state.ship.alive) return state
      if (state.fireCooldown > 0) return state
      if (state.bullets.length >= MAX_BULLETS) return state
      const angle = state.ship.angle
      const bullet: Bullet = {
        id: state.bulletIdCounter,
        x: state.ship.x + Math.sin(angle) * SHIP_RADIUS,
        y: state.ship.y - Math.cos(angle) * SHIP_RADIUS,
        vx: Math.sin(angle) * BULLET_SPEED + state.ship.vx,
        vy: -Math.cos(angle) * BULLET_SPEED + state.ship.vy,
        life: BULLET_LIFETIME,
      }
      return {
        ...state,
        bullets: [...state.bullets, bullet],
        fireCooldown: BULLET_COOLDOWN,
        bulletIdCounter: state.bulletIdCounter + 1,
      }
    }
    default:
      return state
  }
}
