import {
  CheckCircle2,
  Loader2,
  FolderTree,
  FileCode2,
  GitBranch,
  Play,
  Cpu,
  PauseCircle,
  Bot,
} from "lucide-react"

const events = [
  { kind: "tool", label: "repo.read · src/billing.ts", state: "done" as const },
  { kind: "tool", label: "repo.edit · extract retry helper", state: "done" as const },
  { kind: "tool", label: "shell · cargo test billing", state: "done" as const, detail: "12 passed · 0 failed · 8.2s" },
  { kind: "tool", label: "git.commit · refactor: extract retry helper", state: "done" as const },
  { kind: "tool", label: "browser · localhost:3000/billing", state: "running" as const, detail: "navigating · captured a11y snapshot" },
  { kind: "ask", label: "approval · push branch `try-pg` to origin?", state: "paused" as const },
]

const files = [
  { name: "app/", depth: 0, type: "folder" as const },
  { name: "layout.tsx", depth: 1, type: "file" as const },
  { name: "page.tsx", depth: 1, type: "file" as const },
  { name: "(auth)/", depth: 1, type: "folder" as const },
  { name: "login/page.tsx", depth: 2, type: "file" as const },
  { name: "billing/", depth: 1, type: "folder" as const },
  { name: "billing.ts", depth: 2, type: "file" as const, active: true, changed: true },
  { name: "billing.test.ts", depth: 2, type: "file" as const, changed: true },
  { name: "components/", depth: 0, type: "folder" as const },
  { name: "lib/", depth: 0, type: "folder" as const },
]

const panes = [
  { role: "Engineer", model: "claude-opus-4.7", state: "running" },
  { role: "Debug", model: "gpt-5", state: "running" },
  { role: "Ask", model: "gemini-2.5-pro", state: "idle" },
  { role: "Engineer", model: "qwen3:32b · ollama", state: "running" },
  { role: "Engineer", model: "via openrouter", state: "decision" },
  { role: "solana-ops", model: "claude-sonnet-4.6", state: "idle" },
]

