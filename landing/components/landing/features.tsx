import { Bot, Database, Workflow as WorkflowIcon } from "lucide-react"

type Row = {
  tag: string
  icon: React.ReactNode
  title: string
  description: string
  bullets: string[]
  visual: React.ReactNode
}

const rows: Row[] = [
  {
    tag: "Custom agents",
    icon: <Bot className="h-3.5 w-3.5" />,
    title: "Agents you actually design.",
    description:
      "Pick each agent's tools, memory, and where it has to stop and ask.",
    bullets: [
      "Per-agent tools, memory, approvals",
      "Project or global scope",
      "Visual builder, no YAML",
    ],
    visual: <AgentCoreVisual />,
  },
  {
    tag: "Workflows",
    icon: <WorkflowIcon className="h-3.5 w-3.5" />,
    title: "Chain agents. Ship whole projects.",
    description:
      "Compose agents into long workflows that drive a project end to end.",
    bullets: [
      "Steps, branches, loops, gates",
      "Long autonomous runs with handoffs",
      "Human checkpoints only when it matters",
    ],
    visual: <WorkflowFlowVisual />,
  },
  {
    tag: "Sessions",
    icon: <Database className="h-3.5 w-3.5" />,
    title: "Pick up weeks later.",
    description:
      "A local journal per project. Branch, rewind, or hand a thread to a different agent.",
    bullets: [
      "Branch and rewind without overwrites",
      "Search, export, replay",
      "Six panes per project",
    ],
    visual: <TimeStrataVisual />,
  },
]

export function Features() {
  return (
    <section id="product" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            What's different
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Agents you design. Workflows that ship projects.
          </h2>
        </div>

        <div className="mt-20 flex flex-col gap-24 lg:gap-32">
          {rows.map((row, i) => (
            <FeatureRow key={row.tag} row={row} reverse={i % 2 === 1} />
          ))}
        </div>
      </div>
    </section>
  )
}

function FeatureRow({ row, reverse }: { row: Row; reverse: boolean }) {
  return (
    <div className="grid grid-cols-1 items-center gap-10 lg:grid-cols-12 lg:gap-16">
      <div
        className={`lg:col-span-5 ${
          reverse ? "lg:order-2 lg:col-start-8" : "lg:order-1"
        }`}
      >
        <div className="inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-secondary/40 px-2.5 py-1 font-mono text-[11px] text-muted-foreground">
          <span className="text-primary">{row.icon}</span>
          {row.tag}
        </div>
        <h3 className="mt-4 font-sans text-2xl font-medium tracking-tight text-balance sm:text-3xl lg:text-4xl">
          {row.title}
        </h3>
        <p className="mt-4 text-pretty leading-relaxed text-muted-foreground">
          {row.description}
        </p>
        <ul className="mt-6 flex flex-col gap-2.5">
          {row.bullets.map((b) => (
            <li key={b} className="flex items-start gap-2.5 text-sm">
              <span className="mt-[7px] h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
              <span className="text-foreground/90">{b}</span>
            </li>
          ))}
        </ul>
      </div>

      <div className={`lg:col-span-7 ${reverse ? "lg:order-1" : "lg:order-2"}`}>
        <div className="relative">
          <div
            aria-hidden
            className="pointer-events-none absolute -inset-6 -z-10 rounded-[2rem] bg-gradient-to-br from-primary/10 via-transparent to-transparent blur-2xl"
          />
          <div className="overflow-hidden rounded-2xl border border-border/70 bg-card shadow-[0_30px_80px_-30px_rgba(0,0,0,0.5)]">
            {row.visual}
          </div>
        </div>
      </div>
    </div>
  )
}

/* ---------- Visuals ---------- */

/* 1) Agents you actually design — an Excalidraw-style hand-drawn graph.
   Wobbly nodes for tools, memory, approvals, scope orbit a central
   solana-ops agent node. SVG turbulence filter creates the sketch feel. */
