use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use regex::Regex;
use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT, CONTENT_TYPE},
};
use serde_json::{json, Value as JsonValue};

use super::{
    process::apply_sanitized_command_environment,
    repo_scope::{normalize_relative_path, path_to_forward_slash, WalkErrorCodes, WalkState},
    AutonomousCodeDiagnostic, AutonomousCodeIntelAction, AutonomousCodeIntelOutput,
    AutonomousCodeIntelRequest, AutonomousCodeSymbol, AutonomousCommandRequest,
    AutonomousLspAction, AutonomousLspInstallCommand, AutonomousLspInstallSuggestion,
    AutonomousLspOutput, AutonomousLspRequest, AutonomousLspServerStatus, AutonomousMcpAction,
    AutonomousMcpOutput, AutonomousMcpRequest, AutonomousMcpServerSummary,
    AutonomousNotebookEditOutput, AutonomousNotebookEditRequest, AutonomousPowerShellRequest,
    AutonomousSubagentOutput, AutonomousSubagentRequest, AutonomousSubagentTask,
    AutonomousTodoAction, AutonomousTodoItem, AutonomousTodoOutput, AutonomousTodoRequest,
    AutonomousTodoStatus, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AutonomousToolSearchMatch, AutonomousToolSearchOutput, AutonomousToolSearchRequest,
    AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_LSP, AUTONOMOUS_TOOL_MCP,
    AUTONOMOUS_TOOL_NOTEBOOK_EDIT, AUTONOMOUS_TOOL_POWERSHELL, AUTONOMOUS_TOOL_SKILL,
    AUTONOMOUS_TOOL_SUBAGENT, AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_TOOL_SEARCH,
};

use crate::{
    auth::now_timestamp,
    commands::{validate_non_empty, CommandError, CommandResult},
    mcp::{load_mcp_registry_from_path, McpConnectionStatus, McpServerRecord, McpTransport},
};

const DEFAULT_PRIORITY_TOOL_LIMIT: usize = 25;
const MAX_PRIORITY_TOOL_LIMIT: usize = 100;
const DEFAULT_MCP_TIMEOUT_MS: u64 = 5_000;
const MAX_MCP_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_LSP_TIMEOUT_MS: u64 = 3_000;
const MAX_LSP_TIMEOUT_MS: u64 = 15_000;
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

impl AutonomousToolRuntime {
    pub fn tool_search(
        &self,
        request: AutonomousToolSearchRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.query, "query")?;
        let limit = bounded_limit(request.limit, DEFAULT_PRIORITY_TOOL_LIMIT)?;
        let query = request.query.trim().to_ascii_lowercase();
        let mut matches = Vec::new();

        for (tool_name, group, description) in priority_tool_catalog(self.skill_tool_enabled()) {
            let haystack = format!("{tool_name} {group} {description}").to_ascii_lowercase();
            if haystack.contains(&query) {
                matches.push(AutonomousToolSearchMatch {
                    tool_name: (*tool_name).into(),
                    group: (*group).into(),
                    description: (*description).into(),
                });
            }
        }

