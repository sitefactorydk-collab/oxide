//! Preparing files the user attaches.
//!
//! Each file is checked against [`ATTACHMENT_LIMITS`] and copied into the attachments folder
//! (inside the app's data folder) under a new id:
//! - Images are scaled to fit [`AttachmentLimits::max_image_side`] and re-encoded (JPEG, or
//!   PNG when they have transparency) unless they're already small enough. BMPs are always
//!   converted, since the model APIs don't take them.
//! - PDFs are checked for size and page count, and their text is extracted once into a
//!   `<file>.pdf.txt` sidecar for models that can't read PDFs natively.
//! - Text and code files must be UTF-8 (a BOM is dropped; invalid bytes are replaced).
//!
//! Messages store the copy's path; [`AttachmentStore::blocks`] only accepts paths inside the
//! folder, so the UI can't make Oxide send arbitrary files.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};
use oxide_core::attachment::{TEXT_EXTENSIONS, text_media_type};
use oxide_core::{
    ATTACHMENT_LIMITS, Attachment, AttachmentKind, AttachmentLimits, AttachmentSource, ContentBlock,
};
use oxide_llm::media::{join_pages, pdf_pages, pdf_text_sidecar};
use oxide_store::new_id;

/// Images at most this large (and within the side limit) are stored unchanged.
const KEEP_ORIGINAL_BYTES: usize = 3 * 1024 * 1024;
/// The model APIs reject images above 5 MB.
const MAX_STORED_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const JPEG_QUALITY: u8 = 85;
/// Side of the composer chip preview, in pixels.
const THUMBNAIL_SIDE: u32 = 160;

/// Why a file can't be attached. The messages name the file and can be shown as they are.
#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("{name} is too large ({size}). The limit for {what} is {limit}.")]
    TooLarge {
        name: String,
        what: &'static str,
        size: String,
        limit: String,
    },
    #[error("{name} has {pages} pages. PDFs can have at most {limit}.")]
    TooManyPages {
        name: String,
        pages: u32,
        limit: u32,
    },
    #[error("{name} isn't a file type Oxide can read. Attach images, PDFs, text or code.")]
    Unsupported { name: String },
    #[error("{name} couldn't be read: {reason}")]
    Unreadable { name: String, reason: String },
    #[error("at most {0} files can be sent with one message")]
    TooMany(u32),
    #[error("an attachment isn't in Oxide's attachments folder")]
    Foreign,
}

type Result<T> = std::result::Result<T, AttachmentError>;

/// Human-readable size: "820 KB", "4.2 MB".
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{:.1} MB", b / KB / KB)
    }
}

/// The attachments folder.
#[derive(Debug, Clone)]
pub struct AttachmentStore {
    dir: PathBuf,
    limits: AttachmentLimits,
}

