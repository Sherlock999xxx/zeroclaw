//! CLI handler for the `zeroclaw dream` command.

use anyhow::{Context, Result};
use zeroclaw_config::schema::Config;
use zeroclaw_runtime::dream::report::DreamReport;
use zeroclaw_runtime::i18n::{get_cli_string_with_args, get_required_cli_string};

/// Run a manual dream cycle from the CLI.
pub async fn run_dream(config: &Config, dry_run: bool, verbose: bool) -> Result<()> {
    use zeroclaw_runtime::dream::engine::DreamEngine;

    // Build dream config with audit_mode = dry_run.
    let mut dream_config = config.dream_mode.clone();
    if dry_run {
        dream_config.audit_mode = true;
    }

    let engine = DreamEngine::new(dream_config, config.workspace_dir.clone());

    // Resolve provider through the standard provider/runtime resolution stack.
    let fallback = config
        .providers
        .fallback_provider()
        .context("dream: no fallback provider configured")?;
    let provider_name = config.providers.fallback.as_deref().unwrap_or("anthropic");
    let model = config
        .dream_mode
        .model
        .as_deref()
        .or(fallback.model.as_deref())
        .unwrap_or("claude-haiku-4-5-20251001");

    let provider_runtime_options =
        zeroclaw_providers::provider_runtime_options_from_config(config);
    let provider = zeroclaw_providers::create_routed_provider_with_options(
        provider_name,
        fallback.api_key.as_deref(),
        fallback.base_url.as_deref(),
        &config.reliability,
        &config.providers.model_routes,
        model,
        &provider_runtime_options,
    )?;

    // Create memory backend.
    let memory = zeroclaw_memory::create_memory(
        &config.memory,
        &config.workspace_dir,
        config
            .providers
            .fallback_provider()
            .and_then(|e| e.api_key.as_deref()),
    )
    .context("dream: failed to create memory backend")?;

    if verbose {
        println!(
            "{}",
            get_cli_string_with_args(
                "cli-dream-starting",
                &[
                    ("provider", provider_name),
                    ("model", model),
                    ("backend", memory.name()),
                ],
            )
            .unwrap_or_else(|| format!(
                "Dream cycle starting...\n  Provider: {provider_name}\n  Model: {model}\n  Memory backend: {}",
                memory.name()
            ))
        );
        if dry_run {
            println!(
                "{}",
                get_required_cli_string("cli-dream-dry-run-mode")
            );
        }
    }

    let result = engine
        .run_cycle(memory.as_ref(), provider.as_ref(), model)
        .await?;

    println!(
        "{}",
        get_cli_string_with_args(
            "cli-dream-complete",
            &[
                ("gathered", &result.gathered_count.to_string()),
                ("consolidated", &result.consolidated_count.to_string()),
                ("pruned", &result.pruned_count.to_string()),
            ],
        )
        .unwrap_or_else(|| format!(
            "Dream cycle complete: {} memories gathered, {} insights consolidated, {} pruned",
            result.gathered_count, result.consolidated_count, result.pruned_count
        ))
    );

    if !result.insights.is_empty() {
        println!(
            "\n{}",
            get_required_cli_string("cli-dream-insights-header")
        );
        for (i, insight) in result.insights.iter().enumerate() {
            println!("  {}. {insight}", i + 1);
        }
    }

    if let Some(ref summary) = result.report_summary {
        println!(
            "\n{}",
            get_cli_string_with_args("cli-dream-summary", &[("summary", summary.as_str())])
                .unwrap_or_else(|| format!("Summary: {summary}"))
        );
    }

    if dry_run {
        println!(
            "\n{}",
            get_required_cli_string("cli-dream-dry-run-notice")
        );
    }

    Ok(())
}

/// Show the pending dream report, if any.
pub fn show_report(config: &Config) -> Result<()> {
    match DreamReport::load_pending(&config.workspace_dir)? {
        Some(report) => {
            println!("{}", report.format_message());
            DreamReport::mark_delivered(&config.workspace_dir)?;
        }
        None => {
            println!("{}", get_required_cli_string("cli-dream-no-report"));
        }
    }
    Ok(())
}
