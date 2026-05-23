import * as SelectPrimitive from "@radix-ui/react-select";
import { Activity, Brain, ChevronDown, Cpu, MessageCircle, Settings, ShieldCheck } from "lucide-react";
import {
	type ChangeEvent,
	Fragment,
	type KeyboardEvent,
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import {
	classificationRejectionMessage,
	classifyAttachment,
} from "../../lib/agent-attachments";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Drawer, DrawerContent } from "../ui/drawer";
import { Select, SelectContent, SelectItem } from "../ui/select";
import { Textarea } from "../ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { useIsMobile } from "../ui/use-mobile";
import {
	ComposerAttachButton,
	ComposerAutoCompactToggle,
	ComposerMicButton,
	ComposerSendButton,
	ComposerStopButton,
} from "./composer-actions";
import {
	ComposerAttachmentChips,
	type ComposerPendingAttachment,
} from "./composer-attachment-chips";
import {
	ComposerInlinePillSelect,
} from "./composer-inline-pill-select";
import {
	ComposerInlineTrigger,
	composerInlineSelectContentClassName,
} from "./composer-inline-trigger";
import { ComposerModelSelect } from "./composer-model-select";
import type { ComposerSelectGroup, ComposerSelectOption } from "./composer-types";
import {
	type ComposerDictationPhase,
	useComposerDictation,
} from "./use-composer-dictation";

export type { ComposerSelectGroup, ComposerSelectOption } from "./composer-types";
export type ComposerPendingAttachmentType = ComposerPendingAttachment;

/**
 * Minimal dictation contract the composer needs. Compatible with both the
 * browser `useComposerDictation` control and platform-supplied controls (e.g.
 * the Tauri desktop dictation adapter). Optional members are guarded.
 */
export interface ComposerDictationLike {
	ariaLabel: string;
	isListening: boolean;
	isToggleDisabled: boolean;
	phase: ComposerDictationPhase;
	tooltip: string;
	toggle: () => Promise<void>;
	/** Hide the mic button when explicitly false. Defaults to visible. */
	isVisible?: boolean;
	error?: string | null;
	stopBeforeSubmit?: () => Promise<string>;
	updateDraftPrompt?: (value: string) => void;
}

export interface ComposerRuntimeError {
	title: string;
	message: string;
	code?: string | null;
}

export interface ComposerProps {
	draftPrompt: string;
	onDraftPromptChange: (value: string) => void;
	onSubmit: (draftPrompt?: string) => void;
	placeholder?: string;
	promptInputRef?: RefObject<HTMLTextAreaElement | null>;
	promptInputLabel?: string;
	isPromptDisabled?: boolean;
	isSendDisabled?: boolean;

	agentGroups: readonly ComposerSelectGroup[];
	selectedAgentId: string | null;
	onAgentChange: (id: string) => void;
	agentDisabled?: boolean;
	agentTooltip?: ReactNode;
	agentTriggerIcon?: ReactNode;
	agentTriggerLabel?: ReactNode;
	agentPlaceholder?: string;

	modelGroups: readonly ComposerSelectGroup[];
	selectedModelId: string | null;
	onModelChange: (id: string) => void;
	modelDisabled?: boolean;

	thinkingOptions?: readonly ComposerSelectOption[];
	selectedThinkingId?: string | null;
	onThinkingChange?: (id: string) => void;
	thinkingDisabled?: boolean;

	approvalOptions?: readonly ComposerSelectOption[];
	selectedApprovalId?: string | null;
	onApprovalChange?: (id: string) => void;
	approvalDisabled?: boolean;

	autoCompactEnabled?: boolean;
	onAutoCompactEnabledChange?: (next: boolean) => void;
	autoCompactDisabled?: boolean;

	pendingAttachments?: readonly ComposerPendingAttachment[];
	onAddFiles?: (files: File[]) => void;
	onRemoveAttachment?: (id: string) => void;

	dictation?: ComposerDictationLike;
	contextMeter?: ReactNode;

	isStopVisible?: boolean;
	isStopDisabled?: boolean;
	onStop?: () => void;
	sendButtonLabel?: string;
	isSendLoading?: boolean;

	error?: ComposerRuntimeError | null;
	onOpenDiagnostics?: () => void;

	density?: "comfortable" | "compact";
	inSidebar?: boolean;
	className?: string;
}

const MAX_VISIBLE_TEXTAREA_ROWS = 6;
const FALLBACK_TEXTAREA_LINE_HEIGHT_PX = 24;

