"use client"

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react"
import { ArrowUp, Mic, Sparkles, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

const RAINBOW_PALETTE = [
  "#f43f5e",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#a855f7",
  "#ec4899",
] as const

const COMPOSER_WIDTH = 320
const COMPOSER_OFFSET = 12

export function isDevServerUrl(url: string | null | undefined): boolean {
  if (!url) return false
  try {
    const parsed = new URL(url)
    const host = parsed.hostname.toLowerCase()
    if (host === "localhost" || host === "127.0.0.1" || host === "0.0.0.0" || host === "::1") {
      return true
    }
    if (/^10\./.test(host)) return true
    if (/^192\.168\./.test(host)) return true
    if (/^172\.(1[6-9]|2\d|3[0-1])\./.test(host)) return true
    return false
  } catch {
    return false
  }
}

interface Point {
  x: number
  y: number
}

interface Stroke {
  id: string
  points: Point[]
}

interface ComposerAnchor {
  surfaceWidth: number
  surfaceHeight: number
  x: number
  y: number
  contextLabel: string
}

interface PenOverlayProps {
  pageLabel: string | null
  onSubmit: (payload: { text: string; strokes: number }) => void
  onExit: () => void
}

export function PenOverlay({ pageLabel, onSubmit, onExit }: PenOverlayProps) {
  const surfaceRef = useRef<HTMLDivElement | null>(null)
  const [strokes, setStrokes] = useState<Stroke[]>([])
  const [activeStroke, setActiveStroke] = useState<Stroke | null>(null)
  const [anchor, setAnchor] = useState<ComposerAnchor | null>(null)

  const totalStrokes = strokes.length + (activeStroke ? 1 : 0)

  const handlePointerDown = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    const rect = surfaceRef.current?.getBoundingClientRect()
    if (!rect) return
    event.currentTarget.setPointerCapture(event.pointerId)
    setAnchor(null)
    setActiveStroke({
      id: `stroke-${Date.now()}-${Math.random().toString(16).slice(2, 6)}`,
      points: [{ x: event.clientX - rect.left, y: event.clientY - rect.top }],
    })
  }, [])

  const handlePointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    setActiveStroke((current) => {
      if (!current) return current
      const rect = surfaceRef.current?.getBoundingClientRect()
      if (!rect) return current
      const next = { x: event.clientX - rect.left, y: event.clientY - rect.top }
      const last = current.points[current.points.length - 1]
      if (last && Math.hypot(next.x - last.x, next.y - last.y) < 1.5) {
        return current
      }
      return { ...current, points: [...current.points, next] }
    })
  }, [])

  const handlePointerUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    setActiveStroke((current) => {
      if (!current) return null
      if (current.points.length < 2) return null
      setStrokes((existing) => [...existing, current])
      const xs = current.points.map((p) => p.x)
      const ys = current.points.map((p) => p.y)
      const maxX = Math.max(...xs)
      const minY = Math.min(...ys)
      const surface = surfaceRef.current?.getBoundingClientRect()
      if (surface) {
        setAnchor({
          surfaceWidth: surface.width,
          surfaceHeight: surface.height,
          x: maxX,
          y: minY,
          contextLabel: `Sketch · ${current.points.length} pts`,
        })
      }
      return null
    })
  }, [])

  const handleClear = useCallback(() => {
    setStrokes([])
    setActiveStroke(null)
    setAnchor(null)
  }, [])

  const handleSubmit = useCallback(
    (text: string) => {
      onSubmit({ text, strokes: strokes.length })
      setStrokes([])
      setAnchor(null)
    },
    [onSubmit, strokes.length],
  )

  return (
    <div
      ref={surfaceRef}
      className="absolute inset-0 z-20 select-none touch-none"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
      style={{ cursor: "crosshair" }}
      data-testid="browser-pen-overlay"
    >
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_top,rgba(99,102,241,0.08),transparent_60%)]" />
      <div className="pointer-events-none absolute inset-0 [background-image:linear-gradient(rgba(255,255,255,0.04)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.04)_1px,transparent_1px)] [background-size:24px_24px]" />

      <PenStrokesLayer strokes={strokes} active={activeStroke} />

      <div className="pointer-events-none absolute inset-x-0 top-2 flex justify-center">
        <div className="pointer-events-auto flex items-center gap-2 rounded-full border border-border/60 bg-card/85 px-3 py-1 text-[10.5px] text-muted-foreground shadow-lg backdrop-blur">
          <Sparkles className="h-3 w-3 text-primary" />
          <span className="font-medium text-foreground">Pen mode</span>
          <span aria-hidden="true">·</span>
          <span>{pageLabel ? `On ${pageLabel}` : "Sketch over the page"}</span>
          <span aria-hidden="true">·</span>
          <button
            type="button"
            className="rounded px-1.5 py-0.5 text-[10.5px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-40"
            onClick={handleClear}
            disabled={totalStrokes === 0}
          >
            Clear
          </button>
          <button
            type="button"
            className="rounded px-1.5 py-0.5 text-[10.5px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            onClick={onExit}
          >
            Exit
          </button>
        </div>
      </div>

      {anchor ? (
        <MiniComposer
          anchor={anchor}
          accent="rainbow"
          title="Sketch note"
          placeholder="Tell the agent what to do with this sketch…"
          onSubmit={handleSubmit}
          onClose={() => setAnchor(null)}
        />
      ) : null}
    </div>
  )
}

