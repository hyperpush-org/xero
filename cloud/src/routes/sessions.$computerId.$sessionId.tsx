import { createFileRoute } from "@tanstack/react-router";
import {
	Composer,
	type ComposerDictationLike,
	type ComposerSelectGroup,
	type ComposerSelectOption,
	WebComposerContextIndicator,
	type WebComposerContextIndicatorStatus,
} from "@xero/ui/components/composer";
import {
	ComputerUseSidebar,
	ComputerUseSidebarContent,
	type ComputerUseSidebarDensity,
	ComputerUseSidebarHeader,
} from "@xero/ui/components/computer-use-sidebar";
import { EmptySessionState } from "@xero/ui/components/empty-session-state";
import {
	type ConversationMessageAttachment,
	ConversationSection,
	type ConversationTurn,
} from "@xero/ui/components/transcript/conversation-section";
import { Badge } from "@xero/ui/components/ui/badge";
import { Button } from "@xero/ui/components/ui/button";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogTitle,
	DialogTrigger,
} from "@xero/ui/components/ui/dialog";
import {
	getRuntimeAgentDescriptor,
	getRuntimeRunApprovalModeDescription,
	getRuntimeRunApprovalModeLabel,
	type RuntimeAgentIdDto,
	type RuntimeRunApprovalModeDto,
	runtimeAgentIdSchema,
} from "@xero/ui/model/runtime";
import {
	Eraser,
	GripVertical,
	Keyboard,
	Monitor,
	MousePointer2,
	SendHorizontal,
	Square,
	X,
} from "lucide-react";
import type { Channel } from "phoenix";
import {
	type ClipboardEvent,
	type CompositionEvent,
	type CSSProperties,
	type FocusEvent,
	type FormEvent,
	type KeyboardEvent,
	type PointerEvent,
	type ReactNode,
	useCallback,
	useEffect,
	useId,
	useMemo,
	useRef,
	useState,
	type WheelEvent,
} from "react";
import { createPortal } from "react-dom";
import { BrandLogo } from "#/components/brand-logo";
import { LoadingScreen } from "#/components/loading-screen";
import { decodeRelayFrame } from "#/lib/relay/envelope";
import {
	answerComputerUseStreamOffer,
	type CommandAckResult,
	heartbeatComputerUseManualControl,
	type InboundCommand,
	pushInboundCommand,
	releaseComputerUseManualControl,
	requestComputerUseManualControl,
	requestComputerUseStream,
	requestComputerUseStreamKeyframe,
	requestComputerUseStreamStatus,
	requestContextSnapshot,
	requestRuntimeMediaArtifact,
	requestSessionSnapshot,
	sendComputerUseManualInput,
	sendComputerUseStreamIceCandidate,
	setComputerUseStreamQuality,
	stopComputerUseStream,
} from "#/lib/relay/relay-client";
import {
	type SessionModelOption,
	type SessionThinkingEffort,
	type SessionTranscript,
	sessionKey,
	useSessionStore,
} from "#/lib/relay/session-store";
import { useSessionsShell } from "#/lib/relay/sessions-shell-context";
import { useConversationAutoFollow } from "#/lib/relay/use-conversation-auto-follow";
import { useRemoteAttachments } from "#/lib/relay/use-remote-attachments";
import { useSessionStream } from "#/lib/relay/use-session-stream";
import { cn } from "#/lib/utils";

export const Route = createFileRoute("/sessions/$computerId/$sessionId")({
	component: SessionView,
});

interface ControlUpdateOverrides {
	agentId?: string | null;
	modelId?: string | null;
	thinkingEffort?: SessionThinkingEffort | null;
	approvalMode?: RuntimeRunApprovalModeDto | null;
	autoCompactEnabled?: boolean | null;
}

const COMPUTER_USE_EMPTY_TRANSCRIPT: SessionTranscript = {
	turns: [],
	lastSeq: 0,
	isLive: false,
	availableAgents: [{ id: "computer_use", label: "Computer Use" }],
	availableModels: [],
	currentAgentId: "computer_use",
	currentModelId: null,
	currentThinkingEffort: null,
	currentApprovalMode: "suggest",
	currentAutoCompactEnabled: true,
};
const COMPUTER_USE_TOP_BAR_ACTION_CLASS =
	"h-7 gap-2 bg-transparent px-1.5 text-[12px] font-medium text-muted-foreground shadow-none hover:bg-transparent hover:text-foreground active:bg-transparent dark:hover:bg-transparent disabled:opacity-35";
const COMPUTER_USE_TOP_BAR_ACTION_ICON_CLASS = "h-3.5 w-3.5";

