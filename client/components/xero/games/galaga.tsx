"use client"

import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { ArrowLeft, Pause, Play, RotateCcw, Settings2 } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  BULLET_HEIGHT,
  BULLET_WIDTH,
  FIELD_HEIGHT,
  FIELD_WIDTH,
  GROUND_Y,
  PLAYER_Y,
  createInitialState,
  enemyKindForRow,
  enemyWorldPos,
  reduce,
  type EnemyKind,
  type GameState,
} from "./galaga-engine"

// ---------------------------------------------------------------------------
// Sprites — each "X" is one logical pixel; "." is transparent.
// ---------------------------------------------------------------------------

const SPRITES = {
  player: [
    "......X......",
    "......X......",
    "......X......",
    ".....XCX.....",
    ".....XCX.....",
    "...XXCCCXX...",
    "..XWWCCCWWX..",
    ".XWWWCWCWWWX.",
    "XXXXXXXXXXXXX",
    "X.XXXXXXXXX.X",
  ],
  playerExplode: [
    "X.X..X..X.X.X",
    ".X.XX..XX.X..",
    "X..X.XX..X.X.",
    ".XX.X..X.X..X",
    "X..XX.XX..X.X",
    ".X.X.XX.X.X.X",
    "X.X..X..X..X.",
    ".X.X.X.X.X.X.",
    "X..X...X..X.X",
    ".X..X.X..X.X.",
  ],
  boss0: [
    "....XXXX....",
    "...XXYYXX...",
    "..XYYYYYYX..",
    ".XYY.YY.YYX.",
    ".XYYYYYYYYX.",
    ".XXYYYYYYXX.",
    "..XXYYYYXX..",
    "...X.XX.X...",
    "..X..XX..X..",
    ".X...XX...X.",
  ],
  boss1: [
    "....XXXX....",
    "...XXYYXX...",
    "..XYYYYYYX..",
    ".XYY.YY.YYX.",
    ".XYYYYYYYYX.",
    ".XXYYYYYYXX.",
    "..XXYYYYXX..",
    "...XX..XX...",
    "..X.X..X.X..",
    ".X..X..X..X.",
  ],
  butterfly0: [
    "....MM..MM..",
    "...MMMMMMM..",
    "..MCCCMCCCM.",
    ".MCMCMMMCMCM",
    "MMCCMMCMMCCM",
    "MMMMMMCMMMMM",
    ".MMCCCMCCCM.",
    "..MM.M.M.MM.",
    ".M..MM.MM..M",
    "M..........M",
  ],
  butterfly1: [
    "...MM....MM.",
    "..MMMMMMMM..",
    ".MCCCMCCCM..",
    "MCMCMMMCMCM.",
    "MCCMMCMMCCM.",
    "MMMMMCMMMMM.",
    ".MCCCMCCCM..",
    "..MM.M.M.MM.",
    ".M..MM.MM..M",
    "............",
  ],
  bee0: [
    ".....YY.....",
    "....YBBY....",
    "...YBYYBY...",
    "..YBBYYBBY..",
    ".YYYYBBYYYY.",
    ".Y.YBYYBY.Y.",
    "..YYBBBBYY..",
    "...Y.YY.Y...",
    "..Y..YY..Y..",
    ".Y...YY...Y.",
  ],
  bee1: [
    ".....YY.....",
    "....YBBY....",
    "...YBYYBY...",
    "..YBBYYBBY..",
    ".YYYYBBYYYY.",
    ".Y.YBYYBY.Y.",
    "..YYBBBBYY..",
    "...YY..YY...",
    "..Y.Y..Y.Y..",
    ".Y..Y..Y..Y.",
  ],
  enemyExplode: [
    "..X....X....",
    "X..X..X..X..",
    ".X..XX..X...",
    "....XX......",
    "XX.XXXX.XX..",
    "XX.XXXX.XX..",
    "....XX......",
    ".X..XX..X...",
    "X..X..X..X..",
    "..X....X....",
  ],
} as const

const COLORS = {
  playerPrimary: "#ffffff",
  playerAccent: "#ef4444",
  playerWing: "#3b82f6",
  playerExplode: "#fbbf24",
  boss: "#22d3ee",
  bossAccent: "#a855f7",
  butterfly: "#f472b6",
  butterflyAccent: "#facc15",
  bee: "#facc15",
  beeAccent: "#ef4444",
  playerBullet: "#e0f2fe",
  enemyBullet: "#fca5a5",
  enemyExplode: "#fb923c",
  star: "rgba(255,255,255,0.12)",
  starDim: "rgba(255,255,255,0.05)",
}

