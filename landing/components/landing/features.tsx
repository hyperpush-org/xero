import {
  Cpu,
  Database,
  Workflow as WorkflowIcon,
} from "lucide-react"

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
    tag: "Rust core",
    icon: <Cpu className="h-3.5 w-3.5" />,
    title: "A harness built from the ground up in Rust.",
    description:
      "Agent loop, tool executor, sandbox, and persistence are a single Rust binary — not a web app pretending to be a desktop app. Sub-millisecond tool-call overhead and ~42MB idle memory.",
    bullets: [
      "Single static binary · no Electron, no Node runtime",
      "Tokio work-stealing scheduler · dozens of parallel tool calls",
      "Deterministic replays for every run",
    ],
    visual: <HarnessVisual />,
  },
  {
    tag: "Persistence",
    icon: <Database className="h-3.5 w-3.5" />,
    title: "Pick up any run, days or weeks later.",
    description:
      "Every plan, diff, tool call, and decision is journaled to a local SQLite database. Close your laptop mid-build, reopen tomorrow, and Xero resumes from the exact step it left on.",
    bullets: [
      "Append-only journal per project",
      "Branch and fork runs to explore alternatives",
      "Nothing ever leaves your machine by default",
    ],
    visual: <PersistenceVisual />,
  },
  {
    tag: "Agentic workflow",
    icon: <WorkflowIcon className="h-3.5 w-3.5" />,
    title: "Planner, workers, critic — each with a job.",
    description:
      "A planner decomposes your brief into milestones. Workers execute in parallel on your machine. A critic reads every diff, runs the test suite, and sends work back when it isn't good enough.",
    bullets: [
      "Milestone tree you can edit before code is written",
      "Dozens of workers in parallel via Tokio",
      "A critic agent that actually fails PRs",
    ],
    visual: <WorkflowVisual />,
  }
]

export function Features() {
  return (
    <section id="product" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            What&apos;s inside
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            A real desktop app. Not a browser tab with ambitions.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Every layer of Xero — the agent loop, the tool executor, persistence,
            sandboxing — is written in Rust for speed, predictability, and a memory
            footprint your laptop won&apos;t notice.
          </p>
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
        <div className="group relative">
          <div
            aria-hidden
            className="pointer-events-none absolute -inset-6 -z-10 rounded-[2rem] bg-gradient-to-br from-primary/10 via-transparent to-transparent blur-2xl transition-opacity duration-500 group-hover:opacity-80"
          />
          <div className="overflow-hidden rounded-2xl border border-border/70 bg-card p-4 shadow-[0_30px_80px_-30px_rgba(0,0,0,0.5)] transition-colors group-hover:border-border">
            {row.visual}
          </div>
        </div>
      </div>
    </div>
  )
}

/* -------- Visuals -------- */

