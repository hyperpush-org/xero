import { type Channel, Socket } from "phoenix";

import { getServerUrl } from "../server-url";

export type InboundCommandKind =
	| "session_attached"
	| "send_message"
	| "start_session"
	| "archive_session"
	| "resolve_operator_action"
	| "cancel_run"
	| "context_snapshot"
	| "list_sessions"
	| "list_projects"
	| "stage_attachment"
	| "discard_attachment"
	| "update_session_controls"
	| "fetch_runtime_media_artifact"
	| "computer_use_stream_request"
	| "computer_use_stream_offer"
	| "computer_use_stream_answer"
	| "computer_use_stream_ice_candidate"
	| "computer_use_stream_stop"
	| "computer_use_stream_status"
	| "computer_use_stream_set_quality"
	| "computer_use_stream_request_keyframe"
	| "computer_use_manual_control_request"
	| "computer_use_manual_control_grant"
	| "computer_use_manual_control_heartbeat"
	| "computer_use_manual_control_input"
	| "computer_use_manual_control_release";

export type RelayCommandPriority =
	| "critical_reliable"
	| "reliable_idempotent"
	| "coalesced_best_effort";

export type RelayCommandOutcome =
	| "accepted"
	| "executed"
	| "rejected"
	| "rate_limited"
	| "timed_out"
	| "stale"
	| "duplicate";

export interface RelayRateLimitMetadata {
	bucket?: string;
	class?: string;
	kind?: string;
	limit?: number;
	retryAfterMs?: number;
	windowMs?: number;
}

export interface CommandAckResult {
	schema: "xero.remote_command_outcome.v1";
	clientCommandId: string;
	clientSeq: number;
	kind: InboundCommandKind;
	outcome: RelayCommandOutcome;
	priority: RelayCommandPriority;
	reason?: string | null;
	message?: string | null;
	rateLimit?: RelayRateLimitMetadata | null;
	retryAfterMs?: number | null;
	sentAt: number;
	receivedAt?: string | number | null;
	acceptedAt?: string | number | null;
}

export interface InboundCommand {
	v: number;
	seq: number;
	computer_id: string;
	session_id?: string;
	device_id?: string;
	kind: InboundCommandKind;
	clientCommandId?: string;
	clientSeq?: number;
	priority?: RelayCommandPriority;
	sentAt?: number;
	dedupeKey?: string;
	expiresAt?: number;
	payload: unknown;
}

interface StreamTokenOptions {
	runId?: string | null;
	streamToken?: string | null;
}

type ComputerUseManualInputAction =
	| "mouse_down"
	| "mouse_move"
	| "mouse_click"
	| "mouse_double_click"
	| "mouse_right_click"
	| "mouse_drag"
	| "mouse_drag_move"
	| "mouse_up"
	| "scroll"
	| "key_press"
	| "hotkey"
	| "type_text"
	| "paste_text";

let socket: Socket | null = null;
let nextClientSeq = 0;
let fallbackCloudInstanceId: string | null = null;
const commandSchedulers = new WeakMap<Channel, RelayCommandScheduler>();
const CLOUD_INSTANCE_STORAGE_KEY = "xero.cloud.instanceId.v1";

const CRITICAL_COMMAND_KINDS = new Set<InboundCommandKind>([
	"computer_use_manual_control_request",
	"computer_use_manual_control_grant",
	"computer_use_manual_control_heartbeat",
	"computer_use_manual_control_input",
	"computer_use_manual_control_release",
	"computer_use_stream_offer",
	"computer_use_stream_answer",
	"computer_use_stream_ice_candidate",
]);

const RELIABLE_IDEMPOTENT_COMMAND_KINDS = new Set<InboundCommandKind>([
	"session_attached",
	"context_snapshot",
	"list_sessions",
	"list_projects",
	"computer_use_stream_request",
	"computer_use_stream_stop",
	"computer_use_stream_status",
	"computer_use_stream_request_keyframe",
]);
const RELAY_COMMAND_MAX_IN_FLIGHT = 4;
const RELAY_COMMAND_MAX_CRITICAL_QUEUE = 256;
const RELAY_COMMAND_MAX_RELIABLE_QUEUE = 128;
const RELAY_COMMAND_MAX_COALESCED_QUEUE = 64;

