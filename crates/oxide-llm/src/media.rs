//! Reading attachment bytes for requests.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use oxide_core::MediaSource;

use crate::LlmError;

/// The base64 data of an attachment (files on disk are read and encoded).
pub(crate) async fn base64_of(source: &MediaSource) -> Result<String, LlmError> {
    match source {
        MediaSource::Base64 { data } => Ok(data.clone()),
        MediaSource::Path { path } => {
            let bytes = tokio::fs::read(path)
                .await
                .map_err(|e| LlmError::Invalid(format!("couldn't read the attachment: {e}")))?;
            Ok(STANDARD.encode(bytes))
        }
    }
}

/// The text of a text attachment (invalid UTF-8 is replaced).
pub(crate) async fn text_of(source: &MediaSource) -> Result<String, LlmError> {
    let bytes = match source {
        MediaSource::Base64 { data } => STANDARD
            .decode(data.trim())
            .map_err(|e| LlmError::Invalid(format!("attachment isn't valid base64: {e}")))?,
        MediaSource::Path { path } => tokio::fs::read(path)
            .await
            .map_err(|e| LlmError::Invalid(format!("couldn't read the attachment: {e}")))?,
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Whether a document's media type is plain text that can be inlined.
pub(crate) fn is_text(media_type: &str) -> bool {
    media_type.starts_with("text/")
        || matches!(
            media_type,
            "application/json" | "application/xml" | "application/x-yaml" | "application/toml"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_attachments_decode() {
        let src = MediaSource::Base64 {
            data: STANDARD.encode("h\u{e6}j"),
        };
        assert_eq!(text_of(&src).await.expect("decodes"), "h\u{e6}j");
        assert!(is_text("text/markdown"));
        assert!(!is_text("application/pdf"));
    }

    #[tokio::test]
    async fn files_are_read_and_encoded() {
        let dir = std::env::temp_dir().join(format!("oxide-llm-media-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.expect("dir");
        let path = dir.join("a.bin");
        tokio::fs::write(&path, [1u8, 2, 3]).await.expect("writes");
        let src = MediaSource::Path {
            path: path.to_string_lossy().into_owned(),
        };
        assert_eq!(base64_of(&src).await.expect("reads"), "AQID");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
