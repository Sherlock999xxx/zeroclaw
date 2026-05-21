//! RPC session state.

use crate::agent::agent::Agent;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use zeroclaw_infra::session_queue::SessionActorQueue;

pub struct RpcSession {
    pub agent: Arc<Mutex<Agent>>,
    pub created_at: Instant,
    pub last_active: Instant,
    pub agent_alias: String,
    pub workspace_dir: String,
}

impl RpcSession {
    pub fn new(agent: Agent, alias: &str, workspace: &str) -> Self {
        Self {
            agent: Arc::new(Mutex::new(agent)),
            created_at: Instant::now(),
            last_active: Instant::now(),
            agent_alias: alias.to_string(),
            workspace_dir: workspace.to_string(),
        }
    }
}

pub struct SessionStore {
    sessions: Mutex<HashMap<String, RpcSession>>,
    cancel_tokens: std::sync::Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
    max_sessions: usize,
    pub session_queue: Arc<SessionActorQueue>,
}

impl SessionStore {
    pub fn new(max_sessions: usize, session_queue: Arc<SessionActorQueue>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            cancel_tokens: std::sync::Mutex::new(HashMap::new()),
            max_sessions,
            session_queue,
        }
    }

    pub async fn insert(&self, id: String, session: RpcSession) -> Result<(), &'static str> {
        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= self.max_sessions {
            return Err("session limit reached");
        }
        sessions.insert(id, session);
        Ok(())
    }

    pub async fn get_agent(&self, id: &str) -> Option<Arc<Mutex<Agent>>> {
        self.sessions.lock().await.get(id).map(|s| s.agent.clone())
    }

    pub async fn touch(&self, id: &str) {
        if let Some(s) = self.sessions.lock().await.get_mut(id) {
            s.last_active = Instant::now();
        }
    }

    pub async fn seed_history(&self, id: &str, msgs: &[zeroclaw_api::model_provider::ChatMessage]) {
        if let Some(s) = self.sessions.lock().await.get(id) {
            s.agent.lock().await.seed_history(msgs);
        }
    }

    pub async fn remove(&self, id: &str) -> bool {
        self.cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.sessions.lock().await.remove(id).is_some()
    }

    pub async fn list_ids(&self) -> Vec<String> {
        self.sessions.lock().await.keys().cloned().collect()
    }

    pub fn register_cancel_token(&self, id: &str, token: tokio_util::sync::CancellationToken) {
        self.cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), token);
    }

    pub fn remove_cancel_token(&self, id: &str) {
        self.cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }

    pub fn cancel_session(&self, id: &str) -> bool {
        self.cancel_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(|t| {
                t.cancel();
                true
            })
            .unwrap_or(false)
    }

    pub async fn count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}
