/** @vitest-environment jsdom */

import { createMemoryHistory, RouterProvider } from "@tanstack/react-router";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CloudSession } from "#/lib/auth/session";
import {
	type RemoteProjectSummary,
	useSessionStore,
	type VisibleSessionSummary,
} from "#/lib/relay/session-store";
import { getRouter } from "#/router";
import { activeSessionTargetFromPathname } from "./sessions";

const streamMock = vi.hoisted(() => ({
	sessions: [] as VisibleSessionSummary[],
	projects: [] as RemoteProjectSummary[],
	startSession: vi.fn(() => true),
	archiveSession: vi.fn(() => true),
	accountHookMounts: 0,
	accountHookUnmounts: 0,
}));

vi.mock("#/lib/auth/session", () => ({
	getCachedCurrentSession: vi.fn(async () => cloudSession),
	signOut: vi.fn(async () => undefined),
}));

vi.mock("#/lib/server-url", () => ({
	getCanonicalLoopbackCloudUrl: vi.fn(() => null),
	getPublicRuntimeServerUrl: vi.fn(() => "http://127.0.0.1:3000"),
	RUNTIME_SERVER_URL_META_NAME: "xero-runtime-server-url",
}));

vi.mock("#/components/pwa-service-worker-manager", () => ({
	PwaServiceWorkerManager: () => null,
}));

vi.mock("@xero/ui/components/composer", () => ({
	Composer: () => <div data-testid="composer" />,
	WebComposerContextIndicator: () => <div data-testid="context-indicator" />,
}));

vi.mock("@xero/ui/components/transcript/conversation-section", () => ({
	ConversationSection: () => <div data-testid="conversation-section" />,
}));

vi.mock("#/lib/relay/use-conversation-auto-follow", () => ({
	useConversationAutoFollow: () => ({
		contentRef: { current: null },
		followLatest: vi.fn(),
		onScroll: vi.fn(),
		onWheel: vi.fn(),
		viewportRef: { current: null },
	}),
}));

vi.mock("#/lib/relay/use-remote-attachments", () => ({
	useRemoteAttachments: () => ({
		addFiles: vi.fn(),
		clearAttachments: vi.fn(),
		getReadyAttachments: vi.fn(() => []),
		pendingAttachments: [],
		removeAttachment: vi.fn(),
	}),
}));

vi.mock("#/lib/relay/use-session-stream", async () => {
	const { useEffect } = await vi.importActual<typeof import("react")>("react");
	return {
		useAccountRemoteSessions: () => {
			useEffect(() => {
				streamMock.accountHookMounts += 1;
				return () => {
					streamMock.accountHookUnmounts += 1;
				};
			}, []);
			return {
				sessions: streamMock.sessions,
				projects: streamMock.projects,
				startSession: streamMock.startSession,
				archiveSession: streamMock.archiveSession,
			};
		},
		useSessionStream: () => ({
			channel: null,
			joinRejected: false,
		}),
	};
});

const cloudSession: CloudSession = {
	githubLogin: "snowdamiz",
	avatarUrl: null,
	deviceId: "web-1",
	accountId: "account-1",
	devices: [
		{
			id: "desktop-1",
			account_id: "account-1",
			kind: "desktop",
			name: "Studio",
			user_agent: null,
			last_seen: null,
			created_at: null,
			revoked_at: null,
		},
	],
	relayToken: "relay-token",
	relayTokenExpiresAt: "2026-05-19T12:00:00Z",
};

const sessions: VisibleSessionSummary[] = [
	{
		computerId: "desktop-1",
		sessionId: "session-1",
		agentSessionId: "session-1",
		projectId: "project-1",
		projectName: "Clipstack",
		sessionKind: "standard",
		isComputerUse: false,
		title: "Project Overview",
		lastActivityAt: "2026-05-19T10:00:00Z",
		computerName: "Studio",
		remoteVisible: true,
	},
	{
		computerId: "desktop-1",
		sessionId: "session-2",
		agentSessionId: "session-2",
		projectId: "project-1",
		projectName: "Clipstack",
		sessionKind: "standard",
		isComputerUse: false,
		title: "Refactor Shell",
		lastActivityAt: "2026-05-19T11:00:00Z",
		computerName: "Studio",
		remoteVisible: true,
	},
];