const useIsomorphicLayoutEffect =
	typeof window === "undefined" ? useEffect : useLayoutEffect;

const drawerSelectContentClassName =
	"max-h-72 min-w-[min(20rem,90vw)] border-border/70 bg-card text-foreground shadow-xl";

function parseCssPixelValue(value: string): number | null {
	const parsed = Number.parseFloat(value);
	return Number.isFinite(parsed) ? parsed : null;
}

function getComposerTextareaMaxHeightPx(node: HTMLTextAreaElement): number {
	const style = window.getComputedStyle(node);
	const fontSize =
		parseCssPixelValue(style.fontSize) ?? FALLBACK_TEXTAREA_LINE_HEIGHT_PX;
	const fallbackLineHeight = Math.round(fontSize * 1.6);
	const lineHeight = parseCssPixelValue(style.lineHeight) ?? fallbackLineHeight;
	const paddingBlock =
		(parseCssPixelValue(style.paddingTop) ?? 0) +
		(parseCssPixelValue(style.paddingBottom) ?? 0);
	return Math.ceil(lineHeight * MAX_VISIBLE_TEXTAREA_ROWS + paddingBlock);
}

function resizeComposerTextarea(node: HTMLTextAreaElement) {
	node.style.height = "auto";
	const maxHeight = getComposerTextareaMaxHeightPx(node);
	const scrollHeight = node.scrollHeight;
	const shouldScroll = scrollHeight > maxHeight;
	node.style.height = `${Math.min(scrollHeight, maxHeight)}px`;
	node.style.overflowY = shouldScroll ? "auto" : "hidden";
}

function findOption(
	groups: readonly ComposerSelectGroup[],
	id: string | null,
): ComposerSelectOption | null {
	if (id == null) return null;
	for (const group of groups) {
		const match = group.options.find((option) => option.id === id);
		if (match) return match;
	}
	return null;
}

