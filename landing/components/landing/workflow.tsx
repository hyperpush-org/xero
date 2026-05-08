import {
  Bot,
  Layers,
  Rocket,
  Wrench,
  ChevronRight,
} from "lucide-react"

type Stage = {
  n: string
  label: string
  title: string
  caption: string
  icon: React.ReactNode
  visual: React.ReactNode
}

const stages: Stage[] = [
  {
    n: "01",
    label: "Build",
    title: "Design the agent",
    caption: "Pick tools, memory, approval rules.",
    icon: <Wrench className="h-3.5 w-3.5" />,
    visual: <AssemblyVisual />,
  },
  {
    n: "02",
    label: "Chain",
    title: "Compose the workflow",
    caption: "Steps, branches, loops, gates.",
    icon: <Layers className="h-3.5 w-3.5" />,
    visual: <LinkedRingsVisual />,
  },
  {
    n: "03",
    label: "Approve",
    title: "Decide from your phone",
    caption: "Discord or Telegram on real calls.",
    icon: <Bot className="h-3.5 w-3.5" />,
    visual: <GateBeamVisual />,
  },
  {
    n: "04",
    label: "Ship",
    title: "Project lands done",
    caption: "Branch, merge, deploy, repeat.",
    icon: <Rocket className="h-3.5 w-3.5" />,
    visual: <TrajectoryVisual />,
  },
]

export function Workflow() {
  return (
    <section id="workflow" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            How it works
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            From brief to shipped, in four stages.
          </h2>
        </div>

        <ol className="mt-16 grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4 lg:gap-5">
          {stages.map((s, i) => (
            <li key={s.n} className="relative flex">
              <article className="relative flex w-full flex-col overflow-hidden rounded-2xl border border-border/70 bg-card">
                <header className="flex items-center justify-between border-b border-border/60 bg-secondary/20 px-4 py-2.5">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex h-6 w-6 items-center justify-center rounded-md bg-primary/15 text-primary">
                      {s.icon}
                    </span>
                    <span className="font-mono text-[10px] uppercase tracking-[0.2em] text-primary">
                      {s.n} · {s.label}
                    </span>
                  </div>
                  <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground/60">
                    stage
                  </span>
                </header>

                <div className="flex h-48 items-center justify-center bg-gradient-to-b from-transparent to-secondary/[0.08] p-4">
                  {s.visual}
                </div>

                <footer className="border-t border-border/60 px-4 py-4">
                  <h3 className="text-base font-medium tracking-tight">
                    {s.title}
                  </h3>
                  <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                    {s.caption}
                  </p>
                </footer>
              </article>

              {i < stages.length - 1 && (
                <span
                  aria-hidden
                  className="pointer-events-none absolute right-[-14px] top-[80px] z-10 hidden h-7 w-7 items-center justify-center rounded-full border border-primary/40 bg-background text-primary shadow-[0_0_0_4px_var(--background)] lg:flex"
                >
                  <ChevronRight className="h-3.5 w-3.5" />
                </span>
              )}
            </li>
          ))}
        </ol>
      </div>
    </section>
  )
}

/* ---------- Stage visuals ---------- */