impl AttachmentStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            limits: ATTACHMENT_LIMITS,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn limits(&self) -> AttachmentLimits {
        self.limits
    }

    /// Checks, converts and stores one file.
    pub async fn prepare(&self, source: AttachmentSource) -> Result<Attachment> {
        let (name, bytes) = self.read_source(source).await?;
        let original_size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let id = new_id();
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| unreadable(&name, e))?;

        let ext = extension(&name);
        let kind = detect(&bytes, &ext)
            .ok_or_else(|| AttachmentError::Unsupported { name: name.clone() })?;
        let mut attachment = Attachment {
            id: id.clone(),
            name: name.clone(),
            kind,
            media_type: String::new(),
            path: String::new(),
            size: 0,
            original_size,
            width: None,
            height: None,
            pages: None,
            thumbnail: None,
            note: None,
        };
        let stored: Vec<u8> = match kind {
            AttachmentKind::Image => {
                self.check_size(&name, bytes.len(), self.limits.max_image_bytes, "images")?;
                let limits = self.limits;
                let n = name.clone();
                let image = tokio::task::spawn_blocking(move || process_image(&n, bytes, limits))
                    .await
                    .map_err(|e| unreadable(&name, e))??;
                attachment.media_type = image.format.to_mime_type().to_owned();
                attachment.width = Some(image.width);
                attachment.height = Some(image.height);
                attachment.thumbnail = Some(image.thumbnail);
                attachment.note = image.note;
                image.bytes
            }
            AttachmentKind::Pdf => {
                self.check_size(&name, bytes.len(), self.limits.max_pdf_bytes, "PDFs")?;
                attachment.media_type = "application/pdf".into();
                match pdf_pages(bytes.clone()).await {
                    Ok(pages) => {
                        let count = u32::try_from(pages.len()).unwrap_or(u32::MAX);
                        if count > self.limits.max_pdf_pages {
                            return Err(AttachmentError::TooManyPages {
                                name,
                                pages: count,
                                limit: self.limits.max_pdf_pages,
                            });
                        }
                        attachment.pages = Some(count);
                        let text = join_pages(&pages);
                        if text.trim().is_empty() {
                            attachment.note = Some(
                                "No text found (scanned pages?). Only models that read PDFs \
                                 natively can use it."
                                    .into(),
                            );
                        }
                        let sidecar = pdf_text_sidecar(&self.stored_path(&id, "pdf"));
                        tokio::fs::write(&sidecar, text)
                            .await
                            .map_err(|e| unreadable(&name, e))?;
                    }
                    Err(e) => {
                        tracing::info!("no text from {name}: {e}");
                        attachment.note = Some(
                            "Its text couldn't be extracted. Only models that read PDFs \
                             natively can use it."
                                .into(),
                        );
                    }
                }
                bytes
            }
            AttachmentKind::Text => {
                self.check_size(&name, bytes.len(), self.limits.max_text_bytes, "text files")?;
                attachment.media_type = text_media_type(&ext).to_owned();
                let (text, lossy) = decode_text(&bytes);
                if lossy {
                    attachment.note =
                        Some("Some characters weren't UTF-8 and were replaced.".into());
                }
                text.into_bytes()
            }
        };

        let file_ext = match kind {
            AttachmentKind::Image => image_extension(&attachment.media_type),
            AttachmentKind::Pdf => "pdf",
            AttachmentKind::Text if !ext.is_empty() && ext.len() <= 12 => ext.as_str(),
            AttachmentKind::Text => "txt",
        };
        let path = self.stored_path(&id, file_ext);
        attachment.size = u32::try_from(stored.len()).unwrap_or(u32::MAX);
        tokio::fs::write(&path, stored)
            .await
            .map_err(|e| unreadable(&name, e))?;
        attachment.path = path;
        Ok(attachment)
    }

    /// Content blocks for attachments the UI sends back, after checking they're ours.
    pub fn blocks(&self, attachments: &[Attachment]) -> Result<Vec<ContentBlock>> {
        if attachments.len() > self.limits.max_files as usize {
            return Err(AttachmentError::TooMany(self.limits.max_files));
        }
        attachments
            .iter()
            .map(|a| {
                if self.owns(&a.path) {
                    Ok(a.to_block())
                } else {
                    Err(AttachmentError::Foreign)
                }
            })
            .collect()
    }

    /// Whether `path` is a file directly inside the attachments folder.
    pub fn owns(&self, path: &str) -> bool {
        let p = Path::new(path);
        p.parent() == Some(self.dir.as_path())
            && p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && !n.contains(".."))
    }

    /// A stored image as a `data:` URL, scaled to fit `max_side` (or unchanged with `None`).
    pub async fn image_data_url(&self, path: &str, max_side: Option<u32>) -> Result<String> {
        if !self.owns(path) {
            return Err(AttachmentError::Foreign);
        }
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| unreadable(&name, e))?;
        let n = name.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let format = image::guess_format(&bytes).map_err(|e| unreadable(&n, e))?;
            match max_side {
                None => Ok(data_url(format.to_mime_type(), &bytes)),
                Some(side) => {
                    let img = image::load_from_memory_with_format(&bytes, format)
                        .map_err(|e| unreadable(&n, e))?;
                    thumbnail(&n, &img, side)
                }
            }
        })
        .await
        .map_err(|e| unreadable(&name, e))?
    }

    fn stored_path(&self, id: &str, ext: &str) -> String {
        self.dir
            .join(format!("{id}.{ext}"))
            .to_string_lossy()
            .into_owned()
    }

    fn check_size(&self, name: &str, size: usize, limit: u32, what: &'static str) -> Result<()> {
        if size as u64 > u64::from(limit) {
            return Err(AttachmentError::TooLarge {
                name: name.to_owned(),
                what,
                size: format_bytes(size as u64),
                limit: format_bytes(u64::from(limit)),
            });
        }
        Ok(())
    }

    /// The largest file of any kind; anything bigger is refused before it's read.
    fn max_any(&self) -> u64 {
        let l = self.limits;
        u64::from(l.max_image_bytes.max(l.max_pdf_bytes).max(l.max_text_bytes))
    }

    async fn read_source(&self, source: AttachmentSource) -> Result<(String, Vec<u8>)> {
        match source {
            AttachmentSource::Path { path } => {
                let name = clean_name(
                    Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .as_deref()
                        .unwrap_or("file"),
                );
                let meta = tokio::fs::metadata(&path)
                    .await
                    .map_err(|e| unreadable(&name, e))?;
                if meta.is_dir() {
                    return Err(AttachmentError::Unreadable {
                        name,
                        reason: "folders can't be attached".into(),
                    });
                }
                if meta.len() > self.max_any() {
                    return Err(AttachmentError::TooLarge {
                        name,
                        what: "any file",
                        size: format_bytes(meta.len()),
                        limit: format_bytes(self.max_any()),
                    });
                }
                let bytes = tokio::fs::read(&path)
                    .await
                    .map_err(|e| unreadable(&name, e))?;
                Ok((name, bytes))
            }
            AttachmentSource::Bytes { name, data } => {
                let name = clean_name(&name);
                if data.len() as u64 > self.max_any().div_ceil(3) * 4 + 4 {
                    return Err(AttachmentError::TooLarge {
                        name,
                        what: "any file",
                        size: format_bytes(data.len() as u64 / 4 * 3),
                        limit: format_bytes(self.max_any()),
                    });
                }
                let bytes = STANDARD
                    .decode(data.trim())
                    .map_err(|e| unreadable(&name, e))?;
                Ok((name, bytes))
            }
        }
    }
}