        let truncated = matches.len() > limit;
        matches.truncate(limit);
        let summary = if truncated {
            format!(
                "Found {} tool match(es) for `{}` (truncated).",
                matches.len(),
                request.query.trim()
            )
        } else {
            format!(
                "Found {} tool match(es) for `{}`.",
                matches.len(),
                request.query.trim()
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_TOOL_SEARCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::ToolSearch(AutonomousToolSearchOutput {
                query: request.query.trim().into(),
                matches,
                truncated,
            }),
        })
    }

    pub fn todo(&self, request: AutonomousTodoRequest) -> CommandResult<AutonomousToolResult> {
        let mut todos = self.todo_items.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_todo_lock_failed",
                "Cadence could not lock the owned-agent todo store.",
            )
        })?;
        let mut changed_item = None;
        let action = request.action;

        match request.action {
            AutonomousTodoAction::List => {}
            AutonomousTodoAction::Upsert => {
                let title = request
                    .title
                    .as_deref()
                    .ok_or_else(|| CommandError::invalid_request("title"))?;
                validate_non_empty(title, "title")?;
                let id = request
                    .id
                    .as_deref()
                    .map(normalize_todo_id)
                    .transpose()?
                    .unwrap_or_else(|| next_todo_id(&todos));
                let item = AutonomousTodoItem {
                    id: id.clone(),
                    title: title.trim().into(),
                    notes: normalize_optional_text(request.notes),
                    status: request.status.unwrap_or(AutonomousTodoStatus::Pending),
                    updated_at: now_timestamp(),
                };
                todos.insert(id, item.clone());
                changed_item = Some(item);
            }
            AutonomousTodoAction::Complete => {
                let id = required_normalized_id(request.id.as_deref(), "id")?;
                let item = todos.get_mut(&id).ok_or_else(|| {
                    CommandError::user_fixable(
                        "autonomous_tool_todo_not_found",
                        format!("Cadence could not find todo `{id}`."),
                    )
                })?;
                item.status = AutonomousTodoStatus::Completed;
                item.updated_at = now_timestamp();
                changed_item = Some(item.clone());
            }
            AutonomousTodoAction::Delete => {
                let id = required_normalized_id(request.id.as_deref(), "id")?;
                changed_item = todos.remove(&id);
            }
            AutonomousTodoAction::Clear => todos.clear(),
        }

        let items = todos.values().cloned().collect::<Vec<_>>();
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_TODO.into(),
            summary: format!(
                "Todo action `{:?}` returned {} item(s).",
                action,
                items.len()
            ),
            command_result: None,
            output: AutonomousToolOutput::Todo(AutonomousTodoOutput {
                action,
                items,
                changed_item,
            }),
        })
    }

    pub fn subagent(
        &self,
        request: AutonomousSubagentRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.prompt, "prompt")?;

        let task = {
            let mut tasks = self.subagent_tasks.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_subagent_lock_failed",
                    "Cadence could not lock the owned-agent subagent task store.",
                )
            })?;
            let subagent_id = next_subagent_id(&tasks);
            let task = AutonomousSubagentTask {
                subagent_id: subagent_id.clone(),
                agent_type: request.agent_type,
                prompt: request.prompt.trim().into(),
                model_id: normalize_optional_text(request.model_id),
                status: if self.subagent_executor.is_some() {
                    "running".into()
                } else {
                    "registered".into()
                },
                created_at: now_timestamp(),
                started_at: self.subagent_executor.as_ref().map(|_| now_timestamp()),
                completed_at: None,
                run_id: None,
                result_summary: None,
            };
            tasks.insert(subagent_id, task.clone());
            task
        };

        let task = if let Some(executor) = &self.subagent_executor {
            match executor.execute_subagent(task.clone()) {
                Ok(mut completed_task) => {
                    if completed_task.status.trim().is_empty() {
                        completed_task.status = "completed".into();
                    }
                    if completed_task.completed_at.is_none() {
                        completed_task.completed_at = Some(now_timestamp());
                    }
                    completed_task
                }
                Err(error) => AutonomousSubagentTask {
                    status: "failed".into(),
                    completed_at: Some(now_timestamp()),
                    result_summary: Some(format!("Subagent execution failed: {}", error.message)),
                    ..task
                },
            }
        } else {
            task
        };

        let active_tasks = {
            let mut tasks = self.subagent_tasks.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_subagent_lock_failed",
                    "Cadence could not lock the owned-agent subagent task store.",
                )
            })?;
            tasks.insert(task.subagent_id.clone(), task.clone());
            tasks.values().cloned().collect::<Vec<_>>()
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_SUBAGENT.into(),
            summary: format!(
                "Subagent task `{}` is {} as {:?}.",
                task.subagent_id, task.status, task.agent_type
            ),
            command_result: None,
            output: AutonomousToolOutput::Subagent(AutonomousSubagentOutput { task, active_tasks }),
        })
    }

    pub fn notebook_edit(
        &self,
        request: AutonomousNotebookEditRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.path, "path")?;
        let relative_path = normalize_relative_path(&request.path, "path")?;
        let display_path = path_to_forward_slash(&relative_path);
        if !display_path.ends_with(".ipynb") {
            return Err(CommandError::user_fixable(
                "autonomous_tool_notebook_extension_invalid",
                "Cadence only edits Jupyter notebooks with the `.ipynb` extension.",
            ));
        }

        let resolved_path = self.resolve_existing_path(&relative_path)?;
        let contents = fs::read_to_string(&resolved_path).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_notebook_read_failed",
                format!(
                    "Cadence could not read notebook {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;
        let mut notebook = serde_json::from_str::<JsonValue>(&contents).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_notebook_decode_failed",
                format!("Cadence could not parse notebook `{display_path}` as JSON: {error}"),
            )
        })?;

        let cells = notebook
            .get_mut("cells")
            .and_then(JsonValue::as_array_mut)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_notebook_cells_missing",
                    "Cadence requires notebook JSON to contain a `cells` array.",
                )
            })?;
        let cell = cells.get_mut(request.cell_index).ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_notebook_cell_not_found",
                format!(
                    "Cadence could not find notebook cell {}.",
                    request.cell_index
                ),
            )
        })?;
        let cell_type = cell
            .get("cell_type")
            .and_then(JsonValue::as_str)
            .unwrap_or("unknown")
            .to_string();
        let source = cell.get_mut("source").ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_notebook_source_missing",
                "Cadence requires the target notebook cell to contain `source`.",
            )
        })?;
        let old_source = notebook_source_to_string(source)?;
        if let Some(expected) = request.expected_source.as_deref() {
            if expected != old_source {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_notebook_expected_source_mismatch",
                    "Cadence refused to edit the notebook cell because expectedSource no longer matches.",
                ));
            }
        }
        let old_source_was_array = source.is_array();
        *source = notebook_source_from_string(&request.replacement_source, old_source_was_array);

        let serialized = serde_json::to_vec_pretty(&notebook).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_notebook_serialize_failed",
                format!("Cadence could not serialize notebook `{display_path}`: {error}"),
            )
        })?;
        fs::write(&resolved_path, serialized).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_notebook_write_failed",
                format!(
                    "Cadence could not write notebook {}: {error}",
                    resolved_path.display()
                ),
            )
        })?;

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_NOTEBOOK_EDIT.into(),
            summary: format!(
                "Edited cell {} in notebook `{display_path}`.",
                request.cell_index
            ),
            command_result: None,
            output: AutonomousToolOutput::NotebookEdit(AutonomousNotebookEditOutput {
                path: display_path,
                cell_index: request.cell_index,
                cell_type,
                old_source_chars: old_source.chars().count(),
                new_source_chars: request.replacement_source.chars().count(),
            }),
        })
    }

    pub fn code_intel(
        &self,
        request: AutonomousCodeIntelRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let limit = bounded_limit(request.limit, DEFAULT_PRIORITY_TOOL_LIMIT)?;
        let scan = self.scan_code_intel_scope(
            request.action,
            request.path.as_deref(),
            request.query.as_deref(),
            limit,
        )?;

        let summary = match request.action {
            AutonomousCodeIntelAction::Symbols => {
                format!(
                    "Code intelligence returned {} symbol(s).",
                    scan.symbols.len()
                )
            }
            AutonomousCodeIntelAction::Diagnostics => {
                format!(
                    "Code intelligence returned {} diagnostic(s).",
                    scan.diagnostics.len()
                )
            }
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_CODE_INTEL.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::CodeIntel(AutonomousCodeIntelOutput {
                action: request.action,
                symbols: scan.symbols,
                diagnostics: scan.diagnostics,
                scanned_files: scan.scanned_files,
                truncated: scan.truncated,
            }),
        })
    }

    pub fn lsp(&self, request: AutonomousLspRequest) -> CommandResult<AutonomousToolResult> {
        let servers = lsp_server_statuses();
        if request.action == AutonomousLspAction::Servers {
            let available = servers.iter().filter(|server| server.available).count();
            return Ok(AutonomousToolResult {
                tool_name: AUTONOMOUS_TOOL_LSP.into(),
                summary: format!(
                    "Listed {} LSP server(s); {} available on PATH.",
                    servers.len(),
                    available
                ),
                command_result: None,
                output: AutonomousToolOutput::Lsp(AutonomousLspOutput {
                    action: request.action,
                    mode: "server_catalog".into(),
                    servers,
                    symbols: Vec::new(),
                    diagnostics: Vec::new(),
                    scanned_files: 0,
                    truncated: false,
                    used_server: None,
                    lsp_error: None,
                    install_suggestion: None,
                }),
            });
        }

        let limit = bounded_limit(request.limit, DEFAULT_PRIORITY_TOOL_LIMIT)?;
        let timeout_ms = normalize_lsp_timeout(request.timeout_ms)?;
        let fallback_action = match request.action {
            AutonomousLspAction::Symbols => AutonomousCodeIntelAction::Symbols,
            AutonomousLspAction::Diagnostics => AutonomousCodeIntelAction::Diagnostics,
            AutonomousLspAction::Servers => unreachable!("handled above"),
        };
        let mut scan = self.scan_code_intel_scope(
            fallback_action,
            request.path.as_deref(),
            request.query.as_deref(),
            limit,
        )?;
        let scope_path = request
            .path
            .as_deref()
            .map(|path| normalize_relative_path(path, "path"))
            .transpose()?
            .map(|path| self.resolve_existing_path(&path))
            .transpose()?;
        let descriptor =
            matching_lsp_descriptor(scope_path.as_deref(), request.server_id.as_deref())?;
        let mut mode = "native_fallback".to_string();
        let mut used_server = descriptor.map(|descriptor| descriptor.server_id.to_string());
        let mut lsp_error = None;
        let install_suggestion = descriptor
            .filter(|descriptor| !lsp_server_available(descriptor))
            .map(lsp_install_suggestion);

        if let (Some(descriptor), Some(scope_path)) = (descriptor, scope_path.as_deref()) {
            if scope_path.is_file() && lsp_server_available(descriptor) {
                match invoke_lsp_server(
                    descriptor,
                    &self.repo_root,
                    scope_path,
                    request.action,
                    request.query.as_deref(),
                    limit,
                    timeout_ms,
                ) {
                    Ok(lsp_scan) => {
                        let has_results = match request.action {
                            AutonomousLspAction::Symbols => !lsp_scan.symbols.is_empty(),
                            AutonomousLspAction::Diagnostics => !lsp_scan.diagnostics.is_empty(),
                            AutonomousLspAction::Servers => false,
                        };
                        if has_results {
                            scan = lsp_scan;
                            mode = "external_lsp".into();
                        } else {
                            mode = "native_fallback_after_empty_lsp".into();
                        }
                    }
                    Err(error) => {
                        mode = "native_fallback_after_lsp_error".into();
                        lsp_error = Some(error.message);
                    }
                }
            } else if scope_path.is_file() {
                mode = "native_fallback_lsp_unavailable".into();
            } else {
                mode = "native_fallback_directory_scope".into();
            }
        } else {
            used_server = None;
        }

        let summary = match request.action {
            AutonomousLspAction::Symbols => {
                format!("LSP returned {} symbol(s) via {mode}.", scan.symbols.len())
            }
            AutonomousLspAction::Diagnostics => {
                format!(
                    "LSP returned {} diagnostic(s) via {mode}.",
                    scan.diagnostics.len()
                )
            }
            AutonomousLspAction::Servers => unreachable!("handled above"),
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_LSP.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Lsp(AutonomousLspOutput {
                action: request.action,
                mode,
                servers,
                symbols: scan.symbols,
                diagnostics: scan.diagnostics,
                scanned_files: scan.scanned_files,
                truncated: scan.truncated,
                used_server,
                lsp_error,
                install_suggestion,
            }),
        })
    }

    fn scan_code_intel_scope(
        &self,
        action: AutonomousCodeIntelAction,
        path: Option<&str>,
        query: Option<&str>,
        limit: usize,
    ) -> CommandResult<CodeIntelScan> {
        let scope = path
            .map(|path| normalize_relative_path(path, "path"))
            .transpose()?;
        let scope_path = match scope.as_ref() {
            Some(path) => self.resolve_existing_path(path)?,
            None => self.repo_root.clone(),
        };
        let mut walk = WalkState::default();
        let mut scan = CodeIntelScan::default();

        match action {
            AutonomousCodeIntelAction::Symbols => {
                let query = query.map(|value| value.trim().to_ascii_lowercase());
                self.walk_scope(
                    &scope_path,
                    WalkErrorCodes {
                        metadata_failed: "autonomous_tool_code_intel_metadata_failed",
                        read_dir_failed: "autonomous_tool_code_intel_read_dir_failed",
                    },
                    &mut walk,
                    &mut |path, walk| {
                        if !looks_like_source_file(path) {
                            return Ok(());
                        }
                        let relative = path_to_forward_slash(&self.repo_relative_path(path)?);
                        let text = match fs::read_to_string(path) {
                            Ok(text) => text,
                            Err(_) => return Ok(()),
                        };
                        for symbol in extract_symbols(&relative, &text)? {
                            let haystack =
                                format!("{} {} {}", symbol.kind, symbol.name, symbol.preview)
                                    .to_ascii_lowercase();
                            if query
                                .as_ref()
                                .is_none_or(|query| haystack.contains(query.as_str()))
                            {
                                scan.symbols.push(symbol);
                                if scan.symbols.len() >= limit {
                                    walk.truncated = true;
                                    break;
                                }
                            }
                        }
                        Ok(())
                    },
                )?;
            }
            AutonomousCodeIntelAction::Diagnostics => {
                self.walk_scope(
                    &scope_path,
                    WalkErrorCodes {
                        metadata_failed: "autonomous_tool_code_intel_metadata_failed",
                        read_dir_failed: "autonomous_tool_code_intel_read_dir_failed",
                    },
                    &mut walk,
                    &mut |path, walk| {
                        if path.extension().and_then(|value| value.to_str()) == Some("json") {
                            let relative = path_to_forward_slash(&self.repo_relative_path(path)?);
                            if let Some(error) = fs::read_to_string(path)
                                .ok()
                                .and_then(|text| serde_json::from_str::<JsonValue>(&text).err())
                            {
                                scan.diagnostics.push(AutonomousCodeDiagnostic {
                                    path: relative,
                                    line: error.line(),
                                    column: error.column(),
                                    severity: "error".into(),
                                    message: error.to_string(),
                                });
                            }
                            if scan.diagnostics.len() >= limit {
                                walk.truncated = true;
                            }
                            return Ok(());
                        }

                        if looks_like_source_file(path) {
                            let relative = path_to_forward_slash(&self.repo_relative_path(path)?);
                            let text = match fs::read_to_string(path) {
                                Ok(text) => text,
                                Err(_) => return Ok(()),
                            };
                            for diagnostic in delimiter_diagnostics(&relative, &text) {
                                scan.diagnostics.push(diagnostic);
                                if scan.diagnostics.len() >= limit {
                                    walk.truncated = true;
                                    break;
                                }
                            }
                        }
                        Ok(())
                    },
                )?;
            }
        }

        scan.scanned_files = walk.scanned_files;
        scan.truncated = walk.truncated;
        Ok(scan)
    }

    pub fn powershell(
        &self,
        request: AutonomousPowerShellRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.powershell_with_approval(request, false)
    }

    pub fn powershell_with_operator_approval(
        &self,
        request: AutonomousPowerShellRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.powershell_with_approval(request, true)
    }

    fn powershell_with_approval(
        &self,
        request: AutonomousPowerShellRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(&request.script, "script")?;
        let executable = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "pwsh"
        };
        let command_request = AutonomousCommandRequest {
            argv: vec![
                executable.into(),
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                request.script,
            ],
            cwd: request.cwd,
            timeout_ms: request.timeout_ms,
        };
        let mut result = if operator_approved {
            self.command_with_operator_approval(command_request)?
        } else {
            self.command(command_request)?
        };
        result.tool_name = AUTONOMOUS_TOOL_POWERSHELL.into();
        result.summary = format!("PowerShell wrapper: {}", result.summary);
        Ok(result)
    }

    pub fn mcp(&self, request: AutonomousMcpRequest) -> CommandResult<AutonomousToolResult> {
        let registry_path = self.mcp_registry_path.as_ref().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_mcp_registry_unavailable",
                "Cadence cannot use MCP tools because no MCP registry path is wired.",
            )
        })?;
        let registry = load_mcp_registry_from_path(registry_path)?;
        let servers = registry
            .servers
            .iter()
            .map(mcp_server_summary)
            .collect::<Vec<_>>();

        match request.action {
            AutonomousMcpAction::ListServers => Ok(AutonomousToolResult {
                tool_name: AUTONOMOUS_TOOL_MCP.into(),
                summary: format!("Listed {} MCP server(s).", servers.len()),
                command_result: None,
                output: AutonomousToolOutput::Mcp(AutonomousMcpOutput {
                    action: AutonomousMcpAction::ListServers,
                    servers,
                    server_id: None,
                    capability_name: None,
                    result: None,
                }),
            }),
            AutonomousMcpAction::ListTools
            | AutonomousMcpAction::ListResources
            | AutonomousMcpAction::ListPrompts
            | AutonomousMcpAction::InvokeTool
            | AutonomousMcpAction::ReadResource
            | AutonomousMcpAction::GetPrompt => {
                let server_id = required_trimmed(request.server_id.as_deref(), "serverId")?;
                let server = connected_mcp_server(&registry.servers, &server_id)?;
                let timeout = normalize_mcp_timeout(request.timeout_ms)?;
                let (method, params, capability_name) = mcp_method_and_params(&request)?;
                let result = invoke_mcp_server(server, method, params, timeout)?;
                Ok(AutonomousToolResult {
                    tool_name: AUTONOMOUS_TOOL_MCP.into(),
                    summary: format!("Invoked MCP `{method}` on server `{}`.", server.id),
                    command_result: None,
                    output: AutonomousToolOutput::Mcp(AutonomousMcpOutput {
                        action: request.action,
                        servers,
                        server_id: Some(server.id.clone()),
                        capability_name,
                        result: Some(result),
                    }),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CodeIntelScan {
    symbols: Vec<AutonomousCodeSymbol>,
    diagnostics: Vec<AutonomousCodeDiagnostic>,
    scanned_files: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Copy)]
struct LspServerDescriptor {
    server_id: &'static str,
    language: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    language_id: &'static str,
    extensions: &'static [&'static str],
    supports_symbols: bool,
    supports_diagnostics: bool,
    bundle_note: &'static str,
}

const LSP_SERVER_DESCRIPTORS: &[LspServerDescriptor] = &[
    LspServerDescriptor {
        server_id: "rust_analyzer",
        language: "Rust",
        command: "rust-analyzer",
        args: &[],
        language_id: "rust",
        extensions: &["rs"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; rust-analyzer is large and should be managed as a signed per-platform sidecar before shipping in the app.",
    },
    LspServerDescriptor {
        server_id: "typescript_language_server",
        language: "TypeScript/JavaScript",
        command: "typescript-language-server",
        args: &["--stdio"],
        language_id: "typescript",
        extensions: &["ts", "tsx", "js", "jsx"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; TypeScript LSP packaging needs the matching npm runtime and version-update policy.",
    },
    LspServerDescriptor {
        server_id: "vscode_json_language_server",
        language: "JSON",
        command: "vscode-json-language-server",
        args: &["--stdio"],
        language_id: "json",
        extensions: &["json"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; JSON support falls back to native parser diagnostics when the server is absent.",
    },
    LspServerDescriptor {
        server_id: "pyright",
        language: "Python",
        command: "pyright-langserver",
        args: &["--stdio"],
        language_id: "python",
        extensions: &["py"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; Pyright ships through npm and should be version-pinned before app bundling.",
    },
    LspServerDescriptor {
        server_id: "gopls",
        language: "Go",
        command: "gopls",
        args: &["serve"],
        language_id: "go",
        extensions: &["go"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; Go tooling is best discovered from the developer environment.",
    },
    LspServerDescriptor {
        server_id: "clangd",
        language: "C/C++",
        command: "clangd",
        args: &[],
        language_id: "cpp",
        extensions: &["c", "cc", "cpp", "h", "hpp"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; clangd bundling would materially increase platform artifact size.",
    },
    LspServerDescriptor {
        server_id: "lua_language_server",
        language: "Lua",
        command: "lua-language-server",
        args: &[],
        language_id: "lua",
        extensions: &["lua"],
        supports_symbols: true,
        supports_diagnostics: true,
        bundle_note: "Not bundled by default; the server is optional and discovered from PATH.",
    },
];

pub(super) fn connected_mcp_server<'a>(
    servers: &'a [McpServerRecord],
    server_id: &str,
) -> CommandResult<&'a McpServerRecord> {
    let server = servers
        .iter()
        .find(|server| server.id == server_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_mcp_server_not_found",
                format!("Cadence could not find MCP server `{server_id}`."),
            )
        })?;
    if server.connection.status != McpConnectionStatus::Connected {
        return Err(CommandError::user_fixable(
            "autonomous_tool_mcp_server_not_connected",
            format!("MCP server `{server_id}` is not connected."),
        ));
    }
    Ok(server)
}

fn lsp_server_statuses() -> Vec<AutonomousLspServerStatus> {
    LSP_SERVER_DESCRIPTORS
        .iter()
        .map(|descriptor| {
            let available = lsp_server_available(descriptor);
            AutonomousLspServerStatus {
                install_suggestion: (!available).then(|| lsp_install_suggestion(descriptor)),
                server_id: descriptor.server_id.into(),
                language: descriptor.language.into(),
                command: descriptor.command.into(),
                args: descriptor
                    .args
                    .iter()
                    .map(|arg| (*arg).to_owned())
                    .collect(),
                available,
                supports_symbols: descriptor.supports_symbols,
                supports_diagnostics: descriptor.supports_diagnostics,
                bundled: false,
                bundle_note: descriptor.bundle_note.into(),
            }
        })
        .collect()
}

fn lsp_install_suggestion(descriptor: &LspServerDescriptor) -> AutonomousLspInstallSuggestion {
    AutonomousLspInstallSuggestion {
        server_id: descriptor.server_id.into(),
        language: descriptor.language.into(),
        reason: format!(
            "`{}` was not found on PATH. Ask the user before running an install command.",
            descriptor.command
        ),
        candidate_commands: lsp_install_commands(descriptor),
    }
}

fn lsp_install_commands(descriptor: &LspServerDescriptor) -> Vec<AutonomousLspInstallCommand> {
    match descriptor.server_id {
        "rust_analyzer" => vec![
            install_command(
                "rustup component",
                &["rustup", "component", "add", "rust-analyzer"],
            ),
            install_command("Homebrew", &["brew", "install", "rust-analyzer"]),
        ],
        "typescript_language_server" => vec![install_command(
            "npm global",
            &[
                "npm",
                "install",
                "-g",
                "typescript",
                "typescript-language-server",
            ],
        )],
        "vscode_json_language_server" => vec![install_command(
            "npm global",
            &["npm", "install", "-g", "vscode-langservers-extracted"],
        )],
        "pyright" => vec![install_command(
            "npm global",
            &["npm", "install", "-g", "pyright"],
        )],
        "gopls" => vec![install_command(
            "go install",
            &["go", "install", "golang.org/x/tools/gopls@latest"],
        )],
        "clangd" => vec![
            install_command("Homebrew", &["brew", "install", "llvm"]),
            install_command("winget", &["winget", "install", "LLVM.LLVM"]),
        ],
        "lua_language_server" => vec![install_command(
            "Homebrew",
            &["brew", "install", "lua-language-server"],
        )],
        _ => Vec::new(),
    }
}

fn install_command(label: &str, argv: &[&str]) -> AutonomousLspInstallCommand {
    AutonomousLspInstallCommand {
        label: label.into(),
        argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn matching_lsp_descriptor(
    path: Option<&Path>,
    server_id: Option<&str>,
) -> CommandResult<Option<&'static LspServerDescriptor>> {
    if let Some(server_id) = server_id {
        validate_non_empty(server_id, "serverId")?;
        return LSP_SERVER_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.server_id == server_id.trim())
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_tool_lsp_server_not_found",
                    format!("Cadence could not find LSP server `{}`.", server_id.trim()),
                )
            })
            .map(Some);
    }

    let Some(path) = path.filter(|path| path.is_file()) else {
        return Ok(None);
    };
    let Some(extension) = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    else {
        return Ok(None);
    };

    Ok(LSP_SERVER_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.extensions.contains(&extension.as_str())))
}

