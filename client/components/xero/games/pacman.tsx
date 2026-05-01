"use client"

import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  BOARD_COLS,
  BOARD_ROWS,
  FRIGHTENED_WARN_MS,
  createInitialState,
  reduce,
  type Direction,
  type GameState,
  type Ghost,
} from "./pacman-engine"
import { useGameRunCompletion, type GameRunCompletion } from "./use-game-run-completion"

const COLORS = {
  wall: "#1d4ed8",
  wallHighlight: "rgba(96,165,250,0.55)",
  dot: "#fde68a",
  power: "#fef3c7",
  pacman: "#facc15",
  pacmanShadow: "rgba(250,204,21,0.45)",
  ghostFrightened: "#1e3a8a",
  ghostFrightenedFlash: "#f8fafc",
  ghostEatenEye: "#e2e8f0",
  ghostEatenPupil: "#1e3a8a",
}

const CELL_MIN = 10
const CELL_MAX = 24

interface PacmanProps {
  active: boolean
  onRunComplete?: (run: GameRunCompletion) => void
}

export function Pacman({ active, onRunComplete }: PacmanProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const stageRef = useRef<HTMLDivElement | null>(null)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  const [cellSize, setCellSize] = useState(16)
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const running = state.status === "playing"
  useGameRunCompletion({ status: state.status, score: state.score, onRunComplete })

  // -----------------------------------------------------------------------
  // Size the playfield to the stage, preserving maze aspect.
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

    drawScene(ctx, state, cellSize, fieldWidth, fieldHeight)
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
  const dotsEaten = state.totalDots - state.dotsRemaining

  return (
    <div
      aria-label="Pac-Man"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#0a0d1f] via-[#05060f] to-[#02030a] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
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
              {state.score.toString().padStart(5, "0")}
            </span>
          </span>
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Level</span>
            <span className="tabular-nums text-white/90">{state.level}</span>
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Dots</span>
            <span className="tabular-nums text-white/90">
              {dotsEaten}
              <span className="text-white/40">/{state.totalDots}</span>
            </span>
          </span>
          <span className="flex items-center gap-1.5">
            <span className="text-white/40">Lives</span>
            <div className="flex items-center gap-[3px]">
              {Array.from({ length: Math.max(0, state.lives) }).map((_, i) => (
                <MiniPac key={i} />
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
          className="block rounded-[2px] border border-white/10 shadow-[0_0_18px_rgba(0,0,0,0.45)_inset,0_4px_20px_-8px_rgba(0,0,0,0.6)]"
          ref={canvasRef}
          style={{ width: `${fieldWidth}px`, height: `${fieldHeight}px` }}
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
          <span className="truncate text-white/45">Pac-Man</span>
          <span className="flex items-baseline gap-1 text-white/40">
            <span>Best</span>
            <span className="tabular-nums text-white/80">
              {state.best.toString().padStart(5, "0")}
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
// Canvas drawing
// ---------------------------------------------------------------------------

function drawScene(
  ctx: CanvasRenderingContext2D,
  state: GameState,
  cell: number,
  width: number,
  height: number,
) {
  // Background.
  const bg = ctx.createLinearGradient(0, 0, 0, height)
  bg.addColorStop(0, "#06081a")
  bg.addColorStop(1, "#02030a")
  ctx.fillStyle = bg
  ctx.fillRect(0, 0, width, height)

  drawMaze(ctx, state, cell)
  drawDots(ctx, state, cell)
  drawPacman(ctx, state, cell)
  for (const g of state.ghosts) drawGhost(ctx, g, state, cell)
}

function isWall(state: GameState, x: number, y: number): boolean {
  if (x < 0 || x >= BOARD_COLS || y < 0 || y >= BOARD_ROWS) return false
  return state.cells[y * BOARD_COLS + x] === "wall"
}

function drawMaze(ctx: CanvasRenderingContext2D, state: GameState, cell: number) {
  // Filled wall block per cell with a slim inner highlight along edges that
  // face an open neighbor — mimics the classic outlined Pac-Man wall look.
  const inset = Math.max(1, Math.floor(cell * 0.18))
  for (let y = 0; y < BOARD_ROWS; y++) {
    for (let x = 0; x < BOARD_COLS; x++) {
      if (!isWall(state, x, y)) continue
      const px = x * cell
      const py = y * cell
      ctx.fillStyle = COLORS.wall
      ctx.fillRect(px, py, cell, cell)
      ctx.strokeStyle = COLORS.wallHighlight
      ctx.lineWidth = 1
      // Edges: draw a line along sides where the neighbor is NOT a wall.
      const top = !isWall(state, x, y - 1)
      const bottom = !isWall(state, x, y + 1)
      const left = !isWall(state, x - 1, y)
      const right = !isWall(state, x + 1, y)
      ctx.beginPath()
      if (top) {
        ctx.moveTo(px + inset, py + inset + 0.5)
        ctx.lineTo(px + cell - inset, py + inset + 0.5)
      }
      if (bottom) {
        ctx.moveTo(px + inset, py + cell - inset - 0.5)
        ctx.lineTo(px + cell - inset, py + cell - inset - 0.5)
      }
      if (left) {
        ctx.moveTo(px + inset + 0.5, py + inset)
        ctx.lineTo(px + inset + 0.5, py + cell - inset)
      }
      if (right) {
        ctx.moveTo(px + cell - inset - 0.5, py + inset)
        ctx.lineTo(px + cell - inset - 0.5, py + cell - inset)
      }
      ctx.stroke()
    }
  }
}

function drawDots(ctx: CanvasRenderingContext2D, state: GameState, cell: number) {
  const dotR = Math.max(1, Math.floor(cell * 0.12))
  const powerR = Math.max(2, Math.floor(cell * 0.32))
  const t = performance.now()
  const pulse = 0.65 + 0.35 * Math.sin(t / 220)

  for (let y = 0; y < BOARD_ROWS; y++) {
    for (let x = 0; x < BOARD_COLS; x++) {
      const c = state.cells[y * BOARD_COLS + x]
      if (c !== "dot" && c !== "power") continue
      const cx = x * cell + cell / 2
      const cy = y * cell + cell / 2
      if (c === "dot") {
        ctx.fillStyle = COLORS.dot
        ctx.beginPath()
        ctx.arc(cx, cy, dotR, 0, Math.PI * 2)
        ctx.fill()
      } else {
        ctx.save()
        ctx.globalAlpha = 0.35 * pulse
        ctx.fillStyle = COLORS.power
        ctx.beginPath()
        ctx.arc(cx, cy, powerR + 2, 0, Math.PI * 2)
        ctx.fill()
        ctx.restore()
        ctx.fillStyle = COLORS.power
        ctx.beginPath()
        ctx.arc(cx, cy, powerR * (0.85 + 0.15 * pulse), 0, Math.PI * 2)
        ctx.fill()
      }
    }
  }
}

function dirAngle(dir: Direction): number {
  switch (dir) {
    case "right":
      return 0
    case "down":
      return Math.PI / 2
    case "left":
      return Math.PI
    case "up":
      return -Math.PI / 2
  }
}

function drawPacman(ctx: CanvasRenderingContext2D, state: GameState, cell: number) {
  const pac = state.pacman
  const cx = pac.x * cell + cell / 2
  const cy = pac.y * cell + cell / 2
  const r = cell * 0.45
  const open = state.mouthOpen ? 0.34 : 0.04
  const facing = dirAngle(pac.dir)

  // Shadow halo for depth.
  ctx.save()
  ctx.fillStyle = COLORS.pacmanShadow
  ctx.globalAlpha = 0.45
  ctx.beginPath()
  ctx.arc(cx, cy + 1, r + 1.5, 0, Math.PI * 2)
  ctx.fill()
  ctx.restore()

  ctx.fillStyle = COLORS.pacman
  ctx.beginPath()
  ctx.moveTo(cx, cy)
  ctx.arc(cx, cy, r, facing + open, facing - open + Math.PI * 2)
  ctx.closePath()
  ctx.fill()

  // Tiny eye for character.
  if (state.status !== "over") {
    const eyeOffset = r * 0.5
    let ex = cx
    let ey = cy - eyeOffset
    if (pac.dir === "left" || pac.dir === "right") {
      ex = cx
      ey = cy - eyeOffset
    } else if (pac.dir === "up") {
      ex = cx + eyeOffset
      ey = cy
    } else {
      ex = cx + eyeOffset
      ey = cy
    }
    ctx.fillStyle = "#0f172a"
    ctx.beginPath()
    ctx.arc(ex, ey, Math.max(1, cell * 0.07), 0, Math.PI * 2)
    ctx.fill()
  }
}

function drawGhost(
  ctx: CanvasRenderingContext2D,
  ghost: Ghost,
  state: GameState,
  cell: number,
) {
  const cx = ghost.x * cell + cell / 2
  const cy = ghost.y * cell + cell / 2
  const r = cell * 0.45

  let bodyColor = ghost.color
  let drawBody = true
  if (ghost.mode === "frightened") {
    const flashing =
      state.frightenedTimer > 0 && state.frightenedTimer < FRIGHTENED_WARN_MS
    const flashOn = flashing && Math.floor(state.frightenedTimer / 180) % 2 === 0
    bodyColor = flashOn ? COLORS.ghostFrightenedFlash : COLORS.ghostFrightened
  } else if (ghost.mode === "eaten") {
    drawBody = false
  }

  if (drawBody) {
    ctx.fillStyle = bodyColor
    ctx.beginPath()
    // Top half-circle.
    ctx.arc(cx, cy, r, Math.PI, 0, false)
    // Right side down to skirt.
    const skirtY = cy + r * 0.85
    ctx.lineTo(cx + r, skirtY)
    // Three-hump scalloped bottom (right→left).
    const humps = 3
    const span = (2 * r) / humps
    for (let i = 0; i < humps; i++) {
      const peakX = cx + r - (i + 0.5) * span
      const valleyX = cx + r - (i + 1) * span
      ctx.lineTo(peakX, cy + r * 0.55)
      ctx.lineTo(valleyX, skirtY)
    }
    ctx.closePath()
    ctx.fill()
  }

  // Eyes — drawn in every mode (frightened ghosts get faux pupils, eaten
  // ghosts are eyes-only racing home).
  const eyeR = Math.max(2, cell * 0.13)
  const pupilR = Math.max(1, cell * 0.08)
  const eyeOffsetX = r * 0.38
  const eyeOffsetY = r * 0.18
  const leftEyeX = cx - eyeOffsetX
  const rightEyeX = cx + eyeOffsetX
  const eyeY = cy - eyeOffsetY

  if (ghost.mode === "frightened") {
    // Two small dot-eyes and a wavy mouth.
    ctx.fillStyle = "#fef3c7"
    ctx.beginPath()
    ctx.arc(leftEyeX, eyeY, pupilR, 0, Math.PI * 2)
    ctx.arc(rightEyeX, eyeY, pupilR, 0, Math.PI * 2)
    ctx.fill()
    ctx.strokeStyle = "#fef3c7"
    ctx.lineWidth = Math.max(1, cell * 0.08)
    ctx.beginPath()
    const mouthY = cy + r * 0.18
    const seg = (r * 1.4) / 4
    ctx.moveTo(cx - r * 0.7, mouthY)
    ctx.lineTo(cx - r * 0.7 + seg * 0.5, mouthY - r * 0.12)
    ctx.lineTo(cx - r * 0.7 + seg, mouthY)
    ctx.lineTo(cx - r * 0.7 + seg * 1.5, mouthY - r * 0.12)
    ctx.lineTo(cx - r * 0.7 + seg * 2, mouthY)
    ctx.lineTo(cx - r * 0.7 + seg * 2.5, mouthY - r * 0.12)
    ctx.lineTo(cx - r * 0.7 + seg * 3, mouthY)
    ctx.lineTo(cx - r * 0.7 + seg * 3.5, mouthY - r * 0.12)
    ctx.lineTo(cx - r * 0.7 + seg * 4, mouthY)
    ctx.stroke()
    return
  }

  // Whites of eyes.
  ctx.fillStyle = COLORS.ghostEatenEye
  ctx.beginPath()
  ctx.arc(leftEyeX, eyeY, eyeR, 0, Math.PI * 2)
  ctx.arc(rightEyeX, eyeY, eyeR, 0, Math.PI * 2)
  ctx.fill()

  // Pupils offset toward the direction of travel.
  const v = ghost.dir
  const off = eyeR * 0.45
  const pupilDx = v === "left" ? -off : v === "right" ? off : 0
  const pupilDy = v === "up" ? -off : v === "down" ? off : 0
  ctx.fillStyle = ghost.mode === "eaten" ? COLORS.ghostEatenPupil : "#0f172a"
  ctx.beginPath()
  ctx.arc(leftEyeX + pupilDx, eyeY + pupilDy, pupilR, 0, Math.PI * 2)
  ctx.arc(rightEyeX + pupilDx, eyeY + pupilDy, pupilR, 0, Math.PI * 2)
  ctx.fill()
}

// ---------------------------------------------------------------------------
// Small pieces
// ---------------------------------------------------------------------------

function MiniPac() {
  return (
    <svg
      aria-hidden="true"
      className="h-3 w-3"
      preserveAspectRatio="xMidYMid meet"
      viewBox="0 0 12 12"
    >
      <path
        d="M6 6 L11 2.5 A6 6 0 1 0 11 9.5 Z"
        fill={COLORS.pacman}
      />
    </svg>
  )
}

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→", "↑", "↓"], label: "Move" },
  { keys: ["W", "A", "S", "D"], label: "Move (alt)" },
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
      title: "Pac-Man",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (state.status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Catch your breath",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  if (state.status === "won") {
    return {
      eyebrow: `Level ${state.level} cleared`,
      title: "Maze devoured",
      detail: `Score  ${state.score.toLocaleString()}`,
      button: "Next maze",
      hint: "Enter to advance",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "The ghosts caught up",
    detail: `Final score  ${state.score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
