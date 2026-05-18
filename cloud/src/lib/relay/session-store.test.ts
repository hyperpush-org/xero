import { afterEach, describe, expect, it } from "vitest";

import { useSessionStore, type VisibleSessionSummary } from "./session-store";

afterEach(() => {
	useSessionStore.setState({
		transcripts: {},
		visibleSessions: [],
		visibleSessionsVersion: 0,
		visibleSessionsByComputerVersion: {},
	});
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

	it("replaces one desktop visible-session list and drops hidden transcripts", () => {
		useSessionStore.getState().setVisibleSessions([
			{
				computerId: "desktop-1",
				sessionId: "session-1",
				title: "Project Overview",
				lastActivityAt: "2026-05-16T23:32:05.323554Z",
				computerName: "Mac Studio",
			},
			{
				computerId: "desktop-2",
				sessionId: "session-2",
				title: "Other Project",
				lastActivityAt: "2026-05-16T20:32:05.323554Z",
				computerName: "MacBook",
			},
		]);
		useSessionStore.getState().replaceWithSnapshot("desktop-1:session-1", {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
		});
		useSessionStore.getState().replaceWithSnapshot("desktop-2:session-2", {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
		});

		useSessionStore
			.getState()
			.replaceVisibleSessionsForComputer("desktop-1", []);

		expect(useSessionStore.getState().visibleSessions).toEqual([
			{
				computerId: "desktop-2",
				sessionId: "session-2",
				title: "Other Project",
				lastActivityAt: "2026-05-16T20:32:05.323554Z",
				computerName: "MacBook",
			},
		]);
		expect(useSessionStore.getState().transcripts).not.toHaveProperty(
			"desktop-1:session-1",
		);
		expect(useSessionStore.getState().transcripts).toHaveProperty(
			"desktop-2:session-2",
		);
		expect(
			useSessionStore.getState().visibleSessionsByComputerVersion["desktop-1"],
		).toBeGreaterThan(0);
	});

	it("marks a desktop as reconciled even when its visible-session list is empty", () => {
		useSessionStore
			.getState()
			.replaceVisibleSessionsForComputer("desktop-1", []);

		expect(useSessionStore.getState().visibleSessions).toEqual([]);
		expect(
			useSessionStore.getState().visibleSessionsByComputerVersion["desktop-1"],
		).toBeGreaterThan(0);
	});

	it("merges live assistant deltas and tool lifecycle updates", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
		});

		useSessionStore.getState().appendTurn(
			key,
			{
				id: "transcript:run-1:1",
				kind: "message",
				role: "assistant",
				sequence: 1,
				text: "Hello ",
			},
			1,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "transcript:run-1:2",
				kind: "message",
				role: "assistant",
				sequence: 2,
				text: "world",
			},
			2,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-read:3",
				kind: "action",
				sequence: 3,
				toolCallId: "call-read",
				toolName: "read",
				title: "read",
				detail: "Started `read`.",
				detailRows: [{ label: "Input", value: "README.md" }],
				state: "running",
			},
			3,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-read:4",
				kind: "action",
				sequence: 4,
				toolCallId: "call-read",
				toolName: "read",
				title: "read",
				detail: "Read README.",
				detailRows: [{ label: "Output", value: "Project overview" }],
				state: "succeeded",
			},
			4,
		);

		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			{
				id: "transcript:run-1:1",
				kind: "message",
				role: "assistant",
				sequence: 2,
				text: "Hello world",
			},
			{
				id: "tool:run-1:call-read:3",
				kind: "action",
				sequence: 3,
				toolCallId: "call-read",
				toolName: "read",
				title: "read",
				detail: "Read README.",
				detailRows: [
					{ label: "Input", value: "README.md" },
					{ label: "Output", value: "Project overview" },
				],
				state: "succeeded",
			},
		]);
		expect(useSessionStore.getState().transcripts[key]?.lastSeq).toBe(4);
	});

	it("tracks selected runtime controls from the desktop snapshot", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [{ id: "ask", label: "Ask" }],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
		});

		useSessionStore.getState().updateControls(key, {
			agentId: "ask",
			modelId: "gpt-5.5",
		});

		expect(useSessionStore.getState().transcripts[key]).toMatchObject({
			currentAgentId: "ask",
			currentModelId: "gpt-5.5",
			availableModels: [{ id: "gpt-5.5", label: "gpt-5.5" }],
		});
	});
});
