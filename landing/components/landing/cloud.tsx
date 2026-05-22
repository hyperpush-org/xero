import Link from "next/link"
import { ArrowUpRight, CheckCheck, Github, MonitorSmartphone, ShieldCheck } from "lucide-react"
import { Button } from "@/components/ui/button"
import { siteConfig } from "@/lib/site"

export function CloudApp() {
  return (
    <section
      id="cloud"
      className="relative border-y border-border/60 bg-secondary/10"
    >
      <div className="mx-auto grid w-full max-w-7xl grid-cols-1 gap-10 px-4 py-20 sm:px-6 lg:grid-cols-2 lg:gap-16 lg:px-8 lg:py-28">
        <div className="flex flex-col justify-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Xero Cloud
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Your sessions, everywhere.
          </h2>
          <p className="mt-4 max-w-xl text-pretty text-muted-foreground">
            The coding sessions running on your computer follow you to{" "}
            <span className="text-foreground">any browser</span>, on{" "}
            <span className="text-foreground">any device</span>. Start a run,
            send a message, approve a step, or cancel — and your desktop picks
            up right where it stopped. One continuous thread that travels with
            you.
          </p>

          <ul className="mt-8 space-y-3">
            {[
              {
                icon: MonitorSmartphone,
                title: "Drive from any browser",
                copy: "Phone, tablet, or laptop — pick up a live session at any size, nothing to install.",
              },
              {
                icon: CheckCheck,
                title: "Approve without your desk",
                copy: "When a run needs you, resolve it from the cloud app with the real diff or command inline.",
              },
              {
                icon: ShieldCheck,
                title: "Secure by default",
                copy: "GitHub OAuth with scoped per-device tokens. Your code and keys stay on your machine.",
              },
            ].map((f) => (
              <li key={f.title} className="flex gap-3">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <f.icon className="h-3 w-3" />
                </span>
                <div>
                  <p className="text-sm font-medium">{f.title}</p>
                  <p className="text-sm text-muted-foreground">{f.copy}</p>
                </div>
              </li>
            ))}
          </ul>

          <div className="mt-8 flex flex-wrap gap-3">
            <Button asChild className="gap-2 bg-primary text-primary-foreground">
              <Link href={siteConfig.cloudUrl}>
                Open Xero Cloud
                <ArrowUpRight className="h-4 w-4" />
              </Link>
            </Button>
          </div>
        </div>

        <div className="relative flex items-center justify-center">
          <div
            aria-hidden
            className="absolute inset-0 -z-10 rounded-3xl bg-gradient-to-br from-primary/10 via-transparent to-transparent blur-2xl"
          />
          <CloudDeviceMock />
        </div>
      </div>
    </section>
  )
}

type SessionState = "active" | "idle"

interface PreviewSession {
  name: string
  project: string
  state: SessionState
}

const PREVIEW_SESSIONS: PreviewSession[] = [
  { name: "Refactor auth middleware", project: "xero-server", state: "active" },
  { name: "Fix nav overflow on iPad", project: "cloud", state: "idle" },
  { name: "Add session export to JSON", project: "client", state: "idle" },
]

function CloudDeviceMock() {
  return (
    <div className="relative w-full max-w-[19rem]">
      {/* Phone frame */}
      <div className="overflow-hidden rounded-[2rem] border border-border/70 bg-card/80 p-2 shadow-[0_24px_60px_-24px_rgba(0,0,0,0.55)] backdrop-blur-sm">
        <div className="overflow-hidden rounded-[1.5rem] border border-border/60 bg-background/60">
          {/* Status / app bar */}
          <div className="flex items-center gap-2 border-b border-border/60 px-4 py-3">
            <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-primary/15 text-primary">
              <Github className="h-3 w-3" />
            </span>
            <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
              Sessions
            </span>
            <span className="ml-auto inline-flex items-center gap-1.5">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400/90 shadow-[0_0_8px_rgba(52,211,153,0.6)]" />
              <span className="font-mono text-[10.5px] text-muted-foreground">
                relay
              </span>
            </span>
          </div>

          {/* Session list */}
          <ul className="divide-y divide-border/50">
            {PREVIEW_SESSIONS.map((s) => (
              <li
                key={s.name}
                className="flex items-center gap-3 px-4 py-3.5"
              >
                <span
                  className={
                    s.state === "active"
                      ? "h-1.5 w-1.5 shrink-0 rounded-full bg-primary shadow-[0_0_8px_color-mix(in_oklab,var(--primary)_70%,transparent)]"
                      : "h-1.5 w-1.5 shrink-0 rounded-full bg-muted-foreground/30"
                  }
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] font-medium leading-snug text-foreground">
                    {s.name}
                  </p>
                  <p className="truncate font-mono text-[11px] text-muted-foreground">
                    {s.project}
                  </p>
                </div>
                {s.state === "active" ? (
                  <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-primary/90">
                    live
                  </span>
                ) : null}
              </li>
            ))}
          </ul>

          {/* Footer action */}
          <div className="flex items-center gap-2 border-t border-border/60 px-4 py-3">
            <span className="inline-flex h-7 flex-1 items-center justify-center gap-1.5 rounded-md bg-primary px-2.5 text-[12px] font-medium text-primary-foreground">
              <MonitorSmartphone className="h-3 w-3" strokeWidth={2.5} />
              Add to Home Screen
            </span>
          </div>
        </div>
      </div>
    </div>
  )
}
