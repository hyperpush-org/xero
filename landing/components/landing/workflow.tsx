type Step = {
  n: string
  title: string
  copy: string
  sample: string
  soon?: boolean
}

const steps: Step[] = [
  {
    n: "01",
    title: "Open a project",
    copy: "Point Xero at a folder. Local SQLite-backed state spins up next to it — no cloud round-trip, no remote workspace to provision.",
    sample: "~/code/acme · 6 panes available",
  },
  {
    n: "02",
    title: "Pick or define an agent",
    copy: "Built-in Ask, Engineer, Debug, and Agent Create roles ship with the app. Project- or global-level custom agents extend them.",
    sample: "agents: ask · engineer · debug · solana-ops",
  },
  {
    n: "03",
    title: "Bring a model",
    copy: "Add a key for OpenAI, Anthropic, Gemini, OpenRouter, GitHub Models, Azure, Bedrock, Vertex, or point a session at a local Ollama or OpenAI-compatible endpoint.",
    sample: "binding: claude-opus-4.7 · keychain-backed",
  },
  {
    n: "04",
    title: "Run with real tools",
    copy: "Sessions touch the real machine — repo edits, shell, git, browser automation, mobile emulators, MCP servers, skills, and the Solana workbench.",
    sample: "tool calls: repo · shell · git · browser · mobile · mcp · solana",
  },
  {
    n: "05",
    title: "Pause for real decisions",
    copy: "When the agent hits a tradeoff or wants to take an action you flagged, it pauses cleanly and pings you on Discord or Telegram with the relevant context.",
    sample: "→ approval · push branch to origin?",
  },
  {
    n: "06",
    title: "Branch, rewind, hand off",
    copy: "Compact context manually or automatically, branch off to try an alternative, rewind to a prior checkpoint, or hand off to a different agent type — without losing the thread.",
    sample: "branch `try-pg` · 3 checkpoints · 1 handoff",
  },
  {
    n: "07",
    title: "Stitch sessions into workflows",
    copy: "Visual Agent Builder and Workflow Builder are on the way — long workflows with steps, branches, loops, handoffs, and verification gates. Today you compose by hand; soon, on a canvas.",
    sample: "workflow.compose(steps, branches, gates)",
    soon: true,
  },
]

export function Workflow() {
  return (
    <section id="workflow" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            How a session looks today
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            No magic pipeline. A real agent loop.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Six steps you can audit in the UI. The seventh is the one we&apos;re
            building next, and we&apos;ve labeled it as such.
          </p>
        </div>

        <ol className="mt-14 grid grid-cols-1 gap-px overflow-hidden rounded-2xl border border-border/70 bg-border/70 md:grid-cols-2 lg:grid-cols-3">
          {steps.map((s, i) => (
            <li
              key={s.n}
              className={`group relative flex flex-col gap-3 p-6 transition-colors ${
                s.soon ? "bg-secondary/30 hover:bg-secondary/40" : "bg-card hover:bg-card/60"
              }`}
            >
              <div className="flex items-center gap-3">
                <span className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
                  {s.n}
                </span>
                <span aria-hidden className="h-px flex-1 bg-gradient-to-r from-border/80 via-border/50 to-transparent" />
                {s.soon ? (
                  <span className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/[0.06] px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-primary">
                    <span className="h-1 w-1 rounded-full bg-primary" />
                    Coming soon
                  </span>
                ) : (
                  <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground/70">
                    {i === 0 ? "start" : "step"}
                  </span>
                )}
              </div>
              <h3 className="text-lg font-medium tracking-tight">{s.title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{s.copy}</p>
              <div className="mt-auto rounded-md border border-border/60 bg-background/60 px-3 py-2 font-mono text-[11px] text-muted-foreground transition-colors group-hover:border-border/80">
                {s.sample}
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  )
}