interface QueuedRelayCommand {
	attempt: number;
	command: InboundCommand;
	resolve: (result: CommandAckResult) => void;
}

interface RelayCommandPolicy {
	priority: RelayCommandPriority;
	timeoutMs: number;
	maxAttempts: number;
	coalesceKey: string | null;
}

interface PhoenixPushLike {
	receive?: (
		status: "ok" | "error" | "timeout",
		callback: (payload?: unknown) => void,
	) => PhoenixPushLike;
}

class RelayCommandScheduler {
	private readonly criticalQueue: QueuedRelayCommand[] = [];
	private readonly reliableQueue: QueuedRelayCommand[] = [];
	private readonly coalescedQueue = new Map<string, QueuedRelayCommand>();
	private readonly pending = new Map<
		string,
		{
			finish: (result: CommandAckResult) => void;
			timeout: ReturnType<typeof setTimeout>;
		}
	>();
	private inFlight = 0;
	private outcomeRef: number | string | null = null;

	constructor(private readonly channel: Channel) {}

	enqueue(command: InboundCommand): Promise<CommandAckResult> {
		this.installOutcomeListener();
		return new Promise((resolve) => {
			const enriched = completeCommandEnvelope(command);
			const policy = commandPolicy(enriched);
			enriched.priority = policy.priority;
			if (policy.coalesceKey) {
				const previous = this.coalescedQueue.get(policy.coalesceKey);
				if (previous) {
					previous.resolve(
						commandResult(previous.command, "stale", "coalesced"),
					);
					emitRelayCommandMetric("coalesced", previous.command, {
						coalesceKey: policy.coalesceKey,
					});
				} else if (
					this.coalescedQueue.size >= RELAY_COMMAND_MAX_COALESCED_QUEUE
				) {
					const oldest = this.coalescedQueue.entries().next().value as
						| [string, QueuedRelayCommand]
						| undefined;
					if (oldest) {
						this.coalescedQueue.delete(oldest[0]);
						oldest[1].resolve(
							commandResult(oldest[1].command, "stale", "coalesced_queue_full"),
						);
						emitRelayCommandMetric("dropped", oldest[1].command, {
							coalesceKey: oldest[0],
							reason: "coalesced_queue_full",
						});
					}
				}
				this.coalescedQueue.set(policy.coalesceKey, {
					attempt: 0,
					command: enriched,
					resolve,
				});
			} else if (policy.priority === "critical_reliable") {
				if (this.criticalQueue.length >= RELAY_COMMAND_MAX_CRITICAL_QUEUE) {
					resolve(commandResult(enriched, "rejected", "critical_queue_full"));
					emitRelayCommandMetric("dropped", enriched, {
						reason: "critical_queue_full",
					});
					return;
				}
				this.criticalQueue.push({ attempt: 0, command: enriched, resolve });
			} else {
				if (this.reliableQueue.length >= RELAY_COMMAND_MAX_RELIABLE_QUEUE) {
					resolve(commandResult(enriched, "rejected", "reliable_queue_full"));
					emitRelayCommandMetric("dropped", enriched, {
						reason: "reliable_queue_full",
					});
					return;
				}
				this.reliableQueue.push({ attempt: 0, command: enriched, resolve });
			}
			this.flush();
		});
	}

	private installOutcomeListener() {
		if (this.outcomeRef !== null || typeof this.channel.on !== "function") {
			return;
		}
		this.outcomeRef = this.channel.on(
			"computer_use_command_outcome",
			(payload: unknown) => {
				const outcome = commandOutcomeFromPayload(payload);
				if (!outcome) return;
				const pending = this.pending.get(outcome.clientCommandId);
				if (!pending) return;
				pending.finish(outcome);
			},
		);
	}

	private flush() {
		while (this.inFlight < RELAY_COMMAND_MAX_IN_FLIGHT) {
			const next = this.nextQueuedCommand();
			if (!next) return;
			this.inFlight += 1;
			this.send(next);
		}
	}

