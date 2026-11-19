//! Speech to text for dictation (Phase 18): OpenAI-compatible `POST /audio/transcriptions`
//! (OpenAI, Groq, LocalAI, a local whisper server …) with a multipart body built here.

use std::time::Duration;

use oxide_core::ProviderKind;
use serde_json::Value;

use crate::Endpoint;
use crate::LlmError;
use crate::http::{USER_AGENT, describe, status_error};

/// Largest recording sent (OpenAI's limit is 25 MB).
pub const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

/// One recording to transcribe.
pub struct Recording<'a> {
    pub bytes: &'a [u8],
    /// `audio/webm`, `audio/ogg`, `audio/wav`, `audio/mp4`, … (parameters like `;codecs=opus`
    /// are fine).
    pub mime: &'a str,
}

fn file_name(mime: &str) -> &'static str {
    let base = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "audio/ogg" | "audio/opus" => "speech.ogg",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "speech.wav",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "audio/aac" => "speech.m4a",
        "audio/mpeg" | "audio/mp3" => "speech.mp3",
        "audio/flac" => "speech.flac",
        _ => "speech.webm",
    }
}

/// Builds a `multipart/form-data` body. Returns (content type, body).
fn multipart(fields: &[(&str, &str)], recording: &Recording<'_>) -> (String, Vec<u8>) {
    let boundary = format!("oxide-{:016x}", rand_u64());
    let mut body = Vec::with_capacity(recording.bytes.len() + 512);
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    let mime: String = recording
        .mime
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            file_name(&mime),
            if mime.is_empty() { "application/octet-stream" } else { &mime }
        )
        .as_bytes(),
    );
    body.extend_from_slice(recording.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

/// Turns a recording into text with `model` on `endpoint`. `language` is an ISO-639-1 hint.
pub async fn transcribe(
    endpoint: &Endpoint,
    model: &str,
    recording: Recording<'_>,
    language: Option<&str>,
) -> Result<String, LlmError> {
    if endpoint.kind == ProviderKind::Anthropic {
        return Err(LlmError::Invalid(
            "Anthropic's API doesn't turn speech into text; pick an OpenAI-compatible endpoint \
             in Settings → Voice"
                .into(),
        ));
    }
    if recording.bytes.is_empty() {
        return Err(LlmError::Invalid("the recording is empty".into()));
    }
    if recording.bytes.len() > MAX_AUDIO_BYTES {
        return Err(LlmError::Invalid(
            "the recording is longer than the 25 MB a transcription takes".into(),
        ));
    }
    let model = model.trim();
    let mut fields = vec![("model", model), ("response_format", "json")];
    if let Some(lang) = language.map(str::trim).filter(|l| !l.is_empty()) {
        fields.push(("language", lang));
    }
    let (content_type, body) = multipart(&fields, &recording);
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| LlmError::Invalid(format!("HTTP client: {e}")))?;
    let url = format!("{}/audio/transcriptions", endpoint.base_url);
    let mut req = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body);
    match endpoint.api_key() {
        Some(k) => req = req.bearer_auth(k),
        None if endpoint.kind == ProviderKind::OpenAi => return Err(LlmError::MissingKey),
        None => {}
    }
    let res = req.send().await.map_err(|e| LlmError::Unreachable {
        url: endpoint.base_url.clone(),
        reason: describe(&e),
    })?;
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    if status == 404 || status == 405 {
        return Err(LlmError::Invalid(
            "this endpoint doesn't offer speech-to-text (/audio/transcriptions)".into(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(status_error(status, &text));
    }
    // `json` gives {"text": …}; some servers answer plain text anyway.
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => v
            .get("text")
            .and_then(Value::as_str)
            .map(|t| t.trim().to_owned())
            .ok_or_else(|| LlmError::Decode("no text in the transcription".into())),
        Err(_) => Ok(text.trim().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Reply, serve};

    #[tokio::test]
    async fn sends_a_multipart_request_and_reads_the_text() {
        let server = serve(vec![Reply::json(r#"{"text": " Hej med dig "}"#)]).await;
        let endpoint = Endpoint::new(
            "groq",
            ProviderKind::OpenAi,
            &server.base,
            Some("gsk_x".into()),
        );
        let audio = b"\x1aE\xdf\xa3fake-webm".to_vec();
        let text = transcribe(
            &endpoint,
            "whisper-large-v3-turbo",
            Recording {
                bytes: &audio,
                mime: "audio/webm;codecs=opus",
            },
            Some("da"),
        )
        .await
        .expect("text");
        assert_eq!(text, "Hej med dig");
        let req = &server.requests().await[0];
        assert!(
            req.head.starts_with("POST /v1/audio/transcriptions"),
            "{}",
            req.head
        );
        assert!(
            req.head
                .to_ascii_lowercase()
                .contains("authorization: bearer gsk_x")
        );
        assert!(req.head.contains("multipart/form-data; boundary=oxide-"));
        assert!(
            req.body
                .contains("name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n")
        );
        assert!(req.body.contains("name=\"language\"\r\n\r\nda\r\n"));
        assert!(
            req.body
                .contains("filename=\"speech.webm\"\r\nContent-Type: audio/webm;codecs=opus")
        );
    }

    #[tokio::test]
    async fn missing_endpoints_and_errors_are_explained() {
        let server = serve(vec![
            Reply::status("404 Not Found", "{}"),
            Reply::status("401 Unauthorized", r#"{"error":{"message":"bad key"}}"#),
        ])
        .await;
        let endpoint = Endpoint::new("local", ProviderKind::LmStudio, &server.base, None);
        let rec = || Recording {
            bytes: b"x",
            mime: "audio/wav",
        };
        let err = transcribe(&endpoint, "whisper-1", rec(), None)
            .await
            .expect_err("404");
        assert!(
            err.to_string().contains("doesn't offer speech-to-text"),
            "{err}"
        );
        let err = transcribe(&endpoint, "whisper-1", rec(), None)
            .await
            .expect_err("401");
        assert!(err.to_string().to_lowercase().contains("key"), "{err}");

        let anthropic = Endpoint::new(
            "a",
            ProviderKind::Anthropic,
            "https://x/v1",
            Some("k".into()),
        );
        assert!(transcribe(&anthropic, "m", rec(), None).await.is_err());
        let empty = Recording {
            bytes: b"",
            mime: "audio/wav",
        };
        assert!(transcribe(&endpoint, "m", empty, None).await.is_err());
    }
}
