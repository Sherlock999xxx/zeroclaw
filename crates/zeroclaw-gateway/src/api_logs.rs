//! `GET /api/logs` — paginated query over the persisted JSONL log.
//!
//! Thin HTTP adapter over [`zeroclaw_log::load_page`]. Filter parameters
//! map 1:1 onto `LogFilter` fields. Pagination is cursor-based:
//! responses include `next_cursor: (timestamp, id)` which callers pass
//! back as `until_ts` / `until_id` to fetch older events.

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use zeroclaw_log::{LogFilter, LogPage};

use super::AppState;
use super::api::require_auth;

#[derive(Debug, Default, Deserialize)]
pub struct LogsQuery {
    /// RFC 3339 lower bound (inclusive).
    #[serde(default)]
    pub since_ts: Option<String>,
    /// RFC 3339 upper bound (exclusive — used by pagination cursor).
    #[serde(default)]
    pub until_ts: Option<String>,
    /// Event id at the cursor when timestamps tie.
    #[serde(default)]
    pub until_id: Option<String>,
    /// Match exact `event.action`.
    #[serde(default)]
    pub action: Option<String>,
    /// Match exact `event.category`.
    #[serde(default)]
    pub category: Option<String>,
    /// Match exact `event.outcome`.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Minimum OTel severity_number (e.g. `13` = WARN+).
    #[serde(default)]
    pub severity_min: Option<u8>,
    /// Match `zeroclaw.agent_alias`.
    #[serde(default)]
    pub agent: Option<String>,
    /// Match alias-bound `<type>.<alias>` composite.
    #[serde(default)]
    pub channel: Option<String>,
    /// Match `zeroclaw.channel_type` only (no alias filter).
    #[serde(default)]
    pub channel_type: Option<String>,
    /// Match `zeroclaw.tool`.
    #[serde(default)]
    pub tool: Option<String>,
    /// Match `trace_id`.
    #[serde(default)]
    pub trace_id: Option<String>,
    /// Substring search across `message` + `attributes`.
    #[serde(default)]
    pub q: Option<String>,
    /// Hide `event.category = "internal"` events. Default `false`.
    #[serde(default)]
    pub hide_internal: bool,
    /// Page size. Default 200, capped at 10_000 by the reader.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub events: Vec<serde_json::Value>,
    /// `Some((timestamp, id))` when more older events may exist.
    pub next_cursor: Option<(String, String)>,
    /// True when the file was fully scanned for this filter.
    pub at_end: bool,
    /// Daemon start time so callers can implement "since daemon start"
    /// without an extra `/api/status` round-trip.
    pub daemon_started_at: String,
}

/// `GET /api/logs?since_ts=&until_ts=&until_id=&action=&category=&outcome=&severity_min=&agent=&channel=&channel_type=&tool=&trace_id=&q=&hide_internal=&limit=`
pub async fn handle_api_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LogsQuery>,
) -> Response {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(path) = zeroclaw_log::current_log_path() else {
        return Json(LogsResponse {
            events: Vec::new(),
            next_cursor: None,
            at_end: true,
            daemon_started_at: zeroclaw_runtime::health::daemon_started_at(),
        })
        .into_response();
    };

    let filter = LogFilter {
        since_ts: q.since_ts,
        until_ts: q.until_ts,
        until_id: q.until_id,
        action: q.action,
        category: q.category,
        outcome: q.outcome,
        severity_min: q.severity_min,
        agent: q.agent,
        channel: q.channel,
        channel_type: q.channel_type,
        tool: q.tool,
        trace_id: q.trace_id,
        q: q.q,
        hide_internal: q.hide_internal,
    };
    let limit = q.limit.unwrap_or(200);

    let LogPage {
        events,
        next_cursor,
        at_end,
    } = match zeroclaw_log::load_page(&path, &filter, limit) {
        Ok(page) => page,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("log read failed: {err:#}"),
                })),
            )
                .into_response();
        }
    };

    let events_json: Vec<serde_json::Value> = events
        .into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();

    Json(LogsResponse {
        events: events_json,
        next_cursor,
        at_end,
        daemon_started_at: zeroclaw_runtime::health::daemon_started_at(),
    })
    .into_response()
}