export function AppWindowMock() {
  return (
    <div className="overflow-hidden rounded-xl border border-border/80 bg-card shadow-[0_40px_120px_-20px_rgba(0,0,0,0.6)] ring-1 ring-black/5">
      {/* Title bar */}
      <div className="flex h-10 items-center gap-3 border-b border-border/80 bg-secondary/40 px-4">
        <div className="flex gap-1.5">
          <span className="h-3 w-3 rounded-full bg-[#ff5f57]/80" />
          <span className="h-3 w-3 rounded-full bg-[#febc2e]/80" />
          <span className="h-3 w-3 rounded-full bg-[#28c840]/80" />
        </div>
        <div className="mx-auto flex items-center gap-2 rounded-md border border-border/60 bg-background/60 px-3 py-1 text-xs text-muted-foreground">
          <GitBranch className="h-3 w-3" />
          <span className="font-mono">xero / acme-saas</span>
          <span className="mx-1 text-border">·</span>
          <span className="font-mono text-primary">try-pg</span>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Cpu className="h-3.5 w-3.5" />
          <span className="font-mono">6 panes · live</span>
        </div>
      </div>

      <div className="grid grid-cols-12 divide-x divide-border/60">
        {/* Sidebar: file tree */}
        <aside className="col-span-12 hidden border-b border-border/60 p-3 md:col-span-3 md:block md:border-b-0">
          <div className="mb-2 flex items-center gap-1.5 px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
            <FolderTree className="h-3 w-3" />
            Files
          </div>
          <ul className="space-y-0.5 text-sm">
            {files.map((f) => (
              <li
                key={f.name + f.depth}
                className={`flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] ${
                  f.active
                    ? "bg-secondary text-foreground"
                    : "text-muted-foreground hover:bg-secondary/40"
                }`}
                style={{ paddingLeft: 8 + f.depth * 12 }}
              >
                {f.type === "folder" ? (
                  <FolderTree className="h-3.5 w-3.5 shrink-0 opacity-70" />
                ) : (
                  <FileCode2 className="h-3.5 w-3.5 shrink-0 opacity-70" />
                )}
                <span className="truncate font-mono">{f.name}</span>
                {f.changed && (
                  <span className="ml-auto h-1.5 w-1.5 rounded-full bg-primary" />
                )}
              </li>
            ))}
          </ul>

          <div className="mt-5 space-y-1.5">
            <div className="px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              Sessions
            </div>
            {panes.map((p, i) => (
              <div
                key={i}
                className={`flex items-center gap-2 rounded-md border px-2 py-1.5 text-[11px] ${
                  p.state === "decision"
                    ? "border-primary/40 bg-primary/[0.06]"
                    : "border-border/60 bg-background/40"
                }`}
              >
                <Bot
                  className={`h-3 w-3 shrink-0 ${
                    p.state === "running" || p.state === "decision"
                      ? "text-primary"
                      : "text-muted-foreground/70"
                  }`}
                />
                <span className="truncate font-mono text-foreground/90">{p.role}</span>
                <span
                  className={`ml-auto text-[9px] uppercase tracking-wider ${
                    p.state === "running" || p.state === "decision"
                      ? "text-primary"
                      : "text-muted-foreground/70"
                  }`}
                >
                  {p.state}
                </span>
              </div>
            ))}
          </div>
        </aside>

        {/* Middle: agent run */}
        <div className="col-span-12 p-4 md:col-span-6">
          <div className="mb-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className="inline-flex h-2 w-2 rounded-full bg-primary" />
              <span className="text-sm font-medium">Engineer · refactor billing module</span>
            </div>
            <span className="rounded-md border border-border/70 bg-secondary/40 px-2 py-0.5 font-mono text-[11px] text-muted-foreground">
              event 6 / 6
            </span>
          </div>

          <ol className="space-y-2">
            {events.map((s, i) => (
              <li
                key={s.label}
                className={`flex items-start gap-3 rounded-lg border px-3 py-2 ${
                  s.state === "paused"
                    ? "border-primary/40 bg-primary/[0.06]"
                    : "border-border/60 bg-background/40"
                }`}
              >
                <div className="mt-0.5 shrink-0">
                  {s.state === "done" && (
                    <CheckCircle2 className="h-4 w-4 text-primary" />
                  )}
                  {s.state === "running" && (
                    <Loader2 className="h-4 w-4 text-primary" />
                  )}
                  {s.state === "paused" && (
                    <PauseCircle className="h-4 w-4 text-primary" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-sm text-foreground">
                    <span className="mr-2 font-mono text-[10px] uppercase tracking-wider text-muted-foreground/70">
                      {s.kind}
                    </span>
                    {s.label}
                  </div>
                  {s.detail && (
                    <div className="mt-1 font-mono text-[11px] text-muted-foreground">
                      <span className="text-primary">→</span> {s.detail}
                    </div>
                  )}
                </div>
                <span className="font-mono text-[10px] text-muted-foreground">
                  {String(i + 1).padStart(2, "0")}
                </span>
              </li>
            ))}
          </ol>
        </div>

        {/* Right: approval / preview */}
        <div className="col-span-12 flex flex-col gap-3 border-t border-border/60 p-4 md:col-span-3 md:border-t-0">
          <div className="rounded-lg border border-primary/30 bg-primary/[0.06] p-3">
            <div className="flex items-center gap-2 text-xs font-medium text-primary">
              <span className="h-1.5 w-1.5 rounded-full bg-primary" />
              Decision needed
            </div>
            <p className="mt-1.5 text-sm text-foreground">
              Push branch <span className="font-mono text-primary">try-pg</span>{" "}
              to <span className="font-mono">origin</span>? 4 commits ahead.
            </p>
            <div className="mt-3 flex gap-2">
              <button className="flex-1 rounded-md bg-primary px-2.5 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90">
                Approve
              </button>
              <button className="flex-1 rounded-md border border-border/70 bg-secondary/40 px-2.5 py-1.5 text-xs font-medium text-foreground hover:bg-secondary">
                Skip
              </button>
            </div>
            <p className="mt-3 font-mono text-[10px] text-muted-foreground">
              pinged @you on Discord · 00:02s ago
            </p>
          </div>

          <div className="rounded-lg border border-border/60 bg-background/40 p-3">
            <div className="mb-2 flex items-center gap-2 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              <Play className="h-3 w-3" />
              Browser pane
            </div>
            <div className="space-y-1.5">
              <div className="h-1.5 w-3/4 rounded-full bg-secondary" />
              <div className="h-1.5 w-1/2 rounded-full bg-secondary" />
              <div className="mt-3 grid grid-cols-2 gap-1.5">
                <div className="h-10 rounded-md bg-secondary/70" />
                <div className="h-10 rounded-md bg-secondary/70" />
              </div>
              <div className="mt-2 h-6 w-2/3 rounded-md bg-primary/70" />
            </div>
          </div>
        </div>
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between border-t border-border/80 bg-secondary/40 px-4 py-1.5 font-mono text-[11px] text-muted-foreground">
        <div className="flex items-center gap-3">
          <span className="flex items-center gap-1">
            <span className="h-1.5 w-1.5 rounded-full bg-primary" />
            engineer · claude-opus-4.7
          </span>
          <span className="hidden sm:inline">persistence: local sqlite</span>
          <span className="hidden md:inline">redaction: on</span>
        </div>
        <div className="flex items-center gap-3">
          <span className="hidden sm:inline">7 tools wired</span>
          <span>via your own provider keys</span>
          <span className="text-primary">●</span>
        </div>
      </div>
    </div>
  )
}
