/** @vitest-environment jsdom */

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
	resetInstallPromptStoreForTests,
	useXeroCloudInstallState,
} from "./use-install-state";

interface FakePromptEvent extends Event {
	prompt: () => Promise<void>;
	userChoice: Promise<{ outcome: "accepted" | "dismissed"; platform: string }>;
}

function dispatchBeforeInstallPrompt(
	view: Window,
	outcome: "accepted" | "dismissed" = "accepted",
): {
	prompt: ReturnType<typeof vi.fn>;
	preventDefault: ReturnType<typeof vi.fn>;
	event: FakePromptEvent;
} {
	const promptFn = vi.fn(async () => undefined);
	const userChoice = Promise.resolve({ outcome, platform: "web" });
	const event = new Event("beforeinstallprompt") as FakePromptEvent;
	event.prompt = promptFn as unknown as () => Promise<void>;
	(event as { userChoice: typeof userChoice }).userChoice = userChoice;
	const preventDefault = vi.spyOn(event, "preventDefault");
	view.dispatchEvent(event);
	return { prompt: promptFn, preventDefault: preventDefault as never, event };
}

function stubMatchMedia(
	view: Window,
	options: { standalone: boolean } = { standalone: false },
) {
	const matches = options.standalone;
	const listeners = new Set<EventListenerOrEventListenerObject>();
	const legacyListeners = new Set<
		(this: MediaQueryList, event: MediaQueryListEvent) => void
	>();
	let mediaQuery: MediaQueryList;
	const addEventListener: MediaQueryList["addEventListener"] = (
		_type: string,
		listener: EventListenerOrEventListenerObject | null,
	) => {
		if (listener) listeners.add(listener);
	};
	const removeEventListener: MediaQueryList["removeEventListener"] = (
		_type: string,
		listener: EventListenerOrEventListenerObject | null,
	) => {
		if (listener) listeners.delete(listener);
	};
	const addListener: MediaQueryList["addListener"] = (listener) => {
		if (listener) legacyListeners.add(listener);
	};
	const removeListener: MediaQueryList["removeListener"] = (listener) => {
		if (listener) legacyListeners.delete(listener);
	};
	mediaQuery = {
		matches,
		media: "(display-mode: standalone)",
		onchange: null,
		addEventListener,
		removeEventListener,
		addListener,
		removeListener,
		dispatchEvent: () => true,
	} as MediaQueryList;
	Object.defineProperty(view, "matchMedia", {
		configurable: true,
		value: (query: string) => {
			if (query !== "(display-mode: standalone)") {
				return {
					matches: false,
					media: query,
					onchange: null,
					addEventListener: () => undefined,
					removeEventListener: () => undefined,
					addListener: () => undefined,
					removeListener: () => undefined,
					dispatchEvent: () => true,
				};
			}
			return mediaQuery;
		},
	});
	return {
		mediaQuery,
		emitChange(next: boolean) {
			(mediaQuery as { matches: boolean }).matches = next;
			const event = { matches: next } as MediaQueryListEvent;
			for (const listener of listeners) {
				if (typeof listener === "function") {
					listener.call(mediaQuery, event);
				} else {
					listener.handleEvent(event);
				}
			}
			for (const listener of legacyListeners) listener.call(mediaQuery, event);
		},
	};
}

function setUserAgent(value: string) {
	Object.defineProperty(window.navigator, "userAgent", {
		configurable: true,
		value,
	});
}

const ANDROID_CHROME =
	"Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
const IPHONE_SAFARI =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const DESKTOP_CHROME =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const DESKTOP_FIREFOX =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:124.0) Gecko/20100101 Firefox/124.0";

describe("useXeroCloudInstallState", () => {
	afterEach(() => {
		cleanup();
		resetInstallPromptStoreForTests();
		Object.defineProperty(window.navigator, "standalone", {
			configurable: true,
			value: undefined,
		});
		setUserAgent(DESKTOP_CHROME);
	});

	it("reports unsupported when no install signals are present", () => {
		setUserAgent(DESKTOP_FIREFOX);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		expect(result.current.support).toBe("unsupported");
		expect(result.current.isStandalone).toBe(false);
		expect(result.current.hasPromptEvent).toBe(false);
	});

	it("falls back to manual-chromium on desktop Chromium without a prompt event", () => {
		setUserAgent(DESKTOP_CHROME);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		expect(result.current.support).toBe("manual-chromium");
	});

	it("captures beforeinstallprompt, exposes prompt, and clears the event after use", async () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());

		let captured!: ReturnType<typeof dispatchBeforeInstallPrompt>;
		act(() => {
			captured = dispatchBeforeInstallPrompt(window, "accepted");
		});

		expect(result.current.support).toBe("prompt");
		expect(result.current.hasPromptEvent).toBe(true);
		expect(captured.preventDefault).toHaveBeenCalled();

		let outcome: string | undefined;
		await act(async () => {
			outcome = await result.current.promptInstall();
		});
		expect(outcome).toBe("accepted");
		expect(captured.prompt).toHaveBeenCalledTimes(1);
		expect(result.current.hasPromptEvent).toBe(false);
	});

	it("exposes a prompt captured before this instance mounted", () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(window);

		// An earlier-mounted instance (e.g. the desktop sidebar) is what attaches
		// the global listener and captures the one-shot event.
		const earlier = renderHook(() => useXeroCloudInstallState());
		act(() => {
			dispatchBeforeInstallPrompt(window, "accepted");
		});
		expect(earlier.result.current.support).toBe("prompt");

		// A later-mounted instance (e.g. the lazily-rendered mobile drawer) must
		// still see the already-captured event rather than report "unsupported".
		const later = renderHook(() => useXeroCloudInstallState());
		expect(later.result.current.support).toBe("prompt");
		expect(later.result.current.hasPromptEvent).toBe(true);
	});

	it("returns unavailable when promptInstall is called with no stored event", async () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		let outcome: string | undefined;
		await act(async () => {
			outcome = await result.current.promptInstall();
		});
		expect(outcome).toBe("unavailable");
	});

	it("treats appinstalled as standalone and clears the prompt event", () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());

		act(() => {
			dispatchBeforeInstallPrompt(window);
		});
		expect(result.current.support).toBe("prompt");

		act(() => {
			window.dispatchEvent(new Event("appinstalled"));
		});

		expect(result.current.isStandalone).toBe(true);
		expect(result.current.support).toBe("standalone");
		expect(result.current.hasPromptEvent).toBe(false);
	});

	it("falls back to manual-ios on iOS Safari", () => {
		setUserAgent(IPHONE_SAFARI);
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		expect(result.current.support).toBe("manual-ios");
		expect(result.current.platform).toBe("ios");
	});

	it("detects iOS standalone via navigator.standalone", () => {
		setUserAgent(IPHONE_SAFARI);
		Object.defineProperty(window.navigator, "standalone", {
			configurable: true,
			value: true,
		});
		stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		expect(result.current.support).toBe("standalone");
	});

	it("updates standalone state when display-mode changes", () => {
		setUserAgent(ANDROID_CHROME);
		const matcher = stubMatchMedia(window);
		const { result } = renderHook(() => useXeroCloudInstallState());
		expect(result.current.isStandalone).toBe(false);

		act(() => {
			matcher.emitChange(true);
		});

		expect(result.current.isStandalone).toBe(true);
		expect(result.current.support).toBe("standalone");
	});
});
