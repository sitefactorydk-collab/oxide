//! Reading attachment bytes for requests, and PDF text extraction for models that can't
//! read PDFs natively.

use std::panic::AssertUnwindSafe;

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
    let bytes = bytes_of(source).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The raw bytes of an attachment.
async fn bytes_of(source: &MediaSource) -> Result<Vec<u8>, LlmError> {
    match source {
        MediaSource::Base64 { data } => STANDARD
            .decode(data.trim())
            .map_err(|e| LlmError::Invalid(format!("attachment isn't valid base64: {e}"))),
        MediaSource::Path { path } => tokio::fs::read(path)
            .await
            .map_err(|e| LlmError::Invalid(format!("couldn't read the attachment: {e}"))),
    }
}

/// File next to a stored PDF that holds its extracted text (written when it was attached).
pub fn pdf_text_sidecar(pdf_path: &str) -> String {
    format!("{pdf_path}.txt")
}

/// Text of a PDF, one string per page. Runs on a blocking thread; a parser panic on a
/// malformed file becomes an error instead of taking the app down.
pub async fn pdf_pages(bytes: Vec<u8>) -> Result<Vec<String>, LlmError> {
    let job = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem_by_pages(&bytes)
        }))
    });
    match job.await {
        Ok(Ok(Ok(pages))) => Ok(pages),
        Ok(Ok(Err(e))) => Err(LlmError::Invalid(format!("couldn't read the PDF: {e}"))),
        Ok(Err(_)) | Err(_) => Err(LlmError::Invalid(
            "couldn't read the PDF (it may be damaged or encrypted)".into(),
        )),
    }
}

/// Joins page texts with page markers, so the model can cite pages.
pub fn join_pages(pages: &[String]) -> String {
    let mut out = String::new();
    for (i, page) in pages.iter().enumerate() {
        let text = page.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format!("[Page {}]\n{text}", i + 1));
    }
    out
}

/// The text of a PDF attachment: the sidecar written when it was attached, or extracted now.
pub(crate) async fn pdf_text_of(source: &MediaSource) -> Result<String, LlmError> {
    if let MediaSource::Path { path } = source
        && let Ok(text) = tokio::fs::read_to_string(pdf_text_sidecar(path)).await
    {
        return Ok(text);
    }
    let pages = pdf_pages(bytes_of(source).await?).await?;
    Ok(join_pages(&pages))
}

/// The prompt text for a PDF given to a model that can't read PDFs.
pub(crate) async fn pdf_as_text(name: &str, source: &MediaSource) -> String {
    match pdf_text_of(source).await {
        Ok(text) if !text.trim().is_empty() => {
            format!("<file name=\"{name}\" type=\"pdf, extracted text\">\n{text}\n</file>")
        }
        Ok(_) => format!(
            "[Attached PDF {name} has no extractable text (it may be scanned images). \
             This model can't read it.]"
        ),
        Err(e) => format!("[Attached PDF {name} couldn't be read: {e}]"),
    }
}

/// A minimal valid PDF with one line of Helvetica text per page (`text` must not contain
/// parentheses or backslashes). Used by tests and the demo conversation.
pub fn sample_pdf(pages: &[&str]) -> Vec<u8> {
    let n = pages.len();
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n)
                .map(|i| format!("{} 0 R", 4 + 2 * i))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    for (i, text) in pages.iter().enumerate() {
        let stream = format!("BT /F1 18 Tf 72 720 Td ({text}) Tj ET");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for o in offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

/// Note used instead of an image for models that can't see images.
pub(crate) const IMAGE_OMITTED: &str =
    "[An image was attached here, but this model can't see images.]";

/// Whether a document's media type is plain text that can be inlined.
pub fn is_text(media_type: &str) -> bool {
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

    #[test]
    fn pages_are_joined_with_markers() {
        let pages = vec!["one".to_owned(), "  ".to_owned(), "three\n".to_owned()];
        assert_eq!(join_pages(&pages), "[Page 1]\none\n\n[Page 3]\nthree");
    }

    #[tokio::test]
    async fn broken_pdfs_are_errors_not_panics() {
        assert!(pdf_pages(b"%PDF-1.4 garbage".to_vec()).await.is_err());
        assert!(pdf_pages(Vec::new()).await.is_err());
    }

    #[tokio::test]
    async fn pdf_text_prefers_the_sidecar() {
        let dir = std::env::temp_dir().join(format!("oxide-llm-pdf-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.expect("dir");
        let pdf = dir.join("doc.pdf");
        tokio::fs::write(&pdf, b"not really a pdf")
            .await
            .expect("writes");
        let path = pdf.to_string_lossy().into_owned();
        tokio::fs::write(pdf_text_sidecar(&path), "[Page 1]\nhello")
            .await
            .expect("writes");
        let text = pdf_as_text("doc.pdf", &MediaSource::Path { path }).await;
        assert!(text.contains("[Page 1]\nhello") && text.contains("doc.pdf"));
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn pdf_text_is_extracted_per_page() {
        let pages = pdf_pages(sample_pdf(&["Hello Oxide", "Second page"]))
            .await
            .expect("extracts");
        assert_eq!(pages.len(), 2);
        assert!(pages[0].contains("Hello Oxide"), "{pages:?}");
        assert!(pages[1].contains("Second page"), "{pages:?}");
    }
}
