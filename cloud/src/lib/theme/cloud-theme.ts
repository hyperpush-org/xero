import {
	applyThemeToDocument,
	isCustomThemeId,
	isThemeDefinition,
	THEMES,
	type ThemeDefinition,
} from "@xero/ui/theme";

import type { RuntimeEnvelope } from "#/lib/relay/envelope";

export const CLOUD_THEME_SCHEMA = "xero.cloud_theme.v1";

export function applyRemoteThemeEnvelope(envelope: RuntimeEnvelope): boolean {
	const theme = themeFromRemoteEnvelope(envelope);
	if (!theme) return false;
	applyThemeToDocument(theme, { inlineColors: isCustomThemeId(theme.id) });
	updateThemeColorMeta(theme);
	return true;
}

export function themeFromRemoteEnvelope(
	envelope: RuntimeEnvelope,
): ThemeDefinition | null {
	if (envelope.session_id !== "__theme__") return null;
	if (!isRecord(envelope.payload)) return null;
	if (envelope.payload.schema !== CLOUD_THEME_SCHEMA) return null;

	const themeId = stringField(envelope.payload, "themeId", "theme_id");
	if (!themeId) return null;
	if (isCustomThemeId(themeId)) {
		const customTheme = envelope.payload.customTheme;
		if (!isThemeDefinition(customTheme) || customTheme.id !== themeId) {
			return null;
		}
		return customTheme;
	}

	return THEMES.find((theme) => theme.id === themeId) ?? null;
}

function updateThemeColorMeta(theme: ThemeDefinition): void {
	if (typeof document === "undefined") return;
	for (const selector of [
		'meta[name="theme-color"]',
		'meta[name="msapplication-TileColor"]',
	]) {
		const meta = document.querySelector<HTMLMetaElement>(selector);
		if (meta) meta.content = theme.colors.background;
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function stringField(
	record: Record<string, unknown>,
	...keys: string[]
): string | null {
	for (const key of keys) {
		const value = record[key];
		if (typeof value !== "string") continue;
		const trimmed = value.trim();
		if (trimmed) return trimmed;
	}
	return null;
}
