//! OAuth 2.1 sign-in for remote MCP servers (MCP authorization spec):
//!
//! 1. A 401's `WWW-Authenticate` points at the protected resource metadata (RFC 9728), or it
//!    is looked up at `/.well-known/oauth-protected-resource`.
//! 2. The authorization server's metadata (RFC 8414 / OpenID discovery) gives its endpoints.
//! 3. Oxide registers itself (dynamic client registration, RFC 7591) unless the config names
//!    a pre-registered client (`oxide.servers.<name>.oauthClientId`).
//! 4. The user signs in in the system browser; the code comes back to a loopback listener on
//!    `127.0.0.1` (PKCE S256, `state`, and the `resource` indicator of RFC 8707).
//! 5. Tokens are kept in the OS keychain and refreshed when they expire or are rejected.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures_util::future::BoxFuture;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::McpError;
use crate::log::now_ms;
use crate::transport::AuthProvider;

/// Refresh this long before a token expires.
const EXPIRY_MARGIN_MS: i64 = 60_000;

/// Where sign-ins are kept (the app stores them in the OS keychain).
pub trait TokenStore: Send + Sync + 'static {
    fn load<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<String>, String>>;
    fn save<'a>(&'a self, key: &'a str, value: String) -> BoxFuture<'a, Result<(), String>>;
    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// In-memory [`TokenStore`] for tests.
#[derive(Default)]
pub struct MemoryTokens(std::sync::Mutex<HashMap<String, String>>);

impl TokenStore for MemoryTokens {
    fn load<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<String>, String>> {
        let v = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned();
        Box::pin(async move { Ok(v) })
    }
    fn save<'a>(&'a self, key: &'a str, value: String) -> BoxFuture<'a, Result<(), String>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_owned(), value);
        Box::pin(async { Ok(()) })
    }
    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
        Box::pin(async { Ok(()) })
    }
}

/// The keychain entry of a server's sign-in.
pub fn token_key(server: &str) -> String {
    format!("mcp-oauth:{server}")
}

/// A stored sign-in.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthRecord {
    /// The MCP server URL the tokens are for.
    pub resource: String,
    pub issuer: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix ms.
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
}

impl std::fmt::Debug for OAuthRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthRecord")
            .field("resource", &self.resource)
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// The server URL as an RFC 8707 resource indicator: no fragment, no lone trailing slash.
pub fn canonical_resource(server_url: &str) -> String {
    match url::Url::parse(server_url.trim()) {
        Ok(mut u) => {
            u.set_fragment(None);
            let s = u.to_string();
            if u.path() == "/" && u.query().is_none() {
                s.trim_end_matches('/').to_owned()
            } else {
                s
            }
        }
        Err(_) => server_url.trim().to_owned(),
    }
}

/// Parameters of a `WWW-Authenticate: Bearer k="v", …` challenge (keys lowercased).
pub fn parse_challenge(header: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let rest = header
        .trim()
        .split_once(char::is_whitespace)
        .map(|(_, r)| r)
        .unwrap_or("");
    let mut chars = rest.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| *c == ',' || c.is_whitespace()) {
            chars.next();
        }
        let key: String =
            std::iter::from_fn(|| chars.next_if(|c| *c != '=' && *c != ',')).collect();
        if key.is_empty() {
            break;
        }
        if chars.next() != Some('=') {
            continue;
        }
        let value = if chars.peek() == Some(&'"') {
            chars.next();
            let mut v = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            v.push(n);
                        }
                    }
                    '"' => break,
                    c => v.push(c),
                }
            }
            v
        } else {
            std::iter::from_fn(|| chars.next_if(|c| *c != ',')).collect::<String>()
        };
        out.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    out
}

fn origin(u: &url::Url) -> String {
    let mut s = format!("{}://{}", u.scheme(), u.host_str().unwrap_or(""));
    if let Some(p) = u.port() {
        s.push_str(&format!(":{p}"));
    }
    s
}

fn path_suffix(u: &url::Url) -> String {
    let p = u.path().trim_end_matches('/');
    if p.is_empty() {
        String::new()
    } else {
        p.to_owned()
    }
}

