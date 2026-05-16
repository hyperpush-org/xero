import { createServerFn } from "@tanstack/react-start";
import { getRequest } from "@tanstack/react-start/server";

import { getServerUrl } from "../server-url";

export interface AccountDevice {
	id: string;
	account_id: string;
	kind: "desktop" | "web";
	name: string | null;
	user_agent: string | null;
	last_seen: string | null;
	created_at: string | null;
	revoked_at: string | null;
}

export interface CloudSession {
	githubLogin: string;
	avatarUrl: string | null;
	deviceId: string;
	accountId: string;
	devices: AccountDevice[];
	relayToken: string;
	relayTokenExpiresAt: string;
}

const COOKIE_NAME = "_xero_web_session";
// Matches `Xero.GitHubAuth.session_header()` in the Phoenix relay.
const SERVER_SESSION_HEADER = "x-xero-github-session-id";

function readWebSessionCookie(): string | null {
	const request = getRequest();
	if (!request) return null;
	const header = request.headers.get("cookie");
	if (!header) return null;
	const match = new RegExp(`(?:^|;\\s*)${COOKIE_NAME}=([^;]+)`).exec(header);
	return match ? decodeURIComponent(match[1]) : null;
}

/**
 * Server function — exchanges the browser session cookie for a fresh short-lived
 * relay JWT and the current account/device snapshot. Returns null when the user
 * is not signed in.
 */
export const getCurrentSession = createServerFn({ method: "GET" }).handler(
	async (): Promise<CloudSession | null> => {
		const cookie = readWebSessionCookie();
		if (!cookie) return null;

		const serverUrl = getServerUrl();

		const refreshResponse = await fetch(
			`${serverUrl}/api/relay/token/refresh`,
			{
				method: "POST",
				headers: {
					[SERVER_SESSION_HEADER]: cookie,
					"content-type": "application/json",
				},
				body: "{}",
			},
		);
		if (!refreshResponse.ok) return null;
		const refreshed = (await refreshResponse.json()) as {
			relayToken: string;
			relayTokenExpiresAt: string;
			deviceId: string;
			accountId: string;
			account: { githubLogin: string; githubAvatarUrl: string | null };
		};

		const devicesResponse = await fetch(`${serverUrl}/api/devices`, {
			headers: { authorization: `Bearer ${refreshed.relayToken}` },
		});
		const devices: AccountDevice[] = devicesResponse.ok
			? (((await devicesResponse.json()) as { devices: AccountDevice[] })
					.devices ?? [])
			: [];

		return {
			githubLogin: refreshed.account.githubLogin,
			avatarUrl: refreshed.account.githubAvatarUrl,
			deviceId: refreshed.deviceId,
			accountId: refreshed.accountId,
			devices,
			relayToken: refreshed.relayToken,
			relayTokenExpiresAt: refreshed.relayTokenExpiresAt,
		};
	},
);

/**
 * Server function — clears the browser session cookie and revokes the matching
 * `devices` row.
 */
export const signOut = createServerFn({ method: "POST" }).handler(
	async (): Promise<{ ok: true }> => {
		const cookie = readWebSessionCookie();
		if (!cookie) return { ok: true };
		const serverUrl = getServerUrl();
		await fetch(`${serverUrl}/api/github/session`, {
			method: "DELETE",
			headers: { [SERVER_SESSION_HEADER]: cookie },
		});
		return { ok: true };
	},
);
