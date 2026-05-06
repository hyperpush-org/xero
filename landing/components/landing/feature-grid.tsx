import {
  Bot,
  CheckCircle2,
  Code2,
  Globe,
  KeyRound,
  Laptop,
  Loader2,
  PauseCircle,
  Plus,
  PuzzleIcon,
  Smartphone,
  Sparkles,
  Terminal,
  Wrench,
  GitFork,
} from "lucide-react"
import {
  AnthropicIcon,
  DiscordIcon,
  GoogleIcon,
  OpenAIIcon,
  TelegramIcon,
} from "@/components/landing/brand-icons"

export function FeatureGrid() {
  return (
    <section id="capabilities" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Capabilities
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            What's in the build.
          </h2>
        </div>

        {/* Bento */}
        <div className="mt-14 grid auto-rows-[minmax(0,1fr)] grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-12">
          {/* Custom agents — hero card */}
          <BentoCard
            className="lg:col-span-7 lg:row-span-2"
            title="Agents you actually design"
            caption="Pick the tools, memory, and approval rules per agent."
            visual={<CustomAgentsMock />}
            visualClassName="min-h-[280px]"
          />

          {/* Workflow graph */}
          <BentoCard
            className="lg:col-span-5"
            title="Composable workflows"
            caption="Steps, branches, loops, gates."
            visual={<WorkflowGraphMock />}
          />

          {/* Mobile approvals */}
          <BentoCard
            className="lg:col-span-5"
            title="Approve from your phone"
            caption="Discord and Telegram, with the actual diff."
            visual={<MobileApprovalsMock />}
          />

          {/* Six panes */}
          <BentoCard
            className="lg:col-span-4"
            title="Six-pane workspace"
            caption="Mix roles and models per project."
            visual={<SixPanesMock />}
          />

          {/* Branch & rewind */}
          <BentoCard
            className="lg:col-span-4"
            title="Branch & rewind"
            caption="Fork sessions, roll back checkpoints."
            visual={<BranchRewindMock />}
          />

          {/* Run timeline */}
          <BentoCard
            className="lg:col-span-4"
            title="Run timeline"
            caption="Every call, change, and approval."
            visual={<RunTimelineMock />}
          />

          {/* MCP & integrations */}
          <BentoCard
            className="lg:col-span-6"
            title="MCP, Solana, mobile, browser"
            caption="The tools your projects actually need."
            visual={<IntegrationsMock />}
          />

          {/* Local credentials */}
          <BentoCard
            className="lg:col-span-6"
            title="Keys local. Models direct."
            caption="OS keychain. No relay. 10 providers."
            visual={<KeysDirectMock />}
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
      className={`group relative flex flex-col overflow-hidden rounded-2xl border border-border/70 bg-card transition-all hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-[0_24px_60px_-30px_color-mix(in_oklab,var(--primary)_45%,transparent)] ${
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

/* -------- Visual mocks -------- */

function CustomAgentsMock() {
  const tools: { t: string; icon: React.ReactNode; on: boolean; dashed?: boolean }[] = [
    { t: "repo", icon: <Code2 className="h-2.5 w-2.5" />, on: true },
    { t: "shell", icon: <Terminal className="h-2.5 w-2.5" />, on: true },
    { t: "git", icon: <GitFork className="h-2.5 w-2.5" />, on: true },
    { t: "browser", icon: <Globe className="h-2.5 w-2.5" />, on: true },
    { t: "mobile", icon: <Smartphone className="h-2.5 w-2.5" />, on: false },
    { t: "mcp", icon: <Sparkles className="h-2.5 w-2.5" />, on: true },
    { t: "solana", icon: <Sparkles className="h-2.5 w-2.5" />, on: true },
    { t: "skills", icon: <PuzzleIcon className="h-2.5 w-2.5" />, on: true },
    { t: "add", icon: <Plus className="h-2.5 w-2.5" />, on: false, dashed: true },
  ]
  return (
    <div className="grid w-full max-w-[520px] grid-cols-12 gap-3">
      {/* Agent header */}
      <div className="relative col-span-12 flex items-center justify-between overflow-hidden rounded-lg border border-primary/40 bg-primary/[0.06] px-3 py-2">
        <span
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-px overflow-hidden"
        >
          <span className="block h-full w-1/3 animate-shimmer-bar bg-gradient-to-r from-transparent via-primary/80 to-transparent" />
        </span>
        <div className="flex items-center gap-2">
          <span className="relative inline-flex h-8 w-8 items-center justify-center rounded-md bg-primary/15 text-primary ring-1 ring-inset ring-primary/30">
            <Bot className="h-4 w-4" />
            <span
              aria-hidden
              className="absolute -bottom-0.5 -right-0.5 h-2 w-2 rounded-full border-2 border-card bg-primary"
            />
          </span>
          <div>
            <div className="font-mono text-[12px] text-foreground">solana-ops</div>
            <div className="font-mono text-[10px] text-muted-foreground">
              custom · project scope
            </div>
          </div>
        </div>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 font-mono text-[9px] uppercase tracking-wider text-primary">
          <span className="relative inline-flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full animate-ring-ping rounded-full bg-primary" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
          </span>
          live
        </span>
      </div>

      {/* Tools panel */}
      <div className="col-span-7 rounded-lg border border-border/60 bg-background/60 p-3">
        <div className="mb-2 flex items-center justify-between font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
          <span className="inline-flex items-center gap-1">
            <Wrench className="h-2.5 w-2.5" /> tools
          </span>
          <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[9px] text-primary">
            7 wired
          </span>
        </div>
        <div className="grid grid-cols-3 gap-1.5">
          {tools.map((c, i) => (
            <span
              key={i}
              className={`flex items-center justify-center gap-1 truncate rounded border px-1.5 py-1 font-mono text-[10px] transition-colors ${
                c.on
                  ? "border-primary/30 bg-primary/[0.08] text-primary"
                  : c.dashed
                    ? "border-dashed border-border/60 bg-transparent text-muted-foreground/60"
                    : "border-border/60 bg-secondary/30 text-muted-foreground/70"
              }`}
            >
              {c.icon}
              {c.t}
            </span>
          ))}
        </div>
      </div>

      {/* Memory + approval */}
      <div className="col-span-5 flex flex-col gap-2">
        <div className="rounded-lg border border-border/60 bg-background/60 p-3">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
              memory
            </span>
            <span className="font-mono text-[9px] text-muted-foreground/60">2 / 3</span>
          </div>
          <div className="mt-1.5 space-y-1">
            {[
              { l: "session", v: "on" as const },
              { l: "cross-run", v: "on" as const },
              { l: "global", v: "off" as const },
            ].map((row) => (
              <div
                key={row.l}
                className="flex items-center justify-between font-mono text-[10px]"
              >
                <span className="text-foreground/80">{row.l}</span>
                <ToggleDot on={row.v === "on"} />
              </div>
            ))}
          </div>
        </div>
        <div className="rounded-lg border border-border/60 bg-background/60 p-3">
          <div className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
            approvals
          </div>
          <div className="mt-1.5 space-y-1 font-mono text-[10px]">
            {[
              { l: "git push", v: "ask" as const },
              { l: "tx send", v: "ask" as const },
              { l: "repo edit", v: "auto" as const },
            ].map((row) => (
              <div key={row.l} className="flex items-center justify-between">
                <span className="text-foreground/80">{row.l}</span>
                <span
                  className={`rounded px-1.5 py-0.5 text-[9px] uppercase tracking-wider ${
                    row.v === "ask"
                      ? "bg-primary/15 text-primary"
                      : "bg-secondary/60 text-muted-foreground/70"
                  }`}
                >
                  {row.v}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}

function ToggleDot({ on }: { on: boolean }) {
  return (
    <span
      className={`inline-flex h-3 w-5 items-center rounded-full p-0.5 transition-colors ${
        on ? "bg-primary/40" : "bg-secondary/60"
      }`}
    >
      <span
        className={`block h-2 w-2 rounded-full transition-transform ${
          on ? "translate-x-2 bg-primary" : "translate-x-0 bg-muted-foreground/60"
        }`}
      />
    </span>
  )
}

function WorkflowGraphMock() {
  return (
    <svg
      viewBox="0 0 280 180"
      className="h-[180px] w-full max-w-[300px]"
      aria-hidden
    >
      <defs>
        <linearGradient id="cap-edge" x1="0" x2="1">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.25" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0.85" />
        </linearGradient>
        <radialGradient id="cap-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.35" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* gate glow */}
      <circle cx="157" cy="90" r="34" fill="url(#cap-glow)" />

      {/* edges */}
      <path d="M 74 40 C 110 40, 120 90, 138 90" stroke="url(#cap-edge)" strokeWidth="1.4" fill="none" />
      <path d="M 74 140 C 110 140, 120 90, 138 90" stroke="url(#cap-edge)" strokeWidth="1.4" fill="none" />
      <path d="M 176 90 C 200 90, 210 40, 232 40" stroke="url(#cap-edge)" strokeWidth="1.4" fill="none" />
      <path d="M 176 90 C 200 90, 210 140, 232 140" stroke="url(#cap-edge)" strokeWidth="1.4" fill="none" />
      {/* loop dashed back-edge */}
      <path
        d="M 232 140 C 200 156, 90 156, 74 140"
        stroke="color-mix(in oklab, var(--primary) 70%, transparent)"
        strokeWidth="1.2"
        fill="none"
        className="animate-flow-dash"
      />

      {/* node A — ask */}
      <g>
        <rect x="14" y="24" width="60" height="32" rx="8" fill="var(--card)" stroke="var(--border)" />
        <circle cx="26" cy="40" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
        <text x="48" y="44" textAnchor="middle" fontSize="11" fill="var(--foreground)" fontFamily="var(--font-mono)">
          ask
        </text>
      </g>
      {/* node B — engineer */}
      <g>
        <rect x="14" y="124" width="60" height="32" rx="8" fill="var(--card)" stroke="var(--border)" />
        <circle cx="26" cy="140" r="3" fill="color-mix(in oklab, var(--primary) 80%, transparent)" />
        <text x="48" y="144" textAnchor="middle" fontSize="11" fill="var(--foreground)" fontFamily="var(--font-mono)">
          engineer
        </text>
      </g>
      {/* gate */}
      <g>
        <rect
          x="138"
          y="74"
          width="38"
          height="32"
          rx="8"
          fill="color-mix(in oklab, var(--primary) 14%, var(--card))"
          stroke="color-mix(in oklab, var(--primary) 55%, transparent)"
        />
        <text x="157" y="94" textAnchor="middle" fontSize="10.5" fill="var(--primary)" fontFamily="var(--font-mono)" fontWeight="600">
          gate
        </text>
      </g>
      {/* node C — ship */}
      <g>
        <rect x="232" y="24" width="36" height="32" rx="8" fill="var(--card)" stroke="var(--border)" />
        <text x="250" y="44" textAnchor="middle" fontSize="11" fill="var(--foreground)" fontFamily="var(--font-mono)">
          ship
        </text>
      </g>
      {/* node D — retry */}
      <g>
        <rect x="232" y="124" width="36" height="32" rx="8" fill="var(--card)" stroke="var(--border)" />
        <text x="250" y="144" textAnchor="middle" fontSize="11" fill="var(--muted-foreground)" fontFamily="var(--font-mono)">
          retry
        </text>
      </g>

      {/* travelling pulse on top edge */}
      <circle r="2.5" fill="var(--primary)">
        <animateMotion
          dur="3.2s"
          repeatCount="indefinite"
          path="M 74 40 C 110 40, 120 90, 138 90 L 176 90 C 200 90, 210 40, 232 40"
        />
      </circle>
    </svg>
  )
}

function MobileApprovalsMock() {
  return (
    <div className="flex w-full max-w-[300px] flex-col items-center gap-2">
      {/* Phone frame */}
      <div className="relative w-full rounded-[28px] border border-border/70 bg-background/60 p-1.5 shadow-[0_30px_60px_-25px_rgba(0,0,0,0.7)]">
        {/* notch */}
        <span
          aria-hidden
          className="absolute left-1/2 top-1.5 z-10 h-1.5 w-16 -translate-x-1/2 rounded-full bg-secondary/80"
        />
        <div className="overflow-hidden rounded-[22px] border border-border/40 bg-card">
          {/* status bar */}
          <div className="flex items-center justify-between px-3 pt-3 pb-1.5 font-mono text-[9px] text-muted-foreground/70">
            <span>9:41</span>
            <span className="flex items-center gap-1">
              <span className="h-1 w-1 rounded-full bg-muted-foreground/60" />
              <span className="h-1 w-1 rounded-full bg-muted-foreground/60" />
              <span className="h-1 w-1 rounded-full bg-muted-foreground/60" />
            </span>
          </div>
          {/* notification card */}
          <div className="mx-2 mb-2 overflow-hidden rounded-xl border border-border/60 bg-background/70 shadow-[0_10px_24px_-15px_rgba(0,0,0,0.6)]">
            <div className="flex items-center gap-2 border-b border-border/60 bg-secondary/30 px-2.5 py-1.5">
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
                <span className="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-primary" />
                now
              </span>
            </div>
            <div className="px-2.5 py-2">
              <p className="font-mono text-[11px] leading-relaxed text-foreground/90">
                <span className="font-medium text-foreground">acme-saas</span> · push branch{" "}
                <code className="rounded bg-primary/15 px-1 py-0.5 text-[10px] text-primary">
                  try-pg
                </code>{" "}
                → origin
              </p>
              <div className="mt-2 overflow-hidden rounded border border-border/60 bg-secondary/15 font-mono text-[10px] leading-snug">
                <div className="flex items-center gap-1 bg-secondary/30 px-2 py-0.5 text-[9px] uppercase tracking-wider text-muted-foreground/70">
                  <Code2 className="h-2.5 w-2.5" />
                  diff · src/billing.ts
                </div>
                <pre className="px-2 py-1.5 text-muted-foreground">
                  <span className="text-primary">+</span> retry helper · 12 lines
                  {"\n"}
                  <span className="text-destructive/80">-</span> inline retry · 4
                </pre>
              </div>
              <div className="mt-2 grid grid-cols-3 gap-1">
                <span className="rounded bg-primary px-2 py-1 text-center text-[10px] font-medium text-primary-foreground shadow-[0_4px_12px_-4px_color-mix(in_oklab,var(--primary)_60%,transparent)]">
                  Approve
                </span>
                <span className="rounded border border-border/70 bg-secondary/40 px-2 py-1 text-center text-[10px]">
                  Skip
                </span>
                <span className="rounded border border-border/70 bg-secondary/40 px-2 py-1 text-center text-[10px]">
                  Diff
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div className="flex items-center gap-2 font-mono text-[9px] uppercase tracking-wider text-muted-foreground/60">
        <span>also on</span>
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

function SixPanesMock() {
  type Pane = { role: string; state: "running" | "idle" | "decision"; icon: React.ReactNode }
  const panes: Pane[] = [
    { role: "Engineer", state: "running", icon: <Code2 className="h-2.5 w-2.5" /> },
    { role: "Debug", state: "running", icon: <Wrench className="h-2.5 w-2.5" /> },
    { role: "Ask", state: "idle", icon: <Bot className="h-2.5 w-2.5" /> },
    { role: "Engineer", state: "running", icon: <Code2 className="h-2.5 w-2.5" /> },
    { role: "solana-ops", state: "decision", icon: <Sparkles className="h-2.5 w-2.5" /> },
    { role: "Engineer", state: "idle", icon: <Code2 className="h-2.5 w-2.5" /> },
  ]
  return (
    <div className="grid w-full max-w-[260px] grid-cols-3 gap-1.5">
      {panes.map((p, i) => {
        const isDecision = p.state === "decision"
        const isRunning = p.state === "running"
        const isLive = isRunning || isDecision
        return (
          <div
            key={i}
            className={`relative flex aspect-[4/3] flex-col justify-between overflow-hidden rounded-md border p-2 transition-colors ${
              isDecision
                ? "border-primary/45 bg-primary/[0.08] shadow-[0_0_0_1px_color-mix(in_oklab,var(--primary)_18%,transparent)]"
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
            <div className="flex items-center gap-1">
              <span
                className={`inline-flex h-3.5 w-3.5 items-center justify-center rounded ${
                  isLive
                    ? "bg-primary/15 text-primary"
                    : "bg-secondary/60 text-muted-foreground/70"
                }`}
              >
                {p.icon}
              </span>
              <span className="truncate font-mono text-[9px] text-foreground/80">
                {p.role}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span
                className={`font-mono text-[8px] uppercase tracking-wider ${
                  isLive ? "text-primary" : "text-muted-foreground/60"
                }`}
              >
                {p.state}
              </span>
              {isDecision && (
                <span className="relative inline-flex h-1.5 w-1.5">
                  <span className="absolute inline-flex h-full w-full animate-ring-ping rounded-full bg-primary" />
                  <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
                </span>
              )}
              {isRunning && (
                <span className="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-primary" />
              )}
            </div>
          </div>
        )
      })}
    </div>
  )
}