export function Composer({
	draftPrompt,
	onDraftPromptChange,
	onSubmit,
	placeholder = "Ask anything…",
	promptInputRef,
	promptInputLabel,
	isPromptDisabled = false,
	isSendDisabled = false,
	agentGroups,
	selectedAgentId,
	onAgentChange,
	agentDisabled,
	agentTooltip,
	agentTriggerIcon,
	agentTriggerLabel,
	agentPlaceholder = "Agent",
	modelGroups,
	selectedModelId,
	onModelChange,
	modelDisabled,
	thinkingOptions,
	selectedThinkingId,
	onThinkingChange,
	thinkingDisabled,
	approvalOptions,
	selectedApprovalId,
	onApprovalChange,
	approvalDisabled,
	autoCompactEnabled,
	onAutoCompactEnabledChange,
	autoCompactDisabled,
	pendingAttachments,
	onAddFiles,
	onRemoveAttachment,
	dictation: externalDictation,
	contextMeter,
	isStopVisible = false,
	isStopDisabled = false,
	onStop,
	sendButtonLabel = "Send message",
	isSendLoading = false,
	error,
	onOpenDiagnostics,
	density = "comfortable",
	inSidebar = false,
	className,
}: ComposerProps) {
	const internalTextareaRef = useRef<HTMLTextAreaElement>(null);
	const textareaRef = promptInputRef ?? internalTextareaRef;
	const fileInputRef = useRef<HTMLInputElement>(null);
	const [settingsOpen, setSettingsOpen] = useState(false);
	const [classificationError, setClassificationError] = useState<string | null>(
		null,
	);

	const internalDictation = useComposerDictation({
		draftPrompt,
		onDraftPromptChange,
		textareaRef,
	});
	const usingInternalDictation = externalDictation == null;
	const dictation: ComposerDictationLike = externalDictation ?? internalDictation;
	const dictationVisible = dictation.isVisible !== false;
	const dictationError = dictation.error ?? null;

	const isMobile = useIsMobile();
	const hasText = draftPrompt.trim().length > 0;
	const hasPendingAttachments = (pendingAttachments?.length ?? 0) > 0;
	const sendDisabled = isSendDisabled || (!hasText && !hasPendingAttachments);

	// Compact agent panes adopt the sidebar's flush, dense chrome.
	const dense = inSidebar || density === "compact";
	// The settings menu is mobile-only; every desktop surface shows inline pills.
	const useDrawer = isMobile;
	const showInlinePills = !useDrawer;

	const actionDensity = dense ? "sm" : "md";
	const inlineTriggerClassName = dense
		? "h-6 px-1.5 gap-1.5 text-[11.5px]"
		: undefined;

	useIsomorphicLayoutEffect(() => {
		const node = textareaRef.current;
		if (!node) return;
		resizeComposerTextarea(node);
	}, [draftPrompt, textareaRef]);

	const handleTextareaChange = useCallback(
		(value: string) => {
			if (dictation.updateDraftPrompt) {
				dictation.updateDraftPrompt(value);
			} else {
				onDraftPromptChange(value);
			}
		},
		[dictation, onDraftPromptChange],
	);

	const handleSubmit = useCallback(async () => {
		// Only the browser-backed internal dictation returns the flushed draft.
		// Platform-supplied controls (e.g. desktop) handle stop-before-submit in
		// their own submit path, so we must not call it again here.
		let nextDraft = draftPrompt;
		if (usingInternalDictation && internalDictation.stopBeforeSubmit) {
			nextDraft = await internalDictation.stopBeforeSubmit();
		}
		const hasContent =
			nextDraft.trim().length > 0 || hasPendingAttachments;
		if (isSendDisabled || !hasContent) return;
		onSubmit(nextDraft);
	}, [
		draftPrompt,
		hasPendingAttachments,
		internalDictation,
		isSendDisabled,
		onSubmit,
		usingInternalDictation,
	]);

	const handleKeyDown = useCallback(
		(event: KeyboardEvent<HTMLTextAreaElement>) => {
			if (event.key !== "Enter") return;
			if (event.shiftKey) {
				event.preventDefault();
				const textarea = event.currentTarget;
				const value = textarea.value;
				const selectionStart = textarea.selectionStart ?? value.length;
				const selectionEnd = textarea.selectionEnd ?? selectionStart;
				const nextValue = `${value.slice(0, selectionStart)}\n${value.slice(selectionEnd)}`;
				const nextCursor = selectionStart + 1;
				handleTextareaChange(nextValue);
				window.requestAnimationFrame(() => {
					if (!textarea.isConnected) return;
					textarea.setSelectionRange(nextCursor, nextCursor);
				});
				return;
			}
			if (isStopVisible) return;
			event.preventDefault();
			if (!sendDisabled) void handleSubmit();
		},
		[handleSubmit, handleTextareaChange, isStopVisible, sendDisabled],
	);

	const handleFilesPicked = useCallback(
		(event: ChangeEvent<HTMLInputElement>) => {
			const files = Array.from(event.target.files ?? []);
			event.target.value = "";
			if (files.length === 0 || !onAddFiles) return;
			const accepted: File[] = [];
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
				accepted.push(file);
			}
			setClassificationError(rejections.length > 0 ? rejections.join(" ") : null);
			if (accepted.length > 0) onAddFiles(accepted);
		},
		[onAddFiles],
	);

	const hasThinkingOptions = Boolean(thinkingOptions && thinkingOptions.length > 0);
	// Keep the thinking control visible (disabled when empty) so the toolbar layout
	// stays stable across models, matching the desktop composer.
	const showThinking = typeof onThinkingChange === "function";
	const thinkingControlDisabled = Boolean(thinkingDisabled) || !hasThinkingOptions;
	const showApproval = Boolean(
		approvalOptions && approvalOptions.length > 1 && onApprovalChange,
	);
	const supportsAttachments = typeof onAddFiles === "function";
	const supportsAutoCompact =
		typeof onAutoCompactEnabledChange === "function" &&
		typeof autoCompactEnabled === "boolean";

	const selectedAgentOption = useMemo(
		() => findOption(agentGroups, selectedAgentId),
		[agentGroups, selectedAgentId],
	);
	const resolvedAgentTriggerIcon =
		agentTriggerIcon ??
		selectedAgentOption?.icon ??
		<MessageCircle aria-hidden="true" className="size-3" />;
	const resolvedAgentTriggerLabel =
		agentTriggerLabel ?? selectedAgentOption?.label ?? agentPlaceholder;
	const hasAgents = agentGroups.some((group) => group.options.length > 0);
	const hasModels = modelGroups.length > 0;

	const attachmentsRow =
		pendingAttachments && pendingAttachments.length > 0 ? (
			<div className="border-b border-border/40 px-2.5 py-2">
				<ComposerAttachmentChips
					attachments={pendingAttachments}
					onRemove={onRemoveAttachment}
				/>
			</div>
		) : null;

	const agentPill = hasAgents ? (
		<GroupedPillSelect
			ariaLabel="Agent selector"
			triggerIcon={resolvedAgentTriggerIcon}
			triggerLabel={resolvedAgentTriggerLabel}
			groups={agentGroups}
			value={selectedAgentId}
			onChange={onAgentChange}
			disabled={agentDisabled}
			tooltip={agentTooltip}
			triggerClassName={inlineTriggerClassName}
		/>
	) : null;

	const modelPill = (
		<ComposerModelSelect
			groups={modelGroups}
			value={selectedModelId}
			onChange={onModelChange}
			disabled={modelDisabled}
			triggerClassName={inlineTriggerClassName}
		/>
	);

	const thinkingPill =
		showThinking && onThinkingChange ? (
			<ComposerInlinePillSelect
				ariaLabel="Thinking level selector"
				icon={<Brain aria-hidden="true" className="size-3" />}
				options={thinkingOptions ?? []}
				value={selectedThinkingId ?? null}
				onChange={onThinkingChange}
				disabled={thinkingControlDisabled}
				tooltip="Thinking effort"
				placeholder="Thinking"
				triggerClassName={inlineTriggerClassName}
			/>
		) : null;

	const approvalPill =
		showApproval && approvalOptions && onApprovalChange ? (
			<ComposerInlinePillSelect
				ariaLabel="Approval mode selector"
				icon={<ShieldCheck aria-hidden="true" className="size-3" />}
				options={approvalOptions}
				value={selectedApprovalId ?? null}
				onChange={onApprovalChange}
				disabled={approvalDisabled}
				tooltip="Approval mode"
				placeholder="Approval"
				triggerClassName={inlineTriggerClassName}
			/>
		) : null;

	const inlinePills = (
		<div className="flex min-w-0 items-center gap-0.5 overflow-x-auto pb-0.5">
			{agentPill}
			{modelPill}
			{thinkingPill}
			{approvalPill}
		</div>
	);

	const hasSettingsFields = hasAgents || hasModels || hasThinkingOptions || showApproval;
	const settingsBody = (
		<div className="flex flex-col gap-3">
			{hasAgents ? (
				<SettingsField
					label="Agent"
					icon={<MessageCircle aria-hidden="true" className="size-3.5" />}
					groups={agentGroups}
					value={selectedAgentId}
					onChange={onAgentChange}
					contentClassName={drawerSelectContentClassName}
				/>
			) : null}
			{hasModels ? (
				<label className="flex flex-col gap-1 text-left">
					<span className="flex items-center gap-1.5 px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
						<Cpu aria-hidden="true" className="size-3.5" />
						Model
					</span>
					<ComposerModelSelect
						variant="field"
						groups={modelGroups}
						value={selectedModelId}
						onChange={onModelChange}
						disabled={modelDisabled}
					/>
				</label>
			) : null}
			{hasThinkingOptions && thinkingOptions && onThinkingChange ? (
				<SettingsField
					label="Thinking"
					icon={<Brain aria-hidden="true" className="size-3.5" />}
					groups={[{ id: "thinking", options: thinkingOptions }]}
					value={selectedThinkingId ?? null}
					onChange={onThinkingChange}
					contentClassName={drawerSelectContentClassName}
				/>
			) : null}
			{showApproval && approvalOptions && onApprovalChange ? (
				<SettingsField
					label="Approval"
					icon={<ShieldCheck aria-hidden="true" className="size-3.5" />}
					groups={[{ id: "approval", options: approvalOptions }]}
					value={selectedApprovalId ?? null}
					onChange={onApprovalChange}
					contentClassName={drawerSelectContentClassName}
				/>
			) : null}
		</div>
	);

	const sendOrStop = isStopVisible ? (
		<ComposerStopButton
			density={actionDensity}
			disabled={isStopDisabled || !onStop}
			isLoading={isStopDisabled}
			onClick={() => onStop?.()}
		/>
	) : (
		<ComposerSendButton
			ariaLabel={sendButtonLabel}
			density={actionDensity}
			disabled={sendDisabled}
			isLoading={isSendLoading}
			onClick={() => void handleSubmit()}
			showKbdHint
		/>
	);

	const errorRow = error ? (
		<div
			className="border-t border-destructive/25 bg-destructive/5 px-3 py-2 text-[10px] leading-relaxed text-destructive/90"
			role="alert"
		>
			<div className="flex items-start justify-between gap-2">
				<p className="font-medium">{error.title}</p>
				{onOpenDiagnostics ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-6 shrink-0 gap-1 px-1.5 text-[10.5px] text-destructive hover:bg-destructive/10 hover:text-destructive"
						onClick={onOpenDiagnostics}
					>
						<Activity className="h-3 w-3" />
						Diagnostics
					</Button>
				) : null}
			</div>
			<p>{error.message}</p>
			{error.code ? (
				<p className="font-mono text-[10px]">code: {error.code}</p>
			) : null}
		</div>
	) : null;

	return (
		<div
			className={cn(
				"group/composer flex w-full flex-col overflow-hidden bg-card/90 supports-[backdrop-filter]:bg-card/75",
				dense
					? "border-t border-border/60 transition-colors focus-within:border-primary/40"
					: "agent-composer-glow rounded-xl border border-border/60 shadow-[0_8px_24px_-12px_rgba(15,23,42,0.12),0_1px_3px_-1px_rgba(15,23,42,0.06)] ring-1 ring-inset ring-foreground/[0.03] backdrop-blur hover:border-border focus-within:border-primary/40 focus-within:ring-primary/20 dark:shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)]",
				className,
			)}
		>
			{attachmentsRow}
			<Textarea
				ref={textareaRef}
				value={draftPrompt}
				onChange={(event) => handleTextareaChange(event.target.value)}
				onKeyDown={handleKeyDown}
				placeholder={placeholder}
				aria-label={promptInputLabel}
				disabled={isPromptDisabled}
				rows={1}
				className={cn(
					"field-sizing-fixed resize-none overflow-y-hidden border-0 bg-transparent leading-relaxed text-foreground shadow-none placeholder:text-muted-foreground/55 outline-none focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-100 dark:bg-transparent",
					dense
						? "min-h-[28px] px-3 py-2 text-[13px]"
						: "min-h-[32px] px-4 py-2.5 text-[15px]",
				)}
			/>
			<div
				className={cn(
					"flex items-center gap-1 border-t border-border/40",
					dense ? "px-2 py-1" : "px-2.5 py-1.5",
				)}
			>
				<div className="flex min-w-0 flex-1 items-center gap-1">
					{supportsAttachments ? (
						<ComposerAttachButton
							density={actionDensity}
							onClick={() => fileInputRef.current?.click()}
						/>
					) : null}
					{showInlinePills ? inlinePills : null}
					{useDrawer ? (
						<Drawer open={settingsOpen} onOpenChange={setSettingsOpen}>
							<SettingsTriggerButton
								open={settingsOpen}
								disabled={!hasSettingsFields}
								onClick={() => setSettingsOpen((open) => !open)}
							/>
							<DrawerContent className="data-[vaul-drawer-direction=bottom]:rounded-t-3xl border-t border-border/60 px-1.5 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
								<div className="px-3 pb-3 pt-4">{settingsBody}</div>
							</DrawerContent>
						</Drawer>
					) : null}
				</div>
				<div className="ml-auto flex shrink-0 items-center gap-1">
					{contextMeter ? <div className="shrink-0">{contextMeter}</div> : null}
					{supportsAutoCompact ? (
						<ComposerAutoCompactToggle
							density={actionDensity}
							disabled={autoCompactDisabled}
							enabled={autoCompactEnabled === true}
							onChange={onAutoCompactEnabledChange ?? (() => undefined)}
						/>
					) : null}
					{dictationVisible ? (
						<ComposerMicButton density={actionDensity} dictation={dictation} />
					) : null}
					{sendOrStop}
				</div>
			</div>
			{errorRow}
			{dictationError ? (
				<p
					className="border-t border-border/40 px-2.5 py-1.5 text-[11px] leading-relaxed text-destructive"
					role="alert"
				>
					{dictationError}
				</p>
			) : null}
			{classificationError ? (
				<p
					className="border-t border-border/40 px-2.5 py-1.5 text-[11px] leading-relaxed text-destructive"
					role="alert"
				>
					{classificationError}
				</p>
			) : null}
			{supportsAttachments ? (
				<input
					ref={fileInputRef}
					type="file"
					multiple
					className="sr-only"
					onChange={handleFilesPicked}
					aria-hidden="true"
					tabIndex={-1}
				/>
			) : null}
		</div>
	);
}

