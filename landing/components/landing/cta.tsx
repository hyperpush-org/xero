import { Clock } from "lucide-react"
import { Button } from "@/components/ui/button"

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

        <p className="mt-6 font-mono text-[11px] text-muted-foreground/70">
          Desktop apps for macOS, Windows, and Linux are on the way.
        </p>
      </div>
    </section>
  )
}