function BranchRewindMock() {
  return (
    <svg
      viewBox="0 0 220 140"
      className="h-[140px] w-full max-w-[260px]"
      aria-hidden
    >
      <defs>
        <linearGradient id="bm-main" x1="0" x2="1">
          <stop offset="0%" stopColor="var(--border)" stopOpacity="0.4" />
          <stop offset="50%" stopColor="var(--border)" stopOpacity="0.9" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0.9" />
        </linearGradient>
        <linearGradient id="bm-fork" x1="0" x2="1">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.3" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0.9" />
        </linearGradient>
      </defs>

      {/* main branch */}
      <line x1="20" y1="40" x2="200" y2="40" stroke="url(#bm-main)" strokeWidth="1.5" />
      {/* fork branch */}
      <path
        d="M 80 40 C 100 40, 100 100, 120 100 L 180 100"
        stroke="url(#bm-fork)"
        strokeWidth="1.5"
        fill="none"
      />

      {/* main commits */}
      {[20, 50, 80, 110, 140, 170, 200].map((cx, i) => (
        <circle
          key={i}
          cx={cx}
          cy={40}
          r={i === 6 ? 5 : 4}
          fill={i === 6 ? "var(--primary)" : "var(--card)"}
          stroke={i === 6 ? "var(--primary)" : "var(--border)"}
          strokeWidth="1.5"
        />
      ))}
      {/* fork commits */}
      {[120, 150, 180].map((cx, i) => (
        <circle
          key={i}
          cx={cx}
          cy={100}
          r="4"
          fill="color-mix(in oklab, var(--primary) 22%, var(--card))"
          stroke="color-mix(in oklab, var(--primary) 70%, transparent)"
          strokeWidth="1.5"
        />
      ))}

      {/* HEAD pulse on main */}
      <circle cx="200" cy="40" r="8" fill="none" stroke="var(--primary)" strokeWidth="1" opacity="0.5">
        <animate attributeName="r" values="5;10;5" dur="2.4s" repeatCount="indefinite" />
        <animate attributeName="opacity" values="0.6;0;0.6" dur="2.4s" repeatCount="indefinite" />
      </circle>

      {/* labels */}
      <g>
        <rect x="14" y="12" width="36" height="14" rx="3" fill="var(--card)" stroke="var(--border)" />
        <text x="32" y="22" fontSize="9" fill="var(--muted-foreground)" textAnchor="middle" fontFamily="var(--font-mono)">
          main
        </text>
      </g>
      <g>
        <rect x="116" y="116" width="40" height="14" rx="3" fill="color-mix(in oklab, var(--primary) 14%, var(--card))" stroke="color-mix(in oklab, var(--primary) 50%, transparent)" />
        <text x="136" y="126" fontSize="9" fill="var(--primary)" textAnchor="middle" fontFamily="var(--font-mono)">
          try-pg
        </text>
      </g>

      {/* rewind marker */}
      <g>
        <circle cx="110" cy="40" r="10" fill="none" stroke="var(--primary)" strokeWidth="1.2" className="animate-flow-dash" />
        <text x="110" y="68" fontSize="8" fill="var(--primary)" textAnchor="middle" fontFamily="var(--font-mono)" fontWeight="600">
          rewind
        </text>
      </g>
    </svg>
  )
}

