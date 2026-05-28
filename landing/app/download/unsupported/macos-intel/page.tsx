import type { Metadata } from "next"
import Link from "next/link"
import { AlertTriangle, ArrowLeft, Cpu, Download } from "lucide-react"
import { SiteFooter } from "@/components/landing/site-footer"
import { SiteHeader } from "@/components/landing/site-header"
import { Button } from "@/components/ui/button"
import { mailto } from "@/lib/site"

export const metadata: Metadata = {
  title: "macOS Intel support",
  description:
    "Xero for macOS is published for Apple silicon Macs. Intel Mac visitors are warned before downloading.",
  alternates: {
    canonical: "/download/unsupported/macos-intel",
  },
}

export default function MacosIntelUnsupportedPage() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <SiteHeader />
      <main>
        <section className="relative isolate overflow-hidden">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(ellipse_at_top,black_30%,transparent_70%)] opacity-[0.28]"
          />
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 bg-radial-fade"
          />
          <div className="mx-auto flex min-h-[70vh] w-full max-w-3xl flex-col justify-center px-4 py-20 sm:px-6 lg:px-8">
            <div className="flex size-11 items-center justify-center rounded-lg border border-amber-500/35 bg-amber-500/10 text-amber-300">
              <AlertTriangle className="size-5" aria-hidden />
            </div>
            <p className="mt-8 font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Unsupported Mac architecture
            </p>
            <h1 className="mt-3 text-4xl font-medium tracking-tight text-balance sm:text-5xl">
              Xero for macOS requires Apple silicon.
            </h1>
            <p className="mt-5 max-w-2xl text-pretty text-muted-foreground">
              We no longer publish Intel Mac builds. Apple&apos;s Mac lineup has moved to Apple
              silicon, and our macOS release pipeline now ships the ARM64 desktop app and TUI.
            </p>

            <div className="mt-8 grid gap-3 rounded-lg border border-border/60 bg-secondary/20 p-4 sm:grid-cols-[auto_1fr] sm:gap-4">
              <Cpu className="mt-0.5 size-5 text-muted-foreground" aria-hidden />
              <div>
                <h2 className="text-sm font-medium">On an Apple silicon Mac?</h2>
                <p className="mt-1 text-sm leading-6 text-muted-foreground">
                  Continue with the macOS Apple silicon build. If this warning appeared by mistake,
                  your browser did not report the CPU architecture accurately.
                </p>
              </div>
            </div>

            <div className="mt-8 flex flex-col gap-3 sm:flex-row">
              <Button asChild size="lg">
                <Link href="/download/macos-apple-silicon">
                  <Download className="size-4" aria-hidden />
                  Download Apple silicon build
                </Link>
              </Button>
              <Button asChild variant="outline" size="lg">
                <Link href="/">
                  <ArrowLeft className="size-4" aria-hidden />
                  Back to Xero
                </Link>
              </Button>
            </div>

            <p className="mt-6 text-sm text-muted-foreground">
              Need a supported alternative? Use the Windows or Linux builds, or email{" "}
              <Link
                href={mailto("Intel Mac support")}
                className="underline underline-offset-2 hover:text-foreground"
              >
                team@xeroshell.com
              </Link>
              .
            </p>
          </div>
        </section>
      </main>
      <SiteFooter />
    </div>
  )
}
