//! Local zerocode client configuration: theme and (later) keybindings.
//!
//! Always read from the local `<config_dir>/zerocode-config.toml`, independent
//! of the connection target. Layering: defaults -> file -> `ZEROCODE_*` env.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::theme::{self, Theme};

const FILE_NAME: &str = "zerocode-config.toml";
const ENV_PREFIX: &str = "ZEROCODE_";
const ENV_SEP: &str = "__";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ZerocodeConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    theme::DEFAULT_THEME_NAME.to_string()
}

impl Default for ZerocodeConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

impl ZerocodeConfig {
    pub fn resolve_theme(&self) -> Result<Theme> {
        if self.theme.trim().is_empty() {
            return theme::theme_by_name(theme::DEFAULT_THEME_NAME)
                .context("default theme missing from registry");
        }
        theme::theme_by_name(&self.theme).with_context(|| {
            let known = theme::theme_names().collect::<Vec<_>>().join(", ");
            format!(
                "unknown theme '{}' in {FILE_NAME}; known themes: {known}",
                self.theme
            )
        })
    }
}

pub(crate) fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Ensure the config dir and file exist, then load + apply env overrides.
pub(crate) fn ensure_and_load(config_dir: &Path) -> Result<ZerocodeConfig> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("creating config dir {}", config_dir.display()))?;

    let path = config_path(config_dir);
    if !path.exists() {
        let default = ZerocodeConfig::default();
        let body = toml::to_string_pretty(&default).context("serializing default config")?;
        std::fs::write(&path, body)
            .with_context(|| format!("writing default {}", path.display()))?;
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let mut config: ZerocodeConfig =
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;

    apply_env_overrides(&mut config)?;
    Ok(config)
}

/// Persist the selected theme name back to disk via a serde roundtrip set.
pub(crate) fn persist_theme(config_dir: &Path, theme_name: &str) -> Result<()> {
    let path = config_path(config_dir);
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut config: ZerocodeConfig = toml::from_str(&raw).unwrap_or_default();
    set_prop(&mut config, "theme", theme_name)?;
    let body = toml::to_string_pretty(&config).context("serializing config")?;
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Apply every `ZEROCODE_<dotted__path>=value` env var. Hard-errors on any var
/// that does not resolve to a known config path.
fn apply_env_overrides(config: &mut ZerocodeConfig) -> Result<()> {
    let mut entries: Vec<(String, String, String)> = std::env::vars()
        .filter_map(|(k, v)| {
            let tail = k.strip_prefix(ENV_PREFIX)?;
            (!tail.is_empty()
                && tail
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .then(|| (k.clone(), v, tail.replace(ENV_SEP, ".")))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (env_name, value, path) in entries {
        set_prop(config, &path, &value)
            .with_context(|| format!("{env_name} -> {path}"))?;
    }
    Ok(())
}

/// Set a leaf at a dotted `path` via a serde roundtrip through `toml::Value`.
/// No field names are hardcoded: the struct's serialized shape is the registry.
fn set_prop<T: Serialize + serde::de::DeserializeOwned>(
    target: &mut T,
    path: &str,
    value: &str,
) -> Result<()> {
    let mut root = toml::Value::try_from(&*target).context("serializing config for set_prop")?;
    let segments: Vec<&str> = path.split('.').collect();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| anyhow::Error::msg("empty config path"))?;

    let mut cursor = &mut root;
    for seg in parents {
        cursor = cursor
            .as_table_mut()
            .and_then(|t| t.get_mut(*seg))
            .ok_or_else(|| {
                anyhow::Error::msg(format!("path '{path}' did not resolve to a config field"))
            })?;
    }
    let table = cursor
        .as_table_mut()
        .ok_or_else(|| {
            anyhow::Error::msg(format!("path '{path}' did not resolve to a config field"))
        })?;
    if !table.contains_key(*leaf) {
        anyhow::bail!("path '{path}' did not resolve to a config field");
    }
    table.insert((*leaf).to_string(), toml::Value::String(value.to_string()));

    *target = root.try_into().context("deserializing config after set_prop")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_registered() {
        let c = ZerocodeConfig::default();
        assert_eq!(c.theme, theme::DEFAULT_THEME_NAME);
        assert!(c.resolve_theme().is_ok());
    }

    #[test]
    fn set_prop_roundtrip() {
        let mut c = ZerocodeConfig::default();
        set_prop(&mut c, "theme", "nord").unwrap();
        assert_eq!(c.theme, "nord");
    }

    #[test]
    fn set_prop_unknown_path_errors() {
        let mut c = ZerocodeConfig::default();
        let err = set_prop(&mut c, "no_such_field", "x").unwrap_err();
        assert!(err.to_string().contains("did not resolve"));
    }

    #[test]
    fn resolve_unknown_theme_errors() {
        let c = ZerocodeConfig {
            theme: "bogus".to_string(),
        };
        let err = c.resolve_theme().unwrap_err();
        assert!(err.to_string().contains("unknown theme 'bogus'"));
    }

    #[test]
    fn resolve_empty_theme_recovers_to_default() {
        for blank in ["", "   "] {
            let c = ZerocodeConfig {
                theme: blank.to_string(),
            };
            let resolved = c.resolve_theme().expect("empty theme recovers to default");
            assert_eq!(resolved.title, theme::default_theme().title);
        }
    }
}