fn unreadable(name: &str, e: impl std::fmt::Display) -> AttachmentError {
    AttachmentError::Unreadable {
        name: name.to_owned(),
        reason: e.to_string(),
    }
}

/// A file name safe to show and store: no path, no control characters, at most 200 chars.
fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned.to_owned()
    }
}

/// Lower-case extension without the dot ("" if none). Dotfiles count as their name
/// (".gitignore" → "gitignore").
fn extension(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_owned(),
        _ => match lower.as_str() {
            "dockerfile" | "makefile" => lower,
            _ => String::new(),
        },
    }
}

fn image_extension(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// What a file is, from its content first and its extension second.
fn detect(bytes: &[u8], ext: &str) -> Option<AttachmentKind> {
    if bytes.starts_with(b"%PDF-") {
        return Some(AttachmentKind::Pdf);
    }
    if let Ok(format) = image::guess_format(bytes)
        && matches!(
            format,
            ImageFormat::Png
                | ImageFormat::Jpeg
                | ImageFormat::Gif
                | ImageFormat::WebP
                | ImageFormat::Bmp
        )
    {
        return Some(AttachmentKind::Image);
    }
    if ext == "pdf" {
        return None; // claims to be a PDF but isn't one
    }
    let sample = &bytes[..bytes.len().min(8192)];
    let looks_textual = !sample.contains(&0);
    if looks_textual && (TEXT_EXTENSIONS.contains(&ext) || std::str::from_utf8(sample).is_ok()) {
        return Some(AttachmentKind::Text);
    }
    // A UTF-8 sample can end mid-character: accept if only the last few bytes are cut.
    if looks_textual
        && let Err(e) = std::str::from_utf8(sample)
        && e.error_len().is_none()
    {
        return Some(AttachmentKind::Text);
    }
    None
}

/// UTF-8 text without a BOM, and whether invalid bytes had to be replaced.
fn decode_text(bytes: &[u8]) -> (String, bool) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_owned(), false),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
    }
}

