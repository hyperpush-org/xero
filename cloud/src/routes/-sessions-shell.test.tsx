/** @vitest-environment jsdom */

import { createMemoryHistory, RouterProvider } from "@tanstack/react-router";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CloudSession } from "#/lib/auth/session";
import {
	REMOTE_COMPUTER_USE_SESSION_ID,
	type RemoteProjectSummary,
	useSessionStore,
	type VisibleSessionSummary,
} from "#/lib/relay/session-store";
import { getRouter } from "#/router";
import { activeSessionTargetFromPathname } from "./sessions";
import {
	chooseDesktopAdaptiveStreamQuality,
	isMobileTextKeyboardOpenForSnapshot,
	readDesktopControlPresentation,
	shouldRecoverDesktopWebRtcAfterFallback,
} from "./sessions.$computerId.$sessionId";

const streamMock = vi.hoisted(() => ({
	sessions: [] as VisibleSessionSummary[],
	projects: [] as RemoteProjectSummary[],
	startSession: vi.fn(() => true),
	archiveSession: vi.fn(() => true),
	clearComputerUseChat: vi.fn(() => true),
	composerProps: [] as Array<Record<string, unknown>>,
	accountHookMounts: 0,
	accountHookUnmounts: 0,
	channel: null as null | {
		on: ReturnType<typeof vi.fn>;
		off: ReturnType<typeof vi.fn>;
	},
	remoteControl: null as null | {
		available: boolean;
		reason: string | null;
		message: string | null;
		ownerDeviceId: string | null;
		startedAt: string | null;
	},
	remoteControlByComputer: {} as Record<
		string,
		{
			available: boolean;
			reason: string | null;
			message: string | null;
			ownerDeviceId: string | null;
			startedAt: string | null;
		}
	>,
}));

const DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY =
	"xero.cloud.desktopControlPresentation";

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
	Composer: (props: Record<string, unknown>) => {
		streamMock.composerProps.push(props);
		return <div data-testid="composer" />;
	},
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
				remoteControlByComputer: streamMock.remoteControlByComputer,
				startSession: streamMock.startSession,
				archiveSession: streamMock.archiveSession,
				clearComputerUseChat: streamMock.clearComputerUseChat,
			};
		},
		useSessionStream: () => ({
			channel: streamMock.channel,
			iceServers: [],
			joinRejected: false,
			remoteControl: streamMock.remoteControl,
			streamRunId: null,
			streamToken: null,
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
	cleanup();
	document.body.innerHTML = "";
	document.body.removeAttribute("style");
	document.documentElement.removeAttribute("style");
	Object.defineProperty(window, "innerWidth", {
		configurable: true,
		writable: true,
		value: 1024,
	});
	Object.defineProperty(window, "innerHeight", {
		configurable: true,
		writable: true,
		value: 768,
	});
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
	Object.defineProperty(window, "visualViewport", {
		configurable: true,
		writable: true,
		value: undefined,
	});
	Object.defineProperty(window, "scrollTo", {
		writable: true,
		value: vi.fn(),
	});
	Object.defineProperty(URL, "createObjectURL", {
		configurable: true,
		writable: true,
		value: vi.fn(() => "blob:desktop-frame"),
	});
	Object.defineProperty(URL, "revokeObjectURL", {
		configurable: true,
		writable: true,
		value: vi.fn(),
	});
	Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
		configurable: true,
		writable: true,
		value: vi.fn(),
	});
	Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", {
		configurable: true,
		writable: true,
		value: vi.fn(),
	});
	const localStorageValues = new Map<string, string>();
	Object.defineProperty(window, "localStorage", {
		configurable: true,
		writable: true,
		value: {
			clear: vi.fn(() => localStorageValues.clear()),
			getItem: vi.fn((key: string) => localStorageValues.get(key) ?? null),
			removeItem: vi.fn((key: string) => localStorageValues.delete(key)),
			setItem: vi.fn((key: string, value: string) => {
				localStorageValues.set(key, value);
			}),
		},
	});
	streamMock.sessions = sessions;
	streamMock.projects = projects;
	streamMock.startSession.mockClear();
	streamMock.archiveSession.mockClear();
	streamMock.clearComputerUseChat.mockClear();
	streamMock.composerProps = [];
	streamMock.accountHookMounts = 0;
	streamMock.accountHookUnmounts = 0;
	streamMock.channel = null;
	streamMock.remoteControl = null;
	streamMock.remoteControlByComputer = {};
	window.localStorage.removeItem(DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY);
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
	document.body.innerHTML = "";
	document.body.removeAttribute("style");
	document.documentElement.removeAttribute("style");
	useSessionStore.setState({
		transcripts: {},
		visibleSessions: [],
		visibleSessionsVersion: 0,
		visibleSessionsByComputerVersion: {},
		remoteProjectsByComputer: {},
		onlineComputerIds: {},
		computerPresenceKnown: false,
	});
	window.localStorage.removeItem(DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY);
});

