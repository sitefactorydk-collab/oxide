//! Desktop extensions end to end: a `.mcpb` whose server is `oxide-mcp-mock` is opened,
//! installed, configured, switched off and on, updated and uninstalled.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use oxide_core::{ExtensionStatus, ExtensionValue, McpEvent, McpStatus};
use oxide_mcp::archive::write_zip;
use oxide_mcp::client::ConnectOptions;
use oxide_mcp::config::CONFIG_FILE_NAME;
use oxide_mcp::extensions::{UserDirs, secret_key};
use oxide_mcp::oauth::{MemoryTokens, TokenStore};
use oxide_mcp::{Extensions, ManagerOptions, ManagerSink, McpManager, UserAgent};
use serde_json::{Value, json};

const MOCK: &str = env!("CARGO_BIN_EXE_oxide-mcp-mock");

/// The tests here run one at a time. On Windows, `install_configure_toggle_update_uninstall`
/// failed with os error 32 (the old extension folder still in use after 20 s of retries) only
/// while the other tests ran beside it (one starts a real `node` server); alone it passed
/// every time (Phase 19, `scripts\win\ext-diag.cmd`). They test install flows, not
/// concurrency.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Sink;
impl ManagerSink for Sink {
    fn event(&self, _event: McpEvent) {}
}

struct Agent;
impl UserAgent for Agent {
    fn sample(&self, _p: Value, _m: Option<String>) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(async { Err("no".into()) })
    }
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
}

fn manifest(version: &str) -> Value {
    json!({
        "manifest_version": "0.3",
        "name": "mock-ext",
        "display_name": "Mock Extension",
        "version": version,
        "description": "The test server, packed as an extension",
        "author": {"name": "Oxide tests"},
        "icon": "icon.png",
        "server": {
            "type": "binary",
            "entry_point": "server/mock",
            "mcp_config": {
                "command": MOCK,
                "args": ["${user_config.mode}"],
                "env": {"MOCK_KEY": "${user_config.key}", "MOCK_DIR": "${__dirname}"}
            }
        },
        "tools": [{"name": "echo", "description": "Echoes"}],
        "user_config": {
            "mode": {"type": "string", "title": "Mode", "default": "modern", "required": true},
            "key": {"type": "string", "title": "API key", "sensitive": true, "required": true},
            "depth": {"type": "number", "title": "Depth", "min": 1, "max": 5}
        }
    })
}

fn bundle(dir: &Path, file: &str, manifest: &Value) -> std::path::PathBuf {
    let text = manifest.to_string();
    // A 1×1 PNG.
    let png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52,
    ];
    let zip = write_zip(&[
        ("manifest.json", text.as_bytes()),
        ("icon.png", png),
        ("server/", b""),
        (
            "server/README.txt",
            b"the server binary lives elsewhere in tests",
        ),
    ]);
    let path = dir.join(file);
    std::fs::write(&path, zip).expect("write bundle");
    path
}

struct Setup {
    manager: McpManager,
    ext: Extensions,
    tokens: Arc<MemoryTokens>,
    dir: tempfile::TempDir,
}

