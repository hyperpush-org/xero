import { createFileRoute } from "@tanstack/react-router";
import {
	Composer,
	type ComposerDictationLike,
	type ComposerSelectGroup,
	type ComposerSelectOption,
	WebComposerContextIndicator,
	type WebComposerContextIndicatorStatus,
} from "@xero/ui/components/composer";
import { EmptySessionState } from "@xero/ui/components/empty-session-state";
import {
	type ConversationMessageAttachment,
	ConversationSection,
	type ConversationTurn,
} from "@xero/ui/components/transcript/conversation-section";
import { Button } from "@xero/ui/components/ui/button";
import {
	getRuntimeAgentDescriptor,
	getRuntimeRunApprovalModeDescription,
	getRuntimeRunApprovalModeLabel,
	type RuntimeAgentIdDto,
	type RuntimeRunApprovalModeDto,
	runtimeAgentIdSchema,
} from "@xero/ui/model/runtime";
import {
	Monitor,
	MousePointer2,
	OctagonX,
	RefreshCw,
	Square,
} from "lucide-react";
import type { Channel } from "phoenix";
import {
	type KeyboardEvent,
	type PointerEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	type WheelEvent,
} from "react";
import { LoadingScreen } from "#/components/loading-screen";
import { decodeRelayFrame } from "#/lib/relay/envelope";
import {
	answerComputerUseStreamOffer,
	heartbeatComputerUseManualControl,
	type InboundCommand,
	pushInboundCommand,
	releaseComputerUseManualControl,
	requestComputerUseManualControl,
	requestComputerUseStream,
	requestComputerUseStreamKeyframe,
	requestComputerUseStreamStatus,
	requestContextSnapshot,
	requestRunCancel,
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
	sessionKey,
	useSessionStore,
} from "#/lib/relay/session-store";
import { useSessionsShell } from "#/lib/relay/sessions-shell-context";
import { useConversationAutoFollow } from "#/lib/relay/use-conversation-auto-follow";
import { useRemoteAttachments } from "#/lib/relay/use-remote-attachments";
import { useSessionStream } from "#/lib/relay/use-session-stream";

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

