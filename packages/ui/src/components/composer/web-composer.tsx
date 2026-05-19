import * as SelectPrimitive from "@radix-ui/react-select";
import {
	ArrowUp,
	Brain,
	ChevronDown,
	Cpu,
	FileText,
	Image as ImageIcon,
	LoaderCircle,
	MessageCircle,
	Mic,
	Plus,
	Settings,
	X,
} from "lucide-react";
import {
	type ChangeEvent,
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
	formatBytes,
	type AgentAttachmentKind,
} from "../../lib/agent-attachments";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Drawer, DrawerContent } from "../ui/drawer";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import { Select, SelectContent, SelectItem } from "../ui/select";
import { Textarea } from "../ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { useIsMobile } from "../ui/use-mobile";

export interface WebComposerSelectOption {
	id: string;
	label: string;
}

export interface WebComposerPendingAttachment {
	id: string;
	kind: AgentAttachmentKind;
	originalName: string;
	mediaType: string;
	sizeBytes: number;
	status: "staging" | "ready" | "error";
	previewUrl?: string;
	errorMessage?: string;
}

export interface WebComposerProps {
	draftPrompt: string;
	onDraftPromptChange: (value: string) => void;
	onSubmit: (draftPrompt?: string) => void;
	isSendDisabled?: boolean;
	placeholder?: string;
	agentOptions: readonly WebComposerSelectOption[];
	selectedAgentId: string | null;
	onAgentChange: (id: string) => void;
	modelOptions: readonly WebComposerSelectOption[];
	selectedModelId: string | null;
	onModelChange: (id: string) => void;
	thinkingOptions?: readonly WebComposerSelectOption[];
	selectedThinkingId?: string | null;
	onThinkingChange?: (id: string) => void;
	pendingAttachments?: readonly WebComposerPendingAttachment[];
	onAddFiles?: (files: File[]) => void;
	onRemoveAttachment?: (id: string) => void;
	contextMeter?: ReactNode;
	className?: string;
}

type WebSpeechRecognitionPhase =
	| "idle"
	| "requesting"
	| "listening"
	| "stopping";
type WebSpeechRecognitionSupport = "unknown" | "supported" | "unsupported";

interface WebSpeechRecognitionAlternativeLike {
	transcript: string;
}

interface WebSpeechRecognitionResultLike {
	isFinal: boolean;
	length: number;
	item?: (index: number) => WebSpeechRecognitionAlternativeLike;
	[index: number]: WebSpeechRecognitionAlternativeLike | undefined;
}

interface WebSpeechRecognitionResultListLike {
	length: number;
	item?: (index: number) => WebSpeechRecognitionResultLike;
	[index: number]: WebSpeechRecognitionResultLike | undefined;
}

interface WebSpeechRecognitionEventLike extends Event {
	results: WebSpeechRecognitionResultListLike;
}

interface WebSpeechRecognitionErrorEventLike extends Event {
	error?: string;
	message?: string;
}

interface WebSpeechRecognitionLike {
	continuous: boolean;
	interimResults: boolean;
	lang: string;
	onend: ((event: Event) => void) | null;
	onerror: ((event: WebSpeechRecognitionErrorEventLike) => void) | null;
	onresult: ((event: WebSpeechRecognitionEventLike) => void) | null;
	onstart: ((event: Event) => void) | null;
	abort: () => void;
	start: () => void;
	stop: () => void;
}

type WebSpeechRecognitionConstructor = new () => WebSpeechRecognitionLike;

const MAX_VISIBLE_TEXTAREA_ROWS = 6;
const FALLBACK_TEXTAREA_LINE_HEIGHT_PX = 24;

const useIsomorphicLayoutEffect =
	typeof window === "undefined" ? useEffect : useLayoutEffect;

const inlineSelectContentClassName =
	"max-h-72 min-w-40 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90";

const drawerSelectContentClassName =
	"max-h-72 min-w-[min(20rem,90vw)] border-border/70 bg-card text-foreground shadow-xl";

function getSpeechRecognitionConstructor(): WebSpeechRecognitionConstructor | null {
	if (typeof window === "undefined") return null;
	const speechWindow = window as Window & {
		SpeechRecognition?: WebSpeechRecognitionConstructor;
		webkitSpeechRecognition?: WebSpeechRecognitionConstructor;
	};
	return (
		speechWindow.SpeechRecognition ??
		speechWindow.webkitSpeechRecognition ??
		null
	);
}

