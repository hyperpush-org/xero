const steps = [
  {
    n: "01",
    title: "Describe what to build",
    copy: "A sentence or a 10-page spec. Xero asks a handful of clarifying questions up front — then stops interrupting until it actually needs you.",
    sample: "“Build a B2B invoicing SaaS with Stripe, SSO, and a usage dashboard.”",
  },
  {
    n: "02",
    title: "The planner decomposes",
    copy: "Xero drafts a milestone tree, picks a stack, and commits to a plan you can edit before a single file is written.",
    sample: "plan.md · 14 milestones · 6 integrations",
  },
  {
    n: "03",
    title: "Workers build, in parallel",
    copy: "Dozens of tool calls run concurrently on your machine — scaffolding, migrations, tests, Playwright specs.",
    sample: "tokio::spawn × 18 · 4.2s avg",
  },
  {
    n: "04",
    title: "The critic reviews every diff",
    copy: "A second agent reads every change, runs the test suite, and either approves or sends work back with notes.",
    sample: "critic: 2 issues → worker retry",
  },
  {
    n: "05",
    title: "You get pinged, only when needed",
    copy: "Ambiguous tradeoff? Missing secret? Xero messages Discord or Telegram with the context and pauses cleanly.",
    sample: "→ @you: “use Postgres or SQLite?”",
  },
  {
    n: "06",
    title: "Ship — preview, PR, deploy",
    copy: "Xero opens a PR, deploys a preview, and hands you a short changelog. Merge when you're happy.",
    sample: "preview: acme-saas.vercel.app",
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
            From a sentence to a shipped product.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            The loop is deliberately boring — plan, build, review, ship. It&apos;s the
            same thing you&apos;d do, minus the yak&#8209;shaving.
          </p>
        </div>

        <ol className="mt-14 grid grid-cols-1 gap-px overflow-hidden rounded-2xl border border-border/70 bg-border/70 md:grid-cols-2 lg:grid-cols-3">
          {steps.map((s) => (
            <li
              key={s.n}
              className="group relative flex flex-col gap-3 bg-card p-6 transition-colors hover:bg-card/80"
            >
              <div className="flex items-baseline justify-between">
                <span className="font-mono text-[11px] uppercase tracking-[0.2em] text-primary">
                  {s.n}
                </span>
                <span className="h-px flex-1 translate-y-[-1px] bg-border/80 mx-3" />
                <span className="font-mono text-[11px] text-muted-foreground">step</span>
              </div>
              <h3 className="text-lg font-medium tracking-tight">{s.title}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{s.copy}</p>
              <div className="mt-auto rounded-md border border-border/60 bg-background/60 px-3 py-2 font-mono text-[11px] text-muted-foreground">
                {s.sample}
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  )
}
