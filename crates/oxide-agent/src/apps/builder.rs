//! The Utility App builder (Phase 21): drafts, checkpoints and the builder model's tools.
//!
//! - A **draft** is a working copy of an app in `<app data>/app-drafts/<id>/`. The builder's
//!   Preview and Code show it; the gallery keeps running the saved app until
//!   [`AppBuilder::publish`] (**Save to Utility Apps**) copies the draft over it. A new app is
//!   only a draft until then.
//! - **Checkpoints** are whole copies of a draft in the database: one when the builder opens,
//!   after every reply of the builder's model that changed something, and for every save in
//!   Code. [`AppBuilder::restore`] writes one back (after keeping the current files as a
//!   checkpoint, so a restore can be undone).
//! - The builder chat is an ordinary chat that the chat list leaves out. When the chat service
//!   answers it, the model gets the tools of [`AppBuilder::tool_entries`] (confined to the
//!   draft's folder) and [`AppBuilder::prompt`], which carries the app conventions and the
//!   `oxide` SDK's types.
//! - The Preview runs in the main window. [`AppBuilder::run`] asks it for its errors, console
//!   and screenshots through [`BuilderSink::preview_request`]; the window answers with
//!   [`AppBuilder::answer`].

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use oxide_core::{
    APP_MANIFEST, AppCapability, AppCheckpoint, AppDraft, AppManifest, AppModels, CheckpointKind,
    ContentBlock, MAX_APP_BYTES, MAX_APP_FILE_BYTES, MAX_APP_FILES, MediaSource, PreviewReport,
    PreviewRequestKind, UtilityApp, valid_app_path,
};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use super::{UtilityApps, builtin, manifest_of, own_app, read_folder, text_files};
use crate::AgentError;
use crate::tools::{Builtin, ToolEntry, ToolKind, ToolOutput};

/// The `oxide` SDK's types, generated from `src/appframe/sdk.ts` (`npm run sdk-types`).
pub const SDK_TYPES: &str = include_str!("../../../../src/appframe/oxide-sdk.d.ts");

/// The files a new app starts from.
static TEMPLATE: &[(&str, &str)] = &[
    (
        APP_MANIFEST,
        include_str!("../../../../utility-apps/_template/oxide-app.json"),
    ),
    (
        "App.tsx",
        include_str!("../../../../utility-apps/_template/App.tsx"),
    ),
];

/// The icon names the gallery draws (`src/features/apps/appIcons.ts`); others show as sparkles.
pub const APP_ICON_NAMES: &[&str] = &[
    "book-open",
    "bot",
    "braces",
    "brush",
    "calculator",
    "calendar",
    "camera",
    "chart-column",
    "chart-line",
    "chart-pie",
    "clock",
    "code",
    "database",
    "download",
    "file-code",
    "file-spreadsheet",
    "file-text",
    "film",
    "gauge",
    "globe",
    "graduation-cap",
    "hash",
    "heart",
    "image",
    "languages",
    "layout-grid",
    "lightbulb",
    "link",
    "list",
    "list-checks",
    "mail",
    "map",
    "map-pin",
    "message-square",
    "mic",
    "music",
    "newspaper",
    "palette",
    "pen-tool",
    "puzzle",
    "qr-code",
    "rocket",
    "scissors",
    "search",
    "shield",
    "shopping-cart",
    "sparkles",
    "star",
    "table",
    "tags",
    "timer",
    "triangle-alert",
    "type",
    "upload",
    "video",
    "wand",
    "wrench",
    "zap",
];

/// How long a tool waits for the Preview's answer.
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(45);

/// Longest file text a read returns (longer files are cut, with a note).
const MAX_READ_CHARS: usize = 120_000;

/// The prefix of an app reference that means its draft (the builder's Preview).
pub const DRAFT_PREFIX: &str = "draft:";

/// `(id, draft)` of an app reference: `draft:<id>` is the builder's draft of `<id>`.
pub fn split_ref(app_ref: &str) -> (&str, bool) {
    match app_ref.strip_prefix(DRAFT_PREFIX) {
        Some(id) => (id, true),
        None => (app_ref, false),
    }
}

fn invalid(msg: impl Into<String>) -> AgentError {
    AgentError::Invalid(msg.into())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Hears what the builder does, for the window.
pub trait BuilderSink: Send + Sync {
    /// A draft's files changed (the model wrote, a checkpoint was restored).
    fn draft_changed(&self, app_id: &str);
    /// The builder's model wants the Preview's state; answer with [`AppBuilder::answer`].
    fn preview_request(&self, request_id: &str, app_id: &str, kind: PreviewRequestKind);
}

/// A sink that hears nothing (tests, and before the window exists).
pub struct NoBuilderSink;

impl BuilderSink for NoBuilderSink {
    fn draft_changed(&self, _app_id: &str) {}
    fn preview_request(&self, _request_id: &str, _app_id: &str, _kind: PreviewRequestKind) {}
}

/// One of the builder model's tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuilderOp {
    List,
    Read,
    Write,
    Edit,
    Delete,
    Rename,
    Manifest,
    Status,
    Screenshot,
}

impl BuilderOp {
    pub const ALL: [BuilderOp; 9] = [
        BuilderOp::List,
        BuilderOp::Read,
        BuilderOp::Write,
        BuilderOp::Edit,
        BuilderOp::Delete,
        BuilderOp::Rename,
        BuilderOp::Manifest,
        BuilderOp::Status,
        BuilderOp::Screenshot,
    ];

