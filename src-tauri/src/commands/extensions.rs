//! Desktop extensions (`.mcpb`) and skills: Settings → Extensions and Settings → Skills.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oxide_core::{Extension, ExtensionPreview, ExtensionValue, Skill, SkillDetail, SkillInput};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

fn invalid(e: String) -> CommandError {
    let mut chars = e.chars();
    CommandError::Invalid(match chars.next() {
        Some(f) => f.to_uppercase().chain(chars).collect(),
        None => e,
    })
}

/// What a picker should choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PickKind {
    /// A `.mcpb` (or older `.dxt`) bundle.
    Extension,
    /// A zipped skill (`.zip`, `.skill`).
    SkillArchive,
    /// Any folder (a skill folder, or a folder setting of an extension).
    Folder,
    /// Any file (a file setting of an extension).
    File,
}

/// Opens the system picker. Returns the chosen paths (empty if cancelled).
#[tauri::command]
#[specta::specta]
pub async fn pick_paths(app: AppHandle, kind: PickKind, multiple: bool) -> CmdResult<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file();
    let dialog = match kind {
        PickKind::Extension => dialog
            .set_title("Install an extension")
            .add_filter("Desktop extensions", &["mcpb", "dxt"]),
        PickKind::SkillArchive => dialog
            .set_title("Add a skill")
            .add_filter("Zipped skills", &["zip", "skill"]),
        PickKind::Folder => dialog.set_title("Choose a folder"),
        PickKind::File => dialog.set_title("Choose a file"),
    };
    let send = move |paths: Option<Vec<tauri_plugin_dialog::FilePath>>| {
        let _ = tx.send(paths);
    };
    // Folders are picked one at a time (the form adds each to its list).
    match (kind, multiple) {
        (PickKind::Folder, _) => dialog.pick_folder(move |p| send(p.map(|p| vec![p]))),
        (_, true) => dialog.pick_files(send),
        (_, false) => dialog.pick_file(move |p| send(p.map(|p| vec![p]))),
    }
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the picker closed unexpectedly".into()))?;
    let paths: Vec<PathBuf> = picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    crate::chosen::remember(&app, &paths);
    Ok(paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

// ----- Extensions -----

#[tauri::command]
#[specta::specta]
pub async fn list_extensions(state: State<'_, AppState>) -> CmdResult<Vec<Extension>> {
    Ok(state.extensions.list().await)
}

/// Opens and checks a `.mcpb` file without installing it.
#[tauri::command]
#[specta::specta]
pub async fn preview_extension(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<ExtensionPreview> {
    let path = PathBuf::from(path);
    crate::chosen::ensure_chosen(&app, &path, "open this file as a desktop extension").await?;
    Ok(state.extensions.preview(&path).await?)
}

/// Installs (or updates) an opened bundle with the form's settings.
#[tauri::command]
#[specta::specta]
pub async fn install_extension(
    state: State<'_, AppState>,
    staging_id: String,
    values: BTreeMap<String, ExtensionValue>,
) -> CmdResult<Extension> {
    Ok(state.extensions.install(&staging_id, values).await?)
}

/// Forgets an opened bundle (the install dialog was cancelled).
#[tauri::command]
#[specta::specta]
pub async fn discard_extension(state: State<'_, AppState>, staging_id: String) -> CmdResult<()> {
    state.extensions.discard(&staging_id).await;
    Ok(())
}

/// Saves an extension's settings. Sensitive values left empty keep what's stored.
#[tauri::command]
#[specta::specta]
pub async fn configure_extension(
    state: State<'_, AppState>,
    name: String,
    values: BTreeMap<String, ExtensionValue>,
) -> CmdResult<Extension> {
    Ok(state.extensions.configure(&name, values).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_extension_enabled(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> CmdResult<()> {
    Ok(state.extensions.set_enabled(&name, enabled).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_extension(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.extensions.uninstall(&name).await?)
}

/// Opens an extension's folder, or the extensions folder, in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_extensions_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    name: Option<String>,
) -> CmdResult<()> {
    let root = state.extensions.root().to_path_buf();
    let dir = match name {
        Some(n) => root.join(n),
        None => root,
    };
    open_dir(&app, dir).await
}

async fn open_dir(app: &AppHandle, dir: PathBuf) -> CmdResult<()> {
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| CommandError::Internal(format!("couldn't create the folder: {e}")))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            tracing::warn!("couldn't open a folder: {e}");
            CommandError::Internal("The folder couldn't be opened.".into())
        })
}

// ----- Skills -----

async fn skills_blocking<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&oxide_agent::Skills) -> Result<T, String> + Send + 'static,
) -> CmdResult<T> {
    let skills = state.skills.clone();
    tokio::task::spawn_blocking(move || f(&skills))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(invalid)
}

#[tauri::command]
#[specta::specta]
pub async fn list_skills(state: State<'_, AppState>) -> CmdResult<Vec<Skill>> {
    skills_blocking(&state, |s| Ok(s.list())).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_skill(state: State<'_, AppState>, folder: String) -> CmdResult<SkillDetail> {
    skills_blocking(&state, move |s| s.detail(&folder)).await
}

/// Writes a new skill from the form.
#[tauri::command]
#[specta::specta]
pub async fn create_skill(state: State<'_, AppState>, input: SkillInput) -> CmdResult<Skill> {
    skills_blocking(&state, move |s| s.create(&input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_skill(
    state: State<'_, AppState>,
    folder: String,
    input: SkillInput,
) -> CmdResult<Skill> {
    skills_blocking(&state, move |s| s.update(&folder, &input)).await
}

/// Adds a skill from a folder or a zip (`.zip`, `.skill`) on disk.
#[tauri::command]
#[specta::specta]
pub async fn add_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<Skill> {
    crate::chosen::ensure_chosen(&app, std::path::Path::new(&path), "add a skill from here")
        .await?;
    skills_blocking(&state, move |s| {
        let p = PathBuf::from(&path);
        if p.is_dir() {
            s.add_folder(&p)
        } else {
            s.add_archive(&p)
        }
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_skill_enabled(
    state: State<'_, AppState>,
    folder: String,
    enabled: bool,
) -> CmdResult<()> {
    skills_blocking(&state, move |s| s.set_enabled(&folder, enabled)).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_skill(state: State<'_, AppState>, folder: String) -> CmdResult<()> {
    skills_blocking(&state, move |s| s.remove(&folder)).await
}

/// Opens a skill's folder, or the skills folder, in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_skills_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: Option<String>,
) -> CmdResult<()> {
    let root = state.skills.root().to_path_buf();
    let dir = match folder {
        Some(f) => {
            let d = root.join(&f);
            if f.contains(['/', '\\']) || f.contains("..") || !d.is_dir() {
                return Err(CommandError::NotFound("skill not found".into()));
            }
            d
        }
        None => root,
    };
    open_dir(&app, dir).await
}
