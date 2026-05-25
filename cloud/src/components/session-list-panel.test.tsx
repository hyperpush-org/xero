/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
	afterEach(() => {
		cleanup();
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
						agentSessionId: "session-1",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
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

	it("renders Computer Use as one global session row, not a category", () => {
		const { container } = render(
			<SessionListPanel
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "__computer_use__",
						agentSessionId: "__computer_use__",
						projectId: "global-computer-use",
						projectName: null,
						sessionKind: "computer_use",
						isComputerUse: true,
						title: "Computer Use",
						lastActivityAt: null,
						computerName: "Studio",
						remoteVisible: true,
					},
					{
						computerId: "desktop-1",
						sessionId: "session-1",
						agentSessionId: "session-1",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
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

		expect(screen.getAllByText("Computer Use")).toHaveLength(1);
		expect(screen.queryByText(/^Computer$/i)).toBeNull();
		expect(screen.getByText("Project Overview")).toBeTruthy();
		expect(container.innerHTML).not.toContain("New Computer Use");
		expect(container.innerHTML).not.toContain(
			"inline-flex size-5 items-center justify-center rounded-md bg-primary/10 text-primary",
		);
	});

	it("only marks the project-scoped current session as selected", () => {
		render(
			<SessionListPanel
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "project:9:mesh-langagent-session-main",
						agentSessionId: "agent-session-main",
						projectId: "mesh-lang",
						projectName: "mesh-lang",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Main",
						lastActivityAt: "2026-05-19T10:00:00Z",
						computerName: "Studio",
						remoteVisible: true,
					},
					{
						computerId: "desktop-1",
						sessionId: "project:4:xeroagent-session-main",
						agentSessionId: "agent-session-main",
						projectId: "xero",
						projectName: "xero",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Main",
						lastActivityAt: "2026-05-19T11:00:00Z",
						computerName: "Studio",
						remoteVisible: true,
					},
				]}
				currentSessionKey="desktop-1:project:4:xeroagent-session-main"
				onSelectSession={vi.fn()}
				onSignOut={vi.fn()}
			/>,
		);

		const rows = screen.getAllByRole("button", { name: "Open Main" });
		expect(rows).toHaveLength(2);
		expect(
			rows.filter((row) => row.getAttribute("aria-current") === "page"),
		).toHaveLength(1);
	});
});