function AgentCoreVisual() {
  return (
    <div className="relative aspect-[4/3] w-full">
      <svg viewBox="0 0 480 360" className="h-full w-full" aria-hidden>
        <defs>
          {/* Hand-drawn wobble for shapes */}
          <filter id="ag-rough" x="-5%" y="-5%" width="110%" height="110%">
            <feTurbulence type="fractalNoise" baseFrequency="0.025" numOctaves="2" seed="4" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale="1.8" />
          </filter>
          {/* Stronger wobble for connector lines */}
          <filter id="ag-wob" x="-6%" y="-6%" width="112%" height="112%">
            <feTurbulence type="fractalNoise" baseFrequency="0.05" numOctaves="2" seed="11" result="n2" />
            <feDisplacementMap in="SourceGraphic" in2="n2" scale="2.4" />
          </filter>
          <pattern id="ag-paper" x="0" y="0" width="16" height="16" patternUnits="userSpaceOnUse">
            <circle cx="1" cy="1" r="0.5" fill="color-mix(in oklab, var(--muted-foreground) 28%, transparent)" />
          </pattern>
        </defs>

        {/* Paper backdrop */}
        <rect width="480" height="360" fill="url(#ag-paper)" opacity="0.55" />

        {/* === Edges (wobbly hand-drawn lines, stop short of node edges) === */}
        <g
          fill="none"
          stroke="color-mix(in oklab, var(--primary) 55%, transparent)"
          strokeWidth="1.6"
          strokeLinecap="round"
          filter="url(#ag-wob)"
        >
          {/* center -> tools (top) — line ends at arrow base */}
          <path d="M 240 143 Q 240 116 240 96" />
          {/* center -> approvals (right) */}
          <path d="M 288 175 Q 328 175 372 175" />
          {/* center -> memory (bottom) */}
          <path d="M 240 207 Q 240 240 240 268" />
          {/* center -> scope (left) */}
          <path d="M 192 175 Q 152 175 108 175" />
        </g>

        {/* Crisp arrowheads (no filter) — tips just before each node edge */}
        <g fill="color-mix(in oklab, var(--primary) 70%, transparent)">
          {/* up arrow at tools (box bottom y=84, tip at y=86) */}
          <path d="M 233 96 L 240 86 L 247 96 Z" />
          {/* right arrow at approvals (box left x=382, tip at x=380) */}
          <path d="M 370 169 L 380 175 L 370 181 Z" />
          {/* down arrow at memory (box top y=280, tip at y=278) */}
          <path d="M 233 268 L 240 278 L 247 268 Z" />
          {/* left arrow at scope (box right x=98, tip at x=100) */}
          <path d="M 110 169 L 100 175 L 110 181 Z" />
        </g>

        {/* === TOOLS node (top) === */}
        <rect
          x="184"
          y="44"
          width="112"
          height="40"
          rx="9"
          fill="color-mix(in oklab, var(--card) 95%, transparent)"
          stroke="color-mix(in oklab, var(--primary) 65%, transparent)"
          strokeWidth="1.8"
          filter="url(#ag-rough)"
        />
        <text
          x="240"
          y="62"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="11"
          fill="var(--foreground)"
          fontWeight="600"
          letterSpacing="0.4"
        >
          tools
        </text>
        <g transform="translate(208 75)">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <circle key={i} cx={i * 7} cy="0" r="2.2" fill="var(--primary)" />
          ))}
          <circle
            cx="42"
            cy="0"
            r="2"
            fill="none"
            stroke="color-mix(in oklab, var(--border) 95%, transparent)"
            strokeWidth="0.8"
            strokeDasharray="1.5 1.5"
          />
        </g>

        {/* === APPROVALS node (right) === */}
        <rect
          x="382"
          y="134"
          width="88"
          height="82"
          rx="9"
          fill="color-mix(in oklab, var(--card) 95%, transparent)"
          stroke="color-mix(in oklab, var(--primary) 65%, transparent)"
          strokeWidth="1.8"
          filter="url(#ag-rough)"
        />
        <text
          x="426"
          y="152"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="11"
          fill="var(--foreground)"
          fontWeight="600"
          letterSpacing="0.4"
        >
          approvals
        </text>
        {[
          { label: "git push", b: "ASK" as const, y: 170 },
          { label: "tx send", b: "ASK" as const, y: 188 },
          { label: "repo edit", b: "AUTO" as const, y: 206 },
        ].map((r) => (
          <g key={r.label} fontFamily="var(--font-mono)" fontSize="8.5">
            <text x="390" y={r.y} fill="var(--foreground)" opacity="0.85" letterSpacing="0.3">
              {r.label}
            </text>
            <text
              x="462"
              y={r.y}
              textAnchor="end"
              fill={r.b === "ASK" ? "var(--primary)" : "var(--muted-foreground)"}
              fontWeight="700"
              letterSpacing="1.2"
              fontSize="8"
            >
              {r.b}
            </text>
          </g>
        ))}

        {/* === MEMORY node (bottom) === */}
        <rect
          x="184"
          y="280"
          width="112"
          height="40"
          rx="9"
          fill="color-mix(in oklab, var(--card) 95%, transparent)"
          stroke="color-mix(in oklab, var(--primary) 65%, transparent)"
          strokeWidth="1.8"
          filter="url(#ag-rough)"
        />
        <text
          x="240"
          y="298"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="11"
          fill="var(--foreground)"
          fontWeight="600"
          letterSpacing="0.4"
        >
          memory
        </text>
        <g transform="translate(202 308)" filter="url(#ag-rough)">
          {/* full */}
          <rect width="24" height="5" rx="2" fill="var(--primary)" />
          {/* partial */}
          <rect x="29" width="24" height="5" rx="2" fill="color-mix(in oklab, var(--border) 60%, transparent)" />
          <rect x="29" width="14" height="5" rx="2" fill="color-mix(in oklab, var(--primary) 55%, var(--card))" />
          {/* empty */}
          <rect x="58" width="24" height="5" rx="2" fill="color-mix(in oklab, var(--border) 50%, transparent)" />
        </g>

        {/* === SCOPE node (left) === */}
        <rect
          x="10"
          y="134"
          width="88"
          height="82"
          rx="9"
          fill="color-mix(in oklab, var(--card) 95%, transparent)"
          stroke="color-mix(in oklab, var(--primary) 65%, transparent)"
          strokeWidth="1.8"
          filter="url(#ag-rough)"
        />
        <text
          x="54"
          y="156"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="11"
          fill="var(--foreground)"
          fontWeight="600"
          letterSpacing="0.4"
        >
          scope
        </text>
        <text
          x="54"
          y="184"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="13"
          fill="var(--primary)"
          fontWeight="700"
          letterSpacing="0.4"
        >
          project
        </text>
        <text
          x="54"
          y="202"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="8"
          fill="var(--muted-foreground)"
          opacity="0.7"
          letterSpacing="0.5"
        >
          (or global)
        </text>

        {/* === CENTER: solana-ops === */}
        <g filter="url(#ag-rough)">
          <ellipse
            cx="240"
            cy="175"
            rx="48"
            ry="32"
            fill="color-mix(in oklab, var(--primary) 16%, var(--card))"
            stroke="var(--primary)"
            strokeWidth="2.2"
          />
          <ellipse
            cx="240"
            cy="175"
            rx="36"
            ry="22"
            fill="none"
            stroke="color-mix(in oklab, var(--primary) 50%, transparent)"
            strokeWidth="1"
            strokeDasharray="3 4"
          />
        </g>
        <text
          x="240"
          y="173"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="13"
          fontWeight="700"
          fill="var(--foreground)"
          letterSpacing="0.4"
        >
          solana-ops
        </text>
        <text
          x="240"
          y="187"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="8"
          fill="var(--primary)"
          letterSpacing="2"
          opacity="0.85"
        >
          AGENT
        </text>

        {/* Hand-drawn annotation pointing at approvals */}
        <g>
          <path
            d="M 376 80 Q 388 100 400 130"
            fill="none"
            stroke="color-mix(in oklab, var(--primary) 55%, transparent)"
            strokeWidth="1.2"
            strokeLinecap="round"
            filter="url(#ag-wob)"
          />
          <path
            d="M 393 124 L 401 134 L 405 122 Z"
            fill="color-mix(in oklab, var(--primary) 65%, transparent)"
          />
          <text
            x="372"
            y="70"
            textAnchor="end"
            fontFamily="var(--font-mono)"
            fontSize="9"
            fill="var(--muted-foreground)"
            letterSpacing="1"
            opacity="0.85"
          >
            stops to ask
          </text>
        </g>
      </svg>
    </div>
  )
}