interface GroupedPillSelectProps {
	ariaLabel: string;
	triggerIcon: ReactNode;
	triggerLabel: ReactNode;
	groups: readonly ComposerSelectGroup[];
	value: string | null;
	onChange: (id: string) => void;
	disabled?: boolean;
	tooltip?: ReactNode;
	triggerClassName?: string;
}

function GroupedPillSelect({
	ariaLabel,
	triggerIcon,
	triggerLabel,
	groups,
	value,
	onChange,
	disabled,
	tooltip,
	triggerClassName,
}: GroupedPillSelectProps) {
	const trigger = (
		<SelectPrimitive.Trigger asChild>
			<ComposerInlineTrigger
				aria-label={ariaLabel}
				className={triggerClassName}
				disabled={disabled}
				icon={triggerIcon}
				label={triggerLabel}
			/>
		</SelectPrimitive.Trigger>
	);
	return (
		<Select
			disabled={disabled}
			value={value ?? undefined}
			onValueChange={onChange}
		>
			{tooltip ? (
				<Tooltip>
					<TooltipTrigger asChild>{trigger}</TooltipTrigger>
					<TooltipContent side="top">{tooltip}</TooltipContent>
				</Tooltip>
			) : (
				trigger
			)}
			<SelectContent className={composerInlineSelectContentClassName}>
				<SelectGroupOptions groups={groups} />
			</SelectContent>
		</Select>
	);
}