function kindPalette(k: EnemyKind): Record<string, string> {
  if (k === "boss") return { X: COLORS.boss, Y: COLORS.bossAccent }
  if (k === "butterfly") return { M: COLORS.butterfly, C: COLORS.butterflyAccent }
  return { Y: COLORS.bee, B: COLORS.beeAccent }
}

function drawSprite(
  ctx: CanvasRenderingContext2D,
  rows: readonly string[],
  x: number,
  y: number,
  palette: Record<string, string>,
  rotation = 0,
) {
  const h = rows.length
  const w = rows[0].length
  if (rotation !== 0) {
    ctx.save()
    ctx.translate(Math.round(x + w / 2), Math.round(y + h / 2))
    ctx.rotate(rotation)
    ctx.translate(-w / 2, -h / 2)
    for (let row = 0; row < h; row++) {
      const line = rows[row]
      for (let col = 0; col < line.length; col++) {
        const ch = line[col]
        const color = palette[ch]
        if (!color) continue
        ctx.fillStyle = color
        ctx.fillRect(col, row, 1, 1)
      }
    }
    ctx.restore()
    return
  }
  for (let row = 0; row < h; row++) {
    const line = rows[row]
    for (let col = 0; col < line.length; col++) {
      const ch = line[col]
      const color = palette[ch]
      if (!color) continue
      ctx.fillStyle = color
      ctx.fillRect(Math.round(x) + col, Math.round(y) + row, 1, 1)
    }
  }
}

// Fixed multi-layer star field so the background doesn't read as flat black.
const STARS_FAR: Array<[number, number]> = [
  [14, 8], [42, 14], [74, 4], [108, 18], [140, 10], [174, 22], [210, 6], [250, 16],
  [24, 48], [60, 40], [94, 54], [130, 46], [164, 58], [202, 42], [236, 50], [268, 44],
  [18, 92], [50, 100], [86, 84], [122, 96], [158, 104], [194, 88], [228, 98], [260, 92],
  [30, 132], [68, 140], [102, 126], [138, 138], [172, 128], [208, 136], [246, 130],
  [22, 172], [58, 180], [92, 164], [128, 176], [162, 170], [198, 182], [232, 178], [266, 168],
]
const STARS_NEAR: Array<[number, number]> = [
  [38, 22], [96, 32], [150, 16], [222, 34], [260, 28],
  [44, 72], [112, 78], [180, 66], [246, 82],
  [32, 118], [110, 112], [176, 120], [230, 114],
  [50, 156], [126, 148], [200, 160], [258, 150],
]

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface GalagaProps {
  active: boolean
}

