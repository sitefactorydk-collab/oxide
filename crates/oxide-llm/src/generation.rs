//! Media generation on the OpenAI-compatible API (Phase 19): images
//! (`/images/generations`, `/images/edits`), speech (`/audio/speech`) and video (`/videos`:
//! create, poll the job, download the result).
//!
//! Shapes follow OpenAI's API reference (checked 2026-11-19). Other servers that speak the
//! same API (Gemini's and Groq's compatible endpoints, LocalAI, vLLM, gateways) work as far as
//! they implement it; a server without the endpoint gets a plain "this endpoint doesn't
//! offer …" error. Keys never go into URLs, and downloads of result URLs carry no key.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use oxide_core::ProviderKind;
use serde_json::{Map, Value, json};

use crate::Endpoint;
use crate::LlmError;
use crate::http::{Multipart, USER_AGENT, describe, error_message, status_error};

/// Images, video and audio larger than this aren't downloaded.
pub const MAX_RESULT_BYTES: usize = 200 * 1024 * 1024;
/// OpenAI's limit on the text of one speech request.
pub const MAX_SPEECH_CHARS: usize = 4096;
/// Images can take a few minutes at high quality.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// An image given to the model as a reference (edits, a video's first frame).
#[derive(Clone)]
pub struct InputImage {
    pub name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for InputImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputImage")
            .field("name", &self.name)
            .field("media_type", &self.media_type)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// What an endpoint made.
#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub bytes: Vec<u8>,
    pub media_type: String,
    /// The prompt the endpoint actually used, when it rewrote it (DALL·E 3).
    pub note: Option<String>,
}

impl std::fmt::Debug for GeneratedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedFile")
            .field("media_type", &self.media_type)
            .field("bytes", &self.bytes.len())
            .field("note", &self.note)
            .finish()
    }
}

/// An image request. With `references`, `/images/edits` is used.
#[derive(Debug, Clone, Default)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    /// How many images (1–10).
    pub n: u32,
    /// `1024x1024`, `1536x1024`, `auto`, …
    pub size: Option<String>,
    /// `low`, `medium`, `high`, `auto`, …
    pub quality: Option<String>,
    /// `transparent`, `opaque`, `auto`.
    pub background: Option<String>,
    pub references: Vec<InputImage>,
}

/// A speech request.
#[derive(Debug, Clone, Default)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
    /// `mp3`, `wav`, `opus`, `aac`, `flac`.
    pub format: String,
    /// How to speak (tone, pace); not every model takes it.
    pub instructions: Option<String>,
    /// 0.25–4.0.
    pub speed: Option<f32>,
}

/// A video request.
#[derive(Debug, Clone, Default)]
pub struct VideoRequest {
    pub model: String,
    pub prompt: String,
    /// Length in seconds (Sora took 4, 8 or 12; other servers differ).
    pub seconds: Option<u32>,
    /// `1280x720`, `720x1280`, …
    pub size: Option<String>,
    /// An image the video starts from.
    pub reference: Option<InputImage>,
}

/// How often a video job is checked, and for how long.
#[derive(Debug, Clone, Copy)]
pub struct VideoPolling {
    pub interval: Duration,
    pub max_wait: Duration,
}

impl Default for VideoPolling {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            max_wait: Duration::from_secs(20 * 60),
        }
    }
}

/// Where a video job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    Queued,
    InProgress,
    Completed,
    Failed,
}

/// A video job as the endpoint reported it.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStatus {
    pub id: String,
    pub state: VideoState,
    /// 0–100.
    pub progress: Option<u32>,
    pub error: Option<String>,
    /// A direct link to the result some servers give instead of `/content`.
    pub url: Option<String>,
}

fn client() -> Result<reqwest::Client, LlmError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| LlmError::Invalid(format!("HTTP client: {e}")))
}

