use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::{
    core::{Error as WindowsError, Interface, HSTRING},
    Foundation::TypedEventHandler,
    Globalization::Language,
    Media::SpeechRecognition::{
        ISpeechRecognitionConstraint, SpeechContinuousRecognitionCompletedEventArgs,
        SpeechContinuousRecognitionResultGeneratedEventArgs, SpeechContinuousRecognitionSession,
        SpeechRecognitionAudioProblem, SpeechRecognitionCompilationResult,
        SpeechRecognitionConfidence, SpeechRecognitionHypothesisGeneratedEventArgs,
        SpeechRecognitionQualityDegradingEventArgs, SpeechRecognitionResult,
        SpeechRecognitionResultStatus, SpeechRecognitionScenario, SpeechRecognitionTopicConstraint,
        SpeechRecognizer,
    },
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
    },
};

use super::*;

const METER_EMIT_INTERVAL: Duration = Duration::from_millis(33);
const FALLBACK_LOCALE: &str = "en-US";
const STOP_REASON_USER: u8 = 0;
const STOP_REASON_CANCELLED: u8 = 1;
const STOP_REASON_ERROR: u8 = 2;

pub(super) struct Session {
    recognizer: SpeechRecognizer,
    continuous: windows::Media::SpeechRecognition::SpeechContinuousRecognitionSession,
    runtime: Arc<WindowsSessionRuntime>,
    meter: Mutex<Option<AudioMeter>>,
    hypothesis_token: i64,
    result_token: i64,
    completed_token: i64,
    quality_token: i64,
    started: AtomicBool,
}

