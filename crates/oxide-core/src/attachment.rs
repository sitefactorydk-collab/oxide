//! Files attached to a message: what the composer sends, and the limits Oxide enforces.
//!
//! Attachments are copied into the app's data folder when they're added (images resized and
//! compressed, PDFs checked, text files validated). The copy's path is what messages store.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ContentBlock, MediaSource};

/// What kind of file an attachment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    Image,
    Pdf,
    /// Plain text or source code, inlined into the prompt with a filename header.
    Text,
}

/// Where a new attachment comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AttachmentSource {
    /// A file on disk (file picker, drag and drop).
    Path { path: String },
    /// Bytes from the clipboard or a browser file input, base64-encoded.
    Bytes { name: String, data: String },
}

/// A prepared attachment, ready to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    /// The original file name.
    pub name: String,
    pub kind: AttachmentKind,
    pub media_type: String,
    /// The stored copy in the app's data folder.
    pub path: String,
    /// Size of the stored copy, in bytes.
    pub size: u32,
    /// Size of the original file, in bytes.
    pub original_size: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Page count of a PDF.
    pub pages: Option<u32>,
    /// A small JPEG preview of an image, as a `data:` URL.
    pub thumbnail: Option<String>,
    /// Something the user should know, such as "Resized from 4032×3024".
    pub note: Option<String>,
}

impl Attachment {
    /// The content block a message stores for this attachment.
    pub fn to_block(&self) -> ContentBlock {
        let source = MediaSource::Path {
            path: self.path.clone(),
        };
        match self.kind {
            AttachmentKind::Image => ContentBlock::Image {
                media_type: self.media_type.clone(),
                source,
            },
            AttachmentKind::Pdf | AttachmentKind::Text => ContentBlock::Document {
                media_type: self.media_type.clone(),
                name: self.name.clone(),
                source,
            },
        }
    }
}

/// Limits on attachments, shown in the UI and enforced when files are added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentLimits {
    pub max_files: u32,
    /// Largest image accepted before resizing, in bytes.
    pub max_image_bytes: u32,
    /// Images are scaled down to fit inside this many pixels on each side.
    pub max_image_side: u32,
    pub max_pdf_bytes: u32,
    pub max_pdf_pages: u32,
    pub max_text_bytes: u32,
}

/// The limits Oxide uses. The PDF limits are the Anthropic API's; images are scaled to the
/// size its docs recommend (larger images are downscaled by the API anyway).
pub const ATTACHMENT_LIMITS: AttachmentLimits = AttachmentLimits {
    max_files: 20,
    max_image_bytes: 30 * 1024 * 1024,
    max_image_side: 1568,
    max_pdf_bytes: 32 * 1024 * 1024,
    max_pdf_pages: 100,
    max_text_bytes: 1024 * 1024,
};

/// Image formats Oxide accepts, by media type.
pub const IMAGE_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
];

/// File extensions treated as text or source code (lower case, without the dot).
pub const TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "rst",
    "adoc",
    "org",
    "tex",
    "log",
    "csv",
    "tsv",
    "json",
    "jsonl",
    "ndjson",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "env",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "svg",
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "mts",
    "cts",
    "tsx",
    "vue",
    "svelte",
    "astro",
    "py",
    "pyi",
    "ipynb",
    "rb",
    "php",
    "java",
    "kt",
    "kts",
    "scala",
    "groovy",
    "gradle",
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hpp",
    "hh",
    "hxx",
    "m",
    "mm",
    "cs",
    "fs",
    "fsx",
    "vb",
    "go",
    "rs",
    "swift",
    "dart",
    "lua",
    "pl",
    "pm",
    "r",
    "jl",
    "hs",
    "ml",
    "mli",
    "ex",
    "exs",
    "erl",
    "clj",
    "cljs",
    "elm",
    "zig",
    "nim",
    "v",
    "sv",
    "vhd",
    "sol",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ps1",
    "psm1",
    "bat",
    "cmd",
    "sql",
    "graphql",
    "gql",
    "proto",
    "dockerfile",
    "makefile",
    "cmake",
    "mk",
    "nix",
    "tf",
    "hcl",
    "lock",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "properties",
    "srt",
    "vtt",
    "diff",
    "patch",
];

/// The media type stored for a text attachment, by extension.
pub fn text_media_type(ext: &str) -> &'static str {
    match ext {
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "xml" | "svg" => "text/xml",
        "json" | "jsonl" | "ndjson" | "ipynb" => "application/json",
        "yaml" | "yml" => "application/x-yaml",
        "toml" => "application/toml",
        "js" | "mjs" | "cjs" | "jsx" => "text/javascript",
        _ => "text/plain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_become_blocks() {
        let mut a = Attachment {
            id: "a".into(),
            name: "photo.png".into(),
            kind: AttachmentKind::Image,
            media_type: "image/png".into(),
            path: "/data/attachments/a.png".into(),
            size: 10,
            original_size: 10,
            width: Some(1),
            height: Some(1),
            pages: None,
            thumbnail: None,
            note: None,
        };
        assert!(matches!(a.to_block(), ContentBlock::Image { .. }));
        a.kind = AttachmentKind::Pdf;
        a.media_type = "application/pdf".into();
        let ContentBlock::Document { name, source, .. } = a.to_block() else {
            panic!("document block")
        };
        assert_eq!(name, "photo.png");
        assert_eq!(
            source,
            MediaSource::Path {
                path: "/data/attachments/a.png".into()
            }
        );
    }

    #[test]
    fn sources_are_tagged() {
        let json =
            serde_json::to_value(AttachmentSource::Path { path: "x".into() }).expect("serializes");
        assert_eq!(json, serde_json::json!({"kind": "path", "path": "x"}));
    }

    #[test]
    fn text_types() {
        assert_eq!(text_media_type("md"), "text/markdown");
        assert_eq!(text_media_type("rs"), "text/plain");
        assert!(TEXT_EXTENSIONS.contains(&"tsx"));
    }
}
