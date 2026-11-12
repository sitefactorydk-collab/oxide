//! The real extension in a real Chrome, paired through its popup and driven through the bridge.
//!
//! Ignored by default. Needs the built extension (`npm run extension`), a Chrome that loads
//! unpacked extensions from the command line (Chrome for Testing or Chromium), and Python with
//! Playwright (`extension/tests/drive_chrome.py` drives the popup):
//!
//! ```text
//! OXIDE_TEST_CHROME=/usr/bin/google-chrome cargo test -p oxide-bridge --test real_chrome -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxide_bridge::{Bridge, BridgeSink};
use oxide_core::chrome::{ChromeSettings, ChromeStatus};
use oxide_core::{McpContent, McpToolResult};
use oxide_store::MemorySecrets;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[derive(Default)]
struct Stops(Mutex<Vec<String>>);

impl BridgeSink for Stops {
    fn changed(&self, _: &ChromeStatus) {}
    fn stopped(&self, session: &str) {
        self.0.lock().expect("lock").push(session.to_owned());
    }
}

const HOME: &str = r#"<!doctype html><html><head><title>Test bakery</title></head><body>
<h1>Test bakery</h1><p>Fresh bread, open daily.</p>
<a href="/hours">Opening hours</a>
<a href="/hours" target="_blank">Hours in a new tab</a>
<form action="/search"><input name="q" placeholder="Search bread"><button>Go</button></form>
<label>Size <select id="size"><option>Small</option><option>Large</option></select></label>
<label><input type="checkbox" id="gift"> Gift wrap</label>
<label>Password <input type="password" id="pw"></label>
<div style="height:3000px">Tall section</div><p id="end">The end of the page.</p>
<script>console.log("bakery loaded"); console.error("oops 42");</script>
</body></html>"#;

async fn serve_site() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
    let port = l.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = req.split(' ').nth(1).unwrap_or("/").to_owned();
                let path = if path.starts_with("/?") {
                    "/".to_owned()
                } else {
                    path
                };
                let body = if path.starts_with("/hours") {
                    "<!doctype html><title>Opening hours</title><h1>Opening hours</h1>\
                     <p>Saturday 8–14</p>"
                        .to_owned()
                } else if let Some(q) = path.strip_prefix("/search?q=") {
                    format!("<!doctype html><title>Search</title><h1>Results for {q}</h1>")
                } else if path == "/" {
                    HOME.to_owned()
                } else {
                    String::new()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

struct Driver {
    child: Child,
    stdin: ChildStdin,
    out: tokio::io::Lines<BufReader<ChildStdout>>,
}

impl Driver {
    async fn line(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(60), self.out.next_line())
            .await
            .expect("the driver answers")
            .expect("reads")
            .expect("a line")
    }

    async fn ask(&mut self, cmd: &str) -> String {
        self.stdin
            .write_all(format!("{cmd}\n").as_bytes())
            .await
            .expect("writes");
        self.line().await
    }
}