unsafe impl Send for Session {}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowsDictationSession")
            .field("session_id", &self.runtime.session_id)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub(super) fn create(
        request: &NativeSessionRequest,
        context: Arc<NativeCallbackContext>,
    ) -> Result<Self, String> {
        if request.engine != DictationEngineDto::WindowsSdk {
            return Err("windows_session_requires_windows_sdk_engine".into());
        }

        initialize_winrt().map_err(|error| format!("winrt_initialization_failed: {error}"))?;

        let language = language_for_locale(&request.locale).map_err(|error| {
            format!(
                "windows_language_unavailable: {}",
                describe_windows_error(&error)
            )
        })?;
        let recognizer = SpeechRecognizer::Create(&language).map_err(|error| {
            format!(
                "speech_recognizer_create_failed: {}",
                describe_windows_error(&error)
            )
        })?;
        let continuous = recognizer.ContinuousRecognitionSession().map_err(|error| {
            format!(
                "continuous_session_unavailable: {}",
                describe_windows_error(&error)
            )
        })?;
        let runtime = Arc::new(WindowsSessionRuntime::new(request, context));

        let hypothesis_token = register_hypothesis_handler(&recognizer, Arc::clone(&runtime))
            .map_err(|error| {
                format!(
                    "hypothesis_handler_failed: {}",
                    describe_windows_error(&error)
                )
            })?;
        let result_token =
            register_result_handler(&continuous, Arc::clone(&runtime)).map_err(|error| {
                format!("result_handler_failed: {}", describe_windows_error(&error))
            })?;
        let completed_token = register_completed_handler(&continuous, Arc::clone(&runtime))
            .map_err(|error| {
                format!(
                    "completed_handler_failed: {}",
                    describe_windows_error(&error)
                )
            })?;
        let quality_token =
            register_quality_handler(&recognizer, Arc::clone(&runtime)).map_err(|error| {
                format!("quality_handler_failed: {}", describe_windows_error(&error))
            })?;

        Ok(Self {
            recognizer,
            continuous,
            runtime,
            meter: Mutex::new(None),
            hypothesis_token,
            result_token,
            completed_token,
            quality_token,
            started: AtomicBool::new(false),
        })
    }

    pub(super) fn start(&self) -> Result<NativeOperationResponse, NativeOperationError> {
        compile_dictation_constraint(&self.recognizer)?;

        let locale = recognizer_locale(&self.recognizer).unwrap_or_else(|| {
            self.runtime
                .locale
                .clone()
                .unwrap_or_else(|| FALLBACK_LOCALE.to_string())
        });

        self.runtime.emit_permission(
            DictationPermissionStateDto::Authorized,
            DictationPermissionStateDto::Unknown,
        );

        match AudioMeter::start(Arc::clone(&self.runtime)) {
            Ok(meter) => {
                if let Ok(mut guard) = self.meter.lock() {
                    *guard = Some(meter);
                }
            }
            Err(_) => {
                self.runtime.emit_audio_level(0.0);
            }
        }

        self.continuous
            .StartAsync()
            .map_err(|error| windows_start_error("dictation_windows_start_failed", error))?
            .get()
            .map_err(|error| map_windows_start_error(error))?;

        self.started.store(true, Ordering::SeqCst);
        self.runtime.emit_started(locale.clone());

        Ok(NativeOperationResponse {
            ok: true,
            session_id: Some(self.runtime.session_id.clone()),
            engine: Some(DictationEngineDto::WindowsSdk),
            locale: Some(locale),
            code: None,
            message: None,
            retryable: None,
        })
    }

    pub(super) fn stop(&self) -> Result<(), NativeOperationError> {
        self.runtime.set_stop_reason(DictationStopReasonDto::User);
        self.stop_meter();

        if self.started.load(Ordering::SeqCst) {
            self.continuous
                .StopAsync()
                .map_err(|error| windows_start_error("dictation_windows_stop_failed", error))?
                .get()
                .map_err(|error| windows_start_error("dictation_windows_stop_failed", error))?;
        }

        self.runtime.emit_stopped_once(DictationStopReasonDto::User);
        Ok(())
    }

    pub(super) fn cancel(&self) -> Result<(), NativeOperationError> {
        self.runtime
            .set_stop_reason(DictationStopReasonDto::Cancelled);
        self.stop_meter();

        if self.started.load(Ordering::SeqCst) {
            self.continuous
                .CancelAsync()
                .map_err(|error| windows_start_error("dictation_windows_cancel_failed", error))?
                .get()
                .map_err(|error| windows_start_error("dictation_windows_cancel_failed", error))?;
        }

        self.runtime
            .emit_stopped_once(DictationStopReasonDto::Cancelled);
        Ok(())
    }

    fn stop_meter(&self) {
        if let Ok(mut guard) = self.meter.lock() {
            guard.take();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop_meter();
        let _ = self
            .recognizer
            .RemoveHypothesisGenerated(self.hypothesis_token);
        let _ = self.continuous.RemoveResultGenerated(self.result_token);
        let _ = self.continuous.RemoveCompleted(self.completed_token);
        let _ = self
            .recognizer
            .RemoveRecognitionQualityDegrading(self.quality_token);
        if self.started.load(Ordering::SeqCst) && !self.runtime.terminal_sent.load(Ordering::SeqCst)
        {
            let _ = self
                .continuous
                .CancelAsync()
                .and_then(|action| action.get());
        }
        let _ = self.recognizer.Close();
    }
}

pub(super) fn capability_status_json() -> Result<String, String> {
    let mut reason: Option<String> = None;
    let runtime_supported = match initialize_winrt() {
        Ok(()) => true,
        Err(error) => {
            reason = Some(format!("winrt_initialization_failed: {error}"));
            false
        }
    };

    let microphone_available = default_input_device_available();
    let default_locale = if runtime_supported {
        system_speech_locale().ok()
    } else {
        None
    };
    let supported_locales = if runtime_supported {
        supported_topic_locales().unwrap_or_default()
    } else {
        Vec::new()
    };

    let recognizer_result = if runtime_supported {
        probe_recognizer(default_locale.as_deref())
    } else {
        Err(reason
            .clone()
            .unwrap_or_else(|| "windows_speech_runtime_unavailable".into()))
    };
    let recognizer_reason = recognizer_result.err();
    let speech_permission_reason = recognizer_reason
        .as_deref()
        .or(reason.as_deref())
        .map(str::to_owned);
    let recognizer_available = recognizer_reason.is_none() && microphone_available;
    let meter_reason = if recognizer_available {
        audio_meter_probe().err()
    } else {
        None
    };

    if !microphone_available {
        reason = Some("windows_microphone_missing".into());
    } else if let Some(error) = recognizer_reason {
        reason = Some(error);
    } else if let Some(error) = meter_reason {
        reason = Some(error);
    }

    let speech_permission = match speech_permission_reason.as_deref() {
        Some(reason)
            if reason.contains("privacy")
                || reason.contains("permission")
                || reason.contains("speech_service") =>
        {
            "denied"
        }
        _ => "unknown",
    };

    let payload = serde_json::json!({
        "platform": "windows",
        "osVersion": None::<String>,
        "defaultLocale": default_locale,
        "supportedLocales": supported_locales,
        "modernCompiled": false,
        "modernRuntimeSupported": false,
        "modernAssetsStatus": "unavailable",
        "modernAssetsReason": "macos_modern_unavailable_on_windows",
        "legacyRuntimeSupported": false,
        "legacyRecognizerAvailable": false,
        "windowsSdkRuntimeSupported": runtime_supported,
        "windowsSdkRecognizerAvailable": recognizer_available,
        "windowsSdkReason": reason,
        "microphonePermission": if microphone_available { "unknown" } else { "unsupported" },
        "speechPermission": speech_permission,
    });

    serde_json::to_string(&payload)
        .map_err(|error| format!("windows_status_encode_failed: {error}"))
}

