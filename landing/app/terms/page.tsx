import type { Metadata } from "next"
import Link from "next/link"
import { SiteHeader } from "@/components/landing/site-header"
import { SiteFooter } from "@/components/landing/site-footer"
import { mailto, siteDomain } from "@/lib/site"

export const metadata: Metadata = {
  title: "Terms",
  description:
    "Terms of use for the Xero desktop app and website. Plain language, no surprises.",
  alternates: {
    canonical: "/terms",
  },
}

const lastUpdated = "April 2026"

const sections: { id: string; title: string; body: React.ReactNode }[] = [
  {
    id: "agreement",
    title: "1. The deal",
    body: (
      <p>
        These terms cover your use of the Xero desktop app and the website at{" "}
        <span className="font-mono text-xs text-foreground">{siteDomain}</span>. By installing the app or
        using the site you agree to them. If you don&apos;t agree, don&apos;t use them. That&apos;s
        the whole enforcement mechanism.
      </p>
    ),
  },
  {
    id: "license",
    title: "2. License to use the app",
    body: (
      <>
        <p>
          The Xero desktop app is provided to you under a personal, non-exclusive, non-transferable
          license to install and run on machines you control, for your own work or your team&apos;s
          work.
        </p>
        <p>
          You may not redistribute the binaries, reverse-engineer the app to build a competing
          product, or strip the branding and resell it. Normal forensic, security, and
          interoperability research is fine.
        </p>
      </>
    ),
  },
  {
    id: "your-content",
    title: "3. Your content stays yours",
    body: (
      <p>
        Anything you write, paste, or generate inside Xero (prompts, code, diffs, journals) stays
        your property. We don&apos;t claim a license to your content. We don&apos;t see it: the
        desktop app is local-first and the model traffic is provider-direct.
      </p>
    ),
  },
  {
    id: "providers",
    title: "4. Model providers and other services",
    body: (
      <>
        <p>
          When you connect a provider key, your use of that provider is governed by their terms,
          not ours. Xero is a client over their APIs. Costs they bill, rate limits they enforce,
          policies they publish. Those are between you and them.
        </p>
        <p>
          Same for any third-party tools you wire into an agent (MCP servers, browser sites,
          on-chain RPCs). You&apos;re responsible for using them within their own rules.
        </p>
      </>
    ),
  },
  {
    id: "acceptable-use",
    title: "5. Acceptable use",
    body: (
      <>
        <p>Don&apos;t point Xero at things it shouldn&apos;t be pointed at:</p>
        <ul>
          <li>Code, systems, or data you don&apos;t have permission to touch</li>
          <li>Generating malware, mass spam, or content that targets a person to harm them</li>
          <li>Bypassing the safety controls of any model provider you&apos;ve connected</li>
          <li>Anything illegal where you are or where the affected systems live</li>
        </ul>
        <p>
          You&apos;re the operator. Agents do what you tell them to do. The responsibility for
          what they do is yours.
        </p>
      </>
    ),
  },
  {
    id: "beta",
    title: "6. Beta software",
    body: (
      <p>
        Xero is in beta. Things break, schemas change, journals occasionally need a migration. Keep
        backups of anything you can&apos;t afford to lose. We&apos;ll tell you when something
        ships, and we&apos;ll fix what we break, but we won&apos;t promise zero-downtime upgrades
        before v1.
      </p>
    ),
  },
  {
    id: "warranty",
    title: "7. Warranty disclaimer",
    body: (
      <p>
        The app and the site are provided <em>as is</em>, without warranty of any kind, express or
        implied, including merchantability, fitness for a particular purpose, and
        non-infringement. We do our best, and we still ship software.
      </p>
    ),
  },
  {
    id: "liability",
    title: "8. Limitation of liability",
    body: (
      <p>
        To the maximum extent allowed by law, Xero Labs is not liable for indirect, incidental,
        special, consequential, or punitive damages, or for lost profits, revenues, data, or
        goodwill arising out of your use of the app. Where law caps liability at a fixed amount,
        ours is one hundred US dollars.
      </p>
    ),
  },
  {
    id: "changes",
    title: "9. Changes",
    body: (
      <p>
        We may update these terms as the product grows, for example when the optional Solana
        bundle ships. Material changes will be announced on this page with a new last-updated date.
        Keep using Xero after a change and you&apos;re agreeing to the updated terms.
      </p>
    ),
  },
  {
    id: "contact",
    title: "10. Contact",
    body: (
      <p>
        Anything to flag, including a takedown, a security report, or just a question? Email{" "}
        <Link
          href={mailto()}
          className="underline underline-offset-2 hover:text-foreground"
        >
          team@xeroshell.com
        </Link>
        .
      </p>
    ),
  },
]

export default function TermsPage() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <SiteHeader />
      <main>
        <section className="relative isolate overflow-hidden">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(ellipse_at_top,black_30%,transparent_70%)] opacity-[0.3]"
          />
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-radial-fade"
          />
          <div className="mx-auto w-full max-w-3xl px-4 pt-20 pb-10 sm:px-6 sm:pt-28 lg:px-8">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Terms of use
            </p>
            <h1 className="mt-3 font-sans text-4xl font-medium tracking-tight text-balance sm:text-5xl">
              Plain terms. No surprises.
            </h1>
            <p className="mt-4 max-w-2xl text-pretty text-muted-foreground">
              The agreement that governs how you use the Xero desktop app and this website. Short
              and direct on purpose.
            </p>
            <p className="mt-3 font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground/70">
              Last updated · {lastUpdated}
            </p>
          </div>
        </section>

        <div
          aria-hidden
          className="mx-auto h-px w-full max-w-7xl bg-gradient-to-r from-transparent via-border/80 to-transparent"
        />

        <section className="relative">
          <div className="mx-auto w-full max-w-3xl px-4 py-14 sm:px-6 lg:px-8 lg:py-20">
            <nav
              aria-label="Terms sections"
              className="mb-12 rounded-xl border border-border/60 bg-secondary/20 px-5 py-4"
            >
              <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground/70">
                On this page
              </p>
              <ul className="mt-3 grid grid-cols-1 gap-1.5 sm:grid-cols-2">
                {sections.map((s) => (
                  <li key={s.id}>
                    <Link
                      href={`#${s.id}`}
                      className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                    >
                      {s.title}
                    </Link>
                  </li>
                ))}
              </ul>
            </nav>

            <div className="flex flex-col gap-12">
              {sections.map((s) => (
                <article key={s.id} id={s.id} className="scroll-mt-24">
                  <h2 className="font-sans text-xl font-medium tracking-tight sm:text-2xl">
                    {s.title}
                  </h2>
                  <div className="prose-doc mt-3 max-w-none space-y-3 text-pretty leading-relaxed text-muted-foreground">
                    {s.body}
                  </div>
                </article>
              ))}
            </div>
          </div>
        </section>
      </main>
      <SiteFooter />
    </div>
  )
}