fn lsp_server_available(descriptor: &LspServerDescriptor) -> bool {
    executable_on_path(descriptor.command)
}

fn normalize_lsp_timeout(timeout_ms: Option<u64>) -> CommandResult<u64> {
    let timeout = timeout_ms.unwrap_or(DEFAULT_LSP_TIMEOUT_MS);
    if timeout == 0 || timeout > MAX_LSP_TIMEOUT_MS {
        return Err(CommandError::user_fixable(
            "autonomous_tool_lsp_timeout_invalid",
            format!("Cadence requires LSP timeoutMs to be between 1 and {MAX_LSP_TIMEOUT_MS}."),
        ));
    }
    Ok(timeout)
}

fn executable_on_path(command: &str) -> bool {
    find_executable_on_path(command).is_some()
}

fn find_executable_on_path(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return is_executable_file(command_path).then(|| command_path.to_path_buf());
    }

    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        for candidate in executable_candidates(&directory, command) {
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn executable_candidates(directory: &Path, command: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = vec![directory.join(command)];
        for extension in ["exe", "cmd", "bat"] {
            candidates.push(directory.join(format!("{command}.{extension}")));
        }
        candidates
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![directory.join(command)]
    }
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn priority_tool_catalog(
    skill_tool_enabled: bool,
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut catalog = vec![
        ("read", "core", "Read repo-scoped UTF-8 files."),
        ("search", "core", "Search text in repo-scoped files."),
        ("find", "core", "Find repo-scoped files by glob."),
        ("git_status", "core", "Inspect repository status."),
        ("git_diff", "core", "Inspect repository diffs."),
        ("tool_access", "core", "Request deferred tool groups."),
        ("list", "core", "List repo-scoped files and directories."),
        ("file_hash", "core", "Hash repo-scoped files with SHA-256."),
        ("write", "mutation", "Write repo-scoped UTF-8 files."),
        ("edit", "mutation", "Apply expected-text line edits."),
        ("patch", "mutation", "Patch exact file text."),
        (
            "delete",
            "mutation",
            "Delete repo-scoped files or directories.",
        ),
        ("rename", "mutation", "Rename repo-scoped paths."),
        ("mkdir", "mutation", "Create repo-scoped directories."),
        ("command", "command", "Run a repo-scoped command."),
        (
            "command_session_start",
            "command",
            "Start a long-running repo-scoped command session.",
        ),
        (
            "command_session_read",
            "command",
            "Read command session output.",
        ),
        (
            "command_session_stop",
            "command",
            "Stop a long-running command session.",
        ),
        (
            "process_manager",
            "process_manager",
            "Start, interact with, inspect, read output from, and kill Cadence-owned long-running processes.",
        ),
        (
            "macos_automation",
            "macos",
            "Check macOS permissions, inspect apps/windows, and run approval-gated desktop automation.",
        ),
        (
            "web_search",
            "web",
            "Search the web through configured backend.",
        ),
        ("web_fetch", "web", "Fetch HTTP or HTTPS text content."),
        ("browser", "web", "Drive the in-app browser."),
        ("emulator", "emulator", "Drive mobile emulator automation."),
        (
            "solana_cluster",
            "solana",
            "Inspect and control local or forked Solana clusters.",
        ),
        (
            "solana_logs",
            "solana",
            "Read Solana validator, program, and transaction logs.",
        ),
        (
            "solana_tx",
            "solana",
            "Inspect Solana transaction signatures, statuses, and metadata.",
        ),
        (
            "solana_simulate",
            "solana",
            "Simulate Solana transactions before sending them.",
        ),
        (
            "solana_explain",
            "solana",
            "Explain Solana transactions, errors, and account changes.",
        ),
        (
            "solana_alt",
            "solana",
            "Inspect Solana address lookup table data.",
        ),
        (
            "solana_idl",
            "solana",
            "Inspect Anchor IDLs and generated Solana interface metadata.",
        ),
        (
            "solana_codama",
            "solana",
            "Run Codama schema and client-generation helpers.",
        ),
        (
            "solana_pda",
            "solana",
            "Derive and inspect Solana program-derived addresses.",
        ),
        (
            "solana_program",
            "solana",
            "Inspect Solana program metadata and build state.",
        ),
        (
            "solana_deploy",
            "solana",
            "Run Solana deploy planning and guarded deploy actions.",
        ),
        (
            "solana_upgrade_check",
            "solana",
            "Check Solana upgrade authority and deployment safety.",
        ),
        (
            "solana_squads",
            "solana",
            "Inspect Squads multisig proposals and governance state.",
        ),
        (
            "solana_verified_build",
            "solana",
            "Run verified-build checks for Solana programs.",
        ),
        (
            "solana_audit_static",
            "solana",
            "Run static audit checks for Solana programs.",
        ),
        (
            "solana_audit_external",
            "solana",
            "Run external-reference audit checks for Solana programs.",
        ),
        (
            "solana_audit_fuzz",
            "solana",
            "Run fuzzing-oriented audit checks for Solana programs.",
        ),
        (
            "solana_audit_coverage",
            "solana",
            "Inspect Solana audit and test coverage evidence.",
        ),
        (
            "solana_replay",
            "solana",
            "Replay Solana transactions or ledger events.",
        ),
        (
            "solana_indexer",
            "solana",
            "Inspect and manage Solana indexer state.",
        ),
        (
            "solana_secrets",
            "solana",
            "Inspect Solana secret references without exposing raw values.",
        ),
        (
            "solana_cluster_drift",
            "solana",
            "Detect drift between expected and live Solana cluster state.",
        ),
        (
            "solana_cost",
            "solana",
            "Estimate Solana transaction, account, and runtime costs.",
        ),
        (
            "solana_docs",
            "solana",
            "Search and retrieve Solana documentation guidance.",
        ),
        (
            "mcp",
            "mcp",
            "List and invoke connected MCP tools, resources, and prompts over stdio, HTTP, or SSE.",
        ),
        (
            "subagent",
            "agent_ops",
            "Spawn built-in model-routed subagent tasks.",
        ),
        (
            "todo",
            "agent_ops",
            "Maintain model-visible planning todos.",
        ),
        (
            "tool_search",
            "agent_ops",
            "Search deferred tool capabilities.",
        ),
        (
            "notebook_edit",
            "notebook",
            "Edit Jupyter notebook cell source.",
        ),
        (
            "code_intel",
            "intelligence",
            "Find symbols and JSON diagnostics.",
        ),
        (
            "lsp",
            "intelligence",
            "List language servers and resolve symbols or diagnostics through LSP with native fallback.",
        ),
        (
            "powershell",
            "powershell",
            "Run PowerShell through command policy.",
        ),
    ];
    if skill_tool_enabled {
        catalog.push((
            AUTONOMOUS_TOOL_SKILL,
            "skills",
            "Discover, resolve, install, invoke, reload, or create Cadence skills.",
        ));
    }
    catalog
}

fn bounded_limit(value: Option<usize>, default: usize) -> CommandResult<usize> {
    let limit = value.unwrap_or(default);
    if limit == 0 || limit > MAX_PRIORITY_TOOL_LIMIT {
        return Err(CommandError::user_fixable(
            "autonomous_tool_limit_invalid",
            format!("Cadence requires limit to be between 1 and {MAX_PRIORITY_TOOL_LIMIT}."),
        ));
    }
    Ok(limit)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_todo_id(value: &str) -> CommandResult<String> {
    let id = value.trim();
    validate_non_empty(id, "id")?;
    if id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        Ok(id.into())
    } else {
        Err(CommandError::user_fixable(
            "autonomous_tool_todo_id_invalid",
            "Cadence requires todo ids to contain only letters, numbers, hyphen, underscore, or dot.",
        ))
    }
}

fn required_normalized_id(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    let value = value.ok_or_else(|| CommandError::invalid_request(field))?;
    normalize_todo_id(value)
}

fn next_todo_id(todos: &std::collections::BTreeMap<String, AutonomousTodoItem>) -> String {
    let next = todos
        .keys()
        .filter_map(|key| key.strip_prefix("todo-"))
        .filter_map(|suffix| suffix.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("todo-{next}")
}

fn next_subagent_id(tasks: &std::collections::BTreeMap<String, AutonomousSubagentTask>) -> String {
    let next = tasks
        .keys()
        .filter_map(|key| key.strip_prefix("subagent-"))
        .filter_map(|suffix| suffix.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("subagent-{next}")
}

fn notebook_source_to_string(value: &JsonValue) -> CommandResult<String> {
    match value {
        JsonValue::String(text) => Ok(text.clone()),
        JsonValue::Array(parts) => parts
            .iter()
            .map(|part| {
                part.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    CommandError::user_fixable(
                        "autonomous_tool_notebook_source_invalid",
                        "Cadence requires notebook source arrays to contain only strings.",
                    )
                })
            })
            .collect::<CommandResult<Vec<_>>>()
            .map(|parts| parts.join("")),
        _ => Err(CommandError::user_fixable(
            "autonomous_tool_notebook_source_invalid",
            "Cadence requires notebook source to be a string or string array.",
        )),
    }
}

fn notebook_source_from_string(source: &str, as_array: bool) -> JsonValue {
    if !as_array {
        return JsonValue::String(source.into());
    }
    JsonValue::Array(
        source
            .split_inclusive('\n')
            .map(|line| JsonValue::String(line.into()))
            .collect(),
    )
}

fn looks_like_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "swift"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "hpp"
                | "cs"
                | "php"
                | "rb"
        )
    )
}

