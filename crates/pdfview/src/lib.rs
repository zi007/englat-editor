//! Draws a page of an opened PDF into pixels.
//!
//! [`render_page`] rasterises one page with `hayro` (pure Rust, on the CPU, also in the browser).
//! A file chooses its own page size, so the size of the picture is not taken from it: the caller
//! says how wide the picture should be, and the size is kept within [`MAX_SIDE_PX`] and
//! [`MAX_PIXELS`]; a page that can't be drawn within them is refused, not allocated.
//!
//! Panics inside the rasteriser come back as [`RenderError::Crashed`] on native targets; on
//! `wasm32-unknown-unknown` a panic aborts the module (see `docs/pdf-editor.md`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod text;

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use wordcraft_pdfdoc::PdfDoc;

/// Longest side of a picture, in pixels (the largest texture most GPUs and WebGL2 take).
pub const MAX_SIDE_PX: u32 = 4096;
/// Most pixels in a picture (16 million: 64 MB of RGBA, and the rasteriser holds a second copy).
pub const MAX_PIXELS: u64 = 16_000_000;

/// A page drawn on an opaque white background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    /// `width * height` pixels of RGBA, 8 bits each, row by row from the top. Every pixel is
    /// opaque, so premultiplied and straight alpha are the same.
    pub rgba: Vec<u8>,
}

/// Why a page was not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// The document has no such page.
    NoSuchPage,
    /// The page's size, or the width asked for, can't be drawn within the limits.
    BadSize,
    /// The rasteriser failed on this page (a panic inside it, caught on native targets).
    Crashed,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchPage => write!(f, "There is no such page."),
            Self::BadSize => write!(f, "This page's size can't be drawn."),
            Self::Crashed => write!(f, "This page could not be drawn."),
        }
    }
}

impl std::error::Error for RenderError {}

/// The scale (pixels per point) that draws a `page_width` by `page_height` page `target_width`
/// pixels wide, reduced if that goes past the limits; `None` if no scale gives a picture of at
/// least one pixel each way within them, or if a number is not a size.
pub fn scale_for(page_width: f32, page_height: f32, target_width: f32) -> Option<f32> {
    let sizes = [page_width, page_height, target_width];
    if !sizes.iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let mut scale = target_width / page_width;
    let side = MAX_SIDE_PX as f32;
    scale = scale.min(side / page_width).min(side / page_height);
    let pixels = (page_width * scale) as f64 * (page_height * scale) as f64;
    if pixels > MAX_PIXELS as f64 {
        scale *= (MAX_PIXELS as f64 / pixels).sqrt() as f32;
    }
    // The rasteriser truncates to whole pixels: both sides must survive that, and stay in range.
    let (w, h) = (page_width * scale, page_height * scale);
    let fits = w >= 1.0 && h >= 1.0 && w <= side && h <= side && (w as f64) * (h as f64) <= MAX_PIXELS as f64;
    (scale.is_finite() && fits).then_some(scale)
}

/// Draw page `index` (from 0) about `target_width` pixels wide, its height following the page.
pub fn render_page(doc: &PdfDoc, index: usize, target_width: f32) -> Result<PageImage, RenderError> {
    let page = doc.pdf().pages().get(index).ok_or(RenderError::NoSuchPage)?;
    let (page_width, page_height) = page.render_dimensions();
    let scale = scale_for(page_width, page_height, target_width).ok_or(RenderError::BadSize)?;
    let drawn = catch_unwind(AssertUnwindSafe(|| {
        let settings = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
        let pixmap = render(page, &RenderCache::new(), &InterpreterSettings::default(), &RenderSettings::default(), &settings);
        (u32::from(pixmap.width()), u32::from(pixmap.height()), pixmap.data_as_u8_slice().to_vec())
    }));
    let (width, height, rgba) = drawn.map_err(|_| RenderError::Crashed)?;
    // A picture that doesn't add up (an empty one, or bytes missing) is not handed on.
    if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return Err(RenderError::BadSize);
    }
    Ok(PageImage { width, height, rgba })
}

#[cfg(test)]
mod tests;
