//! ZeroClaw TUI colour palette and style helpers.
//!
//! Shared between the onboarding UI (lib target) and the main chat TUI (binary
//! target). Not every helper is used by both targets.
#![allow(dead_code)]

use std::sync::RwLock;

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    pub title: Color,
    pub heading: Color,
    pub body: Color,
    pub dim: Color,
    pub accent: Color,
    pub warn: Color,
    pub selection_bg: Color,
    pub tool: Color,
}

const ICY_BLUE: Theme = Theme {
    title: Color::Rgb(100, 200, 255),
    heading: Color::Rgb(140, 230, 255),
    body: Color::Rgb(220, 240, 255),
    dim: Color::Rgb(80, 130, 170),
    accent: Color::Rgb(255, 100, 80),
    warn: Color::Rgb(255, 220, 80),
    selection_bg: Color::Rgb(30, 60, 100),
    tool: Color::Rgb(180, 140, 255),
};

const SOLARIZED_DARK: Theme = Theme {
    title: Color::Rgb(38, 139, 210),
    heading: Color::Rgb(42, 161, 152),
    body: Color::Rgb(147, 161, 161),
    dim: Color::Rgb(88, 110, 117),
    accent: Color::Rgb(220, 50, 47),
    warn: Color::Rgb(181, 137, 0),
    selection_bg: Color::Rgb(7, 54, 66),
    tool: Color::Rgb(108, 113, 196),
};

const SOLARIZED_LIGHT: Theme = Theme {
    title: Color::Rgb(38, 139, 210),
    heading: Color::Rgb(42, 161, 152),
    body: Color::Rgb(101, 123, 131),
    dim: Color::Rgb(147, 161, 161),
    accent: Color::Rgb(220, 50, 47),
    warn: Color::Rgb(181, 137, 0),
    selection_bg: Color::Rgb(238, 232, 213),
    tool: Color::Rgb(108, 113, 196),
};

const HIGH_CONTRAST_WHITE: Theme = Theme {
    title: Color::Rgb(0, 0, 0),
    heading: Color::Rgb(0, 0, 128),
    body: Color::Rgb(0, 0, 0),
    dim: Color::Rgb(64, 64, 64),
    accent: Color::Rgb(176, 0, 0),
    warn: Color::Rgb(128, 96, 0),
    selection_bg: Color::Rgb(200, 200, 200),
    tool: Color::Rgb(96, 0, 128),
};

const HIGH_CONTRAST_DARK: Theme = Theme {
    title: Color::Rgb(255, 255, 255),
    heading: Color::Rgb(0, 255, 255),
    body: Color::Rgb(255, 255, 255),
    dim: Color::Rgb(170, 170, 170),
    accent: Color::Rgb(255, 85, 85),
    warn: Color::Rgb(255, 255, 0),
    selection_bg: Color::Rgb(60, 60, 60),
    tool: Color::Rgb(255, 0, 255),
};

const GRUVBOX_DARK: Theme = Theme {
    title: Color::Rgb(131, 165, 152),
    heading: Color::Rgb(142, 192, 124),
    body: Color::Rgb(235, 219, 178),
    dim: Color::Rgb(146, 131, 116),
    accent: Color::Rgb(251, 73, 52),
    warn: Color::Rgb(250, 189, 47),
    selection_bg: Color::Rgb(60, 56, 54),
    tool: Color::Rgb(211, 134, 155),
};

const DRACULA: Theme = Theme {
    title: Color::Rgb(139, 233, 253),
    heading: Color::Rgb(80, 250, 123),
    body: Color::Rgb(248, 248, 242),
    dim: Color::Rgb(98, 114, 164),
    accent: Color::Rgb(255, 85, 85),
    warn: Color::Rgb(241, 250, 140),
    selection_bg: Color::Rgb(68, 71, 90),
    tool: Color::Rgb(189, 147, 249),
};

const NORD: Theme = Theme {
    title: Color::Rgb(136, 192, 208),
    heading: Color::Rgb(143, 188, 187),
    body: Color::Rgb(216, 222, 233),
    dim: Color::Rgb(76, 86, 106),
    accent: Color::Rgb(191, 97, 106),
    warn: Color::Rgb(235, 203, 139),
    selection_bg: Color::Rgb(59, 66, 82),
    tool: Color::Rgb(180, 142, 173),
};

pub(crate) const DEFAULT_THEME_NAME: &str = "icy-blue";

pub(crate) const THEMES: &[(&str, Theme)] = &[
    ("icy-blue", ICY_BLUE),
    ("solarized-dark", SOLARIZED_DARK),
    ("solarized-light", SOLARIZED_LIGHT),
    ("high-contrast-white", HIGH_CONTRAST_WHITE),
    ("high-contrast-dark", HIGH_CONTRAST_DARK),
    ("gruvbox-dark", GRUVBOX_DARK),
    ("dracula", DRACULA),
    ("nord", NORD),
];

