import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import type { RuntimeStreamItemDto } from "@xero/ui/model/runtime-stream";

const REMOTE_SESSION_SNAPSHOT_SCHEMA = "xero.remote_session_snapshot.v1";
const REMOTE_RUNTIME_EVENT_SCHEMA = "xero.remote_runtime_event.v1";
const RUNTIME_STREAM_ITEM_KINDS = new Set([
	"transcript",
	"tool",
	"action_required",
	"failure",
]);

/**
 * Project an incremental list of `RuntimeStreamItemDto`s into the
 * `ConversationTurn[]` shape consumed by `<ConversationSection>`.
 *
 * The desktop's projection logic lives next to its Zustand store and accounts
 * for many cross-pane concerns (subagents, plan trays, layout heuristics).
 * The cloud only renders a single session in a single pane, so this projection
 * is deliberately minimal — it preserves user/assistant text, tool runs,
 * action prompts, and failure notices, which together cover the mockup.
 */
export function projectStreamItemsToTurns(
	items: readonly RuntimeStreamItemDto[],
): ConversationTurn[] {
	const turns: ConversationTurn[] = [];
	for (const item of items) {
		const turn = mapItem(item);
		if (turn) turns.push(turn);
	}
	return turns;
}

/**
 * Desktop remote frames use a relay-specific envelope payload. Session
 * snapshots carry persisted agent `runs`, while live updates carry wrapped
 * runtime events. Normalize those shapes before projecting into UI turns.
 */
export function projectRemotePayloadToTurns(
	payload: unknown,
): ConversationTurn[] {
	if (Array.isArray(payload)) {
		return payload.flatMap((item) => projectRemotePayloadToTurns(item));
	}
	if (!isRecord(payload)) return [];
	if (isRuntimeStreamItemPayload(payload)) {
		return projectStreamItemsToTurns([payload as RuntimeStreamItemDto]);
	}
	if (payload.schema === REMOTE_SESSION_SNAPSHOT_SCHEMA) {
		const transcript = recordArray(payload, "transcript");
		if (transcript.length > 0) {
			return transcript.flatMap((item) => projectRemotePayloadToTurns(item));
		}
		return projectRemoteSnapshotRunsToTurns(recordArray(payload, "runs"));
	}
	if (payload.schema === REMOTE_RUNTIME_EVENT_SCHEMA) {
		return projectRemoteRuntimeEventToTurns(payload);
	}
	return [];
}

function mapItem(item: RuntimeStreamItemDto): ConversationTurn | null {
	switch (item.kind) {
		case "transcript":
			return mapTranscript(item);
		case "tool":
			return mapTool(item);
		case "action_required":
			return mapAction(item);
		case "failure":
			return mapFailure(item);
		default:
			return null;
	}
}

function projectRemoteSnapshotRunsToTurns(
	runs: readonly Record<string, unknown>[],
): ConversationTurn[] {
	const turns: ConversationTurn[] = [];
	let nextSequence = 1;

	for (const [runIndex, run] of runs.entries()) {
		const runId = stringField(run, "runId") ?? `run-${runIndex + 1}`;
		const messages = recordArray(run, "messages");
		const hasUserMessage = messages.some(
			(message) =>
				stringField(message, "role") === "user" &&
				nonEmptyStringField(message, "content"),
		);
		if (!hasUserMessage) {
			const prompt = nonEmptyStringField(run, "prompt");
			if (prompt) {
				turns.push({
					id: `transcript:${runId}:prompt`,
					kind: "message",
					role: "user",
					sequence: nextSequence,
					text: prompt,
				});
				nextSequence += 1;
			}
		}

		for (const message of messages) {
			const turn = remoteMessageToTurn(message, runId, nextSequence);
			if (!turn) continue;
			turns.push(turn);
			nextSequence += 1;
		}

		if (
			!messages.some(
				(message) =>
					stringField(message, "role") === "assistant" &&
					nonEmptyStringField(message, "content"),
			)
		) {
			for (const event of recordArray(run, "events")) {
				const eventTurn = remoteAgentRunEventToTurn(event, runId, nextSequence);
				if (!eventTurn) continue;
				turns.push(eventTurn);
				nextSequence += 1;
			}
		}

		const failure = remoteRunFailureToTurn(run, runId, nextSequence);
		if (failure) {
			turns.push(failure);
			nextSequence += 1;
		}
	}

	return turns;
}

