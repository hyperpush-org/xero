import { useCallback, useEffect, useRef, useState } from "react";

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

export interface UseXeroCloudInstallStateOptions {
	view?: Window;
}

export function useXeroCloudInstallState(
	options: UseXeroCloudInstallStateOptions = {},
): XeroCloudInstallState & XeroCloudInstallActions {
	const viewOverride = options.view;
	const promptEventRef = useRef<BeforeInstallPromptEvent | null>(null);
	const [hasPromptEvent, setHasPromptEvent] = useState(false);
	const [isStandalone, setIsStandalone] = useState(() => {
		const view = viewOverride ?? safeWindow();
		return detectInstallEnvironment(view)?.isStandalone ?? false;
	});
	const [supportPlatform, setSupportPlatform] = useState<"ios" | "other">(() =>
		detectPlatform(detectInstallEnvironment(viewOverride ?? safeWindow())),
	);
	const [userAgent, setUserAgent] = useState<string>(() => {
		return (
			detectInstallEnvironment(viewOverride ?? safeWindow())?.userAgent ?? ""
		);
	});

	useEffect(() => {
		const view = viewOverride ?? safeWindow();
		if (!view) return;
		const environment = detectInstallEnvironment(view);
		if (!environment) return;
		setIsStandalone(environment.isStandalone);
		setUserAgent(environment.userAgent);
		setSupportPlatform(detectPlatform(environment));

		const handleBeforeInstallPrompt = (event: Event) => {
			event.preventDefault();
			promptEventRef.current = event as BeforeInstallPromptEvent;
			setHasPromptEvent(true);
		};
		const handleAppInstalled = () => {
			promptEventRef.current = null;
			setHasPromptEvent(false);
			setIsStandalone(true);
		};

		view.addEventListener(
			BEFORE_INSTALL_PROMPT_EVENT,
			handleBeforeInstallPrompt as EventListener,
		);
		view.addEventListener(APP_INSTALLED_EVENT, handleAppInstalled);

		const mediaQuery =
			typeof view.matchMedia === "function"
				? view.matchMedia(DISPLAY_MODE_QUERY)
				: undefined;
		const handleDisplayModeChange = (event: MediaQueryListEvent) => {
			setIsStandalone(event.matches);
		};
		if (mediaQuery) addDisplayModeListener(mediaQuery, handleDisplayModeChange);

		return () => {
			view.removeEventListener(
				BEFORE_INSTALL_PROMPT_EVENT,
				handleBeforeInstallPrompt as EventListener,
			);
			view.removeEventListener(APP_INSTALLED_EVENT, handleAppInstalled);
			if (mediaQuery)
				removeDisplayModeListener(mediaQuery, handleDisplayModeChange);
		};
	}, [viewOverride]);

	const promptInstall = useCallback(async () => {
		const event = promptEventRef.current;
		if (!event) return "unavailable" as const;
		try {
			await event.prompt();
			const choice = await event.userChoice;
			return choice.outcome;
		} finally {
			promptEventRef.current = null;
			setHasPromptEvent(false);
		}
	}, []);

	const clearStoredPromptEvent = useCallback(() => {
		promptEventRef.current = null;
		setHasPromptEvent(false);
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
