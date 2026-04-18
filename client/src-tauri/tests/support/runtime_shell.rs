#![allow(dead_code)]

use cadence_desktop_lib::runtime::{
    resolve_runtime_shell_selection_for_platform, RuntimePlatform, RuntimeShellSelection,
    RuntimeShellSource,
};

const MALFORMED_TEMPLATE_ERROR: &str = "runtime_shell_test_helper_malformed_template";

#[derive(Debug, Clone)]
pub struct RuntimeShellCommand {
    pub platform: RuntimePlatform,
    pub program: String,
    pub args: Vec<String>,
    pub source: RuntimeShellSource,
    pub diagnostic_code: Option<String>,
}

pub fn launch_script(script: impl Into<String>) -> RuntimeShellCommand {
    let script = script.into();
    validate_fragment("script", &script);
    assert!(
        !script.trim().is_empty(),
        "{MALFORMED_TEMPLATE_ERROR}: script template cannot be empty"
    );

    let selection = resolve_current_shell_selection();
    let args = if selection.platform.is_windows() {
        vec!["/Q".into(), "/C".into(), script]
    } else {
        vec!["-c".into(), script]
    };

    RuntimeShellCommand {
        platform: selection.platform,
        program: selection.program,
        args,
        source: selection.source,
        diagnostic_code: selection.diagnostic.map(|diagnostic| diagnostic.code),
    }
}

pub fn script_sleep(seconds: u64) -> String {
    sleep_command(RuntimePlatform::detect(), seconds)
}

pub fn script_print_line(line: &str) -> String {
    validate_fragment("line", line);
    print_line_step(RuntimePlatform::detect(), line)
}

pub fn script_join_steps(steps: &[String]) -> String {
    assert!(
        !steps.is_empty(),
        "{MALFORMED_TEMPLATE_ERROR}: script step list cannot be empty"
    );

    for step in steps {
        validate_fragment("step", step);
        assert!(
            !step.trim().is_empty(),
            "{MALFORMED_TEMPLATE_ERROR}: script step cannot be blank"
        );
    }

    command_join(RuntimePlatform::detect(), steps)
}

pub fn script_exit(code: i32) -> String {
    if RuntimePlatform::detect().is_windows() {
        format!("exit /B {code}")
    } else {
        format!("exit {code}")
    }
}

pub fn script_print_line_and_sleep(line: &str, sleep_seconds: u64) -> String {
    script_print_lines_and_sleep(&[line.to_string()], sleep_seconds)
}

pub fn script_print_lines_and_sleep(lines: &[String], sleep_seconds: u64) -> String {
    build_script(lines, sleep_seconds, |line, platform| {
        print_line_step(platform, line)
    })
}

pub fn script_print_line_then_exit(line: &str, exit_code: i32) -> String {
    validate_fragment("line", line);
    let platform = RuntimePlatform::detect();
    let print = print_line_step(platform, line);
    let exit = if platform.is_windows() {
        format!("exit /B {exit_code}")
    } else {
        format!("exit {exit_code}")
    };
    command_join(platform, &[print, exit])
}

pub fn script_prompt_and_sleep(prompt: &str, sleep_seconds: u64) -> String {
    validate_fragment("prompt", prompt);
    let platform = RuntimePlatform::detect();
    let prompt_step = prompt_step(platform, prompt);
    let sleep_step = sleep_command(platform, sleep_seconds);
    command_join(platform, &[prompt_step, sleep_step])
}

pub fn script_repeat_prompt_and_sleep(
    prompt: &str,
    repetitions: usize,
    sleep_seconds: u64,
) -> String {
    validate_fragment("prompt", prompt);
    assert!(
        repetitions > 0,
        "{MALFORMED_TEMPLATE_ERROR}: prompt repetition count must be >= 1"
    );

    let platform = RuntimePlatform::detect();
    let mut steps = Vec::with_capacity(repetitions + 1);
    for _ in 0..repetitions {
        steps.push(prompt_step(platform, prompt));
    }
    steps.push(sleep_command(platform, sleep_seconds));
    command_join(platform, &steps)
}

