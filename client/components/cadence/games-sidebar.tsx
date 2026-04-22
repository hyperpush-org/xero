"use client"

import { useCallback, useMemo, useRef, useState } from "react"
import { ChevronRight, Play, Search } from "lucide-react"
import { cn } from "@/lib/utils"
import { Asteroids } from "./games/asteroids"
import { Breakout } from "./games/breakout"
import { Pacman } from "./games/pacman"
import { Snake } from "./games/snake"
import { SpaceInvaders } from "./games/space-invaders"
import { Tetris } from "./games/tetris"

const MIN_WIDTH = 200
const MAX_WIDTH = 1200
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
// Catalog (mockup only). Data is player-centric: best score, runs, session
// history — not encyclopedia metadata.
// ---------------------------------------------------------------------------

interface LeaderboardEntry {
  name: string
  score: string
  you?: boolean
}

interface GameStats {
  personalBest: string
  runs: number
  timePlayed: string
  leaderboard: LeaderboardEntry[] // pre-sorted, rank 1 first
}

interface Game {
  id: string
  title: string
  tagline: string
  glyph: PixelGlyph
  stats: GameStats
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
    stats: {
      personalBest: "2,480",
      runs: 17,
      timePlayed: "42m",
      leaderboard: [
        { name: "Maya", score: "2,840" },
        { name: "Andrew", score: "2,480", you: true },
        { name: "Joel", score: "1,980" },
        { name: "Priya", score: "1,720" },
        { name: "Dante", score: "1,420" },
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
    stats: {
      personalBest: "9,870",
      runs: 8,
      timePlayed: "18m",
      leaderboard: [
        { name: "Andrew", score: "9,870", you: true },
        { name: "Rin", score: "8,420" },
        { name: "Maya", score: "6,980" },
        { name: "Joel", score: "5,420" },
        { name: "Sam", score: "4,080" },
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
    stats: {
      personalBest: "1,420",
      runs: 23,
      timePlayed: "55m",
      leaderboard: [
        { name: "Rin", score: "1,680" },
        { name: "Priya", score: "1,520" },
        { name: "Andrew", score: "1,420", you: true },
        { name: "Maya", score: "1,120" },
        { name: "Joel", score: "880" },
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
    stats: {
      personalBest: "24,700",
      runs: 6,
      timePlayed: "22m",
      leaderboard: [
        { name: "Andrew", score: "24,700", you: true },
        { name: "Joel", score: "18,320" },
        { name: "Maya", score: "14,810" },
        { name: "Dante", score: "11,400" },
        { name: "Sam", score: "9,240" },
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
    stats: {
      personalBest: "1,980",
      runs: 9,
      timePlayed: "16m",
      leaderboard: [
        { name: "Maya", score: "2,240" },
        { name: "Andrew", score: "1,980", you: true },
        { name: "Joel", score: "1,520" },
        { name: "Priya", score: "1,240" },
        { name: "Rin", score: "980" },
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
    stats: {
      personalBest: "6,450",
      runs: 4,
      timePlayed: "9m",
      leaderboard: [
        { name: "Andrew", score: "6,450", you: true },
        { name: "Dante", score: "5,120" },
        { name: "Joel", score: "4,210" },
        { name: "Rin", score: "3,680" },
        { name: "Maya", score: "3,080" },
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
    stats: {
      personalBest: "4,300",
      runs: 3,
      timePlayed: "7m",
      leaderboard: [
        { name: "Andrew", score: "4,300", you: true },
        { name: "Priya", score: "3,480" },
        { name: "Maya", score: "2,840" },
        { name: "Joel", score: "2,120" },
        { name: "Sam", score: "1,920" },
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
    stats: {
      personalBest: "15,780",
      runs: 5,
      timePlayed: "14m",
      leaderboard: [
        { name: "Maya", score: "17,420" },
        { name: "Andrew", score: "15,780", you: true },
        { name: "Dante", score: "12,850" },
        { name: "Joel", score: "9,280" },
        { name: "Rin", score: "7,940" },
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
    stats: {
      personalBest: "8,200",
      runs: 2,
      timePlayed: "5m",
      leaderboard: [
        { name: "Andrew", score: "8,200", you: true },
        { name: "Joel", score: "6,540" },
        { name: "Maya", score: "5,310" },
        { name: "Rin", score: "4,180" },
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
    stats: {
      personalBest: "3,650",
      runs: 4,
      timePlayed: "11m",
      leaderboard: [
        { name: "Andrew", score: "3,650", you: true },
        { name: "Dante", score: "3,180" },
        { name: "Maya", score: "2,940" },
        { name: "Joel", score: "2,120" },
        { name: "Priya", score: "1,540" },
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
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)
  const widthRef = useRef(width)
  widthRef.current = width
  const widthBeforeSelectRef = useRef<number | null>(null)
  const viewDirectionRef = useRef<1 | -1>(1)

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

  const handleSelectGame = useCallback((gameId: string) => {
    viewDirectionRef.current = 1
    if (typeof window !== "undefined") {
      widthBeforeSelectRef.current = widthRef.current
      const target = Math.round(window.innerWidth / 2)
      const clamped = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, target))
      setWidth(clamped)
    }
    setSelectedGameId(gameId)
  }, [])

  const handleBack = useCallback(() => {
    viewDirectionRef.current = -1
    const prev = widthBeforeSelectRef.current
    if (prev !== null) {
      setWidth(prev)
      widthBeforeSelectRef.current = null
    }
    setSelectedGameId(null)
  }, [])

  const selectedGame = useMemo(
    () => (selectedGameId ? GAMES.find((g) => g.id === selectedGameId) ?? null : null),
    [selectedGameId],
  )

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return GAMES
    return GAMES.filter(
      (game) =>
        game.title.toLowerCase().includes(q) || game.tagline.toLowerCase().includes(q),
    )
  }, [query])

  return (
    <aside
      aria-hidden={!open}
      className={cn(
        "relative flex shrink-0 flex-col overflow-hidden border-l border-border/80 bg-sidebar",
        !isResizing && "transition-[width] duration-200 ease-out",
        !open && "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={{ width: open ? width : 0 }}
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
        tabIndex={open ? 0 : -1}
      />

      <div
        className={cn(
          "flex min-h-0 flex-1 flex-col animate-in fade-in-0 duration-200 ease-out",
          viewDirectionRef.current === 1 ? "slide-in-from-right-3" : "slide-in-from-left-3",
        )}
        key={selectedGame?.id ?? "__list__"}
      >
        {selectedGame ? (
          <GameDetail game={selectedGame} onBack={handleBack} />
        ) : (
          <GameList
            filtered={filtered}
            onQueryChange={setQuery}
            onSelect={handleSelectGame}
            query={query}
            total={GAMES.length}
          />
        )}
      </div>
    </aside>
  )
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

function GameList({
  filtered,
  onQueryChange,
  onSelect,
  query,
  total,
}: {
  filtered: Game[]
  onQueryChange: (value: string) => void
  onSelect: (gameId: string) => void
  query: string
  total: number
}) {
  return (
    <>
      <div className="flex h-10 items-center justify-between border-b border-border/70 px-3">
        <div className="flex items-center gap-1.5">
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            Arcade
          </span>
          <span className="rounded-full bg-muted/80 px-1.5 py-[1px] font-mono text-[10px] leading-none tabular-nums text-muted-foreground">
            {total}
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
            onChange={(e) => onQueryChange(e.target.value)}
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
                <GameRow game={game} onSelect={onSelect} />
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  )
}

function GameRow({ game, onSelect }: { game: Game; onSelect: (gameId: string) => void }) {
  return (
    <button
      className={cn(
        "group flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors",
        "hover:bg-secondary/50",
      )}
      onClick={() => onSelect(game.id)}
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

// ---------------------------------------------------------------------------
// Detail view — player dashboard
// ---------------------------------------------------------------------------

function GameDetail({ game, onBack }: { game: Game; onBack: () => void }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex py-[7px] shrink-0 items-center gap-2 border-b border-border/70 pl-1.5 pr-3">
        <button
          aria-label="Back to games"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          onClick={onBack}
          type="button"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
        <div className="flex h-5 w-5 shrink-0 items-center justify-center overflow-hidden rounded-sm border border-border/70 bg-background/60 p-[1px]">
          <PixelArt glyph={game.glyph} />
        </div>
        <span className="truncate text-[12.5px] font-medium text-foreground">{game.title}</span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto scrollbar-thin">
        <div className="flex shrink-0 items-center justify-center border-b border-border/70 bg-background/40 px-6 py-7">
          {game.id === "tetris" ? (
            <Tetris active />
          ) : game.id === "space-invaders" ? (
            <SpaceInvaders active />
          ) : game.id === "snake" ? (
            <Snake active />
          ) : game.id === "pacman" ? (
            <Pacman active />
          ) : game.id === "breakout" ? (
            <Breakout active />
          ) : game.id === "asteroids" ? (
            <Asteroids active />
          ) : (
            <GameCanvas glyph={game.glyph} />
          )}
        </div>

        <div className="grid grid-cols-3 gap-px border-b border-border/70 bg-border/60">
          <StatCell label="Personal best" value={game.stats.personalBest} highlight />
          <StatCell label="Runs" value={String(game.stats.runs)} />
          <StatCell label="Time played" value={game.stats.timePlayed} />
        </div>

        <section className="border-b border-border/70 px-4 py-3">
          <div className="mb-2 flex items-center justify-between">
            <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Leaderboard
            </div>
            <button
              className="text-[10px] text-muted-foreground transition-colors hover:text-foreground"
              type="button"
            >
              Full board →
            </button>
          </div>
          <ul className="flex flex-col gap-px">
            {game.stats.leaderboard.map((entry, index) => {
              const rank = index + 1
              return (
                <li
                  className={cn(
                    "flex items-center gap-2.5 rounded-sm px-1.5 py-1.5 transition-colors",
                    entry.you ? "bg-primary/10" : "hover:bg-secondary/40",
                  )}
                  key={`${entry.name}-${index}`}
                >
                  <span
                    className={cn(
                      "w-4 shrink-0 text-center font-mono text-[11px] tabular-nums",
                      rank === 1 ? "text-primary" : "text-muted-foreground",
                    )}
                  >
                    {rank}
                  </span>
                  <span
                    className={cn(
                      "min-w-0 flex-1 truncate text-[12px]",
                      entry.you ? "font-medium text-foreground" : "text-foreground/85",
                    )}
                  >
                    {entry.name}
                  </span>
                  {entry.you ? (
                    <span className="rounded-sm bg-primary/20 px-1 py-[1px] font-mono text-[9px] uppercase tracking-[0.14em] text-primary">
                      You
                    </span>
                  ) : null}
                  <span className="font-mono text-[11.5px] tabular-nums text-foreground/90">
                    {entry.score}
                  </span>
                </li>
              )
            })}
          </ul>
        </section>
      </div>
    </div>
  )
}

function GameCanvas({ glyph }: { glyph: PixelGlyph }) {
  return (
    <button
      aria-label="Play"
      className="group relative flex aspect-[4/3] w-full max-w-xl items-center justify-center overflow-hidden rounded-md border border-border bg-black transition-colors hover:border-primary/50 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
      type="button"
    >
      <div className="flex h-2/3 w-2/3 items-center justify-center transition-opacity group-hover:opacity-60">
        <PixelArt glyph={glyph} />
      </div>
      <div className="pointer-events-none absolute inset-x-0 bottom-3 animate-pulse text-center font-mono text-[10px] uppercase tracking-[0.28em] text-foreground/55 transition-opacity group-hover:opacity-0">
        Press Start
      </div>

      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/45 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
        <div className="flex h-14 w-14 items-center justify-center rounded-full border border-white/70 bg-black/50">
          <Play className="h-5 w-5 fill-current text-white" />
        </div>
        <span className="font-mono text-[10px] uppercase tracking-[0.28em] text-white/80">
          Click to play
        </span>
      </div>

      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 opacity-25"
        style={{
          backgroundImage:
            "repeating-linear-gradient(0deg, rgba(255,255,255,0.05) 0 1px, transparent 1px 3px)",
        }}
      />
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0"
        style={{ boxShadow: "inset 0 0 60px rgba(0,0,0,0.6)" }}
      />
    </button>
  )
}

function StatCell({
  label,
  value,
  highlight = false,
}: {
  label: string
  value: string
  highlight?: boolean
}) {
  return (
    <div className="flex flex-col gap-1 bg-sidebar px-3 py-2.5">
      <span className="text-[9px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </span>
      <span
        className={cn(
          "font-mono text-[14.5px] font-medium tabular-nums leading-none",
          highlight ? "text-primary" : "text-foreground/90",
        )}
      >
        {value}
      </span>
    </div>
  )
}

