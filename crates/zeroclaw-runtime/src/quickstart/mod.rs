//! Quickstart apply path.
//!
//! Single entry point both surfaces (web gateway, zerocode RPC, CLI)
//! call to land a [`BuilderSubmission`] into the live [`Config`]. The
//! runtime never enumerates channel types, provider types, or storage
//! backends itself — every write goes through `Config::set_prop_persistent`,
//! which dispatches through the schema-derived `Configurable` tree.
//! Adding a new channel / provider / storage backend to the schema
//! lights up in the Quickstart for free.

use serde::{Deserialize, Serialize};

use zeroclaw_config::presets::{
    AgentIdentity, BuilderSubmission, ChannelQuickStart, MemoryChoice, ModelProviderChoice,
    SelectorChoice, risk_preset, runtime_preset,
};
use zeroclaw_config::schema::Config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedAgent {
    pub alias: String,
    pub model_provider: String,
    pub risk_profile: String,
    pub runtime_profile: String,
    pub channels: Vec<String>,
    pub memory_backend: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuickstartStep {
    ModelProvider,
    RiskProfile,
    RuntimeProfile,
    Memory,
    Channels,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuickstartError {
    pub step: QuickstartStep,
    pub field: String,
    pub message: String,
}

impl QuickstartError {
    fn new(step: QuickstartStep, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            step,
            field: field.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for QuickstartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.field.is_empty() {
            write!(f, "{:?}: {}", self.step, self.message)
        } else {
            write!(f, "{:?}.{}: {}", self.step, self.field, self.message)
        }
    }
}

pub fn validate_only(
    submission: &BuilderSubmission,
    config: &Config,
) -> Result<(), Vec<QuickstartError>> {
    let mut staged = config.clone();
    let mut errors = Vec::new();
    apply_into(&mut staged, submission, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub async fn apply(
    submission: BuilderSubmission,
    config: &mut Config,
) -> Result<AppliedAgent, Vec<QuickstartError>> {
    ::zeroclaw_log::record!(
        INFO,
        ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Start),
        "quickstart: apply"
    );
    let mut errors = Vec::new();
    let applied = apply_into(config, &submission, &mut errors);
    if !errors.is_empty() {
        ::zeroclaw_log::record!(
            WARN,
            ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Fail)
                .with_outcome(::zeroclaw_log::EventOutcome::Failure)
                .with_attrs(::serde_json::json!({"error_count": errors.len()})),
            "quickstart: apply rejected"
        );
        return Err(errors);
    }
    let applied = applied.expect("apply_into yields Some when errors is empty");

    config
        .set_prop_persistent("onboard-state.quickstart-completed", "true")
        .map_err(|err| {
            vec![QuickstartError::new(
                QuickstartStep::Agent,
                "",
                format!("failed to flip quickstart-completed: {err}"),
            )]
        })?;

    config.save_dirty().await.map_err(|err| {
        vec![QuickstartError::new(
            QuickstartStep::Agent,
            "",
            format!("failed to persist config: {err}"),
        )]
    })?;

    ::zeroclaw_log::record!(
        INFO,
        ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Complete)
            .with_outcome(::zeroclaw_log::EventOutcome::Success)
            .with_attrs(::serde_json::json!({
                "agent": applied.alias,
                "channels": applied.channels.len(),
            })),
        "quickstart: apply complete"
    );
    Ok(applied)
}

/// Implicit-completion rule: the Quickstart auto-launches only when
/// `onboard_state.quickstart_completed` is false **and** no
/// `agents.*` entries exist. Returning users with existing agents
/// never see the auto-trigger even if the flag was never flipped.
pub fn should_auto_launch(config: &Config) -> bool {
    !config.onboard_state.quickstart_completed && config.agents.is_empty()
}

fn apply_into(
    config: &mut Config,
    submission: &BuilderSubmission,
    errors: &mut Vec<QuickstartError>,
) -> Option<AppliedAgent> {
    let provider_ref = apply_model_provider(config, &submission.model_provider, errors)?;
    let risk_alias = apply_named_preset(
        config,
        &submission.risk_profile,
        QuickstartStep::RiskProfile,
        risk_preset_keys,
        write_risk_preset,
        errors,
    )?;
    let runtime_alias = apply_named_preset(
        config,
        &submission.runtime_profile,
        QuickstartStep::RuntimeProfile,
        runtime_preset_keys,
        write_runtime_preset,
        errors,
    )?;
    let memory_backend = apply_memory(config, &submission.memory, errors)?;
    let channel_refs = apply_channels(config, &submission.channels, errors);
    if !errors.is_empty() {
        return None;
    }
    let alias = apply_agent(
        config,
        &submission.agent,
        &provider_ref,
        &risk_alias,
        &runtime_alias,
        &channel_refs,
        errors,
    )?;
    Some(AppliedAgent {
        alias,
        model_provider: provider_ref,
        risk_profile: risk_alias,
        runtime_profile: runtime_alias,
        channels: channel_refs,
        memory_backend,
    })
}

// ── Model provider ─────────────────────────────────────────────────

fn apply_model_provider(
    config: &mut Config,
    choice: &SelectorChoice<ModelProviderChoice>,
    errors: &mut Vec<QuickstartError>,
) -> Option<String> {
    match choice {
        SelectorChoice::Existing(reference) => {
            let (family, alias) = match split_ref(reference) {
                Some(parts) => parts,
                None => {
                    errors.push(QuickstartError::new(
                        QuickstartStep::ModelProvider,
                        "",
                        format!("`{reference}` is not a `<type>.<alias>` reference"),
                    ));
                    return None;
                }
            };
            if !section_has_alias(config, "providers.models", family, alias) {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "",
                    format!("no `providers.models.{family}.{alias}` configured"),
                ));
                return None;
            }
            Some(reference.clone())
        }
        SelectorChoice::Fresh(choice) => {
            if choice.provider_type.trim().is_empty()
                || choice.alias.trim().is_empty()
                || choice.default_model.trim().is_empty()
            {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "",
                    "provider type, alias, and default model are required",
                ));
                return None;
            }
            if section_has_alias(
                config,
                "providers.models",
                &choice.provider_type,
                &choice.alias,
            ) {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "alias",
                    format!(
                        "alias `{}.{}` already exists",
                        choice.provider_type, choice.alias
                    ),
                ));
                return None;
            }
            let prefix = format!("providers.models.{}.{}", choice.provider_type, choice.alias);
            if let Err(err) = config.create_map_key(
                &format!("providers.models.{}", choice.provider_type),
                &choice.alias,
            ) {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "provider_type",
                    err.to_string(),
                ));
                return None;
            }
            if let Err(err) =
                config.set_prop_persistent(&format!("{prefix}.model"), &choice.default_model)
            {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "default_model",
                    err.to_string(),
                ));
                return None;
            }
            if let Some(key) = &choice.api_key
                && let Err(err) = config.set_prop_persistent(&format!("{prefix}.api-key"), key)
            {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "api_key",
                    err.to_string(),
                ));
                return None;
            }
            if let Some(uri) = &choice.base_url
                && let Err(err) = config.set_prop_persistent(&format!("{prefix}.uri"), uri)
            {
                errors.push(QuickstartError::new(
                    QuickstartStep::ModelProvider,
                    "base_url",
                    err.to_string(),
                ));
                return None;
            }
            Some(format!("{}.{}", choice.provider_type, choice.alias))
        }
    }
}

