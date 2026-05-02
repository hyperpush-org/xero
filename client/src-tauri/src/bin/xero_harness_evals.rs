use std::{env, process};

use xero_desktop_lib::runtime::run_xero_quality_eval_suites;

fn main() {
    let format = env::args()
        .skip_while(|arg| arg != "--format")
        .nth(1)
        .unwrap_or_else(|| "markdown".into());
    let repo_root = env::current_dir().unwrap_or_else(|error| {
        eprintln!("Could not resolve current directory for harness evals: {error}");
        process::exit(2);
    });
    let report = run_xero_quality_eval_suites(&repo_root);

    match format.as_str() {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("serialize eval report")
            );
        }
        "markdown" => {
            println!("{}", report.to_markdown());
        }
        other => {
            eprintln!("Unsupported --format `{other}`. Use `markdown` or `json`.");
            process::exit(2);
        }
    }

    if !report.passed {
        process::exit(1);
    }
}