fn check_endpoint(endpoint: &Endpoint, what: &str) -> Result<(), LlmError> {
    if endpoint.kind == ProviderKind::Anthropic {
        return Err(LlmError::Generation(format!(
            "Anthropic's API doesn't make {what}; pick an OpenAI-compatible endpoint in \
             Settings → Media generation"
        )));
    }
    if !(endpoint.base_url.starts_with("http://") || endpoint.base_url.starts_with("https://")) {
        return Err(LlmError::Invalid(
            "the base URL must start with http:// or https://".into(),
        ));
    }
    Ok(())
}

fn authed(endpoint: &Endpoint, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match endpoint.api_key() {
        Some(k) => req.bearer_auth(k),
        None => req,
    }
}

async fn send(
    endpoint: &Endpoint,
    req: reqwest::RequestBuilder,
) -> Result<reqwest::Response, LlmError> {
    authed(endpoint, req)
        .send()
        .await
        .map_err(|e| LlmError::Unreachable {
            url: endpoint.base_url.clone(),
            reason: describe(&e),
        })
}

/// A failed answer: a missing endpoint says so plainly, anything else as the API put it.
fn failure(status: u16, body: &str, what: &str, path: &str) -> LlmError {
    let message = error_message(body);
    let about_model = message.to_ascii_lowercase().contains("model");
    if matches!(status, 404 | 405 | 501) && !about_model {
        return LlmError::Generation(format!("this endpoint doesn't offer {what} ({path})"));
    }
    status_error(status, body)
}

/// Reads a successful body, refusing anything over [`MAX_RESULT_BYTES`].
async fn read_body(mut res: reqwest::Response, base_url: &str) -> Result<Vec<u8>, LlmError> {
    if res
        .content_length()
        .is_some_and(|n| n > MAX_RESULT_BYTES as u64)
    {
        return Err(LlmError::Generation(
            "the result is larger than 200 MB".into(),
        ));
    }
    let mut out = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| LlmError::Unreachable {
        url: base_url.to_owned(),
        reason: describe(&e),
    })? {
        if out.len() + chunk.len() > MAX_RESULT_BYTES {
            return Err(LlmError::Generation(
                "the result is larger than 200 MB".into(),
            ));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn content_type(res: &reqwest::Response) -> String {
    res.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

/// The image type of some bytes, by their first bytes.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// Downloads a result URL an endpoint gave (no key is sent along).
async fn download(url: &str, base_url: &str) -> Result<(Vec<u8>, String), LlmError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(LlmError::Decode(
            "the result link isn't a web address".into(),
        ));
    }
    let res = client()?
        .get(url)
        .send()
        .await
        .map_err(|e| LlmError::Unreachable {
            url: base_url.to_owned(),
            reason: format!("downloading the result: {}", describe(&e)),
        })?;
    let status = res.status().as_u16();
    if !(200..300).contains(&status) {
        let body = res.text().await.unwrap_or_default();
        return Err(LlmError::Http {
            status,
            message: format!("downloading the result: {}", error_message(&body)),
        });
    }
    let mime = content_type(&res);
    Ok((read_body(res, base_url).await?, mime))
}

fn optional(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
        map.insert(key.into(), json!(v));
    }
}