interface SettingsFieldProps {
	label: string;
	icon: ReactNode;
	groups: readonly ComposerSelectGroup[];
	value: string | null;
	onChange: (id: string) => void;
	contentClassName: string;
}

function SettingsField({
	label,
	icon,
	groups,
	value,
	onChange,
	contentClassName,
}: SettingsFieldProps) {
	const selectedLabel = findOption(groups, value)?.label ?? label;
	return (
		<label className="flex flex-col gap-1 text-left">
			<span className="flex items-center gap-1.5 px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{icon}
				{label}
			</span>
			<Select value={value ?? undefined} onValueChange={onChange}>
				<SelectPrimitive.Trigger asChild>
					<button
						type="button"
						aria-label={label}
						className="flex h-9 w-full items-center justify-between gap-2 rounded-md border border-border/60 bg-background px-2.5 text-[13px] font-medium text-foreground shadow-none transition-colors hover:bg-muted/50 focus-visible:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/15 data-[state=open]:border-primary/40 data-[state=open]:bg-muted/50"
					>
						<span className="line-clamp-1 truncate">{selectedLabel}</span>
						<ChevronDown
							aria-hidden="true"
							className="size-3.5 text-muted-foreground/70"
						/>
					</button>
				</SelectPrimitive.Trigger>
				<SelectContent align="start" className={contentClassName}>
					<SelectGroupOptions groups={groups} />
				</SelectContent>
			</Select>
		</label>
	);
}

