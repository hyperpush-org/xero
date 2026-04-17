pub mod platform_adapter;
pub mod protocol;
pub mod stream;
pub mod supervisor;

pub use platform_adapter::{
    bind_openai_callback_listener, default_openai_callback_policy, resolve_openai_callback_policy,
    resolve_runtime_shell_selection, resolve_runtime_shell_selection_for_platform,
    resolve_runtime_supervisor_binary, resolve_runtime_supervisor_binary_with_current_executable,
    OpenAiCallbackBindResult, OpenAiCallbackPolicy, RuntimeAdapterDiagnostic, RuntimePlatform,
    RuntimeShellSelection, RuntimeShellSource, RuntimeSupervisorBinaryResolution,
};
pub use stream::{start_runtime_stream, RuntimeStreamController, RuntimeStreamRequest};
pub use supervisor::{
    launch_detached_runtime_supervisor, probe_runtime_run, run_supervisor_sidecar_from_env,
    stop_runtime_run, submit_runtime_run_input, RuntimeSupervisorController,
    RuntimeSupervisorLaunchRequest, RuntimeSupervisorProbeRequest, RuntimeSupervisorStopRequest,
    RuntimeSupervisorSubmitInputRequest,
};
