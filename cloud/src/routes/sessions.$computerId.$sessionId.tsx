import { createFileRoute } from "@tanstack/react-router";
import {
	Composer,
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
	const thinkingOptionsForModel =
		resolvedModelOption?.thinkingEffortOptions ?? [];
	const resolvedThinkingEffort =
		selectedThinkingEffort ??
		currentThinkingEffort ??
		resolvedModelOption?.defaultThinkingEffort ??
		(thinkingOptionsForModel.length > 0 ? thinkingOptionsForModel[0] : null);
	const thinkingComposerOptions = useMemo<ComposerSelectOption[]>(() => {
		if (!resolvedModelOption?.thinkingSupported) return [];
		return thinkingOptionsForModel.map((effort) => ({
			id: effort,
			label: formatThinkingEffortLabel(effort),
		}));
	}, [resolvedModelOption?.thinkingSupported, thinkingOptionsForModel]);
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
			if (
				!channel ||
				!session.deviceId ||
				!resolvedAgentId ||
				!resolvedModelId
			) {
				return;
			}
			const payload: Record<string, unknown> = {
				agent: resolvedAgentId,
				modelId: resolvedModelOption?.modelId ?? resolvedModelId,
				autoCompactEnabled: next,
			};
			if (resolvedModelOption?.providerProfileId) {
				payload.providerProfileId = resolvedModelOption.providerProfileId;
			}
			if (resolvedThinkingEffort && resolvedModelOption?.thinkingSupported) {
				payload.thinkingEffort = resolvedThinkingEffort;
			}
			const command: InboundCommand = {
				v: 1,
				seq: Date.now(),
				computer_id: computerId,
				session_id: sessionId,
				device_id: session.deviceId,
				kind: "update_session_controls",
				payload,
			};
			pushInboundCommand(channel, command);
		},
		[
			channel,
			computerId,
			key,
			resolvedAgentId,
			resolvedModelId,
			resolvedModelOption,
			resolvedThinkingEffort,
			session.deviceId,
			sessionId,
		],
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
						onAgentChange={(agentId) =>
							setSelectedControls((current) => ({
								key,
								agentId,
								modelId: current.key === key ? current.modelId : null,
								thinkingEffort:
									current.key === key ? current.thinkingEffort : null,
								autoCompactEnabled:
									current.key === key ? current.autoCompactEnabled : null,
							}))
						}
						modelGroups={[{ id: "models", options: availableModels }]}
						selectedModelId={resolvedModelId}
						onModelChange={(modelId) =>
							setSelectedControls((current) => ({
								key,
								agentId: current.key === key ? current.agentId : null,
								modelId,
								thinkingEffort: null,
								autoCompactEnabled:
									current.key === key ? current.autoCompactEnabled : null,
							}))
						}
						thinkingOptions={thinkingComposerOptions}
						selectedThinkingId={resolvedThinkingEffort}
						onThinkingChange={(value) =>
							setSelectedControls((current) => ({
								key,
								agentId: current.key === key ? current.agentId : null,
								modelId: current.key === key ? current.modelId : null,
								thinkingEffort: value as SessionThinkingEffort,
								autoCompactEnabled:
									current.key === key ? current.autoCompactEnabled : null,
							}))
						}
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

function formatThinkingEffortLabel(effort: SessionThinkingEffort): string {
	switch (effort) {
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