/// Where to look for the protected resource metadata.
pub fn resource_metadata_urls(server_url: &str, challenge: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(u) = challenge.and_then(|c| parse_challenge(c).remove("resource_metadata")) {
        out.push(u);
    }
    if let Ok(u) = url::Url::parse(server_url) {
        let o = origin(&u);
        let p = path_suffix(&u);
        if !p.is_empty() {
            out.push(format!("{o}/.well-known/oauth-protected-resource{p}"));
        }
        out.push(format!("{o}/.well-known/oauth-protected-resource"));
    }
    out
}

/// Where to look for an authorization server's metadata (MCP spec order).
pub fn auth_server_metadata_urls(issuer: &str) -> Vec<String> {
    let Ok(u) = url::Url::parse(issuer) else {
        return Vec::new();
    };
    let o = origin(&u);
    let p = path_suffix(&u);
    if p.is_empty() {
        vec![
            format!("{o}/.well-known/oauth-authorization-server"),
            format!("{o}/.well-known/openid-configuration"),
        ]
    } else {
        vec![
            format!("{o}/.well-known/oauth-authorization-server{p}"),
            format!("{o}/.well-known/openid-configuration{p}"),
            format!("{o}{p}/.well-known/openid-configuration"),
        ]
    }
}

fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

/// A PKCE verifier and its S256 challenge.
pub fn pkce_pair() -> (String, String) {
    let verifier = random_token(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn http() -> Result<reqwest::Client, McpError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .user_agent(format!("Oxide/{}", oxide_core::version()))
        .build()
        .map_err(|e| McpError::Network(e.to_string()))
}

async fn get_json(client: &reqwest::Client, url: &str) -> Option<Value> {
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .header("MCP-Protocol-Version", crate::protocol::MODERN_VERSIONS[0])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().filter(|(_, v)| !v.is_empty()))
        .finish()
}

/// Tokens from a token endpoint answer.
struct Tokens {
    access: String,
    refresh: Option<String>,
    expires_at: Option<i64>,
    scope: Option<String>,
}

async fn token_request(
    client: &reqwest::Client,
    endpoint: &str,
    body: String,
) -> Result<Tokens, McpError> {
    let resp = client
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let what = v
            .get("error_description")
            .or_else(|| v.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("no details");
        return Err(McpError::Auth(format!(
            "the sign-in server refused the token request (HTTP {}): {what}",
            status.as_u16()
        )));
    }
    let access = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::Auth("the sign-in server sent no access token".into()))?
        .to_owned();
    Ok(Tokens {
        access,
        refresh: v
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expires_at: v
            .get("expires_in")
            .and_then(Value::as_i64)
            .map(|s| now_ms() + s * 1000),
        scope: v.get("scope").and_then(Value::as_str).map(str::to_owned),
    })
}

/// What a sign-in needs to know.
pub struct SignIn<'a> {
    pub server: &'a str,
    pub url: &'a str,
    /// The `WWW-Authenticate` header of the 401, if there was one.
    pub challenge: Option<&'a str>,
    /// A pre-registered client (id, secret) from the config.
    pub client: Option<(String, Option<String>)>,
    /// How long to wait for the browser.
    pub timeout: Duration,
}