struct WindowsSessionRuntime {
    session_id: String,
    locale: Option<String>,
    context: Arc<NativeCallbackContext>,
    sequence: AtomicU64,
    terminal_sent: AtomicBool,
    stop_reason: AtomicU8,
}

// Tauri channels are already used from native callback threads by the macOS shim.
unsafe impl Send for WindowsSessionRuntime {}
unsafe impl Sync for WindowsSessionRuntime {}

impl WindowsSessionRuntime {
    fn new(request: &NativeSessionRequest, context: Arc<NativeCallbackContext>) -> Self {
        Self {
            session_id: request.session_id.clone(),
            locale: normalize_windows_locale(&request.locale),
            context,
            sequence: AtomicU64::new(1),
            terminal_sent: AtomicBool::new(false),
            stop_reason: AtomicU8::new(STOP_REASON_USER),
        }
    }

    fn emit_permission(
        &self,
        microphone: DictationPermissionStateDto,
        speech: DictationPermissionStateDto,
    ) {
        self.emit(DictationEventDto::Permission { microphone, speech });
    }

    fn emit_started(&self, locale: String) {
        self.emit(DictationEventDto::Started {
            session_id: self.session_id.clone(),
            engine: DictationEngineDto::WindowsSdk,
            locale,
        });
    }

    fn emit_partial(&self, text: String) {
        if !text.trim().is_empty() {
            self.emit(DictationEventDto::Partial {
                session_id: self.session_id.clone(),
                text,
                sequence: self.next_sequence(),
            });
        }
    }

    fn emit_final(&self, text: String) {
        if !text.trim().is_empty() {
            self.emit(DictationEventDto::Final {
                session_id: self.session_id.clone(),
                text,
                sequence: self.next_sequence(),
            });
        }
    }

    fn emit_audio_level(&self, level: f32) {
        self.emit(DictationEventDto::AudioLevel {
            session_id: self.session_id.clone(),
            level: if level.is_finite() {
                level.clamp(0.0, 1.0)
            } else {
                0.0
            },
            sequence: self.next_sequence(),
        });
    }

    fn emit_error_once(&self, error: NativeOperationError) {
        if self.terminal_sent.swap(true, Ordering::SeqCst) {
            return;
        }

        self.set_stop_reason(DictationStopReasonDto::Error);
        self.emit(DictationEventDto::Error {
            session_id: Some(self.session_id.clone()),
            code: error.code,
            message: error.message,
            retryable: error.retryable,
        });
    }

    fn emit_stopped_once(&self, reason: DictationStopReasonDto) {
        if self.terminal_sent.swap(true, Ordering::SeqCst) {
            return;
        }

        self.emit(DictationEventDto::Stopped {
            session_id: self.session_id.clone(),
            reason,
        });
    }

    fn set_stop_reason(&self, reason: DictationStopReasonDto) {
        let value = match reason {
            DictationStopReasonDto::Cancelled => STOP_REASON_CANCELLED,
            DictationStopReasonDto::Error => STOP_REASON_ERROR,
            _ => STOP_REASON_USER,
        };
        self.stop_reason.store(value, Ordering::SeqCst);
    }