interface PenStrokesLayerProps {
  strokes: Stroke[]
  active: Stroke | null
}

function PenStrokesLayer({ strokes, active }: PenStrokesLayerProps) {
  return (
    <svg
      className="pointer-events-none absolute inset-0 h-full w-full"
      data-testid="browser-pen-strokes"
    >
      {strokes.map((stroke) => (
        <StrokePath key={stroke.id} stroke={stroke} />
      ))}
      {active ? <StrokePath stroke={active} /> : null}
    </svg>
  )
}

function StrokePath({ stroke }: { stroke: Stroke }) {
  if (stroke.points.length < 2) {
    const head = stroke.points[0]
    if (!head) return null
    return <circle cx={head.x} cy={head.y} r={1.5} fill={RAINBOW_PALETTE[0]} />
  }
  return (
    <g strokeWidth={3} strokeLinecap="round">
      {stroke.points.slice(1).map((point, index) => {
        const prev = stroke.points[index]
        const color = RAINBOW_PALETTE[index % RAINBOW_PALETTE.length]
        return (
          <line
            key={index}
            x1={prev.x}
            y1={prev.y}
            x2={point.x}
            y2={point.y}
            stroke={color}
          />
        )
      })}
    </g>
  )
}

interface MockElement {
  id: string
  tag: string
  description: string
  top: number
  left: number
  width: number
  height: number
}

const MOCK_ELEMENTS: MockElement[] = [
  { id: "header", tag: "<header>", description: "Top navigation", top: 16, left: 16, width: 1, height: 56 },
  { id: "title", tag: "<h1>", description: "Page heading", top: 96, left: 32, width: 0.45, height: 36 },
  { id: "subtitle", tag: "<p>", description: "Subheading copy", top: 138, left: 32, width: 0.55, height: 22 },
  { id: "card-1", tag: "<Card>", description: "Primary action card", top: 184, left: 32, width: 0.4, height: 168 },
  { id: "card-2", tag: "<Card>", description: "Stats card", top: 184, left: 0.46, width: 0.5, height: 80 },
  { id: "card-3", tag: "<Card>", description: "Activity feed card", top: 272, left: 0.46, width: 0.5, height: 80 },
  { id: "cta", tag: "<Button>", description: "Primary CTA button", top: 376, left: 32, width: 132, height: 36 },
  { id: "secondary-cta", tag: "<Button>", description: "Secondary action", top: 376, left: 176, width: 110, height: 36 },
]

function resolveElementGeometry(
  spec: MockElement,
  surfaceWidth: number,
  surfaceHeight: number,
) {
  // When the surface hasn't measured yet (e.g. before the first paint or in
  // jsdom which reports 0-sized rects) fall back to a sensible default so the
  // overlay still renders deterministically rather than collapsing to nothing.
  const effectiveWidth = surfaceWidth > 0 ? surfaceWidth : 720
  const left = spec.left <= 1 ? Math.round(spec.left * effectiveWidth) : spec.left
  let width: number
  if (spec.width === 1) {
    width = Math.max(64, effectiveWidth - 32)
  } else if (spec.width <= 1) {
    width = Math.round(spec.width * effectiveWidth)
  } else {
    width = spec.width
  }
  if (surfaceWidth > 0 && left + width > surfaceWidth - 8) {
    width = Math.max(48, surfaceWidth - left - 8)
  }
  // We don't drop elements that overflow vertically — overflow:hidden on the
  // viewport handles it, and dropping would make the layout flicker as the
  // sidebar resizes.
  void surfaceHeight
  return { top: spec.top, left, width, height: spec.height }
}

