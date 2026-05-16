use std::time::Duration;

use rusqlite::Connection;
use serde_json::{json, Value as JsonValue};
use xero_remote_bridge::{
    BridgeConfig, BridgeError, FileIdentityStore, FileSessionVisibilityStore, IdentityStore,
    PhoenixChannelClient, PhoenixSocketKind, RemoteBridge,
};

use super::{
    cli_app_data_root, default_headless_state_dir, response, take_bool_flag, take_help,
    take_option, validate_required_cli, workspace_project_database_path, CliError, CliResponse,
    GlobalOptions, OutputMode,
};

const REMOTE_DIR: &str = "remote";
const IDENTITY_FILE: &str = "desktop-identity.json";
const VISIBILITY_FILE: &str = "remote-visibility.json";
const MOCK_WEB_IDENTITY_FILE: &str = "mock-web-identity.json";

pub(crate) fn dispatch_remote(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("login") => command_remote_login(globals, args[1..].to_vec()),
        Some("logout") => command_remote_logout(globals, args[1..].to_vec()),
        Some("connect") => command_remote_connect(globals, args[1..].to_vec()),
        Some("devices") => command_remote_devices(globals, args[1..].to_vec()),
        Some("visibility") => command_remote_visibility(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            remote_help(),
            json!({ "command": "remote" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown remote command `{other}`. Use login, logout, connect, devices, or visibility."
        ))),
    }
}

pub(crate) fn dispatch_mock_web(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("login") => command_mock_web_login(globals, args[1..].to_vec()),
        Some("logout") => command_mock_web_logout(globals, args[1..].to_vec()),
        Some("devices") => command_mock_web_devices(globals, args[1..].to_vec()),
        Some("connect") => command_mock_web_connect(globals, args[1..].to_vec()),
        Some("attach") => command_mock_web_attach(globals, args[1..].to_vec()),
        Some("send") => command_mock_web_send(globals, args[1..].to_vec()),
        Some("list-sessions") => command_mock_web_list_sessions(globals, args[1..].to_vec()),
        Some("start") => command_mock_web_start(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            mock_web_help(),
            json!({ "command": "mock-web" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown mock-web command `{other}`. Use login, logout, devices, connect, attach, send, list-sessions, or start."
        ))),
    }
}

fn command_mock_web_login(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web login [--poll FLOW_ID]",
            json!({ "command": "mock-web login" }),
        ));
    }
    let poll_flow_id = take_option(&mut args, "--poll")?;
    reject_unknown_remote_options(&args)?;
    let bridge = mock_web_bridge_for_cli(&globals);

    let status = if let Some(flow_id) = poll_flow_id {
        bridge
            .poll_github_login(&flow_id)
            .map_err(map_bridge_error)?
    } else {
        bridge
            .sign_in_with_github_kind("web")
            .map_err(map_bridge_error)?
    };

    let text = if status.signed_in {
        "Mock web GitHub login complete.".to_string()
    } else if let (Some(flow_id), Some(url)) = (&status.flow_id, &status.authorization_url) {
        format!(
            "Open this URL to sign in:\n{url}\n\nThen run: xero mock-web login --poll {flow_id}"
        )
    } else {
        "Mock web GitHub login is still pending.".to_string()
    };

    Ok(response(
        &globals,
        text,
        json!({
            "schema": "xero.mock_web_login.v1",
            "status": status,
        }),
    ))
}

fn command_mock_web_logout(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web logout",
            json!({ "command": "mock-web logout" }),
        ));
    }
    reject_unknown_remote_options(&args)?;
    mock_web_bridge_for_cli(&globals)
        .sign_out()
        .map_err(map_bridge_error)?;
    Ok(response(
        &globals,
        "Mock web GitHub session cleared.",
        json!({ "schema": "xero.mock_web_logout.v1" }),
    ))
}

fn command_mock_web_devices(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web devices [list|revoke DEVICE_ID]",
            json!({ "command": "mock-web devices" }),
        ));
    }

    match args.first().map(String::as_str) {
        Some("list") => {
            args.remove(0);
            reject_unknown_remote_options(&args)?;
            list_mock_web_devices(globals)
        }
        Some("revoke") => {
            args.remove(0);
            let device_id = args
                .first()
                .ok_or_else(|| {
                    CliError::usage("Missing DEVICE_ID for xero mock-web devices revoke.")
                })?
                .clone();
            validate_required_cli(&device_id, "deviceId")?;
            if args.len() > 1 {
                return Err(CliError::usage(
                    "Unexpected extra arguments after DEVICE_ID.",
                ));
            }
            mock_web_bridge_for_cli(&globals)
                .revoke_device(&device_id)
                .map_err(map_bridge_error)?;
            Ok(response(
                &globals,
                format!("Revoked remote device `{device_id}`."),
                json!({ "schema": "xero.mock_web_device_revoke.v1", "deviceId": device_id }),
            ))
        }
        Some(other) => Err(CliError::usage(format!(
            "Unknown mock-web devices command `{other}`. Use list or revoke."
        ))),
        None => list_mock_web_devices(globals),
    }
}

