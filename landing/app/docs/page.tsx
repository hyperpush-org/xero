import type { Metadata } from "next"
import Link from "next/link"
import { ArrowRight, BookOpen, Cpu, KeyRound, Layers, Smartphone, Wrench } from "lucide-react"
import { SiteHeader } from "@/components/landing/site-header"
import { SiteFooter } from "@/components/landing/site-footer"

export const metadata: Metadata = {
  title: "Docs — Xero",
  description:
    "Install Xero, bring your provider keys, build a custom agent, compose a workflow, and approve from your phone.",
}

type Section = {
  id: string
  label: string
  title: string
  icon: React.ReactNode
  body: React.ReactNode
}

const sections: Section[] = [
  {
    id: "install",
    label: "Install",
    title: "Get Xero running",
    icon: <Cpu className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          Xero is a native desktop app written in Rust. It runs locally on macOS, Windows, and Linux —
          no account, no cloud round-trip on the hot path.
        </p>
        <ol>
          <li>
            Download the build for your OS from{" "}
            <Link href="/#download" className="underline underline-offset-2 hover:text-foreground">
              the download section
            </Link>
            .
          </li>
          <li>Open the app. The first launch creates your local workspace and key store.</li>
          <li>Pick a project directory — Xero only touches paths you point it at.</li>
        </ol>
        <p className="text-muted-foreground/80">
          macOS 13+, Windows 10+, Linux (.deb / .rpm). Apple silicon and Intel are both universal.
        </p>
      </>
    ),
  },
  {
    id: "providers",
    label: "Providers",
    title: "Bring your own keys",
    icon: <KeyRound className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          Xero talks to model providers directly — your keys, your billing, your rate limits.
          Keys live in the OS keychain, never in plain text on disk.
        </p>
        <ul>
          <li>Anthropic (Claude) · OpenAI · Google (Gemini) · OpenRouter</li>
          <li>GitHub Models · Azure OpenAI · AWS Bedrock · Vertex AI</li>
          <li>Local Ollama for fully offline runs</li>
        </ul>
        <p>
          Open <span className="font-mono text-xs text-foreground">Settings → Providers</span>,
          paste a key, and Xero will pick the strongest available model for each role.
        </p>
      </>
    ),
  },
  {
    id: "agents",
    label: "Agents",
    title: "Design a custom agent",
    icon: <Wrench className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          Every agent is a small contract: a role prompt, a tool list, a memory budget, and an
          approval policy. Start from a built-in (Ask, Engineer, Debug, Agent Create) or fork one.
        </p>
        <ul>
          <li>
            <strong className="text-foreground">Tools.</strong> Repo, shell, git, browser, MCP,
            Solana, mobile pings — pick only what the role needs.
          </li>
          <li>
            <strong className="text-foreground">Memory.</strong> Per-agent journals branch and
            rewind without overwriting siblings.
          </li>
          <li>
            <strong className="text-foreground">Approvals.</strong> Decide which actions auto-run
            and which wait — per session, per tool.
          </li>
        </ul>
      </>
    ),
  },
  {
    id: "workflows",
    label: "Workflows",
    title: "Compose a workflow",
    icon: <Layers className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          A workflow chains agents into a graph: steps, branches, loops, gates. Drop a node, wire
          its inputs, and pin the model you want for that step.
        </p>
        <ul>
          <li>Branch on diff size, test result, or arbitrary predicate</li>
          <li>Loop with a stop condition — token budget, file count, manual halt</li>
          <li>Gate any node behind a Discord or Telegram approval</li>
        </ul>
      </>
    ),
  },
  {
    id: "mobile",
    label: "Mobile",
    title: "Approve from your phone",
    icon: <Smartphone className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          When an agent reaches a gate, Xero posts the diff or command to Discord or Telegram and
          waits for a one-line reply. The session keeps running once you approve.
        </p>
        <ol>
          <li>
            <span className="font-mono text-xs text-foreground">Settings → Notifications</span> —
            connect a Discord webhook or Telegram bot token.
          </li>
          <li>Pick which tools require approval. Read-only calls usually shouldn&apos;t.</li>
          <li>Approve, reject, or reply with a redirect — the agent re-plans on the spot.</li>
        </ol>
      </>
    ),
  },
  {
    id: "rewind",
    label: "Persistence",
    title: "Branch, rewind, hand off",
    icon: <BookOpen className="h-3.5 w-3.5" />,
    body: (
      <>
        <p>
          Every project has a local journal. Branch a session to try something risky, rewind to any
          checkpoint, or hand a thread to a different agent without losing context.
        </p>
        <ul>
          <li>Up to six panes per project, mix roles and models freely</li>
          <li>Compact noisy threads with a summary that survives the rewind</li>
          <li>Export or replay any session locally</li>
        </ul>
      </>
    ),
  },
]