interface InspectOverlayProps {
  pageLabel: string | null
  onSubmit: (payload: { text: string; element: string }) => void
  onExit: () => void
}

export function InspectOverlay({ pageLabel, onSubmit, onExit }: InspectOverlayProps) {
  const surfaceRef = useRef<HTMLDivElement | null>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [hoveredId, setHoveredId] = useState<string | null>(null)
  const [selected, setSelected] = useState<{
    spec: MockElement
    geometry: { top: number; left: number; width: number; height: number }
  } | null>(null)

  useLayoutEffect(() => {
    const element = surfaceRef.current
    if (!element) return
    const update = () => {
      const rect = element.getBoundingClientRect()
      setSize({ width: rect.width, height: rect.height })
    }
    update()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(update)
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  const elements = MOCK_ELEMENTS.map((spec) => ({
    spec,
    geometry: resolveElementGeometry(spec, size.width, size.height),
  }))

  const anchor: ComposerAnchor | null = selected
    ? {
        surfaceWidth: size.width,
        surfaceHeight: size.height,
        x: selected.geometry.left + selected.geometry.width,
        y: selected.geometry.top,
        contextLabel: `${selected.spec.tag} · ${selected.spec.description}`,
      }
    : null

  return (
    <div
      ref={surfaceRef}
      className="absolute inset-0 z-20 select-none overflow-hidden"
      style={{ cursor: "crosshair" }}
      data-testid="browser-inspect-overlay"
    >
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_right,rgba(34,197,94,0.06),transparent_55%)]" />
      <div className="pointer-events-none absolute inset-0 [background-image:linear-gradient(rgba(255,255,255,0.04)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.04)_1px,transparent_1px)] [background-size:24px_24px]" />

      {elements.map(({ spec, geometry }) => {
        const isHovered = hoveredId === spec.id
        const isSelected = selected?.spec.id === spec.id
        return (
          <button
            key={spec.id}
            type="button"
            data-testid={`browser-inspect-element-${spec.id}`}
            className={cn(
              "absolute rounded-md text-left transition-colors",
              "border bg-background/45 backdrop-blur-[1px]",
              isSelected
                ? "border-emerald-400/80 bg-emerald-400/10 ring-2 ring-emerald-400/60"
                : isHovered
                  ? "border-emerald-400/60 bg-emerald-400/5"
                  : "border-border/50 hover:border-emerald-400/60",
            )}
            style={{
              top: geometry.top,
              left: geometry.left,
              width: geometry.width,
              height: geometry.height,
            }}
            onMouseEnter={() => setHoveredId(spec.id)}
            onMouseLeave={() => setHoveredId((current) => (current === spec.id ? null : current))}
            onFocus={() => setHoveredId(spec.id)}
            onBlur={() => setHoveredId((current) => (current === spec.id ? null : current))}
            onClick={() => setSelected({ spec, geometry })}
          >
            <span className={cn(
              "absolute -top-5 left-0 rounded-sm px-1.5 py-0.5 text-[10px] font-mono leading-none",
              isSelected || isHovered
                ? "bg-emerald-400/85 text-black"
                : "bg-background/80 text-muted-foreground",
            )}>
              {spec.tag}
            </span>
            <span className="block px-2 pt-2 text-[10.5px] text-muted-foreground/80">
              {spec.description}
            </span>
          </button>
        )
      })}

      <div className="pointer-events-none absolute inset-x-0 top-2 flex justify-center">
        <div className="pointer-events-auto flex items-center gap-2 rounded-full border border-border/60 bg-card/85 px-3 py-1 text-[10.5px] text-muted-foreground shadow-lg backdrop-blur">
          <Sparkles className="h-3 w-3 text-emerald-400" />
          <span className="font-medium text-foreground">Inspect mode</span>
          <span aria-hidden="true">·</span>
          <span>{pageLabel ? `On ${pageLabel}` : "Hover an element to select it"}</span>
          <span aria-hidden="true">·</span>
          <button
            type="button"
            className="rounded px-1.5 py-0.5 text-[10.5px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
            onClick={onExit}
          >
            Exit
          </button>
        </div>
      </div>

      {anchor && selected ? (
        <MiniComposer
          anchor={anchor}
          accent="emerald"
          title="Element prompt"
          placeholder="Describe how this element should change…"
          onSubmit={(text) => {
            onSubmit({ text, element: selected.spec.tag })
            setSelected(null)
          }}
          onClose={() => setSelected(null)}
        />
      ) : null}
    </div>
  )
}

