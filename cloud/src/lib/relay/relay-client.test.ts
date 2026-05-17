import { describe, expect, it, vi } from "vitest";

import { type InboundCommand, pushInboundCommand } from "./relay-client";

describe("pushInboundCommand", () => {
	it("sends the command as the Phoenix frame payload", () => {
		const push = vi.fn();
		const command: InboundCommand = {
			v: 1,
			seq: 42,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			device_id: "web-1",
			kind: "list_sessions",
			payload: {},
		};

		pushInboundCommand({ push } as never, command);

		expect(push).toHaveBeenCalledWith("frame", command);
	});
});