export default function DocsPage() {
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
          <div className="mx-auto w-full max-w-5xl px-4 pt-20 pb-10 sm:px-6 sm:pt-28 lg:px-8">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Documentation
            </p>
            <h1 className="mt-3 font-sans text-4xl font-medium tracking-tight text-balance sm:text-5xl">
              Build the agent. Ship the project.
            </h1>
            <p className="mt-4 max-w-2xl text-pretty text-muted-foreground">
              A focused walkthrough of the Xero desktop app — install, providers, custom agents,
              workflows, and mobile approvals. Deep references and the API surface land with v1.
            </p>
            <div className="mt-6 flex flex-wrap items-center gap-2">
              {sections.map((s) => (
                <Link
                  key={s.id}
                  href={`#${s.id}`}
                  className="inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-secondary/40 px-3 py-1 font-mono text-[11px] text-muted-foreground transition-colors hover:border-border hover:bg-secondary/70 hover:text-foreground"
                >
                  <span className="text-primary">{s.icon}</span>
                  {s.label}
                </Link>
              ))}
            </div>
          </div>
        </section>

        <div
          aria-hidden
          className="mx-auto h-px w-full max-w-7xl bg-gradient-to-r from-transparent via-border/80 to-transparent"
        />

        <section className="relative">
          <div className="mx-auto w-full max-w-5xl px-4 py-14 sm:px-6 lg:px-8 lg:py-20">
            <div className="flex flex-col gap-16">
              {sections.map((s) => (
                <article key={s.id} id={s.id} className="scroll-mt-24">
                  <div className="inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-secondary/40 px-2.5 py-1 font-mono text-[11px] text-muted-foreground">
                    <span className="text-primary">{s.icon}</span>
                    {s.label}
                  </div>
                  <h2 className="mt-4 font-sans text-2xl font-medium tracking-tight text-balance sm:text-3xl">
                    {s.title}
                  </h2>
                  <div className="prose-doc mt-4 max-w-3xl space-y-4 text-pretty leading-relaxed text-muted-foreground">
                    {s.body}
                  </div>
                </article>
              ))}
            </div>

            <div className="mt-16 rounded-2xl border border-border/60 bg-secondary/20 px-6 py-6">
              <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                Need more?
              </p>
              <h3 className="mt-2 font-sans text-xl font-medium tracking-tight">
                Talk to the team
              </h3>
              <p className="mt-2 text-sm text-muted-foreground">
                Hit a wall, want a feature, or running Xero across a team? Email{" "}
                <Link
                  href="mailto:team@xeroshell.com"
                  className="underline underline-offset-2 hover:text-foreground"
                >
                  team@xeroshell.com
                </Link>{" "}
                — we read every message.
              </p>
              <div className="mt-4">
                <Link
                  href="/changelog"
                  className="inline-flex items-center gap-1.5 text-sm text-foreground transition-colors hover:text-primary"
                >
                  See what just shipped
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
