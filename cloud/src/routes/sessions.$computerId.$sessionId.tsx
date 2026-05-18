import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { WebComposer } from "@xero/ui/components/composer";
import { ConversationSection } from "@xero/ui/components/transcript/conversation-section";
import { useCallback, useEffect, useState } from "react";

import { SessionDrawer } from "#/components/session-drawer";
import { SessionTopBar } from "#/components/session-top-bar";
import {
	type CloudSession,
	getCurrentSession,
	signOut,
} from "#/lib/auth/session";
import {
	type InboundCommand,
	pushInboundCommand,
} from "#/lib/relay/relay-client";
import { sessionKey, useSessionStore } from "#/lib/relay/session-store";
import {
	useAccountVisibleSessions,
	useSessionStream,
} from "#/lib/relay/use-session-stream";

export const Route = createFileRoute("/sessions/$computerId/$sessionId")({
	beforeLoad: async () => {
		const session = await getCurrentSession();
		if (!session) throw redirect({ to: "/" });
		return { session };
	},
	component: SessionView,
});

function SessionView() {
	const { session } = Route.useRouteContext();
	const { computerId, sessionId } = Route.useParams();
	const navigate = useNavigate();
	const key = sessionKey(computerId, sessionId);

	const { channel, joinRejected } = useSessionStream({
		computerId,
		sessionId,
		relayToken: session.relayToken,
		accountId: session.accountId,
	});
	const visibleSessions = useAccountVisibleSessions(
		session.relayToken,
		session.accountId,
		session.devices,
		session.deviceId,
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
	const currentSessionVisible = visibleSessions.some(
		(s) => s.computerId === computerId && s.sessionId === sessionId,
	);

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

	useEffect(() => {
		if (
			!joinRejected &&
			(!currentComputerReconciled || currentSessionVisible)
		) {
			return;
		}
		void navigate({ to: "/sessions", replace: true });
	}, [
		currentComputerReconciled,
		currentSessionVisible,
		joinRejected,
		navigate,
	]);

	const dispatchSend = useCallback(() => {
		if (!channel || !draftPrompt.trim() || !session.deviceId) return;
		const payload: Record<string, unknown> = {
			message: draftPrompt.trim(),
		};
		if (resolvedAgentId && resolvedModelId) {
			payload.agent = resolvedAgentId;
			payload.modelId = resolvedModelId;
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
	}, [
		channel,
		computerId,
		draftPrompt,
		key,
		resolvedAgentId,
		resolvedModelId,
		session.deviceId,
		sessionId,
	]);

	const handleNewSession = () => {
		if (!channel || !session.deviceId || !resolvedAgentId) return;
		const payload: Record<string, unknown> = {
			agent: resolvedAgentId,
			prompt: "",
		};
		if (resolvedModelId) {
			payload.modelId = resolvedModelId;
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

	const handleSignOut = () => {
		void signOut().then(() => {
			if (typeof window !== "undefined") window.location.href = "/";
		});
	};

	const sessionTitle =
		visibleSessions.find(
			(s) => s.computerId === computerId && s.sessionId === sessionId,
		)?.title ?? "Session";

	return (
		<main className="flex h-dvh flex-col bg-background text-foreground">
			<SessionTopBar
				title={sessionTitle}
				onNewSession={handleNewSession}
				drawerTrigger={
					<SessionDrawer
						session={session as CloudSession}
						visibleSessions={visibleSessions}
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
						onSignOut={handleSignOut}
					/>
				}
			/>
			<div className="flex-1 overflow-y-auto px-4 py-4 sm:px-6">
				<div className="mx-auto flex max-w-3xl flex-col gap-4">
					<ConversationSection
						runtimeRun={null}
						visibleTurns={turns}
						streamIssue={null}
						streamFailure={null}
						showActivityIndicator={isLive}
						accountAvatarUrl={session.avatarUrl ?? null}
						accountLogin={session.githubLogin}
					/>
				</div>
			</div>
			<div className="bg-background px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] pt-3 sm:px-6">
				<div className="mx-auto max-w-3xl">
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
		</main>
	);
}
