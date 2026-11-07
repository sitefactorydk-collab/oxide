//! Computer use for Oxide: screen capture and mouse/keyboard input.
//!
//! Phase 9 uses screen capture for quick entry's screenshot attachment. Input and the rest
//! of computer use arrive in Phase 16 (see PLAN.md).
//!
//! Capturing is blocking work: call it from `spawn_blocking`.

use std::io::Cursor;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("there is no screen to capture")]
    NoScreen,
    #[error("the screen couldn't be captured: {0}")]
    Capture(String),
    #[error("the screenshot couldn't be encoded: {0}")]
    Encode(String),
}

/// Captures one screen as PNG bytes: the screen containing `point` (physical pixels, in
/// the virtual desktop's coordinates) when given and found, else the primary screen.
pub fn capture_screen_png(point: Option<(i32, i32)>) -> Result<Vec<u8>, CaptureError> {
    let monitor = match point.and_then(|(x, y)| xcap::Monitor::from_point(x, y).ok()) {
        Some(m) => m,
        None => primary_monitor()?,
    };
    let shot = monitor
        .capture_image()
        .map_err(|e| CaptureError::Capture(e.to_string()))?;
    let (width, height) = (shot.width(), shot.height());
    // Rebuilt with this crate's `image`, so xcap's own `image` version doesn't matter.
    let rgba = image::RgbaImage::from_raw(width, height, shot.into_raw())
        .ok_or_else(|| CaptureError::Encode("unexpected pixel buffer size".into()))?;
    encode_png(&rgba)
}

fn primary_monitor() -> Result<xcap::Monitor, CaptureError> {
    let monitors = xcap::Monitor::all().map_err(|e| CaptureError::Capture(e.to_string()))?;
    let mut first = None;
    for m in monitors {
        if m.is_primary().unwrap_or(false) {
            return Ok(m);
        }
        if first.is_none() {
            first = Some(m);
        }
    }
    first.ok_or(CaptureError::NoScreen)
}

/// Encodes an RGBA image as PNG.
pub fn encode_png(img: &image::RgbaImage) -> Result<Vec<u8>, CaptureError> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| CaptureError::Encode(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_png() {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([200, 100, 50, 255]));
        let png = encode_png(&img).expect("encodes");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let back = image::load_from_memory(&png).expect("decodes").to_rgba8();
        assert_eq!(back.dimensions(), (3, 2));
    }
}
