use std::{fs, path::PathBuf, process::Command};

const APPROVED_COMMAND: &str = "git";
const APPROVED_GIT_SUBCOMMAND: &str = "status";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkflowCommandPolicyViolation {
    pub code: &'static str,
    pub message: String,
}

pub(super) fn validate_workflow_command_policy(
    command: &str,
    args: &[String],
) -> Result<(), WorkflowCommandPolicyViolation> {
    #[cfg(test)]
    if command == "/bin/sh" {
        return Ok(());
    }

    if command != APPROVED_COMMAND {
        return Err(WorkflowCommandPolicyViolation {
            code: "workflow_command_not_allowed_by_app_policy",
            message: format!(
                "Workflow command `{command}` is not approved by Xero's command policy. Command nodes currently support only a constrained `git status` operation."
            ),
        });
    }

    validate_git_status_arguments(args)
}

fn validate_git_status_arguments(args: &[String]) -> Result<(), WorkflowCommandPolicyViolation> {
    if args.first().map(String::as_str) != Some(APPROVED_GIT_SUBCOMMAND) {
        return Err(arguments_denied(
            "Workflow command nodes currently support only the `git status` subcommand.",
        ));
    }

    let mut after_path_separator = false;
    for argument in &args[1..] {
        if argument.contains('\0') {
            return Err(arguments_denied(
                "Workflow command arguments cannot contain NUL bytes.",
            ));
        }
        if after_path_separator {
            validate_repo_relative_pathspec(argument)?;
            continue;
        }
        if argument == "--" {
            after_path_separator = true;
            continue;
        }
        if git_status_option_is_approved(argument) {
            continue;
        }
        return Err(arguments_denied(format!(
            "Workflow `git status` argument `{argument}` is outside Xero's read-only command policy. Put repo-relative pathspecs after `--`."
        )));
    }
    Ok(())
}

fn git_status_option_is_approved(argument: &str) -> bool {
    matches!(
        argument,
        "--short"
            | "-s"
            | "--porcelain"
            | "--porcelain=v1"
            | "--untracked-files=no"
            | "--untracked-files=normal"
            | "--untracked-files=all"
            | "-uno"
            | "-unormal"
            | "-uall"
            | "-z"
            | "--null"
    )
}

fn validate_repo_relative_pathspec(argument: &str) -> Result<(), WorkflowCommandPolicyViolation> {
    use std::path::{Component, Path};

    if argument.is_empty()
        || argument.starts_with(':')
        || argument.starts_with('/')
        || argument.starts_with('\\')
        || argument.contains('\\')
        || argument.as_bytes().get(1) == Some(&b':')
    {
        return Err(arguments_denied(format!(
            "Workflow command pathspec `{argument}` must be a plain repo-relative path."
        )));
    }
    if Path::new(argument)
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(arguments_denied(format!(
            "Workflow command pathspec `{argument}` cannot traverse outside the project."
        )));
    }
    Ok(())
}

fn arguments_denied(message: impl Into<String>) -> WorkflowCommandPolicyViolation {
    WorkflowCommandPolicyViolation {
        code: "workflow_command_arguments_not_allowed_by_app_policy",
        message: message.into(),
    }
}

pub(super) fn resolve_workflow_command_executable(
    command: &str,
) -> Result<PathBuf, WorkflowCommandPolicyViolation> {
    #[cfg(test)]
    if command == "/bin/sh" {
        return trusted_unix_executable(PathBuf::from(command));
    }

    if command != APPROVED_COMMAND {
        return Err(WorkflowCommandPolicyViolation {
            code: "workflow_command_not_allowed_by_app_policy",
            message: format!(
                "Workflow command `{command}` is not approved by Xero's command policy."
            ),
        });
    }

    #[cfg(unix)]
    {
        for candidate in ["/usr/bin/git", "/bin/git"] {
            if let Ok(executable) = trusted_unix_executable(PathBuf::from(candidate)) {
                return Ok(executable);
            }
        }
    }

    Err(WorkflowCommandPolicyViolation {
        code: "workflow_command_approved_executable_unavailable",
        message: "Xero could not find a trusted system `git` executable for this Workflow command."
            .into(),
    })
}

