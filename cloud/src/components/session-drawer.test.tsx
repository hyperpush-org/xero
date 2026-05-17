/** @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

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
	it("closes after a shared session is selected", async () => {
		const onSelectSession = vi.fn();
		render(
			<SessionDrawer
				session={session}
				visibleSessions={[
					{
						computerId: "desktop-1",
						sessionId: "agent-session-1",
						title: "Project Overview",
						lastActivityAt: "2026-05-17T07:00:00Z",
						computerName: "Xero TUI",
					},
				]}
				currentSessionKey={null}
				onSelectSession={onSelectSession}
				onSignOut={vi.fn()}
				trigger={<button type="button">Open sessions</button>}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Open sessions" }));
		fireEvent.click(screen.getByRole("button", { name: /Project Overview/i }));

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
});
