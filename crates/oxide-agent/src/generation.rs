//! Media generation for chats and Settings → Media generation (Phase 19).
//!
//! [`Generation`] resolves the endpoint of each kind of generation (a provider from Settings →
//! Providers or a custom URL, with an optional key of its own in the keychain), tests it, lists
//! its models, and runs the chat tools `generate_image`, `generate_video` and
//! `generate_music`. Results are stored with the attachments, so they show in the transcript
//! and can be downloaded.

use std::sync::Arc;

use oxide_core::{
    ContentBlock, GenerationEndpoint, GenerationKind, GenerationSettings, GenerationTest,
    MediaSource, ProviderKind, ToolProgress,
};
use oxide_llm::Endpoint;
use oxide_llm::generation::{
    self as gen_api, ImageRequest, InputImage, MusicRequest, VideoPolling, VideoRequest,
    VideoState, VideoStatus,
};
use oxide_store::{Secret, SecretStore, Store};
use serde_json::{Value, json};

use crate::attachments::{AttachmentStore, format_bytes};
use crate::tools::{Builtin, ToolEntry, ToolKind, ToolOutput};

/// Most images one call makes.
pub const MAX_IMAGES_PER_CALL: u32 = 4;
/// Most reference images one call sends.
pub const MAX_REFERENCES: usize = 4;

fn kind_name(kind: GenerationKind) -> &'static str {
    match kind {
        GenerationKind::Image => "image",
        GenerationKind::Video => "video",
        GenerationKind::Music => "music",
    }
}

/// Generation endpoints, their keys, and the generation tools.
#[derive(Clone)]
pub struct Generation {
    store: Store,
    secrets: Arc<dyn SecretStore>,
    attachments: AttachmentStore,
    polling: VideoPolling,
}

impl Generation {
    pub fn new(store: Store, secrets: Arc<dyn SecretStore>, attachments: AttachmentStore) -> Self {
        Self {
            store,
            secrets,
            attachments,
            polling: VideoPolling::default(),
        }
    }

    /// How video jobs are checked (tests make it fast).
    pub fn with_polling(mut self, polling: VideoPolling) -> Self {
        self.polling = polling;
        self
    }

    pub fn attachments(&self) -> &AttachmentStore {
        &self.attachments
    }

    /// The kinds that have a key of their own in the keychain.
    pub fn keys(&self) -> Vec<GenerationKind> {
        GenerationKind::ALL
            .into_iter()
            .filter(|k| matches!(self.secrets.get(&k.secret_name()), Ok(Some(_))))
            .collect()
    }

