import * as SelectPrimitive from "@radix-ui/react-select";
import {
	ArrowUp,
	ChevronDown,
	Cpu,
	LoaderCircle,
	MessageCircle,
	Mic,
} from "lucide-react";
import {
	type ComponentPropsWithoutRef,
	forwardRef,
	type KeyboardEvent,
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Select, SelectContent, SelectItem } from "../ui/select";
import { Textarea } from "../ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";

export interface WebComposerSelectOption {
	id: string;
	label: string;
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

const inlineTriggerClassName =
	"flex h-7 w-fit min-w-0 items-center gap-1.5 rounded-md border-0 bg-transparent px-1.5 text-[13px] font-medium text-muted-foreground/90 whitespace-nowrap shadow-none transition-colors outline-none hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg]:text-muted-foreground/70";

const inlineSelectContentClassName =
	"max-h-72 min-w-40 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90";

interface ComposerInlineTriggerProps
	extends ComponentPropsWithoutRef<"button"> {
	icon: ReactNode;
	label: ReactNode;
}

const ComposerInlineTrigger = forwardRef<
	HTMLButtonElement,
	ComposerInlineTriggerProps
>(function ComposerInlineTrigger({ icon, label, className, ...props }, ref) {
	return (
		<button
			ref={ref}
			type="button"
			className={cn(inlineTriggerClassName, className)}
			{...props}
		>
			{icon}
			<span className="line-clamp-1 truncate">{label}</span>
			<ChevronDown aria-hidden="true" className="size-3.5 opacity-60" />
		</button>
	);
});

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
	className,
}: WebComposerProps) {
	const textareaRef = useRef<HTMLTextAreaElement>(null);
	const dictation = useWebSpeechDictation({
		draftPrompt,
		onDraftPromptChange,
		textareaRef,
	});
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

	const agentLabel =
		agentOptions.find((option) => option.id === selectedAgentId)?.label ??
		"Agent";
	const modelLabel =
		modelOptions.find((option) => option.id === selectedModelId)?.label ??
		"Model";

	return (
		<div
			className={cn(
				"flex w-full flex-col gap-1 rounded-2xl border border-border/60 bg-card px-2.5 py-2 shadow-none",
				className,
			)}
		>
			<Textarea
				ref={textareaRef}
				value={draftPrompt}
				onChange={(event) => dictation.updateDraftPrompt(event.target.value)}
				onKeyDown={handleKeyDown}
				placeholder={placeholder}
				rows={1}
				className="field-sizing-fixed min-h-[32px] resize-none overflow-y-hidden border-0 bg-transparent px-1.5 py-1 text-[15px] leading-relaxed shadow-none placeholder:text-muted-foreground/60 focus-visible:ring-0 dark:bg-transparent"
			/>
			<div className="flex items-center gap-1">
				<ComposerInlineSelect
					icon={<MessageCircle aria-hidden="true" className="size-3.5" />}
					label={agentLabel}
					value={selectedAgentId}
					options={agentOptions}
					onChange={onAgentChange}
					ariaLabel="Agent"
				/>
				<ComposerInlineSelect
					icon={<Cpu aria-hidden="true" className="size-3.5" />}
					label={modelLabel}
					value={selectedModelId}
					options={modelOptions}
					onChange={onModelChange}
					ariaLabel="Model"
				/>
				<div className="ml-auto flex items-center gap-1">
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
					className="px-1.5 text-[11px] leading-relaxed text-destructive"
					role="alert"
				>
					{dictation.error}
				</p>
			) : null}
		</div>
	);
}

interface ComposerInlineSelectProps {
	icon: ReactNode;
	label: string;
	value: string | null;
	options: readonly WebComposerSelectOption[];
	onChange: (id: string) => void;
	ariaLabel: string;
}

function ComposerInlineSelect({
	icon,
	label,
	value,
	options,
	onChange,
	ariaLabel,
}: ComposerInlineSelectProps) {
	if (options.length === 0) return null;
	return (
		<Select value={value ?? undefined} onValueChange={onChange}>
			<SelectPrimitive.Trigger asChild>
				<ComposerInlineTrigger
					aria-label={ariaLabel}
					icon={icon}
					label={label}
				/>
			</SelectPrimitive.Trigger>
			<SelectContent align="start" className={inlineSelectContentClassName}>
				{options.map((option) => (
					<SelectItem key={option.id} value={option.id}>
						{option.label}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}
