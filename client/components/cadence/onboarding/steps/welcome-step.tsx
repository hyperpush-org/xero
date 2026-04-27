import { ArrowRight, Bell, FolderGit2, Sparkles } from "lucide-react"
import { Button } from "@/components/ui/button"

interface WelcomeStepProps {
  onContinue: () => void
  onSkipAll: () => void
}

const HIGHLIGHTS: Array<{
  step: string
  Icon: React.ElementType
  label: string
  hint: string
}> = [
  {
    step: "01",
    Icon: Sparkles,
    label: "Pick a provider",
    hint: "OpenAI, Anthropic, Ollama, Bedrock, Vertex, and more",
  },
  {
    step: "02",
    Icon: FolderGit2,
    label: "Import a project",
    hint: "Point Cadence at any local Git repository",
  },
  {
    step: "03",
    Icon: Bell,
    label: "Wire notifications",
    hint: "Optional Telegram or Discord routes per project",
  },
]

export function WelcomeStep({ onContinue, onSkipAll }: WelcomeStepProps) {
  return (
    <div className="flex flex-col items-center text-center">
      <div className="relative animate-in fade-in-0 zoom-in-95 motion-enter">
        <div
          aria-hidden
          className="absolute -inset-8 rounded-full bg-primary/20 blur-3xl"
        />
        <div
          aria-hidden
          className="absolute -inset-3 rounded-2xl bg-primary/10 blur-xl"
        />
        <div className="relative flex h-16 w-16 items-center justify-center rounded-2xl border border-border/80 bg-gradient-to-br from-card to-card/30 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.04)]">
          <svg
            className="text-primary"
            fill="none"
            height="30"
            viewBox="0 0 24 24"
            width="30"
          >
            <path d="M4 4h6v6H4V4Z" fill="currentColor" />
            <path d="M14 4h6v6h-6V4Z" fill="currentColor" fillOpacity="0.35" />
            <path d="M4 14h6v6H4v-6Z" fill="currentColor" fillOpacity="0.35" />
            <path d="M14 14h6v6h-6v-6Z" fill="currentColor" />
          </svg>
        </div>
      </div>

      <span className="mt-7 inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card/40 px-2.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground animate-in fade-in-0 motion-enter [animation-delay:40ms] [animation-fill-mode:both]">
        <span className="h-1 w-1 rounded-full bg-primary/80" />
        First-time setup
      </span>

      <h1 className="mt-3 text-[34px] font-semibold leading-[1.05] tracking-tight text-foreground">
        Welcome to{" "}
        <span className="bg-gradient-to-br from-foreground to-foreground/70 bg-clip-text text-transparent">
          Cadence
        </span>
      </h1>
      <p className="mt-3 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
        Three quick steps to set up your workspace. You can change anything later
        from Settings.
      </p>

      <ol className="mt-8 flex w-full flex-col gap-1.5 animate-in fade-in-0 slide-in-from-bottom-2 motion-enter [animation-delay:80ms] [animation-fill-mode:both]">
        {HIGHLIGHTS.map(({ step, Icon, label, hint }) => (
          <li
            key={label}
            className="group/item flex items-center gap-3 rounded-lg border border-border/60 bg-card/40 px-3 py-2.5 text-left transition-[background-color,border-color] motion-fast hover:border-border hover:bg-card/70"
          >
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border/80 bg-gradient-to-br from-secondary/80 to-secondary/30 text-foreground/80 transition-colors group-hover/item:text-foreground">
              <Icon className="h-4 w-4" />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-baseline gap-2">
                <span className="font-mono text-[10px] font-medium tracking-wider text-muted-foreground/60">
                  {step}
                </span>
                <span className="text-[13px] font-medium leading-tight text-foreground">
                  {label}
                </span>
              </div>
              <p className="mt-0.5 text-[11px] leading-tight text-muted-foreground">
                {hint}
              </p>
            </div>
          </li>
        ))}
      </ol>

      <div className="mt-8 flex w-full items-center justify-center gap-2 animate-in fade-in-0 motion-enter [animation-delay:140ms] [animation-fill-mode:both]">
        <Button
          size="lg"
          onClick={onContinue}
          className="group h-11 gap-2 bg-primary px-6 text-[13px] font-medium shadow-sm hover:bg-primary/90"
        >
          Get started
          <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-0.5" />
        </Button>
        <Button
          size="lg"
          variant="ghost"
          onClick={onSkipAll}
          className="h-11 text-[13px] text-muted-foreground hover:text-foreground"
        >
          Skip
        </Button>
      </div>

      <p className="mt-5 text-[10.5px] text-muted-foreground/70">
        Takes about a minute · Your credentials stay on this device
      </p>
    </div>
  )
}
