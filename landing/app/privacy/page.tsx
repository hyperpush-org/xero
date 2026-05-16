import type { Metadata } from "next"
import Link from "next/link"
import { SiteHeader } from "@/components/landing/site-header"
import { SiteFooter } from "@/components/landing/site-footer"
import { mailto } from "@/lib/site"

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "How Xero handles your data. The desktop app is local-first: keys live in the OS keychain, model traffic goes provider-direct, and we don't host your sessions.",
  alternates: {
    canonical: "/privacy",
  },
}

const lastUpdated = "April 2026"

const sections: { id: string; title: string; body: React.ReactNode }[] = [
  {
    id: "summary",
    title: "Plain-English summary",
    body: (
      <>
        <p>
          Xero is a desktop app. Your projects, sessions, and journals stay on your machine. The
          API keys you paste live in the operating system keychain. We never receive them, and they
          aren&apos;t written in plain text on disk.
        </p>
        <p>
          When an agent calls a model, the request goes from your machine straight to the provider
          (Anthropic, OpenAI, Google, OpenRouter, GitHub, Azure, Bedrock, Vertex, or local Ollama).
          We are not in the middle of those calls.
        </p>
      </>
    ),
  },
  {
    id: "data-we-handle",
    title: "What data Xero handles",
    body: (
      <>
        <p>The desktop app touches three categories of data, all locally:</p>
        <ul>
          <li>
            <strong className="text-foreground">Project files.</strong> Whatever paths you point Xero
            at. Nothing is uploaded; agents read and write under those paths only.
          </li>
          <li>
            <strong className="text-foreground">Session journals.</strong> A local record of agent
            calls, tool results, diffs, and approvals. Stored under your user profile.
          </li>
          <li>
            <strong className="text-foreground">Provider credentials.</strong> API keys and OAuth
            tokens, kept in the OS keychain (Keychain on macOS, Credential Manager on Windows,
            Secret Service on Linux).
          </li>
        </ul>
      </>
    ),
  },
  {
    id: "what-we-collect",
    title: "What we collect",
    body: (
      <>
        <p>
          The website you&apos;re reading uses standard analytics to understand which pages get used
          (Vercel Analytics): page-level only, no personal identifiers, no cross-site tracking.
        </p>
        <p>
          The desktop app sends optional, anonymized crash reports if you opt in. You can disable
          them under{" "}
          <span className="font-mono text-xs text-foreground">Settings → Diagnostics</span>. They
          contain stack traces and version info, never your prompts or files.
        </p>
        <p>
          When you email{" "}
          <Link href={mailto()} className="underline underline-offset-2 hover:text-foreground">
            team@xeroshell.com
          </Link>{" "}
          we hold the contents of your email to reply. That&apos;s it.
        </p>
      </>
    ),
  },
  {
    id: "providers",
    title: "Model providers",
    body: (
      <>
        <p>
          Each provider you connect has its own privacy and data-retention policy. Xero is a thin
          client over their APIs. Your prompts and outputs are subject to whichever provider you
          chose for that call.
        </p>
        <p>
          If you need full isolation, point Xero at a local Ollama instance. No request leaves your
          network in that mode.
        </p>
      </>
    ),
  },
  {
    id: "cloud",
    title: "Solana bundle (when it ships)",
    body: (
      <>
        <p>
          The Solana bundle is an opt-in subscription for managed RPC, indexer, and webhook
          infrastructure. When it ships we&apos;ll publish a dedicated processing addendum covering
          the third-party providers involved. Until then, there is no Xero cloud holding your data.
        </p>
      </>
    ),
  },
  {
    id: "rights",
    title: "Your rights",
    body: (
      <>
        <p>
          The data Xero holds about you (locally) is yours. Delete the app and the journals go with
          it. For anything we hold server-side, such as emails to support or the analytics record
          of your page views, you can request access or deletion at{" "}
          <Link
            href={mailto()}
            className="underline underline-offset-2 hover:text-foreground"
          >
            team@xeroshell.com
          </Link>
          .
        </p>
      </>
    ),
  },
  {
    id: "contact",
    title: "Contact",
    body: (
      <>
        <p>
          Questions about this policy? Email{" "}
          <Link
            href={mailto()}
            className="underline underline-offset-2 hover:text-foreground"
          >
            team@xeroshell.com
          </Link>
          . We read every message.
        </p>
      </>
    ),
  },
]

export default function PrivacyPage() {
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
              Privacy
            </p>
            <h1 className="mt-3 font-sans text-4xl font-medium tracking-tight text-balance sm:text-5xl">
              Local-first by design.
            </h1>
            <p className="mt-4 max-w-2xl text-pretty text-muted-foreground">
              Xero runs on your machine. Your projects, journals, and keys stay there. This page
              spells out what we touch and what we don&apos;t.
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
              aria-label="Privacy sections"
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