	private nextQueuedCommand(): QueuedRelayCommand | null {
		const critical = this.criticalQueue.shift();
		if (critical) return critical;
		const reliable = this.reliableQueue.shift();
		if (reliable) return reliable;
		const coalesced = this.coalescedQueue.entries().next().value as
			| [string, QueuedRelayCommand]
			| undefined;
		if (!coalesced) return null;
		this.coalescedQueue.delete(coalesced[0]);
		return coalesced[1];
	}

	private send(queued: QueuedRelayCommand) {
		const policy = commandPolicy(queued.command);
		let settled = false;
		const finish = (result: CommandAckResult) => {
			if (settled) return;
			settled = true;
			const pending = this.pending.get(queued.command.clientCommandId ?? "");
			if (pending) clearTimeout(pending.timeout);
			this.pending.delete(queued.command.clientCommandId ?? "");
			this.inFlight = Math.max(0, this.inFlight - 1);
			queued.resolve(result);
			this.flush();
		};
		const retryOrFinish = (result: CommandAckResult) => {
			if (
				result.outcome === "timed_out" &&
				queued.attempt + 1 < policy.maxAttempts
			) {
				this.pending.delete(queued.command.clientCommandId ?? "");
				this.inFlight = Math.max(0, this.inFlight - 1);
				const retry = { ...queued, attempt: queued.attempt + 1 };
				if (policy.priority === "critical_reliable") {
					this.criticalQueue.unshift(retry);
				} else if (policy.coalesceKey) {
					this.coalescedQueue.set(policy.coalesceKey, retry);
				} else {
					this.reliableQueue.unshift(retry);
				}
				emitRelayCommandMetric("retry", queued.command, {
					attempt: retry.attempt + 1,
				});
				this.flush();
				return;
			}
			finish(result);
		};
		const timeout = setTimeout(() => {
			retryOrFinish(commandResult(queued.command, "timed_out", "push_timeout"));
		}, policy.timeoutMs);
		this.pending.set(queued.command.clientCommandId ?? "", { finish, timeout });

		let push: PhoenixPushLike | undefined;
		try {
			push = this.channel.push("frame", queued.command) as PhoenixPushLike;
		} catch {
			retryOrFinish(commandResult(queued.command, "rejected", "push_failed"));
			return;
		}

		const receive = push?.receive;
		if (typeof receive !== "function") {
			finish(commandResult(queued.command, "accepted", "push_unobservable"));
			return;
		}

		receive.call(push, "ok", (payload) => {
			finish(
				commandOutcomeFromPayload(payload) ??
					commandResult(queued.command, "accepted", null),
			);
		});
		receive.call(push, "error", (payload) => {
			const outcome = commandOutcomeFromPayload(payload);
			finish(
				outcome ??
					commandResult(
						queued.command,
						rateLimitedPayload(payload) ? "rate_limited" : "rejected",
						errorReason(payload),
						rateLimitMetadata(payload),
					),
			);
		});
		receive.call(push, "timeout", () => {
			retryOrFinish(commandResult(queued.command, "timed_out", "push_timeout"));
		});
	}
}

function socketIsReusable(socketInstance: Socket): boolean {
	const state = socketInstance.connectionState();
	return state === "connecting" || state === "open";
}

function completeCommandEnvelope(command: InboundCommand): InboundCommand {
	const clientSeq = command.clientSeq ?? nextCommandSeq();
	const clientCommandId =
		command.clientCommandId ?? `cmd_${clientSeq}_${Date.now().toString(36)}`;
	const sentAt = command.sentAt ?? Date.now();
	const priority = command.priority ?? commandPolicy(command).priority;
	const expiresAt =
		command.expiresAt ??
		sentAt + commandPolicy({ ...command, priority }).timeoutMs;
	return {
		...command,
		clientCommandId,
		clientSeq,
		priority,
		sentAt,
		dedupeKey: command.dedupeKey ?? clientCommandId,
		expiresAt,
	};
}

function nextCommandSeq(): number {
	nextClientSeq = (nextClientSeq + 1) % Number.MAX_SAFE_INTEGER;
	return nextClientSeq || nextCommandSeq();
}

