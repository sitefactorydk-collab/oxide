//! Media generation endpoints (Phase 19): the global defaults for image, video and audio
//! generation in Settings → Media generation, and the chat tools that use them. Text comes
//! from the chat models in Settings → Providers and Models.
//!
//! Every generation endpoint speaks the OpenAI-compatible API. An endpoint is either one of
//! the endpoints in Settings → Providers, or a custom base URL; a key kept in the keychain
//! under [`GenerationKind::secret_name`] overrides the endpoint's own key.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The chat tool that makes images.
pub const GENERATE_IMAGE: &str = "generate_image";
/// The chat tool that makes videos.
pub const GENERATE_VIDEO: &str = "generate_video";
/// The chat tool that reads text aloud.
pub const GENERATE_AUDIO: &str = "generate_audio";
/// The "+" menu switch for all three generation tools (a `disabledBuiltins` entry).
pub const GENERATION_SWITCH: &str = "generation";

/// Default image model (OpenAI's `gpt-image-1` was shut down on 2026-10-23).
pub const DEFAULT_IMAGE_MODEL: &str = "gpt-image-2";
/// Default speech model.
pub const DEFAULT_AUDIO_MODEL: &str = "gpt-4o-mini-tts";
/// Default voice for speech.
pub const DEFAULT_VOICE: &str = "alloy";
/// Audio formats a speech endpoint is asked for (all play in the app).
pub const AUDIO_FORMATS: [&str; 5] = ["mp3", "wav", "opus", "aac", "flac"];

/// A kind of media generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GenerationKind {
    Image,
    Video,
    Audio,
}

impl GenerationKind {
    pub const ALL: [GenerationKind; 3] = [
        GenerationKind::Image,
        GenerationKind::Video,
        GenerationKind::Audio,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GenerationKind::Image => "image",
            GenerationKind::Video => "video",
            GenerationKind::Audio => "audio",
        }
    }

    /// The keychain entry of the key that overrides the endpoint's own.
    pub fn secret_name(self) -> String {
        format!("generation:{}", self.as_str())
    }

    /// The chat tool that uses this kind.
    pub fn tool(self) -> &'static str {
        match self {
            GenerationKind::Image => GENERATE_IMAGE,
            GenerationKind::Video => GENERATE_VIDEO,
            GenerationKind::Audio => GENERATE_AUDIO,
        }
    }

    /// The kind a generation tool makes.
    pub fn of_tool(tool: &str) -> Option<GenerationKind> {
        match tool {
            GENERATE_IMAGE => Some(GenerationKind::Image),
            GENERATE_VIDEO => Some(GenerationKind::Video),
            GENERATE_AUDIO => Some(GenerationKind::Audio),
            _ => None,
        }
    }
}

/// Where one kind of generation goes: a model on an endpoint from Settings → Providers, or on
/// a custom base URL (which wins when both are set).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct GenerationEndpoint {
    /// An endpoint from Settings → Providers (its URL and key are used).
    pub provider_id: Option<String>,
    /// A custom OpenAI-compatible base URL (`https://…/v1`); empty uses the provider's.
    pub base_url: String,
    /// The model id on that endpoint.
    pub model: String,
}

impl GenerationEndpoint {
    fn with_model(model: &str) -> Self {
        Self {
            model: model.into(),
            ..Self::default()
        }
    }

    /// Has an endpoint (provider or URL) and a model.
    pub fn is_set(&self) -> bool {
        !self.model.is_empty() && (self.provider_id.is_some() || !self.base_url.is_empty())
    }

    pub fn normalized(mut self) -> Self {
        self.provider_id = self
            .provider_id
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty() && !p.contains('/'));
        let url = crate::provider::normalize_base_url(&self.base_url);
        self.base_url = if (url.starts_with("http://") || url.starts_with("https://"))
            && !url.chars().any(char::is_whitespace)
            && url.len() <= 500
        {
            url
        } else {
            String::new()
        };
        self.model = self.model.trim().chars().take(200).collect();
        self
    }
}

