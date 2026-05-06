import type { Metadata } from "next"
import Link from "next/link"
import { ArrowRight, Sparkles } from "lucide-react"
import { SiteHeader } from "@/components/landing/site-header"
import { SiteFooter } from "@/components/landing/site-footer"

export const metadata: Metadata = {
  title: "Changelog — Xero",
  description:
    "What just shipped in Xero — agent runtime, providers, mobile approvals, workflow editor, and persistence.",
}

type Entry = {
  version: string
  date: string
  tag: "Build" | "Beta" | "Preview"
  title: string
  highlights: string[]
}

const entries: Entry[] = [
  {
    version: "0.9.0",
    date: "2026-04-22",
    tag: "Beta",
    title: "Agent Create + workflow editor",
    highlights: [
      "Agent Create: a built-in agent that scaffolds new custom agents from a one-line brief",
      "Workflow editor: drag, wire, and gate agents into a runnable graph",
      "Per-step model pinning — different model per node, no global override needed",
      "Loop nodes with stop conditions on token budget, file count, or manual halt",
    ],
  },
  {
    version: "0.8.0",
    date: "2026-03-30",
    tag: "Beta",
    title: "Mobile approvals out of preview",
    highlights: [
      "Discord and Telegram approvals now ship the actual diff or command, not a generic prompt",
      "Per-tool approval rules — pick which actions auto-run and which wait, per session",
      "Reply with a redirect and the agent re-plans on the spot",
      "Notification batching when several gates fire in the same minute",
    ],
  },
  {
    version: "0.7.0",
    date: "2026-03-04",
    tag: "Beta",
    title: "Six-pane workspace + branch & rewind",
    highlights: [
      "Up to six panes per project — mix roles and models freely",
      "Branch any session without overwriting siblings",
      "Rewind to any checkpoint; compaction survives the rewind",
      "Run timeline now records every call, change, and approval inline",
    ],
  },
  {
    version: "0.6.0",
    date: "2026-02-11",
    tag: "Preview",
    title: "Provider expansion",
    highlights: [
      "GitHub Models, Azure OpenAI, AWS Bedrock, and Vertex AI added",
      "Local Ollama now first-class for fully offline runs",
      "OS keychain storage — keys never written in plain text on disk",
      "Provider router picks the strongest available model per agent role",
    ],
  },
  {
    version: "0.5.0",
    date: "2026-01-15",
    tag: "Preview",
    title: "Solana + MCP tools",
    highlights: [
      "Solana toolset: RPC, indexer, fork helpers, deploy guardrails (preview)",
      "MCP tool integration — bring any MCP server into an agent's tool list",
      "Browser tool with site-scoped permissions",
      "Per-agent memory budgets and journal compaction tuning",
    ],
  },
  {
    version: "0.4.0",
    date: "2025-12-09",
    tag: "Build",
    title: "Rust runtime rewrite",
    highlights: [
      "Agent runtime, harness, and persistence rewritten from the ground up in Rust",
      "Cold-start latency cut by ~3x on a fresh project",
      "Memory-safer plugin host — no more stray child processes after a crash",
      "Crash-only journal with replay; sessions survive a hard kill",
    ],
  },
]

const tagStyles: Record<Entry["tag"], string> = {
  Build:
    "border-border/70 bg-secondary/60 text-muted-foreground",
  Beta:
    "border-primary/40 bg-primary/15 text-primary",
  Preview:
    "border-border/70 bg-background text-muted-foreground",
}

export default function ChangelogPage() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <SiteHeader />
      <main>
        <section className="relative isolate overflow-hidden">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(ellipse_at_top,black_30%,transparent_70%)] opacity-[0.3]"
          />
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-radial-fade"
          />
          <div className="mx-auto w-full max-w-4xl px-4 pt-20 pb-10 sm:px-6 sm:pt-28 lg:px-8">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Changelog
            </p>
            <h1 className="mt-3 font-sans text-4xl font-medium tracking-tight text-balance sm:text-5xl">
              What just shipped.
            </h1>
            <p className="mt-4 max-w-2xl text-pretty text-muted-foreground">
              A running list of releases. Versions cut on a normal cadence; entries are written
              once the build is on the public download.
            </p>
          </div>
        </section>

        <div
          aria-hidden
          className="mx-auto h-px w-full max-w-7xl bg-gradient-to-r from-transparent via-border/80 to-transparent"
        />

        <section className="relative">
          <div className="mx-auto w-full max-w-4xl px-4 py-14 sm:px-6 lg:px-8 lg:py-20">
            <ol className="relative flex flex-col gap-12 border-l border-border/60 pl-8">
              {entries.map((e) => (
                <li key={e.version} className="relative">
                  <span
                    aria-hidden
                    className="absolute -left-[37px] top-1.5 inline-flex h-3 w-3 items-center justify-center rounded-full border border-primary/40 bg-background"
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                  </span>
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-mono text-sm font-semibold tracking-tight text-foreground">
                      v{e.version}
                    </span>
                    <span
                      className={`inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${tagStyles[e.tag]}`}
                    >
                      {e.tag}
                    </span>
                    <span className="font-mono text-xs text-muted-foreground/70">{e.date}</span>
                  </div>
                  <h2 className="mt-2 font-sans text-xl font-medium tracking-tight sm:text-2xl">
                    {e.title}
                  </h2>
                  <ul className="mt-3 flex flex-col gap-2 text-sm text-muted-foreground">
                    {e.highlights.map((h) => (
                      <li key={h} className="flex gap-2.5">
                        <Sparkles className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary/70" />
                        <span>{h}</span>
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ol>

            <div className="mt-16 rounded-2xl border border-border/60 bg-secondary/20 px-6 py-6">
              <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                Heads up
              </p>
              <h3 className="mt-2 font-sans text-xl font-medium tracking-tight">
                Coming up
              </h3>
              <p className="mt-2 text-sm text-muted-foreground">
                Pro and Solana Pro plans are next — cloud runtime, sync, hosted run history, and
                managed Solana infra. Until they ship, the desktop app is the whole product.
              </p>
              <div className="mt-4">
                <Link
                  href="/#pricing"
                  className="inline-flex items-center gap-1.5 text-sm text-foreground transition-colors hover:text-primary"
                >
                  Join the waitlist
                  <ArrowRight className="h-3.5 w-3.5" />
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>
      <SiteFooter />
    </div>
  )
}
