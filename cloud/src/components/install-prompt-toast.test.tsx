/** @vitest-environment jsdom */

import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { resetInstallPromptStoreForTests } from "#/lib/pwa/use-install-state";

import { InstallPromptToast } from "./install-prompt-toast";

const ANDROID_CHROME =
	"Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
const IPHONE_SAFARI =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const DESKTOP_CHROME =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const DESKTOP_FIREFOX =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:124.0) Gecko/20100101 Firefox/124.0";

function setUserAgent(value: string) {
	Object.defineProperty(window.navigator, "userAgent", {
		configurable: true,
		value,
	});
}

function stubMatchMedia(standalone: boolean) {
	Object.defineProperty(window, "matchMedia", {
		configurable: true,
		value: (query: string) => ({
			matches: query === "(display-mode: standalone)" && standalone,
			media: query,
			onchange: null,
			addEventListener: () => undefined,
			removeEventListener: () => undefined,
			addListener: () => undefined,
			removeListener: () => undefined,
			dispatchEvent: () => true,
		}),
	});
}

function dispatchBeforeInstallPrompt() {
	const promptFn = vi.fn(async () => undefined);
	const userChoice = Promise.resolve({ outcome: "accepted", platform: "web" });
	const event = new Event("beforeinstallprompt") as Event & {
		prompt: () => Promise<void>;
		userChoice: typeof userChoice;
	};
	event.prompt = promptFn;
	event.userChoice = userChoice;
	window.dispatchEvent(event);
	return { promptFn };
}

// This vitest environment ships an empty `localStorage` object with no methods,
// so install a Map-backed stub the production helper can read/write against.
function installFakeLocalStorage() {
	const store = new Map<string, string>();
	Object.defineProperty(window, "localStorage", {
		configurable: true,
		value: {
			getItem: (key: string) => store.get(key) ?? null,
			setItem: (key: string, value: string) => {
				store.set(key, String(value));
			},
			removeItem: (key: string) => {
				store.delete(key);
			},
			clear: () => store.clear(),
		},
	});
}

describe("InstallPromptToast", () => {
	beforeEach(() => {
		installFakeLocalStorage();
	});

	afterEach(() => {
		cleanup();
		resetInstallPromptStoreForTests();
		window.localStorage.clear();
		Object.defineProperty(window.navigator, "standalone", {
			configurable: true,
			value: undefined,
		});
		setUserAgent(DESKTOP_CHROME);
	});

	it("nudges proactively on desktop Chromium without a prompt event", async () => {
		setUserAgent(DESKTOP_CHROME);
		stubMatchMedia(false);
		render(<InstallPromptToast />);
		expect(await screen.findByText(/Install Xero Cloud/i)).toBeTruthy();
	});

	it("stays silent on non-installable browsers", async () => {
		setUserAgent(DESKTOP_FIREFOX);
		stubMatchMedia(false);
		render(<InstallPromptToast />);
		await Promise.resolve();
		expect(screen.queryByText(/Install Xero Cloud/i)).toBeNull();
	});

	it("stays silent when already running standalone", () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(true);
		render(<InstallPromptToast />);
		expect(screen.queryByText(/Install Xero Cloud/i)).toBeNull();
	});

	it("triggers the native prompt when one is available", async () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(false);
		render(<InstallPromptToast />);

		let captured!: { promptFn: ReturnType<typeof vi.fn> };
		act(() => {
			captured = dispatchBeforeInstallPrompt();
		});

		const install = await screen.findByRole("button", { name: /^Install$/i });
		fireEvent.click(install);
		await waitFor(() => expect(captured.promptFn).toHaveBeenCalledTimes(1));
	});

	it("opens manual instructions on iOS Safari", async () => {
		setUserAgent(IPHONE_SAFARI);
		stubMatchMedia(false);
		render(<InstallPromptToast />);

		const install = await screen.findByRole("button", { name: /^Install$/i });
		fireEvent.click(install);
		await waitFor(() => {
			expect(screen.getByText(/Add to Home Screen/i)).toBeTruthy();
		});
	});

	it("hides and persists when dismissed, and does not return on remount", async () => {
		setUserAgent(DESKTOP_CHROME);
		stubMatchMedia(false);
		const first = render(<InstallPromptToast />);
		await screen.findByText(/Install Xero Cloud/i);

		fireEvent.click(
			screen.getByRole("button", { name: /dismiss install prompt/i }),
		);
		expect(screen.queryByText(/Install Xero Cloud/i)).toBeNull();

		first.unmount();
		resetInstallPromptStoreForTests();
		setUserAgent(DESKTOP_CHROME);
		stubMatchMedia(false);
		render(<InstallPromptToast />);
		await Promise.resolve();
		expect(screen.queryByText(/Install Xero Cloud/i)).toBeNull();
	});
});