#[cfg(unix)]
fn trusted_unix_executable(candidate: PathBuf) -> Result<PathBuf, WorkflowCommandPolicyViolation> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let canonical = fs::canonicalize(&candidate).map_err(|_| WorkflowCommandPolicyViolation {
        code: "workflow_command_approved_executable_unavailable",
        message: format!(
            "Xero could not resolve approved executable `{}`.",
            candidate.display()
        ),
    })?;
    let metadata = fs::metadata(&canonical).map_err(|_| WorkflowCommandPolicyViolation {
        code: "workflow_command_approved_executable_unavailable",
        message: format!(
            "Xero could not inspect approved executable `{}`.",
            canonical.display()
        ),
    })?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.permissions().mode() & 0o022 != 0
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err(WorkflowCommandPolicyViolation {
            code: "workflow_command_approved_executable_untrusted",
            message: format!(
                "Approved executable `{}` is not an immutable, root-owned system executable.",
                canonical.display()
            ),
        });
    }
    for ancestor in canonical.ancestors().skip(1) {
        let metadata = fs::metadata(ancestor).map_err(|_| WorkflowCommandPolicyViolation {
            code: "workflow_command_approved_executable_untrusted",
            message: format!(
                "Xero could not verify executable directory `{}`.",
                ancestor.display()
            ),
        })?;
        if metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
            return Err(WorkflowCommandPolicyViolation {
                code: "workflow_command_approved_executable_untrusted",
                message: format!(
                    "Approved executable `{}` is stored beneath a writable or non-system directory.",
                    canonical.display()
                ),
            });
        }
    }
    Ok(canonical)
}

#[cfg(not(unix))]
fn trusted_unix_executable(_candidate: PathBuf) -> Result<PathBuf, WorkflowCommandPolicyViolation> {
    Err(WorkflowCommandPolicyViolation {
        code: "workflow_command_secure_launch_unsupported",
        message: "Secure Workflow command execution is not available on this platform.".into(),
    })
}

pub(super) fn harden_workflow_command_process(
    command: &str,
    repo_root: &std::path::Path,
    process: &mut Command,
) {
    if command != APPROVED_COMMAND {
        return;
    }
    let discovery_ceiling = repo_root.parent().unwrap_or(repo_root);
    process
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_DISCOVERY_ACROSS_FILESYSTEM", "0")
        .env("GIT_CEILING_DIRECTORIES", discovery_ceiling)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ]);
}

