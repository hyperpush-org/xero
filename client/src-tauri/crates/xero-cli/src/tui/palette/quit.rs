//! `quit` — exit the TUI.

use crate::GlobalOptions;

use super::{super::app::App, OpenOutcome};

pub fn open(_globals: &GlobalOptions, _app: &mut App) -> OpenOutcome {
    OpenOutcome::Quit
}