    pub fn name(self) -> &'static str {
        use oxide_core::app_builder as n;
        match self {
            BuilderOp::List => n::APP_LIST_FILES,
            BuilderOp::Read => n::APP_READ_FILE,
            BuilderOp::Write => n::APP_WRITE_FILE,
            BuilderOp::Edit => n::APP_EDIT_FILE,
            BuilderOp::Delete => n::APP_DELETE_FILE,
            BuilderOp::Rename => n::APP_RENAME_FILE,
            BuilderOp::Manifest => n::APP_UPDATE_MANIFEST,
            BuilderOp::Status => n::APP_PREVIEW_STATUS,
            BuilderOp::Screenshot => n::APP_SCREENSHOT,
        }
    }

    /// Changes nothing (runs beside other calls).
    pub fn read_only(self) -> bool {
        matches!(self, BuilderOp::List | BuilderOp::Read)
    }

    /// The usage label of a call (the file it touched).
    pub fn label(self, input: &Value) -> Option<String> {
        let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_owned);
        match self {
            BuilderOp::Rename => Some(format!("{} → {}", s("from")?, s("to")?)),
            BuilderOp::Read | BuilderOp::Write | BuilderOp::Edit | BuilderOp::Delete => s("path"),
            BuilderOp::Manifest => Some(APP_MANIFEST.into()),
            BuilderOp::List | BuilderOp::Status | BuilderOp::Screenshot => None,
        }
    }
}

/// The tool entry of one builder op.
pub fn builder_entry(op: BuilderOp) -> ToolEntry {
    let path = json!({"type": "string",
        "description": "A path inside the app, e.g. App.tsx or components/Chart.tsx."});
    let (title, description, schema) = match op {
        BuilderOp::List => (
            "List app files",
            "List the app's files with their sizes.",
            json!({"type": "object", "properties": {}}),
        ),
        BuilderOp::Read => (
            "Read app file",
            "Read one of the app's files.",
            json!({"type": "object", "properties": {"path": path}, "required": ["path"]}),
        ),
        BuilderOp::Write => (
            "Write app file",
            "Create a file of the app or replace it with the given content (the whole file). \
             Folders are made as needed. Prefer app_edit_file for small changes. The Preview \
             reloads after each change.",
            json!({"type": "object", "properties": {
                "path": path,
                "content": {"type": "string", "description": "The whole new content."}
            }, "required": ["path", "content"]}),
        ),
        BuilderOp::Edit => (
            "Edit app file",
            "Replace text in one of the app's files. `old_text` must appear exactly once \
             (include a few lines around the change), unless `replace_all` is true.",
            json!({"type": "object", "properties": {
                "path": path,
                "old_text": {"type": "string"},
                "new_text": {"type": "string"},
                "replace_all": {"type": "boolean"}
            }, "required": ["path", "old_text", "new_text"]}),
        ),
        BuilderOp::Delete => (
            "Delete app file",
            "Delete one of the app's files (not the manifest or the entry file).",
            json!({"type": "object", "properties": {"path": path}, "required": ["path"]}),
        ),
        BuilderOp::Rename => (
            "Rename app file",
            "Rename or move one of the app's files. Update the imports that name it.",
            json!({"type": "object", "properties": {
                "from": {"type": "string"},
                "to": {"type": "string"}
            }, "required": ["from", "to"]}),
        ),
        BuilderOp::Manifest => (
            "Update app manifest",
            "Change the app's oxide-app.json. Give only the fields to change. `capabilities` \
             replaces the list: declare every capability the app's code uses (text, image, \
             video, music, webSearch, webFetch, connectors, python, sandbox, browser, chrome); \
             calls for undeclared ones fail. `icon` is one of the icon names in your \
             instructions or one emoji.",
            json!({"type": "object", "properties": {
                "name": {"type": "string"},
                "description": {"type": "string"},
                "icon": {"type": "string"},
                "category": {"type": "string"},
                "version": {"type": "string"},
                "entry": {"type": "string"},
                "capabilities": {"type": "array", "items": {"type": "string", "enum":
                    AppCapability::ALL.iter().map(|c| c.as_str()).collect::<Vec<_>>()}},
                "models": {"type": "object", "properties": {
                    "text": {"type": "string"}, "image": {"type": "string"},
                    "video": {"type": "string"}, "music": {"type": "string"}
                }}
            }}),
        ),
        BuilderOp::Status => (
            "Check preview",
            "Reload the Preview with the current files and report whether the app compiled \
             and rendered, its errors (compile errors and uncaught runtime errors) and its \
             newest console lines. Call it after changes, and fix what it reports.",
            json!({"type": "object", "properties": {}}),
        ),
        BuilderOp::Screenshot => (
            "Screenshot preview",
            "Reload the Preview and take a screenshot of the app, to check how it looks.",
            json!({"type": "object", "properties": {}}),
        ),
    };
    ToolEntry {
        api_name: op.name().into(),
        kind: ToolKind::Builtin(Builtin::AppBuild(op)),
        title: Some(title.into()),
        description: description.into(),
        schema,
        read_only: op.read_only(),
        destructive: matches!(op, BuilderOp::Delete),
    }
}

/// A draft's files as stored in a checkpoint.
fn snapshot(files: &[(String, Vec<u8>)]) -> (String, String) {
    let b64 = base64::engine::general_purpose::STANDARD;
    let list: Vec<Value> = files
        .iter()
        .map(|(p, b)| json!({"path": p, "data": b64.encode(b)}))
        .collect();
    let mut h = DefaultHasher::new();
    files.hash(&mut h);
    (
        Value::Array(list).to_string(),
        format!("{:016x}", h.finish()),
    )
}

