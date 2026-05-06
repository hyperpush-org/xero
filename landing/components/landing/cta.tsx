import Link from "next/link"
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
          Build the agent. <br className="hidden sm:block" />
          <span className="text-muted-foreground">Ship the project.</span>
        </h2>
        <p className="mx-auto mt-5 max-w-xl text-pretty text-muted-foreground">
          Free desktop app. No credit card. Bring keys for OpenAI, Anthropic,
          Gemini, OpenRouter, GitHub, Azure, Bedrock, Vertex, or a local Ollama.
        </p>

        <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <Button
            asChild
            size="lg"
            className="h-11 gap-2 bg-primary px-5 text-primary-foreground shadow-[0_10px_30px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] transition-all hover:bg-primary/90 hover:shadow-[0_12px_36px_-10px_color-mix(in_oklab,var(--primary)_75%,transparent)]"
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

        <p className="mt-6 font-mono text-[11px] text-muted-foreground/70">
          macOS 13+ · Windows 10+ · Universal binary · Apple silicon &amp; Intel
        </p>
      </div>
    </section>
  )
}
