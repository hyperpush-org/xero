import {
  Bot,
  Bug,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  Code2,
  Database,
  GitBranch,
  GitFork,
  Globe,
  HelpCircle,
  PauseCircle,
  Sparkles,
  Terminal,
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
    visual: <MultiPaneVisual />,
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
    visual: <ToolsVisual />,
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
    visual: <PersistenceVisual />,
  }
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
  type Row =
    | { t: string; msg: string; day: string; icon: React.ReactNode; active?: boolean }
    | { t: string; msg: string; idle: true }
  const rows: Row[] = [
    {
      t: "14:02",
      msg: "checkpoint · spec parsed, plan accepted",
      day: "Mon",
      icon: <CheckCircle2 className="h-3 w-3" />,
    },
    {
      t: "14:07",
      msg: "checkpoint · context auto-compacted (42%)",
      day: "Mon",
      icon: <Sparkles className="h-3 w-3" />,
    },
    {
      t: "14:19",
      msg: "branch · forked main → try-pg",
      day: "Mon",
      icon: <GitFork className="h-3 w-3" />,
    },
    {
      t: "—",
      msg: "laptop closed · 17h 42m idle",
      idle: true,
    },
    {
      t: "08:01",
      msg: "resume · awaiting approval on src/billing.ts",
      day: "Tue",
      icon: <PauseCircle className="h-3 w-3" />,
      active: true,
    },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4">
      <div className="mb-3 flex items-center justify-between text-[11px]">
        <span className="font-mono text-muted-foreground/80">
          ~/Library/Application Support/xero/projects/acme.db
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/[0.08] px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-primary">
          <Database className="h-2.5 w-2.5" />
          local journal
        </span>
      </div>
      <ol className="relative space-y-1.5 pl-5 text-[12px]">
        <span
          aria-hidden
          className="pointer-events-none absolute left-[7px] top-1.5 bottom-1.5 w-px bg-gradient-to-b from-border/60 via-border/40 to-border/60"
        />
        {rows.map((r, i) => {
          const isIdle = "idle" in r
          const isActive = !isIdle && r.active
          return (
            <li key={i} className="relative">
              <span
                aria-hidden
                className={`absolute -left-[14px] top-2.5 inline-flex h-2 w-2 items-center justify-center rounded-full ring-4 ring-background ${
                  isActive
                    ? "bg-primary"
                    : isIdle
                      ? "bg-muted-foreground/30"
                      : "bg-primary/40"
                }`}
              >
                {isActive && (
                  <span className="absolute inline-flex h-2 w-2 animate-ring-ping rounded-full bg-primary" />
                )}
              </span>
              <div
                className={`flex items-center gap-2 rounded-md border px-2.5 py-2 font-mono transition-colors ${
                  isActive
                    ? "border-primary/40 bg-primary/[0.06] text-foreground shadow-[0_0_0_1px_color-mix(in_oklab,var(--primary)_15%,transparent)]"
                    : isIdle
                      ? "border-dashed border-border/60 bg-transparent text-muted-foreground/70"
                      : "border-border/60 bg-background/40 text-muted-foreground"
                }`}
              >
                <span className="w-10 shrink-0 text-[11px] tabular-nums opacity-70">{r.t}</span>
                {!isIdle && (
                  <span className="inline-flex items-center gap-1 rounded bg-secondary/60 px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-muted-foreground">
                    {r.day}
                  </span>
                )}
                {!isIdle && (
                  <span
                    className={`shrink-0 ${
                      isActive ? "text-primary" : "text-primary/60"
                    }`}
                  >
                    {r.icon}
                  </span>
                )}
                <span className="truncate">{r.msg}</span>
                {isActive && (
                  <span className="ml-auto inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-primary">
                    <span className="h-1 w-1 animate-pulse-dot rounded-full bg-primary" />
                    you
                  </span>
                )}
              </div>
            </li>
          )
        })}
      </ol>
    </div>
  )
}

