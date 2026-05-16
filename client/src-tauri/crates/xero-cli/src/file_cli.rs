use serde_json::{json, Value as JsonValue};
use xero_agent_core::{
    HeadlessProductionToolRuntime, ToolBatchDispatchReport, ToolCallInput, ToolDispatchOutcome,
};

use super::{
    core_error, generate_id, parse_positive_usize, project_cli, response, take_help, take_option,
    CliError, CliResponse, GlobalOptions,
};

pub(crate) fn dispatch_file(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") | Some("tree") => command_file_tool(globals, args[1..].to_vec(), "list"),
        Some("read") | Some("preview") => command_file_tool(globals, args[1..].to_vec(), "read"),
        Some("write") | Some("create") => {
            command_file_tool(globals, args[1..].to_vec(), "write")
        }
        Some("patch") => command_file_tool(globals, args[1..].to_vec(), "patch"),
        Some("delete") | Some("remove") => {
            command_file_tool(globals, args[1..].to_vec(), "delete")
        }
        Some("move") | Some("rename") => command_file_tool(globals, args[1..].to_vec(), "move"),
        Some("replace") => command_file_tool(globals, args[1..].to_vec(), "replace"),
        Some("tools") => command_file_tools(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero file list|read|write|patch|delete|move|replace|tools [--project-id ID | --repo PATH]\nThese commands dispatch through the shared owned-agent Tool Registry V2, not a separate filesystem implementation.",
            json!({ "command": "file" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown file command `{other}`. Use list, read, write, patch, delete, move, replace, or tools."
        ))),
    }
}

fn command_file_tools(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero file tools [--read-only] [--repo PATH | --project-id ID]",
            json!({ "command": "file tools" }),
        ));
    }
    let read_only = take_bool_flag_local(&mut args, "--read-only");
    let repo = project_cli::take_repo_path(&globals, &mut args)?;
    reject_file_unknown_options(&args)?;
    let runtime = HeadlessProductionToolRuntime::new(
        Some(&repo),
        !read_only,
        vec![globals.state_dir.to_string_lossy().into_owned()],
    )
    .map_err(core_error)?;
    let descriptors = runtime.descriptors();
    let text = descriptors
        .iter()
        .map(|descriptor| {
            format!(
                "{:<10} {:<18?} {:?}",
                descriptor.name, descriptor.effect_class, descriptor.mutability
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(response(
        &globals,
        text,
        json!({ "kind": "fileTools", "repo": repo, "descriptors": descriptors }),
    ))
}

fn command_file_tool(
    globals: GlobalOptions,
    mut args: Vec<String>,
    tool_name: &'static str,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            usage_for_tool(tool_name),
            json!({ "command": format!("file {tool_name}") }),
        ));
    }
    let repo = project_cli::take_repo_path(&globals, &mut args)?;
    let input = input_for_tool(tool_name, &mut args)?;
    reject_file_unknown_options(&args)?;
    let allow_workspace_writes =
        matches!(tool_name, "write" | "patch" | "delete" | "move" | "replace");
    let runtime = HeadlessProductionToolRuntime::new(
        Some(&repo),
        allow_workspace_writes,
        vec![globals.state_dir.to_string_lossy().into_owned()],
    )
    .map_err(core_error)?;
    let call = ToolCallInput {
        tool_call_id: format!("cli-file-{}", generate_id("tool")),
        tool_name: tool_name.into(),
        input,
    };
    let report = runtime
        .dispatch_batch("cli-file", "cli-file-run", 0, &[call])
        .map_err(core_error)?;
    let (text, output) = summarize_tool_report(&report)?;
    Ok(response(
        &globals,
        text,
        json!({ "kind": "fileToolDispatch", "repo": repo, "tool": tool_name, "output": output, "report": report }),
    ))
}