// ── Risk / Runtime presets ─────────────────────────────────────────

fn apply_named_preset<K, W>(
    config: &mut Config,
    choice: &SelectorChoice<String>,
    step: QuickstartStep,
    list_existing: K,
    write_preset: W,
    errors: &mut Vec<QuickstartError>,
) -> Option<String>
where
    K: Fn(&Config) -> Vec<String>,
    W: Fn(&mut Config, &str) -> Result<String, String>,
{
    match choice {
        SelectorChoice::Existing(alias) => {
            if list_existing(config).iter().any(|a| a == alias) {
                Some(alias.clone())
            } else {
                errors.push(QuickstartError::new(
                    step,
                    "",
                    format!("no `{alias}` profile configured"),
                ));
                None
            }
        }
        SelectorChoice::Fresh(preset_name) => match write_preset(config, preset_name) {
            Ok(alias) => Some(alias),
            Err(msg) => {
                errors.push(QuickstartError::new(step, "", msg));
                None
            }
        },
    }
}

fn risk_preset_keys(config: &Config) -> Vec<String> {
    config.risk_profiles.keys().cloned().collect()
}

fn runtime_preset_keys(config: &Config) -> Vec<String> {
    config.runtime_profiles.keys().cloned().collect()
}

fn write_risk_preset(config: &mut Config, preset_name: &str) -> Result<String, String> {
    let preset =
        risk_preset(preset_name).ok_or_else(|| format!("unknown risk preset `{preset_name}`"))?;
    config
        .create_map_key("risk-profiles", preset.preset_name)
        .map_err(|e| e.to_string())?;
    config
        .risk_profiles
        .insert(preset.preset_name.to_string(), (preset.values)());
    Ok(preset.preset_name.to_string())
}

