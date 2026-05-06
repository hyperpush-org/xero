import {
  Bot,
  Code2,
  GitBranch,
  Globe,
  Layers,
  Rocket,
  Wrench,
  Cpu,
  Sparkles,
  Terminal,
  ChevronRight,
  CircleDot,
} from "lucide-react"
import {
  DiscordIcon,
  TelegramIcon,
} from "@/components/landing/brand-icons"

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
    visual: <BuildVisual />,
  },
  {
    n: "02",
    label: "Chain",
    title: "Compose the workflow",
    caption: "Steps, branches, loops, gates.",
    icon: <Layers className="h-3.5 w-3.5" />,
    visual: <ChainVisual />,
  },
  {
    n: "03",
    label: "Approve",
    title: "Decide from your phone",
    caption: "Discord or Telegram on real calls.",
    icon: <Bot className="h-3.5 w-3.5" />,
    visual: <ApproveVisual />,
  },
  {
    n: "04",
    label: "Ship",
    title: "Project lands done",
    caption: "Branch, merge, deploy, repeat.",
    icon: <Rocket className="h-3.5 w-3.5" />,
    visual: <ShipVisual />,
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
              <article className="group relative flex w-full flex-col overflow-hidden rounded-2xl border border-border/70 bg-card transition-all hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-[0_24px_60px_-30px_color-mix(in_oklab,var(--primary)_45%,transparent)]">
                {/* Card header: stage label */}
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

                {/* Visual area */}
                <div className="flex h-48 items-center justify-center bg-gradient-to-b from-transparent to-secondary/[0.08] p-4">
                  {s.visual}
                </div>

                {/* Footer: title + caption */}
                <footer className="border-t border-border/60 px-4 py-4">
                  <h3 className="text-base font-medium tracking-tight">
                    {s.title}
                  </h3>
                  <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                    {s.caption}
                  </p>
                </footer>
              </article>

              {/* Connector — desktop: between cards; mobile/tablet: hidden */}
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

/* -------- Stage visuals -------- */

function BuildVisual() {
  const chips: { text: string; icon: React.ReactNode; on: boolean }[] = [
    { text: "repo", icon: <Code2 className="h-2.5 w-2.5" />, on: true },
    { text: "shell", icon: <Terminal className="h-2.5 w-2.5" />, on: true },
    { text: "git", icon: <GitBranch className="h-2.5 w-2.5" />, on: true },
    { text: "browser", icon: <Globe className="h-2.5 w-2.5" />, on: false },
    { text: "memory", icon: <Sparkles className="h-2.5 w-2.5" />, on: true },
    { text: "ask · push", icon: <CircleDot className="h-2.5 w-2.5" />, on: true },
  ]
  return (
    <div className="flex w-full max-w-[220px] flex-col items-center gap-3">
      <div className="relative flex items-center gap-2 rounded-md border border-primary/40 bg-primary/[0.08] px-3 py-1.5 shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_8%,transparent)]">
        <span className="relative inline-flex h-5 w-5 items-center justify-center rounded bg-primary/20 text-primary">
          <Bot className="h-3 w-3" />
          <span
            aria-hidden
            className="absolute -bottom-0.5 -right-0.5 h-1.5 w-1.5 rounded-full border-[1.5px] border-card bg-primary"
          />
        </span>
        <span className="font-mono text-[11px] text-foreground">solana-ops</span>
      </div>
      <div className="grid w-full grid-cols-3 gap-1.5">
        {chips.map((c, i) => (
          <span
            key={i}
            className={`flex items-center justify-center gap-1 truncate rounded border px-1.5 py-1 font-mono text-[9px] ${
              c.on
                ? "border-primary/30 bg-primary/[0.08] text-primary"
                : "border-dashed border-border/60 bg-transparent text-muted-foreground/60"
            }`}
          >
            {c.icon}
            {c.text}
          </span>
        ))}
      </div>
    </div>
  )
}

function ChainVisual() {
  return (
    <div className="flex w-full flex-col items-center justify-center gap-2">
      <svg
        viewBox="0 0 240 130"
        className="h-[130px] w-full"
        aria-hidden
      >
        <defs>
          <linearGradient id="wf-edge" x1="0" x2="1">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.25" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0.85" />
          </linearGradient>
          <radialGradient id="wf-gate-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.35" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* gate glow */}
        <circle cx="130" cy="65" r="32" fill="url(#wf-gate-glow)" />

        {/* edges */}
        <path d="M 38 35 C 88 35, 96 65, 116 65" stroke="url(#wf-edge)" strokeWidth="1.4" fill="none" />
        <path d="M 38 95 C 88 95, 96 65, 116 65" stroke="url(#wf-edge)" strokeWidth="1.4" fill="none" />
        <path d="M 144 65 C 168 65, 178 35, 202 35" stroke="url(#wf-edge)" strokeWidth="1.4" fill="none" />
        <path d="M 144 65 C 168 65, 178 95, 202 95" stroke="url(#wf-edge)" strokeWidth="1.4" fill="none" />

        {/* nodes — A */}
        <g>
          <circle cx="22" cy="35" r="13" fill="var(--card)" stroke="var(--border)" />
          <circle cx="22" cy="35" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
          <text x="40" y="20" fontSize="8" fill="var(--muted-foreground)" fontFamily="var(--font-mono)">ask</text>
        </g>
        {/* node B */}
        <g>
          <circle cx="22" cy="95" r="13" fill="var(--card)" stroke="var(--border)" />
          <circle cx="22" cy="95" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
          <text x="40" y="113" fontSize="8" fill="var(--muted-foreground)" fontFamily="var(--font-mono)">eng</text>
        </g>
        {/* gate */}
        <g>
          <rect x="113" y="51" width="34" height="28" rx="7" fill="color-mix(in oklab, var(--primary) 14%, var(--card))" stroke="color-mix(in oklab, var(--primary) 55%, transparent)" />
          <text x="130" y="69" textAnchor="middle" fontSize="10" fill="var(--primary)" fontFamily="var(--font-mono)" fontWeight="600">if</text>
        </g>
        {/* node C */}
        <g>
          <circle cx="216" cy="35" r="13" fill="var(--card)" stroke="var(--border)" />
          <circle cx="216" cy="35" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
          <text x="200" y="20" textAnchor="end" fontSize="8" fill="var(--muted-foreground)" fontFamily="var(--font-mono)">ship</text>
        </g>
        {/* node D */}
        <g>
          <circle cx="216" cy="95" r="13" fill="var(--card)" stroke="var(--border)" />
          <circle cx="216" cy="95" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
          <text x="200" y="113" textAnchor="end" fontSize="8" fill="var(--muted-foreground)" fontFamily="var(--font-mono)">retry</text>
        </g>

        {/* travelling pulse */}
        <circle r="2.5" fill="var(--primary)">
          <animateMotion
            dur="3s"
            repeatCount="indefinite"
            path="M 38 35 C 88 35, 96 65, 116 65 L 144 65 C 168 65, 178 35, 202 35"
          />
        </circle>
      </svg>
      <div className="flex items-center gap-1.5 font-mono text-[9px] uppercase tracking-wider text-muted-foreground">
        <Sparkles className="h-2.5 w-2.5 text-primary/70" />
        steps · branches · gates
      </div>
    </div>
  )
}

