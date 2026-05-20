import { useCallback, useEffect, useState } from "react";

import {
	type BeforeInstallPromptEvent,
	classifyInstallSupport,
	detectInstallEnvironment,
	detectPlatform,
	type XeroCloudInstallActions,
	type XeroCloudInstallState,
} from "./install-state";

const BEFORE_INSTALL_PROMPT_EVENT = "beforeinstallprompt";
const APP_INSTALLED_EVENT = "appinstalled";
const DISPLAY_MODE_QUERY = "(display-mode: standalone)";

// `beforeinstallprompt` fires once per page load. Each hook instance used to
// capture it in its own ref, so whichever instance was mounted at that moment
// "owned" the event. Instances mounted later — e.g. the mobile session drawer,
// whose content Radix only mounts when opened — never saw it and fell back to
// "unsupported". The captured event therefore lives in a module-level store so
// every instance, whenever it mounts, can read the already-captured event.
let storedPromptEvent: BeforeInstallPromptEvent | null = null;
let storedInstalled = false;
const promptSubscribers = new Set<() => void>();
let promptListenersView: Window | null = null;

function notifyPromptSubscribers(): void {
	for (const subscriber of promptSubscribers) subscriber();
}

function ensurePromptListeners(view: Window): void {
	if (promptListenersView === view) return;
	promptListenersView = view;
	view.addEventListener(BEFORE_INSTALL_PROMPT_EVENT, (event) => {
		event.preventDefault();
		storedPromptEvent = event as BeforeInstallPromptEvent;
		notifyPromptSubscribers();
	});
	view.addEventListener(APP_INSTALLED_EVENT, () => {
		storedPromptEvent = null;
		storedInstalled = true;
		notifyPromptSubscribers();
	});
}

function consumePromptEvent(): BeforeInstallPromptEvent | null {
	const event = storedPromptEvent;
	storedPromptEvent = null;
	notifyPromptSubscribers();
	return event;
}

/** Test-only: clears the shared prompt store between cases. */
export function resetInstallPromptStoreForTests(): void {
	storedPromptEvent = null;
	storedInstalled = false;
	promptSubscribers.clear();
}

export interface UseXeroCloudInstallStateOptions {
	view?: Window;
}

export function useXeroCloudInstallState(
	options: UseXeroCloudInstallStateOptions = {},
): XeroCloudInstallState & XeroCloudInstallActions {
	const viewOverride = options.view;
	const [promptEvent, setPromptEvent] =
		useState<BeforeInstallPromptEvent | null>(() =>
			viewOverride ? storedPromptEvent : null,
		);
	const [isStandalone, setIsStandalone] = useState(() => {
		const view = viewOverride;
		return (
			storedInstalled || (detectInstallEnvironment(view)?.isStandalone ?? false)
		);
	});
	const [supportPlatform, setSupportPlatform] = useState<"ios" | "other">(() =>
		detectPlatform(detectInstallEnvironment(viewOverride)),
	);
	const [userAgent, setUserAgent] = useState<string>(() => {
		return detectInstallEnvironment(viewOverride)?.userAgent ?? "";
	});

	useEffect(() => {
		const view = viewOverride ?? safeWindow();
		if (!view) return;
		const environment = detectInstallEnvironment(view);
		if (!environment) return;
		setUserAgent(environment.userAgent);
		setSupportPlatform(detectPlatform(environment));

		ensurePromptListeners(view);
		const sync = () => {
			setPromptEvent(storedPromptEvent);
			setIsStandalone(
				(current) => current || storedInstalled || environment.isStandalone,
			);
		};
		// Adopt any event captured before this instance mounted, then track future
		// changes from the shared store.
		sync();
		promptSubscribers.add(sync);

		const mediaQuery =
			typeof view.matchMedia === "function"
				? view.matchMedia(DISPLAY_MODE_QUERY)
				: undefined;
		const handleDisplayModeChange = (event: MediaQueryListEvent) => {
			setIsStandalone(event.matches);
		};
		if (mediaQuery) addDisplayModeListener(mediaQuery, handleDisplayModeChange);

		return () => {
			promptSubscribers.delete(sync);
			if (mediaQuery)
				removeDisplayModeListener(mediaQuery, handleDisplayModeChange);
		};
	}, [viewOverride]);

	const hasPromptEvent = promptEvent !== null;

	const promptInstall = useCallback(async () => {
		const event = consumePromptEvent();
		if (!event) return "unavailable" as const;
		await event.prompt();
		const choice = await event.userChoice;
		return choice.outcome;
	}, []);

	const clearStoredPromptEvent = useCallback(() => {
		consumePromptEvent();
	}, []);

	const support = classifyInstallSupport(
		{ userAgent, isStandalone },
		hasPromptEvent,
	);

	return {
		support,
		isStandalone,
		hasPromptEvent,
		platform: supportPlatform,
		promptInstall,
		clearStoredPromptEvent,
	};
}

function safeWindow(): Window | undefined {
	if (typeof window === "undefined") return undefined;
	return window;
}

function addDisplayModeListener(
	media: MediaQueryList,
	listener: (event: MediaQueryListEvent) => void,
): void {
	if (typeof media.addEventListener === "function") {
		media.addEventListener("change", listener);
		return;
	}
	(
		media as MediaQueryList & {
			addListener: (listener: (event: MediaQueryListEvent) => void) => void;
		}
	).addListener(listener);
}

function removeDisplayModeListener(
	media: MediaQueryList,
	listener: (event: MediaQueryListEvent) => void,
): void {
	if (typeof media.removeEventListener === "function") {
		media.removeEventListener("change", listener);
		return;
	}
	(
		media as MediaQueryList & {
			removeListener: (listener: (event: MediaQueryListEvent) => void) => void;
		}
	).removeListener(listener);
}