fn list_mock_web_devices(globals: GlobalOptions) -> Result<CliResponse, CliError> {
    let devices = mock_web_bridge_for_cli(&globals)
        .list_account_devices()
        .map_err(map_bridge_error)?;
    let text = if devices.is_empty() {
        "No remote account devices.".to_string()
    } else {
        devices
            .iter()
            .map(|device| {
                format!(
                    "{}\t{}\t{}",
                    device.id,
                    device.kind,
                    device.name.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "schema": "xero.mock_web_devices.v1", "devices": devices }),
    ))
}

fn command_remote_login(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero remote login [--poll FLOW_ID]",
            json!({ "command": "remote login" }),
        ));
    }
    let poll_flow_id = take_option(&mut args, "--poll")?;
    reject_unknown_remote_options(&args)?;
    let bridge = bridge_for_cli(&globals);

    let status = if let Some(flow_id) = poll_flow_id {
        bridge
            .poll_github_login(&flow_id)
            .map_err(map_bridge_error)?
    } else {
        bridge.sign_in_with_github().map_err(map_bridge_error)?
    };

    let text = if status.signed_in {
        "Remote GitHub login complete.".to_string()
    } else if let (Some(flow_id), Some(url)) = (&status.flow_id, &status.authorization_url) {
        format!("Open this URL to sign in:\n{url}\n\nThen run: xero remote login --poll {flow_id}")
    } else {
        "Remote GitHub login is still pending.".to_string()
    };

    Ok(response(
        &globals,
        text,
        json!({
            "schema": "xero.remote_login.v1",
            "status": status,
        }),
    ))
}

fn command_remote_logout(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero remote logout",
            json!({ "command": "remote logout" }),
        ));
    }
    reject_unknown_remote_options(&args)?;
    bridge_for_cli(&globals)
        .sign_out()
        .map_err(map_bridge_error)?;
    Ok(response(
        &globals,
        "Remote GitHub session cleared.",
        json!({ "schema": "xero.remote_logout.v1" }),
    ))
}

fn command_remote_devices(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero remote devices [list|revoke DEVICE_ID]",
            json!({ "command": "remote devices" }),
        ));
    }

    match args.first().map(String::as_str) {
        Some("list") => {
            args.remove(0);
            reject_unknown_remote_options(&args)?;
            list_remote_devices(globals)
        }
        Some("revoke") => {
            args.remove(0);
            let device_id = args
                .first()
                .ok_or_else(|| {
                    CliError::usage("Missing DEVICE_ID for xero remote devices revoke.")
                })?
                .clone();
            validate_required_cli(&device_id, "deviceId")?;
            if args.len() > 1 {
                return Err(CliError::usage(
                    "Unexpected extra arguments after DEVICE_ID.",
                ));
            }
            bridge_for_cli(&globals)
                .revoke_device(&device_id)
                .map_err(map_bridge_error)?;
            Ok(response(
                &globals,
                format!("Revoked remote device `{device_id}`."),
                json!({ "schema": "xero.remote_device_revoke.v1", "deviceId": device_id }),
            ))
        }
        Some(other) => Err(CliError::usage(format!(
            "Unknown remote devices command `{other}`. Use list or revoke."
        ))),
        None => list_remote_devices(globals),
    }
}

fn list_remote_devices(globals: GlobalOptions) -> Result<CliResponse, CliError> {
    let devices = bridge_for_cli(&globals)
        .list_account_devices()
        .map_err(map_bridge_error)?;
    let text = if devices.is_empty() {
        "No remote account devices.".to_string()
    } else {
        devices
            .iter()
            .map(|device| {
                format!(
                    "{}\t{}\t{}",
                    device.id,
                    device.kind,
                    device.name.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "schema": "xero.remote_devices.v1", "devices": devices }),
    ))
}

