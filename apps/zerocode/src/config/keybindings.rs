//! Local keybinding presets and override resolution.
//!
//! A preset is a named, full keymap built from the typed action enums
//! and `Chord` constructors — never from authored `"tag.variant"` or
//! `"ctrl+p"` strings. Picking a preset fully overwrites the
//! `[keybindings]` table; the per-row capture modal merges single rows
//! on top. Presets are walked from `KEY_PRESETS`, mirroring `THEMES`.

use std::collections::HashMap;

use anyhow::{Result, bail};
use crossterm::event::KeyCode;

use crate::keymap::{
    Chord, DashboardTabAction, FileExplorerAction, LogsTabAction, overrides::OverrideTable,
    reserved_reason,
};

/// Default preset name — empty override set, i.e. pure compile-time
/// bindings.
pub const DEFAULT_PRESET_NAME: &str = "default";

/// A named keybinding preset. `build` returns the sparse
/// `action_key -> chords` rows; only diffs from the compile-time
/// defaults are listed.
#[derive(Clone, Copy)]
pub struct KeyPreset {
    pub build: fn() -> Vec<(String, Vec<Chord>)>,
}

fn default_rows() -> Vec<(String, Vec<Chord>)> {
    Vec::new()
}

fn emacs_rows() -> Vec<(String, Vec<Chord>)> {
    let p = Chord::ctrl('p');
    let n = Chord::ctrl('n');
    vec![
        (DashboardTabAction::Up.action_key(), vec![p.clone()]),
        (DashboardTabAction::Down.action_key(), vec![n.clone()]),
        (LogsTabAction::Up.action_key(), vec![p.clone()]),
        (LogsTabAction::Down.action_key(), vec![n.clone()]),
        (FileExplorerAction::Up.action_key(), vec![p]),
        (FileExplorerAction::Down.action_key(), vec![n]),
    ]
}

fn vim_rows() -> Vec<(String, Vec<Chord>)> {
    let k = Chord::char('k');
    let j = Chord::char('j');
    let g = Chord::char('g');
    let cap_g = Chord::char('G');
    vec![
        (DashboardTabAction::Up.action_key(), vec![k.clone()]),
        (DashboardTabAction::Down.action_key(), vec![j.clone()]),
        (
            DashboardTabAction::PrevTab.action_key(),
            vec![Chord::char('h')],
        ),
        (
            DashboardTabAction::NextTab.action_key(),
            vec![Chord::char('l')],
        ),
        (DashboardTabAction::JumpStart.action_key(), vec![g.clone()]),
        (
            DashboardTabAction::JumpEnd.action_key(),
            vec![cap_g.clone()],
        ),
        (LogsTabAction::Up.action_key(), vec![k.clone()]),
        (LogsTabAction::Down.action_key(), vec![j.clone()]),
        (LogsTabAction::JumpStart.action_key(), vec![g.clone()]),
        (LogsTabAction::JumpEnd.action_key(), vec![cap_g.clone()]),
        (FileExplorerAction::Up.action_key(), vec![k]),
        (FileExplorerAction::Down.action_key(), vec![j]),
        (FileExplorerAction::JumpStart.action_key(), vec![g]),
        (FileExplorerAction::JumpEnd.action_key(), vec![cap_g]),
    ]
}

fn arrows_only_rows() -> Vec<(String, Vec<Chord>)> {
    let up = Chord::key(KeyCode::Up);
    let down = Chord::key(KeyCode::Down);
    vec![
        (DashboardTabAction::Up.action_key(), vec![up.clone()]),
        (DashboardTabAction::Down.action_key(), vec![down.clone()]),
        (
            DashboardTabAction::NextTab.action_key(),
            vec![Chord::key(KeyCode::Right)],
        ),
        (
            DashboardTabAction::PrevTab.action_key(),
            vec![Chord::key(KeyCode::Left)],
        ),
        (LogsTabAction::Up.action_key(), vec![up.clone()]),
        (LogsTabAction::Down.action_key(), vec![down.clone()]),
        (FileExplorerAction::Up.action_key(), vec![up]),
        (FileExplorerAction::Down.action_key(), vec![down]),
    ]
}

