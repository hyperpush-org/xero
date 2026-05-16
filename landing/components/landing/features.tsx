import {
  Bot,
  Check,
  ChevronRight,
  Loader2,
  Pause,
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
      "Pick each agent's tools, what it remembers, and the moments it has to stop and ask.",
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
    title: "Chain agents into one long run.",
    description:
      "Compose agents into workflows with steps, branches, gates, and loops. Long autonomous runs that hand off cleanly between specialists.",
    bullets: [
      "Steps, branches, loops, gates",
      "Long autonomous runs with clean handoffs",
      "Human checkpoints, only when one matters",
    ],
    visual: <WorkflowFlowVisual />,
  },
  {
    tag: "Solana workbench",
    icon: <SolanaGlyph className="h-3.5 w-3.5" />,
    title: "A Solana workbench your agents can drive.",
    description:
      "Localnet to mainnet on one switch. Funded personas, deploys, and transaction inspection, all native to Xero and available to your agents.",
    bullets: [
      "Localnet · devnet · mainnet, one switch",
      "Funded personas with scenario replays",
      "Tx inspector, deploy, audit, IDL, SPL",
    ],
    visual: <SolanaWorkbenchVisual />,
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
            Built for the parts other agent tools skip.
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

/* 1) Agents you actually design: an HTML inspector panel mirroring how
   you actually configure an agent in Xero. Header (identity + scope), tool
   toggles, memory meters, approval rules. Uses real DOM so typography,
   spacing, and color stay crisp at every viewport. */
function AgentCoreVisual() {
  const tools: { label: string; on: boolean }[] = [
    { label: "shell", on: true },
    { label: "fs", on: true },
    { label: "git", on: true },
    { label: "web", on: true },
    { label: "solana", on: true },
    { label: "rpc", on: true },
    { label: "db", on: false },
    { label: "docker", on: false },
    { label: "k8s", on: false },
  ]

  const approvals: { action: string; verdict: "ASK" | "AUTO" }[] = [
    { action: "git push", verdict: "ASK" },
    { action: "tx send", verdict: "ASK" },
    { action: "mainnet deploy", verdict: "ASK" },
    { action: "repo edit", verdict: "AUTO" },
  ]

  return (
    <div className="relative aspect-[5/4] w-full overflow-hidden">
      {/* Canvas dot grid */}
      <div
        aria-hidden
        className="absolute inset-0 opacity-25"
        style={{
          backgroundImage:
            "radial-gradient(color-mix(in oklab, var(--muted-foreground) 22%, transparent) 1px, transparent 1px)",
          backgroundSize: "20px 20px",
        }}
      />
      {/* Soft top-right primary glow */}
      <div
        aria-hidden
        className="absolute -right-12 -top-12 h-48 w-48 rounded-full opacity-55 blur-3xl"
        style={{
          background:
            "radial-gradient(circle, color-mix(in oklab, var(--primary) 20%, transparent), transparent 70%)",
        }}
      />

      <div className="relative flex h-full flex-col p-7 sm:p-8">
        {/* ---------- Header ---------- */}
        <div className="flex items-center gap-3.5">
          <div className="relative grid h-12 w-12 place-items-center rounded-xl border bg-[color-mix(in_oklab,var(--primary)_10%,var(--card))]"
               style={{ borderColor: "color-mix(in oklab, var(--primary) 50%, transparent)" }}>
            <span className="h-3 w-3 rounded-full bg-primary" />
            <span
              className="pointer-events-none absolute inset-1 rounded-lg border border-dashed"
              style={{ borderColor: "color-mix(in oklab, var(--primary) 40%, transparent)" }}
            />
          </div>
          <div className="min-w-0">
            <div className="font-mono text-[17px] font-semibold leading-tight tracking-tight text-foreground">
              solana-ops
            </div>
            <div className="mt-1.5 font-mono text-[10.5px] leading-none text-muted-foreground">
              Custom agent · claude-opus-4-7
            </div>
          </div>
          <span
            className="ml-auto inline-flex items-center gap-1.5 rounded-full border px-3 py-1 font-mono text-[11px] tracking-wide text-foreground"
            style={{
              borderColor: "color-mix(in oklab, var(--primary) 45%, transparent)",
              backgroundColor: "color-mix(in oklab, var(--primary) 10%, transparent)",
            }}
          >
            <span className="h-1.5 w-1.5 rounded-full bg-primary" />
            project
          </span>
        </div>

        <Divider className="mt-5" />

        {/* ---------- Tools ---------- */}
        <SectionHead label="Tools" right={<span className="text-primary">6 / 12 enabled</span>} />
        <div className="mt-3 grid grid-cols-3 gap-2">
          {tools.map((t) => (
            <ToolChip key={t.label} label={t.label} on={t.on} />
          ))}
        </div>

        <Divider className="mt-5" />

        {/* ---------- Memory ---------- */}
        <SectionHead label="Memory" right="isolated · per-project" />
        <div className="mt-3 space-y-3">
          <Meter label="working" value={72} />
          <Meter label="long-term" value={31} />
        </div>

        <Divider className="mt-5" />

        {/* ---------- Approvals ---------- */}
        <SectionHead
          label="Approvals"
          right={
            <span className="text-primary">
              {approvals.filter((a) => a.verdict === "ASK").length} stop &amp; ask
            </span>
          }
        />
        <div className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2">
          {approvals.map((a) => (
            <ApprovalRow key={a.action} action={a.action} verdict={a.verdict} />
          ))}
        </div>
      </div>
    </div>
  )
}

function Divider({ className = "" }: { className?: string }) {
  return <div className={`h-px w-full bg-border/80 ${className}`} />
}

function SectionHead({
  label,
  right,
}: {
  label: string
  right?: React.ReactNode
}) {
  return (
    <div className="mt-4 flex items-center justify-between">
      <span className="font-mono text-[11px] uppercase tracking-[0.22em] text-muted-foreground">
        {label}
      </span>
      {right ? (
        <span className="font-mono text-[11px] tracking-wide text-muted-foreground">
          {right}
        </span>
      ) : null}
    </div>
  )
}

function ToolChip({ label, on }: { label: string; on: boolean }) {
  return (
    <div
      className={`flex items-center gap-2 rounded-md border px-2.5 py-2 font-mono text-[12.5px] transition-colors ${
        on
          ? "border-primary/45 text-foreground"
          : "border-border/80 text-muted-foreground"
      }`}
      style={{
        backgroundColor: on
          ? "color-mix(in oklab, var(--primary) 8%, transparent)"
          : "transparent",
      }}
    >
      {on ? (
        <Check className="h-3.5 w-3.5 shrink-0 text-primary" strokeWidth={2.5} />
      ) : (
        <span
          aria-hidden
          className="grid h-3.5 w-3.5 shrink-0 place-items-center rounded-full border border-current opacity-55"
        />
      )}
      <span className={on ? "" : "opacity-80"}>{label}</span>
    </div>
  )
}

function Meter({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-[80px] shrink-0 font-mono text-[12.5px] text-foreground/85">
        {label}
      </span>
      <div className="relative h-2 flex-1 overflow-hidden rounded-full bg-border/55">
        <div
          className="absolute inset-y-0 left-0 rounded-full"
          style={{
            width: `${value}%`,
            background:
              "linear-gradient(90deg, color-mix(in oklab, var(--primary) 55%, transparent), var(--primary))",
          }}
        />
      </div>
      <span className="w-10 text-right font-mono text-[11.5px] text-muted-foreground">
        {value}%
      </span>
    </div>
  )
}

function ApprovalRow({
  action,
  verdict,
}: {
  action: string
  verdict: "ASK" | "AUTO"
}) {
  const ask = verdict === "ASK"
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="truncate font-mono text-[13px] text-foreground/90">
        {action}
      </span>
      <span
        className={`inline-flex h-6 min-w-[60px] shrink-0 items-center justify-center rounded-full border px-2.5 font-mono text-[10px] font-bold tracking-[0.15em] ${
          ask ? "text-primary" : "text-muted-foreground"
        }`}
        style={{
          borderColor: ask
            ? "color-mix(in oklab, var(--primary) 55%, transparent)"
            : "color-mix(in oklab, var(--border) 100%, transparent)",
          backgroundColor: ask
            ? "color-mix(in oklab, var(--primary) 14%, transparent)"
            : "transparent",
        }}
      >
        {ask ? "ASK" : "AUTO"}
      </span>
    </div>
  )
}

