//! Settings → Usage (tokens per provider and model), Settings → About's start-up time and
//! memory, and dictation (speech to text) for the composer (Phase 18).

use std::sync::OnceLock;

use base64::Engine;
use oxide_core::UsageReport;
use oxide_core::usage::build_usage_report;
use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Token usage of saved replies over the last `days` days including today (`0`: all time),
/// grouped in the user's zone (`offset_minutes` east of UTC, from the window's clock).
#[tauri::command]
#[specta::specta]
pub async fn usage_report(
    state: State<'_, AppState>,
    days: u32,
    offset_minutes: i32,
) -> CmdResult<UsageReport> {
    let now = oxide_store::now_ms();
    let offset = offset_minutes.clamp(-14 * 60, 14 * 60);
    // Read a day extra on each side; the report keeps exactly the asked range.
    let since = (days > 0).then(|| now - (i64::from(days) + 1) * 86_400_000);
    let records = state.store.usage_records(since).await?;
    Ok(build_usage_report(&records, now, days, offset))
}

static READY_MS: OnceLock<u32> = OnceLock::new();

/// Called once by the main window when its first screen is drawn; logs the start-up time.
#[tauri::command]
#[specta::specta]
pub fn app_ready() {
    let ms = crate::logging::started().elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
    if READY_MS.set(ms).is_ok() {
        tracing::info!(
            "start-up: first screen after {ms} ms (target 1500 ms){}",
            match oxide_mcp::process::own_memory_bytes() {
                Some(b) => format!(", {} MB in use", b / (1024 * 1024)),
                None => String::new(),
            }
        );
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppMetrics {
    /// From the process start to the main window's first screen (`None` until it reports).
    pub startup_ms: Option<u32>,
    /// Oxide's own process (the web view's processes aren't included), where the system says.
    pub memory_bytes: Option<u64>,
}

/// Start-up time and memory for Settings → About.
#[tauri::command]
#[specta::specta]
pub fn app_metrics() -> AppMetrics {
    AppMetrics {
        startup_ms: READY_MS.get().copied(),
        memory_bytes: oxide_mcp::process::own_memory_bytes(),
    }
}

/// Turns a recording from the composer's microphone into text, with the endpoint and model
/// set in Settings → Voice. `audio` is base64.
#[tauri::command]
#[specta::specta]
pub async fn transcribe_audio(
    state: State<'_, AppState>,
    audio: String,
    mime: String,
) -> CmdResult<String> {
    let settings = state.store.app_settings().await?.dictation;
    let Some(provider_id) = settings.provider_id.clone() else {
        return Err(CommandError::Invalid(
            "Dictation isn't set up yet: pick an endpoint in Settings → Voice.".into(),
        ));
    };
    if audio.len() > oxide_llm::transcribe::MAX_AUDIO_BYTES * 4 / 3 + 16 {
        return Err(CommandError::Invalid(
            "The recording is too long to transcribe (25 MB at most).".into(),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio.trim())
        .map_err(|_| CommandError::Invalid("The recording couldn't be read.".into()))?;
    let (config, key) = state.models.endpoint(&provider_id).await?;
    let endpoint = oxide_llm::Endpoint::new(config.id.clone(), config.kind, &config.base_url, key);
    let language = Some(settings.language.as_str()).filter(|l| !l.is_empty());
    oxide_llm::transcribe::transcribe(
        &endpoint,
        &settings.model,
        oxide_llm::transcribe::Recording {
            bytes: &bytes,
            mime: &mime,
        },
        language,
    )
    .await
    .map_err(|e| CommandError::Invalid(format!("Dictation failed: {e}")))
}
