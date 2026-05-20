import type { ComposerPendingAttachment } from "@xero/ui/components/composer";
import {
	type AgentAttachmentKind,
	classificationRejectionMessage,
	classifyAttachment,
} from "@xero/ui/lib/agent-attachments";
import type { Channel } from "phoenix";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	requestDiscardAttachment,
	requestStageAttachment,
} from "./relay-client";

interface StagedAttachmentReadyEvent {
	schema: "xero.remote_attachment_staged.v1";
	ok: boolean;
	attachmentId: string;
	attachment?: {
		kind: AgentAttachmentKind;
		absolutePath: string;
		mediaType: string;
		originalName: string;
		sizeBytes: number;
	};
	error?: { message?: string | null } | null;
}

interface UseRemoteAttachmentsOptions {
	channel: Channel | null;
	computerId: string;
	sessionId: string;
	deviceId: string | null;
}

export interface RemoteAttachmentReady {
	kind: AgentAttachmentKind;
	absolutePath: string;
	mediaType: string;
	originalName: string;
	sizeBytes: number;
}

export interface UseRemoteAttachmentsResult {
	pendingAttachments: ComposerPendingAttachment[];
	classificationError: string | null;
	addFiles: (files: File[]) => void;
	removeAttachment: (id: string) => void;
	clearAttachments: () => void;
	getReadyAttachments: () => RemoteAttachmentReady[];
}

interface InternalAttachmentRecord extends ComposerPendingAttachment {
	absolutePath?: string;
}

function bytesToBase64(bytes: Uint8Array): string {
	let binary = "";
	const chunkSize = 0x8000;
	for (let index = 0; index < bytes.length; index += chunkSize) {
		const chunk = bytes.subarray(index, index + chunkSize);
		binary += String.fromCharCode(...chunk);
	}
	return typeof btoa === "function" ? btoa(binary) : "";
}

function generateAttachmentId(): string {
	const randomPart = Math.random().toString(36).slice(2, 10);
	return `attachment-${Date.now()}-${randomPart}`;
}

