import Link from "next/link"
import Image from "next/image"
import { Github, Twitter, MessageCircle } from "lucide-react"

const cols = [
  {
    title: "Product",
    items: ["Download", "Models", "Changelog", "Roadmap", "Status"],
  },
  {
    title: "Resources",
    items: ["Docs", "Agent architecture", "Security", "Benchmarks", "Examples"],
  },
  {
    title: "Company",
    items: ["About", "Careers", "Press kit", "Contact", "Brand"],
  },
  {
    title: "Legal",
    items: ["Privacy", "Terms", "DPA", "Subprocessors", "Responsible disclosure"],
  },
]

export function SiteFooter() {
  return (
    <footer className="border-t border-border/60 bg-background">
      <div className="mx-auto w-full max-w-7xl px-4 py-14 sm:px-6 lg:px-8">
        <div className="grid grid-cols-2 gap-8 md:grid-cols-6">
          <div className="col-span-2">
            <Link href="/" className="inline-flex items-center gap-2">
              <Image
                src="/logo.svg"
                alt="Cadence"
                width={112}
                height={26}
                className="h-5 w-auto opacity-95"
              />
            </Link>
            <p className="mt-4 max-w-sm text-sm leading-relaxed text-muted-foreground">
              The agentic coding studio for people who still care about what ships.
              Built in Rust, runs on your desktop, works with the AI subscriptions
              you already pay for.
            </p>
            <div className="mt-5 flex items-center gap-2">
              {[
                { icon: Github, label: "GitHub" },
                { icon: Twitter, label: "X" },
                { icon: MessageCircle, label: "Discord" },
              ].map(({ icon: Icon, label }) => (
                <Link
                  key={label}
                  href="#"
                  aria-label={label}
                  className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-border/60 bg-card text-muted-foreground transition-colors hover:border-border hover:text-foreground"
                >
                  <Icon className="h-4 w-4" />
                </Link>
              ))}
            </div>
          </div>

          {cols.map((c) => (
            <div key={c.title}>
              <p className="text-xs font-medium uppercase tracking-wider text-foreground">
                {c.title}
              </p>
              <ul className="mt-4 space-y-2.5">
                {c.items.map((i) => (
                  <li key={i}>
                    <Link
                      href="#"
                      className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                    >
                      {i}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-12 flex flex-col items-start justify-between gap-4 border-t border-border/60 pt-6 text-xs text-muted-foreground sm:flex-row sm:items-center">
          <span>© {new Date().getFullYear()} Cadence Labs, Inc. All rights reserved.</span>
          <span className="font-mono">
            crafted in Rust · v0.9.2 · 42MB idle
          </span>
        </div>
      </div>
    </footer>
  )
}
