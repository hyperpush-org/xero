import Link from "next/link"
import { Check } from "lucide-react"
import { Button } from "@/components/ui/button"

const tiers = [
  {
    name: "Hobby",
    price: "$0",
    cadence: "forever",
    description: "For tinkerers, side-projects, and evaluating Cadence on real work.",
    cta: "Download Cadence",
    highlight: false,
    features: [
      "Unlimited local projects",
      "Bring your own key — ChatGPT, Claude, Copilot, OpenRouter",
      "Local SQLite persistence",
      "Discord notifications",
      "Community support",
    ],
  },
  {
    name: "Pro",
    price: "$29",
    cadence: "per month",
    description: "For independent engineers shipping real products every week.",
    cta: "Start 14-day trial",
    highlight: true,
    features: [
      "Everything in Hobby",
      "Still use your own subscription keys — no markup",
      "Telegram + Slack notifications",
      "Parallel agent workers · up to 16",
      "Cloud sync across devices",
      "Priority email support",
    ],
  },
  {
    name: "Team",
    price: "$99",
    cadence: "per seat / month",
    description: "For small teams that want shared runs, review, and governance.",
    cta: "Talk to us",
    highlight: false,
    features: [
      "Everything in Pro",
      "Shared run history & review",
      "Role-based sandbox policies",
      "SSO (Google, Okta, Entra)",
      "Audit log & SOC 2 report",
      "Dedicated support channel",
    ],
  },
]

export function Pricing() {
  return (
    <section id="pricing" className="relative">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Pricing
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Simple. Pays for the app, not the tokens.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Cadence never marks up models. Every tier runs on the AI subscriptions
            you already pay for — we just charge for the studio itself.
          </p>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-4 lg:grid-cols-3">
          {tiers.map((t) => (
            <div
              key={t.name}
              className={`relative flex flex-col rounded-2xl border p-6 transition-colors ${
                t.highlight
                  ? "border-primary/50 bg-card ring-1 ring-primary/20"
                  : "border-border/70 bg-card hover:border-border"
              }`}
            >
              {t.highlight && (
                <span className="absolute -top-2.5 right-6 rounded-full bg-primary px-2.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-primary-foreground">
                  Most popular
                </span>
              )}
              <div className="flex items-center justify-between">
                <h3 className="text-lg font-medium">{t.name}</h3>
              </div>
              <div className="mt-5 flex items-baseline gap-1.5">
                <span className="text-4xl font-medium tracking-tight">{t.price}</span>
                <span className="text-sm text-muted-foreground">/ {t.cadence}</span>
              </div>
              <p className="mt-2 min-h-12 text-sm text-muted-foreground">
                {t.description}
              </p>

              <Button
                asChild
                size="lg"
                className={`mt-5 w-full ${
                  t.highlight
                    ? "bg-primary text-primary-foreground hover:bg-primary/90"
                    : "bg-secondary text-foreground hover:bg-secondary/80"
                }`}
              >
                <Link href="#download">{t.cta}</Link>
              </Button>

              <ul className="mt-6 space-y-2.5 border-t border-border/60 pt-6">
                {t.features.map((f) => (
                  <li key={f} className="flex items-start gap-2 text-sm">
                    <Check
                      className={`mt-0.5 h-4 w-4 shrink-0 ${
                        t.highlight ? "text-primary" : "text-muted-foreground"
                      }`}
                    />
                    <span className="text-muted-foreground">{f}</span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
