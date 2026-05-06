import { Check, Cloud, Github, KeyRound, Laptop, Route, Server, Sparkles, Webhook } from "lucide-react"
import {
  AnthropicIcon,
  GoogleIcon,
  OpenAIIcon,
} from "@/components/landing/brand-icons"

type Provider = {
  name: string
  plan: string
  icon: React.ReactNode
  badgeClass: string
}

const providers: Provider[] = [
  {
    name: "OpenAI / Codex",
    plan: "API key, Codex CLI, or ChatGPT plan",
    icon: <OpenAIIcon className="h-4 w-4" />,
    badgeClass: "bg-[#10a37f] text-white",
  },
  {
    name: "Anthropic",
    plan: "API key or Claude plan",
    icon: <AnthropicIcon className="h-4 w-4" />,
    badgeClass: "bg-[#cc785c] text-white",
  },
  {
    name: "Google Gemini",
    plan: "AI Studio API key",
    icon: <GoogleIcon className="h-4 w-4" />,
    badgeClass: "bg-[#4285f4] text-white",
  },
  {
    name: "OpenRouter",
    plan: "One key, hundreds of models",
    icon: <Route className="h-4 w-4" />,
    badgeClass: "border border-border bg-secondary text-foreground",
  },
  {
    name: "GitHub Models",
    plan: "Sign in with GitHub",
    icon: <Github className="h-4 w-4" />,
    badgeClass: "bg-foreground text-background",
  },
  {
    name: "Ollama",
    plan: "Local models, no network",
    icon: <Server className="h-4 w-4" />,
    badgeClass: "border border-primary/30 bg-primary/15 text-primary",
  },
  {
    name: "Azure OpenAI",
    plan: "Enterprise deployments",
    icon: <Cloud className="h-4 w-4" />,
    badgeClass: "bg-[#0078d4] text-white",
  },
  {
    name: "AWS Bedrock",
    plan: "Anthropic, Meta, and more",
    icon: <Cloud className="h-4 w-4" />,
    badgeClass: "bg-[#ff9900] text-white",
  },
  {
    name: "Google Vertex AI",
    plan: "Gemini and partners on GCP",
    icon: <GoogleIcon className="h-4 w-4" />,
    badgeClass: "bg-[#1a73e8] text-white",
  },
  {
    name: "OpenAI-compatible",
    plan: "Groq, xAI, Together, vLLM, LM Studio",
    icon: <Webhook className="h-4 w-4" />,
    badgeClass: "border border-border bg-secondary text-foreground",
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
              Ten providers. Direct from your machine.
            </h2>

            <DirectConnectionVisual />

            <ul className="mt-6 flex flex-col gap-3">
              <li className="flex items-start gap-3 text-sm">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <KeyRound className="h-3 w-3" />
                </span>
                <span className="text-foreground/90">
                  <span className="font-medium">Keys in the OS keychain.</span>{" "}
                  <span className="text-muted-foreground">
                    Redacted in exports.
                  </span>
                </span>
              </li>
              <li className="flex items-start gap-3 text-sm">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <Sparkles className="h-3 w-3" />
                </span>
                <span className="text-foreground/90">
                  <span className="font-medium">Mix models per pane.</span>{" "}
                  <span className="text-muted-foreground">
                    Claude, Gemini, local Ollama, side by side.
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
                  className="group relative flex items-center gap-3 overflow-hidden rounded-xl border border-border/70 bg-card p-4 transition-all hover:-translate-y-0.5 hover:border-border hover:shadow-[0_10px_30px_-18px_rgba(0,0,0,0.6)]"
                >
                  <span
                    className={`inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg ${p.badgeClass}`}
                    aria-hidden
                  >
                    {p.icon}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{p.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {p.plan}
                    </p>
                  </div>
                  <span
                    className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-primary/10 text-primary ring-1 ring-inset ring-primary/20"
                    aria-label="Supported"
                    title="Supported"
                  >
                    <Check className="h-3 w-3" strokeWidth={2.5} />
                  </span>
                </li>
              ))}
            </ul>

            <div className="mt-4 rounded-xl border border-dashed border-border/70 bg-background/40 p-4 font-mono text-[12px] text-muted-foreground">
              <span className="text-primary">$</span> xero providers add anthropic
              <span className="ml-2 text-foreground/70">
                # stored in OS keychain · redacted in doctor reports
              </span>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}