describe.sequential("cloud sessions shell", () => {
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

	it("detects the mobile desktop-control presentation from the developer override", () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			writable: true,
			value: 390,
		});
		Object.defineProperty(window, "innerHeight", {
			configurable: true,
			writable: true,
			value: 844,
		});
		window.localStorage.setItem(
			DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY,
			"mobile",
		);

		expect(readDesktopControlPresentation()).toMatchObject({
			isMobile: true,
			override: "mobile",
			rotateDesktop: false,
		});

		window.localStorage.setItem(
			DESKTOP_CONTROL_PRESENTATION_STORAGE_KEY,
			"desktop",
		);
		expect(readDesktopControlPresentation()).toMatchObject({
			isMobile: false,
			override: "desktop",
			rotateDesktop: false,
		});
	});

	it("detects mobile text keyboard compression from visual viewport metrics", () => {
		expect(
			isMobileTextKeyboardOpenForSnapshot({
				baselineHeight: 844,
				layoutHeight: 844,
				textEntryFocused: true,
				viewportWidth: 390,
				visualHeight: 520,
			}),
		).toBe(true);
		expect(
			isMobileTextKeyboardOpenForSnapshot({
				baselineHeight: 844,
				layoutHeight: 844,
				textEntryFocused: false,
				viewportWidth: 390,
				visualHeight: 520,
			}),
		).toBe(false);
		expect(
			isMobileTextKeyboardOpenForSnapshot({
				baselineHeight: 844,
				layoutHeight: 844,
				textEntryFocused: true,
				viewportWidth: 1024,
				visualHeight: 520,
			}),
		).toBe(false);
	});

	it("adapts desktop stream quality from transport metrics", () => {
		expect(
			chooseDesktopAdaptiveStreamQuality({
				currentQuality: "high",
				lastChangedAt: 0,
				metrics: {
					availableOutgoingBitrateBps: 1_600_000,
					packetLoss: 8,
					packetsSent: 120,
					roundTripTimeMs: 420,
				},
				now: 7_000,
				previousMetrics: {
					packetLoss: 0,
					packetsSent: 100,
				},
				stableSamples: 2,
				state: "live",
			}),
		).toEqual({ quality: "balanced", stableSamples: 0 });

		expect(
			chooseDesktopAdaptiveStreamQuality({
				currentQuality: "balanced",
				lastChangedAt: 0,
				metrics: {
					availableOutgoingBitrateBps: 9_500_000,
					encodeLatencyMs: 24,
					packetLoss: 0,
					packetsSent: 2_000,
					roundTripTimeMs: 42,
				},
				now: 31_000,
				previousMetrics: {
					packetLoss: 0,
					packetsSent: 1_000,
				},
				stableSamples: 2,
				state: "live",
			}),
		).toEqual({ quality: "high", stableSamples: 0 });

		expect(
			chooseDesktopAdaptiveStreamQuality({
				currentQuality: "balanced",
				lastChangedAt: 0,
				metrics: null,
				now: 7_000,
				previousMetrics: null,
				stableSamples: 2,
				state: "degraded",
			}),
		).toEqual({ quality: "low", stableSamples: 0 });
	});

	it("recovers WebRTC instead of accepting screenshot fallback after live video", () => {
		expect(
			shouldRecoverDesktopWebRtcAfterFallback(
				{
					status: "degraded",
					transport: "screenshot_fallback",
					quality: "balanced",
					maxWidth: 1280,
					maxFrameRate: 24,
					metrics: null,
					message: null,
				},
				true,
			),
		).toBe(true);

		expect(
			shouldRecoverDesktopWebRtcAfterFallback(
				{
					status: "degraded",
					transport: "screenshot_fallback",
					quality: "balanced",
					maxWidth: 1280,
					maxFrameRate: 24,
					metrics: null,
					message: null,
				},
				false,
			),
		).toBe(false);

		expect(
			shouldRecoverDesktopWebRtcAfterFallback(
				{
					status: "live",
					transport: "web_rtc",
					quality: "balanced",
					maxWidth: 1280,
					maxFrameRate: 24,
					metrics: null,
					message: null,
				},
				true,
			),
		).toBe(false);
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

	it("resizes the desktop sessions sidebar from the right edge", async () => {
		renderCloudRoute("/sessions/desktop-1/session-1");

		const sidebar = await screen.findByLabelText("Desktop sessions");
		expect(sidebar.getAttribute("style")).toContain("width: 276px");
		const resizeHandle = within(sidebar).getByRole("separator", {
			name: "Resize sessions sidebar",
		});
		expect(resizeHandle.getAttribute("aria-valuenow")).toBe("276");

		fireEvent.pointerDown(resizeHandle, { button: 0, clientX: 276 });
		fireEvent.pointerMove(window, { clientX: 360 });
		fireEvent.pointerUp(window);

		await waitFor(() => {
			expect(sidebar.getAttribute("style")).toContain("width: 360px");
		});
		expect(
			window.localStorage.getItem("xero.cloud.sessionsSidebar.width.v1"),
		).toBe("360");

		fireEvent.keyDown(resizeHandle, { key: "ArrowLeft", shiftKey: true });

		await waitFor(() => {
			expect(sidebar.getAttribute("style")).toContain("width: 328px");
		});
		expect(
			window.localStorage.getItem("xero.cloud.sessionsSidebar.width.v1"),
		).toBe("328");
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

	it("passes synced approval mode controls into the cloud composer", async () => {
		useSessionStore.getState().replaceWithSnapshot("desktop-1:session-1", {
			turns: [],
			lastSeq: 1,
			isLive: true,
			availableAgents: [
				{ id: "engineer", label: "Engineer" },
				{ id: "ask", label: "Ask" },
			],
			availableModels: [
				{
					id: "openai-default:gpt-5.5",
					label: "GPT-5.5",
					modelId: "gpt-5.5",
					providerId: "openai_codex",
					providerLabel: "OpenAI Codex",
					providerProfileId: "openai-default",
					thinkingSupported: false,
					thinkingEffortOptions: [],
					defaultThinkingEffort: null,
				},
			],
			currentAgentId: "engineer",
			currentModelId: "openai-default:gpt-5.5",
			currentThinkingEffort: null,
			currentApprovalMode: "yolo",
			currentAutoCompactEnabled: true,
		});

		renderCloudRoute("/sessions/desktop-1/session-1");

		await screen.findByTestId("composer");
		const props = streamMock.composerProps.at(-1);

		expect(props).toMatchObject({
			selectedApprovalId: "yolo",
		});
		expect(props?.approvalOptions).toEqual([
			expect.objectContaining({ id: "suggest", label: "Ask first" }),
			expect.objectContaining({ id: "auto_edit", label: "Auto-edit files" }),
			expect.objectContaining({ id: "yolo", label: "Full auto" }),
		]);
		expect(typeof props?.onApprovalChange).toBe("function");
	});

	it("opens Computer Use desktop controls from the session header", async () => {
		setupComputerUseSession();
		streamMock.channel = {
			on: vi.fn(() => "frame-ref"),
			off: vi.fn(),
		};

		renderCloudRoute(`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`);

		expect(screen.queryByLabelText("Desktop")).toBeNull();
		const desktopTrigger = await screen.findByRole("button", {
			name: "Open desktop controls",
		});
		expect(desktopTrigger.className).toContain("text-[12px]");
		expect(desktopTrigger.className).toContain("gap-2");
		expect(desktopTrigger.className).toContain("hover:bg-transparent");
		expect(
			desktopTrigger.querySelector("svg")?.getAttribute("class"),
		).toContain("h-3.5");
		fireEvent.click(desktopTrigger);
		const controls = await screen.findByRole("region", {
			name: "Desktop controls",
		});
		expect(
			screen.queryByRole("dialog", { name: "Desktop controls" }),
		).toBeNull();
		const desktop = within(controls).getByLabelText("Desktop");
		expect(desktop).toBeTruthy();
		expect(controls.className).toContain("fixed");
		const toolbar = within(desktop).getByRole("toolbar", {
			name: "Desktop stream controls",
		});
		expect(toolbar.getAttribute("style")).toContain("left: 50%");
		expect(toolbar.getAttribute("style")).toContain("top: 24px");
		expect(
			within(toolbar).getByRole("button", { name: /start/i }),
		).toBeTruthy();
		const manualButton = within(toolbar).getByRole("button", {
			name: /manual/i,
		});
		expect(manualButton).toBeTruthy();
		expect(manualButton.hasAttribute("disabled")).toBe(true);
		expect(
			within(toolbar)
				.getByRole("button", { name: /stop/i })
				.hasAttribute("disabled"),
		).toBe(true);
		expect(
			within(toolbar).queryByRole("button", { name: /refresh/i }),
		).toBeNull();
		expect(
			within(toolbar).queryByRole("combobox", {
				name: "Stream quality",
			}),
		).toBeNull();
		expect(within(toolbar).queryByText("Balanced")).toBeNull();
		expect(
			within(toolbar).getByRole("button", {
				name: "Move desktop controls",
			}),
		).toBeTruthy();
		const closeButton = within(toolbar).getByRole("button", {
			name: "Close desktop controls",
		});
		expect(closeButton).toBeTruthy();
		expect(
			within(controls).queryByRole("button", { name: /emergency stop/i }),
		).toBeNull();
		expect(
			within(controls).getByText("Start desktop viewing when you are ready."),
		).toBeTruthy();
		const agentSidebar = within(controls).getByLabelText("Computer Use agent");
		expect(agentSidebar).toBeTruthy();
		expect(agentSidebar.className).not.toContain("shadow");
		expect(agentSidebar.lastElementChild?.className).toContain("flex-col");
		const agentSidebarTitle = within(agentSidebar).getAllByRole("heading", {
			name: "Computer Use",
		})[0];
		expect(agentSidebarTitle.parentElement?.parentElement?.className).toContain(
			"h-10",
		);
		const resizeHandle = within(agentSidebar).getByRole("separator", {
			name: "Resize Computer Use sidebar",
		});
		expect(resizeHandle.getAttribute("aria-valuenow")).toBe("560");
		expect(screen.getByTestId("composer")).toBeTruthy();
		expect(streamMock.composerProps.at(-1)?.density).toBe("comfortable");
		for (let index = 0; index < 6; index += 1) {
			fireEvent.keyDown(resizeHandle, { key: "ArrowRight", shiftKey: true });
		}
		await waitFor(() => {
			expect(streamMock.composerProps.at(-1)?.density).toBe("compact");
		});
		fireEvent.click(closeButton);
		await waitFor(() => {
			expect(
				screen.queryByRole("region", { name: "Desktop controls" }),
			).toBeNull();
		});
	});

	it("clears Computer Use chat without navigating to a visible replacement session", async () => {
		setupComputerUseSession();
		useSessionStore
			.getState()
			.replaceWithSnapshot(`desktop-1:${REMOTE_COMPUTER_USE_SESSION_ID}`, {
				turns: [
					{
						id: "turn-1",
						kind: "message",
						role: "user",
						sequence: 1,
						text: "Take a screenshot and show it to me",
					},
				],
				lastSeq: 1,
				isLive: false,
				availableAgents: [{ id: "computer_use", label: "Computer Use" }],
				availableModels: [],
				currentAgentId: "computer_use",
				currentModelId: null,
				currentThinkingEffort: null,
				currentApprovalMode: "suggest",
				currentAutoCompactEnabled: true,
			});

		const router = renderCloudRoute(
			`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`,
		);
		const clearButton = await screen.findByRole("button", {
			name: "Clear Computer Use chat",
		});
		expect(clearButton.className).toContain("text-[12px]");
		expect(clearButton.className).toContain("gap-2");
		expect(clearButton.className).toContain("hover:bg-transparent");
		expect(clearButton.querySelector("svg")?.getAttribute("class")).toContain(
			"h-3.5",
		);
		const separator = screen.getByText("|");
		expect(separator.getAttribute("aria-hidden")).toBe("true");

		fireEvent.click(clearButton);

		expect(streamMock.clearComputerUseChat).toHaveBeenCalledWith(
			expect.objectContaining({
				computerId: "desktop-1",
				sessionId: REMOTE_COMPUTER_USE_SESSION_ID,
				isComputerUse: true,
			}),
		);
		expect(router.state.location.pathname).toBe(
			`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`,
		);
		expect(streamMock.startSession).not.toHaveBeenCalled();
	});

	it("disables Computer Use when another cloud app owns the connection", async () => {
		setupComputerUseSession({ withSnapshot: false });
		streamMock.remoteControlByComputer = {
			"desktop-1": {
				available: false,
				reason: "computer_use_connection_already_active",
				message:
					"Stop the running connection in the other cloud app before using it here.",
				ownerDeviceId: "web-other",
				startedAt: "2026-05-29T18:29:00Z",
			},
		};

		renderCloudRoute(`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`);

		const desktopButton = await screen.findByRole("button", {
			name: "Open desktop controls",
		});
		expect((desktopButton as HTMLButtonElement).disabled).toBe(true);
		expect(
			screen.getByRole("heading", { name: "Computer Use is already in use" }),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Stop the running connection in the other cloud app before using it here.",
			),
		).toBeTruthy();
		expect(screen.queryByText("Use an app")).toBeNull();

		expect(screen.queryByTestId("composer")).toBeNull();
		expect(streamMock.composerProps).toHaveLength(0);
	});

	it("disables standard sessions when another cloud app owns the desktop", async () => {
		streamMock.remoteControlByComputer = {
			"desktop-1": {
				available: false,
				reason: "computer_use_connection_already_active",
				message:
					"Stop the running connection in the other cloud app before using it here.",
				ownerDeviceId: "web-other",
				startedAt: "2026-05-29T18:29:00Z",
			},
		};

		renderCloudRoute("/sessions/desktop-1/session-1");

		expect(
			await screen.findByRole("heading", {
				name: "Xero Cloud is already connected elsewhere",
			}),
		).toBeTruthy();
		expect(screen.queryByLabelText("Loading")).toBeNull();

		expect(screen.queryByTestId("composer")).toBeNull();
		expect(streamMock.composerProps).toHaveLength(0);
	});

	it("renders Computer Use after reload before the synthetic session has a transcript snapshot", async () => {
		setupComputerUseSession({ withSnapshot: false });

		renderCloudRoute(`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`);

		expect(
			await screen.findByRole("button", { name: "Open desktop controls" }),
		).toBeTruthy();
		expect(screen.queryByLabelText("Loading")).toBeNull();
		expect(
			document.querySelector("header span[title='Computer Use']")?.className,
		).toContain("text-foreground");
		expect(
			document.querySelector("header span[title='Computer Use']")?.parentElement
				?.className,
		).toContain("-translate-y-[2px]");
		expect(screen.getByRole("heading", { name: "Computer Use" })).toBeTruthy();
		expect(
			screen.getByText(
				"Give a concrete instruction and Xero will use the available computer and project tools.",
			),
		).toBeTruthy();
		expect(screen.getByTestId("composer")).toBeTruthy();
	});

	it("shows session directory skeletons while normal desktop sessions reconcile", async () => {
		setupComputerUseSession({
			withSnapshot: false,
			includeStandardSessions: false,
		});
		useSessionStore.setState({
			visibleSessionsByComputerVersion: {},
		});

		renderCloudRoute(`/sessions/desktop-1/${REMOTE_COMPUTER_USE_SESSION_ID}`);

		expect(
			await screen.findByRole("button", { name: "Open desktop controls" }),
		).toBeTruthy();
		expect(
			screen.getAllByRole("status", {
				name: "Loading desktop sessions",
			}).length,
		).toBeGreaterThan(0);
		expect(screen.queryByText("No sessions yet")).toBeNull();
	});
});

function setupComputerUseSession({
	withSnapshot = true,
	includeStandardSessions = true,
}: {
	withSnapshot?: boolean;
	includeStandardSessions?: boolean;
} = {}) {
	const computerUseSession: VisibleSessionSummary = {
		computerId: "desktop-1",
		sessionId: REMOTE_COMPUTER_USE_SESSION_ID,
		agentSessionId: REMOTE_COMPUTER_USE_SESSION_ID,
		projectId: "global-computer-use",
		projectName: null,
		sessionKind: "computer_use",
		isComputerUse: true,
		title: "Computer Use",
		lastActivityAt: null,
		computerName: "Studio",
		remoteVisible: true,
	};
	const nextSessions = includeStandardSessions
		? [computerUseSession, ...sessions]
		: [computerUseSession];
	streamMock.sessions = nextSessions;
	useSessionStore.setState({
		visibleSessions: nextSessions,
		visibleSessionsVersion: 2,
	});
	if (withSnapshot) {
		useSessionStore
			.getState()
			.replaceWithSnapshot(`desktop-1:${REMOTE_COMPUTER_USE_SESSION_ID}`, {
				turns: [],
				lastSeq: 1,
				isLive: false,
				availableAgents: [{ id: "computer_use", label: "Computer Use" }],
				availableModels: [],
				currentAgentId: "computer_use",
				currentModelId: null,
				currentThinkingEffort: null,
				currentApprovalMode: "suggest",
				currentAutoCompactEnabled: true,
			});
	}
	return computerUseSession;
}

function renderCloudRoute(path: string) {
	const router = getRouter({
		history: createMemoryHistory({ initialEntries: [path] }),
	});
	render(<RouterProvider router={router} />);
	return router;
}
