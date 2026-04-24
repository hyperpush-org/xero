import Link from "next/link"
import type { ElementType } from "react"
import { Check, Cloud, Laptop, Rocket } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"

type PricingTier = {
  name: string
  price: string
  period: string
  icon: ElementType
  description: string
  cta: string
  ctaHref: string
  highlight: boolean
  badge: string | null
  features: string[]
}

const paidPlanHref = "mailto:team@Xero.sh?subject=Xero%20paid%20plan%20beta"

const tiers: PricingTier[] = [
  {
    name: "Free",
    price: "$0",
    period: "forever",
    icon: Laptop,
    description: "The local desktop studio for builders who bring their own model keys.",
    cta: "Download Xero",
    ctaHref: "#download",
    highlight: false,
    badge: null,
    features: [
      "Unlimited local projects",
      "Bring your own key: Claude, GPT, Gemini, OpenRouter, Ollama",
      "Local autonomous agent runtime",
      "Workflow graph & operator loop",
      "Repo editor, search, and file operations",
      "Local SQLite persistence",
      "Discord & Telegram notifications",
      "Community support",
    ],
  },
  {
    name: "Pro",
    price: "$20",
    period: "/ month",
    icon: Cloud,
    description: "Cloud runtime, sync, and durable background runs with no model markup.",
    cta: "Join Pro beta",
    ctaHref: paidPlanHref,
    highlight: true,
    badge: "Recommended",
    features: [
      "Everything in Free",
      "Cloud runtime with your existing model subscriptions",
      "Runs continue while your laptop sleeps",
      "Sync across devices",
      "Hosted run history and replay",
      "Parallel agent workers · up to 16",
      "Priority support",
    ],
  },
  {
    name: "Solana Pro",
    price: "$50",
    period: "/ month",
    icon: Rocket,
    description: "Pro plus the on-chain workbench bundle for serious Solana builds.",
    cta: "Join Solana beta",
    ctaHref: paidPlanHref,
    highlight: false,
    badge: "On-chain dev",
    features: [
      "Everything in Pro",
      "Managed Solana dev subscription bundle",
      "Agent-usable RPC, indexer, and webhook credentials",
      "Hosted forks and reusable snapshots",
      "Tx simulation, fee tuning, ALT and IDL helpers",
      "Deploy safety, Squads proposals, verified builds",
      "Program monitoring, drift, logs, and cost alerts",
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
            Start local for free, add cloud runs when work needs to keep moving,
            and add the Solana bundle when on-chain infrastructure becomes the bottleneck.
          </p>
        </div>

        {/* Cards */}
        <div className="mt-14 mx-auto grid max-w-6xl grid-cols-1 gap-4 lg:grid-cols-3">
          {tiers.map((t) => {
            const Icon = t.icon
            return (
              <Card
                key={t.name}
                className={cn(
                  "relative flex flex-col overflow-hidden rounded-2xl px-1 py-1 transition-all hover:-translate-y-0.5",
                  t.highlight
                    ? "border-primary/40 shadow-[0_30px_80px_-30px_color-mix(in_oklab,var(--primary)_35%,transparent)] ring-1 ring-primary/25"
                    : "border-border/60 hover:border-border",
                )}
              >
                {t.highlight && (
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/60 to-transparent"
                  />
                )}
                {t.badge ? (
                  <span
                    className={cn(
                      "absolute right-6 top-4 inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[10px] font-medium uppercase tracking-wider",
                      t.highlight
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border/70 bg-background text-muted-foreground",
                    )}
                  >
                    {!t.highlight ? (
                      <span className="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-primary" />
                    ) : null}
                    {t.badge}
                  </span>
                ) : null}

                <CardHeader className="gap-0 px-6 pt-6">
                  <div className="flex items-center gap-2.5">
                    <div
                      className={cn(
                        "flex h-8 w-8 items-center justify-center rounded-lg",
                        t.highlight
                          ? "bg-primary/15 text-primary"
                          : "bg-muted text-muted-foreground",
                      )}
                    >
                      <Icon className="h-4 w-4" />
                    </div>
                    <CardTitle className="text-base font-medium">{t.name}</CardTitle>
                  </div>

                  <div className="mt-5 flex items-baseline gap-1.5">
                    <span className="text-5xl font-medium tracking-tight">{t.price}</span>
                    {t.period ? (
                      <span className="text-sm text-muted-foreground">{t.period}</span>
                    ) : null}
                  </div>

                  <p className="mt-3 min-h-12 text-sm leading-relaxed text-muted-foreground">
                    {t.description}
                  </p>
                </CardHeader>

                <CardContent className="flex flex-1 flex-col px-6 pb-6">
                  <Button
                    asChild
                    size="lg"
                    className={cn(
                      "mt-1 w-full",
                      t.highlight
                        ? "bg-primary text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] hover:bg-primary/90"
                        : "bg-secondary text-foreground hover:bg-secondary/80",
                    )}
                  >
                    <Link href={t.ctaHref}>{t.cta}</Link>
                  </Button>

                  <ul className="mt-6 flex flex-1 flex-col gap-2.5 border-t border-border/50 pt-6">
                    {t.features.map((f) => {
                      const isEverything = f.startsWith("Everything in")
                      return (
                        <li key={f} className="flex items-start gap-2.5 text-sm">
                          <Check
                            className={cn(
                              "mt-0.5 h-4 w-4 shrink-0",
                              t.highlight ? "text-primary" : "text-muted-foreground",
                            )}
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
                </CardContent>
              </Card>
            )
          })}
        </div>

        <div className="mx-auto mt-5 max-w-6xl rounded-xl border border-border/60 bg-secondary/20 px-4 py-3 text-center text-xs leading-5 text-muted-foreground">
          AI model usage stays on your own provider accounts. Solana Pro includes a managed
          developer-infrastructure bundle for normal build and test workflows; heavy production
          traffic can move to a custom team plan.
        </div>

        {/* Footer note */}
        <p className="mt-8 text-center text-xs text-muted-foreground/60">
          Need team seats, SSO, or heavier Solana infrastructure?{" "}
          <Link href="mailto:team@Xero.sh" className="underline underline-offset-2 transition-colors hover:text-muted-foreground">
            Talk to us
          </Link>
          .
        </p>
      </div>
    </section>
  )
}
