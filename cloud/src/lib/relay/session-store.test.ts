import { afterEach, describe, expect, it } from "vitest";

import {
	modelOptionId,
	normalizeModelOptions,
	parseThinkingEffort,
	useSessionStore,
	type VisibleSessionSummary,
} from "./session-store";

function sessionSummary(
	overrides: Partial<VisibleSessionSummary> = {},
): VisibleSessionSummary {
	return {
		computerId: "desktop-1",
		sessionId: "session-1",
		agentSessionId: "session-1",
		projectId: "project-1",
		projectName: "Clipstack",
		sessionKind: "standard",
		isComputerUse: false,
		title: "Project Overview",
		lastActivityAt: "2026-05-16T23:32:05.323554Z",
		computerName: "Mac Studio",
		remoteVisible: true,
		...overrides,
	};
}

afterEach(() => {
	useSessionStore.setState({
		transcripts: {},
		visibleSessions: [],
		visibleSessionsVersion: 0,
		visibleSessionsByComputerVersion: {},
		onlineComputerIds: {},
		computerPresenceKnown: false,
	});
});

describe("session store", () => {
	it("treats identical visible-session snapshots as a no-op", () => {
		const sessions: VisibleSessionSummary[] = [sessionSummary()];
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
		const session: VisibleSessionSummary = sessionSummary();
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

	it("replaces one desktop visible-session list and drops missing transcripts", () => {
		useSessionStore.getState().setVisibleSessions([
			sessionSummary(),
			sessionSummary({
				computerId: "desktop-2",
				sessionId: "session-2",
				projectId: "project-2",
				projectName: "Other Workspace",
				title: "Other Project",
				lastActivityAt: "2026-05-16T20:32:05.323554Z",
				computerName: "MacBook",
			}),
		]);
		useSessionStore.getState().replaceWithSnapshot("desktop-1:session-1", {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});
		useSessionStore.getState().replaceWithSnapshot("desktop-2:session-2", {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore
			.getState()
			.replaceVisibleSessionsForComputer("desktop-1", []);

		expect(useSessionStore.getState().visibleSessions).toEqual([
			sessionSummary({
				computerId: "desktop-2",
				sessionId: "session-2",
				projectId: "project-2",
				projectName: "Other Workspace",
				title: "Other Project",
				lastActivityAt: "2026-05-16T20:32:05.323554Z",
				computerName: "MacBook",
			}),
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

	it("removes a visible session and prunes its transcript", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().setVisibleSessions([sessionSummary()]);
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().removeVisibleSession("desktop-1", "session-1");

		expect(useSessionStore.getState().visibleSessions).toEqual([]);
		expect(useSessionStore.getState().transcripts).not.toHaveProperty(key);
		expect(
			useSessionStore.getState().visibleSessionsByComputerVersion["desktop-1"],
		).toBeGreaterThan(0);
	});

	it("keeps transcripts for listed sessions regardless of legacy visibility flags", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().setVisibleSessions([sessionSummary()]);
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore
			.getState()
			.replaceVisibleSessionsForComputer("desktop-1", [
				sessionSummary({ remoteVisible: false }),
			]);

		expect(useSessionStore.getState().visibleSessions).toEqual([
			sessionSummary({ remoteVisible: false }),
		]);
		expect(useSessionStore.getState().transcripts).toHaveProperty(key);
	});

	it("tracks online desktop presence separately from visible-session reconciliation", () => {
		useSessionStore.getState().setOnlineComputers(["desktop-1", "desktop-1"]);

		expect(useSessionStore.getState().computerPresenceKnown).toBe(true);
		expect(useSessionStore.getState().onlineComputerIds).toEqual({
			"desktop-1": true,
		});
		expect(useSessionStore.getState().visibleSessionsByComputerVersion).toEqual(
			{},
		);

		useSessionStore.getState().resetComputerPresence();

		expect(useSessionStore.getState().computerPresenceKnown).toBe(false);
		expect(useSessionStore.getState().onlineComputerIds).toEqual({});
	});

	it("keeps loaded transcripts during optimistic visible-session refreshes", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().setVisibleSessions([sessionSummary()]);
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [
				{
					id: "turn-1",
					kind: "message",
					role: "assistant",
					sequence: 1,
					text: "Already loaded",
				},
			],
			lastSeq: 1,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().clearVisibleSessionsForComputers(["desktop-1"]);

		expect(useSessionStore.getState().visibleSessions).toEqual([]);
		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			{
				id: "turn-1",
				kind: "message",
				role: "assistant",
				sequence: 1,
				text: "Already loaded",
			},
		]);
	});

	it("keeps transcripts when non-authoritative visible-session hints are empty", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().setVisibleSessions([sessionSummary()]);
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [
				{
					id: "turn-1",
					kind: "message",
					role: "assistant",
					sequence: 1,
					text: "Still streaming",
				},
			],
			lastSeq: 1,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().setVisibleSessions([]);

		expect(useSessionStore.getState().visibleSessions).toEqual([]);
		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			{
				id: "turn-1",
				kind: "message",
				role: "assistant",
				sequence: 1,
				text: "Still streaming",
			},
		]);
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
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
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

	it("regroups live completed tool bursts without waiting for a snapshot reload", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 0,
			isLive: true,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-read:1",
				kind: "action",
				sequence: 1,
				toolCallId: "call-read",
				toolName: "read",
				title: "read",
				detail: "Reading package.json.",
				detailRows: [],
				state: "running",
			},
			1,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-read:2",
				kind: "action",
				sequence: 2,
				toolCallId: "call-read",
				toolName: "read",
				title: "read",
				detail: "Read package.json.",
				detailRows: [{ label: "Output", value: "package contents" }],
				state: "succeeded",
			},
			2,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-find:3",
				kind: "action",
				sequence: 3,
				toolCallId: "call-find",
				toolName: "find",
				title: "find",
				detail: "Finding matching files.",
				detailRows: [],
				state: "running",
			},
			3,
		);

		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			expect.objectContaining({
				kind: "action",
				toolCallId: "call-read",
				state: "succeeded",
			}),
			expect.objectContaining({
				kind: "action",
				toolCallId: "call-find",
				state: "running",
			}),
		]);

		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-find:4",
				kind: "action",
				sequence: 4,
				toolCallId: "call-find",
				toolName: "find",
				title: "find",
				detail: "Found matching files.",
				detailRows: [{ label: "Output", value: "src/index.ts" }],
				state: "succeeded",
			},
			4,
		);

		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			expect.objectContaining({
				kind: "action_group",
				title: "2 tool calls",
				actions: [
					expect.objectContaining({
						toolCallId: "call-read",
						state: "succeeded",
					}),
					expect.objectContaining({
						toolCallId: "call-find",
						state: "succeeded",
					}),
				],
			}),
		]);

		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-list:5",
				kind: "action",
				sequence: 5,
				toolCallId: "call-list",
				toolName: "list",
				title: "list",
				detail: "Listing source files.",
				detailRows: [],
				state: "running",
			},
			5,
		);
		useSessionStore.getState().appendTurn(
			key,
			{
				id: "tool:run-1:call-list:6",
				kind: "action",
				sequence: 6,
				toolCallId: "call-list",
				toolName: "list",
				title: "list",
				detail: "Listed source files.",
				detailRows: [{ label: "Output", value: "src/index.ts\nsrc/app.ts" }],
				state: "succeeded",
			},
			6,
		);

		expect(useSessionStore.getState().transcripts[key]?.turns).toEqual([
			expect.objectContaining({
				kind: "action_group",
				title: "3 tool calls",
				actions: [
					expect.objectContaining({ toolCallId: "call-read" }),
					expect.objectContaining({ toolCallId: "call-find" }),
					expect.objectContaining({ toolCallId: "call-list" }),
				],
			}),
		]);
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
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().updateControls(key, {
			agentId: "ask",
			modelId: "gpt-5.5",
		});

		expect(useSessionStore.getState().transcripts[key]).toMatchObject({
			currentAgentId: "ask",
			currentModelId: "gpt-5.5",
			availableModels: [
				{
					id: "gpt-5.5",
					label: "gpt-5.5",
					modelId: "gpt-5.5",
					providerId: null,
					providerProfileId: null,
				},
			],
		});
	});

	it("tracks cloud context snapshots for the composer indicator", () => {
		const key = "desktop-1:session-1";
		useSessionStore.getState().replaceWithSnapshot(key, {
			turns: [],
			lastSeq: 7,
			isLive: false,
			availableAgents: [],
			availableModels: [],
			currentAgentId: null,
			currentModelId: null,
			currentThinkingEffort: null,
			currentAutoCompactEnabled: true,
		});

		useSessionStore.getState().updateContextSnapshot(key, {
			requestId: "context-request-1",
			error: null,
			seq: 9,
			snapshot: {
				modelId: "gpt-5.5",
				budget: {
					effectiveInputBudgetTokens: 100_000,
					estimatedTokens: 42_000,
					knownProviderBudget: true,
					pressure: "medium",
					pressurePercent: 42,
					remainingTokens: 58_000,
				},
			},
		});

		expect(useSessionStore.getState().transcripts[key]).toMatchObject({
			contextSnapshotRequestId: "context-request-1",
			contextSnapshotError: null,
			lastSeq: 9,
			contextSnapshot: {
				modelId: "gpt-5.5",
				budget: {
					remainingTokens: 58_000,
					pressurePercent: 42,
				},
			},
		});
	});

	it("builds provider-scoped model option ids for cloud model picks", () => {
		expect(modelOptionId("openai_codex-default", "gpt-5.5")).toBe(
			"openai_codex-default:gpt-5.5",
		);
		expect(modelOptionId(null, "gpt-5.5")).toBe("gpt-5.5");
		expect(modelOptionId("openai_codex-default", "   ")).toBeNull();
	});

	it("accepts disabled thinking as a synced control value", () => {
		expect(parseThinkingEffort("none")).toBe("none");
	});

	it("preserves provider labels when normalizing cloud model options", () => {
		expect(
			normalizeModelOptions([
				{
					id: "openai_codex-default:gpt-5.5",
					label: "GPT-5.5",
					modelId: "gpt-5.5",
					providerId: "openai_codex",
					providerLabel: "OpenAI Codex",
					providerProfileId: "openai_codex-default",
				},
			]),
		).toEqual([
			expect.objectContaining({
				id: "openai_codex-default:gpt-5.5",
				label: "GPT-5.5",
				modelId: "gpt-5.5",
				providerId: "openai_codex",
				providerLabel: "OpenAI Codex",
				providerProfileId: "openai_codex-default",
			}),
		]);
	});
});
