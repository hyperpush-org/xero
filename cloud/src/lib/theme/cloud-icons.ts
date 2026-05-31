import type { ThemeDefinition } from "@xero/ui/theme";

const ICON_CANVAS_SIZE = 512;
const ICON_TILE_INSET = 30;
const ICON_TILE_RADIUS = 112;
const LOGO_VIEWBOX_SIZE = 455;
const ICON_LOGO_SCALE = ICON_CANVAS_SIZE / LOGO_VIEWBOX_SIZE;
const MASKABLE_ICON_LOGO_SCALE = 0.46;

const LOGO_PATHS = {
	primaryBottomRight:
		"M256.391 256.395H454.326V404.33C454.326 431.944 431.941 454.33 404.326 454.33H256.391V256.395Z",
	primaryTopLeft:
		"M197.936 197.941L0.000289917 197.941L0.000276984 50.0064C0.00027457 22.3921 22.386 0.00637826 50.0003 0.00637585L197.936 0.00636292L197.936 197.941Z",
	mutedBottomLeft:
		"M0 256.395H197.935V454.33H50.0001C22.3858 454.33 0 431.944 0 404.33L0 256.395Z",
	mutedTopRight:
		"M256.392 0L404.327 0C431.941 0 454.327 22.3858 454.327 50V197.935H256.392V0Z",
} as const;

export interface CloudThemeIconDataUrls {
	favicon16: string;
	favicon32: string;
	favicon48: string;
	appleTouchIcon: string;
	icon192: string;
	icon512: string;
	maskableIcon192: string;
	maskableIcon512: string;
}

interface CloudThemeIconRenderOptions {
	size: number;
	maskable?: boolean;
}

export interface CloudThemeIconApplyOptions {
	createObjectUrl?: (blob: Blob) => string;
	revokeObjectUrl?: (url: string) => void;
}

interface CloudManifestIcon {
	src: string;
	sizes: string;
	type: string;
	purpose?: "any" | "maskable";
}

interface CloudWebAppManifest {
	id: string;
	name: string;
	short_name: string;
	description: string;
	start_url: string;
	scope: string;
	display: "standalone";
	theme_color: string;
	background_color: string;
	icons: CloudManifestIcon[];
}

let activeManifestObjectUrl: string | null = null;

export function syncCloudThemeIcons(theme: ThemeDefinition): boolean {
	if (typeof document === "undefined" || !canRenderCanvasIcon()) return false;

	try {
		applyCloudThemeIconDataUrls(theme, createCloudThemeIconDataUrls(theme));
		return true;
	} catch {
		return false;
	}
}

export function createCloudThemeIconDataUrls(
	theme: ThemeDefinition,
): CloudThemeIconDataUrls {
	return {
		favicon16: createCloudThemeIconDataUrl(theme, { size: 16 }),
		favicon32: createCloudThemeIconDataUrl(theme, { size: 32 }),
		favicon48: createCloudThemeIconDataUrl(theme, { size: 48 }),
		appleTouchIcon: createCloudThemeIconDataUrl(theme, { size: 180 }),
		icon192: createCloudThemeIconDataUrl(theme, { size: 192 }),
		icon512: createCloudThemeIconDataUrl(theme, { size: 512 }),
		maskableIcon192: createCloudThemeIconDataUrl(theme, {
			size: 192,
			maskable: true,
		}),
		maskableIcon512: createCloudThemeIconDataUrl(theme, {
			size: 512,
			maskable: true,
		}),
	};
}

export function createCloudThemeIconDataUrl(
	theme: ThemeDefinition,
	{ size, maskable = false }: CloudThemeIconRenderOptions,
): string {
	const canvas = document.createElement("canvas");
	canvas.width = size;
	canvas.height = size;

	const ctx = canvas.getContext("2d");
	if (!ctx) {
		throw new Error("Canvas 2D context is unavailable.");
	}

	const unitScale = size / ICON_CANVAS_SIZE;
	ctx.clearRect(0, 0, size, size);
	ctx.save();
	ctx.scale(unitScale, unitScale);

	ctx.fillStyle = theme.colors.shellBackground || theme.colors.background;
	if (maskable) {
		ctx.fillRect(0, 0, ICON_CANVAS_SIZE, ICON_CANVAS_SIZE);
	} else {
		roundedRect(
			ctx,
			ICON_TILE_INSET,
			ICON_TILE_INSET,
			ICON_CANVAS_SIZE - ICON_TILE_INSET * 2,
			ICON_CANVAS_SIZE - ICON_TILE_INSET * 2,
			ICON_TILE_RADIUS,
		);
		ctx.fill();
	}

	const logoScale = maskable ? MASKABLE_ICON_LOGO_SCALE : ICON_LOGO_SCALE;
	const logoSize = LOGO_VIEWBOX_SIZE * logoScale;
	const offset = (ICON_CANVAS_SIZE - logoSize) / 2;

	ctx.translate(offset, offset);
	ctx.scale(logoScale, logoScale);

	ctx.fillStyle = theme.colors.primary;
	ctx.fill(new Path2D(LOGO_PATHS.primaryTopLeft));
	ctx.fill(new Path2D(LOGO_PATHS.primaryBottomRight));

	ctx.globalAlpha = 0.32;
	ctx.fillStyle = theme.colors.primary;
	ctx.fill(new Path2D(LOGO_PATHS.mutedTopRight));
	ctx.fill(new Path2D(LOGO_PATHS.mutedBottomLeft));

	ctx.restore();

	return canvas.toDataURL("image/png");
}