/* 2) Chain agents: an HTML stepper that reads like an actual workflow
   run. Vertical chain of agents handing off, with status icons, a human
   checkpoint, and a loop-back annotation. */
function WorkflowFlowVisual() {
  type StepStatus = "done" | "running" | "checkpoint" | "queued"
  type Step = {
    agent: string
    desc: string
    status: StepStatus
    meta?: string
  }

  const steps: Step[] = [
    {
      agent: "planner-agent",
      desc: "Read spec · split into 4 PRs",
      status: "done",
      meta: "2m 14s",
    },
    {
      agent: "engineer-agent",
      desc: "Building PR 2 of 4",
      status: "running",
      meta: "12m elapsed",
    },
    {
      agent: "approval-gate",
      desc: "Review migration before merge",
      status: "checkpoint",
      meta: "needs you",
    },
    {
      agent: "test-agent",
      desc: "Queued · retries from engineer on fail",
      status: "queued",
    },
    {
      agent: "ship-agent",
      desc: "Queued",
      status: "queued",
    },
  ]

  return (
    <div className="relative aspect-[6/5] w-full overflow-hidden">
      <div
        aria-hidden
        className="absolute inset-0 opacity-25"
        style={{
          backgroundImage:
            "radial-gradient(color-mix(in oklab, var(--muted-foreground) 22%, transparent) 1px, transparent 1px)",
          backgroundSize: "20px 20px",
        }}
      />
      <div
        aria-hidden
        className="absolute -right-16 top-1/4 h-56 w-56 rounded-full opacity-50 blur-3xl"
        style={{
          background:
            "radial-gradient(circle, color-mix(in oklab, var(--primary) 20%, transparent), transparent 70%)",
        }}
      />

      <div className="relative flex h-full flex-col p-6 sm:p-7">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <div className="font-mono text-[9.5px] uppercase tracking-[0.22em] text-muted-foreground">
              Workflow
            </div>
            <div className="mt-1 font-mono text-[15px] font-semibold leading-tight text-foreground">
              ship-feature
            </div>
          </div>
          <span
            className="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-mono text-[10px] tracking-wide text-foreground"
            style={{
              borderColor: "color-mix(in oklab, var(--primary) 45%, transparent)",
              backgroundColor: "color-mix(in oklab, var(--primary) 10%, transparent)",
            }}
          >
            <span className="relative inline-flex h-1.5 w-1.5">
              <span className="absolute inset-0 animate-ping rounded-full bg-primary opacity-60" />
              <span className="relative h-1.5 w-1.5 rounded-full bg-primary" />
            </span>
            running · step 2 of 5
          </span>
        </div>

        <div className="mt-4 h-px w-full bg-border/80" />

        {/* Run stats strip */}
        <div className="mt-4 grid grid-cols-4 gap-2">
          <RunStat label="Elapsed" value="14m" sub="of est 38m" />
          <RunStat label="Cost" value="$1.83" sub="opus · sonnet" />
          <RunStat label="Tokens" value="124k" sub="↑ 31k  ↓ 93k" />
          <RunStat label="Retries" value="2" sub="auto recovered" tone="muted" />
        </div>

        {/* Stepper */}
        <div className="relative mt-4 flex-1">
          {/* connector rail behind icons */}
          <div
            aria-hidden
            className="absolute left-3 top-3 bottom-3 w-px"
            style={{
              background:
                "linear-gradient(to bottom, color-mix(in oklab, var(--primary) 45%, transparent), color-mix(in oklab, var(--primary) 45%, transparent) 32%, color-mix(in oklab, var(--border) 90%, transparent) 60%, color-mix(in oklab, var(--border) 60%, transparent))",
            }}
          />
          <ol className="relative flex flex-col gap-3">
            {steps.map((s) => (
              <StepRow key={s.agent} step={s} />
            ))}
          </ol>
        </div>

        {/* Footer: loop annotation */}
        <div className="mt-3 flex items-center gap-2 font-mono text-[10px] text-muted-foreground">
          <svg viewBox="0 0 24 12" className="h-2.5 w-6" aria-hidden>
            <path
              d="M 1 6 Q 12 14 23 6"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
              strokeDasharray="2 2"
            />
            <path
              d="M 20 3 L 23 6 L 20 9"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span>loop · retries continue until tests pass</span>
        </div>
      </div>
    </div>
  )
}

