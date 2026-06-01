import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const stylesPath = resolve(
	dirname(fileURLToPath(import.meta.url)),
	"styles.css",
);

describe("cloud stylesheet", () => {
	it("scans shared UI component classes for Tailwind utilities", () => {
		const styles = readFileSync(stylesPath, "utf8");

		expect(styles).toContain('@source "../../packages/ui/src";');
	});

	it("keeps Computer Use toolbar working state contained", () => {
		const styles = readFileSync(stylesPath, "utf8");
		const workingRule =
			styles.match(/\.desktop-control-toolbar-working\s*\{[^}]*\}/)?.[0] ??
			"";

		expect(styles).toContain(".desktop-control-toolbar-working");
		expect(workingRule).toContain("contain: paint");
		expect(workingRule).not.toContain("position:");
		expect(styles).not.toContain(".desktop-control-toolbar-working::after");
		expect(styles).not.toContain("desktop-control-toolbar-border-sweep");
	});
});
