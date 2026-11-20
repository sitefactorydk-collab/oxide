//! The `oxide-app` protocol: serves the sandboxed frame Utility Apps run in
//! (`app-frame.html` and its scripts), like the artifact frame (Phase 8).
//!
//! The `<iframe>` is sandboxed without `allow-same-origin`, so the app has an opaque origin
//! and no Tauri IPC. Its policy is stricter than the artifact frame's: no network at all
//! (scripts, styles, images and fonts come only from the frame itself, `data:` and `blob:`),
//! so everything an app reaches outside goes through the `oxide` SDK, which asks the user
//! for each capability once per app.

use tauri::http::Response;
use tauri::{AppHandle, Runtime};

use crate::artifact_frame::{Frame, serve};

/// The scheme. On Windows the frame loads from `http://oxide-app.localhost/`.
pub const SCHEME: &str = "oxide-app";
/// The frame's HTML entry (a Vite page).
const ENTRY: &str = "/app-frame.html";

/// The frame's Content-Security-Policy. `dev` is the dev server origin (debug builds), so
/// Vite's module scripts load.
pub fn csp(dev: Option<&str>) -> String {
    let own = "'self' http://oxide-app.localhost https://oxide-app.localhost oxide-app:";
    let (scripts, connect) = match dev {
        Some(origin) => {
            let ws = origin
                .replacen("http://", "ws://", 1)
                .replacen("https://", "wss://", 1);
            (format!("{own} {origin}"), format!("{own} {origin} {ws}"))
        }
        None => (own.to_owned(), "'none'".to_owned()),
    };
    format!(
        "default-src 'none'; \
         script-src {scripts} 'unsafe-inline' 'unsafe-eval' blob:; \
         style-src {own} 'unsafe-inline'; \
         img-src {own} data: blob:; \
         font-src {own} data:; \
         media-src {own} data: blob:; \
         worker-src blob:; \
         connect-src {connect}; \
         frame-src 'none'; \
         form-action 'none'; \
         base-uri 'none'"
    )
}

const APP_FRAME: Frame = Frame { entry: ENTRY, csp };

/// Answers one request of the frame.
pub async fn respond<R: Runtime>(app: AppHandle<R>, path_and_query: String) -> Response<Vec<u8>> {
    serve(app, path_and_query, &APP_FRAME).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_frame::allowed_for;

    #[test]
    fn apps_get_no_network_at_all() {
        let p = csp(None);
        assert!(p.contains("connect-src 'none'"));
        assert!(p.contains("default-src 'none'"));
        // Unlike artifacts, no remote scripts, styles, images or fonts: nothing leaves the
        // frame except through the SDK.
        assert!(!p.contains("https:;"), "{p}");
        assert!(!p.contains(" https: "), "{p}");
        assert!(p.contains("img-src 'self' http://oxide-app.localhost"));
        assert!(p.contains("form-action 'none'"));
        let d = csp(Some("http://localhost:1420"));
        assert!(d.contains("connect-src 'self'"));
        assert!(d.contains("ws://localhost:1420"));
    }

    #[test]
    fn only_the_app_frame_is_served() {
        assert!(allowed_for("/app-frame.html", false, ENTRY));
        assert!(allowed_for("/assets/appframe-x.js", false, ENTRY));
        assert!(!allowed_for("/index.html", false, ENTRY));
        assert!(!allowed_for("/artifact-frame.html", false, ENTRY));
        assert!(!allowed_for("/assets/../index.html", false, ENTRY));
    }
}
