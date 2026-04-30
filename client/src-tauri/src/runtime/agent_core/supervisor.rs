use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::commands::{CommandError, CommandResult};

pub const AGENT_RUN_CANCELLED_CODE: &str = "agent_run_cancelled";

#[derive(Debug, Clone, Default)]
pub struct AgentRunSupervisor {
    inner: Arc<Mutex<AgentRunSupervisorState>>,
}

#[derive(Debug, Default)]
struct AgentRunSupervisorState {
    active: HashMap<String, ActiveAgentRun>,
}

#[derive(Debug, Clone)]
struct ActiveAgentRun {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    token: AgentRunCancellationToken,
}

#[derive(Debug, Clone)]
pub struct AgentRunLease {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    token: AgentRunCancellationToken,
    supervisor: AgentRunSupervisor,
}

#[derive(Debug, Clone)]
pub struct AgentRunCancellationToken {
    cancelled: Arc<AtomicBool>,
    linked_parents: Arc<Vec<Arc<AtomicBool>>>,
}

impl Default for AgentRunCancellationToken {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            linked_parents: Arc::new(Vec::new()),
        }
    }
}

impl AgentRunSupervisor {
    pub fn begin(
        &self,
        project_id: &str,
        agent_session_id: &str,
        run_id: &str,
    ) -> CommandResult<AgentRunLease> {
        let mut state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;

        if let Some(active) = state.active.get(run_id) {
            return Err(CommandError::user_fixable(
                "agent_run_already_active",
                format!(
                    "Xero is already driving owned-agent run `{}` for project `{}` session `{}`.",
                    active.run_id, active.project_id, active.agent_session_id
                ),
            ));
        }

        let token = AgentRunCancellationToken::default();
        state.active.insert(
            run_id.into(),
            ActiveAgentRun {
                project_id: project_id.into(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                token: token.clone(),
            },
        );

        Ok(AgentRunLease {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            token,
            supervisor: self.clone(),
        })
    }

    pub fn cancel(&self, run_id: &str) -> CommandResult<bool> {
        let state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;
        let Some(active) = state.active.get(run_id) else {
            return Ok(false);
        };
        active.token.cancel();
        Ok(true)
    }

    pub fn is_active(&self, run_id: &str) -> CommandResult<bool> {
        let state = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_run_supervisor_lock_failed",
                "Xero could not lock the owned-agent run supervisor.",
            )
        })?;
        Ok(state.active.contains_key(run_id))
    }

    fn finish(&self, lease: &AgentRunLease) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        let should_remove = state.active.get(&lease.run_id).is_some_and(|active| {
            active.project_id == lease.project_id
                && active.agent_session_id == lease.agent_session_id
                && active.run_id == lease.run_id
                && active.token.same_token(&lease.token)
        });
        if should_remove {
            state.active.remove(&lease.run_id);
        }
    }
}

impl AgentRunLease {
    pub fn token(&self) -> AgentRunCancellationToken {
        self.token.clone()
    }
}

impl Drop for AgentRunLease {
    fn drop(&mut self) {
        self.supervisor.finish(self);
    }
}

impl AgentRunCancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
            || self
                .linked_parents
                .iter()
                .any(|parent| parent.load(Ordering::SeqCst))
    }

    pub fn check_cancelled(&self) -> CommandResult<()> {
        if self.is_cancelled() {
            return Err(cancelled_error());
        }
        Ok(())
    }

    fn same_token(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }

    pub fn linked_child(&self) -> Self {
        let mut linked_parents = self.linked_parents.as_ref().clone();
        linked_parents.push(self.cancelled.clone());
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            linked_parents: Arc::new(linked_parents),
        }
    }
}

pub fn cancelled_error() -> CommandError {
    CommandError::retryable(
        AGENT_RUN_CANCELLED_CODE,
        "Owned agent run was cancelled before it could finish.",
    )
}