struct ProcessedImage {
    bytes: Vec<u8>,
    format: ImageFormat,
    width: u32,
    height: u32,
    thumbnail: String,
    note: Option<String>,
}

/// Decodes, scales and re-encodes an image as needed. CPU-bound: run on a blocking thread.
fn process_image(name: &str, bytes: Vec<u8>, limits: AttachmentLimits) -> Result<ProcessedImage> {
    let format = image::guess_format(&bytes).map_err(|e| unreadable(name, e))?;
    let img =
        image::load_from_memory_with_format(&bytes, format).map_err(|e| unreadable(name, e))?;
    let (w, h) = img.dimensions();
    let side = limits.max_image_side;
    let fits = w <= side && h <= side;
    let keep = fits
        && bytes.len() <= KEEP_ORIGINAL_BYTES
        && matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
        );
    let thumb = thumbnail(name, &img, THUMBNAIL_SIDE)?;
    if keep {
        return Ok(ProcessedImage {
            bytes,
            format,
            width: w,
            height: h,
            thumbnail: thumb,
            note: None,
        });
    }

    let scaled = if fits {
        img
    } else {
        img.resize(side, side, image::imageops::FilterType::Lanczos3)
    };
    let (sw, sh) = scaled.dimensions();
    let mut out = Vec::new();
    let mut format_out = if is_transparent(&scaled) {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };
    encode(&scaled, format_out, &mut out).map_err(|e| unreadable(name, e))?;
    if out.len() > MAX_STORED_IMAGE_BYTES {
        // A large transparent PNG: flatten it and use JPEG instead.
        out.clear();
        format_out = ImageFormat::Jpeg;
        encode(&scaled, format_out, &mut out).map_err(|e| unreadable(name, e))?;
    }
    let note = if fits {
        Some(format!(
            "Compressed from {}",
            format_bytes(bytes.len() as u64)
        ))
    } else {
        Some(format!("Resized from {w}×{h}"))
    };
    Ok(ProcessedImage {
        bytes: out,
        format: format_out,
        width: sw,
        height: sh,
        thumbnail: thumb,
        note,
    })
}

/// Encodes PNG as is, and JPEG on a white background (JPEG has no transparency).
fn encode(img: &DynamicImage, format: ImageFormat, out: &mut Vec<u8>) -> image::ImageResult<()> {
    let mut cursor = Cursor::new(out);
    match format {
        ImageFormat::Jpeg => {
            let rgb = flatten(img);
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, JPEG_QUALITY);
            rgb.write_with_encoder(encoder)
        }
        _ => img.write_to(&mut cursor, ImageFormat::Png),
    }
}

/// Whether any pixel is see-through (screenshots are often RGBA but fully opaque).
fn is_transparent(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageRgba8(buf) => buf.pixels().any(|p| p[3] < 255),
        DynamicImage::ImageLumaA8(buf) => buf.pixels().any(|p| p[1] < 255),
        other if other.color().has_alpha() => other.to_rgba8().pixels().any(|p| p[3] < 255),
        _ => false,
    }
}

/// The image on a white background, without alpha.
fn flatten(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u16::from(p[3]);
        let blend = |c: u8| u8::try_from((u16::from(c) * a + 255 * (255 - a)) / 255).unwrap_or(255);
        rgb.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    rgb
}

