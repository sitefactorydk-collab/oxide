use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use oxide_core::{
    APP_MANIFEST, AppCapability, AppErrorCode, AppGenerationOverride, AppPrefs, GenerationKind,
    Role,
};
use oxide_llm::ChatEvent;
use oxide_store::{MemorySecrets, SecretStore, Store};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::UtilityApps;
use super::runtime::{AppRuntime, CallEvents};
use crate::ModelRegistry;
use crate::chat::tests::{FakeProvider, Step, fake_connector};
use crate::tools::Tools;
use crate::tools::tests::{FakeMcp, tool};

struct Fixture {
    _dir: tempfile::TempDir,
    apps: UtilityApps,
    runtime: AppRuntime,
    fake: FakeProvider,
    secrets: Arc<dyn SecretStore>,
    store: Store,
}

async fn fixture(scripts: Vec<Vec<Step>>) -> Fixture {
    fixture_with(scripts, None).await
}

async fn fixture_with(scripts: Vec<Vec<Step>>, mcp: Option<Arc<FakeMcp>>) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::open_in_memory().await.expect("store");
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
    let fake = FakeProvider::new(scripts).vision();
    let registry = Arc::new(ModelRegistry::with_connector(
        store.clone(),
        secrets.clone(),
        fake_connector(fake.clone()),
    ));
    let tools = Tools::new(
        mcp.map(|m| m as Arc<dyn crate::tools::McpTools>),
        secrets.clone(),
        None,
    );
    let apps = UtilityApps::new(dir.path().join("apps"), store.clone(), secrets.clone());
    let runtime = AppRuntime::new(apps.clone(), registry, tools, dir.path().join("app-work"));
    Fixture {
        _dir: dir,
        apps,
        runtime,
        fake,
        secrets,
        store,
    }
}

fn text(t: &str) -> Step {
    Step::Event(ChatEvent::TextDelta(t.into()))
}

fn quiet() -> CallEvents {
    Arc::new(|_| {})
}

fn recorder() -> (CallEvents, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    (Arc::new(move |v| s.lock().expect("lock").push(v)), seen)
}

const META: &str = "ai-meta-tag-generator";
const TABLE: &str = "ai-html-table-from-image";

/// Lets an app use the fake model and grants it `caps`.
async fn allow(f: &Fixture, id: &str, caps: &[AppCapability]) {
    f.apps
        .save_prefs(
            id,
            AppPrefs {
                text_model: Some("lmstudio/fake-model".into()),
                ..AppPrefs::default()
            },
        )
        .await
        .expect("prefs");
    for c in caps {
        f.apps.grant(id, *c).await.expect("grant");
    }
}

/// An own app (a clone of the table app) whose manifest declares `caps`.
async fn own_app(f: &Fixture, caps: &str) -> String {
    let copy = f.apps.clone_app(TABLE).await.expect("clone");
    let id = copy.manifest.id.clone();
    let manifest = format!(r#"{{"id":"{id}","name":"Mine","capabilities":{caps}}}"#);
    f.apps
        .save_file(&id, APP_MANIFEST, &manifest)
        .await
        .expect("manifest");
    id
}

#[tokio::test]
async fn built_in_apps_are_listed_and_read_only() {
    let f = fixture(vec![]).await;
    let list = f.apps.list().await;
    let ids: Vec<&str> = list.iter().map(|a| a.manifest.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            META,
            "readability-tone-analyzer",
            TABLE,
            "ai-svg-creator",
            "multi-purpose-ai-translator",
            "ai-plugin-creator",
            "llms-txt-generator",
            "sp-page-builder-addon-creator",
            "elementor-widget-creator",
        ]
    );
    assert!(list.iter().all(|a| a.builtin && a.problem.is_none()));
    let b = f.apps.bundle(META).await.expect("bundle");
    assert!(b.files.iter().any(|f| f.path == "App.tsx"));
    assert!(f.apps.save_file(META, "App.tsx", "x").await.is_err());
    assert!(f.apps.delete(META).await.is_err());
    assert!(f.apps.bundle("../etc").await.is_err());
}