fn unsnapshot(json: &str) -> Result<Vec<(String, Vec<u8>)>, AgentError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let list: Vec<Value> = serde_json::from_str(json)
        .map_err(|e| invalid(format!("the checkpoint is damaged: {e}")))?;
    let mut out = Vec::new();
    for v in list {
        let path = v.get("path").and_then(Value::as_str).unwrap_or("");
        let data = v.get("data").and_then(Value::as_str).unwrap_or("");
        if !valid_app_path(path) {
            return Err(invalid("the checkpoint has a file with an unsafe path"));
        }
        let bytes = b64
            .decode(data)
            .map_err(|_| invalid("the checkpoint is damaged"))?;
        out.push((path.to_owned(), bytes));
    }
    Ok(out)
}

fn digest_of(files: &[(String, Vec<u8>)]) -> u64 {
    let mut h = DefaultHasher::new();
    files.hash(&mut h);
    h.finish()
}

/// Writes `files` into `dir`, replacing everything in it.
fn replace_folder(root: &Path, dir: &Path, files: &[(String, Vec<u8>)]) -> Result<(), AgentError> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = root.join(format!(".{name}.partial"));
    let _ = std::fs::remove_dir_all(&tmp);
    for (path, bytes) in files {
        if !valid_app_path(path) {
            return Err(invalid(format!("“{path}” isn't a valid file path")));
        }
        let target = tmp.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| invalid(format!("can't write the draft: {e}")))?;
        }
        std::fs::write(&target, bytes)
            .map_err(|e| invalid(format!("can't write the draft: {e}")))?;
    }
    if dir.exists() {
        let old = root.join(format!(".{name}.old"));
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(dir, &old)
            .map_err(|e| invalid(format!("the draft couldn't be replaced: {e}")))?;
        if let Err(e) = std::fs::rename(&tmp, dir) {
            let _ = std::fs::rename(&old, dir);
            return Err(invalid(format!("the draft couldn't be written: {e}")));
        }
        let _ = std::fs::remove_dir_all(&old);
    } else {
        std::fs::rename(&tmp, dir)
            .map_err(|e| invalid(format!("the draft couldn't be written: {e}")))?;
    }
    Ok(())
}

/// Removes folders a file's removal left empty, up to (not including) `root`.
fn prune_empty(root: &Path, mut dir: Option<&Path>) {
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            break;
        }
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

// ---- drafts on the app library ----

impl UtilityApps {
    /// `<app data>/app-drafts`.
    pub fn drafts_dir(&self) -> PathBuf {
        self.inner.dir.with_file_name("app-drafts")
    }

    pub(crate) fn draft_dir(&self, id: &str) -> Result<PathBuf, AgentError> {
        if !oxide_core::valid_app_id(id) {
            return Err(AgentError::NotFound("app"));
        }
        Ok(self.drafts_dir().join(id))
    }

    pub fn has_draft(&self, id: &str) -> bool {
        self.draft_dir(id).is_ok_and(|d| d.is_dir())
    }

    pub fn is_published(&self, id: &str) -> bool {
        oxide_core::valid_app_id(id) && self.inner.dir.join(id).is_dir()
    }

    /// The manifest of an app or (`draft`) of its draft.
    pub async fn manifest_at(&self, id: &str, draft: bool) -> Result<AppManifest, AgentError> {
        if !draft {
            return self.manifest(id).await;
        }
        let dir = self.draft_dir(id)?;
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("app"));
            }
            let app = own_app(&id, &dir);
            match app.problem {
                Some(p) => Err(invalid(format!("The app can't run: {p}."))),
                None => Ok(app.manifest),
            }
        })
        .await
        .map_err(|e| invalid(format!("reading the draft failed: {e}")))?
    }

    /// Every file of a draft.
    pub(crate) async fn draft_files(&self, id: &str) -> Result<Vec<(String, Vec<u8>)>, AgentError> {
        let dir = self.draft_dir(id)?;
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("draft"));
            }
            read_folder(&dir).map_err(invalid)
        })
        .await
        .map_err(|e| invalid(format!("reading the draft failed: {e}")))?
    }

    /// Replaces a draft's files.
    pub(crate) async fn write_draft(
        &self,
        id: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), AgentError> {
        let dir = self.draft_dir(id)?;
        let root = self.drafts_dir();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&root).map_err(|e| invalid(format!("can't make {e}")))?;
            replace_folder(&root, &dir, &files)
        })
        .await
        .map_err(|e| invalid(format!("writing the draft failed: {e}")))?
    }

    /// An id no app or draft uses yet, made from `name`.
    pub(crate) fn free_draft_id(&self, name: &str) -> String {
        let base = oxide_core::app_slug(name);
        let drafts = self.drafts_dir();
        let taken = |id: &str| {
            builtin::find(id).is_some()
                || self.inner.dir.join(id).exists()
                || drafts.join(id).exists()
        };
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }

    /// Drafts of apps that were never saved to Utility Apps (the gallery lists them).
    pub async fn list_drafts(&self) -> Vec<UtilityApp> {
        let drafts = self.drafts_dir();
        let apps = self.inner.dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            let Ok(entries) = std::fs::read_dir(&drafts) else {
                return out;
            };
            for e in entries.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if !path.is_dir() || !oxide_core::valid_app_id(&name) || apps.join(&name).is_dir() {
                    continue;
                }
                out.push(own_app(&name, &path));
            }
            out.sort_by_key(|a| a.manifest.name.to_lowercase());
            out
        })
        .await
        .unwrap_or_default()
    }

    /// Removes an app's draft folder (if any).
    pub(crate) async fn remove_draft(&self, id: &str) -> Result<(), AgentError> {
        let dir = self.draft_dir(id)?;
        tokio::task::spawn_blocking(move || {
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| invalid(format!("the draft couldn't be removed: {e}")))?;
            }
            Ok(())
        })
        .await
        .map_err(|e| invalid(format!("removing the draft failed: {e}")))?
    }
}

