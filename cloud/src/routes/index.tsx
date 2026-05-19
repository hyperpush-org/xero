import { createFileRoute, redirect } from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import { Github, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { signInWithGitHub } from "#/lib/auth/oauth";
import { getCachedCurrentSession } from "#/lib/auth/session";
import { getCanonicalLoopbackCloudUrl } from "#/lib/server-url";

export const Route = createFileRoute("/")({
	beforeLoad: async () => {
		const canonicalUrl = getCanonicalLoopbackCloudUrl();
		if (canonicalUrl) throw redirect({ href: canonicalUrl });
		const session = await getCachedCurrentSession();
		if (session) {
			throw redirect({ to: "/sessions" });
		}
	},
	component: LoginScreen,
});

function LoginScreen() {
	const [pending, setPending] = useState(false);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		const canonicalUrl = getCanonicalLoopbackCloudUrl();
		if (canonicalUrl) window.location.replace(canonicalUrl);
	}, []);

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
			<div className="flex flex-1 flex-col items-center justify-center gap-6 text-center">
				<BrandLogo className="h-12 w-12" aria-label="Xero" />

				<div className="flex max-w-xs flex-col items-center gap-2">
					<h1 className="text-xl font-semibold tracking-tight text-foreground">
						Your sessions, everywhere
					</h1>
					<p className="text-[13px] leading-relaxed text-muted-foreground">
						Drive the Xero sessions running on your computer from anywhere.
					</p>
				</div>
			</div>

			<div className="flex w-full max-w-sm flex-col items-center gap-3 pb-[env(safe-area-inset-bottom)]">
				{error ? (
					<p className="text-center text-sm text-destructive" role="alert">
						{error}
					</p>
				) : null}
				<Button
					type="button"
					size="sm"
					className="h-9 gap-2 px-4 text-[12px] font-medium"
					onClick={() => {
						void handleSignIn();
					}}
					disabled={pending}
				>
					{pending ? (
						<>
							<Loader2 className="h-3.5 w-3.5 animate-spin" />
							Signing in…
						</>
					) : (
						<>
							<Github className="h-3.5 w-3.5" />
							Sign in with GitHub
						</>
					)}
				</Button>
			</div>
		</main>
	);
}
