//! OAuth sign-in end to end against a scripted resource + authorization server.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Reply, Req, json_reply, result, serve, status};
use oxide_core::McpEra;
use oxide_mcp::log::ServerLog;
use oxide_mcp::oauth::{MemoryTokens, OAuthSession, SignIn, TokenStore, sign_in, token_key};
use oxide_mcp::transport::{AuthProvider, HttpConfig, HttpTransport, Transport};
use oxide_mcp::{ConnectOptions, McpClient, McpError, NoUser};
use serde_json::{Value, json};
use tokio::sync::mpsc;

fn form_value(text: &str, key: &str) -> Option<String> {
    url::form_urlencoded::parse(text.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

#[tokio::test]
async fn sign_in_registers_exchanges_refreshes_and_authorizes() {
    let origin = Arc::new(Mutex::new(String::new()));
    let o = origin.clone();
    let server = serve(Arc::new(move |req: &Req| {
        let origin = o.lock().expect("lock").clone();
        let path = req.path.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("GET", "/.well-known/oauth-protected-resource/mcp") => json_reply(json!({
                "resource": format!("{origin}/mcp"),
                "authorization_servers": [origin],
                "scopes_supported": ["mcp"]
            })),
            ("GET", "/.well-known/oauth-authorization-server") => json_reply(json!({
                "issuer": origin,
                "authorization_endpoint": format!("{origin}/authorize"),
                "token_endpoint": format!("{origin}/token"),
                "registration_endpoint": format!("{origin}/register"),
                "code_challenge_methods_supported": ["S256"]
            })),
            ("POST", "/register") => {
                assert_eq!(req.body["token_endpoint_auth_method"], json!("none"));
                json_reply(json!({"client_id": "cid-1"}))
            }
            ("POST", "/token") => {
                let grant = form_value(&req.text, "grant_type").unwrap_or_default();
                assert_eq!(form_value(&req.text, "client_id").as_deref(), Some("cid-1"));
                assert_eq!(
                    form_value(&req.text, "resource"),
                    Some(format!("{origin}/mcp"))
                );
                if grant == "authorization_code" {
                    assert_eq!(form_value(&req.text, "code").as_deref(), Some("the-code"));
                    assert!(form_value(&req.text, "code_verifier").is_some());
                    // Expires at once, so the first use refreshes it.
                    json_reply(json!({"access_token": "at-1", "token_type": "Bearer",
                                      "refresh_token": "rt-1", "expires_in": 1}))
                } else {
                    assert_eq!(form_value(&req.text, "refresh_token").as_deref(), Some("rt-1"));
                    json_reply(json!({"access_token": "at-2", "token_type": "Bearer",
                                      "expires_in": 3600}))
                }
            }
            ("POST", "/mcp") => {
                if req.header("authorization") != Some("Bearer at-2") {
                    return Reply {
                        status: "401 Unauthorized",
                        content_type: "application/json",
                        headers: vec![(
                            "WWW-Authenticate",
                            format!(r#"Bearer resource_metadata="{origin}/.well-known/oauth-protected-resource/mcp""#),
                        )],
                        body: "{}".into(),
                    };
                }
                result(
                    req,
                    json!({"resultType": "complete", "supportedVersions": ["2026-07-28"],
                           "capabilities": {}}),
                )
            }
            _ => status("404 Not Found", Value::Null),
        }
    }))
    .await;
    *origin.lock().expect("lock") = server.origin.clone();

    let store: Arc<dyn TokenStore> = Arc::new(MemoryTokens::default());
    // The "browser": checks the authorize URL and follows the redirect with a code.
    let opened = Arc::new(Mutex::new(String::new()));
    let seen = opened.clone();
    let open = move |u: &str| -> Result<(), String> {
        *seen.lock().expect("lock") = u.to_owned();
        let url = url::Url::parse(u).map_err(|e| e.to_string())?;
        let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
        let redirect = format!("{}?code=the-code&state={}", q["redirect_uri"], q["state"]);
        tokio::spawn(async move {
            let _ = reqwest::get(redirect).await;
        });
        Ok(())
    };
    sign_in(
        store.as_ref(),
        SignIn {
            server: "remote",
            url: &server.url,
            challenge: None,
            client: None,
            timeout: Duration::from_secs(10),
        },
        &open,
    )
    .await
    .expect("signs in");

    let auth_url = opened.lock().expect("lock").clone();
    assert!(auth_url.starts_with(&format!("{}/authorize?", server.origin)));
    for p in [
        "code_challenge_method=S256",
        "response_type=code",
        "client_id=cid-1",
        "scope=mcp",
    ] {
        assert!(auth_url.contains(p), "{p} in {auth_url}");
    }
    let saved = store
        .load(&token_key("remote"))
        .await
        .expect("load")
        .expect("saved");
    assert!(saved.contains("at-1"));

    let session = OAuthSession::load(store.clone(), "remote", &server.url)
        .await
        .expect("session");
    assert!(
        OAuthSession::load(store.clone(), "remote", "https://other/mcp")
            .await
            .is_none()
    );
    assert_eq!(
        session.bearer().await.expect("bearer").as_deref(),
        Some("at-2")
    );

    let (tx, rx) = mpsc::unbounded_channel();
    let t = HttpTransport::new(
        &HttpConfig {
            url: server.url.clone(),
            headers: vec![],
        },
        Some(session),
        tx,
        ServerLog::memory(),
    )
    .expect("transport");
    let client = McpClient::connect(
        "remote",
        Transport::Http(t),
        rx,
        Arc::new(NoUser),
        ConnectOptions::default(),
    )
    .await
    .expect("connects with the token");
    assert_eq!(client.info().era, McpEra::Modern);
}

#[tokio::test]
async fn servers_without_registration_need_a_client_id() {
    let server = serve(Arc::new(|req: &Req| {
        if req
            .path
            .starts_with("/.well-known/oauth-authorization-server")
        {
            json_reply(
                json!({"issuer": "x", "authorization_endpoint": "http://127.0.0.1:1/a",
                              "token_endpoint": "http://127.0.0.1:1/t"}),
            )
        } else {
            status("404 Not Found", Value::Null)
        }
    }))
    .await;
    let store = MemoryTokens::default();
    let err = sign_in(
        &store,
        SignIn {
            server: "s",
            url: &server.url,
            challenge: None,
            client: None,
            timeout: Duration::from_secs(1),
        },
        &|_: &str| Ok(()),
    )
    .await
    .expect_err("needs a client id");
    assert!(
        matches!(&err, McpError::Auth(m) if m.contains("oauthClientId")),
        "{err}"
    );
}