function SessionView() {
	const shell = useSessionsShell();
	const {
		session,
		visibleSessions,
		currentComputerOnline,
		reportActiveTargetInvalid,
	} = shell;
	const { computerId, sessionId } = Route.useParams();
	const key = sessionKey(computerId, sessionId);

	const transcript = useSessionStore((state) => state.transcripts[key]);
	const turns = transcript?.turns ?? [];
	const availableAgents = transcript?.availableAgents ?? [];
	const availableModels = transcript?.availableModels ?? [];
	const currentAgentId = transcript?.currentAgentId ?? null;
	const currentModelId = transcript?.currentModelId ?? null;
	const contextSnapshot = transcript?.contextSnapshot ?? null;
	const contextSnapshotError = transcript?.contextSnapshotError ?? null;
	const isLive = transcript?.isLive ?? false;
	const currentSessionAvailable = visibleSessions.some(
		(s) => s.computerId === computerId && s.sessionId === sessionId,
	);
	const visibleSession =
		visibleSessions.find(
			(s) => s.computerId === computerId && s.sessionId === sessionId,
		) ?? null;
	const isComputerUseSession = Boolean(visibleSession?.isComputerUse);
	const { channel, iceServers, joinRejected, streamRunId, streamToken } =
		useSessionStream({
			computerId,
			enabled: currentComputerOnline && currentSessionAvailable,
			sessionId,
			relayToken: session.relayToken,
		});
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
		if (transcript) return;
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
		key,
		session.deviceId,
		sessionId,
		transcript,
	]);

	useEffect(() => {
		if (!channel || !session.deviceId || !transcript || isLive) return;
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

	return (
		<div className="relative min-h-0 flex-1">
			<div
				ref={conversationViewportRef}
				onScroll={handleConversationScroll}
				onWheel={handleConversationWheel}
				className="absolute inset-0 overflow-y-auto px-4 pt-4 sm:px-6"
			>
				<div
					ref={conversationContentRef}
					className="mx-auto flex min-h-full max-w-3xl flex-col gap-4 pb-24 lg:max-w-[47rem]"
				>
					{transcript ? (
						turns.length === 0 ? (
							<div className="flex flex-1 items-center justify-center">
								<EmptySessionState
									projectLabel={projectLabel}
									context={isComputerUseSession ? "computer-use" : "default"}
									onSelectSuggestion={setDraftPrompt}
								/>
							</div>
						) : (
							<>
								{isComputerUseSession ? (
									<ComputerUseDesktopViewport
										channel={channel}
										computerId={computerId}
										deviceId={session.deviceId}
										iceServers={iceServers}
										isOnline={currentComputerOnline}
										isRunLive={isLive}
										previewUrl={desktopPreviewUrl}
										sessionId={sessionId}
										streamRunId={streamRunId}
										streamToken={streamToken}
									/>
								) : null}
								<ConversationSection
									runtimeRun={null}
									visibleTurns={resolvedTurns}
									streamIssue={null}
									streamFailure={null}
									showActivityIndicator={isLive}
									accountAvatarUrl={session.avatarUrl ?? null}
									accountLogin={session.githubLogin}
								/>
							</>
						)
					) : (
						<LoadingScreen className="flex-1" />
					)}
					<div aria-hidden="true" className="h-1 shrink-0" />
				</div>
			</div>
			<div
				aria-hidden="true"
				className="pointer-events-none absolute inset-x-0 top-0 z-10 h-7 bg-gradient-to-b from-background to-background/0"
			/>
			<div className="pointer-events-none absolute inset-x-0 bottom-0 bg-background px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] sm:px-6">
				<div className="pointer-events-auto mx-auto max-w-3xl">
					<Composer
						draftPrompt={draftPrompt}
						onDraftPromptChange={setDraftPrompt}
						onSubmit={dispatchSend}
						autoCompactEnabled={
							isComputerUseSession ? undefined : autoCompactEnabled
						}
						onAutoCompactEnabledChange={
							isComputerUseSession ? undefined : handleAutoCompactEnabledChange
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
						onAddFiles={attachmentsHook.addFiles}
						onRemoveAttachment={attachmentsHook.removeAttachment}
						contextMeter={contextMeter}
						dictation={isComputerUseSession ? hiddenDictation : undefined}
					/>
				</div>
			</div>
		</div>
	);
}

type DesktopViewportState =
	| "waiting"
	| "connecting"
	| "live"
	| "degraded"
	| "paused"
	| "manual"
	| "offline";

type DesktopStreamQuality = "low" | "balanced" | "high";

const DESKTOP_STREAM_DATA_CHANNEL_LABEL = "xero-desktop-stream";
const DESKTOP_STREAM_MAX_BUFFERED_FRAMES = 24;

interface DesktopStreamDetails {
	status?: string | null;
	transport?: string | null;
	quality?: DesktopStreamQuality | null;
	maxWidth?: number | null;
	maxFrameRate?: number | null;
	message?: string | null;
}

function ComputerUseDesktopViewport({
	channel,
	computerId,
	deviceId,
	iceServers,
	isOnline,
	isRunLive,
	previewUrl,
	sessionId,
	streamRunId,
	streamToken,
}: {
	channel: Channel | null;
	computerId: string;
	deviceId?: string | null;
	iceServers: RTCIceServer[];
	isOnline: boolean;
	isRunLive: boolean;
	previewUrl: string | null;
	sessionId: string;
	streamRunId: string | null;
	streamToken: string | null;
}) {
	const [state, setState] = useState<DesktopViewportState>(
		isOnline ? "waiting" : "offline",
	);
	const [streamId, setStreamId] = useState<string | null>(null);
	const [manualControlId, setManualControlId] = useState<string | null>(null);
	const [fallbackPreviewUrl, setFallbackPreviewUrl] = useState<string | null>(
		null,
	);
	const [selectedQuality, setSelectedQuality] =
		useState<DesktopStreamQuality>("balanced");
	const [streamDetails, setStreamDetails] =
		useState<DesktopStreamDetails | null>(null);
	const [hasLiveVideo, setHasLiveVideo] = useState(false);
	const videoRef = useRef<HTMLVideoElement | null>(null);
	const imageRef = useRef<HTMLImageElement | null>(null);
	const peerConnectionRef = useRef<RTCPeerConnection | null>(null);
	const pendingMediaStreamRef = useRef<MediaStream | null>(null);
	const fallbackPreviewObjectUrlRef = useRef<string | null>(null);
	const dataChannelFramesRef = useRef(
		new Map<string, DesktopFrameChunkBuffer>(),
	);
	const lastPointerMoveAtRef = useRef(0);
	const streamIdRef = useRef<string | null>(null);
	const resolvedPreviewUrl = fallbackPreviewUrl ?? previewUrl;

	useEffect(() => {
		return () => {
			if (fallbackPreviewObjectUrlRef.current) {
				URL.revokeObjectURL(fallbackPreviewObjectUrlRef.current);
				fallbackPreviewObjectUrlRef.current = null;
			}
			closeDesktopPeerConnection(peerConnectionRef.current);
			peerConnectionRef.current = null;
			pendingMediaStreamRef.current = null;
			dataChannelFramesRef.current.clear();
			if (videoRef.current) videoRef.current.srcObject = null;
		};
	}, []);

	useEffect(() => {
		streamIdRef.current = streamId;
	}, [streamId]);

	useEffect(() => {
		if (!isOnline) setState("offline");
		else if (state === "offline") setState("waiting");
	}, [isOnline, state]);

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
			setHasLiveVideo(false);
			setState((current) => (current === "manual" ? current : "live"));
		},
		[],
	);

	const handleDesktopDataChannelMessage = useCallback(
		(event: MessageEvent) => {
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

	const ensurePeerConnection = useCallback(() => {
		if (!channel || !deviceId) return null;
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
				setState((current) => (current === "manual" ? current : "live"));
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
			setHasLiveVideo(true);
			setState("live");
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
	}, [
		channel,
		computerId,
		deviceId,
		fallbackPreviewUrl,
		handleDesktopDataChannelMessage,
		iceServers,
		sessionId,
		streamRunId,
		streamToken,
	]);

	const handleWebRtcSignal = useCallback(
		async (payload: ComputerUseDesktopPayload) => {
			if (!channel || !deviceId) return;
			try {
				if (payload.schema === "xero.computer_use_stream_offer.v1") {
					const offer = desktopStreamSessionDescription(payload, "offer");
					if (!offer) return;
					const peerConnection = ensurePeerConnection();
					if (!peerConnection) return;
					setState("connecting");
					await peerConnection.setRemoteDescription(offer);
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
					if (!candidate || !peerConnectionRef.current) return;
					await peerConnectionRef.current.addIceCandidate(candidate);
				}
			} catch {
				closeDesktopPeerConnection(peerConnectionRef.current);
				peerConnectionRef.current = null;
				pendingMediaStreamRef.current = null;
				dataChannelFramesRef.current.clear();
				if (videoRef.current) videoRef.current.srcObject = null;
				setHasLiveVideo(false);
				setState(fallbackPreviewUrl ? "degraded" : "waiting");
			}
		},
		[
			channel,
			computerId,
			deviceId,
			ensurePeerConnection,
			fallbackPreviewUrl,
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
			if (!isComputerUseDesktopPayload(payload)) return;
			if (payload.streamId) {
				streamIdRef.current = payload.streamId;
				setStreamId(payload.streamId);
			}
			const nextStreamDetails = desktopStreamDetails(payload);
			if (nextStreamDetails) {
				setStreamDetails(nextStreamDetails);
				if (nextStreamDetails.quality) {
					setSelectedQuality(nextStreamDetails.quality);
				}
			}
			if (payload.manualControlId) setManualControlId(payload.manualControlId);
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
				setHasLiveVideo(false);
			}
			if (payload.schema === "xero.computer_use_stream_stop.v1") {
				closeDesktopPeerConnection(peerConnectionRef.current);
				peerConnectionRef.current = null;
				pendingMediaStreamRef.current = null;
				dataChannelFramesRef.current.clear();
				if (videoRef.current) videoRef.current.srcObject = null;
				setHasLiveVideo(false);
				setState("paused");
			} else if (
				payload.schema.startsWith("xero.computer_use_stream_") &&
				payload.schema !== "xero.computer_use_stream_offer.v1" &&
				state !== "live" &&
				state !== "manual"
			) {
				setState(
					nextStreamDetails?.transport === "web_rtc" &&
						nextStreamDetails.status !== "degraded"
						? "connecting"
						: "degraded",
				);
			} else if (
				payload.schema === "xero.computer_use_manual_control_request.v1" ||
				payload.schema === "xero.computer_use_manual_control_grant.v1" ||
				payload.schema === "xero.computer_use_manual_control_heartbeat.v1" ||
				payload.schema === "xero.computer_use_manual_control_input.v1"
			) {
				setState("manual");
			} else if (
				payload.schema === "xero.computer_use_manual_control_release.v1"
			) {
				setState(streamId ? "degraded" : "waiting");
			}
		});
		return () => {
			channel.off("frame", ref);
		};
	}, [channel, handleWebRtcSignal, state, streamId]);

	useEffect(() => {
		if (!channel || !deviceId || state !== "manual" || !manualControlId) return;
		const sendHeartbeat = () => {
			heartbeatComputerUseManualControl(channel, {
				computerId,
				sessionId,
				deviceId,
				manualControlId,
				runId: streamRunId,
				streamToken,
				reason: "manual_cloud_control_heartbeat",
			});
		};
		sendHeartbeat();
		const handle = window.setInterval(sendHeartbeat, 10_000);
		return () => window.clearInterval(handle);
	}, [
		channel,
		computerId,
		deviceId,
		manualControlId,
		sessionId,
		state,
		streamRunId,
		streamToken,
	]);

	useEffect(() => {
		if (!channel || !deviceId || !streamId) return;
		if (state !== "degraded" && state !== "manual") return;
		const handle = window.setInterval(() => {
			requestComputerUseStreamStatus(channel, {
				computerId,
				sessionId,
				deviceId,
				runId: streamRunId,
				streamId,
				streamToken,
			});
		}, fallbackFrameIntervalMs(selectedQuality));
		return () => window.clearInterval(handle);
	}, [
		channel,
		computerId,
		deviceId,
		selectedQuality,
		sessionId,
		state,
		streamId,
		streamRunId,
		streamToken,
	]);

	const canSend = Boolean(channel && deviceId && isOnline);
	const status = desktopViewportStatusLabel(state, resolvedPreviewUrl);
	useEffect(() => {
		if (!hasLiveVideo || !videoRef.current || !pendingMediaStreamRef.current)
			return;
		videoRef.current.srcObject = pendingMediaStreamRef.current;
	}, [hasLiveVideo]);
	const startStream = () => {
		if (!channel || !deviceId) return;
		setState("connecting");
		requestComputerUseStream(channel, {
			computerId,
			sessionId,
			deviceId,
			quality: selectedQuality,
			runId: streamRunId,
			streamToken,
			iceServers,
		});
	};
	const updateQuality = (quality: DesktopStreamQuality) => {
		setSelectedQuality(quality);
		if (!channel || !deviceId || !streamId) return;
		setComputerUseStreamQuality(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId,
			quality,
			streamToken,
		});
	};
	const requestKeyframe = () => {
		if (!channel || !deviceId || !streamId) return;
		requestComputerUseStreamKeyframe(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId,
			streamToken,
		});
	};
	const stopStream = () => {
		if (!channel || !deviceId) return;
		closeDesktopPeerConnection(peerConnectionRef.current);
		peerConnectionRef.current = null;
		pendingMediaStreamRef.current = null;
		dataChannelFramesRef.current.clear();
		if (videoRef.current) videoRef.current.srcObject = null;
		setHasLiveVideo(false);
		stopComputerUseStream(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId,
			streamToken,
		});
		setState("paused");
	};
	const emergencyStop = () => {
		if (!channel || !deviceId) return;
		closeDesktopPeerConnection(peerConnectionRef.current);
		peerConnectionRef.current = null;
		pendingMediaStreamRef.current = null;
		dataChannelFramesRef.current.clear();
		if (videoRef.current) videoRef.current.srcObject = null;
		setHasLiveVideo(false);
		releaseComputerUseManualControl(channel, {
			computerId,
			sessionId,
			deviceId,
			manualControlId,
			runId: streamRunId,
			streamToken,
		});
		stopComputerUseStream(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamId,
			streamToken,
		});
		requestRunCancel(channel, {
			computerId,
			sessionId,
			deviceId,
			reason: "cloud_emergency_stop",
		});
		setState("paused");
	};
	const requestManual = () => {
		if (!channel || !deviceId) return;
		requestComputerUseManualControl(channel, {
			computerId,
			sessionId,
			deviceId,
			runId: streamRunId,
			streamToken,
			reason: "manual_cloud_control",
		});
		setState("manual");
	};
	const releaseManual = () => {
		if (!channel || !deviceId) return;
		releaseComputerUseManualControl(channel, {
			computerId,
			sessionId,
			deviceId,
			manualControlId,
			runId: streamRunId,
			streamToken,
		});
		setState(streamId ? "degraded" : "waiting");
	};
	const sendManualInput = useCallback(
		(input: Parameters<typeof sendComputerUseManualInput>[1]["input"]) => {
			if (!channel || !deviceId || state !== "manual" || !manualControlId)
				return;
			sendComputerUseManualInput(channel, {
				computerId,
				sessionId,
				deviceId,
				manualControlId,
				runId: streamRunId,
				streamToken,
				input,
			});
		},
		[
			channel,
			computerId,
			deviceId,
			manualControlId,
			sessionId,
			state,
			streamRunId,
			streamToken,
		],
	);
	const pointFromPointerEvent = useCallback((event: PointerEvent) => {
		const image = imageRef.current;
		const video = videoRef.current;
		const target = image ?? video;
		const sourceWidth = image?.naturalWidth ?? video?.videoWidth ?? 0;
		const sourceHeight = image?.naturalHeight ?? video?.videoHeight ?? 0;
		if (!target || sourceWidth <= 0 || sourceHeight <= 0) return null;
		const rect = target.getBoundingClientRect();
		const scaleX = sourceWidth / rect.width;
		const scaleY = sourceHeight / rect.height;
		const x = Math.round((event.clientX - rect.left) * scaleX);
		const y = Math.round((event.clientY - rect.top) * scaleY);
		if (x < 0 || y < 0 || x > sourceWidth || y > sourceHeight) return null;
		return { x, y };
	}, []);
	const handlePointerDown = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (state !== "manual") return;
			const point = pointFromPointerEvent(event);
			if (!point) return;
			event.preventDefault();
			event.currentTarget.setPointerCapture(event.pointerId);
			sendManualInput({
				action: event.button === 2 ? "mouse_right_click" : "mouse_click",
				x: point.x,
				y: point.y,
				button:
					event.button === 1 ? "middle" : event.button === 2 ? "right" : "left",
				clicks: event.detail > 1 ? 2 : 1,
			});
		},
		[pointFromPointerEvent, sendManualInput, state],
	);
	const handlePointerMove = useCallback(
		(event: PointerEvent<HTMLElement>) => {
			if (state !== "manual" || event.buttons === 0) return;
			const now = Date.now();
			if (now - lastPointerMoveAtRef.current < 80) return;
			lastPointerMoveAtRef.current = now;
			const point = pointFromPointerEvent(event);
			if (!point) return;
			sendManualInput({ action: "mouse_move", x: point.x, y: point.y });
		},
		[pointFromPointerEvent, sendManualInput, state],
	);
	const handleWheel = useCallback(
		(event: WheelEvent<HTMLElement>) => {
			if (state !== "manual") return;
			event.preventDefault();
			sendManualInput({
				action: "scroll",
				deltaX: Math.round(event.deltaX),
				deltaY: Math.round(event.deltaY),
			});
		},
		[sendManualInput, state],
	);
	const handleKeyDown = useCallback(
		(event: KeyboardEvent<HTMLElement>) => {
			if (state !== "manual") return;
			event.preventDefault();
			const modifiers = [
				event.metaKey ? "command" : null,
				event.ctrlKey ? "control" : null,
				event.altKey ? "option" : null,
				event.shiftKey ? "shift" : null,
			].filter(Boolean) as string[];
			if (modifiers.length > 0 && event.key.length > 0) {
				sendManualInput({ action: "hotkey", keys: [...modifiers, event.key] });
				return;
			}
			if (event.key.length === 1) {
				sendManualInput({ action: "type_text", text: event.key });
				return;
			}
			sendManualInput({ action: "key_press", key: event.key });
		},
		[sendManualInput, state],
	);

	return (
		<section
			aria-label="Desktop"
			tabIndex={state === "manual" ? 0 : -1}
			onPointerDown={handlePointerDown}
			onPointerMove={handlePointerMove}
			onWheel={handleWheel}
			onKeyDown={handleKeyDown}
			className="overflow-hidden rounded-md border bg-background"
		>
			<div className="flex min-h-[13rem] items-center justify-center bg-zinc-950 text-zinc-100">
				{hasLiveVideo ? (
					<video
						ref={videoRef}
						autoPlay
						muted
						playsInline
						className="max-h-[18rem] w-full object-contain"
					/>
				) : resolvedPreviewUrl ? (
					<img
						ref={imageRef}
						src={resolvedPreviewUrl}
						alt="Desktop"
						className="max-h-[18rem] w-full object-contain"
						draggable={false}
					/>
				) : (
					<div className="flex flex-col items-center gap-2 text-sm text-zinc-300">
						<Monitor className="h-8 w-8" aria-hidden="true" />
						<span>{status}</span>
					</div>
				)}
			</div>
			<div className="flex flex-wrap items-center justify-between gap-2 border-t px-3 py-2">
				<div className="min-w-0">
					<div className="text-sm font-medium">{status}</div>
					{streamDetails ? (
						<div className="truncate text-xs text-muted-foreground">
							{desktopStreamDetailsLabel(streamDetails)}
						</div>
					) : null}
				</div>
				<div className="flex items-center gap-2">
					<label className="sr-only" htmlFor="desktop-stream-quality">
						Stream quality
					</label>
					<select
						id="desktop-stream-quality"
						value={selectedQuality}
						disabled={!canSend}
						onChange={(event) =>
							updateQuality(event.currentTarget.value as DesktopStreamQuality)
						}
						className="h-8 rounded-md border bg-background px-2 text-sm text-foreground disabled:opacity-50"
					>
						<option value="low">Low</option>
						<option value="balanced">Balanced</option>
						<option value="high">High</option>
					</select>
					<Button
						type="button"
						size="sm"
						variant="secondary"
						disabled={!canSend || state === "connecting"}
						onClick={startStream}
					>
						<Monitor className="mr-2 h-4 w-4" aria-hidden="true" />
						Start
					</Button>
					<Button
						type="button"
						size="sm"
						variant="secondary"
						disabled={!canSend}
						onClick={state === "manual" ? releaseManual : requestManual}
					>
						<MousePointer2 className="mr-2 h-4 w-4" aria-hidden="true" />
						{state === "manual" ? "Release" : "Manual"}
					</Button>
					<Button
						type="button"
						size="sm"
						variant="destructive"
						disabled={!canSend}
						onClick={stopStream}
					>
						<Square className="mr-2 h-4 w-4" aria-hidden="true" />
						Stop
					</Button>
					<Button
						type="button"
						size="sm"
						variant="destructive"
						disabled={
							!canSend || (!isRunLive && state !== "manual" && !streamId)
						}
						onClick={emergencyStop}
					>
						<OctagonX className="mr-2 h-4 w-4" aria-hidden="true" />
						Emergency Stop
					</Button>
					<Button
						type="button"
						size="sm"
						variant="secondary"
						disabled={!canSend || !streamId}
						onClick={requestKeyframe}
					>
						<RefreshCw className="mr-2 h-4 w-4" aria-hidden="true" />
						Refresh
					</Button>
				</div>
			</div>
		</section>
	);
}

