//! The Phase 13 exit check: the browser agent opens, reads, clicks and screenshots a local
//! web site, with live frames, and (when a model is given) Browser-Use finds the opening
//! hours on its own.
//!
//! Ignored by default. Two ways to run it:
//! - **Windows (real):** `OXIDE_UV` = the pinned uv; it installs Python, the packages and the
//!   browser group into `OXIDE_TEST_PYTHON_ROOT` (or a temporary folder) and uses Chrome or
//!   Edge (else downloads Chromium). `scripts\win\phase13.ps1` runs it.
//! - **Linux (stand-in):** `OXIDE_TEST_SYSTEM_PYTHON` = a Python that has `mcp` 2.x and
//!   Playwright, `OXIDE_TEST_CHROME` = a Chrome.
//!
//! With `OXIDE_TEST_LLM_KIND`, `OXIDE_TEST_LLM_BASE_URL`, `OXIDE_TEST_LLM_MODEL` (and
//! `OXIDE_TEST_LLM_API_KEY`) it also runs `browser_task`.
//!
//! `cargo test -p oxide-pyenv --test real_browser -- --ignored --nocapture`

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use oxide_core::browser::{BROWSER_TOOLS, BrowserSettings, BrowserSource};
use oxide_core::{
    BrowserFrame, BrowserStatus, InstalledBrowser, McpContent, McpToolResult, ProviderKind,
};
use oxide_pyenv::browser::find_installed_browsers;
use oxide_pyenv::{BrowserAgents, BrowserLlm, BrowserSink, NoSink, PythonConfig, PythonEnv};
use serde_json::json;

const HOME: &str = "<!doctype html><title>Bakery</title><h1>The Little Bakery</h1>\
<p>Fresh bread every day.</p><nav><a href=\"/hours\">Opening hours</a></nav>\
<form action=\"/search\"><input name=\"q\" placeholder=\"Find bread\"><button>Go</button></form>";
const HOURS: &str = "<!doctype html><title>Hours</title><h2>Opening hours</h2>\
<ul><li>Mon-Fri: 7-18</li><li>Sat: 8-14</li><li>Sun: closed</li></ul>";

/// A tiny web site on 127.0.0.1.
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("an address");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, body) = if path == "/" {
                    ("200 OK", HOME.to_owned())
                } else if path == "/hours" {
                    ("200 OK", HOURS.to_owned())
                } else if let Some(q) = path.strip_prefix("/search?q=") {
                    (
                        "200 OK",
                        format!("<title>Results</title><p>You searched for {q}</p>"),
                    )
                } else {
                    ("404 Not Found", String::from("nope"))
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    format!("http://{addr}")
}

#[derive(Default)]
struct Collect {
    frames: Mutex<Vec<BrowserFrame>>,
    statuses: Mutex<Vec<BrowserStatus>>,
}