function commandPolicy(command: InboundCommand): RelayCommandPolicy {
	const coalesceKey = commandCoalesceKey(command);
	if (coalesceKey) {
		return {
			priority: "coalesced_best_effort",
			timeoutMs: 2_000,
			maxAttempts: 1,
			coalesceKey,
		};
	}
	if (CRITICAL_COMMAND_KINDS.has(command.kind)) {
		return {
			priority: "critical_reliable",
			timeoutMs: 8_000,
			maxAttempts: 2,
			coalesceKey: null,
		};
	}
	if (RELIABLE_IDEMPOTENT_COMMAND_KINDS.has(command.kind)) {
		return {
			priority: "reliable_idempotent",
			timeoutMs: 5_000,
			maxAttempts: 2,
			coalesceKey: null,
		};
	}
	return {
		priority: "reliable_idempotent",
		timeoutMs: 5_000,
		maxAttempts: 1,
		coalesceKey: null,
	};
}

function commandCoalesceKey(command: InboundCommand): string | null {
	const payload = recordPayload(command.payload);
	if (
		command.kind === "computer_use_manual_control_input" &&
		payload?.action === "mouse_move"
	) {
		return [
			command.kind,
			command.session_id ?? "",
			stringValue(payload.manualControlId),
		].join(":");
	}
	if (command.kind === "computer_use_stream_status") {
		return [
			command.kind,
			command.session_id ?? "",
			stringValue(payload?.streamId),
		].join(":");
	}
	if (command.kind === "computer_use_stream_set_quality") {
		return [
			command.kind,
			command.session_id ?? "",
			stringValue(payload?.streamId),
		].join(":");
	}
	return null;
}

function recordPayload(value: unknown): Record<string, unknown> | null {
	return value && typeof value === "object"
		? (value as Record<string, unknown>)
		: null;
}

function stringValue(value: unknown): string {
	return typeof value === "string" ? value : "";
}

function commandResult(
	command: InboundCommand,
	outcome: RelayCommandOutcome,
	reason: string | null,
	rateLimit: RelayRateLimitMetadata | null = null,
): CommandAckResult {
	return {
		schema: "xero.remote_command_outcome.v1",
		clientCommandId: command.clientCommandId ?? "",
		clientSeq: command.clientSeq ?? command.seq,
		kind: command.kind,
		outcome,
		priority: command.priority ?? commandPolicy(command).priority,
		reason,
		message: null,
		rateLimit,
		retryAfterMs: rateLimit?.retryAfterMs ?? null,
		sentAt: command.sentAt ?? Date.now(),
		acceptedAt: outcome === "accepted" ? Date.now() : null,
	};
}

function commandOutcomeFromPayload(payload: unknown): CommandAckResult | null {
	const record = recordPayload(payload);
	const outcomeRecord =
		recordPayload(record?.command) ??
		recordPayload(record?.outcome) ??
		(record?.schema === "xero.remote_command_outcome.v1" ? record : null);
	if (!outcomeRecord) return null;
	const clientCommandId = stringValue(outcomeRecord.clientCommandId);
	const kind = stringValue(outcomeRecord.kind) as InboundCommandKind;
	const outcome = stringValue(outcomeRecord.outcome) as RelayCommandOutcome;
	const priority = stringValue(outcomeRecord.priority) as RelayCommandPriority;
	if (
		!clientCommandId ||
		!isInboundCommandKind(kind) ||
		!isRelayOutcome(outcome)
	) {
		return null;
	}
	return {
		schema: "xero.remote_command_outcome.v1",
		clientCommandId,
		clientSeq: numberValue(outcomeRecord.clientSeq) ?? 0,
		kind,
		outcome,
		priority: isRelayCommandPriority(priority)
			? priority
			: "reliable_idempotent",
		reason: stringValue(outcomeRecord.reason) || null,
		message: stringValue(outcomeRecord.message) || null,
		rateLimit: rateLimitMetadata(outcomeRecord),
		retryAfterMs: numberValue(outcomeRecord.retryAfterMs),
		sentAt: numberValue(outcomeRecord.sentAt) ?? Date.now(),
		receivedAt: outcomeRecord.receivedAt as string | number | null | undefined,
		acceptedAt: outcomeRecord.acceptedAt as string | number | null | undefined,
	};
}

