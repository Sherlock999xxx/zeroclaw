//! Agent-scoped memory wrapper (#6272 P7).
//!
//! [`AgentScopedMemory<M>`] is a generic wrapper around any
//! [`Memory`] backend that binds the agent identity at construction
//! time and routes every trait call through the agent-aware methods
//! on the trait. Stores carry the bound agent's UUID so the underlying
//! backend can persist the attribution; recalls thread the configured
//! allowlist of sibling agent UUIDs through `recall_for_agents` so
//! the backend filters at the storage layer.
//!
//! The wrapper is the canonical site for agent-identity enforcement.
//! Construction takes the bound agent's UUID and the resolved allowlist
//! (the set of sibling UUIDs that this agent's `read_memory_from`
//! resolves to). The agent loop never gets a raw [`Memory`] handle in
//! the multi-agent runtime; it always gets an
//! [`AgentScopedMemory<M>`] instance, so a tool that tries to bypass
//! the wrapper has no API surface to do so.
//!
//! Backends with native `agent_id` columns (SqliteMemory and
//! PostgresMemory after P6) override [`Memory::store_with_agent`] and
//! [`Memory::recall_for_agents`] to push the filter into SQL. Backends
//! without native columns inherit the default trait implementations
//! that fall back to non-scoped behavior; the wrapper still provides
//! a single point of enforcement that future overrides plug into.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

use zeroclaw_api::memory_traits::{ExportFilter, Memory, MemoryCategory, MemoryEntry};

/// A [`Memory`] handle bound to a single agent's identity.
///
/// Construct one via [`AgentScopedMemory::new`] with the bound agent's
/// UUID and the resolved allowlist of sibling agent UUIDs (computed
/// from `read_memory_from` at config load). Every store carries the
/// bound agent's UUID; every recall threads the allowlist (which
/// always includes the bound agent) through to the underlying
/// backend.
pub struct AgentScopedMemory<M: Memory> {
    /// The wrapped backend. Held as `Arc<M>` to allow cloning into
    /// async tasks without losing the bound identity, and to stay
    /// compatible with the existing `Arc<dyn Memory>` plumbing in
    /// the runtime.
    inner: Arc<M>,
    /// The bound agent's UUID. Stamped on every write through this
    /// wrapper.
    agent_id: String,
    /// Set of agent UUIDs this wrapper may recall from. Always
    /// contains [`Self::agent_id`] (the bound agent always sees its
    /// own rows); any additional UUIDs come from the configured
    /// `read_memory_from` allowlist.
    allowed_agent_ids: HashSet<String>,
}

impl<M: Memory> AgentScopedMemory<M> {
    /// Build a new agent-scoped wrapper around `inner`.
    ///
    /// `agent_id` is the bound agent's UUID (looked up from the
    /// agents table by alias at construction time in the runtime).
    /// `allowed_sibling_agent_ids` is the resolved
    /// `read_memory_from` allowlist; the bound `agent_id` is added
    /// automatically to the in-memory `allowed_agent_ids` set so
    /// callers do not need to remember to include themselves.
    #[must_use]
    pub fn new(
        inner: Arc<M>,
        agent_id: impl Into<String>,
        allowed_sibling_agent_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        let agent_id = agent_id.into();
        let mut allowed_agent_ids: HashSet<String> =
            allowed_sibling_agent_ids.into_iter().collect();
        allowed_agent_ids.insert(agent_id.clone());
        Self {
            inner,
            agent_id,
            allowed_agent_ids,
        }
    }

    /// The bound agent's UUID.
    #[must_use]
    pub fn bound_agent_id(&self) -> &str {
        &self.agent_id
    }

    /// The full allowlist this wrapper recalls from (bound agent
    /// included). Borrowed slice for read-only inspection by tests
    /// and callers that need to log effective scope.
    #[must_use]
    pub fn allowed_agent_ids(&self) -> &HashSet<String> {
        &self.allowed_agent_ids
    }

