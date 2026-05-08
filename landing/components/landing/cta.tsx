import Link from "next/link"
import { Download, Github } from "lucide-react"
import { Button } from "@/components/ui/button"
import { siteConfig } from "@/lib/site"

export function CTA() {
  return (
    <section id="download" className="relative isolate overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-grid [mask-image:radial-gradient(ellipse_at_center,black_30%,transparent_70%)] opacity-30"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-radial-fade"
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

        <div className="mt-8 flex flex-wrap justify-center gap-3">
          <Button
            asChild
            size="lg"
            className="h-11 gap-2 bg-primary px-5 text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] hover:bg-primary/90"
          >
            <Link href={siteConfig.githubUrl} target="_blank" rel="noopener noreferrer">
              <Github className="h-4 w-4" />
              Run it locally
            </Link>
          </Button>
          <Button
            size="lg"
            disabled
            aria-disabled
            className="h-11 gap-2 bg-secondary/60 px-5 text-muted-foreground hover:bg-secondary/60 hover:text-muted-foreground disabled:opacity-100 disabled:pointer-events-auto disabled:cursor-not-allowed"
          >
            <Download className="h-4 w-4" />
            Download
            <span className="ml-1 inline-flex items-center gap-1 rounded-full border border-border/70 bg-background/80 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              <span className="h-1.5 w-1.5 rounded-full bg-primary" />
              Coming soon
            </span>
          </Button>
        </div>

        <p className="mt-6 font-mono text-[11px] text-muted-foreground/70">
          Desktop apps for macOS, Windows, and Linux are on the way.
        </p>
      </div>
    </section>
  )
}
