import { CheckCheck } from "lucide-react"
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
            <SignalCard
              platform="Discord"
              icon={<DiscordIcon className="h-3.5 w-3.5 text-white" />}
              accent="#5865F2"
              channel="#xero-approvals"
              kind="approve"
            />
            <SignalCard
              platform="Telegram"
              icon={<TelegramIcon className="h-3.5 w-3.5 text-white" />}
              accent="#26A5E4"
              channel="direct chat"
              kind="checkpoint"
            />
          </div>
        </div>
      </div>
    </section>
  )
}

/* SignalCard — abstract notification visual.
   Concentric ripple rings emanating from the platform glyph;
   small abstract action chips on the periphery. */
function SignalCard({
  platform,
  icon,
  accent,
  channel,
  kind,
}: {
  platform: string
  icon: React.ReactNode
  accent: string
  channel: string
  kind: "approve" | "checkpoint"
}) {
  const isApprove = kind === "approve"
  return (
    <div className="group/sig relative overflow-hidden rounded-xl border border-border/70 bg-card shadow-[0_30px_60px_-30px_rgba(0,0,0,0.6)] transition-colors hover:border-border">
      <span
        aria-hidden
        className="pointer-events-none absolute inset-x-0 top-0 h-px"
        style={{
          background: `linear-gradient(to right, transparent, ${accent}aa, transparent)`,
        }}
      />

      <div className="flex items-center justify-between border-b border-border/60 bg-secondary/40 px-3 py-2">
        <div className="flex items-center gap-2">
          <span
            className="inline-flex h-5 w-5 items-center justify-center rounded-md text-white shadow-[0_2px_8px_-2px_rgba(0,0,0,0.6)]"
            style={{ backgroundColor: accent }}
            aria-hidden
          >
            {icon}
          </span>
          <span className="text-xs font-medium">{platform}</span>
          <span className="text-muted-foreground/40">·</span>
          <span className="text-[11px] text-muted-foreground">{channel}</span>
        </div>
        <span className="inline-flex items-center gap-1 font-mono text-[9px] uppercase tracking-wider text-muted-foreground/70">
          <span className="h-1.5 w-1.5 rounded-full bg-primary" />
          live
        </span>
      </div>

      <SignalSvg accent={accent} kind={isApprove ? "approve" : "checkpoint"} />
    </div>
  )
}

function SignalSvg({
  accent,
  kind,
}: {
  accent: string
  kind: "approve" | "checkpoint"
}) {
  const isApprove = kind === "approve"
  const cx = 200
  const cy = 100

  return (
    <svg viewBox="0 0 400 200" className="h-auto w-full" aria-hidden>
      <defs>
        <radialGradient id={`sig-glow-${accent}`} cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor={accent} stopOpacity="0.42" />
          <stop offset="100%" stopColor={accent} stopOpacity="0" />
        </radialGradient>
        <pattern id={`sig-grid-${accent}`} x="0" y="0" width="20" height="20" patternUnits="userSpaceOnUse">
          <path d="M 20 0 L 0 0 0 20" fill="none" stroke="color-mix(in oklab, var(--border) 45%, transparent)" strokeWidth="0.4" />
        </pattern>
      </defs>

      <rect width="400" height="200" fill={`url(#sig-grid-${accent})`} opacity="0.5" />
      <circle cx={cx} cy={cy} r="86" fill={`url(#sig-glow-${accent})`} />

      {/* Ripple rings */}
      {[28, 50, 72, 92].map((r, i) => (
        <circle
          key={i}
          cx={cx}
          cy={cy}
          r={r}
          fill="none"
          stroke={accent}
          strokeWidth={i === 0 ? 1.4 : 0.9}
          strokeDasharray={i === 3 ? "2 5" : undefined}
          opacity={0.85 - i * 0.18}
        />
      ))}

      {/* Diff segment overlay (a small abstract diff arc) */}
      {isApprove && (
        <g>
          <path
            d="M 130 70 L 170 70"
            stroke="var(--primary)"
            strokeWidth="2"
            strokeLinecap="round"
          />
          <text x="124" y="74" textAnchor="end" fontFamily="var(--font-mono)" fontSize="10" fill="var(--primary)" letterSpacing="1">+</text>
          <path
            d="M 230 130 L 270 130"
            stroke="color-mix(in oklab, var(--destructive) 80%, transparent)"
            strokeWidth="2"
            strokeLinecap="round"
          />
          <text x="276" y="134" fontFamily="var(--font-mono)" fontSize="10" fill="color-mix(in oklab, var(--destructive) 90%, transparent)" letterSpacing="1">−</text>
        </g>
      )}

      {/* Center platform glyph */}
      <g>
        <circle
          cx={cx}
          cy={cy}
          r="22"
          fill="color-mix(in oklab, var(--card) 80%, transparent)"
          stroke={accent}
          strokeWidth="1.4"
        />
        <g transform={`translate(${cx - 10} ${cy - 10}) scale(0.83)`}>
          <rect width="24" height="24" rx="5" fill={accent} />
          <g transform="translate(2 2) scale(0.84)" fill="#fff">
            {accent === "#5865F2" ? (
              <path d="M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z" />
            ) : (
              <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z" />
            )}
          </g>
        </g>
        <circle cx={cx + 18} cy={cy - 18} r="4.5" fill="var(--primary)" />
      </g>

      {/* Action chips (abstract) */}
      {isApprove ? (
        <g fontFamily="var(--font-mono)" fontSize="9" letterSpacing="1.5">
          <g transform="translate(48 86)">
            <rect width="46" height="18" rx="4" fill="color-mix(in oklab, var(--primary) 18%, var(--card))" stroke="var(--primary)" strokeWidth="1" />
            <text x="23" y="12" textAnchor="middle" fill="var(--primary)" fontWeight="600">APPROVE</text>
          </g>
          <g transform="translate(312 86)">
            <rect width="38" height="18" rx="4" fill="var(--card)" stroke="color-mix(in oklab, var(--border) 90%, transparent)" />
            <text x="19" y="12" textAnchor="middle" fill="var(--muted-foreground)">SKIP</text>
          </g>
          <g transform="translate(176 154)">
            <rect width="48" height="16" rx="3" fill="color-mix(in oklab, var(--card) 80%, transparent)" stroke="color-mix(in oklab, var(--border) 80%, transparent)" />
            <text x="24" y="11" textAnchor="middle" fill="var(--muted-foreground)" letterSpacing="1.2">SHOW DIFF</text>
          </g>
        </g>
      ) : (
        <g fontFamily="var(--font-mono)" fontSize="9" letterSpacing="1.5">
          {/* Three checkpoint markers around the orbit */}
          {[
            { x: 80, y: 40, label: "ckpt · 1" },
            { x: 320, y: 40, label: "ckpt · 2" },
            { x: 200, y: 168, label: "ckpt · 3" },
          ].map((m, i) => (
            <g key={i}>
              <circle cx={m.x} cy={m.y} r="4" fill="var(--primary)" />
              <text
                x={m.x}
                y={m.y - 10}
                textAnchor="middle"
                fill="var(--muted-foreground)"
              >
                {m.label}
              </text>
            </g>
          ))}
          <g transform="translate(140 154)">
            <rect width="120" height="16" rx="3" fill="color-mix(in oklab, var(--card) 80%, transparent)" stroke="color-mix(in oklab, var(--border) 80%, transparent)" />
            <text x="60" y="11" textAnchor="middle" fill="var(--muted-foreground)" letterSpacing="1.2">
              compacted · 42% · awaiting
            </text>
          </g>
        </g>
      )}
    </svg>
  )
}