const projects: RemoteProjectSummary[] = [
	{
		computerId: "desktop-1",
		projectId: "project-1",
		projectName: "Clipstack",
	},
];

beforeEach(() => {
	Object.defineProperty(window, "matchMedia", {
		writable: true,
		value: (query: string) => ({
			matches: false,
			media: query,
			onchange: null,
			addListener: vi.fn(),
			removeListener: vi.fn(),
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}),
	});
	Object.defineProperty(window, "scrollTo", {
		writable: true,
		value: vi.fn(),
	});
	streamMock.sessions = sessions;
	streamMock.projects = projects;
	streamMock.startSession.mockClear();
	streamMock.archiveSession.mockClear();
	streamMock.accountHookMounts = 0;
	streamMock.accountHookUnmounts = 0;
	useSessionStore.setState({
		transcripts: {},
		visibleSessions: sessions,
		visibleSessionsVersion: 1,
		visibleSessionsByComputerVersion: { "desktop-1": 1 },
		remoteProjectsByComputer: { "desktop-1": projects },
		onlineComputerIds: { "desktop-1": true },
		computerPresenceKnown: true,
	});
});

afterEach(() => {
	cleanup();
	useSessionStore.setState({
		transcripts: {},
		visibleSessions: [],
		visibleSessionsVersion: 0,
		visibleSessionsByComputerVersion: {},
		remoteProjectsByComputer: {},
		onlineComputerIds: {},
		computerPresenceKnown: false,
	});
});

describe("cloud sessions shell", () => {
	it("resolves active session targets from session URLs", () => {
		expect(
			activeSessionTargetFromPathname("/sessions/desktop-1/session-1"),
		).toEqual({
			computerId: "desktop-1",
			sessionId: "session-1",
		});
		expect(
			activeSessionTargetFromPathname(
				"/sessions/desktop%20one/session%2Fwith%2Fslashes",
			),
		).toEqual({
			computerId: "desktop one",
			sessionId: "session/with/slashes",
		});
		expect(activeSessionTargetFromPathname("/sessions")).toBeNull();
	});

	it("keeps the same sidebar and account directory subscription across session switches", async () => {
		const router = renderCloudRoute("/sessions/desktop-1/session-1");

		const sidebar = await screen.findByLabelText("Desktop sessions");
		fireEvent.click(
			screen.getByRole("button", { name: "Open Refactor Shell" }),
		);

		await waitFor(() => {
			expect(router.state.location.pathname).toBe(
				"/sessions/desktop-1/session-2",
			);
		});
		expect(screen.getByLabelText("Desktop sessions")).toBe(sidebar);
		expect(streamMock.accountHookMounts).toBe(1);
		expect(streamMock.accountHookUnmounts).toBe(0);
	});

	it("keeps the shell visible while a new session request waits for the directory update", async () => {
		renderCloudRoute("/sessions/desktop-1/session-1");

		const sidebar = await screen.findByLabelText("Desktop sessions");
		fireEvent.click(
			screen.getByRole("button", { name: "New session in Clipstack" }),
		);

		expect(streamMock.startSession).toHaveBeenCalledWith(
			expect.objectContaining({
				computerId: "desktop-1",
				projectId: "project-1",
			}),
		);
		expect(screen.getByLabelText("Desktop sessions")).toBe(sidebar);
		expect(
			(
				screen.getByRole("button", {
					name: "New session in Clipstack",
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
	});

	it("renders active transcript loading inside the conversation outlet", async () => {
		renderCloudRoute("/sessions/desktop-1/session-1");

		const loading = await screen.findByLabelText("Loading");

		expect(loading.className).toContain("flex-1");
		expect(loading.className).not.toContain("min-h-dvh");
		expect(screen.getByLabelText("Desktop sessions")).toBeTruthy();
	});
});

function renderCloudRoute(path: string) {
	const router = getRouter({
		history: createMemoryHistory({ initialEntries: [path] }),
	});
	render(<RouterProvider router={router} />);
	return router;
}