/* 01 · Build — exploded modules converging into a center core */
function AssemblyVisual() {
  return (
    <svg viewBox="0 0 220 160" className="h-full w-full max-w-[220px]" aria-hidden>
      <defs>
        <radialGradient id="as-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.45" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* glow */}
      <circle cx="110" cy="80" r="50" fill="url(#as-glow)" />

      {/* Convergence dashed paths */}
      {[
        { x: 22, y: 26 },
        { x: 198, y: 26 },
        { x: 22, y: 134 },
        { x: 198, y: 134 },
      ].map((m, i) => (
        <path
          key={i}
          d={`M ${m.x} ${m.y} L 110 80`}
          stroke="color-mix(in oklab, var(--primary) 55%, transparent)"
          strokeWidth="0.9"
          strokeDasharray="3 4"
          fill="none"
        />
      ))}

      {/* outer modules (geometric primitives) */}
      {/* tools — square */}
      <g>
        <rect x="10" y="14" width="24" height="24" rx="3" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 65%, transparent)" />
        <line x1="15" y1="22" x2="29" y2="22" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" />
        <line x1="15" y1="26" x2="29" y2="26" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" />
        <line x1="15" y1="30" x2="24" y2="30" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" />
      </g>
      {/* memory — triangle */}
      <g>
        <polygon points="198,14 218,38 178,38" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 65%, transparent)" />
        <circle cx="198" cy="32" r="2.5" fill="var(--primary)" />
      </g>
      {/* approvals — diamond */}
      <g>
        <polygon points="22,134 34,122 46,134 34,146" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 65%, transparent)" />
        <path d="M 28 134 L 32 138 L 40 130" fill="none" stroke="var(--primary)" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
      </g>
      {/* scope — hex */}
      <g>
        <polygon points="186,134 198,122 210,134 210,146 198,158 186,146" transform="translate(0 -12)" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 65%, transparent)" />
      </g>

      {/* core */}
      <g>
        <circle cx="110" cy="80" r="20" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="var(--primary)" strokeWidth="1.2" />
        <circle cx="110" cy="80" r="11" fill="color-mix(in oklab, var(--primary) 35%, var(--card))" />
        <circle cx="110" cy="80" r="5" fill="var(--primary)" />
      </g>

      {/* caption */}
      <text x="110" y="152" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        assemble · agent
      </text>
    </svg>
  )
}

/* 02 · Chain — interlocking rings (chain links) along a horizontal axis */
function LinkedRingsVisual() {
  return (
    <svg viewBox="0 0 220 160" className="h-full w-full max-w-[220px]" aria-hidden>
      <defs>
        <linearGradient id="lr-edge" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 25%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
        <radialGradient id="lr-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.32" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* glow band */}
      <ellipse cx="110" cy="80" rx="100" ry="32" fill="url(#lr-glow)" />

      {/* Chain of 4 rings (rotated ovals interlocking) */}
      {[40, 80, 120, 160].map((cx, i) => {
        const isHighlight = i === 2
        return (
          <g key={i}>
            <ellipse
              cx={cx}
              cy="80"
              rx="22"
              ry="14"
              fill="none"
              stroke={isHighlight ? "var(--primary)" : "url(#lr-edge)"}
              strokeWidth={isHighlight ? 2 : 1.6}
              transform={`rotate(${i % 2 === 0 ? -16 : 16} ${cx} 80)`}
              opacity={isHighlight ? 1 : 0.85}
            />
            {isHighlight && (
              <ellipse
                cx={cx}
                cy="80"
                rx="28"
                ry="20"
                fill="none"
                stroke="var(--primary)"
                strokeWidth="0.8"
                opacity="0.4"
                transform={`rotate(16 ${cx} 80)`}
              />
            )}
          </g>
        )
      })}

      {/* Branch indicator off the chain */}
      <path
        d="M 120 80 C 140 80, 150 116, 180 122"
        fill="none"
        stroke="color-mix(in oklab, var(--primary) 60%, transparent)"
        strokeWidth="1.1"
        strokeDasharray="2 4"
      />
      <circle cx="180" cy="122" r="4" fill="var(--card)" stroke="var(--primary)" strokeWidth="1.2" />

      {/* Step number markers */}
      <g fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" letterSpacing="1.5" opacity="0.7">
        {[40, 80, 120, 160].map((cx, i) => (
          <text key={i} x={cx} y="42" textAnchor="middle">{`s${i + 1}`}</text>
        ))}
      </g>

      <text x="110" y="152" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        chain · branch · loop
      </text>
    </svg>
  )
}

