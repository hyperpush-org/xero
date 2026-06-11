import {
	AlertTriangle,
	FileText,
	Image as ImageIcon,
	LoaderCircle,
	Maximize2,
	PencilLine,
	SquareDashedMousePointer,
	X,
} from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import { type AgentAttachmentKind, formatBytes } from "../../lib/agent-attachments";
import { cn } from "../../lib/utils";
import { ImageLightbox } from "../image-lightbox";

export interface ComposerPendingAttachment {
	id: string;
	kind: AgentAttachmentKind;
	originalName: string;
	mediaType: string;
	sizeBytes: number;
	status: "staging" | "ready" | "error";
	previewUrl?: string;
	absolutePath?: string;
	errorMessage?: string;
}

export interface ComposerPendingContext {
	id: string;
	kind: "element" | "sketch";
	title: string;
	subtitle?: string;
}

export interface ComposerAttachmentChipsProps {
	attachments: readonly ComposerPendingAttachment[];
	contexts?: readonly ComposerPendingContext[];
	onRemove?: (id: string) => void;
	onRemoveContext?: (id: string) => void;
}

export function ComposerAttachmentChips({
	attachments,
	contexts = [],
	onRemove,
	onRemoveContext,
}: ComposerAttachmentChipsProps) {
	const reduceMotion = useReducedMotion();
	const chipMotion = getChipMotion(reduceMotion);

	return (
		<div
			className="flex flex-wrap items-center gap-1.5"
			role="list"
			aria-label="Pending composer context and attachments"
		>
			<AnimatePresence>
				{contexts.map((context) => (
					<ComposerContextChip
						key={`context-${context.id}`}
						context={context}
						motionProps={chipMotion}
						onRemove={onRemoveContext}
					/>
				))}
				{attachments.map((attachment) => (
					<ComposerAttachmentChip
						key={`attachment-${attachment.id}`}
						attachment={attachment}
						motionProps={chipMotion}
						onRemove={onRemove}
					/>
				))}
			</AnimatePresence>
		</div>
	);
}

function getChipMotion(reduceMotion: boolean | null) {
	if (reduceMotion) {
		return {
			initial: false,
			animate: { opacity: 1 },
			exit: { opacity: 0 },
			transition: { duration: 0 },
		} as const;
	}
	return {
		initial: { opacity: 0, scale: 0.92, y: 8, filter: "blur(6px)" },
		animate: {
			opacity: 1,
			scale: 1,
			y: 0,
			filter: "blur(0px)",
			transition: { type: "spring", duration: 0.38, bounce: 0 },
		},
		exit: {
			opacity: 0,
			scale: 0.94,
			y: 6,
			filter: "blur(5px)",
			transition: { duration: 0.23, ease: [0.2, 0, 0, 1] },
		},
	} as const;
}

interface ComposerContextChipProps {
	context: ComposerPendingContext;
	motionProps: ReturnType<typeof getChipMotion>;
	onRemove?: (id: string) => void;
}

function ComposerContextChip({
	context,
	motionProps,
	onRemove,
}: ComposerContextChipProps) {
	const subtitle = context.subtitle?.trim() || "Metadata attached";
	const Icon = context.kind === "sketch" ? PencilLine : SquareDashedMousePointer;
	const truncatedTitle =
		context.title.length > 24
			? `${context.title.slice(0, 21)}…`
			: context.title;

	return (
		<motion.div
			layout
			{...motionProps}
			role="listitem"
			data-context-id={context.id}
			className="group relative flex max-w-[220px] items-center gap-2 rounded-md border border-border/60 bg-muted/40 py-1 pl-1 pr-1.5 text-[11px] text-foreground shadow-sm"
		>
			<div className="relative flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-sm bg-background text-muted-foreground">
				<Icon
					className="h-4 w-4"
					aria-hidden="true"
					strokeWidth={2.1}
				/>
			</div>
			<div className="flex min-w-0 flex-1 flex-col leading-tight">
				<span className="line-clamp-1 truncate font-medium" title={context.title}>
					{truncatedTitle}
				</span>
				<span className="line-clamp-1 truncate text-[10px] text-muted-foreground" title={subtitle}>
					{subtitle}
				</span>
			</div>
			{onRemove ? (
				<button
					type="button"
					aria-label={`Remove ${context.title}`}
					className="ml-1 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					onClick={() => onRemove(context.id)}
				>
					<X className="h-3 w-3" aria-hidden="true" />
				</button>
			) : null}
		</motion.div>
	);
}

