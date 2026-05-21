import { describe, expect, it } from "vitest";

import type { RuntimeEnvelope } from "./envelope";
import { remoteVisibleSessionUpdateFromEnvelope } from "./visible-sessions";

describe("remoteVisibleSessionUpdateFromEnvelope", () => {
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
							agentSessionId: "session-2",
							projectName: "Clipstack",
							title: "Follow-up Session",
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
					projectId: "project-1",
					projectName: "Clipstack",
					title: "Project Overview",
					lastActivityAt: "2026-05-16T20:49:15Z",
					computerName: "Mac Studio",
					remoteVisible: true,
				},
				{
					computerId: "desktop-1",
					sessionId: "session-2",
					projectId: "project-1",
					projectName: "Clipstack",
					title: "Follow-up Session",
					lastActivityAt: "2026-05-16T21:49:15Z",
					computerName: "Mac Studio",
					remoteVisible: true,
				},
			],
		});
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
				sessionId: "session-1",
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toEqual({
			kind: "remove",
			computerId: "desktop-1",
			sessionId: "session-1",
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
				sessionId: "session-3",
				projectId: "project-1",
				projectName: "Clipstack",
				title: "Simple Addition",
				lastActivityAt: "2026-05-20T20:42:00Z",
				computerName: null,
				remoteVisible: true,
			},
		});
	});
});
