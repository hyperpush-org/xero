export function FeatureGrid() {
  return (
    <section id="capabilities" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Capabilities
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            What ships in the box.
          </h2>
        </div>

        {/* Bento */}
        <div className="mt-14 grid auto-rows-[minmax(0,1fr)] grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-12">
          <BentoCard
            className="lg:col-span-7 lg:row-span-2"
            title="Agents you actually design"
            caption="Tools, memory, and approval rules, set per agent."
            visual={<AgentBlueprintVisual />}
            visualClassName="min-h-[360px]"
          />

          <BentoCard
            className="lg:col-span-5"
            title="Composable workflows"
            caption="Steps, branches, loops, gates."
            visual={<InfinityFlowVisual />}
          />

          <BentoCard
            className="lg:col-span-5"
            title="Approve from your phone"
            caption="Discord or Telegram, with the actual diff inline."
            visual={<PingWaveVisual />}
          />

          <BentoCard
            className="lg:col-span-4"
            title="Six-pane workspace"
            caption="Mix agents, roles, and models in one project."
            visual={<DotMatrixVisual />}
          />

          <BentoCard
            className="lg:col-span-4"
            title="Branch & rewind"
            caption="Fork a session, roll back to any checkpoint."
            visual={<BranchTreeVisual />}
          />

          <BentoCard
            className="lg:col-span-4"
            title="Run timeline"
            caption="Replay every call, change, and approval."
            visual={<WaveformVisual />}
          />
        </div>
      </div>
    </section>
  )
}

function BentoCard({
  className,
  title,
  caption,
  visual,
  visualClassName,
}: {
  className?: string
  title: string
  caption: string
  visual: React.ReactNode
  visualClassName?: string
}) {
  return (
    <article
      className={`relative flex flex-col overflow-hidden rounded-2xl border border-border/70 bg-card ${
        className ?? ""
      }`}
    >
      <div
        className={`flex flex-1 items-center justify-center bg-gradient-to-b from-transparent to-secondary/[0.08] p-4 sm:p-5 ${
          visualClassName ?? "min-h-[200px]"
        }`}
      >
        {visual}
      </div>
      <footer className="border-t border-border/60 px-5 py-4">
        <h3 className="text-base font-medium tracking-tight">{title}</h3>
        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
          {caption}
        </p>
      </footer>
    </article>
  )
}

/* ---------- Visuals ---------- */

/* Hero card: blueprint of an agent. Annotated technical drawing
   with a central core, three labeled sub-systems wired in, dimension marks. */
