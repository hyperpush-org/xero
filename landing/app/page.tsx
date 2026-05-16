import { SiteHeader } from "@/components/landing/site-header"
import { Hero } from "@/components/landing/hero"
import { Features } from "@/components/landing/features"
import { FeatureGrid } from "@/components/landing/feature-grid"
import { Models } from "@/components/landing/models"
import { Integrations } from "@/components/landing/integrations"
import { Pricing } from "@/components/landing/pricing"
import { CTA } from "@/components/landing/cta"
import { SiteFooter } from "@/components/landing/site-footer"
import { LandingStructuredData } from "@/components/landing/structured-data"

export default function Page() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <LandingStructuredData />
      <SiteHeader />
      <main>
        <Hero />
        <div
          aria-hidden
          className="mx-auto h-px w-full max-w-7xl bg-gradient-to-r from-transparent via-border/80 to-transparent"
        />
        <Features />
        <FeatureGrid />
        <Models />
        <Integrations />
        <Pricing />
        <CTA />
      </main>
      <SiteFooter />
    </div>
  )
}