fn command_remote_connect(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero remote connect [--session SESSION_ID] [--max-frames N] [--auto-authorize]",
            json!({ "command": "remote connect" }),
        ));
    }
    let auto_authorize = take_bool_flag(&mut args, "--auto-authorize");
    let session_id = take_option(&mut args, "--session")?;
    let mut max_frames = take_option(&mut args, "--max-frames")?
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| CliError::usage("--max-frames must be a positive integer."))?
        .unwrap_or(0);
    if auto_authorize && max_frames == 0 {
        max_frames = 1;
    }
    reject_unknown_remote_options(&args)?;

    let bridge = bridge_for_cli(&globals);
    let mut connection = bridge.connect_desktop_channel().map_err(map_bridge_error)?;
    let control_reply = connection.join_control().map_err(map_bridge_error)?;
    let session_reply = if let Some(session_id) = session_id.as_deref() {
        Some(
            connection
                .join_session(session_id)
                .map_err(map_bridge_error)?,
        )
    } else {
        None
    };
    let mut frames = Vec::new();
    for _ in 0..max_frames {
        let message = connection.read().map_err(map_bridge_error)?;
        if auto_authorize && message.3 == "session_join_requested" {
            let join_ref = required_payload_string(&message.4, "join_ref")?;
            let auth_topic = required_payload_string(&message.4, "auth_topic")?;
            let _ = connection
                .authorize_session_join(join_ref, auth_topic, true)
                .map_err(map_bridge_error)?;
        }
        frames.push(json!({ "topic": message.2, "event": message.3, "payload": message.4 }));
    }

    Ok(response(
        &globals,
        format!(
            "Remote desktop connected as `{}`.",
            connection.desktop_device_id
        ),
        json!({
            "schema": "xero.remote_connect.v1",
            "desktopDeviceId": connection.desktop_device_id,
            "controlReply": control_reply,
            "sessionReply": session_reply,
            "frames": frames,
        }),
    ))
}

fn command_remote_visibility(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero remote visibility SESSION_ID on|off --project-id PROJECT_ID",
            json!({ "command": "remote visibility" }),
        ));
    }

    let project_id = take_option(&mut args, "--project-id")?
        .ok_or_else(|| CliError::usage("Missing --project-id for remote visibility."))?;
    validate_required_cli(&project_id, "projectId")?;
    let session_id = args
        .first()
        .ok_or_else(|| CliError::usage("Missing SESSION_ID for remote visibility."))?
        .clone();
    validate_required_cli(&session_id, "sessionId")?;
    let mode = args
        .get(1)
        .ok_or_else(|| CliError::usage("Missing visibility mode. Use on or off."))?
        .as_str();
    if args.len() > 2 {
        return Err(CliError::usage(
            "Unexpected extra arguments after visibility mode.",
        ));
    }
    let visible = match mode {
        "on" | "true" | "visible" => true,
        "off" | "false" | "hidden" => false,
        _ => return Err(CliError::usage("Visibility mode must be on or off.")),
    };

    let database_path = workspace_project_database_path(&globals, &project_id);
    let connection = Connection::open(&database_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_remote_visibility_open_failed",
            format!(
                "Xero could not open project state `{}`: {error}",
                database_path.display()
            ),
        )
    })?;
    ensure_remote_visible_column(&connection)?;
    let affected = connection
        .execute(
            "UPDATE agent_sessions SET remote_visible = ?3 WHERE project_id = ?1 AND agent_session_id = ?2",
            rusqlite::params![project_id, session_id, if visible { 1 } else { 0 }],
        )
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_remote_visibility_write_failed",
                format!("Xero could not update remote visibility: {error}"),
            )
        })?;
    if affected == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_remote_visibility_session_missing",
            format!("Session `{session_id}` was not found in project `{project_id}`."),
        ));
    }
    bridge_for_cli(&globals)
        .set_session_visibility(&session_id, visible)
        .map_err(map_bridge_error)?;

    Ok(response(
        &globals,
        format!(
            "Remote visibility for session `{session_id}` is {}.",
            if visible { "on" } else { "off" }
        ),
        json!({
            "schema": "xero.remote_visibility.v1",
            "projectId": project_id,
            "sessionId": session_id,
            "visible": visible,
        }),
    ))
}

#[derive(Debug, Clone)]
struct MockWebState {
    relay_url: String,
    token: String,
    account_id: String,
    device_id: String,
}

