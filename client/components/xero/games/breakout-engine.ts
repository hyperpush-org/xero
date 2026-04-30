// ---------------------------------------------------------------------------
// Breakout engine — pure game state & reducer.
//
// Coordinate space: logical 240x180 "arcade pixels" with continuous floats,
// +x right, +y DOWN. The component scales this up for display.
// ---------------------------------------------------------------------------

export const FIELD_WIDTH = 240
export const FIELD_HEIGHT = 180

export const WALL_THICKNESS = 4
export const TOP_GUTTER = 10

export const PADDLE_WIDTH = 32
export const PADDLE_HEIGHT = 4
export const PADDLE_Y = 164
export const PADDLE_SPEED = 190 // px per second

export const BALL_SIZE = 3
export const BALL_INITIAL_SPEED = 110
export const BALL_MAX_SPEED = 220
export const BALL_MAX_BOUNCE_ANGLE = (60 * Math.PI) / 180 // from vertical

export const BRICK_COLS = 10
export const BRICK_ROWS = 8
export const BRICK_WIDTH = 22
export const BRICK_HEIGHT = 7
export const BRICK_GAP = 1
export const BRICK_FIELD_TOP = 22

// Centered brick grid.
const BRICK_BLOCK_WIDTH = BRICK_COLS * BRICK_WIDTH + (BRICK_COLS - 1) * BRICK_GAP
export const BRICK_FIELD_LEFT = Math.round((FIELD_WIDTH - BRICK_BLOCK_WIDTH) / 2)

const INITIAL_LIVES = 3
export const LAUNCH_HINT_MS = 650

export type GameStatus = 'idle' | 'playing' | 'paused' | 'over' | 'won'

export interface Ball {
  x: number
  y: number
  vx: number
  vy: number
  attached: boolean
}

export interface Brick {
  col: number
  row: number
  alive: boolean
}

export interface GameState {
  status: GameStatus
  score: number
  best: number
  lives: number
  level: number

  paddleX: number // left edge
  paddleMove: -1 | 0 | 1

  ball: Ball
  speed: number // current ball speed magnitude
  hits: number // paddle+brick hits since last spawn; drives speed ramp

  bricks: Brick[]
  bricksRemaining: number

  launchHintTimer: number // ms since ball attached, for the "press space" hint
}

export type Action =
  | { type: 'start' }
  | { type: 'reset' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'tick'; dt: number }
  | { type: 'setMove'; dir: -1 | 0 | 1 }
  | { type: 'launch' }
  | { type: 'next' }

// ---------------------------------------------------------------------------
// Scoring — rows count down from the top, classic Atari schedule.
// ---------------------------------------------------------------------------

