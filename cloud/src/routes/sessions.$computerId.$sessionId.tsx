import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { WebComposer } from "@xero/ui/components/composer";
import { ConversationSection } from "@xero/ui/components/transcript/conversation-section";
import { useCallback, useState } from "react";

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

	const { channel } = useSessionStream({
		computerId,
		sessionId,
		relayToken: session.relayToken,
		accountId: session.accountId,
	});
	const visibleSessions = useAccountVisibleSessions(
		session.relayToken,
		session.accountId,
	);
	const transcript = useSessionStore((state) => state.transcripts[key]);
	const turns = transcript?.turns ?? [];
	const availableAgents = transcript?.availableAgents ?? [];
	const availableModels = transcript?.availableModels ?? [];
	const isLive = transcript?.isLive ?? false;

	const [draftPrompt, setDraftPrompt] = useState("");
	const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
	const [selectedModelId, setSelectedModelId] = useState<string | null>(null);

	const resolvedAgentId = selectedAgentId ?? availableAgents[0]?.id ?? null;
	const resolvedModelId = selectedModelId ?? availableModels[0]?.id ?? null;

	const dispatchSend = useCallback(() => {
		if (!channel || !draftPrompt.trim() || !session.deviceId) return;
		const command: InboundCommand = {
			v: 1,
			seq: Date.now(),
			computer_id: computerId,
			session_id: sessionId,
			device_id: session.deviceId,
			kind: "send_message",
			payload: {
				text: draftPrompt.trim(),
				agent: resolvedAgentId,
				modelId: resolvedModelId,
			},
		};
		pushInboundCommand(channel, command);
		setDraftPrompt("");
	}, [
		channel,
		computerId,
		draftPrompt,
		resolvedAgentId,
		resolvedModelId,
		session.deviceId,
		sessionId,
	]);

	const handleNewSession = () => {
		if (!channel || !session.deviceId || !resolvedAgentId) return;
		const command: InboundCommand = {
			v: 1,
			seq: Date.now(),
			computer_id: computerId,
			device_id: session.deviceId,
			kind: "start_session",
			payload: {
				agent: resolvedAgentId,
				modelId: resolvedModelId,
				prompt: "",
			},
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
			<div className="border-t border-border bg-background px-3 pb-[max(env(safe-area-inset-bottom),0.75rem)] pt-3 sm:px-6">
				<div className="mx-auto max-w-3xl">
					<WebComposer
						draftPrompt={draftPrompt}
						onDraftPromptChange={setDraftPrompt}
						onSubmit={dispatchSend}
						agentOptions={availableAgents}
						selectedAgentId={resolvedAgentId}
						onAgentChange={setSelectedAgentId}
						modelOptions={availableModels}
						selectedModelId={resolvedModelId}
						onModelChange={setSelectedModelId}
					/>
				</div>
			</div>
		</main>
	);
}