#[tokio::test]
async fn a_clone_is_an_editable_own_app() {
    let f = fixture(vec![]).await;
    let copy = f.apps.clone_app(META).await.expect("clone");
    assert_eq!(copy.manifest.id, "ai-meta-tag-generator-copy");
    assert_eq!(copy.manifest.name, "AI Meta Tag Generator (copy)");
    assert!(!copy.builtin);
    let again = f.apps.clone_app(META).await.expect("clone again");
    assert_eq!(again.manifest.id, "ai-meta-tag-generator-copy-2");

    let id = copy.manifest.id.clone();
    f.apps
        .save_file(
            &id,
            "App.tsx",
            "export default function App() { return null; }",
        )
        .await
        .expect("saves");
    f.apps
        .save_file(&id, "lib/util.ts", "export const x = 1;")
        .await
        .expect("new file");
    let b = f.apps.bundle(&id).await.expect("bundle");
    assert!(b.files.iter().any(|f| f.path == "lib/util.ts"));
    assert!(b.files.iter().any(|f| f.content.contains("return null")));
    // The manifest is checked, and the id can't change.
    assert!(f.apps.save_file(&id, APP_MANIFEST, "{").await.is_err());
    let other = r#"{"id":"other","name":"X"}"#;
    assert!(f.apps.save_file(&id, APP_MANIFEST, other).await.is_err());
    assert!(f.apps.save_file(&id, "../escape.ts", "x").await.is_err());

    let list = f.apps.list().await;
    let builtin = super::builtin::all().len();
    assert_eq!(list.len(), builtin + 2);
    assert!(list[builtin].manifest.name.contains("(copy)"));

    f.apps.delete(&again.manifest.id).await.expect("deletes");
    assert_eq!(f.apps.list().await.len(), builtin + 1);
}