function numberValue(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function isInboundCommandKind(value: string): value is InboundCommandKind {
	return (
		value === "session_attached" ||
		value === "send_message" ||
		value === "start_session" ||
		value === "archive_session" ||
		value === "resolve_operator_action" ||
		value === "cancel_run" ||
		value === "context_snapshot" ||
		value === "list_sessions" ||
		value === "list_projects" ||
		value === "stage_attachment" ||
		value === "discard_attachment" ||
		value === "update_session_controls" ||
		value === "fetch_runtime_media_artifact" ||
		value === "computer_use_stream_request" ||
		value === "computer_use_stream_offer" ||
		value === "computer_use_stream_answer" ||
		value === "computer_use_stream_ice_candidate" ||
		value === "computer_use_stream_stop" ||
		value === "computer_use_stream_status" ||
		value === "computer_use_stream_set_quality" ||
		value === "computer_use_stream_request_keyframe" ||
		value === "computer_use_manual_control_request" ||
		value === "computer_use_manual_control_grant" ||
		value === "computer_use_manual_control_heartbeat" ||
		value === "computer_use_manual_control_input" ||
		value === "computer_use_manual_control_release"
	);
}

function isRelayOutcome(value: string): value is RelayCommandOutcome {
	return (
		value === "accepted" ||
		value === "executed" ||
		value === "rejected" ||
		value === "rate_limited" ||
		value === "timed_out" ||
		value === "stale" ||
		value === "duplicate"
	);
}

function isRelayCommandPriority(value: string): value is RelayCommandPriority {
	return (
		value === "critical_reliable" ||
		value === "reliable_idempotent" ||
		value === "coalesced_best_effort"
	);
}

function rateLimitedPayload(payload: unknown): boolean {
	return errorReason(payload) === "rate_limited";
}

function errorReason(payload: unknown): string | null {
	const record = recordPayload(payload);
	return stringValue(record?.reason) || stringValue(record?.error) || null;
}

function rateLimitMetadata(payload: unknown): RelayRateLimitMetadata | null {
	const record = recordPayload(payload);
	const source =
		recordPayload(record?.rateLimit) ?? recordPayload(record?.rate_limit);
	if (!source && errorReason(payload) !== "rate_limited") return null;
	const metadata = source ?? record ?? {};
	const retryAfterMs =
		numberValue(metadata.retryAfterMs) ?? numberValue(metadata.retry_after_ms);
	return {
		bucket: stringValue(metadata.bucket) || undefined,
		class: stringValue(metadata.class) || undefined,
		kind: stringValue(metadata.kind) || undefined,
		limit: numberValue(metadata.limit) ?? undefined,
		retryAfterMs: retryAfterMs ?? undefined,
		windowMs:
			numberValue(metadata.windowMs) ??
			numberValue(metadata.window_ms) ??
			undefined,
	};
}

function emitRelayCommandMetric(
	event: string,
	command: InboundCommand,
	detail: Record<string, unknown> = {},
) {
	if (typeof window === "undefined") return;
	window.dispatchEvent(
		new CustomEvent("xero:relay-command-metric", {
			detail: {
				event,
				kind: command.kind,
				clientCommandId: command.clientCommandId,
				priority: command.priority,
				...detail,
			},
		}),
	);
}

/**
 * Lazily open the singleton browser → relay WebSocket. The relay URL is derived
 * from `XERO_SERVER_URL` (http → ws, https → wss) and the connection is
 * authenticated with a short-lived JWT obtained server-side from
 * `/api/relay/token/refresh`.
 */
export function getRelaySocket(token: string): Socket {
	if (socket) {
		if (socketIsReusable(socket)) return socket;
		socket.disconnect();
	}
	const url = `${getServerUrl().replace(/^http/, "ws")}/socket/web`;
	socket = new Socket(url, { params: { token } });
	socket.connect();
	return socket;
}

export function disconnectRelay(): void {
	if (socket) {
		socket.disconnect();
		socket = null;
	}
}

function getCloudInstanceId(): string {
	if (typeof window !== "undefined") {
		try {
			const existing = window.sessionStorage
				.getItem(CLOUD_INSTANCE_STORAGE_KEY)
				?.trim();
			if (existing) return existing;
			const next = createCloudInstanceId();
			window.sessionStorage.setItem(CLOUD_INSTANCE_STORAGE_KEY, next);
			return next;
		} catch {
			// Fall through to the in-memory id when storage is unavailable.
		}
	}
	fallbackCloudInstanceId ??= createCloudInstanceId();
	return fallbackCloudInstanceId;
}

function createCloudInstanceId(): string {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`cloud_${Math.random().toString(36).slice(2)}`
	);
}