export function useRemoteAttachments({
	channel,
	computerId,
	sessionId,
	deviceId,
}: UseRemoteAttachmentsOptions): UseRemoteAttachmentsResult {
	const [pendingAttachments, setPendingAttachments] = useState<
		InternalAttachmentRecord[]
	>([]);
	const [classificationError, setClassificationError] = useState<string | null>(
		null,
	);
	const pendingAttachmentsRef = useRef<InternalAttachmentRecord[]>([]);
	pendingAttachmentsRef.current = pendingAttachments;

	useEffect(() => {
		if (!channel) return;
		const ref = channel.on("frame", (rawFrame: unknown) => {
			const event = extractAttachmentStagedEvent(rawFrame);
			if (!event) return;
			setPendingAttachments((prev) =>
				prev.map((attachment) => {
					if (attachment.id !== event.attachmentId) return attachment;
					if (event.ok && event.attachment) {
						return {
							...attachment,
							status: "ready",
							absolutePath: event.attachment.absolutePath,
							sizeBytes: event.attachment.sizeBytes,
							mediaType: event.attachment.mediaType,
						};
					}
					return {
						...attachment,
						status: "error",
						errorMessage: event.error?.message ?? "Attachment upload failed.",
					};
				}),
			);
		});
		return () => {
			channel.off("frame", ref);
		};
	}, [channel]);

	useEffect(() => {
		return () => {
			for (const attachment of pendingAttachmentsRef.current) {
				if (attachment.previewUrl) {
					URL.revokeObjectURL(attachment.previewUrl);
				}
			}
		};
	}, []);

	const addFiles = useCallback(
		(files: File[]) => {
			if (!channel || !deviceId) {
				setClassificationError(
					"Disconnected from the desktop — attachments are unavailable right now.",
				);
				return;
			}
			const rejections: string[] = [];
			for (const file of files) {
				const classification = classifyAttachment({
					name: file.name,
					type: file.type,
					size: file.size,
				});
				if (classification.kind === null) {
					rejections.push(classificationRejectionMessage(file, classification));
					continue;
				}
				const id = generateAttachmentId();
				const previewUrl =
					classification.kind === "image" &&
					typeof URL !== "undefined" &&
					typeof URL.createObjectURL === "function"
						? URL.createObjectURL(file)
						: undefined;
				const optimistic: InternalAttachmentRecord = {
					id,
					kind: classification.kind,
					originalName: file.name,
					mediaType: classification.mediaType,
					sizeBytes: file.size,
					status: "staging",
					previewUrl,
				};
				setPendingAttachments((prev) => [...prev, optimistic]);
				void file
					.arrayBuffer()
					.then((buffer) => {
						const bytesBase64 = bytesToBase64(new Uint8Array(buffer));
						requestStageAttachment(channel, {
							computerId,
							sessionId,
							deviceId,
							attachmentId: id,
							originalName: file.name,
							mediaType: classification.mediaType,
							bytesBase64,
						});
					})
					.catch((error: unknown) => {
						const message =
							error instanceof Error ? error.message : "Upload failed";
						setPendingAttachments((prev) =>
							prev.map((attachment) =>
								attachment.id === id
									? { ...attachment, status: "error", errorMessage: message }
									: attachment,
							),
						);
					});
			}
			setClassificationError(
				rejections.length > 0 ? rejections.join(" ") : null,
			);
		},
		[channel, computerId, deviceId, sessionId],
	);

	const removeAttachment = useCallback(
		(id: string) => {
			setPendingAttachments((prev) => {
				const removed = prev.find((attachment) => attachment.id === id);
				if (removed?.previewUrl) URL.revokeObjectURL(removed.previewUrl);
				if (
					removed?.absolutePath &&
					channel &&
					deviceId &&
					removed.status === "ready"
				) {
					requestDiscardAttachment(channel, {
						computerId,
						sessionId,
						deviceId,
						attachmentId: id,
						absolutePath: removed.absolutePath,
					});
				}
				return prev.filter((attachment) => attachment.id !== id);
			});
		},
		[channel, computerId, deviceId, sessionId],
	);

	const clearAttachments = useCallback(() => {
		setPendingAttachments((prev) => {
			for (const attachment of prev) {
				if (attachment.previewUrl) URL.revokeObjectURL(attachment.previewUrl);
			}
			return [];
		});
	}, []);

	const getReadyAttachments = useCallback((): RemoteAttachmentReady[] => {
		return pendingAttachmentsRef.current
			.filter(
				(
					attachment,
				): attachment is InternalAttachmentRecord & { absolutePath: string } =>
					attachment.status === "ready" &&
					typeof attachment.absolutePath === "string",
			)
			.map((attachment) => ({
				kind: attachment.kind,
				absolutePath: attachment.absolutePath,
				mediaType: attachment.mediaType,
				originalName: attachment.originalName,
				sizeBytes: attachment.sizeBytes,
			}));
	}, []);

	return {
		pendingAttachments: pendingAttachments.map(stripInternalFields),
		classificationError,
		addFiles,
		removeAttachment,
		clearAttachments,
		getReadyAttachments,
	};
}

function stripInternalFields(
	record: InternalAttachmentRecord,
): ComposerPendingAttachment {
	return {
		id: record.id,
		kind: record.kind,
		originalName: record.originalName,
		mediaType: record.mediaType,
		sizeBytes: record.sizeBytes,
		status: record.status,
		previewUrl: record.previewUrl,
		errorMessage: record.errorMessage,
	};
}

function extractAttachmentStagedEvent(
	rawFrame: unknown,
): StagedAttachmentReadyEvent | null {
	if (!isRecord(rawFrame)) return null;
	const payload = rawFrame.payload;
	if (!isRecord(payload)) return null;
	if (payload.schema !== "xero.remote_attachment_staged.v1") return null;
	const attachmentId =
		typeof payload.attachmentId === "string" ? payload.attachmentId : null;
	if (!attachmentId) return null;
	const ok = payload.ok !== false;
	const attachment = isRecord(payload.attachment)
		? (payload.attachment as StagedAttachmentReadyEvent["attachment"])
		: undefined;
	const error =
		isRecord(payload.error) && typeof payload.error.message === "string"
			? { message: payload.error.message }
			: null;
	return {
		schema: "xero.remote_attachment_staged.v1",
		ok,
		attachmentId,
		attachment,
		error,
	};
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value && typeof value === "object" && !Array.isArray(value));
}