fn extract_symbols(path: &str, text: &str) -> CommandResult<Vec<AutonomousCodeSymbol>> {
    let patterns = [
        (
            "function",
            r"\b(fn|function|def)\s+([A-Za-z_][A-Za-z0-9_]*)",
        ),
        (
            "type",
            r"\b(struct|enum|class|interface|type)\s+([A-Za-z_][A-Za-z0-9_]*)",
        ),
        (
            "const",
            r"\b(const|let|var|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
        ),
    ];
    let regexes = patterns
        .iter()
        .map(|(kind, pattern)| {
            Regex::new(pattern)
                .map(|regex| (*kind, regex))
                .map_err(|error| {
                    CommandError::system_fault(
                        "autonomous_tool_code_intel_regex_failed",
                        format!("Cadence could not compile code-intel regex: {error}"),
                    )
                })
        })
        .collect::<CommandResult<Vec<_>>>()?;
    let mut symbols = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        for (kind, regex) in &regexes {
            if let Some(captures) = regex.captures(trimmed) {
                if let Some(name) = captures.get(2) {
                    symbols.push(AutonomousCodeSymbol {
                        path: path.into(),
                        line: line_index + 1,
                        kind: (*kind).into(),
                        name: name.as_str().into(),
                        preview: trimmed.chars().take(160).collect(),
                    });
                    break;
                }
            }
        }
    }
    Ok(symbols)
}

