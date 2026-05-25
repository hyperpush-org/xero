import { describe, expect, it } from "vitest";

import type { RuntimeEnvelope } from "./envelope";
import {
	GLOBAL_COMPUTER_USE_PROJECT_ID,
	REMOTE_COMPUTER_USE_SESSION_ID,
} from "./session-store";
import {
	remoteSnapshotControlSelection,
	withGlobalComputerUseSessions,
} from "./use-session-stream";
import { remoteVisibleSessionUpdateFromEnvelope } from "./visible-sessions";

describe("remoteVisibleSessionUpdateFromEnvelope", () => {
	it("uses durable composer settings when a snapshot has no active runtime run", () => {
		expect(
			remoteSnapshotControlSelection({
				schema: "xero.remote_session_snapshot.v1",
				projectId: "project-1",
				session: { agentSessionId: "session-1" },
				runtimeRun: null,
				selectedControls: {
					runtimeAgentId: "debug",
					providerId: "xai",
					providerProfileId: "xai-default",
					modelId: "grok-4.3",
					thinkingEffort: "low",
					autoCompactEnabled: false,
				},
			}),
		).toEqual({
			agentId: "debug",
			modelId: "xai-default:grok-4.3",
			rawModelId: "grok-4.3",
			providerId: "xai",
			providerProfileId: "xai-default",
			thinkingEffort: "low",
			autoCompactEnabled: false,
		});
	});

	it("prefers durable composer settings over terminal run controls", () => {
		expect(
			remoteSnapshotControlSelection({
				schema: "xero.remote_session_snapshot.v1",
				projectId: "project-1",
				session: { agentSessionId: "session-1" },
				runtimeRun: {
					status: "stopped",
					providerId: "openai_codex",
					controls: {
						active: {
							runtimeAgentId: "ask",
							providerProfileId: "openai-default",
							modelId: "gpt-5.1",
							autoCompactEnabled: true,
						},
					},
				},
				selectedControls: {
					runtimeAgentId: "engineer",
					providerId: "xai",
					providerProfileId: "xai-default",
					modelId: "grok-4.3",
				},
			}),
		).toMatchObject({
			agentId: "engineer",
			modelId: "xai-default:grok-4.3",
			rawModelId: "grok-4.3",
			providerId: "xai",
		});
	});

	it("maps desktop session lists into cloud summaries without transcript content", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_sessions.v1",
				sessions: [
					{
						projectId: "project-1",
						projectName: "Clipstack",
						session: {
							agent_session_id: "session-1",
							title: "Project Overview",
							updated_at: "2026-05-16T20:49:15Z",
						},
					},
					{
						projectId: "project-1",
						session: {
							sessionId: "session-2",
							projectName: "Clipstack",
							title: "Follow-up Session",
							sessionKind: "computer_use",
							remoteVisible: true,
							updatedAt: "2026-05-16T21:49:15Z",
						},
					},
				],
			},
		};

		expect(
			remoteVisibleSessionUpdateFromEnvelope(envelope, [
				{
					id: "desktop-1",
					account_id: "account-1",
					kind: "desktop",
					name: "Mac Studio",
					user_agent: null,
					last_seen: null,
					created_at: null,
					revoked_at: null,
				},
			]),
		).toEqual({
			kind: "replace",
			computerId: "desktop-1",
			sessions: [
				{
					computerId: "desktop-1",
					sessionId: "session-1",
					agentSessionId: "session-1",
					projectId: "project-1",
					projectName: "Clipstack",
					sessionKind: "standard",
					isComputerUse: false,
					title: "Project Overview",
					lastActivityAt: "2026-05-16T20:49:15Z",
					computerName: "Mac Studio",
					remoteVisible: true,
				},
				{
					computerId: "desktop-1",
					sessionId: "session-2",
					agentSessionId: "session-2",
					projectId: "project-1",
					projectName: "Clipstack",
					sessionKind: "computer_use",
					isComputerUse: true,
					title: "Follow-up Session",
					lastActivityAt: "2026-05-16T21:49:15Z",
					computerName: "Mac Studio",
					remoteVisible: true,
				},
			],
		});
	});

	it("uses project-scoped remote session ids from the desktop list", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_sessions.v1",
				sessions: [
					{
						projectId: "mesh-lang",
						projectName: "mesh-lang",
						session: {
							remoteSessionId: "project:9:mesh-langagent-session-main",
							agentSessionId: "agent-session-main",
							title: "Main",
						},
					},
					{
						projectId: "xero",
						projectName: "xero",
						session: {
							remoteSessionId: "project:4:xeroagent-session-main",
							agentSessionId: "agent-session-main",
							title: "Main",
						},
					},
				],
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toEqual({
			kind: "replace",
			computerId: "desktop-1",
			sessions: [
				expect.objectContaining({
					sessionId: "project:9:mesh-langagent-session-main",
					agentSessionId: "agent-session-main",
					projectId: "mesh-lang",
				}),
				expect.objectContaining({
					sessionId: "project:4:xeroagent-session-main",
					agentSessionId: "agent-session-main",
					projectId: "xero",
				}),
			],
		});
	});

	it("keeps hidden Computer Use backing sessions out of project session updates", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_sessions.v1",
				sessions: [
					{
						projectId: "project-1",
						projectName: "Clipstack",
						session: {
							agentSessionId: "session-1",
							title: "Project Overview",
							remoteVisible: true,
						},
					},
					{
						projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
						projectName: "Computer Use",
						session: {
							agentSessionId: "agent-session-global-computer-use",
							title: "Computer Use",
							sessionKind: "computer_use",
							remoteVisible: false,
						},
					},
				],
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toEqual({
			kind: "replace",
			computerId: "desktop-1",
			sessions: [
				expect.objectContaining({
					sessionId: "session-1",
					agentSessionId: "session-1",
					isComputerUse: false,
				}),
			],
		});
	});

	it("adds one global Computer Use surface for each online desktop", () => {
		expect(
			withGlobalComputerUseSessions([], { "desktop-1": true }, [
				{
					id: "desktop-1",
					account_id: "account-1",
					kind: "desktop",
					name: "Studio",
					user_agent: null,
					last_seen: null,
					created_at: null,
					revoked_at: null,
				},
			]),
		).toEqual([
			{
				computerId: "desktop-1",
				sessionId: REMOTE_COMPUTER_USE_SESSION_ID,
				agentSessionId: REMOTE_COMPUTER_USE_SESSION_ID,
				projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
				projectName: null,
				sessionKind: "computer_use",
				isComputerUse: true,
				title: "Computer Use",
				lastActivityAt: null,
				computerName: "Studio",
				remoteVisible: true,
			},
		]);
	});

	it("ignores frames that are not from the desktop session-list channel", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "session-1",
			kind: "event",
			payload: {
				schema: "xero.remote_sessions.v1",
				sessions: [],
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toBeNull();
	});

	it("maps desktop session-removed frames into removal updates", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 2,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "session_removed",
			payload: {
				schema: "xero.remote_session_removed.v1",
				projectId: "project-1",
				remoteSessionId: "project:9:project-1session-1",
				sessionId: "session-1",
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toEqual({
			kind: "remove",
			computerId: "desktop-1",
			sessionId: "project:9:project-1session-1",
		});
	});

	it("maps desktop session-added frames into upsert updates", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 3,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_session_added.v1",
				result: {
					projectId: "project-1",
					projectName: "Clipstack",
					session: {
						remoteSessionId: "project:9:project-1session-3",
						agentSessionId: "session-3",
						title: "Simple Addition",
						remoteVisible: true,
						updatedAt: "2026-05-20T20:42:00Z",
					},
				},
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toEqual({
			kind: "upsert",
			summary: {
				computerId: "desktop-1",
				sessionId: "project:9:project-1session-3",
				agentSessionId: "session-3",
				projectId: "project-1",
				projectName: "Clipstack",
				sessionKind: "standard",
				isComputerUse: false,
				title: "Simple Addition",
				lastActivityAt: "2026-05-20T20:42:00Z",
				computerName: null,
				remoteVisible: true,
			},
		});
	});
});
