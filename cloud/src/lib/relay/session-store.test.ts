import { afterEach, describe, expect, it } from "vitest";

import { useSessionStore, type VisibleSessionSummary } from "./session-store";

afterEach(() => {
	useSessionStore.setState({ transcripts: {}, visibleSessions: [] });
});

describe("session store", () => {
	it("treats identical visible-session snapshots as a no-op", () => {
		const sessions: VisibleSessionSummary[] = [
			{
				computerId: "desktop-1",
				sessionId: "session-1",
				title: "Project Overview",
				lastActivityAt: "2026-05-16T23:32:05.323554Z",
				computerName: "Mac Studio",
			},
		];
		let notifications = 0;
		const unsubscribe = useSessionStore.subscribe(() => {
			notifications += 1;
		});

		try {
			useSessionStore.getState().setVisibleSessions(sessions);
			const currentSessions = useSessionStore.getState().visibleSessions;

			useSessionStore.getState().setVisibleSessions([{ ...sessions[0] }]);

			expect(notifications).toBe(1);
			expect(useSessionStore.getState().visibleSessions).toBe(currentSessions);
		} finally {
			unsubscribe();
		}
	});

	it("publishes visible-session updates when any summary field changes", () => {
		const session: VisibleSessionSummary = {
			computerId: "desktop-1",
			sessionId: "session-1",
			title: "Project Overview",
			lastActivityAt: "2026-05-16T23:32:05.323554Z",
			computerName: "Mac Studio",
		};
		let notifications = 0;
		const unsubscribe = useSessionStore.subscribe(() => {
			notifications += 1;
		});

		try {
			useSessionStore.getState().setVisibleSessions([session]);
			useSessionStore
				.getState()
				.setVisibleSessions([{ ...session, title: "New Chat" }]);

			expect(notifications).toBe(2);
			expect(useSessionStore.getState().visibleSessions[0]?.title).toBe(
				"New Chat",
			);
		} finally {
			unsubscribe();
		}
	});
});
