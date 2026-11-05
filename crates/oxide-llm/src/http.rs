//! HTTP plumbing shared by the clients: client setup, readable errors and retry.

use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde_json::Value;

use crate::LlmError;

pub(crate) const USER_AGENT: &str = concat!("Oxide/", env!("CARGO_PKG_VERSION"));

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest silence allowed inside a stream. Local servers can take minutes to load a model
/// before the first token, so this is generous.
const READ_TIMEOUT: Duration = Duration::from_secs(300);

/// Retries after a 429/503/529 before giving up.
pub(crate) const MAX_RETRIES: u32 = 3;
/// A `retry-after` longer than this isn't waited for; the error is reported instead.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);

/// A client for streaming requests (no overall timeout, a read timeout instead).
pub(crate) fn streaming_client() -> Result<reqwest::Client, LlmError> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| LlmError::Invalid(e.to_string()))
}

/// A short reason without the URL (callers add it) and without request headers.
pub(crate) fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_connect() {
        "connection refused or no route (is the server running?)".into()
    } else {
        let text = e.to_string();
        match e.url() {
            Some(url) => text
                .replace(&format!(" ({url})"), "")
                .replace(url.as_str(), "<url>"),
            None => text,
        }
    }
}

/// The human-readable message of an API error body, whatever shape it has.
pub(crate) fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        let candidates = [
            v.pointer("/error/message"),
            v.get("error").filter(|e| e.is_string()),
            v.get("message"),
            v.get("detail"),
        ];
        for c in candidates.into_iter().flatten() {
            if let Some(s) = c.as_str()
                && !s.trim().is_empty()
            {
                return s.trim().to_owned();
            }
        }
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "no details".into()
    } else {
        trimmed.chars().take(300).collect()
    }
}

/// Maps a failed HTTP status and its body to an error.
pub(crate) fn status_error(status: u16, body: &str) -> LlmError {
    let message = error_message(body);
    match status {
        401 | 403 => LlmError::Unauthorized { status },
        429 => LlmError::RateLimited { message },
        503 | 529 => LlmError::Overloaded,
        _ => LlmError::Http { status, message },
    }
}

/// How long to wait before retry number `attempt` (0-based). Honors `retry-after` (seconds)
/// up to a cap; `None` means don't retry.
pub(crate) fn retry_delay(attempt: u32, headers: &HeaderMap) -> Option<Duration> {
    if attempt >= MAX_RETRIES {
        return None;
    }
    let hinted = headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64);
    match hinted {
        Some(d) if d > MAX_RETRY_WAIT => None,
        Some(d) => Some(d),
        None => Some(Duration::from_millis(1000 << attempt)),
    }
}

/// Sends `request`, retrying on 429/503/529 with backoff. Returns the successful response,
/// or the error the last attempt produced.
pub(crate) async fn send_with_retry(
    request: reqwest::RequestBuilder,
    base_url: &str,
) -> Result<reqwest::Response, LlmError> {
    let mut attempt = 0;
    loop {
        let this = request
            .try_clone()
            .ok_or_else(|| LlmError::Invalid("request body can't be retried".into()))?;
        let res = this.send().await.map_err(|e| LlmError::Unreachable {
            url: base_url.to_owned(),
            reason: describe(&e),
        })?;
        let status = res.status().as_u16();
        if res.status().is_success() {
            return Ok(res);
        }
        let headers = res.headers().clone();
        let body = res.text().await.unwrap_or_default();
        let err = status_error(status, &body);
        match (err.is_retryable(), retry_delay(attempt, &headers)) {
            (true, Some(delay)) => {
                tracing::info!(status, attempt, ?delay, "provider busy; retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            _ => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn error_messages_are_found_in_common_shapes() {
        assert_eq!(
            error_message(r#"{"error":{"message":"bad model"}}"#),
            "bad model"
        );
        assert_eq!(
            error_message(r#"{"error":"no such model"}"#),
            "no such model"
        );
        assert_eq!(error_message(r#"{"message":"oops"}"#), "oops");
        assert_eq!(error_message("plain text"), "plain text");
        assert_eq!(error_message(""), "no details");
    }

    #[test]
    fn statuses_map_to_kinds() {
        assert!(matches!(
            status_error(401, ""),
            LlmError::Unauthorized { status: 401 }
        ));
        assert!(matches!(status_error(529, ""), LlmError::Overloaded));
        assert!(status_error(429, "{}").is_retryable());
        assert!(!status_error(400, "{}").is_retryable());
    }

    #[test]
    fn retry_delay_backs_off_and_honors_retry_after() {
        let none = HeaderMap::new();
        assert_eq!(retry_delay(0, &none), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(2, &none), Some(Duration::from_secs(4)));
        assert_eq!(retry_delay(MAX_RETRIES, &none), None);
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(retry_delay(0, &h), Some(Duration::from_secs(2)));
        h.insert(RETRY_AFTER, HeaderValue::from_static("600"));
        assert_eq!(retry_delay(0, &h), None);
    }
}