function SessionView() {
	const shell = useSessionsShell();
	const {
		session,
		visibleSessions,
		clearComputerUseChat,
		currentComputerOnline,
		reportActiveTargetInvalid,
	} = shell;
	const { computerId, sessionId } = Route.useParams();
	const key = sessionKey(computerId, sessionId);

	const storedTranscript = useSessionStore((state) => state.transcripts[key]);
	const currentSessionAvailable = visibleSessions.some(
		(s) => s.computerId === computerId && s.sessionId === sessionId,
	);
	const visibleSession =
		visibleSessions.find(
			(s) => s.computerId === computerId && s.sessionId === sessionId,
		) ?? null;
	const isComputerUseSession = Boolean(visibleSession?.isComputerUse);
	const transcript =
		storedTranscript ??
		(isComputerUseSession ? COMPUTER_USE_EMPTY_TRANSCRIPT : null);
	const turns = transcript?.turns ?? [];
	const availableAgents = transcript?.availableAgents ?? [];
	const availableModels = transcript?.availableModels ?? [];
	const currentAgentId = transcript?.currentAgentId ?? null;
	const currentModelId = transcript?.currentModelId ?? null;
	const contextSnapshot = transcript?.contextSnapshot ?? null;
	const contextSnapshotError = transcript?.contextSnapshotError ?? null;
	const isLive = transcript?.isLive ?? false;
	const {
		channel,
		iceServers,
		joinRejected,
		remoteControl,
		streamRunId,
		streamToken,
	} = useSessionStream({
		computerId,
		enabled: currentComputerOnline && currentSessionAvailable,
		sessionId,
		relayToken: session.relayToken,
	});
	const hostRemoteControl = shell.currentComputerRemoteControl ?? remoteControl;
	const hostConnectionBlocked = hostRemoteControl?.available === false;
	const hostConnectionBlockedTitle = isComputerUseSession
		? "Computer Use is already in use"
		: "Xero Cloud is already connected elsewhere";
	const hostConnectionBlockedMessage =
		hostRemoteControl?.message ??
		"Stop the running connection in the other cloud app before using it here.";
	const resolvedTurns = useResolvedRemoteMedia({
		channel,
		computerId,
		deviceId: session.deviceId,
		sessionId,
		turns,
	});
	const desktopPreviewUrl = useMemo(
		() => latestDesktopPreviewUrl(resolvedTurns),
		[resolvedTurns],
	);

	const [draftPrompt, setDraftPrompt] = useState("");
	const [selectedControls, setSelectedControls] = useState<{
		key: string;
		agentId: string | null;
		modelId: string | null;
		thinkingEffort: SessionThinkingEffort | null;
		approvalMode: RuntimeRunApprovalModeDto | null;
		autoCompactEnabled: boolean | null;
	}>({
		key,
		agentId: null,
		modelId: null,
		thinkingEffort: null,
		approvalMode: null,
		autoCompactEnabled: null,
	});
	const selectedAgentId =
		selectedControls.key === key ? selectedControls.agentId : null;
	const selectedModelId =
		selectedControls.key === key ? selectedControls.modelId : null;
	const selectedThinkingEffort =
		selectedControls.key === key ? selectedControls.thinkingEffort : null;
	const selectedApprovalMode =
		selectedControls.key === key ? selectedControls.approvalMode : null;
	const selectedAutoCompactEnabled =
		selectedControls.key === key ? selectedControls.autoCompactEnabled : null;
	const currentApprovalMode = transcript?.currentApprovalMode ?? "suggest";
	const currentAutoCompactEnabled =
		transcript?.currentAutoCompactEnabled ?? true;
	const autoCompactEnabled =
		selectedAutoCompactEnabled ?? currentAutoCompactEnabled;

	const selectableAgents = useMemo(() => {
		if (isComputerUseSession) {
			return [
				availableAgents.find((agent) => agent.id === "computer_use") ?? {
					id: "computer_use",
					label: "Computer Use",
				},
			];
		}
		return availableAgents.filter((agent) => agent.id !== "computer_use");
	}, [availableAgents, isComputerUseSession]);
	const resolvedAgentId = isComputerUseSession
		? "computer_use"
		: (selectedAgentId ?? currentAgentId ?? selectableAgents[0]?.id ?? null);
	const resolvedRuntimeAgentId = runtimeAgentIdFromString(resolvedAgentId);
	const resolvedApprovalMode = resolveApprovalModeForAgent(
		resolvedRuntimeAgentId,
		selectedApprovalMode ?? currentApprovalMode,
	);
	const resolvedModelId =
		selectedModelId ?? currentModelId ?? availableModels[0]?.id ?? null;
	const resolvedModelOption =
		availableModels.find((option) => option.id === resolvedModelId) ?? null;
	const resolvedProviderId = resolvedModelOption?.providerId ?? null;
	const resolvedRawModelId = resolvedModelOption?.modelId ?? resolvedModelId;
	const currentThinkingEffort = transcript?.currentThinkingEffort ?? null;
	const currentControlsVersion = [
		currentAgentId ?? "",
		currentModelId ?? "",
		currentThinkingEffort ?? "",
		currentApprovalMode,
		currentAutoCompactEnabled ? "1" : "0",
	].join("\u0000");
	const baseThinkingOptionsForModel =
		resolvedModelOption?.thinkingEffortOptions ?? [];
	const resolvedThinkingEffort =
		selectedThinkingEffort ??
		currentThinkingEffort ??
		resolvedModelOption?.defaultThinkingEffort ??
		(baseThinkingOptionsForModel.length > 0
			? baseThinkingOptionsForModel[0]
			: null);
	const thinkingOptionsForModel = useMemo(() => {
		if (!resolvedThinkingEffort) return baseThinkingOptionsForModel;
		if (baseThinkingOptionsForModel.includes(resolvedThinkingEffort)) {
			return baseThinkingOptionsForModel;
		}
		return [resolvedThinkingEffort, ...baseThinkingOptionsForModel];
	}, [baseThinkingOptionsForModel, resolvedThinkingEffort]);
	const thinkingComposerOptions = useMemo<ComposerSelectOption[]>(() => {
		if (!resolvedModelOption?.thinkingSupported) return [];
		return thinkingOptionsForModel.map((effort) => ({
			id: effort,
			label: formatThinkingEffortLabel(effort),
		}));
	}, [resolvedModelOption?.thinkingSupported, thinkingOptionsForModel]);
	const approvalComposerOptions = useMemo<ComposerSelectOption[]>(() => {
		const descriptor = getRuntimeAgentDescriptor(resolvedRuntimeAgentId);
		return descriptor.allowedApprovalModes.map((mode) => ({
			id: mode,
			label: getRuntimeRunApprovalModeLabel(mode),
			sublabel: getRuntimeRunApprovalModeDescription(mode),
		}));
	}, [resolvedRuntimeAgentId]);
	const modelGroups = useMemo<ComposerSelectGroup[]>(
		() => buildComposerModelGroups(availableModels),
		[availableModels],
	);
	const hasUserMessage = turns.some(
		(turn) => turn.kind === "message" && turn.role === "user",
	);
	const conversationContextKey = `${turns.length}:${
		turns.at(-1)?.sequence ?? "empty"
	}`;
	const debouncedDraftPrompt = useDebouncedValue(draftPrompt, 350);
	const contextRequestKey = [
		key,
		conversationContextKey,
		resolvedProviderId ?? "",
		resolvedRawModelId ?? "",
		debouncedDraftPrompt,
	].join("\u0000");
	const [pendingContextRequestKey, setPendingContextRequestKey] = useState<
		string | null
	>(null);
	const contextRequestSettled =
		transcript?.contextSnapshotRequestId === contextRequestKey &&
		(Boolean(contextSnapshot) || Boolean(contextSnapshotError));
	const contextMeterStatus: WebComposerContextIndicatorStatus =
		pendingContextRequestKey === contextRequestKey
			? contextSnapshot
				? "stale"
				: "loading"
			: contextSnapshotError
				? "error"
				: contextSnapshot
					? "ready"
					: "idle";
	const contextMeter =
		isComputerUseSession || contextMeterStatus === "idle" ? null : (
			<WebComposerContextIndicator
				status={contextMeterStatus}
				snapshot={contextSnapshot}
				hasUserMessage={hasUserMessage}
				error={contextSnapshotError}
			/>
		);
	const {
		contentRef: conversationContentRef,
		followLatest: followLatestConversation,
		onScroll: handleConversationScroll,
		onWheel: handleConversationWheel,
		viewportRef: conversationViewportRef,
	} = useConversationAutoFollow({
		enabled: Boolean(transcript),
		isLive,
		sessionKey: key,
		turns,
	});

	useEffect(() => {
		if (!joinRejected) return;
		reportActiveTargetInvalid(key);
	}, [joinRejected, key, reportActiveTargetInvalid]);

	useEffect(() => {
		setSelectedControls((current) => {
			if (currentControlsVersion.length === 0) return current;
			if (current.key !== key) return current;
			if (
				current.agentId === null &&
				current.modelId === null &&
				current.thinkingEffort === null &&
				current.approvalMode === null &&
				current.autoCompactEnabled === null
			) {
				return current;
			}
			return {
				key,
				agentId: null,
				modelId: null,
				thinkingEffort: null,
				approvalMode: null,
				autoCompactEnabled: null,
			};
		});
	}, [currentControlsVersion, key]);

	useEffect(() => {
		if (!channel || !session.deviceId || !currentSessionAvailable) return;
		if (hostConnectionBlocked) return;
		if (storedTranscript) return;
		const requestSnapshot = () => {
			if (useSessionStore.getState().transcripts[key]) return;
			requestSessionSnapshot(channel, {
				computerId,
				sessionId,
				deviceId: session.deviceId,
			});
		};
		requestSnapshot();
		const retryHandle = setInterval(requestSnapshot, 2_000);
		return () => clearInterval(retryHandle);
	}, [
		channel,
		computerId,
		currentSessionAvailable,
		hostConnectionBlocked,
		key,
		session.deviceId,
		sessionId,
		storedTranscript,
	]);

	useEffect(() => {
		if (!channel || !session.deviceId || !transcript || isLive) return;
		if (hostConnectionBlocked) return;
		if (isComputerUseSession) return;
		if (contextRequestSettled) {
			setPendingContextRequestKey((current) =>
				current === contextRequestKey ? null : current,
			);
			return;
		}
		if (pendingContextRequestKey === contextRequestKey) return;
		setPendingContextRequestKey(contextRequestKey);
		requestContextSnapshot(channel, {
			computerId,
			sessionId,
			deviceId: session.deviceId,
			requestId: contextRequestKey,
			providerId: resolvedProviderId,
			modelId: resolvedRawModelId,
			pendingPrompt: debouncedDraftPrompt,
		});
	}, [
		channel,
		computerId,
		contextRequestKey,
		contextRequestSettled,
		debouncedDraftPrompt,
		hostConnectionBlocked,
		isLive,
		isComputerUseSession,
		pendingContextRequestKey,
		resolvedProviderId,
		resolvedRawModelId,
		session.deviceId,
		sessionId,
		transcript,
	]);

	const attachmentsHook = useRemoteAttachments({
		channel,
		computerId,
		sessionId,
		deviceId: session.deviceId,
	});

	const pushControlUpdate = useCallback(
		(overrides: ControlUpdateOverrides = {}) => {
			if (hostConnectionBlocked) return;
			if (!channel || !session.deviceId) return;
			const nextAgentId = isComputerUseSession
				? "computer_use"
				: (overrides.agentId ?? resolvedAgentId);
			const nextModelId = overrides.modelId ?? resolvedModelId;
			if (!nextAgentId || !nextModelId) return;
			const nextRuntimeAgentId = runtimeAgentIdFromString(nextAgentId);
			const nextApprovalMode = resolveApprovalModeForAgent(
				nextRuntimeAgentId,
				overrides.approvalMode ?? resolvedApprovalMode,
			);
			const nextModelOption =
				availableModels.find((option) => option.id === nextModelId) ??
				resolvedModelOption;
			const nextThinkingEffort =
				overrides.thinkingEffort === undefined
					? resolvedThinkingEffort
					: overrides.thinkingEffort;
			const payload: Record<string, unknown> = {
				agent: nextAgentId,
				modelId: nextModelOption?.modelId ?? nextModelId,
				approvalMode: nextApprovalMode,
				autoCompactEnabled: overrides.autoCompactEnabled ?? autoCompactEnabled,
			};
			if (nextModelOption?.providerId) {
				payload.providerId = nextModelOption.providerId;
			}
			if (nextModelOption?.providerProfileId) {
				payload.providerProfileId = nextModelOption.providerProfileId;
			}
			if (nextThinkingEffort) {
				payload.thinkingEffort = nextThinkingEffort;
			}
			pushInboundCommand(channel, {
				v: 1,
				seq: Date.now(),
				computer_id: computerId,
				session_id: sessionId,
				device_id: session.deviceId,
				kind: "update_session_controls",
				payload,
			});
		},
		[
			availableModels,
			autoCompactEnabled,
			channel,
			computerId,
			hostConnectionBlocked,
			resolvedAgentId,
			resolvedApprovalMode,
			resolvedModelId,
			resolvedModelOption,
			resolvedThinkingEffort,
			isComputerUseSession,
			session.deviceId,
			sessionId,
		],
	);

	const dispatchSend = useCallback(
		(submittedPrompt?: string) => {
			const message = (submittedPrompt ?? draftPrompt).trim();
			if (hostConnectionBlocked) return;
			if (!channel || !message || !session.deviceId) return;
			const readyAttachments = attachmentsHook.getReadyAttachments();
			const payload: Record<string, unknown> = {
				message,
			};
			if (readyAttachments.length > 0) {
				payload.attachments = readyAttachments;
			}
			if (resolvedAgentId && resolvedModelId) {
				payload.agent = isComputerUseSession ? "computer_use" : resolvedAgentId;
				payload.modelId = resolvedModelOption?.modelId ?? resolvedModelId;
				if (resolvedModelOption?.providerId) {
					payload.providerId = resolvedModelOption.providerId;
				}
				if (resolvedModelOption?.providerProfileId) {
					payload.providerProfileId = resolvedModelOption.providerProfileId;
				}
				if (resolvedThinkingEffort && resolvedModelOption?.thinkingSupported) {
					payload.thinkingEffort = resolvedThinkingEffort;
				}
				payload.approvalMode = resolvedApprovalMode;
				payload.autoCompactEnabled = autoCompactEnabled;
			}
			const command: InboundCommand = {
				v: 1,
				seq: Date.now(),
				computer_id: computerId,
				session_id: sessionId,
				device_id: session.deviceId,
				kind: "send_message",
				payload,
			};
			pushInboundCommand(channel, command);
			setDraftPrompt("");
			setSelectedControls({
				key,
				agentId: null,
				modelId: null,
				thinkingEffort: null,
				approvalMode: null,
				autoCompactEnabled: null,
			});
			attachmentsHook.clearAttachments();
			followLatestConversation();
		},
		[
			attachmentsHook,
			autoCompactEnabled,
			channel,
			computerId,
			hostConnectionBlocked,
			draftPrompt,
			followLatestConversation,
			key,
			resolvedAgentId,
			resolvedApprovalMode,
			resolvedModelId,
			resolvedModelOption,
			resolvedThinkingEffort,
			isComputerUseSession,
			session.deviceId,
			sessionId,
		],
	);

	const handleAutoCompactEnabledChange = useCallback(
		(next: boolean) => {
			setSelectedControls((current) => ({
				key,
				agentId: current.key === key ? current.agentId : null,
				modelId: current.key === key ? current.modelId : null,
				thinkingEffort: current.key === key ? current.thinkingEffort : null,
				approvalMode: current.key === key ? current.approvalMode : null,
				autoCompactEnabled: next,
			}));
			pushControlUpdate({ autoCompactEnabled: next });
		},
		[key, pushControlUpdate],
	);

	const handleAgentChange = useCallback(
		(agentId: string) => {
			if (isComputerUseSession) return;
			const nextApprovalMode = resolveApprovalModeForAgent(
				runtimeAgentIdFromString(agentId),
				resolvedApprovalMode,
			);
			setSelectedControls((current) => ({
				key,
				agentId,
				modelId: current.key === key ? current.modelId : null,
				thinkingEffort: current.key === key ? current.thinkingEffort : null,
				approvalMode: nextApprovalMode,
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({ agentId, approvalMode: nextApprovalMode });
		},
		[isComputerUseSession, key, pushControlUpdate, resolvedApprovalMode],
	);

	const handleModelChange = useCallback(
		(modelId: string) => {
			const modelOption =
				availableModels.find((option) => option.id === modelId) ?? null;
			const nextThinkingEffort = defaultThinkingEffortForModel(modelOption);
			setSelectedControls((current) => ({
				key,
				agentId: current.key === key ? current.agentId : null,
				modelId,
				thinkingEffort: nextThinkingEffort,
				approvalMode: current.key === key ? current.approvalMode : null,
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({
				modelId,
				thinkingEffort: nextThinkingEffort,
			});
		},
		[availableModels, key, pushControlUpdate],
	);

	const handleThinkingChange = useCallback(
		(value: string) => {
			const thinkingEffort = value as SessionThinkingEffort;
			setSelectedControls((current) => ({
				key,
				agentId: current.key === key ? current.agentId : null,
				modelId: current.key === key ? current.modelId : null,
				thinkingEffort,
				approvalMode: current.key === key ? current.approvalMode : null,
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({ thinkingEffort });
		},
		[key, pushControlUpdate],
	);

	const handleApprovalModeChange = useCallback(
		(value: string) => {
			const approvalMode = resolveApprovalModeForAgent(
				resolvedRuntimeAgentId,
				value as RuntimeRunApprovalModeDto,
			);
			setSelectedControls((current) => ({
				key,
				agentId: current.key === key ? current.agentId : null,
				modelId: current.key === key ? current.modelId : null,
				thinkingEffort: current.key === key ? current.thinkingEffort : null,
				approvalMode,
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({ approvalMode });
		},
		[key, pushControlUpdate, resolvedRuntimeAgentId],
	);

	const hiddenDictation = useMemo<ComposerDictationLike>(
		() => ({
			ariaLabel: "Voice input unavailable",
			isListening: false,
			isToggleDisabled: true,
			phase: "idle",
			tooltip: "Voice input unavailable",
			toggle: async () => undefined,
			isVisible: false,
		}),
		[],
	);
	const projectLabel = shell.activeProjectLabel;
	const [desktopControlsOpen, setDesktopControlsOpen] = useState(false);
	const [clearChatPending, setClearChatPending] = useState(false);
	const [computerUseSidebarDensity, setComputerUseSidebarDensity] =
		useState<ComputerUseSidebarDensity>("comfortable");
	const mobileTextKeyboardActive = useMobileTextKeyboardActive();
	useEffect(() => {
		if (hostConnectionBlocked) setDesktopControlsOpen(false);
	}, [hostConnectionBlocked]);
	useEffect(() => {
		if (!clearChatPending) return;
		if (turns.length === 0 && !isLive) setClearChatPending(false);
	}, [clearChatPending, isLive, turns.length]);
	useEffect(() => {
		if (!clearChatPending) return;
		const timeout = window.setTimeout(() => setClearChatPending(false), 10_000);
		return () => window.clearTimeout(timeout);
	}, [clearChatPending]);
	const handleClearComputerUseChat = useCallback(() => {
		if (!visibleSession || !isComputerUseSession) return;
		const didRequest = clearComputerUseChat(visibleSession);
		if (didRequest) {
			setClearChatPending(true);
			setDraftPrompt("");
			attachmentsHook.clearAttachments();
		}
	}, [
		attachmentsHook,
		clearComputerUseChat,
		isComputerUseSession,
		visibleSession,
	]);
	const canClearComputerUseChat =
		isComputerUseSession &&
		Boolean(visibleSession) &&
		currentComputerOnline &&
		!hostConnectionBlocked &&
		!isLive &&
		!clearChatPending &&
		turns.length > 0;
	const clearComputerUseChatTitle = !currentComputerOnline
		? "Computer Use is offline"
		: hostConnectionBlocked
			? hostConnectionBlockedMessage
			: isLive
				? "Stop the current run before clearing chat"
				: turns.length === 0
					? "Chat is already clear"
					: undefined;
	const renderConversationPane = (
		surface: "main" | "sidebar" = "main",
		density: ComputerUseSidebarDensity = "comfortable",
	) => {
		const isSidebarSurface = surface === "sidebar";
		const isCompactSidebar = isSidebarSurface && density === "compact";
		const shouldCollapseEmptyState =
			mobileTextKeyboardActive && !isSidebarSurface;
		return (
			<div className="relative h-full min-h-0 flex-1 overflow-hidden">
				<div
					ref={conversationViewportRef}
					onScroll={handleConversationScroll}
					onWheel={handleConversationWheel}
					className={cn(
						"absolute inset-0 overflow-y-auto",
						isCompactSidebar ? "px-2 pt-2" : "px-4 pt-4 sm:px-6",
					)}
				>
					<div
						ref={conversationContentRef}
						className={cn(
							"mx-auto flex min-h-full flex-col",
							isCompactSidebar
								? "max-w-full gap-1 pb-20"
								: "max-w-3xl gap-4 pb-24 lg:max-w-[47rem]",
						)}
					>
						{hostConnectionBlocked && !transcript ? (
							shouldCollapseEmptyState ? null : (
								<div className="flex flex-1 items-center justify-center">
									<EmptySessionState
										projectLabel={projectLabel}
										context={isComputerUseSession ? "computer-use" : "default"}
										variant={isCompactSidebar ? "dense" : "default"}
										disabledTitle={hostConnectionBlockedTitle}
										disabledDescription={hostConnectionBlockedMessage}
									/>
								</div>
							)
						) : transcript ? (
							turns.length === 0 ? (
								shouldCollapseEmptyState ? null : (
									<div className="flex flex-1 items-center justify-center">
										<EmptySessionState
											projectLabel={projectLabel}
											context={
												isComputerUseSession ? "computer-use" : "default"
											}
											variant={isCompactSidebar ? "dense" : "default"}
											disabledTitle={
												hostConnectionBlocked
													? hostConnectionBlockedTitle
													: undefined
											}
											disabledDescription={
												hostConnectionBlocked
													? hostConnectionBlockedMessage
													: undefined
											}
											onSelectSuggestion={
												hostConnectionBlocked ? undefined : setDraftPrompt
											}
										/>
									</div>
								)
							) : (
								<ConversationSection
									runtimeRun={null}
									visibleTurns={resolvedTurns}
									streamIssue={null}
									streamFailure={null}
									showActivityIndicator={isLive}
									accountAvatarUrl={session.avatarUrl ?? null}
									accountLogin={session.githubLogin}
									variant={isCompactSidebar ? "dense" : "default"}
								/>
							)
						) : (
							<LoadingScreen className="flex-1" />
						)}
						<div aria-hidden="true" className="h-1 shrink-0" />
					</div>
				</div>
				<div
					aria-hidden="true"
					className={cn(
						"pointer-events-none absolute inset-x-0 top-0 z-10 h-7 bg-gradient-to-b to-transparent",
						isSidebarSurface ? "from-sidebar" : "from-background",
					)}
				/>
				{hostConnectionBlocked ? null : (
					<div
						className={cn(
							"pointer-events-none absolute inset-x-0 bottom-0 px-3 sm:px-6",
							isCompactSidebar
								? "pb-[max(env(safe-area-inset-bottom),0.5rem)]"
								: "pb-[max(env(safe-area-inset-bottom),0.75rem)]",
							isSidebarSurface ? "bg-sidebar" : "bg-background",
						)}
					>
						<div
							className={cn(
								"pointer-events-auto mx-auto",
								isCompactSidebar ? "max-w-none" : "max-w-3xl",
							)}
						>
							<Composer
								draftPrompt={draftPrompt}
								onDraftPromptChange={setDraftPrompt}
								onSubmit={dispatchSend}
								density={isSidebarSurface ? density : undefined}
								autoCompactEnabled={
									isComputerUseSession ? undefined : autoCompactEnabled
								}
								onAutoCompactEnabledChange={
									isComputerUseSession
										? undefined
										: handleAutoCompactEnabledChange
								}
								agentGroups={
									isComputerUseSession
										? []
										: [{ id: "agents", options: selectableAgents }]
								}
								selectedAgentId={resolvedAgentId}
								onAgentChange={handleAgentChange}
								modelGroups={modelGroups}
								selectedModelId={resolvedModelId}
								onModelChange={handleModelChange}
								thinkingOptions={thinkingComposerOptions}
								selectedThinkingId={resolvedThinkingEffort}
								onThinkingChange={handleThinkingChange}
								approvalOptions={
									isComputerUseSession ? undefined : approvalComposerOptions
								}
								selectedApprovalId={resolvedApprovalMode}
								onApprovalChange={
									isComputerUseSession ? undefined : handleApprovalModeChange
								}
								pendingAttachments={attachmentsHook.pendingAttachments}
								attachmentCompatibility={resolvedModelOption}
								onAddFiles={attachmentsHook.addFiles}
								onRemoveAttachment={attachmentsHook.removeAttachment}
								contextMeter={contextMeter}
								dictation={isComputerUseSession ? hiddenDictation : undefined}
							/>
						</div>
					</div>
				)}
			</div>
		);
	};
	const conversationPane = renderConversationPane();
	const computerUseTopBarAccessory =
		isComputerUseSession && shell.topBarAccessoryElement
			? createPortal(
					<>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							aria-label="Clear Computer Use chat"
							className={COMPUTER_USE_TOP_BAR_ACTION_CLASS}
							disabled={!canClearComputerUseChat}
							title={clearComputerUseChatTitle}
							onClick={handleClearComputerUseChat}
						>
							<Eraser
								className={COMPUTER_USE_TOP_BAR_ACTION_ICON_CLASS}
								aria-hidden="true"
							/>
							<span className="hidden sm:inline">
								{clearChatPending ? "Clearing..." : "Clear Chat"}
							</span>
						</Button>
						<span
							aria-hidden="true"
							className="select-none text-[12px] text-muted-foreground/35"
						>
							|
						</span>
						<ComputerUseDesktopDialog
							open={desktopControlsOpen}
							onOpenChange={setDesktopControlsOpen}
							canClearChat={canClearComputerUseChat}
							clearChatPending={clearChatPending}
							clearChatTitle={clearComputerUseChatTitle}
							onClearChat={handleClearComputerUseChat}
							disabled={hostConnectionBlocked}
							disabledReason={hostConnectionBlockedMessage}
							agentSidebar={renderConversationPane(
								"sidebar",
								computerUseSidebarDensity,
							)}
							onAgentSidebarDensityChange={setComputerUseSidebarDensity}
							channel={channel}
							computerId={computerId}
							deviceId={session.deviceId}
							iceServers={iceServers}
							isOnline={currentComputerOnline}
							isAgentWorking={isLive}
							onPromptSubmit={dispatchSend}
							previewUrl={desktopPreviewUrl}
							sessionId={sessionId}
							streamRunId={streamRunId}
							streamToken={streamToken}
						/>
					</>,
					shell.topBarAccessoryElement,
				)
			: null;

	return (
		<>
			{computerUseTopBarAccessory}
			{desktopControlsOpen && isComputerUseSession ? null : conversationPane}
		</>
	);
}

const MOBILE_TEXT_KEYBOARD_MAX_WIDTH = 768;
const MOBILE_TEXT_KEYBOARD_MIN_HEIGHT_LOSS = 96;

function useMobileTextKeyboardActive() {
	const [active, setActive] = useState(false);
	const baselineHeightRef = useRef(0);

	useEffect(() => {
		if (typeof window === "undefined") return;

		const update = () => {
			const nextActive = readMobileTextKeyboardActive(baselineHeightRef);
			setActive((current) => (current === nextActive ? current : nextActive));
		};
		const resetBaseline = () => {
			baselineHeightRef.current = 0;
			update();
		};
		const visualViewport = window.visualViewport;

		update();
		window.addEventListener("resize", update);
		window.addEventListener("orientationchange", resetBaseline);
		visualViewport?.addEventListener("resize", update);
		visualViewport?.addEventListener("scroll", update);
		document.addEventListener("focusin", update);
		document.addEventListener("focusout", update);

		return () => {
			window.removeEventListener("resize", update);
			window.removeEventListener("orientationchange", resetBaseline);
			visualViewport?.removeEventListener("resize", update);
			visualViewport?.removeEventListener("scroll", update);
			document.removeEventListener("focusin", update);
			document.removeEventListener("focusout", update);
		};
	}, []);

	return active;
}

function readMobileTextKeyboardActive(baselineHeightRef: { current: number }) {
	const visualViewport = window.visualViewport;
	const visualHeight = Math.round(visualViewport?.height ?? window.innerHeight);
	const layoutHeight = Math.round(window.innerHeight || visualHeight);
	const visualWidth = Math.round(visualViewport?.width ?? window.innerWidth);
	const viewportWidth = Math.min(window.innerWidth || visualWidth, visualWidth);
	const textEntryFocused = isTextEntryElement(document.activeElement);
	const viewportHeight = Math.max(visualHeight, layoutHeight);

	if (
		baselineHeightRef.current === 0 ||
		!textEntryFocused ||
		viewportHeight > baselineHeightRef.current
	) {
		baselineHeightRef.current = viewportHeight;
	}

	const coarseMobileViewport =
		typeof window.matchMedia === "function" &&
		window.matchMedia(
			`(pointer: coarse) and (max-width: ${
				MOBILE_TEXT_KEYBOARD_MAX_WIDTH + 132
			}px)`,
		).matches;

	return isMobileTextKeyboardOpenForSnapshot({
		baselineHeight: baselineHeightRef.current,
		coarseMobileViewport,
		layoutHeight,
		textEntryFocused,
		viewportWidth,
		visualHeight,
	});
}

export function isMobileTextKeyboardOpenForSnapshot({
	baselineHeight,
	coarseMobileViewport = false,
	layoutHeight,
	textEntryFocused,
	viewportWidth,
	visualHeight,
}: {
	baselineHeight: number;
	coarseMobileViewport?: boolean;
	layoutHeight: number;
	textEntryFocused: boolean;
	viewportWidth: number;
	visualHeight: number;
}) {
	const heightLoss = Math.max(
		layoutHeight - visualHeight,
		baselineHeight - visualHeight,
	);
	const keyboardThreshold = Math.max(
		MOBILE_TEXT_KEYBOARD_MIN_HEIGHT_LOSS,
		Math.min(180, baselineHeight * 0.15),
	);

	return (
		textEntryFocused &&
		heightLoss >= keyboardThreshold &&
		(viewportWidth <= MOBILE_TEXT_KEYBOARD_MAX_WIDTH || coarseMobileViewport)
	);
}

function isTextEntryElement(element: Element | null) {
	if (!(element instanceof HTMLElement)) return false;
	if (element.isContentEditable) return true;
	if (element instanceof HTMLTextAreaElement) return true;
	if (!(element instanceof HTMLInputElement)) return false;

	return ![
		"button",
		"checkbox",
		"color",
		"file",
		"hidden",
		"image",
		"radio",
		"range",
		"reset",
		"submit",
	].includes(element.type);
}

type DesktopViewportState =
	| "waiting"
	| "connecting"
	| "live"
	| "degraded"
	| "paused"
	| "manual"
	| "blocked"
	| "failed"
	| "offline";

type ManualControlState =
	| "manual_idle"
	| "manual_requesting"
	| "manual_active"
	| "manual_reconnecting"
	| "manual_denied"
	| "manual_releasing"
	| "manual_released";

type DesktopStreamQuality = "low" | "balanced" | "high";
type DesktopControlPresentationOverride = "auto" | "desktop" | "mobile";

const DESKTOP_STREAM_DATA_CHANNEL_LABEL = "xero-desktop-stream";
const DESKTOP_STREAM_MAX_BUFFERED_FRAMES = 24;
const DESKTOP_CONTROL_BAR_MARGIN = 12;
const DESKTOP_CONTROL_BAR_DEFAULT_TOP = 24;
const DESKTOP_PROMPT_BAR_DEFAULT_BOTTOM = 16;
const DESKTOP_KEYFRAME_REFRESH_MS = 10_000;
const DESKTOP_STREAM_CONNECTING_RETRY_MS = 7_000;
const DESKTOP_STREAM_STALE_FRAME_MS = 8_000;
const DESKTOP_STREAM_STATUS_INTERVAL_MS = 5_000;
const DESKTOP_STREAM_FALLBACK_RECOVERY_COOLDOWN_MS = 2_500;
const DESKTOP_STREAM_PENDING_ICE_CANDIDATES_MAX = 128;
const DESKTOP_ADAPTIVE_QUALITY_DOWNGRADE_COOLDOWN_MS = 6_000;
const DESKTOP_ADAPTIVE_QUALITY_UPGRADE_COOLDOWN_MS = 30_000;
const DESKTOP_ADAPTIVE_QUALITY_STABLE_SAMPLES = 3;
const DESKTOP_ADAPTIVE_QUALITY_POOR_LOSS_RATIO = 0.03;
const DESKTOP_ADAPTIVE_QUALITY_GOOD_LOSS_RATIO = 0.005;
const DESKTOP_CONTROL_MOBILE_BREAKPOINT = 768;
const DESKTOP_CONTROL_COARSE_POINTER_MAX_WIDTH = 900;
const DESKTOP_CONTROL_PRESENTATION_QUERY_PARAM = "desktopControlPresentation";
const DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY =
	"xero.cloud.desktopControlPresentation";
const STOPPABLE_DESKTOP_STATES = new Set<DesktopViewportState>([
	"connecting",
	"live",
	"degraded",
	"manual",
]);
const MANUAL_CONTROL_DESKTOP_STATES = new Set<DesktopViewportState>([
	"live",
	"degraded",
	"manual",
]);
const KEYFRAME_REFRESH_STATES = new Set<DesktopViewportState>([
	"connecting",
	"live",
	"degraded",
	"manual",
]);
const STREAM_STATUS_STATES = new Set<DesktopViewportState>([
	"connecting",
	"live",
	"degraded",
	"manual",
]);
function createManualControlId(deviceId: string, sessionId: string): string {
	const nonce = Math.random().toString(36).slice(2, 10);
	const issuedAt = Date.now().toString(36);
	return `manual_${shortClientToken(deviceId)}_${shortClientToken(sessionId)}_${issuedAt}_${nonce}`;
}

function shortClientToken(value: string): string {
	let hash = 0;
	for (let index = 0; index < value.length; index += 1) {
		hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
	}
	return hash.toString(36);
}

const DESKTOP_STREAM_QUALITY_ORDER: DesktopStreamQuality[] = [
	"low",
	"balanced",
	"high",
];
const DESKTOP_STREAM_QUALITY_MIN_AVAILABLE_BITRATE_BPS: Record<
	DesktopStreamQuality,
	number
> = {
	low: 0,
	balanced: 2_200_000,
	high: 5_500_000,
};
const DESKTOP_STREAM_QUALITY_UPGRADE_AVAILABLE_BITRATE_BPS: Record<
	DesktopStreamQuality,
	number
> = {
	low: 3_000_000,
	balanced: 8_500_000,
	high: Number.POSITIVE_INFINITY,
};
const DESKTOP_TOOLBAR_BUTTON_CLASS =
	"h-7 gap-1 rounded-md border border-transparent bg-transparent px-2 text-[12px] font-medium text-muted-foreground shadow-none hover:border-border/70 hover:bg-muted/70 hover:text-foreground disabled:opacity-35";
const DESKTOP_TOOLBAR_DANGER_BUTTON_CLASS =
	"h-7 gap-1 rounded-md border border-red-500/30 bg-red-500/10 px-2 text-[12px] font-medium text-red-200 shadow-none hover:border-red-400/50 hover:bg-red-500/15 hover:text-red-100 disabled:opacity-35";
const DESKTOP_CLICK_RIPPLE_MS = 560;
const MANUAL_KEYBOARD_TEXT_BATCH_MS = 18;
const MANUAL_KEYBOARD_FALLBACK_MS = 24;
const MANUAL_KEYBOARD_TEXT_CHUNK_CHARS = 512;
const MANUAL_KEYBOARD_TEXT_MAX_CHARS = 8_000;
const MANUAL_KEYBOARD_COMPOSITION_DUPLICATE_MS = 80;
const MANUAL_POINTER_MOVE_INTERVAL_MS = 50;
const DESKTOP_MOBILE_ZOOM_MIN = 1;
const DESKTOP_MOBILE_ZOOM_MAX = 4;
const DESKTOP_POINTER_TAP_SLOP_PX = 8;
const REMOTE_CONTROL_ALREADY_ACTIVE_REASON =
	"computer_use_connection_already_active";

type ManualKeyboardCaptureState = "inactive" | "armed" | "composing";

interface DesktopControlBarPosition {
	x: number;
	y: number;
}

interface DesktopSurfaceSize {
	height: number;
	width: number;
}

interface DesktopInputPoint {
	x: number;
	y: number;
	sourceWidth: number;
	sourceHeight: number;
}

interface DesktopMediaContentRect {
	height: number;
	left: number;
	top: number;
	width: number;
}

interface DesktopClickRipple {
	button: "primary" | "secondary";
	id: number;
	x: number;
	y: number;
}

interface DesktopManualPointerClick {
	button: number;
	clientX: number;
	clientY: number;
	clicks: number;
}

interface DesktopManualPointerGesture extends DesktopManualPointerClick {
	dragStarted: boolean;
	dragging: boolean;
	lastDragMoveAt: number;
	lastDragMovePoint: DesktopInputPoint | null;
	latestPoint: DesktopInputPoint;
	pointerId: number;
	startClientX: number;
	startClientY: number;
	startPoint: DesktopInputPoint;
}

type DesktopManualInput = Parameters<
	typeof sendComputerUseManualInput
>[1]["input"];

function desktopMediaContentRect(
	rect: DOMRect,
	sourceWidth: number,
	sourceHeight: number,
	rotated: boolean,
): DesktopMediaContentRect | null {
	if (
		rect.width <= 0 ||
		rect.height <= 0 ||
		sourceWidth <= 0 ||
		sourceHeight <= 0
	) {
		return null;
	}

	const sourceAspect = rotated
		? sourceHeight / sourceWidth
		: sourceWidth / sourceHeight;
	const rectAspect = rect.width / rect.height;
	let width = rect.width;
	let height = rect.height;
	if (rectAspect > sourceAspect) {
		height = rect.height;
		width = height * sourceAspect;
	} else {
		width = rect.width;
		height = width / sourceAspect;
	}

	return {
		height,
		left: rect.left + (rect.width - width) / 2,
		top: rect.top + (rect.height - height) / 2,
		width,
	};
}

const MANUAL_KEYBOARD_SPECIAL_KEYS = new Map<string, string>([
	["Enter", "Enter"],
	["Return", "Enter"],
	["Tab", "Tab"],
	["Backspace", "Backspace"],
	["Delete", "Delete"],
	["Del", "Delete"],
	["Escape", "Escape"],
	["Esc", "Escape"],
	["ArrowUp", "ArrowUp"],
	["ArrowDown", "ArrowDown"],
	["ArrowLeft", "ArrowLeft"],
	["ArrowRight", "ArrowRight"],
	["Home", "Home"],
	["End", "End"],
	["PageUp", "PageUp"],
	["PageDown", "PageDown"],
]);

const MANUAL_KEYBOARD_MODIFIER_KEYS = new Map<string, string>([
	["Alt", "option"],
	["Control", "control"],
	["Meta", "command"],
	["OS", "command"],
	["Shift", "shift"],
]);

function manualKeyboardModifiers(event: KeyboardEvent<HTMLElement>): string[] {
	return [
		event.metaKey ? "command" : null,
		event.ctrlKey ? "control" : null,
		event.altKey ? "option" : null,
		event.shiftKey ? "shift" : null,
	].filter((modifier): modifier is string => Boolean(modifier));
}

function normalizeManualModifierKey(key: string): string | null {
	return MANUAL_KEYBOARD_MODIFIER_KEYS.get(key) ?? null;
}

function normalizeManualFunctionKey(key: string): string | null {
	const match = /^F(\d{1,2})$/i.exec(key);
	if (!match) return null;
	const index = Number.parseInt(match[1] ?? "", 10);
	if (!Number.isInteger(index) || index < 1 || index > 12) return null;
	return `F${index}`;
}

function normalizeManualKeyPress(key: string): string | null {
	return (
		normalizeManualModifierKey(key) ??
		MANUAL_KEYBOARD_SPECIAL_KEYS.get(key) ??
		normalizeManualFunctionKey(key)
	);
}

function normalizeManualShortcutTarget(key: string): string | null {
	if (key === " ") return "space";
	if (key.length === 1) return key.toLowerCase();
	return normalizeManualKeyPress(key);
}

function manualKeyboardTextChunk(text: string): string {
	return Array.from(text).slice(0, MANUAL_KEYBOARD_TEXT_MAX_CHARS).join("");
}

function manualKeyboardTextChunks(text: string): string[] {
	const characters = Array.from(text);
	const chunks: string[] = [];
	for (
		let index = 0;
		index < characters.length;
		index += MANUAL_KEYBOARD_TEXT_CHUNK_CHARS
	) {
		chunks.push(
			characters
				.slice(index, index + MANUAL_KEYBOARD_TEXT_CHUNK_CHARS)
				.join(""),
		);
	}
	return chunks;
}

function isManualPrintableKey(key: string): boolean {
	return key.length === 1 && key !== "\u0000";
}

function beforeInputText(event: FormEvent<HTMLTextAreaElement>): {
	inputType: string;
	isComposing: boolean;
	text: string;
} {
	const nativeEvent = event.nativeEvent as InputEvent;
	return {
		inputType: nativeEvent.inputType ?? "",
		isComposing: Boolean(nativeEvent.isComposing),
		text: typeof nativeEvent.data === "string" ? nativeEvent.data : "",
	};
}

interface DesktopMobileViewportTransform {
	scale: number;
	x: number;
	y: number;
}

interface DesktopMobileTouchPointer {
	clientX: number;
	clientY: number;
	moved: boolean;
	pointerId: number;
	startClientX: number;
	startClientY: number;
	startTransform: DesktopMobileViewportTransform;
}

interface DesktopMobilePinchGesture {
	pointerIds: [number, number];
	startDistance: number;
	startMidpoint: { x: number; y: number };
	startTransform: DesktopMobileViewportTransform;
}

const DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM: DesktopMobileViewportTransform =
	{
		scale: 1,
		x: 0,
		y: 0,
	};

function clampNumber(value: number, min: number, max: number): number {
	return Math.min(max, Math.max(min, value));
}

function desktopMobileViewportTransformsEqual(
	a: DesktopMobileViewportTransform,
	b: DesktopMobileViewportTransform,
): boolean {
	return (
		Math.abs(a.scale - b.scale) < 0.001 &&
		Math.abs(a.x - b.x) < 0.1 &&
		Math.abs(a.y - b.y) < 0.1
	);
}

function clampDesktopMobileViewportTransform(
	transform: DesktopMobileViewportTransform,
	rect: DOMRect | null,
): DesktopMobileViewportTransform {
	const scale = clampNumber(
		transform.scale,
		DESKTOP_MOBILE_ZOOM_MIN,
		DESKTOP_MOBILE_ZOOM_MAX,
	);
	if (scale <= DESKTOP_MOBILE_ZOOM_MIN + 0.001) {
		return DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM;
	}
	if (!rect || rect.width <= 0 || rect.height <= 0) {
		return {
			scale,
			x: transform.x,
			y: transform.y,
		};
	}
	const maxX = (rect.width * (scale - 1)) / 2;
	const maxY = (rect.height * (scale - 1)) / 2;
	return {
		scale,
		x: clampNumber(transform.x, -maxX, maxX),
		y: clampNumber(transform.y, -maxY, maxY),
	};
}

function desktopMobileTouchDistance(
	a: DesktopMobileTouchPointer,
	b: DesktopMobileTouchPointer,
): number {
	return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}

function desktopMobileTouchMidpoint(
	a: DesktopMobileTouchPointer,
	b: DesktopMobileTouchPointer,
): { x: number; y: number } {
	return {
		x: (a.clientX + b.clientX) / 2,
		y: (a.clientY + b.clientY) / 2,
	};
}

function desktopMobilePinchTransform(
	rect: DOMRect,
	gesture: DesktopMobilePinchGesture,
	currentMidpoint: { x: number; y: number },
	nextScale: number,
): DesktopMobileViewportTransform {
	const centerX = rect.left + rect.width / 2;
	const centerY = rect.top + rect.height / 2;
	const anchorX =
		centerX +
		(gesture.startMidpoint.x - centerX - gesture.startTransform.x) /
			gesture.startTransform.scale;
	const anchorY =
		centerY +
		(gesture.startMidpoint.y - centerY - gesture.startTransform.y) /
			gesture.startTransform.scale;
	return {
		scale: nextScale,
		x: currentMidpoint.x - centerX - (anchorX - centerX) * nextScale,
		y: currentMidpoint.y - centerY - (anchorY - centerY) * nextScale,
	};
}

interface DesktopControlBarDrag {
	pointerId: number;
	startClientX: number;
	startClientY: number;
	originX: number;
	originY: number;
}

interface DesktopControlPresentation {
	isMobile: boolean;
	override: DesktopControlPresentationOverride;
	rotateDesktop: boolean;
}

interface DesktopStreamDetails {
	status?: string | null;
	transport?: string | null;
	quality?: DesktopStreamQuality | null;
	maxWidth?: number | null;
	maxFrameRate?: number | null;
	metrics?: DesktopStreamMetrics | null;
	message?: string | null;
}

interface DesktopStreamMetrics {
	captureBackend?: string | null;
	encoderBackend?: string | null;
	encoderHardware?: boolean | null;
	preferredCodec?: string | null;
	fallbackReason?: string | null;
	captureFrameRate?: number | null;
	captureDroppedFrames?: number | null;
	encodeFrameRate?: number | null;
	encodeLatencyMs?: number | null;
	outboundBitrateBps?: number | null;
	availableOutgoingBitrateBps?: number | null;
	packetsSent?: number | null;
	bytesSent?: number | null;
	packetLoss?: number | null;
	roundTripTimeMs?: number | null;
	retransmits?: number | null;
	keyframes?: number | null;
}

interface DesktopAdaptiveStreamQualityInput {
	currentQuality: DesktopStreamQuality;
	lastChangedAt: number;
	metrics: DesktopStreamMetrics | null;
	now: number;
	previousMetrics: DesktopStreamMetrics | null;
	stableSamples: number;
	state: DesktopViewportState;
}

interface DesktopAdaptiveStreamQualityDecision {
	quality: DesktopStreamQuality;
	stableSamples: number;
}

interface ComputerUseDesktopViewportProps {
	channel: Channel | null;
	computerId: string;
	deviceId?: string | null;
	iceServers: RTCIceServer[];
	isAgentWorking: boolean;
	isOnline: boolean;
	onPromptSubmit: (message: string) => void;
	previewUrl: string | null;
	sessionId: string;
	streamRunId: string | null;
	streamToken: string | null;
}

interface ComputerUseDesktopControlsProps
	extends ComputerUseDesktopViewportProps {
	agentSidebar: ReactNode;
	canClearChat: boolean;
	clearChatPending: boolean;
	clearChatTitle?: string;
	disabled?: boolean;
	disabledReason?: string;
	onAgentSidebarDensityChange: (density: ComputerUseSidebarDensity) => void;
	onClearChat: () => void;
	onOpenChange: (open: boolean) => void;
	open: boolean;
}

function ComputerUseDesktopDialog({
	agentSidebar,
	canClearChat,
	clearChatPending,
	clearChatTitle,
	disabled = false,
	disabledReason,
	onAgentSidebarDensityChange,
	onClearChat,
	onOpenChange,
	open,
	...props
}: ComputerUseDesktopControlsProps) {
	const presentation = useDesktopControlPresentation();
	const trigger = (
		<Button
			type="button"
			variant="ghost"
			size="sm"
			aria-label="Open desktop controls"
			className={COMPUTER_USE_TOP_BAR_ACTION_CLASS}
			disabled={disabled}
			title={disabled ? disabledReason : undefined}
			onClick={
				disabled
					? undefined
					: presentation.isMobile
						? () => requestNativeDesktopOrientationLock()
						: () => onOpenChange(true)
			}
		>
			<Monitor
				className={COMPUTER_USE_TOP_BAR_ACTION_ICON_CLASS}
				aria-hidden="true"
			/>
			<span className="hidden sm:inline">Desktop</span>
		</Button>
	);

	useEffect(() => {
		if (!presentation.isMobile || !open) return;
		requestNativeDesktopOrientationLock();
		return () => unlockNativeDesktopOrientation();
	}, [open, presentation.isMobile]);

	if (!presentation.isMobile) {
		return (
			<>
				{trigger}
				<ComputerUseDesktopFullscreen
					open={open}
					onOpenChange={onOpenChange}
					agentSidebar={agentSidebar}
					canClearChat={canClearChat}
					clearChatPending={clearChatPending}
					clearChatTitle={clearChatTitle}
					onAgentSidebarDensityChange={onAgentSidebarDensityChange}
					onClearChat={onClearChat}
					presentation={presentation}
					viewportProps={props}
				/>
			</>
		);
	}

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogTrigger asChild>{trigger}</DialogTrigger>
			<DialogContent
				showCloseButton={false}
				className="grid h-[100dvh] max-h-none w-screen max-w-none grid-rows-[minmax(0,1fr)] gap-0 overflow-hidden rounded-none border-0 bg-background p-0 text-foreground shadow-none sm:max-w-none"
			>
				<ComputerUseDesktopViewport
					{...props}
					closeControl={
						<DialogClose asChild>
							<DesktopControlCloseButton />
						</DialogClose>
					}
					presentation={presentation}
				/>
			</DialogContent>
		</Dialog>
	);
}

function ComputerUseDesktopFullscreen({
	agentSidebar,
	canClearChat,
	clearChatPending,
	clearChatTitle,
	onAgentSidebarDensityChange,
	onClearChat,
	onOpenChange,
	open,
	presentation,
	viewportProps,
}: {
	agentSidebar: ReactNode;
	canClearChat: boolean;
	clearChatPending: boolean;
	clearChatTitle?: string;
	onAgentSidebarDensityChange: (density: ComputerUseSidebarDensity) => void;
	onClearChat: () => void;
	onOpenChange: (open: boolean) => void;
	open: boolean;
	presentation: DesktopControlPresentation;
	viewportProps: ComputerUseDesktopViewportProps;
}) {
	useEffect(() => {
		if (!open) return;
		const handleKeyDown = (event: globalThis.KeyboardEvent) => {
			if (event.key !== "Escape") return;
			event.preventDefault();
			onOpenChange(false);
		};
		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [onOpenChange, open]);

	if (!open || typeof document === "undefined") return null;

	return createPortal(
		<section
			aria-label="Desktop controls"
			className="fixed inset-0 z-[90] flex bg-background text-foreground"
		>
			<main className="min-w-0 flex-1 bg-zinc-950">
				<ComputerUseDesktopViewport
					{...viewportProps}
					closeControl={
						<DesktopControlCloseButton onClick={() => onOpenChange(false)} />
					}
					presentation={presentation}
				/>
			</main>
			<ComputerUseSidebar
				onDensityChange={onAgentSidebarDensityChange}
				resizable
				widthStorageKey="xero.cloud.computerUseSidebar.width.v1"
			>
				<ComputerUseSidebarHeader
					clearDisabled={!canClearChat}
					clearPending={clearChatPending}
					clearTitle={clearChatTitle}
					onClear={onClearChat}
					closeLabel="Close Computer Use"
					onClose={() => onOpenChange(false)}
				/>
				<ComputerUseSidebarContent>{agentSidebar}</ComputerUseSidebarContent>
			</ComputerUseSidebar>
		</section>,
		document.body,
	);
}

function DesktopControlCloseButton({ onClick }: { onClick?: () => void }) {
	return (
		<Button
			type="button"
			size="icon"
			variant="ghost"
			aria-label="Close desktop controls"
			onClick={onClick}
			className="h-8 w-8 rounded-md text-muted-foreground hover:border-border/70 hover:bg-muted/70 hover:text-foreground"
		>
			<X className="h-3.5 w-3.5" aria-hidden="true" />
		</Button>
	);
}

function useDesktopControlPresentation(): DesktopControlPresentation {
	const [presentation, setPresentation] = useState<DesktopControlPresentation>(
		() => readDesktopControlPresentation(),
	);

	useEffect(() => {
		const updatePresentation = () => {
			setPresentation(readDesktopControlPresentation());
		};
		const mediaQueries =
			typeof window !== "undefined" && typeof window.matchMedia === "function"
				? [
						window.matchMedia(
							`(max-width: ${DESKTOP_CONTROL_MOBILE_BREAKPOINT - 1}px)`,
						),
						window.matchMedia("(hover: none) and (pointer: coarse)"),
					]
				: [];
		updatePresentation();
		window.addEventListener("resize", updatePresentation);
		window.addEventListener("orientationchange", updatePresentation);
		window.addEventListener("popstate", updatePresentation);
		window.addEventListener("storage", updatePresentation);
		for (const mediaQuery of mediaQueries) {
			mediaQuery.addEventListener("change", updatePresentation);
		}
		return () => {
			window.removeEventListener("resize", updatePresentation);
			window.removeEventListener("orientationchange", updatePresentation);
			window.removeEventListener("popstate", updatePresentation);
			window.removeEventListener("storage", updatePresentation);
			for (const mediaQuery of mediaQueries) {
				mediaQuery.removeEventListener("change", updatePresentation);
			}
		};
	}, []);

	return presentation;
}

export function readDesktopControlPresentation(): DesktopControlPresentation {
	if (typeof window === "undefined") {
		return { isMobile: false, override: "auto", rotateDesktop: false };
	}
	const override = readDesktopControlPresentationOverride();
	const width =
		window.innerWidth || document.documentElement.clientWidth || 1024;
	const coarsePointer =
		typeof window.matchMedia === "function" &&
		window.matchMedia("(hover: none) and (pointer: coarse)").matches;
	const mobileViewport =
		width < DESKTOP_CONTROL_MOBILE_BREAKPOINT ||
		(coarsePointer && width < DESKTOP_CONTROL_COARSE_POINTER_MAX_WIDTH);
	const isMobile =
		override === "mobile" || (override === "auto" && mobileViewport);
	return {
		isMobile,
		override,
		rotateDesktop: false,
	};
}

function requestNativeDesktopOrientationLock(): void {
	if (typeof window === "undefined") return;
	const orientation = window.screen?.orientation;
	if (!orientation || typeof orientation.lock !== "function") return;
	void orientation.lock("landscape").catch(() => undefined);
}

function unlockNativeDesktopOrientation(): void {
	if (typeof window === "undefined") return;
	const orientation = window.screen?.orientation;
	if (!orientation || typeof orientation.unlock !== "function") return;
	orientation.unlock();
}

function readDesktopControlPresentationOverride(): DesktopControlPresentationOverride {
	const queryOverride = parseDesktopControlPresentationOverride(
		new URLSearchParams(window.location.search).get(
			DESKTOP_CONTROL_PRESENTATION_QUERY_PARAM,
		),
	);
	if (queryOverride) return queryOverride;
	try {
		return (
			parseDesktopControlPresentationOverride(
				window.localStorage.getItem(DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY),
			) ?? "auto"
		);
	} catch {
		return "auto";
	}
}

function parseDesktopControlPresentationOverride(
	value: string | null,
): DesktopControlPresentationOverride | null {
	if (value === "auto" || value === "desktop" || value === "mobile") {
		return value;
	}
	return null;
}

export function chooseDesktopAdaptiveStreamQuality({
	currentQuality,
	lastChangedAt,
	metrics,
	now,
	previousMetrics,
	stableSamples,
	state,
}: DesktopAdaptiveStreamQualityInput): DesktopAdaptiveStreamQualityDecision {
	if (
		desktopStreamNetworkIsPoor(currentQuality, metrics, previousMetrics, state)
	) {
		const quality = lowerDesktopStreamQuality(currentQuality);
		if (
			quality !== currentQuality &&
			now - lastChangedAt >= DESKTOP_ADAPTIVE_QUALITY_DOWNGRADE_COOLDOWN_MS
		) {
			return { quality, stableSamples: 0 };
		}
		return { quality: currentQuality, stableSamples: 0 };
	}

	const nextStableSamples = desktopStreamNetworkIsStrong(
		currentQuality,
		metrics,
		previousMetrics,
		state,
	)
		? stableSamples + 1
		: 0;
	const quality = higherDesktopStreamQuality(currentQuality);
	if (
		quality !== currentQuality &&
		nextStableSamples >= DESKTOP_ADAPTIVE_QUALITY_STABLE_SAMPLES &&
		now - lastChangedAt >= DESKTOP_ADAPTIVE_QUALITY_UPGRADE_COOLDOWN_MS
	) {
		return { quality, stableSamples: 0 };
	}
	return { quality: currentQuality, stableSamples: nextStableSamples };
}

function desktopStreamNetworkIsPoor(
	quality: DesktopStreamQuality,
	metrics: DesktopStreamMetrics | null,
	previousMetrics: DesktopStreamMetrics | null,
	state: DesktopViewportState,
): boolean {
	if (state === "degraded") return true;
	if (!metrics) return false;
	const lossRatio = desktopStreamPacketLossRatio(metrics, previousMetrics);
	const availableBitrate = metrics.availableOutgoingBitrateBps;
	const minimumBitrate =
		DESKTOP_STREAM_QUALITY_MIN_AVAILABLE_BITRATE_BPS[quality];
	return (
		(metrics.roundTripTimeMs ?? 0) >= 350 ||
		(lossRatio !== null &&
			lossRatio >= DESKTOP_ADAPTIVE_QUALITY_POOR_LOSS_RATIO) ||
		(typeof availableBitrate === "number" &&
			minimumBitrate > 0 &&
			availableBitrate < minimumBitrate) ||
		(metrics.encodeLatencyMs ?? 0) >= 120
	);
}

function desktopStreamNetworkIsStrong(
	quality: DesktopStreamQuality,
	metrics: DesktopStreamMetrics | null,
	previousMetrics: DesktopStreamMetrics | null,
	state: DesktopViewportState,
): boolean {
	if (!metrics || state === "degraded" || quality === "high") return false;
	const availableBitrate = metrics.availableOutgoingBitrateBps;
	if (typeof availableBitrate !== "number") return false;
	const lossRatio = desktopStreamPacketLossRatio(metrics, previousMetrics);
	return (
		availableBitrate >=
			DESKTOP_STREAM_QUALITY_UPGRADE_AVAILABLE_BITRATE_BPS[quality] &&
		(metrics.roundTripTimeMs ?? 0) <= 120 &&
		(lossRatio === null ||
			lossRatio <= DESKTOP_ADAPTIVE_QUALITY_GOOD_LOSS_RATIO) &&
		(metrics.encodeLatencyMs ?? 0) <= 60
	);
}

function desktopStreamPacketLossRatio(
	metrics: DesktopStreamMetrics,
	previousMetrics: DesktopStreamMetrics | null,
): number | null {
	const packetsSent = metrics.packetsSent;
	const packetsLost = Math.max(0, metrics.packetLoss ?? 0);
	if (
		previousMetrics &&
		typeof packetsSent === "number" &&
		typeof previousMetrics.packetsSent === "number"
	) {
		const sentDelta = packetsSent - previousMetrics.packetsSent;
		const lostDelta = Math.max(
			0,
			packetsLost - Math.max(0, previousMetrics.packetLoss ?? 0),
		);
		const totalDelta = sentDelta + lostDelta;
		if (sentDelta >= 0 && totalDelta > 0) return lostDelta / totalDelta;
	}
	if (typeof packetsSent === "number" && packetsSent > 0) {
		return packetsLost / (packetsSent + packetsLost);
	}
	return null;
}

function lowerDesktopStreamQuality(
	quality: DesktopStreamQuality,
): DesktopStreamQuality {
	const index = DESKTOP_STREAM_QUALITY_ORDER.indexOf(quality);
	return DESKTOP_STREAM_QUALITY_ORDER[Math.max(0, index - 1)] ?? quality;
}

function higherDesktopStreamQuality(
	quality: DesktopStreamQuality,
): DesktopStreamQuality {
	const index = DESKTOP_STREAM_QUALITY_ORDER.indexOf(quality);
	return (
		DESKTOP_STREAM_QUALITY_ORDER[
			Math.min(DESKTOP_STREAM_QUALITY_ORDER.length - 1, index + 1)
		] ?? quality
	);
}

export function ComputerUseDesktopViewport({
	channel,
	closeControl,
	computerId,
	deviceId,
	iceServers,
	isAgentWorking,
	isOnline,
	onPromptSubmit,
	previewUrl,
	sessionId,
	streamRunId,
	streamToken,
	presentation,
}: ComputerUseDesktopViewportProps & {
	closeControl?: ReactNode;
	presentation: DesktopControlPresentation;
}) {
	const [state, setState] = useState<DesktopViewportState>(
		isOnline ? "waiting" : "offline",
	);
	const [manualState, setManualState] =
		useState<ManualControlState>("manual_idle");
	const [streamId, setStreamId] = useState<string | null>(null);
	const [manualControlId, setManualControlId] = useState<string | null>(null);
	const [fallbackPreviewUrl, setFallbackPreviewUrl] = useState<string | null>(
		null,
	);
	const [streamQuality, setStreamQuality] =
		useState<DesktopStreamQuality>("balanced");
	const [hasLiveVideo, setHasLiveVideo] = useState(false);
	const [toolbarPromptPending, setToolbarPromptPending] = useState(false);
	const [streamFailureMessage, setStreamFailureMessage] = useState<
		string | null
	>(null);
	const [clickRipples, setClickRipples] = useState<DesktopClickRipple[]>([]);
	const [keyboardCaptureState, setKeyboardCaptureState] =
		useState<ManualKeyboardCaptureState>("inactive");
	const [mobileViewportTransform, setMobileViewportTransform] =
		useState<DesktopMobileViewportTransform>(
			DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM,
		);
	const videoRef = useRef<HTMLVideoElement | null>(null);
	const imageRef = useRef<HTMLImageElement | null>(null);
	const keyboardSinkRef = useRef<HTMLTextAreaElement | null>(null);
	const peerConnectionRef = useRef<RTCPeerConnection | null>(null);
	const pendingMediaStreamRef = useRef<MediaStream | null>(null);
	const pendingIceCandidatesRef = useRef<RTCIceCandidateInit[]>([]);
	const fallbackPreviewObjectUrlRef = useRef<string | null>(null);
	const dataChannelFramesRef = useRef(
		new Map<string, DesktopFrameChunkBuffer>(),
	);
	const manualControlIdRef = useRef<string | null>(null);
	const keyboardCaptureManualControlIdRef = useRef<string | null>(null);
	const manualPointerGestureRef = useRef<DesktopManualPointerGesture | null>(
		null,
	);
	const releaseManualPointerDragRef = useRef<
		((gesture: DesktopManualPointerGesture) => void) | null
	>(null);
	const manualPointerGestureResetKeyRef = useRef("");
	const manualInputOrderRef = useRef<Promise<void>>(Promise.resolve());
	const lastPointerMoveAtRef = useRef(0);
	const desktopSurfaceRef = useRef<HTMLElement | null>(null);
	const controlBarRef = useRef<HTMLDivElement | null>(null);
	const promptBarRef = useRef<HTMLDivElement | null>(null);
	const controlBarDragRef = useRef<DesktopControlBarDrag | null>(null);
	const promptBarDragRef = useRef<DesktopControlBarDrag | null>(null);
	const clickRippleTimeoutsRef = useRef<Set<number>>(new Set());
	const mobileViewportTransformRef = useRef<DesktopMobileViewportTransform>(
		DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM,
	);
	const mobileTouchPointersRef = useRef(
		new Map<number, DesktopMobileTouchPointer>(),
	);
	const mobileTapPointerIdRef = useRef<number | null>(null);
	const mobilePinchGestureRef = useRef<DesktopMobilePinchGesture | null>(null);
	const textBatchRef = useRef("");
	const textBatchTimeoutRef = useRef<number | null>(null);
	const printableFallbackTimeoutRef = useRef<number | null>(null);
	const lastCompositionTextRef = useRef<{ at: number; text: string } | null>(
		null,
	);
	const disarmKeyboardCaptureRef = useRef<
		((options?: { flushText?: boolean }) => void) | null
	>(null);
	const keyboardCaptureSessionKeyRef = useRef(`${computerId}:${sessionId}`);
	const nextClickRippleIdRef = useRef(1);
	const streamIdRef = useRef<string | null>(null);
	const streamQualityRef = useRef<DesktopStreamQuality>("balanced");
	const adaptiveQualityMetricsRef = useRef<DesktopStreamMetrics | null>(null);
	const adaptiveQualityStableSamplesRef = useRef(0);
	const adaptiveQualityLastChangedAtRef = useRef(0);
	const liveVideoSeenRef = useRef(false);
	const hasLiveVideoRef = useRef(false);
	const fallbackRecoveryLastAttemptAtRef = useRef(0);
	const lastDesktopStreamRequestAtRef = useRef(0);
	const lastDesktopVideoFrameAtRef = useRef(0);
	const lastDesktopVideoHealthProbeAtRef = useRef(0);
	const videoFrameCallbackIdRef = useRef<number | null>(null);
	const streamStopRequestedRef = useRef(false);
	const [controlBarPosition, setControlBarPosition] =
		useState<DesktopControlBarPosition | null>(null);
	const [promptBarPosition, setPromptBarPosition] =
		useState<DesktopControlBarPosition | null>(null);
	const [desktopSurfaceSize, setDesktopSurfaceSize] =
		useState<DesktopSurfaceSize | null>(null);
	const resolvedPreviewUrl =
		state === "paused" ? null : (fallbackPreviewUrl ?? previewUrl);
	const hasVisibleDesktopMedia = hasLiveVideo || Boolean(resolvedPreviewUrl);
	const manualActive = manualState === "manual_active";
	const manualBusy =
		manualState === "manual_requesting" ||
		manualState === "manual_reconnecting" ||
		manualState === "manual_releasing";
	const shouldRotateDesktopContent =
		presentation.rotateDesktop && hasVisibleDesktopMedia;
	const presentationModeKey = `${presentation.isMobile}:${shouldRotateDesktopContent}`;
	const mobileViewportResetKey = `${computerId}:${sessionId}:${presentationModeKey}`;
	const manualPointerGestureResetKey = `${mobileViewportResetKey}:${streamId ?? ""}`;
	const setClampedMobileViewportTransform = useCallback(
		(transform: DesktopMobileViewportTransform) => {
			const rect = desktopSurfaceRef.current?.getBoundingClientRect() ?? null;
			const next = clampDesktopMobileViewportTransform(transform, rect);
			mobileViewportTransformRef.current = next;
			setMobileViewportTransform((current) =>
				desktopMobileViewportTransformsEqual(current, next) ? current : next,
			);
		},
		[],
	);
	const resetMobileViewportTransform = useCallback(() => {
		mobileTouchPointersRef.current.clear();
		mobileTapPointerIdRef.current = null;
		mobilePinchGestureRef.current = null;
		mobileViewportTransformRef.current =
			DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM;
		setMobileViewportTransform(DEFAULT_DESKTOP_MOBILE_VIEWPORT_TRANSFORM);
	}, []);
	const clearManualPointerGesture = useCallback(() => {
		const gesture = manualPointerGestureRef.current;
		if (!gesture) return;
		manualPointerGestureRef.current = null;
		if (gesture.dragStarted) {
			releaseManualPointerDragRef.current?.(gesture);
		}
		const surface = desktopSurfaceRef.current;
		if (!surface) return;
		try {
			surface.releasePointerCapture(gesture.pointerId);
		} catch {
			// The browser may have already dropped capture after cancel/release.
		}
	}, []);

	useEffect(() => {
		setControlBarPosition((position) =>
			presentationModeKey && position ? null : position,
		);
		setPromptBarPosition((position) =>
			presentationModeKey && position ? null : position,
		);
	}, [presentationModeKey]);

	useEffect(() => {
		if (!mobileViewportResetKey) return;
		resetMobileViewportTransform();
	}, [mobileViewportResetKey, resetMobileViewportTransform]);

	useEffect(() => {
		if (presentation.isMobile && hasVisibleDesktopMedia) return;
		resetMobileViewportTransform();
	}, [
		hasVisibleDesktopMedia,
		presentation.isMobile,
		resetMobileViewportTransform,
	]);

	useEffect(() => {
		return () => {
			if (fallbackPreviewObjectUrlRef.current) {
				URL.revokeObjectURL(fallbackPreviewObjectUrlRef.current);
				fallbackPreviewObjectUrlRef.current = null;
			}
			closeDesktopPeerConnection(peerConnectionRef.current);
			peerConnectionRef.current = null;
			const pendingVideo = videoRef.current as
				| (HTMLVideoElement & {
						cancelVideoFrameCallback?: (handle: number) => void;
				  })
				| null;
			if (
				pendingVideo &&
				videoFrameCallbackIdRef.current !== null &&
				typeof pendingVideo.cancelVideoFrameCallback === "function"
			) {
				pendingVideo.cancelVideoFrameCallback(videoFrameCallbackIdRef.current);
			}
			videoFrameCallbackIdRef.current = null;
			pendingMediaStreamRef.current = null;
			pendingIceCandidatesRef.current = [];
			dataChannelFramesRef.current.clear();
			if (videoRef.current) videoRef.current.srcObject = null;
			for (const timeout of clickRippleTimeoutsRef.current) {
				window.clearTimeout(timeout);
			}
			clickRippleTimeoutsRef.current.clear();
			mobileTouchPointersRef.current.clear();
			mobileTapPointerIdRef.current = null;
			mobilePinchGestureRef.current = null;
			const gesture = manualPointerGestureRef.current;
			manualPointerGestureRef.current = null;
			if (gesture?.dragStarted) {
				releaseManualPointerDragRef.current?.(gesture);
			}
			if (textBatchTimeoutRef.current !== null) {
				window.clearTimeout(textBatchTimeoutRef.current);
				textBatchTimeoutRef.current = null;
			}
			if (printableFallbackTimeoutRef.current !== null) {
				window.clearTimeout(printableFallbackTimeoutRef.current);
				printableFallbackTimeoutRef.current = null;
			}
		};
	}, []);

	useEffect(() => {
		streamIdRef.current = streamId;
	}, [streamId]);

	useEffect(() => {
		manualControlIdRef.current = manualControlId;
	}, [manualControlId]);

	useEffect(() => {
		if (!manualActive) clearManualPointerGesture();
	}, [clearManualPointerGesture, manualActive]);

	useEffect(() => {
		if (
			manualPointerGestureResetKeyRef.current === manualPointerGestureResetKey
		) {
			return;
		}
		manualPointerGestureResetKeyRef.current = manualPointerGestureResetKey;
		clearManualPointerGesture();
	}, [clearManualPointerGesture, manualPointerGestureResetKey]);

	useEffect(() => {
		streamQualityRef.current = streamQuality;
	}, [streamQuality]);

	useEffect(() => {
		const surface = desktopSurfaceRef.current;
		if (!surface || typeof ResizeObserver === "undefined") return;
		const updateSurfaceSize = () => {
			const rect = surface.getBoundingClientRect();
			const width = rect.width || surface.clientWidth;
			const height = rect.height || surface.clientHeight;
			if (width <= 0 || height <= 0) return;
			setDesktopSurfaceSize({
				height,
				width,
			});
		};
		updateSurfaceSize();
		const observer = new ResizeObserver(updateSurfaceSize);
		observer.observe(surface);
		return () => observer.disconnect();
	}, []);

	useEffect(() => {
		if (!isOnline) {
			setManualState("manual_idle");
			setState("offline");
		} else if (state === "offline") setState("waiting");
	}, [isOnline, state]);

	useEffect(() => {
		if (!toolbarPromptPending) return;
		if (isAgentWorking) {
			setToolbarPromptPending(false);
			return;
		}
		const handle = window.setTimeout(() => {
			setToolbarPromptPending(false);
		}, 3000);
		return () => window.clearTimeout(handle);
	}, [isAgentWorking, toolbarPromptPending]);

	const clearFallbackPreview = useCallback(() => {
		if (fallbackPreviewObjectUrlRef.current) {
			URL.revokeObjectURL(fallbackPreviewObjectUrlRef.current);
			fallbackPreviewObjectUrlRef.current = null;
		}
		setFallbackPreviewUrl(null);
	}, []);
	const setDesktopHasLiveVideo = useCallback((next: boolean) => {
		hasLiveVideoRef.current = next;
		setHasLiveVideo(next);
	}, []);
	const desktopStateAfterManualControlEnds =
		useCallback((): DesktopViewportState => {
			if (hasLiveVideoRef.current) return "live";
			return streamIdRef.current ? "degraded" : "waiting";
		}, []);

	const clearDesktopStreamMedia = useCallback(
		({
			clearPreview = false,
			clearStreamId = false,
		}: {
			clearPreview?: boolean;
			clearStreamId?: boolean;
		} = {}) => {
			closeDesktopPeerConnection(peerConnectionRef.current);
			peerConnectionRef.current = null;
			const pendingVideo = videoRef.current as
				| (HTMLVideoElement & {
						cancelVideoFrameCallback?: (handle: number) => void;
				  })
				| null;
			if (
				pendingVideo &&
				videoFrameCallbackIdRef.current !== null &&
				typeof pendingVideo.cancelVideoFrameCallback === "function"
			) {
				pendingVideo.cancelVideoFrameCallback(videoFrameCallbackIdRef.current);
			}
			videoFrameCallbackIdRef.current = null;
			pendingMediaStreamRef.current = null;
			pendingIceCandidatesRef.current = [];
			dataChannelFramesRef.current.clear();
			if (videoRef.current) videoRef.current.srcObject = null;
			setDesktopHasLiveVideo(false);
			lastDesktopVideoHealthProbeAtRef.current = 0;
			if (clearPreview) clearFallbackPreview();
			if (clearStreamId) {
				streamIdRef.current = null;
				setStreamId(null);
			}
		},
		[clearFallbackPreview, setDesktopHasLiveVideo],
	);

	const showDesktopDataChannelFrame = useCallback(
		(bytesBase64: string, mediaType: string) => {
			const bytes = base64ToBytes(bytesBase64);
			const blobBytes = new Uint8Array(bytes);
			const objectUrl = URL.createObjectURL(
				new Blob([blobBytes.buffer], { type: mediaType }),
			);
			if (fallbackPreviewObjectUrlRef.current) {
				URL.revokeObjectURL(fallbackPreviewObjectUrlRef.current);
			}
			fallbackPreviewObjectUrlRef.current = objectUrl;
			pendingMediaStreamRef.current = null;
			if (videoRef.current) videoRef.current.srcObject = null;
			setFallbackPreviewUrl(objectUrl);
			setDesktopHasLiveVideo(false);
			setState((current) => (current === "manual" ? current : "degraded"));
		},
		[setDesktopHasLiveVideo],
	);
	const markDesktopVideoFrame = useCallback(() => {
		lastDesktopVideoFrameAtRef.current = Date.now();
		lastDesktopVideoHealthProbeAtRef.current = 0;
	}, []);

	const handleDesktopDataChannelMessage = useCallback(
		(event: MessageEvent) => {
			if (streamStopRequestedRef.current) return;
			const chunk = desktopFrameChunkFromMessage(event.data);
			if (!chunk) return;
			const activeStreamId = streamIdRef.current;
			if (activeStreamId && chunk.streamId !== activeStreamId) return;
			if (!activeStreamId) {
				streamIdRef.current = chunk.streamId;
				setStreamId(chunk.streamId);
			}
			const frames = dataChannelFramesRef.current;
			let frame = frames.get(chunk.frameId);
			if (!frame || frame.total !== chunk.total) {
				frame = {
					chunks: Array<string | undefined>(chunk.total),
					mediaType: chunk.mediaType,
					received: 0,
					total: chunk.total,
				};
				frames.set(chunk.frameId, frame);
				pruneDesktopFrameBuffers(frames);
			}
			if (typeof frame.chunks[chunk.seq] !== "string") {
				frame.received += 1;
			}
			frame.chunks[chunk.seq] = chunk.data;
			if (frame.received !== frame.total) return;
			const chunks = frame.chunks;
			if (
				!chunks.every((value): value is string => typeof value === "string")
			) {
				return;
			}
			frames.delete(chunk.frameId);
			showDesktopDataChannelFrame(chunks.join(""), frame.mediaType);
		},
		[showDesktopDataChannelFrame],
	);

	const ensurePeerConnection = useCallback(
		(options: { fresh?: boolean } = {}) => {
			if (!channel || !deviceId) return null;
			if (options.fresh && peerConnectionRef.current) {
				closeDesktopPeerConnection(peerConnectionRef.current);
				peerConnectionRef.current = null;
				pendingMediaStreamRef.current = null;
				pendingIceCandidatesRef.current = [];
				dataChannelFramesRef.current.clear();
				if (videoRef.current) videoRef.current.srcObject = null;
				setDesktopHasLiveVideo(false);
			}
			if (peerConnectionRef.current) return peerConnectionRef.current;
			if (typeof RTCPeerConnection === "undefined") {
				setState(fallbackPreviewUrl ? "degraded" : "waiting");
				return null;
			}
			const peerConnection = new RTCPeerConnection({ iceServers });
			peerConnection.ondatachannel = (event) => {
				if (event.channel.label !== DESKTOP_STREAM_DATA_CHANNEL_LABEL) {
					event.channel.close();
					return;
				}
				event.channel.onmessage = handleDesktopDataChannelMessage;
				event.channel.onopen = () => {
					setState((current) => (current === "manual" ? current : "degraded"));
				};
				event.channel.onclose = () => {
					setState((current) =>
						current === "manual"
							? current
							: fallbackPreviewUrl
								? "degraded"
								: "paused",
					);
				};
			};
			peerConnection.ontrack = (event) => {
				const [mediaStream] = event.streams;
				if (!mediaStream) return;
				pendingMediaStreamRef.current = mediaStream;
				if (videoRef.current) videoRef.current.srcObject = mediaStream;
				liveVideoSeenRef.current = true;
				markDesktopVideoFrame();
				fallbackRecoveryLastAttemptAtRef.current = 0;
				clearFallbackPreview();
				setDesktopHasLiveVideo(true);
				setState((current) =>
					current === "manual" || manualActive ? "manual" : "live",
				);
			};
			peerConnection.onicecandidate = (event) => {
				if (!event.candidate || !channel || !deviceId) return;
				sendComputerUseStreamIceCandidate(channel, {
					computerId,
					sessionId,
					deviceId,
					runId: streamRunId,
					streamId: streamIdRef.current,
					streamToken,
					candidate: event.candidate.toJSON(),
				});
			};
			peerConnection.onconnectionstatechange = () => {
				if (
					peerConnection.connectionState === "failed" ||
					peerConnection.connectionState === "disconnected"
				) {
					setState(fallbackPreviewUrl ? "degraded" : "connecting");
				}
				if (peerConnection.connectionState === "closed") {
					setState(fallbackPreviewUrl ? "degraded" : "paused");
				}
			};
			peerConnectionRef.current = peerConnection;
			return peerConnection;
		},
		[
			channel,
			clearFallbackPreview,
			computerId,
			deviceId,
			fallbackPreviewUrl,
			handleDesktopDataChannelMessage,
			iceServers,
			manualActive,
			markDesktopVideoFrame,
			sessionId,
			setDesktopHasLiveVideo,
			streamRunId,
			streamToken,
		],
	);

	const handleWebRtcSignal = useCallback(
		async (payload: ComputerUseDesktopPayload) => {
			if (!channel || !deviceId) return;
			try {
				if (payload.schema === "xero.computer_use_stream_offer.v1") {
					const offer = desktopStreamSessionDescription(payload, "offer");
					if (!offer) return;
					if (payload.streamId) {
						streamIdRef.current = payload.streamId;
						setStreamId(payload.streamId);
					}
					const peerConnection = ensurePeerConnection({ fresh: true });
					if (!peerConnection) return;
					setStreamFailureMessage(null);
					setState("connecting");
					await peerConnection.setRemoteDescription(offer);
					await flushPendingDesktopIceCandidates(
						peerConnection,
						pendingIceCandidatesRef.current,
					);
					const answer = await peerConnection.createAnswer();
					await peerConnection.setLocalDescription(answer);
					answerComputerUseStreamOffer(channel, {
						computerId,
						sessionId,
						deviceId,
						runId: streamRunId,
						streamId: payload.streamId ?? streamIdRef.current,
						streamToken,
						answer,
					});
					return;
				}
				if (payload.schema === "xero.computer_use_stream_ice_candidate.v1") {
					const candidate = desktopStreamIceCandidate(payload);
					if (!candidate) return;
					if (
						!peerConnectionRef.current ||
						!peerConnectionRef.current.remoteDescription
					) {
						queuePendingDesktopIceCandidate(
							pendingIceCandidatesRef.current,
							candidate,
						);
						return;
					}
					await peerConnectionRef.current.addIceCandidate(candidate);
				}
			} catch {
				clearDesktopStreamMedia();
				if (fallbackPreviewUrl) {
					setState("degraded");
				} else {
					setStreamFailureMessage(
						"The desktop stream could not complete WebRTC negotiation. Try starting it again.",
					);
					setState("failed");
				}
			}
		},
		[
			channel,
			clearDesktopStreamMedia,
			computerId,
			deviceId,
			ensurePeerConnection,
			fallbackPreviewUrl,
			sessionId,
			streamRunId,
			streamToken,
		],
	);
	const handleRemoteCommandResult = useCallback(
		(payload: RemoteCommandResultPayload) => {
			if (!remoteCommandResultFailed(payload)) return false;
			const kind = remoteCommandKind(payload);
			if (!kind || !kind.startsWith("computer_use_stream_")) return false;
			streamStopRequestedRef.current = true;
			clearDesktopStreamMedia({ clearPreview: true, clearStreamId: true });
			setStreamFailureMessage(remoteCommandFailureMessage(payload));
			setState(
				remoteCommandFailureReason(payload) ===
					REMOTE_CONTROL_ALREADY_ACTIVE_REASON
					? "blocked"
					: "failed",
			);
			return true;
		},
		[clearDesktopStreamMedia],
	);
	const applyAdaptiveStreamQuality = useCallback(
		(
			metrics: DesktopStreamMetrics | null,
			sampleState: DesktopViewportState,
		) => {
			const activeStreamId = streamIdRef.current;
			if (!channel || !deviceId || !activeStreamId) return;
			const now = Date.now();
			const decision = chooseDesktopAdaptiveStreamQuality({
				currentQuality: streamQualityRef.current,
				lastChangedAt: adaptiveQualityLastChangedAtRef.current,
				metrics,
				now,
				previousMetrics: adaptiveQualityMetricsRef.current,
				stableSamples: adaptiveQualityStableSamplesRef.current,
				state: sampleState,
			});
			adaptiveQualityMetricsRef.current = metrics;
			adaptiveQualityStableSamplesRef.current = decision.stableSamples;
			if (decision.quality === streamQualityRef.current) return;
			streamQualityRef.current = decision.quality;
			adaptiveQualityLastChangedAtRef.current = now;
			setStreamQuality(decision.quality);
			setComputerUseStreamQuality(channel, {
				computerId,
				sessionId,
				deviceId,
				runId: streamRunId,
				streamId: activeStreamId,
				quality: decision.quality,
				streamToken,
			});
			requestComputerUseStreamKeyframe(channel, {
				computerId,
				sessionId,
				deviceId,
				runId: streamRunId,
				streamId: activeStreamId,
				streamToken,
			});
		},
		[channel, computerId, deviceId, sessionId, streamRunId, streamToken],
	);
	const recoverDesktopWebRtcStream = useCallback(
		(activeStreamId: string | null) => {
			if (
				!channel ||
				!deviceId ||
				!isOnline ||
				streamStopRequestedRef.current
			) {
				return false;
			}
			const streamIdForRestart = activeStreamId ?? streamIdRef.current;
			if (!streamIdForRestart) return false;
			const now = Date.now();
			if (
				now - fallbackRecoveryLastAttemptAtRef.current <
				DESKTOP_STREAM_FALLBACK_RECOVERY_COOLDOWN_MS
			) {
				return true;
			}
			fallbackRecoveryLastAttemptAtRef.current = now;
			clearDesktopStreamMedia({ clearPreview: true });
			setState("connecting");
			lastDesktopStreamRequestAtRef.current = now;
			requestComputerUseStream(channel, {
				computerId,
				sessionId,
				deviceId,
				streamId: streamIdForRestart,
				quality: streamQualityRef.current,
				runId: streamRunId,
				streamToken,
				iceServers,
			});
			return true;
		},
		[
			channel,
			clearDesktopStreamMedia,
			computerId,
			deviceId,
			iceServers,
			isOnline,
			sessionId,
			streamRunId,
			streamToken,
		],
	);

	useEffect(() => {
		if (!channel) return;
		const ref = channel.on("frame", (rawFrame: unknown) => {
			const envelope = decodeRelayFrame(rawFrame);
			const payload = envelope?.payload;
			if (isRemoteCommandResultPayload(payload)) {
				if (handleRemoteCommandResult(payload)) return;
			}
			if (!isComputerUseDesktopPayload(payload)) return;
			const isStreamStopPayload =
				payload.schema === "xero.computer_use_stream_stop.v1";
			if (streamStopRequestedRef.current && !isStreamStopPayload) return;
			if (isStreamStopPayload) {
				disarmKeyboardCaptureRef.current?.();
				clearDesktopStreamMedia({ clearPreview: true, clearStreamId: true });
				adaptiveQualityMetricsRef.current = null;
				adaptiveQualityStableSamplesRef.current = 0;
				setState("paused");
				return;
			}
			if (payload.streamId) {
				streamIdRef.current = payload.streamId;
				setStreamId(payload.streamId);
			}
			if (payload.ok !== false && payload.outcome !== "rejected") {
				setStreamFailureMessage(null);
			}
			const nextStreamDetails = desktopStreamDetails(payload);
			const shouldRecoverFromFallback = shouldRecoverDesktopWebRtcAfterFallback(
				nextStreamDetails,
				liveVideoSeenRef.current,
			);
			if (nextStreamDetails) {
				if (nextStreamDetails.quality) {
					streamQualityRef.current = nextStreamDetails.quality;
					setStreamQuality(nextStreamDetails.quality);
				}
				if (!shouldRecoverFromFallback) {
					applyAdaptiveStreamQuality(
						nextStreamDetails.metrics ?? null,
						nextStreamDetails.status === "degraded" ||
							nextStreamDetails.transport === "screenshot_fallback"
							? "degraded"
							: state,
					);
				}
			}
			if (
				shouldRecoverFromFallback &&
				recoverDesktopWebRtcStream(payload.streamId ?? streamIdRef.current)
			) {
				return;
			}
			const payloadManualControlId = payload.manualControlId ?? null;
			const manualResponseMatches =
				!payloadManualControlId ||
				!manualControlIdRef.current ||
				payloadManualControlId === manualControlIdRef.current;
			void handleWebRtcSignal(payload);
			if (payload.desktopFrame?.ok && payload.desktopFrame.bytesBase64) {
				const bytes = base64ToBytes(payload.desktopFrame.bytesBase64);
				const blobBytes = new Uint8Array(bytes);
				const objectUrl = URL.createObjectURL(
					new Blob([blobBytes.buffer], {
						type: payload.desktopFrame.mediaType ?? "image/png",
					}),
				);
				if (fallbackPreviewObjectUrlRef.current) {
					URL.revokeObjectURL(fallbackPreviewObjectUrlRef.current);
				}
				fallbackPreviewObjectUrlRef.current = objectUrl;
				setFallbackPreviewUrl(objectUrl);
				setDesktopHasLiveVideo(false);
				setState((current) => (current === "manual" ? current : "degraded"));
			}
			if (
				payload.schema.startsWith("xero.computer_use_stream_") &&
				payload.schema !== "xero.computer_use_stream_offer.v1" &&
				state !== "live" &&
				state !== "manual"
			) {
				const webRtcStreamHealthy =
					nextStreamDetails?.transport === "web_rtc" &&
					nextStreamDetails.status !== "degraded";
				setState(
					webRtcStreamHealthy
						? hasLiveVideoRef.current
							? "live"
							: "connecting"
						: "degraded",
				);
			} else if (
				payload.schema === "xero.computer_use_manual_control_request.v1" ||
				payload.schema === "xero.computer_use_manual_control_grant.v1"
			) {
				if (!manualResponseMatches) return;
				if (payloadManualControlId) {
					manualControlIdRef.current = payloadManualControlId;
					setManualControlId(payloadManualControlId);
				}
				if (computerUseCommandSucceeded(payload)) {
					setManualState("manual_active");
					setState("manual");
				} else {
					disarmKeyboardCaptureRef.current?.();
					manualControlIdRef.current = null;
					setManualControlId(null);
					setManualState("manual_denied");
					setState(desktopStateAfterManualControlEnds());
				}
			} else if (
				payload.schema === "xero.computer_use_manual_control_heartbeat.v1"
			) {
				if (!manualResponseMatches) return;
				if (computerUseCommandSucceeded(payload)) {
					setManualState("manual_active");
					setState("manual");
				} else {
					disarmKeyboardCaptureRef.current?.({ flushText: true });
					setManualState("manual_reconnecting");
					setState(desktopStateAfterManualControlEnds());
				}
			} else if (
				payload.schema === "xero.computer_use_manual_control_input.v1"
			) {
				if (!manualResponseMatches) return;
				if (!computerUseCommandSucceeded(payload)) {
					disarmKeyboardCaptureRef.current?.({ flushText: true });
					setManualState("manual_reconnecting");
					setState(desktopStateAfterManualControlEnds());
				}
			} else if (
				payload.schema === "xero.computer_use_manual_control_release.v1"
			) {
				if (!manualResponseMatches) return;
				disarmKeyboardCaptureRef.current?.();
				manualControlIdRef.current = null;
				setManualControlId(null);
				setManualState("manual_released");
				setState(desktopStateAfterManualControlEnds());
			}
		});
		return () => {
			channel.off("frame", ref);
		};
	}, [
		applyAdaptiveStreamQuality,
		channel,
		clearDesktopStreamMedia,
		desktopStateAfterManualControlEnds,
		handleRemoteCommandResult,
		handleWebRtcSignal,
		recoverDesktopWebRtcStream,
		setDesktopHasLiveVideo,
		state,
	]);

	useEffect(() => {
		if (!channel) return;
		const ref = channel.on(
			"computer_use_command_outcome",
			(payload: unknown) => {
				if (!isCommandAckResult(payload)) return;
				const activeManualControlId = manualControlIdRef.current;
				const isActiveManualCommand =
					payload.kind === "computer_use_manual_control_request" ||
					payload.kind === "computer_use_manual_control_heartbeat" ||
					payload.kind === "computer_use_manual_control_input" ||
					payload.kind === "computer_use_manual_control_release";
				if (!activeManualControlId || !isActiveManualCommand) return;
				if (
					payload.outcome === "rate_limited" ||
					payload.outcome === "rejected" ||
					payload.outcome === "timed_out" ||
					payload.outcome === "stale"
				) {
					disarmKeyboardCaptureRef.current?.({ flushText: true });
					if (
						payload.kind === "computer_use_manual_control_request" ||
						payload.kind === "computer_use_manual_control_release"
					) {
						manualControlIdRef.current = null;
						setManualControlId(null);
					}
					setManualState(
						payload.kind === "computer_use_manual_control_heartbeat" ||
							payload.kind === "computer_use_manual_control_input"
							? "manual_reconnecting"
							: "manual_denied",
					);
					setState(desktopStateAfterManualControlEnds());
				}
				if (
					payload.kind === "computer_use_manual_control_release" &&
					(payload.outcome === "executed" || payload.outcome === "duplicate")
				) {
					disarmKeyboardCaptureRef.current?.();
					manualControlIdRef.current = null;
					setManualControlId(null);
					setManualState("manual_released");
					setState(desktopStateAfterManualControlEnds());
				}
			},
		);
		return () => {
			channel.off("computer_use_command_outcome", ref);
		};
	}, [channel, desktopStateAfterManualControlEnds]);

	useEffect(() => {
		if (!channel || !deviceId || !manualActive || !manualControlId) return;
		const sendHeartbeat = () => {
			void heartbeatComputerUseManualControl(channel, {
				computerId,
				sessionId,
				deviceId,
				manualControlId,
				runId: streamRunId,
				streamToken,
				reason: "manual_cloud_control_heartbeat",
			}).then((result) => {
				if (
					result.outcome === "rate_limited" ||
					result.outcome === "rejected" ||
					result.outcome === "timed_out"
				) {
					setManualState("manual_reconnecting");
					setState(desktopStateAfterManualControlEnds());
				}
			});
		};
		sendHeartbeat();
		const handle = window.setInterval(sendHeartbeat, 10_000);
		return () => window.clearInterval(handle);
	}, [
		channel,
		computerId,
		deviceId,
		desktopStateAfterManualControlEnds,
		manualControlId,
		manualActive,
		sessionId,
		streamRunId,
		streamToken,
	]);

	useEffect(() => {
		if (
			!channel ||
			!deviceId ||
			manualState !== "manual_reconnecting" ||
			!manualControlId
		) {
			return;
		}
		const handle = window.setTimeout(() => {
			void requestComputerUseManualControl(channel, {
				computerId,
				sessionId,
				deviceId,
				manualControlId,
				runId: streamRunId,
				streamToken,
				reason: "manual_cloud_control_reacquire",
			}).then((result) => {
				if (
					result.outcome === "rate_limited" ||
					result.outcome === "rejected" ||
					result.outcome === "timed_out"
				) {
					setManualState("manual_denied");
				}
			});
		}, 750);
		return () => window.clearTimeout(handle);
	}, [
		channel,
		computerId,
		deviceId,
		manualControlId,
		manualState,
		sessionId,
		streamRunId,
		streamToken,
	]);

	useEffect(() => {
		if (
			manualState !== "manual_requesting" &&
			manualState !== "manual_reconnecting" &&
			manualState !== "manual_releasing"
		) {
			return;
		}
		const timeoutMs = manualState === "manual_releasing" ? 8_000 : 10_000;
		const handle = window.setTimeout(() => {
			disarmKeyboardCaptureRef.current?.({ flushText: true });
			if (manualState === "manual_releasing") {
				manualControlIdRef.current = null;
				setManualControlId(null);
				setManualState("manual_released");
			} else {
				manualControlIdRef.current = null;
				setManualControlId(null);
				setManualState("manual_denied");
			}
			setState(desktopStateAfterManualControlEnds());
		}, timeoutMs);
		return () => window.clearTimeout(handle);
	}, [desktopStateAfterManualControlEnds, manualState]);

	useEffect(() => {
		if (!channel || !deviceId || !streamId) return;
		if (!STREAM_STATUS_STATES.has(state)) return;
		const intervalMs =
			state === "degraded" || state === "manual"
				? fallbackFrameIntervalMs(streamQuality)
				: DESKTOP_STREAM_STATUS_INTERVAL_MS;
		const handle = window.setInterval(() => {
			requestComputerUseStreamStatus(channel, {
				computerId,
				sessionId,
				deviceId,
				runId: streamRunId,
				streamId,
				streamToken,
			});
		}, intervalMs);
		return () => window.clearInterval(handle);
	}, [
		channel,
		computerId,
		deviceId,
		streamQuality,
		sessionId,
		state,
		streamId,
		streamRunId,
		streamToken,
	]);

	const canSend = Boolean(channel && deviceId && isOnline);
	const canStopDesktop = canSend && STOPPABLE_DESKTOP_STATES.has(state);
	const canUseManualControl =
		canSend && MANUAL_CONTROL_DESKTOP_STATES.has(state) && !manualBusy;
	const status = desktopViewportStatusLabel(
		state,
		resolvedPreviewUrl,
		manualState,
	);
	const requestStreamKeyframe = useCallback(() => {
		if (!channel || !deviceId || !streamId) return;
		requestComputerUseStreamKeyframe(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId,
			streamToken,
		});
	}, [
		channel,
		computerId,
		deviceId,
		sessionId,
		streamId,
		streamRunId,
		streamToken,
	]);
	useEffect(() => {
		if (!hasLiveVideo || !videoRef.current || !pendingMediaStreamRef.current)
			return;
		videoRef.current.srcObject = pendingMediaStreamRef.current;
	}, [hasLiveVideo]);

	useEffect(() => {
		const video = videoRef.current as
			| (HTMLVideoElement & {
					requestVideoFrameCallback?: (callback: () => void) => number;
					cancelVideoFrameCallback?: (handle: number) => void;
			  })
			| null;
		if (!hasLiveVideo || !video) return;

		let disposed = false;
		const markFrame = () => {
			markDesktopVideoFrame();
		};
		const scheduleFrameProbe = () => {
			if (disposed || typeof video.requestVideoFrameCallback !== "function") {
				return;
			}
			videoFrameCallbackIdRef.current = video.requestVideoFrameCallback(() => {
				videoFrameCallbackIdRef.current = null;
				markFrame();
				scheduleFrameProbe();
			});
		};

		markFrame();
		scheduleFrameProbe();
		return () => {
			disposed = true;
			if (
				videoFrameCallbackIdRef.current !== null &&
				typeof video.cancelVideoFrameCallback === "function"
			) {
				video.cancelVideoFrameCallback(videoFrameCallbackIdRef.current);
			}
			videoFrameCallbackIdRef.current = null;
		};
	}, [hasLiveVideo, markDesktopVideoFrame]);

	useEffect(() => {
		if (!canSend || !streamId || !KEYFRAME_REFRESH_STATES.has(state)) return;
		const firstRequest = window.setTimeout(requestStreamKeyframe, 250);
		const recurringRequest = window.setInterval(
			requestStreamKeyframe,
			DESKTOP_KEYFRAME_REFRESH_MS,
		);
		return () => {
			window.clearTimeout(firstRequest);
			window.clearInterval(recurringRequest);
		};
	}, [canSend, requestStreamKeyframe, state, streamId]);

	useEffect(() => {
		if (!canSend || !channel || !deviceId || state !== "connecting") return;
		const retryConnectingStream = () => {
			if (streamStopRequestedRef.current) return;
			const now = Date.now();
			if (
				now - lastDesktopStreamRequestAtRef.current <
				DESKTOP_STREAM_CONNECTING_RETRY_MS
			) {
				return;
			}
			lastDesktopStreamRequestAtRef.current = now;
			closeDesktopPeerConnection(peerConnectionRef.current);
			peerConnectionRef.current = null;
			pendingMediaStreamRef.current = null;
			pendingIceCandidatesRef.current = [];
			dataChannelFramesRef.current.clear();
			if (videoRef.current) videoRef.current.srcObject = null;
			setDesktopHasLiveVideo(false);
			requestComputerUseStream(channel, {
				computerId,
				sessionId,
				deviceId,
				streamId: streamIdRef.current,
				quality: streamQualityRef.current,
				runId: streamRunId,
				streamToken,
				iceServers,
			});
		};
		const handle = window.setInterval(retryConnectingStream, 1_000);
		return () => window.clearInterval(handle);
	}, [
		canSend,
		channel,
		computerId,
		deviceId,
		iceServers,
		sessionId,
		setDesktopHasLiveVideo,
		state,
		streamRunId,
		streamToken,
	]);

	useEffect(() => {
		if (
			!canSend ||
			!streamId ||
			!hasLiveVideo ||
			(state !== "live" && state !== "manual")
		) {
			return;
		}
		const recoverStaleVideo = () => {
			if (streamStopRequestedRef.current) return;
			const lastFrameAt = lastDesktopVideoFrameAtRef.current;
			const now = Date.now();
			if (!lastFrameAt || now - lastFrameAt < DESKTOP_STREAM_STALE_FRAME_MS) {
				return;
			}
			if (!desktopPeerConnectionNeedsMediaRecovery(peerConnectionRef.current)) {
				if (
					now - lastDesktopVideoHealthProbeAtRef.current >=
					DESKTOP_STREAM_STALE_FRAME_MS
				) {
					lastDesktopVideoHealthProbeAtRef.current = now;
					requestStreamKeyframe();
				}
				return;
			}
			requestStreamKeyframe();
			recoverDesktopWebRtcStream(streamIdRef.current);
		};
		const handle = window.setInterval(recoverStaleVideo, 1_000);
		return () => window.clearInterval(handle);
	}, [
		canSend,
		hasLiveVideo,
		recoverDesktopWebRtcStream,
		requestStreamKeyframe,
		state,
		streamId,
	]);

	const startStream = () => {
		if (!channel || !deviceId) return;
		if (presentation.isMobile) requestNativeDesktopOrientationLock();
		clearManualPointerGesture();
		disarmKeyboardCaptureRef.current?.();
		streamStopRequestedRef.current = false;
		liveVideoSeenRef.current = false;
		fallbackRecoveryLastAttemptAtRef.current = 0;
		lastDesktopStreamRequestAtRef.current = Date.now();
		lastDesktopVideoFrameAtRef.current = 0;
		lastDesktopVideoHealthProbeAtRef.current = 0;
		streamIdRef.current = null;
		streamQualityRef.current = "balanced";
		adaptiveQualityMetricsRef.current = null;
		adaptiveQualityStableSamplesRef.current = 0;
		adaptiveQualityLastChangedAtRef.current = Date.now();
		clearDesktopStreamMedia({ clearPreview: true, clearStreamId: true });
		setStreamFailureMessage(null);
		setStreamQuality("balanced");
		setState("connecting");
		void requestComputerUseStream(channel, {
			computerId,
			sessionId,
			deviceId,
			quality: "balanced",
			runId: streamRunId,
			streamToken,
			iceServers,
		}).then((result) => {
			if (remoteControlConnectionAlreadyActive(result)) {
				streamStopRequestedRef.current = true;
				clearDesktopStreamMedia({ clearPreview: true, clearStreamId: true });
				setState("blocked");
			}
		});
	};
	const stopStream = () => {
		if (!channel || !deviceId) return;
		clearManualPointerGesture();
		disarmKeyboardCaptureRef.current?.({ flushText: true });
		const activeStreamId = streamIdRef.current ?? streamId;
		const activeManualControlId = manualControlIdRef.current ?? manualControlId;
		streamStopRequestedRef.current = true;
		clearDesktopStreamMedia({ clearPreview: true, clearStreamId: true });
		setStreamFailureMessage(null);
		adaptiveQualityMetricsRef.current = null;
		adaptiveQualityStableSamplesRef.current = 0;
		if (state === "manual" || activeManualControlId) {
			setManualState("manual_releasing");
			void releaseComputerUseManualControl(channel, {
				computerId,
				sessionId,
				deviceId,
				manualControlId: activeManualControlId,
				runId: streamRunId,
				streamToken,
			});
		}
		void stopComputerUseStream(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId: activeStreamId,
			streamToken,
		});
		setState("paused");
	};
	const requestManual = () => {
		if (!channel || !deviceId) return;
		clearManualPointerGesture();
		disarmKeyboardCaptureRef.current?.();
		const nextManualControlId =
			manualControlIdRef.current ?? createManualControlId(deviceId, sessionId);
		manualControlIdRef.current = nextManualControlId;
		setManualControlId(nextManualControlId);
		setManualState("manual_requesting");
		void requestComputerUseManualControl(channel, {
			computerId,
			sessionId,
			deviceId,
			manualControlId: nextManualControlId,
			runId: streamRunId,
			streamToken,
			reason: "manual_cloud_control",
		}).then((result) => {
			if (
				result.outcome === "rate_limited" ||
				result.outcome === "rejected" ||
				result.outcome === "timed_out"
			) {
				manualControlIdRef.current = null;
				setManualControlId(null);
				setManualState("manual_denied");
				setState(desktopStateAfterManualControlEnds());
			}
		});
	};
	const releaseManual = () => {
		if (!channel || !deviceId) return;
		clearManualPointerGesture();
		disarmKeyboardCaptureRef.current?.({ flushText: true });
		const activeManualControlId = manualControlIdRef.current ?? manualControlId;
		setManualState("manual_releasing");
		void releaseComputerUseManualControl(channel, {
			computerId,
			sessionId,
			deviceId,
			manualControlId: activeManualControlId,
			runId: streamRunId,
			streamToken,
		}).then((result) => {
			if (result.outcome === "executed" || result.outcome === "duplicate") {
				manualControlIdRef.current = null;
				setManualControlId(null);
				setManualState("manual_released");
				setState(desktopStateAfterManualControlEnds());
			}
		});
	};
	const sendManualInput = useCallback(
		(
			input: DesktopManualInput,
			options: { allowInactive?: boolean; ordered?: boolean } = {},
		) => {
			const send = async () => {
				const activeManualControlId =
					manualControlIdRef.current ?? manualControlId;
				if (
					!channel ||
					!deviceId ||
					(!manualActive && !options.allowInactive) ||
					!activeManualControlId
				) {
					return;
				}
				await sendComputerUseManualInput(channel, {
					computerId,
					sessionId,
					deviceId,
					manualControlId: activeManualControlId,
					runId: streamRunId,
					streamToken,
					input,
				});
			};
			if (options.ordered) {
				const next = manualInputOrderRef.current
					.catch(() => undefined)
					.then(send);
				manualInputOrderRef.current = next.then(
					() => undefined,
					() => undefined,
				);
				void next;
				return;
			}
			void send();
		},
		[
			channel,
			computerId,
			deviceId,
			manualControlId,
			manualActive,
			sessionId,
			streamRunId,
			streamToken,
		],
	);
	const clearKeyboardSinkValue = useCallback(() => {
		if (keyboardSinkRef.current) keyboardSinkRef.current.value = "";
	}, []);
	const cancelTextBatchTimeout = useCallback(() => {
		if (textBatchTimeoutRef.current === null) return;
		window.clearTimeout(textBatchTimeoutRef.current);
		textBatchTimeoutRef.current = null;
	}, []);
	const cancelPrintableFallback = useCallback(() => {
		if (printableFallbackTimeoutRef.current === null) return;
		window.clearTimeout(printableFallbackTimeoutRef.current);
		printableFallbackTimeoutRef.current = null;
	}, []);
	const keyboardCaptureIsActive = useCallback(() => {
		const activeManualControlId = manualControlIdRef.current ?? manualControlId;
		return (
			manualActive &&
			keyboardCaptureState !== "inactive" &&
			Boolean(activeManualControlId) &&
			keyboardCaptureManualControlIdRef.current === activeManualControlId
		);
	}, [keyboardCaptureState, manualActive, manualControlId]);
	const flushTextBatch = useCallback(() => {
		cancelTextBatchTimeout();
		const text = textBatchRef.current;
		if (!text) return;
		textBatchRef.current = "";
		for (const chunk of manualKeyboardTextChunks(text)) {
			sendManualInput({ action: "type_text", text: chunk });
		}
	}, [cancelTextBatchTimeout, sendManualInput]);
	const queueTypeText = useCallback(
		(text: string) => {
			const chunk = manualKeyboardTextChunk(text);
			if (!chunk || !keyboardCaptureIsActive()) return;
			textBatchRef.current += chunk;
			if (
				Array.from(textBatchRef.current).length >=
				MANUAL_KEYBOARD_TEXT_CHUNK_CHARS
			) {
				flushTextBatch();
				return;
			}
			cancelTextBatchTimeout();
			textBatchTimeoutRef.current = window.setTimeout(
				flushTextBatch,
				MANUAL_KEYBOARD_TEXT_BATCH_MS,
			);
		},
		[cancelTextBatchTimeout, flushTextBatch, keyboardCaptureIsActive],
	);
	const disarmKeyboardCapture = useCallback(
		({ flushText = false }: { flushText?: boolean } = {}) => {
			cancelPrintableFallback();
			if (flushText) {
				flushTextBatch();
			} else {
				cancelTextBatchTimeout();
				textBatchRef.current = "";
			}
			keyboardCaptureManualControlIdRef.current = null;
			lastCompositionTextRef.current = null;
			setKeyboardCaptureState("inactive");
			clearKeyboardSinkValue();
		},
		[
			cancelPrintableFallback,
			cancelTextBatchTimeout,
			clearKeyboardSinkValue,
			flushTextBatch,
		],
	);
	useEffect(() => {
		disarmKeyboardCaptureRef.current = disarmKeyboardCapture;
		return () => {
			if (disarmKeyboardCaptureRef.current === disarmKeyboardCapture) {
				disarmKeyboardCaptureRef.current = null;
			}
		};
	}, [disarmKeyboardCapture]);
	const armKeyboardCapture = useCallback(() => {
		const activeManualControlId = manualControlIdRef.current ?? manualControlId;
		if (!manualActive || !activeManualControlId) return;
		keyboardCaptureManualControlIdRef.current = activeManualControlId;
		setKeyboardCaptureState("armed");
		keyboardSinkRef.current?.focus({ preventScroll: true });
		clearKeyboardSinkValue();
	}, [clearKeyboardSinkValue, manualActive, manualControlId]);
	const queuePrintableFallback = useCallback(
		(text: string) => {
			cancelPrintableFallback();
			if (!keyboardCaptureIsActive()) return;
			printableFallbackTimeoutRef.current = window.setTimeout(() => {
				printableFallbackTimeoutRef.current = null;
				queueTypeText(text);
			}, MANUAL_KEYBOARD_FALLBACK_MS);
		},
		[cancelPrintableFallback, keyboardCaptureIsActive, queueTypeText],
	);

	useEffect(() => {
		const activeManualControlId = manualControlIdRef.current ?? manualControlId;
		if (
			!manualActive ||
			!activeManualControlId ||
			(keyboardCaptureManualControlIdRef.current &&
				keyboardCaptureManualControlIdRef.current !== activeManualControlId)
		) {
			disarmKeyboardCapture();
		}
	}, [disarmKeyboardCapture, manualActive, manualControlId]);

	useEffect(() => {
		const nextSessionKey = `${computerId}:${sessionId}`;
		if (keyboardCaptureSessionKeyRef.current === nextSessionKey) return;
		keyboardCaptureSessionKeyRef.current = nextSessionKey;
		disarmKeyboardCapture();
	}, [computerId, disarmKeyboardCapture, sessionId]);

	useEffect(() => {
		if (keyboardCaptureState === "inactive") return;
		const handleDocumentPointerDown = (event: globalThis.PointerEvent) => {
			const surface = desktopSurfaceRef.current;
			if (
				surface &&
				event.target instanceof Node &&
				surface.contains(event.target)
			) {
				return;
			}
			disarmKeyboardCapture({ flushText: true });
		};
		document.addEventListener("pointerdown", handleDocumentPointerDown, true);
		return () => {
			document.removeEventListener(
				"pointerdown",
				handleDocumentPointerDown,
				true,
			);
		};
	}, [disarmKeyboardCapture, keyboardCaptureState]);

	const handleKeyboardBeforeInput = useCallback(
		(event: FormEvent<HTMLTextAreaElement>) => {
			if (!keyboardCaptureIsActive()) return;
			const input = beforeInputText(event);
			if (!input.text) {
				clearKeyboardSinkValue();
				return;
			}
			event.preventDefault();
			cancelPrintableFallback();
			clearKeyboardSinkValue();
			if (
				input.isComposing ||
				input.inputType === "insertCompositionText" ||
				input.inputType === "insertFromPaste"
			) {
				return;
			}
			const lastComposition = lastCompositionTextRef.current;
			if (
				lastComposition &&
				lastComposition.text === input.text &&
				Date.now() - lastComposition.at <
					MANUAL_KEYBOARD_COMPOSITION_DUPLICATE_MS
			) {
				return;
			}
			queueTypeText(input.text);
		},
		[
			cancelPrintableFallback,
			clearKeyboardSinkValue,
			keyboardCaptureIsActive,
			queueTypeText,
		],
	);
	const handleKeyboardInput = useCallback(
		(event: FormEvent<HTMLTextAreaElement>) => {
			if (keyboardCaptureIsActive()) {
				const text = event.currentTarget.value;
				const lastComposition = lastCompositionTextRef.current;
				if (
					text &&
					!(
						lastComposition &&
						lastComposition.text === text &&
						Date.now() - lastComposition.at <
							MANUAL_KEYBOARD_COMPOSITION_DUPLICATE_MS
					)
				) {
					cancelPrintableFallback();
					queueTypeText(text);
				}
			}
			clearKeyboardSinkValue();
		},
		[
			cancelPrintableFallback,
			clearKeyboardSinkValue,
			keyboardCaptureIsActive,
			queueTypeText,
		],
	);
	const handleKeyboardCompositionStart = useCallback(() => {
		if (!keyboardCaptureIsActive()) return;
		cancelPrintableFallback();
		setKeyboardCaptureState("composing");
	}, [cancelPrintableFallback, keyboardCaptureIsActive]);
	const handleKeyboardCompositionEnd = useCallback(
		(event: CompositionEvent<HTMLTextAreaElement>) => {
			if (!keyboardCaptureIsActive()) return;
			setKeyboardCaptureState("armed");
			cancelPrintableFallback();
			clearKeyboardSinkValue();
			const text = event.data;
			if (!text) return;
			lastCompositionTextRef.current = { at: Date.now(), text };
			queueTypeText(text);
		},
		[
			cancelPrintableFallback,
			clearKeyboardSinkValue,
			keyboardCaptureIsActive,
			queueTypeText,
		],
	);
	const handleKeyboardPaste = useCallback(
		(event: ClipboardEvent<HTMLTextAreaElement>) => {
			if (!keyboardCaptureIsActive()) return;
			event.preventDefault();
			cancelPrintableFallback();
			flushTextBatch();
			clearKeyboardSinkValue();
			const text = manualKeyboardTextChunk(
				event.clipboardData.getData("text/plain"),
			);
			if (!text) return;
			sendManualInput({ action: "paste_text", text });
		},
		[
			cancelPrintableFallback,
			clearKeyboardSinkValue,
			flushTextBatch,
			keyboardCaptureIsActive,
			sendManualInput,
		],
	);
	const handleKeyboardSinkBlur = useCallback(
		(event: FocusEvent<HTMLTextAreaElement>) => {
			const nextTarget = event.relatedTarget;
			const surface = desktopSurfaceRef.current;
			if (
				nextTarget instanceof Node &&
				surface &&
				surface.contains(nextTarget)
			) {
				return;
			}
			disarmKeyboardCapture({ flushText: true });
		},
		[disarmKeyboardCapture],
	);
	const handleManualKeyboardKeyDown = useCallback(
		(event: KeyboardEvent<HTMLTextAreaElement>) => {
			if (!keyboardCaptureIsActive()) return;
			const nativeEvent = event.nativeEvent as globalThis.KeyboardEvent;
			if (
				nativeEvent.isComposing ||
				keyboardCaptureState === "composing" ||
				event.key === "Dead" ||
				event.key === "Process"
			) {
				cancelPrintableFallback();
				return;
			}

			const modifierKey = normalizeManualModifierKey(event.key);
			if (modifierKey) {
				event.preventDefault();
				event.stopPropagation();
				cancelPrintableFallback();
				flushTextBatch();
				if (!event.repeat) {
					sendManualInput({ action: "key_press", key: modifierKey });
				}
				return;
			}

			const printable = isManualPrintableKey(event.key);
			const hasShortcutModifier =
				event.metaKey ||
				event.ctrlKey ||
				event.altKey ||
				(event.shiftKey && !printable);
			if (hasShortcutModifier) {
				const target = normalizeManualShortcutTarget(event.key);
				if (!target) return;
				event.preventDefault();
				event.stopPropagation();
				cancelPrintableFallback();
				flushTextBatch();
				const keys = [...manualKeyboardModifiers(event), target].filter(
					(key, index, values) => values.indexOf(key) === index,
				);
				sendManualInput({ action: "hotkey", keys });
				return;
			}

			if (printable) {
				queuePrintableFallback(event.key);
				return;
			}

			const key = normalizeManualKeyPress(event.key);
			if (!key) return;
			event.preventDefault();
			event.stopPropagation();
			cancelPrintableFallback();
			flushTextBatch();
			sendManualInput({ action: "key_press", key });
		},
		[
			cancelPrintableFallback,
			flushTextBatch,
			keyboardCaptureIsActive,
			keyboardCaptureState,
			queuePrintableFallback,
			sendManualInput,
		],
	);
	const pointFromPointerEvent = useCallback(
		(event: PointerEvent): DesktopInputPoint | null => {
			const image = imageRef.current;
			const video = videoRef.current;
			const target = image ?? video;
			const sourceWidth = image?.naturalWidth ?? video?.videoWidth ?? 0;
			const sourceHeight = image?.naturalHeight ?? video?.videoHeight ?? 0;
			if (!target || sourceWidth <= 0 || sourceHeight <= 0) return null;
			const rect = target.getBoundingClientRect();
			const contentRect = desktopMediaContentRect(
				rect,
				sourceWidth,
				sourceHeight,
				shouldRotateDesktopContent,
			);
			if (!contentRect) return null;
			const relativeX = (event.clientX - contentRect.left) / contentRect.width;
			const relativeY = (event.clientY - contentRect.top) / contentRect.height;
			if (relativeX < 0 || relativeY < 0 || relativeX > 1 || relativeY > 1) {
				return null;
			}
			if (shouldRotateDesktopContent) {
				return {
					x: Math.round(relativeY * sourceWidth),
					y: Math.round((1 - relativeX) * sourceHeight),
					sourceWidth,
					sourceHeight,
				};
			}
			return {
				x: Math.round(relativeX * sourceWidth),
				y: Math.round(relativeY * sourceHeight),
				sourceWidth,
				sourceHeight,
			};
		},
		[shouldRotateDesktopContent],
	);
	const showDesktopClickRipple = useCallback(
		(
			clientX: number,
			clientY: number,
			button: DesktopClickRipple["button"],
		) => {
			const surface = desktopSurfaceRef.current;
			if (!surface) return;
			const surfaceRect = surface.getBoundingClientRect();
			const id = nextClickRippleIdRef.current;
			nextClickRippleIdRef.current += 1;
			const timeout = window.setTimeout(() => {
				clickRippleTimeoutsRef.current.delete(timeout);
				setClickRipples((current) =>
					current.filter((ripple) => ripple.id !== id),
				);
			}, DESKTOP_CLICK_RIPPLE_MS);
			clickRippleTimeoutsRef.current.add(timeout);
			setClickRipples((current) => [
				...current.slice(-5),
				{
					button,
					id,
					x: clientX - surfaceRect.left,
					y: clientY - surfaceRect.top,
				},
			]);
		},
		[],
	);
	const sendManualPointerClick = useCallback(
		(
			click: DesktopManualPointerClick,
			point: DesktopInputPoint,
			options: { captureKeyboard?: boolean } = {},
		) => {
			if (options.captureKeyboard ?? true) {
				armKeyboardCapture();
			}
			showDesktopClickRipple(
				click.clientX,
				click.clientY,
				click.button === 2 ? "secondary" : "primary",
			);
			sendManualInput({
				action: click.button === 2 ? "mouse_right_click" : "mouse_click",
				x: point.x,
				y: point.y,
				sourceWidth: point.sourceWidth,
				sourceHeight: point.sourceHeight,
				button:
					click.button === 1 ? "middle" : click.button === 2 ? "right" : "left",
				clicks: click.clicks,
			});
		},
		[armKeyboardCapture, sendManualInput, showDesktopClickRipple],
	);
	const sendManualPointerDragMove = useCallback(
		(point: DesktopInputPoint) => {
			sendManualInput(
				{
					action: "mouse_drag_move",
					x: point.x,
					y: point.y,
					sourceWidth: point.sourceWidth,
					sourceHeight: point.sourceHeight,
					button: "left",
				},
				{ allowInactive: true, ordered: true },
			);
		},
		[sendManualInput],
	);
	const startManualPointerDrag = useCallback(
		(gesture: DesktopManualPointerGesture, targetPoint: DesktopInputPoint) => {
			if (gesture.dragStarted) return;
			armKeyboardCapture();
			gesture.dragStarted = true;
			gesture.lastDragMoveAt = Date.now();
			gesture.lastDragMovePoint = targetPoint;
			sendManualInput(
				{
					action: "mouse_down",
					x: gesture.startPoint.x,
					y: gesture.startPoint.y,
					sourceWidth: gesture.startPoint.sourceWidth,
					sourceHeight: gesture.startPoint.sourceHeight,
					button: "left",
				},
				{ ordered: true },
			);
			sendManualPointerDragMove(targetPoint);
		},
		[armKeyboardCapture, sendManualInput, sendManualPointerDragMove],
	);
	const continueManualPointerDrag = useCallback(
		(
			gesture: DesktopManualPointerGesture,
			targetPoint: DesktopInputPoint,
			options: { force?: boolean } = {},
		) => {
			if (!gesture.dragStarted) return;
			const now = Date.now();
			if (
				!options.force &&
				now - gesture.lastDragMoveAt < MANUAL_POINTER_MOVE_INTERVAL_MS
			) {
				return;
			}
			gesture.lastDragMoveAt = now;
			gesture.lastDragMovePoint = targetPoint;
			sendManualPointerDragMove(targetPoint);
		},
		[sendManualPointerDragMove],
	);
	const releaseManualPointerDrag = useCallback(
		(
			gesture: DesktopManualPointerGesture,
			targetPoint: DesktopInputPoint = gesture.latestPoint,
		) => {
			if (!gesture.dragStarted) return;
			if (
				!gesture.lastDragMovePoint ||
				gesture.lastDragMovePoint.x !== targetPoint.x ||
				gesture.lastDragMovePoint.y !== targetPoint.y
			) {
				continueManualPointerDrag(gesture, targetPoint, { force: true });
			}
			gesture.dragStarted = false;
			sendManualInput(
				{
					action: "mouse_up",
					x: targetPoint.x,
					y: targetPoint.y,
					sourceWidth: targetPoint.sourceWidth,
					sourceHeight: targetPoint.sourceHeight,
					button: "left",
				},
				{ allowInactive: true, ordered: true },
			);
		},
		[continueManualPointerDrag, sendManualInput],
	);
	useEffect(() => {
		releaseManualPointerDragRef.current = releaseManualPointerDrag;
		return () => {
			if (releaseManualPointerDragRef.current === releaseManualPointerDrag) {
				releaseManualPointerDragRef.current = null;
			}
		};
	}, [releaseManualPointerDrag]);
	const beginMobilePinchGesture = useCallback(() => {
		const [first, second] = Array.from(
			mobileTouchPointersRef.current.values(),
		).slice(0, 2);
		if (!first || !second) {
			mobilePinchGestureRef.current = null;
			return;
		}
		const distance = desktopMobileTouchDistance(first, second);
		if (distance <= 0) {
			mobilePinchGestureRef.current = null;
			return;
		}
		mobileTapPointerIdRef.current = null;
		mobilePinchGestureRef.current = {
			pointerIds: [first.pointerId, second.pointerId],
			startDistance: distance,
			startMidpoint: desktopMobileTouchMidpoint(first, second),
			startTransform: mobileViewportTransformRef.current,
		};
	}, []);
	const updateMobilePinchGesture = useCallback(() => {
		const gesture = mobilePinchGestureRef.current;
		const surface = desktopSurfaceRef.current;
		if (!gesture || !surface) return;
		const first = mobileTouchPointersRef.current.get(gesture.pointerIds[0]);
		const second = mobileTouchPointersRef.current.get(gesture.pointerIds[1]);
		if (!first || !second) return;
		const distance = desktopMobileTouchDistance(first, second);
		if (distance <= 0) return;
		const rect = surface.getBoundingClientRect();
		if (rect.width <= 0 || rect.height <= 0) return;
		const nextScale =
			gesture.startTransform.scale * (distance / gesture.startDistance);
		setClampedMobileViewportTransform(
			desktopMobilePinchTransform(
				rect,
				gesture,
				desktopMobileTouchMidpoint(first, second),
				nextScale,
			),
		);
	}, [setClampedMobileViewportTransform]);
	const handleMobileTouchPointerDown = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			try {
				event.currentTarget.setPointerCapture(event.pointerId);
			} catch {
				// Some embedded webviews report touch pointer ids before capture exists.
			}
			const pointer: DesktopMobileTouchPointer = {
				clientX: event.clientX,
				clientY: event.clientY,
				moved: false,
				pointerId: event.pointerId,
				startClientX: event.clientX,
				startClientY: event.clientY,
				startTransform: mobileViewportTransformRef.current,
			};
			mobileTouchPointersRef.current.set(event.pointerId, pointer);
			if (mobileTouchPointersRef.current.size === 1) {
				mobileTapPointerIdRef.current = manualActive ? event.pointerId : null;
			}
			if (mobileTouchPointersRef.current.size >= 2) {
				disarmKeyboardCapture({ flushText: true });
				if (hasVisibleDesktopMedia) beginMobilePinchGesture();
			} else if (!manualActive) {
				disarmKeyboardCapture({ flushText: true });
			}
		},
		[
			beginMobilePinchGesture,
			disarmKeyboardCapture,
			hasVisibleDesktopMedia,
			manualActive,
		],
	);
	const handleMobileTouchPointerMove = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			const pointer = mobileTouchPointersRef.current.get(event.pointerId);
			if (!pointer) return;
			pointer.clientX = event.clientX;
			pointer.clientY = event.clientY;
			if (
				Math.hypot(
					pointer.clientX - pointer.startClientX,
					pointer.clientY - pointer.startClientY,
				) > DESKTOP_POINTER_TAP_SLOP_PX
			) {
				pointer.moved = true;
				if (mobileTapPointerIdRef.current === pointer.pointerId) {
					mobileTapPointerIdRef.current = null;
				}
			}
			if (mobilePinchGestureRef.current) {
				updateMobilePinchGesture();
				return;
			}
			if (mobileTouchPointersRef.current.size >= 2) {
				if (hasVisibleDesktopMedia) {
					beginMobilePinchGesture();
					updateMobilePinchGesture();
				}
				return;
			}
			if (
				pointer.moved &&
				mobileViewportTransformRef.current.scale >
					DESKTOP_MOBILE_ZOOM_MIN + 0.001
			) {
				setClampedMobileViewportTransform({
					scale: pointer.startTransform.scale,
					x: pointer.startTransform.x + event.clientX - pointer.startClientX,
					y: pointer.startTransform.y + event.clientY - pointer.startClientY,
				});
			}
		},
		[
			beginMobilePinchGesture,
			hasVisibleDesktopMedia,
			setClampedMobileViewportTransform,
			updateMobilePinchGesture,
		],
	);
	const handleMobileTouchPointerEnd = useCallback(
		(event: PointerEvent<HTMLElement>, cancelled = false) => {
			const pointer = mobileTouchPointersRef.current.get(event.pointerId);
			if (!pointer) return;
			try {
				event.currentTarget.releasePointerCapture(event.pointerId);
			} catch {
				// Capture can already be released by the webview after touch cancel.
			}
			const wasPinching = Boolean(mobilePinchGestureRef.current);
			const shouldTap =
				!cancelled &&
				!wasPinching &&
				manualActive &&
				mobileTapPointerIdRef.current === event.pointerId &&
				!pointer.moved;
			mobileTouchPointersRef.current.delete(event.pointerId);
			if (wasPinching) {
				mobileTapPointerIdRef.current = null;
				if (
					mobileTouchPointersRef.current.size >= 2 &&
					hasVisibleDesktopMedia
				) {
					beginMobilePinchGesture();
				} else {
					mobilePinchGestureRef.current = null;
					const [remainingPointer] = mobileTouchPointersRef.current.values();
					if (remainingPointer) {
						remainingPointer.startClientX = remainingPointer.clientX;
						remainingPointer.startClientY = remainingPointer.clientY;
						remainingPointer.startTransform =
							mobileViewportTransformRef.current;
						remainingPointer.moved = true;
					}
				}
				return;
			}
			if (mobileTouchPointersRef.current.size >= 2 && hasVisibleDesktopMedia) {
				beginMobilePinchGesture();
			} else {
				mobilePinchGestureRef.current = null;
			}
			if (mobileTapPointerIdRef.current === event.pointerId) {
				mobileTapPointerIdRef.current = null;
			}
			if (!shouldTap) return;
			const point = pointFromPointerEvent(event);
			if (!point) {
				disarmKeyboardCapture();
				return;
			}
			sendManualPointerClick(
				{
					button: event.button,
					clientX: event.clientX,
					clientY: event.clientY,
					clicks: event.detail > 1 ? 2 : 1,
				},
				point,
				{ captureKeyboard: false },
			);
		},
		[
			beginMobilePinchGesture,
			disarmKeyboardCapture,
			hasVisibleDesktopMedia,
			pointFromPointerEvent,
			sendManualPointerClick,
			manualActive,
		],
	);
	const handlePointerDown = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (presentation.isMobile && event.pointerType === "touch") {
				handleMobileTouchPointerDown(event);
				return;
			}
			if (!manualActive) {
				clearManualPointerGesture();
				disarmKeyboardCapture();
				return;
			}
			if (event.button > 2) {
				clearManualPointerGesture();
				disarmKeyboardCapture();
				return;
			}
			const point = pointFromPointerEvent(event);
			if (!point) {
				clearManualPointerGesture();
				disarmKeyboardCapture();
				return;
			}
			event.preventDefault();
			clearManualPointerGesture();
			try {
				event.currentTarget.setPointerCapture(event.pointerId);
			} catch {
				// Embedded webviews can occasionally deny capture during teardown.
			}
			manualPointerGestureRef.current = {
				button: event.button,
				clientX: event.clientX,
				clientY: event.clientY,
				clicks: event.detail > 1 ? 2 : 1,
				dragStarted: false,
				dragging: false,
				lastDragMoveAt: 0,
				lastDragMovePoint: null,
				latestPoint: point,
				pointerId: event.pointerId,
				startClientX: event.clientX,
				startClientY: event.clientY,
				startPoint: point,
			};
		},
		[
			clearManualPointerGesture,
			disarmKeyboardCapture,
			handleMobileTouchPointerDown,
			manualActive,
			pointFromPointerEvent,
			presentation.isMobile,
		],
	);
	const handlePointerMove = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (presentation.isMobile && event.pointerType === "touch") {
				handleMobileTouchPointerMove(event);
				return;
			}
			const gesture = manualPointerGestureRef.current;
			if (gesture?.pointerId === event.pointerId) {
				event.preventDefault();
				const movedBeyondSlop =
					Math.hypot(
						event.clientX - gesture.startClientX,
						event.clientY - gesture.startClientY,
					) > DESKTOP_POINTER_TAP_SLOP_PX;
				if (movedBeyondSlop) {
					gesture.dragging = true;
				}
				const point = pointFromPointerEvent(event);
				if (point) gesture.latestPoint = point;
				const targetPoint = point ?? gesture.latestPoint;
				if (gesture.button === 0 && gesture.dragging) {
					if (gesture.dragStarted) {
						continueManualPointerDrag(gesture, targetPoint);
					} else {
						startManualPointerDrag(gesture, targetPoint);
					}
				}
				return;
			}
			if (!manualActive) return;
			const now = Date.now();
			if (
				now - lastPointerMoveAtRef.current <
				MANUAL_POINTER_MOVE_INTERVAL_MS
			) {
				return;
			}
			const point = pointFromPointerEvent(event);
			if (!point) return;
			event.preventDefault();
			lastPointerMoveAtRef.current = now;
			sendManualInput({
				action: "mouse_move",
				x: point.x,
				y: point.y,
				sourceWidth: point.sourceWidth,
				sourceHeight: point.sourceHeight,
			});
		},
		[
			continueManualPointerDrag,
			handleMobileTouchPointerMove,
			manualActive,
			pointFromPointerEvent,
			presentation.isMobile,
			sendManualInput,
			startManualPointerDrag,
		],
	);
	const handlePointerUp = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (presentation.isMobile && event.pointerType === "touch") {
				handleMobileTouchPointerEnd(event);
				return;
			}
			const gesture = manualPointerGestureRef.current;
			if (!gesture || gesture.pointerId !== event.pointerId) return;
			event.preventDefault();
			manualPointerGestureRef.current = null;
			try {
				event.currentTarget.releasePointerCapture(event.pointerId);
			} catch {
				// Capture can already be gone after browser-level cancellation.
			}
			const releasePoint = pointFromPointerEvent(event);
			const targetPoint = releasePoint ?? gesture.latestPoint;
			const movedBeyondSlop =
				gesture.dragging ||
				Math.hypot(
					event.clientX - gesture.startClientX,
					event.clientY - gesture.startClientY,
				) > DESKTOP_POINTER_TAP_SLOP_PX;
			if (!movedBeyondSlop) {
				sendManualPointerClick(
					{
						button: gesture.button,
						clientX: gesture.startClientX,
						clientY: gesture.startClientY,
						clicks: gesture.clicks,
					},
					gesture.startPoint,
				);
				return;
			}
			if (gesture.button !== 0) return;
			if (!gesture.dragStarted) {
				startManualPointerDrag(gesture, targetPoint);
			}
			releaseManualPointerDrag(gesture, targetPoint);
		},
		[
			handleMobileTouchPointerEnd,
			pointFromPointerEvent,
			presentation.isMobile,
			releaseManualPointerDrag,
			sendManualPointerClick,
			startManualPointerDrag,
		],
	);
	const handlePointerCancel = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (presentation.isMobile && event.pointerType === "touch") {
				handleMobileTouchPointerEnd(event, true);
				return;
			}
			if (manualPointerGestureRef.current?.pointerId !== event.pointerId) {
				return;
			}
			event.preventDefault();
			clearManualPointerGesture();
		},
		[
			clearManualPointerGesture,
			handleMobileTouchPointerEnd,
			presentation.isMobile,
		],
	);
	const handleLostPointerCapture = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			const gesture = manualPointerGestureRef.current;
			if (gesture?.pointerId === event.pointerId) {
				manualPointerGestureRef.current = null;
				if (gesture.dragStarted) {
					releaseManualPointerDrag(gesture);
				}
			}
		},
		[releaseManualPointerDrag],
	);
	const handleWheel = useCallback(
		(event: WheelEvent<HTMLElement>) => {
			if (!manualActive) return;
			const deltaX = Math.round(event.deltaX);
			const deltaY = Math.round(event.deltaY);
			if (deltaX === 0 && deltaY === 0) return;
			event.preventDefault();
			sendManualInput({
				action: "scroll",
				deltaX,
				deltaY,
			});
		},
		[manualActive, sendManualInput],
	);
	const clampFloatingBarPosition = useCallback(
		(
			position: DesktopControlBarPosition,
			bar: HTMLElement | null,
		): DesktopControlBarPosition => {
			const surface = desktopSurfaceRef.current;
			if (!surface || !bar) return position;
			const maxX = Math.max(
				DESKTOP_CONTROL_BAR_MARGIN,
				surface.clientWidth - bar.offsetWidth - DESKTOP_CONTROL_BAR_MARGIN,
			);
			const maxY = Math.max(
				DESKTOP_CONTROL_BAR_MARGIN,
				surface.clientHeight - bar.offsetHeight - DESKTOP_CONTROL_BAR_MARGIN,
			);
			return {
				x: Math.min(maxX, Math.max(DESKTOP_CONTROL_BAR_MARGIN, position.x)),
				y: Math.min(maxY, Math.max(DESKTOP_CONTROL_BAR_MARGIN, position.y)),
			};
		},
		[],
	);
	const clampControlBarPosition = useCallback(
		(position: DesktopControlBarPosition): DesktopControlBarPosition =>
			clampFloatingBarPosition(position, controlBarRef.current),
		[clampFloatingBarPosition],
	);
	const clampPromptBarPosition = useCallback(
		(position: DesktopControlBarPosition): DesktopControlBarPosition =>
			clampFloatingBarPosition(position, promptBarRef.current),
		[clampFloatingBarPosition],
	);
	const startControlBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			if (event.button !== 0) return;
			const surface = desktopSurfaceRef.current;
			const controlBar = controlBarRef.current;
			if (!surface || !controlBar) return;
			const surfaceRect = surface.getBoundingClientRect();
			const controlBarRect = controlBar.getBoundingClientRect();
			const origin = clampControlBarPosition({
				x: controlBarRect.left - surfaceRect.left,
				y: controlBarRect.top - surfaceRect.top,
			});
			controlBarDragRef.current = {
				pointerId: event.pointerId,
				startClientX: event.clientX,
				startClientY: event.clientY,
				originX: origin.x,
				originY: origin.y,
			};
			setControlBarPosition(origin);
			event.currentTarget.setPointerCapture(event.pointerId);
			event.preventDefault();
			event.stopPropagation();
		},
		[clampControlBarPosition],
	);
	const moveControlBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			const drag = controlBarDragRef.current;
			if (!drag || drag.pointerId !== event.pointerId) return;
			setControlBarPosition(
				clampControlBarPosition({
					x: drag.originX + event.clientX - drag.startClientX,
					y: drag.originY + event.clientY - drag.startClientY,
				}),
			);
			event.preventDefault();
			event.stopPropagation();
		},
		[clampControlBarPosition],
	);
	const endControlBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			const drag = controlBarDragRef.current;
			if (!drag || drag.pointerId !== event.pointerId) return;
			controlBarDragRef.current = null;
			event.currentTarget.releasePointerCapture(event.pointerId);
			event.preventDefault();
			event.stopPropagation();
		},
		[],
	);
	const startPromptBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			if (event.button !== 0) return;
			const surface = desktopSurfaceRef.current;
			const promptBar = promptBarRef.current;
			if (!surface || !promptBar) return;
			const surfaceRect = surface.getBoundingClientRect();
			const promptBarRect = promptBar.getBoundingClientRect();
			const origin = clampPromptBarPosition({
				x: promptBarRect.left - surfaceRect.left,
				y: promptBarRect.top - surfaceRect.top,
			});
			promptBarDragRef.current = {
				pointerId: event.pointerId,
				startClientX: event.clientX,
				startClientY: event.clientY,
				originX: origin.x,
				originY: origin.y,
			};
			setPromptBarPosition(origin);
			event.currentTarget.setPointerCapture(event.pointerId);
			event.preventDefault();
			event.stopPropagation();
		},
		[clampPromptBarPosition],
	);
	const movePromptBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			const drag = promptBarDragRef.current;
			if (!drag || drag.pointerId !== event.pointerId) return;
			setPromptBarPosition(
				clampPromptBarPosition({
					x: drag.originX + event.clientX - drag.startClientX,
					y: drag.originY + event.clientY - drag.startClientY,
				}),
			);
			event.preventDefault();
			event.stopPropagation();
		},
		[clampPromptBarPosition],
	);
	const endPromptBarDrag = useCallback(
		(event: PointerEvent<HTMLButtonElement>) => {
			const drag = promptBarDragRef.current;
			if (!drag || drag.pointerId !== event.pointerId) return;
			promptBarDragRef.current = null;
			event.currentTarget.releasePointerCapture(event.pointerId);
			event.preventDefault();
			event.stopPropagation();
		},
		[],
	);
	const controlBarStyle: CSSProperties = controlBarPosition
		? {
				left: controlBarPosition.x,
				top: controlBarPosition.y,
			}
		: shouldRotateDesktopContent
			? {
					right: `calc(env(safe-area-inset-right) + ${DESKTOP_CONTROL_BAR_MARGIN}px)`,
					top: "50%",
					transform: "translateY(-50%)",
				}
			: {
					left: "50%",
					top: presentation.isMobile
						? `calc(env(safe-area-inset-top) + ${DESKTOP_CONTROL_BAR_MARGIN}px)`
						: DESKTOP_CONTROL_BAR_DEFAULT_TOP,
					transform: "translateX(-50%)",
				};
	const promptBarStyle: CSSProperties = {
		width: "min(28rem, calc(100% - 24px))",
		...(promptBarPosition
			? {
					left: promptBarPosition.x,
					top: promptBarPosition.y,
				}
			: {
					bottom: `calc(env(safe-area-inset-bottom) + ${DESKTOP_PROMPT_BAR_DEFAULT_BOTTOM}px)`,
					left: "50%",
					transform: "translateX(-50%)",
				}),
	};
	const desktopMediaClassName = cn(
		"block min-h-0 min-w-0 object-contain",
		shouldRotateDesktopContent
			? "absolute left-1/2 top-1/2 max-w-none -translate-x-1/2 -translate-y-1/2 rotate-90"
			: "h-full w-full max-h-full max-w-full",
	);
	const desktopMediaStyle: CSSProperties | undefined =
		shouldRotateDesktopContent
			? desktopSurfaceSize
				? {
						height: desktopSurfaceSize.width,
						width: desktopSurfaceSize.height,
					}
				: {
						height: "100dvw",
						width: "100dvh",
					}
			: undefined;
	const desktopMediaLayerStyle: CSSProperties | undefined =
		presentation.isMobile && hasVisibleDesktopMedia
			? {
					transform: `translate3d(${mobileViewportTransform.x}px, ${mobileViewportTransform.y}px, 0) scale(${mobileViewportTransform.scale})`,
					transformOrigin: "center center",
					willChange:
						mobileViewportTransform.scale > DESKTOP_MOBILE_ZOOM_MIN + 0.001
							? "transform"
							: undefined,
				}
			: undefined;
	const toolbarButtonClassName = cn(
		DESKTOP_TOOLBAR_BUTTON_CLASS,
		presentation.isMobile && "h-8 w-8 px-0",
	);
	const toolbarDangerButtonClassName = cn(
		DESKTOP_TOOLBAR_DANGER_BUTTON_CLASS,
		presentation.isMobile && "h-8 w-8 px-0",
	);
	const toolbarIconClassName = cn(
		"h-3.5 w-3.5 transition-transform",
		shouldRotateDesktopContent && "rotate-90",
	);
	const toolbarLabelClassName = presentation.isMobile ? "sr-only" : undefined;
	const showMobilePrompt = presentation.isMobile && hasVisibleDesktopMedia;
	const toolbarWorking = isAgentWorking || toolbarPromptPending;
	const activeManualControlId = manualControlIdRef.current ?? manualControlId;
	const canCaptureKeyboard = manualActive && Boolean(activeManualControlId);
	const keyboardCaptured =
		canCaptureKeyboard &&
		keyboardCaptureState !== "inactive" &&
		keyboardCaptureManualControlIdRef.current === activeManualControlId;

	return (
		<>
			{presentation.isMobile ? (
				<div className="sr-only">
					<DialogTitle>Desktop controls</DialogTitle>
					<DialogDescription>
						View and control the paired desktop for this Computer Use session.
					</DialogDescription>
				</div>
			) : null}
			<div className="h-full w-full min-h-0 min-w-0 overflow-hidden">
				<section
					ref={desktopSurfaceRef}
					aria-label="Desktop"
					tabIndex={manualActive ? 0 : -1}
					onPointerDown={handlePointerDown}
					onPointerMove={handlePointerMove}
					onPointerUp={handlePointerUp}
					onPointerCancel={handlePointerCancel}
					onLostPointerCapture={handleLostPointerCapture}
					onWheel={handleWheel}
					className={cn(
						"relative flex h-full w-full min-h-0 min-w-0 overflow-hidden bg-zinc-950 outline-none",
						presentation.isMobile && "touch-none select-none",
						keyboardCaptured && "ring-2 ring-primary/40 ring-inset",
					)}
				>
					<textarea
						ref={keyboardSinkRef}
						aria-label="Desktop keyboard passthrough"
						autoCapitalize="off"
						autoComplete="off"
						autoCorrect="off"
						spellCheck={false}
						tabIndex={-1}
						onBeforeInput={handleKeyboardBeforeInput}
						onBlur={handleKeyboardSinkBlur}
						onCompositionEnd={handleKeyboardCompositionEnd}
						onCompositionStart={handleKeyboardCompositionStart}
						onInput={handleKeyboardInput}
						onKeyDown={handleManualKeyboardKeyDown}
						onPaste={handleKeyboardPaste}
						className="pointer-events-none absolute left-0 top-0 z-0 h-px w-px resize-none border-0 bg-transparent p-0 opacity-0 outline-none"
					/>
					<div
						className="desktop-control-media-layer relative z-0 flex h-full w-full min-h-0 min-w-0 flex-1 basis-full items-center justify-center overflow-hidden bg-zinc-950 text-zinc-100 [contain:layout_paint]"
						style={desktopMediaLayerStyle}
					>
						{hasLiveVideo ? (
							<video
								ref={videoRef}
								autoPlay
								muted
								playsInline
								onLoadedData={markDesktopVideoFrame}
								onPlaying={markDesktopVideoFrame}
								onTimeUpdate={markDesktopVideoFrame}
								className={desktopMediaClassName}
								style={desktopMediaStyle}
							/>
						) : resolvedPreviewUrl ? (
							<img
								ref={imageRef}
								src={resolvedPreviewUrl}
								alt="Desktop"
								className={desktopMediaClassName}
								style={desktopMediaStyle}
								draggable={false}
							/>
						) : (
							<DesktopViewportEmptyState
								description={streamFailureMessage}
								state={state}
								status={status}
							/>
						)}
					</div>
					{clickRipples.length > 0 ? (
						<div
							className="pointer-events-none absolute inset-0 z-10 overflow-hidden"
							aria-hidden="true"
						>
							{clickRipples.map((ripple) => (
								<span
									key={ripple.id}
									className={cn(
										"desktop-click-ripple",
										ripple.button === "secondary" &&
											"desktop-click-ripple-secondary",
									)}
									style={{
										left: ripple.x,
										top: ripple.y,
									}}
								/>
							))}
						</div>
					) : null}
					<div
						ref={controlBarRef}
						role="toolbar"
						aria-label="Desktop stream controls"
						style={controlBarStyle}
						onFocusCapture={() => disarmKeyboardCapture({ flushText: true })}
						onPointerDown={(event) => {
							disarmKeyboardCapture({ flushText: true });
							event.stopPropagation();
						}}
						onKeyDown={(event) => event.stopPropagation()}
						onWheel={(event) => event.stopPropagation()}
						aria-busy={toolbarWorking || undefined}
						className={cn(
							"absolute z-20 flex items-center gap-1 rounded-xl border border-white/10 bg-background/70 p-1 text-foreground shadow-[0_18px_45px_rgba(0,0,0,0.45)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/55",
							shouldRotateDesktopContent && "flex-col",
							toolbarWorking && "desktop-control-toolbar-working",
						)}
					>
						<button
							type="button"
							aria-label="Move desktop controls"
							className="flex h-7 w-6 touch-none cursor-grab items-center justify-center rounded-md text-muted-foreground hover:bg-white/10 hover:text-foreground active:cursor-grabbing"
							onPointerDown={startControlBarDrag}
							onPointerMove={moveControlBarDrag}
							onPointerUp={endControlBarDrag}
							onPointerCancel={endControlBarDrag}
						>
							<GripVertical
								className={toolbarIconClassName}
								aria-hidden="true"
							/>
						</button>
						<div
							className={cn(
								"bg-border/60",
								shouldRotateDesktopContent ? "h-px w-5" : "h-5 w-px",
							)}
							aria-hidden="true"
						/>
						{keyboardCaptured && !presentation.isMobile ? (
							<Badge
								variant="secondary"
								className="h-7 rounded-md border border-emerald-400/25 bg-emerald-500/15 px-2 text-[11px] font-medium text-emerald-100"
							>
								Keyboard captured
							</Badge>
						) : null}
						<Button
							type="button"
							size="sm"
							variant="ghost"
							className={toolbarButtonClassName}
							disabled={!canSend || state === "connecting"}
							onClick={startStream}
						>
							<Monitor className={toolbarIconClassName} aria-hidden="true" />
							<span className={toolbarLabelClassName}>Start</span>
						</Button>
						<Button
							type="button"
							size="sm"
							variant="ghost"
							className={toolbarButtonClassName}
							disabled={!canUseManualControl}
							onClick={manualActive ? releaseManual : requestManual}
						>
							<MousePointer2
								className={toolbarIconClassName}
								aria-hidden="true"
							/>
							<span className={toolbarLabelClassName}>
								{manualControlButtonLabel(manualState)}
							</span>
						</Button>
						{presentation.isMobile ? (
							<Button
								type="button"
								size="sm"
								variant="ghost"
								aria-label={
									keyboardCaptured
										? "Hide desktop keyboard"
										: "Show desktop keyboard"
								}
								aria-pressed={keyboardCaptured}
								className={toolbarButtonClassName}
								disabled={!canCaptureKeyboard}
								onClick={() => {
									if (keyboardCaptured) {
										disarmKeyboardCapture({ flushText: true });
									} else {
										armKeyboardCapture();
									}
								}}
							>
								<Keyboard className={toolbarIconClassName} aria-hidden="true" />
								<span className={toolbarLabelClassName}>Keyboard</span>
							</Button>
						) : null}
						<Button
							type="button"
							size="sm"
							variant="ghost"
							className={toolbarDangerButtonClassName}
							disabled={!canStopDesktop}
							onClick={stopStream}
						>
							<Square className={toolbarIconClassName} aria-hidden="true" />
							<span className={toolbarLabelClassName}>Stop</span>
						</Button>
						{closeControl ? (
							<>
								<div
									className={cn(
										"bg-border/60",
										shouldRotateDesktopContent ? "h-px w-5" : "h-5 w-px",
									)}
									aria-hidden="true"
								/>
								<span
									className={cn(
										"inline-flex",
										shouldRotateDesktopContent && "[&_svg]:rotate-90",
									)}
								>
									{closeControl}
								</span>
							</>
						) : null}
					</div>
					{showMobilePrompt ? (
						<div
							ref={promptBarRef}
							role="toolbar"
							aria-label="Desktop prompt controls"
							style={promptBarStyle}
							onFocusCapture={() => disarmKeyboardCapture({ flushText: true })}
							onPointerDown={(event) => {
								disarmKeyboardCapture({ flushText: true });
								event.stopPropagation();
							}}
							onKeyDown={(event) => event.stopPropagation()}
							onWheel={(event) => event.stopPropagation()}
							aria-busy={toolbarWorking || undefined}
							className={cn(
								"absolute z-20 flex items-center gap-1 rounded-xl border border-white/10 bg-background/70 p-1 text-foreground shadow-[0_18px_45px_rgba(0,0,0,0.45)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/55",
								toolbarWorking && "desktop-control-toolbar-working",
							)}
						>
							<button
								type="button"
								aria-label="Move desktop prompt controls"
								className="flex h-8 w-7 touch-none cursor-grab items-center justify-center rounded-md text-muted-foreground hover:bg-white/10 hover:text-foreground active:cursor-grabbing"
								onPointerDown={startPromptBarDrag}
								onPointerMove={movePromptBarDrag}
								onPointerUp={endPromptBarDrag}
								onPointerCancel={endPromptBarDrag}
							>
								<GripVertical
									className={toolbarIconClassName}
									aria-hidden="true"
								/>
							</button>
							<div className="h-6 w-px bg-border/60" aria-hidden="true" />
							<DesktopToolbarPromptForm
								canSend={canSend}
								isAgentWorking={isAgentWorking}
								onPromptAccepted={() => setToolbarPromptPending(true)}
								onSubmit={onPromptSubmit}
								rotated={false}
							/>
						</div>
					) : null}
				</section>
			</div>
		</>
	);
}