/* 03 · Approve — checkmark beam crossing a gate */
function GateBeamVisual() {
  return (
    <svg viewBox="0 0 220 160" className="h-full w-full max-w-[220px]" aria-hidden>
      <defs>
        <radialGradient id="gb-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.5" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
        <linearGradient id="gb-beam" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 5%, transparent)" />
          <stop offset="50%" stopColor="var(--primary)" />
          <stop offset="100%" stopColor="color-mix(in oklab, var(--primary) 5%, transparent)" />
        </linearGradient>
      </defs>

      {/* Halo */}
      <circle cx="110" cy="80" r="58" fill="url(#gb-glow)" />

      {/* Beam */}
      <line x1="14" y1="80" x2="206" y2="80" stroke="url(#gb-beam)" strokeWidth="2.4" strokeLinecap="round" />
      {/* secondary thin beam */}
      <line x1="14" y1="80" x2="206" y2="80" stroke="color-mix(in oklab, var(--primary) 50%, transparent)" strokeWidth="0.6" strokeDasharray="2 4" />

      {/* Gate frame (two posts) */}
      <g stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" fill="none">
        <line x1="80" y1="36" x2="80" y2="124" />
        <line x1="140" y1="36" x2="140" y2="124" />
        <line x1="74" y1="36" x2="146" y2="36" />
        <line x1="74" y1="124" x2="146" y2="124" />
      </g>

      {/* Check glyph at gate center */}
      <g>
        <circle cx="110" cy="80" r="20" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="var(--primary)" strokeWidth="1.4" />
        <path
          d="M 100 80 L 108 88 L 122 72"
          fill="none"
          stroke="var(--primary)"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </g>

      {/* Source / target end caps */}
      <circle cx="14" cy="80" r="4" fill="color-mix(in oklab, var(--primary) 60%, transparent)" />
      <polygon points="196,76 206,80 196,84" fill="var(--primary)" />

      {/* Annotation labels */}
      <g fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" letterSpacing="1.5" opacity="0.75">
        <text x="14" y="68">request</text>
        <text x="206" y="68" textAnchor="end">resume</text>
      </g>

      <text x="110" y="152" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        you · in the loop
      </text>
    </svg>
  )
}

/* 04 · Ship — trajectory curve with a moving comet, landing in a target */
function TrajectoryVisual() {
  return (
    <svg viewBox="0 0 220 160" className="h-full w-full max-w-[220px]" aria-hidden>
      <defs>
        <linearGradient id="tj-trail" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 5%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
        <radialGradient id="tj-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.5" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Stippled "stars" */}
      <g fill="color-mix(in oklab, var(--muted-foreground) 35%, transparent)">
        <circle cx="40" cy="32" r="1" />
        <circle cx="68" cy="20" r="0.6" />
        <circle cx="160" cy="24" r="1.2" />
        <circle cx="190" cy="48" r="0.8" />
        <circle cx="22" cy="100" r="0.6" />
        <circle cx="56" cy="118" r="1" />
        <circle cx="200" cy="100" r="0.7" />
      </g>

      {/* Trajectory curve */}
      <path
        d="M 16 130 Q 90 130, 130 80 T 200 32"
        fill="none"
        stroke="url(#tj-trail)"
        strokeWidth="2"
        strokeLinecap="round"
      />
      {/* Animated dash overlay */}
      <path
        d="M 16 130 Q 90 130, 130 80 T 200 32"
        fill="none"
        stroke="color-mix(in oklab, var(--primary) 80%, transparent)"
        strokeWidth="1.4"
        strokeDasharray="3 8"
        className="animate-flow-dash"
      />

      {/* Origin (branch dot) */}
      <g transform="translate(16 130)">
        <circle r="6" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" />
        <circle r="2.4" fill="color-mix(in oklab, var(--primary) 70%, transparent)" />
      </g>

      {/* Target (concentric rings) */}
      <g transform="translate(200 32)">
        <circle r="22" fill="url(#tj-glow)" />
        <circle r="14" fill="none" stroke="var(--primary)" strokeWidth="0.8" strokeDasharray="2 3" />
        <circle r="9" fill="none" stroke="var(--primary)" strokeWidth="1" />
        <circle r="4" fill="var(--primary)" />
      </g>

      {/* Comet head along the curve */}
      <circle cx="130" cy="80" r="5" fill="var(--primary)" />
      <circle cx="130" cy="80" r="11" fill="none" stroke="var(--primary)" strokeWidth="0.8" opacity="0.5" />

      {/* Labels */}
      <g fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" letterSpacing="1.5" opacity="0.75">
        <text x="16" y="148">branch</text>
        <text x="200" y="22" textAnchor="end" fill="var(--primary)" fontWeight="600">prod</text>
      </g>

      <text x="110" y="152" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        merge · deploy · repeat
      </text>
    </svg>
  )
}