fn write_runtime_preset(config: &mut Config, preset_name: &str) -> Result<String, String> {
    let preset = runtime_preset(preset_name)
        .ok_or_else(|| format!("unknown runtime preset `{preset_name}`"))?;
    config
        .create_map_key("runtime-profiles", preset.preset_name)
        .map_err(|e| e.to_string())?;
    config
        .runtime_profiles
        .insert(preset.preset_name.to_string(), (preset.values)());
    Ok(preset.preset_name.to_string())
}

// ── Memory ─────────────────────────────────────────────────────────

fn apply_memory(
    config: &mut Config,
    choice: &SelectorChoice<MemoryChoice>,
    errors: &mut Vec<QuickstartError>,
) -> Option<String> {
    match choice {
        SelectorChoice::Existing(reference) => {
            let (family, alias) = match split_ref(reference) {
                Some(parts) => parts,
                None => {
                    errors.push(QuickstartError::new(
                        QuickstartStep::Memory,
                        "",
                        format!("`{reference}` is not a `<type>.<alias>` reference"),
                    ));
                    return None;
                }
            };
            if !section_has_alias(config, "storage", family, alias) {
                errors.push(QuickstartError::new(
                    QuickstartStep::Memory,
                    "",
                    format!("no `storage.{family}.{alias}` configured"),
                ));
                return None;
            }
            if let Err(err) = config.set_prop_persistent("memory.backend", reference) {
                errors.push(QuickstartError::new(
                    QuickstartStep::Memory,
                    "backend",
                    err.to_string(),
                ));
                return None;
            }
            Some(reference.clone())
        }
        SelectorChoice::Fresh(MemoryChoice::Sqlite) => {
            let backend_ref = "sqlite.sqlite".to_string();
            if let Err(err) = config.create_map_key("storage.sqlite", "sqlite") {
                errors.push(QuickstartError::new(
                    QuickstartStep::Memory,
                    "",
                    err.to_string(),
                ));
                return None;
            }
            if let Err(err) = config.set_prop_persistent("memory.backend", &backend_ref) {
                errors.push(QuickstartError::new(
                    QuickstartStep::Memory,
                    "backend",
                    err.to_string(),
                ));
                return None;
            }
            Some(backend_ref)
        }
        SelectorChoice::Fresh(MemoryChoice::None) => {
            if let Err(err) = config.set_prop_persistent("memory.backend", "none") {
                errors.push(QuickstartError::new(
                    QuickstartStep::Memory,
                    "backend",
                    err.to_string(),
                ));
                return None;
            }
            Some("none".to_string())
        }
    }
}

// ── Channels ───────────────────────────────────────────────────────