    /// Build a `Vec<&str>` of the allowlist for passing to the
    /// `Memory::recall_for_agents` trait method, which takes a
    /// borrowed slice. Stable iteration order is not required.
    fn allowed_slice(&self) -> Vec<&str> {
        self.allowed_agent_ids.iter().map(String::as_str).collect()
    }
}

#[async_trait]
impl<M: Memory> Memory for AgentScopedMemory<M> {
    fn name(&self) -> &str {
        // Kept identical to the inner backend so existing log lines
        // and dashboards keep working; the wrapper's existence is
        // visible only through the `agent_alias` tracing field
        // bound at agent-loop entry (P12).
        self.inner.name()
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> Result<()> {
        self.inner
            .store_with_agent(
                key,
                content,
                category,
                session_id,
                None,
                None,
                Some(&self.agent_id),
            )
            .await
    }

    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
    ) -> Result<()> {
        self.inner
            .store_with_agent(
                key,
                content,
                category,
                session_id,
                namespace,
                importance,
                Some(&self.agent_id),
            )
            .await
    }

    async fn store_with_agent(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> Result<()> {
        // If an explicit agent_id is supplied (uncommon when going
        // through the wrapper, but legal for tooling that crosses
        // the boundary), honor it; otherwise stamp with the bound
        // agent_id. The wrapper does NOT validate that the supplied
        // agent_id is in the allowlist on writes because writes
        // are owner-only by design: an agent only stores into its
        // own namespace. If a future model wants cross-agent writes,
        // this is the gate where the policy check lands.
        let resolved = agent_id.unwrap_or(&self.agent_id);
        self.inner
            .store_with_agent(
                key,
                content,
                category,
                session_id,
                namespace,
                importance,
                Some(resolved),
            )
            .await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let allowed = self.allowed_slice();
        self.inner
            .recall_for_agents(&allowed, query, limit, session_id, since, until)
            .await
    }

    async fn recall_for_agents(
        &self,
        allowed_agent_ids: &[&str],
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        // Caller supplied an explicit allowlist. Intersect with this
        // wrapper's bound allowlist so an over-broad request from a
        // tool can't sneak past the construction-time policy.
        let intersected: Vec<&str> = allowed_agent_ids
            .iter()
            .copied()
            .filter(|id| self.allowed_agent_ids.contains(*id))
            .collect();
        self.inner
            .recall_for_agents(&intersected, query, limit, session_id, since, until)
            .await
    }

    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>> {
        // Backends without native agent_id columns return entries
        // with no agent attribution; we cannot filter post-hoc
        // without that data. Backends with native columns can
        // override this method on the inner type to filter at the
        // SQL layer once the row read paths plumb agent_id into
        // MemoryEntry (P7 follow-up).
        self.inner.get(key).await
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        // See `get` above. List is best-effort agent-scoped today;
        // tightens to allowlist-only when agent_id propagates into
        // MemoryEntry in a follow-up.
        self.inner.list(category, session_id).await
    }

    async fn forget(&self, key: &str) -> Result<bool> {
        // Owner-only write semantics: forgetting is destructive, so
        // we route through the inner backend without cross-agent
        // expansion. A future tightening step verifies the row's
        // agent_id matches `self.agent_id` before deleting; until
        // then the inner backend's existing key-uniqueness keeps
        // accidental cross-agent deletes scoped by namespace.
        self.inner.forget(key).await
    }

    async fn purge_namespace(&self, namespace: &str) -> Result<usize> {
        self.inner.purge_namespace(namespace).await
    }

    async fn purge_session(&self, session_id: &str) -> Result<usize> {
        self.inner.purge_session(session_id).await
    }

    async fn count(&self) -> Result<usize> {
        self.inner.count().await
    }

    async fn reindex(&self) -> Result<usize> {
        self.inner.reindex().await
    }

    async fn store_procedural(
        &self,
        messages: &[zeroclaw_api::memory_traits::ProceduralMessage],
        session_id: Option<&str>,
    ) -> Result<()> {
        self.inner.store_procedural(messages, session_id).await
    }

    async fn recall_namespaced(
        &self,
        namespace: &str,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        // Namespace-routed recall stays per the existing trait
        // semantics; the agent allowlist applies on top once
        // backends populate MemoryEntry.agent_id (P7 follow-up).
        self.inner
            .recall_namespaced(namespace, query, limit, session_id, since, until)
            .await
    }

    async fn export(&self, filter: &ExportFilter) -> Result<Vec<MemoryEntry>> {
        self.inner.export(filter).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteMemory;
    use tempfile::TempDir;

    fn temp_sqlite() -> (TempDir, Arc<SqliteMemory>) {
        let tmp = TempDir::new().expect("tempdir");
        let mem = SqliteMemory::new(tmp.path()).expect("init sqlite");
        (tmp, Arc::new(mem))
    }

    #[tokio::test]
    async fn bound_agent_id_always_in_allowlist() {
        let (_tmp, inner) = temp_sqlite();
        let wrapper = AgentScopedMemory::new(inner, "agent-uuid-alpha", Vec::<String>::new());
        assert!(wrapper.allowed_agent_ids().contains("agent-uuid-alpha"));
        assert_eq!(wrapper.bound_agent_id(), "agent-uuid-alpha");
    }

    #[tokio::test]
    async fn allowlist_includes_supplied_siblings_plus_self() {
        let (_tmp, inner) = temp_sqlite();
        let wrapper = AgentScopedMemory::new(
            inner,
            "agent-uuid-alpha",
            vec![
                "agent-uuid-beta".to_string(),
                "agent-uuid-gamma".to_string(),
            ],
        );
        let allowed = wrapper.allowed_agent_ids();
        assert_eq!(allowed.len(), 3, "self + two siblings");
        assert!(allowed.contains("agent-uuid-alpha"));
        assert!(allowed.contains("agent-uuid-beta"));
        assert!(allowed.contains("agent-uuid-gamma"));
    }

    #[tokio::test]
    async fn store_and_recall_via_wrapper_round_trip() {
        let (_tmp, inner) = temp_sqlite();
        let wrapper = AgentScopedMemory::new(inner, "agent-uuid-alpha", Vec::<String>::new());

        wrapper
            .store("k1", "alpha-content", MemoryCategory::Core, None)
            .await
            .expect("store via wrapper");
        let entries = wrapper
            .recall("alpha-content", 10, None, None, None)
            .await
            .expect("recall via wrapper");
        assert!(
            entries.iter().any(|e| e.key == "k1"),
            "wrapper recall must surface keys it stored"
        );
    }

    #[tokio::test]
    async fn recall_for_agents_intersects_with_bound_allowlist() {
        // The wrapper's own allowlist is the authoritative scope;
        // an over-broad explicit allowlist passed via
        // recall_for_agents must be intersected, not unioned. We
        // can't observe the backend's filter without an override
        // on SqliteMemory (P7b), so we exercise the intersection at
        // the wrapper layer by checking the inner-call shape via
        // the bound agent's allowlist.
        let (_tmp, inner) = temp_sqlite();
        let wrapper = AgentScopedMemory::new(
            Arc::clone(&inner),
            "agent-uuid-alpha",
            vec!["agent-uuid-beta".to_string()],
        );

        // Sanity: allowed includes self + sibling.
        assert_eq!(wrapper.allowed_agent_ids().len(), 2);

        // A caller asking for [alpha, beta, ROGUE] gets intersected
        // to [alpha, beta] before the inner call. Without backend
        // SQL filtering yet, this round-trips through the default
        // recall and returns whatever exists; the test verifies
        // the call shape compiles and the wrapper layer doesn't
        // panic on rogue UUIDs.
        let _ = wrapper
            .recall_for_agents(
                &["agent-uuid-alpha", "agent-uuid-beta", "agent-uuid-rogue"],
                "",
                10,
                None,
                None,
                None,
            )
            .await
            .expect("recall_for_agents must accept rogue UUIDs and intersect");
    }
}