function appendDictationSegment(
	baseDraft: string,
	dictatedText: string,
): string {
	const segment = dictatedText.trimStart();
	if (segment.length === 0) return baseDraft;
	if (baseDraft.length === 0) return segment;

	const startsWithPunctuation = /^[,.;:!?)]/.test(segment);
	const needsSpace = !/\s$/.test(baseDraft) && !startsWithPunctuation;
	return `${baseDraft}${needsSpace ? " " : ""}${segment}`;
}

function getRecognitionResult(
	results: WebSpeechRecognitionResultListLike,
	index: number,
): WebSpeechRecognitionResultLike | undefined {
	return results[index] ?? results.item?.(index);
}

function getRecognitionAlternative(
	result: WebSpeechRecognitionResultLike,
	index: number,
): WebSpeechRecognitionAlternativeLike | undefined {
	return result[index] ?? result.item?.(index);
}

function getRecognizedText(
	results: WebSpeechRecognitionResultListLike,
): string {
	const segments: string[] = [];
	for (let index = 0; index < results.length; index += 1) {
		const result = getRecognitionResult(results, index);
		const transcript = result
			? getRecognitionAlternative(result, 0)?.transcript.trim()
			: null;
		if (transcript) segments.push(transcript);
	}
	return segments.join(" ");
}

function getVoiceInputErrorMessage(
	event: WebSpeechRecognitionErrorEventLike,
): string {
	if (event.error === "not-allowed" || event.error === "service-not-allowed") {
		return "Allow microphone access to use voice input.";
	}
	if (event.error === "no-speech") {
		return "No speech was detected. Try again when you are ready.";
	}
	return event.message?.trim() || "Voice input stopped unexpectedly.";
}

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

interface UseWebSpeechDictationOptions {
	draftPrompt: string;
	onDraftPromptChange: (value: string) => void;
	textareaRef: RefObject<HTMLTextAreaElement | null>;
}

