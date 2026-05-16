import { Check, CheckCheck, FileDiff, Hash } from "lucide-react"
import {
  DiscordIcon,
  TelegramIcon,
} from "@/components/landing/brand-icons"

export function Integrations() {
  return (
    <section
      id="integrations"
      className="relative border-y border-border/60 bg-secondary/10"
    >
      <div className="mx-auto grid w-full max-w-7xl grid-cols-1 gap-10 px-4 py-20 sm:px-6 lg:grid-cols-2 lg:gap-16 lg:px-8 lg:py-28">
        <div className="flex flex-col justify-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Mobile approvals
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Step away. Approve from your phone.
          </h2>
          <p className="mt-4 max-w-xl text-pretty text-muted-foreground">
            When an agent hits a decision only you can make, it posts to{" "}
            <span className="text-foreground">Discord</span> or{" "}
            <span className="text-foreground">Telegram</span> with the actual
            diff, command, or tradeoff. Reply in a line and the run picks up
            where it stopped.
          </p>

          <ul className="mt-8 space-y-3">
            {[
              {
                title: "Diff in the message",
                copy: "Each notification carries the real change or command, not a vague \"needs input\".",
              },
              {
                title: "Approve from anywhere",
                copy: "Phone, laptop, watch, anywhere Discord or Telegram already lives.",
              },
              {
                title: "Per-tool rules",
                copy: "Choose what auto-runs and what waits for you, set per tool and per session.",
              },
            ].map((f) => (
              <li key={f.title} className="flex gap-3">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/15 text-primary">
                  <CheckCheck className="h-3 w-3" />
                </span>
                <div>
                  <p className="text-sm font-medium">{f.title}</p>
                  <p className="text-sm text-muted-foreground">{f.copy}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>

        <div className="relative flex items-center justify-center">
          <div
            aria-hidden
            className="absolute inset-0 -z-10 rounded-3xl bg-gradient-to-br from-primary/10 via-transparent to-transparent blur-2xl"
          />
          <div className="grid w-full max-w-sm gap-3">
            <ApprovalCard
              platform="discord"
              channel="#xero-approvals"
              timestamp="2:14 PM"
              tag="Swap · Raydium"
              title="Approve mainnet swap?"
              context="tx send → mainnet · 0.42 SOL"
              diff={[
                { type: "remove", text: "slippage 5.00%" },
                { type: "add", text: "slippage 1.20%" },
              ]}
            />
            <ApprovalCard
              platform="telegram"
              channel="Xero Bot"
              timestamp="14:21"
              tag="Checkpoint"
              title="Review migration before merge."
              context={
                <span className="inline-flex items-center gap-1.5">
                  <FileDiff className="h-3 w-3 text-primary/80" />
                  <span className="font-mono">0042_user_schema.sql</span>
                  <span className="text-emerald-400/90">+12</span>
                  <span className="text-rose-400/90">−3</span>
                </span>
              }
            />
          </div>
        </div>
      </div>
    </section>
  )
}

type DiffLine = { type: "add" | "remove"; text: string }

interface ApprovalCardProps {
  platform: "discord" | "telegram"
  channel: string
  timestamp: string
  tag: string
  title: string
  context: React.ReactNode
  diff?: DiffLine[]
}

function ApprovalCard({
  platform,
  channel,
  timestamp,
  tag,
  title,
  context,
  diff,
}: ApprovalCardProps) {
  const PlatformIcon = platform === "discord" ? DiscordIcon : TelegramIcon
  const platformLabel = platform === "discord" ? "Discord" : "Telegram"

  return (
    <div className="group relative overflow-hidden rounded-xl border border-border/70 bg-card/80 backdrop-blur-sm transition-colors hover:border-primary/30">
      <span
        aria-hidden
        className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/40 to-transparent"
      />

      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border/60 px-3.5 py-2">
        <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-secondary/60 text-foreground/80">
          <PlatformIcon className="h-3 w-3" />
        </span>
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
          {platformLabel}
        </span>
        <span className="h-3 w-px bg-border/80" />
        <span className="truncate font-mono text-[11px] text-foreground/80">
          {channel}
        </span>
        <span className="ml-auto font-mono text-[10.5px] text-muted-foreground">
          {timestamp}
        </span>
      </div>

      {/* Body */}
      <div className="px-3.5 py-3">
        <div className="flex items-center gap-1.5">
          <Hash className="h-3 w-3 text-primary/80" strokeWidth={2.5} />
          <span className="font-mono text-[10.5px] uppercase tracking-[0.14em] text-primary/90">
            {tag}
          </span>
        </div>
        <p className="mt-1.5 text-[14px] font-medium leading-snug text-foreground">
          {title}
        </p>
        <div className="mt-1 text-[12.5px] leading-snug text-muted-foreground">
          {context}
        </div>

        {diff && diff.length > 0 ? (
          <pre className="mt-2.5 overflow-hidden rounded-md border border-border/60 bg-background/60 px-2.5 py-1.5 font-mono text-[11px] leading-[1.55]">
            {diff.map((line, i) => (
              <span
                key={i}
                className={`block ${
                  line.type === "remove" ? "text-rose-400/90" : "text-emerald-400/90"
                }`}
              >
                {line.type === "remove" ? "- " : "+ "}
                {line.text}
              </span>
            ))}
          </pre>
        ) : null}

        {/* Actions */}
        <div className="mt-3 flex items-center gap-2">
          <span className="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-2.5 text-[12px] font-medium text-primary-foreground">
            <Check className="h-3 w-3" strokeWidth={3} />
            Approve
          </span>
          <span className="inline-flex h-7 items-center rounded-md border border-border/70 bg-secondary/40 px-2.5 text-[12px] font-medium text-foreground/80">
            Skip
          </span>
          <span className="ml-auto font-mono text-[10.5px] text-muted-foreground">
            reply ↵
          </span>
        </div>
      </div>
    </div>
  )
}
