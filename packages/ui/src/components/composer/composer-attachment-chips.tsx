import {
	AlertTriangle,
	FileText,
	Image as ImageIcon,
	LoaderCircle,
	X,
} from "lucide-react";
import { type AgentAttachmentKind, formatBytes } from "../../lib/agent-attachments";
import { cn } from "../../lib/utils";

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

export interface ComposerAttachmentChipsProps {
	attachments: readonly ComposerPendingAttachment[];
	onRemove?: (id: string) => void;
}

export function ComposerAttachmentChips({
	attachments,
	onRemove,
}: ComposerAttachmentChipsProps) {
	return (
		<div
			className="flex flex-wrap items-center gap-1.5"
			role="list"
			aria-label="Pending attachments"
		>
			{attachments.map((attachment) => (
				<ComposerAttachmentChip
					key={attachment.id}
					attachment={attachment}
					onRemove={onRemove}
				/>
			))}
		</div>
	);
}

interface ComposerAttachmentChipProps {
	attachment: ComposerPendingAttachment;
	onRemove?: (id: string) => void;
}

function ComposerAttachmentChip({
	attachment,
	onRemove,
}: ComposerAttachmentChipProps) {
	const isImage = attachment.kind === "image";
	const isStaging = attachment.status === "staging";
	const isError = attachment.status === "error";
	const previewUrl = attachment.previewUrl;
	const truncatedName =
		attachment.originalName.length > 24
			? `${attachment.originalName.slice(0, 21)}…`
			: attachment.originalName;

	return (
		<div
			role="listitem"
			data-attachment-id={attachment.id}
			data-attachment-status={attachment.status}
			className={cn(
				"group relative flex max-w-[220px] items-center gap-2 rounded-md border border-border/60 bg-muted/40 py-1 pl-1 pr-1.5 text-[11px] text-foreground shadow-sm",
				isError ? "border-destructive/60 bg-destructive/10" : null,
			)}
		>
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
		</div>
	);
}