function RunStat({
  label,
  value,
  sub,
  tone = "primary",
}: {
  label: string
  value: string
  sub: string
  tone?: "primary" | "muted"
}) {
  return (
    <div className="rounded-md border border-border/55 px-2.5 py-1.5">
      <div className="font-mono text-[8.5px] uppercase tracking-[0.18em] text-muted-foreground">
        {label}
      </div>
      <div
        className={`mt-1 font-mono text-[13px] font-semibold leading-none tabular-nums ${
          tone === "primary" ? "text-foreground" : "text-muted-foreground"
        }`}
      >
        {value}
      </div>
      <div className="mt-1 truncate font-mono text-[9px] text-muted-foreground/80">
        {sub}
      </div>
    </div>
  )
}

function StepRow({
  step,
}: {
  step: {
    agent: string
    desc: string
    status: "done" | "running" | "checkpoint" | "queued"
    meta?: string
  }
}) {
  const isQueued = step.status === "queued"
  return (
    <li className="flex items-start gap-3">
      <StepIcon status={step.status} />
      <div
        className={`min-w-0 flex-1 rounded-lg border px-3 py-2 transition-colors ${
          step.status === "checkpoint"
            ? "border-primary/45"
            : step.status === "running"
              ? "border-primary/35"
              : "border-border/70"
        }`}
        style={{
          backgroundColor:
            step.status === "checkpoint"
              ? "color-mix(in oklab, var(--primary) 10%, transparent)"
              : step.status === "running"
                ? "color-mix(in oklab, var(--primary) 6%, transparent)"
                : "transparent",
        }}
      >
        <div className="flex items-baseline justify-between gap-3">
          <span
            className={`truncate font-mono text-[12.5px] font-semibold leading-none tracking-tight ${
              isQueued ? "text-muted-foreground" : "text-foreground"
            }`}
          >
            {step.agent}
          </span>
          {step.meta ? (
            <span
              className={`shrink-0 font-mono text-[10px] tracking-wide ${
                step.status === "checkpoint" || step.status === "running"
                  ? "text-primary"
                  : "text-muted-foreground"
              }`}
            >
              {step.meta}
            </span>
          ) : null}
        </div>
        <div
          className={`mt-1 truncate font-mono text-[10.5px] leading-snug ${
            isQueued ? "text-muted-foreground/55" : "text-muted-foreground"
          }`}
        >
          {step.desc}
        </div>
      </div>
    </li>
  )
}

