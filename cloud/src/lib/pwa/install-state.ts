export interface BeforeInstallPromptEvent extends Event {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: "accepted" | "dismissed"; platform: string }>;
}

export type XeroCloudInstallSupport =
	| "standalone"
	| "prompt"
	| "manual-ios"
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
	return "unsupported";
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