export interface DesktopToolbarPromptFormProps {
	canSend: boolean;
	isAgentWorking: boolean;
	onPromptAccepted?: () => void;
	onSubmit: (message: string) => void;
	rotated: boolean;
}

export function DesktopToolbarPromptForm({
	canSend,
	isAgentWorking,
	onPromptAccepted,
	onSubmit,
	rotated,
}: DesktopToolbarPromptFormProps) {
	const inputId = useId();
	const [prompt, setPrompt] = useState("");
	const submitDisabled =
		!canSend || isAgentWorking || prompt.trim().length === 0;

	const handleSubmit = useCallback(
		(event: FormEvent<HTMLFormElement>) => {
			event.preventDefault();
			event.stopPropagation();
			const message = prompt.trim();
			if (!message || !canSend || isAgentWorking) return;
			onSubmit(message);
			setPrompt("");
			onPromptAccepted?.();
		},
		[canSend, isAgentWorking, onPromptAccepted, onSubmit, prompt],
	);

	const form = (
		<form
			aria-label="Computer Use prompt"
			className={cn(
				"desktop-control-mobile-prompt flex min-w-0 items-center gap-1 rounded-lg border border-white/10 bg-black/25 px-1.5 py-1 shadow-inner shadow-black/20",
				rotated
					? "desktop-control-mobile-prompt-rotated absolute left-1/2 top-1/2 h-8 w-[clamp(9rem,34dvh,18rem)] -translate-x-1/2 -translate-y-1/2 rotate-90"
					: "flex-1",
			)}
			onSubmit={handleSubmit}
			onPointerDown={(event) => event.stopPropagation()}
			onKeyDown={(event) => event.stopPropagation()}
			onWheel={(event) => event.stopPropagation()}
		>
			<label className="sr-only" htmlFor={inputId}>
				Tell Computer Use what to do
			</label>
			<input
				id={inputId}
				type="text"
				value={prompt}
				disabled={!canSend}
				onChange={(event) => setPrompt(event.currentTarget.value)}
				placeholder={
					isAgentWorking ? "Xero is working..." : "Tell Xero what to do..."
				}
				className="h-7 min-w-0 flex-1 bg-transparent px-1 text-[12px] text-foreground outline-none placeholder:text-muted-foreground/70 disabled:cursor-not-allowed disabled:opacity-50"
			/>
			<Button
				type="submit"
				size="icon"
				variant="ghost"
				aria-label="Send Computer Use prompt"
				disabled={submitDisabled}
				className="h-7 w-7 shrink-0 rounded-md text-primary hover:bg-primary/10 hover:text-primary disabled:opacity-35"
			>
				<SendHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
			</Button>
		</form>
	);

	if (rotated) {
		return (
			<div className="desktop-control-mobile-prompt-slot relative h-[clamp(9rem,34dvh,18rem)] w-8 shrink-0 overflow-hidden">
				{form}
			</div>
		);
	}

	return form;
}

