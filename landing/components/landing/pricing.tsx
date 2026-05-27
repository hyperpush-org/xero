import Link from "next/link"
import type { ElementType } from "react"
import { Check, Cloud, Github, Rocket } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { mailto, siteConfig } from "@/lib/site"

type AddOn = {
  name: string
  price: string
  period: string
  statusBadge?: string
  priceBadge?: string
  previousPrice?: string
  icon: ElementType
  description: string
  features: string[]
}

const addOns: AddOn[] = [
  {
    name: "Solana Bundle",
    price: "$50",
    period: "/ month",
    statusBadge: "Soon",
    icon: Rocket,
    description: "Premium RPC and indexer, hosted forks, deploy guardrails, and program monitoring, bundled into one subscription.",
    features: [
      "Premium RPC + indexer credentials",
      "Hosted forks and snapshots",
      "Squads proposals and deploy guardrails",
      "Replaces $200+/mo of stitched-together services",
    ],
  },
  {
    name: "Remote sessions",
    price: "Free",
    period: "for a limited time",
    priceBadge: "Limited-time offer",
    previousPrice: "$10 / month",
    icon: Cloud,
    description: "Drive a desktop agent run from the Xero cloud browser app. Review, redirect, and approve from any signed-in browser.",
    features: [
      "Approve, redirect, and review in browser",
      "Live activity stream and diffs",
      "Remote session approval inbox",
      "End-to-end encrypted browser-to-desktop relay",
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
            The app is free. Two optional add-ons.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Every feature, every model provider, no trial timer. Two
            independent add-ons take you further if you want them. Turn either
            on or off, anytime.
          </p>
        </div>

        {/* Free product: inline hero, no card chrome */}
        <div className="mt-12 mx-auto flex max-w-3xl flex-col items-center gap-4 text-center">
          <div className="flex items-baseline gap-2">
            <span className="text-6xl font-medium tracking-tight">$0</span>
            <span className="text-sm text-muted-foreground">forever</span>
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button
              asChild
              size="lg"
              className="bg-primary text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] hover:bg-primary/90"
            >
              <a href="/download">
                Download app
              </a>
            </Button>
            <Button asChild size="lg" variant="secondary" className="gap-2 border border-border/70">
              <Link href={siteConfig.githubUrl} target="_blank" rel="noopener noreferrer">
                <Github className="h-4 w-4" />
                Source
              </Link>
            </Button>
          </div>
        </div>

        {/* Divider with add-on label */}
        <div className="mx-auto mt-14 flex max-w-3xl items-center gap-4">
          <div className="h-px flex-1 bg-border/60" />
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-muted-foreground">
            Optional add-ons · independent
          </p>
          <div className="h-px flex-1 bg-border/60" />
        </div>

        {/* Add-on cards */}
        <div className="mt-8 mx-auto grid max-w-4xl grid-cols-1 gap-4 md:grid-cols-2">
          {addOns.map((t) => {
            const Icon = t.icon
            return (
              <Card
                key={t.name}
                className="relative flex h-full flex-col gap-0 overflow-hidden rounded-2xl border-dashed border-border/70 px-1 py-1"
              >
                <CardHeader className="gap-0 px-5 pb-0 pt-5">
                  <div className="flex min-h-10 items-start justify-between gap-4">
                    <div className="flex min-w-0 items-center gap-2.5">
                      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                        <Icon className="h-3.5 w-3.5" />
                      </div>
                      <CardTitle className="truncate text-sm font-medium">
                        {t.name}
                      </CardTitle>
                    </div>
                    {t.statusBadge ? (
                      <span className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-border/70 bg-background px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                        <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                        {t.statusBadge}
                      </span>
                    ) : null}
                  </div>

                  <div className="flex min-h-[5rem] flex-wrap items-end gap-x-2 gap-y-1 pt-6">
                    <span className="text-3xl font-medium tracking-tight">{t.price}</span>
                    <span className="text-xs text-muted-foreground">{t.period}</span>
                    {t.previousPrice ? (
                      <span className="text-xs text-muted-foreground/60 line-through">
                        {t.previousPrice}
                      </span>
                    ) : null}
                  </div>

                  <div className="mt-3 flex min-h-8 items-start">
                    {t.priceBadge ? (
                      <Badge
                        variant="secondary"
                        className="border border-primary/20 bg-primary/10 text-primary"
                      >
                        {t.priceBadge}
                      </Badge>
                    ) : null}
                  </div>

                  <p className="mt-2 min-h-[5.25rem] text-sm leading-relaxed text-muted-foreground">
                    {t.description}
                  </p>
                </CardHeader>

                <CardContent className="flex flex-1 flex-col px-5 pb-5 pt-0">
                  <ul className="mt-5 flex flex-1 flex-col gap-2 border-t border-border/50 pt-4">
                    {t.features.map((f) => (
                      <li key={f} className="flex items-start gap-2 text-sm">
                        <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                        <span className="text-muted-foreground">{f}</span>
                      </li>
                    ))}
                  </ul>
                </CardContent>
              </Card>
            )
          })}
        </div>

        <p className="mt-8 text-center text-xs text-muted-foreground/60">
          Model usage runs on your own provider accounts. Need team seats or larger infrastructure?{" "}
          <Link href={mailto()} className="underline underline-offset-2 transition-colors hover:text-muted-foreground">
            Talk to us
          </Link>
          .
        </p>
      </div>
    </section>
  )
}