fn command_mock_web_connect(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web connect",
            json!({ "command": "mock-web connect" }),
        ));
    }
    reject_unknown_remote_options(&args)?;
    let web = mock_web_state()?;
    let mut client = mock_web_client(&web)?;
    let topic = format!("account:{}", web.account_id);
    let reply = client.join(&topic, json!({})).map_err(map_bridge_error)?;
    Ok(response(
        &globals,
        format!("Mock web connected to `{topic}`."),
        json!({ "schema": "xero.mock_web_connect.v1", "topic": topic, "reply": reply }),
    ))
}

fn command_mock_web_attach(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web attach COMPUTER_ID SESSION_ID [--max-frames N]",
            json!({ "command": "mock-web attach" }),
        ));
    }
    let max_frames = take_option(&mut args, "--max-frames")?
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| CliError::usage("--max-frames must be a positive integer."))?
        .unwrap_or(0);
    let (computer_id, session_id) = computer_and_session_args(&args, "attach")?;
    let web = mock_web_state()?;
    let mut client = mock_web_client(&web)?;
    let topic = format!("session:{computer_id}:{session_id}");
    let reply = client
        .join(&topic, json!({"join_ref": "mock-web", "last_seq": 0}))
        .map_err(map_bridge_error)?;
    let frames = read_remote_frames(&mut client, max_frames)?;
    Ok(response(
        &globals,
        format!("Mock web attached to `{topic}`."),
        json!({ "schema": "xero.mock_web_attach.v1", "topic": topic, "reply": reply, "frames": frames }),
    ))
}

fn command_mock_web_send(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web send COMPUTER_ID SESSION_ID MESSAGE [--max-frames N]",
            json!({ "command": "mock-web send" }),
        ));
    }
    let max_frames = take_option(&mut args, "--max-frames")?
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| CliError::usage("--max-frames must be a positive integer."))?
        .unwrap_or(1);
    if args.len() < 3 {
        return Err(CliError::usage(
            "Missing COMPUTER_ID, SESSION_ID, or MESSAGE for mock-web send.",
        ));
    }
    let computer_id = args[0].clone();
    let session_id = args[1].clone();
    let message = args[2].clone();
    if args.len() > 3 {
        return Err(CliError::usage("Unexpected extra arguments after MESSAGE."));
    }
    let web = mock_web_state()?;
    let mut client = mock_web_client(&web)?;
    let topic = format!("session:{computer_id}:{session_id}");
    client
        .join(&topic, json!({"join_ref": "mock-web", "last_seq": 0}))
        .map_err(map_bridge_error)?;
    let reply = client
        .push_and_wait(
            &topic,
            "frame",
            mock_web_command_payload(
                &web,
                Some(&session_id),
                "send_message",
                json!({ "message": message }),
            ),
        )
        .map_err(map_bridge_error)?;
    let frames = read_remote_frames(&mut client, max_frames)?;
    Ok(response(
        &globals,
        format!("Mock web sent a message to `{topic}`."),
        json!({ "schema": "xero.mock_web_send.v1", "topic": topic, "reply": reply, "frames": frames }),
    ))
}

fn command_mock_web_list_sessions(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web list-sessions COMPUTER_ID",
            json!({ "command": "mock-web list-sessions" }),
        ));
    }
    let computer_id = args
        .first()
        .ok_or_else(|| CliError::usage("Missing COMPUTER_ID for mock-web list-sessions."))?
        .clone();
    if args.len() > 1 {
        return Err(CliError::usage(
            "Unexpected extra arguments after COMPUTER_ID.",
        ));
    }
    let web = mock_web_state()?;
    let mut client = mock_web_client(&web)?;
    let topic = format!("session:{computer_id}:__sessions__");
    client
        .join(&topic, json!({"join_ref": "mock-web", "last_seq": 0}))
        .map_err(map_bridge_error)?;
    let reply = client
        .push_and_wait(
            &topic,
            "frame",
            mock_web_command_payload(&web, None, "list_sessions", json!({})),
        )
        .map_err(map_bridge_error)?;
    let frames = read_remote_frames(&mut client, 1)?;
    Ok(response(
        &globals,
        "Mock web requested visible sessions.",
        json!({ "schema": "xero.mock_web_list_sessions.v1", "topic": topic, "reply": reply, "frames": frames }),
    ))
}

