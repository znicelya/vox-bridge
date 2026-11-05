//! Keep remote diagnostics classified; never retain a remote body or message.
use crate::providers::ProviderError;
use tokio_tungstenite::tungstenite::Error;

pub(super) fn transport_error(error: Error) -> ProviderError {
    match error {
        Error::Http(response) if matches!(response.status().as_u16(), 401 | 403) => {
            ProviderError::Authentication
        }
        Error::Capacity(_) | Error::Protocol(_) | Error::Utf8(_) => ProviderError::Protocol,
        _ => ProviderError::Disconnected,
    }
}

pub(super) fn remote_error(code: i32) -> ProviderError {
    ProviderError::RemoteFailure { code }
}

pub(super) fn safe_log_id(value: &str, api_key: &[u8]) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 128
        && (api_key.is_empty()
            || !value
                .as_bytes()
                .windows(api_key.len())
                .any(|part| part == api_key))
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
    .then(|| value.to_owned())
}
