import {
  Database,
  LayoutGrid,
  Wrench,
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
    tag: "Persistence",
    icon: <Database className="h-3.5 w-3.5" />,
    title: "Pick up any session, days or weeks later.",
    description:
      "Each project keeps its own local journal. Long runs survive compaction, context pressure, branch and rewind. Search a transcript, export a slice, fork a branch, or hand off to a different agent type without losing the thread.",
    bullets: [
      "Reviewed memory · manual or automatic compact",
      "Branch and rewind without losing the original timeline",
      "Same-type handoff between Ask, Engineer, Debug, and your own agents",
    ],
    visual: <PersistenceVisual />,
  },
  {
    tag: "Multi-pane",
    icon: <LayoutGrid className="h-3.5 w-3.5" />,
    title: "Up to six agent sessions in one project.",
    description:
      "Run independent sessions side by side, each with its own agent type and model. Active swarm coordination — file reservations, presence, conflict warnings, and shared swarm notes — is on the way.",
    bullets: [
      "Six panes per project · independent contexts and models",
      "Built-in agent types: Ask, Engineer, Debug, Agent Create",
      "Coming soon: visual Agent Builder and Workflow Builder",
    ],
    visual: <MultiPaneVisual />,
  },
  {
    tag: "Tool control",
    icon: <Wrench className="h-3.5 w-3.5" />,
    title: "Tools that touch the real machine.",
    description:
      "Agents work with your repo, shell, git, browser, mobile emulators, MCP servers, skills, and a Solana workbench. Every action shows up in the run timeline with diffs, file changes, and approval gates where you want them.",
    bullets: [
      "Browser automation · tabs, clicks, console, network, a11y snapshots",
      "Mobile · iOS / Android sidebars with screenshots, UI tree, gestures",
      "Solana workbench · local clusters, tx build / sim / send, IDL / PDA, deploy helpers",
    ],
    visual: <ToolsVisual />,
  }
]

export function Features() {
  return (
    <section id="product" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Hero features
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Built to run agents, not chat with them.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            A real desktop app for sessions that need to keep going — through
            long runs, branch points, and decisions worth pausing for.
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

function PersistenceVisual() {
  const rows = [
    { t: "14:02", msg: "checkpoint · spec parsed, plan accepted", day: "Mon" },
    { t: "14:07", msg: "checkpoint · context auto-compacted (42%)", day: "Mon" },
    { t: "14:19", msg: "branch · forked from `main` to `try-pg`", day: "Mon" },
    { t: "—", msg: "laptop closed · 17h 42m idle", idle: true },
    { t: "08:01", msg: "resume · awaiting approval on src/billing.ts", day: "Tue", active: true },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4">
      <div className="mb-3 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="font-mono">~/Library/Application Support/xero/projects/acme.db</span>
        <span className="font-mono text-primary">local · journal</span>
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

function MultiPaneVisual() {
  const panes = [
    { role: "Engineer", model: "claude-opus-4.7", task: "refactor billing module", state: "running" },
    { role: "Debug", model: "gpt-5", task: "trace failing webhook test", state: "running" },
    { role: "Ask", model: "gemini-2.5-pro", task: "explain provider loop", state: "idle" },
    { role: "Engineer", model: "qwen3:32b · ollama", task: "draft retry helper", state: "running" },
    { role: "Engineer", model: "anthropic via openrouter", task: "wire MCP search tool", state: "decision" },
    { role: "Custom · solana-ops", model: "claude-sonnet-4.6", task: "simulate proposal tx", state: "idle" },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4">
      <div className="mb-3 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="font-mono">project · acme-saas · 6 / 6 panes</span>
        <span className="font-mono text-primary">live</span>
      </div>
      <ul className="grid grid-cols-2 gap-2 text-[11px]">
        {panes.map((p, i) => (
          <li
            key={i}
            className={`flex flex-col gap-1.5 rounded-md border px-2.5 py-2 font-mono ${
              p.state === "decision"
                ? "border-primary/50 bg-primary/[0.06]"
                : "border-border/60 bg-background/40"
            }`}
          >
            <div className="flex items-center justify-between text-foreground">
              <span className="text-[11px] font-medium">{p.role}</span>
              <span
                className={`text-[10px] uppercase tracking-wider ${
                  p.state === "running"
                    ? "text-primary"
                    : p.state === "decision"
                      ? "text-primary"
                      : "text-muted-foreground/70"
                }`}
              >
                {p.state}
              </span>
            </div>
            <div className="truncate text-[10px] text-muted-foreground">{p.model}</div>
            <div className="truncate text-[11px] text-foreground/85">{p.task}</div>
          </li>
        ))}
      </ul>
      <div className="mt-3 rounded-md border border-dashed border-border/70 bg-secondary/20 px-2.5 py-2 font-mono text-[11px] text-muted-foreground">
        <span className="text-primary">soon ·</span> file reservations, presence, swarm notes
      </div>
    </div>
  )
}

function ToolsVisual() {
  const calls = [
    { t: "tool", c: "repo.read(src/billing.ts)", ok: true },
    { t: "tool", c: "repo.edit(src/billing.ts)", ok: true },
    { t: "tool", c: "shell(\"cargo test billing\")", ok: true },
    { t: "tool", c: "git.commit(\"refactor: extract retry helper\")", ok: true },
    { t: "tool", c: "browser.navigate(\"localhost:3000/billing\")", ok: true },
    { t: "tool", c: "mcp.search(\"stripe webhook idempotency\")", ok: true },
    { t: "tool", c: "solana.simulate(tx)", ok: true },
    { t: "ask",  c: "approval · push branch to origin?", pending: true },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4 font-mono text-[12px] leading-relaxed">
      <div className="flex items-center justify-between border-b border-border/60 pb-2 text-[11px] text-muted-foreground">
        <span>session · engineer · run timeline</span>
        <span className="text-primary">● live · 8 events</span>
      </div>
      <div className="mt-3 space-y-1.5">
        {calls.map((l, i) => (
          <div key={i} className="flex items-start gap-2">
            <span className="w-10 shrink-0 text-muted-foreground/60">{l.t}</span>
            <span className="text-foreground/90">
              {l.c}
              {l.ok && <span className="ml-2 text-primary">// ok</span>}
              {l.pending && <span className="ml-2 text-primary">// awaiting you</span>}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
