import { ArrowRight, Bell, FolderGit2, Sparkles } from "lucide-react"
import { Button } from "@/components/ui/button"

interface WelcomeStepProps {
  onContinue: () => void
  onSkipAll: () => void
}

const HIGHLIGHTS: Array<{ Icon: React.ElementType; label: string; hint: string }> = [
  { Icon: Sparkles, label: "Pick a provider", hint: "OpenAI, Anthropic, Ollama, Bedrock, Vertex, and more" },
  { Icon: FolderGit2, label: "Import a project", hint: "Local Git repo" },
  { Icon: Bell, label: "Wire notifications", hint: "Optional, per project" },
]

export function WelcomeStep({ onContinue, onSkipAll }: WelcomeStepProps) {
  return (
    <div className="flex flex-col items-center text-center">
      <div className="relative flex h-14 w-14 items-center justify-center rounded-xl border border-border bg-card/50 animate-in fade-in-0 zoom-in-95 motion-enter">
        <svg className="relative text-primary" fill="none" height="26" viewBox="0 0 24 24" width="26">
          <path d="M4 4h6v6H4V4Z" fill="currentColor" />
          <path d="M14 4h6v6h-6V4Z" fill="currentColor" fillOpacity="0.35" />
          <path d="M4 14h6v6H4v-6Z" fill="currentColor" fillOpacity="0.35" />
          <path d="M14 14h6v6h-6v-6Z" fill="currentColor" />
        </svg>
      </div>

      <h1 className="mt-7 text-3xl font-semibold tracking-tight text-foreground">
        Welcome to Cadence
      </h1>
      <p className="mt-3 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
        Configure a provider, import a project, and optionally add notification routes before you enter the app.
      </p>

      <ul className="mt-8 grid w-full grid-cols-3 gap-2 animate-in fade-in-0 slide-in-from-bottom-2 motion-enter [animation-delay:80ms] [animation-fill-mode:both]">
        {HIGHLIGHTS.map(({ Icon, label, hint }) => (
          <li
            key={label}
            className="flex flex-col items-center gap-1.5 rounded-lg border border-border/70 bg-card/40 px-2 py-3 text-center"
          >
            <span className="flex h-7 w-7 items-center justify-center rounded-md border border-border bg-secondary/60 text-foreground/80">
              <Icon className="h-3.5 w-3.5" />
            </span>
            <span className="text-[11px] font-medium leading-tight text-foreground">{label}</span>
            <span className="text-[10px] leading-tight text-muted-foreground">{hint}</span>
          </li>
        ))}
      </ul>

      <div className="mt-9 flex items-center gap-2">
        <Button
          size="lg"
          onClick={onContinue}
          className="group h-10 gap-2 bg-primary px-5 text-[13px] font-medium hover:bg-primary/90"
        >
          Get started
          <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-0.5" />
        </Button>
        <Button
          size="lg"
          variant="ghost"
          onClick={onSkipAll}
          className="h-10 text-[13px] text-muted-foreground hover:text-foreground"
        >
          Skip
        </Button>
      </div>
    </div>
  )
}