fn delimiter_diagnostics(path: &str, text: &str) -> Vec<AutonomousCodeDiagnostic> {
    let mut stack: Vec<(char, usize, usize)> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (line_index, line) in text.lines().enumerate() {
        for (column_index, character) in line.chars().enumerate() {
            if let Some(quote) = in_string {
                if escaped {
                    escaped = false;
                    continue;
                }
                if character == '\\' {
                    escaped = true;
                    continue;
                }
                if character == quote {
                    in_string = None;
                }
                continue;
            }

            if character == '"' {
                in_string = Some(character);
                continue;
            }

            match character {
                '(' | '[' | '{' => stack.push((character, line_index + 1, column_index + 1)),
                ')' | ']' | '}' => {
                    let Some((opening, _, _)) = stack.pop() else {
                        diagnostics.push(AutonomousCodeDiagnostic {
                            path: path.into(),
                            line: line_index + 1,
                            column: column_index + 1,
                            severity: "error".into(),
                            message: format!("Unmatched closing delimiter `{character}`."),
                        });
                        continue;
                    };
                    if !delimiters_match(opening, character) {
                        diagnostics.push(AutonomousCodeDiagnostic {
                            path: path.into(),
                            line: line_index + 1,
                            column: column_index + 1,
                            severity: "error".into(),
                            message: format!(
                                "Mismatched delimiter `{opening}` closed by `{character}`."
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics.extend(stack.into_iter().rev().map(|(opening, line, column)| {
        AutonomousCodeDiagnostic {
            path: path.into(),
            line,
            column,
            severity: "error".into(),
            message: format!("Unclosed delimiter `{opening}`."),
        }
    }));
    diagnostics
}

fn delimiters_match(opening: char, closing: char) -> bool {
    matches!((opening, closing), ('(', ')') | ('[', ']') | ('{', '}'))
}

fn invoke_lsp_server(
    descriptor: &LspServerDescriptor,
    repo_root: &Path,
    file_path: &Path,
    action: AutonomousLspAction,
    query: Option<&str>,
    limit: usize,
    timeout_ms: u64,
) -> CommandResult<CodeIntelScan> {
    let executable = find_executable_on_path(descriptor.command).ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_tool_lsp_command_not_found",
            format!(
                "Cadence could not find LSP command `{}`.",
                descriptor.command
            ),
        )
    })?;
    let relative_path = path_to_forward_slash(file_path.strip_prefix(repo_root).map_err(|_| {
        CommandError::policy_denied(
            "Cadence denied LSP access outside the imported repository root.",
        )
    })?);
    let target_uri = file_uri(file_path)?;
    let root_uri = file_uri(repo_root)?;
    let text = fs::read_to_string(file_path).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_lsp_read_failed",
            format!("Cadence could not read `{relative_path}` for LSP: {error}"),
        )
    })?;

    let mut process = Command::new(executable);
    process
        .args(descriptor.args)
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_sanitized_command_environment(&mut process);

    let mut child = process.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => CommandError::user_fixable(
            "autonomous_tool_lsp_command_not_found",
            format!(
                "Cadence could not find LSP command `{}`.",
                descriptor.command
            ),
        ),
        _ => CommandError::retryable(
            "autonomous_tool_lsp_spawn_failed",
            format!(
                "Cadence could not launch LSP server `{}`: {error}",
                descriptor.server_id
            ),
        ),
    })?;

    let result = (|| -> CommandResult<CodeIntelScan> {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_lsp_stdin_missing",
                "Cadence could not open stdin for the LSP server.",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_lsp_stdout_missing",
                "Cadence could not open stdout for the LSP server.",
            )
        })?;
        let (message_tx, message_rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = read_next_stdio_lsp_message(&mut reader) {
                if message_tx.send(message).is_err() {
                    return;
                }
            }
        });

        let timeout = Duration::from_millis(timeout_ms);
        write_lsp_message(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": null,
                    "rootUri": root_uri.clone(),
                    "workspaceFolders": [{
                        "uri": root_uri.clone(),
                        "name": repo_root
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("workspace")
                    }],
                    "capabilities": {
                        "textDocument": {
                            "documentSymbol": {
                                "hierarchicalDocumentSymbolSupport": true
                            }
                        }
                    },
                    "clientInfo": {
                        "name": "cadence-owned-agent",
                        "version": "0.1.0"
                    }
                }
            }),
        )?;
        let _ = read_lsp_response(&message_rx, 1, timeout, &mut child)?;
        write_lsp_message(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "method": "initialized",
                "params": {}
            }),
        )?;
        write_lsp_message(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": target_uri.clone(),
                        "languageId": descriptor.language_id,
                        "version": 1,
                        "text": text
                    }
                }
            }),
        )?;

        let scan = match action {
            AutonomousLspAction::Symbols => {
                write_lsp_message(
                    &mut stdin,
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "textDocument/documentSymbol",
                        "params": {
                            "textDocument": {
                                "uri": target_uri.clone()
                            }
                        }
                    }),
                )?;
                read_lsp_symbols_response(
                    &message_rx,
                    repo_root,
                    &relative_path,
                    query,
                    limit,
                    timeout,
                    &mut child,
                )?
            }
            AutonomousLspAction::Diagnostics => {
                read_lsp_diagnostics_notifications(&message_rx, repo_root, limit, timeout)?
            }
            AutonomousLspAction::Servers => unreachable!("server listing does not invoke LSP"),
        };

        let _ = write_lsp_message(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "shutdown",
                "params": null
            }),
        );
        let _ = write_lsp_message(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "method": "exit"
            }),
        );
        Ok(scan)
    })();
    terminate_child(&mut child);
    result
}

