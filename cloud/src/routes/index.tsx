import { createFileRoute, redirect } from "@tanstack/react-router";
import { AppLogo } from "@xero/ui/components/app-logo";
import { Button } from "@xero/ui/components/ui/button";
import { Github, Loader2 } from "lucide-react";
import { useState } from "react";

import { signInWithGitHub } from "#/lib/auth/oauth";
import { getCurrentSession } from "#/lib/auth/session";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		const session = await getCurrentSession();
		if (session) {
			throw redirect({ to: "/sessions" });
		}
	},
	component: LoginScreen,
});

function LoginScreen() {
	const [pending, setPending] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleSignIn = async () => {
		setPending(true);
		setError(null);
		try {
			await signInWithGitHub();
		} catch (caught) {
			setError(caught instanceof Error ? caught.message : "Sign-in failed");
			setPending(false);
		}
	};

	return (
		<main className="flex min-h-dvh flex-col items-center justify-between bg-background px-6 py-10 text-foreground sm:px-10">
			<div className="flex flex-1 flex-col items-center justify-center gap-8 text-center">
				<div className="flex size-16 items-center justify-center rounded-2xl border border-border/70 bg-secondary/30 animate-in fade-in-0 zoom-in-95 motion-enter sm:size-[68px]">
					<AppLogo className="size-8 sm:size-9" aria-label="Xero" />
				</div>

				<div className="flex max-w-xs flex-col items-center gap-2.5 animate-in fade-in-0 slide-in-from-bottom-2 motion-enter [animation-delay:80ms] [animation-fill-mode:both]">
					<h1 className="text-balance text-[28px] font-semibold leading-[1.1] tracking-tight text-foreground sm:text-[32px]">
						Your sessions, everywhere
					</h1>
					<p className="text-pretty text-[13px] leading-relaxed text-muted-foreground sm:text-sm">
						Drive the Xero sessions running on your computer from anywhere.
					</p>
				</div>
			</div>

			<div className="flex w-full max-w-sm flex-col gap-3 pb-[env(safe-area-inset-bottom)] animate-in fade-in-0 motion-enter [animation-delay:140ms] [animation-fill-mode:both]">
				{error ? (
					<p className="text-center text-sm text-destructive" role="alert">
						{error}
					</p>
				) : null}
				<Button
					type="button"
					size="lg"
					className="group h-12 w-full bg-primary text-base font-medium text-primary-foreground hover:bg-primary/90"
					onClick={() => {
						void handleSignIn();
					}}
					disabled={pending}
				>
					<span className="inline-flex items-center gap-2.5">
						{pending ? (
							<>
								<Loader2 className="size-5 animate-spin" />
								Signing in…
							</>
						) : (
							<>
								<Github className="size-5 transition-transform group-hover:-translate-x-0.5" />
								Sign in with GitHub
							</>
						)}
					</span>
				</Button>
			</div>
		</main>
	);
}