export function Galaga({ active }: GalagaProps) {
  const [state, dispatch] = useReducer(reduce, undefined, createInitialState)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const stageRef = useRef<HTMLDivElement | null>(null)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  const [fitSize, setFitSize] = useState({ w: FIELD_WIDTH * 2, h: FIELD_HEIGHT * 2 })
  const [hasFocus, setHasFocus] = useState(false)
  const [showKeybinds, setShowKeybinds] = useState(false)

  const keysRef = useRef({ left: false, right: false, fire: false })

  const running = state.status === "playing"

  // -----------------------------------------------------------------------
  // Stage fit: preserve aspect, integer-ish scale for pixel-perfect display.
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
      const dt = Math.min(100, now - last)
      last = now

      const keys = keysRef.current
      const dir: -1 | 0 | 1 =
        keys.left && !keys.right ? -1 : keys.right && !keys.left ? 1 : 0
      dispatch({ type: "setMove", dir })
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
      const { key, repeat } = event

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
      } else if (key === " ") {
        event.preventDefault()
        if (!repeat) {
          keysRef.current.fire = true
          dispatch({ type: "fire" })
        }
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

    // Background.
    const bg = ctx.createLinearGradient(0, 0, 0, FIELD_HEIGHT)
    bg.addColorStop(0, "#040618")
    bg.addColorStop(1, "#01020a")
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, FIELD_WIDTH, FIELD_HEIGHT)

    // Stars (two layers).
    ctx.fillStyle = COLORS.starDim
    for (const [sx, sy] of STARS_FAR) ctx.fillRect(sx, sy, 1, 1)
    ctx.fillStyle = COLORS.star
    for (const [sx, sy] of STARS_NEAR) ctx.fillRect(sx, sy, 1, 1)

    // Animation frame toggle based on formation breathing phase.
    const frame = state.formationPhase < 0.5 ? 0 : 1

    // Enemies.
    for (const e of state.enemies) {
      if (!e.alive) continue
      const { x, y } = enemyWorldPos(e, state)
      const kind = enemyKindForRow(e.row)
      const palette = kindPalette(kind)
      const sprite =
        kind === "boss"
          ? frame
            ? SPRITES.boss1
            : SPRITES.boss0
          : kind === "butterfly"
            ? frame
              ? SPRITES.butterfly1
              : SPRITES.butterfly0
            : frame
              ? SPRITES.bee1
              : SPRITES.bee0
      // Divers rotate so their noses point along the flight path.
      const rotation = e.mode === "diving" ? e.heading - Math.PI / 2 : 0
      drawSprite(ctx, sprite, x, y, palette, rotation)
    }

    // Bullets.
    for (const b of state.bullets) {
      ctx.fillStyle = b.from === "player" ? COLORS.playerBullet : COLORS.enemyBullet
      ctx.fillRect(Math.round(b.x), Math.round(b.y), BULLET_WIDTH, BULLET_HEIGHT)
    }

    // Explosions.
    for (const e of state.explosions) {
      const sprite = e.kind === "enemy" ? SPRITES.enemyExplode : SPRITES.playerExplode
      const palette: Record<string, string> =
        e.kind === "enemy"
          ? { X: COLORS.enemyExplode }
          : { X: COLORS.playerExplode }
      const w = sprite[0].length
      const h = sprite.length
      drawSprite(ctx, sprite, e.x - w / 2, e.y - h / 2, palette)
    }

    // Player.
    const blink = state.respawnTimer > 0 && state.playerHitTimer <= 0
    const hidden = state.playerHitTimer > 0
    const visible = !hidden && (!blink || Math.floor(state.respawnTimer / 90) % 2 === 0)
    if (visible && state.status !== "idle") {
      drawSprite(ctx, SPRITES.player, state.playerX, PLAYER_Y, {
        X: COLORS.playerPrimary,
        C: COLORS.playerAccent,
        W: COLORS.playerWing,
      })
    }

    // Baseline.
    ctx.fillStyle = "rgba(59,130,246,0.35)"
    ctx.fillRect(0, GROUND_Y, FIELD_WIDTH, 1)
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
      aria-label="Galaga"
      className={cn(
        "relative flex aspect-[4/3] w-full max-w-xl select-none flex-col overflow-hidden rounded-md border border-white/10 bg-gradient-to-br from-[#070b20] via-[#03050f] to-[#01020a] shadow-[0_10px_40px_-12px_rgba(0,0,0,0.6),inset_0_1px_0_rgba(255,255,255,0.06)] outline-none",
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
              {state.score.toString().padStart(5, "0")}
            </span>
          </span>
          <span className="flex items-baseline gap-1">
            <span className="text-white/40">Stage</span>
            <span className="tabular-nums text-white/90">{state.level}</span>
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className="text-white/40">Lives</span>
          <div className="flex items-center gap-[3px]">
            {Array.from({ length: Math.max(0, state.lives) }).map((_, i) => (
              <MiniShip key={i} />
            ))}
            {state.lives === 0 ? <span className="text-white/30">—</span> : null}
          </div>
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
            <div className="pointer-events-auto flex min-w-[240px] flex-col items-center gap-3 rounded-md border border-white/10 bg-[#05080f]/85 px-5 py-4 text-center shadow-[0_12px_36px_-8px_rgba(0,0,0,0.65)] backdrop-blur-md">
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
          <span className="truncate text-white/45">Galaga</span>
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

function MiniShip() {
  return (
    <svg
      aria-hidden="true"
      className="h-3 w-3"
      preserveAspectRatio="xMidYMid meet"
      viewBox="0 0 13 10"
    >
      <g fill={COLORS.playerPrimary}>
        <rect x="6" y="0" width="1" height="3" />
        <rect x="5" y="3" width="3" height="2" />
        <rect x="3" y="5" width="7" height="1" />
        <rect x="0" y="6" width="13" height="3" />
        <rect x="0" y="9" width="1" height="1" />
        <rect x="12" y="9" width="1" height="1" />
      </g>
      <rect x="6" y="3" width="1" height="3" fill={COLORS.playerAccent} />
    </svg>
  )
}

const KEYBINDS: Array<{ keys: string[]; label: string }> = [
  { keys: ["←", "→"], label: "Move" },
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
      title: "Galaga",
      button: "Press Start",
      hint: "Enter or Space",
    }
  }
  if (state.status === "paused") {
    return {
      eyebrow: "Paused",
      title: "Squadron holding",
      button: "Resume",
      hint: "Esc or P",
    }
  }
  if (state.status === "won") {
    return {
      eyebrow: `Stage ${state.level} cleared`,
      title: "Swarm routed",
      detail: `Score  ${state.score.toLocaleString()}`,
      button: "Next stage",
      hint: "Enter to advance",
    }
  }
  return {
    eyebrow: "Game Over",
    title: "Fighter down",
    detail: `Final score  ${state.score.toLocaleString()}`,
    button: "Play again",
    hint: "Enter to retry",
  }
}
