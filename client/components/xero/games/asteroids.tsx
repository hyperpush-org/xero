"use client"

import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  ASTEROID_VERTEX_COUNT,
  FIELD_HEIGHT,
  FIELD_WIDTH,
  SHIP_RADIUS,
  createInitialState,
  radiusForSize,
  reduce,
  type Asteroid,
  type GameState,
  type Ship,
} from "./asteroids-engine"
import { useGameRunCompletion, type GameRunCompletion } from "./use-game-run-completion"

const COLORS = {
  ship: "#e2e8f0",
  shipFlame: "#fbbf24",
  bullet: "#fef3c7",
  asteroidLine: "#cbd5e1",
  asteroidFill: "rgba(148,163,184,0.10)",
  particle: "253,230,138",
}

// Deterministic starfield — small LCG so the seed is stable across renders.
const STARS: Array<{ x: number; y: number; b: number }> = (() => {
  let s = 0x1f37a
  const rand = () => {
    s = (s * 1103515245 + 12345) & 0x7fffffff
    return s / 0x7fffffff
  }
  const list: Array<{ x: number; y: number; b: number }> = []
  for (let i = 0; i < 55; i++) {
    list.push({
      x: Math.floor(rand() * FIELD_WIDTH),
      y: Math.floor(rand() * FIELD_HEIGHT),
      b: rand(),
    })
  }
  return list
})()

interface AsteroidsProps {
  active: boolean
  onRunComplete?: (run: GameRunCompletion) => void
}