fn text(r: &McpToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| match c {
            McpContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn call(b: &Bridge, session: &str, tool: &str, args: Value) -> McpToolResult {
    let r = b.call(session, tool, args.clone()).await.expect("answer");
    println!(
        "--- {tool} {args} → error={}\n{}\n",
        r.is_error,
        text(&r).chars().take(600).collect::<String>()
    );
    r
}

/// The number of the first element on a `read_page` page whose description contains `what`.
fn ref_of(page: &str, what: &str) -> u64 {
    for (i, _) in page.match_indices('[') {
        let rest = &page[i + 1..];
        let Some((n, after)) = rest.split_once(']') else {
            continue;
        };
        let Ok(n) = n.parse::<u64>() else {
            continue;
        };
        let desc = after.split(['[', '\n']).next().unwrap_or("");
        if desc.contains(what) {
            return n;
        }
    }
    panic!("{what} on the page:\n{page}")
}

#[tokio::test]
#[ignore = "needs Chrome, Python Playwright and the built extension"]
async fn the_extension_pairs_reads_clicks_types_and_screenshots() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ext = root.join("extension/dist");
    assert!(
        ext.join("manifest.json").exists(),
        "build the extension first (npm run extension)"
    );
    let chrome = std::env::var("OXIDE_TEST_CHROME").unwrap_or_else(|_| "google-chrome".into());
    let python = std::env::var("OXIDE_TEST_PYTHON").unwrap_or_else(|_| "python3".into());
    let site = serve_site().await;
    let origin = format!("http://127.0.0.1:{site}");

    let dir = tempfile::tempdir().expect("tempdir");
    let stops = Arc::new(Stops::default());
    let bridge = Bridge::open(
        dir.path().join("chrome.json"),
        Arc::new(MemorySecrets::default()),
        stops.clone(),
    )
    .await;
    let port = {
        let l = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
        l.local_addr().expect("addr").port()
    };
    bridge
        .configure(&ChromeSettings {
            enabled: true,
            port,
            ask: true,
        })
        .await;
    let code = bridge.start_pairing().code;

    let mut child = Command::new(python)
        .arg(root.join("extension/tests/drive_chrome.py"))
        .args([
            chrome.as_str(),
            &ext.to_string_lossy(),
            &port.to_string(),
            &code,
            &origin,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("starts the driver");
    let stdin = child.stdin.take().expect("stdin");
    let out = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let mut d = Driver { child, stdin, out };
    let ready = d.line().await;
    assert!(ready.starts_with("READY"), "{ready}");
    assert!(bridge.wait_connected(Duration::from_secs(5)).await);
    let st = bridge.status();
    assert!(
        st.connected
            .as_deref()
            .is_some_and(|n| n.contains("Chrome")),
        "{st:?}"
    );
    assert_eq!(st.paired.len(), 1);

    let s = "chat-e2e";
    // Nothing open for the chat yet.
    let r = call(&bridge, s, "read_page", json!({})).await;
    assert!(r.is_error && text(&r).contains("No tab is open"));
    let r = call(&bridge, s, "tabs_list", json!({})).await;
    assert!(!r.is_error && text(&r).contains("popup.html"));

    // Open, read, find, click.
    let r = call(&bridge, s, "navigate", json!({"url": format!("{origin}/")})).await;
    let page = text(&r);
    assert!(!r.is_error, "{page}");
    assert!(page.contains("# Test bakery") && page.contains("link \"Opening hours\""));
    assert_eq!(
        d.ask("banner").await,
        "1",
        "the banner shows while Oxide works"
    );
    let r = call(&bridge, s, "get_console", json!({})).await;
    assert!(
        text(&r).contains("bakery loaded") && text(&r).contains("[error] oops 42"),
        "console messages from the page's load: {}",
        text(&r)
    );
    let r = call(&bridge, s, "find", json!({"query": "opening hours"})).await;
    assert!(text(&r).contains("link \"Opening hours\""));
    let r = call(&bridge, s, "click", json!({"text": "Opening hours"})).await;
    assert!(
        !r.is_error && text(&r).contains("Saturday 8–14"),
        "{}",
        text(&r)
    );
    let r = call(&bridge, s, "navigate", json!({"url": "back"})).await;
    assert!(text(&r).contains("Test bakery"));

    // Type and submit.
    let r = call(
        &bridge,
        s,
        "type",
        json!({"field": "Search bread", "text": "rye", "submit": true}),
    )
    .await;
    assert!(
        !r.is_error && text(&r).contains("Results for rye"),
        "{}",
        text(&r)
    );
    let r = call(&bridge, s, "navigate", json!({"url": "back"})).await;
    let page = text(&r);

    // Forms: a select, a checkbox; passwords are refused.
    let size = ref_of(&page, "select \"Size");
    let r = call(
        &bridge,
        s,
        "form_input",
        json!({"ref": size, "value": "large"}),
    )
    .await;
    assert!(!r.is_error && text(&r).contains("Large"));
    let gift = ref_of(&page, "checkbox");
    let r = call(
        &bridge,
        s,
        "form_input",
        json!({"ref": gift, "value": true}),
    )
    .await;
    assert!(!r.is_error && text(&r).contains("Checked"));
    let r = call(&bridge, s, "read_page", json!({"filter": "interactive"})).await;
    assert!(text(&r).contains("selected=\"Large\"") && text(&r).contains("(checked)"));
    let pw = ref_of(&text(&r), "textbox(password)");
    let r = call(&bridge, s, "type", json!({"ref": pw, "text": "hunter2"})).await;
    assert!(r.is_error && text(&r).contains("password"));
    let r = call(&bridge, s, "click", json!({"ref": 999})).await;
    assert!(r.is_error && text(&r).contains("read_page again"));

    // Keys, scrolling, the console, a screenshot.
    let r = call(
        &bridge,
        s,
        "scroll",
        json!({"direction": "down", "amount": 2}),
    )
    .await;
    assert!(!r.is_error && text(&r).contains("Scrolled down"));
    let r = call(&bridge, s, "press_key", json!({"key": "Home"})).await;
    assert!(!r.is_error);
    let r = call(&bridge, s, "get_console", json!({"pattern": "oops"})).await;
    assert!(!r.is_error && text(&r).contains("oops 42") && !text(&r).contains("bakery loaded"));
    let r = call(&bridge, s, "screenshot", json!({})).await;
    assert!(!r.is_error);
    let image = r.content.iter().find_map(|c| match c {
        McpContent::Image { data, mime_type } => Some((data.len(), mime_type.clone())),
        _ => None,
    });
    assert!(
        matches!(&image, Some((n, m)) if *n > 2_000 && m == "image/jpeg"),
        "{image:?}"
    );

    // A link that opens a new tab is followed; Oxide's own tabs can be closed.
    let r = call(&bridge, s, "click", json!({"text": "Hours in a new tab"})).await;
    assert!(text(&r).contains("opened a new tab") && text(&r).contains("Saturday"));
    let r = call(&bridge, s, "tabs_list", json!({})).await;
    assert!(text(&r).contains("this chat's tab"));
    let r = call(
        &bridge,
        s,
        "tabs_create",
        json!({"url": format!("{origin}/hours")}),
    )
    .await;
    assert!(!r.is_error && text(&r).contains("Opening hours"));
    let r = call(&bridge, s, "tabs_close", json!({})).await;
    assert!(!r.is_error && text(&r).contains("Closed tab"));

    // A blocked site is refused before it opens.
    assert_eq!(d.ask(&format!("block localhost:{site}")).await, "BLOCKED");
    let r = call(
        &bridge,
        s,
        "navigate",
        json!({"url": format!("http://localhost:{site}/")}),
    )
    .await;
    assert!(r.is_error && text(&r).contains("blocked"), "{}", text(&r));

    // Stop in Chrome reaches Oxide, and calls fail until Oxide releases the chat.
    assert_eq!(d.ask("stop").await, "STOPPED");
    for _ in 0..100 {
        if !stops.0.lock().expect("lock").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(*stops.0.lock().expect("lock"), vec![s.to_owned()]);
    let r = call(&bridge, s, "read_page", json!({})).await;
    assert!(r.is_error && text(&r).contains("pressed Stop"));
    bridge.release(s).await;
    let r = call(&bridge, s, "read_page", json!({})).await;
    assert!(!r.is_error, "a new reply may use Chrome again");
    // Releasing takes the banner down.
    assert_eq!(d.ask("banner").await, "1");
    bridge.release(s).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(d.ask("banner").await, "0");

    // Unpairing in Oxide reaches the extension.
    let id = bridge.status().paired[0].id.clone();
    bridge.forget(&id).await;
    assert_eq!(d.ask("state").await, "unpaired");
    assert!(!bridge.connected());

    let _ = d.stdin.write_all(b"quit\n").await;
    let _ = tokio::time::timeout(Duration::from_secs(10), d.child.wait()).await;
}