fn input_for_tool(tool_name: &str, args: &mut Vec<String>) -> Result<JsonValue, CliError> {
    match tool_name {
        "list" => {
            let path = take_option(args, "--path")?.or_else(|| take_optional_positional(args));
            Ok(json!({ "path": path.unwrap_or_else(|| ".".into()) }))
        }
        "read" => {
            let path = take_option(args, "--path")?
                .or_else(|| take_optional_positional(args))
                .ok_or_else(|| CliError::usage("Missing path for `xero file read`."))?;
            Ok(json!({ "path": path }))
        }
        "write" => {
            let path = take_option(args, "--path")?
                .or_else(|| take_optional_positional(args))
                .ok_or_else(|| CliError::usage("Missing path for `xero file write`."))?;
            let content = take_option(args, "--content")?
                .ok_or_else(|| CliError::usage("Missing `--content`."))?;
            Ok(json!({ "path": path, "content": content }))
        }
        "patch" => {
            let patch = take_option(args, "--patch")?
                .ok_or_else(|| CliError::usage("Missing `--patch`."))?;
            Ok(json!({ "patch": patch }))
        }
        "delete" => {
            let path = take_option(args, "--path")?
                .or_else(|| take_optional_positional(args))
                .ok_or_else(|| CliError::usage("Missing path for `xero file delete`."))?;
            let recursive = take_bool_flag_local(args, "--recursive");
            Ok(json!({ "path": path, "recursive": recursive }))
        }
        "move" => {
            let from = take_option(args, "--from")?
                .or_else(|| take_optional_positional(args))
                .ok_or_else(|| CliError::usage("Missing source path for `xero file move`."))?;
            let to = take_option(args, "--to")?
                .or_else(|| take_optional_positional(args))
                .ok_or_else(|| CliError::usage("Missing destination path for `xero file move`."))?;
            Ok(json!({ "from": from, "to": to }))
        }
        "replace" => {
            let path = take_option(args, "--path")?.or_else(|| take_optional_positional(args));
            let search = take_option(args, "--search")?
                .ok_or_else(|| CliError::usage("Missing `--search`."))?;
            let replacement = take_option(args, "--replacement")?
                .ok_or_else(|| CliError::usage("Missing `--replacement`."))?;
            let dry_run = take_bool_flag_local(args, "--dry-run");
            let max_replacements = take_option(args, "--max-replacements")?
                .map(|value| parse_positive_usize(&value, "--max-replacements"))
                .transpose()?;
            let mut input = json!({
                "path": path.unwrap_or_else(|| ".".into()),
                "search": search,
                "replacement": replacement,
                "dryRun": dry_run,
            });
            if let Some(max_replacements) = max_replacements {
                input["maxReplacements"] = json!(max_replacements);
            }
            Ok(input)
        }
        _ => unreachable!("known file tool"),
    }
}

fn summarize_tool_report(
    report: &ToolBatchDispatchReport,
) -> Result<(String, JsonValue), CliError> {
    let Some(outcome) = report
        .groups
        .iter()
        .flat_map(|group| group.outcomes.iter())
        .next()
    else {
        return Err(CliError::system_fault(
            "xero_cli_file_tool_empty_report",
            "Tool Registry V2 returned no dispatch outcome.",
        ));
    };
    match outcome {
        ToolDispatchOutcome::Succeeded(success) => {
            let text = success
                .output
                .get("content")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .or_else(|| success.output.get("files").map(|files| files.to_string()))
                .unwrap_or_else(|| success.summary.clone());
            Ok((text, success.output.clone()))
        }
        ToolDispatchOutcome::Failed(failure) => Err(CliError::user_fixable(
            failure.error.code.clone(),
            failure.error.message.clone(),
        )),
    }
}

fn usage_for_tool(tool_name: &str) -> &'static str {
    match tool_name {
        "list" => "Usage: xero file list [PATH|--path PATH] [--repo PATH | --project-id ID]",
        "read" => "Usage: xero file read PATH [--repo PATH | --project-id ID]",
        "write" => {
            "Usage: xero file write PATH --content TEXT [--repo PATH | --project-id ID]\nUses the shared Tool Registry V2 write tool with its policy, sandbox, and rollback metadata."
        }
        "patch" => {
            "Usage: xero file patch --patch UNIFIED_DIFF [--repo PATH | --project-id ID]\nUses the shared Tool Registry V2 patch tool."
        }
        "delete" => {
            "Usage: xero file delete PATH [--recursive] [--repo PATH | --project-id ID]\nUses the shared Tool Registry V2 delete tool."
        }
        "move" => {
            "Usage: xero file move FROM TO [--repo PATH | --project-id ID]\nUses the shared Tool Registry V2 move tool."
        }
        "replace" => {
            "Usage: xero file replace [PATH|--path PATH] --search TEXT --replacement TEXT [--dry-run] [--max-replacements N] [--repo PATH | --project-id ID]\nUses the shared Tool Registry V2 replace tool."
        }
        _ => "Usage: xero file list|read|write|patch|delete|move|replace",
    }
}

fn take_optional_positional(args: &mut Vec<String>) -> Option<String> {
    if args.first().is_some_and(|arg| !arg.starts_with('-')) {
        Some(args.remove(0))
    } else {
        None
    }
}