function StepIcon({
  status,
}: {
  status: "done" | "running" | "checkpoint" | "queued"
}) {
  if (status === "done") {
    return (
      <span
        className="relative z-10 grid h-6 w-6 shrink-0 place-items-center rounded-full border"
        style={{
          borderColor: "color-mix(in oklab, var(--primary) 60%, transparent)",
          backgroundColor: "color-mix(in oklab, var(--primary) 22%, var(--card))",
        }}
      >
        <Check className="h-3 w-3 text-primary" strokeWidth={3} />
      </span>
    )
  }
  if (status === "running") {
    return (
      <span
        className="relative z-10 grid h-6 w-6 shrink-0 place-items-center rounded-full border bg-card"
        style={{
          borderColor: "var(--primary)",
          boxShadow:
            "0 0 0 3px color-mix(in oklab, var(--primary) 22%, transparent)",
        }}
      >
        <Loader2 className="h-3 w-3 animate-spin text-primary" strokeWidth={2.5} />
      </span>
    )
  }
  if (status === "checkpoint") {
    return (
      <span
        className="relative z-10 grid h-6 w-6 shrink-0 place-items-center rounded-full border"
        style={{
          borderColor: "color-mix(in oklab, var(--primary) 70%, transparent)",
          backgroundColor: "color-mix(in oklab, var(--primary) 25%, var(--card))",
        }}
      >
        <Pause className="h-3 w-3 fill-primary text-primary" strokeWidth={0} />
      </span>
    )
  }
  return (
    <span
      className="relative z-10 grid h-6 w-6 shrink-0 place-items-center rounded-full border bg-card"
      style={{ borderColor: "color-mix(in oklab, var(--border) 100%, transparent)" }}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/45" />
    </span>
  )
}