function DesktopViewportEmptyState({
	description,
	state,
	status,
}: {
	description?: string | null;
	state: DesktopViewportState;
	status: string;
}) {
	const isLoading = state === "connecting";

	return (
		<div className="relative flex max-w-sm flex-col items-center px-6 text-center">
			{isLoading ? (
				<div className="cloud-halo-soft relative flex size-20 items-center justify-center">
					<span
						aria-hidden="true"
						className="absolute inset-0 rounded-full xero-loading-ring"
						style={{
							border:
								"1px solid color-mix(in oklab, var(--primary) 42%, transparent)",
						}}
					/>
					<BrandLogo className="h-7 w-7 xero-loading-breathe" />
				</div>
			) : (
				<div className="flex size-14 items-center justify-center rounded-xl border border-white/10 bg-white/[0.04] text-zinc-200 shadow-[0_18px_50px_rgba(0,0,0,0.35)]">
					<Monitor className="h-6 w-6" aria-hidden="true" />
				</div>
			)}
			<h3 className="mt-4 text-[15px] font-semibold tracking-tight text-zinc-100">
				{status}
			</h3>
			<p className="mt-1.5 text-[12px] leading-relaxed text-zinc-400">
				{description ?? desktopViewportEmptyDescription(state)}
			</p>
			<div className="mt-4 inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.035] px-3 py-1 text-[11px] font-medium text-zinc-400">
				<span
					className="size-1.5 rounded-full bg-primary/70"
					aria-hidden="true"
				/>
				{desktopViewportEmptyBadge(state)}
			</div>
		</div>
	);
}

