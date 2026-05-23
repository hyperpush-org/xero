import { createFileRoute, redirect } from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import {
	ArrowRight,
	Github,
	Loader2,
	Monitor,
	ShieldCheck,
	Smartphone,
} from "lucide-react";
import { type CSSProperties, useEffect, useState } from "react";

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
		numeral: "I",
		icon: Monitor,
		title: "Drive your local machine from anywhere",
		body: "Pick up your Xero sessions from any browser.",
	},
	{
		numeral: "II",
		icon: Smartphone,
		title: "Phone, tablet, laptop",
		body: "Designed to work at any size, on any device.",
	},
	{
		numeral: "III",
		icon: ShieldCheck,
		title: "Secure by default",
		body: "GitHub OAuth, scoped per-device tokens.",
	},
] as const;

function rise(delay: number): CSSProperties {
	return { "--cloud-rise-delay": `${delay}ms` } as CSSProperties;
}

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
		<main className="grid min-h-dvh bg-background text-foreground lg:grid-cols-[1.15fr_1fr]">
			{/* ────────────────────────  Desktop hero rail  ──────────────────── */}
			<aside className="cloud-halo-edge cloud-grain relative hidden overflow-hidden border-r border-border/40 bg-background lg:flex lg:flex-col lg:justify-between lg:px-16 lg:py-14">
				<div
					aria-hidden
					className="pointer-events-none absolute inset-0 opacity-[0.045]"
					style={{
						backgroundImage:
							"radial-gradient(currentColor 1px, transparent 1px)",
						backgroundSize: "28px 28px",
					}}
				/>

				<div className="relative flex max-w-lg flex-col gap-12">
					<div className="cloud-rise flex items-center gap-3" style={rise(0)}>
						<BrandLogo className="h-9 w-9" aria-label="Xero" />
						<span className="font-display text-[15px] font-medium tracking-tight text-foreground">
							Xero
						</span>
						<span
							aria-hidden
							className="ml-1 h-px w-12"
							style={{
								background:
									"linear-gradient(to right, var(--cloud-rule-strong), transparent)",
							}}
						/>
					</div>

					<div className="flex flex-col gap-7">
						<h1
							className="font-display cloud-rise text-[64px] font-medium leading-[1.02] tracking-[-0.025em] text-foreground"
							style={rise(120)}
						>
							Your sessions,
							<br />
							<em className="font-display-italic font-normal text-primary">
								everywhere.
							</em>
						</h1>
						<p
							className="cloud-rise max-w-md text-[15px] leading-[1.65] text-muted-foreground"
							style={rise(220)}
						>
							Drive the Xero coding sessions running on your computer from any
							browser, on any device — a continuous thread that travels with
							you.
						</p>
					</div>

					<ul className="cloud-rise flex flex-col gap-5" style={rise(320)}>
						{FEATURES.map(({ numeral, icon: Icon, title, body }) => (
							<li key={title} className="flex items-baseline gap-4">
								<span className="font-display-italic mt-0.5 w-7 shrink-0 text-[15px] font-normal leading-none text-primary/80">
									{numeral.toLowerCase()}
								</span>
								<span
									aria-hidden
									className="mt-2 h-px w-6 shrink-0"
									style={{ backgroundColor: "var(--cloud-rule-strong)" }}
								/>
								<div className="flex flex-1 items-start gap-3">
									<Icon
										className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary/75"
										aria-hidden
									/>
									<div className="flex flex-col gap-0.5">
										<p className="text-[13.5px] font-medium leading-snug text-foreground">
											{title}
										</p>
										<p className="text-[12.5px] leading-relaxed text-muted-foreground">
											{body}
										</p>
									</div>
								</div>
							</li>
						))}
					</ul>
				</div>

				<div
					className="cloud-rise relative flex max-w-md flex-col gap-3"
					style={rise(440)}
				>
					<p className="font-display-italic pl-1 text-[12px] text-muted-foreground/80">
						A live look at what awaits inside.
					</p>
				</div>
			</aside>

			{/* ────────────────────────  Sign-in panel  ──────────────────────── */}
			<section className="cloud-halo-soft relative flex flex-col items-center justify-between overflow-hidden px-6 pb-10 pt-[max(env(safe-area-inset-top),2.5rem)] sm:px-10 lg:justify-center lg:py-10">
				<div
					aria-hidden
					className="pointer-events-none absolute inset-0 opacity-[0.04] lg:hidden"
					style={{
						backgroundImage:
							"radial-gradient(currentColor 1px, transparent 1px)",
						backgroundSize: "24px 24px",
					}}
				/>

				{/* Mobile hero */}
				<div className="relative flex flex-1 flex-col items-center justify-center gap-7 text-center lg:hidden">
					<div
						className="cloud-halo cloud-rise flex items-center justify-center"
						style={rise(0)}
					>
						<BrandLogo className="h-14 w-14" aria-label="Xero" />
					</div>
					<div className="flex max-w-xs flex-col items-center gap-3">
						<h1
							className="font-display cloud-rise text-[34px] font-medium leading-[1.05] tracking-[-0.022em] text-foreground"
							style={rise(120)}
						>
							Your sessions,
							<br />
							<em className="font-display-italic font-normal text-primary">
								everywhere.
							</em>
						</h1>
						<p
							className="cloud-rise text-[13.5px] leading-relaxed text-muted-foreground"
							style={rise(220)}
						>
							Drive the Xero sessions running on your computer from anywhere.
						</p>
					</div>
					<ul
						className="cloud-rise flex w-full max-w-[15rem] flex-col gap-2.5 pt-1 text-left"
						style={rise(320)}
					>
						{FEATURES.map(({ icon: Icon, title }) => (
							<li key={title} className="flex items-center gap-3">
								<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-primary/25 bg-primary/10">
									<Icon className="h-3.5 w-3.5 text-primary" aria-hidden />
								</span>
								<span className="text-[12.5px] font-medium leading-snug text-foreground">
									{title}
								</span>
							</li>
						))}
					</ul>
				</div>

				{/* Sign-in card (both desktop and mobile, different chrome) */}
				<div className="relative flex w-full max-w-sm flex-col gap-4 pb-[env(safe-area-inset-bottom)] lg:pb-0">
					<div
						className="cloud-rise flex flex-col items-stretch gap-7 lg:gap-8 lg:rounded-2xl lg:border lg:border-border/70 lg:bg-card/60 lg:p-10 lg:shadow-[0_24px_60px_-24px_rgba(0,0,0,0.55)] lg:backdrop-blur-md"
						style={rise(440)}
					>
						<div className="hidden flex-col items-center gap-3 text-center lg:flex">
							<div className="cloud-halo flex items-center justify-center">
								<BrandLogo className="h-11 w-11" aria-label="Xero" />
							</div>
							<h2 className="font-display pt-3 text-[28px] font-medium leading-tight tracking-[-0.02em] text-foreground">
								Welcome <em className="font-display-italic">back</em>.
							</h2>
							<p className="text-[12.5px] leading-relaxed text-muted-foreground">
								Sign in to access your sessions.
							</p>
						</div>

						<div className="flex flex-col items-stretch gap-3">
							{error ? (
								<p
									className="text-center text-[12.5px] text-destructive"
									role="alert"
								>
									{error}
								</p>
							) : null}
							<Button
								type="button"
								className="group relative h-12 w-full justify-center gap-2.5 rounded-lg px-5 text-[13.5px] font-medium tracking-tight shadow-[0_8px_24px_-12px_var(--cloud-halo)] transition-shadow hover:shadow-[0_10px_30px_-10px_var(--cloud-halo)]"
								onClick={() => {
									void handleSignIn();
								}}
								disabled={pending}
							>
								{pending ? (
									<>
										<Loader2 className="h-4 w-4 animate-spin" />
										Signing in…
									</>
								) : (
									<>
										<Github className="h-4 w-4" />
										<span>Continue with GitHub</span>
										<ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
									</>
								)}
							</Button>
							{pending ? null : <InstallAppAction />}
							<p className="font-display-italic hidden text-center text-[11.5px] leading-relaxed text-muted-foreground/80 lg:block">
								By signing in you agree to our terms and privacy policy.
							</p>
						</div>
					</div>
				</div>
			</section>
		</main>
	);
}