/* 3) Solana workbench: an HTML "command center" panel showing the
   workbench bits: cluster status, funded test personas with balances, and
   a live transaction feed with status. */
function SolanaWorkbenchVisual() {
  type Trend = "up" | "down" | "flat"
  const personas: { name: string; role: string; balance: string; trend: Trend }[] = [
    { name: "alice", role: "payer", balance: "12.4", trend: "up" },
    { name: "bob", role: "buyer", balance: "8.10", trend: "flat" },
    { name: "mallory", role: "attacker", balance: "0.52", trend: "down" },
  ]

  const txs: { sig: string; program: string; status: "ok" | "err"; time: string }[] = [
    { sig: "5KqRz…7Pq2", program: "initialize_pool", status: "ok", time: "+0.03s" },
    { sig: "8aJpX…cFm4", program: "swap", status: "ok", time: "+0.05s" },
    { sig: "9aRtY…dE5h", program: "route_hop", status: "ok", time: "+0.12s" },
    { sig: "2mLnK…aB1d", program: "withdraw", status: "err", time: "+0.18s" },
  ]

  return (
    <div className="relative aspect-[6/5] w-full overflow-hidden">
      <div
        aria-hidden
        className="absolute inset-0 opacity-25"
        style={{
          backgroundImage:
            "radial-gradient(color-mix(in oklab, var(--muted-foreground) 22%, transparent) 1px, transparent 1px)",
          backgroundSize: "20px 20px",
        }}
      />
      <div
        aria-hidden
        className="absolute -left-16 -top-12 h-56 w-56 rounded-full opacity-50 blur-3xl"
        style={{
          background:
            "radial-gradient(circle, color-mix(in oklab, var(--primary) 20%, transparent), transparent 70%)",
        }}
      />

      <div className="relative flex h-full flex-col p-7 sm:p-8">
        {/* Header */}
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3.5">
            <div
              className="grid h-12 w-12 shrink-0 place-items-center rounded-xl border"
              style={{
                borderColor: "color-mix(in oklab, var(--primary) 50%, transparent)",
                backgroundColor:
                  "color-mix(in oklab, var(--primary) 10%, var(--card))",
              }}
            >
              <SolanaGlyph className="h-[18px] w-[18px] text-primary" />
            </div>
            <div className="min-w-0">
              <div className="truncate font-mono text-[16px] font-semibold leading-tight tracking-tight text-foreground">
                Solana workbench
              </div>
              <div className="mt-1.5 font-mono text-[10.5px] uppercase leading-none tracking-[0.22em] text-muted-foreground">
                Cluster · personas · deploys
              </div>
            </div>
          </div>
          <span
            className="inline-flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1 font-mono text-[11px] tracking-wide text-foreground"
            style={{
              borderColor: "color-mix(in oklab, var(--primary) 45%, transparent)",
              backgroundColor:
                "color-mix(in oklab, var(--primary) 10%, transparent)",
            }}
          >
            <span className="relative inline-flex h-1.5 w-1.5">
              <span className="absolute inset-0 animate-ping rounded-full bg-primary opacity-60" />
              <span className="relative h-1.5 w-1.5 rounded-full bg-primary" />
            </span>
            localnet · 47ms
          </span>
        </div>

        <div className="mt-5 h-px w-full bg-border/80" />

        {/* Personas */}
        <SectionHead
          label="Personas"
          right={<span className="text-primary">3 of 6 funded</span>}
        />
        <div className="mt-3 space-y-2">
          {personas.map((p) => (
            <PersonaRow key={p.name} {...p} />
          ))}
        </div>

        <Divider className="mt-5" />

        {/* Live tx */}
        <SectionHead label="Live tx" right={<span>slot 273M · +200/s</span>} />
        <div className="mt-3 space-y-2.5">
          {txs.map((t) => (
            <TxRow key={t.sig} {...t} />
          ))}
        </div>

        {/* Toolchain footer */}
        <div className="mt-auto flex items-center justify-between gap-3 pt-5 font-mono text-[11px] text-muted-foreground">
          <div className="flex min-w-0 items-center gap-2">
            <span className="text-foreground/80">$</span>
            <span className="truncate">anchor 0.31</span>
            <span className="text-muted-foreground/50">·</span>
            <span className="truncate">solana-cli 1.18</span>
            <span className="text-muted-foreground/50">·</span>
            <span className="truncate">spl 6.0</span>
          </div>
          <span className="inline-flex shrink-0 items-center gap-1.5 text-primary">
            <span className="h-1.5 w-1.5 rounded-full bg-primary" />
            toolchain ready
          </span>
        </div>
      </div>
    </div>
  )
}

