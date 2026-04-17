const logos = [
  "Axiom Labs",
  "Northwind",
  "Cobalt",
  "Helix",
  "Parallel",
  "Vector",
  "Meridian",
  "Prism",
]

export function LogoCloud() {
  return (
    <section className="border-y border-border/60 bg-background">
      <div className="mx-auto w-full max-w-7xl px-4 py-10 sm:px-6 lg:px-8">
        <p className="text-center font-mono text-xs uppercase tracking-[0.2em] text-muted-foreground">
          Trusted by engineering teams shipping faster than their standups
        </p>
        <div className="mt-6 grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4 lg:grid-cols-8">
          {logos.map((l) => (
            <div
              key={l}
              className="flex items-center justify-center text-center font-mono text-sm text-muted-foreground/70 transition-colors hover:text-foreground"
            >
              {l}
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
