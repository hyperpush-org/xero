/** @vitest-environment jsdom */

import { THEMES } from "@xero/ui/theme";
import { afterEach, describe, expect, it } from "vitest";
import type { RuntimeEnvelope } from "#/lib/relay/envelope";
import {
	applyRemoteThemeEnvelope,
	themeFromRemoteEnvelope,
} from "./cloud-theme";

function envelope(payload: unknown): RuntimeEnvelope {
	return {
		v: 1,
		seq: 1,
		computer_id: "desktop-1",
		session_id: "__theme__",
		kind: "event",
		payload,
	};
}

afterEach(() => {
	document.documentElement.removeAttribute("class");
	document.documentElement.removeAttribute("style");
	document.documentElement.removeAttribute("data-theme");
});

describe("cloud theme sync", () => {
	it("resolves built-in themes from the shared registry by id only", () => {
		const theme = themeFromRemoteEnvelope(
			envelope({
				schema: "xero.cloud_theme.v1",
				themeId: "midnight",
			}),
		);

		expect(theme).toBe(THEMES.find((candidate) => candidate.id === "midnight"));
	});

	it("ignores token payloads for built-in themes", () => {
		const theme = themeFromRemoteEnvelope(
			envelope({
				schema: "xero.cloud_theme.v1",
				themeId: "midnight",
				customTheme: {
					id: "midnight",
					name: "Fake Midnight",
					description: "Should not win",
					appearance: "light",
					shiki: "github-light",
					colors: {},
					editor: {},
				},
			}),
		);

		expect(theme?.name).toBe("Midnight");
	});

	it("applies built-in themes by class without inline tokens", () => {
		document.documentElement.className = "theme-custom-ember light";
		document.documentElement.style.setProperty("--background", "#ffeeee");

		const applied = applyRemoteThemeEnvelope(
			envelope({
				schema: "xero.cloud_theme.v1",
				themeId: "midnight",
			}),
		);

		expect(applied).toBe(true);
		expect(document.documentElement.dataset.theme).toBe("midnight");
		expect(document.documentElement.classList.contains("theme-midnight")).toBe(
			true,
		);
		expect(document.documentElement.classList.contains("dark")).toBe(true);
		expect(
			document.documentElement.style.getPropertyValue("--background"),
		).toBe("");
	});

	it("applies custom themes with their runtime token values", () => {
		const base = THEMES[0];
		const custom = {
			...base,
			id: "custom-ember",
			name: "Ember",
			colors: {
				...base.colors,
				background: "#fff1e8",
				foreground: "#2a1710",
				primary: "#b7431d",
			},
		};

		const applied = applyRemoteThemeEnvelope(
			envelope({
				schema: "xero.cloud_theme.v1",
				themeId: "custom-ember",
				customTheme: custom,
			}),
		);

		expect(applied).toBe(true);
		expect(document.documentElement.dataset.theme).toBe("custom-ember");
		expect(
			document.documentElement.classList.contains("theme-custom-ember"),
		).toBe(true);
		expect(
			document.documentElement.style.getPropertyValue("--background"),
		).toBe("#fff1e8");
		expect(document.documentElement.style.getPropertyValue("--primary")).toBe(
			"#b7431d",
		);
	});
});