    fn requested_stop_reason(&self) -> DictationStopReasonDto {
        match self.stop_reason.load(Ordering::SeqCst) {
            STOP_REASON_CANCELLED => DictationStopReasonDto::Cancelled,
            STOP_REASON_ERROR => DictationStopReasonDto::Error,
            _ => DictationStopReasonDto::User,
        }
    }

    fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    fn emit(&self, event: DictationEventDto) {
        let terminal = matches!(
            event,
            DictationEventDto::Stopped { .. } | DictationEventDto::Error { .. }
        );
        if self.context.channel.send(event).is_err() {
            self.cleanup_session(true);
            return;
        }

        if terminal {
            self.cleanup_session(false);
        }
    }

    fn cleanup_session(&self, cancel: bool) {
        let state = self.context.state.clone();
        let session_id = self.session_id.clone();
        std::thread::spawn(move || {
            drop(state.take_session(&session_id, cancel));
        });
    }
}

struct AudioMeter {
    _stream: cpal::Stream,
}

impl AudioMeter {
    fn start(runtime: Arc<WindowsSessionRuntime>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "windows_microphone_missing".to_string())?;
        let supported_config = device
            .default_input_config()
            .map_err(|error| format!("windows_meter_config_unavailable: {error}"))?;
        let sample_format = supported_config.sample_format();
        let config = cpal::StreamConfig::from(supported_config);
        let last_emit = Arc::new(Mutex::new(Instant::now() - METER_EMIT_INTERVAL));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let runtime = Arc::clone(&runtime);
                let last_emit = Arc::clone(&last_emit);
                let err_runtime = Arc::clone(&runtime);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        if let Some(level) = normalized_level_f32(data) {
                            emit_meter_level(&runtime, &last_emit, level);
                        }
                    },
                    move |_error| {
                        err_runtime.emit_audio_level(0.0);
                    },
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let runtime = Arc::clone(&runtime);
                let last_emit = Arc::clone(&last_emit);
                let err_runtime = Arc::clone(&runtime);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        if let Some(level) = normalized_level_i16(data) {
                            emit_meter_level(&runtime, &last_emit, level);
                        }
                    },
                    move |_error| {
                        err_runtime.emit_audio_level(0.0);
                    },
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let runtime = Arc::clone(&runtime);
                let last_emit = Arc::clone(&last_emit);
                let err_runtime = Arc::clone(&runtime);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        if let Some(level) = normalized_level_u16(data) {
                            emit_meter_level(&runtime, &last_emit, level);
                        }
                    },
                    move |_error| {
                        err_runtime.emit_audio_level(0.0);
                    },
                    None,
                )
            }
            _ => {
                return Err(format!(
                    "windows_meter_sample_format_unsupported: {sample_format:?}"
                ))
            }
        }
        .map_err(|error| format!("windows_meter_stream_build_failed: {error}"))?;

        stream
            .play()
            .map_err(|error| format!("windows_meter_stream_start_failed: {error}"))?;

        Ok(Self { _stream: stream })
    }
}

fn register_hypothesis_handler(
    recognizer: &SpeechRecognizer,
    runtime: Arc<WindowsSessionRuntime>,
) -> windows::core::Result<i64> {
    let handler = TypedEventHandler::<
        SpeechRecognizer,
        SpeechRecognitionHypothesisGeneratedEventArgs,
    >::new(move |_sender, args| {
        if let Some(args) = args.as_ref() {
            if let Ok(hypothesis) = args.Hypothesis() {
                if let Ok(text) = hypothesis.Text() {
                    runtime.emit_partial(text.to_string_lossy());
                }
            }
        }
        Ok(())
    });
    recognizer.HypothesisGenerated(&handler)
}

fn register_result_handler(
    continuous: &SpeechContinuousRecognitionSession,
    runtime: Arc<WindowsSessionRuntime>,
) -> windows::core::Result<i64> {
    let handler = TypedEventHandler::<
        SpeechContinuousRecognitionSession,
        SpeechContinuousRecognitionResultGeneratedEventArgs,
    >::new(move |_sender, args| {
        if let Some(args) = args.as_ref() {
            if let Ok(result) = args.Result() {
                handle_recognition_result(&runtime, result);
            }
        }
        Ok(())
    });
    continuous.ResultGenerated(&handler)
}

