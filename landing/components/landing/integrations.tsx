import { Bell, CheckCheck, Send } from "lucide-react"

export function Integrations() {
  return (
    <section
      id="integrations"
      className="relative border-y border-border/60 bg-secondary/10"
    >
      <div className="mx-auto grid w-full max-w-7xl grid-cols-1 gap-10 px-4 py-20 sm:px-6 lg:grid-cols-2 lg:gap-16 lg:px-8 lg:py-28">
        <div className="flex flex-col justify-center">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Notifications
          </p>
          <h2 className="mt-3 font-sans text-3xl font-medium tracking-tight text-balance sm:text-5xl">
            Go for a walk. Cadence will message you when it matters.
          </h2>
          <p className="mt-4 max-w-xl text-pretty text-muted-foreground">
            Most agents either stop cold or hallucinate forward when they hit ambiguity.
            Cadence pauses, states the tradeoff clearly, and pings you on{" "}
            <span className="text-foreground">Discord</span> or{" "}
            <span className="text-foreground">Telegram</span>. Reply in a sentence —
            it picks up exactly where it left off.
          </p>

          <ul className="mt-8 space-y-3">
            {[
              {
                title: "Rich, contextual decisions",
                copy: "Messages include the exact diff, failing test, or tradeoff — not a vague 'need your input'.",
              },
              {
                title: "Reply from anywhere",
                copy: "Approve, redirect, or answer a clarifying question in natural language from your phone.",
              },
              {
                title: "Smart batching & quiet hours",
                copy: "Cadence groups minor decisions and respects your focus time. No 3am pings.",
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
              accent="#5865F2"
              username="cadence-bot"
              tag="APP"
              messages={[
                {
                  kind: "bot",
                  body: (
                    <>
                      <div className="mb-1.5 inline-flex items-center gap-1.5 rounded-full bg-primary/15 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-primary">
                        <Bell className="h-3 w-3" /> decision
                      </div>
                      <p className="text-sm">
                        <span className="font-medium">acme-saas</span> · Stripe test key
                        detected. Use{" "}
                        <code className="rounded bg-secondary px-1 py-0.5 font-mono text-[11px]">
                          test
                        </code>{" "}
                        mode or prompt for live keys?
                      </p>
                      <div className="mt-2 flex gap-2">
                        <span className="rounded-md bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground">
                          Use test
                        </span>
                        <span className="rounded-md border border-border/70 bg-secondary px-2 py-1 text-[11px]">
                          Prompt me
                        </span>
                        <span className="rounded-md border border-border/70 bg-secondary px-2 py-1 text-[11px]">
                          Skip billing
                        </span>
                      </div>
                    </>
                  ),
                },
                {
                  kind: "you",
                  body: <p className="text-sm">test mode — we&apos;ll wire live keys later</p>,
                },
              ]}
            />

            <ChatMock
              platform="Telegram"
              accent="#26A5E4"
              username="Cadence"
              tag="BOT"
              messages={[
                {
                  kind: "bot",
                  body: (
                    <>
                      <p className="text-sm">
                        <span className="font-medium">acme-saas</span> · Build green ✓
                        <br />
                        <span className="text-muted-foreground">
                          42 files · 6 migrations · 18 tests passing
                        </span>
                      </p>
                      <div className="mt-2 rounded-md border border-border/70 bg-background/60 p-2 font-mono text-[11px]">
                        <span className="text-primary">preview</span> →
                        acme-saas-git-main.vercel.app
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
  accent,
  username,
  tag,
  messages,
}: {
  platform: string
  accent: string
  username: string
  tag: string
  messages: Message[]
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-border/70 bg-card shadow-xl">
      <div className="flex items-center justify-between border-b border-border/60 bg-secondary/40 px-3 py-2">
        <div className="flex items-center gap-2">
          <span
            className="inline-flex h-5 w-5 items-center justify-center rounded-md text-[11px] font-bold text-white"
            style={{ backgroundColor: accent }}
            aria-hidden
          >
            {platform[0]}
          </span>
          <span className="text-xs font-medium">{platform}</span>
          <span className="text-[11px] text-muted-foreground">#cadence-alerts</span>
        </div>
        <Send className="h-3.5 w-3.5 text-muted-foreground" />
      </div>
      <div className="space-y-3 p-3">
        {messages.map((m, i) =>
          m.kind === "bot" ? (
            <div key={i} className="flex gap-2.5">
              <span
                className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-[10px] font-bold text-white"
                style={{ backgroundColor: accent }}
                aria-hidden
              >
                C
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-medium">{username}</span>
                  <span
                    className="rounded px-1 py-0.5 text-[9px] font-bold text-white"
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
              <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary text-[10px] font-bold text-primary-foreground">
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
      </div>
    </div>
  )
}
