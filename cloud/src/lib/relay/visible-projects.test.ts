import { describe, expect, it } from "vitest";

import type { RuntimeEnvelope } from "./envelope";
import { remoteProjectsUpdateFromEnvelope } from "./visible-projects";

describe("remoteProjectsUpdateFromEnvelope", () => {
	it("maps a remote projects envelope into RemoteProjectSummary entries", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__projects__",
			kind: "event",
			payload: {
				schema: "xero.remote_projects.v1",
				projects: [
					{ projectId: "project-1", projectName: "Clipstack" },
					{ project_id: "project-2", project_name: "Xero" },
					{ projectId: "project-3" },
				],
			},
		};

		expect(remoteProjectsUpdateFromEnvelope(envelope)).toEqual({
			computerId: "desktop-1",
			projects: [
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
				{
					computerId: "desktop-1",
					projectId: "project-3",
					projectName: null,
				},
			],
		});
	});

	it("returns null for envelopes on other session topics", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			kind: "event",
			payload: {
				schema: "xero.remote_projects.v1",
				projects: [],
			},
		};

		expect(remoteProjectsUpdateFromEnvelope(envelope)).toBeNull();
	});

	it("returns null for envelopes with the wrong schema", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__projects__",
			kind: "event",
			payload: {
				schema: "xero.remote_sessions.v1",
				projects: [],
			},
		};

		expect(remoteProjectsUpdateFromEnvelope(envelope)).toBeNull();
	});

	it("ignores entries that are missing a projectId", () => {
		const envelope: RuntimeEnvelope = {
			v: 1,
			seq: 1,
			computer_id: "desktop-1",
			session_id: "__projects__",
			kind: "event",
			payload: {
				schema: "xero.remote_projects.v1",
				projects: [
					{ projectName: "Orphan" },
					{ projectId: "project-1", projectName: "Clipstack" },
				],
			},
		};

		expect(remoteProjectsUpdateFromEnvelope(envelope)).toEqual({
			computerId: "desktop-1",
			projects: [
				{
					computerId: "desktop-1",
					projectId: "project-1",
					projectName: "Clipstack",
				},
			],
		});
	});
});