function ApproveVisual() {
  return (
    <div className="flex w-full max-w-[210px] flex-col items-center gap-2.5">
      {/* phone-like card */}
      <div className="relative w-full rounded-2xl border border-border/70 bg-background/60 p-1.5 shadow-[0_20px_40px_-20px_rgba(0,0,0,0.7)]">
        <span
          aria-hidden
          className="absolute left-1/2 top-1.5 z-10 h-1 w-10 -translate-x-1/2 rounded-full bg-secondary/80"
        />
        <div className="overflow-hidden rounded-xl border border-border/40 bg-card pt-2.5">
          <div className="flex items-center gap-1.5 border-b border-border/60 bg-secondary/40 px-2.5 py-1.5">
            <span
              className="inline-flex h-4 w-4 items-center justify-center rounded text-white"
              style={{ backgroundColor: "#5865F2" }}
            >
              <DiscordIcon className="h-2.5 w-2.5 text-white" />
            </span>
            <span className="font-mono text-[10px] text-foreground/80">
              #xero-approvals
            </span>
            <span className="ml-auto inline-flex items-center gap-1 font-mono text-[9px] text-muted-foreground/70">
              <span className="h-1 w-1 animate-pulse-dot rounded-full bg-primary" />
              now
            </span>
          </div>
          <div className="px-2.5 py-2">
            <p className="font-mono text-[11px] leading-snug text-foreground/90">
              push{" "}
              <code className="rounded bg-primary/15 px-1 py-0.5 text-[10px] text-primary">
                try-pg
              </code>{" "}
              → origin?
            </p>
            <div className="mt-2 flex gap-1.5">
              <span className="rounded bg-primary px-2 py-0.5 text-[10px] font-medium text-primary-foreground shadow-[0_4px_10px_-4px_color-mix(in_oklab,var(--primary)_60%,transparent)]">
                Approve
              </span>
              <span className="rounded border border-border/70 bg-secondary/40 px-2 py-0.5 text-[10px]">
                Skip
              </span>
            </div>
          </div>
        </div>
      </div>
      <div className="flex items-center gap-2 font-mono text-[9px] uppercase tracking-wider text-muted-foreground/70">
        <span
          className="inline-flex h-3.5 w-3.5 items-center justify-center rounded text-white"
          style={{ backgroundColor: "#5865F2" }}
        >
          <DiscordIcon className="h-2 w-2 text-white" />
        </span>
        <span>or</span>
        <span
          className="inline-flex h-3.5 w-3.5 items-center justify-center rounded text-white"
          style={{ backgroundColor: "#26A5E4" }}
        >
          <TelegramIcon className="h-2 w-2 text-white" />
        </span>
      </div>
    </div>
  )
}

