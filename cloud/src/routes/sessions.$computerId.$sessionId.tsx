import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { WebComposer } from "@xero/ui/components/composer";
import { ConversationSection } from "@xero/ui/components/transcript/conversation-section";
import { useCallback, useEffect, useRef, useState } from "react";

import { EmptySessionState } from "#/components/empty-session-state";
import { LoadingScreen } from "#/components/loading-screen";
import { SessionDrawer } from "#/components/session-drawer";
import { SessionTopBar } from "#/components/session-top-bar";
import type { CloudSession } from "#/lib/auth/session";
import { signOut } from "#/lib/auth/session";
import {
	type InboundCommand,
	pushInboundCommand,
	requestSessionSnapshot,
} from "#/lib/relay/relay-client";
import { sessionKey, useSessionStore } from "#/lib/relay/session-store";
import { useConversationAutoFollow } from "#/lib/relay/use-conversation-auto-follow";
import {
	useAccountRemoteSessions,
	useSessionStream,
} from "#/lib/relay/use-session-stream";
import { Route as SessionsRoute } from "./sessions";

export const Route = createFileRoute("/sessions/$computerId/$sessionId")({
	component: SessionView,
});

function SessionView() {
	const { session } = SessionsRoute.useRouteContext();
	const { computerId, sessionId } = Route.useParams();
	const navigate = useNavigate();
	const key = sessionKey(computerId, sessionId);
	const remoteSessions = useAccountRemoteSessions(
		session.relayToken,
		session.accountId,
		session.devices,
		session.deviceId,
	);

	const visibleSessions = useSessionStore((state) => state.visibleSessions);
	const computerPresenceKnown = useSessionStore(
		(state) => state.computerPresenceKnown,
	);
	const currentComputerOnline = useSessionStore((state) =>
		Boolean(state.onlineComputerIds[computerId]),
	);
	const transcript = useSessionStore((state) => state.transcripts[key]);
	const turns = transcript?.turns ?? [];
	const availableAgents = transcript?.availableAgents ?? [];
	const availableModels = transcript?.availableModels ?? [];
	const currentAgentId = transcript?.currentAgentId ?? null;
	const currentModelId = transcript?.currentModelId ?? null;
	const isLive = transcript?.isLive ?? false;
	const currentComputerReconciled = useSessionStore((state) =>
		Boolean(state.visibleSessionsByComputerVersion[computerId]),
	);
	const visibleSessionsVersion = useSessionStore(
		(state) => state.visibleSessionsVersion,
	);
	const currentSessionVisible = visibleSessions.some(
		(s) =>
			s.computerId === computerId &&
			s.sessionId === sessionId &&
			s.remoteVisible,
	);
	const { channel, joinRejected } = useSessionStream({
		computerId,
		enabled: currentComputerOnline && currentSessionVisible,
		sessionId,
		relayToken: session.relayToken,
	});
	const lastSnapshotRequestKey = useRef<string | null>(null);

	const [draftPrompt, setDraftPrompt] = useState("");
	const [selectedControls, setSelectedControls] = useState<{
		key: string;
		agentId: string | null;
		modelId: string | null;
	}>({ key, agentId: null, modelId: null });
	const selectedAgentId =
		selectedControls.key === key ? selectedControls.agentId : null;
	const selectedModelId =
		selectedControls.key === key ? selectedControls.modelId : null;

	const resolvedAgentId =
		selectedAgentId ?? currentAgentId ?? availableAgents[0]?.id ?? null;
	const resolvedModelId =
		selectedModelId ?? currentModelId ?? availableModels[0]?.id ?? null;
	const resolvedModelOption =
		availableModels.find((option) => option.id === resolvedModelId) ?? null;
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
		if (computerPresenceKnown && !currentComputerOnline) {
			void navigate({ to: "/sessions", replace: true });
			return;
		}
		if (
			!joinRejected &&
			(!currentComputerReconciled || currentSessionVisible)
		) {
			return;
		}
		void navigate({ to: "/sessions", replace: true });
	}, [
		computerPresenceKnown,
		currentComputerReconciled,
		currentComputerOnline,
		currentSessionVisible,
		joinRejected,
		navigate,
	]);

	useEffect(() => {
		if (!channel || !session.deviceId || !currentSessionVisible) return;
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
		currentSessionVisible,
		key,
		session.deviceId,
		sessionId,
		transcript,
		visibleSessionsVersion,
	]);

	const dispatchSend = useCallback(
		(submittedPrompt?: string) => {
			const message = (submittedPrompt ?? draftPrompt).trim();
			if (!channel || !message || !session.deviceId) return;
			const payload: Record<string, unknown> = {
				message,
			};
			if (resolvedAgentId && resolvedModelId) {
				payload.agent = resolvedAgentId;
				payload.modelId = resolvedModelOption?.modelId ?? resolvedModelId;
				if (resolvedModelOption?.providerProfileId) {
					payload.providerProfileId = resolvedModelOption.providerProfileId;
				}
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
			setSelectedControls({ key, agentId: null, modelId: null });
			followLatestConversation();
		},
		[
			channel,
			computerId,
			draftPrompt,
			followLatestConversation,
			key,
			resolvedAgentId,
			resolvedModelId,
			resolvedModelOption,
			session.deviceId,
			sessionId,
		],
	);

	const projectsForCurrentComputer = remoteSessions.projects.filter(
		(project) => project.computerId === computerId,
	);

	const [pendingNewSession, setPendingNewSession] = useState<{
		projectId: string;
		knownSessionIds: Set<string>;
	} | null>(null);

	const handleNewSession = (projectId: string) => {
		if (!channel || !session.deviceId || !resolvedAgentId) return;
		const knownSessionIds = new Set(
			visibleSessions
				.filter(
					(summary) =>
						summary.computerId === computerId &&
						summary.projectId === projectId,
				)
				.map((summary) => summary.sessionId),
		);
		setPendingNewSession({ projectId, knownSessionIds });
		const payload: Record<string, unknown> = {
			agent: resolvedAgentId,
			projectId,
			prompt: "",
		};
		if (resolvedModelId) {
			payload.modelId = resolvedModelOption?.modelId ?? resolvedModelId;
			if (resolvedModelOption?.providerProfileId) {
				payload.providerProfileId = resolvedModelOption.providerProfileId;
			}
		}
		const command: InboundCommand = {
			v: 1,
			seq: Date.now(),
			computer_id: computerId,
			device_id: session.deviceId,
			kind: "start_session",
			payload,
		};
		pushInboundCommand(channel, command);
	};

	useEffect(() => {
		if (!pendingNewSession) return;
		const created = visibleSessions.find(
			(summary) =>
				summary.computerId === computerId &&
				summary.projectId === pendingNewSession.projectId &&
				!pendingNewSession.knownSessionIds.has(summary.sessionId),
		);
		if (!created) return;
		setPendingNewSession(null);
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId, sessionId: created.sessionId },
			replace: true,
		});
	}, [computerId, navigate, pendingNewSession, visibleSessions]);

	const handleSignOut = () => {
		void signOut().then(() => {
			if (typeof window !== "undefined") window.location.href = "/";
		});
	};

	const currentSessionSummary = visibleSessions.find(
		(s) => s.computerId === computerId && s.sessionId === sessionId,
	);
	const sessionTitle = currentSessionSummary?.title ?? "Session";
	const projectLabel =
		currentSessionSummary?.projectName ??
		currentSessionSummary?.projectId ??
		"this project";

	return (
		<main className="flex h-dvh flex-col bg-background text-foreground">
			<SessionTopBar
				title={sessionTitle}
				projects={projectsForCurrentComputer}
				onSelectProject={handleNewSession}
				drawerTrigger={
					<SessionDrawer
						session={session as CloudSession}
						visibleSessions={visibleSessions}
						projects={projectsForCurrentComputer}
						currentSessionKey={key}
						onSelectSession={(nextComputerId, nextSessionId) => {
							void navigate({
								to: "/sessions/$computerId/$sessionId",
								params: {
									computerId: nextComputerId,
									sessionId: nextSessionId,
								},
							});
						}}
						onSelectProject={handleNewSession}
						onSetSessionRemoteVisibility={
							remoteSessions.setSessionRemoteVisibility
						}
						onArchiveSession={remoteSessions.archiveSession}
						onSignOut={handleSignOut}
					/>
				}
			/>
			<div className="relative min-h-0 flex-1">
				<div
					ref={conversationViewportRef}
					onScroll={handleConversationScroll}
					onWheel={handleConversationWheel}
					className="absolute inset-0 overflow-y-auto px-4 pt-4 sm:px-6"
				>
					<div
						ref={conversationContentRef}
						className="mx-auto flex min-h-full max-w-3xl flex-col gap-4 pb-24"
					>
						{transcript ? (
							turns.length === 0 ? (
								<EmptySessionState
									projectLabel={projectLabel}
									onSelectSuggestion={setDraftPrompt}
								/>
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
				<div className="pointer-events-none absolute inset-x-0 bottom-0 bg-background px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] sm:px-6">
					<div className="pointer-events-auto mx-auto max-w-3xl">
						<WebComposer
							draftPrompt={draftPrompt}
							onDraftPromptChange={setDraftPrompt}
							onSubmit={dispatchSend}
							agentOptions={availableAgents}
							selectedAgentId={resolvedAgentId}
							onAgentChange={(agentId) =>
								setSelectedControls((current) => ({
									key,
									agentId,
									modelId: current.key === key ? current.modelId : null,
								}))
							}
							modelOptions={availableModels}
							selectedModelId={resolvedModelId}
							onModelChange={(modelId) =>
								setSelectedControls((current) => ({
									key,
									agentId: current.key === key ? current.agentId : null,
									modelId,
								}))
							}
						/>
					</div>
				</div>
			</div>
		</main>
	);
}
