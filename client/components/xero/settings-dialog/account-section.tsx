import { useMemo } from "react"
import { AlertTriangle, ArrowUpRight, Github, Loader2, LogOut } from "lucide-react"
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
        description="Sign in with GitHub to associate your identity with this Xero install. The OAuth handshake happens on the Xero server — sign-in is optional."
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

  return (
    <div className="flex items-center gap-3 rounded-md border border-border/60 bg-secondary/10 px-3.5 py-3">
      <Avatar className="size-9 border border-border/60">
        <AvatarImage src={session.user.avatarUrl} alt="" referrerPolicy="no-referrer" />
        <AvatarFallback className="text-[11px] font-medium text-muted-foreground">
          {initials}
        </AvatarFallback>
      </Avatar>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <p className="truncate text-[12.5px] font-semibold text-foreground">{displayName}</p>
          <StatusPill tone="success" label="Connected" />
        </div>
        <p className="truncate text-[11.5px] text-muted-foreground">
          @{session.user.login}
          {session.user.email ? (
            <span className="text-muted-foreground/70"> · {session.user.email}</span>
          ) : null}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-muted-foreground hover:text-foreground"
          onClick={() => {
            void openUrl(session.user.htmlUrl).catch(() => undefined)
          }}
          aria-label="View profile on GitHub"
          title="View on GitHub"
        >
          <ArrowUpRight className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-muted-foreground hover:text-destructive"
          onClick={onLogout}
          disabled={loading}
          aria-label="Sign out of GitHub"
          title="Sign out"
        >
          <LogOut className="h-3.5 w-3.5" />
        </Button>
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
    <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-border/60 bg-secondary/10 px-5 py-8 text-center">
      <Github className="h-5 w-5 text-muted-foreground" />
      <p className="text-[12.5px] font-medium text-foreground">Connect a GitHub account</p>
      <Button
        type="button"
        size="sm"
        className="h-8 gap-2 px-3 text-[12.5px]"
        onClick={onLogin}
        disabled={authenticating || loading}
        aria-label="Sign in with GitHub"
      >
        {authenticating ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <Github className="h-3.5 w-3.5" />
        )}
        {authenticating ? "Waiting for browser…" : "Sign in with GitHub"}
      </Button>
    </div>
  )
}

function ErrorCallout({ error }: { error: GitHubAuthError }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive"
    >
      <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
      <span>{error.message}</span>
    </div>
  )
}

function StatusPill({ tone, label }: { tone: "success"; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10.5px] font-medium uppercase tracking-[0.08em]",
        tone === "success" && "bg-success/10 text-success dark:text-success",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          tone === "success" && "bg-success dark:bg-success",
        )}
        aria-hidden
      />
      {label}
    </span>
  )
}

function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return "?"
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase()
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase()
}
