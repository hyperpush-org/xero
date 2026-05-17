import { describe, expect, it } from "vitest";

import type { RuntimeEnvelope } from "./envelope";
import { remoteVisibleSessionUpdateFromEnvelope } from "./visible-sessions";

describe("remoteVisibleSessionUpdateFromEnvelope", () => {
	it("maps desktop visible-session lists into cloud summaries", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_visible_sessions.v1",
				sessions: [
					{
						projectId: "project-1",
						session: {
							agent_session_id: "session-1",
							title: "Project Overview",
							updated_at: "2026-05-16T20:49:15Z",
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
					title: "Project Overview",
					lastActivityAt: "2026-05-16T20:49:15Z",
					computerName: "Mac Studio",
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
				schema: "xero.remote_visible_sessions.v1",
				sessions: [],
			},
		};

		expect(remoteVisibleSessionUpdateFromEnvelope(envelope)).toBeNull();
	});
});
