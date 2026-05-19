import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	type WheelEvent,
} from "react";

const CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX = 96;

export function isCloudConversationNearBottom(
	viewport: Pick<HTMLElement, "scrollTop" | "scrollHeight" | "clientHeight">,
	thresholdPx = CONVERSATION_NEAR_BOTTOM_THRESHOLD_PX,
): boolean {
	if (viewport.scrollHeight <= viewport.clientHeight) {
		return true;
	}

	return (
		viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight <=
		thresholdPx
	);
}

export function getCloudConversationScrollKey(
	turns: readonly ConversationTurn[],
	isLive: boolean,
): string {
	const latestTurn = turns.at(-1);
	return [
		turns.length,
		latestTurn?.id ?? "none",
		latestTurn?.sequence ?? "none",
		latestTurn?.kind ?? "none",
		latestTurn ? getTurnContentVersion(latestTurn) : "none",
		isLive ? "live" : "idle",
	].join(":");
}

interface ConversationAutoFollowOptions {
	enabled: boolean;
	isLive: boolean;
	sessionKey: string;
	turns: readonly ConversationTurn[];
}

interface ConversationAutoFollowController {
	contentRef: RefObject<HTMLDivElement | null>;
	followLatest: () => void;
	onScroll: () => void;
	onWheel: (event: WheelEvent<HTMLDivElement>) => void;
	viewportRef: RefObject<HTMLDivElement | null>;
}

export function useConversationAutoFollow({
	enabled,
	isLive,
	sessionKey,
	turns,
}: ConversationAutoFollowOptions): ConversationAutoFollowController {
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const shouldAutoFollowRef = useRef(true);
	const scrollFrameRef = useRef<number | null>(null);
	const sessionKeyRef = useRef<string | null>(null);
	const observedScrollKeyRef = useRef<string | null>(null);
	const scrollKey = getCloudConversationScrollKey(turns, isLive);

	const scrollToBottom = useCallback((options: { defer?: boolean } = {}) => {
		const run = () => {
			const viewport = viewportRef.current;
			if (!viewport) return;
			viewport.scrollTop = viewport.scrollHeight;
		};

		if (
			!options.defer ||
			typeof window === "undefined" ||
			typeof window.requestAnimationFrame !== "function"
		) {
			run();
			return;
		}

		if (
			scrollFrameRef.current !== null &&
			typeof window.cancelAnimationFrame === "function"
		) {
			window.cancelAnimationFrame(scrollFrameRef.current);
		}
		scrollFrameRef.current = window.requestAnimationFrame(() => {
			scrollFrameRef.current = null;
			run();
		});
	}, []);

	useEffect(() => {
		return () => {
			if (
				scrollFrameRef.current !== null &&
				typeof window !== "undefined" &&
				typeof window.cancelAnimationFrame === "function"
			) {
				window.cancelAnimationFrame(scrollFrameRef.current);
				scrollFrameRef.current = null;
			}
		};
	}, []);

	useEffect(() => {
		if (sessionKeyRef.current !== sessionKey) {
			sessionKeyRef.current = sessionKey;
			observedScrollKeyRef.current = null;
			shouldAutoFollowRef.current = true;
		}
		if (!enabled) return;
		const scrollKeyChanged = observedScrollKeyRef.current !== scrollKey;
		observedScrollKeyRef.current = scrollKey;
		if (!scrollKeyChanged && (turns.length > 0 || isLive)) return;
		if (turns.length === 0 && !isLive) {
			shouldAutoFollowRef.current = true;
			return;
		}
		if (shouldAutoFollowRef.current) {
			scrollToBottom({ defer: true });
		}
	}, [enabled, isLive, scrollKey, scrollToBottom, sessionKey, turns.length]);

	useEffect(() => {
		if (
			!enabled ||
			typeof ResizeObserver === "undefined" ||
			!contentRef.current
		) {
			return;
		}

		const observer = new ResizeObserver(() => {
			if (shouldAutoFollowRef.current) {
				scrollToBottom({ defer: true });
			}
		});
		observer.observe(contentRef.current);
		return () => observer.disconnect();
	}, [enabled, scrollToBottom]);

	const onScroll = useCallback(() => {
		const viewport = viewportRef.current;
		if (!viewport) return;
		shouldAutoFollowRef.current = isCloudConversationNearBottom(viewport);
	}, []);

	const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
		const viewport = viewportRef.current;
		if (
			event.deltaY < 0 &&
			viewport &&
			viewport.scrollHeight > viewport.clientHeight
		) {
			shouldAutoFollowRef.current = false;
		}
	}, []);

	const followLatest = useCallback(() => {
		shouldAutoFollowRef.current = true;
		scrollToBottom({ defer: true });
	}, [scrollToBottom]);

	return {
		contentRef,
		followLatest,
		onScroll,
		onWheel,
		viewportRef,
	};
}

function getTurnContentVersion(turn: ConversationTurn): string {
	switch (turn.kind) {
		case "message":
			return `${turn.role}:${turn.text.length}:${turn.attachments?.length ?? 0}`;
		case "thinking":
			return String(turn.text.length);
		case "action":
			return `${turn.state ?? "unknown"}:${turn.detail.length}:${turn.detailRows.length}`;
		case "action_group": {
			const latestAction = turn.actions.at(-1);
			return [
				turn.state ?? "unknown",
				turn.detail.length,
				turn.actions.length,
				latestAction?.state ?? "unknown",
				latestAction?.detail.length ?? 0,
			].join(":");
		}
		case "file_change":
			return `${turn.operation}:${turn.path}:${turn.detail.length}`;
		case "failure":
			return `${turn.code}:${turn.message.length}`;
		case "action_prompt":
			return `${turn.isResolved}:${turn.pendingDecision ?? "open"}:${turn.detail.length}`;
		case "handoff_notice":
			return `${turn.sourceRunId}:${turn.targetRunId}`;
		case "routing_suggestion":
			return `${turn.isResolved}:${turn.acceptedTarget ?? "none"}:${turn.summary.length}`;
		case "subagent_group":
			return `${turn.status}:${turn.children.length}:${turn.resultSummary?.length ?? 0}`;
	}
}
