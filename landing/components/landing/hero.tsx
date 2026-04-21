import Link from "next/link"
import { ArrowRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { AppWindowMock } from "@/components/landing/app-window-mock"
import { AppleIcon, WindowsIcon } from "@/components/landing/brand-icons"

export function Hero() {
  return (
    <section className="relative isolate overflow-hidden">
      {/* Ambient background */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(ellipse_at_top,black_30%,transparent_70%)] opacity-[0.35]"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-radial-fade"
      />

      <div className="mx-auto w-full max-w-7xl px-4 pb-16 pt-20 sm:px-6 sm:pt-28 lg:px-8 lg:pb-24">
        <div className="mx-auto max-w-3xl text-center">
          <Link
            href="#"
            className="group inline-flex items-center gap-2 rounded-full border border-border/70 bg-secondary/40 px-3 py-1 text-xs text-muted-foreground backdrop-blur transition-colors hover:border-border hover:bg-secondary/70 hover:text-foreground"
          >
            <span className="relative inline-flex h-1.5 w-1.5">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60" />
              <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
            </span>
            v0.9 — Persistent agents, now in public beta
            <ArrowRight className="h-3 w-3 transition-transform group-hover:translate-x-0.5" />
          </Link>

          <h1 className="mt-6 font-sans text-4xl font-medium tracking-tight text-balance sm:text-6xl lg:text-7xl">
            The agentic coding studio{" "}
            <span className="text-muted-foreground">that lives on your desktop.</span>
          </h1>

          <p className="mx-auto mt-6 max-w-2xl text-pretty text-base leading-relaxed text-muted-foreground sm:text-lg">
            Xero ships production software end-to-end — scaffold, build, test, deploy.
            Written from the ground up in Rust, it runs agents locally with a persistent
            harness and pings you on Discord or Telegram the moment it needs a decision.
          </p>

          <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Button
              asChild
              size="lg"
              className="h-11 gap-2 bg-primary px-5 text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] transition-all hover:bg-primary/90 hover:shadow-[0_10px_28px_-10px_color-mix(in_oklab,var(--primary)_75%,transparent)]"
            >
              <Link href="#download">
                <AppleIcon className="h-4 w-4" />
                Download for macOS
              </Link>
            </Button>
            <Button
              asChild
              size="lg"
              variant="outline"
              className="h-11 gap-2 border-border/80 bg-secondary/40 px-5 text-foreground hover:bg-secondary"
            >
              <Link href="#download">
                <WindowsIcon className="h-4 w-4" aria-hidden />
                Windows &amp; Linux
              </Link>
            </Button>
          </div>
        </div>

        <div className="relative mx-auto mt-16 max-w-6xl">
          <div
            aria-hidden
            className="absolute -inset-x-8 -top-8 -bottom-16 -z-10 rounded-[2rem] bg-gradient-to-b from-primary/10 via-transparent to-transparent blur-2xl"
          />
          <AppWindowMock />
        </div>
      </div>
    </section>
  )
}
