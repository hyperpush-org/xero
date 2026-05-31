import {
	createRootRoute,
	HeadContent,
	Link,
	Scripts,
} from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import { Toaster } from "@xero/ui/components/ui/toaster";
import type { ReactNode } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { InstallPromptToast } from "#/components/install-prompt-toast";
import { PwaServiceWorkerManager } from "#/components/pwa-service-worker-manager";
import {
	getPublicRuntimeServerUrl,
	RUNTIME_SERVER_URL_META_NAME,
} from "#/lib/server-url";
import appCss from "../styles.css?url";

export const Route = createRootRoute({
	head: () => ({
		meta: [
			{
				charSet: "utf-8",
			},
			{
				name: "viewport",
				content: "width=device-width, initial-scale=1, viewport-fit=cover",
			},
			{
				title: "Xero Cloud",
			},
			{
				name: "application-name",
				content: "Xero Cloud",
			},
			{
				name: "apple-mobile-web-app-capable",
				content: "yes",
			},
			{
				name: "mobile-web-app-capable",
				content: "yes",
			},
			{
				name: "apple-mobile-web-app-title",
				content: "Xero",
			},
			{
				name: "apple-mobile-web-app-status-bar-style",
				content: "black-translucent",
			},
			{
				name: "theme-color",
				content: "#121212",
			},
			{
				name: "msapplication-TileColor",
				content: "#121212",
			},
		],
		links: [
			{
				rel: "stylesheet",
				href: appCss,
			},
			{
				rel: "manifest",
				href: "/manifest.webmanifest",
			},
			{
				rel: "icon",
				type: "image/png",
				sizes: "16x16",
				href: "/icons/favicon-16x16.png",
			},
			{
				rel: "icon",
				type: "image/png",
				sizes: "32x32",
				href: "/icons/favicon-32x32.png",
			},
			{
				rel: "icon",
				type: "image/png",
				sizes: "48x48",
				href: "/icons/favicon-48x48.png",
			},
			{
				rel: "apple-touch-icon",
				sizes: "180x180",
				href: "/apple-touch-icon.png",
			},
		],
	}),
	notFoundComponent: CloudNotFound,
	shellComponent: RootDocument,
});

function RootDocument({ children }: { children: ReactNode }) {
	if (import.meta.env.MODE === "test") {
		return (
			<>
				{children}
				<Toaster />
			</>
		);
	}

	return (
		<html className="theme-dusk dark" data-theme="dusk" lang="en">
			<head>
				<HeadContent />
				<meta
					name={RUNTIME_SERVER_URL_META_NAME}
					content={getPublicRuntimeServerUrl()}
				/>
			</head>
			<body>
				{children}
				<PwaServiceWorkerManager />
				<InstallPromptToast />
				<Toaster />
				<Scripts />
			</body>
		</html>
	);
}

function CloudNotFound() {
	return (
		<main className="flex min-h-dvh flex-col items-center justify-center gap-6 bg-background px-6 py-10 text-center text-foreground">
			<BrandLogo className="size-20" aria-label="Xero" />
			<div className="flex max-w-sm flex-col items-center gap-2">
				<h1 className="text-xl font-medium">Page not found</h1>
				<p className="text-sm text-muted-foreground">
					This cloud page is not available.
				</p>
			</div>
			<Button asChild>
				<Link to="/">Return to Xero</Link>
			</Button>
		</main>
	);
}