function HarnessVisual() {
  const lines = [
    { t: "spawn", c: "planner::decompose(&brief).await?;" },
    { t: "spawn", c: "worker::scaffold(Framework::NextJs).await?;" },
    { t: "await", c: "let diff = worker::implement(&task).await?;" },
    { t: "check", c: "critic::review(&diff).await?;", ok: true },
    { t: "spawn", c: "sandbox::run(\"pnpm test\", 30s).await?;" },
    { t: "emit", c: "notify::discord(&decision).await?;", ok: true },
    { t: "await", c: "journal::checkpoint(&state).await?;" },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4 font-mono text-[12px] leading-relaxed">
      <div className="flex items-center justify-between border-b border-border/60 pb-2 text-[11px] text-muted-foreground">
        <span>src/harness.rs</span>
        <span className="text-primary">● running · 14 tasks</span>
      </div>
      <div className="mt-3 space-y-1.5">
        {lines.map((l, i) => (
          <div key={i} className="flex items-start gap-2">
            <span className="w-10 shrink-0 text-muted-foreground/60">{l.t}</span>
            <span className="text-foreground/90">
              {l.c}
              {l.ok && <span className="ml-2 text-primary">// ok</span>}
            </span>
          </div>
        ))}
        <div className="flex items-center gap-2 pt-1">
          <span className="w-10 shrink-0 text-muted-foreground/60">next</span>
          <span className="h-3 w-2 animate-pulse bg-primary" />
        </div>
      </div>
    </div>
  )
}

function PersistenceVisual() {
  const rows = [
    { t: "14:02", msg: "checkpoint: migrations applied", day: "Mon" },
    { t: "14:07", msg: "checkpoint: auth flow complete", day: "Mon" },
    { t: "14:19", msg: "checkpoint: billing in progress", day: "Mon" },
    { t: "—", msg: "laptop closed · 17h 42m idle", idle: true },
    { t: "08:01", msg: "resume · billing → critic review", day: "Tue", active: true },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4">
      <div className="mb-3 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="font-mono">~/.Xero/runs/acme-saas.db</span>
        <span className="font-mono text-primary">SQLite · journal</span>
      </div>
      <ul className="space-y-1.5 text-[12px]">
        {rows.map((r, i) => (
          <li
            key={i}
            className={`flex items-center gap-2 rounded-md border px-2.5 py-2 font-mono ${
              r.active
                ? "border-primary/40 bg-primary/[0.06] text-foreground"
                : r.idle
                  ? "border-dashed border-border/60 bg-transparent text-muted-foreground/70"
                  : "border-border/60 bg-background/40 text-muted-foreground"
            }`}
          >
            <span className="w-10 text-[11px] opacity-70">{r.t}</span>
            {r.day && (
              <span className="rounded bg-secondary/60 px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-muted-foreground">
                {r.day}
              </span>
            )}
            <span className="truncate">{r.msg}</span>
            {r.active && (
              <span className="ml-auto h-1.5 w-1.5 animate-pulse-dot rounded-full bg-primary" />
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}

function WorkflowVisual() {
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-6">
      <div className="mb-4 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="font-mono">agent-graph · live</span>
        <span className="font-mono text-primary">18 active tasks</span>
      </div>
      <svg viewBox="0 0 480 260" className="h-auto w-full">
        <defs>
          <marker
            id="arr2"
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto"
          >
            <path d="M0,0 L10,5 L0,10 z" fill="currentColor" className="text-border" />
          </marker>
        </defs>

        {/* edges */}
        <g stroke="currentColor" className="text-border" strokeWidth="1" fill="none">
          <path d="M240,52 C180,85 130,110 115,128" markerEnd="url(#arr2)" />
          <path d="M240,52 C215,85 200,110 195,128" markerEnd="url(#arr2)" />
          <path d="M240,52 L240,128" markerEnd="url(#arr2)" />
          <path d="M240,52 C265,85 280,110 285,128" markerEnd="url(#arr2)" />
          <path d="M240,52 C300,85 350,110 365,128" markerEnd="url(#arr2)" />
          <path d="M115,152 C145,185 200,200 212,208" markerEnd="url(#arr2)" />
          <path d="M195,152 C205,180 225,200 230,208" markerEnd="url(#arr2)" />
          <path d="M240,152 L240,208" markerEnd="url(#arr2)" />
          <path d="M285,152 C275,180 255,200 250,208" markerEnd="url(#arr2)" />
          <path d="M365,152 C335,185 280,200 268,208" markerEnd="url(#arr2)" />
        </g>

        {/* planner */}
        <g>
          <rect x="196" y="24" width="88" height="28" rx="8" className="fill-primary/10 stroke-primary/50" strokeWidth="1" />
          <text x="240" y="42" textAnchor="middle" className="fill-primary font-mono text-[11px]">Planner</text>
        </g>

        {/* workers row */}
        {[
          { x: 115, label: "worker·a" },
          { x: 195, label: "worker·b" },
          { x: 240, label: "worker·c" },
          { x: 285, label: "worker·d" },
          { x: 365, label: "worker·e" },
        ].map((n) => (
          <g key={n.label}>
            <rect x={n.x - 36} y={128} width="72" height="24" rx="6" className="fill-secondary stroke-border" strokeWidth="1" />
            <text x={n.x} y={144} textAnchor="middle" className="fill-foreground font-mono text-[10px]">{n.label}</text>
          </g>
        ))}

        {/* critic */}
        <g>
          <rect x="196" y="208" width="88" height="28" rx="8" className="fill-primary/10 stroke-primary/50" strokeWidth="1" />
          <text x="240" y="226" textAnchor="middle" className="fill-primary font-mono text-[11px]">Critic</text>
        </g>
      </svg>
      <div className="mt-4 grid grid-cols-3 gap-2 font-mono text-[11px]">
        <div className="rounded-md border border-border/60 bg-secondary/30 px-2.5 py-1.5 text-muted-foreground">
          <span className="text-primary">●</span> 1 planner
        </div>
        <div className="rounded-md border border-border/60 bg-secondary/30 px-2.5 py-1.5 text-muted-foreground">
          <span className="text-primary">●</span> 5 workers
        </div>
        <div className="rounded-md border border-border/60 bg-secondary/30 px-2.5 py-1.5 text-muted-foreground">
          <span className="text-primary">●</span> 1 critic
        </div>
      </div>
    </div>
  )
}