pub fn script_prompt_read_echo_and_sleep(
    prompt: &str,
    variable_name: &str,
    output_prefix: &str,
    sleep_seconds: u64,
) -> String {
    validate_fragment("prompt", prompt);
    validate_fragment("output_prefix", output_prefix);
    validate_variable_name(variable_name);

    let platform = RuntimePlatform::detect();
    let read_step = if platform.is_windows() {
        format!(
            "set /p \"{variable_name}={}\"",
            escape_cmd_double_quoted(prompt)
        )
    } else {
        format!(
            "printf '%s' '{}'; read {variable_name}",
            shell_quote(prompt)
        )
    };

    let echo_step = if platform.is_windows() {
        format!(
            "echo({}%{variable_name}%",
            escape_cmd_for_echo(output_prefix)
        )
    } else {
        format!(
            "printf '%s%s\\n' '{}' \"${variable_name}\"",
            shell_quote(output_prefix)
        )
    };

    let sleep_step = sleep_command(platform, sleep_seconds);
    command_join(platform, &[read_step, echo_step, sleep_step])
}

fn resolve_current_shell_selection() -> RuntimeShellSelection {
    let platform = RuntimePlatform::detect();
    let comspec = std::env::var("COMSPEC").ok();
    let explicit_test_shell = std::env::var("CADENCE_TEST_SHELL").ok();

    // Keep test fixtures deterministic by default: shells like zsh source user startup files
    // even in non-interactive mode, which can inject unrelated transcript lines.
    let shell = if platform.is_windows() {
        None
    } else {
        explicit_test_shell
    };

    resolve_runtime_shell_selection_for_platform(platform, shell.as_deref(), comspec.as_deref())
}

fn build_script(
    lines: &[String],
    sleep_seconds: u64,
    line_renderer: impl Fn(&str, RuntimePlatform) -> String,
) -> String {
    let platform = RuntimePlatform::detect();
    let mut steps = Vec::with_capacity(lines.len() + 1);

    for line in lines {
        validate_fragment("line", line);
        steps.push(line_renderer(line, platform));
    }

    steps.push(sleep_command(platform, sleep_seconds));
    command_join(platform, &steps)
}

fn print_line_step(platform: RuntimePlatform, line: &str) -> String {
    if platform.is_windows() {
        format!("echo({}", escape_cmd_for_echo(line))
    } else {
        format!("printf '%s\\n' '{}'", shell_quote(line))
    }
}

fn prompt_step(platform: RuntimePlatform, prompt: &str) -> String {
    if platform.is_windows() {
        format!("<nul set /p \"={}\"", escape_cmd_double_quoted(prompt))
    } else {
        format!("printf '%s' '{}'", shell_quote(prompt))
    }
}

fn sleep_command(platform: RuntimePlatform, seconds: u64) -> String {
    assert!(
        seconds > 0,
        "{MALFORMED_TEMPLATE_ERROR}: sleep duration must be > 0"
    );
    if platform.is_windows() {
        format!("timeout /T {seconds} /NOBREAK > NUL")
    } else {
        format!("sleep {seconds}")
    }
}

fn command_join(platform: RuntimePlatform, steps: &[String]) -> String {
    let separator = if platform.is_windows() { " & " } else { "; " };
    steps.join(separator)
}

fn shell_quote(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn escape_cmd_for_echo(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '^' => escaped.push_str("^^"),
            '&' => escaped.push_str("^&"),
            '|' => escaped.push_str("^|"),
            '<' => escaped.push_str("^<"),
            '>' => escaped.push_str("^>"),
            '%' => escaped.push_str("%%"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_cmd_double_quoted(value: &str) -> String {
    escape_cmd_for_echo(value).replace('"', "^\"")
}

fn validate_fragment(label: &str, value: &str) {
    assert!(
        !value.contains('\0'),
        "{MALFORMED_TEMPLATE_ERROR}: {label} contains NUL"
    );
    assert!(
        !value.contains('\n') && !value.contains('\r'),
        "{MALFORMED_TEMPLATE_ERROR}: {label} must be single-line"
    );
}

fn validate_variable_name(name: &str) {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        panic!("{MALFORMED_TEMPLATE_ERROR}: variable name must not be empty");
    };

    assert!(
        first.is_ascii_alphabetic() || first == '_',
        "{MALFORMED_TEMPLATE_ERROR}: unsupported variable name `{name}`"
    );
    assert!(
        chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
        "{MALFORMED_TEMPLATE_ERROR}: unsupported variable name `{name}`"
    );
}