    /// Sets (or with `None` or an empty key, removes) a kind's own key.
    pub fn set_key(&self, kind: GenerationKind, key: Option<&str>) -> Result<(), String> {
        let name = kind.secret_name();
        match key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(k) => self
                .secrets
                .set(&name, &Secret::new(k))
                .map_err(|e| format!("The key couldn't be saved: {e}")),
            None => self
                .secrets
                .delete(&name)
                .map_err(|e| format!("The key couldn't be removed: {e}")),
        }
    }

    /// The endpoint a kind of generation uses, with its key.
    pub async fn endpoint(
        &self,
        kind: GenerationKind,
        ep: &GenerationEndpoint,
    ) -> Result<Endpoint, String> {
        self.endpoint_keyed(kind, ep, vec![kind.secret_name()])
            .await
    }

    /// Like [`Generation::endpoint`], with the keychain entries to try for an own key, in
    /// order (a Utility App's own key, then the global one).
    pub async fn endpoint_keyed(
        &self,
        kind: GenerationKind,
        ep: &GenerationEndpoint,
        key_names: Vec<String>,
    ) -> Result<Endpoint, String> {
        let what = kind_name(kind);
        let secrets = self.secrets.clone();
        // Keychains block; keep them off the async threads.
        let own_key = tokio::task::spawn_blocking(move || {
            key_names
                .iter()
                .find_map(|name| secrets.get(name).ok().flatten())
        })
        .await
        .ok()
        .flatten()
        .map(|k| k.expose().to_owned());
        let provider = match &ep.provider_id {
            Some(id) => match self.store.get_provider(id).await {
                Ok(p) => Some(p),
                Err(_) => {
                    return Err(format!(
                        "The endpoint “{id}” chosen for {what} generation no longer exists. \
                         Pick another in Settings → Media generation."
                    ));
                }
            },
            None => None,
        };
        let provider_key = match &provider {
            Some(p) if own_key.is_none() => self
                .store
                .provider_key(&self.secrets, &p.id)
                .await
                .ok()
                .flatten()
                .map(|k| k.expose().to_owned()),
            _ => None,
        };
        let key = own_key.or(provider_key);
        if !ep.base_url.is_empty() {
            return Ok(Endpoint::new(
                format!("generation-{what}"),
                ProviderKind::OpenAi,
                &ep.base_url,
                key,
            ));
        }
        match provider {
            Some(p) if p.kind == ProviderKind::Anthropic => Err(format!(
                "{} is an Anthropic endpoint, and Anthropic's API doesn't do {what} generation. \
                 Pick an OpenAI-compatible endpoint in Settings → Media generation.",
                p.name
            )),
            Some(p) => Ok(Endpoint::new(p.id.clone(), p.kind, &p.base_url, key)),
            None => Err(format!(
                "No endpoint is set for {what} generation. Choose one in Settings → Media generation."
            )),
        }
    }

    /// Every model the endpoint lists (for the model picker).
    pub async fn models(
        &self,
        kind: GenerationKind,
        ep: &GenerationEndpoint,
    ) -> Result<Vec<String>, String> {
        let endpoint = self.endpoint(kind, ep).await?;
        gen_api::list_model_ids(&endpoint)
            .await
            .map_err(|e| e.to_string())
    }

    /// Test in Settings → Media generation: reaches the endpoint and looks for the model in its list
    /// (nothing is generated, so it costs nothing).
    pub async fn test(&self, kind: GenerationKind, ep: &GenerationEndpoint) -> GenerationTest {
        let fail = |message: String| GenerationTest { ok: false, message };
        if ep.model.trim().is_empty() {
            return fail("Choose a model first.".into());
        }
        let endpoint = match self.endpoint(kind, ep).await {
            Ok(e) => e,
            Err(e) => return fail(e),
        };
        match gen_api::list_model_ids(&endpoint).await {
            Ok(ids) if ids.iter().any(|m| m == ep.model.trim()) => GenerationTest {
                ok: true,
                message: format!("Connected. {} is offered.", ep.model.trim()),
            },
            Ok(ids) if ids.is_empty() => GenerationTest {
                ok: true,
                message: "Connected. The endpoint lists no models, so the name can't be checked."
                    .into(),
            },
            Ok(ids) => GenerationTest {
                ok: true,
                message: format!(
                    "Connected, but {} isn't among the {} models the endpoint lists. It may \
                     still work; check the name.",
                    ep.model.trim(),
                    ids.len()
                ),
            },
            Err(e) => fail(format!("The test failed: {e}")),
        }
    }

    /// Makes what a generation tool asks for. `references` are images it may build on;
    /// `progress` hears how a video job is going.
    pub async fn run(
        &self,
        kind: GenerationKind,
        input: &Value,
        settings: &GenerationSettings,
        references: Vec<InputImage>,
        progress: &(dyn Fn(ToolProgress) + Send + Sync),
    ) -> ToolOutput {
        let ep = settings.endpoint(kind);
        self.run_on(
            kind,
            input,
            ep,
            vec![kind.secret_name()],
            references,
            progress,
        )
        .await
    }

    /// Runs a generation on a given endpoint (a Utility App's), with the keychain entries to
    /// try for its key.
    pub async fn run_on(
        &self,
        kind: GenerationKind,
        input: &Value,
        ep: &GenerationEndpoint,
        key_names: Vec<String>,
        references: Vec<InputImage>,
        progress: &(dyn Fn(ToolProgress) + Send + Sync),
    ) -> ToolOutput {
        let endpoint = match self.endpoint_keyed(kind, ep, key_names).await {
            Ok(e) => e,
            Err(e) => return ToolOutput::error(e),
        };
        let model = ep.model.trim().to_owned();
        tracing::info!(kind = kind_name(kind), %model, endpoint = %endpoint.id, "generating");
        match kind {
            GenerationKind::Image => self.image(&endpoint, &model, input, references).await,
            GenerationKind::Video => {
                self.video(&endpoint, &model, input, references, progress)
                    .await
            }
            GenerationKind::Music => self.music(&endpoint, &model, input, progress).await,
        }
    }

    async fn image(
        &self,
        endpoint: &Endpoint,
        model: &str,
        input: &Value,
        references: Vec<InputImage>,
    ) -> ToolOutput {
        let Some(prompt) = str_arg(input, "prompt") else {
            return ToolOutput::error("generate_image needs a \"prompt\" describing the image.");
        };
        let n = input
            .get("n")
            .and_then(Value::as_u64)
            .map(|n| u32::try_from(n).unwrap_or(1))
            .unwrap_or(1)
            .clamp(1, MAX_IMAGES_PER_CALL);
        let transparent = input
            .get("transparent_background")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let req = ImageRequest {
            model: model.to_owned(),
            prompt: prompt.clone(),
            n,
            size: str_arg(input, "size"),
            quality: str_arg(input, "quality"),
            background: transparent.then(|| "transparent".to_owned()),
            references,
        };
        let edited = !req.references.is_empty();
        let images = match gen_api::generate_images(endpoint, &req).await {
            Ok(i) => i,
            Err(e) => return ToolOutput::error(format!("The image couldn't be made: {e}")),
        };
        let mut blocks = Vec::new();
        let mut lines = Vec::new();
        for (i, img) in images.into_iter().enumerate() {
            let ext = img
                .media_type
                .rsplit('/')
                .next()
                .unwrap_or("png")
                .to_owned();
            let size = img.bytes.len();
            match self
                .attachments
                .save_generated_image(&format!("generated-image-{}.{ext}", i + 1), img.bytes)
                .await
            {
                Ok(att) => {
                    let dims = match (att.width, att.height) {
                        (Some(w), Some(h)) => format!("{w}×{h}, "),
                        _ => String::new(),
                    };
                    let mut line = format!("Image {}: {dims}{}", i + 1, format_bytes(size as u64));
                    if let Some(note) = &img.note {
                        line.push_str(&format!(". The endpoint drew it as: {note}"));
                    }
                    lines.push(line);
                    blocks.push(att.to_block());
                }
                Err(e) => lines.push(format!("Image {} couldn't be stored: {e}", i + 1)),
            }
        }
        if blocks.is_empty() {
            return ToolOutput::error(format!("The image couldn't be stored. {}", lines.join(" ")));
        }
        let head = format!(
            "{} {} {} with {model}. {} shown to the user in the chat, where they can open and \
             download {}.",
            if edited { "Edited" } else { "Made" },
            blocks.len(),
            if blocks.len() == 1 { "image" } else { "images" },
            if blocks.len() == 1 {
                "It is"
            } else {
                "They are"
            },
            if blocks.len() == 1 { "it" } else { "them" },
        );
        let mut content = vec![ContentBlock::text(format!("{head}\n{}", lines.join("\n")))];
        content.extend(blocks);
        ToolOutput {
            content,
            is_error: false,
            duration_ms: 0,
        }
    }

    async fn video(
        &self,
        endpoint: &Endpoint,
        model: &str,
        input: &Value,
        references: Vec<InputImage>,
        progress: &(dyn Fn(ToolProgress) + Send + Sync),
    ) -> ToolOutput {
        let Some(prompt) = str_arg(input, "prompt") else {
            return ToolOutput::error("generate_video needs a \"prompt\" describing the video.");
        };
        let seconds = input
            .get("seconds")
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && *s > 0.0)
            .map(|s| s.round().clamp(1.0, 120.0) as u32);
        let req = VideoRequest {
            model: model.to_owned(),
            prompt,
            seconds,
            size: str_arg(input, "size"),
            reference: references.into_iter().next(),
        };
        let report = |s: &VideoStatus| {
            let note = match s.state {
                VideoState::Queued => "Queued",
                VideoState::InProgress => "Making the video",
                VideoState::Completed => "Downloading",
                VideoState::Failed => "Failed",
            };
            progress(ToolProgress {
                percent: s.progress,
                note: note.into(),
            });
        };
        let file = match gen_api::generate_video(endpoint, &req, self.polling, report).await {
            Ok(f) => f,
            Err(e) => return ToolOutput::error(format!("The video couldn't be made: {e}")),
        };
        let ext = match file.media_type.as_str() {
            "video/webm" => "webm",
            "video/quicktime" => "mov",
            _ => "mp4",
        };
        let path = match self.attachments.save_media(ext, &file.bytes).await {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(format!("The video couldn't be stored: {e}")),
        };
        let length = seconds.map(|s| format!("{s}-second ")).unwrap_or_default();
        ToolOutput {
            content: vec![
                ContentBlock::text(format!(
                    "Made a {length}video with {model} ({}). It is shown to the user with a \
                     player and a download button; you can't watch it yourself.",
                    format_bytes(file.bytes.len() as u64)
                )),
                ContentBlock::Document {
                    media_type: file.media_type,
                    name: format!("generated-video.{ext}"),
                    source: MediaSource::Path { path },
                },
            ],
            is_error: false,
            duration_ms: 0,
        }
    }

    async fn music(
        &self,
        endpoint: &Endpoint,
        model: &str,
        input: &Value,
        progress: &(dyn Fn(ToolProgress) + Send + Sync),
    ) -> ToolOutput {
        let Some(prompt) = str_arg(input, "style").or_else(|| str_arg(input, "prompt")) else {
            return ToolOutput::error(
                "generate_music needs a \"style\": comma-separated genre and mood tags.",
            );
        };
        let duration = input
            .get("duration_seconds")
            .and_then(Value::as_f64)
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(|d| d.round().clamp(5.0, 240.0) as u32);
        let req = MusicRequest {
            model: model.to_owned(),
            prompt,
            lyrics: str_arg(input, "lyrics"),
            duration,
            seed: input.get("seed").and_then(Value::as_i64),
        };
        // The answer comes in one piece, possibly after a queue: say what is happening.
        progress(ToolProgress {
            percent: None,
            note: "Making the music".into(),
        });
        let file = match gen_api::generate_music(endpoint, &req).await {
            Ok(f) => f,
            Err(e) => return ToolOutput::error(format!("The music couldn't be made: {e}")),
        };
        let ext = match file.media_type.as_str() {
            "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
            "audio/ogg" | "audio/opus" => "ogg",
            "audio/aac" => "aac",
            "audio/flac" => "flac",
            _ => "mp3",
        };
        let path = match self.attachments.save_media(ext, &file.bytes).await {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(format!("The music couldn't be stored: {e}")),
        };
        let kind = if req.lyrics.is_some() {
            "a song"
        } else {
            "an instrumental"
        };
        let length = duration
            .map(|d| format!(" of {d} seconds"))
            .unwrap_or_default();
        ToolOutput {
            content: vec![
                ContentBlock::text(format!(
                    "Made {kind}{length} with {model} ({}). The user gets a player and a \
                     download button; you can't hear it yourself.",
                    format_bytes(file.bytes.len() as u64)
                )),
                ContentBlock::Document {
                    media_type: file.media_type,
                    name: format!("generated-music.{ext}"),
                    source: MediaSource::Path { path },
                },
            ],
            is_error: false,
            duration_ms: 0,
        }
    }
}

