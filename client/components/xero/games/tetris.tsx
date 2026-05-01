"use client"

import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  ARR_MS,
  BOARD_HEIGHT,
  BOARD_WIDTH,
  DAS_MS,
  LINE_CLEAR_MS,
  PIECE_COLORS,
  PIECE_ROTATIONS,
  PIECE_TYPES,
  SOFT_DROP_MS,
  createInitialState,
  ghostPieceFor,
  pieceCells,
  reduce,
  type GameState,
  type PieceType,
} from "./tetris-engine"
import { useGameRunCompletion, type GameRunCompletion } from "./use-game-run-completion"

// ---------------------------------------------------------------------------
// Input state — per-key DAS/ARR trackers.
// ---------------------------------------------------------------------------

interface RepeatKey {
  held: boolean
  heldFor: number
  repeating: boolean
  repeatTimer: number
}

function newRepeatKey(): RepeatKey {
  return { held: false, heldFor: 0, repeating: false, repeatTimer: 0 }
}

function resetKey(k: RepeatKey) {
  k.held = false
  k.heldFor = 0
  k.repeating = false
  k.repeatTimer = 0
}

function processRepeat(
  k: RepeatKey,
  dt: number,
  das: number,
  arr: number,
  fire: () => void,
) {
  if (!k.held) return
  if (!k.repeating) {
    k.heldFor += dt
    if (k.heldFor >= das) {
      k.repeating = true
      k.repeatTimer = 0
    }
    return
  }
  k.repeatTimer += dt
  let safety = 32
  while (k.repeatTimer >= arr && safety-- > 0) {
    fire()
    if (arr <= 0) {
      k.repeatTimer = 0
      break
    }
    k.repeatTimer -= arr
  }
}

// ---------------------------------------------------------------------------
// Canvas drawing helpers.
// ---------------------------------------------------------------------------

function drawFilledCell(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  size: number,
  color: string,
) {
  ctx.fillStyle = color
  ctx.fillRect(x, y, size, size)

  const inset = 1
  // top/left highlight — brighter so pieces pop against the lighter field
  ctx.fillStyle = "rgba(255,255,255,0.3)"
  ctx.fillRect(x + inset, y + inset, size - inset * 2, Math.max(1, size * 0.18))
  ctx.fillRect(x + inset, y + inset, Math.max(1, size * 0.18), size - inset * 2)
  // bottom/right shadow — softer so cells feel less heavy
  ctx.fillStyle = "rgba(0,0,0,0.22)"
  const shadow = Math.max(1, size * 0.12)
  ctx.fillRect(x + inset, y + size - inset - shadow, size - inset * 2, shadow)
  ctx.fillRect(x + size - inset - shadow, y + inset, shadow, size - inset * 2)

  ctx.strokeStyle = "rgba(0,0,0,0.38)"
  ctx.lineWidth = 1
  ctx.strokeRect(x + 0.5, y + 0.5, size - 1, size - 1)
}

function drawGhostCell(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  size: number,
  color: string,
) {
  ctx.save()
  ctx.globalAlpha = 0.18
  ctx.fillStyle = color
  ctx.fillRect(x + 2, y + 2, size - 4, size - 4)
  ctx.globalAlpha = 0.85
  ctx.strokeStyle = color
  ctx.lineWidth = 1.5
  ctx.strokeRect(x + 2, y + 2, size - 4, size - 4)
  ctx.restore()
}

function drawFlashCell(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  size: number,
  progress: number,
) {
  const alpha = 1 - progress
  ctx.fillStyle = `rgba(255,255,255,${alpha.toFixed(3)})`
  ctx.fillRect(x, y, size, size)
}

function drawGrid(
  ctx: CanvasRenderingContext2D,
  cols: number,
  rows: number,
  size: number,
) {
  ctx.save()
  ctx.strokeStyle = "rgba(148,163,184,0.09)"
  ctx.lineWidth = 1
  ctx.beginPath()
  for (let x = 0; x <= cols; x++) {
    ctx.moveTo(x * size + 0.5, 0)
    ctx.lineTo(x * size + 0.5, rows * size)
  }
  for (let y = 0; y <= rows; y++) {
    ctx.moveTo(0, y * size + 0.5)
    ctx.lineTo(cols * size, y * size + 0.5)
  }
  ctx.stroke()
  ctx.restore()
}

