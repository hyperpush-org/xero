use super::*;

pub(crate) fn append_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    role: AgentMessageRole,
    content: String,
) -> CommandResult<AgentMessageRecord> {
    project_store::append_agent_message(
        repo_root,
        &NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role,
            content,
            created_at: now_timestamp(),
        },
    )
}

pub(crate) fn append_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    event_kind: AgentRunEventKind,
    payload: JsonValue,
) -> CommandResult<AgentEventRecord> {
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_event_serialize_failed",
            format!("Cadence could not serialize owned-agent event payload: {error}"),
        )
    })?;
    let event = project_store::append_agent_event(
        repo_root,
        &NewAgentEventRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            event_kind,
            payload_json,
            created_at: now_timestamp(),
        },
    )?;
    publish_agent_event(event.clone());
    Ok(event)
}

pub(crate) fn touch_agent_run_heartbeat(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<()> {
    project_store::touch_agent_run_heartbeat(repo_root, project_id, run_id, &now_timestamp())
}

pub(crate) fn repo_fingerprint(repo_root: &Path) -> JsonValue {
    match git2::Repository::discover(repo_root) {
        Ok(repository) => {
            let head = repository
                .head()
                .ok()
                .and_then(|head| head.target())
                .map(|oid| oid.to_string());
            let dirty = repository
                .statuses(None)
                .map(|statuses| !statuses.is_empty())
                .unwrap_or(false);
            json!({
                "kind": "git",
                "head": head,
                "dirty": dirty,
            })
        }
        Err(_) => json!({ "kind": "filesystem" }),
    }
}

pub(crate) fn record_file_change_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    old_hash: Option<String>,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    let (operation, path) = match output {
        AutonomousToolOutput::Write(output) => (
            if output.created { "create" } else { "write" },
            output.path.as_str(),
        ),
        AutonomousToolOutput::Edit(output) => ("edit", output.path.as_str()),
        AutonomousToolOutput::Patch(output) => ("patch", output.path.as_str()),
        AutonomousToolOutput::NotebookEdit(output) => ("notebook_edit", output.path.as_str()),
        AutonomousToolOutput::Delete(output) => ("delete", output.path.as_str()),
        AutonomousToolOutput::Rename(output) => ("rename", output.from_path.as_str()),
        AutonomousToolOutput::Mkdir(output) => ("mkdir", output.path.as_str()),
        _ => return Ok(()),
    };

    let new_hash_path = match output {
        AutonomousToolOutput::Rename(output) => output.to_path.as_str(),
        _ => path,
    };
    let new_hash = file_hash_if_present(repo_root, new_hash_path)?;
    let to_path = match output {
        AutonomousToolOutput::Rename(output) => Some(output.to_path.clone()),
        _ => None,
    };
    project_store::append_agent_file_change(
        repo_root,
        &NewAgentFileChangeRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            path: path.into(),
            operation: operation.into(),
            old_hash: old_hash.clone(),
            new_hash: new_hash.clone(),
            created_at: now_timestamp(),
        },
    )?;

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::FileChanged,
        json!({
            "path": path,
            "operation": operation,
            "toPath": to_path,
            "oldHash": old_hash,
            "newHash": new_hash,
        }),
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct AgentRollbackCheckpoint {
    path: String,
    operation: String,
    old_hash: Option<String>,
    old_content_base64: Option<String>,
    old_content_omitted_reason: Option<String>,
    old_content_bytes: Option<u64>,
}

pub(crate) fn rollback_checkpoint_for_request(
    repo_root: &Path,
    request: &AutonomousToolRequest,
    old_hash: Option<&str>,
) -> CommandResult<Option<AgentRollbackCheckpoint>> {
    let (path, operation) = match request {
        AutonomousToolRequest::Edit(request) => (request.path.as_str(), "edit"),
        AutonomousToolRequest::Patch(request) => (request.path.as_str(), "patch"),
        AutonomousToolRequest::NotebookEdit(request) => (request.path.as_str(), "notebook_edit"),
        AutonomousToolRequest::Delete(request) => (request.path.as_str(), "delete"),
        AutonomousToolRequest::Rename(request) => (request.from_path.as_str(), "rename"),
        AutonomousToolRequest::Write(request) if old_hash.is_some() => {
            (request.path.as_str(), "write")
        }
        AutonomousToolRequest::Write(request) => (request.path.as_str(), "create"),
        _ => return Ok(None),
    };
    let Some(path_key) = relative_path_key(path) else {
        return Ok(None);
    };
    let old_content = match old_hash {
        Some(_) => capture_rollback_content(repo_root, &path_key)?,
        None => RollbackContentCapture::NotNeeded,
    };
    let (old_content_base64, old_content_omitted_reason, old_content_bytes) = match old_content {
        RollbackContentCapture::Captured { base64, bytes } => (Some(base64), None, Some(bytes)),
        RollbackContentCapture::Omitted { reason, bytes } => (None, Some(reason), bytes),
        RollbackContentCapture::NotNeeded => (None, None, None),
    };

    Ok(Some(AgentRollbackCheckpoint {
        path: path_key,
        operation: operation.into(),
        old_hash: old_hash.map(ToOwned::to_owned),
        old_content_base64,
        old_content_omitted_reason,
        old_content_bytes,
    }))
}

