pub(crate) use xero_redaction::{
    find_prohibited_persistence_content, high_confidence_secret_text, is_sensitive_argument_name,
    redact_command_argv_for_persistence, redact_json_for_persistence,
    render_command_for_persistence,
};