function AgentBlueprintVisual() {
  return (
    <div className="relative w-full">
      <svg viewBox="0 0 600 360" className="h-auto w-full" aria-hidden>
        <defs>
          <radialGradient id="bp-core" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.55" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
          </radialGradient>
          <pattern id="bp-grid" x="0" y="0" width="24" height="24" patternUnits="userSpaceOnUse">
            <path d="M 24 0 L 0 0 0 24" fill="none" stroke="color-mix(in oklab, var(--border) 60%, transparent)" strokeWidth="0.4" />
          </pattern>
          <linearGradient id="bp-conn" x1="0" x2="1">
            <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 25%, transparent)" />
            <stop offset="100%" stopColor="var(--primary)" />
          </linearGradient>
        </defs>

        {/* Blueprint grid */}
        <rect width="600" height="360" fill="url(#bp-grid)" opacity="0.7" />

        {/* Core glow */}
        <circle cx="300" cy="180" r="120" fill="url(#bp-core)" />

        {/* Frame border (technical drawing) */}
        <rect x="20" y="20" width="560" height="320" fill="none" stroke="color-mix(in oklab, var(--border) 90%, transparent)" strokeWidth="0.7" />
        <rect x="28" y="28" width="544" height="304" fill="none" stroke="color-mix(in oklab, var(--border) 60%, transparent)" strokeWidth="0.4" strokeDasharray="2 4" />

        {/* Title block */}
        <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="2">
          <rect x="28" y="28" width="170" height="26" fill="color-mix(in oklab, var(--primary) 6%, transparent)" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.5" />
          <text x="38" y="45" fill="var(--primary)">AGENT · solana-ops</text>
          <rect x="402" y="28" width="170" height="26" fill="color-mix(in oklab, var(--primary) 6%, transparent)" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.5" />
          <text x="412" y="45">REV · 0.4 · project</text>
        </g>

        {/* Sub-system: TOOLS (left) */}
        <SubSystem
          x={70}
          y={120}
          w={130}
          h={120}
          label="01 · TOOLS"
          rows={[
            { t: "repo", on: true },
            { t: "shell", on: true },
            { t: "git", on: true },
            { t: "browser", on: true },
            { t: "mcp", on: true },
            { t: "mobile", on: false },
          ]}
        />

        {/* Sub-system: MEMORY (top right) */}
        <SubSystem
          x={400}
          y={70}
          w={130}
          h={88}
          label="02 · MEMORY"
          rows={[
            { t: "session", on: true },
            { t: "cross-run", on: true },
            { t: "global", on: false },
          ]}
        />

        {/* Sub-system: APPROVALS (bottom right) */}
        <SubSystem
          x={400}
          y={200}
          w={130}
          h={88}
          label="03 · APPROVALS"
          rows={[
            { t: "git push", on: false, note: "ASK" },
            { t: "tx send", on: false, note: "ASK" },
            { t: "repo edit", on: true, note: "AUTO" },
          ]}
        />

        {/* Connectors → core */}
        <path d="M 200 180 C 240 180, 260 180, 280 180" stroke="url(#bp-conn)" strokeWidth="1.4" fill="none" />
        <path d="M 400 114 C 360 114, 350 160, 320 170" stroke="url(#bp-conn)" strokeWidth="1.4" fill="none" />
        <path d="M 400 244 C 360 244, 350 200, 320 190" stroke="url(#bp-conn)" strokeWidth="1.4" fill="none" />

        {/* Core (the agent) */}
        <g>
          <circle cx="300" cy="180" r="42" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" />
          <circle cx="300" cy="180" r="26" fill="color-mix(in oklab, var(--primary) 35%, var(--card))" />
          <circle cx="300" cy="180" r="11" fill="var(--primary)" />
          <text x="300" y="244" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="10" fill="var(--primary)" letterSpacing="2" fontWeight="600">CORE</text>
        </g>

        {/* Dimension marks (decorative) */}
        <g stroke="color-mix(in oklab, var(--muted-foreground) 30%, transparent)" strokeWidth="0.5" fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" opacity="0.7">
          <line x1="70" y1="108" x2="200" y2="108" />
          <line x1="70" y1="104" x2="70" y2="112" />
          <line x1="200" y1="104" x2="200" y2="112" />
          <text x="135" y="102" textAnchor="middle" letterSpacing="1.5">tools[6]</text>
          <line x1="400" y1="298" x2="530" y2="298" />
          <line x1="400" y1="294" x2="400" y2="302" />
          <line x1="530" y1="294" x2="530" y2="302" />
          <text x="465" y="292" textAnchor="middle" letterSpacing="1.5">rules[3]</text>
        </g>

        {/* Status footer */}
        <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="1.5">
          <line x1="28" y1="316" x2="572" y2="316" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.5" />
          <text x="38" y="328">SCALE 1:1</text>
          <text x="200" y="328">SCOPE · project</text>
          <text x="332" y="328">VISUAL BUILDER</text>
          <text x="490" y="328" fill="var(--primary)">● LIVE</text>
        </g>
      </svg>
    </div>
  )
}