pub(crate) fn record_rollback_checkpoint(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call_id: &str,
    checkpoint: Option<&AgentRollbackCheckpoint>,
) -> CommandResult<()> {
    let Some(checkpoint) = checkpoint else {
        return Ok(());
    };

    let payload_json = serde_json::to_string(&json!({
        "kind": "file_rollback",
        "toolCallId": tool_call_id,
        "path": checkpoint.path.clone(),
        "operation": checkpoint.operation.clone(),
        "oldHash": checkpoint.old_hash.clone(),
        "oldContentBase64": checkpoint.old_content_base64.clone(),
        "oldContentOmittedReason": checkpoint.old_content_omitted_reason.clone(),
        "oldContentBytes": checkpoint.old_content_bytes,
    }))
    .map_err(|error| {
        CommandError::system_fault(
            "agent_checkpoint_payload_serialize_failed",
            format!("Cadence could not serialize owned-agent rollback checkpoint: {error}"),
        )
    })?;

    project_store::append_agent_checkpoint(
        repo_root,
        &NewAgentCheckpointRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            checkpoint_kind: "tool".into(),
            summary: format!("Rollback data for `{}`.", checkpoint.path),
            payload_json: Some(payload_json),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RollbackContentCapture {
    Captured { base64: String, bytes: u64 },
    Omitted { reason: String, bytes: Option<u64> },
    NotNeeded,
}

fn capture_rollback_content(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<RollbackContentCapture> {
    use base64::Engine as _;

    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Err(CommandError::new(
            "agent_file_path_invalid",
            CommandErrorClass::PolicyDenied,
            format!(
                "Cadence refused to capture rollback data for `{repo_relative_path}` because it is not a safe repo-relative path."
            ),
            false,
        ));
    };
    let path = repo_root.join(relative_path);
    if is_sensitive_rollback_path(repo_relative_path) {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_path".into(),
            bytes: fs::metadata(&path).ok().map(|metadata| metadata.len()),
        });
    }
    let metadata = fs::metadata(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Cadence could not inspect rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_ROLLBACK_CONTENT_BYTES {
        return Ok(RollbackContentCapture::Omitted {
            reason: "file_too_large".into(),
            bytes: Some(metadata.len()),
        });
    }
    let bytes = fs::read(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Cadence could not capture rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    if find_prohibited_persistence_content(&text).is_some() {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_content".into(),
            bytes: Some(bytes.len() as u64),
        });
    }
    Ok(RollbackContentCapture::Captured {
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn is_sensitive_rollback_path(repo_relative_path: &str) -> bool {
    let normalized = repo_relative_path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(normalized.as_str());

    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "service-account.json"
        )
        || normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
        || normalized.contains("/.gnupg/")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".key")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
}

pub(crate) fn record_command_output_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    match output {
        AutonomousToolOutput::Command(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "stdout": output.stdout.clone(),
                    "stderr": output.stderr.clone(),
                    "stdoutTruncated": output.stdout_truncated,
                    "stderrTruncated": output.stderr_truncated,
                    "stdoutRedacted": output.stdout_redacted,
                    "stderrRedacted": output.stderr_redacted,
                    "exitCode": output.exit_code,
                    "timedOut": output.timed_out,
                    "spawned": output.spawned,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                record_command_action_required(
                    repo_root,
                    project_id,
                    run_id,
                    "command",
                    &argv,
                    &output.policy.reason,
                    &output.policy.code,
                )?;
            }
        }
        AutonomousToolOutput::CommandSession(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.operation.clone(),
                    "sessionId": output.session_id.clone(),
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "running": output.running,
                    "exitCode": output.exit_code,
                    "spawned": output.spawned,
                    "chunks": output.chunks.clone(),
                    "nextSequence": output.next_sequence,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                if let Some(policy) = output.policy.as_ref() {
                    record_command_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        "command_session_start",
                        &argv,
                        &policy.reason,
                        &policy.code,
                    )?;
                }
            }
        }
        AutonomousToolOutput::ProcessManager(output) => {
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.action.clone(),
                    "processId": output.process_id.clone(),
                    "spawned": output.spawned,
                    "processes": output.processes.clone(),
                    "chunks": output.chunks.clone(),
                    "nextCursor": output.next_cursor,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned
                && matches!(
                    output.action,
                    AutonomousProcessManagerAction::Start
                        | AutonomousProcessManagerAction::AsyncStart
                )
            {
                if let Some(process) = output.processes.first() {
                    let argv = redact_command_argv_for_persistence(&process.command.argv);
                    record_command_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        "process_manager",
                        &argv,
                        &output.policy.reason,
                        &output.policy.code,
                    )?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn record_command_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_name: &str,
    argv: &[String],
    reason: &str,
    code: &str,
) -> CommandResult<()> {
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &format!("command-{}", argv.join("-")),
        "command_approval",
        "Command requires review",
        reason,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "reason": reason,
            "code": code,
            "toolName": tool_name,
            "argv": argv,
        }),
    )?;
    Ok(())
}

