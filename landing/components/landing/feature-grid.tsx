import {
  Activity,
  Bot,
  Boxes,
  ClipboardList,
  Compass,
  Gauge,
  Layers,
  LayoutGrid,
  Lock,
  MessagesSquare,
  Network,
  Rewind,
  Smartphone,
  Sparkles,
  Workflow as WorkflowIcon,
  Wrench,
} from "lucide-react"

type Feature = {
  icon: React.ReactNode
  title: string
  body: string
  soon?: boolean
}

const today: Feature[] = [
  {
    icon: <LayoutGrid className="h-4 w-4" />,
    title: "Multi-pane agent workspace",
    body: "Up to six independent sessions per project, each with its own agent type and model.",
  },
  {
    icon: <Bot className="h-4 w-4" />,
    title: "Custom agent definitions",
    body: "Built-in Ask, Engineer, Debug, and Agent Create roles, plus project- or global-level custom agents.",
  },
  {
    icon: <Rewind className="h-4 w-4" />,
    title: "Session memory & recovery",
    body: "Transcript search, export, context visualization, reviewed memory, manual / auto compact, branch, rewind, same-type handoff.",
  },
  {
    icon: <Lock className="h-4 w-4" />,
    title: "Local-first privacy",
    body: "Project state lives in OS app-data. Credentials are kept local and keychain-backed. Redaction runs before context, memory, and export.",
  },
  {
    icon: <MessagesSquare className="h-4 w-4" />,
    title: "Human approval loop",
    body: "Agents pause on real decisions and can ping you on Discord or Telegram with the relevant diff or tradeoff.",
  },
  {
    icon: <Compass className="h-4 w-4" />,
    title: "In-app browser automation",
    body: "Tabs, navigation, clicks, typing, screenshots, cookies / storage, console & network diagnostics, accessibility snapshots.",
  },
  {
    icon: <Smartphone className="h-4 w-4" />,
    title: "Mobile app automation",
    body: "iOS and Android sidebars with device lifecycle, screenshots, UI tree, tap / swipe / type, and app lifecycle hooks.",
  },
  {
    icon: <Sparkles className="h-4 w-4" />,
    title: "Solana workbench",
    body: "Local clusters, personas, transaction build / sim / send / explain, IDL / PDA / ALT flows, deploy helpers, rollback.",
  },
  {
    icon: <Network className="h-4 w-4" />,
    title: "MCP & plugins",
    body: "Connect external tools through MCP, install skills and plugins, manage trust states and diagnostics.",
  },
  {
    icon: <ClipboardList className="h-4 w-4" />,
    title: "Provider diagnostics",
    body: "Check auth, model catalogs, stale bindings, local endpoints, and produce redacted doctor reports.",
  },
  {
    icon: <Activity className="h-4 w-4" />,
    title: "Operator-grade observability",
    body: "Tool events, checkpoints, file changes, action requests, usage records, and full run timelines.",
  },
  {
    icon: <Gauge className="h-4 w-4" />,
    title: "Benchmarkable harness",
    body: "Fixed-model eval strategy for SWE-style tasks, Terminal-Bench-style tasks, and private Xero-shaped evals.",
  },
]

const soon: Feature[] = [
  {
    icon: <Boxes className="h-4 w-4" />,
    title: "Agentic swarm coordination",
    body: "Summon up to 6 sessions that share file reservations, live presence, conflict warnings, and temporary swarm notes — so they collaborate instead of overwriting each other.",
    soon: true,
  },
  {
    icon: <Wrench className="h-4 w-4" />,
    title: "Visual Agent Builder",
    body: "Create new agent types with AI assistance and a drag-and-drop canvas — no hand-rolled YAML required.",
    soon: true,
  },
  {
    icon: <WorkflowIcon className="h-4 w-4" />,
    title: "Workflow Builder",
    body: "Stitch multiple agents into long workflows with steps, branches, loops, handoffs, and verification gates.",
    soon: true,
  },
  {
    icon: <Layers className="h-4 w-4" />,
    title: "Build-an-entire-app workflows",
    body: "Once the workflow graph lands, position Xero as the harness that can drive a full application from a single workflow definition.",
    soon: true,
  },
]

export function FeatureGrid() {
  return (
    <section id="capabilities" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Capabilities
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            What ships today.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Everything below is wired and running in the current build. The four
            cards further down are clearly marked &quot;coming soon&quot; — we&apos;d
            rather under-promise than ship vibes.
          </p>
        </div>

        <ul className="mt-12 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {today.map((f) => (
            <FeatureCard key={f.title} f={f} />
          ))}
        </ul>

        <div className="mt-20">
          <div className="mx-auto max-w-2xl text-center">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Coming soon
            </p>
            <h3 className="mt-3 font-sans text-2xl font-medium tracking-tight text-balance sm:text-3xl">
              On the roadmap, not in the bundle yet.
            </h3>
            <p className="mt-3 text-pretty text-muted-foreground">
              These are the features we&apos;re actively building. They&apos;ll
              ship behind clear toggles, not silently swapped in.
            </p>
          </div>

          <ul className="mt-10 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {soon.map((f) => (
              <FeatureCard key={f.title} f={f} />
            ))}
          </ul>
        </div>
      </div>
    </section>
  )
}

function FeatureCard({ f }: { f: Feature }) {
  return (
    <li
      className={`group relative flex flex-col gap-3 overflow-hidden rounded-xl border p-5 transition-all hover:-translate-y-0.5 ${
        f.soon
          ? "border-dashed border-border/70 bg-secondary/20 hover:border-border"
          : "border-border/70 bg-card hover:border-border hover:shadow-[0_18px_40px_-30px_rgba(0,0,0,0.6)]"
      }`}
    >
      <div className="flex items-center justify-between gap-3">
        <span
          className={`inline-flex h-8 w-8 items-center justify-center rounded-lg ${
            f.soon
              ? "bg-secondary text-muted-foreground"
              : "bg-primary/15 text-primary"
          }`}
        >
          {f.icon}
        </span>
        {f.soon ? (
          <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/[0.06] px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-primary">
            <span className="h-1 w-1 rounded-full bg-primary" />
            Coming soon
          </span>
        ) : (
          <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground/70">
            Live
          </span>
        )}
      </div>
      <h4 className="text-sm font-medium tracking-tight">{f.title}</h4>
      <p className="text-sm leading-relaxed text-muted-foreground">{f.body}</p>
    </li>
  )
}