fn command_mock_web_start(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero mock-web start COMPUTER_ID AGENT PROMPT [--project-id PROJECT_ID] [--max-frames N]",
            json!({ "command": "mock-web start" }),
        ));
    }
    let project_id = take_option(&mut args, "--project-id")?;
    let max_frames = take_option(&mut args, "--max-frames")?
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| CliError::usage("--max-frames must be a positive integer."))?
        .unwrap_or(1);
    if args.len() < 3 {
        return Err(CliError::usage(
            "Missing COMPUTER_ID, AGENT, or PROMPT for mock-web start.",
        ));
    }
    let computer_id = args[0].clone();
    let agent = args[1].clone();
    let prompt = args[2].clone();
    if args.len() > 3 {
        return Err(CliError::usage("Unexpected extra arguments after PROMPT."));
    }
    let web = mock_web_state()?;
    let mut client = mock_web_client(&web)?;
    let topic = format!("session:{computer_id}:__new__");
    client
        .join(&topic, json!({"join_ref": "mock-web", "last_seq": 0}))
        .map_err(map_bridge_error)?;
    let reply = client
        .push_and_wait(
            &topic,
            "frame",
            mock_web_command_payload(
                &web,
                None,
                "start_session",
                json!({ "agent": agent, "prompt": prompt, "projectId": project_id }),
            ),
        )
        .map_err(map_bridge_error)?;
    let frames = read_remote_frames(&mut client, max_frames)?;
    Ok(response(
        &globals,
        "Mock web requested a new session.",
        json!({ "schema": "xero.mock_web_start.v1", "topic": topic, "reply": reply, "frames": frames }),
    ))
}

fn bridge_for_cli(
    globals: &GlobalOptions,
) -> RemoteBridge<FileIdentityStore, FileSessionVisibilityStore> {
    let remote_dir = cli_app_data_root(globals).join(REMOTE_DIR);
    RemoteBridge::new(
        BridgeConfig::from_env_or_local("Xero CLI"),
        FileIdentityStore::new(remote_dir.join(IDENTITY_FILE)),
        FileSessionVisibilityStore::new(remote_dir.join(VISIBILITY_FILE)),
    )
}

fn mock_web_bridge_for_cli(
    globals: &GlobalOptions,
) -> RemoteBridge<FileIdentityStore, FileSessionVisibilityStore> {
    let remote_dir = cli_app_data_root(globals).join(REMOTE_DIR);
    RemoteBridge::new(
        BridgeConfig::from_env_or_local("Xero Mock Web"),
        FileIdentityStore::new(remote_dir.join(MOCK_WEB_IDENTITY_FILE)),
        FileSessionVisibilityStore::new(remote_dir.join(VISIBILITY_FILE)),
    )
}

fn mock_web_state() -> Result<MockWebState, CliError> {
    let relay_url = BridgeConfig::from_env_or_local("Xero Mock Web").relay_url;

    // Prefer the on-disk identity written by `xero mock-web login`. Fall back
    // to environment variables so existing scripted setups keep working.
    let globals_for_path = GlobalOptions {
        output_mode: OutputMode::Text,
        ci: false,
        state_dir: default_headless_state_dir()?,
        tui_adapter: None,
    };
    let identity_path = cli_app_data_root(&globals_for_path)
        .join(REMOTE_DIR)
        .join(MOCK_WEB_IDENTITY_FILE);
    let stored_identity = FileIdentityStore::new(identity_path).load().ok().flatten();

    let (token, account_id, device_id) = if let Some(identity) = stored_identity {
        let token = identity
            .desktop_jwt
            .clone()
            .ok_or_else(|| {
                CliError::usage(
                    "Stored mock-web identity is missing a relay token. Run `xero mock-web login` again.",
                )
            })?;
        let account_id = identity.account_id.clone().ok_or_else(|| {
            CliError::usage(
                "Stored mock-web identity is missing an account id. Run `xero mock-web login` again.",
            )
        })?;
        let device_id = identity.desktop_device_id.clone().ok_or_else(|| {
            CliError::usage(
                "Stored mock-web identity is missing a device id. Run `xero mock-web login` again.",
            )
        })?;
        (token, account_id, device_id)
    } else {
        (
            required_env("XERO_MOCK_WEB_TOKEN")?,
            required_env("XERO_MOCK_WEB_ACCOUNT_ID")?,
            required_env("XERO_MOCK_WEB_DEVICE_ID")?,
        )
    };

    Ok(MockWebState {
        relay_url,
        token,
        account_id,
        device_id,
    })
}

fn required_env(name: &str) -> Result<String, CliError> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::usage(format!(
                "{name} must be set for mock-web channel commands. Use the relay token and ids returned by the OAuth flow."
            ))
        })
}