/* 2) Chain agents — a topographic flow diagram. Multiple curved streams
   converge through a gate, then split toward outcomes. A dashed back-edge
   suggests loops. */
function WorkflowFlowVisual() {
  return (
    <div className="relative aspect-[6/5] w-full">
      <svg viewBox="0 0 480 400" className="h-full w-full" aria-hidden>
        <defs>
          <linearGradient id="wf-stream" x1="0" x2="1">
            <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 25%, transparent)" />
            <stop offset="55%" stopColor="color-mix(in oklab, var(--primary) 75%, transparent)" />
            <stop offset="100%" stopColor="var(--primary)" />
          </linearGradient>
          <linearGradient id="wf-loop" x1="1" x2="0">
            <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 60%, transparent)" />
            <stop offset="100%" stopColor="color-mix(in oklab, var(--primary) 15%, transparent)" />
          </linearGradient>
          <radialGradient id="wf-gate-glow-2" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.42" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* Topographic contour lines (decorative depth) */}
        <g opacity="0.18" fill="none" stroke="var(--border)">
          {[60, 100, 140, 180, 220, 260, 300, 340].map((y, i) => (
            <path
              key={i}
              d={`M -10 ${y} Q 120 ${y - 8} 240 ${y} T 490 ${y}`}
              strokeWidth={i % 2 === 0 ? 0.8 : 0.5}
            />
          ))}
        </g>

        {/* Gate glow */}
        <circle cx="240" cy="200" r="78" fill="url(#wf-gate-glow-2)" />

        {/* Inbound streams (3 sources -> gate) */}
        <path d="M 30 110 C 130 110, 160 200, 220 200" stroke="url(#wf-stream)" strokeWidth="1.6" fill="none" />
        <path d="M 30 200 C 110 200, 150 200, 220 200" stroke="url(#wf-stream)" strokeWidth="1.6" fill="none" />
        <path d="M 30 290 C 130 290, 160 200, 220 200" stroke="url(#wf-stream)" strokeWidth="1.6" fill="none" />

        {/* Outbound streams (gate -> 2 outcomes) */}
        <path d="M 260 200 C 320 200, 350 130, 450 130" stroke="url(#wf-stream)" strokeWidth="1.6" fill="none" />
        <path d="M 260 200 C 320 200, 350 270, 450 270" stroke="url(#wf-stream)" strokeWidth="1.6" fill="none" />

        {/* Loop-back edge (dashed) */}
        <path
          d="M 450 270 C 380 360, 110 360, 30 290"
          stroke="url(#wf-loop)"
          strokeWidth="1.2"
          strokeDasharray="3 4"
          fill="none"
        />

        {/* Source nodes (left) */}
        {[
          { y: 110, label: "ask" },
          { y: 200, label: "engineer" },
          { y: 290, label: "debug" },
        ].map((s, i) => (
          <g key={i}>
            <circle cx="30" cy={s.y} r="14" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 55%, transparent)" />
            <circle cx="30" cy={s.y} r="4" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
            <text
              x="-4"
              y={s.y + 3}
              textAnchor="end"
              fontFamily="var(--font-mono)"
              fontSize="10"
              fill="var(--muted-foreground)"
              letterSpacing="1"
            >
              {s.label}
            </text>
          </g>
        ))}

        {/* Gate (diamond) */}
        <g>
          <polygon
            points="240,168 272,200 240,232 208,200"
            fill="color-mix(in oklab, var(--primary) 16%, var(--card))"
            stroke="color-mix(in oklab, var(--primary) 70%, transparent)"
          />
          <text
            x="240"
            y="204"
            textAnchor="middle"
            fontFamily="var(--font-mono)"
            fontSize="11"
            fontWeight="600"
            fill="var(--primary)"
            letterSpacing="1.5"
          >
            gate
          </text>
        </g>

        {/* Outcome nodes (right) */}
        {[
          { y: 130, label: "ship", lit: true },
          { y: 270, label: "retry", lit: false },
        ].map((s, i) => (
          <g key={i}>
            <circle
              cx="450"
              cy={s.y}
              r="14"
              fill={s.lit ? "color-mix(in oklab, var(--primary) 22%, var(--card))" : "var(--card)"}
              stroke={s.lit ? "var(--primary)" : "color-mix(in oklab, var(--border) 90%, transparent)"}
              strokeWidth={s.lit ? 1.4 : 1}
            />
            {s.lit && <circle cx="450" cy={s.y} r="22" fill="none" stroke="color-mix(in oklab, var(--primary) 45%, transparent)" strokeWidth="0.8" />}
            <circle cx="450" cy={s.y} r="4" fill={s.lit ? "var(--primary)" : "color-mix(in oklab, var(--muted-foreground) 50%, transparent)"} />
            <text
              x="468"
              y={s.y + 3}
              fontFamily="var(--font-mono)"
              fontSize="10"
              fill={s.lit ? "var(--foreground)" : "var(--muted-foreground)"}
              letterSpacing="1"
            >
              {s.label}
            </text>
          </g>
        ))}

        {/* Annotations */}
        <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" opacity="0.7">
          <text x="240" y="155" textAnchor="middle" letterSpacing="1.5">
            decide
          </text>
          <text x="240" y="385" textAnchor="middle" letterSpacing="1.5">
            loop · until done
          </text>
        </g>
      </svg>
    </div>
  )
}

