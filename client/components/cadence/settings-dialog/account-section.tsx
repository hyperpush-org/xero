import { useMemo } from "react"
import {
  AlertTriangle,
  ArrowUpRight,
  CheckCircle2,
  Github,
  KeyRound,
  Loader2,
  LogOut,
  ShieldCheck,
  Unplug,
} from "lucide-react"
import { openUrl } from "@tauri-apps/plugin-opener"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type {
  GitHubAuthError,
  GitHubAuthStatus,
  GitHubSessionView,
} from "@/src/lib/github-auth"
import { SectionHeader } from "./section-header"

export interface AccountSectionProps {
  session: GitHubSessionView | null
  status: GitHubAuthStatus
  error: GitHubAuthError | null
  onLogin: () => void
  onLogout: () => void
}

export function AccountSection({
  session,
  status,
  error,
  onLogin,
  onLogout,
}: AccountSectionProps) {
  const authenticating = status === "authenticating"
  const loading = status === "loading"

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader
        title="Account"
        description="Sign in with GitHub to associate your identity with this Cadence install. The OAuth handshake happens on the Cadence server — sign-in is optional."
      />

      {session ? (
        <ConnectedCard session={session} onLogout={onLogout} loading={loading} />
      ) : (
        <SignInCard
          onLogin={onLogin}
          authenticating={authenticating}
          loading={loading}
        />
      )}

      {error ? <ErrorCallout error={error} /> : null}

      <ConnectionDetails session={session} />
    </div>
  )
}

function ConnectedCard({
  session,
  onLogout,
  loading,
}: {
  session: GitHubSessionView
  onLogout: () => void
  loading: boolean
}) {
  const displayName = session.user.name?.trim() || session.user.login
  const initials = useMemo(() => initialsFor(displayName), [displayName])
  const connectedAt = useMemo(() => formatConnectedAt(session.createdAt), [session.createdAt])

  return (
    <div className="rounded-xl border border-border/70 bg-card/40 shadow-[0_1px_0_0_rgba(255,255,255,0.03)_inset]">
      <div className="flex items-start gap-4 p-5">
        <Avatar className="size-12 border border-border/70">
          <AvatarImage src={session.user.avatarUrl} alt="" referrerPolicy="no-referrer" />
          <AvatarFallback className="text-[12px] font-medium text-muted-foreground">
            {initials}
          </AvatarFallback>
        </Avatar>

        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="truncate text-[14px] font-semibold leading-tight text-foreground">
              {displayName}
            </p>
            <StatusPill tone="success" label="Connected" />
          </div>
          <p className="truncate text-[12.5px] text-muted-foreground">
            @{session.user.login}
            {session.user.email ? <span className="text-muted-foreground/70"> · {session.user.email}</span> : null}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-[12px] text-muted-foreground hover:text-foreground"
            onClick={() => {
              void openUrl(session.user.htmlUrl).catch(() => undefined)
            }}
            aria-label="View profile on GitHub"
          >
            View on GitHub
            <ArrowUpRight className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-[12px]"
            onClick={onLogout}
            disabled={loading}
            aria-label="Sign out of GitHub"
          >
            <LogOut className="h-3.5 w-3.5" />
            Sign out
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border/60 px-5 py-3 text-[12px] text-muted-foreground">
        <MetaItem icon={KeyRound} label="Scope" value={session.scope || "default"} mono />
        <MetaItem icon={CheckCircle2} label="Connected" value={connectedAt} />
      </div>
    </div>
  )
}

function SignInCard({
  onLogin,
  authenticating,
  loading,
}: {
  onLogin: () => void
  authenticating: boolean
  loading: boolean
}) {
  return (
    <div className="relative overflow-hidden rounded-xl border border-dashed border-border/70 bg-secondary/15 px-6 py-9">
      <div className="mx-auto flex max-w-sm flex-col items-center gap-4 text-center">
        <div className="flex size-11 items-center justify-center rounded-full border border-border/60 bg-background/60 text-foreground/80">
          <Github className="h-5 w-5" />
        </div>
        <div className="flex flex-col gap-1.5">
          <p className="text-[14px] font-semibold text-foreground">Connect a GitHub account</p>
          <p className="text-[12.5px] leading-[1.55] text-muted-foreground">
            Identify this Cadence install with your GitHub identity. You can disconnect at any time, and Cadence works fine without it.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          className={cn("mt-1 h-9 gap-2 px-4 text-[12.5px]")}
          onClick={onLogin}
          disabled={authenticating || loading}
          aria-label="Sign in with GitHub"
        >
          {authenticating ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Github className="h-4 w-4" />
          )}
          {authenticating ? "Waiting for browser…" : "Sign in with GitHub"}
        </Button>
        {authenticating ? (
          <p className="text-[11.5px] text-muted-foreground/80">
            Complete the handshake in your browser, then return here.
          </p>
        ) : null}
      </div>
    </div>
  )
}

function ConnectionDetails({ session }: { session: GitHubSessionView | null }) {
  return (
    <section className="flex flex-col gap-3">
      <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/80">
        About this connection
      </h4>
      <ul className="flex flex-col divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-card/30">
        <DetailRow
          icon={ShieldCheck}
          title="Server-side OAuth"
          body="The Cadence server owns the exchange — your access token never lives in this app."
        />
        <DetailRow
          icon={KeyRound}
          title="Minimal scope"
          body={
            session
              ? `Granted ${session.scope || "default"} on the Cadence GitHub app.`
              : "Cadence requests the smallest scope GitHub will allow for identity."
          }
        />
        <DetailRow
          icon={Unplug}
          title="Disconnect anytime"
          body="Signing out revokes the local session immediately. Revoke server-side access from your GitHub settings."
        />
      </ul>
    </section>
  )
}

function DetailRow({
  icon: Icon,
  title,
  body,
}: {
  icon: React.ElementType
  title: string
  body: string
}) {
  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <div className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/60 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-muted-foreground">{body}</p>
      </div>
    </li>
  )
}

function ErrorCallout({ error }: { error: GitHubAuthError }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-medium text-destructive">Sign-in failed</p>
        <p className="mt-0.5 text-[12px] leading-[1.5] text-destructive/85">{error.message}</p>
      </div>
    </div>
  )
}

function StatusPill({ tone, label }: { tone: "success"; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em]",
        tone === "success" && "bg-emerald-500/10 text-emerald-500 dark:text-emerald-400",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          tone === "success" && "bg-emerald-500 dark:bg-emerald-400",
        )}
        aria-hidden
      />
      {label}
    </span>
  )
}

function MetaItem({
  icon: Icon,
  label,
  value,
  mono = false,
}: {
  icon: React.ElementType
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Icon className="h-3 w-3 text-muted-foreground/70" aria-hidden />
      <span className="text-muted-foreground/70">{label}</span>
      <span className={cn("text-foreground/80", mono && "font-mono text-[11.5px]")}>{value}</span>
    </span>
  )
}

function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return "?"
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase()
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase()
}

function formatConnectedAt(iso: string): string {
  const parsed = new Date(iso)
  if (Number.isNaN(parsed.getTime())) return "—"
  return parsed.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  })
}