/// Checks a manifest a draft would get: valid, the same id, an existing entry file.
fn check_manifest(raw: &[u8], id: &str) -> Result<AppManifest, String> {
    let m: AppManifest =
        serde_json::from_slice(raw).map_err(|e| format!("{APP_MANIFEST} isn't valid JSON: {e}"))?;
    m.validate()?;
    if m.id != id {
        return Err(format!("The app's id can't be changed (it stays “{id}”)."));
    }
    Ok(m)
}

// ---- the builder ----

/// The builder: drafts, checkpoints, the model's tools and the bridge to the Preview.
#[derive(Clone)]
pub struct AppBuilder {
    inner: Arc<BuilderInner>,
}

struct BuilderInner {
    apps: UtilityApps,
    sink: Mutex<Arc<dyn BuilderSink>>,
    pending: Mutex<HashMap<String, oneshot::Sender<PreviewReport>>>,
    timeout: Mutex<Duration>,
}

impl AppBuilder {
    pub fn new(apps: UtilityApps) -> Self {
        Self {
            inner: Arc::new(BuilderInner {
                apps,
                sink: Mutex::new(Arc::new(NoBuilderSink)),
                pending: Mutex::new(HashMap::new()),
                timeout: Mutex::new(PREVIEW_TIMEOUT),
            }),
        }
    }

    /// Where draft changes and preview requests go (the app sets the window's sink).
    pub fn set_sink(&self, sink: Arc<dyn BuilderSink>) {
        *lock(&self.inner.sink) = sink;
    }

    /// How long tools wait for the Preview (tests make it short).
    pub fn set_preview_timeout(&self, timeout: Duration) {
        *lock(&self.inner.timeout) = timeout;
    }

    fn sink(&self) -> Arc<dyn BuilderSink> {
        lock(&self.inner.sink).clone()
    }

    pub fn apps(&self) -> &UtilityApps {
        &self.inner.apps
    }

    fn store(&self) -> &oxide_store::Store {
        self.inner.apps.store()
    }

    /// Opens the builder: on a new app made from the template (`id` `None`), or on an own
    /// app (its draft, made from the saved app the first time). Makes the builder chat with
    /// `model` when the app has none.
    pub async fn open(&self, id: Option<&str>, model: &str) -> Result<AppDraft, AgentError> {
        let apps = self.apps();
        let id = match id {
            None => {
                let id = apps.free_draft_id("New app");
                let mut files: Vec<(String, Vec<u8>)> = TEMPLATE
                    .iter()
                    .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
                    .collect();
                let mut m: AppManifest = serde_json::from_str(TEMPLATE[0].1)
                    .map_err(|e| invalid(format!("the template is broken: {e}")))?;
                m.id = id.clone();
                let json = serde_json::to_vec_pretty(&m)
                    .map_err(|e| invalid(format!("the manifest couldn't be written: {e}")))?;
                if let Some(f) = files.iter_mut().find(|f| f.0 == APP_MANIFEST) {
                    f.1 = json;
                }
                apps.write_draft(&id, files).await?;
                id
            }
            Some(id) => {
                if builtin::find(id).is_some() {
                    return Err(invalid(
                        "Built-in apps can't be changed. Clone the app first, then edit the copy.",
                    ));
                }
                if !apps.has_draft(id) {
                    if !apps.is_published(id) {
                        return Err(AgentError::NotFound("app"));
                    }
                    let files = apps.raw_files(id).await?;
                    apps.write_draft(id, files).await?;
                }
                id.to_owned()
            }
        };
        let files = apps.draft_files(&id).await?;
        let name = manifest_of(&files)
            .map(|m| m.name)
            .unwrap_or_else(|_| id.clone());
        self.store()
            .create_builder_chat(&id, &format!("Building {name}"), model)
            .await?;
        self.checkpoint_files(&id, "Start", CheckpointKind::Start, &files)
            .await?;
        self.state(&id).await
    }

    /// The draft as the builder shows it.
    pub async fn state(&self, id: &str) -> Result<AppDraft, AgentError> {
        let apps = self.apps();
        let dir = apps.draft_dir(id)?;
        let files = apps.draft_files(id).await?;
        let published = apps.is_published(id);
        let unsaved = if published {
            match apps.raw_files(id).await {
                Ok(saved) => digest_of(&saved) != digest_of(&files),
                Err(_) => true,
            }
        } else {
            true
        };
        let conversation_id = self
            .store()
            .builder_chat(id)
            .await?
            .ok_or(AgentError::NotFound("builder chat"))?;
        let app = {
            let (id, dir) = (id.to_owned(), dir.clone());
            tokio::task::spawn_blocking(move || own_app(&id, &dir))
                .await
                .map_err(|e| invalid(format!("reading the draft failed: {e}")))?
        };
        Ok(AppDraft {
            app,
            files: text_files(files),
            published,
            unsaved,
            conversation_id,
        })
    }

    /// Saves a checkpoint of `files` unless the newest one has the same files.
    async fn checkpoint_files(
        &self,
        id: &str,
        label: &str,
        kind: CheckpointKind,
        files: &[(String, Vec<u8>)],
    ) -> Result<Option<AppCheckpoint>, AgentError> {
        let (json, digest) = snapshot(files);
        let count = u32::try_from(files.len()).unwrap_or(u32::MAX);
        Ok(self
            .store()
            .add_app_checkpoint(id, label, kind, count, &json, &digest)
            .await?)
    }

    /// A checkpoint of the draft as it is now.
    pub async fn checkpoint(
        &self,
        id: &str,
        label: &str,
        kind: CheckpointKind,
    ) -> Result<Option<AppCheckpoint>, AgentError> {
        let files = self.apps().draft_files(id).await?;
        self.checkpoint_files(id, label, kind, &files).await
    }