/// Settings → Media generation. (A `text` entry saved before it was dropped is ignored.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct GenerationSettings {
    pub image: GenerationEndpoint,
    pub video: GenerationEndpoint,
    pub audio: GenerationEndpoint,
    /// The voice speech uses unless the model picks another (`alloy`, `nova`, …).
    pub voice: String,
    /// The audio format speech is made in (one of [`AUDIO_FORMATS`]).
    pub audio_format: String,
}

impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            image: GenerationEndpoint::with_model(DEFAULT_IMAGE_MODEL),
            video: GenerationEndpoint::default(),
            audio: GenerationEndpoint::with_model(DEFAULT_AUDIO_MODEL),
            voice: DEFAULT_VOICE.into(),
            audio_format: "mp3".into(),
        }
    }
}

impl GenerationSettings {
    pub fn endpoint(&self, kind: GenerationKind) -> &GenerationEndpoint {
        match kind {
            GenerationKind::Image => &self.image,
            GenerationKind::Video => &self.video,
            GenerationKind::Audio => &self.audio,
        }
    }

    pub fn normalized(mut self) -> Self {
        self.image = self.image.normalized();
        self.video = self.video.normalized();
        self.audio = self.audio.normalized();
        let voice: String = self.voice.trim().chars().take(64).collect();
        self.voice = if voice.is_empty() {
            DEFAULT_VOICE.into()
        } else {
            voice
        };
        let format = self.audio_format.trim().to_ascii_lowercase();
        self.audio_format = if AUDIO_FORMATS.contains(&format.as_str()) {
            format
        } else {
            "mp3".into()
        };
        self
    }
}

/// The result of Test in Settings → Media generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerationTest {
    pub ok: bool,
    pub message: String,
}

/// Progress of a long tool call (a video being made), shown on its line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    /// 0–100, when the endpoint says.
    pub percent: Option<u32>,
    /// A short status such as "Queued" or "Rendering".
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_cleaning() {
        let s = GenerationSettings::default();
        assert_eq!(s.image.model, DEFAULT_IMAGE_MODEL);
        assert!(!s.image.is_set());
        let mut s = s;
        s.image.provider_id = Some(" openai ".into());
        s.video.base_url = " https://api.example.com/v1/ ".into();
        s.video.model = " sora-2 ".into();
        s.audio.base_url = "ftp://nope".into();
        s.voice = "  ".into();
        s.audio_format = "PCM".into();
        let s = s.normalized();
        assert_eq!(s.image.provider_id.as_deref(), Some("openai"));
        assert!(s.image.is_set());
        assert_eq!(s.video.base_url, "https://api.example.com/v1");
        assert_eq!(s.video.model, "sora-2");
        assert!(s.video.is_set());
        assert_eq!(s.audio.base_url, "");
        assert_eq!(s.voice, DEFAULT_VOICE);
        assert_eq!(s.audio_format, "mp3");
    }

    #[test]
    fn settings_round_trip_and_fill_in_missing_fields() {
        let s: GenerationSettings =
            // `text` was saved by the first Phase 19 build; it is ignored now.
            serde_json::from_str(
                r#"{"text":{"model":"t"},"video":{"baseUrl":"http://x/v1","model":"v"}}"#,
            )
                .expect("parses");
        assert_eq!(s.video.model, "v");
        assert_eq!(s.audio.model, DEFAULT_AUDIO_MODEL);
        let json = serde_json::to_string(&s).expect("serializes");
        let back: GenerationSettings = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, s);
    }

    #[test]
    fn kinds_name_their_tools_and_keys() {
        for k in GenerationKind::ALL {
            assert_eq!(GenerationKind::of_tool(k.tool()), Some(k));
        }
        assert_eq!(GenerationKind::Image.secret_name(), "generation:image");
        assert_eq!(
            serde_json::to_string(&GenerationKind::Audio).expect("json"),
            "\"audio\""
        );
    }
}
