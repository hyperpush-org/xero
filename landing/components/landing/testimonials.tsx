import { Quote } from "lucide-react"

type Testimonial = {
  quote: string
  name: string
  role: string
  initials: string
  colorClass: string
  size?: "lg" | "md" | "sm"
}

const testimonials: Testimonial[] = [
  {
    quote:
      "I kicked off a Stripe-powered SaaS before a flight. Landed, opened my laptop, and Xero was waiting with a green PR and a deploy preview. It had pinged me on Telegram twice mid-air. I just hadn't replied yet.",
    name: "Maya Chen",
    role: "Founder, Northwind",
    initials: "MC",
    colorClass: "bg-primary/20 text-primary",
    size: "lg",
  },
  {
    quote:
      "The critic agent is the thing. It actually fails bad diffs instead of rubber-stamping them. Finally a tool I'd let open a PR unsupervised.",
    name: "Dmitri Volkov",
    role: "Staff Eng, Axiom Labs",
    initials: "DV",
    colorClass: "bg-chart-2/20 text-chart-2",
    size: "md",
  },
  {
    quote:
      "Runs on my existing Claude Max plan. No new billing relationship, no data leaving the laptop. Legal signed off in a day.",
    name: "Priya Raghavan",
    role: "CTO, Helix Health",
    initials: "PR",
    colorClass: "bg-chart-3/25 text-chart-2",
    size: "sm",
  },
  {
    quote:
      "Resumed a 3-day-old run this morning like it was a browser tab. The SQLite journal is genuinely magic. I stopped worrying about losing context entirely.",
    name: "Jonas Keller",
    role: "Indie dev, shipyard.sh",
    initials: "JK",
    colorClass: "bg-primary/15 text-primary",
    size: "md",
  },
  {
    quote:
      "We replaced three web-based agent seats with Xero and a shared OpenRouter key. Same output, a third of the bill, and no more stale browser sessions.",
    name: "Eve Okafor",
    role: "Head of Eng, Cobalt",
    initials: "EO",
    colorClass: "bg-chart-2/20 text-chart-2",
    size: "sm",
  },
  {
    quote:
      "The Discord integration is absurdly well-tuned. I get the exact diff and tradeoff, not a vague 'need your input'. I reply in one sentence from bed.",
    name: "Theo Marín",
    role: "Solo founder, Prism",
    initials: "TM",
    colorClass: "bg-primary/20 text-primary",
    size: "md",
  },
]

export function Testimonials() {
  return (
    <section className="relative border-y border-border/60 bg-background">
      <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 lg:px-8 lg:py-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Ships in production
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Loved by people who actually ship.
          </h2>
          <p className="mt-4 text-pretty text-muted-foreground">
            Founders, staff engineers, and indie devs use Xero to turn briefs into
            shipped product while they sleep, commute, and go outside.
          </p>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-4 md:grid-cols-6 md:grid-rows-[auto_auto] lg:gap-5">
          {testimonials.map((t) => (
            <TestimonialCard key={t.name} t={t} />
          ))}
        </div>
      </div>
    </section>
  )
}

function TestimonialCard({ t }: { t: Testimonial }) {
  // Bento span classes
  const span =
    t.size === "lg"
      ? "md:col-span-4 md:row-span-2"
      : t.size === "md"
        ? "md:col-span-2"
        : "md:col-span-2"

  return (
    <figure
      className={`group relative flex h-full flex-col gap-4 overflow-hidden rounded-2xl border border-border/70 bg-card p-6 transition-all hover:-translate-y-0.5 hover:border-border hover:shadow-[0_20px_50px_-30px_rgba(0,0,0,0.6)] ${span}`}
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent opacity-0 transition-opacity group-hover:opacity-100"
      />
      <Quote
        className="h-5 w-5 shrink-0 text-primary/70"
        aria-hidden
      />
      <blockquote
        className={`text-pretty leading-relaxed text-foreground/90 ${
          t.size === "lg" ? "text-xl sm:text-2xl" : "text-base"
        }`}
      >
        {t.quote}
      </blockquote>
      <figcaption className="mt-auto flex items-center gap-3 border-t border-border/60 pt-4">
        <span
          className={`inline-flex h-9 w-9 items-center justify-center rounded-full font-mono text-xs font-semibold ring-1 ring-inset ring-border/50 ${t.colorClass}`}
          aria-hidden
        >
          {t.initials}
        </span>
        <div className="min-w-0">
          <div className="truncate text-sm font-medium">{t.name}</div>
          <div className="truncate text-xs text-muted-foreground">{t.role}</div>
        </div>
      </figcaption>
    </figure>
  )
}
