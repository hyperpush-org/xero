/** @vitest-environment jsdom */

import { THEMES } from "@xero/ui/theme";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	applyCloudThemeIconDataUrls,
	type CloudThemeIconDataUrls,
	createCloudThemeIconDataUrl,
	createCloudThemeManifest,
} from "./cloud-icons";

const ICON_DATA_URLS: CloudThemeIconDataUrls = {
	favicon16: "data:image/png;base64,favicon16",
	favicon32: "data:image/png;base64,favicon32",
	favicon48: "data:image/png;base64,favicon48",
	appleTouchIcon: "data:image/png;base64,apple",
	icon192: "data:image/png;base64,icon192",
	icon512: "data:image/png;base64,icon512",
	maskableIcon192: "data:image/png;base64,maskable192",
	maskableIcon512: "data:image/png;base64,maskable512",
};

afterEach(() => {
	document.head.innerHTML = "";
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe("cloud themed icons", () => {
	it("builds a PWA manifest from the active theme colors and generated icons", () => {
		const theme =
			THEMES.find((candidate) => candidate.id === "midnight") ?? THEMES[0];

		const manifest = createCloudThemeManifest(theme, ICON_DATA_URLS);

		expect(manifest.theme_color).toBe(theme.colors.background);
		expect(manifest.background_color).toBe(theme.colors.shellBackground);
		expect(manifest.icons).toEqual([
			{
				src: ICON_DATA_URLS.icon192,
				sizes: "192x192",
				type: "image/png",
			},
			{
				src: ICON_DATA_URLS.icon512,
				sizes: "512x512",
				type: "image/png",
			},
			{
				src: ICON_DATA_URLS.maskableIcon192,
				sizes: "192x192",
				type: "image/png",
				purpose: "maskable",
			},
			{
				src: ICON_DATA_URLS.maskableIcon512,
				sizes: "512x512",
				type: "image/png",
				purpose: "maskable",
			},
		]);
	});

	it("updates favicon, touch icon, and manifest links in the document head", async () => {
		document.head.innerHTML = `
			<link rel="manifest" href="/manifest.webmanifest">
			<link rel="icon" type="image/png" sizes="16x16" href="/icons/favicon-16x16.png">
			<link rel="icon" type="image/png" sizes="32x32" href="/icons/favicon-32x32.png">
			<link rel="icon" type="image/png" sizes="48x48" href="/icons/favicon-48x48.png">
			<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
		`;
		const theme =
			THEMES.find((candidate) => candidate.id === "midnight") ?? THEMES[0];
		const createObjectUrl = vi.fn((_blob: Blob) => "blob:xero-theme-manifest");

		applyCloudThemeIconDataUrls(theme, ICON_DATA_URLS, { createObjectUrl });

		expect(
			document
				.querySelector('link[rel="icon"][sizes="16x16"]')
				?.getAttribute("href"),
		).toBe(ICON_DATA_URLS.favicon16);
		expect(
			document
				.querySelector('link[rel="icon"][sizes="32x32"]')
				?.getAttribute("href"),
		).toBe(ICON_DATA_URLS.favicon32);
		expect(
			document
				.querySelector('link[rel="icon"][sizes="48x48"]')
				?.getAttribute("href"),
		).toBe(ICON_DATA_URLS.favicon48);
		expect(
			document
				.querySelector('link[rel="apple-touch-icon"]')
				?.getAttribute("href"),
		).toBe(ICON_DATA_URLS.appleTouchIcon);
		expect(
			document.querySelector('link[rel="manifest"]')?.getAttribute("href"),
		).toBe("blob:xero-theme-manifest");

		const manifestBlob = createObjectUrl.mock.calls[0]?.[0];
		expect(manifestBlob).toBeInstanceOf(Blob);
		if (!manifestBlob) throw new Error("Expected a generated manifest blob.");
		await expect(manifestBlob.text()).resolves.toContain(
			`"theme_color": "${theme.colors.background}"`,
		);
	});

	it("renders non-maskable themed icons at the full static favicon mark size", () => {
		const theme =
			THEMES.find((candidate) => candidate.id === "midnight") ?? THEMES[0];
		const { drawCalls, toDataURL } = installCanvasMock();

		createCloudThemeIconDataUrl(theme, { size: 512 });

		const scaleCalls = drawCalls.filter((call) => call.name === "scale");
		const translateCalls = drawCalls.filter(
			(call) => call.name === "translate",
		);
		expect(scaleCalls[0]?.args).toEqual([1, 1]);
		expect(scaleCalls[1]?.args[0]).toBeCloseTo(512 / 455, 6);
		expect(scaleCalls[1]?.args[1]).toBeCloseTo(512 / 455, 6);
		expect(translateCalls[0]?.args[0]).toBeCloseTo(0, 6);
		expect(translateCalls[0]?.args[1]).toBeCloseTo(0, 6);
		expect(toDataURL).toHaveBeenCalledWith("image/png");
	});

	it("keeps maskable themed icons inside the install safe area", () => {
		const theme =
			THEMES.find((candidate) => candidate.id === "midnight") ?? THEMES[0];
		const { drawCalls } = installCanvasMock();

		createCloudThemeIconDataUrl(theme, { size: 512, maskable: true });

		const scaleCalls = drawCalls.filter((call) => call.name === "scale");
		const translateCalls = drawCalls.filter(
			(call) => call.name === "translate",
		);
		expect(scaleCalls[1]?.args[0]).toBeCloseTo(0.46, 6);
		expect(scaleCalls[1]?.args[1]).toBeCloseTo(0.46, 6);
		expect(translateCalls[0]?.args[0]).toBeCloseTo((512 - 455 * 0.46) / 2, 6);
		expect(translateCalls[0]?.args[1]).toBeCloseTo((512 - 455 * 0.46) / 2, 6);
	});

	it("renders every logo block from the selected primary color", () => {
		const theme = {
			...THEMES[0],
			colors: {
				...THEMES[0].colors,
				primary: "#1d7afc",
				foreground: "#101828",
			},
		};
		const { drawCalls } = installCanvasMock();

		createCloudThemeIconDataUrl(theme, { size: 512 });

		const logoFills = drawCalls
			.filter((call) => call.name === "fill")
			.slice(-4);
		expect(logoFills.map((call) => call.fillStyle)).toEqual([
			theme.colors.primary,
			theme.colors.primary,
			theme.colors.primary,
			theme.colors.primary,
		]);
		expect(logoFills.map((call) => call.globalAlpha)).toEqual([
			1, 1, 0.32, 0.32,
		]);
	});
});

interface DrawCall {
	name: string;
	args: number[];
	fillStyle?: string;
	globalAlpha?: number;
}

function installCanvasMock(): {
	drawCalls: DrawCall[];
	toDataURL: ReturnType<typeof vi.fn>;
} {
	const drawCalls: DrawCall[] = [];
	const record = (
		name: string,
		args: number[] = [],
		state?: Pick<DrawCall, "fillStyle" | "globalAlpha">,
	) => {
		drawCalls.push({ name, args, ...state });
	};
	let fillStyle = "";
	let globalAlpha = 1;
	const ctx = {
		clearRect: vi.fn((...args: number[]) => record("clearRect", args)),
		save: vi.fn(() => record("save")),
		scale: vi.fn((x: number, y: number) => record("scale", [x, y])),
		fillRect: vi.fn((...args: number[]) => record("fillRect", args)),
		translate: vi.fn((x: number, y: number) => record("translate", [x, y])),
		fill: vi.fn(() => record("fill", [], { fillStyle, globalAlpha })),
		beginPath: vi.fn(() => record("beginPath")),
		moveTo: vi.fn((x: number, y: number) => record("moveTo", [x, y])),
		lineTo: vi.fn((x: number, y: number) => record("lineTo", [x, y])),
		quadraticCurveTo: vi.fn((...args: number[]) =>
			record("quadraticCurveTo", args),
		),
		closePath: vi.fn(() => record("closePath")),
		restore: vi.fn(() => record("restore")),
	} as unknown as CanvasRenderingContext2D;
	Object.defineProperties(ctx, {
		fillStyle: {
			get: () => fillStyle,
			set: (next: string) => {
				fillStyle = next;
			},
		},
		globalAlpha: {
			get: () => globalAlpha,
			set: (next: number) => {
				globalAlpha = next;
			},
		},
	});
	const toDataURL = vi.fn(() => "data:image/png;base64,themed");
	const canvas = {
		width: 0,
		height: 0,
		getContext: vi.fn(() => ctx),
		toDataURL,
	} as unknown as HTMLCanvasElement;
	const originalCreateElement = document.createElement.bind(document);

	vi.spyOn(document, "createElement").mockImplementation(
		(tagName: string, options?: ElementCreationOptions) => {
			if (tagName === "canvas") return canvas;
			return originalCreateElement(tagName, options);
		},
	);
	vi.stubGlobal("Path2D", class FakePath2D {});

	return { drawCalls, toDataURL };
}
