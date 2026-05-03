import { GitBranch, Lightbulb, Search, Sparkles } from 'lucide-react'

import { cn } from '@/lib/utils'
import { AppLogo } from '../app-logo'

interface EmptySessionStateProps {
  projectLabel: string
  greetingName?: string | null
  onSelectSuggestion?: (prompt: string) => void
  /** Density variant. `dense` strips the brand glyph + paragraph for ultra-compact panes. */
  variant?: 'default' | 'dense'
}

interface Suggestion {
  icon: typeof Sparkles
  label: string
  prompt: string
}

const SUGGESTIONS: Suggestion[] = [
  {
    icon: Search,
    label: 'Explore the codebase',
    prompt: 'Walk me through this codebase: structure, key modules, and where to start reading.',
  },
  {
    icon: GitBranch,
    label: 'Review recent commits',
    prompt: 'Review my recent commits for correctness risks and maintainability concerns.',
  },
  {
    icon: Lightbulb,
    label: 'Suggest next steps',
    prompt: 'Based on this project, suggest a few high-impact things I could work on next.',
  },
]

export function EmptySessionState({
  projectLabel,
  greetingName,
  onSelectSuggestion,
  variant = 'default',
}: EmptySessionStateProps) {
  const greeting = greetingName ? `${getDaypartGreeting()}, ${greetingName}` : null

  if (variant === 'dense') {
    return (
      <div className="relative flex min-h-full w-full items-center justify-center overflow-hidden">
        <div className="relative flex w-full max-w-[260px] flex-col items-stretch px-3 py-4">
          <h2 className="text-center text-[13px] font-semibold tracking-tight text-foreground">
            <span className="text-primary">{projectLabel}</span>
          </h2>
          {onSelectSuggestion ? (
            <ul className="mt-3 flex w-full flex-col divide-y divide-border/40 overflow-hidden rounded-md border border-border/60 bg-card/30">
              {SUGGESTIONS.map((suggestion) => (
                <li key={suggestion.label}>
                  <button
                    className={cn(
                      'group flex w-full items-center gap-2 px-2 py-1.5 text-left transition-colors',
                      'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                    )}
                    onClick={() => onSelectSuggestion(suggestion.prompt)}
                    type="button"
                  >
                    <suggestion.icon className="h-3 w-3 shrink-0 text-muted-foreground transition-colors group-hover:text-primary" />
                    <span className="flex-1 truncate text-[11.5px] text-foreground/80 group-hover:text-foreground">
                      {suggestion.label}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </div>
    )
  }

  return (
    <div className="relative flex min-h-full w-full items-center justify-center overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[680px] -translate-x-1/2 -translate-y-[60%] rounded-full opacity-[0.07] blur-[120px]"
        style={{
          background:
            'radial-gradient(closest-side, #d4a574 0%, rgba(212,165,116,0.4) 45%, transparent 75%)',
        }}
      />

      <div className="relative flex w-full max-w-xl flex-col items-center px-8 py-12 text-center">
        <BrandGlyph />

        {greeting ? (
          <p className="mt-5 text-[12px] font-medium uppercase tracking-[0.18em] text-muted-foreground/80">
            {greeting}
          </p>
        ) : null}

        <h2 className="mt-3 text-2xl font-semibold tracking-tight text-foreground sm:text-[26px]">
          What can we build together in <span className="text-primary">{projectLabel}</span>?
        </h2>
        <p className="mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          Just ask — I can read your code, suggest changes, or run a task for you. Everything we do will show up right here.
        </p>

        {onSelectSuggestion ? (
          <ul className="mt-8 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/40 backdrop-blur-sm">
            {SUGGESTIONS.map((suggestion) => (
              <li key={suggestion.label}>
                <button
                  className={cn(
                    'group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors',
                    'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                  )}
                  onClick={() => onSelectSuggestion(suggestion.prompt)}
                  type="button"
                >
                  <suggestion.icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-primary" />
                  <span className="flex-1 truncate text-[13px] text-foreground/85 group-hover:text-foreground">
                    {suggestion.label}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </div>
  )
}

function getDaypartGreeting(): string {
  const hour = new Date().getHours()
  if (hour < 5) return 'Late night'
  if (hour < 12) return 'Good morning'
  if (hour < 17) return 'Good afternoon'
  if (hour < 21) return 'Good evening'
  return 'Good night'
}

function BrandGlyph() {
  return (
    <div className="relative">
      <div className="absolute inset-0 -z-10 rounded-3xl bg-primary/10 blur-2xl" />
      <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-card/60">
        <AppLogo className="h-7 w-7" />
      </div>
    </div>
  )
}