fn read_lsp_symbols_response(
    message_rx: &mpsc::Receiver<String>,
    repo_root: &Path,
    relative_path: &str,
    query: Option<&str>,
    limit: usize,
    timeout: Duration,
    child: &mut Child,
) -> CommandResult<CodeIntelScan> {
    let deadline = Instant::now() + timeout;
    let query = query.map(|value| value.trim().to_ascii_lowercase());
    let mut scan = CodeIntelScan {
        scanned_files: 1,
        ..CodeIntelScan::default()
    };

    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            terminate_child(child);
            return Err(CommandError::retryable(
                "autonomous_tool_lsp_timeout",
                "Cadence timed out waiting for LSP document symbols.",
            ));
        };
        let message = recv_lsp_message(message_rx, remaining, child)?;
        let value = decode_lsp_message(&message)?;
        collect_lsp_diagnostics(repo_root, &value, limit, &mut scan)?;
        if value.get("id").and_then(JsonValue::as_i64) != Some(2) {
            continue;
        }
        let result = extract_lsp_json_rpc_result(value, 2)?;
        let mut symbols = Vec::new();
        parse_lsp_symbols(&result, relative_path, repo_root, &mut symbols);
        for symbol in symbols {
            let haystack =
                format!("{} {} {}", symbol.kind, symbol.name, symbol.preview).to_ascii_lowercase();
            if query
                .as_ref()
                .is_none_or(|query| haystack.contains(query.as_str()))
            {
                scan.symbols.push(symbol);
                if scan.symbols.len() >= limit {
                    scan.truncated = true;
                    break;
                }
            }
        }
        return Ok(scan);
    }
}

fn read_lsp_diagnostics_notifications(
    message_rx: &mpsc::Receiver<String>,
    repo_root: &Path,
    limit: usize,
    timeout: Duration,
) -> CommandResult<CodeIntelScan> {
    let deadline = Instant::now() + timeout;
    let mut scan = CodeIntelScan {
        scanned_files: 1,
        ..CodeIntelScan::default()
    };

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let message = match message_rx.recv_timeout(remaining) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(CommandError::retryable(
                    "autonomous_tool_lsp_disconnected",
                    "Cadence lost the LSP server stdout stream.",
                ));
            }
        };
        let value = decode_lsp_message(&message)?;
        collect_lsp_diagnostics(repo_root, &value, limit, &mut scan)?;
        if scan.truncated {
            break;
        }
    }

    Ok(scan)
}

fn recv_lsp_message(
    message_rx: &mpsc::Receiver<String>,
    timeout: Duration,
    child: &mut Child,
) -> CommandResult<String> {
    match message_rx.recv_timeout(timeout) {
        Ok(message) => Ok(message),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_child(child);
            Err(CommandError::retryable(
                "autonomous_tool_lsp_timeout",
                "Cadence timed out waiting for LSP server response.",
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CommandError::retryable(
            "autonomous_tool_lsp_disconnected",
            "Cadence lost the LSP server stdout stream.",
        )),
    }
}

fn decode_lsp_message(message: &str) -> CommandResult<JsonValue> {
    serde_json::from_str::<JsonValue>(message).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_lsp_decode_failed",
            format!("Cadence could not decode LSP JSON-RPC response: {error}"),
        )
    })
}

fn write_lsp_message(stdin: &mut impl Write, value: JsonValue) -> CommandResult<()> {
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_lsp_serialize_failed",
            format!("Cadence could not serialize an LSP request: {error}"),
        )
    })?;
    let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
    stdin.write_all(header.as_bytes()).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_lsp_write_failed",
            format!("Cadence could not write LSP stdio headers: {error}"),
        )
    })?;
    stdin.write_all(&bytes).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_lsp_write_failed",
            format!("Cadence could not write to LSP stdio: {error}"),
        )
    })?;
    stdin.flush().map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_lsp_write_failed",
            format!("Cadence could not flush LSP stdio: {error}"),
        )
    })
}

fn read_lsp_response(
    message_rx: &mpsc::Receiver<String>,
    expected_id: i64,
    timeout: Duration,
    child: &mut Child,
) -> CommandResult<JsonValue> {
    loop {
        let message = recv_lsp_message(message_rx, timeout, child)?;
        let value = decode_lsp_message(&message)?;
        if value.get("id").and_then(JsonValue::as_i64) != Some(expected_id) {
            continue;
        }
        return extract_lsp_json_rpc_result(value, expected_id);
    }
}

fn extract_lsp_json_rpc_result(value: JsonValue, expected_id: i64) -> CommandResult<JsonValue> {
    if value.get("id").and_then(JsonValue::as_i64) != Some(expected_id) {
        return Err(CommandError::retryable(
            "autonomous_tool_lsp_response_id_mismatch",
            format!("LSP response did not match JSON-RPC id {expected_id}."),
        ));
    }
    if let Some(error) = value.get("error") {
        return Err(CommandError::user_fixable(
            "autonomous_tool_lsp_error",
            format!("LSP server returned an error: {error}"),
        ));
    }
    Ok(value.get("result").cloned().unwrap_or(JsonValue::Null))
}

fn read_next_stdio_lsp_message(
    reader: &mut BufReader<impl Read>,
) -> std::io::Result<Option<String>> {
    read_next_stdio_mcp_message(reader)
}

fn parse_lsp_symbols(
    result: &JsonValue,
    default_path: &str,
    repo_root: &Path,
    symbols: &mut Vec<AutonomousCodeSymbol>,
) {
    let Some(items) = result.as_array() else {
        return;
    };
    for item in items {
        if item.get("location").is_some() {
            parse_lsp_symbol_information(item, default_path, repo_root, symbols);
        } else {
            parse_lsp_document_symbol(item, default_path, symbols);
        }
    }
}

fn parse_lsp_document_symbol(
    item: &JsonValue,
    path: &str,
    symbols: &mut Vec<AutonomousCodeSymbol>,
) {
    let Some(name) = item.get("name").and_then(JsonValue::as_str) else {
        return;
    };
    let line = lsp_range_start_line(item).unwrap_or(0) + 1;
    let kind = item
        .get("kind")
        .and_then(JsonValue::as_u64)
        .map(lsp_symbol_kind)
        .unwrap_or("symbol");
    let preview = item
        .get("detail")
        .and_then(JsonValue::as_str)
        .filter(|detail| !detail.trim().is_empty())
        .unwrap_or(name)
        .chars()
        .take(160)
        .collect();
    symbols.push(AutonomousCodeSymbol {
        path: path.into(),
        line: line as usize,
        kind: kind.into(),
        name: name.into(),
        preview,
    });
    if let Some(children) = item.get("children").and_then(JsonValue::as_array) {
        for child in children {
            parse_lsp_document_symbol(child, path, symbols);
        }
    }
}