fn register_completed_handler(
    continuous: &SpeechContinuousRecognitionSession,
    runtime: Arc<WindowsSessionRuntime>,
) -> windows::core::Result<i64> {
    let handler = TypedEventHandler::<
        SpeechContinuousRecognitionSession,
        SpeechContinuousRecognitionCompletedEventArgs,
    >::new(move |_sender, args| {
        if let Some(args) = args.as_ref() {
            match args.Status() {
                Ok(status)
                    if status == SpeechRecognitionResultStatus::Success
                        || status == SpeechRecognitionResultStatus::UserCanceled =>
                {
                    runtime.emit_stopped_once(runtime.requested_stop_reason());
                }
                Ok(status) => runtime.emit_error_once(status_to_native_error(status)),
                Err(error) => runtime.emit_error_once(windows_start_error(
                    "dictation_windows_completed_status_failed",
                    error,
                )),
            }
        }
        Ok(())
    });
    continuous.Completed(&handler)
}

fn register_quality_handler(
    recognizer: &SpeechRecognizer,
    runtime: Arc<WindowsSessionRuntime>,
) -> windows::core::Result<i64> {
    let handler =
        TypedEventHandler::<SpeechRecognizer, SpeechRecognitionQualityDegradingEventArgs>::new(
            move |_sender, args| {
                if let Some(args) = args.as_ref() {
                    if let Ok(problem) = args.Problem() {
                        if problem == SpeechRecognitionAudioProblem::TooNoisy
                            || problem == SpeechRecognitionAudioProblem::TooFast
                            || problem == SpeechRecognitionAudioProblem::TooSlow
                        {
                            runtime.emit_audio_level(0.0);
                        }
                    }
                }
                Ok(())
            },
        );
    recognizer.RecognitionQualityDegrading(&handler)
}

fn handle_recognition_result(runtime: &WindowsSessionRuntime, result: SpeechRecognitionResult) {
    match result.Status() {
        Ok(status) if status == SpeechRecognitionResultStatus::Success => {
            let rejected = result
                .Confidence()
                .map(|confidence| confidence == SpeechRecognitionConfidence::Rejected)
                .unwrap_or(false);
            if !rejected {
                if let Ok(text) = result.Text() {
                    runtime.emit_final(text.to_string_lossy());
                }
            }
        }
        Ok(status) if status == SpeechRecognitionResultStatus::UserCanceled => {
            runtime.emit_stopped_once(runtime.requested_stop_reason());
        }
        Ok(status) => runtime.emit_error_once(status_to_native_error(status)),
        Err(error) => runtime.emit_error_once(windows_start_error(
            "dictation_windows_result_status_failed",
            error,
        )),
    }
}

fn compile_dictation_constraint(
    recognizer: &SpeechRecognizer,
) -> Result<SpeechRecognitionCompilationResult, NativeOperationError> {
    let topic = SpeechRecognitionTopicConstraint::Create(
        SpeechRecognitionScenario::Dictation,
        &HSTRING::from("dictation"),
    )
    .map_err(|error| windows_start_error("dictation_windows_constraint_failed", error))?;
    let constraint = topic
        .cast::<ISpeechRecognitionConstraint>()
        .map_err(|error| windows_start_error("dictation_windows_constraint_failed", error))?;
    recognizer
        .Constraints()
        .map_err(|error| windows_start_error("dictation_windows_constraints_unavailable", error))?
        .Append(&constraint)
        .map_err(|error| windows_start_error("dictation_windows_constraint_failed", error))?;

    let result = recognizer
        .CompileConstraintsAsync()
        .map_err(|error| windows_start_error("dictation_windows_compile_failed", error))?
        .get()
        .map_err(|error| windows_start_error("dictation_windows_compile_failed", error))?;
    let status = result
        .Status()
        .map_err(|error| windows_start_error("dictation_windows_compile_status_failed", error))?;

    if status == SpeechRecognitionResultStatus::Success {
        Ok(result)
    } else {
        Err(status_to_native_error(status))
    }
}