function DirectConnectionVisual() {
  return (
    <div className="relative mt-6 overflow-hidden rounded-xl border border-border/70 bg-background/40 p-4">
      <span
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-px overflow-hidden"
      >
        <span className="block h-full w-1/3 animate-shimmer-bar bg-gradient-to-r from-transparent via-primary/70 to-transparent" />
      </span>
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          <span className="relative inline-flex h-1.5 w-1.5">
            <span className="absolute inline-flex h-full w-full animate-ring-ping rounded-full bg-primary" />
            <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
          </span>
          no relay · no markup
        </div>
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/[0.08] px-2 py-0.5 font-mono text-[9px] uppercase tracking-[0.18em] text-primary">
          <KeyRound className="h-2.5 w-2.5" />
          your keys
        </span>
      </div>

      <div className="mt-5 flex items-center gap-3">
        {/* You */}
        <div className="flex shrink-0 flex-col items-center gap-1.5">
          <span className="relative inline-flex h-11 w-11 items-center justify-center rounded-lg border border-primary/40 bg-primary/[0.08] text-primary shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_8%,transparent)]">
            <Laptop className="h-4 w-4" />
            <span
              aria-hidden
              className="absolute -bottom-1 -right-1 inline-flex h-4 w-4 items-center justify-center rounded-full border border-primary/40 bg-card text-primary"
            >
              <KeyRound className="h-2.5 w-2.5" />
            </span>
          </span>
          <span className="font-mono text-[10px] text-muted-foreground">
            your machine
          </span>
        </div>

        {/* Direct line with traveling pulse */}
        <div className="relative flex h-12 flex-1 items-center">
          <div
            aria-hidden
            className="relative h-px w-full overflow-hidden bg-gradient-to-r from-primary/30 via-primary/70 to-primary"
          >
            <span
              aria-hidden
              className="absolute inset-y-[-1px] left-0 w-10 bg-gradient-to-r from-transparent via-white/85 to-transparent opacity-80 animate-travel-x"
            />
          </div>
          {/* arrowhead */}
          <span
            aria-hidden
            className="absolute right-0 top-1/2 -translate-y-1/2"
          >
            <span className="block h-0 w-0 border-y-[5px] border-l-[7px] border-y-transparent border-l-primary" />
          </span>
          {/* label */}
          <span className="absolute left-1/2 top-0 -translate-x-1/2 rounded-full border border-border/70 bg-background px-2 py-0.5 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
            direct
          </span>
          {/* sub label */}
          <span className="absolute left-1/2 bottom-0 -translate-x-1/2 font-mono text-[9px] tracking-wider text-muted-foreground/60">
            tls · streaming
          </span>
        </div>

        {/* Providers cluster */}
        <div className="flex shrink-0 flex-col items-center gap-1.5">
          <div className="grid grid-cols-3 gap-1 rounded-lg border border-border/60 bg-card p-1.5 shadow-[0_8px_24px_-12px_rgba(0,0,0,0.6)]">
            {[
              "bg-[#10a37f]",
              "bg-[#cc785c]",
              "bg-[#4285f4]",
              "bg-foreground",
              "bg-[#0078d4]",
              "bg-[#ff9900]",
            ].map((c, i) => (
              <span
                key={i}
                className={`h-2.5 w-2.5 rounded-sm ${c} opacity-90`}
                aria-hidden
              />
            ))}
          </div>
          <span className="font-mono text-[10px] text-muted-foreground">
            10 providers
          </span>
        </div>
      </div>
    </div>
  )
}
