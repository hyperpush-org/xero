import type {
	ConversationMessageAttachment,
	ConversationTurn,
} from "@xero/ui/components/transcript/conversation-section";
import type {
	RuntimeStreamItemDto,
	RuntimeStreamMediaAttachmentDto,
} from "@xero/ui/model/runtime-stream";

const REMOTE_SESSION_SNAPSHOT_SCHEMA = "xero.remote_session_snapshot.v1";
const REMOTE_RUNTIME_EVENT_SCHEMA = "xero.remote_runtime_event.v1";
const RUNTIME_STREAM_ITEM_KINDS = new Set([
	"transcript",
	"tool",
	"skill",
	"activity",
	"action_required",
	"plan",
	"complete",
	"failure",
	"subagent_lifecycle",
]);
const OWNED_AGENT_REASONING_ACTIVITY_CODE = "owned_agent_reasoning";
const OWNED_AGENT_FILE_CHANGED_ACTIVITY_CODE = "owned_agent_file_changed";
const COMPACT_TOOL_BURST_THRESHOLD = 2;
const CODE_EDIT_TOOL_NAMES = new Set([
	"edit",
	"patch",
	"write",
	"apply_patch",
	"notebook_edit",
]);

type ActionTurn = Extract<ConversationTurn, { kind: "action" }>;
type ToolState = ActionTurn["state"];