fn apply_channels(
    config: &mut Config,
    channels: &[SelectorChoice<ChannelQuickStart>],
    errors: &mut Vec<QuickstartError>,
) -> Vec<String> {
    let mut refs = Vec::with_capacity(channels.len());
    for (idx, ch) in channels.iter().enumerate() {
        match ch {
            SelectorChoice::Existing(reference) => {
                if let Some((family, alias)) = split_ref(reference) {
                    if !channel_exists(config, family, alias) {
                        errors.push(QuickstartError::new(
                            QuickstartStep::Channels,
                            format!("channels[{idx}]"),
                            format!("no `channels.{family}.{alias}` configured"),
                        ));
                        continue;
                    }
                    refs.push(reference.clone());
                } else {
                    errors.push(QuickstartError::new(
                        QuickstartStep::Channels,
                        format!("channels[{idx}]"),
                        format!("`{reference}` is not a `<type>.<alias>` reference"),
                    ));
                }
            }
            SelectorChoice::Fresh(entry) => {
                if entry.channel_type.trim().is_empty() || entry.alias.trim().is_empty() {
                    errors.push(QuickstartError::new(
                        QuickstartStep::Channels,
                        format!("channels[{idx}]"),
                        "channel type and alias are required",
                    ));
                    continue;
                }
                if channel_exists(config, &entry.channel_type, &entry.alias) {
                    errors.push(QuickstartError::new(
                        QuickstartStep::Channels,
                        format!("channels[{idx}].alias"),
                        format!(
                            "alias `{}.{}` already exists",
                            entry.channel_type, entry.alias
                        ),
                    ));
                    continue;
                }
                if let Err(err) =
                    config.create_map_key(&format!("channels.{}", entry.channel_type), &entry.alias)
                {
                    errors.push(QuickstartError::new(
                        QuickstartStep::Channels,
                        format!("channels[{idx}].channel_type"),
                        err.to_string(),
                    ));
                    continue;
                }
                let token_path =
                    format!("channels.{}.{}.bot-token", entry.channel_type, entry.alias);
                if let Some(tok) = &entry.token {
                    if let Err(err) = config.set_prop_persistent(&token_path, tok) {
                        errors.push(QuickstartError::new(
                            QuickstartStep::Channels,
                            format!("channels[{idx}].token"),
                            err.to_string(),
                        ));
                        continue;
                    }
                } else {
                    // No creds — still need to materialize the entry so the agent
                    // record can reference it. Set `enabled = true` as the minimum
                    // schema-recognised field; channels without creds will fail
                    // their own bootstrap loudly, which is the desired behaviour.
                    let enabled_path =
                        format!("channels.{}.{}.enabled", entry.channel_type, entry.alias);
                    if let Err(err) = config.set_prop_persistent(&enabled_path, "true") {
                        errors.push(QuickstartError::new(
                            QuickstartStep::Channels,
                            format!("channels[{idx}]"),
                            err.to_string(),
                        ));
                        continue;
                    }
                }
                refs.push(format!("{}.{}", entry.channel_type, entry.alias));
            }
        }
    }
    refs
}

fn channel_exists(config: &Config, channel_type: &str, alias: &str) -> bool {
    let probe = format!("channels.{channel_type}.{alias}.enabled");
    config.get_prop(&probe).is_ok()
}

// ── Agent ──────────────────────────────────────────────────────────