fn str_arg(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The tool a kind of generation gives models.
pub fn tool_entry(kind: GenerationKind, settings: &GenerationSettings) -> ToolEntry {
    let model = settings.endpoint(kind).model.trim();
    let (description, schema) = match kind {
        GenerationKind::Image => (
            format!(
                "Make images from a description with {model}. Use it when the user asks for a \
                 picture, illustration, logo, photo or other image. The images are shown to the \
                 user in the chat. Each call costs money: make one image unless asked for more. \
                 To change images that are already in the chat (the user's attachments or images \
                 you made before), set use_attached_images."
            ),
            json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "A detailed description of the image: subject, style, composition, colours, any text it must show."
                    },
                    "size": {
                        "type": "string",
                        "description": "WIDTHxHEIGHT, e.g. 1024x1024 (square), 1536x1024 (landscape), 1024x1536 (portrait). Leave out for the model's default."
                    },
                    "quality": {
                        "type": "string",
                        "enum": ["low", "medium", "high", "auto"],
                        "description": "Higher costs more. Leave out for the default."
                    },
                    "n": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_IMAGES_PER_CALL,
                        "description": "How many images (default 1)."
                    },
                    "transparent_background": {
                        "type": "boolean",
                        "description": "A transparent background (logos, stickers), where the model supports it."
                    },
                    "use_attached_images": {
                        "type": "boolean",
                        "description": "Edit or build on the most recent images in this chat instead of starting from nothing."
                    }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        ),
        GenerationKind::Video => (
            format!(
                "Make a short video from a description with {model}. Use it when the user asks \
                 for a video or animation. It takes from one minute to a quarter of an hour (a \
                 local GPU is slow), and the user sees its progress. Each call costs money or \
                 GPU time, often a lot: make one video per request. To start from an image in the chat, set use_attached_image."
            ),
            json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "What happens in the video: subject, action, setting, camera movement, style, sound."
                    },
                    "seconds": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 60,
                        "description": "Length in seconds, as the user asked. Leave out for the model's default. Local models often make only a few seconds (e.g. 1-5)."
                    },
                    "size": {
                        "type": "string",
                        "description": "WIDTHxHEIGHT, e.g. 1280x720 (landscape) or 720x1280 (portrait). Leave out for the default."
                    },
                    "use_attached_image": {
                        "type": "boolean",
                        "description": "Start the video from the most recent image in this chat."
                    }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        ),
        GenerationKind::Music => (
            format!(
                "Make music (a song with vocals, or an instrumental) with {model}. Use it when \
                 the user asks for a song, a jingle, a melody, background music or a soundtrack. \
                 The user gets an audio player. It takes from a few seconds to several minutes \
                 (the server may be busy). Write the style as comma-separated tags, and for a \
                 song write the lyrics yourself unless the user gave them."
            ),
            json!({
                "type": "object",
                "properties": {
                    "style": {
                        "type": "string",
                        "description": "Comma-separated style tags: genre, mood, instruments, vocal type, tempo. E.g. \"danish indie folk, acoustic guitar, warm male vocals, 90 bpm\"."
                    },
                    "lyrics": {
                        "type": "string",
                        "description": "The words to sing, with section markers on their own lines: [verse], [chorus], [bridge]. Leave out for an instrumental."
                    },
                    "duration_seconds": {
                        "type": "integer",
                        "minimum": 5,
                        "maximum": 240,
                        "description": "Length in seconds (default 30). Fit it to the lyrics."
                    },
                    "seed": {
                        "type": "integer",
                        "description": "Repeat a seed to get the same result again. Usually leave out."
                    }
                },
                "required": ["style"],
                "additionalProperties": false
            }),
        ),
    };
    ToolEntry {
        api_name: kind.tool().to_owned(),
        kind: ToolKind::Builtin(Builtin::Generate(kind)),
        title: None,
        description,
        schema,
        read_only: false,
        destructive: false,
    }
}