function remoteMessageToTurn(
	message: Record<string, unknown>,
	runId: string,
	sequence: number,
): ConversationTurn | null {
	const role = stringField(message, "role");
	if (role !== "user" && role !== "assistant") return null;
	const text = nonEmptyStringField(message, "content");
	if (!text) return null;
	return {
		id: `transcript:${runId}:${numberField(message, "id") ?? sequence}`,
		kind: "message",
		role,
		sequence,
		text,
	};
}

function remoteAgentRunEventToTurn(
	event: Record<string, unknown>,
	runId: string,
	sequence: number,
): ConversationTurn | null {
	const eventKind = stringField(event, "eventKind");
	const payload = recordField(event, "payload");
	if (eventKind === "message_delta" && payload) {
		return remoteMessageDeltaToTurn(payload, runId, sequence, event);
	}
	if (eventKind === "run_failed") {
		const message =
			nonEmptyStringField(payload, "message") ??
			nonEmptyStringField(event, "message") ??
			"Agent run failed.";
		return {
			id: `failure:${runId}:${numberField(event, "id") ?? sequence}`,
			kind: "failure",
			sequence,
			message,
			code:
				nonEmptyStringField(payload, "code") ??
				nonEmptyStringField(event, "code") ??
				"run_failed",
		};
	}
	return null;
}

function projectRemoteRuntimeEventToTurns(
	event: Record<string, unknown>,
): ConversationTurn[] {
	const payload = recordField(event, "payload");
	if (payload && isRuntimeStreamItemPayload(payload)) {
		return projectStreamItemsToTurns([payload as RuntimeStreamItemDto]);
	}

	const runId = stringField(event, "runId") ?? "run";
	const sequence = numberField(event, "eventId") ?? 1;
	const eventKind = stringField(event, "eventKind");

	if (eventKind === "message_delta" && payload) {
		const turn = remoteMessageDeltaToTurn(payload, runId, sequence, event);
		return turn ? [turn] : [];
	}

	if (eventKind === "tool_started" && payload) {
		const turn = remoteToolEventToTurn(
			payload,
			runId,
			sequence,
			event,
			"running",
		);
		return turn ? [turn] : [];
	}

	if (eventKind === "tool_completed" && payload) {
		const ok = booleanField(payload, "ok");
		const turn = remoteToolEventToTurn(
			payload,
			runId,
			sequence,
			event,
			ok === false ? "failed" : "succeeded",
		);
		return turn ? [turn] : [];
	}

	if (eventKind === "action_required" && payload) {
		const turn = remoteActionRequiredToTurn(payload, runId, sequence, event);
		return turn ? [turn] : [];
	}

	if (eventKind === "run_failed") {
		const turn = remoteAgentRunEventToTurn(event, runId, sequence);
		return turn ? [turn] : [];
	}

	return [];
}

function remoteMessageDeltaToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event?: Record<string, unknown>,
): ConversationTurn | null {
	const role = stringField(payload, "role");
	if (role !== "user" && role !== "assistant") return null;
	const text = nonEmptyStringField(payload, "text");
	if (!text) return null;
	return {
		id: `transcript:${runId}:${numberField(event, "eventId", "id") ?? sequence}`,
		kind: "message",
		role,
		sequence,
		text,
	};
}

function remoteToolEventToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event: Record<string, unknown>,
	state: "running" | "succeeded" | "failed",
): ConversationTurn | null {
	const toolCallId = stringField(payload, "toolCallId");
	const toolName = stringField(payload, "toolName");
	if (!toolCallId || !toolName) return null;
	return {
		id: `tool:${runId}:${toolCallId}:${numberField(event, "eventId") ?? sequence}`,
		kind: "action",
		sequence,
		toolCallId,
		toolName,
		title: toolName,
		detail: nonEmptyStringField(payload, "outcome") ?? "",
		detailRows: [],
		state,
	};
}

function remoteActionRequiredToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event: Record<string, unknown>,
): ConversationTurn | null {
	const actionId =
		stringField(payload, "actionId") ??
		`remote-action-${numberField(event, "eventId") ?? sequence}`;
	const actionType = stringField(payload, "actionType") ?? "operator_review";
	const title = stringField(payload, "title") ?? "Action required";
	const detail = stringField(payload, "detail") ?? "";
	return {
		id: `action:${runId}:${actionId}`,
		kind: "action_prompt",
		sequence,
		actionId,
		actionType,
		title,
		detail,
		shape: "plain_text",
		options: null,
		allowMultiple: false,
		pendingDecision: null,
		isResolved: false,
	};
}