interface TurnProjectionContext {
	turns: ConversationTurn[];
	actionTurnIndexByToolCallId: Map<string, number>;
}

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
	const context = createTurnProjectionContext();
	for (const item of items) {
		const turn = mapItem(item);
		if (turn) appendProjectedTurn(context, turn);
	}
	return compactActionBursts(context.turns);
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
		case "activity":
			return mapActivity(item);
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
		const eventProjectionEnabled = isLiveAgentRunStatus(
			stringField(run, "status"),
		);
		if (!eventProjectionEnabled) {
			const terminalTurns = projectTerminalRemoteRunToTurns(
				run,
				runId,
				nextSequence,
			);
			turns.push(...terminalTurns);
			const failure = remoteRunFailureToTurn(
				run,
				runId,
				nextSequence + terminalTurns.length,
			);
			if (failure && !terminalTurns.some((turn) => turn.kind === "failure")) {
				turns.push(failure);
			}
			nextSequence =
				Math.max(nextSequence, ...turns.map((turn) => turn.sequence)) + 1;
			continue;
		}

		const eventTurns = projectRemoteRunEventsToTurns(run, runId);
		if (eventTurns.length > 0) {
			const hasUserMessage = eventTurns.some(
				(turn) => turn.kind === "message" && turn.role === "user",
			);
			const hasAssistantMessage = eventTurns.some(
				(turn) => turn.kind === "message" && turn.role === "assistant",
			);
			if (!hasUserMessage) {
				const promptTurn = remoteRunPromptTurn(run, runId, nextSequence);
				if (promptTurn) turns.push(promptTurn);
			}
			turns.push(...eventTurns);
			if (!hasAssistantMessage) {
				for (const message of recordArray(run, "messages")) {
					if (stringField(message, "role") !== "assistant") continue;
					const turn = remoteMessageToTurn(message, runId, nextSequence);
					if (!turn) continue;
					turns.push(turn);
					nextSequence = Math.max(nextSequence, turn.sequence + 1);
				}
			}
			const failure = remoteRunFailureToTurn(run, runId, nextSequence);
			if (failure && !eventTurns.some((turn) => turn.kind === "failure")) {
				turns.push(failure);
			}
			nextSequence =
				Math.max(nextSequence, ...turns.map((turn) => turn.sequence)) + 1;
			continue;
		}

		const messages = recordArray(run, "messages");
		const hasUserMessage = messages.some(
			(message) =>
				stringField(message, "role") === "user" &&
				nonEmptyStringField(message, "content"),
		);
		if (!hasUserMessage) {
			const promptTurn = remoteRunPromptTurn(run, runId, nextSequence);
			if (promptTurn) {
				turns.push(promptTurn);
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

function projectTerminalRemoteRunToTurns(
	run: Record<string, unknown>,
	runId: string,
	initialSequence: number,
): ConversationTurn[] {
	const eventTurns = projectRemoteRunEventsToTurns(run, runId);
	if (eventTurns.length === 0) {
		return projectRemoteRunMessagesToTurns(run, runId, initialSequence);
	}

	const messageTurns = projectRemoteRunMessagesToTurns(
		run,
		runId,
		initialSequence,
	).filter(
		(turn): turn is Extract<ConversationTurn, { kind: "message" }> =>
			turn.kind === "message",
	);
	const messagesByRole = {
		user: messageTurns.filter((turn) => turn.role === "user"),
		assistant: messageTurns.filter((turn) => turn.role === "assistant"),
	};
	const nextMessageIndexByRole = {
		user: 0,
		assistant: 0,
	};
	const usedMessageIds = new Set<string>();

	return eventTurns
		.map((turn) => {
			if (turn.kind !== "message") return turn;
			const replacement =
				messagesByRole[turn.role][nextMessageIndexByRole[turn.role]++];
			if (!replacement) return turn;
			usedMessageIds.add(replacement.id);
			return {
				...turn,
				id: replacement.id,
				text: replacement.text,
			};
		})
		.concat(
			messageTurns
				.filter((turn) => !usedMessageIds.has(turn.id))
				.map((turn, index) => ({
					...turn,
					sequence:
						Math.max(
							initialSequence - 1,
							...eventTurns.map((item) => item.sequence),
						) +
						index +
						1,
				})),
		);
}

function projectRemoteRunMessagesToTurns(
	run: Record<string, unknown>,
	runId: string,
	initialSequence: number,
): ConversationTurn[] {
	const turns: ConversationTurn[] = [];
	let nextSequence = initialSequence;
	const messages = recordArray(run, "messages");
	const hasUserMessage = messages.some(
		(message) =>
			stringField(message, "role") === "user" &&
			nonEmptyStringField(message, "content"),
	);

	if (!hasUserMessage) {
		const promptTurn = remoteRunPromptTurn(run, runId, nextSequence);
		if (promptTurn) {
			turns.push(promptTurn);
			nextSequence += 1;
		}
	}

	for (const message of messages) {
		const turn = remoteMessageToTurn(message, runId, nextSequence);
		if (!turn) continue;
		turns.push(turn);
		nextSequence += 1;
	}

	return turns;
}

function projectRemoteRunEventsToTurns(
	run: Record<string, unknown>,
	runId: string,
): ConversationTurn[] {
	const events = recordArray(run, "events").sort(
		(left, right) =>
			(numberField(left, "id", "eventId") ?? 0) -
			(numberField(right, "id", "eventId") ?? 0),
	);
	const context = createTurnProjectionContext();
	for (const event of events) {
		const eventTurn = remoteAgentRunEventToTurn(
			event,
			runId,
			numberField(event, "id", "eventId") ?? context.turns.length + 1,
		);
		if (eventTurn) appendProjectedTurn(context, eventTurn);
	}
	return compactActionBursts(context.turns);
}

function remoteRunPromptTurn(
	run: Record<string, unknown>,
	runId: string,
	sequence: number,
): Extract<ConversationTurn, { kind: "message" }> | null {
	const prompt = nonEmptyStringField(run, "prompt");
	if (!prompt) return null;
	return {
		id: `transcript:${runId}:prompt`,
		kind: "message",
		role: "user",
		sequence,
		text: prompt,
	};
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
	if (eventKind === "reasoning_summary" && payload) {
		return remoteReasoningSummaryToTurn(payload, runId, sequence, event);
	}
	if (eventKind === "tool_started" && payload) {
		return remoteToolEventToTurn(payload, runId, sequence, event, "running");
	}
	if (eventKind === "tool_completed" && payload) {
		const ok = booleanField(payload, "ok");
		return remoteToolEventToTurn(
			payload,
			runId,
			sequence,
			event,
			ok === false ? "failed" : "succeeded",
		);
	}
	if (eventKind === "command_output" && payload) {
		return remoteCommandOutputToTurn(payload, runId, sequence, event);
	}
	if (
		eventKind === "context_manifest_recorded" ||
		eventKind === "retrieval_performed"
	) {
		return null;
	}
	if (eventKind === "memory_candidate_captured") {
		return remoteContextEventToTurn(payload, runId, sequence, event, eventKind);
	}
	if (eventKind === "file_changed" && payload) {
		return remoteFileChangeToTurn(payload, runId, sequence, event);
	}
	if (
		(eventKind === "action_required" || eventKind === "approval_required") &&
		payload
	) {
		return remoteActionRequiredToTurn(payload, runId, sequence, event);
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

	if (
		eventKind === "message_delta" ||
		eventKind === "reasoning_summary" ||
		eventKind === "tool_started" ||
		eventKind === "tool_completed" ||
		eventKind === "command_output" ||
		eventKind === "context_manifest_recorded" ||
		eventKind === "retrieval_performed" ||
		eventKind === "memory_candidate_captured" ||
		eventKind === "file_changed" ||
		eventKind === "action_required" ||
		eventKind === "approval_required" ||
		eventKind === "run_failed"
	) {
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

function remoteReasoningSummaryToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event?: Record<string, unknown>,
): ConversationTurn | null {
	if (recordField(payload, "usage")) return null;
	const text = nonEmptyStringField(payload, "summary");
	if (!text) return null;
	return {
		id: `thinking:${runId}:${numberField(event, "eventId", "id") ?? sequence}`,
		kind: "thinking",
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
		title: toolTitle(toolName),
		detail: remoteToolDetail(payload, toolName, state),
		detailRows: remoteToolDetailRows(payload),
		mediaAttachments: runtimeMediaAttachmentsToConversation(
			arrayField(payload, "mediaAttachments", "media_attachments") as
				| RuntimeStreamMediaAttachmentDto[]
				| undefined,
		),
		state,
		...(isCodeEditToolName(toolName) ? { defaultOpen: true } : {}),
	};
}

function remoteCommandOutputToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event: Record<string, unknown>,
): ConversationTurn | null {
	const toolCallId = stringField(payload, "toolCallId");
	const toolName = stringField(payload, "toolName") ?? "command";
	if (!toolCallId) return null;
	const partial = booleanField(payload, "partial") === true;
	return {
		id: `tool:${runId}:${toolCallId}:${numberField(event, "eventId", "id") ?? sequence}`,
		kind: "action",
		sequence,
		toolCallId,
		toolName,
		title: toolTitle(toolName),
		detail: remoteCommandOutputDetail(payload),
		detailRows: remoteToolDetailRows(payload),
		state: partial ? "running" : "succeeded",
		...(isCodeEditToolName(toolName) ? { defaultOpen: true } : {}),
	};
}

function remoteContextEventToTurn(
	payload: Record<string, unknown> | null,
	runId: string,
	sequence: number,
	event: Record<string, unknown>,
	eventKind: string,
): ConversationTurn {
	const eventId = numberField(event, "eventId", "id") ?? sequence;
	const detail =
		nonEmptyStringField(payload, "summary", "message") ??
		contextEventFallback(eventKind);
	return {
		id: `tool:${runId}:runtime-project-context:${eventId}:${eventKind}`,
		kind: "action",
		sequence,
		toolCallId: `runtime-project-context:${eventId}:${eventKind}`,
		toolName: "project_context",
		title: contextEventTitle(eventKind),
		detail,
		detailRows: payload ? remoteToolDetailRows(payload) : [],
		state: "succeeded",
	};
}

function remoteFileChangeToTurn(
	payload: Record<string, unknown>,
	runId: string,
	sequence: number,
	event: Record<string, unknown>,
): ConversationTurn {
	const operation = stringField(payload, "operation") ?? "changed";
	const path = stringField(payload, "path") ?? "unknown path";
	const toPath = stringField(payload, "toPath");
	const detail = toPath
		? `${operation}: ${path} -> ${toPath}`
		: `${operation}: ${path}`;
	return {
		id: `file-change:${runId}:${numberField(event, "eventId", "id") ?? sequence}`,
		kind: "file_change",
		runId,
		sequence,
		title: "File changed",
		detail,
		operation,
		path,
		toPath,
		changeGroupId: stringField(payload, "changeGroupId"),
		workspaceEpoch: numberField(payload, "workspaceEpoch"),
		patchAvailability: null,
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

function isLiveAgentRunStatus(status: string | null): boolean {
	return (
		status === "starting" ||
		status === "running" ||
		status === "paused" ||
		status === "cancelling"
	);
}

function mapTranscript(item: RuntimeStreamItemDto): ConversationTurn | null {
	if ((!item.text && !item.mediaAttachments?.length) || !item.transcriptRole) {
		return null;
	}
	const role = item.transcriptRole;
	if (role !== "user" && role !== "assistant") return null;
	return {
		id: `transcript:${item.runId ?? "run"}:${item.sequence}`,
		kind: "message",
		role,
		sequence: item.sequence,
		text: item.text ?? "",
		attachments: runtimeMediaAttachmentsToConversation(item.mediaAttachments),
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
		title: toolTitle(item.toolName),
		detail: item.detail ?? item.text ?? "Tool activity recorded.",
		detailRows: runtimeToolDetailRows(item),
		mediaAttachments: runtimeMediaAttachmentsToConversation(
			item.mediaAttachments,
		),
		state: item.toolState ?? null,
		...(isCodeEditToolName(item.toolName) ? { defaultOpen: true } : {}),
	};
}

function isCodeEditToolName(toolName: string): boolean {
	return CODE_EDIT_TOOL_NAMES.has(toolName);
}

function mapActivity(item: RuntimeStreamItemDto): ConversationTurn | null {
	if (item.code === OWNED_AGENT_REASONING_ACTIVITY_CODE) {
		const text = item.text ?? item.detail;
		if (!text?.trim()) return null;
		return {
			id: `thinking:${item.runId ?? "run"}:${item.sequence}`,
			kind: "thinking",
			sequence: item.sequence,
			text,
		};
	}
	if (item.code === OWNED_AGENT_FILE_CHANGED_ACTIVITY_CODE) {
		const detail = item.detail ?? item.text ?? "File changed.";
		const parsed = parseFileChangeDetail(detail);
		return {
			id: `file-change:${item.runId ?? "run"}:${item.sequence}`,
			kind: "file_change",
			runId: item.runId ?? "run",
			sequence: item.sequence,
			title: item.title ?? "File changed",
			detail,
			operation: parsed.operation,
			path: parsed.path,
			toPath: parsed.toPath,
			changeGroupId: item.codeChangeGroupId ?? null,
			workspaceEpoch: item.codeWorkspaceEpoch ?? null,
			patchAvailability: item.codePatchAvailability ?? null,
		};
	}
	return null;
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

function createTurnProjectionContext(): TurnProjectionContext {
	return {
		turns: [],
		actionTurnIndexByToolCallId: new Map(),
	};
}

export function appendConversationTurn(
	turns: readonly ConversationTurn[],
	turn: ConversationTurn,
): ConversationTurn[] {
	const context = createTurnProjectionContext();
	for (const currentTurn of expandActionGroups(turns)) {
		appendProjectedTurn(context, currentTurn);
	}
	appendProjectedTurn(context, cloneConversationTurn(turn));
	return compactActionBursts(context.turns);
}

function appendProjectedTurn(
	context: TurnProjectionContext,
	turn: ConversationTurn,
): void {
	const previous = context.turns.at(-1);
	if (
		turn.kind === "message" &&
		turn.role === "assistant" &&
		previous?.kind === "message" &&
		previous.role === "assistant"
	) {
		previous.text = `${previous.text}${turn.text}`;
		previous.attachments = mergeConversationAttachments(
			previous.attachments,
			turn.attachments,
		);
		previous.sequence = turn.sequence;
		return;
	}

	if (turn.kind === "thinking" && previous?.kind === "thinking") {
		previous.text = `${previous.text}${turn.text}`;
		previous.sequence = turn.sequence;
		return;
	}

	if (turn.kind === "action") {
		const existingIndex = context.actionTurnIndexByToolCallId.get(
			turn.toolCallId,
		);
		const existing =
			existingIndex != null ? context.turns[existingIndex] : undefined;
		if (existing?.kind === "action") {
			mergeActionTurn(existing, turn);
			return;
		}
		context.actionTurnIndexByToolCallId.set(
			turn.toolCallId,
			context.turns.length,
		);
	}

	context.turns.push(turn);
}

function expandActionGroups(
	turns: readonly ConversationTurn[],
): ConversationTurn[] {
	const expandedTurns: ConversationTurn[] = [];
	for (const turn of turns) {
		if (turn.kind !== "action_group") {
			expandedTurns.push(cloneConversationTurn(turn));
			continue;
		}
		expandedTurns.push(
			...turn.actions.map((action) => ({
				id: action.id,
				kind: "action" as const,
				sequence: action.sequence,
				toolCallId: action.toolCallId,
				toolName: action.toolName,
				title: action.title,
				detail: action.detail,
				detailRows: cloneActionRows(action.detailRows),
				mediaAttachments: action.mediaAttachments?.map((attachment) => ({
					...attachment,
				})),
				state: action.state,
				...(action.defaultOpen ? { defaultOpen: true } : {}),
			})),
		);
	}
	return expandedTurns;
}

function cloneConversationTurn(turn: ConversationTurn): ConversationTurn {
	if (turn.kind === "message") {
		return {
			...turn,
			attachments: turn.attachments?.map((attachment) => ({ ...attachment })),
		};
	}
	if (turn.kind === "thinking") {
		return { ...turn };
	}
	if (turn.kind === "action") {
		return {
			...turn,
			detailRows: cloneActionRows(turn.detailRows),
			mediaAttachments: turn.mediaAttachments?.map((attachment) => ({
				...attachment,
			})),
		};
	}
	if (turn.kind === "action_group") {
		return {
			...turn,
			actions: turn.actions.map((action) => ({
				...action,
				detailRows: cloneActionRows(action.detailRows),
				mediaAttachments: action.mediaAttachments?.map((attachment) => ({
					...attachment,
				})),
			})),
		};
	}
	if (turn.kind === "subagent_group") {
		return {
			...turn,
			children: turn.children.map(cloneConversationTurn),
		};
	}
	return { ...turn };
}

function cloneActionRows(
	rows: ActionTurn["detailRows"],
): ActionTurn["detailRows"] {
	return rows.map((row) => ({ ...row }));
}

function mergeActionTurn(existing: ActionTurn, incoming: ActionTurn): void {
	existing.title = incoming.title || existing.title;
	existing.detail = incoming.detail || existing.detail;
	existing.detailRows = mergeActionRows(
		existing.detailRows,
		incoming.detailRows,
	);
	existing.mediaAttachments = mergeConversationAttachments(
		existing.mediaAttachments,
		incoming.mediaAttachments,
	);
	existing.state = incoming.state ?? existing.state;
	existing.defaultOpen = incoming.defaultOpen ?? existing.defaultOpen;
}

function mergeActionRows(
	existing: ActionTurn["detailRows"],
	incoming: ActionTurn["detailRows"],
): ActionTurn["detailRows"] {
	const merged = existing.map((row) => ({ ...row }));
	const seen = new Set(merged.map((row) => `${row.label}\u0000${row.value}`));
	for (const row of incoming) {
		const key = `${row.label}\u0000${row.value}`;
		if (seen.has(key)) continue;
		seen.add(key);
		merged.push(row);
	}
	return merged;
}

function mergeConversationAttachments(
	existing: ConversationMessageAttachment[] | undefined,
	incoming: ConversationMessageAttachment[] | undefined,
): ConversationMessageAttachment[] | undefined {
	if (!existing?.length) return incoming;
	if (!incoming?.length) return existing;
	const merged = existing.map((attachment) => ({ ...attachment }));
	const seen = new Set(merged.map((attachment) => attachment.id));
	for (const attachment of incoming) {
		if (seen.has(attachment.id)) continue;
		seen.add(attachment.id);
		merged.push({ ...attachment });
	}
	return merged;
}

export function compactActionBursts(
	turns: ConversationTurn[],
): ConversationTurn[] {
	const compactedTurns: ConversationTurn[] = [];
	let actionBuffer: ActionTurn[] = [];

	const flushActionBuffer = () => {
		if (actionBuffer.length >= COMPACT_TOOL_BURST_THRESHOLD) {
			compactedTurns.push(actionGroupTurnFromActions(actionBuffer));
		} else {
			compactedTurns.push(...actionBuffer);
		}
		actionBuffer = [];
	};

	for (const turn of turns) {
		if (turn.kind === "action") {
			if (isCodeEditAction(turn) || !isTerminalActionState(turn.state)) {
				flushActionBuffer();
				compactedTurns.push(turn);
				continue;
			}
			actionBuffer.push(turn);
			continue;
		}
		flushActionBuffer();
		compactedTurns.push(turn);
	}

	flushActionBuffer();
	return compactedTurns;
}

function actionGroupTurnFromActions(actions: ActionTurn[]): ConversationTurn {
	const firstAction = actions[0];
	const lastAction = actions.at(-1) ?? firstAction;
	return {
		id: `tool-group:${firstAction.id}:${lastAction.id}`,
		kind: "action_group",
		sequence: lastAction.sequence,
		title: `${actions.length} tool calls`,
		detail: summarizeActionGroup(actions),
		state: actionGroupState(actions),
		actions: actions.map((action) => ({
			id: action.id,
			sequence: action.sequence,
			toolCallId: action.toolCallId,
			toolName: action.toolName,
			title: action.title,
			detail: action.detail,
			detailRows: cloneActionRows(action.detailRows),
			mediaAttachments: action.mediaAttachments?.map((attachment) => ({
				...attachment,
			})),
			state: action.state ?? null,
			...(action.defaultOpen ? { defaultOpen: true } : {}),
		})),
	};
}

function isCodeEditAction(action: ActionTurn): boolean {
	return isCodeEditToolName(action.toolName);
}

function actionGroupState(actions: ActionTurn[]): ToolState | null {
	if (actions.some((action) => action.state === "failed")) return "failed";
	if (actions.some((action) => action.state === "running")) return "running";
	if (actions.some((action) => action.state === "pending")) return "pending";
	if (actions.some((action) => action.state === "succeeded")) {
		return "succeeded";
	}
	return null;
}

function summarizeActionGroup(actions: ActionTurn[]): string {
	const stateCounts = new Map<ToolState, number>();
	for (const action of actions) {
		if (action.state) {
			stateCounts.set(action.state, (stateCounts.get(action.state) ?? 0) + 1);
		}
	}

	const stateSummary = (["failed", "running", "pending", "succeeded"] as const)
		.map((state) => {
			const count = stateCounts.get(state) ?? 0;
			return count > 0
				? `${count} ${getToolStateLabel(state).toLowerCase()}`
				: null;
		})
		.filter((part): part is string => Boolean(part))
		.join(" · ");
	const latestAction = actions.at(-1);
	return [
		stateSummary || `${actions.length} recorded`,
		latestAction ? `latest ${latestAction.title}` : null,
	]
		.filter((part): part is string => Boolean(part))
		.join(" · ");
}

function isTerminalActionState(state: ToolState | null | undefined): boolean {
	return state === "succeeded" || state === "failed";
}

function getToolStateLabel(state: NonNullable<ToolState>): string {
	switch (state) {
		case "failed":
			return "Failed";
		case "running":
			return "Running";
		case "pending":
			return "Pending";
		case "succeeded":
			return "Succeeded";
	}
}

function toolTitle(toolName: string): string {
	return toolName.replace(/[_-]+/g, " ");
}

function remoteToolDetail(
	payload: Record<string, unknown>,
	toolName: string,
	state: NonNullable<ToolState>,
): string {
	return (
		nonEmptyStringField(payload, "summary", "message", "outcome", "detail") ??
		(state === "running"
			? `Started \`${toolName}\`.`
			: `Completed \`${toolName}\`.`)
	);
}

function remoteToolDetailRows(
	payload: Record<string, unknown>,
): ActionTurn["detailRows"] {
	const rows: ActionTurn["detailRows"] = [];
	const input = payloadValuePreview(payload, "input");
	if (input) rows.push({ label: "Input", value: input });
	const hasMedia = Boolean(
		arrayField(payload, "mediaAttachments", "media_attachments")?.length,
	);
	if (hasMedia) return rows;
	const output =
		payloadValuePreview(payload, "output") ??
		payloadValuePreview(payload, "result") ??
		payloadValuePreview(payload, "stdout") ??
		payloadValuePreview(payload, "stderr");
	if (output) rows.push({ label: "Output", value: output });
	return rows;
}

function runtimeToolDetailRows(
	item: RuntimeStreamItemDto,
): ActionTurn["detailRows"] {
	const rows: ActionTurn["detailRows"] = [];
	if (item.detail) {
		rows.push({
			label: item.toolState === "running" ? "Input" : "Outcome",
			value: item.detail,
		});
	}
	if (!item.mediaAttachments?.length && item.toolResultPreview) {
		rows.push({ label: "Output", value: item.toolResultPreview });
	}
	return rows;
}

function runtimeMediaAttachmentsToConversation(
	attachments: readonly RuntimeStreamMediaAttachmentDto[] | null | undefined,
): ConversationMessageAttachment[] | undefined {
	if (!attachments?.length) return undefined;
	return attachments.map((attachment) => {
		const originalName =
			attachment.title?.trim() ||
			(attachment.source.kind === "app_data_path"
				? attachment.source.absolutePath.split(/[\\/]/).pop()
				: null) ||
			attachment.id;
		const absolutePath =
			attachment.source.kind === "app_data_path"
				? attachment.source.absolutePath
				: attachment.source.kind === "artifact"
					? (attachment.source.absolutePath ?? undefined)
					: undefined;
		return {
			id: attachment.id,
			kind: attachment.kind,
			mediaType: attachment.mediaType,
			originalName,
			sizeBytes: attachment.sizeBytes ?? 0,
			title: attachment.title ?? null,
			alt: attachment.alt ?? null,
			width: attachment.width ?? null,
			height: attachment.height ?? null,
			source: attachment.source,
			renderUrl: attachment.renderUrl ?? null,
			previewSrc:
				attachment.renderUrl ??
				(attachment.source.kind === "data_url"
					? attachment.source.dataUrl
					: undefined),
			absolutePath,
		};
	});
}

function remoteCommandOutputDetail(payload: Record<string, unknown>): string {
	const stream = stringField(payload, "stream") ?? "output";
	if (booleanField(payload, "partial")) return `Command ${stream} streamed.`;
	const argv = arrayStringField(payload, "argv")?.join(" ") || "command";
	const exitCode = numberField(payload, "exitCode");
	if (typeof exitCode === "number") {
		return `Command exited with status ${exitCode}: ${argv}.`;
	}
	return `Command output: ${argv}.`;
}

function payloadValuePreview(
	value: Record<string, unknown> | undefined | null,
	key: string,
): string | null {
	if (!value) return null;
	const candidate = value[key];
	if (typeof candidate === "string") {
		return candidate.trim().length > 0 ? truncateText(candidate) : null;
	}
	if (candidate == null) return null;
	try {
		return truncateText(JSON.stringify(candidate, null, 2));
	} catch {
		return null;
	}
}

function truncateText(value: string, maxChars = 24_000): string {
	return value.length <= maxChars
		? value
		: `${value.slice(0, maxChars - 3)}...`;
}

function contextEventFallback(eventKind: string): string {
	switch (eventKind) {
		case "context_manifest_recorded":
			return "Context manifest recorded.";
		case "retrieval_performed":
			return "Durable context retrieval performed.";
		case "memory_candidate_captured":
			return "Memory candidate captured.";
		default:
			return "Project context updated.";
	}
}

function contextEventTitle(eventKind: string): string {
	switch (eventKind) {
		case "context_manifest_recorded":
			return "project context manifest";
		case "retrieval_performed":
			return "project context retrieval";
		case "memory_candidate_captured":
			return "project context memory";
		default:
			return "project context";
	}
}

function parseFileChangeDetail(detail: string): {
	operation: string;
	path: string;
	toPath: string | null;
} {
	const summary = detail.split(" · ")[0]?.trim() ?? "";
	const match = /^([^:]+):\s*(.+)$/.exec(summary);
	if (!match) {
		return {
			operation: "changed",
			path: summary || "unknown path",
			toPath: null,
		};
	}
	const operation = match[1]?.trim() || "changed";
	const pathSegment = match[2]?.trim() || "unknown path";
	const renameSeparator = " -> ";
	const renameIndex = pathSegment.indexOf(renameSeparator);
	if (renameIndex >= 0) {
		return {
			operation,
			path: pathSegment.slice(0, renameIndex).trim() || "unknown path",
			toPath:
				pathSegment.slice(renameIndex + renameSeparator.length).trim() || null,
		};
	}
	return {
		operation,
		path: pathSegment,
		toPath: null,
	};
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

function arrayStringField(
	value: Record<string, unknown> | undefined | null,
	key: string,
): string[] | null {
	if (!value) return null;
	const candidate = value[key];
	if (!Array.isArray(candidate)) return null;
	const strings = candidate.filter(
		(item): item is string => typeof item === "string",
	);
	return strings.length > 0 ? strings : null;
}

function arrayField(
	value: Record<string, unknown> | undefined | null,
	...keys: string[]
): unknown[] | null {
	if (!value) return null;
	for (const key of keys) {
		const candidate = value[key];
		if (Array.isArray(candidate)) return candidate;
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