function SubSystem({
  x,
  y,
  w,
  h,
  label,
  rows,
}: {
  x: number
  y: number
  w: number
  h: number
  label: string
  rows: { t: string; on: boolean; note?: string }[]
}) {
  return (
    <g>
      <rect
        x={x}
        y={y}
        width={w}
        height={h}
        rx="6"
        fill="color-mix(in oklab, var(--card) 90%, transparent)"
        stroke="color-mix(in oklab, var(--border) 90%, transparent)"
        strokeWidth="0.8"
      />
      <line
        x1={x}
        y1={y + 22}
        x2={x + w}
        y2={y + 22}
        stroke="color-mix(in oklab, var(--border) 80%, transparent)"
        strokeWidth="0.5"
      />
      <text
        x={x + 8}
        y={y + 15}
        fontFamily="var(--font-mono)"
        fontSize="8.5"
        fill="var(--primary)"
        letterSpacing="1.8"
      >
        {label}
      </text>
      {rows.map((r, i) => {
        const ry = y + 36 + i * 14
        return (
          <g key={i} fontFamily="var(--font-mono)" fontSize="9">
            <circle
              cx={x + 12}
              cy={ry - 3}
              r="2.5"
              fill={r.on ? "var(--primary)" : "var(--card)"}
              stroke={r.on ? "var(--primary)" : "color-mix(in oklab, var(--border) 90%, transparent)"}
              strokeWidth="0.8"
            />
            <text
              x={x + 22}
              y={ry}
              fill={r.on ? "var(--foreground)" : "var(--muted-foreground)"}
              opacity={r.on ? 0.9 : 0.65}
              letterSpacing="0.5"
            >
              {r.t}
            </text>
            {r.note && (
              <text
                x={x + w - 8}
                y={ry}
                textAnchor="end"
                fill={r.note === "AUTO" ? "var(--muted-foreground)" : "var(--primary)"}
                opacity="0.85"
                letterSpacing="1.5"
                fontSize="8"
              >
                {r.note}
              </text>
            )}
          </g>
        )
      })}
    </g>
  )
}

/* Composable workflows: figure-eight loop with two branching choices */
function InfinityFlowVisual() {
  return (
    <svg viewBox="0 0 280 180" className="h-auto w-full max-w-[320px]" aria-hidden>
      <defs>
        <linearGradient id="if-edge" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 20%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
        <radialGradient id="if-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.42" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Loop glow */}
      <ellipse cx="140" cy="90" rx="100" ry="48" fill="url(#if-glow)" />

      {/* Figure-eight */}
      <path
        d="M 60 90 C 60 50, 110 50, 140 90 C 170 130, 220 130, 220 90 C 220 50, 170 50, 140 90 C 110 130, 60 130, 60 90 Z"
        fill="none"
        stroke="url(#if-edge)"
        strokeWidth="1.6"
      />

      {/* Animated traveling dash */}
      <path
        d="M 60 90 C 60 50, 110 50, 140 90 C 170 130, 220 130, 220 90 C 220 50, 170 50, 140 90 C 110 130, 60 130, 60 90 Z"
        fill="none"
        stroke="color-mix(in oklab, var(--primary) 70%, transparent)"
        strokeWidth="1.4"
        strokeDasharray="3 8"
        className="animate-flow-dash"
      />

      {/* Center gate */}
      <g>
        <rect x="125" y="79" width="30" height="22" rx="4" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="color-mix(in oklab, var(--primary) 65%, transparent)" />
        <text x="140" y="94" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="9" fill="var(--primary)" fontWeight="600" letterSpacing="1.5">if</text>
      </g>

      {/* Step nodes on the loop */}
      {[
        { cx: 60, cy: 90, label: "ask" },
        { cx: 220, cy: 90, label: "ship" },
        { cx: 100, cy: 50, label: "" },
        { cx: 180, cy: 130, label: "" },
        { cx: 100, cy: 130, label: "" },
        { cx: 180, cy: 50, label: "" },
      ].map((n, i) => (
        <g key={i}>
          <circle cx={n.cx} cy={n.cy} r="6" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" strokeWidth="1.2" />
          <circle cx={n.cx} cy={n.cy} r="2" fill="var(--primary)" />
          {n.label && (
            <text
              x={n.cx}
              y={n.cy - 14}
              textAnchor="middle"
              fontFamily="var(--font-mono)"
              fontSize="9"
              fill="var(--muted-foreground)"
              letterSpacing="1.5"
            >
              {n.label}
            </text>
          )}
        </g>
      ))}

      {/* Caption */}
      <text x="140" y="170" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8.5" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        steps · branches · gates · loops
      </text>
    </svg>
  )
}

