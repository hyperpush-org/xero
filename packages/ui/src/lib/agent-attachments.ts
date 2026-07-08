export type AgentAttachmentKind = "image" | "document" | "text";

export type AgentAttachmentClassification =
	| { kind: AgentAttachmentKind; mediaType: string }
	| { kind: null; reason: "unsupported" | "too_large" | "empty" };

export interface AgentAttachmentCompatibilityProfile {
	modelLabel?: string | null;
	label?: string | null;
	displayName?: string | null;
	providerId?: string | null;
	modelId?: string | null;
	inputModalities?: readonly string[] | null;
	attachmentStatus?: string | null;
	supportedTypes?: readonly string[] | null;
	capabilities?: {
		capabilities?: {
			attachments?: {
				status?: string | null;
				supportedTypes?: readonly string[] | null;
			} | null;
		} | null;
	} | null;
}

export type AgentAttachmentCompatibilityResult =
	| { supported: true }
	| {
			supported: false;
			requiredModality: "image" | "file";
			message: string;
	  };

export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024;
export const MAX_TOTAL_ATTACHMENT_BYTES = 50 * 1024 * 1024;

const IMAGE_MIME_TYPES = new Set<string>([
	"image/png",
	"image/jpeg",
	"image/jpg",
	"image/gif",
	"image/webp",
]);

const DOCUMENT_MIME_TYPES = new Set<string>(["application/pdf"]);

const TEXT_MIME_TYPES = new Set<string>([
	"application/json",
	"application/javascript",
	"application/x-typescript",
	"application/typescript",
	"application/xml",
	"application/x-yaml",
	"application/x-toml",
	"application/sql",
	"application/x-sh",
]);

const EXTENSION_FALLBACKS: Record<string, string> = {
	png: "image/png",
	jpg: "image/jpeg",
	jpeg: "image/jpeg",
	gif: "image/gif",
	webp: "image/webp",
	pdf: "application/pdf",
	txt: "text/plain",
	md: "text/markdown",
	markdown: "text/markdown",
	html: "text/html",
	htm: "text/html",
	css: "text/css",
	csv: "text/csv",
	json: "application/json",
	js: "application/javascript",
	mjs: "application/javascript",
	cjs: "application/javascript",
	ts: "application/x-typescript",
	tsx: "application/x-typescript",
	jsx: "application/javascript",
	xml: "application/xml",
	yml: "application/x-yaml",
	yaml: "application/x-yaml",
	toml: "application/x-toml",
	sql: "application/sql",
	sh: "application/x-sh",
	bash: "application/x-sh",
	zsh: "application/x-sh",
	rs: "text/plain",
	py: "text/plain",
	go: "text/plain",
	rb: "text/plain",
	c: "text/plain",
	h: "text/plain",
	cpp: "text/plain",
	hpp: "text/plain",
	swift: "text/plain",
	kt: "text/plain",
	java: "text/plain",
	log: "text/plain",
	conf: "text/plain",
	ini: "text/plain",
	env: "text/plain",
	dockerfile: "text/plain",
};

export function classifyAttachment(file: {
	type: string;
	name: string;
	size: number;
}): AgentAttachmentClassification {
	if (file.size === 0) {
		return { kind: null, reason: "empty" };
	}
	if (file.size > MAX_ATTACHMENT_BYTES) {
		return { kind: null, reason: "too_large" };
	}
	const mediaType = resolveMediaType(file.type, file.name);
	if (!mediaType) {
		return { kind: null, reason: "unsupported" };
	}
	const lower = mediaType.toLowerCase();
	if (lower.startsWith("image/") && IMAGE_MIME_TYPES.has(lower)) {
		return { kind: "image", mediaType: lower };
	}
	if (DOCUMENT_MIME_TYPES.has(lower)) {
		return { kind: "document", mediaType: lower };
	}
	if (lower.startsWith("text/") || TEXT_MIME_TYPES.has(lower)) {
		return { kind: "text", mediaType: lower };
	}
	return { kind: null, reason: "unsupported" };
}

