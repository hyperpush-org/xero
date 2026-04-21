import Link from "next/link"
import { Github } from "lucide-react"
import { DiscordIcon, XBrandIcon } from "@/components/landing/brand-icons"

const links = [
  { label: "Docs", href: "#" },
  { label: "Changelog", href: "#" },
  { label: "Privacy", href: "#" },
  { label: "Terms", href: "#" },
]

const social = [
  { icon: Github, label: "GitHub", href: "#" },
  { icon: XBrandIcon, label: "X", href: "#" },
  { icon: DiscordIcon, label: "Discord", href: "#" },
]

export function SiteFooter() {
  return (
    <footer className="border-t border-border/60">
      <div className="mx-auto w-full max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
        <div className="flex flex-col items-center justify-between gap-4 sm:flex-row">
          <span className="text-xs text-muted-foreground/60">
            © {new Date().getFullYear()} Xero Labs
          </span>

          <nav className="flex items-center gap-5">
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

          <div className="flex items-center gap-2">
            {social.map(({ icon: Icon, label, href }) => (
              <Link
                key={label}
                href={href}
                aria-label={label}
                className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:text-foreground"
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