function desktopViewportStatusLabel(
	state: DesktopViewportState,
	previewUrl: string | null,
): string {
	if (state === "offline") return "Device offline";
	if (state === "connecting") return "Connecting stream";
	if (state === "live") return "Live stream";
	if (state === "manual") return "Manual control active";
	if (state === "paused") return "Stream paused";
	if (previewUrl) return "Stream degraded";
	return "Waiting for desktop";
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
		message: typeof stream.message === "string" ? stream.message : null,
	};
}

function desktopStreamDetailsLabel(details: DesktopStreamDetails): string {
	const transport = details.transport?.replace(/_/g, " ") ?? "stream";
	const quality = details.quality ?? "balanced";
	const width = details.maxWidth ? `${details.maxWidth}px` : null;
	const frameRate = details.maxFrameRate ? `${details.maxFrameRate} fps` : null;
	return [transport, quality, width, frameRate].filter(Boolean).join(" · ");
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
			message?: unknown;
		} | null;
	} | null;
	desktopFrame?: {
		ok?: boolean;
		mediaType?: string | null;
		bytesBase64?: string | null;
	};
}

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
			if (
				attachment.source?.kind === "remote_artifact" &&
				!attachmentPreviewAvailable(attachment) &&
				!resolvedUrls[attachment.source.artifactId]
			) {
				ids.add(attachment.source.artifactId);
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
		const renderUrl = resolvedUrls[attachment.source.artifactId];
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
