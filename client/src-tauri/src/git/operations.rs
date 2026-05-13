use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;

use git2::{
    build::CheckoutBuilder, Cred, CredentialType, FetchOptions, IndexAddOption, PushOptions,
    RemoteCallbacks, Repository, Signature,
};

use crate::{
    commands::{
        CommandError, CommandResult, GitCommitResponseDto, GitFetchResponseDto, GitPullResponseDto,
        GitPushResponseDto, GitRemoteRefUpdateDto, GitSignatureDto,
    },
    git::{repository, status},
};

const MAX_REVERT_PATCH_BYTES: usize = 256 * 1024;

pub fn stage_paths(
    expected_project_id: &str,
    paths: &[String],
    registry_path: &Path,
) -> CommandResult<()> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;
    let mut index = repo.index().map_err(|error| {
        CommandError::retryable(
            "git_index_read_failed",
            format!("Xero could not open the repository index: {error}"),
        )
    })?;

    if paths.is_empty() {
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .map_err(|error| {
                CommandError::retryable(
                    "git_index_stage_failed",
                    format!("Xero could not stage all changes: {error}"),
                )
            })?;
    } else {
        for path in paths {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                continue;
            }
            let workdir_path = canonical.root_path.join(trimmed);
            if !workdir_path.exists() {
                index.remove_path(Path::new(trimmed)).map_err(|error| {
                    CommandError::retryable(
                        "git_index_stage_failed",
                        format!("Xero could not stage `{trimmed}`: {error}"),
                    )
                })?;
            } else {
                index.add_path(Path::new(trimmed)).map_err(|error| {
                    CommandError::retryable(
                        "git_index_stage_failed",
                        format!("Xero could not stage `{trimmed}`: {error}"),
                    )
                })?;
            }
        }
    }

    index.write().map_err(|error| {
        CommandError::retryable(
            "git_index_write_failed",
            format!("Xero could not persist the repository index: {error}"),
        )
    })?;

    Ok(())
}

pub fn unstage_paths(
    expected_project_id: &str,
    paths: &[String],
    registry_path: &Path,
) -> CommandResult<()> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let head_obj = head.as_ref().map(|c| c.as_object().clone());

    let target_paths: Vec<&str> = if paths.is_empty() {
        vec!["*"]
    } else {
        paths
            .iter()
            .map(|path| path.as_str())
            .filter(|path| !path.trim().is_empty())
            .collect()
    };

    if target_paths.is_empty() {
        return Ok(());
    }

    if let Some(head_obj) = head_obj {
        repo.reset_default(Some(&head_obj), target_paths.iter().copied())
            .map_err(|error| {
                CommandError::retryable(
                    "git_index_unstage_failed",
                    format!("Xero could not unstage changes: {error}"),
                )
            })?;
    } else {
        let mut index = repo.index().map_err(|error| {
            CommandError::retryable(
                "git_index_read_failed",
                format!("Xero could not open the repository index: {error}"),
            )
        })?;
        for path in &target_paths {
            if *path == "*" {
                index.clear().map_err(|error| {
                    CommandError::retryable(
                        "git_index_unstage_failed",
                        format!("Xero could not unstage all changes: {error}"),
                    )
                })?;
                break;
            }
            let _ = index.remove_path(Path::new(path));
        }
        index.write().map_err(|error| {
            CommandError::retryable(
                "git_index_write_failed",
                format!("Xero could not persist the repository index: {error}"),
            )
        })?;
    }

    Ok(())
}

pub fn discard_changes(
    expected_project_id: &str,
    paths: &[String],
    registry_path: &Path,
) -> CommandResult<()> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    if paths.is_empty() {
        checkout.remove_untracked(false);
    } else {
        for path in paths {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                checkout.path(trimmed);
            }
        }
    }

    repo.checkout_head(Some(&mut checkout)).map_err(|error| {
        CommandError::retryable(
            "git_discard_failed",
            format!("Xero could not discard local changes: {error}"),
        )
    })?;

    Ok(())
}

pub fn revert_patch(
    expected_project_id: &str,
    patch: &str,
    registry_path: &Path,
) -> CommandResult<()> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    revert_patch_at_root(&canonical.root_path, patch)
}

