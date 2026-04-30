use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use tauri::http;
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use super::{ClusterKind, LogFilter, SolanaState};

pub const URI_SCHEME: &str = "solana";

/// Handles large Solana workbench payloads addressed by stable app-local URLs.
///
/// Supported routes:
/// - `solana://idl/<program-id>[?cluster=devnet]`
/// - `solana://snapshot/<snapshot-id>`
/// - `solana://program/<program-id>/<sha256>.so`
/// - `solana://tx/<signature>/trace[?cluster=localnet]`
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let route = SolanaUriRoute::from_uri(request.uri());
    let app = ctx.app_handle().clone();

    std::thread::spawn(move || {
        let state = match app.try_state::<SolanaState>() {
            Some(state) => state,
            None => {
                responder.respond(not_found("solana state unavailable"));
                return;
            }
        };
        responder.respond(serve_route(state.inner(), route));
    });
}

fn serve_route(state: &SolanaState, route: SolanaUriRoute) -> http::Response<Vec<u8>> {
    match route.segments.as_slice() {
        [kind, program_id] if kind == "idl" => {
            match state.idl_registry().get_cached(program_id, route.cluster) {
                Some(idl) => json_response(&idl.value),
                None => not_found(&format!("cached IDL not found for program {program_id}")),
            }
        }
        [kind, id] if kind == "snapshot" => match state.snapshots().read(id) {
            Ok(manifest) => json_response(&manifest),
            Err(error) => command_error_response(http::StatusCode::NOT_FOUND, &error),
        },
        [kind, program_id, file_name] if kind == "program" => {
            serve_program_archive(program_id, file_name)
        }
        [kind, signature, action] if kind == "tx" && action == "trace" => {
            serve_tx_trace(state, signature, route.cluster)
        }
        _ => not_found(&format!(
            "unknown solana route: {}",
            route.segments.join("/")
        )),
    }
}

fn serve_program_archive(program_id: &str, file_name: &str) -> http::Response<Vec<u8>> {
    let path = match program_archive_path(program_id, file_name) {
        Some(path) => path,
        None => return bad_request("invalid program archive path"),
    };
    match fs::read(&path) {
        Ok(bytes) => http::Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .header(http::header::CACHE_CONTROL, "no-cache, no-store")
            .body(bytes)
            .unwrap_or_else(|_| empty_response(http::StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            not_found(&format!("program archive not found: {}", path.display()))
        }
        Err(err) => status_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "text/plain; charset=utf-8",
            format!("could not read program archive {}: {err}", path.display()).into_bytes(),
        ),
    }
}

fn serve_tx_trace(
    state: &SolanaState,
    signature: &str,
    cluster: Option<ClusterKind>,
) -> http::Response<Vec<u8>> {
    let clusters: Vec<ClusterKind> = match cluster {
        Some(cluster) => vec![cluster],
        None => ClusterKind::ALL.to_vec(),
    };

    for cluster in clusters {
        let filter = LogFilter {
            cluster,
            program_ids: Vec::new(),
            include_decoded: true,
        };
        if let Some(entry) = state
            .log_bus()
            .recent(&filter, 1024)
            .into_iter()
            .find(|entry| entry.signature == signature)
        {
            return json_response(&entry);
        }
    }

    not_found(&format!(
        "transaction trace not found for signature {signature}"
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SolanaUriRoute {
    segments: Vec<String>,
    cluster: Option<ClusterKind>,
}

impl SolanaUriRoute {
    fn from_uri(uri: &http::Uri) -> Self {
        let mut segments = Vec::new();
        if let Some(host) = uri.host() {
            if !host.is_empty() {
                segments.push(host.to_string());
            }
        }
        segments.extend(
            uri.path()
                .split('/')
                .filter(|segment| !segment.is_empty())
                .map(ToOwned::to_owned),
        );

        Self {
            segments,
            cluster: cluster_from_query(uri.query()),
        }
    }
}

fn cluster_from_query(query: Option<&str>) -> Option<ClusterKind> {
    query?
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == "cluster").then_some(value)
        })
        .find_map(cluster_from_str)
}

