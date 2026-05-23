import { createFileRoute } from "@tanstack/react-router";
import {
	Composer,
	type ComposerSelectGroup,
	type ComposerSelectOption,
	WebComposerContextIndicator,
	type WebComposerContextIndicatorStatus,
} from "@xero/ui/components/composer";
import { EmptySessionState } from "@xero/ui/components/empty-session-state";
import { ConversationSection } from "@xero/ui/components/transcript/conversation-section";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoadingScreen } from "#/components/loading-screen";
import {
	type InboundCommand,
	pushInboundCommand,
	requestContextSnapshot,
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
		visibleSessionsVersion,
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
	const { channel, joinRejected } = useSessionStream({
		computerId,
		enabled: currentComputerOnline && currentSessionAvailable,
		sessionId,
		relayToken: session.relayToken,
	});
	const lastSnapshotRequestKey = useRef<string | null>(null);

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

	const resolvedAgentId =
		selectedAgentId ?? currentAgentId ?? availableAgents[0]?.id ?? null;
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
		contextMeterStatus === "idle" ? null : (
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
		if (transcript) {
			lastSnapshotRequestKey.current = null;
			return;
		}
		const requestKey = `${key}:${visibleSessionsVersion}`;
		if (lastSnapshotRequestKey.current === requestKey) return;
		lastSnapshotRequestKey.current = requestKey;
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
		visibleSessionsVersion,
	]);

	useEffect(() => {
		if (!channel || !session.deviceId || !transcript || isLive) return;
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
			const nextAgentId = overrides.agentId ?? resolvedAgentId;
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
				payload.agent = resolvedAgentId;
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
		[key, pushControlUpdate],
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
									onSelectSuggestion={setDraftPrompt}
								/>
							</div>
						) : (
							<ConversationSection
								runtimeRun={null}
								visibleTurns={turns}
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
						autoCompactEnabled={autoCompactEnabled}
						onAutoCompactEnabledChange={handleAutoCompactEnabledChange}
						agentGroups={[{ id: "agents", options: availableAgents }]}
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
					/>
				</div>
			</div>
		</div>
	);
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