function desktopViewportEmptyDescription(state: DesktopViewportState): string {
	if (state === "offline")
		return "This computer is not connected to Xero Cloud.";
	if (state === "blocked")
		return "Stop the running connection in the other cloud app before using it here.";
	if (state === "failed") return "The desktop stream could not be started.";
	if (state === "connecting") return "Opening the live desktop stream.";
	if (state === "paused")
		return "The desktop stream is stopped for this session.";
	return "Start desktop viewing when you are ready.";
}

function desktopViewportEmptyBadge(state: DesktopViewportState): string {
	if (state === "offline") return "offline";
	if (state === "blocked") return "in use";
	if (state === "failed") return "error";
	if (state === "connecting") return "connecting";
	if (state === "paused") return "paused";
	return "ready";
}

function desktopViewportStatusLabel(
	state: DesktopViewportState,
	previewUrl: string | null,
	manualState: ManualControlState = "manual_idle",
): string {
	if (manualState === "manual_requesting") return "Requesting manual control";
	if (manualState === "manual_reconnecting") return "Recovering manual control";
	if (manualState === "manual_denied") return "Manual control denied";
	if (manualState === "manual_releasing") return "Releasing manual control";
	if (state === "offline") return "Device offline";
	if (state === "blocked") return "Already in use";
	if (state === "failed") return "Desktop unavailable";
	if (state === "connecting") return "Connecting stream";
	if (state === "live") return "Live stream";
	if (state === "manual") return "Manual control active";
	if (state === "paused") return "Stream paused";
	if (previewUrl) return "Stream degraded";
	return "Waiting for desktop";
}