function useWebSpeechDictation({
	draftPrompt,
	onDraftPromptChange,
	textareaRef,
}: UseWebSpeechDictationOptions) {
	const [support, setSupport] =
		useState<WebSpeechRecognitionSupport>("unknown");
	const [phase, setPhase] = useState<WebSpeechRecognitionPhase>("idle");
	const [error, setError] = useState<string | null>(null);

	const recognitionRef = useRef<WebSpeechRecognitionLike | null>(null);
	const phaseRef = useRef<WebSpeechRecognitionPhase>("idle");
	const draftPromptRef = useRef(draftPrompt);
	const dictationBaseRef = useRef(draftPrompt);
	const renderedDraftRef = useRef(draftPrompt);
	const spokenTextRef = useRef("");
	const spokenOffsetRef = useRef(0);
	const stopResolverRef = useRef<((draft: string) => void) | null>(null);

	const updatePhase = useCallback((nextPhase: WebSpeechRecognitionPhase) => {
		phaseRef.current = nextPhase;
		setPhase(nextPhase);
	}, []);

	const updateDraftPrompt = useCallback(
		(value: string) => {
			draftPromptRef.current = value;
			onDraftPromptChange(value);
		},
		[onDraftPromptChange],
	);

	useEffect(() => {
		setSupport(getSpeechRecognitionConstructor() ? "supported" : "unsupported");
	}, []);

	useEffect(() => {
		draftPromptRef.current = draftPrompt;
		if (phaseRef.current === "idle") {
			dictationBaseRef.current = draftPrompt;
			renderedDraftRef.current = draftPrompt;
			spokenTextRef.current = "";
			spokenOffsetRef.current = 0;
		}
	}, [draftPrompt]);

	const detachRecognition = useCallback(
		(recognition: WebSpeechRecognitionLike) => {
			recognition.onend = null;
			recognition.onerror = null;
			recognition.onresult = null;
			recognition.onstart = null;
		},
		[],
	);

	const releaseRecognition = useCallback(() => {
		const recognition = recognitionRef.current;
		if (recognition) detachRecognition(recognition);
		recognitionRef.current = null;
		dictationBaseRef.current = draftPromptRef.current;
		renderedDraftRef.current = draftPromptRef.current;
		spokenTextRef.current = "";
		spokenOffsetRef.current = 0;

		const resolveStop = stopResolverRef.current;
		stopResolverRef.current = null;
		resolveStop?.(draftPromptRef.current);
	}, [detachRecognition]);

	const focusTextarea = useCallback(() => {
		window.requestAnimationFrame(() => textareaRef.current?.focus());
	}, [textareaRef]);

	const applyRecognizedText = useCallback(
		(nextSpokenText: string) => {
			const currentDraft = draftPromptRef.current;
			const previousSpokenText = spokenTextRef.current;

			if (currentDraft !== renderedDraftRef.current) {
				dictationBaseRef.current = currentDraft;
				spokenOffsetRef.current = previousSpokenText.length;
			}

			const dictatedSegment = nextSpokenText.slice(spokenOffsetRef.current);
			const nextDraft = appendDictationSegment(
				dictationBaseRef.current,
				dictatedSegment,
			);

			spokenTextRef.current = nextSpokenText;
			renderedDraftRef.current = nextDraft;
			updateDraftPrompt(nextDraft);
		},
		[updateDraftPrompt],
	);

	const start = useCallback(() => {
		if (phaseRef.current !== "idle") return;

		const Recognition = getSpeechRecognitionConstructor();
		if (!Recognition) {
			setSupport("unsupported");
			setError("Voice input is not available in this browser.");
			return;
		}

		const recognition = new Recognition();
		recognition.continuous = true;
		recognition.interimResults = true;
		recognition.lang =
			typeof navigator !== "undefined" ? navigator.language : "en-US";
		recognitionRef.current = recognition;
		dictationBaseRef.current = draftPromptRef.current;
		renderedDraftRef.current = draftPromptRef.current;
		spokenTextRef.current = "";
		spokenOffsetRef.current = 0;
		setError(null);
		updatePhase("requesting");

		recognition.onstart = () => {
			updatePhase("listening");
			focusTextarea();
		};
		recognition.onresult = (event) => {
			updatePhase("listening");
			applyRecognizedText(getRecognizedText(event.results));
		};
		recognition.onerror = (event) => {
			setError(getVoiceInputErrorMessage(event));
			releaseRecognition();
			updatePhase("idle");
		};
		recognition.onend = () => {
			releaseRecognition();
			updatePhase("idle");
		};

		try {
			recognition.start();
		} catch (startError) {
			releaseRecognition();
			updatePhase("idle");
			setError(
				startError instanceof Error
					? startError.message
					: "Voice input could not start.",
			);
		}
	}, [applyRecognizedText, focusTextarea, releaseRecognition, updatePhase]);

	const stopBeforeSubmit = useCallback(async (): Promise<string> => {
		const recognition = recognitionRef.current;
		if (!recognition || phaseRef.current === "idle") {
			return draftPromptRef.current;
		}

		updatePhase("stopping");
		return new Promise((resolve) => {
			const timeout = window.setTimeout(() => {
				releaseRecognition();
				updatePhase("idle");
			}, 1_000);

			stopResolverRef.current = (nextDraft) => {
				window.clearTimeout(timeout);
				resolve(nextDraft);
			};

			try {
				recognition.stop();
			} catch {
				releaseRecognition();
				updatePhase("idle");
			}
		});
	}, [releaseRecognition, updatePhase]);

	const toggle = useCallback(async () => {
		if (phaseRef.current === "listening") {
			await stopBeforeSubmit();
			return;
		}
		start();
	}, [start, stopBeforeSubmit]);

	useEffect(() => {
		return () => {
			const recognition = recognitionRef.current;
			if (!recognition) return;
			detachRecognition(recognition);
			recognitionRef.current = null;
			recognition.abort();
		};
	}, [detachRecognition]);

	const isListening = phase === "listening";
	const isBusy = phase === "requesting" || phase === "stopping";
	const isToggleDisabled = support !== "supported" || isBusy;
	const ariaLabel = isListening ? "Stop dictation" : "Start dictation";
	const tooltip =
		phase === "requesting"
			? "Requesting microphone access"
			: phase === "stopping"
				? "Stopping dictation"
				: support === "unsupported"
					? "Voice input is not available in this browser"
					: (error ?? ariaLabel);

	return {
		ariaLabel,
		error,
		isListening,
		isToggleDisabled,
		phase,
		stopBeforeSubmit,
		tooltip,
		toggle,
		updateDraftPrompt,
	};
}

