use crate::{commands::CommandError, notifications::NotificationAdapterError};

pub(super) fn command_error_from_adapter(error: NotificationAdapterError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