function RunTimelineMock() {
  const events: { kind: string; label: string; state: "done" | "running" | "paused"; t: string }[] = [
    { kind: "tool", label: "repo.edit · billing.ts", state: "done", t: "00:42" },
    { kind: "tool", label: "shell · cargo test", state: "done", t: "01:18" },
    { kind: "tool", label: "git.commit", state: "done", t: "01:24" },
    { kind: "tool", label: "browser · /billing", state: "running", t: "01:30" },
    { kind: "ask", label: "approval · push origin?", state: "paused", t: "01:31" },
  ]
  return (
    <ol className="relative w-full max-w-[280px] space-y-1 pl-5">
      <span
        aria-hidden
        className="pointer-events-none absolute left-[7px] top-2 bottom-2 w-px bg-gradient-to-b from-border/40 via-border/70 to-primary/40"
      />
      {events.map((e, i) => (
        <li key={i} className="relative">
          <span
            aria-hidden
            className={`absolute -left-[14px] top-[7px] inline-flex h-3.5 w-3.5 items-center justify-center rounded-full ring-4 ring-background ${
              e.state === "paused"
                ? "bg-primary/15 text-primary"
                : e.state === "running"
                  ? "bg-primary/15 text-primary"
                  : "bg-primary/10 text-primary/80"
            }`}
          >
            {e.state === "done" && <CheckCircle2 className="h-2.5 w-2.5" />}
            {e.state === "running" && <Loader2 className="h-2.5 w-2.5 animate-spin" />}
            {e.state === "paused" && <PauseCircle className="h-2.5 w-2.5" />}
          </span>
          <div
            className={`flex items-center gap-2 rounded-md border px-2 py-1.5 ${
              e.state === "paused"
                ? "border-primary/40 bg-primary/[0.08]"
                : "border-border/60 bg-background/40"
            }`}
          >
            <span className="font-mono text-[9px] uppercase tracking-wider text-muted-foreground/60">
              {e.kind}
            </span>
            <span className="flex-1 truncate font-mono text-[10px] text-foreground/85">
              {e.label}
            </span>
            <span className="shrink-0 font-mono text-[9px] tabular-nums text-muted-foreground/50">
              {e.t}
            </span>
          </div>
        </li>
      ))}
    </ol>
  )
}

