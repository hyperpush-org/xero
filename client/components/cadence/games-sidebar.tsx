"use client"

import { useCallback, useMemo, useRef, useState } from "react"
import { Search } from "lucide-react"
import { cn } from "@/lib/utils"

const MIN_WIDTH = 200
const MAX_WIDTH = 520
const DEFAULT_WIDTH = 256

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface GamesSidebarProps {
  open: boolean
}

// ---------------------------------------------------------------------------
// Pixel art — 8x8 grid. "." is transparent; container provides the backdrop.
// ---------------------------------------------------------------------------

type Palette = Record<string, string>

interface PixelGlyph {
  rows: string[]
  palette: Palette
}

function PixelArt({ glyph }: { glyph: PixelGlyph }) {
  const { rows, palette } = glyph
  return (
    <svg aria-hidden="true" className="h-full w-full" preserveAspectRatio="xMidYMid meet" viewBox="0 0 8 8">
      {rows.flatMap((row, y) =>
        row
          .replace(/\s+/g, "")
          .split("")
          .map((ch, x) => {
            const color = palette[ch]
            if (!color) return null
            return <rect fill={color} height={1} key={`${x}-${y}`} width={1} x={x} y={y} />
          }),
      )}
    </svg>
  )
}

// ---------------------------------------------------------------------------
// Catalog (mockup only)
// ---------------------------------------------------------------------------

interface Game {
  id: string
  title: string
  tagline: string
  glyph: PixelGlyph
}

