/** @vitest-environment jsdom */

import { renderToString } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CloudSession } from "#/lib/auth/session";

import { SessionListPanel } from "./session-list-panel";

const COLLAPSED_GROUPS_STORAGE_KEY = "xero.cloud.sidebar.projectCollapsed.v1";

const session: CloudSession = {
	githubLogin: "snowdamiz",
	avatarUrl: null,
	deviceId: "web-1",
	accountId: "account-1",
	devices: [],
	relayToken: "relay-token",
	relayTokenExpiresAt: "2026-05-19T12:00:00Z",
};

describe("SessionListPanel", () => {
	beforeEach(() => {
		const store = new Map<string, string>();
		Object.defineProperty(window, "localStorage", {
			configurable: true,
			value: {
				getItem: (key: string) => store.get(key) ?? null,
				setItem: (key: string, value: string) => {
					store.set(key, value);
				},
				removeItem: (key: string) => {
					store.delete(key);
				},
			},
		});
	});

	it("keeps initial markup independent of local collapsed-group state", () => {
		window.localStorage.setItem(
			COLLAPSED_GROUPS_STORAGE_KEY,
			JSON.stringify({ "desktop-1:project-1": true }),
		);

		const html = renderToString(
			<SessionListPanel
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "session-1",
						projectId: "project-1",
						projectName: "Clipstack",
						title: "Project Overview",
						lastActivityAt: "2026-05-19T10:00:00Z",
						computerName: "Studio",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={vi.fn()}
				onSignOut={vi.fn()}
			/>,
		);

		expect(html).toContain("Collapse Clipstack");
		expect(html).not.toContain("Expand Clipstack");
	});
});
