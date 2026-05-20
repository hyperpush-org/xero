export interface BeforeInstallPromptEvent extends Event {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: "accepted" | "dismissed"; platform: string }>;
}

export type XeroCloudInstallSupport =
	| "standalone"
	| "prompt"
	| "manual-ios"
	| "manual-chromium"
	| "unsupported";

export type XeroCloudInstallPromptOutcome =
	| "accepted"
	| "dismissed"
	| "unavailable";

export interface XeroCloudInstallState {
	support: XeroCloudInstallSupport;
	isStandalone: boolean;
	hasPromptEvent: boolean;
	platform: "ios" | "other";
}

export interface XeroCloudInstallActions {
	promptInstall(): Promise<XeroCloudInstallPromptOutcome>;
	clearStoredPromptEvent(): void;
}

export interface XeroCloudInstallEnvironment {
	userAgent: string;
	isStandalone: boolean;
}

export function detectInstallEnvironment(
	view: Window | undefined,
): XeroCloudInstallEnvironment | undefined {
	if (!view) return undefined;
	const navigator = view.navigator;
	if (!navigator) return undefined;
	const userAgent = navigator.userAgent ?? "";
	const matchMedia =
		typeof view.matchMedia === "function" ? view.matchMedia : undefined;
	const standaloneMedia =
		matchMedia?.("(display-mode: standalone)").matches ?? false;
	const iosStandalone = readIosStandalone(navigator);
	return {
		userAgent,
		isStandalone: standaloneMedia || iosStandalone,
	};
}

export function classifyInstallSupport(
	environment: XeroCloudInstallEnvironment | undefined,
	hasPromptEvent: boolean,
): XeroCloudInstallSupport {
	if (!environment) return "unsupported";
	if (environment.isStandalone) return "standalone";
	if (hasPromptEvent) return "prompt";
	if (isIosSafari(environment.userAgent)) return "manual-ios";
	// Chromium fires `beforeinstallprompt` only once its install heuristics are
	// met (and never again for ~90 days after a dismissal, or if already
	// installed). Surface a manual affordance so the install path is always
	// discoverable rather than silently absent.
	if (isInstallableChromium(environment.userAgent)) return "manual-chromium";
	return "unsupported";
}

export function isInstallableChromium(userAgent: string): boolean {
	// iOS only installs through Safari's share sheet; Chromium-on-iOS cannot.
	if (isIos(userAgent)) return false;
	if (!/Chrome\/|Chromium\//.test(userAgent)) return false;
	// Chromium-based in-app webviews expose the engine string but cannot install.
	if (/FBAN|FBAV|Instagram|Line\/|Twitter|; wv\)/.test(userAgent)) return false;
	return true;
}

export function detectPlatform(
	environment: XeroCloudInstallEnvironment | undefined,
): "ios" | "other" {
	if (!environment) return "other";
	if (isIos(environment.userAgent)) return "ios";
	return "other";
}

export function isIos(userAgent: string): boolean {
	if (/iPad|iPhone|iPod/.test(userAgent)) return true;
	// iPadOS 13+ reports Mac in the UA, distinguishable by maxTouchPoints. We
	// can only sniff the UA string here, so fall back to "Macintosh" + touch.
	return /Macintosh/.test(userAgent) && /Mobile/.test(userAgent);
}

export function isIosSafari(userAgent: string): boolean {
	if (!isIos(userAgent)) return false;
	// Exclude in-app browsers and Chromium-on-iOS variants which cannot install.
	if (/CriOS|FxiOS|EdgiOS|OPiOS|YaBrowser|GSA/.test(userAgent)) return false;
	return /Safari/.test(userAgent);
}

function readIosStandalone(navigator: Navigator): boolean {
	const value = (navigator as Navigator & { standalone?: boolean }).standalone;
	return value === true;
}