    /// After a builder reply: a checkpoint when the draft changed, named after the request.
    pub async fn checkpoint_turn(&self, id: &str, conversation_id: &str) {
        let label = match self.store().last_user_text(conversation_id).await {
            Ok(Some(t)) => oxide_core::checkpoint_label(&t),
            _ => oxide_core::checkpoint_label(""),
        };
        if let Err(e) = self.checkpoint(id, &label, CheckpointKind::Model).await {
            tracing::warn!("couldn't make a checkpoint of {id}: {e}");
        }
    }

    pub async fn checkpoints(&self, id: &str) -> Result<Vec<AppCheckpoint>, AgentError> {
        Ok(self.store().app_checkpoints(id).await?)
    }

    /// Writes a checkpoint back into the draft. The current files become a checkpoint first.
    pub async fn restore(&self, id: &str, checkpoint_id: &str) -> Result<AppDraft, AgentError> {
        let (app_id, json) = self.store().app_checkpoint_files(checkpoint_id).await?;
        if app_id != id {
            return Err(AgentError::NotFound("checkpoint"));
        }
        let files = unsnapshot(&json)?;
        manifest_of(&files).map_err(|e| invalid(format!("The checkpoint can't be used: {e}")))?;
        self.checkpoint(id, "Before restoring", CheckpointKind::Restore)
            .await?;
        self.apps().write_draft(id, files).await?;
        self.sink().draft_changed(id);
        self.state(id).await
    }

    /// Saves one file from Code (a checkpoint follows).
    pub async fn save_file(
        &self,
        id: &str,
        path: &str,
        content: &str,
    ) -> Result<AppDraft, AgentError> {
        self.write(id, path, content.as_bytes().to_vec()).await?;
        self.checkpoint(id, &format!("Saved {path}"), CheckpointKind::Manual)
            .await?;
        self.state(id).await
    }

    /// Deletes one file from Code (a checkpoint follows).
    pub async fn delete_file(&self, id: &str, path: &str) -> Result<AppDraft, AgentError> {
        self.delete(id, path).await?;
        self.checkpoint(id, &format!("Deleted {path}"), CheckpointKind::Manual)
            .await?;
        self.state(id).await
    }

    /// Renames one file from Code (a checkpoint follows).
    pub async fn rename_file(
        &self,
        id: &str,
        from: &str,
        to: &str,
    ) -> Result<AppDraft, AgentError> {
        self.rename(id, from, to).await?;
        self.checkpoint(id, &format!("Renamed {from}"), CheckpointKind::Manual)
            .await?;
        self.state(id).await
    }

    /// Changes the manifest from the builder's header (name, icon, category, description).
    pub async fn update_manifest(&self, id: &str, patch: &Value) -> Result<AppDraft, AgentError> {
        self.patch_manifest(id, patch).await?;
        self.checkpoint(id, "Changed the app's details", CheckpointKind::Manual)
            .await?;
        self.state(id).await
    }

    /// **Save to Utility Apps**: the draft becomes the saved app.
    pub async fn publish(&self, id: &str) -> Result<UtilityApp, AgentError> {
        let files = self.apps().draft_files(id).await?;
        let m =
            manifest_of(&files).map_err(|e| invalid(format!("The app can't be saved: {e}.")))?;
        if m.id != id {
            return Err(invalid("The manifest's id doesn't match the app."));
        }
        self.apps().write_app(id, files).await
    }

    /// Throws the draft away. An app that was never saved goes completely (its chat,
    /// checkpoints and settings); a saved app keeps its chat and checkpoints.
    pub async fn discard(&self, id: &str) -> Result<(), AgentError> {
        let apps = self.apps();
        apps.remove_draft(id).await?;
        if !apps.is_published(id) {
            self.store().delete_builder_data(id).await?;
            self.store().delete_app_data(id).await?;
        }
        Ok(())
    }

    // ---- file operations on the draft ----

