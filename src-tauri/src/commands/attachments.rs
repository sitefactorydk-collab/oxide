//! Attaching files: the native file picker, preparing files, and image previews.

use oxide_core::{Attachment, AttachmentLimits, AttachmentSource};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Opens the system file picker. Returns the chosen paths (empty if cancelled).
#[tauri::command]
#[specta::specta]
pub async fn pick_files(app: AppHandle) -> CmdResult<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Add files")
        .add_filter(
            "Images, PDFs, text and code",
            &[
                "png", "jpg", "jpeg", "gif", "webp", "bmp", "pdf", "txt", "md", "csv", "json",
                "yaml", "yml", "toml", "xml", "html", "css", "js", "ts", "tsx", "jsx", "py", "rs",
                "go", "java", "c", "cpp", "h", "cs", "rb", "php", "sh", "ps1", "sql", "log",
            ],
        )
        .add_filter("All files", &["*"])
        .pick_files(move |paths| {
            let _ = tx.send(paths);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the file picker closed unexpectedly".into()))?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Checks, converts and stores one file for sending. Images are resized and compressed,
/// PDFs checked and their text extracted, text files validated.
#[tauri::command]
#[specta::specta]
pub async fn prepare_attachment(
    state: State<'_, AppState>,
    source: AttachmentSource,
) -> CmdResult<Attachment> {
    Ok(state.attachments.prepare(source).await?)
}

/// A stored attachment image as a `data:` URL, scaled to fit `max_side` pixels (full size
/// when `None`). Only files in the attachments folder can be read.
#[tauri::command]
#[specta::specta]
pub async fn attachment_preview(
    state: State<'_, AppState>,
    path: String,
    max_side: Option<u32>,
) -> CmdResult<String> {
    Ok(state.attachments.image_data_url(&path, max_side).await?)
}

/// The size and count limits for attachments.
#[tauri::command]
#[specta::specta]
pub fn attachment_limits(state: State<'_, AppState>) -> AttachmentLimits {
    state.attachments.limits()
}