function drawPieceInto(
  ctx: CanvasRenderingContext2D,
  type: PieceType,
  rotation: 0 | 1 | 2 | 3,
  cellSize: number,
  originX: number,
  originY: number,
) {
  const cells = PIECE_ROTATIONS[type][rotation]
  const color = PIECE_COLORS[type]
  for (const [cx, cy] of cells) {
    drawFilledCell(
      ctx,
      originX + cx * cellSize,
      originY + cy * cellSize,
      cellSize,
      color,
    )
  }
}

function drawCenteredPiece(
  ctx: CanvasRenderingContext2D,
  type: PieceType,
  cssW: number,
  cssH: number,
  cellSize: number,
) {
  const cells = PIECE_ROTATIONS[type][0]
  let minX = Infinity
  let maxX = -Infinity
  let minY = Infinity
  let maxY = -Infinity
  for (const [cx, cy] of cells) {
    if (cx < minX) minX = cx
    if (cx > maxX) maxX = cx
    if (cy < minY) minY = cy
    if (cy > maxY) maxY = cy
  }
  const w = (maxX - minX + 1) * cellSize
  const h = (maxY - minY + 1) * cellSize
  const originX = Math.round((cssW - w) / 2 - minX * cellSize)
  const originY = Math.round((cssH - h) / 2 - minY * cellSize)
  drawPieceInto(ctx, type, 0, cellSize, originX, originY)
}