pub(crate) fn record_action_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    action_type: &str,
    title: &str,
    detail: &str,
) -> CommandResult<()> {
    project_store::append_agent_action_request(
        repo_root,
        &NewAgentActionRequestRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            action_id: sanitize_action_id(action_id),
            action_type: action_type.into(),
            title: title.into(),
            detail: detail.into(),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

pub(crate) fn sanitize_action_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug, Default)]
pub(crate) struct AgentWorkspaceGuard {
    observed_hashes: BTreeMap<String, Option<String>>,
}

impl AgentWorkspaceGuard {
    pub(crate) fn validate_write_intent(
        &self,
        repo_root: &Path,
        request: &AutonomousToolRequest,
        approved_existing_write: bool,
    ) -> CommandResult<Option<String>> {
        let Some(path) = planned_file_change_path(request) else {
            return Ok(None);
        };
        let Some(path_key) = relative_path_key(path) else {
            return Err(CommandError::new(
                "agent_file_path_invalid",
                CommandErrorClass::PolicyDenied,
                format!("Cadence refused to modify `{path}` because it is not a safe repo-relative path."),
                false,
            ));
        };

        let current_hash = file_hash_if_present(repo_root, &path_key)?;
        if approved_existing_write {
            return Ok(current_hash);
        }
        match (&current_hash, self.observed_hashes.get(&path_key)) {
            (None, _) => Ok(None),
            (Some(_), None) => Err(CommandError::new(
                "agent_file_write_requires_observation",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Cadence refused to modify `{path_key}` because the owned agent has not read this existing file during the run."
                ),
                false,
            )),
            (Some(current_hash), Some(observed_hash)) if observed_hash.as_ref() == Some(current_hash) => {
                Ok(Some(current_hash.clone()))
            }
            (Some(_), Some(observed_hash)) => Err(CommandError::new(
                "agent_file_changed_since_observed",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Cadence refused to modify `{path_key}` because the file changed after the owned agent last observed it (last observed hash: {}).",
                    observed_hash
                        .as_deref()
                        .unwrap_or("absent")
                ),
                false,
            )),
        }
    }

    pub(crate) fn record_tool_output(
        &mut self,
        repo_root: &Path,
        output: &AutonomousToolOutput,
    ) -> CommandResult<()> {
        let path = match output {
            AutonomousToolOutput::Read(output) => output.path.as_str(),
            AutonomousToolOutput::Edit(output) => output.path.as_str(),
            AutonomousToolOutput::Write(output) => output.path.as_str(),
            AutonomousToolOutput::Patch(output) => output.path.as_str(),
            AutonomousToolOutput::NotebookEdit(output) => output.path.as_str(),
            AutonomousToolOutput::Delete(output) => output.path.as_str(),
            AutonomousToolOutput::Rename(output) => output.from_path.as_str(),
            AutonomousToolOutput::Hash(output) => output.path.as_str(),
            _ => return Ok(()),
        };
        let Some(path_key) = relative_path_key(path) else {
            return Ok(());
        };
        let hash = file_hash_if_present(repo_root, &path_key)?;
        self.observed_hashes.insert(path_key, hash);
        if let AutonomousToolOutput::Rename(output) = output {
            if let Some(to_path_key) = relative_path_key(&output.to_path) {
                let hash = file_hash_if_present(repo_root, &to_path_key)?;
                self.observed_hashes.insert(to_path_key, hash);
            }
        }
        Ok(())
    }
}

fn planned_file_change_path(request: &AutonomousToolRequest) -> Option<&str> {
    match request {
        AutonomousToolRequest::Edit(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Write(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Patch(request) => Some(request.path.as_str()),
        AutonomousToolRequest::NotebookEdit(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Delete(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Rename(request) => Some(request.from_path.as_str()),
        _ => None,
    }
}

fn relative_path_key(value: &str) -> Option<String> {
    let relative = safe_relative_path(value)?;
    Some(
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(segment) => segment.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn file_hash_if_present(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<Option<String>> {
    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Ok(None);
    };
    let path = repo_root.join(relative_path);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(CommandError::retryable(
            "agent_file_hash_read_failed",
            format!(
                "Cadence could not hash owned-agent file change target {}: {error}",
                path.display()
            ),
        )),
    }
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir => {}
            _ => return None,
        }
    }

    (!sanitized.as_os_str().is_empty()).then_some(sanitized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

pub(crate) fn validate_prompt(prompt: &str) -> CommandResult<()> {
    if prompt.trim().is_empty() {
        return Err(CommandError::invalid_request("prompt"));
    }
    Ok(())
}