fn probe_recognizer(locale: Option<&str>) -> Result<(), String> {
    let locale = locale.unwrap_or(FALLBACK_LOCALE);
    let language = language_for_locale(locale).map_err(|error| describe_windows_error(&error))?;
    let recognizer =
        SpeechRecognizer::Create(&language).map_err(|error| describe_windows_error(&error))?;
    let compile_result = compile_dictation_constraint(&recognizer).map_err(|error| error.code)?;
    drop(compile_result);
    let _ = recognizer.Close();
    Ok(())
}

fn language_for_locale(locale: &str) -> windows::core::Result<Language> {
    let normalized =
        normalize_windows_locale(locale).unwrap_or_else(|| FALLBACK_LOCALE.to_string());
    Language::CreateLanguage(&HSTRING::from(normalized))
}

fn system_speech_locale() -> windows::core::Result<String> {
    let language = SpeechRecognizer::SystemSpeechLanguage()?;
    Ok(language.LanguageTag()?.to_string_lossy())
}

fn supported_topic_locales() -> windows::core::Result<Vec<String>> {
    let languages = SpeechRecognizer::SupportedTopicLanguages()?;
    let size = languages.Size()?;
    let mut locales = Vec::new();
    for index in 0..size {
        if let Ok(language) = languages.GetAt(index) {
            if let Ok(tag) = language.LanguageTag() {
                let tag = tag.to_string_lossy();
                if !tag.trim().is_empty() {
                    locales.push(tag);
                }
            }
        }
    }
    locales.sort();
    locales.dedup();
    Ok(locales)
}

fn recognizer_locale(recognizer: &SpeechRecognizer) -> Option<String> {
    recognizer
        .CurrentLanguage()
        .ok()
        .and_then(|language| language.LanguageTag().ok())
        .map(|tag| tag.to_string_lossy())
        .and_then(|tag| normalize_windows_locale(&tag))
}

fn initialize_winrt() -> Result<(), String> {
    static WINRT_INIT: OnceLock<Result<(), String>> = OnceLock::new();
    WINRT_INIT
        .get_or_init(|| {
            let result = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            match result {
                Ok(()) => Ok(()),
                Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(()),
                Err(error) => Err(describe_windows_error(&error)),
            }
        })
        .clone()
}

fn default_input_device_available() -> bool {
    cpal::default_host().default_input_device().is_some()
}

fn audio_meter_probe() -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "windows_microphone_missing".to_string())?;
    device
        .default_input_config()
        .map_err(|error| format!("windows_meter_config_unavailable: {error}"))?;
    Ok(())
}

fn normalize_windows_locale(locale: &str) -> Option<String> {
    let normalized = locale.trim().replace('_', "-");
    (!normalized.is_empty()).then_some(normalized)
}

fn status_to_native_error(status: SpeechRecognitionResultStatus) -> NativeOperationError {
    let (code, message, retryable) = if status
        == SpeechRecognitionResultStatus::TopicLanguageNotSupported
        || status == SpeechRecognitionResultStatus::GrammarLanguageMismatch
    {
        (
            "dictation_windows_locale_unsupported",
            "Windows Speech Recognition does not support the selected dictation locale.",
            false,
        )
    } else if status == SpeechRecognitionResultStatus::NetworkFailure {
        (
            "dictation_windows_speech_service_unavailable",
            "Windows Speech Recognition could not reach the Windows speech service. Check online speech recognition privacy settings and network access.",
            true,
        )
    } else if status == SpeechRecognitionResultStatus::MicrophoneUnavailable {
        (
            "dictation_windows_microphone_unavailable",
            "Windows Speech Recognition could not access a microphone.",
            true,
        )
    } else if status == SpeechRecognitionResultStatus::AudioQualityFailure {
        (
            "dictation_windows_audio_quality_failed",
            "Windows Speech Recognition could not continue because microphone audio quality was too low.",
            true,
        )
    } else if status == SpeechRecognitionResultStatus::UserCanceled {
        (
            "dictation_windows_cancelled",
            "Windows dictation was cancelled.",
            false,
        )
    } else {
        (
            "dictation_windows_recognition_failed",
            "Windows Speech Recognition could not complete dictation.",
            true,
        )
    };

    NativeOperationError {
        code: code.into(),
        message: format!("{message} Status: {}.", status_reason(status)),
        retryable,
    }
}

