import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";

export type ComposerDictationPhase =
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

export interface UseComposerDictationOptions {
	draftPrompt: string;
	onDraftPromptChange: (value: string) => void;
	textareaRef: RefObject<HTMLTextAreaElement | null>;
}

export interface ComposerDictationControl {
	ariaLabel: string;
	error: string | null;
	isListening: boolean;
	isToggleDisabled: boolean;
	phase: ComposerDictationPhase;
	stopBeforeSubmit: () => Promise<string>;
	tooltip: string;
	toggle: () => Promise<void>;
	updateDraftPrompt: (value: string) => void;
}

export function useComposerDictation({
	draftPrompt,
	onDraftPromptChange,
	textareaRef,
}: UseComposerDictationOptions): ComposerDictationControl {
	const [support, setSupport] =
		useState<WebSpeechRecognitionSupport>("unknown");
	const [phase, setPhase] = useState<ComposerDictationPhase>("idle");
	const [error, setError] = useState<string | null>(null);

	const recognitionRef = useRef<WebSpeechRecognitionLike | null>(null);
	const phaseRef = useRef<ComposerDictationPhase>("idle");
	const draftPromptRef = useRef(draftPrompt);
	const dictationBaseRef = useRef(draftPrompt);
	const renderedDraftRef = useRef(draftPrompt);
	const spokenTextRef = useRef("");
	const spokenOffsetRef = useRef(0);
	const stopResolverRef = useRef<((draft: string) => void) | null>(null);

	const updatePhase = useCallback((nextPhase: ComposerDictationPhase) => {
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