pub(super) fn append_workflow_command_arguments(
    command: &str,
    args: &[String],
    process: &mut Command,
) {
    if command == APPROVED_COMMAND {
        process
            .arg(APPROVED_GIT_SUBCOMMAND)
            .arg("--ignore-submodules=all")
            .args(&args[1..]);
    } else {
        process.args(args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_policy_rejects_self_authorized_executables_and_unsafe_git_arguments() {
        assert_eq!(
            validate_workflow_command_policy("npm", &["test".into()])
                .expect_err("definition allowlists cannot authorize npm")
                .code,
            "workflow_command_not_allowed_by_app_policy"
        );
        for args in [
            vec!["push".into()],
            vec!["status".into(), "--".into(), "../outside".into()],
            vec!["status".into(), "--".into(), ":(attr:filter)file".into()],
            vec!["status".into(), "--exec-path=/tmp".into()],
        ] {
            assert_eq!(
                validate_workflow_command_policy("git", &args)
                    .expect_err("unsafe git arguments must be rejected")
                    .code,
                "workflow_command_arguments_not_allowed_by_app_policy"
            );
        }
    }

    #[test]
    fn app_policy_accepts_bounded_read_only_git_status() {
        validate_workflow_command_policy(
            "git",
            &[
                "status".into(),
                "--short".into(),
                "--".into(),
                "client/src".into(),
            ],
        )
        .expect("read-only git status is approved");
    }

    #[test]
    fn process_policy_clears_hostile_git_environment_and_forces_submodule_ignoring() {
        use std::ffi::OsStr;

        let mut process = Command::new("/usr/bin/git");
        process
            .env("GIT_DIR", "/tmp/hostile-git-dir")
            .env("GIT_WORK_TREE", "/tmp/hostile-work-tree")
            .env("SSH_AUTH_SOCK", "/tmp/secret-agent");
        harden_workflow_command_process("git", std::path::Path::new("/repo"), &mut process);
        append_workflow_command_arguments(
            "git",
            &["status".into(), "--short".into()],
            &mut process,
        );

        let explicit_environment = process
            .get_envs()
            .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(!explicit_environment.contains_key(OsStr::new("GIT_DIR")));
        assert!(!explicit_environment.contains_key(OsStr::new("GIT_WORK_TREE")));
        assert!(!explicit_environment.contains_key(OsStr::new("SSH_AUTH_SOCK")));
        assert_eq!(
            explicit_environment
                .get(OsStr::new("GIT_CONFIG_GLOBAL"))
                .and_then(Option::as_deref),
            Some(OsStr::new("/dev/null"))
        );
        let arguments = process
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["status", "--ignore-submodules=all"]));
    }

    #[test]
    fn app_policy_covers_every_approved_status_option() {
        for option in [
            "--short",
            "-s",
            "--porcelain",
            "--porcelain=v1",
            "--untracked-files=no",
            "--untracked-files=normal",
            "--untracked-files=all",
            "-uno",
            "-unormal",
            "-uall",
            "-z",
            "--null",
        ] {
            validate_workflow_command_policy("git", &["status".into(), option.into()])
                .unwrap_or_else(|error| {
                    panic!("approved option {option} failed: {}", error.message)
                });
        }
        validate_workflow_command_policy(
            "git",
            &["status".into(), "--".into(), "./client/src".into()],
        )
        .expect("current-directory path components are safe");
    }

    #[test]
    fn app_policy_rejects_malformed_repo_relative_pathspecs() {
        for pathspec in [
            "",
            ":(glob)src/**",
            "/absolute",
            "\\absolute",
            "dir\\file",
            "C:drive-relative",
            "../outside",
            "client/../../outside",
        ] {
            let error = validate_workflow_command_policy(
                "git",
                &["status".into(), "--".into(), pathspec.into()],
            )
            .expect_err("malformed pathspec must fail");
            assert_eq!(
                error.code, "workflow_command_arguments_not_allowed_by_app_policy",
                "pathspec {pathspec}"
            );
        }

        let error = validate_workflow_command_policy("git", &["status".into(), "bad\0path".into()])
            .expect_err("NUL argument must fail");
        assert!(error.message.contains("NUL"));
    }

    #[test]
    fn executable_policy_resolves_only_trusted_approved_commands() {
        assert_eq!(
            resolve_workflow_command_executable("npm")
                .expect_err("unapproved executable must fail")
                .code,
            "workflow_command_not_allowed_by_app_policy"
        );

        #[cfg(unix)]
        {
            let git = resolve_workflow_command_executable("git").expect("resolve system git");
            assert!(git.is_absolute());
            let shell = resolve_workflow_command_executable("/bin/sh").expect("resolve test shell");
            assert!(shell.is_absolute());
            assert_eq!(
                trusted_unix_executable(PathBuf::from("/definitely/missing/xero-command"))
                    .expect_err("missing executable must fail")
                    .code,
                "workflow_command_approved_executable_unavailable"
            );
        }
    }

    #[test]
    fn process_policy_leaves_test_commands_unhardened_and_appends_arguments_verbatim() {
        use std::ffi::OsStr;

        let mut process = Command::new("/bin/sh");
        process.env("PRESERVE_ME", "yes");
        harden_workflow_command_process("/bin/sh", std::path::Path::new("/repo"), &mut process);
        append_workflow_command_arguments(
            "/bin/sh",
            &["-c".into(), "printf ok".into()],
            &mut process,
        );

        assert_eq!(
            process
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("PRESERVE_ME"))
                .and_then(|(_, value)| value),
            Some(OsStr::new("yes"))
        );
        assert_eq!(
            process
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["-c", "printf ok"]
        );
    }
}