function IntegrationsMock() {
  const tiles: { label: string; caption: string; icon: React.ReactNode }[] = [
    { label: "browser", caption: "tabs · console", icon: <Globe className="h-3 w-3" /> },
    { label: "mobile", caption: "iOS · Android", icon: <Smartphone className="h-3 w-3" /> },
    { label: "solana", caption: "sim · deploy", icon: <Sparkles className="h-3 w-3" /> },
    { label: "mcp", caption: "external tools", icon: <PuzzleIcon className="h-3 w-3" /> },
    { label: "skills", caption: "plugins", icon: <Wrench className="h-3 w-3" /> },
    { label: "shell", caption: "scripts · CI", icon: <Terminal className="h-3 w-3" /> },
  ]
  return (
    <div className="grid w-full max-w-[480px] grid-cols-3 gap-2">
      {tiles.map((t) => (
        <div
          key={t.label}
          className="group/tile flex items-center gap-2 rounded-md border border-border/60 bg-background/40 px-2.5 py-2 transition-colors hover:border-primary/40 hover:bg-primary/[0.04]"
        >
          <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-inset ring-primary/20 transition-colors group-hover/tile:bg-primary/15">
            {t.icon}
          </span>
          <div className="min-w-0 flex flex-col">
            <span className="truncate font-mono text-[10px] text-foreground/85">
              {t.label}
            </span>
            <span className="truncate font-mono text-[9px] text-muted-foreground/70">
              {t.caption}
            </span>
          </div>
        </div>
      ))}
    </div>
  )
}

