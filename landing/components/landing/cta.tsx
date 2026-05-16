import Link from "next/link"
import { Download, Github, ShieldCheck } from "lucide-react"
import { Button } from "@/components/ui/button"
import { InstallCommand } from "@/components/landing/install-command"
import { siteConfig, tuiInstallCommand, tuiPowerShellInstallCommand } from "@/lib/site"

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
        <h2 className="mx-auto max-w-3xl font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl lg:text-6xl">
          Install Xero TUI. <br className="hidden sm:block" />
          <span className="text-muted-foreground">Bring your own keys.</span>
        </h2>
        <p className="mx-auto mt-5 max-w-xl text-pretty text-muted-foreground">
          The terminal build is packaged by GitHub CI and served from this Fly
          app. The installer picks the right archive, verifies its SHA-256
          checksum, and drops `xero-tui` into your local bin directory.
        </p>

        <div className="mx-auto mt-8 grid max-w-3xl gap-3">
          <InstallCommand
            command={tuiInstallCommand}
            label="macOS and Linux"
            tone="primary"
          />
          <InstallCommand command={tuiPowerShellInstallCommand} label="Windows PowerShell" />
        </div>

        <div className="mt-8 flex flex-wrap justify-center gap-3">
          <Button
            asChild
            size="lg"
            className="h-11 gap-2 bg-primary px-5 text-primary-foreground shadow-[0_8px_24px_-12px_color-mix(in_oklab,var(--primary)_70%,transparent)] hover:bg-primary/90"
          >
            <Link href={siteConfig.githubUrl} target="_blank" rel="noopener noreferrer">
              <Github className="h-4 w-4" />
              Run it locally
            </Link>
          </Button>
          <Button
            asChild
            size="lg"
            variant="secondary"
            className="h-11 gap-2 border border-border/70 bg-secondary/70 px-5 text-secondary-foreground hover:bg-secondary"
          >
            <Link href="/install.sh">
              <Download className="h-4 w-4" />
              View install script
            </Link>
          </Button>
        </div>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-[11px] text-muted-foreground/70">
          <span className="inline-flex items-center gap-1.5">
            <ShieldCheck className="h-3.5 w-3.5 text-primary" />
            Checksummed release archives
          </span>
          <span className="font-mono">/downloads/tui/latest</span>
        </div>
      </div>
    </section>
  )
}