function manualControlButtonLabel(state: ManualControlState): string {
	if (state === "manual_active") return "Release";
	if (state === "manual_requesting") return "Requesting";
	if (state === "manual_reconnecting") return "Recovering";
	if (state === "manual_releasing") return "Releasing";
	if (state === "manual_denied") return "Retry";
	return "Manual";
}

function computerUseCommandSucceeded(
	payload: ComputerUseDesktopPayload,
): boolean {
	if (payload.ok === false) return false;
	return (
		payload.outcome !== "rejected" &&
		payload.outcome !== "rate_limited" &&
		payload.outcome !== "timed_out" &&
		payload.outcome !== "stale"
	);
}

function isCommandAckResult(value: unknown): value is CommandAckResult {
	if (!value || typeof value !== "object") return false;
	const payload = value as Partial<CommandAckResult>;
	return (
		payload.schema === "xero.remote_command_outcome.v1" &&
		typeof payload.clientCommandId === "string" &&
		typeof payload.kind === "string" &&
		typeof payload.outcome === "string"
	);
}

function isRemoteCommandResultPayload(
	value: unknown,
): value is RemoteCommandResultPayload {
	if (isCommandAckResult(value)) return true;
	if (!value || typeof value !== "object") return false;
	const payload = value as Partial<RemoteCommandExecutionResultPayload>;
	return payload.schema === "xero.remote_command_result.v1";
}

