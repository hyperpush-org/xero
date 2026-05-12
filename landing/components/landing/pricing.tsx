import Link from "next/link"
import type { ElementType } from "react"
import { Check, Download, Laptop, Rocket } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { mailto, siteConfig } from "@/lib/site"

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
  comingSoon?: boolean
  features: string[]
}

const tiers: PricingTier[] = [
  {
    name: "Free",
    price: "$0",
    period: "forever",
    icon: Laptop,
    description: "The full desktop app. Every feature, every model provider, no trial timer. Bring your own keys and run as much as your machine handles.",
    cta: "Run it locally",
    ctaHref: siteConfig.githubUrl,
    highlight: true,
    badge: "Coming soon",
    comingSoon: true,
    features: [
      "Custom agents with per-agent tools, memory, and approval rules",
      "Built-in Ask, Engineer, Debug, Agent Create",
      "10 model providers, BYO keys",
      "Branch, rewind, compact, handoff",
      "Repo, shell, git, browser, mobile, MCP, Solana",
      "Discord and Telegram approvals",
      "Up to 6 panes per project",
      "Keys in OS keychain, community support",
    ],
  },
  {
    name: "Solana Bundle",
    price: "$50",
    period: "/ month",
    icon: Rocket,
    description: "Optional. One subscription for the Solana infrastructure a builder usually pays four or five vendors for.",
    cta: "Coming soon",
    ctaHref: "#",
    highlight: false,
    badge: "Coming soon",
    comingSoon: true,
    features: [
      "Premium RPC with elevated rate limits",
      "Indexer and webhook credentials, no vendor sprawl",
      "Hosted forks and reusable snapshots",
      "Tx simulation, fee tuning, ALT and IDL helpers",
      "Squads proposals, verified builds, deploy guardrails",
      "Program monitoring: drift, logs, cost alerts",
      "Replaces $200+/mo of stitched-together Solana tools",
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
            Free forever. Solana, optional.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            The app is free for everyone — no trial, no paywalled features, no
            seat counts. The Solana bundle is the one optional add-on, for teams
            that want their RPC, indexer, and deploy infra in a single subscription.
          </p>
        </div>

        {/* Cards */}
        <div className="mt-14 mx-auto grid max-w-5xl grid-cols-1 gap-4 md:grid-cols-2">
          {tiers.map((t) => {
            const Icon = t.icon
            return (
              <Card
                key={t.name}
                className={cn(
                  "relative flex flex-col overflow-hidden rounded-2xl px-1 py-1",
                  t.highlight
                    ? "border-primary/40 shadow-[0_30px_80px_-30px_color-mix(in_oklab,var(--primary)_35%,transparent)] ring-1 ring-primary/25"
                    : t.comingSoon
                      ? "border-dashed border-border/70"
                      : "border-border/60",
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
                    {t.comingSoon ? (
                      <span className="h-1.5 w-1.5 rounded-full bg-primary" />
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
                  {t.name === "Free" ? (
                    <div className="mt-1 grid grid-cols-2 gap-2">
                      <Button
                        asChild
                        size="lg"
                        className="w-full bg-primary text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] hover:bg-primary/90"
                      >
                        <Link href={siteConfig.githubUrl} target="_blank" rel="noopener noreferrer">
                          Run it locally
                        </Link>
                      </Button>
                      <Button
                        size="lg"
                        disabled
                        aria-disabled
                        className="relative w-full bg-secondary text-muted-foreground disabled:opacity-100 disabled:pointer-events-auto disabled:cursor-not-allowed"
                      >
                        <Download className="h-4 w-4" />
                        Download
                        <span className="absolute -top-2 -right-2 inline-flex items-center rounded-full border border-primary/40 bg-background px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-primary">
                          Soon
                        </span>
                      </Button>
                    </div>
                  ) : (
                    <Button
                      size="lg"
                      disabled
                      aria-disabled
                      className="mt-1 w-full bg-secondary text-muted-foreground disabled:opacity-100 disabled:pointer-events-auto disabled:cursor-not-allowed"
                    >
                      {t.cta}
                    </Button>
                  )}

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

        <div className="mx-auto mt-5 max-w-5xl rounded-xl border border-border/60 bg-secondary/20 px-4 py-3 text-center text-xs leading-5 text-muted-foreground">
          Model usage runs on your own provider accounts. Run it locally from
          source today — packaged downloads and the Solana bundle land soon.
        </div>

        <p className="mt-8 text-center text-xs text-muted-foreground/60">
          Need team seats or larger Solana infrastructure?{" "}
          <Link href={mailto()} className="underline underline-offset-2 transition-colors hover:text-muted-foreground">
            Talk to us
          </Link>
          .
        </p>
      </div>
    </section>
  )
}
