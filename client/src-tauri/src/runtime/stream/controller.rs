use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tauri::{ipc::Channel, AppHandle, Runtime};

use crate::{
    commands::{RuntimeStreamItemDto, RuntimeStreamItemKind},
    state::DesktopState,
};

#[derive(Debug, Clone, Default)]
pub struct RuntimeStreamController {
    inner: Arc<Mutex<RuntimeStreamRegistry>>,
}

#[derive(Debug, Default)]
struct RuntimeStreamRegistry {
    next_generation: u64,
    active: Option<ActiveRuntimeStream>,
}

#[derive(Debug, Clone)]
struct ActiveRuntimeStream {
    project_id: String,
    session_id: String,
    run_id: String,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeStreamLease {
    project_id: String,
    session_id: String,
    run_id: String,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct RuntimeStreamRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub runtime_kind: String,
    pub run_id: String,
    pub requested_item_kinds: Vec<RuntimeStreamItemKind>,
}

impl RuntimeStreamController {
    fn begin_stream(&self, project_id: &str, session_id: &str, run_id: &str) -> RuntimeStreamLease {
        let mut registry = self.inner.lock().expect("runtime stream registry poisoned");

        if let Some(active) = registry.active.take() {
            active.cancelled.store(true, Ordering::SeqCst);
        }

        registry.next_generation = registry.next_generation.saturating_add(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let generation = registry.next_generation;

        registry.active = Some(ActiveRuntimeStream {
            project_id: project_id.into(),
            session_id: session_id.into(),
            run_id: run_id.into(),
            generation,
            cancelled: cancelled.clone(),
        });

        RuntimeStreamLease {
            project_id: project_id.into(),
            session_id: session_id.into(),
            run_id: run_id.into(),
            generation,
            cancelled,
        }
    }

    fn finish_stream(&self, lease: &RuntimeStreamLease) {
        let mut registry = self.inner.lock().expect("runtime stream registry poisoned");
        let should_clear = registry.active.as_ref().is_some_and(|active| {
            active.project_id == lease.project_id
                && active.session_id == lease.session_id
                && active.run_id == lease.run_id
                && active.generation == lease.generation
        });

        if should_clear {
            registry.active = None;
        }
    }
}

impl RuntimeStreamLease {
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub fn start_runtime_stream<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: RuntimeStreamRequest,
    channel: Channel<RuntimeStreamItemDto>,
) {
    let lease = state.runtime_stream_controller().begin_stream(
        &request.project_id,
        &request.session_id,
        &request.run_id,
    );
    let controller = state.runtime_stream_controller().clone();

    std::thread::spawn(move || {
        let outcome = super::emit_runtime_stream(&app, &state, &request, &lease, &channel);

        if let Err(super::StreamExit::Failed(failure)) = outcome {
            let _ = super::emit_failure_item(
                &channel,
                &request,
                failure.last_sequence.saturating_add(1),
                failure.error,
            );
        }

        controller.finish_stream(&lease);
    });
}