export function Asteroids({ active, onRunComplete }: AsteroidsProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const stageRef = useRef<HTMLDivElement | null>(null)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  const [fitSize, setFitSize] = useState({ w: FIELD_WIDTH * 2, h: FIELD_HEIGHT * 2 })
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const keysRef = useRef({ left: false, right: false, thrust: false, fire: false })

  const running = state.status === "playing"
  useGameRunCompletion({ status: state.status, score: state.score, onRunComplete })

  // -----------------------------------------------------------------------
  // Fit canvas into the stage, preserving aspect ratio.
  // -----------------------------------------------------------------------

  useLayoutEffect(() => {
    const stage = stageRef.current
    if (!stage) return
    const measure = () => {
      const rect = stage.getBoundingClientRect()
      if (rect.width < 1 || rect.height < 1) return
      const aspect = FIELD_WIDTH / FIELD_HEIGHT
      let w = rect.width
      let h = w / aspect
      if (h > rect.height) {
        h = rect.height
        w = h * aspect
      }
      setFitSize({ w: Math.floor(w), h: Math.floor(h) })
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(stage)
    return () => observer.disconnect()
  }, [])

  // -----------------------------------------------------------------------
  // Game loop.
  // -----------------------------------------------------------------------

  useEffect(() => {
    if (!running) return
    let raf = 0
    let last = performance.now()
    const loop = (now: number) => {
      const dt = Math.min(50, now - last)
      last = now

      const keys = keysRef.current
      const dir: -1 | 0 | 1 =
        keys.left && !keys.right ? -1 : keys.right && !keys.left ? 1 : 0
      dispatch({ type: "setRotate", dir })
      dispatch({ type: "setThrust", thrusting: keys.thrust })
      if (keys.fire) dispatch({ type: "fire" })

      dispatch({ type: "tick", dt })
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(raf)
  }, [running])

  useEffect(() => {
    if (!active && state.status === "playing") dispatch({ type: "pause" })
  }, [active, state.status])

  useEffect(() => {
    if (state.status !== "paused") setShowKeybinds(false)
  }, [state.status])

  useEffect(() => {
    const onBlur = () => {
      if (state.status === "playing") dispatch({ type: "pause" })
      keysRef.current.left = false
      keysRef.current.right = false
      keysRef.current.thrust = false
      keysRef.current.fire = false
    }
    window.addEventListener("blur", onBlur)
    return () => window.removeEventListener("blur", onBlur)
  }, [state.status])

  // -----------------------------------------------------------------------
  // Keyboard input.
  // -----------------------------------------------------------------------

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const { key } = event

      if (state.status === "idle") {
        if (key === "Enter" || key === " ") {
          event.preventDefault()
          dispatch({ type: "start" })
        }
        return
      }
      if (state.status === "over") {
        if (key === "Enter" || key === " ") {
          event.preventDefault()
          dispatch({ type: "reset" })
        }
        return
      }
      if (state.status === "won") {
        if (key === "Enter" || key === " ") {
          event.preventDefault()
          dispatch({ type: "next" })
        }
        return
      }
      if (key === "Escape" || key === "p" || key === "P") {
        event.preventDefault()
        dispatch({ type: state.status === "paused" ? "resume" : "pause" })
        return
      }
      if (state.status === "paused") {
        if (key === "Enter") {
          event.preventDefault()
          dispatch({ type: "resume" })
        }
        return
      }

      if (key === "ArrowLeft" || key === "a" || key === "A") {
        event.preventDefault()
        keysRef.current.left = true
      } else if (key === "ArrowRight" || key === "d" || key === "D") {
        event.preventDefault()
        keysRef.current.right = true
      } else if (key === "ArrowUp" || key === "w" || key === "W") {
        event.preventDefault()
        keysRef.current.thrust = true
      } else if (key === " ") {
        event.preventDefault()
        if (!keysRef.current.fire) dispatch({ type: "fire" })
        keysRef.current.fire = true
      } else if (key === "r" || key === "R") {
        event.preventDefault()
        dispatch({ type: "reset" })
      }
    },
    [state.status],
  )

  const handleKeyUp = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const { key } = event
    if (key === "ArrowLeft" || key === "a" || key === "A") keysRef.current.left = false
    else if (key === "ArrowRight" || key === "d" || key === "D") keysRef.current.right = false
    else if (key === "ArrowUp" || key === "w" || key === "W") keysRef.current.thrust = false
    else if (key === " ") keysRef.current.fire = false
  }, [])

  // -----------------------------------------------------------------------
  // Rendering.
  // -----------------------------------------------------------------------

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    if (canvas.width !== FIELD_WIDTH) canvas.width = FIELD_WIDTH
    if (canvas.height !== FIELD_HEIGHT) canvas.height = FIELD_HEIGHT
    const ctx = canvas.getContext("2d")
    if (!ctx) return
    ctx.imageSmoothingEnabled = false
    ctx.setTransform(1, 0, 0, 1, 0, 0)

    // Background — deep-space gradient.
    const bg = ctx.createLinearGradient(0, 0, 0, FIELD_HEIGHT)
    bg.addColorStop(0, "#060b1f")
    bg.addColorStop(1, "#02030a")
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, FIELD_WIDTH, FIELD_HEIGHT)

    // Stars.
    for (const star of STARS) {
      const alpha = 0.15 + star.b * 0.45
      ctx.fillStyle = `rgba(226,232,240,${alpha.toFixed(3)})`
      ctx.fillRect(star.x, star.y, 1, 1)
    }

    // Particles — dots that fade as they age.
    for (const p of state.particles) {
      const alpha = Math.max(0, (p.life / p.maxLife) * 0.9)
      ctx.fillStyle = `rgba(${COLORS.particle},${alpha.toFixed(3)})`
      ctx.fillRect(Math.round(p.x), Math.round(p.y), 1, 1)
    }

    // Asteroids — wireframe polygons, drawn at wrap-around offsets when
    // they straddle an edge so the shape stays continuous.
    ctx.lineWidth = 1
    ctx.strokeStyle = COLORS.asteroidLine
    ctx.fillStyle = COLORS.asteroidFill
    for (const asteroid of state.asteroids) {
      drawAsteroid(ctx, asteroid)
    }

    // Bullets.
    ctx.fillStyle = COLORS.bullet
    for (const b of state.bullets) {
      const bx = Math.round(b.x)
      const by = Math.round(b.y)
      ctx.fillRect(bx, by, 2, 2)
    }

    // Ship — flickers while invulnerable after respawn.
    if (state.ship.alive) {
      const invuln = state.ship.invulnTimer > 0
      const showShip = !invuln || Math.floor(performance.now() / 100) % 2 === 0
      if (showShip) drawShip(ctx, state.ship)
    }
  }, [state])

  // -----------------------------------------------------------------------
  // Focus + overlay handlers.
  // -----------------------------------------------------------------------

  const focusContainer = useCallback(() => {
    containerRef.current?.focus({ preventScroll: true })
  }, [])

  const handleStartClick = useCallback(() => {
    focusContainer()
    if (state.status === "idle") dispatch({ type: "start" })
    else if (state.status === "over") dispatch({ type: "reset" })
    else if (state.status === "paused") dispatch({ type: "resume" })
    else if (state.status === "won") dispatch({ type: "next" })
  }, [state.status, focusContainer])

  const handlePauseToggle = useCallback(() => {
    focusContainer()
    if (state.status === "playing") dispatch({ type: "pause" })
    else if (state.status === "paused") dispatch({ type: "resume" })
  }, [state.status, focusContainer])

  const handleRestartClick = useCallback(() => {
    focusContainer()
    dispatch({ type: "reset" })
  }, [focusContainer])

  useEffect(() => {
    if (active) {
      const handle = window.requestAnimationFrame(() => focusContainer())
      return () => window.cancelAnimationFrame(handle)
    }
    return undefined
  }, [active, focusContainer])

  const overlay = renderOverlay(state)

  return (
    <div
      aria-label="Asteroids"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#0a1128] via-[#05070f] to-[#02030a] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
        "focus-visible:ring-2 focus-visible:ring-primary/60",
      )}
      onBlur={() => setHasFocus(false)}
      onFocus={() => setHasFocus(true)}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      ref={containerRef}
      role="application"
      tabIndex={0}
    >
      {/* Top HUD */}
      <div className="flex h-7 shrink-0 items-center justify-between border-b border-white/10 bg-white/[0.025] px-3 font-mono text-[9.5px] uppercase tracking-[0.22em] text-white/60">
        <div className="flex items-center gap-3">
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Score</span>
            <span className="tabular-nums text-white/90">
              {state.score.toString().padStart(4, "0")}
            </span>
          </span>
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Wave</span>
            <span className="tabular-nums text-white/90">{state.level}</span>
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Rocks</span>
            <span className="tabular-nums text-white/90">{state.asteroids.length}</span>
          </span>
          <span className="flex items-center gap-1.5">
            <span className="text-white/40">Ships</span>
            <div className="flex items-center gap-[3px]">
              {Array.from({ length: Math.max(0, state.lives) }).map((_, i) => (
                <MiniShip key={i} />
              ))}
              {state.lives === 0 ? <span className="text-white/30">—</span> : null}
            </div>
          </span>
        </div>
      </div>

      {/* Playfield */}
      <div
        className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden p-1"
        ref={stageRef}
      >
        <canvas
          className="block rounded-[2px] shadow-[0_0_18px_rgba(0,0,0,0.45)_inset,0_4px_20px_-8px_rgba(0,0,0,0.6)]"
          ref={canvasRef}
          style={{
            imageRendering: "pixelated",
            width: `${fitSize.w}px`,
            height: `${fitSize.h}px`,
          }}
        />
        {overlay ? (
          <div
            aria-live="polite"
            className="pointer-events-none absolute inset-0 flex items-center justify-center"
          >
            <div className="pointer-events-auto flex min-w-[240px] flex-col items-center gap-3 rounded-md border border-white/10 bg-[#06081a]/85 px-5 py-4 text-center shadow-[0_12px_36px_-8px_rgba(0,0,0,0.65)] backdrop-blur-md">
              {state.status === "paused" && showKeybinds ? (
                <KeybindsView onBack={() => setShowKeybinds(false)} />
              ) : (
                <>
                  <div className="font-mono text-[10px] uppercase tracking-[0.32em] text-white/60">
                    {overlay.eyebrow}
                  </div>
                  <div className="text-[18px] font-semibold text-white">{overlay.title}</div>
                  {overlay.detail ? (
                    <div className="font-mono text-[11px] tabular-nums text-white/70">
                      {overlay.detail}
                    </div>
                  ) : null}
                  <button
                    className="mt-1 flex items-center gap-2 rounded-sm border border-white/40 bg-white/5 px-4 py-1.5 font-mono text-[10.5px] uppercase tracking-[0.24em] text-white transition-colors hover:bg-white/10"
                    onClick={handleStartClick}
                    type="button"
                  >
                    <Play className="h-3 w-3 fill-current" />
                    {overlay.button}
                  </button>
                  <div className="font-mono text-[9px] uppercase tracking-[0.2em] text-white/40">
                    {overlay.hint}
                  </div>
                  {state.status === "paused" ? (
                    <button
                      aria-label="Show controls"
                      className="mt-1 flex items-center gap-1.5 rounded-sm border border-white/15 bg-white/5 px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.22em] text-white/70 transition-colors hover:bg-white/10 hover:text-white"
                      onClick={() => setShowKeybinds(true)}
                      type="button"
                    >
                      <Settings2 className="h-3 w-3" />
                      Controls
                    </button>
                  ) : null}
                </>
              )}
            </div>
          </div>
        ) : null}
      </div>

      {/* Bottom strip */}
      <div className="flex h-9 shrink-0 items-center justify-between gap-2 border-t border-white/10 bg-white/[0.025] px-3 font-mono text-[9.5px] uppercase tracking-[0.22em] text-white/60">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-white/45">Asteroids</span>
          <span className="flex items-baseline gap-1 text-white/40">
            <span>Best</span>
            <span className="tabular-nums text-white/80">
              {state.best.toString().padStart(4, "0")}
            </span>
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <button
            aria-label={state.status === "paused" ? "Resume" : "Pause"}
            className="flex h-6 w-6 items-center justify-center rounded-sm border border-white/20 bg-white/10 text-white/80 transition-colors hover:border-white/30 hover:bg-white/15 hover:text-white disabled:opacity-40"
            disabled={state.status !== "playing" && state.status !== "paused"}
            onClick={handlePauseToggle}
            type="button"
          >
            {state.status === "paused" ? (
              <Play className="h-3 w-3 fill-current" />
            ) : (
              <Pause className="h-3 w-3 fill-current" />
            )}
          </button>
          <button
            aria-label="Restart"
            className="flex h-6 w-6 items-center justify-center rounded-sm border border-white/20 bg-white/10 text-white/80 transition-colors hover:border-white/30 hover:bg-white/15 hover:text-white disabled:opacity-40"
            disabled={state.status === "idle"}
            onClick={handleRestartClick}
            type="button"
          >
            <RotateCcw className="h-3 w-3" />
          </button>
        </div>
      </div>

      {!hasFocus && running ? (
        <div className="pointer-events-none absolute inset-x-0 top-9 flex justify-center">
          <span className="rounded-sm bg-white/10 px-2 py-[1px] font-mono text-[9px] uppercase tracking-[0.24em] text-white/70">
            Click to focus
          </span>
        </div>
      ) : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Canvas helpers
// ---------------------------------------------------------------------------

function drawShip(ctx: CanvasRenderingContext2D, ship: Ship) {
  const a = ship.angle
  const sin = Math.sin(a)
  const cos = Math.cos(a)
  const tip = { x: sin * SHIP_RADIUS, y: -cos * SHIP_RADIUS }
  // Wings swept slightly back for a classic vector look.
  const wing = SHIP_RADIUS * 0.9
  const sweep = 2.55
  const left = {
    x: Math.sin(a - sweep) * wing,
    y: -Math.cos(a - sweep) * wing,
  }
  const right = {
    x: Math.sin(a + sweep) * wing,
    y: -Math.cos(a + sweep) * wing,
  }

  ctx.strokeStyle = COLORS.ship
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(ship.x + tip.x, ship.y + tip.y)
  ctx.lineTo(ship.x + left.x, ship.y + left.y)
  // Small notch at the tail so the ship reads as solid, not a chevron.
  ctx.lineTo(ship.x - sin * SHIP_RADIUS * 0.35, ship.y + cos * SHIP_RADIUS * 0.35)
  ctx.lineTo(ship.x + right.x, ship.y + right.y)
  ctx.closePath()
  ctx.stroke()

  // Thruster flame — flickers each frame by time.
  if (ship.thrusting && Math.floor(performance.now() / 70) % 2 === 0) {
    const flameLen = SHIP_RADIUS * 0.95 + Math.random() * 1.5
    const bx = -sin * (SHIP_RADIUS * 0.35 + flameLen)
    const by = cos * (SHIP_RADIUS * 0.35 + flameLen)
    const sideX = cos * SHIP_RADIUS * 0.35
    const sideY = sin * SHIP_RADIUS * 0.35
    ctx.strokeStyle = COLORS.shipFlame
    ctx.beginPath()
    ctx.moveTo(ship.x - sin * SHIP_RADIUS * 0.35 + sideX, ship.y + cos * SHIP_RADIUS * 0.35 + sideY)
    ctx.lineTo(ship.x + bx, ship.y + by)
    ctx.lineTo(ship.x - sin * SHIP_RADIUS * 0.35 - sideX, ship.y + cos * SHIP_RADIUS * 0.35 - sideY)
    ctx.stroke()
  }
}

function drawAsteroid(ctx: CanvasRenderingContext2D, asteroid: Asteroid) {
  const baseR = radiusForSize(asteroid.size)
  const offsets: Array<[number, number]> = [[0, 0]]
  if (asteroid.x - baseR < 0) offsets.push([FIELD_WIDTH, 0])
  if (asteroid.x + baseR > FIELD_WIDTH) offsets.push([-FIELD_WIDTH, 0])
  if (asteroid.y - baseR < 0) offsets.push([0, FIELD_HEIGHT])
  if (asteroid.y + baseR > FIELD_HEIGHT) offsets.push([0, -FIELD_HEIGHT])
  if (asteroid.x - baseR < 0 && asteroid.y - baseR < 0)
    offsets.push([FIELD_WIDTH, FIELD_HEIGHT])
  if (asteroid.x + baseR > FIELD_WIDTH && asteroid.y - baseR < 0)
    offsets.push([-FIELD_WIDTH, FIELD_HEIGHT])
  if (asteroid.x - baseR < 0 && asteroid.y + baseR > FIELD_HEIGHT)
    offsets.push([FIELD_WIDTH, -FIELD_HEIGHT])
  if (asteroid.x + baseR > FIELD_WIDTH && asteroid.y + baseR > FIELD_HEIGHT)
    offsets.push([-FIELD_WIDTH, -FIELD_HEIGHT])

  for (const [ox, oy] of offsets) {
    ctx.beginPath()
    for (let i = 0; i < ASTEROID_VERTEX_COUNT; i++) {
      const theta = (i / ASTEROID_VERTEX_COUNT) * Math.PI * 2 + asteroid.angle
      const r = baseR * asteroid.shape[i]
      const px = asteroid.x + ox + Math.cos(theta) * r
      const py = asteroid.y + oy + Math.sin(theta) * r
      if (i === 0) ctx.moveTo(px, py)
      else ctx.lineTo(px, py)
    }
    ctx.closePath()
    ctx.fill()
    ctx.stroke()
  }
}

// ---------------------------------------------------------------------------
// Small pieces
// ---------------------------------------------------------------------------

function MiniShip() {
  return (
    <svg
      aria-hidden="true"
      className="h-3 w-3"
      preserveAspectRatio="xMidYMid meet"
      viewBox="0 0 8 8"
    >
      <polygon
        points="4,1 6.5,6.5 4,5.5 1.5,6.5"
        fill="none"
        stroke={COLORS.ship}
        strokeWidth="0.6"
        strokeLinejoin="round"
      />
    </svg>
  )
}

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→"], label: "Rotate" },
  { keys: ["A", "D"], label: "Rotate (alt)" },
  { keys: ["↑", "W"], label: "Thrust" },
  { keys: ["Space"], label: "Fire" },
  { keys: ["Esc", "P"], label: "Pause" },
  { keys: ["R"], label: "Restart" },
]

