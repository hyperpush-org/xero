import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import type { RuntimeStreamItemDto } from "@xero/ui/model/runtime-stream";

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