/// What a generation call is about, for "Used in this chat".
pub fn label(input: &Value) -> Option<String> {
    let text = str_arg(input, "prompt")
        .or_else(|| str_arg(input, "style"))
        .or_else(|| str_arg(input, "text"))?;
    let line = text.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        out.push('…');
    }
    Some(out)
}

/// The images of a content list (tool results included), newest last.
pub fn images_in(content: &[ContentBlock], out: &mut Vec<(String, MediaSource)>) {
    for b in content {
        match b {
            ContentBlock::Image { media_type, source } => {
                out.push((media_type.clone(), source.clone()));
            }
            ContentBlock::ToolResult { content, .. } => images_in(content, out),
            _ => {}
        }
    }
}

/// Reads images for a request (files must be in the attachments folder).
pub async fn load_references(
    attachments: &AttachmentStore,
    images: Vec<(String, MediaSource)>,
) -> Vec<InputImage> {
    use base64::Engine;
    let mut out = Vec::new();
    for (i, (media_type, source)) in images.into_iter().enumerate() {
        let bytes = match &source {
            MediaSource::Path { path } => attachments.read_file(path).await.ok(),
            MediaSource::Base64 { data } => base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .ok(),
        };
        let Some(bytes) = bytes else { continue };
        let media_type = gen_api::sniff_image(&bytes)
            .map(str::to_owned)
            .unwrap_or(media_type);
        let ext = media_type.rsplit('/').next().unwrap_or("png").to_owned();
        out.push(InputImage {
            name: format!("reference-{}.{ext}", i + 1),
            media_type,
            bytes,
        });
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use oxide_core::ProviderInput;
    use oxide_store::MemorySecrets;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub(crate) const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    /// Serves canned (status, content type, body) replies in order; records request heads.
    pub(crate) async fn serve(
        replies: Vec<(&'static str, &'static str, Vec<u8>)>,
    ) -> (String, Arc<tokio::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let addr = listener.local_addr().expect("addr");
        let seen = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            for (status, ct, body) in replies {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..p]).into_owned();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        while buf.len() < p + 4 + len {
                            let n = sock.read(&mut chunk).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&chunk[..n]);
                        }
                        log.lock().await.push(head);
                        break;
                    }
                }
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: {ct}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                sock.write_all(head.as_bytes()).await.ok();
                sock.write_all(&body).await.ok();
                sock.shutdown().await.ok();
            }
        });
        (format!("http://{addr}/v1"), seen)
    }

    async fn fixture() -> (Generation, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let g = Generation::new(store, secrets, AttachmentStore::new(dir.path().join("att")))
            .with_polling(VideoPolling {
                interval: Duration::from_millis(5),
                max_wait: Duration::from_secs(5),
            });
        (g, dir)
    }

    fn custom(base: &str, model: &str) -> GenerationEndpoint {
        GenerationEndpoint {
            provider_id: None,
            base_url: base.into(),
            model: model.into(),
        }
    }

    #[tokio::test]
    async fn endpoints_resolve_from_providers_urls_and_own_keys() {
        let (g, _dir) = fixture().await;
        let none = GenerationEndpoint::default();
        let err = g
            .endpoint(GenerationKind::Image, &none)
            .await
            .expect_err("unset");
        assert!(
            err.contains("No endpoint is set for image generation"),
            "{err}"
        );

        let p = g
            .store
            .save_provider(ProviderInput {
                id: None,
                name: "OpenAI".into(),
                kind: ProviderKind::OpenAi,
                base_url: "https://api.openai.com/v1".into(),
                enabled: true,
            })
            .await
            .expect("provider");
        g.store
            .set_provider_key(&g.secrets, &p.id, Secret::new("sk-provider"))
            .await
            .expect("key");
        let via_provider = GenerationEndpoint {
            provider_id: Some(p.id.clone()),
            base_url: String::new(),
            model: "gpt-image-2".into(),
        };
        let e = g
            .endpoint(GenerationKind::Image, &via_provider)
            .await
            .expect("endpoint");
        assert_eq!(e.base_url, "https://api.openai.com/v1");
        assert!(e.has_key());

        // A custom URL wins; with no key of its own it borrows the provider's.
        let both = GenerationEndpoint {
            base_url: "http://127.0.0.1:9/v1".into(),
            ..via_provider.clone()
        };
        let e = g
            .endpoint(GenerationKind::Image, &both)
            .await
            .expect("custom");
        assert_eq!(e.base_url, "http://127.0.0.1:9/v1");
        assert_eq!(e.kind, ProviderKind::OpenAi);
        assert!(e.has_key());

        g.set_key(GenerationKind::Video, Some(" sk-own "))
            .expect("set");
        assert_eq!(g.keys(), vec![GenerationKind::Video]);
        g.set_key(GenerationKind::Video, None).expect("removes");
        assert!(g.keys().is_empty());

        let a = g
            .store
            .save_provider(ProviderInput {
                id: None,
                name: "Anthropic".into(),
                kind: ProviderKind::Anthropic,
                base_url: "https://api.anthropic.com/v1".into(),
                enabled: true,
            })
            .await
            .expect("anthropic");
        let err = g
            .endpoint(
                GenerationKind::Music,
                &GenerationEndpoint {
                    provider_id: Some(a.id),
                    base_url: String::new(),
                    model: "m".into(),
                },
            )
            .await
            .expect_err("anthropic");
        assert!(
            err.contains("Anthropic's API doesn't do music generation"),
            "{err}"
        );
        let err = g
            .endpoint(
                GenerationKind::Music,
                &GenerationEndpoint {
                    provider_id: Some("gone".into()),
                    base_url: String::new(),
                    model: "m".into(),
                },
            )
            .await
            .expect_err("gone");
        assert!(err.contains("no longer exists"), "{err}");
    }

    #[tokio::test]
    async fn test_checks_the_model_list_without_generating() {
        let (g, _dir) = fixture().await;
        let (base, seen) = serve(vec![
            (
                "200 OK",
                "application/json",
                br#"{"data":[{"id":"gpt-image-2"},{"id":"tts-1"}]}"#.to_vec(),
            ),
            (
                "200 OK",
                "application/json",
                br#"{"data":[{"id":"tts-1"}]}"#.to_vec(),
            ),
            ("404 Not Found", "text/plain", b"nope".to_vec()),
            (
                "401 Unauthorized",
                "application/json",
                br#"{"error":{"message":"bad key"}}"#.to_vec(),
            ),
        ])
        .await;
        g.set_key(GenerationKind::Image, Some("sk-own"))
            .expect("key");
        let t = g
            .test(GenerationKind::Image, &custom(&base, "gpt-image-2"))
            .await;
        assert!(
            t.ok && t.message.contains("gpt-image-2 is offered"),
            "{t:?}"
        );
        let t = g.test(GenerationKind::Image, &custom(&base, "other")).await;
        assert!(
            t.ok && t.message.contains("isn't among the 1 models"),
            "{t:?}"
        );
        // No model list (comfy-bridge): reachable, but the name can't be checked.
        let t = g.test(GenerationKind::Image, &custom(&base, "x")).await;
        assert!(t.ok && t.message.contains("lists no models"), "{t:?}");
        let t = g.test(GenerationKind::Image, &custom(&base, "x")).await;
        assert!(!t.ok && t.message.contains("key was rejected"), "{t:?}");
        let t = g.test(GenerationKind::Image, &custom(&base, " ")).await;
        assert!(!t.ok && t.message.contains("model"), "{t:?}");
        let heads = seen.lock().await.clone();
        assert!(heads.iter().all(|h| h.starts_with("GET /v1/models")));
        assert!(heads[0].to_ascii_lowercase().contains("bearer sk-own"));
    }

    #[tokio::test]
    async fn the_three_tools_store_their_results() {
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD
            .decode(PNG_1X1)
            .expect("png");
        let (g, _dir) = fixture().await;
        let (base, seen) = serve(vec![
            (
                "200 OK",
                "application/json",
                format!(r#"{{"data":[{{"b64_json":"{PNG_1X1}"}}]}}"#).into_bytes(),
            ),
            (
                "200 OK",
                "application/json",
                // `SUQzLW1wMw==` is "ID3-mp3".
                br#"   {"data":[{"format":"mp3","b64_json":"SUQzLW1wMw=="}]}"#.to_vec(),
            ),
            (
                "200 OK",
                "application/json",
                br#"{"id":"video_1","status":"queued","progress":0}"#.to_vec(),
            ),
            (
                "200 OK",
                "application/json",
                br#"{"id":"video_1","status":"completed","progress":100}"#.to_vec(),
            ),
            ("200 OK", "video/mp4", b"mp4-bytes".to_vec()),
        ])
        .await;
        let s = GenerationSettings {
            image: custom(&base, "gpt-image-2"),
            music: custom(&base, "ace-step"),
            video: custom(&base, "sora-2"),
        };
        let ticks = std::sync::Mutex::new(Vec::new());
        let progress = |p: ToolProgress| ticks.lock().expect("lock").push(p);

        let out = g
            .run(
                GenerationKind::Image,
                &json!({"prompt": "a fox"}),
                &s,
                Vec::new(),
                &progress,
            )
            .await;
        assert!(!out.is_error, "{out:?}");
        let ContentBlock::Image { source, .. } = &out.content[1] else {
            panic!("an image block: {out:?}");
        };
        let MediaSource::Path { path } = source else {
            panic!("stored on disk");
        };
        assert_eq!(g.attachments.read_file(path).await.expect("reads"), png);

        let out = g
            .run(
                GenerationKind::Music,
                &json!({"style": "lo-fi, piano", "lyrics": "[verse]\nHello", "duration_seconds": 20}),
                &s,
                Vec::new(),
                &progress,
            )
            .await;
        assert!(!out.is_error, "{out:?}");
        assert!(matches!(
            &out.content[1],
            ContentBlock::Document { media_type, name, .. }
                if media_type == "audio/mpeg" && name.ends_with(".mp3")
        ));

        let out = g
            .run(
                GenerationKind::Video,
                &json!({"prompt": "waves", "seconds": 5}),
                &s,
                Vec::new(),
                &progress,
            )
            .await;
        assert!(!out.is_error, "{out:?}");
        let ContentBlock::Document {
            media_type,
            source: MediaSource::Path { path },
            ..
        } = &out.content[1]
        else {
            panic!("a video document: {out:?}");
        };
        assert_eq!(media_type, "video/mp4");
        assert_eq!(
            g.attachments.read_file(path).await.expect("reads"),
            b"mp4-bytes"
        );
        assert!(
            out.content[0]
                == ContentBlock::text(
                    "Made a 5-second video with sora-2 (9 B). It is shown to the user with a player \
             and a download button; you can't watch it yourself."
                )
        );
        let ticks = ticks.lock().expect("lock").clone();
        let notes: Vec<&str> = ticks.iter().map(|t| t.note.as_str()).collect();
        assert_eq!(notes.first(), Some(&"Making the music"));
        assert_eq!(notes.get(1), Some(&"Queued"));
        assert_eq!(ticks.last().and_then(|t| t.percent), Some(100));
        let heads = seen.lock().await.clone();
        assert!(heads[0].starts_with("POST /v1/images/generations"));
        assert!(heads[1].starts_with("POST /v1/music/generations"));
        assert!(heads[2].starts_with("POST /v1/videos "));
    }

    #[tokio::test]
    async fn failures_are_tool_errors() {
        let (g, _dir) = fixture().await;
        let (base, _) = serve(vec![
            ("404 Not Found", "text/plain", b"Not Found".to_vec()),
            ("404 Not Found", "text/plain", b"Not Found".to_vec()),
        ])
        .await;
        let s = GenerationSettings {
            video: custom(&base, "v"),
            ..GenerationSettings::default()
        };
        let out = g
            .run(
                GenerationKind::Video,
                &json!({"prompt": "x"}),
                &s,
                Vec::new(),
                &|_| {},
            )
            .await;
        assert!(out.is_error);
        assert_eq!(
            out.content,
            vec![ContentBlock::text(
                "The video couldn't be made: this endpoint doesn't offer video generation (/videos or /videos/generations)"
            )]
        );
        let out = g
            .run(GenerationKind::Image, &json!({}), &s, Vec::new(), &|_| {})
            .await;
        assert!(out.is_error);
    }

    #[test]
    fn entries_and_labels() {
        let s = GenerationSettings::default();
        for k in [
            GenerationKind::Image,
            GenerationKind::Video,
            GenerationKind::Music,
        ] {
            let e = tool_entry(k, &s);
            assert_eq!(e.api_name, k.tool());
            assert_eq!(e.kind, ToolKind::Builtin(Builtin::Generate(k)));
            assert!(e.schema["required"].is_array());
        }
        assert_eq!(
            label(&json!({"prompt": "A fox\nin snow"})).as_deref(),
            Some("A fox")
        );
        assert_eq!(
            label(&json!({"text": "x".repeat(90)})).map(|l| l.chars().count()),
            Some(81)
        );
    }
}
