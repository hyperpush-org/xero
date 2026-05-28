import * as SelectPrimitive from "@radix-ui/react-select";
import { Activity, Brain, ChevronDown, Cpu, MessageCircle, Settings, ShieldCheck, Sparkles } from "lucide-react";
import {
	type ChangeEvent,
	type CSSProperties,
	Fragment,
	type KeyboardEvent,
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useId,
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
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "../ui/dialog";
import { Drawer, DrawerContent } from "../ui/drawer";
import { Select, SelectContent, SelectItem } from "../ui/select";
import { Switch } from "../ui/switch";
import { Textarea } from "../ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { useIsMobile } from "../ui/use-mobile";
import {
	ComposerAttachButton,
	ComposerMicButton,
	ComposerSendButton,
	ComposerStopButton,
} from "./composer-actions";
import {
	ComposerAttachmentChips,
	type ComposerPendingAttachment,
} from "./composer-attachment-chips";
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

export interface ComposerShortcutBinding {
	/** "Mod" — Cmd on macOS, Ctrl on Windows/Linux. */
	mod: boolean;
	shift: boolean;
	alt: boolean;
	key: string;
}

export const COMPOSER_DICTATION_SHORTCUT: ComposerShortcutBinding = {
	mod: true,
	shift: true,
	alt: false,
	key: "d",
};

/**
 * Minimal dictation contract the composer needs. Compatible with both the
 * browser `useComposerDictation` control and platform-supplied controls (e.g.
 * the Tauri desktop dictation adapter). Optional members are guarded.
 */
export interface ComposerDictationLike {
	audioLevel?: number;
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
	thinkingPlaceholder?: string;

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
	/** Defaults to Cmd/Ctrl+Shift+D. Pass null to disable the composer-owned shortcut. */
	dictationShortcut?: ComposerShortcutBinding | null;
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

const WAVEFORM_BANDS = [0.58, 0.82, 0.46, 1, 0.64, 0.9, 0.52, 0.76, 0.42, 0.68, 0.95, 0.56];

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

function normalizeShortcutKey(key: string): string {
	return key.length === 1 ? key.toLowerCase() : key;
}

function isShortcutBindingEmpty(binding: ComposerShortcutBinding): boolean {
	return binding.key.trim() === "";
}

function detectShortcutPlatform(): "macos" | "other" {
	if (typeof navigator === "undefined") return "other";
	return /mac|iphone|ipad|ipod/i.test(navigator.platform) ? "macos" : "other";
}

function eventMatchesComposerShortcut(
	event: globalThis.KeyboardEvent,
	binding: ComposerShortcutBinding,
): boolean {
	if (isShortcutBindingEmpty(binding)) return false;
	if (normalizeShortcutKey(event.key) !== normalizeShortcutKey(binding.key)) return false;

	const platform = detectShortcutPlatform();
	const modPressed = platform === "macos" ? event.metaKey : event.ctrlKey;
	const otherModPressed = platform === "macos" ? event.ctrlKey : event.metaKey;
	if (binding.mod !== modPressed) return false;
	if (otherModPressed) return false;
	if (binding.shift !== event.shiftKey) return false;
	if (binding.alt !== event.altKey) return false;
	return true;
}

function clampAudioLevel(level: number | null | undefined): number {
	if (level == null || !Number.isFinite(level)) return 0;
	return Math.max(0, Math.min(1, level));
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
	thinkingPlaceholder = "Thinking unavailable",
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
	dictationShortcut,
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
	const autoCompactSwitchId = useId();
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
	const resolvedDictationShortcut =
		dictationShortcut === undefined ? COMPOSER_DICTATION_SHORTCUT : dictationShortcut;
	const dictationRunning =
		dictationVisible &&
		(dictation.isListening || dictation.phase === "requesting" || dictation.phase === "stopping");
	const dictationAudioLevel = clampAudioLevel(dictation.audioLevel);

	const isMobile = useIsMobile();
	const hasText = draftPrompt.trim().length > 0;
	const hasPendingAttachments = (pendingAttachments?.length ?? 0) > 0;
	const sendDisabled = isSendDisabled || (!hasText && !hasPendingAttachments);

	// Compact agent panes adopt the sidebar's flush, dense chrome.
	const dense = inSidebar || density === "compact";
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

	useEffect(() => {
		if (!dictationVisible || !resolvedDictationShortcut) return;
		if (typeof window === "undefined") return;

		const handleDictationShortcut = (event: globalThis.KeyboardEvent) => {
			if (event.repeat) return;
			if (!eventMatchesComposerShortcut(event, resolvedDictationShortcut)) return;
			if (dictation.isToggleDisabled) return;
			event.preventDefault();
			void dictation.toggle();
		};

		window.addEventListener("keydown", handleDictationShortcut);
		return () => window.removeEventListener("keydown", handleDictationShortcut);
	}, [dictation, dictationVisible, resolvedDictationShortcut]);

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
			thinkingOptions={thinkingOptions}
			selectedThinkingId={selectedThinkingId}
			onThinkingChange={onThinkingChange}
			thinkingDisabled={thinkingControlDisabled}
			thinkingPlaceholder={thinkingPlaceholder}
			triggerClassName={inlineTriggerClassName}
		/>
	);

	const inlinePills = (
		<div className="flex min-w-0 items-center gap-0.5 overflow-x-auto pb-0.5">
			{agentPill}
			{modelPill}
		</div>
	);

	const hasDrawerSettingsFields =
		hasAgents || hasModels || typeof onThinkingChange === "function" || showApproval || supportsAutoCompact;
	const hasDesktopSettingsFields = showApproval || supportsAutoCompact;
	const drawerSettingsBody = (
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
			{typeof onThinkingChange === "function" ? (
				<SettingsField
					label="Thinking"
					icon={<Brain aria-hidden="true" className="size-3.5" />}
					groups={[{ id: "thinking", options: thinkingOptions ?? [] }]}
					value={selectedThinkingId ?? null}
					onChange={onThinkingChange}
					contentClassName={drawerSelectContentClassName}
					disabled={thinkingControlDisabled}
					placeholder={thinkingPlaceholder}
				/>
			) : null}
			{showApproval && approvalOptions && onApprovalChange ? (
				<SettingsField
					label="Approval mode"
					icon={<ShieldCheck aria-hidden="true" className="size-3.5" />}
					groups={[{ id: "approval", options: approvalOptions }]}
					value={selectedApprovalId ?? null}
					onChange={onApprovalChange}
					contentClassName={drawerSelectContentClassName}
					disabled={approvalDisabled}
				/>
			) : null}
			{supportsAutoCompact ? (
				<SettingsSwitchRow
					id={autoCompactSwitchId}
					label="Auto-compact before sending"
					icon={<Sparkles aria-hidden="true" className="size-3.5" />}
					checked={autoCompactEnabled === true}
					disabled={autoCompactDisabled}
					onCheckedChange={onAutoCompactEnabledChange ?? (() => undefined)}
				/>
			) : null}
		</div>
	);
	const desktopSettingsBody = (
		<div className="flex flex-col gap-3">
			{showApproval && approvalOptions && onApprovalChange ? (
				<SettingsField
					label="Approval mode"
					icon={<ShieldCheck aria-hidden="true" className="size-3.5" />}
					groups={[{ id: "approval", options: approvalOptions }]}
					value={selectedApprovalId ?? null}
					onChange={onApprovalChange}
					contentClassName={drawerSelectContentClassName}
					disabled={approvalDisabled}
				/>
			) : null}
			{supportsAutoCompact ? (
				<SettingsSwitchRow
					id={autoCompactSwitchId}
					label="Auto-compact before sending"
					icon={<Sparkles aria-hidden="true" className="size-3.5" />}
					checked={autoCompactEnabled === true}
					disabled={autoCompactDisabled}
					onCheckedChange={onAutoCompactEnabledChange ?? (() => undefined)}
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
				"group/composer relative flex w-full flex-col overflow-hidden bg-card/90 supports-[backdrop-filter]:bg-card/75",
				dense
					? "border-t border-border/60 transition-colors focus-within:border-primary/40"
					: "agent-composer-glow rounded-xl border border-border/60 shadow-[0_8px_24px_-12px_rgba(15,23,42,0.12),0_1px_3px_-1px_rgba(15,23,42,0.06)] ring-1 ring-inset ring-foreground/[0.03] backdrop-blur hover:border-border focus-within:border-primary/40 focus-within:ring-primary/20 dark:shadow-[0_20px_60px_-20px_rgba(0,0,0,0.6),0_2px_8px_-2px_rgba(0,0,0,0.3)]",
				dictationRunning && "pb-1.5",
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
								disabled={!hasDrawerSettingsFields}
								onClick={() => setSettingsOpen((open) => !open)}
							/>
							<DrawerContent className="data-[vaul-drawer-direction=bottom]:rounded-t-3xl border-t border-border/60 px-1.5 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
								<div className="px-3 pb-3 pt-4">{drawerSettingsBody}</div>
							</DrawerContent>
						</Drawer>
					) : null}
				</div>
				<div className="ml-auto flex shrink-0 items-center gap-1">
					{contextMeter ? <div className="shrink-0">{contextMeter}</div> : null}
					{!useDrawer && hasDesktopSettingsFields ? (
						<Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
							<SettingsTriggerButton
								open={settingsOpen}
								disabled={!hasDesktopSettingsFields}
								onClick={() => setSettingsOpen((open) => !open)}
								className={actionDensity === "md" ? "h-8 w-8" : undefined}
							/>
							<DialogContent className="gap-0 overflow-hidden border-border/70 bg-background p-0 text-foreground sm:max-w-[420px]">
								<div
									aria-hidden="true"
									className="pointer-events-none absolute inset-x-0 top-0 h-24 bg-gradient-to-b from-primary/[0.06] to-transparent"
								/>
								<div className="relative px-5 pb-2 pt-5">
									<DialogHeader className="gap-1 pr-7">
										<div className="flex items-center gap-2.5">
											<span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
												<Settings aria-hidden="true" className="h-4 w-4" />
											</span>
											<DialogTitle className="text-[15px]">Composer settings</DialogTitle>
										</div>
									</DialogHeader>
									<DialogDescription className="sr-only">
										Adjust approval mode and auto-compact.
									</DialogDescription>
								</div>
								<div className="relative px-5 pb-5 pt-3">{desktopSettingsBody}</div>
							</DialogContent>
						</Dialog>
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
			{dictationRunning ? (
				<ComposerDictationWaveform level={dictationAudioLevel} />
			) : null}
		</div>
	);
}

function ComposerDictationWaveform({ level }: { level: number }) {
	return (
		<div
			aria-hidden="true"
			className="composer-dictation-waveform"
			style={{ "--composer-dictation-level": level } as CSSProperties}
		>
			<div className="composer-dictation-waveform__rail">
				{WAVEFORM_BANDS.map((band, index) => (
					<span
						key={`${band}-${index}`}
						className="composer-dictation-waveform__band"
						style={{ "--composer-dictation-band": band } as CSSProperties}
					/>
				))}
			</div>
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
	disabled?: boolean;
	placeholder?: string;
}

function SettingsField({
	label,
	icon,
	groups,
	value,
	onChange,
	contentClassName,
	disabled,
	placeholder,
}: SettingsFieldProps) {
	const selectedLabel = findOption(groups, value)?.label ?? placeholder ?? label;
	return (
		<label className="flex flex-col gap-2 text-left">
			<span className="flex items-center gap-1.5 px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{icon}
				{label}
			</span>
			<Select
				disabled={disabled}
				value={value ?? undefined}
				onValueChange={onChange}
			>
				<SelectPrimitive.Trigger asChild>
					<button
						type="button"
						aria-label={label}
						disabled={disabled}
						className="flex h-9 w-full items-center justify-between gap-2 rounded-md border border-border/60 bg-background px-2.5 text-[13px] font-medium text-foreground shadow-none transition-colors hover:bg-muted/50 focus-visible:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/15 data-[state=open]:border-primary/40 data-[state=open]:bg-muted/50"
					>
						<span
							className={cn(
								"line-clamp-1 truncate",
								disabled && !findOption(groups, value)
									? "text-muted-foreground"
									: null,
							)}
						>
							{selectedLabel}
						</span>
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

interface SettingsSwitchRowProps {
	id: string;
	label: string;
	icon: ReactNode;
	checked: boolean;
	disabled?: boolean;
	onCheckedChange: (next: boolean) => void;
}

function SettingsSwitchRow({
	id,
	label,
	icon,
	checked,
	disabled,
	onCheckedChange,
}: SettingsSwitchRowProps) {
	return (
			<div className="flex items-center justify-between gap-3 px-1 py-1.5">
			<label
				htmlFor={id}
				className="flex min-w-0 items-center gap-2 text-[13px] font-medium text-foreground"
			>
				<span className="text-muted-foreground">{icon}</span>
				<span className="truncate">{label}</span>
			</label>
			<Switch
				id={id}
				aria-label={label}
				checked={checked}
				disabled={disabled}
				onCheckedChange={onCheckedChange}
			/>
		</div>
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
	className?: string;
}

function SettingsTriggerButton({
	open,
	disabled,
	onClick,
	className,
}: SettingsTriggerButtonProps) {
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className={cn(
						"h-7 w-7 rounded-md text-muted-foreground/80 hover:text-foreground data-[state=open]:bg-muted/60 data-[state=open]:text-foreground",
						className,
					)}
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