/// Makes images from a prompt, or from reference images and a prompt.
pub async fn generate_images(
    endpoint: &Endpoint,
    req: &ImageRequest,
) -> Result<Vec<GeneratedFile>, LlmError> {
    check_endpoint(endpoint, "images")?;
    if req.prompt.trim().is_empty() {
        return Err(LlmError::Generation("the image needs a description".into()));
    }
    let n = req.n.clamp(1, 10);
    let edit = !req.references.is_empty();
    let path = if edit {
        "/images/edits"
    } else {
        "/images/generations"
    };
    let url = format!("{}{path}", endpoint.base_url);
    let http = client()?.post(&url);
    let http = if edit {
        let mut form = Multipart::new();
        form.text("model", req.model.trim())
            .text("prompt", &req.prompt)
            .text("n", &n.to_string());
        for (k, v) in [
            ("size", &req.size),
            ("quality", &req.quality),
            ("background", &req.background),
        ] {
            if let Some(v) = v.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                form.text(k, v);
            }
        }
        // One image goes as `image`; several as `image[]` (OpenAI's GPT image models).
        let field = if req.references.len() == 1 {
            "image"
        } else {
            "image[]"
        };
        for img in req.references.iter().take(16) {
            form.file(field, &img.name, &img.media_type, &img.bytes);
        }
        let (ct, body) = form.finish();
        http.header(reqwest::header::CONTENT_TYPE, ct).body(body)
    } else {
        let mut body = Map::new();
        body.insert("model".into(), json!(req.model.trim()));
        body.insert("prompt".into(), json!(req.prompt));
        body.insert("n".into(), json!(n));
        optional(&mut body, "size", req.size.as_deref());
        optional(&mut body, "quality", req.quality.as_deref());
        optional(&mut body, "background", req.background.as_deref());
        http.json(&Value::Object(body))
    };
    let res = send(endpoint, http).await?;
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        let what = if edit {
            "image editing"
        } else {
            "image generation"
        };
        return Err(failure(status, &text, what, path));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| LlmError::Decode("the image answer isn't JSON".into()))?;
    let items = v
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for item in items {
        let note = item
            .get("revised_prompt")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty());
        if let Some(b64) = item.get("b64_json").and_then(Value::as_str) {
            let bytes = STANDARD
                .decode(b64.trim())
                .map_err(|_| LlmError::Decode("an image isn't valid base64".into()))?;
            let media_type = sniff_image(&bytes).unwrap_or("image/png").to_owned();
            out.push(GeneratedFile {
                bytes,
                media_type,
                note,
            });
        } else if let Some(link) = item.get("url").and_then(Value::as_str) {
            let (bytes, mime) = if let Some(data) = link.strip_prefix("data:") {
                let (head, b64) = data.split_once(',').unwrap_or(("", data));
                let bytes = STANDARD
                    .decode(b64.trim())
                    .map_err(|_| LlmError::Decode("an image isn't valid base64".into()))?;
                (bytes, head.split(';').next().unwrap_or("").to_owned())
            } else {
                download(link, &endpoint.base_url).await?
            };
            let media_type = sniff_image(&bytes)
                .map(str::to_owned)
                .or_else(|| mime.starts_with("image/").then_some(mime))
                .unwrap_or_else(|| "image/png".into());
            out.push(GeneratedFile {
                bytes,
                media_type,
                note,
            });
        }
    }
    if out.is_empty() {
        return Err(LlmError::Decode("the endpoint returned no images".into()));
    }
    Ok(out)
}

/// The media type of a speech format.
pub fn audio_media_type(format: &str) -> &'static str {
    match format {
        "wav" => "audio/wav",
        "opus" => "audio/ogg",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        _ => "audio/mpeg",
    }
}

/// Reads text aloud.
pub async fn generate_speech(
    endpoint: &Endpoint,
    req: &SpeechRequest,
) -> Result<GeneratedFile, LlmError> {
    check_endpoint(endpoint, "speech")?;
    let input = req.input.trim();
    if input.is_empty() {
        return Err(LlmError::Generation("there's no text to read aloud".into()));
    }
    if input.chars().count() > MAX_SPEECH_CHARS {
        return Err(LlmError::Generation(format!(
            "the text is longer than the {MAX_SPEECH_CHARS} characters one request takes; \
             split it into parts"
        )));
    }
    let format = if req.format.trim().is_empty() {
        "mp3"
    } else {
        req.format.trim()
    };
    let mut body = Map::new();
    body.insert("model".into(), json!(req.model.trim()));
    body.insert("input".into(), json!(input));
    body.insert("voice".into(), json!(req.voice.trim()));
    body.insert("response_format".into(), json!(format));
    optional(&mut body, "instructions", req.instructions.as_deref());
    if let Some(speed) = req.speed.filter(|s| s.is_finite()) {
        body.insert("speed".into(), json!(speed.clamp(0.25, 4.0)));
    }
    let url = format!("{}/audio/speech", endpoint.base_url);
    let res = send(endpoint, client()?.post(&url).json(&Value::Object(body))).await?;
    let status = res.status().as_u16();
    if !(200..300).contains(&status) {
        let text = res.text().await.unwrap_or_default();
        return Err(failure(status, &text, "text-to-speech", "/audio/speech"));
    }
    let mime = content_type(&res);
    let bytes = read_body(res, &endpoint.base_url).await?;
    if mime == "application/json" || mime.starts_with("text/") {
        // Some servers answer an error with 200.
        let text = String::from_utf8_lossy(&bytes);
        return Err(LlmError::Decode(format!(
            "the endpoint sent text instead of audio: {}",
            error_message(&text)
        )));
    }
    if bytes.is_empty() {
        return Err(LlmError::Decode("the endpoint sent no audio".into()));
    }
    let media_type = if mime.starts_with("audio/") {
        mime
    } else {
        audio_media_type(format).to_owned()
    };
    Ok(GeneratedFile {
        bytes,
        media_type,
        note: None,
    })
}

