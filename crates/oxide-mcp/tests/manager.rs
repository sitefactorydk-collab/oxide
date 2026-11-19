//! The server manager against real `oxide-mcp-mock` processes.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use oxide_core::{
    McpAnswer, McpAsk, McpContent, McpEvent, McpServerConfig, McpStatus, McpTransport,
};
use oxide_mcp::client::ConnectOptions;
use oxide_mcp::config::CONFIG_FILE_NAME;
use oxide_mcp::oauth::MemoryTokens;
use oxide_mcp::{ManagerOptions, ManagerSink, McpManager, UserAgent};
use serde_json::{Value, json};

const MOCK: &str = env!("CARGO_BIN_EXE_oxide-mcp-mock");

#[derive(Default)]
struct Sink(Mutex<Vec<McpEvent>>);

impl ManagerSink for Sink {
    fn event(&self, event: McpEvent) {
        self.0.lock().expect("lock").push(event);
    }
}

impl Sink {
    fn events(&self) -> Vec<McpEvent> {
        self.0.lock().expect("lock").clone()
    }
}

struct Agent;

impl UserAgent for Agent {
    fn sample(
        &self,
        _params: Value,
        model: Option<String>,
    ) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(async move {
            Ok(
                json!({"role": "assistant", "model": model.unwrap_or_default(),
                      "content": {"type": "text", "text": "from the model"}}),
            )
        })
    }
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
}

fn mock_entry(mode: &str) -> Value {
    json!({"command": MOCK, "args": [mode]})
}

