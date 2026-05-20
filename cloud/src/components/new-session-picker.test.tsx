/** @vitest-environment jsdom */

import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NewSessionPicker } from "./new-session-picker";

function mockMatchMedia(matches: boolean) {
	Object.defineProperty(window, "matchMedia", {
		writable: true,
		value: (query: string) => ({
			matches,
			media: query,
			onchange: null,
			addListener: vi.fn(),
			removeListener: vi.fn(),
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}),
	});
}

describe("NewSessionPicker", () => {
	beforeEach(() => {
		mockMatchMedia(false);
		Object.defineProperty(window, "innerWidth", {
			writable: true,
			configurable: true,
			value: 1024,
		});
	});

	afterEach(() => {
		cleanup();
	});

	it("renders a disabled button when no projects are available", () => {
		const onSelect = vi.fn();
		render(<NewSessionPicker projects={[]} onSelectProject={onSelect} />);
		const button = screen.getByRole("button", {
			name: /no projects available/i,
		});
		expect((button as HTMLButtonElement).disabled).toBe(true);
		fireEvent.click(button);
		expect(onSelect).not.toHaveBeenCalled();
	});

	it("creates directly when only one project is available", () => {
		const onSelect = vi.fn();
		render(
			<NewSessionPicker
				projects={[
					{
						computerId: "desktop-1",
						projectId: "project-1",
						projectName: "Clipstack",
					},
				]}
				onSelectProject={onSelect}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: /start new session in clipstack/i }),
		);
		expect(onSelect).toHaveBeenCalledWith(
			expect.objectContaining({
				computerId: "desktop-1",
				projectId: "project-1",
			}),
		);
	});

	it("renders a single trigger when multiple projects are available", () => {
		const onSelect = vi.fn();
		render(
			<NewSessionPicker
				projects={[
					{
						computerId: "desktop-1",
						projectId: "project-1",
						projectName: "Clipstack",
					},
					{
						computerId: "desktop-1",
						projectId: "project-2",
						projectName: "Xero",
					},
				]}
				onSelectProject={onSelect}
			/>,
		);
		const trigger = screen.getByRole("button", { name: /start new session/i });
		expect((trigger as HTMLButtonElement).disabled).toBe(false);
		// Sanity: the trigger does not invoke onSelect just by being rendered.
		expect(onSelect).not.toHaveBeenCalled();
	});

	it("notifies the parent when the picker opens on mobile", async () => {
		mockMatchMedia(true);
		Object.defineProperty(window, "innerWidth", {
			writable: true,
			configurable: true,
			value: 320,
		});
		const onSelect = vi.fn();
		const onPickerOpenChange = vi.fn();
		render(
			<NewSessionPicker
				projects={[
					{
						computerId: "desktop-1",
						projectId: "project-1",
						projectName: "Clipstack",
					},
					{
						computerId: "desktop-1",
						projectId: "project-2",
						projectName: "Xero",
					},
				]}
				onSelectProject={onSelect}
				onPickerOpenChange={onPickerOpenChange}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: /start new session/i }));
		await waitFor(() => {
			expect(onPickerOpenChange).toHaveBeenCalledWith(true);
		});
	});
});
