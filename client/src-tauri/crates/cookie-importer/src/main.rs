use std::process::ExitCode;

use serde::Serialize;

#[derive(Serialize)]
struct OutCookie {
    domain: String,
    path: String,
    secure: bool,
    expires: Option<u64>,
    name: String,
    value: String,
    http_only: bool,
    same_site: i64,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Output {
    Ok { cookies: Vec<OutCookie> },
    Err { message: String },
    Probe { available: bool },
}

fn fetch(source: &str, domains: Option<Vec<String>>) -> rookie::Result<Vec<rookie::enums::Cookie>> {
    match source {
        "chrome" => rookie::chrome(domains),
        "chromium" => rookie::chromium(domains),
        "brave" => rookie::brave(domains),
        "edge" => rookie::edge(domains),
        "opera" => rookie::opera(domains),
        "opera_gx" => rookie::opera_gx(domains),
        "vivaldi" => rookie::vivaldi(domains),
        "arc" => rookie::arc(domains),
        "firefox" => rookie::firefox(domains),
        "librewolf" => rookie::librewolf(domains),
        "zen" => rookie::zen(domains),
        #[cfg(target_os = "macos")]
        "safari" => rookie::safari(domains),
        other => Err(eyre::eyre!("unknown browser source `{other}`")),
    }
}

fn run() -> Output {
    let mut args = std::env::args().skip(1);
    let subcommand = match args.next() {
        Some(s) => s,
        None => {
            return Output::Err {
                message: "usage: xero-cookie-importer <probe|import> <source> [domain ...]".into(),
            }
        }
    };
    let source = match args.next() {
        Some(s) => s,
        None => {
            return Output::Err {
                message: "missing source argument".into(),
            }
        }
    };
    let domains: Vec<String> = args.collect();
    let domains = if domains.is_empty() {
        None
    } else {
        Some(domains)
    };

    match subcommand.as_str() {
        "probe" => Output::Probe {
            // Use a made-up hostname as the filter — rookie only returns matches,
            // so an empty list is fine; what we care about is whether it reached
            // the DB at all.
            available: fetch(&source, Some(vec!["__xero_probe__.invalid".into()])).is_ok(),
        },
        "import" => match fetch(&source, domains) {
            Ok(cookies) => Output::Ok {
                cookies: cookies
                    .into_iter()
                    .map(|c| OutCookie {
                        domain: c.domain,
                        path: c.path,
                        secure: c.secure,
                        expires: c.expires,
                        name: c.name,
                        value: c.value,
                        http_only: c.http_only,
                        same_site: c.same_site,
                    })
                    .collect(),
            },
            Err(error) => Output::Err {
                message: error.to_string(),
            },
        },
        other => Output::Err {
            message: format!("unknown subcommand `{other}`"),
        },
    }
}

fn main() -> ExitCode {
    let output = run();
    let exit = matches!(output, Output::Err { .. });
    let json = serde_json::to_string(&output)
        .unwrap_or_else(|_| "{\"kind\":\"err\",\"message\":\"serialization failed\"}".into());
    println!("{json}");
    if exit {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
