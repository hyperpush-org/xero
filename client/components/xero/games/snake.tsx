"use client"

import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  BOARD_COLS,
  BOARD_ROWS,
  createInitialState,
  reduce,
  type Direction,
  type GameState,
} from "./snake-engine"
import { useGameRunCompletion, type GameRunCompletion } from "./use-game-run-completion"

const COLORS = {
  grid: "rgba(74,222,128,0.05)",
  snakeHead: "#bef264",
  snakeBody: "#4ade80",
  snakeBodyAlt: "#22c55e",
  snakeOutline: "rgba(0,0,0,0.35)",
  food: "#ef4444",
  foodGlow: "rgba(252,165,165,0.55)",
}

const CELL_MIN = 10
const CELL_MAX = 26

interface SnakeProps {
  active: boolean
  onRunComplete?: (run: GameRunCompletion) => void
}

export function Snake({ active, onRunComplete }: SnakeProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const stageRef = useRef<HTMLDivElement | null>(null)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  const [cellSize, setCellSize] = useState(18)
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const running = state.status === "playing"
  useGameRunCompletion({ status: state.status, score: state.score, onRunComplete })

  // -----------------------------------------------------------------------
  // Size the playfield to the stage, preserving grid aspect.
  // -----------------------------------------------------------------------

  useLayoutEffect(() => {
    const stage = stageRef.current
    if (!stage) return
    const measure = () => {
      const rect = stage.getBoundingClientRect()
      if (rect.width < 1 || rect.height < 1) return
      const byWidth = (rect.width - 4) / BOARD_COLS
      const byHeight = (rect.height - 4) / BOARD_ROWS
      const size = Math.max(
        CELL_MIN,
        Math.min(CELL_MAX, Math.floor(Math.min(byWidth, byHeight))),
      )
      setCellSize(size)
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
      const dt = Math.min(100, now - last)
      last = now
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
    }
    window.addEventListener("blur", onBlur)
    return () => window.removeEventListener("blur", onBlur)
  }, [state.status])

  // -----------------------------------------------------------------------
  // Keyboard input.
  // -----------------------------------------------------------------------

  const queue = useCallback((dir: Direction) => {
    dispatch({ type: "queueDir", dir })
  }, [])

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

      if (key === "ArrowUp" || key === "w" || key === "W") {
        event.preventDefault()
        queue("up")
      } else if (key === "ArrowDown" || key === "s" || key === "S") {
        event.preventDefault()
        queue("down")
      } else if (key === "ArrowLeft" || key === "a" || key === "A") {
        event.preventDefault()
        queue("left")
      } else if (key === "ArrowRight" || key === "d" || key === "D") {
        event.preventDefault()
        queue("right")
      } else if (key === "r" || key === "R") {
        event.preventDefault()
        dispatch({ type: "reset" })
      }
    },
    [state.status, queue],
  )

  // -----------------------------------------------------------------------
  // Rendering.
  // -----------------------------------------------------------------------

  const fieldWidth = BOARD_COLS * cellSize
  const fieldHeight = BOARD_ROWS * cellSize

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1
    const pxW = Math.floor(fieldWidth * dpr)
    const pxH = Math.floor(fieldHeight * dpr)
    if (canvas.width !== pxW) canvas.width = pxW
    if (canvas.height !== pxH) canvas.height = pxH
    const ctx = canvas.getContext("2d")
    if (!ctx) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    // Background.
    const bg = ctx.createLinearGradient(0, 0, 0, fieldHeight)
    bg.addColorStop(0, "#07160c")
    bg.addColorStop(1, "#030a07")
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, fieldWidth, fieldHeight)

    // Grid.
    ctx.strokeStyle = COLORS.grid
    ctx.lineWidth = 1
    ctx.beginPath()
    for (let x = 0; x <= BOARD_COLS; x++) {
      ctx.moveTo(x * cellSize + 0.5, 0)
      ctx.lineTo(x * cellSize + 0.5, fieldHeight)
    }
    for (let y = 0; y <= BOARD_ROWS; y++) {
      ctx.moveTo(0, y * cellSize + 0.5)
      ctx.lineTo(fieldWidth, y * cellSize + 0.5)
    }
    ctx.stroke()

    // Food — pulsing glow + solid body.
    {
      const fx = state.food.x * cellSize
      const fy = state.food.y * cellSize
      const t = performance.now()
      const pulse = 0.55 + 0.45 * Math.sin(t / 220)
      ctx.save()
      ctx.globalAlpha = 0.25 + pulse * 0.35
      ctx.fillStyle = COLORS.foodGlow
      const glowInset = Math.max(0, Math.floor(cellSize * 0.08))
      ctx.fillRect(
        fx + glowInset,
        fy + glowInset,
        cellSize - glowInset * 2,
        cellSize - glowInset * 2,
      )
      ctx.restore()
      ctx.fillStyle = COLORS.food
      const inset = Math.max(2, Math.floor(cellSize * 0.22))
      ctx.fillRect(
        fx + inset,
        fy + inset,
        cellSize - inset * 2,
        cellSize - inset * 2,
      )
    }

    // Snake — head brighter, body alternates for a subtle banded look.
    for (let i = state.segments.length - 1; i >= 0; i--) {
      const seg = state.segments[i]
      const x = seg.x * cellSize
      const y = seg.y * cellSize
      const isHead = i === 0
      const color = isHead
        ? COLORS.snakeHead
        : i % 2 === 0
          ? COLORS.snakeBody
          : COLORS.snakeBodyAlt
      ctx.fillStyle = color
      ctx.fillRect(x + 1, y + 1, cellSize - 2, cellSize - 2)
      // Soft top highlight.
      ctx.fillStyle = "rgba(255,255,255,0.18)"
      ctx.fillRect(
        x + 2,
        y + 2,
        cellSize - 4,
        Math.max(1, Math.floor(cellSize * 0.18)),
      )
      // Bottom shadow so the body reads 3D.
      ctx.fillStyle = "rgba(0,0,0,0.24)"
      const shadow = Math.max(1, Math.floor(cellSize * 0.14))
      ctx.fillRect(x + 2, y + cellSize - 2 - shadow, cellSize - 4, shadow)
      // Outline.
      ctx.strokeStyle = COLORS.snakeOutline
      ctx.lineWidth = 1
      ctx.strokeRect(x + 1.5, y + 1.5, cellSize - 3, cellSize - 3)

      // Head eye: face direction of travel.
      if (isHead) {
        ctx.fillStyle = "#0f172a"
        const eye = Math.max(1, Math.floor(cellSize * 0.14))
        const cx = x + cellSize / 2
        const cy = y + cellSize / 2
        const offset = Math.max(2, Math.floor(cellSize * 0.22))
        let e1x = cx - eye - offset
        let e1y = cy - eye
        let e2x = cx + offset
        let e2y = cy - eye
        if (state.direction === "right") {
          e1x = cx + offset - eye
          e1y = cy - offset - eye / 2
          e2x = cx + offset - eye
          e2y = cy + offset - eye / 2
        } else if (state.direction === "left") {
          e1x = cx - offset - eye
          e1y = cy - offset - eye / 2
          e2x = cx - offset - eye
          e2y = cy + offset - eye / 2
        } else if (state.direction === "up") {
          e1x = cx - offset - eye / 2
          e1y = cy - offset - eye
          e2x = cx + offset - eye / 2
          e2y = cy - offset - eye
        } else {
          e1x = cx - offset - eye / 2
          e1y = cy + offset - eye
          e2x = cx + offset - eye / 2
          e2y = cy + offset - eye
        }
        ctx.fillRect(Math.round(e1x), Math.round(e1y), eye, eye)
        ctx.fillRect(Math.round(e2x), Math.round(e2y), eye, eye)
      }
    }
  }, [state, cellSize, fieldWidth, fieldHeight])

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
  const length = state.segments.length

  return (
    <div
      aria-label="Snake"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#0b1f14] via-[#06150c] to-[#020806] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
        "focus-visible:ring-2 focus-visible:ring-primary/60",
      )}
      onBlur={() => setHasFocus(false)}
      onFocus={() => setHasFocus(true)}
      onKeyDown={handleKeyDown}
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
            <span className="text-white/40">Len</span>
            <span className="tabular-nums text-white/90">{length}</span>
          </span>
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Best</span>
            <span className="tabular-nums text-white/90">
              {state.best.toString().padStart(4, "0")}
            </span>
          </span>
        </div>
      </div>

      {/* Playfield */}
      <div
        className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden p-1"
        ref={stageRef}
      >
        <canvas
          className="block rounded-[2px] border border-white/10 shadow-[0_0_18px_rgba(0,0,0,0.45)_inset,0_4px_20px_-8px_rgba(0,0,0,0.6)]"
          ref={canvasRef}
          style={{ width: `${fieldWidth}px`, height: `${fieldHeight}px` }}
        />
        {overlay ? (
          <div
            aria-live="polite"
            className="pointer-events-none absolute inset-0 flex items-center justify-center"
          >
            <div className="pointer-events-auto flex min-w-[240px] flex-col items-center gap-3 rounded-md border border-white/10 bg-[#05100a]/85 px-5 py-4 text-center shadow-[0_12px_36px_-8px_rgba(0,0,0,0.65)] backdrop-blur-md">
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
          <span className="truncate text-white/45">Snake</span>
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
// Keybinds overlay
// ---------------------------------------------------------------------------

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→", "↑", "↓"], label: "Turn" },
  { keys: ["W", "A", "S", "D"], label: "Turn (alt)" },
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
      title: "Snake",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (state.status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Slither steady",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "You bit yourself",
    detail: `Final score  ${state.score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