interface MiniComposerProps {
  anchor: ComposerAnchor
  accent: "rainbow" | "emerald"
  title: string
  placeholder: string
  onSubmit: (text: string) => void
  onClose: () => void
}

function MiniComposer({ anchor, accent, title, placeholder, onSubmit, onClose }: MiniComposerProps) {
  const [text, setText] = useState("")
  const [isListening, setIsListening] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)

  useEffect(() => {
    textareaRef.current?.focus()
  }, [])

  // Position the composer next to the anchor, clamping inside the surface.
  let left = anchor.x + COMPOSER_OFFSET
  let top = anchor.y
  if (anchor.surfaceWidth > 0 && left + COMPOSER_WIDTH + 8 > anchor.surfaceWidth) {
    left = Math.max(8, anchor.x - COMPOSER_WIDTH - COMPOSER_OFFSET)
  }
  if (anchor.surfaceHeight > 0) {
    top = Math.max(8, Math.min(top, anchor.surfaceHeight - 168))
  }

  const handleSend = () => {
    const trimmed = text.trim()
    if (trimmed.length === 0) return
    onSubmit(trimmed)
    setText("")
  }

  const accentRing = accent === "emerald" ? "ring-emerald-400/40" : "ring-primary/30"

  return (
    <div
      className={cn(
        "absolute z-30 w-[320px] overflow-hidden rounded-2xl border border-border/70 bg-card/95 shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)] ring-1",
        accentRing,
      )}
      data-testid="browser-mini-composer"
      style={{ top, left }}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <div className="flex items-center justify-between border-b border-border/40 px-3 py-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            {accent === "rainbow" ? (
              <span aria-hidden="true" className="inline-flex h-1.5 w-7 overflow-hidden rounded-full">
                {RAINBOW_PALETTE.map((color) => (
                  <span key={color} className="h-full flex-1" style={{ background: color }} />
                ))}
              </span>
            ) : (
              <span aria-hidden="true" className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" />
            )}
            <span className="text-[11.5px] font-semibold text-foreground">{title}</span>
          </div>
          <div className="truncate text-[10px] text-muted-foreground/80">{anchor.contextLabel}</div>
        </div>
        <button
          type="button"
          aria-label="Close mini composer"
          className="flex h-5 w-5 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          onClick={onClose}
        >
          <X className="h-3 w-3" />
        </button>
      </div>

      <Textarea
        aria-label="Mini composer input"
        ref={textareaRef}
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault()
            handleSend()
          }
          if (event.key === "Escape") {
            event.preventDefault()
            onClose()
          }
        }}
        placeholder={placeholder}
        rows={3}
        className="max-h-32 min-h-[64px] resize-none border-0 bg-transparent px-3 pb-1 pt-2 text-[12px] leading-relaxed text-foreground placeholder:text-muted-foreground/60 shadow-none focus-visible:ring-0"
      />

      <div className="flex items-center justify-between border-t border-border/40 bg-background/30 px-2 py-1.5">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              size="icon-sm"
              variant={isListening ? "outline" : "ghost"}
              aria-label={isListening ? "Stop dictation" : "Start dictation"}
              aria-pressed={isListening}
              className={cn(
                "h-7 w-7 rounded-md px-0",
                isListening ? "border-destructive/35 bg-destructive/10 text-destructive" : "text-muted-foreground/80",
              )}
              onClick={() => setIsListening((current) => !current)}
            >
              <Mic className={cn("h-3.5 w-3.5", isListening ? "animate-pulse" : null)} strokeWidth={2.5} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{isListening ? "Listening" : "Dictate"}</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              size="icon-sm"
              variant="secondary"
              aria-label="Send"
              disabled={text.trim().length === 0}
              className="h-7 w-7 rounded-md px-0"
              onClick={handleSend}
            >
              <ArrowUp className="h-3.5 w-3.5" strokeWidth={2.5} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">Send</TooltipContent>
        </Tooltip>
      </div>
    </div>
  )
}