function MultiPaneVisual() {
  type Pane = {
    role: string
    model: string
    task: string
    state: "running" | "idle" | "decision"
    icon: React.ReactNode
  }
  const panes: Pane[] = [
    {
      role: "Engineer",
      model: "claude-opus-4.7",
      task: "refactor billing module",
      state: "running",
      icon: <Code2 className="h-3 w-3" />,
    },
    {
      role: "Debug",
      model: "gpt-5",
      task: "trace failing webhook test",
      state: "running",
      icon: <Bug className="h-3 w-3" />,
    },
    {
      role: "Ask",
      model: "gemini-2.5-pro",
      task: "explain provider loop",
      state: "idle",
      icon: <HelpCircle className="h-3 w-3" />,
    },
    {
      role: "Engineer",
      model: "qwen3:32b · ollama",
      task: "draft retry helper",
      state: "running",
      icon: <Code2 className="h-3 w-3" />,
    },
    {
      role: "Engineer",
      model: "anthropic via openrouter",
      task: "wire MCP search tool",
      state: "decision",
      icon: <Code2 className="h-3 w-3" />,
    },
    {
      role: "solana-ops",
      model: "claude-sonnet-4.6",
      task: "simulate proposal tx",
      state: "idle",
      icon: <Sparkles className="h-3 w-3" />,
    },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4">
      <div className="mb-3 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="font-mono">
          project · <span className="text-foreground/80">acme-saas</span>{" "}
          <span className="text-muted-foreground/60">· 6 / 6 panes</span>
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/[0.08] px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-primary">
          <span className="relative inline-flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full animate-ring-ping rounded-full bg-primary" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
          </span>
          live
        </span>
      </div>
      <ul className="grid grid-cols-2 gap-2 text-[11px]">
        {panes.map((p, i) => {
          const isDecision = p.state === "decision"
          const isRunning = p.state === "running"
          return (
            <li
              key={i}
              className={`relative flex flex-col gap-1.5 overflow-hidden rounded-md border px-2.5 py-2 font-mono transition-colors ${
                isDecision
                  ? "border-primary/50 bg-primary/[0.06] shadow-[0_0_0_1px_color-mix(in_oklab,var(--primary)_18%,transparent)]"
                  : "border-border/60 bg-background/40"
              }`}
            >
              {isRunning && (
                <span
                  aria-hidden
                  className="pointer-events-none absolute inset-x-0 top-0 h-px overflow-hidden"
                >
                  <span className="block h-full w-1/3 animate-shimmer-bar bg-gradient-to-r from-transparent via-primary/70 to-transparent" />
                </span>
              )}
              <div className="flex items-center justify-between text-foreground">
                <span className="flex items-center gap-1.5 text-[11px] font-medium">
                  <span
                    className={`inline-flex h-4 w-4 items-center justify-center rounded ${
                      isRunning || isDecision
                        ? "bg-primary/15 text-primary"
                        : "bg-secondary/60 text-muted-foreground/80"
                    }`}
                  >
                    {p.icon}
                  </span>
                  {p.role}
                </span>
                <StatePill state={p.state} />
              </div>
              <div className="truncate text-[10px] text-muted-foreground">{p.model}</div>
              <div className="truncate text-[11px] text-foreground/85">{p.task}</div>
            </li>
          )
        })}
      </ul>
      <div className="mt-3 flex items-center gap-2 rounded-md border border-border/60 bg-secondary/20 px-2.5 py-2 font-mono text-[11px] text-muted-foreground">
        <span className="inline-flex h-4 w-4 items-center justify-center rounded bg-primary/15 text-primary">
          <Sparkles className="h-2.5 w-2.5" />
        </span>
        <span className="text-foreground/80">swarm</span>
        <span className="text-muted-foreground/50">·</span>
        <span>file reservations · presence · shared notes</span>
      </div>
    </div>
  )
}

function StatePill({ state }: { state: "running" | "idle" | "decision" }) {
  if (state === "running") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-primary">
        <span className="h-1 w-1 animate-pulse-dot rounded-full bg-primary" />
        running
      </span>
    )
  }
  if (state === "decision") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-primary/40 bg-primary/15 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-primary">
        <CircleDot className="h-2.5 w-2.5" />
        decide
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border/60 bg-secondary/40 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-muted-foreground/70">
      idle
    </span>
  )
}