function KeybindsView({ onBack }: { onBack: () => void }) {
  return (
    <>
      <div className="font-mono text-[10px] uppercase tracking-[0.32em] text-white/60">
        Controls
      </div>
      <ul className="mt-0.5 flex flex-col gap-1.5">
        {KEYBINDS.map((bind) => (
          <li
            className="flex items-center justify-between gap-6 text-left font-mono text-[11px] text-white/85"
            key={bind.label}
          >
            <span className="flex items-center gap-1">
              {bind.keys.map((k, i) => (
                <span className="flex items-center gap-1" key={k}>
                  <kbd className="rounded-[3px] border border-white/25 bg-white/10 px-1.5 py-[1px] font-mono text-[10px] text-white/95">
                    {k}
                  </kbd>
                  {i < bind.keys.length - 1 ? (
                    <span className="text-white/40">/</span>
                  ) : null}
                </span>
              ))}
            </span>
            <span className="uppercase tracking-[0.18em] text-white/70">{bind.label}</span>
          </li>
        ))}
      </ul>
      <button
        className="mt-2 flex items-center gap-1.5 rounded-sm border border-white/30 bg-white/5 px-3 py-1 font-mono text-[10px] uppercase tracking-[0.24em] text-white transition-colors hover:bg-white/10"
        onClick={onBack}
        type="button"
      >
        <ArrowLeft className="h-3 w-3" />
        Back
      </button>
    </>
  )
}

function renderOverlay(
  state: GameState,
): { eyebrow: string; title: string; detail?: string; button: string; hint: string } | null {
  if (state.status === "playing") return null
  if (state.status === "idle") {
    return {
      eyebrow: "Arcade",
      title: "Asteroids",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (state.status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Hold the course",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  if (state.status === "won") {
    return {
      eyebrow: `Wave ${state.level} cleared`,
      title: "Debris field empty",
      detail: `Score  ${state.score.toLocaleString()}`,
      button: "Next wave",
      hint: "Enter to advance",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "Ship lost",
    detail: `Final score  ${state.score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
