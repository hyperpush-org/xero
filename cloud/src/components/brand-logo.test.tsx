/** @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BrandLogo } from "./brand-logo";

describe("BrandLogo", () => {
	it("uses the active theme CSS variables for its mark colors", () => {
		const { container } = render(<BrandLogo aria-label="Xero" />);
		const paths = Array.from(container.querySelectorAll("path"));

		expect(paths.map((path) => path.getAttribute("fill"))).toEqual([
			"var(--primary)",
			"var(--primary)",
			"var(--primary)",
			"var(--primary)",
		]);
		expect(paths[2]?.getAttribute("fill-opacity")).toBe("0.32");
		expect(paths[3]?.getAttribute("fill-opacity")).toBe("0.32");
	});

	it("forwards standard SVG accessibility props", () => {
		const { container } = render(<BrandLogo aria-hidden className="size-4" />);
		const svg = container.querySelector("svg");

		expect(svg?.getAttribute("aria-hidden")).toBe("true");
		expect(svg?.classList.contains("size-4")).toBe(true);
	});
});
