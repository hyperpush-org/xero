//! OpenCode-style terminal client for Xero (M1: single-column transcript +
//! composer + footer; palette stub).

mod app;
mod composer;
mod footer;
mod palette;
mod project;
mod runtime;
mod slash;
mod theme;
mod transcript;

use serde_json::json;

use crate::{response, take_bool_flag, take_help, CliError, CliResponse, GlobalOptions};

pub(crate) fn command_tui(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero tui [--smoke|--smoke-run]\n\
             Opens the OpenCode-style Xero agent client. Use --smoke for headless launch \
             verification and --smoke-run for an explicit fake-provider run check.",
            json!({ "command": "tui" }),
        ));
    }
    let smoke = take_bool_flag(&mut args, "--smoke");
    let smoke_run = take_bool_flag(&mut args, "--smoke-run");
    crate::reject_unknown_options(&args)?;
    if smoke {
        let snapshot = app::smoke_snapshot(&globals)?;
        return Ok(response(
            &globals,
            "Xero TUI smoke passed.",
            json!({ "kind": "tuiSmoke", "snapshot": snapshot }),
        ));
    }
    if smoke_run {
        let snapshot = app::smoke_fake_provider_run(&globals)?;
        return Ok(response(
            &globals,
            "Xero TUI fake-provider run smoke passed.",
            json!({ "kind": "tuiSmokeRun", "snapshot": snapshot }),
        ));
    }
    app::run_interactive(globals)
}
