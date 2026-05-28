/** @vitest-environment jsdom */

import { THEMES } from "@xero/ui/theme";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	applyCloudThemeIconDataUrls,
	type CloudThemeIconDataUrls,
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
});
