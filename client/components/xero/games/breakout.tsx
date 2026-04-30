"use client"

import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  BALL_SIZE,
  FIELD_HEIGHT,
  FIELD_WIDTH,
  LAUNCH_HINT_MS,
  PADDLE_HEIGHT,
  PADDLE_WIDTH,
  PADDLE_Y,
  TOP_GUTTER,
  WALL_THICKNESS,
  brickRect,
  createInitialState,
  reduce,
  type GameState,
} from "./breakout-engine"

const ROW_COLORS = [
  "#ef4444", // red
  "#ef4444",
  "#f97316", // orange
  "#f97316",
  "#facc15", // yellow
  "#facc15",
  "#4ade80", // green
  "#4ade80",
]

const COLORS = {
  wall: "#1e293b",
  wallHighlight: "rgba(148,163,184,0.28)",
  paddle: "#e2e8f0",
  paddleShadow: "rgba(226,232,240,0.35)",
  ball: "#fef3c7",
  ballGlow: "rgba(253,224,71,0.35)",
  brickShade: "rgba(0,0,0,0.25)",
  brickHighlight: "rgba(255,255,255,0.18)",
}

interface BreakoutProps {
  active: boolean
}

export function Breakout({ active }: BreakoutProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const stageRef = useRef<HTMLDivElement | null>(null)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  const [fitSize, setFitSize] = useState({ w: FIELD_WIDTH * 2, h: FIELD_HEIGHT * 2 })
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const keysRef = useRef({ left: false, right: false })

  const running = state.status === "playing"

  // -----------------------------------------------------------------------
  // Fit the canvas into the stage, preserving aspect. Canvas stays at 1:1
  // logical pixels; CSS + pixelated rendering handles the upscale.
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
      dispatch({ type: "setMove", dir })

      dispatch({ type: "tick", dt })
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(raf)
  }, [running])

  // Auto-pause when the panel hides the game or the window loses focus.
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
      } else if (key === " " || key === "Enter") {
        event.preventDefault()
        dispatch({ type: "launch" })
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

    // Background.
    const bg = ctx.createLinearGradient(0, 0, 0, FIELD_HEIGHT)
    bg.addColorStop(0, "#060913")
    bg.addColorStop(1, "#02030a")
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, FIELD_WIDTH, FIELD_HEIGHT)

    // Side + top walls.
    ctx.fillStyle = COLORS.wall
    ctx.fillRect(0, 0, WALL_THICKNESS, FIELD_HEIGHT)
    ctx.fillRect(FIELD_WIDTH - WALL_THICKNESS, 0, WALL_THICKNESS, FIELD_HEIGHT)
    ctx.fillRect(0, 0, FIELD_WIDTH, TOP_GUTTER)
    ctx.fillStyle = COLORS.wallHighlight
    ctx.fillRect(WALL_THICKNESS - 1, TOP_GUTTER, 1, FIELD_HEIGHT - TOP_GUTTER)
    ctx.fillRect(FIELD_WIDTH - WALL_THICKNESS, TOP_GUTTER, 1, FIELD_HEIGHT - TOP_GUTTER)
    ctx.fillRect(WALL_THICKNESS, TOP_GUTTER - 1, FIELD_WIDTH - WALL_THICKNESS * 2, 1)

    // Bricks.
    for (const br of state.bricks) {
      if (!br.alive) continue
      const r = brickRect(br)
      const color = ROW_COLORS[br.row] ?? "#4ade80"
      ctx.fillStyle = color
      ctx.fillRect(r.x, r.y, r.w, r.h)
      // Top highlight.
      ctx.fillStyle = COLORS.brickHighlight
      ctx.fillRect(r.x, r.y, r.w, 1)
      // Bottom + right shade for a subtle bevel.
      ctx.fillStyle = COLORS.brickShade
      ctx.fillRect(r.x, r.y + r.h - 1, r.w, 1)
      ctx.fillRect(r.x + r.w - 1, r.y, 1, r.h)
    }

    // Paddle.
    const px = Math.round(state.paddleX)
    ctx.fillStyle = COLORS.paddleShadow
    ctx.fillRect(px - 1, PADDLE_Y + PADDLE_HEIGHT, PADDLE_WIDTH + 2, 1)
    ctx.fillStyle = COLORS.paddle
    ctx.fillRect(px, PADDLE_Y, PADDLE_WIDTH, PADDLE_HEIGHT)
    ctx.fillStyle = "rgba(255,255,255,0.6)"
    ctx.fillRect(px, PADDLE_Y, PADDLE_WIDTH, 1)

    // Ball.
    if (state.status !== "over") {
      const bx = Math.round(state.ball.x)
      const by = Math.round(state.ball.y)
      ctx.fillStyle = COLORS.ballGlow
      ctx.fillRect(bx - 1, by - 1, BALL_SIZE + 2, BALL_SIZE + 2)
      ctx.fillStyle = COLORS.ball
      ctx.fillRect(bx, by, BALL_SIZE, BALL_SIZE)
    }

    // Launch hint — a small arrow above the paddle while the ball is glued.
    if (
      state.status === "playing" &&
      state.ball.attached &&
      state.launchHintTimer > LAUNCH_HINT_MS
    ) {
      const t = performance.now()
      if (Math.floor(t / 400) % 2 === 0) {
        const cx = Math.round(state.paddleX + PADDLE_WIDTH / 2)
        ctx.fillStyle = "rgba(254,243,199,0.85)"
        // Upward chevron at 3 rows above the ball.
        const ay = PADDLE_Y - BALL_SIZE - 9
        ctx.fillRect(cx - 2, ay + 2, 1, 1)
        ctx.fillRect(cx - 1, ay + 1, 1, 1)
        ctx.fillRect(cx, ay, 1, 1)
        ctx.fillRect(cx + 1, ay + 1, 1, 1)
        ctx.fillRect(cx + 2, ay + 2, 1, 1)
      }
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
      aria-label="Breakout"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#0b1020] via-[#05070f] to-[#02030a] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
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
            <span className="text-white/40">Level</span>
            <span className="tabular-nums text-white/90">{state.level}</span>
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Bricks</span>
            <span className="tabular-nums text-white/90">{state.bricksRemaining}</span>
          </span>
          <span className="flex items-center gap-1.5">
            <span className="text-white/40">Lives</span>
            <div className="flex items-center gap-[3px]">
              {Array.from({ length: Math.max(0, state.lives) }).map((_, i) => (
                <MiniBall key={i} />
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
          <span className="truncate text-white/45">Breakout</span>
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
// Small pieces
// ---------------------------------------------------------------------------

function MiniBall() {
  return (
    <svg
      aria-hidden="true"
      className="h-3 w-3"
      preserveAspectRatio="xMidYMid meet"
      viewBox="0 0 6 6"
    >
      <rect x="1" y="1" width="4" height="4" fill={COLORS.ball} />
    </svg>
  )
}

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→"], label: "Move paddle" },
  { keys: ["A", "D"], label: "Move (alt)" },
  { keys: ["Space", "Enter"], label: "Launch ball" },
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
      title: "Breakout",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (state.status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Steady the paddle",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  if (state.status === "won") {
    return {
      eyebrow: `Level ${state.level} cleared`,
      title: "Wall demolished",
      detail: `Score  ${state.score.toLocaleString()}`,
      button: "Next wall",
      hint: "Enter to advance",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "The ball got away",
    detail: `Final score  ${state.score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
