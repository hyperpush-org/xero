import {
	createRootRoute,
	HeadContent,
	Link,
	Scripts,
} from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import type { ReactNode } from "react";

import { BrandLogo } from "#/components/brand-logo";
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
				content: "width=device-width, initial-scale=1",
			},
			{
				title: "Cloud",
			},
		],
		links: [
			{
				rel: "stylesheet",
				href: appCss,
			},
		],
	}),
	notFoundComponent: CloudNotFound,
	shellComponent: RootDocument,
});

function RootDocument({ children }: { children: ReactNode }) {
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