/// A small preview that fits `side`, as a `data:` URL (PNG when transparent, else JPEG).
fn thumbnail(name: &str, img: &DynamicImage, side: u32) -> Result<String> {
    let small = if img.width() > side || img.height() > side {
        img.thumbnail(side, side)
    } else {
        img.clone()
    };
    let format = if is_transparent(&small) {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };
    let mut out = Vec::new();
    encode(&small, format, &mut out).map_err(|e| unreadable(name, e))?;
    Ok(data_url(format.to_mime_type(), &out))
}

fn data_url(media_type: &str, bytes: &[u8]) -> String {
    format!("data:{media_type};base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    /// A PNG of the given size (a gradient, optionally with transparency).
    pub(crate) fn png(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let img = ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([
                u8::try_from(x % 256).unwrap_or(0),
                u8::try_from(y % 256).unwrap_or(0),
                128,
                if alpha { 100 } else { 255 },
            ])
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("encodes");
        out
    }

    fn store() -> (tempfile::TempDir, AttachmentStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = AttachmentStore::new(dir.path().join("attachments"));
        (dir, store)
    }

    fn bytes(name: &str, data: &[u8]) -> AttachmentSource {
        AttachmentSource::Bytes {
            name: name.into(),
            data: STANDARD.encode(data),
        }
    }

    #[tokio::test]
    async fn small_images_are_kept_and_get_a_thumbnail() {
        let (_tmp, s) = store();
        let a = s
            .prepare(bytes("shot.png", &png(40, 30, false)))
            .await
            .expect("prepares");
        assert_eq!(a.kind, AttachmentKind::Image);
        assert_eq!(a.media_type, "image/png");
        assert_eq!((a.width, a.height), (Some(40), Some(30)));
        assert_eq!(a.size, a.original_size);
        assert!(
            a.thumbnail
                .as_deref()
                .is_some_and(|t| t.starts_with("data:image/jpeg;base64,"))
        );
        assert!(a.path.ends_with(".png") && s.owns(&a.path));
        assert!(Path::new(&a.path).exists());
        assert!(a.note.is_none());
    }

    #[tokio::test]
    async fn large_images_are_scaled_and_reencoded() {
        let (_tmp, s) = store();
        let a = s
            .prepare(bytes("big.png", &png(3000, 1000, false)))
            .await
            .expect("prepares");
        assert_eq!((a.width, a.height), (Some(1568), Some(523)));
        assert_eq!(a.media_type, "image/jpeg");
        assert!(a.path.ends_with(".jpg"));
        assert_eq!(a.note.as_deref(), Some("Resized from 3000×1000"));

        // Transparency is kept as PNG; the thumbnail too.
        let t = s
            .prepare(bytes("logo.png", &png(2000, 2000, true)))
            .await
            .expect("prepares");
        assert_eq!(t.media_type, "image/png");
        assert!(
            t.thumbnail
                .as_deref()
                .is_some_and(|x| x.starts_with("data:image/png"))
        );
        let url = s.image_data_url(&t.path, Some(64)).await.expect("previews");
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn text_files_are_checked_and_named() {
        let (_tmp, s) = store();
        let a = s
            .prepare(bytes("C:\\code\\main.rs", b"\xEF\xBB\xBFfn main() {}\n"))
            .await
            .expect("prepares");
        assert_eq!(a.name, "main.rs");
        assert_eq!(a.kind, AttachmentKind::Text);
        assert_eq!(a.media_type, "text/plain");
        assert!(a.path.ends_with(".rs"));
        assert_eq!(
            tokio::fs::read_to_string(&a.path).await.expect("reads"),
            "fn main() {}\n"
        );

        // Unknown extension but plain text: accepted. Latin-1 bytes are replaced.
        let a = s
            .prepare(bytes("LICENSE", b"caf\xE9"))
            .await
            .expect("prepares");
        assert_eq!(a.kind, AttachmentKind::Text);
        assert!(a.note.is_some());

        // Binary junk isn't.
        let err = s
            .prepare(bytes("blob.bin", &[0, 1, 2, 3, 0, 255]))
            .await
            .expect_err("rejects");
        assert!(matches!(err, AttachmentError::Unsupported { .. }));
        assert!(err.to_string().contains("blob.bin"));

        let err = s
            .prepare(bytes("fake.pdf", b"hello"))
            .await
            .expect_err("rejects");
        assert!(matches!(err, AttachmentError::Unsupported { .. }));
    }

    #[tokio::test]
    async fn size_limits_name_the_file_and_the_limit() {
        let (_tmp, mut s) = store();
        s.limits.max_text_bytes = 10;
        let err = s
            .prepare(bytes("notes.md", b"this is longer than ten bytes"))
            .await
            .expect_err("too large");
        let text = err.to_string();
        assert!(text.contains("notes.md") && text.contains("10 B"), "{text}");
    }

    #[tokio::test]
    async fn pdfs_are_counted_and_their_text_saved() {
        let (_tmp, mut s) = store();
        let pdf = oxide_llm::media::sample_pdf(&["Quarterly report", "Totals"]);
        let a = s
            .prepare(bytes("report.pdf", &pdf))
            .await
            .expect("prepares");
        assert_eq!(a.kind, AttachmentKind::Pdf);
        assert_eq!(a.pages, Some(2));
        assert!(a.note.is_none());
        let text = tokio::fs::read_to_string(pdf_text_sidecar(&a.path))
            .await
            .expect("sidecar");
        assert!(
            text.starts_with("[Page 1]") && text.contains("Totals"),
            "{text}"
        );

        s.limits.max_pdf_pages = 1;
        let err = s
            .prepare(bytes("report.pdf", &pdf))
            .await
            .expect_err("too many pages");
        assert!(matches!(
            err,
            AttachmentError::TooManyPages { pages: 2, .. }
        ));
    }

    #[tokio::test]
    async fn unreadable_pdfs_are_kept_with_a_note() {
        let (_tmp, s) = store();
        let a = s
            .prepare(bytes("report.pdf", b"%PDF-1.7\nnot really"))
            .await
            .expect("keeps it for native PDF models");
        assert_eq!(a.kind, AttachmentKind::Pdf);
        assert_eq!(a.pages, None);
        assert!(a.note.is_some());
    }

    #[tokio::test]
    async fn files_from_disk_and_ownership() {
        let (tmp, s) = store();
        let src = tmp.path().join("a.txt");
        tokio::fs::write(&src, "hello").await.expect("writes");
        let a = s
            .prepare(AttachmentSource::Path {
                path: src.to_string_lossy().into_owned(),
            })
            .await
            .expect("prepares");
        assert_eq!(a.name, "a.txt");
        assert_eq!(s.blocks(std::slice::from_ref(&a)).expect("ours").len(), 1);

        let mut foreign = a.clone();
        foreign.path = src.to_string_lossy().into_owned();
        assert!(matches!(
            s.blocks(&[foreign]),
            Err(AttachmentError::Foreign)
        ));
        assert!(!s.owns(&format!("{}/../x", s.dir().display())));
        assert!(
            s.image_data_url(&src.to_string_lossy(), None)
                .await
                .is_err()
        );

        let folder = AttachmentSource::Path {
            path: tmp.path().to_string_lossy().into_owned(),
        };
        assert!(s.prepare(folder).await.is_err());
    }

    #[test]
    fn helpers() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(extension("Archive.TAR.GZ"), "gz");
        assert_eq!(extension("Dockerfile"), "dockerfile");
        assert_eq!(extension("README"), "");
        assert_eq!(clean_name("/tmp/x/\u{7}evil.txt"), "evil.txt");
        assert_eq!(clean_name("  "), "file");
    }
}
