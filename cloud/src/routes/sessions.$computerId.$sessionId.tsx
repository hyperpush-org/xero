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
import type { Channel } from "phoenix";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoadingScreen } from "#/components/loading-screen";
import { decodeRelayFrame } from "#/lib/relay/envelope";
import {
	type InboundCommand,
	pushInboundCommand,
	requestContextSnapshot,
	requestRuntimeMediaArtifact,
	requestSessionSnapshot,
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
	const { channel, joinRejected } = useSessionStream({
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

	const [draftPrompt, setDraftPrompt] = useState("");
	const [selectedControls, setSelectedControls] = useState<{
		key: string;
		agentId: string | null;
		modelId: string | null;
		thinkingEffort: SessionThinkingEffort | null;
		autoCompactEnabled: boolean | null;
	}>({
		key,
		agentId: null,
		modelId: null,
		thinkingEffort: null,
		autoCompactEnabled: null,
	});
	const selectedAgentId =
		selectedControls.key === key ? selectedControls.agentId : null;
	const selectedModelId =
		selectedControls.key === key ? selectedControls.modelId : null;
	const selectedThinkingEffort =
		selectedControls.key === key ? selectedControls.thinkingEffort : null;
	const selectedAutoCompactEnabled =
		selectedControls.key === key ? selectedControls.autoCompactEnabled : null;
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
				current.autoCompactEnabled === null
			) {
				return current;
			}
			return {
				key,
				agentId: null,
				modelId: null,
				thinkingEffort: null,
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
				autoCompactEnabled: next,
			}));
			pushControlUpdate({ autoCompactEnabled: next });
		},
		[key, pushControlUpdate],
	);

	const handleAgentChange = useCallback(
		(agentId: string) => {
			if (isComputerUseSession) return;
			setSelectedControls((current) => ({
				key,
				agentId,
				modelId: current.key === key ? current.modelId : null,
				thinkingEffort: current.key === key ? current.thinkingEffort : null,
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({ agentId });
		},
		[isComputerUseSession, key, pushControlUpdate],
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
				autoCompactEnabled:
					current.key === key ? current.autoCompactEnabled : null,
			}));
			pushControlUpdate({ thinkingEffort });
		},
		[key, pushControlUpdate],
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
							<ConversationSection
								runtimeRun={null}
								visibleTurns={resolvedTurns}
								streamIssue={null}
								streamFailure={null}
								showActivityIndicator={isLive}
								accountAvatarUrl={session.avatarUrl ?? null}
								accountLogin={session.githubLogin}
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
			const url = URL.createObjectURL(
				new Blob([bytes], { type: payload.mediaType }),
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
