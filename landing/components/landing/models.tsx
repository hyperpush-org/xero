import { Check, Cloud, Github, KeyRound, Route, Server, Sparkles, Webhook } from "lucide-react"
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
    plan: "API key",
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
        <div className="mb-10 max-w-2xl">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Bring your own model
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-4xl lg:text-5xl">
            Ten providers. Direct from your machine.
          </h2>
        </div>

        <div>
          <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {providers.map((p) => (
                <li
                  key={p.name}
                  className="relative flex items-center gap-3 overflow-hidden rounded-xl border border-border/70 bg-card p-4"
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
        </div>
      </div>
    </section>
  )
}
