import { describe, expect, it } from "vitest";

import {
	projectRemotePayloadToTurns,
	projectStreamItemsToTurns,
} from "./stream-projection";

describe("projectRemotePayloadToTurns", () => {
	it("maps desktop session snapshots from persisted runs", () => {
		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_session_snapshot.v1",
			projectId: "project-1",
			session: {
				agentSessionId: "agent-session-1",
			},
			runs: [
				{
					runId: "run-1",
					prompt: "Summarize the repo",
					status: "completed",
					messages: [
						{
							id: 11,
							role: "user",
							content: "Summarize the repo",
						},
						{
							id: 12,
							role: "assistant",
							content: "Here is the project overview.",
						},
					],
				},
			],
		});

		expect(turns).toEqual([
			{
				id: "transcript:run-1:11",
				kind: "message",
				role: "user",
				sequence: 1,
				text: "Summarize the repo",
			},
			{
				id: "transcript:run-1:12",
				kind: "message",
				role: "assistant",
				sequence: 2,
				text: "Here is the project overview.",
			},
		]);
	});

	it("uses the run prompt when a snapshot has no persisted user message yet", () => {
		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_session_snapshot.v1",
			runs: [
				{
					runId: "run-2",
					prompt: "Start a new task",
					status: "running",
					messages: [],
					events: [
						{
							id: 41,
							eventKind: "message_delta",
							payload: {
								role: "assistant",
								text: "Working on it.",
							},
						},
					],
				},
			],
		});

		expect(turns).toEqual([
			{
				id: "transcript:run-2:prompt",
				kind: "message",
				role: "user",
				sequence: 1,
				text: "Start a new task",
			},
			{
				id: "transcript:run-2:41",
				kind: "message",
				role: "assistant",
				sequence: 2,
				text: "Working on it.",
			},
		]);
	});

	it("maps wrapped remote runtime message events", () => {
		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_runtime_event.v1",
			runId: "run-3",
			eventId: 7,
			eventKind: "message_delta",
			payload: {
				role: "assistant",
				text: "A live update",
			},
		});

		expect(turns).toEqual([
			{
				id: "transcript:run-3:7",
				kind: "message",
				role: "assistant",
				sequence: 7,
				text: "A live update",
			},
		]);
	});
});

describe("projectStreamItemsToTurns", () => {
	it("continues to project raw runtime stream transcript items", () => {
		expect(
			projectStreamItemsToTurns([
				{
					kind: "transcript",
					runId: "run-1",
					sequence: 1,
					createdAt: "2026-05-17T00:00:00.000Z",
					transcriptRole: "assistant",
					text: "Raw stream item",
				},
			]),
		).toEqual([
			{
				id: "transcript:run-1:1",
				kind: "message",
				role: "assistant",
				sequence: 1,
				text: "Raw stream item",
			},
		]);
	});
});