/* Approve from your phone: radiating signal/ripple from a phone-shaped glyph */
function PingWaveVisual() {
  return (
    <svg viewBox="0 0 280 200" className="h-auto w-full max-w-[280px]" aria-hidden>
      <defs>
        <radialGradient id="pw-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.4" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Glow */}
      <circle cx="140" cy="100" r="90" fill="url(#pw-glow)" />

      {/* Ripple rings */}
      {[35, 55, 75, 95].map((r, i) => (
        <circle
          key={i}
          cx="140"
          cy="100"
          r={r}
          fill="none"
          stroke="color-mix(in oklab, var(--primary) 55%, transparent)"
          strokeWidth={i === 0 ? 1.4 : 0.9}
          strokeDasharray={i === 3 ? "2 4" : undefined}
          opacity={1 - i * 0.18}
        />
      ))}

      {/* Phone glyph (abstract rounded rect with notch) */}
      <g>
        <rect
          x="120"
          y="74"
          width="40"
          height="62"
          rx="7"
          fill="color-mix(in oklab, var(--primary) 18%, var(--card))"
          stroke="var(--primary)"
          strokeWidth="1.2"
        />
        <line x1="132" y1="80" x2="148" y2="80" stroke="color-mix(in oklab, var(--primary) 60%, transparent)" strokeWidth="1.2" strokeLinecap="round" />
        {/* Inner check icon */}
        <path
          d="M 132 108 L 138 114 L 150 100"
          fill="none"
          stroke="var(--primary)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </g>

      {/* Approve / Skip glyphs as small badges to the sides */}
      <g fontFamily="var(--font-mono)" fontSize="8.5" letterSpacing="1.5">
        <g transform="translate(56 96)">
          <rect width="36" height="14" rx="3" fill="color-mix(in oklab, var(--primary) 15%, var(--card))" stroke="color-mix(in oklab, var(--primary) 70%, transparent)" />
          <text x="18" y="10" textAnchor="middle" fill="var(--primary)" fontWeight="600">YES</text>
        </g>
        <g transform="translate(188 96)">
          <rect width="36" height="14" rx="3" fill="var(--card)" stroke="color-mix(in oklab, var(--border) 90%, transparent)" />
          <text x="18" y="10" textAnchor="middle" fill="var(--muted-foreground)">SKIP</text>
        </g>
      </g>

      {/* Caption */}
      <text x="140" y="186" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8.5" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        notification · diff · one tap
      </text>
    </svg>
  )
}

/* Six-pane workspace: heatmap-style dot matrix */
function DotMatrixVisual() {
  // 8 cols × 5 rows of dots; six are "live" (gold), rest are dim
  const cols = 8
  const rows = 5
  const live = new Set(["1,1", "3,0", "4,2", "6,1", "2,3", "5,4"])
  const decision = "4,2"
  const dots: { x: number; y: number; key: string }[] = []
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      dots.push({ x: 30 + c * 24, y: 30 + r * 24, key: `${c},${r}` })
    }
  }
  return (
    <svg viewBox="0 0 240 160" className="h-auto w-full max-w-[280px]" aria-hidden>
      <defs>
        <radialGradient id="dm-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.42" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {dots.map((d) => {
        const isLive = live.has(d.key)
        const isDecision = d.key === decision
        if (isDecision) {
          return (
            <g key={d.key}>
              <circle cx={d.x} cy={d.y} r="11" fill="url(#dm-glow)" />
              <circle cx={d.x} cy={d.y} r="6" fill="var(--primary)" />
              <circle cx={d.x} cy={d.y} r="9" fill="none" stroke="var(--primary)" strokeWidth="1" opacity="0.6" />
            </g>
          )
        }
        if (isLive) {
          return <circle key={d.key} cx={d.x} cy={d.y} r="3.6" fill="var(--primary)" />
        }
        return <circle key={d.key} cx={d.x} cy={d.y} r="2" fill="color-mix(in oklab, var(--muted-foreground) 30%, transparent)" />
      })}

      {/* Caption */}
      <text x="120" y="148" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8.5" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        6 panes · running, idle, deciding
      </text>
    </svg>
  )
}