export function joinAccountChannel(
	socketInstance: Socket,
	accountId: string,
): Channel {
	const channel = socketInstance.channel(`account:${accountId}`, {});
	channel.join();
	return channel;
}

export function joinSessionChannel(
	socketInstance: Socket,
	computerId: string,
	sessionId: string,
	lastSeq?: number,
	onJoined?: (channel: Channel, payload: unknown) => void,
	onJoinError?: (payload: unknown) => void,
): Channel {
	const channel = socketInstance.channel(`session:${computerId}:${sessionId}`, {
		cloud_instance_id: getCloudInstanceId(),
		last_seq: lastSeq ?? 0,
	});
	const join = channel.join();
	if (onJoined) {
		join.receive("ok", (payload) => onJoined(channel, payload));
	}
	if (onJoinError) {
		join
			.receive("error", (payload) => onJoinError(payload))
			.receive("timeout", () => onJoinError({ reason: "timeout" }));
	}
	return channel;
}

/**
 * Send an inbound command frame on the given session channel. The payload is
 * forwarded verbatim to the owning desktop via the Phoenix relay.
 */
export function pushInboundCommand(
	channel: Channel,
	command: InboundCommand,
): Promise<CommandAckResult> {
	let scheduler = commandSchedulers.get(channel);
	if (!scheduler) {
		scheduler = new RelayCommandScheduler(channel);
		commandSchedulers.set(channel, scheduler);
	}
	return scheduler.enqueue(command);
}

export function requestSessionSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "session_attached",
		payload: { lastSeq: 0 },
	});
}

export function requestThemeSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__theme__",
		device_id: options.deviceId,
		kind: "session_attached",
		payload: { lastSeq: 0 },
	});
}

export function requestContextSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		requestId: string;
		providerId?: string | null;
		modelId?: string | null;
		pendingPrompt?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		requestId: options.requestId,
	};
	if (options.providerId) payload.providerId = options.providerId;
	if (options.modelId) payload.modelId = options.modelId;
	if (options.pendingPrompt) payload.pendingPrompt = options.pendingPrompt;

	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "context_snapshot",
		payload,
	});
}

export function requestRuntimeMediaArtifact(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		artifactId: string;
	},
): void {
	const artifactId = options.artifactId.trim();
	if (!artifactId) return;
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "fetch_runtime_media_artifact",
		payload: { artifactId },
	});
}

export function requestComputerUseStream(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		displayId?: string | null;
		streamId?: string | null;
		quality?: "low" | "balanced" | "high";
		iceServers?: RTCIceServer[];
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_request",
		payload: {
			displayId: options.displayId ?? null,
			streamId: options.streamId ?? null,
			quality: options.quality ?? "balanced",
			includeCursor: true,
			iceServers: options.iceServers ?? [],
			...streamSecurityPayload(options),
		},
	});
}

export function requestRunCancel(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		runId?: string | null;
		reason?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		reason: options.reason ?? "cloud_run_cancel",
	};
	if (options.runId) payload.runId = options.runId;
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "cancel_run",
		payload,
	});
}

export function stopComputerUseStream(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_stop",
		payload: {
			streamId: options.streamId ?? null,
			...streamSecurityPayload(options),
		},
	});
}

export function requestComputerUseStreamStatus(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_status",
		payload: {
			streamId: options.streamId ?? null,
			...streamSecurityPayload(options),
		},
	});
}

export function setComputerUseStreamQuality(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
		quality: "low" | "balanced" | "high";
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_set_quality",
		payload: {
			streamId: options.streamId ?? null,
			quality: options.quality,
			...streamSecurityPayload(options),
		},
	});
}

export function requestComputerUseStreamKeyframe(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_request_keyframe",
		payload: {
			streamId: options.streamId ?? null,
			...streamSecurityPayload(options),
		},
	});
}