export function requiredAttachmentInputModality(
	kind: AgentAttachmentKind,
): "image" | "file" | null {
	switch (kind) {
		case "image":
			return "image";
		case "document":
			return "file";
		case "text":
			return null;
	}
}

export function checkAttachmentModelCompatibility(
	attachment: { kind: AgentAttachmentKind; mediaType?: string | null },
	profile: AgentAttachmentCompatibilityProfile | null | undefined,
): AgentAttachmentCompatibilityResult {
	const requiredModality = requiredAttachmentInputModality(attachment.kind);
	if (requiredModality === null) {
		return { supported: true };
	}

	const normalizedProfile = normalizeAttachmentCompatibilityProfile(profile);
	if (!normalizedProfile) {
		return {
			supported: false,
			requiredModality,
			message: `The selected model's support for ${attachmentModalityLabel(requiredModality)} attachments is unknown.`,
		};
	}

	const mediaType = normalizeMediaType(attachment.mediaType ?? "");
	const supportedValues = [
		...normalizedProfile.inputModalities,
		...normalizedProfile.supportedTypes,
	].map(normalizeCapabilityValue);
	const hasRequiredModality = supportedValues.some((value) =>
		attachmentCapabilityMatches(value, requiredModality, mediaType),
	) || providerModelSupportsAttachmentModality(
		normalizedProfile.providerId,
		normalizedProfile.modelId,
		requiredModality,
	);

	if (hasRequiredModality) {
		return { supported: true };
	}

	return {
		supported: false,
		requiredModality,
		message: `${normalizedProfile.modelLabel} does not support ${attachmentModalityLabel(requiredModality)} attachments.`,
	};
}

export function attachmentCompatibilityRejectionMessage(
	file: { name: string },
	classification: Extract<AgentAttachmentClassification, { kind: AgentAttachmentKind }>,
	profile: AgentAttachmentCompatibilityProfile | null | undefined,
): string | null {
	const compatibility = checkAttachmentModelCompatibility(
		{ kind: classification.kind, mediaType: classification.mediaType },
		profile,
	);
	if (compatibility.supported) {
		return null;
	}
	return `Skipped "${file.name}" — ${compatibility.message} Choose a compatible model or remove this file.`;
}

function normalizeAttachmentCompatibilityProfile(
	profile: AgentAttachmentCompatibilityProfile | null | undefined,
): {
	modelLabel: string;
	providerId: string | null;
	modelId: string | null;
	inputModalities: readonly string[];
	supportedTypes: readonly string[];
} | null {
	if (!profile) return null;
	const attachments = profile.capabilities?.capabilities?.attachments ?? null;
	const inputModalities = normalizeCapabilityList(profile.inputModalities);
	const supportedTypes = normalizeCapabilityList(
		profile.supportedTypes ?? attachments?.supportedTypes ?? null,
	);
	return {
		modelLabel: selectedModelLabel(profile),
		providerId: normalizeOptionalIdentifier(profile.providerId),
		modelId: normalizeOptionalIdentifier(profile.modelId),
		inputModalities,
		supportedTypes,
	};
}

function normalizeCapabilityList(
	values: readonly string[] | null | undefined,
): readonly string[] {
	if (!values) return [];
	return values
		.map((value) => value.trim())
		.filter((value) => value.length > 0);
}

function selectedModelLabel(profile: AgentAttachmentCompatibilityProfile): string {
	const label =
		profile.modelLabel ??
		profile.displayName ??
		profile.label ??
		profile.modelId ??
		null;
	const trimmed = label?.trim() ?? "";
	return trimmed.length > 0 ? trimmed : "The selected model";
}

function normalizeCapabilityValue(value: string): string {
	return value.trim().toLowerCase().replace(/-/g, "_");
}

function normalizeOptionalIdentifier(value: string | null | undefined): string | null {
	const normalized = value?.trim().toLowerCase() ?? "";
	return normalized.length > 0 ? normalized : null;
}

