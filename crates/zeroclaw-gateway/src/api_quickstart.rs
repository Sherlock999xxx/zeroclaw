//! HTTP routes for the Quickstart flow.
//!
//! Thin wrapper over `zeroclaw_runtime::quickstart::{validate_only, apply}`.
//! Routes:
//!
//! - `GET  /api/quickstart/state`     — current Quickstart state (completed flag + live-config slices for each step's "Use existing" section).
//! - `POST /api/quickstart/validate`  — run `validate_only` against the submitted `BuilderSubmission`; returns `{ ok: true }` or `{ ok: false, errors: [...] }`.
//! - `POST /api/quickstart/apply`     — atomically apply the submission, then signal an in-place daemon reload through the existing `reload_tx` watch channel (same mechanism `/admin/reload` uses); returns the `AppliedAgent` summary or a structured error list.
//!
//! All business logic lives in `zeroclaw-runtime`; this module is route
//! plumbing only.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use zeroclaw_config::presets::BuilderSubmission;
use zeroclaw_runtime::quickstart::{
    AppliedAgent, QuickstartError, QuickstartStep, Surface, apply_with_surface, record_dismissed,
    validate_only_with_surface,
};

use super::AppState;
use super::api::require_auth;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidateResult {
    Ok,
    Errors { errors: Vec<QuickstartError> },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApplyResult {
    Applied {
        agent: AppliedAgent,
        /// `true` when the in-place daemon reload was signalled (the
        /// supervisor will drain and re-init subsystems). `false` means
        /// apply succeeded but no daemon supervisor is attached (e.g.
        /// `zeroclaw gateway start` standalone) — the caller must
        /// restart the process to pick up the change.
        daemon_restarted: bool,
    },
    Errors {
        errors: Vec<QuickstartError>,
    },
}

/// `GET /api/quickstart/state` — minimal payload the Quickstart UI
/// needs to render every step's "Use existing" section without
/// pulling the entire config.
#[derive(Debug, Serialize)]
pub struct QuickstartState {
    pub quickstart_completed: bool,
    pub agents: Vec<String>,
    pub risk_profiles: Vec<String>,
    pub runtime_profiles: Vec<String>,
    /// `<provider_type>.<alias>` refs for every configured model provider.
    pub model_providers: Vec<String>,
    /// `<channel_type>.<alias>` refs for every configured per-alias channel.
    pub channels: Vec<String>,
    /// `<storage_type>.<alias>` refs for every configured storage backend.
    pub storage: Vec<String>,
}

pub async fn handle_state(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let cfg = state.config.read().clone();
    let body = QuickstartState {
        quickstart_completed: cfg.onboard_state.quickstart_completed,
        agents: cfg.agents.keys().cloned().collect(),
        risk_profiles: cfg.risk_profiles.keys().cloned().collect(),
        runtime_profiles: cfg.runtime_profiles.keys().cloned().collect(),
        model_providers: cfg
            .providers
            .models
            .iter_entries()
            .map(|(family, alias, _)| format!("{family}.{alias}"))
            .collect(),
        channels: collect_channel_refs(&cfg.channels),
        storage: collect_storage_refs(&cfg.storage),
    };
    (StatusCode::OK, Json(body)).into_response()
}

pub async fn handle_validate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(submission): Json<BuilderSubmission>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let cfg = state.config.read().clone();
    let body = match validate_only_with_surface(&submission, &cfg, Surface::Web) {
        Ok(()) => ValidateResult::Ok,
        Err(errors) => ValidateResult::Errors { errors },
    };
    (StatusCode::OK, Json(body)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct DismissRequest {
    pub run_id: String,
    /// Surface name as emitted in earlier events for this run. Echoed
    /// into the dismiss event so the SSE stream can correlate the
    /// dismissal back to the same `(run_id, surface)` pair.
    pub surface: String,
    /// Furthest step the user reached. `None` = didn't progress past
    /// the first selector.
    #[serde(default)]
    pub last_step: Option<QuickstartStep>,
}

pub async fn handle_dismiss(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DismissRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let surface = match req.surface.as_str() {
        "web" => Surface::Web,
        "tui" => Surface::Tui,
        "cli" => Surface::Cli,
        _ => Surface::Web,
    };
    record_dismissed(&req.run_id, surface, req.last_step);
    (StatusCode::NO_CONTENT, ()).into_response()
}

pub async fn handle_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(submission): Json<BuilderSubmission>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let mut working = state.config.read().clone();
    let result = apply_with_surface(submission, &mut working, Surface::Web).await;
    let body = match result {
        Ok(agent) => {
            *state.config.write() = working;
            state
                .pending_reload
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let reload_signalled = signal_daemon_reload(&state);
            ApplyResult::Applied {
                agent,
                daemon_restarted: reload_signalled,
            }
        }
        Err(errors) => ApplyResult::Errors { errors },
    };
    (StatusCode::OK, Json(body)).into_response()
}

/// Signal the in-place daemon reload using the same `reload_tx` watch
/// channel `/admin/reload` uses. The daemon supervisor reacts by
/// draining the current gateway/channels/scheduler and bringing them
/// back up against the new in-memory config — no process kill, no
/// PID respawn, no service-manager dependency.
fn signal_daemon_reload(state: &AppState) -> bool {
    let Some(reload_tx) = state.reload_tx.clone() else {
        ::zeroclaw_log::record!(
            WARN,
            ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Note)
                .with_outcome(::zeroclaw_log::EventOutcome::Unknown)
                .with_attrs(::serde_json::json!({
                    "reason": "no_supervisor",
                })),
            "quickstart: daemon reload not available (standalone gateway)"
        );
        return false;
    };
    ::zeroclaw_log::record!(
        INFO,
        ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Start),
        "quickstart: daemon reload signalled"
    );
    let shutdown_tx = state.shutdown_tx.clone();
    state
        .pending_reload
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let started = std::time::Instant::now();
    zeroclaw_spawn::spawn!(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = shutdown_tx.send(true);
        let _ = reload_tx.send(true);
        ::zeroclaw_log::record!(
            INFO,
            ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Complete)
                .with_outcome(::zeroclaw_log::EventOutcome::Success)
                .with_attrs(::serde_json::json!({
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                })),
            "quickstart: daemon reload dispatched"
        );
    });
    true
}

// ── Helpers: collect every aliased entry from each typed-family slot ──
//
// `Configurable::map_key_sections()` reports every keyed section in the
// schema tree. We list aliased entries by reading the per-section
// `HashMap` directly via `serde_json` introspection, so adding a
// channel/storage slot to the schema shows up here for free.

fn collect_channel_refs(channels: &zeroclaw_config::schema::ChannelsConfig) -> Vec<String> {
    collect_aliased_refs(channels, "channels")
}

fn collect_storage_refs(storage: &zeroclaw_config::schema::StorageConfig) -> Vec<String> {
    collect_aliased_refs(storage, "storage")
}

/// Walk the serialized form of `value` and yield `<type>.<alias>` refs
/// for every `HashMap<String, _>`-shaped subsection. Schema-driven —
/// adding a slot needs no code change here.
fn collect_aliased_refs<T: serde::Serialize>(value: &T, _root_prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(serde_json::Value::Object(map)) = serde_json::to_value(value) else {
        return out;
    };
    for (family, subvalue) in map {
        if let serde_json::Value::Object(entries) = subvalue {
            for alias in entries.keys() {
                out.push(format!("{family}.{alias}"));
            }
        }
    }
    out.sort();
    out
}
