import { createFileRoute, redirect } from "@tanstack/react-router";
import { AppLogo } from "@xero/ui/components/app-logo";
import { Button } from "@xero/ui/components/ui/button";
import { Loader2 } from "lucide-react";
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
			<div className="flex flex-1 flex-col items-center justify-center gap-6">
				<AppLogo className="size-24 sm:size-28" aria-label="Xero" />
				<p className="max-w-xs text-center text-sm text-muted-foreground sm:text-base">
					Drive the Xero sessions running on your computer from anywhere.
				</p>
			</div>
			<div className="flex w-full max-w-sm flex-col gap-3 pb-[env(safe-area-inset-bottom)]">
				{error ? (
					<p className="text-center text-sm text-destructive" role="alert">
						{error}
					</p>
				) : null}
				<Button
					type="button"
					size="lg"
					className="h-12 w-full text-base"
					onClick={() => {
						void handleSignIn();
					}}
					disabled={pending}
				>
					{pending ? (
						<>
							<Loader2 className="h-4 w-4 animate-spin" /> Signing in…
						</>
					) : (
						"Sign in with GitHub"
					)}
				</Button>
			</div>
		</main>
	);
}