struct Setup {
    manager: McpManager,
    sink: Arc<Sink>,
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

async fn setup(config: Value) -> Setup {
    setup_with(config, false).await
}

/// `trust`: remember trusted commands in `<dir>/mcp-trust.json` (seeded at the first start).
async fn setup_with(config: Value, trust: bool) -> Setup {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join(CONFIG_FILE_NAME);
    std::fs::write(&path, config.to_string()).expect("write");
    let sink = Arc::new(Sink::default());
    let manager = McpManager::new(
        path.clone(),
        Some(dir.path().join("logs")),
        sink.clone(),
        Arc::new(Agent),
        Arc::new(MemoryTokens::default()),
        ManagerOptions {
            connect: ConnectOptions {
                probe_timeout: Duration::from_secs(5),
                ..ConnectOptions::default()
            },
            max_attempts: 2,
            backoff: Duration::from_millis(50),
            ask_timeout: Duration::from_secs(10),
            watch_every: Duration::from_millis(50),
            cwd: None,
            trust_file: trust.then(|| dir.path().join("mcp-trust.json")),
        },
    );
    manager.start().await.expect("starts");
    Setup {
        manager,
        sink,
        _dir: dir,
        path,
    }
}

async fn wait_status(m: &McpManager, name: &str, status: McpStatus) -> bool {
    for _ in 0..250 {
        if m.state(name).is_some_and(|s| s.status == status) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn servers_start_list_call_and_stop() {
    let s = setup(json!({"mcpServers": {
        "modern": mock_entry("modern"),
        "legacy": mock_entry("legacy"),
    }}))
    .await;
    let m = &s.manager;
    assert!(
        wait_status(m, "modern", McpStatus::Running).await,
        "{:?}",
        m.state("modern")
    );
    assert!(
        wait_status(m, "legacy", McpStatus::Running).await,
        "{:?}",
        m.logs("legacy")
    );
    // Lists arrive just after the status flips; wait for them too.
    for _ in 0..100 {
        if m.state("modern").is_some_and(|st| st.tools == 6) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let state = m.state("modern").expect("state");
    assert_eq!(state.tools, 6);
    assert_eq!(state.prompts, 1);
    assert_eq!(state.server_name.as_deref(), Some("mock"));
    assert_eq!(
        m.lists("modern").expect("lists").resources[0].uri,
        "mock://readme"
    );
    assert_eq!(
        m.states()
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["legacy", "modern"]
    );

    let r = m
        .call_tool("legacy", "echo", json!({"text": "yo"}), Some("tok".into()))
        .await
        .expect("calls");
    assert!(matches!(&r.content[0], McpContent::Text { text } if text == "yo"));
    assert!(
        m.logs("legacy")
            .iter()
            .any(|l| l.text.starts_with("connected"))
    );
    assert!(
        m.logs("legacy")
            .iter()
            .any(|l| l.text.contains("mock server starting"))
    );
    assert!(
        s.sink
            .events()
            .iter()
            .any(|e| matches!(e, McpEvent::Log { server, .. } if server == "legacy"))
    );

    m.shutdown().await;
    assert_eq!(m.state("modern").expect("state").status, McpStatus::Stopped);
}

#[tokio::test]
async fn asks_go_to_the_user_and_back() {
    let s =
        setup(json!({"mcpServers": {"m": mock_entry("modern"), "l": mock_entry("legacy")}})).await;
    let m = s.manager.clone();
    assert!(wait_status(&m, "m", McpStatus::Running).await);
    assert!(wait_status(&m, "l", McpStatus::Running).await);
    // Answer every question: forms with a colour, sampling approved with a model.
    let answerer = {
        let m = m.clone();
        tokio::spawn(async move {
            loop {
                for ask in m.pending_asks() {
                    let answer = match ask.ask {
                        McpAsk::Elicitation { .. } => McpAnswer::Elicitation {
                            action: "accept".into(),
                            content: Some(r#"{"color":"red"}"#.into()),
                        },
                        McpAsk::Sampling { .. } => McpAnswer::Sampling {
                            approved: true,
                            model: Some("p/m".into()),
                        },
                    };
                    m.answer(&ask.id, answer);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
    };
    for server in ["m", "l"] {
        let r = m
            .call_tool(server, "ask", json!({}), None)
            .await
            .expect("ask");
        let McpContent::Text { text } = &r.content[0] else {
            panic!()
        };
        assert!(text.starts_with("accept red"), "{server}: {text}");
        let r = m
            .call_tool(server, "sample", json!({}), None)
            .await
            .expect("sample");
        assert!(matches!(&r.content[0], McpContent::Text { text } if text == "from the model"));
    }
    answerer.abort();
    let events = s.sink.events();
    assert!(events.iter().any(|e| matches!(e, McpEvent::Ask { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, McpEvent::AskClosed { .. }))
    );
    assert!(m.pending_asks().is_empty());
    m.shutdown().await;
}

#[tokio::test]
async fn config_edits_apply_live() {
    let s = setup(json!({"mcpServers": {"a": mock_entry("modern")}, "keep": 1})).await;
    let m = &s.manager;
    assert!(wait_status(m, "a", McpStatus::Running).await);

    m.set_enabled("a", false).await.expect("disables");
    assert!(wait_status(m, "a", McpStatus::Disabled).await);
    m.set_enabled("a", true).await.expect("enables");
    assert!(wait_status(m, "a", McpStatus::Running).await);

    // A hand edit of the file is picked up by the watcher.
    std::fs::write(
        &s.path,
        json!({"mcpServers": {"b": mock_entry("legacy")}, "keep": 1}).to_string(),
    )
    .expect("write");
    assert!(wait_status(m, "b", McpStatus::Running).await);
    assert!(m.state("a").is_none());
    assert!(
        s.sink
            .events()
            .iter()
            .any(|e| matches!(e, McpEvent::Removed { name } if name == "a"))
    );

    // Saving through the manager keeps unknown keys.
    let cfg = McpServerConfig {
        name: "c".into(),
        transport: McpTransport::Stdio,
        command: MOCK.into(),
        args: vec!["modern".into()],
        env: Default::default(),
        url: String::new(),
        headers: Default::default(),
        enabled: true,
    };
    m.save_server(cfg.clone(), None).await.expect("saves");
    assert!(
        m.save_server(cfg, None).await.is_err(),
        "duplicate names are refused"
    );
    assert!(wait_status(m, "c", McpStatus::Running).await);
    let text = m.config_file().await.expect("file").text;
    assert!(text.contains("\"keep\": 1"));
    m.remove_server("c").await.expect("removes");
    assert!(m.state("c").is_none());

    // Invalid editor text is refused and changes nothing.
    assert!(m.save_config_text("{ nope").await.is_err());
    assert!(m.state("b").is_some());
    m.shutdown().await;
}

#[tokio::test]
async fn failing_servers_report_and_stop_retrying() {
    let s = setup(json!({"mcpServers": {
        "missing": {"command": "no-such-program-oxide-test", "args": []},
        "crashy": mock_entry("legacy-crash"),
        "bad": {"args": []},
    }}))
    .await;
    let m = &s.manager;
    // Dies on the modern probe, then works as a legacy server.
    assert!(
        wait_status(m, "crashy", McpStatus::Running).await,
        "{:?}",
        m.logs("crashy")
    );
    assert!(wait_status(m, "missing", McpStatus::Error).await);
    for _ in 0..100 {
        if m.state("missing")
            .and_then(|s| s.error)
            .is_some_and(|e| e.contains("stopped retrying"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let err = m.state("missing").and_then(|s| s.error).expect("error");
    assert!(
        err.contains("Couldn't find") && err.contains("stopped retrying"),
        "{err}"
    );
    let bad = m.state("bad").expect("bad");
    assert_eq!(bad.status, McpStatus::Error);
    assert!(bad.error.expect("why").contains("command"));
    m.shutdown().await;
}

/// Tauri runs synchronous commands on the main thread, outside the runtime: restarting from
/// there must not panic ("no reactor running").
#[tokio::test]
async fn restart_works_from_a_thread_outside_the_runtime() {
    let s = setup(json!({"mcpServers": {"modern": mock_entry("modern")}})).await;
    let m = s.manager.clone();
    assert!(wait_status(&m, "modern", McpStatus::Running).await);
    let from_thread = m.clone();
    std::thread::spawn(move || from_thread.restart("modern"))
        .join()
        .expect("the thread doesn't panic")
        .expect("restarts");
    assert!(
        wait_status(&m, "modern", McpStatus::Running).await,
        "{:?}",
        m.logs("modern")
    );
    assert!(
        m.logs("modern")
            .iter()
            .filter(|l| l.text.starts_with("connected"))
            .count()
            >= 2,
        "{:?}",
        m.logs("modern")
    );
    m.shutdown().await;
}

#[tokio::test]
async fn new_or_changed_local_commands_wait_until_trusted() {
    // The first start trusts what is already configured (an upgrade keeps working).
    let s = setup_with(json!({"mcpServers": {"old": mock_entry("modern")}}), true).await;
    let m = &s.manager;
    assert!(
        wait_status(m, "old", McpStatus::Running).await,
        "{:?}",
        m.state("old")
    );

    // Another program adds a server to the file: it waits.
    std::fs::write(
        &s.path,
        json!({"mcpServers": {"old": mock_entry("modern"), "added": mock_entry("legacy")}})
            .to_string(),
    )
    .expect("write");
    assert!(
        wait_status(m, "added", McpStatus::NeedsTrust).await,
        "{:?}",
        m.state("added")
    );
    let st = m.state("added").expect("state");
    assert!(!st.trust_changed);
    assert!(st.summary.contains("legacy"));
    // A restart doesn't get around it.
    m.restart("added").expect("restart");
    assert!(wait_status(m, "added", McpStatus::NeedsTrust).await);

    // The user trusts it: it starts, and stays trusted across a restart of Oxide.
    m.trust("added").expect("trust");
    assert!(
        wait_status(m, "added", McpStatus::Running).await,
        "{:?}",
        m.state("added")
    );

    // Its command changes behind Oxide's back: it stops and asks again, marked as changed.
    std::fs::write(
        &s.path,
        json!({"mcpServers": {"old": mock_entry("modern"), "added": {"command": MOCK, "args": ["legacy"], "env": {"X": "1"}}}})
            .to_string(),
    )
    .expect("write");
    assert!(wait_status(m, "added", McpStatus::NeedsTrust).await);
    assert!(m.state("added").expect("state").trust_changed);

    // Saved from Settings: trusted as saved.
    let mut cfg = m.server_config("added").expect("config");
    cfg.name = "added".into();
    m.save_server(cfg, Some("added".into()))
        .await
        .expect("save");
    assert!(
        wait_status(m, "added", McpStatus::Running).await,
        "{:?}",
        m.state("added")
    );
    m.shutdown().await;

    // The trust file holds fingerprints only (no env values).
    let text = std::fs::read_to_string(s._dir.path().join("mcp-trust.json")).expect("file");
    assert!(text.contains("\"added\""));
    assert!(!text.contains("legacy"));
}
