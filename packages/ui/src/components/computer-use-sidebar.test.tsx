/** @vitest-environment jsdom */

import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	COMPUTER_USE_SIDEBAR_DEFAULT_WIDTH,
	ComputerUseSidebar,
	type ComputerUseSidebarDensity,
} from "./computer-use-sidebar";

const STORAGE_KEY = "xero.test.computerUseSidebar.width";

describe("ComputerUseSidebar", () => {
	beforeEach(() => {
		const localStorageValues = new Map<string, string>();
		Object.defineProperty(window, "localStorage", {
			configurable: true,
			writable: true,
			value: {
				clear: vi.fn(() => localStorageValues.clear()),
				getItem: vi.fn((key: string) => localStorageValues.get(key) ?? null),
				removeItem: vi.fn((key: string) => localStorageValues.delete(key)),
				setItem: vi.fn((key: string, value: string) => {
					localStorageValues.set(key, value);
				}),
			},
		});
		window.localStorage.clear();
	});

	afterEach(() => {
		cleanup();
		vi.clearAllMocks();
		window.localStorage.clear();
		document.body.style.cursor = "";
		document.body.style.userSelect = "";
	});

	it("renders the shared non-resizable sidebar by default", () => {
		render(
			<ComputerUseSidebar>
				<div>Computer Use content</div>
			</ComputerUseSidebar>,
		);

		const sidebar = screen.getByLabelText("Computer Use agent");
		expect(sidebar).toHaveClass("bg-sidebar");
		expect(sidebar).toHaveClass("w-[min(31rem,34vw)]");
		expect(
			screen.queryByRole("separator", { name: "Resize Computer Use sidebar" }),
		).toBeNull();
	});

	it("resizes from the left edge and reports compact density", async () => {
		const densityChanges: ComputerUseSidebarDensity[] = [];
		const widthChanges: number[] = [];

		render(
			<ComputerUseSidebar
				resizable
				onDensityChange={(density) => densityChanges.push(density)}
				onWidthChange={(width) => widthChanges.push(width)}
				widthStorageKey={STORAGE_KEY}
			>
				<div>Computer Use content</div>
			</ComputerUseSidebar>,
		);

		const sidebar = screen.getByLabelText("Computer Use agent");
		expect(sidebar).toHaveStyle({
			width: `${COMPUTER_USE_SIDEBAR_DEFAULT_WIDTH}px`,
		});
		expect(densityChanges).toContain("comfortable");

		const separator = screen.getByRole("separator", {
			name: "Resize Computer Use sidebar",
		});
		fireEvent.pointerDown(separator, { button: 0, clientX: 700 });
		fireEvent.pointerMove(window, { clientX: 900 });
		fireEvent.pointerUp(window);

		await waitFor(() => {
			expect(sidebar).toHaveStyle({ width: "360px" });
		});
		expect(densityChanges).toContain("compact");
		expect(widthChanges.at(-1)).toBe(360);
		expect(window.localStorage.getItem(STORAGE_KEY)).toBe("360");
	});

	it("supports keyboard resizing with the same right-sidebar direction", async () => {
		const densityChanges: ComputerUseSidebarDensity[] = [];

		render(
			<ComputerUseSidebar
				defaultWidth={408}
				resizable
				onDensityChange={(density) => densityChanges.push(density)}
				widthStorageKey={STORAGE_KEY}
			>
				<div>Computer Use content</div>
			</ComputerUseSidebar>,
		);

		const sidebar = screen.getByLabelText("Computer Use agent");
		const separator = screen.getByRole("separator", {
			name: "Resize Computer Use sidebar",
		});
		fireEvent.keyDown(separator, { key: "ArrowRight" });
		fireEvent.keyDown(separator, { key: "ArrowRight" });

		await waitFor(() => {
			expect(sidebar).toHaveStyle({ width: "392px" });
		});
		expect(densityChanges).toContain("compact");
		expect(window.localStorage.getItem(STORAGE_KEY)).toBe("392");
	});

	it("restores a persisted width when available", async () => {
		window.localStorage.setItem(STORAGE_KEY, "384");
		const onDensityChange = vi.fn();

		render(
			<ComputerUseSidebar
				resizable
				onDensityChange={onDensityChange}
				widthStorageKey={STORAGE_KEY}
			>
				<div>Computer Use content</div>
			</ComputerUseSidebar>,
		);

		expect(screen.getByLabelText("Computer Use agent")).toHaveStyle({
			width: "384px",
		});
		await waitFor(() => expect(onDensityChange).toHaveBeenCalledWith("compact"));
	});
});
