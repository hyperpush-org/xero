import { ChevronRight, GitBranch, Hammer, Lightbulb, Search, ShieldCheck, Sparkles, Workflow } from 'lucide-react'

import { cn } from '@/lib/utils'
import { AppLogo } from '../app-logo'

interface EmptySessionStateProps {
  projectLabel: string
  greetingName?: string | null
  onSelectSuggestion?: (prompt: string) => void
  context?: 'default' | 'agent-create'
  agentCreateCanvasIncluded?: boolean
  onStartWorkflowAgentCreate?: () => void
  /** Density variant. `dense` strips the brand glyph + paragraph for ultra-compact panes. */
  variant?: 'default' | 'dense'
}

interface Suggestion {
  icon: typeof Sparkles
  label: string
  prompt: string
}

const DEFAULT_SUGGESTIONS: Suggestion[] = [
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

const AGENT_CREATE_SUGGESTIONS: Suggestion[] = [
  {
    icon: Hammer,
    label: 'Create a coding helper',
    prompt:
      'Create a project agent that can make focused code changes, run scoped verification, and summarize the result.',
  },
  {
    icon: ShieldCheck,
    label: 'Create a read-only analyst',
    prompt:
      'Create an observe-only agent that explores this repository, answers project questions, and does not edit files.',
  },
]

export function EmptySessionState({
  projectLabel,
  greetingName,
  onSelectSuggestion,
  context = 'default',
  agentCreateCanvasIncluded = false,
  onStartWorkflowAgentCreate,
  variant = 'default',
}: EmptySessionStateProps) {
  const greeting = greetingName ? `${getDaypartGreeting()}, ${greetingName}` : null
  const isAgentCreate = context === 'agent-create'
  const suggestions = isAgentCreate ? AGENT_CREATE_SUGGESTIONS : DEFAULT_SUGGESTIONS
  const showWorkflowCanvasAction =
    isAgentCreate && !agentCreateCanvasIncluded && Boolean(onStartWorkflowAgentCreate)

  if (variant === 'dense') {
    return (
      <div className="relative flex min-h-full w-full items-center justify-center overflow-hidden">
        <div className="agent-empty-fade-in relative flex w-full max-w-[260px] flex-col items-stretch px-3 py-4">
          <h2 className="text-center text-[13px] font-semibold tracking-tight text-foreground">
            {isAgentCreate ? 'Shape this new agent' : <span className="text-primary">{projectLabel}</span>}
          </h2>
          {onSelectSuggestion || showWorkflowCanvasAction ? (
            <ul className="mt-3 flex w-full flex-col divide-y divide-border/40 overflow-hidden rounded-md border border-border/60 bg-card/30">
              {showWorkflowCanvasAction ? (
                <li>
                  <button
                    className={cn(
                      'agent-suggestion-row group flex w-full items-center gap-2 bg-primary/10 px-2 py-1.5 text-left transition-colors',
                      'hover:bg-primary/15 focus-visible:bg-primary/15 focus-visible:outline-none',
                    )}
                    onClick={onStartWorkflowAgentCreate}
                    type="button"
                  >
                    <Workflow data-suggestion-icon className="h-3 w-3 shrink-0 text-primary" />
                    <span className="flex-1 truncate text-[11.5px] font-medium text-foreground">
                      Start on canvas
                    </span>
                    <ChevronRight
                      data-suggestion-caret
                      aria-hidden="true"
                      className="h-3 w-3 shrink-0 text-primary"
                    />
                  </button>
                </li>
              ) : null}
              {onSelectSuggestion
                ? suggestions.map((suggestion) => (
                    <li key={suggestion.label}>
                      <button
                        className={cn(
                          'agent-suggestion-row group flex w-full items-center gap-2 px-2 py-1.5 text-left transition-colors',
                          'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                        )}
                        onClick={() => onSelectSuggestion(suggestion.prompt)}
                        type="button"
                      >
                        <suggestion.icon
                          data-suggestion-icon
                          className="h-3 w-3 shrink-0 text-muted-foreground group-hover:text-primary"
                        />
                        <span className="flex-1 truncate text-[11.5px] text-foreground/80 group-hover:text-foreground">
                          {suggestion.label}
                        </span>
                        <ChevronRight
                          data-suggestion-caret
                          aria-hidden="true"
                          className="h-3 w-3 shrink-0 text-muted-foreground/70"
                        />
                      </button>
                    </li>
                  ))
                : null}
            </ul>
          ) : null}
        </div>
      </div>
    )
  }

  return (
    <div className="relative flex min-h-full w-full items-center justify-center overflow-hidden">
      <div className="agent-empty-fade-in relative flex w-full max-w-xl flex-col items-center px-8 py-12 text-center">
        <BrandGlyph context={context} />

        {greeting ? (
          <p className="mt-5 text-[12px] font-medium uppercase tracking-[0.18em] text-muted-foreground/80">
            {greeting}
          </p>
        ) : null}

        <h2 className="mt-3 text-2xl font-semibold tracking-tight text-foreground sm:text-[26px]">
          {isAgentCreate ? (
            'Shape this new agent'
          ) : (
            <>
              What can we build together in <span className="text-primary">{projectLabel}</span>?
            </>
          )}
        </h2>
        <p className="mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
          {isAgentCreate
            ? agentCreateCanvasIncluded
              ? 'The canvas is already included. Describe the role, boundaries, and workflow to draft the agent.'
              : 'Start from a description. Agent Create will draft a definition for review.'
            : 'Just ask — I can read your code, suggest changes, or run a task for you. Everything we do will show up right here.'}
        </p>

        {onSelectSuggestion || showWorkflowCanvasAction ? (
          <ul className="mt-8 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/40 backdrop-blur-sm">
            {showWorkflowCanvasAction ? (
              <li>
                <button
                  className={cn(
                    'agent-suggestion-row group flex w-full items-center gap-3 bg-primary/10 px-4 py-3.5 text-left transition-colors',
                    'hover:bg-primary/15 focus-visible:bg-primary/15 focus-visible:outline-none',
                  )}
                  onClick={onStartWorkflowAgentCreate}
                  type="button"
                >
                  <span
                    data-suggestion-icon
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/15 text-primary"
                  >
                    <Workflow className="h-3.5 w-3.5" />
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate text-[13px] font-medium text-foreground">
                      Start on workflow canvas
                    </span>
                    <span className="truncate text-[11.5px] text-muted-foreground">
                      Open Workflow with the canvas included
                    </span>
                  </span>
                  <ChevronRight
                    data-suggestion-caret
                    aria-hidden="true"
                    className="h-3.5 w-3.5 shrink-0 text-primary"
                  />
                </button>
              </li>
            ) : null}
            {onSelectSuggestion
              ? suggestions.map((suggestion) => (
                  <li key={suggestion.label}>
                    <button
                      className={cn(
                        'agent-suggestion-row group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors',
                        'hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none',
                      )}
                      onClick={() => onSelectSuggestion(suggestion.prompt)}
                      type="button"
                    >
                      <suggestion.icon
                        data-suggestion-icon
                        className="h-3.5 w-3.5 shrink-0 text-muted-foreground group-hover:text-primary"
                      />
                      <span className="flex-1 truncate text-[13px] text-foreground/85 group-hover:text-foreground">
                        {suggestion.label}
                      </span>
                      <ChevronRight
                        data-suggestion-caret
                        aria-hidden="true"
                        className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                      />
                    </button>
                  </li>
                ))
              : null}
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

function BrandGlyph({ context }: { context: EmptySessionStateProps['context'] }) {
  const isAgentCreate = context === 'agent-create'

  return (
    <div className="relative">
      <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-card/60">
        {isAgentCreate ? <Sparkles className="h-6 w-6 text-primary" /> : <AppLogo className="h-7 w-7" />}
      </div>
    </div>
  )
}
