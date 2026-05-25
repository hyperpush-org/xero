import type { RuntimeStreamMediaAttachmentDto } from "@xero/ui/model/runtime-stream";
import { describe, expect, it } from "vitest";

import {
	projectRemotePayloadToTurns,
	projectStreamItemsToTurns,
} from "./stream-projection";

function imageMediaAttachment(
	overrides: Partial<RuntimeStreamMediaAttachmentDto> = {},
): RuntimeStreamMediaAttachmentDto {
	return {
		id: "media-browser-screenshot",
		kind: "image",
		mediaType: "image/png",
		title: "Browser screenshot",
		alt: "Screenshot from browser automation",
		sizeBytes: 67,
		width: 1,
		height: 1,
		source: {
			kind: "remote_artifact",
			artifactId: "artifact-browser-screenshot",
			computerId: "computer-1",
			sessionId: "session-1",
		},
		renderUrl: null,
		...overrides,
	};
}

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
				sequence: 41,
				text: "Working on it.",
			},
		]);
	});

	it("rebuilds live snapshot timelines from events when persisted assistant messages exist", () => {
		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_session_snapshot.v1",
			runs: [
				{
					runId: "run-rich",
					prompt: "What is this project about.",
					status: "running",
					messages: [
						{
							id: 12,
							role: "assistant",
							content: "Persisted final answer.",
						},
					],
					events: [
						{
							id: 1,
							eventKind: "message_delta",
							payload: {
								role: "user",
								text: "What is this project about.",
							},
						},
						{
							id: 2,
							eventKind: "context_manifest_recorded",
							payload: {
								summary: "Latest project context manifest recorded.",
							},
						},
						{
							id: 3,
							eventKind: "retrieval_performed",
							payload: {
								summary: "Latest project context retrieval.",
							},
						},
						{
							id: 4,
							eventKind: "reasoning_summary",
							payload: {
								summary: "Inspecting project details",
							},
						},
						{
							id: 5,
							eventKind: "message_delta",
							payload: {
								role: "assistant",
								text: "This project is ",
							},
						},
						{
							id: 6,
							eventKind: "message_delta",
							payload: {
								role: "assistant",
								text: "Clippster.",
							},
						},
					],
				},
			],
		});

		expect(turns).toEqual([
			{
				id: "transcript:run-rich:1",
				kind: "message",
				role: "user",
				sequence: 1,
				text: "What is this project about.",
			},
			{
				id: "thinking:run-rich:4",
				kind: "thinking",
				sequence: 4,
				text: "Inspecting project details",
			},
			{
				id: "transcript:run-rich:5",
				kind: "message",
				role: "assistant",
				sequence: 6,
				text: "This project is Clippster.",
			},
		]);
	});

	it("uses finalized messages while preserving terminal event timelines", () => {
		const finalAnswer = [
			"Here's a straightforward FizzBuzz in Elixir:",
			"",
			"```elixir",
			"for n <- 1..100 do",
			"  cond do",
			'    rem(n, 15) == 0 -> IO.puts("FizzBuzz")',
			'    rem(n, 3) == 0 -> IO.puts("Fizz")',
			'    rem(n, 5) == 0 -> IO.puts("Buzz")',
			"    true -> IO.puts(n)",
			"  end",
			"end",
			"```",
		].join("\n");

		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_session_snapshot.v1",
			runs: [
				{
					runId: "run-terminal",
					prompt: "How do I write fizz buzz in Elixir?",
					status: "completed",
					messages: [
						{
							id: 29,
							role: "user",
							content: "How do I write fizz buzz in Elixir?",
						},
						{
							id: 30,
							role: "assistant",
							content: finalAnswer,
						},
					],
					events: [
						{
							id: 1,
							eventKind: "message_delta",
							payload: {
								role: "user",
								text: "How do I write fizz buzz in Elixir?",
							},
						},
						{
							id: 2,
							eventKind: "context_manifest_recorded",
							payload: {
								summary: "Latest project context manifest recorded.",
							},
						},
						{
							id: 3,
							eventKind: "retrieval_performed",
							payload: {
								summary: "Latest project context retrieval.",
							},
						},
						{
							id: 4,
							eventKind: "message_delta",
							payload: {
								role: "assistant",
								text: "Here's a streamed fragment that should not win.",
							},
						},
					],
				},
			],
		});

		expect(turns).toEqual([
			{
				id: "transcript:run-terminal:29",
				kind: "message",
				role: "user",
				sequence: 1,
				text: "How do I write fizz buzz in Elixir?",
			},
			{
				id: "transcript:run-terminal:30",
				kind: "message",
				role: "assistant",
				sequence: 4,
				text: finalAnswer,
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

	it("maps wrapped remote tool image events without duplicating raw output", () => {
		const turns = projectRemotePayloadToTurns({
			schema: "xero.remote_runtime_event.v1",
			runId: "run-media",
			eventId: 9,
			eventKind: "tool_completed",
			payload: {
				toolCallId: "call-browser-screenshot",
				toolName: "browser",
				ok: true,
				summary: "Browser screenshot captured.",
				output: '{"type":"image","data":"..."}',
				mediaAttachments: [imageMediaAttachment()],
			},
		});

		expect(turns).toEqual([
			expect.objectContaining({
				id: "tool:run-media:call-browser-screenshot:9",
				kind: "action",
				toolCallId: "call-browser-screenshot",
				toolName: "browser",
				detail: "Browser screenshot captured.",
				detailRows: [],
				state: "succeeded",
				mediaAttachments: [
					expect.objectContaining({
						id: "media-browser-screenshot",
						kind: "image",
						mediaType: "image/png",
						source: expect.objectContaining({
							kind: "remote_artifact",
							artifactId: "artifact-browser-screenshot",
						}),
					}),
				],
			}),
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

	it("projects media-only transcript items and merges later assistant media", () => {
		const turns = projectStreamItemsToTurns([
			{
				kind: "transcript",
				runId: "run-media",
				sequence: 1,
				createdAt: "2026-05-17T00:00:00.000Z",
				transcriptRole: "assistant",
				text: null,
				mediaAttachments: [imageMediaAttachment()],
			},
			{
				kind: "transcript",
				runId: "run-media",
				sequence: 2,
				createdAt: "2026-05-17T00:00:01.000Z",
				transcriptRole: "assistant",
				text: "Screenshot captured.",
				mediaAttachments: [
					imageMediaAttachment({
						id: "media-browser-screenshot-2",
						title: "Second screenshot",
					}),
				],
			},
		]);

		expect(turns).toEqual([
			expect.objectContaining({
				id: "transcript:run-media:1",
				kind: "message",
				role: "assistant",
				sequence: 2,
				text: "Screenshot captured.",
				attachments: [
					expect.objectContaining({ id: "media-browser-screenshot" }),
					expect.objectContaining({ id: "media-browser-screenshot-2" }),
				],
			}),
		]);
	});

	it("projects runtime tool image outputs without the raw tool preview row", () => {
		const turns = projectStreamItemsToTurns([
			{
				kind: "tool",
				runId: "run-media",
				sequence: 4,
				createdAt: "2026-05-17T00:00:02.000Z",
				toolCallId: "call-browser-screenshot",
				toolName: "browser",
				toolState: "succeeded",
				detail: "Browser screenshot captured.",
				toolResultPreview: '{"type":"image","data":"..."}',
				mediaAttachments: [imageMediaAttachment()],
			},
		]);

		expect(turns).toEqual([
			expect.objectContaining({
				kind: "action",
				toolCallId: "call-browser-screenshot",
				detailRows: [
					{
						label: "Outcome",
						value: "Browser screenshot captured.",
					},
				],
				mediaAttachments: [
					expect.objectContaining({
						id: "media-browser-screenshot",
						previewSrc: undefined,
					}),
				],
			}),
		]);
	});
});
