import { describe, expect, it } from "vitest";

import {
	classifyInstallSupport,
	detectInstallEnvironment,
	detectPlatform,
	isInstallableChromium,
	isIos,
	isIosSafari,
} from "./install-state";

const ANDROID_WEBVIEW =
	"Mozilla/5.0 (Linux; Android 14; Pixel 8; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/120.0.0.0 Mobile Safari/537.36";
const DESKTOP_FIREFOX =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:124.0) Gecko/20100101 Firefox/124.0";

const IPHONE_SAFARI =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const IPHONE_CHROME =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/120.0.6099.119 Mobile/15E148 Safari/604.1";
const ANDROID_CHROME =
	"Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
const DESKTOP_CHROME =
	"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

describe("isIos / isIosSafari", () => {
	it("matches iPhone Safari but not Chrome on iPhone", () => {
		expect(isIos(IPHONE_SAFARI)).toBe(true);
		expect(isIos(IPHONE_CHROME)).toBe(true);
		expect(isIosSafari(IPHONE_SAFARI)).toBe(true);
		expect(isIosSafari(IPHONE_CHROME)).toBe(false);
	});

	it("excludes Android and desktop", () => {
		expect(isIos(ANDROID_CHROME)).toBe(false);
		expect(isIos(DESKTOP_CHROME)).toBe(false);
		expect(isIosSafari(ANDROID_CHROME)).toBe(false);
	});
});

describe("classifyInstallSupport", () => {
	it("returns unsupported when no environment is provided", () => {
		expect(classifyInstallSupport(undefined, false)).toBe("unsupported");
	});

	it("prefers standalone over everything else", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: IPHONE_SAFARI, isStandalone: true },
				true,
			),
		).toBe("standalone");
	});

	it("returns prompt when beforeinstallprompt is available", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: ANDROID_CHROME, isStandalone: false },
				true,
			),
		).toBe("prompt");
	});

	it("falls back to manual-ios for iOS Safari without prompt event", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: IPHONE_SAFARI, isStandalone: false },
				false,
			),
		).toBe("manual-ios");
	});

	it("returns unsupported for non-Safari iOS browsers without prompt event", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: IPHONE_CHROME, isStandalone: false },
				false,
			),
		).toBe("unsupported");
	});

	it("falls back to manual-chromium for desktop Chromium without a prompt event", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: DESKTOP_CHROME, isStandalone: false },
				false,
			),
		).toBe("manual-chromium");
	});

	it("falls back to manual-chromium for Android Chrome without a prompt event", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: ANDROID_CHROME, isStandalone: false },
				false,
			),
		).toBe("manual-chromium");
	});

	it("returns unsupported for non-Chromium desktop browsers", () => {
		expect(
			classifyInstallSupport(
				{ userAgent: DESKTOP_FIREFOX, isStandalone: false },
				false,
			),
		).toBe("unsupported");
	});
});

describe("isInstallableChromium", () => {
	it("accepts desktop and Android Chromium", () => {
		expect(isInstallableChromium(DESKTOP_CHROME)).toBe(true);
		expect(isInstallableChromium(ANDROID_CHROME)).toBe(true);
	});

	it("rejects Firefox, iOS, and Android in-app webviews", () => {
		expect(isInstallableChromium(DESKTOP_FIREFOX)).toBe(false);
		expect(isInstallableChromium(IPHONE_CHROME)).toBe(false);
		expect(isInstallableChromium(ANDROID_WEBVIEW)).toBe(false);
	});
});

describe("detectInstallEnvironment", () => {
	function buildWindow(overrides: {
		userAgent?: string;
		standaloneMedia?: boolean;
		iosStandalone?: boolean;
	}) {
		const matchMedia = (query: string) => ({
			matches:
				query === "(display-mode: standalone)" &&
				(overrides.standaloneMedia ?? false),
		});
		const navigator: Record<string, unknown> = {
			userAgent: overrides.userAgent ?? DESKTOP_CHROME,
		};
		if (overrides.iosStandalone !== undefined) {
			navigator.standalone = overrides.iosStandalone;
		}
		return {
			navigator: navigator as unknown as Navigator,
			matchMedia: matchMedia as Window["matchMedia"],
		} as unknown as Window;
	}

	it("returns undefined for an undefined view", () => {
		expect(detectInstallEnvironment(undefined)).toBeUndefined();
	});

	it("detects standalone display mode", () => {
		const env = detectInstallEnvironment(
			buildWindow({ standaloneMedia: true }),
		);
		expect(env?.isStandalone).toBe(true);
	});

	it("detects iOS standalone flag", () => {
		const env = detectInstallEnvironment(
			buildWindow({ userAgent: IPHONE_SAFARI, iosStandalone: true }),
		);
		expect(env?.isStandalone).toBe(true);
	});

	it("reports non-standalone when nothing matches", () => {
		const env = detectInstallEnvironment(buildWindow({}));
		expect(env?.isStandalone).toBe(false);
	});
});

describe("detectPlatform", () => {
	it("returns ios for iPhone UAs", () => {
		expect(
			detectPlatform({ userAgent: IPHONE_SAFARI, isStandalone: false }),
		).toBe("ios");
		expect(
			detectPlatform({ userAgent: IPHONE_CHROME, isStandalone: false }),
		).toBe("ios");
	});

	it("returns other for non-iOS", () => {
		expect(
			detectPlatform({ userAgent: ANDROID_CHROME, isStandalone: false }),
		).toBe("other");
		expect(
			detectPlatform({ userAgent: DESKTOP_CHROME, isStandalone: false }),
		).toBe("other");
	});

	it("returns other when no environment", () => {
		expect(detectPlatform(undefined)).toBe("other");
	});
});