const GAMES: Game[] = [
  {
    id: "tetris",
    title: "Tetris",
    tagline: "Stack blocks, clear lines",
    glyph: {
      palette: { C: "#22d3ee", M: "#a855f7", Y: "#facc15", R: "#ef4444" },
      rows: [
        "........",
        ".CC.MM..",
        ".CC.MM..",
        "........",
        ".YY.RR..",
        ".YYYRR..",
        "..YYRR..",
        "........",
      ],
    },
  },
  {
    id: "space-invaders",
    title: "Space Invaders",
    tagline: "Hold the line against the swarm",
    glyph: {
      palette: { G: "#4ade80" },
      rows: [
        "..G..G..",
        "...GG...",
        "..GGGG..",
        ".GG.GGG.",
        "GGGGGGGG",
        "G.GGGG.G",
        "G.G..G.G",
        "........",
      ],
    },
  },
  {
    id: "pong",
    title: "Pong",
    tagline: "The duel that started it all",
    glyph: {
      palette: { W: "#e5e7eb" },
      rows: [
        "W......W",
        "W......W",
        "W......W",
        "W..WW..W",
        "W..WW..W",
        "W......W",
        "W......W",
        "W......W",
      ],
    },
  },
  {
    id: "snake",
    title: "Snake",
    tagline: "Grow long, don't bite",
    glyph: {
      palette: { G: "#84cc16", A: "#ef4444" },
      rows: [
        "........",
        ".GGGG...",
        "....G...",
        "....G...",
        "....GG..",
        ".....G..",
        ".....GG.",
        "......GA",
      ],
    },
  },
  {
    id: "pacman",
    title: "Pac-Man",
    tagline: "Chase pellets, flee ghosts",
    glyph: {
      palette: { Y: "#facc15", P: "#fde68a" },
      rows: [
        "..YYYY..",
        ".YYYYYY.",
        "YYYY....",
        "YYY.....",
        "YYYY....",
        ".YYYYYY.",
        "..YYYY..",
        "P.P.P.P.",
      ],
    },
  },
  {
    id: "breakout",
    title: "Breakout",
    tagline: "Smash bricks, keep the ball alive",
    glyph: {
      palette: { R: "#ef4444", O: "#fb923c", Y: "#facc15", G: "#4ade80", W: "#e5e7eb" },
      rows: [
        "RRRRRRRR",
        "OOOOOOOO",
        "YYYYYYYY",
        "GGGGGGGG",
        "........",
        "....W...",
        "........",
        "..WWWW..",
      ],
    },
  },
  {
    id: "asteroids",
    title: "Asteroids",
    tagline: "Blast rocks in deep space",
    glyph: {
      palette: { R: "#94a3b8", D: "#475569", W: "#e5e7eb" },
      rows: [
        "..RRR...",
        ".RDDDR..",
        "RDDDDDR.",
        "RDDDDDR.",
        ".RDDDR..",
        "..RRR.W.",
        "......W.",
        ".....WWW",
      ],
    },
  },
  {
    id: "minesweeper",
    title: "Minesweeper",
    tagline: "Tile by tile, avoid the boom",
    glyph: {
      palette: { G: "#64748b", L: "#cbd5e1", B: "#0f172a", R: "#ef4444" },
      rows: [
        "GLGLGLGL",
        "LGLGLGLG",
        "GL.BB.LG",
        "LGBBBBGL",
        "GLBBBBLG",
        "LG.BB.GL",
        "GLGLGLGL",
        "LGLGLRLG",
      ],
    },
  },
  {
    id: "frogger",
    title: "Frogger",
    tagline: "Cross the traffic, hop the river",
    glyph: {
      palette: { G: "#34d399", W: "#0ea5e9", L: "#6b7280" },
      rows: [
        "WWWWWWWW",
        "LLLLLLLL",
        "........",
        "LLLLLLLL",
        "........",
        "..GG.G..",
        ".GGGGGG.",
        "..G..G..",
      ],
    },
  },
  {
    id: "galaga",
    title: "Galaga",
    tagline: "Squadron shooter from the arcade era",
    glyph: {
      palette: { M: "#f472b6", W: "#e5e7eb", C: "#22d3ee" },
      rows: [
        "M......M",
        ".M.WW.M.",
        ".MWWWWM.",
        "MWWCCWWM",
        "MWWCCWWM",
        ".MWWWWM.",
        ".M.WW.M.",
        "M......M",
      ],
    },
  },
  {
    id: "centipede",
    title: "Centipede",
    tagline: "Mushroom field, spraying legs",
    glyph: {
      palette: { O: "#f97316", M: "#f472b6", G: "#4ade80" },
      rows: [
        ".M..M.M.",
        "........",
        "OOOOOOO.",
        "......O.",
        ".OOOOOO.",
        ".O......",
        ".OOOOOOO",
        "..G..G..",
      ],
    },
  },
  {
    id: "dig-dug",
    title: "Dig Dug",
    tagline: "Tunnel deep, pop the Pookas",
    glyph: {
      palette: { S: "#78350f", D: "#fb923c", R: "#ef4444", W: "#e5e7eb" },
      rows: [
        "SSSSSSSS",
        "S......S",
        "S.DWD..S",
        "S.DDD..S",
        "S......S",
        "SR....RS",
        "SSRRRRSS",
        "SSSSSSSS",
      ],
    },
  },
]

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function GamesSidebar({ open }: GamesSidebarProps) {
  const [query, setQuery] = useState("")
  const [width, setWidth] = useState(DEFAULT_WIDTH)
  const [isResizing, setIsResizing] = useState(false)
  const widthRef = useRef(width)
  widthRef.current = width

  const handleResizeStart = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"

    const handleMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX
      const next = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta))
      setWidth(next)
    }
    const handleUp = () => {
      window.removeEventListener("pointermove", handleMove)
      window.removeEventListener("pointerup", handleUp)
      window.removeEventListener("pointercancel", handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
    }

    window.addEventListener("pointermove", handleMove)
    window.addEventListener("pointerup", handleUp)
    window.addEventListener("pointercancel", handleUp)
  }, [])

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, current + delta))
    })
  }, [])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return GAMES
    return GAMES.filter(
      (game) =>
        game.title.toLowerCase().includes(q) || game.tagline.toLowerCase().includes(q),
    )
  }, [query])

  if (!open) return null

  return (
    <aside
      className="relative flex shrink-0 flex-col overflow-hidden border-l border-border/80 bg-sidebar"
      style={{ width }}
    >
      <div
        aria-label="Resize arcade sidebar"
        aria-orientation="vertical"
        aria-valuemax={MAX_WIDTH}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={0}
      />
      <div className="flex h-10 items-center justify-between border-b border-border/70 px-3">
        <div className="flex items-center gap-1.5">
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Arcade
          </span>
          <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
            {GAMES.length}
          </span>
        </div>
      </div>

      <div className="border-b border-border/70 px-3 py-2">
        <div className="relative">
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground"
          />
          <input
            aria-label="Search games"
            className="h-7 w-full rounded-md border border-border/70 bg-background/40 pl-7 pr-2 text-[11.5px] text-foreground placeholder:text-muted-foreground/70 focus:border-primary/50 focus:outline-none"
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search"
            type="search"
            value={query}
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto scrollbar-thin">
        {filtered.length === 0 ? (
          <div className="px-3 py-5 text-center text-[11px] leading-relaxed text-muted-foreground/80">
            No games match.
          </div>
        ) : (
          <ul className="flex flex-col">
            {filtered.map((game) => (
              <li key={game.id}>
                <GameRow game={game} />
              </li>
            ))}
          </ul>
        )}
      </div>
    </aside>
  )
}

function GameRow({ game }: { game: Game }) {
  return (
    <button
      className={cn(
        "group flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors",
        "hover:bg-secondary/50",
      )}
      type="button"
    >
      <div className="flex h-7 w-7 shrink-0 items-center justify-center overflow-hidden rounded-md border border-border/70 bg-background/60 p-0.5">
        <PixelArt glyph={game.glyph} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12.5px] font-medium leading-tight text-foreground/85 group-hover:text-foreground">
          {game.title}
        </div>
        <div className="mt-0.5 truncate text-[11px] leading-tight text-muted-foreground">
          {game.tagline}
        </div>
      </div>
    </button>
  )
}