async fn setup(config: Value) -> Setup {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join(CONFIG_FILE_NAME);
    std::fs::write(&path, config.to_string()).expect("write");
    let tokens = Arc::new(MemoryTokens::default());
    let manager = McpManager::new(
        path,
        None,
        Arc::new(Sink),
        Arc::new(Agent),
        tokens.clone(),
        ManagerOptions {
            connect: ConnectOptions {
                probe_timeout: Duration::from_secs(5),
                ..ConnectOptions::default()
            },
            max_attempts: 2,
            backoff: Duration::from_millis(50),
            watch_every: Duration::from_millis(50),
            ..ManagerOptions::default()
        },
    );
    manager.start().await.expect("starts");
    let ext = Extensions::with(
        dir.path().join("extensions"),
        manager.clone(),
        tokens.clone(),
        UserDirs::from_home(dir.path().join("home")),
        "linux".into(),
    );
    ext.refresh().await;
    Setup {
        manager,
        ext,
        tokens,
        dir,
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

fn config_json(s: &Setup) -> Value {
    let text = std::fs::read_to_string(s.dir.path().join(CONFIG_FILE_NAME)).expect("read");
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn install_configure_toggle_update_uninstall() {
    let _serial = SERIAL.lock().await;
    let s = setup(json!({"mcpServers": {}})).await;
    let file = bundle(s.dir.path(), "mock.mcpb", &manifest("1.0.0"));

    // Open it: details, fields, defaults; nothing installed yet.
    let p = s.ext.preview(&file).await.expect("preview");
    assert_eq!(p.info.display_name, "Mock Extension");
    assert_eq!(p.info.fields.len(), 3);
    assert!(
        p.info
            .icon
            .as_deref()
            .is_some_and(|i| i.starts_with("data:image/png;base64,"))
    );
    assert_eq!(p.installed_version, None);
    assert_eq!(
        p.values.get("mode"),
        Some(&ExtensionValue::Text("modern".into()))
    );
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert!(s.ext.list().await.is_empty());

    // Out-of-range values are refused and the bundle stays open for another try.
    let bad: BTreeMap<String, ExtensionValue> =
        [("depth".to_string(), ExtensionValue::Number(9.0))].into();
    let err = s.ext.install(&p.staging_id, bad).await.expect_err("range");
    assert!(err.to_string().contains("at most 5"), "{err}");

    // Installed without the required key: it needs setup and no server runs.
    let e = s
        .ext
        .install(&p.staging_id, BTreeMap::new())
        .await
        .expect("installs");
    assert_eq!(e.status, ExtensionStatus::NeedsSetup, "{:?}", e.problem);
    assert!(e.problem.as_deref().is_some_and(|m| m.contains("API key")));
    assert!(s.manager.state("mock-ext").is_none());
    assert!(
        s.dir
            .path()
            .join("extensions/mock-ext/manifest.json")
            .is_file()
    );

    // Configure: the key goes to the keychain, the server starts.
    let values: BTreeMap<String, ExtensionValue> = [
        ("key".to_string(), ExtensionValue::Text("sk-123456".into())),
        ("depth".to_string(), ExtensionValue::Number(2.0)),
    ]
    .into();
    let e = s
        .ext
        .configure("mock-ext", values)
        .await
        .expect("configures");
    assert_eq!(e.status, ExtensionStatus::Ready, "{:?}", e.problem);
    assert_eq!(e.secrets_set, vec!["key".to_string()]);
    assert!(!e.values.contains_key("key"));
    assert!(
        wait_status(&s.manager, "mock-ext", McpStatus::Running).await,
        "{:?} {:?}",
        s.manager.state("mock-ext"),
        s.manager.logs("mock-ext")
    );
    let state = s.manager.state("mock-ext").expect("state");
    assert_eq!(state.extension.as_deref(), Some("Mock Extension"));
    assert_eq!(
        s.tokens
            .load(&secret_key("mock-ext", "key"))
            .await
            .expect("load"),
        Some("sk-123456".into())
    );
    let cfg = config_json(&s);
    assert_eq!(
        cfg["oxide"]["extensions"]["mock-ext"]["version"],
        json!("1.0.0")
    );
    assert!(cfg["mcpServers"].get("mock-ext").is_none());
    assert!(
        !cfg.to_string().contains("sk-123456"),
        "secrets stay out of the file"
    );
    // The server runs in the extension's folder with its variables filled in.
    let sc = s.manager.server_config("mock-ext").expect("config");
    assert_eq!(sc.args, vec!["modern"]);
    assert_eq!(
        sc.env.get("MOCK_KEY").map(String::as_str),
        Some("sk-123456")
    );
    assert!(sc.env["MOCK_DIR"].ends_with("mock-ext"));

    // Connectors can't remove or rename it; switching it off and on works.
    assert!(s.manager.remove_server("mock-ext").await.is_err());
    s.ext.set_enabled("mock-ext", false).await.expect("off");
    assert!(wait_status(&s.manager, "mock-ext", McpStatus::Disabled).await);
    s.manager.set_enabled("mock-ext", true).await.expect("on");
    assert!(wait_status(&s.manager, "mock-ext", McpStatus::Running).await);

    // An update keeps the settings (the stored key isn't asked for again).
    let file2 = bundle(s.dir.path(), "mock2.mcpb", &manifest("1.1.0"));
    let p2 = s.ext.preview(&file2).await.expect("preview 2");
    assert_eq!(p2.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(p2.values.get("depth"), Some(&ExtensionValue::Number(2.0)));
    assert_eq!(p2.secrets_set, vec!["key".to_string()]);
    let e = s
        .ext
        .install(&p2.staging_id, p2.values.clone())
        .await
        .expect("updates");
    assert_eq!(e.version, "1.1.0");
    assert_eq!(e.status, ExtensionStatus::Ready);
    assert!(wait_status(&s.manager, "mock-ext", McpStatus::Running).await);

    // Uninstall: server, files, entry and secret are gone.
    s.ext.uninstall("mock-ext").await.expect("uninstalls");
    for _ in 0..100 {
        if s.manager.state("mock-ext").is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(s.manager.state("mock-ext").is_none());
    // Every version folder goes; one Windows still holds (an antivirus scan, under load) is
    // removed in the background shortly after.
    let ext_root = s.dir.path().join("extensions");
    let left = || {
        std::fs::read_dir(&ext_root)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("mock-ext"))
                    .count()
            })
            .unwrap_or(0)
    };
    for _ in 0..300 {
        if left() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(left(), 0, "extension folders left behind");
    assert!(config_json(&s)["oxide"].get("extensions").is_none());
    assert_eq!(
        s.tokens
            .load(&secret_key("mock-ext", "key"))
            .await
            .expect("load"),
        None
    );
    assert!(s.ext.list().await.is_empty());
}

#[tokio::test]
async fn bad_bundles_are_refused_and_leave_nothing_behind() {
    let _serial = SERIAL.lock().await;
    let s = setup(json!({"mcpServers": {"mock-ext": {"command": MOCK, "args": ["modern"]}}})).await;
    let staging = s.dir.path().join("extensions/.staging");
    let count = || std::fs::read_dir(&staging).map(|d| d.count()).unwrap_or(0);

    // A connector of the same name already exists.
    let file = bundle(s.dir.path(), "clash.mcpb", &manifest("1.0.0"));
    let err = s.ext.preview(&file).await.expect_err("clash");
    assert!(err.to_string().contains("already set up"), "{err}");

    // No manifest.
    let file = s.dir.path().join("empty.mcpb");
    std::fs::write(&file, write_zip(&[("readme.txt", b"hi")])).expect("write");
    let err = s.ext.preview(&file).await.expect_err("no manifest");
    assert!(err.to_string().contains("manifest.json"), "{err}");

    // A path that escapes the folder.
    let file = s.dir.path().join("evil.mcpb");
    std::fs::write(&file, write_zip(&[("../../evil.txt", b"x")])).expect("write");
    assert!(s.ext.preview(&file).await.is_err());
    assert!(!s.dir.path().join("evil.txt").exists());

    // Not a zip.
    let file = s.dir.path().join("text.mcpb");
    std::fs::write(&file, "hello").expect("write");
    assert!(s.ext.preview(&file).await.is_err());

    // Made for another OS: it can be opened (with a warning) and is listed as unsupported.
    let mut mac = manifest("1.0.0");
    mac["name"] = json!("mac-only");
    mac["compatibility"] = json!({"platforms": ["darwin"]});
    let file = bundle(s.dir.path(), "mac.mcpb", &mac);
    let p = s.ext.preview(&file).await.expect("opens");
    assert!(
        p.warnings.iter().any(|w| w.contains("macOS")),
        "{:?}",
        p.warnings
    );
    let staged = count();
    assert_eq!(staged, 1, "only the open bundle is staged");
    let values = [("key".to_string(), ExtensionValue::Text("k-1".into()))].into();
    let e = s
        .ext
        .install(&p.staging_id, values)
        .await
        .expect("installs");
    assert_eq!(e.status, ExtensionStatus::Unsupported);
    assert!(s.manager.state("mac-only").is_none());

    // Discarding an opened bundle removes its files.
    let mut other = manifest("1.0.0");
    other["name"] = json!("other");
    let file = bundle(s.dir.path(), "other.mcpb", &other);
    let p = s.ext.preview(&file).await.expect("opens");
    assert_eq!(count(), 1);
    s.ext.discard(&p.staging_id).await;
    assert_eq!(count(), 0);
    assert!(s.ext.install(&p.staging_id, BTreeMap::new()).await.is_err());
}

/// The sample bundle in `docs/samples` (what the Phase 10 exit check installs).
#[tokio::test]
async fn the_sample_bundle_installs_and_runs_with_node() {
    let _serial = SERIAL.lock().await;
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/samples/oxide-dice.mcpb");
    let s = setup(json!({"mcpServers": {}})).await;
    let p = s.ext.preview(&sample).await.expect("opens the sample");
    assert_eq!(p.info.display_name, "Oxide Dice");
    assert_eq!(p.info.fields.len(), 4);
    assert!(p.info.icon.is_some());
    assert_eq!(
        p.values.get("lucky_number"),
        Some(&ExtensionValue::Number(7.0))
    );
    if !p.warnings.is_empty() {
        // No Node.js here: the bundle was still read correctly.
        eprintln!("skipping the run: {:?}", p.warnings);
        return;
    }
    let mut values = p.values.clone();
    values.insert(
        "api_token".into(),
        ExtensionValue::Text("secret-token".into()),
    );
    values.insert("shout".into(), ExtensionValue::Bool(true));
    let e = s
        .ext
        .install(&p.staging_id, values)
        .await
        .expect("installs");
    assert_eq!(e.status, ExtensionStatus::Ready, "{:?}", e.problem);
    assert!(
        wait_status(&s.manager, "oxide-dice", McpStatus::Running).await,
        "{:?} {:?}",
        s.manager.state("oxide-dice"),
        s.manager.logs("oxide-dice")
    );
    let r = s
        .manager
        .call_tool("oxide-dice", "settings_check", json!({}), None)
        .await
        .expect("calls");
    let text = format!("{:?}", r.content);
    assert!(text.contains("token: set (12 characters)"), "{text}");
    assert!(text.contains("shout: true"), "{text}");
    assert!(text.contains("lucky number: 7"), "{text}");
    let r = s
        .manager
        .call_tool("oxide-dice", "greet", json!({"name": "Ada"}), None)
        .await
        .expect("calls");
    assert!(format!("{:?}", r.content).contains("HELLO, ADA!"));
}