interface ComposerAttachmentChipProps {
	attachment: ComposerPendingAttachment;
	motionProps: ReturnType<typeof getChipMotion>;
	onRemove?: (id: string) => void;
}

function ComposerAttachmentChip({
	attachment,
	motionProps,
	onRemove,
}: ComposerAttachmentChipProps) {
	const [isPreviewOpen, setIsPreviewOpen] = useState(false);
	const isImage = attachment.kind === "image";
	const isStaging = attachment.status === "staging";
	const isError = attachment.status === "error";
	const previewUrl = attachment.previewUrl;
	const truncatedName =
		attachment.originalName.length > 24
			? `${attachment.originalName.slice(0, 21)}…`
			: attachment.originalName;

	return (
		<motion.div
			layout
			{...motionProps}
			role="listitem"
			data-attachment-id={attachment.id}
			data-attachment-status={attachment.status}
			className={cn(
				"group relative flex max-w-[220px] items-center gap-2 rounded-md border border-border/60 bg-muted/40 py-1 pl-1 pr-1.5 text-[11px] text-foreground shadow-sm",
				isError ? "border-destructive/60 bg-destructive/10" : null,
			)}
		>
			{isImage && previewUrl ? (
				<ImageLightbox
					open={isPreviewOpen}
					onOpenChange={setIsPreviewOpen}
					src={previewUrl}
					title={attachment.originalName}
					alt={attachment.originalName}
					mediaType={attachment.mediaType}
					downloadName={attachment.originalName}
					trigger={
						<button
							type="button"
							aria-label={`Open image preview for ${attachment.originalName}`}
							className={cn(
								"group/attachment-preview relative shrink-0 rounded-sm",
								"focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
							)}
						>
							<AttachmentThumbnail
								isError={isError}
								isImage={isImage}
								isStaging={isStaging}
								previewUrl={previewUrl}
							/>
							<span
								aria-hidden="true"
								className="absolute right-0.5 top-0.5 inline-flex h-4 w-4 items-center justify-center rounded-sm bg-background/80 text-muted-foreground opacity-0 shadow-sm ring-1 ring-border/50 backdrop-blur transition-opacity group-hover/attachment-preview:opacity-100"
							>
								<Maximize2 className="h-2.5 w-2.5" />
							</span>
						</button>
					}
				/>
			) : (
				<AttachmentThumbnail
					isError={isError}
					isImage={isImage}
					isStaging={isStaging}
					previewUrl={previewUrl}
				/>
			)}
			<div className="flex min-w-0 flex-1 flex-col leading-tight">
				<span
					className="line-clamp-1 truncate font-medium"
					title={attachment.originalName}
				>
					{truncatedName}
				</span>
				<span className="text-[10px] text-muted-foreground">
					{isError
						? (attachment.errorMessage ?? "Upload failed")
						: formatBytes(attachment.sizeBytes)}
				</span>
			</div>
			{onRemove && !isStaging ? (
				<button
					type="button"
					aria-label={`Remove ${attachment.originalName}`}
					className="ml-1 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					onClick={() => onRemove(attachment.id)}
				>
					<X className="h-3 w-3" aria-hidden="true" />
				</button>
			) : null}
		</motion.div>
	);
}

interface AttachmentThumbnailProps {
	isError: boolean;
	isImage: boolean;
	isStaging: boolean;
	previewUrl?: string;
}

function AttachmentThumbnail({
	isError,
	isImage,
	isStaging,
	previewUrl,
}: AttachmentThumbnailProps) {
	return (
		<div className="relative flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-sm bg-background text-muted-foreground">
			{isImage && previewUrl ? (
				// biome-ignore lint/performance/noImgElement: chip preview uses a local object URL
				<img
					src={previewUrl}
					alt=""
					className="h-full w-full object-cover"
					draggable={false}
				/>
			) : isError ? (
				<AlertTriangle className="h-4 w-4 text-destructive" aria-hidden="true" />
			) : isImage ? (
				<ImageIcon className="h-3.5 w-3.5" aria-hidden="true" strokeWidth={2.25} />
			) : (
				<FileText className="h-3.5 w-3.5" aria-hidden="true" strokeWidth={2.25} />
			)}
			{isStaging ? (
				<span
					className="absolute inset-0 flex items-center justify-center bg-background/60"
					aria-hidden="true"
				>
					<LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" />
				</span>
			) : null}
		</div>
	);
}