function SelectGroupOptions({
	groups,
}: {
	groups: readonly ComposerSelectGroup[];
}) {
	return (
		<>
			{groups.map((group, index) => (
				<Fragment key={group.id}>
					{index > 0 ? (
						<SelectPrimitive.Separator className="my-1 h-px bg-border/60" />
					) : null}
					<SelectPrimitive.Group>
						{group.label ? (
							<SelectPrimitive.Label className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70">
								{group.label}
							</SelectPrimitive.Label>
						) : null}
						{group.options.map((option) => (
							<SelectItem
								key={option.id}
								value={option.id}
								disabled={option.disabled}
							>
								<span className="flex items-center gap-1.5">
									{option.icon ?? null}
									{option.label}
									{option.sublabel ? (
										<span className="text-[10px] uppercase tracking-wider text-muted-foreground">
											· {option.sublabel}
										</span>
									) : null}
								</span>
							</SelectItem>
						))}
					</SelectPrimitive.Group>
				</Fragment>
			))}
		</>
	);
}

interface SettingsTriggerButtonProps {
	open: boolean;
	disabled?: boolean;
	onClick?: () => void;
}

function SettingsTriggerButton({
	open,
	disabled,
	onClick,
}: SettingsTriggerButtonProps) {
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="h-7 w-7 rounded-md text-muted-foreground/80 hover:text-foreground data-[state=open]:bg-muted/60 data-[state=open]:text-foreground"
					aria-label="Composer settings"
					aria-haspopup="dialog"
					aria-expanded={open}
					onClick={onClick}
					disabled={disabled}
				>
					<Settings className="h-4 w-4" strokeWidth={2.25} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">Composer settings</TooltipContent>
		</Tooltip>
	);
}