fn revert_patch_at_root(root_path: &Path, patch: &str) -> CommandResult<()> {
    let trimmed = patch.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("patch"));
    }
    if patch.len() > MAX_REVERT_PATCH_BYTES {
        return Err(CommandError::user_fixable(
            "git_revert_patch_too_large",
            "This hunk is too large to revert from the editor.",
        ));
    }
    if !patch.contains("diff --git ") || !patch.contains("\n@@ ") {
        return Err(CommandError::user_fixable(
            "git_revert_patch_invalid",
            "Xero could not recognize this Git hunk patch.",
        ));
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(root_path)
        .args(["apply", "--reverse", "--whitespace=nowarn", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CommandError::retryable(
                "git_revert_patch_failed",
                format!("Xero could not start git apply: {error}"),
            )
        })?;

    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(CommandError::retryable(
                "git_revert_patch_failed",
                "Xero could not open git apply stdin.",
            ));
        };
        stdin.write_all(patch.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "git_revert_patch_failed",
                format!("Xero could not pass the hunk to git apply: {error}"),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|error| {
        CommandError::retryable(
            "git_revert_patch_failed",
            format!("Xero could not finish git apply: {error}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            "Git could not reverse-apply this hunk.".to_string()
        } else {
            stderr
        };
        return Err(CommandError::user_fixable(
            "git_revert_patch_failed",
            detail,
        ));
    }

    Ok(())
}

pub fn commit(
    expected_project_id: &str,
    message: &str,
    registry_path: &Path,
) -> CommandResult<GitCommitResponseDto> {
    let trimmed_message = message.trim();
    if trimmed_message.is_empty() {
        return Err(CommandError::user_fixable(
            "git_commit_message_required",
            "A non-empty commit message is required.",
        ));
    }

    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let signature = resolve_signature(&repo)?;

    let mut index = repo.index().map_err(|error| {
        CommandError::retryable(
            "git_index_read_failed",
            format!("Xero could not open the repository index: {error}"),
        )
    })?;
    let tree_oid = index.write_tree().map_err(|error| {
        CommandError::retryable(
            "git_commit_tree_failed",
            format!("Xero could not write the staged tree: {error}"),
        )
    })?;
    let tree = repo.find_tree(tree_oid).map_err(|error| {
        CommandError::retryable(
            "git_commit_tree_failed",
            format!("Xero could not load the staged tree: {error}"),
        )
    })?;

    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(head) => match head.peel_to_commit() {
            Ok(commit) => vec![commit],
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            trimmed_message,
            &tree,
            &parent_refs,
        )
        .map_err(|error| {
            CommandError::retryable(
                "git_commit_failed",
                format!("Xero could not create the commit: {error}"),
            )
        })?;

    Ok(GitCommitResponseDto {
        sha: oid.to_string(),
        summary: trimmed_message
            .lines()
            .next()
            .unwrap_or(trimmed_message)
            .to_string(),
        signature: GitSignatureDto {
            name: signature.name().unwrap_or_default().to_string(),
            email: signature.email().unwrap_or_default().to_string(),
        },
    })
}

pub fn fetch(
    expected_project_id: &str,
    remote_name: Option<&str>,
    registry_path: &Path,
) -> CommandResult<GitFetchResponseDto> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let remote_name = resolve_remote_name(&repo, remote_name)?;
    let mut remote = repo.find_remote(&remote_name).map_err(|error| {
        CommandError::user_fixable(
            "git_remote_not_found",
            format!("Remote `{remote_name}` is not configured: {error}"),
        )
    })?;

    let mut callbacks = RemoteCallbacks::new();
    install_credential_callback(&mut callbacks);

    let mut options = FetchOptions::new();
    options.remote_callbacks(callbacks);

    let refspecs = remote
        .fetch_refspecs()
        .map(|refs| {
            refs.iter()
                .filter_map(|item| item.map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    remote
        .fetch(&refspecs, Some(&mut options), None)
        .map_err(map_network_error("git_fetch_failed", "fetch"))?;

    Ok(GitFetchResponseDto {
        remote: remote_name,
        refspecs,
    })
}

pub fn pull(
    expected_project_id: &str,
    remote_name: Option<&str>,
    registry_path: &Path,
) -> CommandResult<GitPullResponseDto> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let remote_name = resolve_remote_name(&repo, remote_name)?;
    let head_ref = repo.head().map_err(|error| {
        CommandError::user_fixable(
            "git_pull_no_head",
            format!("Xero could not resolve HEAD: {error}"),
        )
    })?;
    if !head_ref.is_branch() {
        return Err(CommandError::user_fixable(
            "git_pull_detached_head",
            "Cannot pull while HEAD is detached.",
        ));
    }
    let branch_name = head_ref
        .shorthand()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "git_pull_no_branch",
                "Could not determine the current branch name.",
            )
        })?
        .to_string();

    let mut remote = repo.find_remote(&remote_name).map_err(|error| {
        CommandError::user_fixable(
            "git_remote_not_found",
            format!("Remote `{remote_name}` is not configured: {error}"),
        )
    })?;

    let mut callbacks = RemoteCallbacks::new();
    install_credential_callback(&mut callbacks);
    let mut options = FetchOptions::new();
    options.remote_callbacks(callbacks);

    remote
        .fetch(&[branch_name.as_str()], Some(&mut options), None)
        .map_err(map_network_error("git_pull_failed", "pull"))?;

    let fetch_head = repo.find_reference("FETCH_HEAD").map_err(|error| {
        CommandError::retryable(
            "git_pull_failed",
            format!("Xero could not read FETCH_HEAD after fetching: {error}"),
        )
    })?;
    let fetch_commit = fetch_head.peel_to_commit().map_err(|error| {
        CommandError::retryable(
            "git_pull_failed",
            format!("Xero could not resolve the fetched commit: {error}"),
        )
    })?;
    let fetch_annotated = repo
        .reference_to_annotated_commit(&fetch_head)
        .map_err(|error| {
            CommandError::retryable(
                "git_pull_failed",
                format!("Xero could not annotate the fetched commit: {error}"),
            )
        })?;

    let (analysis, _) = repo.merge_analysis(&[&fetch_annotated]).map_err(|error| {
        CommandError::retryable(
            "git_pull_failed",
            format!("Xero could not analyse the merge: {error}"),
        )
    })?;

    let mut updated = false;
    let mut summary = "already up to date".to_string();
    let mut new_head_sha: Option<String> = None;

    if analysis.is_up_to_date() {
        // No changes — leave summary as default.
    } else if analysis.is_fast_forward() {
        let refname = format!("refs/heads/{branch_name}");
        let mut reference = repo.find_reference(&refname).map_err(|error| {
            CommandError::retryable(
                "git_pull_failed",
                format!("Xero could not resolve `{refname}`: {error}"),
            )
        })?;
        reference
            .set_target(fetch_commit.id(), "fast-forward")
            .map_err(|error| {
                CommandError::retryable(
                    "git_pull_failed",
                    format!("Xero could not advance `{refname}`: {error}"),
                )
            })?;
        repo.set_head(&refname).map_err(|error| {
            CommandError::retryable(
                "git_pull_failed",
                format!("Xero could not update HEAD: {error}"),
            )
        })?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout)).map_err(|error| {
            CommandError::retryable(
                "git_pull_failed",
                format!("Xero could not checkout the fast-forwarded tree: {error}"),
            )
        })?;
        updated = true;
        new_head_sha = Some(fetch_commit.id().to_string());
        summary = "fast-forwarded".to_string();
    } else {
        return Err(CommandError::user_fixable(
            "git_pull_merge_required",
            "Pull would require a merge — resolve manually before pulling from Xero.",
        ));
    }

    Ok(GitPullResponseDto {
        remote: remote_name,
        branch: branch_name,
        updated,
        summary,
        new_head_sha,
    })
}

