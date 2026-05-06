import { Bell, CheckCheck, Send } from "lucide-react"
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
            When an agent needs a call, it posts to{" "}
            <span className="text-foreground">Discord</span> or{" "}
            <span className="text-foreground">Telegram</span> with the diff,
            command, or tradeoff inline. Reply in a sentence and the session
            keeps going.
          </p>

          <ul className="mt-8 space-y-3">
            {[
              {
                title: "Diff in the message",
                copy: "Notifications carry the actual change or command, not a generic \"need input\".",
              },
              {
                title: "Approve from anywhere",
                copy: "Phone, laptop, watch — wherever Discord or Telegram already lives.",
              },
              {
                title: "Per-tool rules",
                copy: "Decide which actions auto-run and which wait for you, per session and per tool.",
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
          <div className="grid w-full max-w-md gap-4">
            <ChatMock
              platform="Discord"
              platformIcon={<DiscordIcon className="h-3 w-3 text-white" />}
              channelLabel="#xero-approvals"
              accent="#5865F2"
              username="xero-bot"
              tag="APP"
              messages={[
                {
                  kind: "bot",
                  body: (
                    <>
                      <div className="mb-1.5 inline-flex items-center gap-1.5 rounded-full bg-primary/15 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-primary">
                        <Bell className="h-3 w-3" /> approval
                      </div>
                      <p className="text-sm">
                        <span className="font-medium">acme-saas</span> · Engineer
                        wants to push branch{" "}
                        <code className="rounded bg-secondary px-1 py-0.5 font-mono text-[11px]">
                          try-pg
                        </code>{" "}
                        to <code className="rounded bg-secondary px-1 py-0.5 font-mono text-[11px]">origin</code>.
                        4 commits ahead.
                      </p>
                      <div className="mt-2 flex gap-2">
                        <span className="rounded-md bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground">
                          Approve
                        </span>
                        <span className="rounded-md border border-border/70 bg-secondary px-2 py-1 text-[11px]">
                          Reject
                        </span>
                        <span className="rounded-md border border-border/70 bg-secondary px-2 py-1 text-[11px]">
                          Show diff
                        </span>
                      </div>
                    </>
                  ),
                },
                {
                  kind: "you",
                  body: <p className="text-sm">approve — but rebase on main first</p>,
                },
              ]}
            />

            <ChatMock
              platform="Telegram"
              platformIcon={<TelegramIcon className="h-3 w-3 text-white" />}
              channelLabel="direct chat"
              accent="#26A5E4"
              username="xero"
              tag="BOT"
              messages={[
                {
                  kind: "bot",
                  body: (
                    <>
                      <p className="text-sm">
                        <span className="font-medium">acme-saas</span> · Engineer
                        paused at checkpoint
                        <br />
                        <span className="text-muted-foreground">
                          context auto-compacted to 42% · awaiting your call on the next step
                        </span>
                      </p>
                      <div className="mt-2 rounded-md border border-border/70 bg-background/60 p-2 font-mono text-[11px]">
                        <span className="text-primary">branch</span> · try-pg ·
                        3 checkpoints · 1 handoff
                      </div>
                    </>
                  ),
                },
              ]}
            />
          </div>
        </div>
      </div>
    </section>
  )
}

type Message = {
  kind: "bot" | "you"
  body: React.ReactNode
}

function ChatMock({
  platform,
  platformIcon,
  channelLabel,
  accent,
  username,
  tag,
  messages,
}: {
  platform: string
  platformIcon: React.ReactNode
  channelLabel: string
  accent: string
  username: string
  tag: string
  messages: Message[]
}) {
  return (
    <div className="group/chat relative overflow-hidden rounded-xl border border-border/70 bg-card shadow-[0_30px_60px_-30px_rgba(0,0,0,0.6)] transition-colors hover:border-border">
      <span
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-px"
        style={{
          background: `linear-gradient(to right, transparent, ${accent}80, transparent)`,
        }}
      />
      <div className="flex items-center justify-between border-b border-border/60 bg-secondary/40 px-3 py-2">
        <div className="flex items-center gap-2">
          <span
            className="inline-flex h-5 w-5 items-center justify-center rounded-md text-white shadow-[0_2px_8px_-2px_rgba(0,0,0,0.6)]"
            style={{ backgroundColor: accent }}
            aria-hidden
          >
            {platformIcon}
          </span>
          <span className="text-xs font-medium">{platform}</span>
          <span className="text-muted-foreground/40">·</span>
          <span className="text-[11px] text-muted-foreground">{channelLabel}</span>
        </div>
        <span className="inline-flex items-center gap-1 font-mono text-[9px] uppercase tracking-wider text-muted-foreground/70">
          <span className="h-1.5 w-1.5 rounded-full bg-primary" />
          live
        </span>
      </div>
      <div className="space-y-3 p-3">
        {messages.map((m, i) =>
          m.kind === "bot" ? (
            <div key={i} className="flex gap-2.5">
              <span
                className="relative mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-[10px] font-bold text-white shadow-[0_4px_12px_-4px_rgba(0,0,0,0.6)]"
                style={{ backgroundColor: accent }}
                aria-hidden
              >
                C
                <span
                  aria-hidden
                  className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-card bg-primary"
                />
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-medium">{username}</span>
                  <span
                    className="rounded px-1 py-0.5 text-[9px] font-bold uppercase tracking-wide text-white"
                    style={{ backgroundColor: accent }}
                  >
                    {tag}
                  </span>
                  <span className="text-[11px] text-muted-foreground">just now</span>
                </div>
                <div className="mt-1">{m.body}</div>
              </div>
            </div>
          ) : (
            <div key={i} className="flex gap-2.5">
              <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary text-[10px] font-bold text-primary-foreground shadow-[0_4px_12px_-4px_color-mix(in_oklab,var(--primary)_50%,transparent)]">
                You
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-medium">you</span>
                  <span className="text-[11px] text-muted-foreground">just now</span>
                </div>
                <div className="mt-1">{m.body}</div>
              </div>
            </div>
          ),
        )}
        {/* composer */}
        <div className="mt-3 flex items-center gap-2 rounded-md border border-border/60 bg-background/60 px-3 py-1.5 text-xs text-muted-foreground/60">
          <span className="flex-1 truncate font-mono text-[11px]">type a reply…</span>
          <Send className="h-3 w-3" />
        </div>
      </div>
    </div>
  )
}
