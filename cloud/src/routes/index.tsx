import { createFileRoute, redirect } from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import {
	Github,
	Loader2,
	Monitor,
	ShieldCheck,
	Smartphone,
} from "lucide-react";
import { useEffect, useState } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { InstallAppAction } from "#/components/install-app-action";
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

const FEATURES = [
	{
		icon: Monitor,
		title: "Drive your local machine from anywhere",
		body: "Pick up your Xero sessions from any browser.",
	},
	{
		icon: Smartphone,
		title: "Phone, tablet, laptop",
		body: "Designed to work at any size, on any device.",
	},
	{
		icon: ShieldCheck,
		title: "Secure by default",
		body: "GitHub OAuth, scoped per-device tokens.",
	},
] as const;

const PREVIEW_SESSIONS = [
	{
		name: "Refactor auth middleware",
		project: "xero-server",
		state: "active" as const,
	},
	{
		name: "Fix nav overflow on iPad",
		project: "cloud",
		state: "idle" as const,
	},
	{
		name: "Add session export to JSON",
		project: "client",
		state: "idle" as const,
	},
];

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
		<main className="grid min-h-dvh bg-background text-foreground lg:grid-cols-[1.1fr_1fr]">
			<aside className="relative hidden overflow-hidden border-r border-border/40 bg-background lg:flex lg:flex-col lg:justify-between lg:px-14 lg:py-14">
				<div
					aria-hidden
					className="pointer-events-none absolute inset-0 bg-black/30"
				/>
				<div
					aria-hidden
					className="pointer-events-none absolute inset-0 opacity-[0.06]"
					style={{
						backgroundImage:
							"radial-gradient(currentColor 1px, transparent 1px)",
						backgroundSize: "26px 26px",
					}}
				/>

				<div className="relative flex max-w-md flex-col gap-10">
					<div className="flex items-center gap-3">
						<BrandLogo className="h-11 w-11" aria-label="Xero" />
						<span className="text-sm font-semibold tracking-tight text-foreground">
							Xero
						</span>
					</div>

					<div className="flex flex-col gap-4">
						<h1 className="text-4xl font-semibold leading-[1.1] tracking-tight text-foreground">
							Your sessions, <span className="text-primary">everywhere</span>
						</h1>
						<p className="max-w-sm text-base leading-relaxed text-muted-foreground">
							Drive the Xero coding sessions running on your computer from any
							browser, on any device.
						</p>
					</div>

					<ul className="flex flex-col gap-4 pt-2">
						{FEATURES.map(({ icon: Icon, title, body }) => (
							<li key={title} className="flex items-start gap-3">
								<span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-primary/20 bg-primary/10">
									<Icon className="h-4 w-4 text-primary" aria-hidden />
								</span>
								<div className="flex flex-col gap-0.5">
									<p className="text-sm font-medium text-foreground">{title}</p>
									<p className="text-[13px] leading-relaxed text-muted-foreground">
										{body}
									</p>
								</div>
							</li>
						))}
					</ul>
				</div>

				<div className="relative flex max-w-md flex-col gap-3">
					<div className="overflow-hidden rounded-lg border border-border bg-card shadow-sm">
						<div className="flex items-center justify-between border-b border-border px-3 py-2">
							<div className="flex items-center gap-2">
								<span className="h-1.5 w-1.5 rounded-full bg-primary" />
								<span className="text-[11px] font-medium text-foreground">
									Sessions
								</span>
							</div>
							<span className="text-[10px] text-muted-foreground">
								3 active
							</span>
						</div>
						<ul className="divide-y divide-border">
							{PREVIEW_SESSIONS.map((s) => (
								<li
									key={s.name}
									className="flex items-center gap-3 px-3 py-2.5"
								>
									<span
										className={
											s.state === "active"
												? "h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
												: "h-1.5 w-1.5 shrink-0 rounded-full bg-muted-foreground/40"
										}
									/>
									<span className="flex-1 truncate text-[12px] text-foreground">
										{s.name}
									</span>
									<span className="text-[10px] text-muted-foreground">
										{s.project}
									</span>
								</li>
							))}
						</ul>
					</div>
					<p className="text-[11px] text-muted-foreground">
						A live look at what awaits inside.
					</p>
				</div>
			</aside>

			<section className="relative flex flex-col items-center justify-between overflow-hidden px-6 pb-10 pt-[max(env(safe-area-inset-top),2.5rem)] sm:px-10 lg:justify-center lg:py-10">
				<div
					aria-hidden
					className="pointer-events-none absolute inset-0 opacity-[0.05] lg:hidden"
					style={{
						backgroundImage:
							"radial-gradient(currentColor 1px, transparent 1px)",
						backgroundSize: "24px 24px",
					}}
				/>

				<div className="relative flex flex-1 flex-col items-center justify-center gap-7 text-center lg:hidden">
					<BrandLogo className="h-14 w-14" aria-label="Xero" />
					<div className="flex max-w-xs flex-col items-center gap-2.5">
						<h1 className="text-2xl font-semibold leading-tight tracking-tight text-foreground">
							Your sessions, <span className="text-primary">everywhere</span>
						</h1>
						<p className="text-[13px] leading-relaxed text-muted-foreground">
							Drive the Xero sessions running on your computer from anywhere.
						</p>
					</div>
					<ul className="flex w-full max-w-xs flex-col gap-2 pt-1 text-left">
						{FEATURES.map(({ icon: Icon, title }) => (
							<li key={title} className="flex items-center gap-3">
								<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-primary/20 bg-primary/10">
									<Icon className="h-3.5 w-3.5 text-primary" aria-hidden />
								</span>
								<span className="text-[13px] font-medium text-foreground">
									{title}
								</span>
							</li>
						))}
					</ul>
				</div>

				<div className="relative flex w-full max-w-sm flex-col gap-4 pb-[env(safe-area-inset-bottom)] lg:pb-0">
					<div className="flex flex-col items-stretch gap-6 lg:gap-7 lg:rounded-xl lg:border lg:border-border lg:bg-card lg:p-8 lg:shadow-sm">
						<div className="hidden flex-col items-center gap-2 text-center lg:flex">
							<BrandLogo className="h-10 w-10" aria-label="Xero" />
							<h2 className="pt-2 text-xl font-semibold tracking-tight text-foreground">
								Welcome back
							</h2>
							<p className="text-[13px] text-muted-foreground">
								Sign in to access your sessions.
							</p>
						</div>

						<div className="flex flex-col items-stretch gap-3">
							{error ? (
								<p
									className="text-center text-sm text-destructive"
									role="alert"
								>
									{error}
								</p>
							) : null}
							<Button
								type="button"
								size="sm"
								className="h-10 w-full gap-2 px-4 text-[13px] font-medium"
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
							{pending ? null : <InstallAppAction />}
							<p className="hidden text-center text-[11px] leading-relaxed text-muted-foreground lg:block">
								By signing in you agree to our terms and privacy policy.
							</p>
						</div>
					</div>
				</div>
			</section>
		</main>
	);
}