function remoteCommandResultFailed(
	payload: RemoteCommandResultPayload,
): boolean {
	if (payload.schema === "xero.remote_command_result.v1") {
		return payload.ok === false;
	}
	return (
		payload.outcome === "rejected" ||
		payload.outcome === "rate_limited" ||
		payload.outcome === "timed_out" ||
		payload.outcome === "stale"
	);
}

function remoteCommandKind(payload: RemoteCommandResultPayload): string | null {
	return typeof payload.kind === "string" && payload.kind.length > 0
		? payload.kind
		: null;
}

function remoteCommandFailureReason(
	payload: RemoteCommandResultPayload,
): string | null {
	if (typeof payload.reason === "string" && payload.reason.length > 0) {
		return payload.reason;
	}
	if (
		payload.schema === "xero.remote_command_result.v1" &&
		typeof payload.error?.code === "string" &&
		payload.error.code.length > 0
	) {
		return payload.error.code;
	}
	return null;
}

function remoteCommandFailureMessage(
	payload: RemoteCommandResultPayload,
): string {
	if (
		payload.schema === "xero.remote_command_result.v1" &&
		typeof payload.error?.message === "string" &&
		payload.error.message.length > 0
	) {
		return payload.error.message;
	}
	if (typeof payload.message === "string" && payload.message.length > 0) {
		return payload.message;
	}
	if (
		remoteCommandFailureReason(payload) === REMOTE_CONTROL_ALREADY_ACTIVE_REASON
	) {
		return "Stop the running connection in the other cloud app before using it here.";
	}
	return "The desktop app rejected the stream command. Try starting the desktop stream again.";
}

