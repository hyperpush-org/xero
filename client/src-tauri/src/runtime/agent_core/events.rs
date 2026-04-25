use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Mutex, OnceLock,
    },
    time::Duration,
};

use crate::db::project_store::AgentEventRecord;

#[derive(Debug)]
pub struct AgentEventSubscription {
    key: String,
    id: u64,
    receiver: Receiver<AgentEventRecord>,
}

#[derive(Debug, Default)]
struct AgentEventBus {
    next_id: AtomicU64,
    subscribers: Mutex<HashMap<String, Vec<AgentEventSubscriber>>>,
}

#[derive(Debug)]
struct AgentEventSubscriber {
    id: u64,
    sender: Sender<AgentEventRecord>,
}

pub fn publish_agent_event(event: AgentEventRecord) {
    event_bus().publish(event);
}

pub fn subscribe_agent_events(project_id: &str, run_id: &str) -> AgentEventSubscription {
    event_bus().subscribe(project_id, run_id)
}

impl AgentEventSubscription {
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<AgentEventRecord, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

impl Drop for AgentEventSubscription {
    fn drop(&mut self) {
        event_bus().unsubscribe(&self.key, self.id);
    }
}

impl AgentEventBus {
    fn subscribe(&self, project_id: &str, run_id: &str) -> AgentEventSubscription {
        let key = event_key(project_id, run_id);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = mpsc::channel();

        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers
                .entry(key.clone())
                .or_default()
                .push(AgentEventSubscriber { id, sender });
        }

        AgentEventSubscription { key, id, receiver }
    }

    fn publish(&self, event: AgentEventRecord) {
        let key = event_key(&event.project_id, &event.run_id);
        let Ok(mut subscribers) = self.subscribers.lock() else {
            return;
        };
        let Some(run_subscribers) = subscribers.get_mut(&key) else {
            return;
        };

        run_subscribers.retain(|subscriber| subscriber.sender.send(event.clone()).is_ok());
        if run_subscribers.is_empty() {
            subscribers.remove(&key);
        }
    }

    fn unsubscribe(&self, key: &str, id: u64) {
        let Ok(mut subscribers) = self.subscribers.lock() else {
            return;
        };
        let Some(run_subscribers) = subscribers.get_mut(key) else {
            return;
        };
        run_subscribers.retain(|subscriber| subscriber.id != id);
        if run_subscribers.is_empty() {
            subscribers.remove(key);
        }
    }
}

fn event_bus() -> &'static AgentEventBus {
    static EVENT_BUS: OnceLock<AgentEventBus> = OnceLock::new();
    EVENT_BUS.get_or_init(AgentEventBus::default)
}

fn event_key(project_id: &str, run_id: &str) -> String {
    format!("{project_id}\u{1f}{run_id}")
}