function ShipVisual() {
  return (
    <div className="flex w-full max-w-[220px] flex-col gap-1.5">
      <div className="flex items-center gap-2 rounded-md border border-border/60 bg-background/60 px-2.5 py-2">
        <span className="inline-flex h-5 w-5 items-center justify-center rounded bg-primary/15 text-primary">
          <GitBranch className="h-3 w-3" />
        </span>
        <span className="flex-1 truncate font-mono text-[10px]">try-pg</span>
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/15 px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wider text-primary">
          merged
        </span>
      </div>
      <div className="relative overflow-hidden rounded-md border border-border/60 bg-background/60 px-2.5 py-2">
        <div className="flex items-center gap-2">
          <span className="inline-flex h-5 w-5 items-center justify-center rounded bg-primary/15 text-primary">
            <Cpu className="h-3 w-3" />
          </span>
          <span className="flex-1 truncate font-mono text-[10px]">deploy · prod</span>
          <span className="font-mono text-[9px] tabular-nums text-primary">72%</span>
        </div>
        <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-secondary/60">
          <div
            className="h-full rounded-full bg-gradient-to-r from-primary/60 to-primary"
            style={{ width: "72%" }}
          />
        </div>
        <span
          aria-hidden
          className="pointer-events-none absolute inset-x-0 bottom-1.5 h-1 overflow-hidden rounded-full"
        >
          <span className="block h-full w-1/3 animate-shimmer-bar bg-gradient-to-r from-transparent via-white/40 to-transparent" />
        </span>
      </div>
      <div className="flex items-center gap-2 rounded-md border border-dashed border-border/60 bg-transparent px-2.5 py-2">
        <span className="inline-flex h-5 w-5 items-center justify-center rounded bg-secondary/40 text-muted-foreground/70">
          <Sparkles className="h-3 w-3" />
        </span>
        <span className="flex-1 truncate font-mono text-[10px] text-muted-foreground">
          next brief
        </span>
        <ChevronRight className="h-3 w-3 text-muted-foreground/60" />
      </div>
    </div>
  )
}
