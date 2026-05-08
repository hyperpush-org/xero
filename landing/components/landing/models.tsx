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

/* Constellation: a single point (you) at center casts direct rays to a
   surrounding ring of provider points. No middleman, no relay node. */
function DirectConnectionVisual() {
  const cx = 200
  const cy = 110
  const ringR = 78
  const providers = [
    { c: "#10a37f", deg: 0 },
    { c: "#cc785c", deg: 36 },
    { c: "#4285f4", deg: 72 },
    { c: "#0078d4", deg: 108 },
    { c: "#ff9900", deg: 144 },
    { c: "#1a73e8", deg: 180 },
    { c: "#f8f9fa", deg: 216 },
    { c: "var(--primary)", deg: 252 },
    { c: "#cc785c", deg: 288 },
    { c: "#10a37f", deg: 324 },
  ]
  const pt = (deg: number, r: number) => {
    const a = ((deg - 90) * Math.PI) / 180
    return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) }
  }

  return (
    <div className="relative mt-6 overflow-hidden rounded-xl border border-border/70 bg-background/40">
      {/* Header chrome */}
      <div className="flex items-center justify-between border-b border-border/60 px-4 py-2.5">
        <div className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          <span className="inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
          no relay · no markup
        </div>
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/[0.08] px-2 py-0.5 font-mono text-[9px] uppercase tracking-[0.18em] text-primary">
          <KeyRound className="h-2.5 w-2.5" />
          your keys
        </span>
      </div>

      <svg viewBox="0 0 400 220" className="h-auto w-full" aria-hidden>
        <defs>
          <radialGradient id="dc-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.45" />
            <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
          </radialGradient>
          <linearGradient id="dc-beam" x1="0" x2="1">
            <stop offset="0%" stopColor="color-mix(in oklab, var(--primary) 65%, transparent)" />
            <stop offset="100%" stopColor="color-mix(in oklab, var(--primary) 5%, transparent)" />
          </linearGradient>
          <pattern id="dc-grid" x="0" y="0" width="20" height="20" patternUnits="userSpaceOnUse">
            <path d="M 20 0 L 0 0 0 20" fill="none" stroke="color-mix(in oklab, var(--border) 50%, transparent)" strokeWidth="0.4" />
          </pattern>
        </defs>

        {/* Background grid */}
        <rect width="400" height="220" fill="url(#dc-grid)" opacity="0.45" />

        {/* Center halo */}
        <circle cx={cx} cy={cy} r="100" fill="url(#dc-glow)" />

        {/* Outer ring boundary */}
        <circle cx={cx} cy={cy} r={ringR + 4} fill="none" stroke="color-mix(in oklab, var(--border) 80%, transparent)" strokeWidth="0.6" strokeDasharray="2 5" />

        {/* Direct beams */}
        {providers.map((p, i) => {
          const e = pt(p.deg, ringR)
          return (
            <g key={i}>
              <line x1={cx} y1={cy} x2={e.x} y2={e.y} stroke="url(#dc-beam)" strokeWidth="1" />
              {/* tiny traveling notches midway */}
              <circle cx={(cx + e.x) / 2} cy={(cy + e.y) / 2} r="1.2" fill="color-mix(in oklab, var(--primary) 70%, transparent)" />
            </g>
          )
        })}

        {/* Provider nodes */}
        {providers.map((p, i) => {
          const e = pt(p.deg, ringR)
          return (
            <g key={`n-${i}`}>
              <circle cx={e.x} cy={e.y} r="9" fill="var(--card)" stroke={p.c} strokeWidth="1.4" />
              <circle cx={e.x} cy={e.y} r="4" fill={p.c} />
            </g>
          )
        })}

        {/* You — central glyph (laptop-ish rounded square w/ keyhole) */}
        <g transform={`translate(${cx} ${cy})`}>
          <rect x="-22" y="-16" width="44" height="28" rx="4" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="var(--primary)" strokeWidth="1.4" />
          <rect x="-26" y="13" width="52" height="3" rx="1.5" fill="color-mix(in oklab, var(--primary) 55%, transparent)" />
          <circle cx="0" cy="-2" r="4" fill="var(--card)" stroke="var(--primary)" strokeWidth="1" />
          <rect x="-1" y="-2" width="2" height="8" fill="var(--primary)" />
        </g>

        {/* Annotation tick — center label */}
        <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="2">
          <line x1={cx} y1={cy + 22} x2={cx} y2={cy + 38} stroke="color-mix(in oklab, var(--primary) 50%, transparent)" strokeWidth="0.7" />
          <text x={cx} y={cy + 50} textAnchor="middle" fill="var(--foreground)" opacity="0.8">YOUR MACHINE</text>
        </g>

        {/* Footer label */}
        <g fontFamily="var(--font-mono)" fontSize="9" fill="var(--muted-foreground)" letterSpacing="2">
          <text x="20" y="208">10 PROVIDERS</text>
          <text x="380" y="208" textAnchor="end" fill="var(--primary)" opacity="0.85">DIRECT · TLS</text>
        </g>
      </svg>
    </div>
  )
}
