//! The text of a page: every character with the box it is drawn in, for selecting, copying and
//! searching.
//!
//! [`page_text`] runs the page through `hayro`'s interpreter with a device that draws nothing and
//! writes down each glyph's Unicode value and position. Boxes are in points from the page's top
//! left (the same space as [`wordcraft_pdfdoc::PageSize`]), so the viewer scales them like the
//! page picture. A glyph whose font has no Unicode mapping is left out. The file is hostile
//! input: the number of characters is capped at [`MAX_CHARS`] and a box that isn't a finite
//! number is dropped.

use std::panic::{AssertUnwindSafe, catch_unwind};

use hayro::hayro_interpret::font::{GlyphRun, PositionedGlyph};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache, InterpreterSettings, SoftMask, TransformExt,
    interpret_page,
};
use hayro::kurbo::{Affine, BezPath, Point, Rect};
use wordcraft_pdfdoc::PdfDoc;

use crate::RenderError;

/// Most characters kept from one page (a page of dense type has a few thousand).
pub const MAX_CHARS: usize = 200_000;

/// Glyph space is 1000 units to the em; a glyph's box runs from the descender to the ascender.
const DESCENT: f64 = -200.0;
const ASCENT: f64 = 800.0;
/// The width used when a font gives none, in glyph units.
const FALLBACK_ADVANCE: f64 = 500.0;
/// A gap wider than this fraction of the line's height is a space, when the file has none.
const SPACE_GAP: f64 = 0.25;

/// One character and the box it is drawn in.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    pub ch: char,
    /// Left, top, right, bottom in points from the page's top left.
    pub rect: [f32; 4],
    /// Which line of the page this character is on (from 0, in drawing order).
    pub line: u32,
}

/// The characters of one page, in the order the file draws them (reading order for most files).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<TextChar>,
}

impl PageText {
    /// Index of the character whose box holds the point (in points from the top left), or the
    /// nearest one on the line under it; `None` if the page has no text or the point is not a
    /// number.
    pub fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in self.chars.iter().enumerate() {
            let [l, t, r, b] = c.rect;
            let dx = if x < l {
                l - x
            } else if x > r {
                x - r
            } else {
                0.0
            };
            let dy = if y < t {
                t - y
            } else if y > b {
                y - b
            } else {
                0.0
            };
            // Closeness down the page counts for more than along it, so a point between two
            // lines picks the nearer line rather than the nearer column.
            let d = dy * 4.0 + dx;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The text of characters `from..=to` (either order), with a space where the file leaves a
    /// gap and a newline where the line changes. Out-of-range ends are clamped.
    pub fn text_between(&self, from: usize, to: usize) -> String {
        let Some(last) = self.chars.len().checked_sub(1) else {
            return String::new();
        };
        let (a, b) = (from.min(to).min(last), from.max(to).min(last));
        let mut out = String::new();
        let mut prev: Option<&TextChar> = None;
        for c in self.chars.iter().take(b + 1).skip(a) {
            if let Some(p) = prev {
                if c.line != p.line {
                    out.push('\n');
                } else if gap(p, c) && !p.ch.is_whitespace() && !c.ch.is_whitespace() {
                    out.push(' ');
                }
            }
            out.push(c.ch);
            prev = Some(c);
        }
        out
    }

    /// The whole page's text (see [`PageText::text_between`]).
    pub fn text(&self) -> String {
        self.text_between(0, self.chars.len().saturating_sub(1))
    }

    /// The boxes to highlight for characters `from..=to`: one per line, joined across the line.
    pub fn boxes_between(&self, from: usize, to: usize) -> Vec<[f32; 4]> {
        let Some(last) = self.chars.len().checked_sub(1) else {
            return Vec::new();
        };
        let (a, b) = (from.min(to).min(last), from.max(to).min(last));
        let mut boxes: Vec<(u32, [f32; 4])> = Vec::new();
        for c in self.chars.iter().take(b + 1).skip(a) {
            match boxes.last_mut() {
                Some((line, r)) if *line == c.line => {
                    r[0] = r[0].min(c.rect[0]);
                    r[1] = r[1].min(c.rect[1]);
                    r[2] = r[2].max(c.rect[2]);
                    r[3] = r[3].max(c.rect[3]);
                }
                _ => boxes.push((c.line, c.rect)),
            }
        }
        boxes.into_iter().map(|(_, r)| r).collect()
    }

    /// Every place `needle` occurs, as `(first, last)` character indexes. Case is ignored;
    /// matches don't cross a line. An empty needle matches nothing.
    pub fn find(&self, needle: &str) -> Vec<(usize, usize)> {
        let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
        if needle.is_empty() || needle.len() > self.chars.len() {
            return Vec::new();
        }
        let hay: Vec<char> = self.chars.iter().map(|c| c.ch.to_lowercase().next().unwrap_or(c.ch)).collect();
        let mut found = Vec::new();
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            let same_line = self.chars[i].line == self.chars[i + needle.len() - 1].line;
            if same_line && hay[i..i + needle.len()] == needle[..] {
                found.push((i, i + needle.len() - 1));
                i += needle.len();
            } else {
                i += 1;
            }
        }
        found
    }
}