fn mock_web_client(web: &MockWebState) -> Result<PhoenixChannelClient, CliError> {
    PhoenixChannelClient::connect(
        &BridgeConfig {
            relay_url: web.relay_url.clone(),
            device_name: Some("Xero Mock Web".into()),
        },
        &web.token,
        PhoenixSocketKind::Web,
    )
    .map_err(map_bridge_error)
}

fn computer_and_session_args<'a>(
    args: &'a [String],
    command: &str,
) -> Result<(&'a str, &'a str), CliError> {
    let computer_id = args
        .first()
        .ok_or_else(|| CliError::usage(format!("Missing COMPUTER_ID for mock-web {command}.")))?;
    let session_id = args
        .get(1)
        .ok_or_else(|| CliError::usage(format!("Missing SESSION_ID for mock-web {command}.")))?;
    if args.len() > 2 {
        return Err(CliError::usage(
            "Unexpected extra arguments after SESSION_ID.",
        ));
    }
    Ok((computer_id, session_id))
}

fn required_payload_string<'a>(payload: &'a JsonValue, key: &str) -> Result<&'a str, CliError> {
    payload.get(key).and_then(JsonValue::as_str).ok_or_else(|| {
        CliError::system_fault(
            "xero_cli_remote_join_request_malformed",
            format!("Remote relay join request was missing `{key}`."),
        )
    })
}

fn read_remote_frames(
    client: &mut PhoenixChannelClient,
    max_frames: usize,
) -> Result<Vec<JsonValue>, CliError> {
    let mut frames = Vec::new();
    for _ in 0..max_frames {
        match client
            .read_timeout(Duration::from_secs(5))
            .map_err(map_bridge_error)?
        {
            Some(message) => frames.push(json!({
                "topic": message.2,
                "event": message.3,
                "payload": message.4,
            })),
            None => break,
        }
    }
    Ok(frames)
}

fn mock_web_command_payload(
    web: &MockWebState,
    session_id: Option<&str>,
    kind: &str,
    payload: JsonValue,
) -> JsonValue {
    json!({
        "v": 1,
        "seq": mock_web_command_seq(),
        "computer_id": "",
        "session_id": session_id,
        "kind": kind,
        "device_id": web.device_id,
        "payload": payload,
    })
}

fn mock_web_command_seq() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn ensure_remote_visible_column(connection: &Connection) -> Result<(), CliError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(agent_sessions)")
        .map_err(|error| {
            CliError::system_fault("xero_cli_remote_visibility_probe_failed", error.to_string())
        })?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| {
            CliError::system_fault("xero_cli_remote_visibility_probe_failed", error.to_string())
        })?;
    for column in columns {
        if column.map_err(|error| {
            CliError::system_fault("xero_cli_remote_visibility_probe_failed", error.to_string())
        })? == "remote_visible"
        {
            return Ok(());
        }
    }
    connection
        .execute(
            "ALTER TABLE agent_sessions ADD COLUMN remote_visible INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_remote_visibility_migration_failed",
                format!("Xero could not add the remote visibility column: {error}"),
            )
        })?;
    Ok(())
}

fn reject_unknown_remote_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if !args.is_empty() {
        return Err(CliError::usage("Unexpected arguments for remote command."));
    }
    Ok(())
}

fn map_bridge_error(error: BridgeError) -> CliError {
    match error {
        BridgeError::Http(_)
        | BridgeError::HttpStatus { .. }
        | BridgeError::InvalidRelayUrl { .. }
        | BridgeError::UnsupportedUrlScheme(_)
        | BridgeError::WebSocket(_)
        | BridgeError::Io(_) => {
            CliError::user_fixable("xero_cli_remote_relay_unavailable", error.to_string())
        }
        BridgeError::IdentityRead { .. }
        | BridgeError::IdentityWrite { .. }
        | BridgeError::IdentityDecode { .. }
        | BridgeError::StateRead { .. }
        | BridgeError::StateWrite { .. }
        | BridgeError::StateDecode { .. }
        | BridgeError::Encode(_)
        | BridgeError::Decode(_)
        | BridgeError::Json(_)
        | BridgeError::MissingServerField(_)
        | BridgeError::LockPoisoned => {
            CliError::system_fault("xero_cli_remote_bridge_failed", error.to_string())
        }
    }
}

fn remote_help() -> &'static str {
    "Usage: xero remote login|logout|connect|devices|visibility"
}

fn mock_web_help() -> &'static str {
    "Usage: xero mock-web connect|list-sessions|attach|send|start"
}