fn take_bool_flag_local(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn reject_file_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn file_write_input_preserves_tool_registry_schema() {
        let mut args = vec!["src/lib.rs".into(), "--content".into(), "hello".into()];
        let input = input_for_tool("write", &mut args).expect("write input");
        assert_eq!(input, json!({ "path": "src/lib.rs", "content": "hello" }));
        assert!(args.is_empty());
    }

    #[test]
    fn file_patch_requires_patch_payload() {
        let mut args = Vec::new();
        let error = input_for_tool("patch", &mut args).expect_err("patch required");
        assert_eq!(error.code, "xero_cli_usage");
    }

    #[test]
    fn file_delete_and_move_inputs_preserve_tool_registry_schema() {
        let mut delete_args = vec!["tmp/remove-me.txt".into(), "--recursive".into()];
        let delete_input = input_for_tool("delete", &mut delete_args).expect("delete input");
        assert_eq!(
            delete_input,
            json!({ "path": "tmp/remove-me.txt", "recursive": true })
        );
        assert!(delete_args.is_empty());

        let mut move_args = vec!["tmp/source.txt".into(), "tmp/dest.txt".into()];
        let move_input = input_for_tool("move", &mut move_args).expect("move input");
        assert_eq!(
            move_input,
            json!({ "from": "tmp/source.txt", "to": "tmp/dest.txt" })
        );
        assert!(move_args.is_empty());
    }

    #[test]
    fn file_replace_input_preserves_tool_registry_schema() {
        let mut args = vec![
            "src".into(),
            "--search".into(),
            "alpha".into(),
            "--replacement".into(),
            "".into(),
            "--dry-run".into(),
            "--max-replacements".into(),
            "3".into(),
        ];
        let input = input_for_tool("replace", &mut args).expect("replace input");
        assert_eq!(
            input,
            json!({
                "path": "src",
                "search": "alpha",
                "replacement": "",
                "dryRun": true,
                "maxReplacements": 3,
            })
        );
        assert!(args.is_empty());
    }

    #[test]
    fn file_write_command_dispatches_through_tool_registry_v2() {
        let state_dir = temp_dir("file-cli-state");
        let repo = temp_dir("file-cli-repo");
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "file",
            "write",
            "--repo",
            repo.to_str().expect("repo"),
            "src/hello.txt",
            "--content",
            "hello",
        ])
        .expect("file write command");

        assert_eq!(output.json["kind"], json!("fileToolDispatch"));
        assert_eq!(output.json["tool"], json!("write"));
        assert_eq!(
            output.json["output"]["fileReservation"]["kind"],
            json!("file_reservation")
        );
        assert_eq!(
            fs::read_to_string(repo.join("src/hello.txt")).expect("written file"),
            "hello"
        );
    }

    #[test]
    fn file_move_and_delete_dispatch_through_tool_registry_v2() {
        let state_dir = temp_dir("file-cli-state");
        let repo = temp_dir("file-cli-repo");
        fs::create_dir_all(repo.join("src")).expect("src dir");
        fs::write(repo.join("src/old.txt"), "hello").expect("seed file");

        let moved = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "file",
            "move",
            "--repo",
            repo.to_str().expect("repo"),
            "src/old.txt",
            "src/new.txt",
        ])
        .expect("file move command");
        assert_eq!(moved.json["kind"], json!("fileToolDispatch"));
        assert_eq!(moved.json["tool"], json!("move"));
        assert!(!repo.join("src/old.txt").exists());
        assert_eq!(
            fs::read_to_string(repo.join("src/new.txt")).expect("moved file"),
            "hello"
        );

        let deleted = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "file",
            "delete",
            "--repo",
            repo.to_str().expect("repo"),
            "src/new.txt",
        ])
        .expect("file delete command");
        assert_eq!(deleted.json["kind"], json!("fileToolDispatch"));
        assert_eq!(deleted.json["tool"], json!("delete"));
        assert!(!repo.join("src/new.txt").exists());
    }

    #[test]
    fn file_replace_dispatches_through_tool_registry_v2() {
        let state_dir = temp_dir("file-cli-state");
        let repo = temp_dir("file-cli-repo");
        fs::create_dir_all(repo.join("src")).expect("src dir");
        fs::write(repo.join("src/one.txt"), "alpha beta alpha").expect("seed one");
        fs::write(repo.join("src/two.txt"), "gamma alpha").expect("seed two");

        let preview = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "file",
            "replace",
            "--repo",
            repo.to_str().expect("repo"),
            "src",
            "--search",
            "alpha",
            "--replacement",
            "omega",
            "--dry-run",
            "--max-replacements",
            "2",
        ])
        .expect("file replace dry run");
        assert_eq!(preview.json["kind"], json!("fileToolDispatch"));
        assert_eq!(preview.json["tool"], json!("replace"));
        assert_eq!(preview.json["output"]["dryRun"], json!(true));
        assert_eq!(
            fs::read_to_string(repo.join("src/one.txt")).expect("preview leaves file unchanged"),
            "alpha beta alpha"
        );

        let replaced = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "file",
            "replace",
            "--repo",
            repo.to_str().expect("repo"),
            "src/one.txt",
            "--search",
            "alpha",
            "--replacement",
            "omega",
            "--max-replacements",
            "1",
        ])
        .expect("file replace command");
        assert_eq!(replaced.json["kind"], json!("fileToolDispatch"));
        assert_eq!(replaced.json["tool"], json!("replace"));
        assert_eq!(
            replaced.json["output"]["changedFiles"][0]["fileReservation"]["kind"],
            json!("file_reservation")
        );
        assert_eq!(
            fs::read_to_string(repo.join("src/one.txt")).expect("replaced file"),
            "omega beta alpha"
        );
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