fn video_id(v: &Value) -> Option<String> {
    v.get("id")
        .or_else(|| v.get("task_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 200
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
        })
}

/// Reads a video job object.
pub fn parse_video_status(v: &Value) -> Result<VideoStatus, LlmError> {
    let id =
        video_id(v).ok_or_else(|| LlmError::Decode("the video job has no usable id".into()))?;
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let state = match status.as_str() {
        "completed" | "complete" | "succeeded" | "success" | "done" | "finished" => {
            VideoState::Completed
        }
        "failed" | "error" | "cancelled" | "canceled" | "expired" | "rejected" => {
            VideoState::Failed
        }
        "queued" | "pending" | "submitted" | "" => VideoState::Queued,
        _ => VideoState::InProgress,
    };
    let progress = v.get("progress").and_then(|p| {
        p.as_f64().or_else(|| {
            p.as_str()
                .and_then(|s| s.trim_end_matches('%').parse().ok())
        })
    });
    let progress = progress
        .filter(|p| p.is_finite())
        .map(|p| p.clamp(0.0, 100.0).round() as u32);
    let error = match v.get("error") {
        Some(Value::Null) | None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(e) => e
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(e.to_string())),
    };
    let url = ["video_url", "url", "download_url"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .map(str::to_owned);
    Ok(VideoStatus {
        id,
        state,
        progress,
        error,
        url,
    })
}

async fn video_status(endpoint: &Endpoint, id: &str) -> Result<VideoStatus, LlmError> {
    let url = format!("{}/videos/{id}", endpoint.base_url);
    let res = send(endpoint, client()?.get(&url)).await?;
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(status_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| LlmError::Decode("the video job answer isn't JSON".into()))?;
    parse_video_status(&v)
}

