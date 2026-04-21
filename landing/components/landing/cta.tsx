import Link from "next/link"
import { Terminal } from "lucide-react"
import { Button } from "@/components/ui/button"
import { AppleIcon, WindowsIcon } from "@/components/landing/brand-icons"

export function CTA() {
  return (
    <section id="download" className="relative isolate overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-grid [mask-image:radial-gradient(ellipse_at_center,black_30%,transparent_70%)] opacity-30"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-radial-fade"
      />
      <div className="mx-auto w-full max-w-5xl px-4 py-24 text-center sm:px-6 lg:px-8 lg:py-32">
        <h2 className="mx-auto max-w-3xl font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl lg:text-6xl">
          Stop watching a spinner. <br className="hidden sm:block" />
          <span className="text-muted-foreground">Ship while you&apos;re AFK.</span>
        </h2>
        <p className="mx-auto mt-5 max-w-xl text-pretty text-muted-foreground">
          Xero is a free download. No credit card. Sign in with the ChatGPT,
          Claude, or Copilot plan you already have and start a build in under two
          minutes.
        </p>

        <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <Button
            asChild
            size="lg"
            className="h-11 gap-2 bg-primary px-5 text-primary-foreground hover:bg-primary/90"
          >
            <Link href="#">
              <AppleIcon className="h-4 w-4" />
              Download for macOS · Universal
            </Link>
          </Button>
          <Button
            asChild
            size="lg"
            variant="outline"
            className="h-11 gap-2 border-border/80 bg-secondary/40 px-5 text-foreground hover:bg-secondary"
          >
            <Link href="#">
              <WindowsIcon className="h-4 w-4" aria-hidden />
              Windows · Linux (.deb / .rpm)
            </Link>
          </Button>
        </div>
      </div>
    </section>
  )
}