/// Whether the horizontal gap between two characters on one line is wide enough to be a space.
fn gap(a: &TextChar, b: &TextChar) -> bool {
    let height = (a.rect[3] - a.rect[1]).max(b.rect[3] - b.rect[1]);
    f64::from(b.rect[0] - a.rect[2]) > SPACE_GAP * f64::from(height)
}

/// The characters of page `index` (from 0).
pub fn page_text(doc: &PdfDoc, index: usize) -> Result<PageText, RenderError> {
    let page = doc.pdf().pages().get(index).ok_or(RenderError::NoSuchPage)?;
    let (width, height) = page.render_dimensions();
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err(RenderError::BadSize);
    }
    let collected = catch_unwind(AssertUnwindSafe(|| {
        let transform = page.initial_transform(true).to_kurbo();
        let cache = InterpreterCache::new();
        let bounds = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        let mut context = Context::new(transform, bounds, &cache, page.xref(), InterpreterSettings::default());
        let mut device = Collector::default();
        interpret_page(page, &mut context, &mut device);
        device.chars
    }));
    let raw = collected.map_err(|_| RenderError::Crashed)?;
    Ok(PageText { chars: assign_lines(raw) })
}

/// A character before its line is known.
struct RawChar {
    ch: char,
    rect: [f32; 4],
}

/// Number the lines: a new line starts when the next character is clearly above or below the
/// last one's middle, or has jumped back to the left by more than its own width.
fn assign_lines(raw: Vec<RawChar>) -> Vec<TextChar> {
    let mut line = 0u32;
    let mut out: Vec<TextChar> = Vec::with_capacity(raw.len());
    for c in raw {
        if let Some(p) = out.last() {
            let height = (p.rect[3] - p.rect[1]).max(c.rect[3] - c.rect[1]);
            let (pm, cm) = ((p.rect[1] + p.rect[3]) / 2.0, (c.rect[1] + c.rect[3]) / 2.0);
            let moved_down = (cm - pm).abs() > height * 0.5;
            let jumped_back = c.rect[2] < p.rect[0];
            if moved_down || jumped_back {
                line = line.saturating_add(1);
            }
        }
        out.push(TextChar { ch: c.ch, rect: c.rect, line });
    }
    out
}

/// A device that draws nothing and records the characters.
#[derive(Default)]
struct Collector {
    chars: Vec<RawChar>,
}

impl Collector {
    fn push_glyph(&mut self, glyph: &PositionedGlyph<'_>, to_page: Affine) {
        if self.chars.len() >= MAX_CHARS {
            return;
        }
        let (text, advance) = match &**glyph {
            hayro::hayro_interpret::font::Glyph::Outline(g) => (g.as_unicode(), g.advance_width()),
            hayro::hayro_interpret::font::Glyph::Type3(g) => (g.as_unicode(), None),
        };
        let Some(text) = text else { return };
        let advance = advance.map_or(FALLBACK_ADVANCE, f64::from);
        let advance = if advance.is_finite() && advance > 0.0 { advance } else { FALLBACK_ADVANCE };
        let to_page = to_page * glyph.transform();
        let corners =
            [Point::new(0.0, DESCENT), Point::new(advance, DESCENT), Point::new(advance, ASCENT), Point::new(0.0, ASCENT)].map(|p| to_page * p);
        let (mut l, mut t, mut r, mut b) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in corners {
            l = l.min(p.x);
            t = t.min(p.y);
            r = r.max(p.x);
            b = b.max(p.y);
        }
        if ![l, t, r, b].iter().all(|v| v.is_finite()) {
            return;
        }
        let rect = [l as f32, t as f32, r as f32, b as f32];
        let mut push = |ch: char, rect: [f32; 4]| {
            if self.chars.len() < MAX_CHARS {
                self.chars.push(RawChar { ch, rect });
            }
        };
        match text {
            BfString::Char(ch) => push(ch, rect),
            BfString::String(s) => {
                // A ligature or a glyph for several characters: they share the glyph's box,
                // split evenly left to right.
                let count = s.chars().count().max(1) as f32;
                let step = (rect[2] - rect[0]) / count;
                for (k, ch) in s.chars().enumerate() {
                    let left = rect[0] + step * k as f32;
                    push(ch, [left, rect[1], left + step, rect[3]]);
                }
            }
        }
    }
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            self.push_glyph(glyph, props.transform);
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