fn apply_agent(
    config: &mut Config,
    identity: &AgentIdentity,
    provider_ref: &str,
    risk_alias: &str,
    runtime_alias: &str,
    channel_refs: &[String],
    errors: &mut Vec<QuickstartError>,
) -> Option<String> {
    if identity.name.trim().is_empty() {
        errors.push(QuickstartError::new(
            QuickstartStep::Agent,
            "name",
            "agent name is required",
        ));
        return None;
    }
    if config.agents.contains_key(&identity.name) {
        errors.push(QuickstartError::new(
            QuickstartStep::Agent,
            "name",
            format!("agent `{}` already exists", identity.name),
        ));
        return None;
    }

    let prefix = format!("agents.{}", identity.name);
    if let Err(err) = config.create_map_key("agents", &identity.name) {
        errors.push(QuickstartError::new(
            QuickstartStep::Agent,
            "name",
            err.to_string(),
        ));
        return None;
    }
    let writes: [(&str, &str); 3] = [
        ("model-provider", provider_ref),
        ("risk-profile", risk_alias),
        ("runtime-profile", runtime_alias),
    ];
    for (field, value) in writes {
        let path = format!("{prefix}.{field}");
        if let Err(err) = config.set_prop_persistent(&path, value) {
            errors.push(QuickstartError::new(
                QuickstartStep::Agent,
                field,
                err.to_string(),
            ));
            return None;
        }
    }
    for r in channel_refs {
        let path = format!("{prefix}.channels");
        if let Err(err) = config.set_prop_persistent(&path, r) {
            errors.push(QuickstartError::new(
                QuickstartStep::Agent,
                "channels",
                err.to_string(),
            ));
            return None;
        }
    }
    Some(identity.name.clone())
}

// ── Shared helpers ─────────────────────────────────────────────────

fn split_ref(reference: &str) -> Option<(&str, &str)> {
    let (ty, alias) = reference.split_once('.')?;
    if ty.is_empty() || alias.is_empty() {
        None
    } else {
        Some((ty, alias))
    }
}

/// Probe whether `<prefix>.<family>.<alias>` resolves to a populated
/// entry. Uses the schema's own `get_prop` dispatch — no per-family
/// list. We probe a path the entry's own struct must have if it
/// exists (`enabled` or `model`); the schema bubbles an error for
/// unknown families which we treat as "not present".
fn section_has_alias(config: &Config, prefix: &str, family: &str, alias: &str) -> bool {
    for probe_field in ["enabled", "model", "uri"] {
        let probe = format!("{prefix}.{family}.{alias}.{probe_field}");
        if config.get_prop(&probe).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_config::presets::{
        AgentIdentity, BuilderSubmission, MemoryChoice, ModelProviderChoice, SelectorChoice,
    };

    fn fresh_submission(agent_name: &str) -> BuilderSubmission {
        BuilderSubmission {
            model_provider: SelectorChoice::Fresh(ModelProviderChoice {
                provider_type: "anthropic".into(),
                alias: "anthropic".into(),
                default_model: "claude-sonnet-4-5".into(),
                api_key: Some("sk-test".into()),
                base_url: None,
            }),
            risk_profile: SelectorChoice::Fresh("balanced".into()),
            runtime_profile: SelectorChoice::Fresh("balanced".into()),
            memory: SelectorChoice::Fresh(MemoryChoice::Sqlite),
            channels: vec![],
            agent: AgentIdentity {
                name: agent_name.into(),
                system_prompt: "You are helpful.".into(),
                personality_file: None,
            },
        }
    }

    #[test]
    fn validate_only_passes_on_fresh_submission() {
        let cfg = Config::default();
        let submission = fresh_submission("bot");
        validate_only(&submission, &cfg).expect("fresh submission validates");
    }

    #[test]
    fn validate_only_rejects_blank_agent_name() {
        let cfg = Config::default();
        let submission = fresh_submission("");
        let errors = validate_only(&submission, &cfg).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.step == QuickstartStep::Agent && e.field == "name")
        );
    }

    #[test]
    fn validate_only_rejects_existing_agent_name() {
        let mut cfg = Config::default();
        cfg.agents.insert(
            "bot".into(),
            zeroclaw_config::schema::AliasedAgentConfig::default(),
        );
        let submission = fresh_submission("bot");
        let errors = validate_only(&submission, &cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.step == QuickstartStep::Agent));
    }

    #[test]
    fn validate_only_rejects_unknown_risk_preset() {
        let cfg = Config::default();
        let mut submission = fresh_submission("bot");
        submission.risk_profile = SelectorChoice::Fresh("does-not-exist".into());
        let errors = validate_only(&submission, &cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.step == QuickstartStep::RiskProfile));
    }
}
