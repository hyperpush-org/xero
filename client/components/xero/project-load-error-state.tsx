import { AlertTriangle, RotateCw } from "lucide-react"
import { Button } from "@/components/ui/button"

interface ProjectLoadErrorStateProps {
  message: string
  onRetry: () => void
}

export function ProjectLoadErrorState({ message, onRetry }: ProjectLoadErrorStateProps) {
  return (
    <div className="relative flex flex-1 items-center justify-center overflow-hidden bg-background">
      {/* Faint destructive halo */}
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[680px] -translate-x-1/2 -translate-y-[55%] rounded-full opacity-[0.07] blur-[120px]"
        style={{
          background:
            "radial-gradient(closest-side, #ef4444 0%, rgba(239,68,68,0.4) 45%, transparent 75%)",
        }}
      />

      <div className="relative flex w-full max-w-md flex-col items-center px-8 text-center">
        <div className="relative">
          <div className="absolute inset-0 -z-10 rounded-3xl bg-destructive/10 blur-2xl" />
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-destructive/30 bg-card/60 text-destructive">
            <AlertTriangle className="h-6 w-6" strokeWidth={2} />
          </div>
        </div>

        <h2 className="mt-6 text-xl font-semibold tracking-tight text-foreground">
          Couldn&rsquo;t load project state
        </h2>
        <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
          Xero ran into a problem reading from the desktop backend.
        </p>

        <pre className="mt-5 max-h-40 w-full overflow-y-auto rounded-lg border border-border bg-card/40 px-3 py-2.5 text-left font-mono text-[11px] leading-relaxed text-muted-foreground whitespace-pre-wrap break-words">
          {message}
        </pre>

        <div className="mt-5">
          <Button
            onClick={onRetry}
            size="sm"
            className="h-9 gap-2 bg-primary px-4 text-[12px] font-medium hover:bg-primary/90"
          >
            <RotateCw className="h-3.5 w-3.5" />
            Try again
          </Button>
        </div>
      </div>
    </div>
  )
}
