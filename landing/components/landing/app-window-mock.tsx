import {
  ArrowUp,
  Bell,
  Bot,
  Brain,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  GitBranch,
  GitCompareArrows,
  Globe,
  Lightbulb,
  MessageCircle,
  Mic,
  PanelLeftClose,
  Plus,
  Search,
  Settings,
  Sparkles,
  Workflow as WorkflowIcon,
} from "lucide-react"

const projects = [
  { initial: "C", active: false },
  { initial: "M", active: false },
  { initial: "X", active: true },
]

const suggestions = [
  { icon: Search, label: "Explore the codebase" },
  { icon: GitBranch, label: "Review recent commits" },
  { icon: Lightbulb, label: "Suggest next steps" },
]

const tabs = [
  { label: "Workflow", active: false },
  { label: "Agent", active: true },
  { label: "Editor", active: false },
]

function AppleLogoIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      fill="currentColor"
      className={className}
      aria-hidden
    >
      <path d="M17.05 20.28c-.98.95-2.05.8-3.08.35-1.09-.46-2.09-.48-3.24 0-1.44.62-2.2.44-3.06-.35C2.79 15.25 3.51 7.59 9.05 7.31c1.35.07 2.29.74 3.08.8 1.18-.24 2.31-.93 3.57-.84 1.51.12 2.65.72 3.4 1.8-3.12 1.87-2.38 5.98.48 7.13-.57 1.5-1.31 2.99-2.54 4.09zM12.03 7.25c-.15-2.23 1.66-4.07 3.74-4.25.29 2.58-2.34 4.5-3.74 4.25z" />
    </svg>
  )
}

function SolanaLogoIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      fill="currentColor"
      className={className}
      aria-hidden
    >
      <path d="M23.876 18.362l-4.017 4.326a.93.93 0 01-.723.31H.452a.452.452 0 01-.33-.764l4.021-4.325a.93.93 0 01.72-.311h18.686a.452.452 0 01.328.764zM19.859 9.648a.93.93 0 00-.723-.31H.452a.452.452 0 00-.33.763l4.021 4.325a.93.93 0 00.72.31h18.686a.452.452 0 00.328-.764L19.859 9.65zM.452 6.574h18.684a.93.93 0 00.723-.31l4.017-4.326A.452.452 0 0023.6 1.175H4.915a.93.93 0 00-.72.31L.178 5.811a.452.452 0 00.274.763z" />
    </svg>
  )
}

function XeroGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 455 455"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-hidden
    >
      <path
        d="M256.391 256.395H454.326V404.33C454.326 431.944 431.941 454.33 404.326 454.33H256.391V256.395Z"
        fill="var(--primary)"
      />
      <path
        d="M197.936 197.941L0.000289917 197.941L0.000276984 50.0064C0.00027457 22.3921 22.386 0.00637826 50.0003 0.00637585L197.936 0.00636292L197.936 197.941Z"
        fill="var(--primary)"
      />
      <path
        d="M0 256.395H197.935V454.33H50.0001C22.3858 454.33 0 431.944 0 404.33L0 256.395Z"
        fill="var(--foreground)"
        fillOpacity="0.32"
      />
      <path
        d="M256.392 0L404.327 0C431.941 0 454.327 22.3858 454.327 50V197.935H256.392V0Z"
        fill="var(--foreground)"
        fillOpacity="0.32"
      />
    </svg>
  )
}

