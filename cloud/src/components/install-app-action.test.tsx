/** @vitest-environment jsdom */

import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { InstallAppAction } from "./install-app-action";

const ANDROID_CHROME =
	"Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
const IPHONE_SAFARI =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const DESKTOP_CHROME =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

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

function dispatchBeforeInstallPrompt(
	outcome: "accepted" | "dismissed" = "accepted",
) {
	const promptFn = vi.fn(async () => undefined);
	const userChoice = Promise.resolve({ outcome, platform: "web" });
	const event = new Event("beforeinstallprompt") as Event & {
		prompt: () => Promise<void>;
		userChoice: typeof userChoice;
	};
	event.prompt = promptFn;
	event.userChoice = userChoice;
	window.dispatchEvent(event);
	return { promptFn };
}

describe("InstallAppAction", () => {
	afterEach(() => {
		cleanup();
		Object.defineProperty(window.navigator, "standalone", {
			configurable: true,
			value: undefined,
		});
		setUserAgent(DESKTOP_CHROME);
	});

	it("renders nothing when no install path is available", () => {
		setUserAgent(DESKTOP_CHROME);
		stubMatchMedia(false);
		const { container } = render(<InstallAppAction />);
		expect(container.firstChild).toBeNull();
	});

	it("renders nothing when running in standalone mode", () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(true);
		const { container } = render(<InstallAppAction />);
		expect(container.firstChild).toBeNull();
	});

	it("triggers the install prompt when Chromium beforeinstallprompt is available", async () => {
		setUserAgent(ANDROID_CHROME);
		stubMatchMedia(false);
		render(<InstallAppAction />);

		let captured!: { promptFn: ReturnType<typeof vi.fn> };
		act(() => {
			captured = dispatchBeforeInstallPrompt("accepted");
		});

		const button = await screen.findByRole("button", {
			name: /install xero cloud/i,
		});
		fireEvent.click(button);
		await waitFor(() => expect(captured.promptFn).toHaveBeenCalledTimes(1));
	});

	it("shows manual install instructions on iOS Safari", async () => {
		setUserAgent(IPHONE_SAFARI);
		stubMatchMedia(false);
		render(<InstallAppAction />);

		const button = await screen.findByRole("button", {
			name: /install xero cloud/i,
		});
		fireEvent.click(button);

		await waitFor(() => {
			expect(screen.getByText(/Add to Home Screen/i)).toBeTruthy();
		});
		expect(screen.getByText(/Share/i)).toBeTruthy();
	});

	it("hides the manual instructions dialog when launched in standalone mode", () => {
		setUserAgent(IPHONE_SAFARI);
		Object.defineProperty(window.navigator, "standalone", {
			configurable: true,
			value: true,
		});
		stubMatchMedia(false);
		const { container } = render(<InstallAppAction />);
		expect(container.firstChild).toBeNull();
		expect(screen.queryByText(/Add to Home Screen/i)).toBeNull();
	});
});
