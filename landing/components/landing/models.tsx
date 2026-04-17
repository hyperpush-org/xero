import { KeyRound, Sparkles } from "lucide-react"

type Provider = {
  name: string
  plan: string
  mark: string // short monogram for the badge
  badgeClass: string
}

const providers: Provider[] = [
  {
    name: "ChatGPT / Codex",
    plan: "Plus · Pro · Team plans",
    mark: "AI",
    badgeClass: "bg-[#10a37f] text-white",
  },
  {
    name: "Claude",
    plan: "Pro · Max · Team plans",
    mark: "Cl",
    badgeClass: "bg-[#cc785c] text-white",
  },
  {
    name: "GitHub Copilot",
    plan: "Individual · Business",
    mark: "GH",
    badgeClass: "bg-foreground text-background",
  },
  {
    name: "OpenRouter",
    plan: "Unified billing across 200+ models",
    mark: "OR",
    badgeClass: "bg-secondary text-foreground border border-border",
  },
  {
    name: "Google Gemini",
    plan: "AI Pro · Ultra subscriptions",
    mark: "Gm",
    badgeClass: "bg-[#4285f4] text-white",
  },
  {
    name: "Any OpenAI-compatible API",
    plan: "Ollama · Groq · xAI · Together · vLLM",
    mark: "···",
    badgeClass: "bg-primary/15 text-primary border border-primary/30",
  },
]

export function Models() {
  return (
    <section
      id="models"
      className="relative border-y border-border/60 bg-secondary/[0.12]"
    >
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-24">
        <div className="grid grid-cols-1 gap-10 lg:grid-cols-12 lg:gap-16">
          <div className="lg:col-span-5">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Bring your own model
            </p>
            <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-4xl lg:text-5xl">
              Works with the subscriptions you already pay for.
            </h2>
            <p className="mt-5 text-pretty leading-relaxed text-muted-foreground">
              Cadence is model-agnostic. Sign in with your ChatGPT, Claude, or Copilot
              plan, paste an OpenRouter key, or point it at a self-hosted endpoint. Your
              keys live in the OS keychain — never on our servers.
            </p>

            <ul className="mt-6 flex flex-col gap-3">
              <li className="flex items-start gap-3 text-sm">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <KeyRound className="h-3 w-3" />
                </span>
                <span className="text-foreground/90">
                  <span className="font-medium">No extra vendor lock-in.</span>{" "}
                  <span className="text-muted-foreground">
                    Keep using the plan your team already has.
                  </span>
                </span>
              </li>
              <li className="flex items-start gap-3 text-sm">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <Sparkles className="h-3 w-3" />
                </span>
                <span className="text-foreground/90">
                  <span className="font-medium">Mix models per agent.</span>{" "}
                  <span className="text-muted-foreground">
                    Route the planner to Opus, workers to a fast Sonnet / GPT-5-mini,
                    critic to whichever you trust most.
                  </span>
                </span>
              </li>
            </ul>
          </div>

          <div className="lg:col-span-7">
            <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {providers.map((p) => (
                <li
                  key={p.name}
                  className="group flex items-center gap-3 rounded-xl border border-border/70 bg-card p-4 transition-colors hover:border-border"
                >
                  <span
                    className={`inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg font-mono text-sm font-semibold ${p.badgeClass}`}
                    aria-hidden
                  >
                    {p.mark}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{p.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {p.plan}
                    </p>
                  </div>
                  <span className="rounded-md border border-primary/30 bg-primary/10 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-primary">
                    ready
                  </span>
                </li>
              ))}
            </ul>

            <div className="mt-4 rounded-xl border border-dashed border-border/70 bg-background/40 p-4 font-mono text-[12px] text-muted-foreground">
              <span className="text-primary">$</span> cadence auth add anthropic
              <span className="text-primary"> --plan=max</span>
              <span className="ml-2 text-foreground/70">
                # stored securely in macOS keychain
              </span>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