export function AppWindowMock() {
  return (
    <div className="overflow-hidden rounded-xl border border-border/80 bg-background shadow-[0_40px_120px_-20px_rgba(0,0,0,0.6)] ring-1 ring-black/5">
      {/* Title bar */}
      <div className="relative flex h-11 items-center gap-2 border-b border-border bg-[#141414] px-3">
        <div className="flex gap-1.5">
          <span className="h-3 w-3 rounded-full bg-[#ff5f57]/85" />
          <span className="h-3 w-3 rounded-full bg-[#febc2e]/85" />
          <span className="h-3 w-3 rounded-full bg-[#28c840]/85" />
        </div>

        {/* Left tabs */}
        <div className="flex items-center gap-0.5 pl-2">
          {tabs.map((t) => (
            <span
              key={t.label}
              className={`rounded-md px-2.5 py-1 text-[12px] ${
                t.active
                  ? "bg-secondary/80 text-foreground"
                  : "text-muted-foreground"
              }`}
            >
              {t.label}
            </span>
          ))}
        </div>

        {/* Center project pill */}
        <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 flex items-center gap-1.5 text-[12px]">
          <XeroGlyph className="h-3.5 w-3.5" />
          <span className="font-medium text-foreground">Xero</span>
          <span className="text-muted-foreground/70">/</span>
          <span className="text-muted-foreground">xero</span>
        </div>

        {/* Right cluster */}
        <div className="ml-auto flex items-center gap-2 text-muted-foreground">
          <div className="flex items-center gap-1.5 rounded-md px-1.5 py-1">
            <GitCompareArrows className="h-4 w-4" />
            <span className="flex items-center gap-1 font-mono text-[10.5px] font-semibold leading-none tabular-nums">
              <span className="text-[#5fd97a]">+1.2k</span>
              <span className="text-[#ff6b6b]">−685</span>
            </span>
          </div>
          <span className="flex h-7 w-7 items-center justify-center rounded-md">
            <WorkflowIcon className="h-4 w-4" />
          </span>
          <span className="flex h-7 w-7 items-center justify-center rounded-md">
            <Bot className="h-[17px] w-[17px]" />
          </span>
          <span className="flex h-7 w-7 items-center justify-center rounded-md">
            <AppleLogoIcon className="h-[18px] w-[18px]" />
          </span>
          <span className="flex h-7 w-7 items-center justify-center rounded-md">
            <Globe className="h-4 w-4" />
          </span>
          <span className="flex h-7 w-7 items-center justify-center rounded-md">
            <SolanaLogoIcon className="h-4 w-4" />
          </span>
        </div>
      </div>

      {/* Body */}
      <div className="flex" style={{ minHeight: 680 }}>
        {/* Project rail */}
        <aside className="flex w-12 shrink-0 flex-col border-r border-border/70 bg-[#141414]">
          <ul className="flex flex-1 flex-col items-center gap-1 px-2 py-2.5">
            {projects.map((p) => (
              <li key={p.initial} className="w-full">
                <div
                  className={`mx-auto flex h-8 w-8 items-center justify-center rounded-lg border text-[12px] font-medium leading-none ${
                    p.active
                      ? "border-border/80 bg-secondary text-foreground"
                      : "border-transparent bg-secondary/30 text-foreground/55"
                  }`}
                >
                  {p.initial}
                </div>
              </li>
            ))}
            <li className="mt-1 w-full">
              <div className="mx-auto flex h-8 w-8 items-center justify-center rounded-lg border border-dashed border-border/70 text-muted-foreground/80">
                <Plus className="h-3.5 w-3.5" />
              </div>
            </li>
          </ul>
          <div className="flex items-center justify-center px-2 py-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground/80">
              <Settings className="h-4 w-4" />
            </div>
          </div>
        </aside>

        {/* Main column */}
        <div className="relative flex flex-1 flex-col">
          {/* Left expand handle, anchored to the seam between rail and main */}
          <div className="pointer-events-none absolute left-0 top-1/2 -translate-y-1/2 -translate-x-px">
            <div className="flex h-7 w-4 items-center justify-center rounded-r-md border border-l-0 border-border/60 bg-card/40">
              <ChevronRight className="h-3 w-3 text-muted-foreground" />
            </div>
          </div>

          {/* Right collapsed rail handle */}
          <div className="pointer-events-none absolute right-0 top-1/2 -translate-y-1/2 translate-x-px">
            <div className="flex h-7 w-4 items-center justify-center rounded-l-md border border-r-0 border-border/60 bg-card/40">
              <ChevronLeft className="h-3 w-3 text-muted-foreground" />
            </div>
          </div>

          {/* Breadcrumb + actions */}
          <div className="flex items-center justify-between px-5 pt-4 pb-2">
            <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
              <span>xero</span>
              <ChevronRight className="h-3 w-3" />
              <span className="text-foreground/80">Main</span>
            </div>
            <div className="flex items-center gap-3 text-[12px] text-muted-foreground">
              <span className="inline-flex items-center gap-1.5">
                <Plus className="h-3.5 w-3.5" />
                New Session
              </span>
              <PanelLeftClose className="h-3.5 w-3.5" />
            </div>
          </div>

          {/* Empty state */}
          <div className="flex flex-1 flex-col items-center justify-center px-8 pb-6 pt-2">
            <div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-border/70 bg-card/60">
              <XeroGlyph className="h-7 w-7" />
            </div>

            <h2 className="mt-5 text-center text-[22px] font-semibold tracking-tight text-foreground">
              What can we build together in{" "}
              <span className="text-primary">xero</span>?
            </h2>
            <p className="mt-3 max-w-md text-center text-[13px] leading-relaxed text-muted-foreground">
              Just ask. I can read your code, suggest changes, or run a task
              for you. Everything we do will show up right here.
            </p>

            <ul className="mt-7 flex w-full max-w-md flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/40">
              {suggestions.map((s) => (
                <li
                  key={s.label}
                  className="flex items-center gap-3 px-4 py-3 text-[13px]"
                >
                  <s.icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <span className="flex-1 truncate text-foreground/85">
                    {s.label}
                  </span>
                  <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70" />
                </li>
              ))}
            </ul>
          </div>

          {/* Composer dock (centered, capped width to match real app) */}
          <div className="px-4 pb-3">
            <div className="mx-auto w-full max-w-[720px]">
              <div className="overflow-hidden rounded-xl border border-border/60 bg-card/75 ring-1 ring-inset ring-foreground/[0.03] shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)]">
                <div className="px-3 pb-1.5 pt-2.5 text-[13px] leading-relaxed text-muted-foreground/60">
                  Ask anything to get started with OpenAI Codex.
                </div>
                <div className="border-t border-border/40 px-2 py-1.5">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto">
                      <span className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] font-medium text-muted-foreground/90">
                        <MessageCircle className="h-3 w-3 text-muted-foreground/70" />
                        Ask
                        <ChevronDown className="h-3 w-3 opacity-50" />
                      </span>
                      <span className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] font-medium text-muted-foreground/90">
                        <Sparkles className="h-3 w-3 text-muted-foreground/70" />
                        gpt-5.4
                        <ChevronDown className="h-3 w-3 opacity-50" />
                      </span>
                      <span className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] font-medium text-muted-foreground/90">
                        <Brain className="h-3 w-3 text-muted-foreground/70" />
                        Medium
                        <ChevronDown className="h-3 w-3 opacity-50" />
                      </span>
                    </div>
                    <div className="flex items-center gap-1 text-muted-foreground/70">
                      <span className="flex h-7 w-7 items-center justify-center rounded-md">
                        <span className="h-3.5 w-3.5 rounded-full border border-muted-foreground/40" />
                      </span>
                      <span className="flex h-7 w-7 items-center justify-center rounded-md">
                        <Sparkles className="h-3.5 w-3.5" />
                      </span>
                      <span className="flex h-7 w-7 items-center justify-center rounded-md">
                        <Mic className="h-3.5 w-3.5" />
                      </span>
                      <span className="flex h-7 w-7 items-center justify-center rounded-md bg-secondary text-foreground">
                        <ArrowUp className="h-3.5 w-3.5" />
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between border-t border-border bg-[#141414] px-3 py-1.5 text-[11px] text-muted-foreground">
        <div className="flex items-center gap-4">
          <span className="inline-flex items-center gap-1 font-mono">
            <GitBranch className="h-3 w-3" />
            main
            <span className="ml-1 opacity-70">↑0 ↓0</span>
          </span>
          <span className="hidden font-mono sm:inline">
            <span className="text-primary">●</span> 20 changes
          </span>
          <span className="hidden font-mono md:inline">
            269d200 save · about 8 hours ago
          </span>
        </div>
        <div className="flex items-center gap-4 font-mono">
          <span>0 tok</span>
          <span>$ $0.00</span>
          <span className="inline-flex items-center gap-1">
            <Bell className="h-3 w-3" />
            <span className="opacity-70">0</span>
          </span>
        </div>
      </div>
    </div>
  )
}