export function WebComposer({
	draftPrompt,
	onDraftPromptChange,
	onSubmit,
	isSendDisabled = false,
	placeholder = "Ask anything…",
	agentOptions,
	selectedAgentId,
	onAgentChange,
	modelOptions,
	selectedModelId,
	onModelChange,
	thinkingOptions,
	selectedThinkingId,
	onThinkingChange,
	pendingAttachments,
	onAddFiles,
	onRemoveAttachment,
	contextMeter,
	className,
}: WebComposerProps) {
	const textareaRef = useRef<HTMLTextAreaElement>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const [settingsOpen, setSettingsOpen] = useState(false);
	const [classificationError, setClassificationError] = useState<string | null>(
		null,
	);
	const dictation = useWebSpeechDictation({
		draftPrompt,
		onDraftPromptChange,
		textareaRef,
	});
	const isMobile = useIsMobile();
	const hasText = draftPrompt.trim().length > 0;
	const disabled = isSendDisabled || !hasText;

	useIsomorphicLayoutEffect(() => {
		const node = textareaRef.current;
		if (!node) return;
		resizeComposerTextarea(node);
	}, [draftPrompt]);

	const handleSubmit = useCallback(async () => {
		const nextDraft = await dictation.stopBeforeSubmit();
		if (isSendDisabled || nextDraft.trim().length === 0) return;
		onSubmit(nextDraft);
	}, [dictation, isSendDisabled, onSubmit]);

	const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
		if (event.key === "Enter" && !event.shiftKey) {
			event.preventDefault();
			if (!disabled) void handleSubmit();
		}
	};

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
			setClassificationError(
				rejections.length > 0 ? rejections.join(" ") : null,
			);
			if (accepted.length > 0) onAddFiles(accepted);
		},
		[onAddFiles],
	);

	const agentLabel =
		agentOptions.find((option) => option.id === selectedAgentId)?.label ??
		"Agent";
	const modelLabel =
		modelOptions.find((option) => option.id === selectedModelId)?.label ??
		"Model";
	const thinkingLabel = useMemo(() => {
		if (!thinkingOptions || thinkingOptions.length === 0) return null;
		return (
			thinkingOptions.find((option) => option.id === selectedThinkingId)
				?.label ?? thinkingOptions[0]?.label ?? null
		);
	}, [thinkingOptions, selectedThinkingId]);

	const hasThinking = Boolean(thinkingOptions && thinkingOptions.length > 0);
	const supportsAttachments = typeof onAddFiles === "function";
	const settingsSections: SettingsSection[] = useMemo(() => {
		const sections: SettingsSection[] = [];
		if (agentOptions.length > 0) {
			sections.push({
				key: "agent",
				label: "Agent",
				icon: <MessageCircle aria-hidden="true" className="size-3.5" />,
				value: selectedAgentId ?? agentOptions[0]?.id ?? "",
				options: agentOptions,
				onChange: onAgentChange,
				placeholder: agentLabel,
			});
		}
		if (modelOptions.length > 0) {
			sections.push({
				key: "model",
				label: "Model",
				icon: <Cpu aria-hidden="true" className="size-3.5" />,
				value: selectedModelId ?? modelOptions[0]?.id ?? "",
				options: modelOptions,
				onChange: onModelChange,
				placeholder: modelLabel,
			});
		}
		if (hasThinking && thinkingOptions && onThinkingChange) {
			sections.push({
				key: "thinking",
				label: "Thinking",
				icon: <Brain aria-hidden="true" className="size-3.5" />,
				value: selectedThinkingId ?? thinkingOptions[0]?.id ?? "",
				options: thinkingOptions,
				onChange: onThinkingChange,
				placeholder: thinkingLabel ?? "Thinking",
			});
		}
		return sections;
	}, [
		agentLabel,
		agentOptions,
		hasThinking,
		modelLabel,
		modelOptions,
		onAgentChange,
		onModelChange,
		onThinkingChange,
		selectedAgentId,
		selectedModelId,
		selectedThinkingId,
		thinkingLabel,
		thinkingOptions,
	]);

	const settingsButton = (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="h-7 w-7 rounded-md text-muted-foreground/80 hover:text-foreground data-[state=open]:bg-muted/60 data-[state=open]:text-foreground"
					aria-label="Composer settings"
					aria-haspopup="dialog"
					aria-expanded={settingsOpen}
					onClick={() => setSettingsOpen((open) => !open)}
					disabled={settingsSections.length === 0}
				>
					<Settings className="h-4 w-4" strokeWidth={2.25} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">Composer settings</TooltipContent>
		</Tooltip>
	);

	const attachButton = supportsAttachments ? (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="h-7 w-7 rounded-md text-muted-foreground/80 hover:text-foreground"
					aria-label="Add files"
					onClick={() => fileInputRef.current?.click()}
				>
					<Plus className="h-4 w-4" strokeWidth={2.25} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">Add files</TooltipContent>
		</Tooltip>
	) : null;

	const attachmentsRow =
		pendingAttachments && pendingAttachments.length > 0 ? (
			<div className="border-b border-border/40 px-2.5 py-2">
				<ComposerAttachmentChips
					attachments={pendingAttachments}
					onRemove={onRemoveAttachment}
				/>
			</div>
		) : null;

	return (
		<div
			className={cn(
				"flex w-full flex-col rounded-2xl border border-border/60 bg-card shadow-none",
				className,
			)}
		>
			{attachmentsRow}
			<Textarea
				ref={textareaRef}
				value={draftPrompt}
				onChange={(event) => dictation.updateDraftPrompt(event.target.value)}
				onKeyDown={handleKeyDown}
				placeholder={placeholder}
				rows={1}
				className="field-sizing-fixed min-h-[32px] resize-none overflow-y-hidden border-0 bg-transparent px-4 py-2.5 text-[15px] leading-relaxed shadow-none placeholder:text-muted-foreground/60 focus-visible:ring-0 dark:bg-transparent"
			/>
			<div className="flex items-center gap-1 border-t border-border/40 px-2.5 py-1.5">
				<div className="flex min-w-0 items-center gap-1">
					{attachButton}
					{isMobile ? (
						<Drawer open={settingsOpen} onOpenChange={setSettingsOpen}>
							{settingsButton}
							<DrawerContent className="data-[vaul-drawer-direction=bottom]:rounded-t-3xl border-t border-border/60 px-1.5 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
								<div className="flex flex-col gap-3 px-3 pb-3 pt-4">
									{settingsSections.map((section) => (
										<ComposerSettingsField
											key={section.key}
											section={section}
											contentClassName={drawerSelectContentClassName}
										/>
									))}
								</div>
							</DrawerContent>
						</Drawer>
					) : (
						<Popover open={settingsOpen} onOpenChange={setSettingsOpen}>
							<PopoverTrigger asChild>{settingsButton}</PopoverTrigger>
							<PopoverContent
								align="start"
								side="top"
								sideOffset={8}
								className="w-72 border-border/70 bg-card/95 p-3 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90"
							>
								<p className="px-1 pb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
									Composer settings
								</p>
								<div className="flex flex-col gap-2.5">
									{settingsSections.map((section) => (
										<ComposerSettingsField
											key={section.key}
											section={section}
											contentClassName={inlineSelectContentClassName}
										/>
									))}
								</div>
							</PopoverContent>
						</Popover>
					)}
				</div>
				<div className="ml-auto flex shrink-0 items-center gap-1">
					{contextMeter ? <div className="shrink-0">{contextMeter}</div> : null}
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant={dictation.isListening ? "outline" : "ghost"}
								size="icon"
								className={cn(
									"h-7 w-7 rounded-md text-muted-foreground/70 hover:text-foreground",
									dictation.isListening
										? "border-destructive/35 bg-destructive/10 text-destructive hover:bg-destructive/15 hover:text-destructive"
										: null,
								)}
								disabled={dictation.isToggleDisabled}
								aria-label={dictation.ariaLabel}
								aria-pressed={dictation.isListening}
								onClick={() => void dictation.toggle()}
							>
								{dictation.phase === "requesting" ||
								dictation.phase === "stopping" ? (
									<LoaderCircle
										className="h-4 w-4 animate-spin"
										strokeWidth={2.25}
									/>
								) : (
									<Mic
										className={cn(
											"h-4 w-4",
											dictation.isListening ? "animate-pulse" : null,
										)}
										strokeWidth={2.25}
									/>
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{dictation.tooltip}</TooltipContent>
					</Tooltip>
					<Button
						type="button"
						size="icon"
						variant="secondary"
						className="h-7 w-7 rounded-md"
						onClick={() => void handleSubmit()}
						disabled={disabled}
						aria-label="Send message"
					>
						<ArrowUp className="h-4 w-4" strokeWidth={2.25} />
					</Button>
				</div>
			</div>
			{dictation.error ? (
				<p
					className="border-t border-border/40 px-2.5 py-1.5 text-[11px] leading-relaxed text-destructive"
					role="alert"
				>
					{dictation.error}
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

interface SettingsSection {
	key: string;
	label: string;
	icon: ReactNode;
	value: string;
	options: readonly WebComposerSelectOption[];
	onChange: (id: string) => void;
	placeholder: string;
}

interface ComposerSettingsFieldProps {
	section: SettingsSection;
	contentClassName: string;
}

function ComposerSettingsField({
	section,
	contentClassName,
}: ComposerSettingsFieldProps) {
	return (
		<label className="flex flex-col gap-1 text-left">
			<span className="flex items-center gap-1.5 px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{section.icon}
				{section.label}
			</span>
			<Select
				value={section.value || undefined}
				onValueChange={section.onChange}
			>
				<SelectPrimitive.Trigger asChild>
					<button
						type="button"
						aria-label={section.label}
						className="flex h-9 w-full items-center justify-between gap-2 rounded-md border border-border/60 bg-background px-2.5 text-[13px] font-medium text-foreground shadow-none transition-colors hover:bg-muted/50 focus-visible:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/15 data-[state=open]:border-primary/40 data-[state=open]:bg-muted/50"
					>
						<span className="line-clamp-1 truncate">
							{section.options.find((option) => option.id === section.value)
								?.label ?? section.placeholder}
						</span>
						<ChevronDown
							aria-hidden="true"
							className="size-3.5 text-muted-foreground/70"
						/>
					</button>
				</SelectPrimitive.Trigger>
				<SelectContent align="start" className={contentClassName}>
					{section.options.map((option) => (
						<SelectItem key={option.id} value={option.id}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</label>
	);
}

interface ComposerAttachmentChipsProps {
	attachments: readonly WebComposerPendingAttachment[];
	onRemove?: (id: string) => void;
}

function ComposerAttachmentChips({
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
	attachment: WebComposerPendingAttachment;
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
		attachment.originalName.length > 28
			? `${attachment.originalName.slice(0, 25)}…`
			: attachment.originalName;

	return (
		<div
			role="listitem"
			data-attachment-id={attachment.id}
			data-attachment-status={attachment.status}
			className={cn(
				"group relative flex max-w-[240px] items-center gap-2 rounded-md border border-border/60 bg-muted/40 py-1 pl-1 pr-1.5 text-[11px] text-foreground shadow-sm",
				isError ? "border-destructive/40 bg-destructive/5" : null,
			)}
		>
			<span className="flex size-7 shrink-0 items-center justify-center overflow-hidden rounded bg-background text-muted-foreground">
				{isImage && previewUrl ? (
					// biome-ignore lint/performance/noImgElement: chip preview uses a local object URL
					<img
						src={previewUrl}
						alt=""
						className="h-full w-full object-cover"
					/>
				) : isImage ? (
					<ImageIcon className="h-3.5 w-3.5" strokeWidth={2.25} />
				) : (
					<FileText className="h-3.5 w-3.5" strokeWidth={2.25} />
				)}
			</span>
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
			{isStaging ? (
				<LoaderCircle
					className="h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground"
					strokeWidth={2.25}
					aria-label="Uploading"
				/>
			) : (
				<button
					type="button"
					aria-label={`Remove ${attachment.originalName}`}
					className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground"
					onClick={() => onRemove?.(attachment.id)}
				>
					<X className="h-3 w-3" strokeWidth={2.5} />
				</button>
			)}
		</div>
	);
}