/// Registry of named presets. Walked by the zerocode tab's preset picker.
pub const KEY_PRESETS: &[(&str, KeyPreset)] = &[
    (
        DEFAULT_PRESET_NAME,
        KeyPreset {
            build: default_rows,
        },
    ),
    ("vim", KeyPreset { build: vim_rows }),
    ("emacs", KeyPreset { build: emacs_rows }),
    (
        "arrows_only",
        KeyPreset {
            build: arrows_only_rows,
        },
    ),
];

pub fn preset_names() -> impl Iterator<Item = &'static str> {
    KEY_PRESETS.iter().map(|(n, _)| *n)
}

pub fn preset_by_name(name: &str) -> Option<&'static KeyPreset> {
    KEY_PRESETS
        .iter()
        .find_map(|(n, p)| (*n == name).then_some(p))
}

impl KeyPreset {
    /// Resolve into a validated override table keyed `tag -> variant ->
    /// chords`, running the full validation battery.
    pub fn resolve(&self) -> Result<OverrideTable> {
        let rows: HashMap<String, Vec<Chord>> = (self.build)().into_iter().collect();
        build_override_table(rows)
    }
}

/// Turn a sparse `action_key -> chords` map into the nested
/// `tag -> variant -> chords` override table, validating reserved chords,
/// intra-action duplicates, and intra-tag chord uniqueness.
pub fn build_override_table(rows: HashMap<String, Vec<Chord>>) -> Result<OverrideTable> {
    let mut table: OverrideTable = HashMap::new();
    let mut seen: HashMap<String, HashMap<Chord, String>> = HashMap::new();

    for (action_key, chords) in rows {
        let (tag, variant) = action_key
            .split_once('.')
            .ok_or_else(|| anyhow::anyhow!("keybinding key '{action_key}' missing '.<variant>'"))?;

        for c in &chords {
            if let Some(reason) = reserved_reason(c) {
                bail!("'{action_key}' -> '{}' is {reason}", c.wire());
            }
        }
        for (i, a) in chords.iter().enumerate() {
            if chords[i + 1..].contains(a) {
                bail!("'{action_key}' lists '{}' twice", a.wire());
            }
        }
        let tag_seen = seen.entry(tag.to_string()).or_default();
        for c in &chords {
            if let Some(other) = tag_seen.get(c) {
                bail!(
                    "chord '{}' bound to both '{action_key}' and '{other}'",
                    c.wire()
                );
            }
            tag_seen.insert(c.clone(), action_key.clone());
        }

        table
            .entry(tag.to_string())
            .or_default()
            .insert(variant.to_string(), chords);
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_is_empty() {
        let t = preset_by_name(DEFAULT_PRESET_NAME)
            .unwrap()
            .resolve()
            .unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn every_preset_resolves_and_is_clean() {
        for name in preset_names() {
            preset_by_name(name)
                .unwrap()
                .resolve()
                .unwrap_or_else(|e| panic!("preset '{name}' invalid: {e}"));
        }
    }

    #[test]
    fn preset_names_are_snake_case() {
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !s.starts_with('_')
                && !s.ends_with('_')
        };
        for name in preset_names() {
            assert!(ok(name), "preset name '{name}' is not snake_case");
        }
    }

    #[test]
    fn reserved_chord_in_table_is_rejected() {
        let mut rows = HashMap::new();
        rows.insert("chat.scroll_up".to_string(), vec![Chord::key(KeyCode::Esc)]);
        assert!(build_override_table(rows).is_err());
    }

    #[test]
    fn intra_tag_chord_clash_is_rejected() {
        let mut rows = HashMap::new();
        rows.insert("dashboard.up".to_string(), vec![Chord::char('z')]);
        rows.insert("dashboard.down".to_string(), vec![Chord::char('z')]);
        assert!(build_override_table(rows).is_err());
    }

    #[test]
    fn intra_action_duplicate_is_rejected() {
        let mut rows = HashMap::new();
        rows.insert(
            "dashboard.up".to_string(),
            vec![Chord::char('z'), Chord::char('z')],
        );
        assert!(build_override_table(rows).is_err());
    }
}