impl BrowserSink for Collect {
    fn status(&self, s: &BrowserStatus) {
        self.statuses.lock().expect("lock").push(s.clone());
    }
    fn frame(&self, f: BrowserFrame) {
        self.frames.lock().expect("lock").push(f);
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

fn images(r: &McpToolResult) -> usize {
    r.content
        .iter()
        .filter(|c| matches!(c, McpContent::Image { .. }))
        .count()
}

/// The environment and the browsers to use, for the mode the variables ask for.
fn setup(root: &std::path::Path) -> (PythonEnv, Vec<InstalledBrowser>) {
    let installed = match std::env::var("OXIDE_TEST_CHROME") {
        Ok(p) if !p.is_empty() => vec![InstalledBrowser {
            name: "Test Chrome".into(),
            path: p,
        }],
        _ => find_installed_browsers(),
    };
    #[cfg(unix)]
    if let Some(py) = std::env::var_os("OXIDE_TEST_SYSTEM_PYTHON") {
        let env = PythonEnv::for_tests(&root.join("python"), &PathBuf::from(py));
        env.mark_group_for_tests("browser");
        return (env, installed);
    }
    let uv = std::env::var_os("OXIDE_UV")
        .map(PathBuf::from)
        .expect("set OXIDE_UV (or OXIDE_TEST_SYSTEM_PYTHON on Linux)");
    let mut cfg = PythonConfig::new(root.join("python"));
    cfg.uv = Some(uv);
    (PythonEnv::new(cfg, Arc::new(NoSink)), installed)
}

/// A stand-in for Browser-Use (Linux mode): its agent opens the site's /hours page over CDP
/// in the same browser and reports the Saturday line, like the real one would.
const FAKE_BROWSER_USE: &str = r#"
import asyncio, re

class ChatOpenAI:
    def __init__(self, model, api_key=None, base_url=None):
        self.model = model

class ChatAnthropic(ChatOpenAI):
    pass

class Browser:
    def __init__(self, cdp_url=None, keep_alive=None):
        self.cdp_url = cdp_url

class History:
    def __init__(self, text):
        self.text = text
    def final_result(self):
        return self.text
    def is_done(self):
        return True
    def is_successful(self):
        return True

class Agent:
    def __init__(self, task, llm=None, browser=None, use_vision=True):
        self.task, self.browser = task, browser
    async def run(self, max_steps=100):
        from playwright.async_api import async_playwright
        url = re.search(r"(http://\S+)", self.task).group(1)
        async with async_playwright() as p:
            b = await p.chromium.connect_over_cdp(self.browser.cdp_url)
            page = b.contexts[0].pages[-1]
            await page.goto(url + "/hours")
            await asyncio.sleep(3.5)
            text = await page.inner_text("body")
        return History([l for l in text.splitlines() if l.startswith("Sat")][0])
"#;

fn llm() -> Option<BrowserLlm> {
    if std::env::var("OXIDE_TEST_FAKE_BROWSER_USE").is_ok_and(|v| v == "1") {
        return Some(BrowserLlm {
            kind: ProviderKind::LmStudio,
            base_url: "http://127.0.0.1:1/v1".into(),
            model: "stand-in".into(),
            api_key: None,
            vision: false,
        });
    }
    let kind = ProviderKind::parse(&std::env::var("OXIDE_TEST_LLM_KIND").ok()?)?;
    Some(BrowserLlm {
        kind,
        base_url: std::env::var("OXIDE_TEST_LLM_BASE_URL").ok()?,
        model: std::env::var("OXIDE_TEST_LLM_MODEL").ok()?,
        api_key: std::env::var("OXIDE_TEST_LLM_API_KEY")
            .ok()
            .filter(|k| !k.is_empty()),
        vision: std::env::var("OXIDE_TEST_LLM_VISION").is_ok_and(|v| v == "1"),
    })
}

#[tokio::test]
#[ignore = "installs packages and runs a real browser"]
async fn the_browser_agent_opens_reads_clicks_and_screenshots() {
    let site = serve();
    let temp = tempfile::tempdir().expect("a temporary folder");
    let root = std::env::var_os("OXIDE_TEST_PYTHON_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_path_buf());
    let (env, installed) = setup(&root);
    println!(
        "browsers on this computer: {:?}",
        installed.iter().map(|b| &b.name).collect::<Vec<_>>()
    );
    let sink = Arc::new(Collect::default());
    let agents = BrowserAgents::with_installed(env, installed, sink.clone());
    agents.configure(&BrowserSettings {
        enabled: true,
        source: BrowserSource::Auto,
        task_timeout_secs: 300,
        ..BrowserSettings::default()
    });

    let status = agents.status().await;
    println!(
        "before: packages={} using={:?}",
        status.packages, status.using
    );

    // Open: sets up what is missing, starts the agent and its browser.
    let r = agents
        .call("chat-1", "navigate", json!({"url": site}), "call-1", None)
        .await
        .expect("navigate runs");
    let t = text(&r);
    println!("navigate:\n{t}\n");
    assert!(!r.is_error, "{t}");
    assert!(t.contains("# The Little Bakery"), "{t}");
    assert!(t.contains("[1] link \"Opening hours\""), "{t}");
    assert!(agents.is_open("chat-1"));
    let status = agents.status().await;
    assert!(status.packages, "{status:?}");
    assert_eq!(status.open_chats, vec!["chat-1".to_owned()]);

    // The server offers exactly the tools Oxide describes to models.
    let mut names = agents.server_tools("chat-1").await.expect("tools");
    names.sort();
    let mut want: Vec<String> = BROWSER_TOOLS.iter().map(|s| (*s).to_owned()).collect();
    want.sort();
    assert_eq!(names, want);

    let r = agents
        .call("chat-1", "click", json!({"ref": 1}), "call-2", None)
        .await
        .expect("click runs");
    let t = text(&r);
    assert!(!r.is_error && t.contains("Sun: closed"), "{t}");

    let r = agents
        .call("chat-1", "read_page", json!({}), "call-3", None)
        .await
        .expect("read_page runs");
    assert!(text(&r).contains("Sat: 8-14"));

    let r = agents
        .call("chat-1", "screenshot", json!({}), "call-4", None)
        .await
        .expect("screenshot runs");
    assert_eq!(images(&r), 1, "{}", text(&r));

    let r = agents
        .call("chat-1", "click", json!({"ref": 42}), "call-5", None)
        .await
        .expect("a bad click still answers");
    assert!(r.is_error && text(&r).contains("read_page again"));

    // Live frames arrived for the actions, tagged with their calls.
    {
        let frames = sink.frames.lock().expect("lock");
        assert!(
            frames.iter().any(|f| f.tool_use_id == "call-1"
                && f.image.is_some()
                && f.conversation_id == "chat-1"),
            "frames: {}",
            frames.len()
        );
        assert!(frames.iter().any(|f| f.tool_use_id == "call-2"));
    }

    // A real Google search (Windows run): the results lead to the museum's own site.
    if std::env::var("OXIDE_TEST_REAL_SEARCH").is_ok_and(|v| v == "1") {
        let r = agents
            .call(
                "chat-1",
                "search",
                json!({"query": "National Museum of Denmark opening hours"}),
                "call-s",
                None,
            )
            .await
            .expect("search runs");
        let t = text(&r);
        println!("search:\n{}\n", t.chars().take(3000).collect::<String>());
        // Either real results, or a plainly refused search (a hidden browser may be shown a
        // check for robots, which Oxide never tries to get around).
        if r.is_error {
            assert!(t.contains("The search was refused"), "{t}");
        } else {
            assert!(
                t.to_lowercase().contains("natmus"),
                "no natmus.dk link in the results: {t}"
            );
        }
        // A made-up address on that site is searched for instead (when it answers 404).
        let r = agents
            .call(
                "chat-1",
                "navigate",
                json!({"url": "https://natmus.dk/this-page-does-not-exist-oxide-test"}),
                "call-d",
                None,
            )
            .await
            .expect("navigate runs");
        let t = text(&r);
        println!(
            "dead address:\n{}\n",
            t.chars().take(1500).collect::<String>()
        );
        // Sites differ (some answer a missing page with 200): only reported.
        println!(
            "dead address searched for instead: {}",
            t.contains("Searched for")
        );
    }
    if std::env::var("OXIDE_TEST_FAKE_BROWSER_USE").is_ok_and(|v| v == "1") {
        let dir = root.join("python").join("agents").join("browser_use");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("__init__.py"), FAKE_BROWSER_USE).expect("write");
    }
    if let Some(llm) = llm() {
        println!("browser_task with {llm:?}");
        let r = agents
            .call(
                "chat-1",
                "browser_task",
                json!({"goal": format!("Open {site} and find the bakery's opening hours on Saturday. Report them."), "max_steps": 8}),
                "call-6",
                Some(llm),
            )
            .await
            .expect("browser_task answers");
        let t = text(&r);
        println!(
            "browser_task ({} screenshots, error={}):\n{t}",
            images(&r),
            r.is_error
        );
        for bad in [
            "isn't available",
            "TypeError",
            "AttributeError",
            "ImportError",
        ] {
            assert!(!t.contains(bad), "Browser-Use didn't run: {t}");
        }
        assert!(images(&r) >= 1, "no screenshots: {t}");
        if std::env::var("OXIDE_TEST_FAKE_BROWSER_USE").is_ok_and(|v| v == "1") {
            assert!(!r.is_error && t.contains("Sat: 8-14"), "{t}");
        }
        let frames = sink.frames.lock().expect("lock");
        assert!(
            frames.iter().any(|f| f.tool_use_id == "call-6"),
            "no live frames"
        );
    } else {
        println!("browser_task skipped: set OXIDE_TEST_LLM_* to run it");
    }

    agents.close("chat-1").await;
    assert!(!agents.is_open("chat-1"));
    // A new call starts it again (and the page is fresh).
    let r = agents
        .call("chat-1", "read_page", json!({}), "call-7", None)
        .await
        .expect("read_page runs");
    assert!(r.is_error && text(&r).contains("navigate first"));
    agents.shutdown().await;
    assert!(agents.open_chats().is_empty());
}
