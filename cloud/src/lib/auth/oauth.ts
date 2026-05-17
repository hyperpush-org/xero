import { getServerUrl } from "../server-url";

/**
 * Browser-side helper — starts the GitHub OAuth flow with `kind=web`. On
 * success the server 302s back to `redirectTo` and sets the
 * `_xero_web_session` cookie.
 */
export async function signInWithGitHub(redirectTo?: string): Promise<void> {
	const serverUrl = getServerUrl();
	const target = redirectTo ?? getDefaultOAuthReturnUrl(serverUrl);
	const response = await fetch(`${serverUrl}/api/github/login`, {
		method: "POST",
		headers: { "content-type": "application/json" },
		body: JSON.stringify({ kind: "web", redirectTo: target }),
	});
	if (!response.ok) {
		throw new Error(`GitHub sign-in failed: ${response.status}`);
	}
	const { authorizationUrl } = (await response.json()) as {
		authorizationUrl: string;
	};
	if (typeof window !== "undefined") {
		window.location.href = authorizationUrl;
	}
}

export function getDefaultOAuthReturnUrl(serverUrl: string): string {
	if (typeof window === "undefined") return "/sessions";
	const current = new URL(window.location.href);
	const server = new URL(serverUrl);

	if (
		isLoopbackHostname(current.hostname) &&
		isLoopbackHostname(server.hostname) &&
		current.hostname !== server.hostname
	) {
		current.hostname = server.hostname;
	}

	current.pathname = "/sessions";
	current.search = "";
	current.hash = "";
	return current.toString();
}

function isLoopbackHostname(hostname: string): boolean {
	return (
		hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1"
	);
}