/* Branch & rewind: branching tree with a circular rewind arrow */
function BranchTreeVisual() {
  return (
    <svg viewBox="0 0 280 180" className="h-auto w-full max-w-[300px]" aria-hidden>
      <defs>
        <linearGradient id="bt-trunk" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--border) 50%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
        <linearGradient id="bt-fork" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 35%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
      </defs>

      {/* main trunk */}
      <line x1="20" y1="90" x2="260" y2="90" stroke="url(#bt-trunk)" strokeWidth="1.6" />
      {/* fork up */}
      <path d="M 110 90 C 130 90, 130 50, 150 50 L 240 50" stroke="url(#bt-fork)" strokeWidth="1.4" fill="none" />
      {/* fork down */}
      <path d="M 150 90 C 170 90, 170 130, 190 130 L 240 130" stroke="url(#bt-fork)" strokeWidth="1.2" fill="none" strokeDasharray="2 3" />

      {/* commit dots on trunk */}
      {[20, 50, 80, 110, 150, 180, 210, 240, 260].map((x, i) => {
        const isHead = i === 8
        return (
          <circle
            key={i}
            cx={x}
            cy={90}
            r={isHead ? 5 : 3.2}
            fill={isHead ? "var(--primary)" : "var(--card)"}
            stroke={isHead ? "var(--primary)" : "color-mix(in oklab, var(--border) 90%, transparent)"}
            strokeWidth="1.2"
          />
        )
      })}
      {/* fork dots up */}
      {[180, 210, 240].map((x, i) => (
        <circle
          key={`u-${i}`}
          cx={x}
          cy={50}
          r="3.2"
          fill="color-mix(in oklab, var(--primary) 25%, var(--card))"
          stroke="color-mix(in oklab, var(--primary) 80%, transparent)"
          strokeWidth="1.2"
        />
      ))}
      {/* fork dots down (lighter / pending) */}
      {[210, 240].map((x, i) => (
        <circle
          key={`d-${i}`}
          cx={x}
          cy={130}
          r="2.6"
          fill="var(--card)"
          stroke="color-mix(in oklab, var(--muted-foreground) 50%, transparent)"
          strokeWidth="1"
          strokeDasharray="2 2"
        />
      ))}

      {/* HEAD ring */}
      <circle cx="260" cy="90" r="9" fill="none" stroke="var(--primary)" strokeWidth="1" opacity="0.5" />

      {/* Rewind glyph (circular arrow) at an earlier commit */}
      <g transform="translate(80 90)">
        <circle r="13" fill="none" stroke="var(--primary)" strokeWidth="1.2" strokeDasharray="3 3" />
        <path
          d="M -7 -4 A 8 8 0 1 0 0 -8"
          fill="none"
          stroke="var(--primary)"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
        <polygon points="-7,-7 -3,-2 -10,-1" fill="var(--primary)" />
      </g>

      {/* Labels */}
      <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="1.5">
        <text x="20" y="80" opacity="0.7">main</text>
        <text x="200" y="42" fill="var(--primary)">try-pg</text>
        <text x="200" y="148" opacity="0.6">retry</text>
        <text x="80" y="120" textAnchor="middle" fill="var(--primary)" fontWeight="600">rewind</text>
      </g>
    </svg>
  )
}