/// Makes a video: starts the job, checks it every `polling.interval` (calling `progress` with
/// each status), and downloads the result. Dropping the future stops waiting; the job itself
/// may still finish on the server.
pub async fn generate_video(
    endpoint: &Endpoint,
    req: &VideoRequest,
    polling: VideoPolling,
    mut progress: impl FnMut(&VideoStatus) + Send,
) -> Result<GeneratedFile, LlmError> {
    check_endpoint(endpoint, "videos")?;
    if req.prompt.trim().is_empty() {
        return Err(LlmError::Generation("the video needs a description".into()));
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(req.model.trim()));
    body.insert("prompt".into(), json!(req.prompt));
    if let Some(s) = req.seconds {
        // OpenAI's API takes the length as a string.
        body.insert("seconds".into(), json!(s.clamp(1, 120).to_string()));
    }
    optional(&mut body, "size", req.size.as_deref());
    if let Some(img) = &req.reference {
        let url = format!(
            "data:{};base64,{}",
            img.media_type,
            STANDARD.encode(&img.bytes)
        );
        body.insert("input_reference".into(), json!({ "image_url": url }));
    }
    let url = format!("{}/videos", endpoint.base_url);
    let res = send(endpoint, client()?.post(&url).json(&Value::Object(body))).await?;
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(failure(status, &text, "video generation", "/videos"));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| LlmError::Decode("the video answer isn't JSON".into()))?;
    let mut job = parse_video_status(&v)?;
    tracing::info!(job = %job.id, "video job started");
    let started = Instant::now();
    loop {
        progress(&job);
        match job.state {
            VideoState::Completed => break,
            VideoState::Failed => {
                return Err(LlmError::Generation(format!(
                    "the video couldn't be made: {}",
                    job.error
                        .as_deref()
                        .unwrap_or("the endpoint gave no reason")
                )));
            }
            VideoState::Queued | VideoState::InProgress => {}
        }
        if started.elapsed() >= polling.max_wait {
            return Err(LlmError::Generation(format!(
                "the video wasn't ready after {} minutes (job {})",
                polling.max_wait.as_secs().div_ceil(60),
                job.id
            )));
        }
        tokio::time::sleep(polling.interval).await;
        job = video_status(endpoint, &job.id).await?;
    }
    // The finished file: `/videos/{id}/content`, or a link the job gave.
    let url = format!("{}/videos/{}/content", endpoint.base_url, job.id);
    let res = send(endpoint, client()?.get(&url)).await?;
    let status = res.status().as_u16();
    let (bytes, mime) = if (200..300).contains(&status) {
        let mime = content_type(&res);
        (read_body(res, &endpoint.base_url).await?, mime)
    } else if let Some(link) = &job.url {
        download(link, &endpoint.base_url).await?
    } else {
        let text = res.text().await.unwrap_or_default();
        return Err(failure(
            status,
            &text,
            "video downloads",
            "/videos/{id}/content",
        ));
    };
    if bytes.is_empty() {
        return Err(LlmError::Decode("the endpoint sent an empty video".into()));
    }
    let media_type = if mime.starts_with("video/") {
        mime
    } else {
        "video/mp4".to_owned()
    };
    Ok(GeneratedFile {
        bytes,
        media_type,
        note: None,
    })
}