fn map_windows_start_error(error: WindowsError) -> NativeOperationError {
    let description = describe_windows_error(&error);
    let lower = description.to_ascii_lowercase();
    if lower.contains("microphone") {
        return NativeOperationError {
            code: "dictation_microphone_permission_denied".into(),
            message: format!("Windows blocked microphone access for dictation. {description}"),
            retryable: false,
        };
    }

    if lower.contains("privacy") || lower.contains("speech") {
        return NativeOperationError {
            code: "dictation_speech_permission_denied".into(),
            message: format!("Windows blocked speech recognition for dictation. {description}"),
            retryable: false,
        };
    }

    windows_start_error("dictation_windows_start_failed", error)
}

fn windows_start_error(code: &'static str, error: WindowsError) -> NativeOperationError {
    NativeOperationError {
        code: code.into(),
        message: format!(
            "Windows Speech Recognition failed: {}",
            describe_windows_error(&error)
        ),
        retryable: true,
    }
}

fn describe_windows_error(error: &WindowsError) -> String {
    let message = error.message();
    if message.trim().is_empty() {
        format!("HRESULT 0x{:08X}", error.code().0 as u32)
    } else {
        format!("{} (HRESULT 0x{:08X})", message, error.code().0 as u32)
    }
}

fn status_reason(status: SpeechRecognitionResultStatus) -> &'static str {
    if status == SpeechRecognitionResultStatus::Success {
        "success"
    } else if status == SpeechRecognitionResultStatus::TopicLanguageNotSupported {
        "topic_language_not_supported"
    } else if status == SpeechRecognitionResultStatus::GrammarLanguageMismatch {
        "grammar_language_mismatch"
    } else if status == SpeechRecognitionResultStatus::GrammarCompilationFailure {
        "grammar_compilation_failure"
    } else if status == SpeechRecognitionResultStatus::AudioQualityFailure {
        "audio_quality_failure"
    } else if status == SpeechRecognitionResultStatus::UserCanceled {
        "user_cancelled"
    } else if status == SpeechRecognitionResultStatus::TimeoutExceeded {
        "timeout_exceeded"
    } else if status == SpeechRecognitionResultStatus::PauseLimitExceeded {
        "pause_limit_exceeded"
    } else if status == SpeechRecognitionResultStatus::NetworkFailure {
        "network_failure"
    } else if status == SpeechRecognitionResultStatus::MicrophoneUnavailable {
        "microphone_unavailable"
    } else {
        "unknown"
    }
}

fn emit_meter_level(runtime: &WindowsSessionRuntime, last_emit: &Arc<Mutex<Instant>>, level: f32) {
    let Ok(mut last_emit) = last_emit.lock() else {
        return;
    };
    if last_emit.elapsed() < METER_EMIT_INTERVAL {
        return;
    }
    *last_emit = Instant::now();
    runtime.emit_audio_level(level);
}

fn normalized_level_f32(samples: &[f32]) -> Option<f32> {
    normalized_level(samples.iter().copied())
}

fn normalized_level_i16(samples: &[i16]) -> Option<f32> {
    normalized_level(
        samples
            .iter()
            .map(|sample| *sample as f32 / i16::MAX as f32),
    )
}

fn normalized_level_u16(samples: &[u16]) -> Option<f32> {
    normalized_level(
        samples
            .iter()
            .map(|sample| ((*sample as f32) - 32768.0) / 32768.0),
    )
}

fn normalized_level(samples: impl Iterator<Item = f32>) -> Option<f32> {
    let mut sum_squares = 0.0f64;
    let mut count = 0u64;

    for sample in samples {
        if sample.is_finite() {
            let sample = sample as f64;
            sum_squares += sample * sample;
            count += 1;
        }
    }

    if count == 0 {
        return None;
    }

    let rms = (sum_squares / count as f64).sqrt();
    let decibels = 20.0 * rms.max(0.000_001).log10();
    Some(((decibels + 60.0) / 60.0).clamp(0.0, 1.0) as f32)
}