/* Run timeline: seismograph waveform with event markers */
function WaveformVisual() {
  // Generate a deterministic-looking waveform
  const points: string[] = []
  const samples = 80
  const w = 260
  const h = 50
  const baseY = 90
  for (let i = 0; i <= samples; i++) {
    const x = 10 + (i / samples) * w
    const t = i / samples
    const env = Math.sin(t * Math.PI) * 0.9 + 0.1
    const y =
      baseY -
      env *
        h *
        0.5 *
        (Math.sin(t * 22) * 0.6 +
          Math.sin(t * 8 + 1.7) * 0.3 +
          Math.sin(t * 36 + 0.3) * 0.15)
    points.push(`${x},${y}`)
  }

  // Event markers (vertical ticks)
  const events = [
    { x: 38, kind: "done" as const },
    { x: 92, kind: "done" as const },
    { x: 142, kind: "done" as const },
    { x: 196, kind: "running" as const },
    { x: 246, kind: "ask" as const },
  ]

  return (
    <svg viewBox="0 0 280 160" className="h-auto w-full max-w-[300px]" aria-hidden>
      <defs>
        <linearGradient id="wv-line" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 25%, transparent)" />
          <stop offset="100%" stopColor="var(--primary)" />
        </linearGradient>
      </defs>

      {/* Baseline */}
      <line x1="10" y1="90" x2="270" y2="90" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.6" strokeDasharray="2 3" />

      {/* Waveform */}
      <polyline
        points={points.join(" ")}
        fill="none"
        stroke="url(#wv-line)"
        strokeWidth="1.4"
        strokeLinejoin="round"
        strokeLinecap="round"
      />

      {/* Event ticks */}
      {events.map((e, i) => {
        const isAsk = e.kind === "ask"
        const isRunning = e.kind === "running"
        return (
          <g key={i}>
            <line
              x1={e.x}
              y1={50}
              x2={e.x}
              y2={130}
              stroke={isAsk ? "var(--primary)" : "color-mix(in oklab, var(--primary) 50%, transparent)"}
              strokeWidth={isAsk ? 1.2 : 0.8}
              strokeDasharray={isAsk ? undefined : "2 2"}
            />
            <circle
              cx={e.x}
              cy={130}
              r={isAsk ? 5 : 3}
              fill={isAsk || isRunning ? "var(--primary)" : "color-mix(in oklab, var(--primary) 65%, transparent)"}
              stroke={isAsk ? "var(--primary)" : undefined}
            />
            {isAsk && (
              <circle cx={e.x} cy={130} r="9" fill="none" stroke="var(--primary)" strokeWidth="0.8" opacity="0.55" />
            )}
          </g>
        )
      })}

      {/* Tick labels */}
      <g fontFamily="var(--font-mono)" fontSize="8" fill="var(--muted-foreground)" letterSpacing="1">
        <text x="38" y="146" textAnchor="middle">edit</text>
        <text x="92" y="146" textAnchor="middle">test</text>
        <text x="142" y="146" textAnchor="middle">commit</text>
        <text x="196" y="146" textAnchor="middle" fill="var(--primary)" opacity="0.85">browser</text>
        <text x="246" y="146" textAnchor="middle" fill="var(--primary)" fontWeight="600">ask</text>
      </g>
    </svg>
  )
}

/* MCP / integrations: radial hub with spokes to symbolic nodes */
function HubSpokeVisual() {
  const cx = 140
  const cy = 100
  const r = 68
  const items = [
    { deg: 0, label: "shell", glyph: ">_" },
    { deg: 60, label: "browser", glyph: "◯" },
    { deg: 120, label: "mobile", glyph: "▭" },
    { deg: 180, label: "skills", glyph: "◇" },
    { deg: 240, label: "solana", glyph: "◈" },
    { deg: 300, label: "mcp", glyph: "✸" },
  ]
  const pt = (deg: number, rr: number) => {
    const a = ((deg - 90) * Math.PI) / 180
    return { x: cx + rr * Math.cos(a), y: cy + rr * Math.sin(a) }
  }
  return (
    <svg viewBox="0 0 280 200" className="h-auto w-full max-w-[300px]" aria-hidden>
      <defs>
        <radialGradient id="hs-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.45" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Ring */}
      <circle cx={cx} cy={cy} r={r} fill="none" stroke="color-mix(in oklab, var(--border) 90%, transparent)" strokeWidth="0.7" strokeDasharray="2 4" />
      <circle cx={cx} cy={cy} r="42" fill="url(#hs-glow)" />

      {/* Spokes */}
      {items.map((it, i) => {
        const p = pt(it.deg, r)
        return (
          <line
            key={i}
            x1={cx}
            y1={cy}
            x2={p.x}
            y2={p.y}
            stroke="color-mix(in oklab, var(--primary) 50%, transparent)"
            strokeWidth="1"
          />
        )
      })}

      {/* Outer nodes */}
      {items.map((it, i) => {
        const p = pt(it.deg, r)
        return (
          <g key={i}>
            <circle cx={p.x} cy={p.y} r="13" fill="var(--card)" stroke="color-mix(in oklab, var(--primary) 60%, transparent)" />
            <text
              x={p.x}
              y={p.y + 3}
              textAnchor="middle"
              fontFamily="var(--font-mono)"
              fontSize="9"
              fill="var(--primary)"
              fontWeight="600"
            >
              {it.glyph}
            </text>
            <text
              x={p.x}
              y={p.y + 26}
              textAnchor="middle"
              fontFamily="var(--font-mono)"
              fontSize="8.5"
              fill="var(--muted-foreground)"
              letterSpacing="1.5"
            >
              {it.label}
            </text>
          </g>
        )
      })}

      {/* Hub */}
      <g>
        <circle cx={cx} cy={cy} r="22" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="var(--primary)" strokeWidth="1" />
        <circle cx={cx} cy={cy} r="9" fill="var(--primary)" />
      </g>

      {/* Caption */}
      <text x={cx} y="190" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8.5" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        one agent · many tool surfaces
      </text>
    </svg>
  )
}

