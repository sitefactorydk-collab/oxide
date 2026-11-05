//! Connection test for an endpoint: lists its models with the given key.

use std::time::Duration;

use oxide_core::ProviderKind;
use serde::Deserialize;

pub use crate::anthropic::ANTHROPIC_VERSION;
use crate::http::{USER_AGENT, describe};

const TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("couldn't reach {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("the API key was rejected ({status})")]
    Unauthorized { status: u16 },
    #[error("the server answered {status}: {message}")]
    Http { status: u16, message: String },
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("invalid request: {0}")]
    Invalid(String),
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}

/// Lists the model ids an endpoint offers. Every kind sends the key as a bearer token
/// (optional for local servers); Anthropic also gets its `anthropic-version` header.
pub async fn list_models(
    kind: ProviderKind,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProbeError> {
    let base = oxide_core::provider::normalize_base_url(base_url);
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(ProbeError::Invalid(
            "the base URL must start with http:// or https://".into(),
        ));
    }
    let url = match kind {
        ProviderKind::Anthropic => format!("{base}/models?limit=1000"),
        _ => format!("{base}/models"),
    };
    let key = api_key.map(str::trim).filter(|k| !k.is_empty());

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| ProbeError::Invalid(e.to_string()))?;

    let req = match (kind, key) {
        (ProviderKind::Anthropic, k) => crate::anthropic::with_auth(client.get(&url), k),
        (_, Some(k)) => client.get(&url).bearer_auth(k),
        (_, None) => client.get(&url),
    };

    let res = req.send().await.map_err(|e| ProbeError::Unreachable {
        url: base.clone(),
        reason: describe(&e),
    })?;
    let status = res.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(ProbeError::Unauthorized {
            status: status.as_u16(),
        });
    }
    let body = res.text().await.map_err(|e| ProbeError::Unreachable {
        url: base.clone(),
        reason: describe(&e),
    })?;
    if !status.is_success() {
        let message = serde_json::from_str::<ErrorBody>(&body)
            .map(|b| b.error.message)
            .unwrap_or_else(|_| body.chars().take(200).collect());
        return Err(ProbeError::Http {
            status: status.as_u16(),
            message,
        });
    }
    let list: ModelList = serde_json::from_str(&body)
        .map_err(|e| ProbeError::Decode(format!("no model list ({e})")))?;
    Ok(list.data.into_iter().map(|m| m.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves one canned HTTP response and returns the raw request it received.
    async fn serve_once(status: &str, body: &str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let addr = listener.local_addr().expect("addr");
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.expect("accepts");
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.expect("reads");
            sock.write_all(response.as_bytes()).await.expect("writes");
            sock.shutdown().await.ok();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (format!("http://{addr}/v1"), handle)
    }

    #[tokio::test]
    async fn anthropic_sends_bearer_key_and_version_headers() {
        let (base, req) = serve_once(
            "200 OK",
            r#"{"data":[{"id":"model-a"},{"id":"model-b"}],"has_more":false}"#,
        )
        .await;
        let models = list_models(ProviderKind::Anthropic, &base, Some("sk-test"))
            .await
            .expect("lists");
        assert_eq!(models, vec!["model-a", "model-b"]);
        let raw = req.await.expect("joins").to_lowercase();
        assert!(raw.starts_with("get /v1/models?limit=1000 "), "{raw}");
        assert!(raw.contains("authorization: bearer sk-test"));
        assert!(raw.contains(&format!("anthropic-version: {ANTHROPIC_VERSION}")));
    }

    #[tokio::test]
    async fn openai_compatible_uses_bearer_auth() {
        let (base, req) = serve_once("200 OK", r#"{"object":"list","data":[{"id":"qwen"}]}"#).await;
        let models = list_models(ProviderKind::LmStudio, &format!("{base}/"), Some("k"))
            .await
            .expect("lists");
        assert_eq!(models, vec!["qwen"]);
        let raw = req.await.expect("joins").to_lowercase();
        assert!(raw.starts_with("get /v1/models "), "{raw}");
        assert!(raw.contains("authorization: bearer k"));
    }

    #[tokio::test]
    async fn rejected_keys_are_reported_without_the_key() {
        let (base, _req) = serve_once("401 Unauthorized", r#"{"error":{"message":"bad"}}"#).await;
        let err = list_models(ProviderKind::OpenAi, &base, Some("sk-secret"))
            .await
            .expect_err("fails");
        assert!(matches!(err, ProbeError::Unauthorized { status: 401 }));
        assert!(!err.to_string().contains("sk-secret"));
    }

    #[tokio::test]
    async fn server_errors_carry_the_api_message() {
        let (base, _req) = serve_once(
            "500 Internal Server Error",
            r#"{"error":{"message":"overloaded"}}"#,
        )
        .await;
        let err = list_models(ProviderKind::Strata, &base, None)
            .await
            .expect_err("fails");
        assert_eq!(err.to_string(), "the server answered 500: overloaded");
    }

    #[tokio::test]
    async fn unreachable_servers_are_reported() {
        // Bind then drop, so the port is very likely closed.
        let port = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("binds")
            .local_addr()
            .expect("addr")
            .port();
        let err = list_models(
            ProviderKind::LmStudio,
            &format!("http://127.0.0.1:{port}/v1"),
            None,
        )
        .await
        .expect_err("fails");
        assert!(matches!(err, ProbeError::Unreachable { .. }), "{err}");
    }

    #[tokio::test]
    async fn bad_urls_are_rejected_before_any_request() {
        let err = list_models(ProviderKind::OpenAi, "localhost:1234", None)
            .await
            .expect_err("fails");
        assert!(matches!(err, ProbeError::Invalid(_)));
    }
}