function remoteControlConnectionAlreadyActive(
	result: CommandAckResult,
): boolean {
	return (
		result.reason === REMOTE_CONTROL_ALREADY_ACTIVE_REASON &&
		(result.outcome === "rejected" || result.outcome === "rate_limited")
	);
}

function desktopPeerConnectionNeedsMediaRecovery(
	peerConnection: RTCPeerConnection | null,
): boolean {
	if (!peerConnection) return true;
	return (
		peerConnection.connectionState === "failed" ||
		peerConnection.connectionState === "disconnected" ||
		peerConnection.connectionState === "closed" ||
		peerConnection.iceConnectionState === "failed" ||
		peerConnection.iceConnectionState === "disconnected" ||
		peerConnection.iceConnectionState === "closed"
	);
}

function queuePendingDesktopIceCandidate(
	pending: RTCIceCandidateInit[],
	candidate: RTCIceCandidateInit,
) {
	if (pending.length >= DESKTOP_STREAM_PENDING_ICE_CANDIDATES_MAX) {
		pending.shift();
	}
	pending.push(candidate);
}

async function flushPendingDesktopIceCandidates(
	peerConnection: RTCPeerConnection,
	pending: RTCIceCandidateInit[],
) {
	while (pending.length > 0) {
		const candidate = pending.shift();
		if (!candidate) continue;
		await peerConnection.addIceCandidate(candidate);
	}
}

function desktopStreamDetails(
	payload: ComputerUseDesktopPayload,
): DesktopStreamDetails | null {
	const stream = payload.desktop?.stream;
	if (!stream || typeof stream !== "object") return null;
	return {
		status: typeof stream.status === "string" ? stream.status : null,
		transport: typeof stream.transport === "string" ? stream.transport : null,
		quality: isDesktopStreamQuality(stream.quality) ? stream.quality : null,
		maxWidth: typeof stream.maxWidth === "number" ? stream.maxWidth : null,
		maxFrameRate:
			typeof stream.maxFrameRate === "number" ? stream.maxFrameRate : null,
		metrics: desktopStreamMetrics(stream.metrics),
		message: typeof stream.message === "string" ? stream.message : null,
	};
}

export function shouldRecoverDesktopWebRtcAfterFallback(
	stream: DesktopStreamDetails | null,
	liveVideoSeen: boolean,
): boolean {
	return liveVideoSeen && stream?.transport === "screenshot_fallback";
}

function desktopStreamMetrics(value: unknown): DesktopStreamMetrics | null {
	if (!value || typeof value !== "object") return null;
	const metrics = value as Record<string, unknown>;
	return {
		captureBackend: stringOrNull(metrics.captureBackend),
		encoderBackend: stringOrNull(metrics.encoderBackend),
		encoderHardware:
			typeof metrics.encoderHardware === "boolean"
				? metrics.encoderHardware
				: null,
		preferredCodec: stringOrNull(metrics.preferredCodec),
		fallbackReason: stringOrNull(metrics.fallbackReason),
		captureFrameRate: numberOrNull(metrics.captureFrameRate),
		captureDroppedFrames: numberOrNull(metrics.captureDroppedFrames),
		encodeFrameRate: numberOrNull(metrics.encodeFrameRate),
		encodeLatencyMs: numberOrNull(metrics.encodeLatencyMs),
		outboundBitrateBps: numberOrNull(metrics.outboundBitrateBps),
		availableOutgoingBitrateBps: numberOrNull(
			metrics.availableOutgoingBitrateBps,
		),
		packetsSent: numberOrNull(metrics.packetsSent),
		bytesSent: numberOrNull(metrics.bytesSent),
		packetLoss: numberOrNull(metrics.packetLoss),
		roundTripTimeMs: numberOrNull(metrics.roundTripTimeMs),
		retransmits: numberOrNull(metrics.retransmits),
		keyframes: numberOrNull(metrics.keyframes),
	};
}

function stringOrNull(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function numberOrNull(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function fallbackFrameIntervalMs(quality: DesktopStreamQuality): number {
	if (quality === "high") return 500;
	if (quality === "low") return 2_000;
	return 1_000;
}

function isDesktopStreamQuality(value: unknown): value is DesktopStreamQuality {
	return value === "low" || value === "balanced" || value === "high";
}

interface ComputerUseDesktopPayload {
	schema: string;
	ok?: boolean;
	outcome?: string | null;
	streamId?: string | null;
	manualControlId?: string | null;
	type?: string | null;
	sdp?: string | null;
	candidate?: RTCIceCandidateInit | string | null;
	payload?: DesktopStreamSignalPayload | null;
	desktop?: {
		stream?: {
			status?: unknown;
			transport?: unknown;
			quality?: unknown;
			maxWidth?: unknown;
			maxFrameRate?: unknown;
			metrics?: unknown;
			message?: unknown;
		} | null;
	} | null;
	desktopFrame?: {
		ok?: boolean;
		mediaType?: string | null;
		bytesBase64?: string | null;
	};
}

interface RemoteCommandExecutionResultPayload {
	schema: "xero.remote_command_result.v1";
	ok?: boolean;
	clientCommandId?: string | null;
	clientSeq?: number | null;
	kind?: string | null;
	outcome?: string | null;
	reason?: string | null;
	message?: string | null;
	error?: {
		code?: string | null;
		message?: string | null;
	} | null;
}

type RemoteCommandResultPayload =
	| CommandAckResult
	| RemoteCommandExecutionResultPayload;

interface DesktopStreamSignalPayload {
	type?: string | null;
	sdp?: string | null;
	candidate?: RTCIceCandidateInit | string | null;
}

interface DesktopFrameChunk {
	schema: "xero.desktop_stream_frame_chunk.v1";
	streamId: string;
	frameId: string;
	seq: number;
	total: number;
	mediaType: string;
	data: string;
}

interface DesktopFrameChunkBuffer {
	chunks: Array<string | undefined>;
	mediaType: string;
	received: number;
	total: number;
}

function isComputerUseDesktopPayload(
	value: unknown,
): value is ComputerUseDesktopPayload {
	if (!value || typeof value !== "object") return false;
	const payload = value as Partial<ComputerUseDesktopPayload>;
	return (
		typeof payload.schema === "string" &&
		(payload.schema.startsWith("xero.computer_use_stream_") ||
			payload.schema.startsWith("xero.computer_use_manual_control_"))
	);
}

function desktopStreamSessionDescription(
	payload: ComputerUseDesktopPayload,
	expectedType: RTCSdpType,
): RTCSessionDescriptionInit | null {
	const signal = desktopStreamSignalPayload(payload);
	const sdp = typeof signal.sdp === "string" ? signal.sdp : null;
	if (!sdp) return null;
	const type = isRtcSdpType(signal.type) ? signal.type : expectedType;
	return { type, sdp };
}

function desktopStreamIceCandidate(
	payload: ComputerUseDesktopPayload,
): RTCIceCandidateInit | null {
	const signal = desktopStreamSignalPayload(payload);
	const candidate = signal.candidate;
	if (typeof candidate === "string") return { candidate };
	if (
		candidate &&
		typeof candidate === "object" &&
		typeof candidate.candidate === "string"
	) {
		return candidate;
	}
	return null;
}

function desktopStreamSignalPayload(
	payload: ComputerUseDesktopPayload,
): DesktopStreamSignalPayload {
	if (payload.payload && typeof payload.payload === "object") {
		return payload.payload;
	}
	return payload;
}

function isRtcSdpType(value: unknown): value is RTCSdpType {
	return (
		value === "offer" ||
		value === "answer" ||
		value === "pranswer" ||
		value === "rollback"
	);
}

function desktopFrameChunkFromMessage(
	value: unknown,
): DesktopFrameChunk | null {
	if (typeof value !== "string") return null;
	let parsed: unknown;
	try {
		parsed = JSON.parse(value);
	} catch {
		return null;
	}
	if (!parsed || typeof parsed !== "object") return null;
	const chunk = parsed as Partial<DesktopFrameChunk>;
	if (chunk.schema !== "xero.desktop_stream_frame_chunk.v1") return null;
	if (
		typeof chunk.streamId !== "string" ||
		typeof chunk.frameId !== "string" ||
		typeof chunk.mediaType !== "string" ||
		typeof chunk.data !== "string"
	) {
		return null;
	}
	const seq = chunk.seq;
	const total = chunk.total;
	if (
		!Number.isInteger(seq) ||
		!Number.isInteger(total) ||
		typeof seq !== "number" ||
		typeof total !== "number" ||
		seq < 0 ||
		total <= 0 ||
		total > 4096 ||
		seq >= total
	) {
		return null;
	}
	return {
		schema: "xero.desktop_stream_frame_chunk.v1",
		streamId: chunk.streamId,
		frameId: chunk.frameId,
		seq,
		total,
		mediaType: chunk.mediaType,
		data: chunk.data,
	};
}

function pruneDesktopFrameBuffers(
	frames: Map<string, DesktopFrameChunkBuffer>,
): void {
	while (frames.size > DESKTOP_STREAM_MAX_BUFFERED_FRAMES) {
		const oldestFrameId = frames.keys().next().value;
		if (!oldestFrameId) return;
		frames.delete(oldestFrameId);
	}
}

function closeDesktopPeerConnection(
	peerConnection: RTCPeerConnection | null,
): void {
	if (!peerConnection) return;
	for (const receiver of peerConnection.getReceivers()) {
		receiver.track.stop();
	}
	peerConnection.close();
}

function useResolvedRemoteMedia({
	channel,
	computerId,
	deviceId,
	sessionId,
	turns,
}: {
	channel: Channel | null;
	computerId: string;
	deviceId?: string | null;
	sessionId: string;
	turns: readonly ConversationTurn[];
}): ConversationTurn[] {
	const requestedRef = useRef(new Set<string>());
	const objectUrlsRef = useRef(new Map<string, string>());
	const [resolvedUrls, setResolvedUrls] = useState<Record<string, string>>({});

	useEffect(() => {
		return () => {
			for (const url of objectUrlsRef.current.values()) {
				URL.revokeObjectURL(url);
			}
			objectUrlsRef.current.clear();
		};
	}, []);

	useEffect(() => {
		if (!channel) return;
		const ref = channel.on("frame", (rawFrame: unknown) => {
			const envelope = decodeRelayFrame(rawFrame);
			const payload = envelope?.payload;
			if (!isRuntimeMediaArtifactPayload(payload)) return;
			if (!payload.ok || !payload.bytesBase64) return;
			const bytes = base64ToBytes(payload.bytesBase64);
			const blobBytes = new Uint8Array(bytes);
			const url = URL.createObjectURL(
				new Blob([blobBytes.buffer], { type: payload.mediaType }),
			);
			const previousUrl = objectUrlsRef.current.get(payload.artifactId);
			if (previousUrl) URL.revokeObjectURL(previousUrl);
			objectUrlsRef.current.set(payload.artifactId, url);
			setResolvedUrls((current) => ({
				...current,
				[payload.artifactId]: url,
			}));
		});
		return () => {
			channel.off("frame", ref);
		};
	}, [channel]);

	useEffect(() => {
		if (!channel || !deviceId) return;
		for (const artifactId of collectMissingRemoteArtifactIds(
			turns,
			resolvedUrls,
		)) {
			if (requestedRef.current.has(artifactId)) continue;
			requestedRef.current.add(artifactId);
			requestRuntimeMediaArtifact(channel, {
				computerId,
				sessionId,
				deviceId,
				artifactId,
			});
		}
	}, [channel, computerId, deviceId, resolvedUrls, sessionId, turns]);

	return useMemo(
		() => applyResolvedRemoteMedia(turns, resolvedUrls),
		[resolvedUrls, turns],
	);
}

function collectMissingRemoteArtifactIds(
	turns: readonly ConversationTurn[],
	resolvedUrls: Record<string, string>,
): string[] {
	const ids = new Set<string>();
	for (const turn of turns) {
		for (const attachment of turnMediaAttachments(turn)) {
			const artifactId = remoteArtifactId(attachment.source);
			if (
				artifactId &&
				!attachmentPreviewAvailable(attachment) &&
				!resolvedUrls[artifactId]
			) {
				ids.add(artifactId);
			}
		}
	}
	return Array.from(ids);
}

function applyResolvedRemoteMedia(
	turns: readonly ConversationTurn[],
	resolvedUrls: Record<string, string>,
): ConversationTurn[] {
	return turns.map((turn) => {
		if (turn.kind === "message") {
			return {
				...turn,
				attachments: resolveAttachments(turn.attachments, resolvedUrls),
			};
		}
		if (turn.kind === "action") {
			return {
				...turn,
				mediaAttachments: resolveAttachments(
					turn.mediaAttachments,
					resolvedUrls,
				),
			};
		}
		if (turn.kind === "action_group") {
			return {
				...turn,
				actions: turn.actions.map((action) => ({
					...action,
					mediaAttachments: resolveAttachments(
						action.mediaAttachments,
						resolvedUrls,
					),
				})),
			};
		}
		if (turn.kind === "subagent_group") {
			return {
				...turn,
				children: applyResolvedRemoteMedia(turn.children, resolvedUrls),
			};
		}
		return turn;
	});
}

function resolveAttachments(
	attachments: ConversationMessageAttachment[] | undefined,
	resolvedUrls: Record<string, string>,
): ConversationMessageAttachment[] | undefined {
	if (!attachments?.length) return attachments;
	return attachments.map((attachment) => {
		if (attachment.source?.kind !== "remote_artifact") return attachment;
		const artifactId = remoteArtifactId(attachment.source);
		if (!artifactId) return attachment;
		const renderUrl = resolvedUrls[artifactId];
		if (!renderUrl) return attachment;
		return {
			...attachment,
			renderUrl,
			previewSrc: renderUrl,
		};
	});
}

function turnMediaAttachments(
	turn: ConversationTurn,
): ConversationMessageAttachment[] {
	if (turn.kind === "message") return turn.attachments ?? [];
	if (turn.kind === "action") return turn.mediaAttachments ?? [];
	if (turn.kind === "action_group") {
		return turn.actions.flatMap((action) => action.mediaAttachments ?? []);
	}
	if (turn.kind === "subagent_group") {
		return turn.children.flatMap(turnMediaAttachments);
	}
	return [];
}

function remoteArtifactId(
	source: ConversationMessageAttachment["source"],
): string | null {
	if (source?.kind !== "remote_artifact") return null;
	const sourceRecord = source as { artifactId?: unknown };
	const artifactId =
		typeof sourceRecord.artifactId === "string"
			? sourceRecord.artifactId.trim()
			: "";
	return artifactId.length > 0 ? artifactId : null;
}

function latestDesktopPreviewUrl(
	turns: readonly ConversationTurn[],
): string | null {
	for (const turn of [...turns].reverse()) {
		const attachments = turnMediaAttachments(turn);
		for (const attachment of [...attachments].reverse()) {
			const label =
				`${attachment.title ?? ""} ${attachment.alt ?? ""}`.toLowerCase();
			if (!label.includes("desktop")) continue;
			const url = attachment.previewSrc ?? attachment.renderUrl ?? null;
			if (url) return url;
		}
	}
	return null;
}

function attachmentPreviewAvailable(
	attachment: ConversationMessageAttachment,
): boolean {
	return Boolean(attachment.previewSrc || attachment.renderUrl);
}

interface RuntimeMediaArtifactPayload {
	schema: "xero.remote_runtime_media_artifact.v1";
	ok: boolean;
	artifactId: string;
	mediaType: string;
	bytesBase64?: string;
}

function isRuntimeMediaArtifactPayload(
	value: unknown,
): value is RuntimeMediaArtifactPayload {
	if (!value || typeof value !== "object") return false;
	const payload = value as Partial<RuntimeMediaArtifactPayload>;
	return (
		payload.schema === "xero.remote_runtime_media_artifact.v1" &&
		typeof payload.artifactId === "string" &&
		typeof payload.mediaType === "string" &&
		typeof payload.ok === "boolean"
	);
}

function base64ToBytes(input: string): Uint8Array {
	const binary = atob(input);
	const bytes = new Uint8Array(binary.length);
	for (let index = 0; index < binary.length; index += 1) {
		bytes[index] = binary.charCodeAt(index);
	}
	return bytes;
}

function buildComposerModelGroups(
	options: readonly SessionModelOption[],
): ComposerSelectGroup[] {
	const groups = new Map<string, ComposerSelectGroup>();
	for (const option of options) {
		const providerLabel = modelProviderLabel(option);
		const providerSlug =
			providerLabel.toLowerCase().replace(/\s+/g, "-") || "models";
		const groupId =
			option.providerId ?? option.providerProfileId ?? providerSlug;
		const modelLabel = modelDisplayLabel(option, providerLabel);
		const existing = groups.get(groupId);
		const nextOption = {
			id: option.id,
			label: modelLabel,
		};
		if (existing) {
			groups.set(groupId, {
				...existing,
				options: [...existing.options, nextOption],
			});
			continue;
		}
		groups.set(groupId, {
			id: groupId,
			label: providerLabel,
			options: [nextOption],
		});
	}
	return Array.from(groups.values());
}

function modelDisplayLabel(
	option: SessionModelOption,
	providerLabel: string,
): string {
	const fallback = option.modelId.trim() || option.id;
	let label = option.label.trim() || fallback;
	const suffixes = [
		providerLabel,
		option.providerLabel,
		option.providerId,
		option.providerProfileId,
	]
		.filter((value): value is string => Boolean(value?.trim()))
		.map((value) => ` · ${value.trim()}`);
	for (const suffix of suffixes) {
		if (label.endsWith(suffix)) {
			label = label.slice(0, -suffix.length).trim();
			break;
		}
	}
	return label || fallback;
}

function modelProviderLabel(option: SessionModelOption): string {
	if (option.providerLabel?.trim()) return option.providerLabel.trim();
	return providerLabelForId(option.providerId) ?? "Current selection";
}

function defaultThinkingEffortForModel(
	option: SessionModelOption | null,
): SessionThinkingEffort | null {
	if (!option?.thinkingSupported) return null;
	return (
		option.defaultThinkingEffort ??
		(option.thinkingEffortOptions.length > 0
			? option.thinkingEffortOptions[0]
			: null)
	);
}

function runtimeAgentIdFromString(value: string | null): RuntimeAgentIdDto {
	const parsed = runtimeAgentIdSchema.safeParse(value);
	return parsed.success ? parsed.data : "ask";
}

function resolveApprovalModeForAgent(
	runtimeAgentId: RuntimeAgentIdDto,
	approvalMode: RuntimeRunApprovalModeDto | null | undefined,
): RuntimeRunApprovalModeDto {
	const descriptor = getRuntimeAgentDescriptor(runtimeAgentId);
	if (approvalMode && descriptor.allowedApprovalModes.includes(approvalMode)) {
		return approvalMode;
	}
	return descriptor.defaultApprovalMode;
}

function providerLabelForId(providerId: string | null): string | null {
	switch (providerId) {
		case "openai_codex":
			return "OpenAI Codex";
		case "openrouter":
			return "OpenRouter";
		case "anthropic":
			return "Anthropic";
		case "xai":
			return "xAI / Grok";
		case "github_models":
			return "GitHub Models";
		case "openai_api":
			return "OpenAI-compatible";
		case "deepseek":
			return "DeepSeek";
		case "ollama":
			return "Ollama";
		case "azure_openai":
			return "Azure OpenAI";
		case "gemini_ai_studio":
			return "Gemini AI Studio";
		case "bedrock":
			return "Amazon Bedrock";
		case "vertex":
			return "Google Vertex AI";
		default:
			return providerId?.trim() || null;
	}
}

function formatThinkingEffortLabel(effort: SessionThinkingEffort): string {
	switch (effort) {
		case "none":
			return "None";
		case "minimal":
			return "Minimal";
		case "low":
			return "Low";
		case "medium":
			return "Medium";
		case "high":
			return "High";
		case "x_high":
			return "Very high";
	}
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
	const [debounced, setDebounced] = useState(value);

	useEffect(() => {
		const timeout = window.setTimeout(() => setDebounced(value), delayMs);
		return () => window.clearTimeout(timeout);
	}, [delayMs, value]);

	return debounced;
}