function normalizeMediaType(value: string): string {
	return value.split(";")[0]?.trim().toLowerCase() ?? "";
}

function attachmentCapabilityMatches(
	value: string,
	requiredModality: "image" | "file",
	mediaType: string,
): boolean {
	if (value === requiredModality) return true;
	if (mediaType && value === mediaType) return true;
	if (requiredModality === "image") {
		return (
			value === "image/*" ||
			(mediaType.startsWith("image/") && value.startsWith("image/"))
		);
	}
	return value === "document" || value === "pdf" || value === "application/pdf";
}

function providerModelSupportsAttachmentModality(
	providerId: string | null,
	modelId: string | null,
	requiredModality: "image" | "file",
): boolean {
	if (!providerId || !modelId) return false;
	if (
		requiredModality === "image" &&
		providerId === "xai" &&
		["grok-4.5", "grok-latest", "grok-4.3", "grok-4.3-latest", "grok-build-0.1"].includes(
			modelId.split("/").pop() ?? modelId,
		)
	) {
		return true;
	}
	if (providerId !== "openai_codex" && providerId !== "openai_api") {
		return false;
	}
	const modelName = modelId.split("/").pop() ?? modelId;
	return (
		isOpenAiGptAttachmentModel(modelName) &&
		(requiredModality === "image" || requiredModality === "file")
	);
}

function isOpenAiGptAttachmentModel(modelName: string): boolean {
	const normalized = modelName.trim().toLowerCase();
	if (normalized === "chat-latest") return true;
	if (!normalized.startsWith("gpt-")) return false;
	if (
		normalized.startsWith("gpt-image") ||
		normalized.startsWith("gpt-audio") ||
		normalized.startsWith("gpt-realtime") ||
		normalized.includes("search") ||
		normalized.includes("transcribe") ||
		normalized.includes("tts")
	) {
		return false;
	}
	return (
		normalized === "gpt-5" ||
		normalized.startsWith("gpt-5.") ||
		normalized.startsWith("gpt-5-") ||
		normalized === "gpt-4.1" ||
		normalized.startsWith("gpt-4.1-") ||
		normalized === "gpt-4o" ||
		normalized.startsWith("gpt-4o-")
	);
}

function attachmentModalityLabel(modality: "image" | "file"): string {
	return modality === "image" ? "image" : "file";
}

function resolveMediaType(
	reportedType: string,
	fileName: string,
): string | null {
	const trimmed = reportedType.trim().toLowerCase();
	if (trimmed && trimmed !== "application/octet-stream") {
		return trimmed;
	}
	return mediaTypeFromExtension(fileName);
}

function mediaTypeFromExtension(fileName: string): string | null {
	const lastDot = fileName.lastIndexOf(".");
	if (lastDot <= 0) {
		if (fileName.toLowerCase() === "dockerfile")
			return EXTENSION_FALLBACKS.dockerfile;
		return null;
	}
	const ext = fileName.slice(lastDot + 1).toLowerCase();
	return EXTENSION_FALLBACKS[ext] ?? null;
}

export function classificationRejectionMessage(
	file: { name: string; size: number },
	classification: Extract<AgentAttachmentClassification, { kind: null }>,
): string {
	switch (classification.reason) {
		case "empty":
			return `Skipped "${file.name}" because it is empty.`;
		case "too_large":
			return `Skipped "${file.name}" because it is larger than ${formatBytes(MAX_ATTACHMENT_BYTES)} (got ${formatBytes(file.size)}).`;
		case "unsupported":
			return `Skipped "${file.name}" — that file type can't be sent to the agent yet.`;
	}
}

export function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	const units = ["KB", "MB", "GB"];
	let value = bytes / 1024;
	let unitIndex = 0;
	while (value >= 1024 && unitIndex < units.length - 1) {
		value /= 1024;
		unitIndex += 1;
	}
	return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`;
}