/// Runs the whole sign-in: discovery, registration, browser, token exchange. `open` opens a
/// URL in the system browser.
pub async fn sign_in(
    store: &dyn TokenStore,
    req: SignIn<'_>,
    open: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
) -> Result<(), McpError> {
    let client = http()?;
    let resource = canonical_resource(req.url);

    // 1. Protected resource metadata.
    let mut prm = None;
    for u in resource_metadata_urls(req.url, req.challenge) {
        if let Some(v) = get_json(&client, &u).await {
            prm = Some(v);
            break;
        }
    }
    let server_url = url::Url::parse(req.url)
        .map_err(|_| McpError::Config("the server URL isn't valid".into()))?;
    let issuer = prm
        .as_ref()
        .and_then(|p| p.get("authorization_servers"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map(str::to_owned)
        // Servers from before RFC 9728 support: the MCP server's origin is the issuer.
        .unwrap_or_else(|| origin(&server_url));
    let challenge_scope = req
        .challenge
        .and_then(|c| parse_challenge(c).remove("scope"));
    let scope = challenge_scope.or_else(|| {
        prm.as_ref()
            .and_then(|p| p.get("scopes_supported"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
    });

    // 2. Authorization server metadata.
    let mut meta = None;
    for u in auth_server_metadata_urls(&issuer) {
        if let Some(v) = get_json(&client, &u).await {
            meta = Some(v);
            break;
        }
    }
    let issuer_origin = url::Url::parse(&issuer)
        .map(|u| origin(&u))
        .unwrap_or_else(|_| issuer.clone());
    let endpoint = |key: &str, fallback: &str| -> String {
        meta.as_ref()
            .and_then(|m| m.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{issuer_origin}{fallback}"))
    };
    let authorize = endpoint("authorization_endpoint", "/authorize");
    let token = endpoint("token_endpoint", "/token");
    let registration = meta
        .as_ref()
        .and_then(|m| m.get("registration_endpoint"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| meta.is_none().then(|| format!("{issuer_origin}/register")));
    if let Some(methods) = meta
        .as_ref()
        .and_then(|m| m.get("code_challenge_methods_supported"))
        .and_then(Value::as_array)
        && !methods.iter().any(|m| m.as_str() == Some("S256"))
    {
        return Err(McpError::Auth(
            "the sign-in server doesn't support PKCE (S256), so Oxide can't sign in safely".into(),
        ));
    }

    // 3. Loopback listener; reuse the previous port so an earlier registration still fits.
    let previous: Option<OAuthRecord> = store
        .load(&token_key(req.server))
        .await
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str(&t).ok());
    let previous_port = previous
        .as_ref()
        .filter(|p| p.issuer == issuer)
        .and_then(|p| url::Url::parse(&p.redirect_uri).ok())
        .and_then(|u| u.port());
    let listener = match previous_port {
        Some(port) => match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => Ok(l),
            Err(_) => TcpListener::bind(("127.0.0.1", 0)).await,
        },
        None => TcpListener::bind(("127.0.0.1", 0)).await,
    }
    .map_err(|e| McpError::Auth(format!("couldn't listen for the sign-in reply: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| McpError::Auth(e.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    // 4. Client registration.
    let (client_id, client_secret) = if let Some(c) = req.client {
        c
    } else if let Some(p) = previous
        .as_ref()
        .filter(|p| p.issuer == issuer && p.redirect_uri == redirect_uri)
    {
        (p.client_id.clone(), p.client_secret.clone())
    } else {
        let Some(reg) = registration else {
            return Err(McpError::Auth(format!(
                "this server doesn't let apps register themselves. Ask its owner for an OAuth \
                 client ID and add it to oxide_config.json under \"oxide\" → \"servers\" → \
                 \"{}\" → \"oauthClientId\".",
                req.server
            )));
        };
        let mut body = json!({
            "client_name": "Oxide",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        });
        if let Some(s) = &scope {
            body["scope"] = json!(s);
        }
        let resp = client.post(&reg).json(&body).send().await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        let id = v
            .get("client_id")
            .and_then(Value::as_str)
            .filter(|_| status.is_success());
        let Some(id) = id else {
            return Err(McpError::Auth(format!(
                "registering Oxide with the sign-in server failed (HTTP {})",
                status.as_u16()
            )));
        };
        (
            id.to_owned(),
            v.get("client_secret")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    };

    // 5. Browser.
    let (verifier, challenge) = pkce_pair();
    let state = random_token(24);
    let mut auth_url = url::Url::parse(&authorize)
        .map_err(|_| McpError::Auth("the sign-in server's address is invalid".into()))?;
    {
        let mut q = auth_url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &resource);
        if let Some(s) = &scope {
            q.append_pair("scope", s);
        }
    }
    open(auth_url.as_str()).map_err(McpError::Auth)?;
    let code = tokio::time::timeout(req.timeout, wait_for_code(&listener, &state))
        .await
        .map_err(|_| McpError::Auth("the sign-in timed out".into()))??;

    // 6. Tokens.
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &client_id),
        ("code_verifier", &verifier),
        ("resource", &resource),
        ("client_secret", client_secret.as_deref().unwrap_or("")),
    ]);
    let tokens = token_request(&client, &token, body).await?;
    let record = OAuthRecord {
        resource,
        issuer,
        token_endpoint: token,
        client_id,
        client_secret,
        redirect_uri,
        access_token: tokens.access,
        refresh_token: tokens.refresh,
        expires_at: tokens.expires_at,
        scope: tokens.scope.or(scope),
    };
    save_record(store, req.server, &record).await
}

async fn save_record(
    store: &dyn TokenStore,
    server: &str,
    record: &OAuthRecord,
) -> Result<(), McpError> {
    let text = serde_json::to_string(record).map_err(|e| McpError::Auth(e.to_string()))?;
    store
        .save(&token_key(server), text)
        .await
        .map_err(|e| McpError::Auth(format!("couldn't save the sign-in: {e}")))
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Oxide</title>\
<body style=\"font:16px system-ui;margin:4em auto;max-width:28em;text-align:center\">\
<h1 style=\"font-weight:500\">Signed in</h1><p>You can close this tab and return to Oxide.</p>";

/// Accepts browser requests on the loopback listener until the redirect with a code (or an
/// error) arrives.
async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String, McpError> {
    loop {
        let (mut sock, _) = listener
            .accept()
            .await
            .map_err(|e| McpError::Auth(e.to_string()))?;
        let mut buf = vec![0u8; 8192];
        let mut len = 0;
        while len < buf.len() {
            let n = sock.read(&mut buf[len..]).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            len += n;
            if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8_lossy(&buf[..len]).into_owned();
        let target = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_owned();
        let Ok(u) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
            continue;
        };
        if u.path() != "/callback" {
            let _ = sock
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                )
                .await;
            continue;
        }
        let q: HashMap<String, String> = u.query_pairs().into_owned().collect();
        let outcome = if q.get("state").map(String::as_str) != Some(state) {
            Err(McpError::Auth(
                "the sign-in reply didn't match (state)".into(),
            ))
        } else if let Some(code) = q.get("code") {
            Ok(code.clone())
        } else {
            let why = q
                .get("error_description")
                .or_else(|| q.get("error"))
                .cloned()
                .unwrap_or_else(|| "no code".into());
            Err(McpError::Auth(format!(
                "the sign-in was cancelled or failed: {why}"
            )))
        };
        let page = match &outcome {
            Ok(_) => DONE_PAGE.to_owned(),
            Err(e) => format!(
                "<!doctype html><meta charset=utf-8><title>Oxide</title><body style=\"font:16px system-ui;margin:4em auto;max-width:28em\"><h1>Sign-in failed</h1><p>{}</p>",
                html_escape(&e.to_string())
            ),
        };
        let resp = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        return outcome;
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A stored sign-in used by the HTTP transport: hands out the access token and refreshes it.
pub struct OAuthSession {
    store: Arc<dyn TokenStore>,
    server: String,
    client: reqwest::Client,
    record: tokio::sync::Mutex<OAuthRecord>,
}

impl OAuthSession {
    /// The stored sign-in for `server`, if there is one for this URL.
    pub async fn load(store: Arc<dyn TokenStore>, server: &str, url: &str) -> Option<Arc<Self>> {
        let text = store.load(&token_key(server)).await.ok().flatten()?;
        let record: OAuthRecord = serde_json::from_str(&text).ok()?;
        if record.resource != canonical_resource(url) {
            return None;
        }
        Some(Arc::new(Self {
            store,
            server: server.to_owned(),
            client: http().ok()?,
            record: tokio::sync::Mutex::new(record),
        }))
    }

    async fn refresh_locked(&self, record: &mut OAuthRecord) -> Result<bool, McpError> {
        let Some(refresh) = record.refresh_token.clone() else {
            return Ok(false);
        };
        let body = form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &record.client_id),
            ("resource", &record.resource),
            (
                "client_secret",
                record.client_secret.as_deref().unwrap_or(""),
            ),
        ]);
        match token_request(&self.client, &record.token_endpoint, body).await {
            Ok(t) => {
                record.access_token = t.access;
                if t.refresh.is_some() {
                    record.refresh_token = t.refresh;
                }
                record.expires_at = t.expires_at;
                if t.scope.is_some() {
                    record.scope = t.scope;
                }
                save_record(self.store.as_ref(), &self.server, record).await?;
                Ok(true)
            }
            Err(e) => {
                tracing::info!("MCP token refresh for {} failed: {e}", self.server);
                Ok(false)
            }
        }
    }
}

impl AuthProvider for OAuthSession {
    fn bearer(&self) -> BoxFuture<'_, Result<Option<String>, McpError>> {
        Box::pin(async move {
            let mut record = self.record.lock().await;
            let expired = record
                .expires_at
                .is_some_and(|t| t - EXPIRY_MARGIN_MS <= now_ms());
            if expired {
                self.refresh_locked(&mut record).await?;
            }
            Ok(Some(record.access_token.clone()))
        })
    }

    fn refresh(&self) -> BoxFuture<'_, Result<bool, McpError>> {
        Box::pin(async move {
            let mut record = self.record.lock().await;
            self.refresh_locked(&mut record).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenges_are_parsed() {
        let p = parse_challenge(
            r#"Bearer error="invalid_token", resource_metadata="https://x/.well-known/oauth-protected-resource/mcp", scope="files:read files:write""#,
        );
        assert_eq!(p.get("error").map(String::as_str), Some("invalid_token"));
        assert_eq!(
            p.get("resource_metadata").map(String::as_str),
            Some("https://x/.well-known/oauth-protected-resource/mcp")
        );
        assert_eq!(
            p.get("scope").map(String::as_str),
            Some("files:read files:write")
        );
        let p = parse_challenge("Bearer realm=example, Scope=a");
        assert_eq!(p.get("realm").map(String::as_str), Some("example"));
        assert_eq!(p.get("scope").map(String::as_str), Some("a"));
        assert!(parse_challenge("Bearer").is_empty());
    }

    #[test]
    fn discovery_urls_follow_the_spec_order() {
        assert_eq!(
            resource_metadata_urls("https://ex.com/public/mcp", None),
            vec![
                "https://ex.com/.well-known/oauth-protected-resource/public/mcp",
                "https://ex.com/.well-known/oauth-protected-resource",
            ]
        );
        assert_eq!(
            auth_server_metadata_urls("https://auth.ex.com/tenant1"),
            vec![
                "https://auth.ex.com/.well-known/oauth-authorization-server/tenant1",
                "https://auth.ex.com/.well-known/openid-configuration/tenant1",
                "https://auth.ex.com/tenant1/.well-known/openid-configuration",
            ]
        );
        assert_eq!(auth_server_metadata_urls("https://auth.ex.com").len(), 2);
    }

    #[test]
    fn resources_are_canonical() {
        assert_eq!(
            canonical_resource("HTTPS://MCP.Example.com/"),
            "https://mcp.example.com"
        );
        assert_eq!(
            canonical_resource("https://mcp.example.com/mcp#frag"),
            "https://mcp.example.com/mcp"
        );
    }

    #[test]
    fn pkce_challenge_is_s256_of_the_verifier() {
        let (v, c) = pkce_pair();
        assert!((43..=128).contains(&v.len()));
        assert_eq!(c, URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())));
        assert_ne!(pkce_pair().0, v);
    }

    #[test]
    fn records_never_print_tokens() {
        let r = OAuthRecord {
            resource: "r".into(),
            issuer: "i".into(),
            token_endpoint: "t".into(),
            client_id: "c".into(),
            client_secret: Some("SECRET".into()),
            redirect_uri: "u".into(),
            access_token: "ACCESS".into(),
            refresh_token: Some("REFRESH".into()),
            expires_at: None,
            scope: None,
        };
        let s = format!("{r:?}");
        assert!(!s.contains("ACCESS") && !s.contains("REFRESH") && !s.contains("SECRET"));
    }
}
