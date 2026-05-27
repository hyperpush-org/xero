import Link from "next/link"
import { Github, ShieldCheck, Terminal } from "lucide-react"
import { TuiInstall } from "@/components/landing/tui-install"
import { siteConfig, tuiInstallCommand, tuiPowerShellInstallCommand } from "@/lib/site"

const installTargets = [
  { id: "unix", label: "macOS / Linux", command: tuiInstallCommand },
  { id: "windows", label: "Windows", command: tuiPowerShellInstallCommand },
]

export function CTA() {
  return (
    <section id="download" className="relative isolate overflow-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-grid [mask-image:radial-gradient(ellipse_at_center,black_30%,transparent_70%)] opacity-30"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-radial-fade"
      />
      <div className="mx-auto w-full max-w-5xl px-4 py-24 text-center sm:px-6 lg:px-8 lg:py-32">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
          Xero TUI · The terminal edition
        </p>
        <h2 className="mx-auto mt-3 max-w-3xl font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl lg:text-6xl">
          Prefer the terminal? <br className="hidden sm:block" />
          <span className="text-muted-foreground">Install Xero TUI.</span>
        </h2>
        <p className="mx-auto mt-5 max-w-lg text-pretty text-muted-foreground">
          A shell-native interface for the same agents, workflows, and keys as
          the desktop app.
        </p>

        <div className="mx-auto mt-10 max-w-2xl">
          <div className="rounded-lg border border-border/70 bg-secondary/20 p-4 text-left backdrop-blur sm:p-5">
            <div className="mb-4 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <p className="inline-flex items-center gap-2 font-mono text-xs uppercase tracking-[0.18em] text-primary">
                  <Terminal className="h-3.5 w-3.5" />
                  Terminal edition
                </p>
                <h3 className="mt-2 text-xl font-medium tracking-tight text-foreground">
                  Install Xero TUI
                </h3>
              </div>
              <span className="text-xs text-muted-foreground">
                Same agents and keys, without leaving your shell.
              </span>
            </div>

            <TuiInstall targets={installTargets} />

            <div className="mt-5 flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-xs text-muted-foreground/80">
              <Link
                href="/install.sh"
                className="underline-offset-4 transition-colors hover:text-foreground hover:underline"
              >
                View install script
              </Link>
              <Link
                href={siteConfig.githubUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 underline-offset-4 transition-colors hover:text-foreground hover:underline"
              >
                <Github className="h-3.5 w-3.5" />
                Source on GitHub
              </Link>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
