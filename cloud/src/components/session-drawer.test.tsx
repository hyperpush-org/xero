/** @vitest-environment jsdom */

import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CloudSession } from "#/lib/auth/session";

import { SessionDrawer } from "./session-drawer";

const session: CloudSession = {
	githubLogin: "snowdamiz",
	avatarUrl: null,
	deviceId: "web-1",
	accountId: "account-1",
	devices: [],
	relayToken: "relay-token",
	relayTokenExpiresAt: "2026-05-17T08:00:00Z",
};

const COLLAPSED_GROUPS_STORAGE_KEY = "xero.cloud.sidebar.projectCollapsed.v1";

function mockLocalStorage() {
	const values = new Map<string, string>();
	Object.defineProperty(window, "localStorage", {
		configurable: true,
		writable: true,
		value: {
			clear: vi.fn(() => values.clear()),
			getItem: vi.fn((key: string) => values.get(key) ?? null),
			removeItem: vi.fn((key: string) => values.delete(key)),
			setItem: vi.fn((key: string, value: string) => {
				values.set(key, value);
			}),
		},
	});
}

function mockMobileViewport() {
	Object.defineProperty(window, "matchMedia", {
		writable: true,
		value: (query: string) => ({
			matches: true,
			media: query,
			onchange: null,
			addListener: vi.fn(),
			removeListener: vi.fn(),
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}),
	});
	Object.defineProperty(window, "innerWidth", {
		writable: true,
		configurable: true,
		value: 320,
	});
}

describe("SessionDrawer", () => {
	beforeEach(() => {
		mockLocalStorage();
	});

	afterEach(() => {
		cleanup();
		window.localStorage.removeItem(COLLAPSED_GROUPS_STORAGE_KEY);
	});

	it("closes after a session is selected", async () => {
		const onSelectSession = vi.fn();
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-1",
						agentSessionId: "agent-session-1",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		expect(screen.getAllByText("Clipstack")).toHaveLength(1);
		expect(screen.queryByText(/Xero TUI/)).toBeNull();
		expect(
			document.querySelector(
				'.cloud-session-drawer-content [class*="safe-area-inset-top"]',
			),
		).toBeTruthy();
		fireEvent.click(
			screen.getByRole("button", { name: "Open Project Overview" }),
		);

		expect(onSelectSession).toHaveBeenCalledWith(
			"desktop-1",
			"agent-session-1",
		);
		await waitFor(() => {
			const content = document.querySelector(".cloud-session-drawer-content");
			expect(
				content === null || content.getAttribute("data-state") === "closed",
			).toBe(true);
		});
	});

	it("groups sessions under their project headers", () => {
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-a",
						agentSessionId: "agent-session-a",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
					{
						computerId: "desktop-1",
						sessionId: "agent-session-b",
						agentSessionId: "agent-session-b",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Refactor auth",
						lastActivityAt: "2026-05-17T06:30:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
					{
						computerId: "desktop-1",
						sessionId: "agent-session-c",
						agentSessionId: "agent-session-c",
						projectId: "project-2",
						projectName: "Mesh Lang",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Parse spec",
						lastActivityAt: "2026-05-17T05:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={vi.fn()}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));

		// Each project name appears exactly once — as a group header, not inside rows.
		expect(screen.getAllByText("Clipstack")).toHaveLength(1);
		expect(screen.getAllByText("Mesh Lang")).toHaveLength(1);
		// All three sessions render under their groups.
		expect(
			screen.getByRole("button", { name: "Open Project Overview" }),
		).toBeTruthy();
		expect(
			screen.getByRole("button", { name: "Open Refactor auth" }),
		).toBeTruthy();
		expect(
			screen.getByRole("button", { name: "Open Parse spec" }),
		).toBeTruthy();
		// Group with most recent session activity comes first.
		const collapseTriggers = screen.getAllByRole("button", {
			name: /^Collapse /,
		});
		expect(collapseTriggers[0].getAttribute("aria-label")).toBe(
			"Collapse Clipstack",
		);
		expect(collapseTriggers[1].getAttribute("aria-label")).toBe(
			"Collapse Mesh Lang",
		);
	});

	it("collapses a project group on header click", () => {
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-a",
						agentSessionId: "agent-session-a",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={vi.fn()}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		expect(
			screen.getByRole("button", { name: "Open Project Overview" }),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Collapse Clipstack" }));
		expect(
			screen.queryByRole("button", { name: "Open Project Overview" }),
		).toBeNull();
		expect(
			screen.getByRole("button", { name: "Expand Clipstack" }),
		).toBeTruthy();
	});

	it("archives a session after a two-tap confirmation", async () => {
		const onSelectSession = vi.fn();
		const onArchiveSession = vi.fn(() => true);
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-3",
						agentSessionId: "agent-session-3",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onArchiveSession={onArchiveSession}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		fireEvent.click(
			screen.getByRole("button", { name: "Archive Project Overview" }),
		);
		expect(onArchiveSession).not.toHaveBeenCalled();
		fireEvent.click(
			screen.getByRole("button", {
				name: "Confirm archive Project Overview",
			}),
		);

		expect(onArchiveSession).toHaveBeenCalledWith(
			expect.objectContaining({ sessionId: "agent-session-3" }),
		);
		expect(onSelectSession).not.toHaveBeenCalled();
	});

	it("selects any listed session directly", async () => {
		const onSelectSession = vi.fn();
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-2",
						agentSessionId: "agent-session-2",
						projectId: "project-1",
						projectName: "Clipstack",
						sessionKind: "standard",
						isComputerUse: false,
						title: "New Chat",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: false,
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		fireEvent.click(screen.getByRole("button", { name: "Open New Chat" }));

		expect(onSelectSession).toHaveBeenCalledWith(
			"desktop-1",
			"agent-session-2",
		);
	});

	it("keeps the mobile project picker open until a project is selected", async () => {
		mockMobileViewport();
		const onSelectProject = vi.fn();
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[]}
				projects={[
					{
						computerId: "desktop-1",
						projectId: "project-1",
						projectName: "Clipstack",
					},
					{
						computerId: "desktop-1",
						projectId: "project-2",
						projectName: "Mesh Lang",
					},
				]}
				currentSessionKey={null}
				onSelectSession={vi.fn()}
				onSelectProject={onSelectProject}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		fireEvent.click(
			await screen.findByRole("button", { name: "Start new session" }),
		);

		expect(
			await screen.findByRole("heading", { name: "New session" }),
		).toBeTruthy();
		expect(
			document
				.querySelector(".cloud-session-drawer-content")
				?.getAttribute("data-state"),
		).toBe("open");

		fireEvent.click(screen.getByRole("button", { name: "Mesh Lang" }));

		expect(onSelectProject).toHaveBeenCalledWith(
			expect.objectContaining({
				computerId: "desktop-1",
				projectId: "project-2",
			}),
		);
		await waitFor(() => {
			const content = document.querySelector(".cloud-session-drawer-content");
			expect(
				content === null || content.getAttribute("data-state") === "closed",
			).toBe(true);
		});
	});
});