/* Keys local: central key glyph with light beams to provider color swatches */
function KeyRayVisual() {
  const cx = 140
  const cy = 100
  // Provider color rings around the key
  const providers = [
    "#10a37f", // openai
    "#cc785c", // anthropic
    "#4285f4", // gemini
    "#0078d4", // azure
    "#ff9900", // bedrock
    "#1a73e8", // vertex
    "#f8f9fa", // ollama / local
    "#d4a574", // generic
  ]
  const pt = (deg: number, r: number) => {
    const a = ((deg - 90) * Math.PI) / 180
    return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) }
  }
  return (
    <svg viewBox="0 0 280 200" className="h-auto w-full max-w-[320px]" aria-hidden>
      <defs>
        <radialGradient id="kr-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.5" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
        <linearGradient id="kr-beam" x1="0" x2="1">
          <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 60%, transparent)" />
          <stop offset="100%" stopColor="color-mix(in oklab, var(--primary) 0%, transparent)" />
        </linearGradient>
      </defs>

      {/* Glow */}
      <circle cx={cx} cy={cy} r="86" fill="url(#kr-glow)" />

      {/* Beams */}
      {providers.map((_, i) => {
        const deg = (360 / providers.length) * i + 22.5
        const p = pt(deg, 80)
        return (
          <line
            key={i}
            x1={cx}
            y1={cy}
            x2={p.x}
            y2={p.y}
            stroke="url(#kr-beam)"
            strokeWidth="1"
          />
        )
      })}

      {/* Provider color dots */}
      {providers.map((c, i) => {
        const deg = (360 / providers.length) * i + 22.5
        const p = pt(deg, 80)
        return (
          <g key={i}>
            <circle cx={p.x} cy={p.y} r="7" fill="var(--card)" stroke={c} strokeWidth="1.4" />
            <circle cx={p.x} cy={p.y} r="3" fill={c} />
          </g>
        )
      })}

      {/* Outer dashed boundary */}
      <circle cx={cx} cy={cy} r="80" fill="none" stroke="color-mix(in oklab, var(--border) 70%, transparent)" strokeWidth="0.6" strokeDasharray="2 5" />

      {/* Key glyph */}
      <g transform={`translate(${cx} ${cy})`}>
        <circle r="20" fill="color-mix(in oklab, var(--primary) 20%, var(--card))" stroke="var(--primary)" strokeWidth="1.4" />
        <circle r="7" fill="var(--card)" stroke="var(--primary)" strokeWidth="1.2" />
        <rect x="14" y="-3" width="22" height="6" rx="1" fill="var(--primary)" />
        <rect x="32" y="-1" width="2" height="6" fill="var(--card)" />
        <rect x="28" y="-1" width="2" height="6" fill="var(--card)" />
      </g>

      {/* Caption */}
      <text x={cx} y="190" textAnchor="middle" fontFamily="var(--font-mono)" fontSize="8.5" fill="var(--muted-foreground)" opacity="0.7" letterSpacing="2">
        keys local · 10 providers · no relay
      </text>
    </svg>
  )
}