function setupCanvas(
  canvas: HTMLCanvasElement,
  cssW: number,
  cssH: number,
): CanvasRenderingContext2D | null {
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1
  canvas.width = Math.floor(cssW * dpr)
  canvas.height = Math.floor(cssH * dpr)
  canvas.style.width = `${cssW}px`
  canvas.style.height = `${cssH}px`
  const ctx = canvas.getContext("2d")
  if (!ctx) return null
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  return ctx
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

const NEXT_COUNT = 4
const PANEL_COLS = 5
const HORIZONTAL_CHROME = 32 // inner flex padding (16) + 2 column gaps (16)
const VERTICAL_CHROME = 58 // inner flex padding (16) + bottom strip (~36) + border (1) + safety
const CELL_MIN = 10
const CELL_MAX = 30

interface TetrisProps {
  active: boolean
  onRunComplete?: (run: GameRunCompletion) => void
}

export function Tetris({ active, onRunComplete }: TetrisProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const playfieldRef = useRef<HTMLCanvasElement | null>(null)
  const holdRef = useRef<HTMLCanvasElement | null>(null)
  const nextRefs = useRef<Array<HTMLCanvasElement | null>>([])

  const [cellSize, setCellSize] = useState(20)
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const inputRef = useRef({
    left: newRepeatKey(),
    right: newRepeatKey(),
    down: newRepeatKey(),
  })

  const running = state.status === "playing"
  useGameRunCompletion({ status: state.status, score: state.score, onRunComplete })

  // -----------------------------------------------------------------------
  // Size the playfield by observing container dims.
  // -----------------------------------------------------------------------

  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return
    const measure = () => {
      const rect = container.getBoundingClientRect()
      if (rect.width < 1 || rect.height < 1) return
      // Horizontal budget: playfield (10 cols) + two side panels (PANEL_COLS each) + chrome.
      const widthBudget = (rect.width - HORIZONTAL_CHROME) / (BOARD_WIDTH + PANEL_COLS * 2)
      // Vertical budget: playfield (20 rows) plus chrome for bottom strip + padding.
      const heightBudget = (rect.height - VERTICAL_CHROME) / BOARD_HEIGHT
      const size = Math.max(
        CELL_MIN,
        Math.min(CELL_MAX, Math.floor(Math.min(widthBudget, heightBudget))),
      )
      setCellSize(size)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(container)
    return () => observer.disconnect()
  }, [])

  // -----------------------------------------------------------------------
  // Game loop — RAF drives both input repeat and engine tick.
  // -----------------------------------------------------------------------

  useEffect(() => {
    if (!running) return
    let raf = 0
    let last = performance.now()
    const loop = (now: number) => {
      const dt = Math.min(100, now - last)
      last = now

      const input = inputRef.current
      processRepeat(input.left, dt, DAS_MS, ARR_MS, () =>
        dispatch({ type: "move", dx: -1 }),
      )
      processRepeat(input.right, dt, DAS_MS, ARR_MS, () =>
        dispatch({ type: "move", dx: 1 }),
      )
      processRepeat(input.down, dt, 0, SOFT_DROP_MS, () =>
        dispatch({ type: "softDrop" }),
      )

      dispatch({ type: "tick", dt })
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(raf)
  }, [running])

  // -----------------------------------------------------------------------
  // Auto-pause when the sidebar hides Tetris or window loses focus.
  // -----------------------------------------------------------------------

  useEffect(() => {
    if (!active && state.status === "playing") {
      dispatch({ type: "pause" })
    }
  }, [active, state.status])

  useEffect(() => {
    if (state.status !== "paused") setShowKeybinds(false)
  }, [state.status])

  useEffect(() => {
    const onBlur = () => {
      if (state.status === "playing") dispatch({ type: "pause" })
      // Drop held keys so we don't have phantom auto-repeat on return.
      const input = inputRef.current
      resetKey(input.left)
      resetKey(input.right)
      resetKey(input.down)
    }
    window.addEventListener("blur", onBlur)
    return () => window.removeEventListener("blur", onBlur)
  }, [state.status])

  // -----------------------------------------------------------------------
  // Keyboard input.
  // -----------------------------------------------------------------------

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const { key, repeat } = event

      // Menu-level keys.
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

      // Gameplay keys. Browsers auto-repeat — we swallow repeats and rely on
      // our own DAS/ARR timers instead.
      if (key === "ArrowLeft") {
        event.preventDefault()
        const k = inputRef.current.left
        if (!k.held) {
          k.held = true
          k.heldFor = 0
          k.repeating = false
          k.repeatTimer = 0
          dispatch({ type: "move", dx: -1 })
        }
      } else if (key === "ArrowRight") {
        event.preventDefault()
        const k = inputRef.current.right
        if (!k.held) {
          k.held = true
          k.heldFor = 0
          k.repeating = false
          k.repeatTimer = 0
          dispatch({ type: "move", dx: 1 })
        }
      } else if (key === "ArrowDown") {
        event.preventDefault()
        const k = inputRef.current.down
        if (!k.held) {
          k.held = true
          k.heldFor = 0
          k.repeating = false
          k.repeatTimer = 0
          dispatch({ type: "softDrop" })
        }
      } else if (key === " ") {
        event.preventDefault()
        if (!repeat) dispatch({ type: "hardDrop" })
      } else if (key === "ArrowUp" || key === "x" || key === "X") {
        event.preventDefault()
        if (!repeat) dispatch({ type: "rotate", dir: 1 })
      } else if (key === "z" || key === "Z") {
        event.preventDefault()
        if (!repeat) dispatch({ type: "rotate", dir: -1 })
      } else if (key === "c" || key === "C" || key === "Shift") {
        event.preventDefault()
        if (!repeat) dispatch({ type: "hold" })
      } else if (key === "r" || key === "R") {
        event.preventDefault()
        dispatch({ type: "reset" })
      }
    },
    [state.status],
  )

  const handleKeyUp = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const { key } = event
    if (key === "ArrowLeft") resetKey(inputRef.current.left)
    else if (key === "ArrowRight") resetKey(inputRef.current.right)
    else if (key === "ArrowDown") resetKey(inputRef.current.down)
  }, [])

  // -----------------------------------------------------------------------
  // Rendering.
  // -----------------------------------------------------------------------

  const fieldWidth = BOARD_WIDTH * cellSize
  const fieldHeight = BOARD_HEIGHT * cellSize

  // Draw the playfield.
  useEffect(() => {
    const canvas = playfieldRef.current
    if (!canvas) return
    const ctx = setupCanvas(canvas, fieldWidth, fieldHeight)
    if (!ctx) return

    // Background — subtle vertical gradient so the field doesn't read as flat black.
    const bg = ctx.createLinearGradient(0, 0, 0, fieldHeight)
    bg.addColorStop(0, "#141a2e")
    bg.addColorStop(1, "#0c1122")
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, fieldWidth, fieldHeight)
    drawGrid(ctx, BOARD_WIDTH, BOARD_HEIGHT, cellSize)

    const clearing = state.lineClear
    const clearingSet = clearing ? new Set(clearing.rows) : null
    const clearProgress = clearing ? Math.min(1, clearing.elapsed / LINE_CLEAR_MS) : 0

    // Locked cells.
    for (let y = 0; y < BOARD_HEIGHT; y++) {
      const row = state.board[y]
      for (let x = 0; x < BOARD_WIDTH; x++) {
        const v = row[x]
        if (v === 0) continue
        const px = x * cellSize
        const py = y * cellSize
        if (clearingSet && clearingSet.has(y)) {
          const type = PIECE_TYPES[v - 1]
          drawFilledCell(ctx, px, py, cellSize, PIECE_COLORS[type])
          drawFlashCell(ctx, px, py, cellSize, clearProgress)
        } else {
          const type = PIECE_TYPES[v - 1]
          drawFilledCell(ctx, px, py, cellSize, PIECE_COLORS[type])
        }
      }
    }

    // Ghost piece (only during normal play).
    if (!clearing && state.current && state.status === "playing") {
      const ghost = ghostPieceFor(state)
      if (ghost && (ghost.y !== state.current.y || ghost.x !== state.current.x)) {
        const color = PIECE_COLORS[ghost.type]
        for (const [gx, gy] of pieceCells(ghost)) {
          if (gy < 0) continue
          drawGhostCell(ctx, gx * cellSize, gy * cellSize, cellSize, color)
        }
      }
    }

    // Current piece.
    if (!clearing && state.current) {
      const color = PIECE_COLORS[state.current.type]
      for (const [cx, cy] of pieceCells(state.current)) {
        if (cy < 0) continue
        drawFilledCell(ctx, cx * cellSize, cy * cellSize, cellSize, color)
      }
    }
  }, [state, cellSize, fieldWidth, fieldHeight])

  // Draw the hold preview.
  useEffect(() => {
    const canvas = holdRef.current
    if (!canvas) return
    const cssW = cellSize * 4
    const cssH = cellSize * 3
    const ctx = setupCanvas(canvas, cssW, cssH)
    if (!ctx) return
    ctx.fillStyle = "#10162a"
    ctx.fillRect(0, 0, cssW, cssH)
    if (state.hold) {
      const previewSize = Math.max(6, Math.floor(cellSize * 0.82))
      ctx.save()
      if (state.hasHeld) ctx.globalAlpha = 0.35
      drawCenteredPiece(ctx, state.hold, cssW, cssH, previewSize)
      ctx.restore()
    }
  }, [state.hold, state.hasHeld, cellSize])

  // Draw each next-piece preview.
  const previewQueue = useMemo(
    () => state.queue.slice(0, NEXT_COUNT),
    [state.queue],
  )
  useEffect(() => {
    for (let i = 0; i < NEXT_COUNT; i++) {
      const canvas = nextRefs.current[i]
      if (!canvas) continue
      const cssW = cellSize * 4
      const cssH = cellSize * 3
      const ctx = setupCanvas(canvas, cssW, cssH)
      if (!ctx) continue
      ctx.fillStyle = "#10162a"
      ctx.fillRect(0, 0, cssW, cssH)
      const type = previewQueue[i]
      if (type) {
        const size = Math.max(6, Math.floor(cellSize * 0.72))
        drawCenteredPiece(ctx, type, cssW, cssH, size)
      }
    }
  }, [previewQueue, cellSize])

  // -----------------------------------------------------------------------
  // Overlay handlers (pointer start / resume / restart).
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

  // Auto-focus when the game becomes active so keys work straight away.
  useEffect(() => {
    if (active) {
      const handle = window.requestAnimationFrame(() => focusContainer())
      return () => window.cancelAnimationFrame(handle)
    }
    return undefined
  }, [active, focusContainer])

  // -----------------------------------------------------------------------
  // Layout
  // -----------------------------------------------------------------------

  const overlay = renderOverlay(state.status, state.score)

  return (
    <div
      aria-label="Tetris"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#181f36] via-[#10162a] to-[#0b1020] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
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
      <div className="flex min-h-0 flex-1 items-stretch justify-center gap-2 p-2">
        {/* Left column — hold + stats */}
        <div
          className="flex min-h-0 shrink-0 flex-col gap-2"
          style={{ width: cellSize * PANEL_COLS }}
        >
          <SidePanel label="Hold">
            <canvas
              ref={holdRef}
              style={{ width: `${cellSize * 4}px`, height: `${cellSize * 3}px` }}
            />
          </SidePanel>
          <StatBlock label="Score" value={state.score.toLocaleString()} primary />
          <StatBlock label="Level" value={String(state.level)} />
          <StatBlock label="Lines" value={String(state.lines)} />
        </div>

        {/* Playfield */}
        <div className="relative flex min-h-0 shrink-0 flex-col items-center">
          <canvas
            className="rounded-sm border border-white/15 shadow-[0_0_18px_rgba(0,0,0,0.45)_inset,0_4px_20px_-8px_rgba(0,0,0,0.6)]"
            ref={playfieldRef}
            style={{ width: `${fieldWidth}px`, height: `${fieldHeight}px` }}
          />
          {overlay ? (
            <div
              aria-live="polite"
              className="pointer-events-none absolute inset-0 flex items-center justify-center"
            >
              <div className="pointer-events-auto flex min-w-[240px] flex-col items-center gap-3 rounded-md border border-white/10 bg-[#0a0e1c]/85 px-5 py-4 text-center shadow-[0_12px_36px_-8px_rgba(0,0,0,0.65)] backdrop-blur-md">
                {state.status === "paused" && showKeybinds ? (
                  <KeybindsView onBack={() => setShowKeybinds(false)} />
                ) : (
                  <>
                    <div className="font-mono text-[10px] uppercase tracking-[0.32em] text-white/60">
                      {overlay.eyebrow}
                    </div>
                    <div className="text-[18px] font-semibold text-white">
                      {overlay.title}
                    </div>
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

        {/* Right column — next queue */}
        <div
          className="flex min-h-0 shrink-0 flex-col gap-2"
          style={{ width: cellSize * PANEL_COLS }}
        >
          <SidePanel label="Next" className="flex-1">
            <div className="flex flex-col items-stretch gap-[2px]">
              {Array.from({ length: NEXT_COUNT }).map((_, i) => (
                <canvas
                  key={i}
                  ref={(el) => {
                    nextRefs.current[i] = el
                  }}
                  style={{ width: `${cellSize * 4}px`, height: `${cellSize * 3}px` }}
                />
              ))}
            </div>
          </SidePanel>
        </div>
      </div>

      {/* Bottom control strip */}
      <div className="flex h-9 shrink-0 items-center justify-between gap-2 border-t border-white/10 bg-white/[0.025] px-3 font-mono text-[9.5px] uppercase tracking-[0.22em] text-white/60">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-white/45">Tetris</span>
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

      {/* Focus hint when container lacks focus so users know where to click */}
      {!hasFocus && running ? (
        <div className="pointer-events-none absolute inset-x-0 top-1.5 flex justify-center">
          <span className="rounded-sm bg-white/10 px-2 py-[1px] font-mono text-[9px] uppercase tracking-[0.24em] text-white/70">
            Click to focus
          </span>
        </div>
      ) : null}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Small subcomponents
// ---------------------------------------------------------------------------

function SidePanel({
  label,
  children,
  className,
}: {
  label: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "flex flex-col rounded-sm border border-white/10 bg-white/[0.03] shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]",
        className,
      )}
    >
      <div className="border-b border-white/10 px-2 py-1 font-mono text-[9px] uppercase tracking-[0.24em] text-white/65">
        {label}
      </div>
      <div className="flex flex-1 items-center justify-center p-1.5">{children}</div>
    </div>
  )
}

function StatBlock({
  label,
  value,
  primary = false,
}: {
  label: string
  value: string
  primary?: boolean
}) {
  return (
    <div className="flex flex-col rounded-sm border border-white/10 bg-white/[0.04] px-2 py-1.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <span className="font-mono text-[8.5px] uppercase tracking-[0.22em] text-white/60">
        {label}
      </span>
      <span
        className={cn(
          "mt-0.5 font-mono tabular-nums leading-none",
          primary ? "text-[17px] text-primary" : "text-[13px] text-white",
        )}
      >
        {value}
      </span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Keybind settings panel (shown inside the pause overlay).
// ---------------------------------------------------------------------------

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→"], label: "Move" },
  { keys: ["↑", "X"], label: "Rotate CW" },
  { keys: ["Z"], label: "Rotate CCW" },
  { keys: ["↓"], label: "Soft drop" },
  { keys: ["Space"], label: "Hard drop" },
  { keys: ["C", "Shift"], label: "Hold" },
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
            <span className="uppercase tracking-[0.18em] text-white/70">
              {bind.label}
            </span>
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

// ---------------------------------------------------------------------------
// Overlay copy per game state.
// ---------------------------------------------------------------------------

function renderOverlay(
  status: GameState["status"],
  score: number,
): { eyebrow: string; title: string; detail?: string; button: string; hint: string } | null {
  if (status === "playing") return null
  if (status === "idle") {
    return {
      eyebrow: "Arcade",
      title: "Tetris",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Take a breath",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "Stack topped out",
    detail: `Final score  ${score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
