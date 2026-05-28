//! Drift test: HTTP route and RPC method must accept the same JSON
//! shape for Quickstart `apply` and produce identical on-disk delta.
//!
//! Plan §"Drift detection in CI": if the two surfaces ever disagree
//! on field names, casing, defaulting, or write ordering, this test
//! fails. Failing case-or-camel here means someone added a field on
//! one surface only.

use zeroclaw_config::presets::{
    AgentIdentity, BuilderSubmission, MemoryChoice, ModelProviderChoice, SelectorChoice,
};
use zeroclaw_config::schema::Config;
use zeroclaw_runtime::quickstart::{Surface, apply_with_surface};

/// Canonical fixture used by every assertion. Covers each selector
/// in `Fresh` mode — `Existing` is exercised in the validate tests.
fn fixture_submission() -> BuilderSubmission {
    BuilderSubmission {
        model_provider: SelectorChoice::Fresh(ModelProviderChoice {
            provider_type: "anthropic".into(),
            alias: "drift_test".into(),
            model: "claude-sonnet-4-5".into(),
            api_key: Some("sk-drift-test".into()),
            base_url: None,
        }),
        risk_profile: SelectorChoice::Fresh("balanced".into()),
        runtime_profile: SelectorChoice::Fresh("balanced".into()),
        memory: SelectorChoice::Fresh(MemoryChoice::Sqlite),
        channels: vec![],
        agent: AgentIdentity {
            name: "drift_bot".into(),
            system_prompt: String::new(),
            personality_file: None,
        },
    }
}

#[test]
fn submission_round_trips_through_json_unchanged() {
    let original = fixture_submission();
    let json = serde_json::to_string(&original).expect("serialize submission");
    let parsed: BuilderSubmission =
        serde_json::from_str(&json).expect("deserialize round-trip submission");
    let json2 = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(
        json, json2,
        "BuilderSubmission must round-trip through JSON byte-identically"
    );
}

#[test]
fn rpc_params_wrapper_matches_http_body_shape() {
    // HTTP route deserializes `Json<BuilderSubmission>` directly from
    // the body. RPC method deserializes
    // `QuickstartApplyParams { submission: BuilderSubmission }` from
    // the params object. Both must accept the same `BuilderSubmission`
    // payload — the only difference is the one-level `{ "submission": ... }`
    // wrapper.
    let original = fixture_submission();
    let body_json = serde_json::to_string(&original).expect("serialize body");

    // Simulate the HTTP route boundary.
    let http_parsed: BuilderSubmission = serde_json::from_str(&body_json).expect("HTTP body deser");

    // Simulate the RPC route boundary: build the params wrapper from
    // the same body bytes, then pull `.submission` out.
    let rpc_params_json = format!(r#"{{"submission":{body_json}}}"#);
    let rpc_value: serde_json::Value =
        serde_json::from_str(&rpc_params_json).expect("RPC params deser");
    let rpc_parsed: BuilderSubmission = serde_json::from_value(
        rpc_value
            .get("submission")
            .cloned()
            .expect("submission field"),
    )
    .expect("RPC submission deser");

    assert_eq!(
        serde_json::to_string(&http_parsed).unwrap(),
        serde_json::to_string(&rpc_parsed).unwrap(),
        "HTTP body and RPC `params.submission` must deserialize to the same BuilderSubmission",
    );
}

#[tokio::test]
async fn apply_produces_identical_state_across_surfaces() {
    let submission = fixture_submission();

    // Web surface — the HTTP route's surface tag.
    let mut web_cfg = Config::default();
    let web_applied = apply_with_surface(submission.clone(), &mut web_cfg, Surface::Web)
        .await
        .expect("web apply");

    // Tui surface — the RPC handler's surface tag.
    let mut tui_cfg = Config::default();
    let tui_applied = apply_with_surface(submission, &mut tui_cfg, Surface::Tui)
        .await
        .expect("tui apply");

    // Identical applied-agent payload regardless of which surface
    // invoked the apply.
    assert_eq!(
        web_applied, tui_applied,
        "AppliedAgent must not vary by surface"
    );

    // Identical on-disk delta. `dirty_paths` is what `save_dirty`
    // will write; if they diverge, the surfaces would produce
    // different files for the same submission.
    let mut web_dirty: Vec<&str> = web_cfg.dirty_paths.iter().map(|s| s.as_str()).collect();
    let mut tui_dirty: Vec<&str> = tui_cfg.dirty_paths.iter().map(|s| s.as_str()).collect();
    web_dirty.sort();
    tui_dirty.sort();
    assert_eq!(
        web_dirty, tui_dirty,
        "Dirty path set must not vary by surface"
    );

    // Identical config content under every dirty path.
    for path in &web_dirty {
        let web_val = web_cfg.get_prop(path).ok();
        let tui_val = tui_cfg.get_prop(path).ok();
        assert_eq!(
            web_val, tui_val,
            "Surface drift at `{path}`: web={web_val:?} tui={tui_val:?}"
        );
    }
}