fn cluster_from_str(value: &str) -> Option<ClusterKind> {
    match value {
        "localnet" => Some(ClusterKind::Localnet),
        "mainnet_fork" | "mainnet-fork" => Some(ClusterKind::MainnetFork),
        "devnet" => Some(ClusterKind::Devnet),
        "mainnet" | "mainnet-beta" | "mainnet_beta" => Some(ClusterKind::Mainnet),
        _ => None,
    }
}

fn program_archive_path(program_id: &str, file_name: &str) -> Option<PathBuf> {
    if !is_safe_segment(program_id) {
        return None;
    }
    let sha = file_name.strip_suffix(".so").unwrap_or(file_name);
    if !is_safe_segment(sha) {
        return None;
    }
    Some(
        default_program_archive_root()
            .join(program_id)
            .join(format!("{sha}.so")),
    )
}

fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains('/')
        && !segment.contains('\\')
}

fn default_program_archive_root() -> PathBuf {
    if let Some(dir) = dirs::data_dir() {
        return dir.join("xero/solana/program-archive");
    }
    std::env::temp_dir().join("xero-solana-program-archive")
}

fn json_response<T: Serialize + ?Sized>(value: &T) -> http::Response<Vec<u8>> {
    match serde_json::to_vec(value) {
        Ok(bytes) => status_response(http::StatusCode::OK, "application/json", bytes),
        Err(err) => status_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "text/plain; charset=utf-8",
            format!("could not serialize response: {err}").into_bytes(),
        ),
    }
}

fn command_error_response(
    status: http::StatusCode,
    error: &crate::commands::CommandError,
) -> http::Response<Vec<u8>> {
    match serde_json::to_vec(error) {
        Ok(bytes) => status_response(status, "application/json", bytes),
        Err(_) => status_response(
            status,
            "text/plain; charset=utf-8",
            error.message.as_bytes().to_vec(),
        ),
    }
}

fn bad_request(message: &str) -> http::Response<Vec<u8>> {
    status_response(
        http::StatusCode::BAD_REQUEST,
        "text/plain; charset=utf-8",
        message.as_bytes().to_vec(),
    )
}

fn not_found(message: &str) -> http::Response<Vec<u8>> {
    status_response(
        http::StatusCode::NOT_FOUND,
        "text/plain; charset=utf-8",
        message.as_bytes().to_vec(),
    )
}

fn status_response(
    status: http::StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .body(body)
        .unwrap_or_else(|_| empty_response(status))
}

fn empty_response(status: http::StatusCode) -> http::Response<Vec<u8>> {
    let mut resp = http::Response::new(Vec::new());
    *resp.status_mut() = status;
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_based_solana_routes() {
        let uri = "solana://idl/Prog111?cluster=devnet"
            .parse::<http::Uri>()
            .unwrap();
        let route = SolanaUriRoute::from_uri(&uri);
        assert_eq!(route.segments, vec!["idl", "Prog111"]);
        assert_eq!(route.cluster, Some(ClusterKind::Devnet));
    }

    #[test]
    fn parses_path_based_solana_routes() {
        let uri = "/tx/Sig111/trace?cluster=mainnet_fork"
            .parse::<http::Uri>()
            .unwrap();
        let route = SolanaUriRoute::from_uri(&uri);
        assert_eq!(route.segments, vec!["tx", "Sig111", "trace"]);
        assert_eq!(route.cluster, Some(ClusterKind::MainnetFork));
    }

    #[test]
    fn rejects_program_archive_traversal() {
        assert!(program_archive_path("Prog111", "../bad.so").is_none());
        assert!(program_archive_path("..", "hash.so").is_none());
    }
}
