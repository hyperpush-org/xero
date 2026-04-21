import Link from "next/link"
import { Check, Laptop, Cloud } from "lucide-react"
import { Button } from "@/components/ui/button"

const tiers = [
  {
    name: "Free",
    price: "$0",
    Xero: "forever",
    icon: Laptop,
    description: "Runs entirely on your machine. Bring your own API keys and ship immediately.",
    cta: "Download Xero",
    ctaHref: "#download",
    highlight: false,
    badge: null,
    comingSoon: false,
    features: [
      "Unlimited local projects",
      "Bring your own key — Claude, GPT, Gemini, OpenRouter",
      "Full autonomous agent runtime",
      "Workflow graph & operator loop",
      "Local SQLite persistence",
      "Discord & Telegram notifications",
      "Community support",
    ],
  },
  {
    name: "Pro",
    price: "$20",
    Xero: "/ month",
    icon: Cloud,
    description: "Everything in Free, running in the cloud. Same keys, no token markup — just your work, anywhere.",
    cta: "Notify me",
    ctaHref: "#notify",
    highlight: true,
    comingSoon: true,
    badge: "Coming soon",
    features: [
      "Everything in Free",
      "Still bring your own API keys — zero markup",
      "Cloud runtime — no local machine required",
      "Runs continue while your laptop sleeps",
      "Sync across devices",
      "Parallel agent workers · up to 16",
      "Priority support",
    ],
  },
]

export function Pricing() {
  return (
    <section id="pricing" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        {/* Header */}
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Pricing
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Pay for the studio, not the tokens.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Xero never marks up AI models. Every plan runs on the subscriptions
            you already have — we just charge for the tool.
          </p>
        </div>

        {/* Cards */}
        <div className="mt-14 mx-auto grid max-w-4xl grid-cols-1 gap-4 md:grid-cols-2">
          {tiers.map((t) => {
            const Icon = t.icon
            return (
              <div
                key={t.name}
                className={`relative flex flex-col rounded-2xl border p-7 transition-colors ${
                  t.highlight
                    ? "border-primary/40 bg-card ring-1 ring-primary/20"
                    : "border-border/60 bg-card"
                }`}
              >
                {t.badge && (
                  <span
                    className={`absolute -top-2.5 right-6 rounded-full px-3 py-0.5 text-[10px] font-medium uppercase tracking-wider ${
                      t.comingSoon
                        ? "bg-muted text-muted-foreground"
                        : "bg-primary text-primary-foreground"
                    }`}
                  >
                    {t.badge}
                  </span>
                )}

                {/* Tier label + icon */}
                <div className="flex items-center gap-2.5">
                  <div
                    className={`flex h-8 w-8 items-center justify-center rounded-lg ${
                      t.highlight
                        ? "bg-primary/15 text-primary"
                        : "bg-muted text-muted-foreground"
                    }`}
                  >
                    <Icon className="h-4 w-4" />
                  </div>
                  <h3 className="text-base font-medium">{t.name}</h3>
                </div>

                {/* Price */}
                <div className="mt-5 flex items-baseline gap-1.5">
                  <span className="text-5xl font-medium tracking-tight">{t.price}</span>
                  {t.Xero && (
                    <span className="text-sm text-muted-foreground">{t.Xero}</span>
                  )}
                </div>

                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  {t.description}
                </p>

                <Button
                  asChild={!t.comingSoon}
                  disabled={t.comingSoon}
                  size="lg"
                  className={`mt-6 w-full ${
                    t.comingSoon
                      ? "cursor-not-allowed opacity-40"
                      : t.highlight
                        ? "bg-primary text-primary-foreground hover:bg-primary/90"
                        : "bg-secondary text-foreground hover:bg-secondary/80"
                  }`}
                >
                  {t.comingSoon ? (
                    t.cta
                  ) : (
                    <Link href={t.ctaHref}>{t.cta}</Link>
                  )}
                </Button>

                {/* Feature list */}
                <ul className="mt-6 space-y-2.5 border-t border-border/50 pt-6">
                  {t.features.map((f, i) => {
                    const isEverything = f.startsWith("Everything in")
                    return (
                      <li key={f} className="flex items-start gap-2.5 text-sm">
                        <Check
                          className={`mt-0.5 h-4 w-4 shrink-0 ${
                            t.highlight
                              ? "text-primary"
                              : "text-muted-foreground"
                          }`}
                        />
                        <span
                          className={
                            isEverything
                              ? "font-medium text-foreground"
                              : "text-muted-foreground"
                          }
                        >
                          {f}
                        </span>
                      </li>
                    )
                  })}
                </ul>
              </div>
            )
          })}
        </div>

        {/* Footer note */}
        <p className="mt-8 text-center text-xs text-muted-foreground/60">
          Need a team plan?{" "}
          <Link href="mailto:team@Xero.sh" className="underline underline-offset-2 hover:text-muted-foreground transition-colors">
            Talk to us
          </Link>
          .
        </p>
      </div>
    </section>
  )
}
