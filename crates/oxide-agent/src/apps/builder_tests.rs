use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use oxide_core::Usage;
use oxide_core::{
    APP_MANIFEST, AppCapability, AppErrorCode, CheckpointKind, ContentBlock, GenerateOptions,
    PreviewConsoleLine, PreviewReport, PreviewRequestKind, StopReason,
};
use oxide_llm::ChatEvent;
use oxide_store::{MemorySecrets, SecretStore, Store};
use serde_json::{Value, json};

use super::*;
use crate::ModelRegistry;
use crate::apps::runtime::AppRuntime;
use crate::chat::tests::{FakeProvider, Recorder, Step, fake_connector};
use crate::tools::Tools;
use crate::{ChatService, UtilityApps};

const MODEL: &str = "lmstudio/fake-model";

/// Answers preview requests the way the builder's Preview would.
#[derive(Default)]
struct FakePreview {
    builder: OnceLock<AppBuilder>,
    report: Mutex<Option<PreviewReport>>,
    requests: Mutex<Vec<(String, PreviewRequestKind)>>,
    changed: Mutex<Vec<String>>,
}

impl BuilderSink for FakePreview {
    fn draft_changed(&self, app_id: &str) {
        self.changed.lock().expect("lock").push(app_id.to_owned());
    }
    fn preview_request(&self, request_id: &str, app_id: &str, kind: PreviewRequestKind) {
        self.requests
            .lock()
            .expect("lock")
            .push((app_id.to_owned(), kind));
        let report = self.report.lock().expect("lock").clone();
        if let (Some(b), Some(r)) = (self.builder.get().cloned(), report) {
            let id = request_id.to_owned();
            tokio::spawn(async move {
                b.answer(&id, r);
            });
        }
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    store: Store,
    builder: AppBuilder,
    preview: Arc<FakePreview>,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::open_in_memory().await.expect("store");
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
    let apps = UtilityApps::new(dir.path().join("apps"), store.clone(), secrets);
    let builder = AppBuilder::new(apps);
    let preview = Arc::new(FakePreview::default());
    let _ = preview.builder.set(builder.clone());
    builder.set_sink(preview.clone());
    builder.set_preview_timeout(Duration::from_millis(300));
    Fixture {
        dir,
        store,
        builder,
        preview,
    }
}

fn text_of(out: &ToolOutput) -> String {
    out.content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn run(f: &Fixture, id: &str, op: BuilderOp, input: Value) -> ToolOutput {
    f.builder.run(id, op, &input).await
}

/// Every file under `root`, relative, `/`-separated.
fn tree(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn a_new_app_is_a_draft_with_a_chat_and_a_start_checkpoint() {
    let f = fixture().await;
    let draft = f.builder.open(None, MODEL).await.expect("open");
    let id = draft.app.manifest.id.clone();
    assert_eq!(id, "new-app");
    assert!(draft.app.problem.is_none(), "{:?}", draft.app.problem);
    assert!(!draft.published);
    assert!(draft.unsaved);
    assert!(draft.files.iter().any(|f| f.path == "App.tsx"));
    assert_eq!(
        f.store
            .builder_app_of(&draft.conversation_id)
            .await
            .expect("ok"),
        Some(id.clone())
    );
    // The chat isn't in the chat list; the draft isn't in the gallery but in Drafts.
    assert!(f.store.list_conversations().await.expect("list").is_empty());
    let apps = f.builder.apps();
    assert!(apps.list().await.iter().all(|a| a.manifest.id != id));
    assert_eq!(
        apps.list_drafts()
            .await
            .iter()
            .map(|a| a.manifest.id.as_str())
            .collect::<Vec<_>>(),
        vec![id.as_str()]
    );
    let cps = f.builder.checkpoints(&id).await.expect("checkpoints");
    assert_eq!(cps.len(), 1);
    assert_eq!(cps[0].kind, CheckpointKind::Start);
    // A second new app gets its own id; opening again keeps the chat and adds no checkpoint.
    let other = f.builder.open(None, MODEL).await.expect("open");
    assert_eq!(other.app.manifest.id, "new-app-2");
    let again = f.builder.open(Some(&id), MODEL).await.expect("reopen");
    assert_eq!(again.conversation_id, draft.conversation_id);
    assert_eq!(f.builder.checkpoints(&id).await.expect("ok").len(), 1);
}

#[tokio::test]
async fn builder_tools_stay_inside_the_draft() {
    let f = fixture().await;
    let id = f
        .builder
        .open(None, MODEL)
        .await
        .expect("open")
        .app
        .manifest
        .id;
    for bad in [
        "../escape.tsx",
        "/abs.tsx",
        "a\\b.tsx",
        "C:/x.tsx",
        "c:x.tsx",
        "a/../../b.tsx",
        "./",
        "",
    ] {
        let out = run(
            &f,
            &id,
            BuilderOp::Write,
            json!({"path": bad, "content": "x"}),
        )
        .await;
        assert!(out.is_error, "{bad} was written");
    }
    let out = run(
        &f,
        &id,
        BuilderOp::Rename,
        json!({"from": "App.tsx", "to": "../App.tsx"}),
    )
    .await;
    assert!(out.is_error);
    let out = run(&f, &id, BuilderOp::Read, json!({"path": "../apps/x"})).await;
    assert!(out.is_error);
    // Nothing was written outside the draft's folder.
    let all = tree(f.dir.path());
    assert!(
        all.iter()
            .all(|p| p.starts_with(&format!("app-drafts/{id}/"))),
        "{all:?}"
    );
    // The manifest keeps its id and stays valid; it and the entry can't be deleted.
    let out = run(
        &f,
        &id,
        BuilderOp::Write,
        json!({"path": APP_MANIFEST, "content": "{\"id\": \"other\", \"name\": \"X\"}"}),
    )
    .await;
    assert!(
        text_of(&out).contains("can't be changed"),
        "{}",
        text_of(&out)
    );
    for p in [APP_MANIFEST, "App.tsx"] {
        assert!(
            run(&f, &id, BuilderOp::Delete, json!({"path": p}))
                .await
                .is_error
        );
    }
}

#[tokio::test]
async fn files_are_written_edited_renamed_listed_and_deleted() {
    let f = fixture().await;
    let id = f
        .builder
        .open(None, MODEL)
        .await
        .expect("open")
        .app
        .manifest
        .id;
    let out = run(
        &f,
        &id,
        BuilderOp::Write,
        json!({"path": "lib/csv.ts", "content": "export const a = 1;\nexport const b = 1;\n"}),
    )
    .await;
    assert!(!out.is_error, "{}", text_of(&out));
    assert!(text_of(&out).starts_with("Created lib/csv.ts"));
    // An ambiguous edit is refused, replace_all or a unique edit works.
    let out = run(
        &f,
        &id,
        BuilderOp::Edit,
        json!({"path": "lib/csv.ts", "old_text": "= 1", "new_text": "= 2"}),
    )
    .await;
    assert!(text_of(&out).contains("found 2 times"), "{}", text_of(&out));
    let out = run(
        &f,
        &id,
        BuilderOp::Edit,
        json!({"path": "lib/csv.ts", "old_text": "= 1", "new_text": "= 2", "replace_all": true}),
    )
    .await;
    assert!(!out.is_error);
    let out = run(&f, &id, BuilderOp::Read, json!({"path": "lib/csv.ts"})).await;
    assert_eq!(text_of(&out), "export const a = 2;\nexport const b = 2;\n");
    let out = run(
        &f,
        &id,
        BuilderOp::Edit,
        json!({"path": "lib/csv.ts", "old_text": "nope", "new_text": "x"}),
    )
    .await;
    assert!(out.is_error);
    // Several edit blocks in one call, the second misquoted: nothing changes, and the error
    // quotes the closest line.
    let out = run(
        &f,
        &id,
        BuilderOp::Edit,
        json!({"path": "lib/csv.ts", "edits": [
            {"old_text": "export const a = 2;", "new_text": "export const a = 3;"},
            {"old_text": "export const bb = 2;", "new_text": "export const b = 3;"}
        ]}),
    )
    .await;
    assert!(out.is_error);
    assert!(text_of(&out).contains("Edit 2 of 2"), "{}", text_of(&out));
    assert!(
        text_of(&out).contains("export const b = 2;"),
        "{}",
        text_of(&out)
    );
    let out = run(&f, &id, BuilderOp::Read, json!({"path": "lib/csv.ts"})).await;
    assert_eq!(text_of(&out), "export const a = 2;\nexport const b = 2;\n");
    let out = run(
        &f,
        &id,
        BuilderOp::Edit,
        json!({"path": "lib/csv.ts", "edits": [
            {"old_text": "export const a = 2;", "new_text": "export const a = 3;"},
            {"old_text": "export const b = 2;", "new_text": "export const b = 3;"}
        ]}),
    )
    .await;
    assert!(!out.is_error, "{}", text_of(&out));
    assert!(
        text_of(&out).starts_with("Edited lib/csv.ts: 2 edits applied: lines 1–2."),
        "{}",
        text_of(&out)
    );
    let out = run(&f, &id, BuilderOp::Read, json!({"path": "lib/csv.ts"})).await;
    assert_eq!(text_of(&out), "export const a = 3;\nexport const b = 3;\n");
    // Renaming the entry file updates the manifest.
    let out = run(
        &f,
        &id,
        BuilderOp::Rename,
        json!({"from": "App.tsx", "to": "src/Main.tsx"}),
    )
    .await;
    assert!(!out.is_error, "{}", text_of(&out));
    let state = f.builder.state(&id).await.expect("state");
    assert_eq!(state.app.manifest.entry, "src/Main.tsx");
    assert!(state.app.problem.is_none());
    let listed = text_of(&run(&f, &id, BuilderOp::List, json!({})).await);
    assert!(
        listed.contains("src/Main.tsx") && !listed.contains("App.tsx"),
        "{listed}"
    );
    let out = run(&f, &id, BuilderOp::Delete, json!({"path": "lib/csv.ts"})).await;
    assert!(!out.is_error);
    assert!(
        !f.dir
            .path()
            .join("app-drafts")
            .join(&id)
            .join("lib")
            .exists()
    );
    // The manifest tool changes fields and checks capabilities.
    let out = run(
        &f,
        &id,
        BuilderOp::Manifest,
        json!({"name": "CSV Chart", "icon": "bar-chart-3", "category": "Data",
               "capabilities": ["text", "webFetch"]}),
    )
    .await;
    assert!(!out.is_error, "{}", text_of(&out));
    let m = f.builder.state(&id).await.expect("state").app.manifest;
    assert_eq!(
        (m.id.as_str(), m.name.as_str(), m.category.as_str()),
        (id.as_str(), "CSV Chart", "Data")
    );
    assert_eq!(
        m.capabilities,
        vec![AppCapability::Text, AppCapability::WebFetch]
    );
    let out = run(
        &f,
        &id,
        BuilderOp::Manifest,
        json!({"capabilities": ["teleport"]}),
    )
    .await;
    assert!(text_of(&out).contains("isn't a capability"));
    // Each change told the window.
    assert!(f.preview.changed.lock().expect("lock").len() >= 5);
}

#[tokio::test]
async fn checkpoints_follow_turns_and_saves_and_restore_goes_back() {
    let f = fixture().await;
    let draft = f.builder.open(None, MODEL).await.expect("open");
    let id = draft.app.manifest.id.clone();
    let original = draft
        .files
        .iter()
        .find(|f| f.path == "App.tsx")
        .expect("app")
        .content
        .clone();
    // A model turn that changed nothing adds no checkpoint.
    f.builder.checkpoint_turn(&id, &draft.conversation_id).await;
    assert_eq!(f.builder.checkpoints(&id).await.expect("ok").len(), 1);
    f.store
        .add_message(
            &draft.conversation_id,
            None,
            oxide_core::Role::User,
            vec![ContentBlock::text("make it blue\nthanks")],
            None,
        )
        .await
        .expect("message");
    run(
        &f,
        &id,
        BuilderOp::Write,
        json!({"path": "App.tsx", "content": "export default () => 'blue';"}),
    )
    .await;
    f.builder.checkpoint_turn(&id, &draft.conversation_id).await;
    let saved = f
        .builder
        .save_file(&id, "App.tsx", "export default () => 'red';")
        .await
        .expect("save");
    assert!(saved.files.iter().any(|f| f.content.contains("red")));
    let cps = f.builder.checkpoints(&id).await.expect("ok");
    assert_eq!(
        cps.iter()
            .map(|c| (c.kind, c.label.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (CheckpointKind::Manual, "Saved App.tsx"),
            (CheckpointKind::Model, "After: make it blue"),
            (CheckpointKind::Start, "Start"),
        ]
    );
    // Each checkpoint says what changed since the one before it.
    assert!(cps[2].changes.is_empty());
    let change = |c: &oxide_core::AppCheckpoint| {
        c.changes
            .iter()
            .map(|x| (x.path.clone(), x.kind, x.added, x.removed))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        change(&cps[0]),
        vec![(
            "App.tsx".to_owned(),
            oxide_core::FileChangeKind::Changed,
            1,
            1
        )]
    );
    assert_eq!(change(&cps[1])[0].0, "App.tsx");
    assert_eq!(change(&cps[1])[0].1, oxide_core::FileChangeKind::Changed);
    // Restore the start: the files come back and the red version is kept.
    let start = cps.last().expect("start").id.clone();
    let restored = f.builder.restore(&id, &start).await.expect("restore");
    let app = restored
        .files
        .iter()
        .find(|f| f.path == "App.tsx")
        .expect("app");
    assert_eq!(app.content, original);
    // The red version was the newest checkpoint already, so none was added for it ...
    let cps = f.builder.checkpoints(&id).await.expect("ok");
    assert_eq!(cps.len(), 3);
    // ... and the restore can be undone with it.
    let back = f.builder.restore(&id, &cps[0].id).await.expect("undo");
    assert!(back.files.iter().any(|f| f.content.contains("red")));
    // Restoring away from files no checkpoint has keeps them first.
    run(
        &f,
        &id,
        BuilderOp::Write,
        json!({"path": "App.tsx", "content": "export default () => 'green';"}),
    )
    .await;
    f.builder.restore(&id, &start).await.expect("restore");
    let cps = f.builder.checkpoints(&id).await.expect("ok");
    assert_eq!(
        (cps[0].kind, cps[0].label.as_str()),
        (CheckpointKind::Restore, "Before restoring")
    );
    // Another app's checkpoint can't be restored here.
    let other = f.builder.open(None, MODEL).await.expect("other");
    assert!(
        f.builder
            .restore(&other.app.manifest.id, &start)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_preview_reports_errors_console_and_screenshots_to_the_model() {
    let f = fixture().await;
    let id = f
        .builder
        .open(None, MODEL)
        .await
        .expect("open")
        .app
        .manifest
        .id;
    // No preview open: the model hears so (after the timeout).
    let out = run(&f, &id, BuilderOp::Status, json!({})).await;
    assert!(out.is_error);
    assert!(text_of(&out).contains("didn't answer"));
    *f.preview.report.lock().expect("lock") = Some(PreviewReport {
        rendered: false,
        errors: vec!["App.tsx: Unexpected token (3:4)".into()],
        console: vec![PreviewConsoleLine {
            level: "warn".into(),
            text: "careful".into(),
        }],
        screenshot: None,
        note: None,
    });
    let out = run(&f, &id, BuilderOp::Status, json!({})).await;
    let t = text_of(&out);
    assert!(!out.is_error);
    assert!(
        t.contains("Errors (1)") && t.contains("Unexpected token"),
        "{t}"
    );
    assert!(t.contains("[warn] careful"), "{t}");
    *f.preview.report.lock().expect("lock") = Some(PreviewReport {
        rendered: true,
        screenshot: Some("data:image/png;base64,iVBORw0KGgo=".into()),
        ..PreviewReport::default()
    });
    let out = run(&f, &id, BuilderOp::Screenshot, json!({})).await;
    assert!(text_of(&out).contains("without errors"));
    assert!(out.content.iter().any(|b| matches!(
        b,
        ContentBlock::Image { media_type, .. } if media_type == "image/png"
    )));
    let kinds: Vec<PreviewRequestKind> = f
        .preview
        .requests
        .lock()
        .expect("lock")
        .iter()
        .map(|(_, k)| *k)
        .collect();
    assert_eq!(
        kinds,
        vec![
            PreviewRequestKind::Status,
            PreviewRequestKind::Status,
            PreviewRequestKind::Screenshot
        ]
    );
    // A late answer finds nobody waiting.
    assert!(!f.builder.answer("gone", PreviewReport::default()));
}

#[tokio::test]
async fn save_publishes_the_draft_and_later_changes_wait_for_the_next_save() {
    let f = fixture().await;
    let id = f
        .builder
        .open(None, MODEL)
        .await
        .expect("open")
        .app
        .manifest
        .id;
    run(&f, &id, BuilderOp::Manifest, json!({"name": "Chart maker"})).await;
    let app = f.builder.publish(&id).await.expect("publish");
    assert_eq!(app.manifest.name, "Chart maker");
    let apps = f.builder.apps();
    assert!(apps.list().await.iter().any(|a| a.manifest.id == id));
    assert!(apps.list_drafts().await.is_empty());
    let state = f.builder.state(&id).await.expect("state");
    assert!(state.published && !state.unsaved);
    // A change shows in the draft only, until saved again.
    run(
        &f,
        &id,
        BuilderOp::Write,
        json!({"path": "App.tsx", "content": "export default () => 'v2';"}),
    )
    .await;
    assert!(f.builder.state(&id).await.expect("state").unsaved);
    let bundle = apps.bundle(&id).await.expect("bundle");
    assert!(bundle.files.iter().all(|f| !f.content.contains("v2")));
    // Discarding the draft of a saved app keeps the app and its chat.
    f.builder.discard(&id).await.expect("discard");
    assert!(apps.bundle(&id).await.is_ok());
    assert!(f.store.builder_chat(&id).await.expect("ok").is_some());
    // Reopening starts from the saved app.
    let again = f.builder.open(Some(&id), MODEL).await.expect("reopen");
    assert!(!again.unsaved);
    // Deleting the app removes the draft, chat and checkpoints too.
    apps.delete(&id).await.expect("delete");
    assert!(!apps.has_draft(&id));
    assert_eq!(f.store.builder_chat(&id).await.expect("ok"), None);
    assert!(f.builder.checkpoints(&id).await.expect("ok").is_empty());
    // A never-saved draft goes completely when discarded.
    let d = f.builder.open(None, MODEL).await.expect("open");
    let did = d.app.manifest.id;
    f.builder.discard(&did).await.expect("discard");
    assert_eq!(f.store.builder_chat(&did).await.expect("ok"), None);
    assert!(f.store.get_conversation(&d.conversation_id).await.is_err());
}

#[tokio::test]
async fn built_in_apps_are_cloned_before_building() {
    let f = fixture().await;
    let builtin = "ai-svg-creator";
    assert!(f.builder.open(Some(builtin), MODEL).await.is_err());
    let copy = f.builder.apps().clone_app(builtin).await.expect("clone");
    let draft = f
        .builder
        .open(Some(&copy.manifest.id), MODEL)
        .await
        .expect("open the copy");
    assert!(draft.published && !draft.unsaved);
    assert!(draft.files.iter().any(|f| f.path == "App.tsx"));
    assert!(f.builder.open(Some("no-such-app"), MODEL).await.is_err());
}

#[tokio::test]
async fn the_preview_runs_the_drafts_manifest_with_the_apps_grants() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::open_in_memory().await.expect("store");
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
    let fake = FakeProvider::new(vec![]);
    let registry = Arc::new(ModelRegistry::with_connector(
        store.clone(),
        secrets.clone(),
        fake_connector(fake),
    ));
    let apps = UtilityApps::new(dir.path().join("apps"), store.clone(), secrets.clone());
    let builder = AppBuilder::new(apps.clone());
    let runtime = AppRuntime::new(
        apps.clone(),
        registry,
        Tools::new(None, secrets, None),
        dir.path().join("work"),
    );
    let id = builder
        .open(None, MODEL)
        .await
        .expect("open")
        .app
        .manifest
        .id;
    builder.publish(&id).await.expect("publish");
    builder
        .run(
            &id,
            BuilderOp::Manifest,
            &json!({"capabilities": ["text", "webSearch"]}),
        )
        .await;
    let quiet: crate::apps::runtime::CallEvents = Arc::new(|_| {});
    // The saved app doesn't declare web search yet; the draft does.
    let saved = runtime
        .call(
            &id,
            "c1",
            "web.search",
            json!({"query": "x"}),
            quiet.clone(),
        )
        .await
        .expect_err("undeclared");
    assert_eq!(saved.code, AppErrorCode::Undeclared);
    let draft_ref = format!("{DRAFT_PREFIX}{id}");
    let asked = runtime
        .call(&draft_ref, "c2", "web.search", json!({"query": "x"}), quiet)
        .await
        .expect_err("asks");
    assert_eq!(asked.code, AppErrorCode::NeedsPermission);
    // Allowing it in the Preview grants it to the app.
    apps.grant(&draft_ref, AppCapability::WebSearch)
        .await
        .expect("grant");
    assert!(
        apps.prefs(&id)
            .await
            .expect("prefs")
            .allows(AppCapability::WebSearch)
    );
    assert!(apps.grant(&draft_ref, AppCapability::Chrome).await.is_err());
}

// ---- through a chat ----

fn ev(e: ChatEvent) -> Step {
    Step::Event(e)
}

fn call(id: &str, name: &str, input: Value) -> Vec<Step> {
    vec![
        ev(ChatEvent::ToolUseStart {
            id: id.into(),
            name: name.into(),
        }),
        ev(ChatEvent::ToolUseDelta {
            id: id.into(),
            partial_json: input.to_string(),
        }),
        ev(ChatEvent::ToolUseEnd { id: id.into() }),
    ]
}

fn turn(mut steps: Vec<Step>, stop: StopReason) -> Vec<Step> {
    steps.push(ev(ChatEvent::Usage(Usage {
        input_tokens: 10,
        output_tokens: 5,
        ..Usage::default()
    })));
    steps.push(ev(ChatEvent::Stop(stop)));
    steps
}

#[tokio::test]
async fn a_builder_chat_builds_the_draft_hears_preview_errors_and_checkpoints_the_turn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::open_in_memory().await.expect("store");
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
    let mut first = call(
        "w1",
        "app_write_file",
        json!({"path": "App.tsx", "content": "export default function App() { return <p>CSV</p> }"}),
    );
    first.extend(call("s1", "app_preview_status", json!({})));
    let fake = FakeProvider::new(vec![
        turn(first, StopReason::ToolUse),
        turn(
            vec![ev(ChatEvent::TextDelta("Fixed and done.".into()))],
            StopReason::EndTurn,
        ),
    ]);
    let registry = Arc::new(ModelRegistry::with_connector(
        store.clone(),
        secrets.clone(),
        fake_connector(fake.clone()),
    ));
    let apps = UtilityApps::new(dir.path().join("apps"), store.clone(), secrets.clone());
    let builder = AppBuilder::new(apps);
    let preview = Arc::new(FakePreview::default());
    let _ = preview.builder.set(builder.clone());
    *preview.report.lock().expect("lock") = Some(PreviewReport {
        rendered: false,
        errors: vec!["ReferenceError: Chart is not defined".into()],
        ..PreviewReport::default()
    });
    builder.set_sink(preview.clone());
    let tools = Tools::new(None, secrets, None).with_builder(builder.clone());
    let sink = Arc::new(Recorder::default());
    let service = ChatService::new(store.clone(), registry, sink, tools);
    let draft = builder.open(None, MODEL).await.expect("open");
    let (id, cid) = (draft.app.manifest.id.clone(), draft.conversation_id.clone());
    service
        .send(
            &cid,
            None,
            vec![ContentBlock::text("build a CSV chart app")],
            GenerateOptions::default(),
        )
        .await
        .expect("send");
    tokio::time::timeout(Duration::from_secs(30), async {
        while service.is_running(&cid) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the reply ends");

    let requests = fake.requests();
    assert_eq!(
        requests.len(),
        2,
        "no title or memory requests for a builder"
    );
    // The model got the builder tools (and no file tools for folders, no artifacts) and the
    // builder's prompt with the SDK types and the app's files.
    let names: Vec<&str> = requests[0].tools.iter().map(|t| t.name.as_str()).collect();
    for t in oxide_core::APP_BUILDER_TOOLS {
        assert!(names.contains(&t), "{t} missing from {names:?}");
    }
    assert!(!names.contains(&"artifact") && !names.contains(&"write_file"));
    let system = requests[0].system.clone().unwrap_or_default();
    assert!(system.contains("# Building a Utility App"));
    assert!(system.contains("export declare const ai"));
    assert!(system.contains("- App.tsx ("));
    // The preview's error reached the model.
    let second = format!("{:?}", requests[1].messages);
    assert!(second.contains("Chart is not defined"), "{second}");
    // The draft changed, and the turn made a checkpoint named after the request.
    let state = builder.state(&id).await.expect("state");
    assert!(
        state
            .files
            .iter()
            .any(|f| f.path == "App.tsx" && f.content.contains("<p>CSV</p>"))
    );
    let cps = builder.checkpoints(&id).await.expect("checkpoints");
    assert_eq!(cps[0].kind, CheckpointKind::Model);
    assert_eq!(cps[0].label, "After: build a CSV chart app");
    // The chat keeps its title and stays out of the chat list.
    assert!(store.list_conversations().await.expect("list").is_empty());
    assert_eq!(
        store.get_conversation(&cid).await.expect("chat").title,
        "Building New app"
    );
    assert!(
        preview
            .requests
            .lock()
            .expect("lock")
            .iter()
            .all(|(app, _)| *app == id)
    );
}