pub(crate) fn theme_by_name(name: &str) -> Option<Theme> {
    THEMES
        .iter()
        .find_map(|(n, t)| (*n == name).then_some(*t))
}

pub(crate) fn theme_names() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|(n, _)| *n)
}

static ACTIVE: RwLock<Theme> = RwLock::new(ICY_BLUE);

pub(crate) fn set_active(theme: Theme) {
    if let Ok(mut guard) = ACTIVE.write() {
        *guard = theme;
    }
}

pub(crate) fn active() -> Theme {
    ACTIVE.read().map(|g| *g).unwrap_or(ICY_BLUE)
}

pub(crate) fn default_theme() -> Theme {
    ICY_BLUE
}

pub(crate) fn fg_primary() -> Color {
    active().body
}

pub(crate) fn selection_bg() -> Color {
    active().selection_bg
}

pub(crate) fn title_style() -> Style {
    Style::default()
        .fg(active().title)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn heading_style() -> Style {
    Style::default()
        .fg(active().heading)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn body_style() -> Style {
    Style::default().fg(active().body)
}

pub(crate) fn dim_style() -> Style {
    Style::default().fg(active().dim)
}

pub(crate) fn accent_style() -> Style {
    Style::default()
        .fg(active().accent)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn warn_style() -> Style {
    Style::default().fg(active().warn)
}

pub(crate) fn selected_style() -> Style {
    let t = active();
    Style::default()
        .fg(t.title)
        .bg(t.selection_bg)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn input_style() -> Style {
    Style::default().fg(active().body)
}

/// "You:" label in the chat conversation.
pub(crate) fn user_label_style() -> Style {
    Style::default()
        .fg(active().heading)
        .add_modifier(Modifier::BOLD)
}

/// "Agent:" label in the chat conversation.
pub(crate) fn agent_label_style() -> Style {
    Style::default()
        .fg(active().title)
        .add_modifier(Modifier::BOLD)
}

/// Error messages (error phase, etc.).
pub(crate) fn error_style() -> Style {
    Style::default().fg(active().accent)
}

/// Tool call label `[tool: name]`.
pub(crate) fn tool_label_style() -> Style {
    Style::default()
        .fg(active().tool)
        .add_modifier(Modifier::BOLD)
}

/// Inline code spans in markdown.
pub(crate) fn code_inline_style() -> Style {
    Style::default().fg(active().warn)
}

/// Code block body lines.
pub(crate) fn code_block_style() -> Style {
    Style::default().fg(active().body)
}

/// Thought / thinking output.
pub(crate) fn thought_style() -> Style {
    Style::default()
        .fg(active().dim)
        .add_modifier(Modifier::ITALIC)
}

/// Overlay border/title accent (session list, rename, approval).
pub(crate) fn overlay_border_style() -> Style {
    Style::default().fg(active().heading)
}

/// Approval overlay border (warning tone).
pub(crate) fn approval_border_style() -> Style {
    Style::default().fg(active().warn)
}

/// Highlight style for list items (agent picker, session list).
pub(crate) fn list_highlight_style() -> Style {
    Style::default()
        .fg(active().heading)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icy_blue_rgb_unchanged() {
        let t = theme_by_name("icy-blue").expect("icy-blue registered");
        assert_eq!(t.title, Color::Rgb(100, 200, 255));
        assert_eq!(t.heading, Color::Rgb(140, 230, 255));
        assert_eq!(t.body, Color::Rgb(220, 240, 255));
        assert_eq!(t.dim, Color::Rgb(80, 130, 170));
        assert_eq!(t.accent, Color::Rgb(255, 100, 80));
        assert_eq!(t.warn, Color::Rgb(255, 220, 80));
        assert_eq!(t.selection_bg, Color::Rgb(30, 60, 100));
        assert_eq!(t.tool, Color::Rgb(180, 140, 255));
    }

    #[test]
    fn unknown_theme_is_none() {
        assert!(theme_by_name("no-such-theme").is_none());
    }

    #[test]
    fn default_is_registered() {
        assert!(theme_by_name(DEFAULT_THEME_NAME).is_some());
    }

    #[test]
    fn set_active_swaps_palette() {
        set_active(theme_by_name("nord").unwrap());
        assert_eq!(active().title, Color::Rgb(136, 192, 208));
        set_active(theme_by_name("icy-blue").unwrap());
        assert_eq!(active().title, Color::Rgb(100, 200, 255));
    }
}
