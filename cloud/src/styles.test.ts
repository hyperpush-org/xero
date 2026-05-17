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
});
