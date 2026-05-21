//! `OnboardUi` backends. `term` is the dialoguer-based terminal UI; `quick`
//! is the headless, flag-driven backend for scripted/CI runs. The ratatui
//! backend lives in `apps/tui` (`zeroclaw-tui` crate).

pub mod quick;
pub mod term;

pub use quick::QuickUi;
pub use term::TermUi;
