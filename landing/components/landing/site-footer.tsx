import Link from "next/link"
import { Github } from "lucide-react"
import { siteConfig } from "@/lib/site"

const links = [
  { label: "Privacy", href: "/privacy" },
  { label: "Terms", href: "/terms" },
]

const social = [
  { icon: Github, label: "GitHub", href: siteConfig.githubUrl },
]

export function SiteFooter() {
  return (
    <footer className="relative">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-border/80 to-transparent"
      />
      <div className="mx-auto w-full max-w-7xl px-4 py-10 sm:px-6 lg:px-8">
        <div className="flex flex-col items-center justify-between gap-6 sm:flex-row">
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-border/60 bg-secondary/40 font-mono text-[11px] font-semibold tracking-tighter text-primary"
            >
              X
            </span>
            <span className="text-xs text-muted-foreground/70">
              © {new Date().getFullYear()} Xero Labs · Built in Rust, runs on your machine
            </span>
          </div>

          <nav className="flex items-center gap-5" aria-label="Footer">
            {links.map((l) => (
              <Link
                key={l.label}
                href={l.href}
                className="text-xs text-muted-foreground transition-colors hover:text-foreground"
              >
                {l.label}
              </Link>
            ))}
          </nav>

          <div className="flex items-center gap-1">
            {social.map(({ icon: Icon, label, href }) => (
              <Link
                key={label}
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                aria-label={label}
                className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-secondary/40 hover:text-foreground"
              >
                <Icon className="h-3.5 w-3.5" />
              </Link>
            ))}
          </div>
        </div>
      </div>
    </footer>
  )
}