export function applyCloudThemeIconDataUrls(
	theme: ThemeDefinition,
	dataUrls: CloudThemeIconDataUrls,
	options: CloudThemeIconApplyOptions = {},
): void {
	if (typeof document === "undefined") return;

	updateHeadLink('link[rel="icon"][sizes="16x16"]', {
		"data-xero-generated": "theme-icon",
		href: dataUrls.favicon16,
		rel: "icon",
		sizes: "16x16",
		type: "image/png",
	});
	updateHeadLink('link[rel="icon"][sizes="32x32"]', {
		"data-xero-generated": "theme-icon",
		href: dataUrls.favicon32,
		rel: "icon",
		sizes: "32x32",
		type: "image/png",
	});
	updateHeadLink('link[rel="icon"][sizes="48x48"]', {
		"data-xero-generated": "theme-icon",
		href: dataUrls.favicon48,
		rel: "icon",
		sizes: "48x48",
		type: "image/png",
	});
	updateHeadLink('link[rel="apple-touch-icon"]', {
		"data-xero-generated": "theme-icon",
		href: dataUrls.appleTouchIcon,
		rel: "apple-touch-icon",
		sizes: "180x180",
	});

	const manifestHref = createManifestObjectUrl(
		renderCloudThemeManifest(theme, dataUrls),
		options,
	);
	if (manifestHref) {
		updateHeadLink('link[rel="manifest"]', {
			"data-xero-generated": "theme-icon",
			href: manifestHref,
			rel: "manifest",
		});
	}
}

export function renderCloudThemeManifest(
	theme: ThemeDefinition,
	dataUrls: CloudThemeIconDataUrls,
): string {
	return JSON.stringify(createCloudThemeManifest(theme, dataUrls), null, "\t");
}

export function createCloudThemeManifest(
	theme: ThemeDefinition,
	dataUrls: CloudThemeIconDataUrls,
): CloudWebAppManifest {
	return {
		id: "/",
		name: "Xero Cloud",
		short_name: "Xero",
		description:
			"Install Xero Cloud to drive your desktop coding sessions from any supported device.",
		start_url: "/?source=pwa",
		scope: "/",
		display: "standalone",
		theme_color: theme.colors.background,
		background_color: theme.colors.shellBackground || theme.colors.background,
		icons: [
			{
				src: dataUrls.icon192,
				sizes: "192x192",
				type: "image/png",
			},
			{
				src: dataUrls.icon512,
				sizes: "512x512",
				type: "image/png",
			},
			{
				src: dataUrls.maskableIcon192,
				sizes: "192x192",
				type: "image/png",
				purpose: "maskable",
			},
			{
				src: dataUrls.maskableIcon512,
				sizes: "512x512",
				type: "image/png",
				purpose: "maskable",
			},
		],
	};
}

function updateHeadLink(
	selector: string,
	attributes: Record<string, string>,
): HTMLLinkElement {
	const parent = document.head ?? document.documentElement;
	let link = parent.querySelector<HTMLLinkElement>(selector);
	if (!link) {
		link = document.createElement("link");
		parent.append(link);
	}
	for (const [name, value] of Object.entries(attributes)) {
		link.setAttribute(name, value);
	}
	return link;
}

function createManifestObjectUrl(
	manifestJson: string,
	{ createObjectUrl, revokeObjectUrl }: CloudThemeIconApplyOptions,
): string | null {
	if (typeof Blob === "undefined") return null;

	const createUrl =
		createObjectUrl ??
		(typeof URL !== "undefined" && typeof URL.createObjectURL === "function"
			? URL.createObjectURL.bind(URL)
			: null);
	if (!createUrl) return null;

	const nextUrl = createUrl(
		new Blob([manifestJson], {
			type: "application/manifest+json",
		}),
	);
	const revokeUrl =
		revokeObjectUrl ??
		(typeof URL !== "undefined" && typeof URL.revokeObjectURL === "function"
			? URL.revokeObjectURL.bind(URL)
			: null);
	if (activeManifestObjectUrl && revokeUrl) {
		revokeUrl(activeManifestObjectUrl);
	}
	activeManifestObjectUrl = nextUrl;
	return nextUrl;
}

function canRenderCanvasIcon(): boolean {
	return (
		typeof Path2D !== "undefined" &&
		typeof HTMLCanvasElement !== "undefined" &&
		typeof document.createElement === "function"
	);
}

function roundedRect(
	ctx: CanvasRenderingContext2D,
	x: number,
	y: number,
	width: number,
	height: number,
	radius: number,
): void {
	const right = x + width;
	const bottom = y + height;

	ctx.beginPath();
	ctx.moveTo(x + radius, y);
	ctx.lineTo(right - radius, y);
	ctx.quadraticCurveTo(right, y, right, y + radius);
	ctx.lineTo(right, bottom - radius);
	ctx.quadraticCurveTo(right, bottom, right - radius, bottom);
	ctx.lineTo(x + radius, bottom);
	ctx.quadraticCurveTo(x, bottom, x, bottom - radius);
	ctx.lineTo(x, y + radius);
	ctx.quadraticCurveTo(x, y, x + radius, y);
	ctx.closePath();
}