export function brickScore(row: number): number {
  if (row <= 1) return 7
  if (row <= 3) return 5
  if (row <= 5) return 3
  return 1
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeBricks(): Brick[] {
  const bricks: Brick[] = []
  for (let row = 0; row < BRICK_ROWS; row++) {
    for (let col = 0; col < BRICK_COLS; col++) {
      bricks.push({ col, row, alive: true })
    }
  }
  return bricks
}

export function brickRect(brick: Brick): {
  x: number
  y: number
  w: number
  h: number
} {
  return {
    x: BRICK_FIELD_LEFT + brick.col * (BRICK_WIDTH + BRICK_GAP),
    y: BRICK_FIELD_TOP + brick.row * (BRICK_HEIGHT + BRICK_GAP),
    w: BRICK_WIDTH,
    h: BRICK_HEIGHT,
  }
}

function paddleCenter(paddleX: number): number {
  return paddleX + PADDLE_WIDTH / 2
}

function attachedBall(paddleX: number): Ball {
  return {
    x: paddleCenter(paddleX) - BALL_SIZE / 2,
    y: PADDLE_Y - BALL_SIZE - 0.5,
    vx: 0,
    vy: 0,
    attached: true,
  }
}

function launchSpeed(level: number): number {
  return Math.min(BALL_MAX_SPEED, BALL_INITIAL_SPEED + (level - 1) * 10)
}

// Speed ramp milestones — classic breakout speeds up after the 4th hit and
// again after the 12th, plus modest bumps as rows thin out.
function rampSpeed(base: number, hits: number): number {
  let mult = 1
  if (hits >= 4) mult = 1.12
  if (hits >= 12) mult = 1.26
  if (hits >= 24) mult = 1.4
  if (hits >= 40) mult = 1.55
  return Math.min(BALL_MAX_SPEED, base * mult)
}

function freshLevel(level: number, lives: number, score: number, best: number): GameState {
  const paddleX = Math.round((FIELD_WIDTH - PADDLE_WIDTH) / 2)
  return {
    status: 'playing',
    score,
    best,
    lives,
    level,
    paddleX,
    paddleMove: 0,
    ball: attachedBall(paddleX),
    speed: launchSpeed(level),
    hits: 0,
    bricks: makeBricks(),
    bricksRemaining: BRICK_COLS * BRICK_ROWS,
    launchHintTimer: 0,
  }
}

// ---------------------------------------------------------------------------
// Collision helpers
// ---------------------------------------------------------------------------

interface Rect {
  x: number
  y: number
  w: number
  h: number
}

function rectsOverlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

function ballRect(ball: Ball): Rect {
  return { x: ball.x, y: ball.y, w: BALL_SIZE, h: BALL_SIZE }
}

// Reflect the ball off the paddle. Angle depends on where along the paddle
// the ball hit — classic breakout "english".
function bouncePaddle(ball: Ball, paddleX: number, speed: number): Ball {
  const hitCenter = ball.x + BALL_SIZE / 2 - paddleCenter(paddleX)
  const half = PADDLE_WIDTH / 2
  const norm = Math.max(-1, Math.min(1, hitCenter / half))
  const angle = norm * BALL_MAX_BOUNCE_ANGLE
  return {
    ...ball,
    y: PADDLE_Y - BALL_SIZE - 0.01,
    vx: Math.sin(angle) * speed,
    vy: -Math.cos(angle) * speed,
  }
}

// Resolve a swept ball-vs-brick overlap. We know they overlap after the move;
// decide which axis to flip by comparing penetration depths using the pre-move
// position. Returns updated ball and which side was hit.
function resolveBrickHit(
  ball: Ball,
  prevX: number,
  prevY: number,
  rect: Rect,
): Ball {
  const ballW = BALL_SIZE
  const ballH = BALL_SIZE

  const wasLeftOf = prevX + ballW <= rect.x
  const wasRightOf = prevX >= rect.x + rect.w
  const wasAbove = prevY + ballH <= rect.y
  const wasBelow = prevY >= rect.y + rect.h

  // Pure axis cases — the ball came from exactly one side.
  if ((wasLeftOf || wasRightOf) && !(wasAbove || wasBelow)) {
    const vx = -ball.vx
    const x = wasLeftOf ? rect.x - ballW - 0.01 : rect.x + rect.w + 0.01
    return { ...ball, x, vx }
  }
  if ((wasAbove || wasBelow) && !(wasLeftOf || wasRightOf)) {
    const vy = -ball.vy
    const y = wasAbove ? rect.y - ballH - 0.01 : rect.y + rect.h + 0.01
    return { ...ball, y, vy }
  }

  // Diagonal approach — flip the axis with the shallower penetration.
  const penLeft = ball.x + ballW - rect.x
  const penRight = rect.x + rect.w - ball.x
  const penTop = ball.y + ballH - rect.y
  const penBottom = rect.y + rect.h - ball.y
  const minX = Math.min(penLeft, penRight)
  const minY = Math.min(penTop, penBottom)

  if (minX < minY) {
    const vx = -ball.vx
    const x =
      penLeft < penRight ? rect.x - ballW - 0.01 : rect.x + rect.w + 0.01
    return { ...ball, x, vx }
  }
  const vy = -ball.vy
  const y =
    penTop < penBottom ? rect.y - ballH - 0.01 : rect.y + rect.h + 0.01
  return { ...ball, y, vy }
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

function stepPhysics(state: GameState, dt: number): GameState {
  const dtSec = dt / 1000

  // Paddle.
  const minX = WALL_THICKNESS
  const maxX = FIELD_WIDTH - WALL_THICKNESS - PADDLE_WIDTH
  const paddleX = Math.max(
    minX,
    Math.min(maxX, state.paddleX + state.paddleMove * PADDLE_SPEED * dtSec),
  )

  if (state.ball.attached) {
    return {
      ...state,
      paddleX,
      ball: attachedBall(paddleX),
      launchHintTimer: state.launchHintTimer + dt,
    }
  }

  let ball = state.ball
  let bricks = state.bricks
  let bricksRemaining = state.bricksRemaining
  let score = state.score
  let hits = state.hits
  let speed = state.speed

  // Substep by speed so the ball can't tunnel past a brick or the paddle in
  // a single tick. Cap each step at half the ball size.
  const distance = Math.hypot(ball.vx, ball.vy) * dtSec
  const maxStep = BALL_SIZE * 0.5
  const steps = Math.max(1, Math.ceil(distance / maxStep))
  const stepDt = dtSec / steps

  for (let i = 0; i < steps; i++) {
    const prevX = ball.x
    const prevY = ball.y
    let nx = ball.x + ball.vx * stepDt
    let ny = ball.y + ball.vy * stepDt
    let vx = ball.vx
    let vy = ball.vy

    // Side walls.
    if (nx < WALL_THICKNESS) {
      nx = WALL_THICKNESS
      vx = Math.abs(vx)
    } else if (nx + BALL_SIZE > FIELD_WIDTH - WALL_THICKNESS) {
      nx = FIELD_WIDTH - WALL_THICKNESS - BALL_SIZE
      vx = -Math.abs(vx)
    }

    // Top wall.
    if (ny < TOP_GUTTER) {
      ny = TOP_GUTTER
      vy = Math.abs(vy)
    }

    ball = { ...ball, x: nx, y: ny, vx, vy }

    // Miss — ball fell off the bottom.
    if (ball.y > FIELD_HEIGHT) {
      const lives = state.lives - 1
      if (lives <= 0) {
        const best = Math.max(state.best, score)
        return {
          ...state,
          paddleX,
          ball,
          lives: 0,
          score,
          best,
          status: 'over',
          bricks,
          bricksRemaining,
        }
      }
      return {
        ...state,
        paddleX,
        ball: attachedBall(paddleX),
        lives,
        hits: 0,
        speed: launchSpeed(state.level),
        score,
        bricks,
        bricksRemaining,
        launchHintTimer: 0,
      }
    }

    // Paddle — only reflect if moving downward and overlapping the paddle
    // strip. This sidesteps the "ball glued under paddle" edge case.
    if (
      ball.vy > 0 &&
      rectsOverlap(ballRect(ball), {
        x: paddleX,
        y: PADDLE_Y,
        w: PADDLE_WIDTH,
        h: PADDLE_HEIGHT + 2,
      })
    ) {
      hits += 1
      speed = rampSpeed(launchSpeed(state.level), hits)
      ball = bouncePaddle(ball, paddleX, speed)
      continue
    }

    // Bricks. At most one per substep keeps the bounce stable; any further
    // overlaps resolve on subsequent substeps.
    let hitIndex = -1
    for (let b = 0; b < bricks.length; b++) {
      const br = bricks[b]
      if (!br.alive) continue
      const r = brickRect(br)
      if (rectsOverlap(ballRect(ball), r)) {
        hitIndex = b
        break
      }
    }
    if (hitIndex >= 0) {
      const br = bricks[hitIndex]
      const r = brickRect(br)
      ball = resolveBrickHit(ball, prevX, prevY, r)
      bricks = bricks.map((bb, idx) =>
        idx === hitIndex ? { ...bb, alive: false } : bb,
      )
      bricksRemaining -= 1
      score += brickScore(br.row)
      hits += 1
      speed = rampSpeed(launchSpeed(state.level), hits)
      // Re-normalize speed after the flip so the ramp actually bites.
      const mag = Math.hypot(ball.vx, ball.vy) || 1
      ball = {
        ...ball,
        vx: (ball.vx / mag) * speed,
        vy: (ball.vy / mag) * speed,
      }
    }

    if (bricksRemaining <= 0) {
      const best = Math.max(state.best, score)
      return {
        ...state,
        paddleX,
        ball,
        status: 'won',
        bricks,
        bricksRemaining,
        score,
        best,
        hits,
        speed,
      }
    }
  }

  const best = Math.max(state.best, score)
  return {
    ...state,
    paddleX,
    ball,
    bricks,
    bricksRemaining,
    score,
    best,
    hits,
    speed,
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
    case 'setMove':
      if (state.status !== 'playing') return state
      if (state.paddleMove === action.dir) return state
      return { ...state, paddleMove: action.dir }
    case 'launch': {
      if (state.status !== 'playing') return state
      if (!state.ball.attached) return state
      // Launch upward, slightly angled toward the paddle's drift.
      const angle =
        state.paddleMove === 0
          ? (Math.random() * 0.5 - 0.25) * BALL_MAX_BOUNCE_ANGLE
          : state.paddleMove * 0.5 * BALL_MAX_BOUNCE_ANGLE
      const speed = state.speed
      return {
        ...state,
        ball: {
          x: state.ball.x,
          y: state.ball.y,
          vx: Math.sin(angle) * speed,
          vy: -Math.cos(angle) * speed,
          attached: false,
        },
        launchHintTimer: 0,
      }
    }
    default:
      return state
  }
}