function remoteRunFailureToTurn(
	run: Record<string, unknown>,
	runId: string,
	sequence: number,
): ConversationTurn | null {
	if (stringField(run, "status") !== "failed") return null;
	const lastError = recordField(run, "lastError");
	const message =
		nonEmptyStringField(lastError, "message") ??
		nonEmptyStringField(run, "lastError") ??
		"Agent run failed.";
	return {
		id: `failure:${runId}:terminal`,
		kind: "failure",
		sequence,
		message,
		code:
			nonEmptyStringField(lastError, "code") ??
			nonEmptyStringField(run, "lastErrorCode") ??
			"run_failed",
	};
}

function mapTranscript(item: RuntimeStreamItemDto): ConversationTurn | null {
	if (!item.text || !item.transcriptRole) return null;
	const role = item.transcriptRole;
	if (role !== "user" && role !== "assistant") return null;
	return {
		id: `transcript:${item.runId ?? "run"}:${item.sequence}`,
		kind: "message",
		role,
		sequence: item.sequence,
		text: item.text,
	};
}

function mapTool(item: RuntimeStreamItemDto): ConversationTurn | null {
	if (!item.toolName || !item.toolCallId) return null;
	return {
		id: `tool:${item.toolCallId}`,
		kind: "action",
		sequence: item.sequence,
		toolCallId: item.toolCallId,
		toolName: item.toolName,
		title: item.toolName,
		detail: "",
		detailRows: [],
		state: item.toolState ?? null,
	};
}

function mapAction(item: RuntimeStreamItemDto): ConversationTurn | null {
	if (!item.actionId || !item.title || !item.answerShape || !item.actionType)
		return null;
	return {
		id: `action:${item.actionId}`,
		kind: "action_prompt",
		sequence: item.sequence,
		actionId: item.actionId,
		actionType: item.actionType,
		title: item.title,
		detail: item.detail ?? "",
		shape: item.answerShape,
		options: item.options ?? null,
		allowMultiple: item.allowMultiple ?? false,
		pendingDecision: null,
		isResolved: false,
	};
}

function mapFailure(item: RuntimeStreamItemDto): ConversationTurn | null {
	if (!item.message) return null;
	return {
		id: `failure:${item.runId ?? "run"}:${item.sequence}`,
		kind: "failure",
		sequence: item.sequence,
		message: item.message,
		code: item.code ?? "failure",
	};
}

function isRuntimeStreamItemPayload(value: Record<string, unknown>): boolean {
	const kind = stringField(value, "kind");
	return Boolean(kind && RUNTIME_STREAM_ITEM_KINDS.has(kind));
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function recordField(
	value: Record<string, unknown> | undefined | null,
	key: string,
): Record<string, unknown> | null {
	if (!value) return null;
	const candidate = value[key];
	return isRecord(candidate) ? candidate : null;
}

function recordArray(
	value: Record<string, unknown>,
	key: string,
): Record<string, unknown>[] {
	const candidate = value[key];
	return Array.isArray(candidate) ? candidate.filter(isRecord) : [];
}

function stringField(
	value: Record<string, unknown> | undefined | null,
	...keys: string[]
): string | null {
	if (!value) return null;
	for (const key of keys) {
		const candidate = value[key];
		if (typeof candidate === "string") return candidate;
	}
	return null;
}

function nonEmptyStringField(
	value: Record<string, unknown> | undefined | null,
	...keys: string[]
): string | null {
	const candidate = stringField(value, ...keys);
	return candidate && candidate.trim().length > 0 ? candidate : null;
}

function numberField(
	value: Record<string, unknown> | undefined | null,
	...keys: string[]
): number | null {
	if (!value) return null;
	for (const key of keys) {
		const candidate = value[key];
		if (typeof candidate === "number" && Number.isFinite(candidate)) {
			return candidate;
		}
	}
	return null;
}

function booleanField(
	value: Record<string, unknown> | undefined | null,
	key: string,
): boolean | null {
	if (!value) return null;
	const candidate = value[key];
	return typeof candidate === "boolean" ? candidate : null;
}