fn parse_lsp_symbol_information(
    item: &JsonValue,
    default_path: &str,
    repo_root: &Path,
    symbols: &mut Vec<AutonomousCodeSymbol>,
) {
    let Some(name) = item.get("name").and_then(JsonValue::as_str) else {
        return;
    };
    let path = item
        .pointer("/location/uri")
        .and_then(JsonValue::as_str)
        .and_then(|uri| repo_relative_path_from_file_uri(repo_root, uri))
        .unwrap_or_else(|| default_path.to_owned());
    let line = item
        .pointer("/location/range/start/line")
        .and_then(JsonValue::as_u64)
        .unwrap_or(0)
        + 1;
    let kind = item
        .get("kind")
        .and_then(JsonValue::as_u64)
        .map(lsp_symbol_kind)
        .unwrap_or("symbol");
    symbols.push(AutonomousCodeSymbol {
        path,
        line: line as usize,
        kind: kind.into(),
        name: name.into(),
        preview: name.chars().take(160).collect(),
    });
}

fn lsp_range_start_line(item: &JsonValue) -> Option<u64> {
    item.pointer("/selectionRange/start/line")
        .and_then(JsonValue::as_u64)
        .or_else(|| {
            item.pointer("/range/start/line")
                .and_then(JsonValue::as_u64)
        })
}

fn collect_lsp_diagnostics(
    repo_root: &Path,
    value: &JsonValue,
    limit: usize,
    scan: &mut CodeIntelScan,
) -> CommandResult<()> {
    if value.get("method").and_then(JsonValue::as_str) != Some("textDocument/publishDiagnostics") {
        return Ok(());
    }
    let Some(params) = value.get("params") else {
        return Ok(());
    };
    let path = params
        .get("uri")
        .and_then(JsonValue::as_str)
        .and_then(|uri| repo_relative_path_from_file_uri(repo_root, uri));
    let Some(path) = path else {
        return Ok(());
    };
    let Some(items) = params.get("diagnostics").and_then(JsonValue::as_array) else {
        return Ok(());
    };
    for item in items {
        let Some(message) = item.get("message").and_then(JsonValue::as_str) else {
            continue;
        };
        let line = item
            .pointer("/range/start/line")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0)
            + 1;
        let column = item
            .pointer("/range/start/character")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0)
            + 1;
        let severity = item
            .get("severity")
            .and_then(JsonValue::as_u64)
            .map(lsp_diagnostic_severity)
            .unwrap_or("diagnostic");
        scan.diagnostics.push(AutonomousCodeDiagnostic {
            path: path.clone(),
            line: line as usize,
            column: column as usize,
            severity: severity.into(),
            message: message.into(),
        });
        if scan.diagnostics.len() >= limit {
            scan.truncated = true;
            break;
        }
    }
    Ok(())
}

fn lsp_symbol_kind(kind: u64) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "symbol",
    }
}

fn lsp_diagnostic_severity(severity: u64) -> &'static str {
    match severity {
        1 => "error",
        2 => "warning",
        3 => "info",
        4 => "hint",
        _ => "diagnostic",
    }
}

fn file_uri(path: &Path) -> CommandResult<String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_lsp_file_uri_failed",
                format!(
                    "Cadence could not convert `{}` to a file URI.",
                    path.display()
                ),
            )
        })
}

fn repo_relative_path_from_file_uri(repo_root: &Path, uri: &str) -> Option<String> {
    let path = url::Url::parse(uri).ok()?.to_file_path().ok()?;
    let relative = path.strip_prefix(repo_root).ok()?;
    Some(path_to_forward_slash(relative))
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn mcp_server_summary(server: &McpServerRecord) -> AutonomousMcpServerSummary {
    let transport = match &server.transport {
        McpTransport::Stdio { .. } => "stdio",
        McpTransport::Http { .. } => "http",
        McpTransport::Sse { .. } => "sse",
    };
    let status = match &server.connection.status {
        McpConnectionStatus::Connected => "connected",
        McpConnectionStatus::Failed => "failed",
        McpConnectionStatus::Blocked => "blocked",
        McpConnectionStatus::Misconfigured => "misconfigured",
        McpConnectionStatus::Stale => "stale",
    };
    AutonomousMcpServerSummary {
        server_id: server.id.clone(),
        name: server.name.clone(),
        transport: transport.into(),
        status: status.into(),
    }
}

fn required_trimmed(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    let value = value.ok_or_else(|| CommandError::invalid_request(field))?;
    validate_non_empty(value, field)?;
    Ok(value.trim().into())
}

pub(super) fn normalize_mcp_timeout(timeout_ms: Option<u64>) -> CommandResult<u64> {
    let timeout = timeout_ms.unwrap_or(DEFAULT_MCP_TIMEOUT_MS);
    if timeout == 0 || timeout > MAX_MCP_TIMEOUT_MS {
        return Err(CommandError::user_fixable(
            "autonomous_tool_mcp_timeout_invalid",
            format!("Cadence requires MCP timeoutMs to be between 1 and {MAX_MCP_TIMEOUT_MS}."),
        ));
    }
    Ok(timeout)
}

fn mcp_method_and_params(
    request: &AutonomousMcpRequest,
) -> CommandResult<(&'static str, JsonValue, Option<String>)> {
    match request.action {
        AutonomousMcpAction::ListTools => Ok(("tools/list", json!({}), None)),
        AutonomousMcpAction::ListResources => Ok(("resources/list", json!({}), None)),
        AutonomousMcpAction::ListPrompts => Ok(("prompts/list", json!({}), None)),
        AutonomousMcpAction::InvokeTool => {
            let name = required_trimmed(request.name.as_deref(), "name")?;
            Ok((
                "tools/call",
                json!({
                    "name": name,
                    "arguments": request.arguments.clone().unwrap_or_else(|| json!({})),
                }),
                Some(name),
            ))
        }
        AutonomousMcpAction::ReadResource => {
            let uri = required_trimmed(request.uri.as_deref(), "uri")?;
            Ok(("resources/read", json!({ "uri": uri }), Some(uri)))
        }
        AutonomousMcpAction::GetPrompt => {
            let name = required_trimmed(request.name.as_deref(), "name")?;
            Ok((
                "prompts/get",
                json!({
                    "name": name,
                    "arguments": request.arguments.clone().unwrap_or_else(|| json!({})),
                }),
                Some(name),
            ))
        }
        AutonomousMcpAction::ListServers => Err(CommandError::invalid_request("action")),
    }
}

pub(super) fn invoke_mcp_server(
    server: &McpServerRecord,
    method: &str,
    params: JsonValue,
    timeout_ms: u64,
) -> CommandResult<JsonValue> {
    match &server.transport {
        McpTransport::Stdio { .. } => invoke_stdio_mcp(server, method, params, timeout_ms),
        McpTransport::Http { .. } | McpTransport::Sse { .. } => {
            invoke_http_mcp(server, method, params, timeout_ms)
        }
    }
}

fn invoke_stdio_mcp(
    server: &McpServerRecord,
    method: &str,
    params: JsonValue,
    timeout_ms: u64,
) -> CommandResult<JsonValue> {
    let McpTransport::Stdio { command, args } = &server.transport else {
        return Err(CommandError::user_fixable(
            "autonomous_tool_mcp_transport_unsupported",
            format!(
                "Cadence currently invokes MCP capabilities only over stdio; server `{}` uses another transport.",
                server.id
            ),
        ));
    };

    let mut process = Command::new(command);
    process
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_sanitized_command_environment(&mut process);
    if let Some(cwd) = server.cwd.as_deref() {
        process.current_dir(cwd);
    }
    for env_ref in &server.env {
        let value = env::var(&env_ref.from_env).map_err(|_| {
            CommandError::user_fixable(
                "autonomous_tool_mcp_env_missing",
                format!(
                    "Cadence could not invoke MCP server `{}` because environment variable `{}` is missing.",
                    server.id, env_ref.from_env
                ),
            )
        })?;
        process.env(&env_ref.key, value);
    }

    let mut child = process.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => CommandError::user_fixable(
            "autonomous_tool_mcp_command_not_found",
            format!("Cadence could not find MCP command `{command}`."),
        ),
        _ => CommandError::system_fault(
            "autonomous_tool_mcp_spawn_failed",
            format!(
                "Cadence could not launch MCP server `{}`: {error}",
                server.id
            ),
        ),
    })?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        CommandError::system_fault(
            "autonomous_tool_mcp_stdin_missing",
            "Cadence could not open stdin for the MCP server.",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        CommandError::system_fault(
            "autonomous_tool_mcp_stdout_missing",
            "Cadence could not open stdout for the MCP server.",
        )
    })?;
    let (message_tx, message_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(message)) = read_next_stdio_mcp_message(&mut reader) {
            if message_tx.send(message).is_err() {
                return;
            }
        }
    });

    let timeout = Duration::from_millis(timeout_ms);
    write_mcp_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "cadence-owned-agent",
                    "version": "0.1.0"
                }
            }
        }),
    )?;
    let _ = read_mcp_response(&message_rx, 1, timeout, &mut child)?;
    write_mcp_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    )?;
    write_mcp_message(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params,
        }),
    )?;
    let result = read_mcp_response(&message_rx, 2, timeout, &mut child)?;
    let _ = child.kill();
    Ok(result)
}

