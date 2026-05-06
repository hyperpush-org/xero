import Link from "next/link"
import { ArrowRight, Clock } from "lucide-react"
import { Button } from "@/components/ui/button"
import { AppWindowMock } from "@/components/landing/app-window-mock"

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
            href="#capabilities"
            className="inline-flex items-center gap-2 rounded-full border border-border/70 bg-secondary/40 px-3 py-1 text-xs text-muted-foreground backdrop-blur hover:border-border hover:bg-secondary/70 hover:text-foreground"
          >
            <span className="inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
            Beta — custom agents, composable workflows, mobile approvals
            <ArrowRight className="h-3 w-3" />
          </Link>

          <h1 className="mt-6 font-sans text-4xl font-medium tracking-tight text-balance sm:text-6xl lg:text-7xl">
            Build the agent.{" "}
            <span className="text-muted-foreground">Build the workflow.</span>
          </h1>

          <p className="mx-auto mt-6 max-w-xl text-pretty text-base leading-relaxed text-muted-foreground sm:text-lg">
            Custom agents and visual workflows that ship whole projects —
            pinging your phone only when a real call needs you.
          </p>

          <div className="mt-8 flex justify-center">
            <Button
              size="lg"
              disabled
              aria-disabled
              className="h-11 gap-2 bg-secondary/60 px-5 text-muted-foreground disabled:opacity-100 disabled:pointer-events-auto disabled:cursor-not-allowed"
            >
              <Clock className="h-4 w-4" />
              Coming soon
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