pub fn push(
    expected_project_id: &str,
    remote_name: Option<&str>,
    registry_path: &Path,
) -> CommandResult<GitPushResponseDto> {
    let canonical = status::resolve_project_repository(expected_project_id, registry_path)?;
    let repo = repository::open_repository_root(&canonical.root_path)?.repository;

    let remote_name = resolve_remote_name(&repo, remote_name)?;
    let head_ref = repo.head().map_err(|error| {
        CommandError::user_fixable(
            "git_push_no_head",
            format!("Xero could not resolve HEAD: {error}"),
        )
    })?;
    if !head_ref.is_branch() {
        return Err(CommandError::user_fixable(
            "git_push_detached_head",
            "Cannot push while HEAD is detached.",
        ));
    }
    let branch_name = head_ref
        .shorthand()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "git_push_no_branch",
                "Could not determine the current branch name.",
            )
        })?
        .to_string();

    let mut remote = repo.find_remote(&remote_name).map_err(|error| {
        CommandError::user_fixable(
            "git_remote_not_found",
            format!("Remote `{remote_name}` is not configured: {error}"),
        )
    })?;

    let mut callbacks = RemoteCallbacks::new();
    install_credential_callback(&mut callbacks);

    let updates: Rc<RefCell<Vec<GitRemoteRefUpdateDto>>> = Rc::new(RefCell::new(Vec::new()));
    let updates_for_cb = Rc::clone(&updates);
    callbacks.push_update_reference(move |refname, status_msg| {
        updates_for_cb.borrow_mut().push(GitRemoteRefUpdateDto {
            ref_name: refname.to_string(),
            ok: status_msg.is_none(),
            message: status_msg.map(|s| s.to_string()),
        });
        Ok(())
    });

    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks);

    let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .map_err(map_network_error("git_push_failed", "push"))?;

    let updates = Rc::try_unwrap(updates)
        .map(|cell| cell.into_inner())
        .unwrap_or_else(|rc| rc.borrow().clone());
    let rejected = updates.iter().any(|item| !item.ok);
    if rejected {
        let detail = updates
            .iter()
            .filter(|item| !item.ok)
            .map(|item| {
                format!(
                    "{}: {}",
                    item.ref_name,
                    item.message.clone().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CommandError::user_fixable(
            "git_push_rejected",
            format!("Push was rejected by the remote: {detail}"),
        ));
    }

    Ok(GitPushResponseDto {
        remote: remote_name,
        branch: branch_name,
        updates,
    })
}

fn resolve_remote_name(repo: &Repository, requested: Option<&str>) -> CommandResult<String> {
    if let Some(name) = requested {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let remotes = repo.remotes().map_err(|error| {
        CommandError::user_fixable(
            "git_remote_lookup_failed",
            format!("Xero could not list remotes: {error}"),
        )
    })?;

    let names: Vec<String> = remotes
        .iter()
        .filter_map(|r| r.map(|s| s.to_string()))
        .collect();

    if names.is_empty() {
        return Err(CommandError::user_fixable(
            "git_no_remote_configured",
            "This repository has no remotes configured.",
        ));
    }

    if let Some(origin) = names.iter().find(|n| n.as_str() == "origin") {
        return Ok(origin.clone());
    }

    Ok(names.into_iter().next().unwrap())
}

fn install_credential_callback(callbacks: &mut RemoteCallbacks<'_>) {
    callbacks.credentials(|url, username_from_url, allowed_types| {
        if allowed_types.contains(CredentialType::SSH_KEY) {
            if let Some(user) = username_from_url {
                if let Ok(cred) = Cred::ssh_key_from_agent(user) {
                    return Ok(cred);
                }
            }
        }

        if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT) {
            if let Ok(cred) =
                Cred::credential_helper(&git2::Config::open_default()?, url, username_from_url)
            {
                return Ok(cred);
            }
        }

        if allowed_types.contains(CredentialType::DEFAULT) {
            if let Ok(cred) = Cred::default() {
                return Ok(cred);
            }
        }

        Err(git2::Error::from_str(
            "no usable git credentials are available — configure SSH agent or a credential helper",
        ))
    });
}

fn map_network_error(
    code: &'static str,
    op: &'static str,
) -> impl FnOnce(git2::Error) -> CommandError {
    move |error| {
        let class = error.class();
        let message = error.message();
        if matches!(
            class,
            git2::ErrorClass::Net | git2::ErrorClass::Http | git2::ErrorClass::Ssh
        ) || matches!(error.code(), git2::ErrorCode::Auth)
        {
            CommandError::user_fixable(code, format!("Xero could not {op}: {message}"))
        } else {
            CommandError::retryable(code, format!("Xero could not {op}: {message}"))
        }
    }
}

fn resolve_signature(repo: &Repository) -> CommandResult<Signature<'static>> {
    if let Ok(sig) = repo.signature() {
        let name = sig.name().unwrap_or("Xero User").to_string();
        let email = sig.email().unwrap_or("user@xero.local").to_string();
        return Signature::now(&name, &email).map_err(|error| {
            CommandError::system_fault(
                "git_signature_failed",
                format!("Xero could not build a commit signature: {error}"),
            )
        });
    }

    let config = repo.config().map_err(|error| {
        CommandError::system_fault(
            "git_config_failed",
            format!("Xero could not read the git config: {error}"),
        )
    })?;
    let name = config
        .get_string("user.name")
        .unwrap_or_else(|_| "Xero User".to_string());
    let email = config
        .get_string("user.email")
        .unwrap_or_else(|_| "user@xero.local".to_string());

    Signature::now(&name, &email).map_err(|error| {
        CommandError::system_fault(
            "git_signature_failed",
            format!("Xero could not build a commit signature: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn revert_patch_at_root_reverse_applies_a_single_hunk() {
        let temp_dir = tempfile::tempdir().unwrap();
        init_repository(temp_dir.path());
        let path = temp_dir.path().join("file.txt");
        fs::write(&path, "alpha\nBETA\ngamma\n").unwrap();

        let patch = [
            "diff --git a/file.txt b/file.txt",
            "--- a/file.txt",
            "+++ b/file.txt",
            "@@ -1,3 +1,3 @@",
            " alpha",
            "-beta",
            "+BETA",
            " gamma",
            "",
        ]
        .join("\n");

        revert_patch_at_root(temp_dir.path(), &patch).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "alpha\nbeta\ngamma\n");
    }

    fn init_repository(root: &Path) {
        let repository = Repository::init(root).unwrap();
        fs::write(root.join("file.txt"), "alpha\nbeta\ngamma\n").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = Signature::now("Xero Test", "xero@example.test").unwrap();
        repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
    }
}