function ToolsVisual() {
  type Call =
    | { kind: "tool"; icon: React.ReactNode; ns: string; call: string; ok: true; pending?: never }
    | { kind: "ask"; icon: React.ReactNode; ns: string; call: string; pending: true; ok?: never }
  const calls: Call[] = [
    { kind: "tool", icon: <Code2 className="h-3 w-3" />, ns: "repo", call: 'read("src/billing.ts")', ok: true },
    { kind: "tool", icon: <Code2 className="h-3 w-3" />, ns: "repo", call: 'edit("src/billing.ts")', ok: true },
    { kind: "tool", icon: <Terminal className="h-3 w-3" />, ns: "shell", call: '"cargo test billing"', ok: true },
    { kind: "tool", icon: <GitBranch className="h-3 w-3" />, ns: "git", call: 'commit("refactor: retry helper")', ok: true },
    { kind: "tool", icon: <Globe className="h-3 w-3" />, ns: "browser", call: 'navigate("localhost:3000/billing")', ok: true },
    { kind: "tool", icon: <Sparkles className="h-3 w-3" />, ns: "mcp", call: 'search("stripe idempotency")', ok: true },
    { kind: "tool", icon: <Sparkles className="h-3 w-3" />, ns: "solana", call: "simulate(tx)", ok: true },
    { kind: "ask", icon: <PauseCircle className="h-3 w-3" />, ns: "ask", call: "approval · push branch → origin?", pending: true },
  ]
  return (
    <div className="rounded-lg border border-border/60 bg-background/70 p-4 font-mono text-[12px] leading-relaxed">
      <div className="flex items-center justify-between border-b border-border/60 pb-2 text-[11px] text-muted-foreground">
        <span>
          session · <span className="text-foreground/80">engineer</span> · run timeline
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/[0.08] px-2 py-0.5 text-[10px] uppercase tracking-wider text-primary">
          <span className="relative inline-flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full animate-ring-ping rounded-full bg-primary" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
          </span>
          live · 8 events
        </span>
      </div>
      <ol className="relative mt-3 space-y-1 pl-6">
        <span
          aria-hidden
          className="pointer-events-none absolute left-[10px] top-2 bottom-2 w-px bg-gradient-to-b from-border/30 via-border/70 to-primary/40"
        />
        {calls.map((l, i) => {
          const isPending = l.kind === "ask"
          return (
            <li key={i} className="relative">
              <span
                aria-hidden
                className={`absolute left-[-19px] top-[7px] inline-flex h-4 w-4 items-center justify-center rounded-full ring-4 ring-background ${
                  isPending
                    ? "bg-primary/20 text-primary"
                    : "bg-primary/10 text-primary/80"
                }`}
              >
                {isPending ? (
                  <PauseCircle className="h-2.5 w-2.5" />
                ) : (
                  <CheckCircle2 className="h-2.5 w-2.5" />
                )}
              </span>
              <div
                className={`flex items-center gap-2 rounded-md border px-2 py-1.5 ${
                  isPending
                    ? "border-primary/40 bg-primary/[0.06]"
                    : "border-transparent hover:border-border/40 hover:bg-secondary/20"
                }`}
              >
                <span
                  className={`inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[10px] uppercase tracking-wider ${
                    isPending
                      ? "bg-primary/15 text-primary"
                      : "bg-secondary/60 text-muted-foreground/80"
                  }`}
                >
                  {l.icon}
                  {l.ns}
                </span>
                <span className="min-w-0 flex-1 truncate text-foreground/90">{l.call}</span>
                {l.ok && (
                  <span className="shrink-0 inline-flex items-center gap-1 text-[10px] uppercase tracking-wider text-primary/80">
                    ok
                    <ChevronRight className="h-2.5 w-2.5" />
                  </span>
                )}
                {isPending && (
                  <span className="shrink-0 inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/15 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-primary">
                    <span className="h-1 w-1 animate-pulse-dot rounded-full bg-primary" />
                    you
                  </span>
                )}
              </div>
            </li>
          )
        })}
      </ol>
    </div>
  )
}