    async fn write(&self, id: &str, path: &str, bytes: Vec<u8>) -> Result<(), AgentError> {
        if !valid_app_path(path) {
            return Err(invalid(format!(
                "“{path}” isn't a valid path: use a relative path inside the app, like \
                 components/Chart.tsx."
            )));
        }
        if bytes.len() as u64 > MAX_APP_FILE_BYTES {
            return Err(invalid(format!("{path} would be larger than 2 MB.")));
        }
        if path == APP_MANIFEST {
            check_manifest(&bytes, id).map_err(invalid)?;
        }
        let files = self.apps().draft_files(id).await?;
        let others: u64 = files
            .iter()
            .filter(|(p, _)| p != path)
            .map(|(_, b)| b.len() as u64)
            .sum();
        let count = files.iter().filter(|(p, _)| p != path).count() + 1;
        if others + bytes.len() as u64 > MAX_APP_BYTES || count > MAX_APP_FILES {
            return Err(invalid(
                "The app would be larger than 20 MB or have more than 200 files.",
            ));
        }
        // A folder can't take a file's name and a file can't take a folder's.
        if files
            .iter()
            .any(|(p, _)| p.starts_with(&format!("{path}/")) || path.starts_with(&format!("{p}/")))
        {
            return Err(invalid(format!(
                "“{path}” clashes with a file or folder of the app."
            )));
        }
        let dir = self.apps().draft_dir(id)?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let target = dir.join(&path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| invalid(format!("can't write {path}: {e}")))?;
            }
            std::fs::write(&target, bytes).map_err(|e| invalid(format!("can't write {path}: {e}")))
        })
        .await
        .map_err(|e| invalid(format!("writing failed: {e}")))?
    }

    async fn read(&self, id: &str, path: &str) -> Result<Vec<u8>, AgentError> {
        let files = self.apps().draft_files(id).await?;
        files
            .into_iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b)
            .ok_or_else(|| {
                invalid(format!(
                    "The app has no file “{path}”. List the files first."
                ))
            })
    }

    async fn delete(&self, id: &str, path: &str) -> Result<(), AgentError> {
        let files = self.apps().draft_files(id).await?;
        let m = manifest_of(&files).ok();
        if path == APP_MANIFEST {
            return Err(invalid("The manifest can't be deleted."));
        }
        if m.as_ref().is_some_and(|m| m.entry == path) {
            return Err(invalid(
                "The entry file can't be deleted (change the manifest's entry first).",
            ));
        }
        if !files.iter().any(|(p, _)| p == path) {
            return Err(invalid(format!("The app has no file “{path}”.")));
        }
        let dir = self.apps().draft_dir(id)?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            let target = dir.join(&path);
            std::fs::remove_file(&target)
                .map_err(|e| invalid(format!("can't delete {path}: {e}")))?;
            prune_empty(&dir, target.parent());
            Ok(())
        })
        .await
        .map_err(|e| invalid(format!("deleting failed: {e}")))?
    }

    async fn rename(&self, id: &str, from: &str, to: &str) -> Result<(), AgentError> {
        if from == APP_MANIFEST || to == APP_MANIFEST {
            return Err(invalid("The manifest can't be renamed."));
        }
        if !valid_app_path(to) {
            return Err(invalid(format!("“{to}” isn't a valid path.")));
        }
        let files = self.apps().draft_files(id).await?;
        if !files.iter().any(|(p, _)| p == from) {
            return Err(invalid(format!("The app has no file “{from}”.")));
        }
        if files.iter().any(|(p, _)| p == to) {
            return Err(invalid(format!("“{to}” exists already.")));
        }
        let bytes = self.read(id, from).await?;
        self.write(id, to, bytes).await?;
        // The entry file keeps working when it moves.
        if let Ok(mut m) = manifest_of(&files)
            && m.entry == from
        {
            m.entry = to.to_owned();
            let json = serde_json::to_vec_pretty(&m)
                .map_err(|e| invalid(format!("the manifest couldn't be written: {e}")))?;
            self.write(id, APP_MANIFEST, json).await?;
        }
        let dir = self.apps().draft_dir(id)?;
        let from = from.to_owned();
        tokio::task::spawn_blocking(move || {
            let target = dir.join(&from);
            std::fs::remove_file(&target)
                .map_err(|e| invalid(format!("can't remove {from}: {e}")))?;
            prune_empty(&dir, target.parent());
            Ok(())
        })
        .await
        .map_err(|e| invalid(format!("renaming failed: {e}")))?
    }

    async fn patch_manifest(&self, id: &str, patch: &Value) -> Result<AppManifest, AgentError> {
        let raw = self.read(id, APP_MANIFEST).await?;
        let mut m: AppManifest = serde_json::from_slice(&raw)
            .map_err(|e| invalid(format!("{APP_MANIFEST} isn't valid: {e}")))?;
        let s = |k: &str| patch.get(k).and_then(Value::as_str).map(str::trim);
        if let Some(v) = s("name") {
            m.name = v.to_owned();
        }
        if let Some(v) = patch.get("description").and_then(Value::as_str) {
            m.description = v.trim().to_owned();
        }
        if let Some(v) = s("icon") {
            m.icon = v.to_owned();
        }
        if let Some(v) = s("category") {
            m.category = v.to_owned();
        }
        if let Some(v) = s("version") {
            m.version = v.to_owned();
        }
        if let Some(v) = s("entry") {
            m.entry = v.to_owned();
        }
        if let Some(list) = patch.get("capabilities") {
            let list = list
                .as_array()
                .ok_or_else(|| invalid("`capabilities` must be a list."))?;
            let mut caps = Vec::new();
            for c in list {
                let name = c.as_str().unwrap_or("");
                let cap = AppCapability::parse(name).ok_or_else(|| {
                    invalid(format!(
                        "“{name}” isn't a capability. Use: {}.",
                        AppCapability::ALL
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
                if !caps.contains(&cap) {
                    caps.push(cap);
                }
            }
            m.capabilities = caps;
        }
        if let Some(models) = patch.get("models") {
            let mut next: AppModels = m.models.clone();
            let pick = |k: &str| {
                models
                    .get(k)
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_owned())
            };
            if let Some(v) = pick("text") {
                next.text = (!v.is_empty()).then_some(v);
            }
            if let Some(v) = pick("image") {
                next.image = (!v.is_empty()).then_some(v);
            }
            if let Some(v) = pick("video") {
                next.video = (!v.is_empty()).then_some(v);
            }
            if let Some(v) = pick("music") {
                next.music = (!v.is_empty()).then_some(v);
            }
            m.models = next;
        }
        m.validate().map_err(invalid)?;
        let files = self.apps().draft_files(id).await?;
        if !files.iter().any(|(p, _)| *p == m.entry) {
            return Err(invalid(format!(
                "The entry file {} doesn't exist.",
                m.entry
            )));
        }
        let json = serde_json::to_vec_pretty(&m)
            .map_err(|e| invalid(format!("the manifest couldn't be written: {e}")))?;
        self.write(id, APP_MANIFEST, json).await?;
        Ok(m)
    }

    // ---- the model's tools ----

    /// The builder tools.
    pub fn tool_entries() -> Vec<ToolEntry> {
        BuilderOp::ALL.into_iter().map(builder_entry).collect()
    }

    /// Runs one builder tool on the draft of `id`.
    pub async fn run(&self, id: &str, op: BuilderOp, input: &Value) -> ToolOutput {
        let text = |t: String| ToolOutput {
            content: vec![ContentBlock::text(t)],
            is_error: false,
            duration_ms: 0,
        };
        let s = |k: &str| input.get(k).and_then(Value::as_str);
        let path = || {
            s("path")
                .map(|p| p.trim().trim_start_matches("./").to_owned())
                .ok_or_else(|| invalid("Give the file's `path`."))
        };
        let result: Result<ToolOutput, AgentError> = async {
            match op {
                BuilderOp::List => {
                    let files = self.apps().draft_files(id).await?;
                    let mut out = String::new();
                    for (p, b) in &files {
                        out.push_str(&format!("{p} ({} bytes)\n", b.len()));
                    }
                    if out.is_empty() {
                        out.push_str("The app has no files.");
                    }
                    Ok(text(out))
                }
                BuilderOp::Read => {
                    let p = path()?;
                    let bytes = self.read(id, &p).await?;
                    let Ok(t) = String::from_utf8(bytes) else {
                        return Ok(text(format!("{p} isn't a text file.")));
                    };
                    if t.chars().count() > MAX_READ_CHARS {
                        let cut: String = t.chars().take(MAX_READ_CHARS).collect();
                        Ok(text(format!("{cut}\n[… {p} is cut here: it is longer]")))
                    } else {
                        Ok(text(t))
                    }
                }
                BuilderOp::Write => {
                    let p = path()?;
                    let content = s("content").ok_or_else(|| invalid("Give the `content`."))?;
                    let existed = self.read(id, &p).await.is_ok();
                    self.write(id, &p, content.as_bytes().to_vec()).await?;
                    self.sink().draft_changed(id);
                    Ok(text(format!(
                        "{} {p} ({} lines).",
                        if existed { "Replaced" } else { "Created" },
                        content.lines().count()
                    )))
                }
                BuilderOp::Edit => {
                    let p = path()?;
                    let old = s("old_text").ok_or_else(|| invalid("Give the `old_text`."))?;
                    let new = s("new_text").unwrap_or("");
                    if old.is_empty() {
                        return Err(invalid("`old_text` is empty."));
                    }
                    let bytes = self.read(id, &p).await?;
                    let current = String::from_utf8(bytes)
                        .map_err(|_| invalid(format!("{p} isn't a text file.")))?;
                    let all = input.get("replace_all").and_then(Value::as_bool) == Some(true);
                    // Files may have Windows line ends while the model writes "\n".
                    let (hay, needle, replacement) = if !current.contains(old)
                        && current.contains("\r\n")
                        && !old.contains('\r')
                    {
                        (
                            current.replace("\r\n", "\n"),
                            old.to_owned(),
                            new.to_owned(),
                        )
                    } else {
                        (current.clone(), old.to_owned(), new.to_owned())
                    };
                    let n = hay.matches(&needle).count();
                    if n == 0 {
                        return Err(invalid(format!(
                            "`old_text` isn't in {p}. Read the file and copy the text exactly."
                        )));
                    }
                    if n > 1 && !all {
                        return Err(invalid(format!(
                            "`old_text` appears {n} times in {p}. Include more lines around it, \
                             or set replace_all."
                        )));
                    }
                    let next = if all {
                        hay.replace(&needle, &replacement)
                    } else {
                        hay.replacen(&needle, &replacement, 1)
                    };
                    self.write(id, &p, next.into_bytes()).await?;
                    self.sink().draft_changed(id);
                    Ok(text(format!(
                        "Edited {p} ({} replacement{}).",
                        if all { n } else { 1 },
                        if all && n != 1 { "s" } else { "" }
                    )))
                }
                BuilderOp::Delete => {
                    let p = path()?;
                    self.delete(id, &p).await?;
                    self.sink().draft_changed(id);
                    Ok(text(format!("Deleted {p}.")))
                }
                BuilderOp::Rename => {
                    let from = s("from").ok_or_else(|| invalid("Give `from`."))?;
                    let to = s("to").ok_or_else(|| invalid("Give `to`."))?;
                    self.rename(id, from.trim(), to.trim()).await?;
                    self.sink().draft_changed(id);
                    Ok(text(format!(
                        "Renamed {from} to {to}. Update imports that name it."
                    )))
                }
                BuilderOp::Manifest => {
                    let m = self.patch_manifest(id, input).await?;
                    self.sink().draft_changed(id);
                    let json = serde_json::to_string_pretty(&m).unwrap_or_default();
                    Ok(text(format!("Updated {APP_MANIFEST}:\n{json}")))
                }
                BuilderOp::Status | BuilderOp::Screenshot => {
                    let kind = if op == BuilderOp::Status {
                        PreviewRequestKind::Status
                    } else {
                        PreviewRequestKind::Screenshot
                    };
                    let report = self.ask_preview(id, kind).await;
                    Ok(report_output(&report, kind))
                }
            }
        }
        .await;
        result.unwrap_or_else(|e| ToolOutput::error(e.to_string()))
    }

    /// Asks the open Preview; a report with a note when nobody answers in time.
    async fn ask_preview(&self, id: &str, kind: PreviewRequestKind) -> PreviewReport {
        let request_id = uuid::Uuid::now_v7().to_string();
        let (tx, rx) = oneshot::channel();
        lock(&self.inner.pending).insert(request_id.clone(), tx);
        self.sink().preview_request(&request_id, id, kind);
        let timeout = *lock(&self.inner.timeout);
        let answer = tokio::time::timeout(timeout, rx).await;
        lock(&self.inner.pending).remove(&request_id);
        match answer {
            Ok(Ok(report)) => report,
            _ => PreviewReport {
                note: Some(
                    "The Preview didn't answer: the builder may be closed or showing Code. \
                     Ask the user to open the builder's Preview, or carry on without it."
                        .into(),
                ),
                ..PreviewReport::default()
            },
        }
    }

    /// The Preview answered a request. `false`: nobody waits for it any more.
    pub fn answer(&self, request_id: &str, report: PreviewReport) -> bool {
        match lock(&self.inner.pending).remove(request_id) {
            Some(tx) => tx.send(report).is_ok(),
            None => false,
        }
    }

    /// The system prompt's builder part: what the app is, the conventions and the SDK.
    pub async fn prompt(&self, id: &str) -> String {
        let files = self.apps().draft_files(id).await.unwrap_or_default();
        let manifest = files
            .iter()
            .find(|(p, _)| p == APP_MANIFEST)
            .and_then(|(_, b)| String::from_utf8(b.clone()).ok())
            .unwrap_or_else(|| "(missing)".into());
        let list: String = files
            .iter()
            .map(|(p, b)| format!("- {p} ({} bytes)\n", b.len()))
            .collect();
        format!(
            "\n\n# Building a Utility App\n\
             You are Oxide's app builder. In this chat you build and change one Utility App \
             with the user, the way AI Studio's build mode does: the user describes what they \
             want and you write the code. The user sees the app's Preview and Code beside \
             this chat; every change you make shows there at once. Work in small steps, keep \
             your messages short, and say what you changed.\n\n\
             ## How apps work\n\
             - A React 18 + TypeScript app. The manifest's `entry` file (normally App.tsx) \
               default-exports the root component. Split bigger apps into files \
               (components/…, lib/…) and import them with relative paths.\n\
             - The only packages are `react`, `react-dom`, `react-dom/client`, \
               `lucide-react`, `react-markdown`, `remark-gfm`, `jszip` and `oxide` (Oxide's \
               SDK, types below). Nothing else can be imported. `.json` and `.css` files of \
               the app can be imported too.\n\
             - The app runs in a sandboxed frame with **no network**: `fetch`, CDNs, remote \
               images, fonts and scripts are blocked. Use the SDK instead: `ai` for models \
               (text, JSON, image/video/music generation), `web` for search, fetching pages \
               and crawling, `files` to pick, save and zip files, `storage` to keep data.\n\
             - Style with Tailwind CSS classes (generated in the frame; no config). Support \
               dark mode with `dark:` variants (the frame follows Oxide's theme). Make the app \
               look clean and modern, with clear empty, loading and error states.\n\
             - Every capability the code uses (text, image, video, music, webSearch, \
               webFetch, connectors, python, sandbox, browser, chrome) must be declared in the \
               manifest with app_update_manifest; calls for undeclared ones fail. The user is \
               asked once per capability. Files and storage need no capability.\n\
             - Catch `OxideError` from SDK calls and show its message in the app.\n\
             - Downloads: `files.save(name, blob)`. Copying: `app.copy(text)`.\n\
             - The browser's own APIs all work without a package: `<canvas>` with \
               `toBlob`/`toDataURL` (PNG/JPEG export), drawing an SVG onto a canvas through \
               `new Image()` with a `data:` or `blob:` URL, `URL.createObjectURL`, \
               `FileReader`, `TextEncoder`. For charts, draw them yourself as SVG or on a \
               canvas; to save a chart as PNG, draw it on a canvas and pass \
               `canvas.toBlob` to `files.save`. Don't tell the user something can't be done \
               before trying it with these.\n\n\
             ## How to work\n\
             - Read a file before changing it. Use app_edit_file for small changes and \
               app_write_file for new files or rewrites. Keep the manifest's id.\n\
             - After changes, call app_preview_status and fix every error it reports before \
               you finish. Use app_screenshot when the look matters.\n\
             - Give the app a fitting name, description, icon and category \
               (app_update_manifest) when you first build it. Icons: {icons}, or one \
               emoji.\n\
             - Don't explain the code at length; the user can read it in Code.\n\n\
             ## This app now\n\
             Manifest (oxide-app.json):\n```json\n{manifest}\n```\nFiles:\n{list}\n\
             ## The `oxide` SDK (import {{ ai, web, files, … }} from \"oxide\")\n\
             ```ts\n{SDK_TYPES}\n```\n",
            icons = APP_ICON_NAMES.join(", ")
        )
    }
}

/// What a Preview report tells the model.
fn report_output(report: &PreviewReport, kind: PreviewRequestKind) -> ToolOutput {
    let mut t = String::new();
    if let Some(n) = &report.note {
        t.push_str(n);
        t.push('\n');
        if !report.rendered && report.errors.is_empty() && report.screenshot.is_none() {
            return ToolOutput {
                content: vec![ContentBlock::text(t.trim_end())],
                is_error: true,
                duration_ms: 0,
            };
        }
    }
    if report.errors.is_empty() {
        t.push_str(if report.rendered {
            "The app compiled and rendered without errors.\n"
        } else {
            "The app hasn't rendered yet (no errors so far).\n"
        });
    } else {
        t.push_str(&format!("Errors ({}):\n", report.errors.len()));
        for e in &report.errors {
            t.push_str(&format!(
                "- {}\n",
                e.chars().take(2_000).collect::<String>()
            ));
        }
    }
    if kind == PreviewRequestKind::Status {
        if report.console.is_empty() {
            t.push_str("Console: empty.\n");
        } else {
            t.push_str("Console (newest last):\n");
            for l in &report.console {
                t.push_str(&format!(
                    "[{}] {}\n",
                    l.level,
                    l.text.chars().take(500).collect::<String>()
                ));
            }
        }
    }
    let mut content = vec![ContentBlock::text(t.trim_end())];
    if let Some(url) = &report.screenshot
        && let Some(data) = url.strip_prefix("data:image/png;base64,")
    {
        content.push(ContentBlock::Image {
            media_type: "image/png".into(),
            source: MediaSource::Base64 {
                data: data.to_owned(),
            },
        });
    }
    ToolOutput {
        content,
        is_error: false,
        duration_ms: 0,
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