#[tokio::test]
async fn export_and_import_round_trip_without_settings() {
    let f = fixture(vec![]).await;
    let copy = f.apps.clone_app(TABLE).await.expect("clone");
    let id = copy.manifest.id.clone();
    allow(&f, &id, &[AppCapability::Text]).await;
    f.apps
        .set_key(&id, GenerationKind::Image, Some("sk-app".into()))
        .await
        .expect("key");
    let bytes = f.apps.export_bytes(&id).await.expect("exports");

    // Re-importing the same id replaces the app and forgets its grants and keys.
    let p = f.apps.preview_import(bytes).await.expect("preview");
    assert_eq!(p.manifest.id, id);
    assert!(p.replaces);
    assert_eq!(p.manifest.capabilities, vec![AppCapability::Text]);
    let installed = f.apps.install_import(&p.token).await.expect("installs");
    assert_eq!(installed.manifest.id, id);
    assert!(f.apps.prefs(&id).await.expect("prefs").granted.is_empty());
    assert!(f.apps.keys(&id).await.is_empty());
    assert!(
        f.apps.install_import(&p.token).await.is_err(),
        "a token is used once"
    );

    // A built-in app's export comes in under a new id.
    let builtin = f.apps.export_bytes(TABLE).await.expect("exports");
    let p = f.apps.preview_import(builtin).await.expect("preview");
    assert_eq!(p.renamed_from.as_deref(), Some(TABLE));
    let app = f.apps.install_import(&p.token).await.expect("installs");
    assert_ne!(app.manifest.id, TABLE);
    assert!(!app.builtin);

    // Unsafe or broken files are refused before anything is installed.
    let evil = oxide_mcp::archive::write_zip(&[
        (APP_MANIFEST, br#"{"id":"evil","name":"E"}"#.as_slice()),
        ("App.tsx", b"x".as_slice()),
        ("../outside.txt", b"x".as_slice()),
    ]);
    assert!(f.apps.preview_import(evil).await.is_err());
    let no_entry =
        oxide_mcp::archive::write_zip(&[(APP_MANIFEST, br#"{"id":"e","name":"E"}"#.as_slice())]);
    let e = f
        .apps
        .preview_import(no_entry)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("entry file"), "{e}");
    assert!(f.apps.preview_import(b"not a zip".to_vec()).await.is_err());
    // A zip with one top folder is fine.
    let nested = oxide_mcp::archive::write_zip(&[
        (
            "my-app/oxide-app.json",
            br#"{"id":"nested","name":"N"}"#.as_slice(),
        ),
        ("my-app/App.tsx", b"export default () => null".as_slice()),
    ]);
    let p = f.apps.preview_import(nested).await.expect("nested");
    assert_eq!(p.files, 2);
}

#[tokio::test]
async fn capabilities_ask_once_per_app_and_can_be_revoked() {
    let f = fixture(vec![vec![text("hello")]]).await;
    let rt = &f.runtime;
    let e = rt
        .call(
            META,
            "c1",
            "text.generate",
            json!({ "prompt": "hi" }),
            quiet(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, AppErrorCode::NeedsPermission);
    assert_eq!(e.capability, Some(AppCapability::Text));
    assert!(e.message.contains("AI Meta Tag Generator"), "{}", e.message);
    // Not declared in the manifest: refused even when asked.
    let e = rt
        .call(
            META,
            "c1",
            "python.run",
            json!({ "code": "print(1)" }),
            quiet(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, AppErrorCode::Undeclared);
    assert!(f.apps.grant(META, AppCapability::Python).await.is_err());
    let e = rt
        .call(META, "c1", "fs.read", json!({}), quiet())
        .await
        .unwrap_err();
    assert_eq!(e.code, AppErrorCode::Unknown);

    allow(&f, META, &[AppCapability::Text]).await;
    let r = rt
        .call(
            META,
            "c1",
            "text.generate",
            json!({ "prompt": "hi" }),
            quiet(),
        )
        .await
        .expect("runs");
    assert_eq!(r["text"], "hello");
    assert_eq!(r["model"], "lmstudio/fake-model");

    // Revoking (saving the settings without the grant) asks again.
    let mut prefs = f.apps.prefs(META).await.expect("prefs");
    prefs.granted.clear();
    f.apps.save_prefs(META, prefs).await.expect("saves");
    let e = rt
        .call(
            META,
            "c1",
            "text.generate",
            json!({ "prompt": "hi" }),
            quiet(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, AppErrorCode::NeedsPermission);
    // The settings dialog can't add grants by itself.
    let sneaky = AppPrefs {
        granted: vec![AppCapability::Text],
        ..AppPrefs::default()
    };
    let saved = f.apps.save_prefs(META, sneaky).await.expect("saves");
    assert!(saved.granted.is_empty());
}

#[tokio::test]
async fn text_streams_takes_images_and_reads_json() {
    let f = fixture(vec![
        vec![text("Hel"), text("lo")],
        vec![text("```json\n{\"suggestions\": [1]}\n```")],
        vec![text("sorry, no")],
        vec![text("{\"ok\": true}")],
    ])
    .await;
    allow(&f, TABLE, &[AppCapability::Text]).await;
    let (events, seen) = recorder();
    let r = f
        .runtime
        .call(
            TABLE,
            "c",
            "text.generate",
            json!({
                "system": "Be brief.",
                "prompt": "What is in the image?",
                "images": [{ "mediaType": "image/png", "data": "aGk=" }],
                "stream": true,
            }),
            events,
        )
        .await
        .expect("runs");
    assert_eq!(r["text"], "Hello");
    assert_eq!(r["truncated"], false);
    let deltas: Vec<Value> = seen.lock().expect("lock").clone();
    assert_eq!(
        deltas,
        vec![
            json!({"type":"delta","text":"Hel"}),
            json!({"type":"delta","text":"lo"})
        ]
    );
    let req = &f.fake.requests()[0];
    assert_eq!(req.system.as_deref(), Some("Be brief."));
    let last = req.messages.last().expect("message");
    assert_eq!(last.role, Role::User);
    assert!(matches!(
        last.content[0],
        oxide_core::ContentBlock::Image { .. }
    ));

    let r = f
        .runtime
        .call(
            TABLE,
            "c",
            "text.generate",
            json!({ "prompt": "x", "json": {"type":"object"} }),
            quiet(),
        )
        .await
        .expect("json");
    assert_eq!(r["json"], json!({"suggestions": [1]}));
    let sys = f.fake.requests()[1].system.clone().unwrap_or_default();
    assert!(sys.contains("JSON Schema"), "{sys}");

    // Not JSON at first: one more try, telling the model.
    let r = f
        .runtime
        .call(
            TABLE,
            "c",
            "text.generate",
            json!({ "prompt": "x", "json": true }),
            quiet(),
        )
        .await
        .expect("retried");
    assert_eq!(r["json"], json!({"ok": true}));
    let retry = &f.fake.requests()[3];
    assert_eq!(retry.messages.len(), 3);
}

#[tokio::test]
async fn storage_is_the_apps_own_and_needs_no_grant() {
    let f = fixture(vec![]).await;
    let rt = &f.runtime;
    rt.call(
        META,
        "s",
        "storage.set",
        json!({"key":"k","value":{"a":[1,2]}}),
        quiet(),
    )
    .await
    .expect("sets");
    let v = rt
        .call(META, "s", "storage.get", json!({"key":"k"}), quiet())
        .await
        .expect("gets");
    assert_eq!(v, json!({"a":[1,2]}));
    let other = rt
        .call(TABLE, "s", "storage.get", json!({"key":"k"}), quiet())
        .await
        .expect("gets");
    assert_eq!(other, Value::Null);
    assert_eq!(
        rt.call(META, "s", "storage.keys", json!({}), quiet())
            .await
            .expect("keys"),
        json!(["k"])
    );
    f.apps.clear_storage(META).await.expect("clears");
    assert_eq!(f.store.app_storage_bytes(META).await.expect("bytes"), 0);
}

#[tokio::test]
async fn connectors_are_listed_and_called() {
    let mcp = Arc::new(FakeMcp {
        tools: vec![tool("files", "read", true)],
        ..FakeMcp::default()
    });
    let f = fixture_with(vec![], Some(mcp)).await;
    let id = own_app(&f, r#"["connectors"]"#).await;
    f.apps
        .grant(&id, AppCapability::Connectors)
        .await
        .expect("grant");
    let list = f
        .runtime
        .call(&id, "c", "connectors.list", json!({}), quiet())
        .await
        .expect("lists");
    assert_eq!(list[0]["server"], "files");
    assert_eq!(list[0]["readOnly"], true);
    let r = f
        .runtime
        .call(
            &id,
            "c",
            "connectors.call",
            json!({"server":"files","tool":"read","arguments":{"p":1}}),
            quiet(),
        )
        .await
        .expect("calls");
    assert_eq!(r["isError"], false);
}

#[tokio::test]
async fn generation_keys_are_per_app_and_removed_with_it() {
    let f = fixture(vec![]).await;
    let id = own_app(&f, r#"["image"]"#).await;
    f.apps
        .grant(&id, AppCapability::Image)
        .await
        .expect("grant");
    let e = f
        .runtime
        .call(&id, "g", "media.image", json!({"prompt":"a cat"}), quiet())
        .await
        .unwrap_err();
    // These tools have no Generation: the call says so instead of failing oddly.
    assert!(e.message.contains("isn't available"), "{}", e.message);
    let prefs = AppPrefs {
        image: AppGenerationOverride {
            provider_id: None,
            base_url: "http://127.0.0.1:9/v1".into(),
            model: "flux".into(),
        },
        ..AppPrefs::default()
    };
    f.apps.save_prefs(&id, prefs).await.expect("prefs");
    f.apps
        .set_key(&id, GenerationKind::Image, Some("sk-own".into()))
        .await
        .expect("key");
    assert_eq!(f.apps.keys(&id).await, vec![GenerationKind::Image]);
    let name = oxide_core::app_secret_name(&id, GenerationKind::Image);
    assert!(matches!(f.secrets.get(&name), Ok(Some(_))));
    f.apps.delete(&id).await.expect("deletes");
    assert!(matches!(f.secrets.get(&name), Ok(None)));
    assert_eq!(f.apps.prefs(&id).await.expect("prefs"), AppPrefs::default());
}

/// A tiny HTTP server answering fixed paths.
async fn site(pages: HashMap<&'static str, (&'static str, String)>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("addr");
    let pages = Arc::new(pages);
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let pages = pages.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, ctype, body) = match pages.get(path.as_str()) {
                    Some((ct, b)) => ("200 OK", *ct, b.clone()),
                    None => ("404 Not Found", "text/plain", "nope".to_owned()),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

fn page(title: &str, body: &str) -> (&'static str, String) {
    (
        "text/html",
        format!(
            "<!doctype html><html><head><title>{title}</title></head><body>{body}</body></html>"
        ),
    )
}

#[tokio::test]
async fn web_fetch_gives_markdown_or_html_and_crawl_respects_robots() {
    let mut pages = HashMap::new();
    pages.insert(
        "/robots.txt",
        (
            "text/plain",
            "User-agent: *\nDisallow: /private\n".to_owned(),
        ),
    );
    pages.insert(
        "/",
        page(
            "Home",
            "<h1>Welcome</h1><p>Hello there.</p><a href=\"/a\">A</a> \
             <a href=\"/private/x\">P</a> <a href=\"https://elsewhere.example/\">E</a> \
             <a href=\"/logo.png\">L</a>",
        ),
    );
    pages.insert("/a", page("A", "<p>Page A</p><a href=\"/b\">B</a>"));
    pages.insert("/b", page("B", "<p>Page B</p>"));
    let base = site(pages).await;
    let f = fixture(vec![]).await;
    allow(&f, META, &[AppCapability::WebFetch]).await;
    let rt = &f.runtime;
    let md = rt
        .call(
            META,
            "w",
            "web.fetch",
            json!({ "url": format!("{base}/") }),
            quiet(),
        )
        .await
        .expect("fetches");
    assert_eq!(md["title"], "Home");
    assert!(
        md["content"].as_str().unwrap_or("").contains("# Welcome"),
        "{md}"
    );
    let raw = rt
        .call(
            META,
            "w",
            "web.fetch",
            json!({ "url": format!("{base}/"), "format": "html" }),
            quiet(),
        )
        .await
        .expect("fetches");
    assert!(
        raw["content"]
            .as_str()
            .unwrap_or("")
            .contains("<h1>Welcome</h1>")
    );

    let r = rt
        .call(
            META,
            "w",
            "web.crawl",
            json!({ "url": format!("{base}/"), "maxPages": 10, "maxDepth": 2 }),
            quiet(),
        )
        .await
        .expect("crawls");
    let urls: Vec<String> = r["pages"]
        .as_array()
        .expect("pages")
        .iter()
        .map(|p| p["url"].as_str().unwrap_or("").replace(&base, ""))
        .collect();
    assert_eq!(urls, vec!["/", "/a", "/b"]);
    assert_eq!(r["robots"], true);
    let skipped = r["skipped"].to_string();
    assert!(
        skipped.contains("/private/x") && skipped.contains("robots.txt"),
        "{skipped}"
    );
    let one = rt
        .call(
            META,
            "w",
            "web.crawl",
            json!({ "url": format!("{base}/"), "maxDepth": 0 }),
            quiet(),
        )
        .await
        .expect("crawls");
    assert_eq!(one["pages"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_running_call_can_be_cancelled() {
    let f = fixture(vec![vec![Step::Gate, text("late")]]).await;
    allow(&f, META, &[AppCapability::Text]).await;
    let rt = &f.runtime;
    let run = rt.call(
        META,
        "slow",
        "text.generate",
        json!({"prompt":"x"}),
        quiet(),
    );
    let stop = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        rt.cancel("slow");
    };
    let (r, ()) = tokio::join!(run, stop);
    assert_eq!(r.unwrap_err().code, AppErrorCode::Cancelled);
}

#[tokio::test]
async fn the_apps_folder_is_set_by_the_picker_and_reached_with_its_grant() {
    let f = fixture(vec![]).await;
    let id = own_app(&f, r#"["folder"]"#).await;
    let picked = tempfile::tempdir().expect("tempdir");
    let get = f
        .runtime
        .call(&id, "c1", "folder.get", json!({}), quiet())
        .await
        .expect("get");
    assert_eq!(get, Value::Null);
    let e = f
        .runtime
        .call(&id, "c2", "folder.list", json!({}), quiet())
        .await
        .expect_err("asks first");
    assert_eq!(e.code, AppErrorCode::NeedsPermission);

    // Picking the folder allows the app to use it.
    let prefs = f
        .apps
        .set_folder(&id, Some(picked.path()))
        .await
        .expect("sets");
    assert!(prefs.allows(AppCapability::Folder));
    f.runtime
        .call(
            &id,
            "c3",
            "folder.write",
            json!({ "path": "plg_x/x.xml", "text": "<extension/>" }),
            quiet(),
        )
        .await
        .expect("writes");
    assert_eq!(
        std::fs::read_to_string(picked.path().join("plg_x/x.xml")).expect("file"),
        "<extension/>"
    );
    let list = f
        .runtime
        .call(&id, "c4", "folder.list", json!({ "depth": 3 }), quiet())
        .await
        .expect("lists");
    assert_eq!(list["entries"][1]["path"], "plg_x/x.xml");
    let get = f
        .runtime
        .call(&id, "c5", "folder.get", json!({}), quiet())
        .await
        .expect("get");
    assert_eq!(get["exists"], true);

    // The settings dialog can't move the folder; Oxide's own data can't be picked; an app
    // that doesn't declare the capability can't get a folder.
    let sneaky = AppPrefs {
        folder: Some("/".into()),
        granted: prefs.granted.clone(),
        ..AppPrefs::default()
    };
    let saved = f.apps.save_prefs(&id, sneaky).await.expect("saves");
    assert_eq!(saved.folder, prefs.folder);
    assert!(f.apps.set_folder(&id, Some(f._dir.path())).await.is_err());
    assert!(f.apps.set_folder(META, Some(picked.path())).await.is_err());

    // Forgetting it takes the grant too.
    let cleared = f.apps.set_folder(&id, None).await.expect("clears");
    assert!(cleared.folder.is_none());
    assert!(!cleared.allows(AppCapability::Folder));
}

#[tokio::test]
async fn json_calls_ask_for_structured_output_and_explain_a_cut_off_answer() {
    let f = fixture(vec![
        // Structured output: the schema goes to the provider.
        vec![text("{\"ok\": true}")],
        // A server that refuses it: asked again without it.
        vec![Step::Reject(400, "response_format is not supported")],
        vec![text("{\"ok\": 2}")],
        // Cut off at the output limit: no second try, a message that says why.
        vec![
            text("{\"files\": [{\"path\": \"a.php\", \"content\": \"<?php"),
            Step::Event(ChatEvent::Stop(oxide_core::StopReason::MaxTokens)),
        ],
    ])
    .await;
    allow(&f, TABLE, &[AppCapability::Text]).await;
    let schema = json!({"type": "object", "properties": {"ok": {}}});
    let call = |id: &'static str| {
        f.runtime.call(
            TABLE,
            id,
            "text.generate",
            json!({ "prompt": "x", "json": schema }),
            quiet(),
        )
    };
    let r = call("a").await.expect("json");
    assert_eq!(r["json"], json!({"ok": true}));
    assert_eq!(f.fake.requests()[0].json_schema.as_ref(), Some(&schema));

    let r = call("b").await.expect("json without structured output");
    assert_eq!(r["json"], json!({"ok": 2}));
    let reqs = f.fake.requests();
    assert!(reqs[1].json_schema.is_some());
    assert!(reqs[2].json_schema.is_none());
    assert!(
        reqs[2]
            .system
            .as_deref()
            .unwrap_or("")
            .contains("JSON Schema")
    );

    let e = call("c").await.expect_err("cut off");
    assert!(e.message.contains("cut off"), "{}", e.message);
    assert!(e.message.contains("Context Length"), "{}", e.message);
    assert_eq!(f.fake.requests().len(), 4);
}

#[tokio::test]
async fn ai_edit_applies_edit_blocks_retries_once_and_falls_back_to_a_rewrite() {
    let site = "<?php\nclass A {\n    public function render() {\n        $title = 'x';\n        return $title;\n    }\n\n    public function css() {\n        return '';\n    }\n}\n";
    let blocks =
        |old: &str, new: &str| text(&json!({"edits": [{"old": old, "new": new}]}).to_string());
    let f = fixture(vec![
        // 1. Edit blocks that apply at once (quoted without indentation).
        vec![blocks(
            "$title = 'x';\nreturn $title;",
            "$title = 'x';\n$subtitle = 'y';\nreturn $title . $subtitle;",
        )],
        // 2. A misquoted block, then the corrected one.
        vec![blocks("return '' ;", "return 'a{}';")],
        vec![blocks("        return '';", "        return 'a{}';")],
        // 3. Wrong twice: the whole file is written instead (in a fence).
        vec![blocks("nope", "x")],
        vec![blocks("still nope", "x")],
        vec![text("Here is the file:\n```php\n<?php\n// new\n```")],
        // 4. A rewrite asked for, cut off at the output limit.
        vec![
            text("<?php\nclass B {"),
            Step::Event(ChatEvent::Stop(oxide_core::StopReason::MaxTokens)),
        ],
    ])
    .await;
    allow(&f, TABLE, &[AppCapability::Text]).await;
    let (events, seen) = recorder();
    let edit = |id: &'static str, extra: Value, events: CallEvents| {
        let mut p = json!({ "path": "site.php", "content": site, "request": "Add a subtitle" });
        if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        f.runtime.call(TABLE, id, "text.edit", p, events)
    };

    let r = edit("a", json!({ "context": "Other files: none" }), quiet())
        .await
        .expect("edits");
    assert_eq!(r["rewritten"], false);
    assert_eq!(r["retried"], false);
    assert_eq!(r["changed"], true);
    let content = r["content"].as_str().expect("content");
    assert_eq!(
        content,
        site.replace(
            "        return $title;\n",
            "        $subtitle = 'y';\n        return $title . $subtitle;\n"
        )
    );
    assert_eq!(
        r["summary"],
        "1 edit applied: lines 4–6. Edit 1 matched ignoring indentation (the new text was re-indented)."
    );
    let req = &f.fake.requests()[0];
    assert!(
        req.json_schema
            .as_ref()
            .is_some_and(|s| s["properties"]["edits"].is_object())
    );
    assert!(req.system.as_deref().unwrap_or("").contains("edit blocks"));
    let asked = oxide_core::message::plain_text(&req.messages[0].content);
    assert!(asked.starts_with("Other files: none"), "{asked}");
    assert!(asked.contains("<file path=\"site.php\">"), "{asked}");

    let r = edit("b", json!({}), events).await.expect("corrected");
    assert_eq!(r["retried"], true);
    assert!(
        r["content"]
            .as_str()
            .expect("content")
            .contains("return 'a{}';")
    );
    let reqs = f.fake.requests();
    let retry = &reqs[2].messages;
    assert_eq!(retry.len(), 3);
    assert_eq!(retry[1].role, Role::Assistant);
    let said = oxide_core::message::plain_text(&retry[2].content);
    assert!(said.contains("couldn't be applied"), "{said}");
    assert!(
        said.contains("return '';"),
        "the closest line is quoted: {said}"
    );
    let stages: Vec<String> = seen
        .lock()
        .expect("lock")
        .iter()
        .filter_map(|e| e["stage"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(stages, vec!["edits", "retry"]);

    let r = edit("c", json!({}), quiet()).await.expect("rewritten");
    assert_eq!(r["rewritten"], true);
    assert_eq!(r["content"], "<?php\n// new\n");
    assert!(
        r["reason"]
            .as_str()
            .expect("reason")
            .contains("didn't apply either"),
        "{r}"
    );
    assert!(f.fake.requests()[5].json_schema.is_none());

    let e = edit("d", json!({ "mode": "rewrite" }), quiet())
        .await
        .expect_err("cut off");
    assert!(e.message.contains("cut off"), "{}", e.message);
    assert!(e.message.contains("wasn't changed"), "{}", e.message);
    assert_eq!(f.fake.requests().len(), 7);
}