function KeysDirectMock() {
  return (
    <div className="flex w-full max-w-[480px] items-center gap-3">
      {/* Machine */}
      <div className="flex shrink-0 flex-col items-center gap-1.5">
        <div className="relative flex h-12 w-12 items-center justify-center rounded-xl border border-primary/40 bg-primary/[0.08] text-primary shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_8%,transparent)]">
          <Laptop className="h-5 w-5" />
          <span
            aria-hidden
            className="absolute -bottom-1 -right-1 inline-flex h-4 w-4 items-center justify-center rounded-full border border-primary/40 bg-card text-primary"
          >
            <KeyRound className="h-2.5 w-2.5" />
          </span>
        </div>
        <span className="font-mono text-[10px] text-muted-foreground">
          your machine
        </span>
        <span className="rounded-full border border-border/60 bg-secondary/30 px-1.5 py-0.5 font-mono text-[8px] uppercase tracking-wider text-muted-foreground">
          keychain
        </span>
      </div>

      {/* Lines with traveling pulses */}
      <div className="relative flex flex-1 flex-col justify-center gap-2.5 py-2">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="relative flex items-center">
            <div className="relative h-px flex-1 overflow-hidden bg-gradient-to-r from-primary/30 to-primary/80">
              <span
                aria-hidden
                className="absolute inset-y-[-1px] left-0 w-6 bg-gradient-to-r from-transparent via-white/80 to-transparent opacity-70 animate-travel-x"
                style={{ animationDelay: `${i * 0.45}s` }}
              />
            </div>
            <span className="absolute right-0 top-1/2 -translate-y-1/2">
              <span className="block h-0 w-0 border-y-[4px] border-l-[6px] border-y-transparent border-l-primary" />
            </span>
          </div>
        ))}
        <span className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full border border-border/70 bg-background px-2 py-0.5 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
          direct
        </span>
      </div>

      {/* Providers */}
      <div className="flex shrink-0 flex-col items-end gap-1.5">
        {[
          { icon: <OpenAIIcon className="h-3 w-3" />, name: "openai", bg: "bg-[#10a37f]" },
          { icon: <AnthropicIcon className="h-3 w-3" />, name: "anthropic", bg: "bg-[#cc785c]" },
          { icon: <GoogleIcon className="h-3 w-3" />, name: "gemini", bg: "bg-[#4285f4]" },
          { icon: <Sparkles className="h-3 w-3" />, name: "+ 7 more", bg: "bg-secondary" },
        ].map((p, i) => (
          <div
            key={i}
            className="flex items-center gap-1.5 rounded-md border border-border/60 bg-background/40 px-2 py-1 transition-colors hover:border-primary/30"
          >
            <span
              className={`inline-flex h-4 w-4 items-center justify-center rounded text-white ${p.bg}`}
            >
              {p.icon}
            </span>
            <span className="font-mono text-[10px] text-foreground/85">
              {p.name}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