/* 3) Pick up weeks later — geological strata of session time. Horizontal
   bands stack vertically; each band is a day; a break in the middle is the
   idle gap; a glowing node marks where the user resumed. */
function TimeStrataVisual() {
  type Band = {
    y: number
    label: string
    nodes: { x: number; kind: "checkpoint" | "branch" | "resume" }[]
    idle?: boolean
  }
  const bands: Band[] = [
    {
      y: 60,
      label: "Mon",
      nodes: [
        { x: 90, kind: "checkpoint" },
        { x: 200, kind: "checkpoint" },
        { x: 320, kind: "branch" },
      ],
    },
    {
      y: 120,
      label: "Mon · pm",
      nodes: [{ x: 130, kind: "checkpoint" }, { x: 270, kind: "checkpoint" }],
    },
    { y: 180, label: "— idle 17h —", nodes: [], idle: true },
    {
      y: 240,
      label: "Tue",
      nodes: [{ x: 110, kind: "resume" }],
    },
    {
      y: 300,
      label: "Tue · pm",
      nodes: [],
    },
  ]

  return (
    <div className="relative aspect-[6/5] w-full">
      <svg viewBox="0 0 480 400" className="h-full w-full" aria-hidden>
        <defs>
          <linearGradient id="strata-fill" x1="0" x2="1">
            <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 8%, transparent)" />
            <stop offset="100%" stopColor="color-mix(in oklab, var(--primary) 0%, transparent)" />
          </linearGradient>
          <radialGradient id="resume-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.55" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
          </radialGradient>
          <pattern id="strata-stipple" x="0" y="0" width="6" height="6" patternUnits="userSpaceOnUse">
            <circle cx="3" cy="3" r="0.6" fill="color-mix(in oklab, var(--muted-foreground) 35%, transparent)" />
          </pattern>
        </defs>

        {/* Margin label gutter */}
        <line x1="60" y1="30" x2="60" y2="370" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.8" />
        <text x="60" y="22" fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="2">TIMELINE</text>

        {/* Strata bands */}
        {bands.map((b, i) => (
          <g key={i}>
            <rect
              x="60"
              y={b.y - 18}
              width="400"
              height="36"
              fill={b.idle ? "url(#strata-stipple)" : "url(#strata-fill)"}
              opacity={b.idle ? 0.5 : 0.9}
            />
            {/* band edges */}
            <line
              x1="60"
              y1={b.y - 18}
              x2="460"
              y2={b.y - 18}
              stroke="color-mix(in oklab, var(--border) 80%, transparent)"
              strokeWidth="0.7"
              strokeDasharray={b.idle ? "3 3" : undefined}
            />
            {i === bands.length - 1 && (
              <line
                x1="60"
                y1={b.y + 18}
                x2="460"
                y2={b.y + 18}
                stroke="color-mix(in oklab, var(--border) 80%, transparent)"
                strokeWidth="0.7"
              />
            )}
            {/* label */}
            <text
              x="52"
              y={b.y + 3}
              textAnchor="end"
              fontFamily="var(--font-mono)"
              fontSize="10"
              fill={b.idle ? "var(--muted-foreground)" : "var(--foreground)"}
              opacity={b.idle ? 0.7 : 0.85}
              letterSpacing="1"
            >
              {b.label}
            </text>
            {/* nodes within band */}
            {b.nodes.map((n, j) => {
              const x = 60 + n.x
              if (n.kind === "checkpoint") {
                return <circle key={j} cx={x} cy={b.y} r="3" fill="color-mix(in oklab, var(--primary) 70%, transparent)" />
              }
              if (n.kind === "branch") {
                return (
                  <g key={j}>
                    <circle cx={x} cy={b.y} r="3.5" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 80%, transparent)" strokeWidth="1.2" />
                    <path
                      d={`M ${x} ${b.y} C ${x + 12} ${b.y - 4}, ${x + 28} ${b.y + 30}, ${x + 50} ${b.y + 60}`}
                      fill="none"
                      stroke="color-mix(in oklab, var(--primary) 60%, transparent)"
                      strokeWidth="1"
                      strokeDasharray="2 3"
                    />
                  </g>
                )
              }
              // resume
              return (
                <g key={j}>
                  <circle cx={x} cy={b.y} r="22" fill="url(#resume-glow)" />
                  <circle cx={x} cy={b.y} r="6" fill="var(--primary)" />
                  <circle cx={x} cy={b.y} r="11" fill="none" stroke="var(--primary)" strokeWidth="1" opacity="0.55" />
                  <text
                    x={x + 18}
                    y={b.y + 3}
                    fontFamily="var(--font-mono)"
                    fontSize="10"
                    fill="var(--primary)"
                    letterSpacing="1.5"
                    fontWeight="600"
                  >
                    RESUME
                  </text>
                </g>
              )
            })}
          </g>
        ))}

        {/* Vertical "now" line */}
        <line
          x1="170"
          y1="42"
          x2="170"
          y2="318"
          stroke="color-mix(in oklab, var(--primary) 45%, transparent)"
          strokeDasharray="3 4"
          strokeWidth="0.8"
        />
        <text
          x="170"
          y="332"
          textAnchor="middle"
          fontFamily="var(--font-mono)"
          fontSize="9"
          fill="var(--muted-foreground)"
          letterSpacing="2"
        >
          now
        </text>

        {/* Footer caption */}
        <text
          x="60"
          y="378"
          fontFamily="var(--font-mono)"
          fontSize="9"
          fill="var(--muted-foreground)"
          opacity="0.65"
          letterSpacing="1.5"
        >
          ~/projects/acme · local journal · 14 checkpoints · 2 branches
        </text>
      </svg>
    </div>
  )
}