export function answerComputerUseStreamOffer(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
		answer: RTCSessionDescriptionInit;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_answer",
		payload: {
			streamId: options.streamId ?? null,
			type: options.answer.type,
			sdp: options.answer.sdp,
			...streamSecurityPayload(options),
		},
	});
}

export function sendComputerUseStreamIceCandidate(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		streamId?: string | null;
		candidate: RTCIceCandidateInit;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_stream_ice_candidate",
		payload: {
			streamId: options.streamId ?? null,
			candidate: options.candidate,
			...streamSecurityPayload(options),
		},
	});
}

export function requestComputerUseManualControl(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		manualControlId?: string | null;
		reason?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_manual_control_request",
		payload: {
			manualControlId: options.manualControlId ?? null,
			reason: options.reason ?? "cloud_manual_control",
			...streamSecurityPayload(options),
		},
	});
}

export function heartbeatComputerUseManualControl(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		manualControlId?: string | null;
		reason?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_manual_control_heartbeat",
		payload: {
			manualControlId: options.manualControlId ?? null,
			reason: options.reason ?? "manual_cloud_control_heartbeat",
			...streamSecurityPayload(options),
		},
	});
}

export function releaseComputerUseManualControl(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		manualControlId?: string | null;
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_manual_control_release",
		payload: {
			manualControlId: options.manualControlId ?? null,
			...streamSecurityPayload(options),
		},
	});
}

export function sendComputerUseManualInput(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		manualControlId?: string | null;
		input: {
			action: ComputerUseManualInputAction;
			x?: number;
			y?: number;
			toX?: number;
			toY?: number;
			sourceWidth?: number;
			sourceHeight?: number;
			deltaX?: number;
			deltaY?: number;
			button?: "left" | "middle" | "right";
			clicks?: number;
			key?: string;
			keys?: string[];
			text?: string;
			reason?: string;
		};
	} & StreamTokenOptions,
): Promise<CommandAckResult> {
	return pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "computer_use_manual_control_input",
		payload: {
			manualControlId: options.manualControlId ?? null,
			reason: options.input.reason ?? "cloud_manual_control_input",
			...options.input,
			...streamSecurityPayload(options),
		},
	});
}

function streamSecurityPayload(
	options: StreamTokenOptions,
): Record<string, string> {
	const payload: Record<string, string> = {};
	const runId = options.runId?.trim();
	const token = options.streamToken?.trim();
	if (runId) payload.runId = runId;
	if (token) payload.streamToken = token;
	return payload;
}

export function requestStartSession(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		deviceId: string;
		title?: string | null;
		prompt?: string | null;
		sessionKind?: "standard" | "computer_use";
		agent?: string | null;
		resetExisting?: boolean;
	},
): void {
	const payload: Record<string, unknown> = {
		projectId: options.projectId,
		prompt: options.prompt ?? "",
	};
	if (options.title?.trim()) payload.title = options.title.trim();
	if (options.sessionKind) payload.sessionKind = options.sessionKind;
	if (options.agent?.trim()) payload.agent = options.agent.trim();
	if (options.resetExisting) payload.resetExisting = true;

	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__new__",
		device_id: options.deviceId,
		kind: "start_session",
		payload,
	});
}

export function requestStageAttachment(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		attachmentId: string;
		originalName: string;
		mediaType: string;
		bytesBase64: string;
		runId?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		attachmentId: options.attachmentId,
		originalName: options.originalName,
		mediaType: options.mediaType,
		bytesBase64: options.bytesBase64,
	};
	if (options.runId) payload.runId = options.runId;
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "stage_attachment",
		payload,
	});
}

export function requestDiscardAttachment(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		attachmentId: string;
		absolutePath: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "discard_attachment",
		payload: {
			attachmentId: options.attachmentId,
			absolutePath: options.absolutePath,
		},
	});
}

export function requestSessionArchive(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		sessionId: string;
		agentSessionId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__sessions__",
		device_id: options.deviceId,
		kind: "archive_session",
		payload: {
			projectId: options.projectId,
			agentSessionId: options.agentSessionId,
			remoteSessionId: options.sessionId,
		},
	});
}