/// Every model id an endpoint lists (`GET /models`), sorted. Unlike the chat model list,
/// image, speech and video models are kept.
pub async fn list_model_ids(endpoint: &Endpoint) -> Result<Vec<String>, LlmError> {
    check_endpoint(endpoint, "media")?;
    let url = format!("{}/models", endpoint.base_url);
    let res = send(
        endpoint,
        client()?.get(&url).timeout(Duration::from_secs(30)),
    )
    .await?;
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(failure(status, &text, "a model list", "/models"));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| LlmError::Decode("the model list isn't JSON".into()))?;
    let list = v
        .get("data")
        .or_else(|| v.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids: Vec<String> = list
        .iter()
        .filter_map(|m| {
            m.get("id")
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)
                .map(|s| s.trim_start_matches("models/").to_owned())
        })
        .filter(|s| !s.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Reply, serve};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-png";

    fn endpoint(base: &str) -> Endpoint {
        Endpoint::new("gen", ProviderKind::OpenAi, base, Some("sk-test".into()))
    }

    #[tokio::test]
    async fn image_requests_are_json_and_bad_links_are_reported() {
        let server = serve(vec![Reply::json(&format!(
            r#"{{"created":1,"data":[{{"b64_json":"{}"}},{{"url":"LINK"}}]}}"#,
            STANDARD.encode(PNG)
        ))])
        .await;
        let req = ImageRequest {
            model: "gpt-image-2".into(),
            prompt: "a fox".into(),
            n: 2,
            size: Some("1024x1024".into()),
            quality: Some(" ".into()),
            ..ImageRequest::default()
        };
        // "LINK" isn't a web address, so the second image fails clearly.
        let err = generate_images(&endpoint(&server.base), &req)
            .await
            .expect_err("bad link");
        assert!(err.to_string().contains("web address"), "{err}");
        let sent = &server.requests().await[0];
        assert!(
            sent.head.starts_with("POST /v1/images/generations"),
            "{}",
            sent.head
        );
        assert!(
            sent.head
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-test")
        );
        let body: Value = serde_json::from_str(&sent.body).expect("json body");
        assert_eq!(body["model"], "gpt-image-2");
        assert_eq!(body["n"], 2);
        assert_eq!(body["size"], "1024x1024");
        assert!(body.get("quality").is_none(), "blank values aren't sent");
        assert!(body.get("response_format").is_none());
    }

    #[tokio::test]
    async fn image_links_are_downloaded_without_the_key() {
        let files = serve(vec![Reply::bytes("image/jpeg", b"\xff\xd8\xff\xe0jpeg")]).await;
        let link = format!("{}/img.jpg", files.base);
        let api = serve(vec![Reply::json(&format!(
            r#"{{"data":[{{"b64_json":"{}"}},{{"url":"{link}","revised_prompt":"a red fox"}}]}}"#,
            STANDARD.encode(PNG)
        ))])
        .await;
        let req = ImageRequest {
            model: "dall-e-3".into(),
            prompt: "a fox".into(),
            n: 2,
            ..ImageRequest::default()
        };
        let out = generate_images(&endpoint(&api.base), &req)
            .await
            .expect("images");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].media_type, "image/png");
        assert_eq!(out[0].bytes, PNG);
        assert_eq!(out[1].media_type, "image/jpeg");
        assert_eq!(out[1].note.as_deref(), Some("a red fox"));
        let got = &files.requests().await[0];
        assert!(got.head.starts_with("GET /v1/img.jpg"), "{}", got.head);
        assert!(!got.head.to_ascii_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn reference_images_go_to_edits_as_multipart() {
        let server = serve(vec![
            Reply::json(&format!(
                r#"{{"data":[{{"b64_json":"{}"}}]}}"#,
                STANDARD.encode(PNG)
            )),
            Reply::json(&format!(
                r#"{{"data":[{{"b64_json":"{}"}}]}}"#,
                STANDARD.encode(PNG)
            )),
        ])
        .await;
        let img = |n: &str| InputImage {
            name: n.into(),
            media_type: "image/png".into(),
            bytes: PNG.to_vec(),
        };
        let mut req = ImageRequest {
            model: "gpt-image-2".into(),
            prompt: "make it blue".into(),
            n: 1,
            references: vec![img("a.png")],
            ..ImageRequest::default()
        };
        generate_images(&endpoint(&server.base), &req)
            .await
            .expect("edit");
        req.references.push(img("b.png"));
        generate_images(&endpoint(&server.base), &req)
            .await
            .expect("edit");
        let sent = server.requests().await;
        assert!(sent[0].head.starts_with("POST /v1/images/edits"));
        assert!(
            sent[0]
                .head
                .contains("multipart/form-data; boundary=oxide-")
        );
        assert!(
            sent[0]
                .body
                .contains("name=\"prompt\"\r\n\r\nmake it blue\r\n")
        );
        assert!(
            sent[0]
                .body
                .contains("name=\"image\"; filename=\"a.png\"\r\nContent-Type: image/png")
        );
        assert!(
            sent[1]
                .body
                .contains("name=\"image[]\"; filename=\"b.png\"")
        );
    }

    #[tokio::test]
    async fn missing_endpoints_and_errors_are_explained() {
        let server = serve(vec![
            Reply::status("404 Not Found", "Not Found"),
            Reply::status(
                "404 Not Found",
                r#"{"error":{"message":"The model `x` does not exist"}}"#,
            ),
            Reply::status("401 Unauthorized", r#"{"error":{"message":"bad key"}}"#),
            Reply::status("405 Method Not Allowed", ""),
            Reply::status("404 Not Found", "{}"),
            Reply::status("400 Bad Request", r#"{"error":{"message":"bad size"}}"#),
        ])
        .await;
        let e = endpoint(&server.base);
        let req = ImageRequest {
            model: "x".into(),
            prompt: "p".into(),
            ..ImageRequest::default()
        };
        let err = generate_images(&e, &req).await.expect_err("404");
        assert_eq!(
            err.to_string(),
            "this endpoint doesn't offer image generation (/images/generations)"
        );
        let err = generate_images(&e, &req).await.expect_err("model");
        assert!(err.to_string().contains("does not exist"), "{err}");
        let err = generate_images(&e, &req).await.expect_err("401");
        assert!(matches!(err, LlmError::Unauthorized { status: 401 }));
        let speech = SpeechRequest {
            model: "tts".into(),
            input: "hi".into(),
            voice: "alloy".into(),
            ..SpeechRequest::default()
        };
        let err = generate_speech(&e, &speech).await.expect_err("405");
        assert!(
            err.to_string().contains("doesn't offer text-to-speech"),
            "{err}"
        );
        let video = VideoRequest {
            model: "v".into(),
            prompt: "p".into(),
            ..VideoRequest::default()
        };
        let err = generate_video(&e, &video, VideoPolling::default(), |_| {})
            .await
            .expect_err("404");
        assert!(
            err.to_string()
                .contains("doesn't offer video generation (/videos)")
        );
        let err = generate_images(&e, &req).await.expect_err("400");
        assert!(err.to_string().contains("bad size"), "{err}");

        let anthropic = Endpoint::new("a", ProviderKind::Anthropic, "https://x/v1", None);
        let err = generate_images(&anthropic, &req)
            .await
            .expect_err("anthropic");
        assert!(
            err.to_string()
                .contains("Anthropic's API doesn't make images")
        );
    }

    #[tokio::test]
    async fn speech_returns_audio_bytes() {
        let server = serve(vec![
            Reply::bytes("audio/mpeg", b"ID3fake-mp3"),
            Reply::bytes("application/octet-stream", b"RIFFfake-wav"),
            Reply::json(r#"{"error":{"message":"voice not found"}}"#),
        ])
        .await;
        let e = endpoint(&server.base);
        let mut req = SpeechRequest {
            model: "gpt-4o-mini-tts".into(),
            input: "Hello there".into(),
            voice: "nova".into(),
            format: "mp3".into(),
            instructions: Some("Cheerful".into()),
            speed: Some(9.0),
        };
        let out = generate_speech(&e, &req).await.expect("mp3");
        assert_eq!(out.media_type, "audio/mpeg");
        assert_eq!(out.bytes, b"ID3fake-mp3");
        req.format = "wav".into();
        let out = generate_speech(&e, &req).await.expect("wav");
        assert_eq!(out.media_type, "audio/wav");
        let err = generate_speech(&e, &req).await.expect_err("json");
        assert!(err.to_string().contains("voice not found"), "{err}");
        let sent = server.requests().await;
        assert!(sent[0].head.starts_with("POST /v1/audio/speech"));
        let body: Value = serde_json::from_str(&sent[0].body).expect("json");
        assert_eq!(body["voice"], "nova");
        assert_eq!(body["response_format"], "mp3");
        assert_eq!(body["instructions"], "Cheerful");
        assert_eq!(body["speed"], 4.0);

        req.input = "x".repeat(MAX_SPEECH_CHARS + 1);
        assert!(generate_speech(&e, &req).await.is_err());
        req.input = "  ".into();
        assert!(generate_speech(&e, &req).await.is_err());
    }

    #[tokio::test]
    async fn videos_are_polled_with_progress_and_downloaded() {
        let server = serve(vec![
            Reply::json(r#"{"id":"video_123","object":"video","status":"queued","progress":0}"#),
            Reply::json(r#"{"id":"video_123","status":"in_progress","progress":42.4}"#),
            Reply::json(r#"{"id":"video_123","status":"completed","progress":100}"#),
            Reply::bytes("video/mp4", b"\x00\x00\x00\x18ftypmp42fake"),
        ])
        .await;
        let req = VideoRequest {
            model: "sora-2".into(),
            prompt: "a cat on a piano".into(),
            seconds: Some(5),
            size: Some("1280x720".into()),
            reference: Some(InputImage {
                name: "start.png".into(),
                media_type: "image/png".into(),
                bytes: PNG.to_vec(),
            }),
        };
        let polling = VideoPolling {
            interval: Duration::from_millis(10),
            max_wait: Duration::from_secs(10),
        };
        let mut seen = Vec::new();
        let out = generate_video(&endpoint(&server.base), &req, polling, |s| {
            seen.push((s.state, s.progress));
        })
        .await
        .expect("video");
        assert_eq!(out.media_type, "video/mp4");
        assert!(out.bytes.ends_with(b"fake"));
        assert_eq!(
            seen,
            vec![
                (VideoState::Queued, Some(0)),
                (VideoState::InProgress, Some(42)),
                (VideoState::Completed, Some(100)),
            ]
        );
        let sent = server.requests().await;
        assert!(
            sent[0].head.starts_with("POST /v1/videos "),
            "{}",
            sent[0].head
        );
        let body: Value = serde_json::from_str(&sent[0].body).expect("json");
        assert_eq!(body["seconds"], "5");
        assert_eq!(body["size"], "1280x720");
        assert!(
            body["input_reference"]["image_url"]
                .as_str()
                .is_some_and(|u| u.starts_with("data:image/png;base64,"))
        );
        assert!(sent[1].head.starts_with("GET /v1/videos/video_123 "));
        assert!(sent[3].head.starts_with("GET /v1/videos/video_123/content"));
    }

    #[tokio::test]
    async fn failed_and_slow_video_jobs_end_with_a_reason() {
        let server = serve(vec![
            Reply::json(r#"{"id":"v1","status":"queued"}"#),
            Reply::json(
                r#"{"id":"v1","status":"failed","error":{"code":"moderation","message":"blocked by moderation"}}"#,
            ),
            Reply::json(r#"{"id":"v2","status":"in_progress","progress":"10%"}"#),
            Reply::json(r#"{"id":"v2","status":"in_progress","progress":"20%"}"#),
            Reply::json(r#"{"id":"../etc","status":"queued"}"#),
        ])
        .await;
        let e = endpoint(&server.base);
        let req = VideoRequest {
            model: "v".into(),
            prompt: "p".into(),
            ..VideoRequest::default()
        };
        let fast = VideoPolling {
            interval: Duration::from_millis(10),
            max_wait: Duration::from_secs(10),
        };
        let err = generate_video(&e, &req, fast, |_| {})
            .await
            .expect_err("failed");
        assert!(err.to_string().contains("blocked by moderation"), "{err}");
        let short = VideoPolling {
            interval: Duration::from_millis(30),
            max_wait: Duration::from_millis(20),
        };
        let mut last = None;
        let err = generate_video(&e, &req, short, |s| last = s.progress)
            .await
            .expect_err("slow");
        assert!(err.to_string().contains("wasn't ready"), "{err}");
        assert!(err.to_string().contains("job v2"), "{err}");
        assert_eq!(last, Some(20));
        let err = generate_video(&e, &req, fast, |_| {})
            .await
            .expect_err("bad id");
        assert!(err.to_string().contains("no usable id"), "{err}");
    }

    #[tokio::test]
    async fn finished_videos_fall_back_to_the_link_the_job_gave() {
        let files = serve(vec![Reply::bytes("application/octet-stream", b"mp4-bytes")]).await;
        let link = format!("{}/out.mp4", files.base);
        let api = serve(vec![
            Reply::json(&format!(
                r#"{{"id":"task_1","status":"succeeded","video_url":"{link}"}}"#
            )),
            Reply::status("404 Not Found", "{}"),
        ])
        .await;
        let req = VideoRequest {
            model: "v".into(),
            prompt: "p".into(),
            ..VideoRequest::default()
        };
        let out = generate_video(&endpoint(&api.base), &req, VideoPolling::default(), |_| {})
            .await
            .expect("video");
        assert_eq!(out.bytes, b"mp4-bytes");
        assert_eq!(out.media_type, "video/mp4");
    }

    #[tokio::test]
    async fn model_ids_are_listed_in_full() {
        let server = serve(vec![Reply::json(
            r#"{"data":[{"id":"whisper-1"},{"id":"gpt-image-2"},{"id":"gpt-4o"},{"id":"gpt-image-2"}]}"#,
        )])
        .await;
        let ids = list_model_ids(&endpoint(&server.base)).await.expect("ids");
        assert_eq!(ids, vec!["gpt-4o", "gpt-image-2", "whisper-1"]);
    }

    #[test]
    fn image_types_are_sniffed() {
        assert_eq!(sniff_image(PNG), Some("image/png"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"nope"), None);
        assert_eq!(audio_media_type("opus"), "audio/ogg");
    }
}