fn invoke_http_mcp(
    server: &McpServerRecord,
    method: &str,
    params: JsonValue,
    timeout_ms: u64,
) -> CommandResult<JsonValue> {
    let url = match &server.transport {
        McpTransport::Http { url } | McpTransport::Sse { url } => url,
        McpTransport::Stdio { .. } => {
            return Err(CommandError::user_fixable(
                "autonomous_tool_mcp_transport_invalid",
                "Cadence cannot invoke stdio MCP through the HTTP transport helper.",
            ));
        }
    };
    let client = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_mcp_http_client_failed",
                format!("Cadence could not build MCP HTTP client: {error}"),
            )
        })?;
    let timeout = Duration::from_millis(timeout_ms);
    let mut session_id = None;

    let initialize = http_mcp_json_rpc(
        &client,
        url,
        session_id.as_deref(),
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "cadence-owned-agent",
                    "version": "0.1.0"
                }
            }
        }),
        Some(1),
        timeout,
    )?;
    session_id = initialize.session_id;

    let _ = http_mcp_json_rpc(
        &client,
        url,
        session_id.as_deref(),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        None,
        timeout,
    )?;

    let response = http_mcp_json_rpc(
        &client,
        url,
        session_id.as_deref(),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params,
        }),
        Some(2),
        timeout,
    )?;
    response.result.ok_or_else(|| {
        CommandError::retryable(
            "autonomous_tool_mcp_result_missing",
            "MCP HTTP response did not include a result.",
        )
    })
}

#[derive(Debug, Clone)]
struct HttpMcpResponse {
    session_id: Option<String>,
    result: Option<JsonValue>,
}

fn http_mcp_json_rpc(
    client: &Client,
    url: &str,
    session_id: Option<&str>,
    body: JsonValue,
    expected_id: Option<i64>,
    timeout: Duration,
) -> CommandResult<HttpMcpResponse> {
    let mut request = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .body(body.to_string());
    if let Some(session_id) = session_id {
        request = request.header(MCP_SESSION_ID_HEADER, session_id);
    }
    let response = request.send().map_err(|error| {
        if error.is_timeout() {
            CommandError::retryable(
                "autonomous_tool_mcp_timeout",
                format!("Cadence timed out waiting for MCP HTTP response after {timeout:?}."),
            )
        } else {
            CommandError::retryable(
                "autonomous_tool_mcp_http_failed",
                format!("Cadence could not reach MCP HTTP endpoint `{url}`: {error}"),
            )
        }
    })?;
    parse_http_mcp_response(response, expected_id)
}

fn parse_http_mcp_response(
    response: Response,
    expected_id: Option<i64>,
) -> CommandResult<HttpMcpResponse> {
    let status = response.status();
    let session_id = response
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let body = response.text().map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_mcp_http_read_failed",
            format!("Cadence could not read MCP HTTP response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_mcp_http_status",
            format!("MCP HTTP endpoint returned status {status}: {body}"),
        ));
    }
    if expected_id.is_none() && body.trim().is_empty() {
        return Ok(HttpMcpResponse {
            session_id,
            result: None,
        });
    }

    let value = if content_type.contains("text/event-stream") || looks_like_sse_body(&body) {
        parse_mcp_sse_body(&body, expected_id)?
    } else {
        serde_json::from_str::<JsonValue>(&body).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_mcp_decode_failed",
                format!("Cadence could not decode MCP HTTP JSON-RPC response: {error}"),
            )
        })?
    };

    let result = match expected_id {
        Some(expected_id) => Some(extract_json_rpc_result(value, expected_id)?),
        None => None,
    };
    Ok(HttpMcpResponse { session_id, result })
}

fn looks_like_sse_body(body: &str) -> bool {
    body.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("event:") || trimmed.starts_with("data:")
    })
}

fn parse_mcp_sse_body(body: &str, expected_id: Option<i64>) -> CommandResult<JsonValue> {
    let mut fallback = None;
    for block in body.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<JsonValue>(&data).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_mcp_decode_failed",
                format!("Cadence could not decode MCP SSE JSON-RPC event: {error}"),
            )
        })?;
        if expected_id.is_none() || value.get("id").and_then(JsonValue::as_i64) == expected_id {
            return Ok(value);
        }
        fallback = Some(value);
    }
    fallback.ok_or_else(|| {
        CommandError::retryable(
            "autonomous_tool_mcp_sse_event_missing",
            "MCP SSE response did not contain a JSON-RPC event.",
        )
    })
}

fn write_mcp_message(stdin: &mut impl Write, value: JsonValue) -> CommandResult<()> {
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_mcp_serialize_failed",
            format!("Cadence could not serialize an MCP request: {error}"),
        )
    })?;
    let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
    stdin.write_all(header.as_bytes()).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_mcp_write_failed",
            format!("Cadence could not write MCP stdio headers: {error}"),
        )
    })?;
    stdin.write_all(&bytes).map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_mcp_write_failed",
            format!("Cadence could not write to MCP stdio: {error}"),
        )
    })?;
    stdin.flush().map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_mcp_write_failed",
            format!("Cadence could not flush MCP stdio: {error}"),
        )
    })
}

fn read_mcp_response(
    message_rx: &mpsc::Receiver<String>,
    expected_id: i64,
    timeout: Duration,
    child: &mut Child,
) -> CommandResult<JsonValue> {
    loop {
        let message = match message_rx.recv_timeout(timeout) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                return Err(CommandError::retryable(
                    "autonomous_tool_mcp_timeout",
                    "Cadence timed out waiting for MCP server response.",
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(CommandError::retryable(
                    "autonomous_tool_mcp_disconnected",
                    "Cadence lost the MCP server stdout stream.",
                ));
            }
        };
        let value = serde_json::from_str::<JsonValue>(&message).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_mcp_decode_failed",
                format!("Cadence could not decode MCP JSON-RPC response: {error}"),
            )
        })?;
        if value.get("id").and_then(JsonValue::as_i64) != Some(expected_id) {
            continue;
        }
        return extract_json_rpc_result(value, expected_id);
    }
}

fn extract_json_rpc_result(value: JsonValue, expected_id: i64) -> CommandResult<JsonValue> {
    if value.get("id").and_then(JsonValue::as_i64) != Some(expected_id) {
        return Err(CommandError::retryable(
            "autonomous_tool_mcp_response_id_mismatch",
            format!("MCP response did not match JSON-RPC id {expected_id}."),
        ));
    }
    if let Some(error) = value.get("error") {
        return Err(CommandError::user_fixable(
            "autonomous_tool_mcp_error",
            format!("MCP server returned an error: {error}"),
        ));
    }
    value.get("result").cloned().ok_or_else(|| {
        CommandError::retryable(
            "autonomous_tool_mcp_result_missing",
            "MCP server response did not include a result.",
        )
    })
}

fn read_next_stdio_mcp_message(
    reader: &mut BufReader<impl Read>,
) -> std::io::Result<Option<String>> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') {
            return Ok(Some(trimmed.to_string()));
        }

        let mut content_length = parse_content_length_header(trimmed);
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if content_length.is_none() {
                content_length = parse_content_length_header(trimmed);
            }
        }

        if let Some(content_length) = content_length {
            let mut body = vec![0_u8; content_length];
            reader.read_exact(&mut body)?;
            return Ok(Some(String::from_utf8_lossy(&body).into_owned()));
        }
    }
}

fn parse_content_length_header(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_install_suggestion_exposes_reviewable_candidate_argvs() {
        let descriptor = LSP_SERVER_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.server_id == "typescript_language_server")
            .expect("typescript lsp descriptor");
        let suggestion = lsp_install_suggestion(descriptor);

        assert_eq!(suggestion.server_id, "typescript_language_server");
        assert!(suggestion.reason.contains("Ask the user"));
        assert!(suggestion.candidate_commands.iter().any(|command| {
            command.label == "npm global"
                && command.argv.iter().map(String::as_str).collect::<Vec<_>>()
                    == vec![
                        "npm",
                        "install",
                        "-g",
                        "typescript",
                        "typescript-language-server",
                    ]
        }));
    }
}
