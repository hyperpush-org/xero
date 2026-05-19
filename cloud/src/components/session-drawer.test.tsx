/** @vitest-environment jsdom */

import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

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

describe("SessionDrawer", () => {
	afterEach(() => {
		cleanup();
	});

	it("closes after a shared session is selected", async () => {
		const onSelectSession = vi.fn();
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-1",
						projectId: "project-1",
						projectName: "Clipstack",
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
		expect(screen.getByText(/Clipstack ·/)).toBeTruthy();
		expect(screen.queryByText(/Xero TUI/)).toBeNull();
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

	it("unlinks a linked session without selecting it", async () => {
		const onSelectSession = vi.fn();
		const onSetSessionRemoteVisibility = vi.fn(() => true);
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-3",
						projectId: "project-1",
						projectName: "Clipstack",
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: true,
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onSetSessionRemoteVisibility={onSetSessionRemoteVisibility}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		fireEvent.click(
			screen.getByRole("button", { name: "Unlink Project Overview" }),
		);

		expect(onSetSessionRemoteVisibility).toHaveBeenCalledWith(
			expect.objectContaining({ sessionId: "agent-session-3" }),
			false,
		);
		expect(onSelectSession).not.toHaveBeenCalled();
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
						projectId: "project-1",
						projectName: "Clipstack",
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

	it("links and selects an unshared session when it is opened", async () => {
		const onSelectSession = vi.fn();
		const onSetSessionRemoteVisibility = vi.fn(() => true);
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-2",
						projectId: "project-1",
						projectName: "Clipstack",
						title: "New Chat",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
						remoteVisible: false,
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onSetSessionRemoteVisibility={onSetSessionRemoteVisibility}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		expect(screen.queryByRole("button", { name: "Link New Chat" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Open New Chat" }));

		await waitFor(() => {
			expect(onSetSessionRemoteVisibility).toHaveBeenCalledWith(
				expect.objectContaining({ sessionId: "agent-session-2" }),
				true,
			);
			expect(onSelectSession).toHaveBeenCalledWith(
				"desktop-1",
				"agent-session-2",
			);
		});
	});
});