function PersonaRow({
  name,
  role,
  balance,
  trend,
}: {
  name: string
  role: string
  balance: string
  trend: "up" | "down" | "flat"
}) {
  const trendArrow = trend === "up" ? "▲" : trend === "down" ? "▼" : "·"
  const trendClass =
    trend === "up"
      ? "text-primary"
      : trend === "down"
        ? "text-destructive/85"
        : "text-muted-foreground/60"

  return (
    <div className="flex items-center gap-3 rounded-md border border-border/50 px-3 py-2">
      <span
        className="grid h-6 w-6 shrink-0 place-items-center rounded-full border font-mono text-[11px] font-semibold text-primary"
        style={{
          borderColor: "color-mix(in oklab, var(--primary) 50%, transparent)",
          backgroundColor:
            "color-mix(in oklab, var(--primary) 12%, transparent)",
        }}
      >
        {name[0].toUpperCase()}
      </span>
      <span className="font-mono text-[13px] font-medium text-foreground/90">
        {name}
      </span>
      <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
        {role}
      </span>
      <span className="ml-auto font-mono text-[12.5px] tabular-nums text-foreground">
        {balance}{" "}
        <span className="text-[11px] text-muted-foreground/80">SOL</span>
      </span>
      <span className={`w-4 text-center font-mono text-[12px] ${trendClass}`}>
        {trendArrow}
      </span>
    </div>
  )
}

function TxRow({
  sig,
  program,
  status,
  time,
}: {
  sig: string
  program: string
  status: "ok" | "err"
  time: string
}) {
  const ok = status === "ok"
  return (
    <div className="flex items-center gap-3">
      <ChevronRight
        className={`h-3.5 w-3.5 shrink-0 ${ok ? "text-primary/75" : "text-destructive/80"}`}
        strokeWidth={2.5}
      />
      <span className="font-mono text-[12px] tabular-nums text-muted-foreground">
        {sig}
      </span>
      <span className="truncate font-mono text-[12.5px] font-medium text-foreground/90">
        {program}
      </span>
      <span
        className={`ml-auto inline-flex h-5 shrink-0 items-center justify-center rounded-full border px-2.5 font-mono text-[9.5px] font-bold tracking-[0.18em] ${
          ok ? "text-primary" : "text-destructive/90"
        }`}
        style={{
          borderColor: ok
            ? "color-mix(in oklab, var(--primary) 55%, transparent)"
            : "color-mix(in oklab, var(--destructive) 60%, transparent)",
          backgroundColor: ok
            ? "color-mix(in oklab, var(--primary) 14%, transparent)"
            : "color-mix(in oklab, var(--destructive) 14%, transparent)",
        }}
      >
        {ok ? "OK" : "ERR"}
      </span>
      <span className="w-14 shrink-0 text-right font-mono text-[11px] tabular-nums text-muted-foreground">
        {time}
      </span>
    </div>
  )
}

function SolanaGlyph({ className = "" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      fill="currentColor"
      className={className}
      aria-hidden
    >
      <path d="M23.876 18.362l-4.017 4.326a.93.93 0 01-.723.31H.452a.452.452 0 01-.33-.764l4.021-4.325a.93.93 0 01.72-.311h18.686a.452.452 0 01.328.764zM19.859 9.648a.93.93 0 00-.723-.31H.452a.452.452 0 00-.33.763l4.021 4.325a.93.93 0 00.72.31h18.686a.452.452 0 00.328-.764L19.859 9.65zM.452 6.574h18.684a.93.93 0 00.723-.31l4.017-4.326A.452.452 0 0023.6 1.175H4.915a.93.93 0 00-.72.31L.178 5.811a.452.452 0 00.274.763z" />
    </svg>
  )
}
