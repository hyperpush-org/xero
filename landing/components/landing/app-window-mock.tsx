import {
  CheckCircle2,
  Circle,
  Loader2,
  FolderTree,
  FileCode2,
  GitBranch,
  Play,
  Cpu,
} from "lucide-react"

const steps = [
  { label: "Parse spec & scaffold Next.js app", state: "done" as const },
  { label: "Provision Postgres + run migrations", state: "done" as const },
  { label: "Generate auth flow (credentials + OAuth)", state: "done" as const },
  { label: "Implement billing w/ Stripe checkout", state: "running" as const },
  { label: "Write Playwright e2e suite", state: "queued" as const },
  { label: "Deploy preview to Vercel", state: "queued" as const },
]

const files = [
  { name: "app/", depth: 0, type: "folder" as const },
  { name: "layout.tsx", depth: 1, type: "file" as const },
  { name: "page.tsx", depth: 1, type: "file" as const, active: true },
  { name: "(auth)/", depth: 1, type: "folder" as const },
  { name: "login/page.tsx", depth: 2, type: "file" as const },
  { name: "billing/route.ts", depth: 1, type: "file" as const, changed: true },
  { name: "components/", depth: 0, type: "folder" as const },
  { name: "pricing.tsx", depth: 1, type: "file" as const },
  { name: "lib/", depth: 0, type: "folder" as const },
  { name: "db.ts", depth: 1, type: "file" as const },
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
          <span className="font-mono">cadence / acme-saas</span>
          <span className="mx-1 text-border">·</span>
          <span className="font-mono text-primary">main</span>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Cpu className="h-3.5 w-3.5" />
          <span className="font-mono">42MB</span>
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
        </aside>

        {/* Middle: agent run */}
        <div className="col-span-12 p-4 md:col-span-6">
          <div className="mb-3 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <div className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-70" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-primary" />
              </div>
              <span className="text-sm font-medium">Agent · build SaaS with auth &amp; billing</span>
            </div>
            <span className="rounded-md border border-border/70 bg-secondary/40 px-2 py-0.5 font-mono text-[11px] text-muted-foreground">
              step 4 / 6
            </span>
          </div>

          <ol className="space-y-2">
            {steps.map((s, i) => (
              <li
                key={s.label}
                className="flex items-start gap-3 rounded-lg border border-border/60 bg-background/40 px-3 py-2"
              >
                <div className="mt-0.5 shrink-0">
                  {s.state === "done" && (
                    <CheckCircle2 className="h-4 w-4 text-primary" />
                  )}
                  {s.state === "running" && (
                    <Loader2 className="h-4 w-4 animate-spin text-primary" />
                  )}
                  {s.state === "queued" && (
                    <Circle className="h-4 w-4 text-muted-foreground/60" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <div
                    className={`text-sm ${
                      s.state === "queued"
                        ? "text-muted-foreground"
                        : "text-foreground"
                    }`}
                  >
                    {s.label}
                  </div>
                  {s.state === "running" && (
                    <div className="mt-1 font-mono text-[11px] text-muted-foreground">
                      <span className="text-primary">→</span> running `cargo test billing` · 8.2s
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

        {/* Right: approval / notification */}
        <div className="col-span-12 flex flex-col gap-3 border-t border-border/60 p-4 md:col-span-3 md:border-t-0">
          <div className="rounded-lg border border-primary/30 bg-primary/[0.06] p-3">
            <div className="flex items-center gap-2 text-xs font-medium text-primary">
              <span className="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-primary" />
              Decision needed
            </div>
            <p className="mt-1.5 text-sm text-foreground">
              Stripe test key detected. Use <span className="font-mono text-primary">test</span>{" "}
              mode or prompt for live keys?
            </p>
            <div className="mt-3 flex gap-2">
              <button className="flex-1 rounded-md bg-primary px-2.5 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90">
                Test mode
              </button>
              <button className="flex-1 rounded-md border border-border/70 bg-secondary/40 px-2.5 py-1.5 text-xs font-medium text-foreground hover:bg-secondary">
                Prompt me
              </button>
            </div>
            <p className="mt-3 font-mono text-[10px] text-muted-foreground">
              pinged @you on Discord · 00:02s ago
            </p>
          </div>

          <div className="rounded-lg border border-border/60 bg-background/40 p-3">
            <div className="mb-2 flex items-center gap-2 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              <Play className="h-3 w-3" />
              Preview
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
            agent: claude-opus-4.6
          </span>
          <span>harness: rust-tokio</span>
          <span className="hidden sm:inline">persistence: sqlite</span>
        </div>
        <div className="flex items-center gap-3">
          <span className="hidden sm:inline">123 tool calls</span>
          <span>via your ChatGPT plan</span>
          <span className="text-primary">●</span>
        </div>
      </div>
    </div>
  )
}
