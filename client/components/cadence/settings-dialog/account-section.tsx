import { Github, Loader2, LogOut } from "lucide-react"
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
        description="Sign in with GitHub to associate your identity with this Cadence install. The server owns the OAuth exchange and the connection is optional."
      />

      <section className="flex flex-col gap-4">
        {session ? (
          <div className="flex items-center gap-3 rounded-md border border-border/70 bg-secondary/20 px-3 py-3">
            <img
              src={session.user.avatarUrl}
              alt=""
              referrerPolicy="no-referrer"
              className="h-10 w-10 rounded-full border border-border/70"
            />
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-semibold text-foreground">
                {session.user.name ?? session.user.login}
              </p>
              <p className="truncate text-[12px] text-muted-foreground">
                @{session.user.login}
                {session.user.email ? ` · ${session.user.email}` : null}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-[12px]"
              onClick={onLogout}
              aria-label="Sign out of GitHub"
            >
              <LogOut className="h-3.5 w-3.5" />
              Sign out
            </Button>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-border/60 bg-secondary/10 px-4 py-8 text-center">
            <Github className="h-7 w-7 text-muted-foreground" />
            <div>
              <p className="text-[13px] font-medium text-foreground">Not signed in</p>
              <p className="mt-1 text-[12px] leading-[1.5] text-muted-foreground">
                Connect your GitHub account to identify this install. You can disconnect at any time.
              </p>
            </div>
            <Button
              type="button"
              size="sm"
              className={cn("mt-1 h-8 gap-1.5 text-[12px]")}
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
        )}

        {error ? (
          <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12.5px] text-destructive">
            {error.message}
          </p>
        ) : null}
      </section>
    </div>
  )
}
