//! The `oxide-artifact` protocol: serves the sandboxed frame that runs HTML and React
//! artifacts (`artifact-frame.html` and its scripts).
//!
//! The frame is a different origin from the app, the `<iframe>` is sandboxed without
//! `allow-same-origin` (so its origin is opaque), and its own Content-Security-Policy allows
//! inline scripts (artifacts need them) but no network requests. The app's CSP isn't applied
//! here. In debug builds the files come from the Vite dev server; in release from the
//! bundled assets.

use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Runtime};

/// The scheme. On Windows the frame loads from `http://oxide-artifact.localhost/`.
pub const SCHEME: &str = "oxide-artifact";
/// The frame's HTML entry (a Vite page).
const ENTRY: &str = "/artifact-frame.html";

/// The frame's Content-Security-Policy. `dev` is the dev server origin (debug builds), so
/// Vite's module scripts load.
pub fn csp(dev: Option<&str>) -> String {
    let own = "'self' http://oxide-artifact.localhost https://oxide-artifact.localhost \
               oxide-artifact:";
    let connect = match dev {
        Some(origin) => {
            let ws = origin
                .replacen("http://", "ws://", 1)
                .replacen("https://", "wss://", 1);
            format!("{own} {origin} {ws}")
        }
        None => "'none'".to_owned(),
    };
    format!(
        "default-src 'none'; \
         script-src {own} 'unsafe-inline' 'unsafe-eval' blob: https:; \
         style-src {own} 'unsafe-inline' https:; \
         img-src {own} data: blob: https:; \
         font-src {own} data: https:; \
         media-src {own} data: blob: https:; \
         worker-src blob:; \
         connect-src {connect}; \
         frame-src 'none'; \
         form-action 'none'; \
         base-uri 'none'"
    )
}

/// Removes `<meta http-equiv="Content-Security-Policy">` tags, so only the frame's own
/// policy applies.
pub fn strip_csp_meta(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(rel) = lower[at..].find("<meta") {
        let start = at + rel;
        let Some(end_rel) = lower[start..].find('>') else {
            break;
        };
        let end = start + end_rel + 1;
        out.push_str(&html[at..start]);
        if !lower[start..end].contains("content-security-policy") {
            out.push_str(&html[start..end]);
        }
        at = end;
    }
    out.push_str(&html[at..]);
    out
}

/// Which paths the frame may load. Release: the entry and the bundled assets. Debug: what
/// the Vite dev server serves for it, except files outside the project (`/@fs/`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn allowed(path: &str, dev: bool) -> bool {
    allowed_for(path, dev, ENTRY)
}

/// [`allowed`] for a frame with another entry page (the Utility Apps frame).
pub fn allowed_for(path: &str, dev: bool, entry: &str) -> bool {
    if path.contains("..") {
        return false;
    }
    if dev {
        return !path.starts_with("/@fs/");
    }
    path == entry || path.starts_with("/assets/")
}

/// A sandboxed frame served by its own protocol: its entry page and its policy.
pub struct Frame {
    pub entry: &'static str,
    pub csp: fn(Option<&str>) -> String,
}

const ARTIFACT_FRAME: Frame = Frame { entry: ENTRY, csp };

fn plain(status: StatusCode, text: &str) -> Response<Vec<u8>> {
    let mut r = Response::new(text.as_bytes().to_vec());
    *r.status_mut() = status;
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    r
}

fn respond_with(
    frame: &Frame,
    status: StatusCode,
    mime: &str,
    mut body: Vec<u8>,
    path: &str,
    dev: Option<&str>,
) -> Response<Vec<u8>> {
    let is_html = mime.starts_with("text/html");
    if is_html {
        body = strip_csp_meta(&String::from_utf8_lossy(&body)).into_bytes();
    }
    let builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        // Module scripts from the sandboxed (opaque-origin) frame are CORS requests.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CACHE_CONTROL,
            if dev.is_some() {
                "no-store"
            } else {
                "no-cache"
            },
        );
    let builder = if is_html || path == frame.entry {
        builder.header("Content-Security-Policy", (frame.csp)(dev))
    } else {
        builder
    };
    builder
        .body(body)
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "bad response"))
}

/// The dev server's origin in debug builds (`build.devUrl`), else `None`.
fn dev_origin<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    app.config()
        .build
        .dev_url
        .as_ref()
        .map(|u| u.origin().ascii_serialization())
}

/// Answers one request of the frame.
pub async fn respond<R: Runtime>(app: AppHandle<R>, path_and_query: String) -> Response<Vec<u8>> {
    serve(app, path_and_query, &ARTIFACT_FRAME).await
}

/// Answers one request of a sandboxed frame.
pub async fn serve<R: Runtime>(
    app: AppHandle<R>,
    path_and_query: String,
    frame: &Frame,
) -> Response<Vec<u8>> {
    let dev = dev_origin(&app);
    let path = path_and_query
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .to_owned();
    let path = if path == "/" {
        frame.entry.to_owned()
    } else {
        path
    };
    if !allowed_for(&path, dev.is_some(), frame.entry) {
        return plain(StatusCode::NOT_FOUND, "not found");
    }
    match &dev {
        Some(origin) => {
            let url = format!("{origin}{path_and_query}");
            match reqwest::get(&url).await {
                Ok(r) => {
                    let status = StatusCode::from_u16(r.status().as_u16())
                        .unwrap_or(StatusCode::BAD_GATEWAY);
                    let mime = r
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("application/octet-stream")
                        .to_owned();
                    match r.bytes().await {
                        Ok(b) => {
                            respond_with(frame, status, &mime, b.to_vec(), &path, dev.as_deref())
                        }
                        Err(e) => {
                            tracing::warn!("artifact frame: dev server read failed: {e}");
                            plain(StatusCode::BAD_GATEWAY, "dev server read failed")
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("artifact frame: dev server unreachable: {e}");
                    plain(StatusCode::BAD_GATEWAY, "dev server unreachable")
                }
            }
        }
        None => match app.asset_resolver().get(path.clone()) {
            Some(asset) => {
                let mime = asset.mime_type().to_owned();
                respond_with(
                    frame,
                    StatusCode::OK,
                    &mime,
                    asset.bytes().to_vec(),
                    &path,
                    None,
                )
            }
            None => plain(StatusCode::NOT_FOUND, "not found"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csp_blocks_network_but_allows_inline_scripts() {
        let p = csp(None);
        assert!(p.contains("connect-src 'none'"));
        assert!(p.contains("'unsafe-inline' 'unsafe-eval'"));
        assert!(p.contains("form-action 'none'"));
        let d = csp(Some("http://localhost:1420"));
        assert!(d.contains("connect-src 'self'"));
        assert!(d.contains("ws://localhost:1420"));
    }

    #[test]
    fn csp_meta_tags_are_removed() {
        let html = r#"<head><meta charset="utf-8"><META http-equiv="Content-Security-Policy" content="default-src 'self'"><title>x</title></head>"#;
        assert_eq!(
            strip_csp_meta(html),
            r#"<head><meta charset="utf-8"><title>x</title></head>"#
        );
    }

    #[test]
    fn only_the_frame_files_are_served() {
        assert!(allowed("/artifact-frame.html", false));
        assert!(allowed("/assets/frame-abc.js", false));
        assert!(!allowed("/index.html", false));
        assert!(!allowed("/assets/../index.html", false));
        assert!(allowed("/@vite/client", true));
        assert!(!allowed("/@fs/C:/Users/me/secret.txt", true));
    }
}
